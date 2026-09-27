//! The counted permission judgment [PAR-2]: whether the iterations of one
//! counted `for` may be executed with overlapping execution, and whether the
//! one value they carry may be recombined across them.
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
//! itself introduces, permission holds exactly when all four conditions hold:
//!
//! 1. **One accumulator, or none.** "Among whole-place writes of B, at most
//!    one place is rooted in a binding declared outside L; that binding is
//!    L's accumulator, and every occurrence of it in B is one operand of one
//!    `set` statement whose target is that whole binding and whose right-hand
//!    side is one operation applied to that operand and to a second operand
//!    reaching the accumulator nowhere." The operation is one of ten fixed
//!    for the accumulator across the whole of B.
//! 2. **Every written place is admitted.** "Every place a footprint of B
//!    writes is iteration-own storage, the accumulator's whole place, one
//!    proved single-binder affine element write, or one proved range
//!    reference." Nothing else is admitted, and no injectivity argument is
//!    searched for.
//! 3. **Resolved footprints.** "A footprint element whose caller place the
//!    implementation does not resolve overlaps every place, so an unresolved
//!    element denies permission rather than granting it."
//! 4. **No exit edge.** "Every normal continuation of every statement of B
//!    reaches L's compiler-owned binder update, so no statement of B is a
//!    `return_stmt`, a `give_stmt`, a `break_stmt` resolved to L or a loop
//!    enclosing L, or a `let_stmt` selecting `propagate_let_rhs`."
//!
//! # The two element families
//!
//! A **proved single-binder affine element write** is a `set_stmt` whose
//! target is one direct `Array` or `Slots` subscript rooted in an own binding
//! declared outside L, or reached through `deref` of a reference parameter
//! whose row declares the write, and whose discharged [OP-4] bounds
//! obligation retains the offset's exact value `a*i + b` for L's binder with
//! `a` nonzero. Distinct binder values therefore select distinct elements.
//! The `deref` needs no rule of its own here: it is an ordinary step of the
//! resolved path [REF-1, TYPE-7], so `deref(b)[a*i + c]` is recognized
//! exactly as an inline subscript is, which is what keeps every
//! runtime-capacity kernel in the family — [TYPE-9] admits a runtime-capacity
//! shape only as `Box` content [checker-facts].
//!
//! A `Ring` in an element-map position denies, "because a `Ring` subscript
//! selects the slot `(r.head + i) mod r.cap` [WIN-1], a wrapping map onto
//! storage rather than a linear offset".
//!
//! One affine map is admitted per resolved root. Every write to that root
//! must carry the same `a` and `b`, and every operand read through the same
//! root must be a direct subscript whose own discharged bounds result retains
//! the same `a` and `b`; a whole-root read, a different map, or an
//! unavailable one denies. That admits a same-index read-modify-write and
//! refuses a stencil, without any pairwise range search.
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
//! descend from a range with that partition. Read-only input origins may
//! overlap across iterations; they require no write partition.
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
//!   `+checked`, and `+sat` are absent because each application carries an
//!   obligation, a `Result` route, or a clamp that regrouping moves.
//! - No entailment fact established inside one counted iteration survives to
//!   a later head or to the continuation, so a regrouped accumulator can
//!   falsify no surviving proof.
//!
//! **Invariant.** This judgment consults typing, declared effect rows,
//! resolved places [REF-1, OWN-7], and the statement graph's exit edges. For
//! the two element families it additionally consumes the successful [OP-4] and
//! [REF-4] dispositions and the exact value images already retained on the
//! checked function; it never repeats a bounds proof or reconstructs a value
//! from parser shape. Every form it has not classified refuses, and the
//! statement match is exhaustive for that reason: a missed statement would
//! contribute an empty footprint and *widen* permission.

use super::entailment::{
    ObligationFamily, ObligationOutcome, ProvedAffineIndexMap, ProvedRangePartition,
};
use super::model::{
    BindingId, CheckedArrayRoot, CheckedBooleanOperation, CheckedExpression, CheckedFunction,
    CheckedIntegerOperation, CheckedLoopId, CheckedPlaceStep, CheckedSetTarget, CheckedStatement,
    CheckedType, WindowShape, expression_children,
};
use super::permission::{
    Footprint, Program, argument_places, call_projection, collect_consumed_places, container_steps,
    field_steps, set_target_place, visit_read_bindings,
};
use super::places::{PlaceMap, PlaceRoot, PlaceStep, ResolvedPlace, UnprovedSeparations};
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

/// The closed set of operations an accumulator may be combined under: exactly
/// the ten [PAR-2] admits, named once so the judgment, the ledger, and the
/// emitted combination tree cannot hold three drifting copies of it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoopCombine {
    AddWrap,
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
    /// Condition 2: a written or read place that is none of the four the rule
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
}

impl LoopDenial {
    /// The judgment condition this denial cites. The permission ledger prints
    /// it and the judgment tests assert it; acceptance never reads it.
    pub(crate) const fn condition(&self) -> u8 {
        match self {
            Self::NotAReduction { .. }
            | Self::ManyAccumulators { .. }
            | Self::AccumulatorRead { .. } => 1,
            Self::SharedWrite { .. } | Self::BodyForm { .. } => 2,
            Self::UnresolvedWrite { .. } => 3,
            Self::Exit { .. } => 4,
        }
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
            judged.push(judge(
                program,
                places,
                obligations,
                node_path.clone(),
                *id,
                *binder,
                body,
            ));
        }
        for nested in nested_bodies(statement) {
            collect(program, places, obligations, nested, judged);
        }
    }
}

/// Whether one statement lies inside another, by node path prefix.
fn encloses(outer: &NodePath, inner: &NodePath) -> bool {
    let outer = outer.components();
    let inner = inner.components();
    inner.len() > outer.len() && inner.starts_with(outer)
}

fn judge<'check>(
    program: &Program<'check>,
    places: &PlaceMap,
    obligations: &'check [ObligationOutcome],
    statement: NodePath,
    id: CheckedLoopId,
    binder: BindingId,
    body: &'check [CheckedStatement],
) -> LoopPermission {
    let mut survey = Survey {
        program,
        places,
        obligations,
        outer_loop: id,
        cite: statement.clone(),
        introduced: vec![binder],
        inner_loops: Vec::new(),
        reads: Vec::new(),
        accumulates: Vec::new(),
        carried: None,
        shared: None,
        unresolved: None,
        element_writes: Vec::new(),
        element_reads: Vec::new(),
        range_references: Vec::new(),
        form: None,
        exit: None,
    };
    survey.introduce(body);
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
}

/// One already-proved element read whose exact offset is the same affine map.
/// This is consumer evidence only: permission uses it once to compare read
/// and write ranges, then lowering forgets it.
struct ProvenElementRead {
    root: ResolvedPlace,
    map: ProvedAffineIndexMap,
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

/// One source read occurrence and the places reached by that spelling.
///
/// The binding remains condition 1's accumulator identity. The resolved
/// places are condition 2's collection identity: sibling fields of one struct
/// are distinct, while a whole-parent or alias access overlaps a mapped root
/// and must be accounted for.
struct ReadOccurrence {
    binding: BindingId,
    places: Vec<ResolvedPlace>,
}

/// One accepted accumulate statement: `set a = a (+) e` with `(+)` admitted.
struct Accumulate {
    binding: BindingId,
    combine: LoopCombine,
    statement: NodePath,
}

struct Survey<'check, 'run> {
    program: &'run Program<'check>,
    places: &'run PlaceMap,
    /// Successful source obligations already computed by [ENT]. Permission
    /// consumes their disposition by source-node identity and never reruns
    /// the underlying proof.
    obligations: &'run [ObligationOutcome],
    /// The counted loop under judgment, so a `break` that closes it is told
    /// apart from one that closes a loop opened inside it.
    outer_loop: CheckedLoopId,
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

impl<'check> Survey<'check, '_> {
    /// Records every binding the body introduces, and every loop it opens,
    /// before anything is judged against those sets.
    fn introduce(&mut self, statements: &'check [CheckedStatement]) {
        collect_introduced(statements, &mut self.introduced);
        collect_inner_loops(statements, &mut self.inner_loops);
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
        for statement in statements {
            if let Some(node) = statement_node(statement) {
                self.cite = node.clone();
            }
            self.statement(statement, initializers);
            let inside = initializers
                + usize::from(matches!(statement, CheckedStatement::ValueMatchLet { .. }));
            for nested in nested_bodies(statement) {
                self.walk(nested, inside);
            }
        }
    }

    /// One body statement. The match is exhaustive on purpose: every form is
    /// either given a footprint here or refused here.
    fn statement(&mut self, statement: &'check CheckedStatement, initializers: usize) {
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
            // [CALL-4] a binder list writes more than one place in one
            // statement. The iteration footprint below describes one written
            // target per statement, so this form is refused rather than given
            // a footprint that does not describe it.
            CheckedStatement::DestructuringLet { .. } => {
                self.refuse_form("a statement that binds an ordered result list");
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
                    self.shared.get_or_insert(node_path.clone());
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
            CheckedStatement::Return { .. } => self.leaves("a return"),
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
            CheckedStatement::PropagateLet { .. } => self.leaves("a propagate"),
            CheckedStatement::Break { target, .. } => {
                if !self.inner_loops.contains(&target.0) || target.0 == self.outer_loop.0 {
                    self.leaves("a break");
                }
            }
            CheckedStatement::Match { scrutinee, .. }
            | CheckedStatement::ValueMatchLet { scrutinee, .. } => self.expression(scrutinee),
            // A nested loop is judged on its own terms elsewhere; here its
            // endpoint atoms are two ordinary reads this iteration performs.
            // No rule joins two index ranges into one iteration space.
            CheckedStatement::CountedRange { lower, upper, .. } => {
                self.expression(lower);
                self.expression(upper);
            }
            CheckedStatement::Loop { .. } => {}
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
            if self.is_iteration_own(&write.place) {
                continue;
            }
            if self.record_range_write(&write.place) {
                continue;
            }
            if let Some(map) = affine_map
                && let Some(root) = element_root(&write.place)
            {
                // One affine map per resolved root: two different maps can
                // cross between iterations even where each is injective by
                // itself. This fixed rule performs no pairwise range search.
                if self
                    .element_writes
                    .iter()
                    .any(|written| same_element_root(&written.root, &root) && written.map != map)
                {
                    self.shared.get_or_insert(node.clone());
                }
                self.element_writes.push(ProvenElementWrite {
                    root,
                    statement: node.clone(),
                    map,
                });
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

    /// Reads the exact single-binder affine image [ENT] retained beside this
    /// subscript's successful [OP-4] outcome, refusing a `Ring` base.
    ///
    /// Permission neither evaluates the source expression nor reruns proof:
    /// absence of this checked evidence fails closed. Whether the subscript's
    /// root is an own binding or a reference parameter whose row declares the
    /// write was already decided when [SET-1] formed a writable target.
    fn proven_affine_map(&self, target: &CheckedSetTarget) -> Option<ProvedAffineIndexMap> {
        let obligation = match target {
            // [REF-4] the outer position selects from a range. A suffix may
            // select a nested Array or Slots element; PAR-2 consumes the
            // innermost written element's retained affine map, as it does for
            // an ordinary typed storage path.
            CheckedSetTarget::RangeIndex(target) => {
                if let Some(index) = target.path.iter().rev().find_map(|step| match step {
                    CheckedPlaceStep::Subscript(index) => Some(index),
                    CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
                }) {
                    if checked_type_is_ring(index.base_type) {
                        return None;
                    }
                    &index.obligation
                } else {
                    &target.obligation
                }
            }
            CheckedSetTarget::Storage(target) => {
                let index = target.path.iter().rev().find_map(|step| match step {
                    CheckedPlaceStep::Subscript(index) => Some(index),
                    CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
                })?;
                if checked_type_is_ring(index.base_type) {
                    return None;
                }
                &index.obligation
            }
            // A whole-place target is no element, and the view element store
            // has no v0.60 subject.
            CheckedSetTarget::Place(_) => return None,
        };
        self.proven_affine_map_at(obligation)
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
            .find(|map| map.loop_id == self.outer_loop && map.coefficient != 0)
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
                partition.loop_id == self.outer_loop
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
        let occurrence = match expression {
            // Forming a reference reads no content, but naming an
            // accumulator outside its one combine operand still violates
            // PAR-2's occurrence restriction. Keep the occurrence without
            // inventing an element-read footprint for address formation.
            CheckedExpression::BorrowAddressed { root, .. } => {
                if let Some(binding) = root.binding() {
                    self.reads.push(ReadOccurrence {
                        binding,
                        places: Vec::new(),
                    });
                }
                None
            }
            CheckedExpression::BorrowRangeIndex { place, .. } => {
                self.reads.push(ReadOccurrence {
                    binding: place.root.binding,
                    places: Vec::new(),
                });
                None
            }
            CheckedExpression::ContainerMeasure { root, .. } => root
                .binding()
                .map(|binding| (binding, self.places.resolve(root.root, &container_steps(root)))),
            // A subscripted storage read: its own discharged [OP-4] image is
            // what puts it in the element family, and a `Ring` base is
            // refused that position [WIN-1].
            CheckedExpression::ReadStorage { root, .. } => {
                let places = self.places.resolve(root.root, &container_steps(root));
                if let Some(index) = root.path.iter().rev().find_map(|step| match step {
                    CheckedPlaceStep::Subscript(index) => Some(index),
                    CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
                }) && !checked_type_is_ring(index.base_type)
                    && let Some(map) = self.proven_affine_map_at(&index.obligation)
                {
                    for place in &places {
                        if let Some(root) = element_root(place) {
                            self.element_reads.push(ProvenElementRead { root, map });
                        }
                    }
                }
                root.binding().map(|binding| (binding, places))
            }
            CheckedExpression::Binding { binding, .. }
            | CheckedExpression::DerefAddressed { binding, .. } => Some((
                *binding,
                self.places.resolve(PlaceRoot::Binding(*binding), &[]),
            )),
            // [REF-4, MSR-2] a read through a range reference reads the path
            // the reference names; its own offset is this node's child.
            CheckedExpression::RangeMeasure { root, .. } => Some((
                root.binding,
                self.places.resolve(PlaceRoot::Binding(root.binding), &[]),
            )),
            CheckedExpression::RangeElementMeasure { place, .. } => Some((
                place.root.binding,
                self.places.resolve(
                    PlaceRoot::Binding(place.root.binding),
                    &place.place_path(),
                ),
            )),
            // A read through a range first selects its outer element and may
            // then select a nested Array or Slots element. As for a storage
            // read and range write, PAR-2 consumes the innermost selected
            // element's retained map; with no nested subscript, the range's
            // own admitted offset is that element map.
            CheckedExpression::RangeIndex { place, .. } => {
                let places = self.places.resolve(
                    PlaceRoot::Binding(place.root.binding),
                    &place.place_path(),
                );
                let obligation = match place.path.iter().rev().find_map(|step| match step {
                    CheckedPlaceStep::Subscript(index) => Some(index),
                    CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
                }) {
                    Some(index) if checked_type_is_ring(index.base_type) => None,
                    Some(index) => Some(&index.obligation),
                    None => Some(&place.obligation),
                };
                if let Some(map) = obligation.and_then(|path| self.proven_affine_map_at(path)) {
                    for resolved in &places {
                        if let Some(root) = element_root(resolved) {
                            self.element_reads.push(ProvenElementRead { root, map });
                        }
                    }
                }
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
                        self.element_reads.push(ProvenElementRead { root, map });
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
                        self.element_reads.push(ProvenElementRead { root, map });
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
            | CheckedExpression::EnumEquality { .. }
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
        if let Some((binding, places)) = occurrence {
            if places.is_empty() {
                self.unresolved.get_or_insert(self.cite.clone());
            } else {
                self.reads.push(ReadOccurrence { binding, places });
            }
        }
        for child in expression_children(expression) {
            self.record_reads(child);
        }
    }

    /// The caller places one expression transfers away by consuming an `own`
    /// value. A move out of enclosing storage is a write of it [PAR-1].
    fn moved_places(&mut self, value: &CheckedExpression, node: &NodePath) {
        let mut footprint = Footprint::default();
        collect_consumed_places(self.places, value, node, &mut footprint);
        self.record_writes(&footprint);
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
            let footprint = self.program.footprint(self.places, &projection);
            self.record_writes(&footprint);
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
    fn record_writes(&mut self, footprint: &Footprint) {
        if let Some(argument) = &footprint.unresolved {
            self.unresolved.get_or_insert(argument.clone());
        }
        // The declared row's reads only: this statement's own operand
        // expressions are walked by `record_reads`, and counting them again
        // here would double every source read occurrence.
        for read in &footprint.reads {
            self.reads.push(ReadOccurrence {
                binding: match read.place.root {
                    PlaceRoot::Binding(binding) => binding,
                    // A const root is never an accumulator; the place still
                    // takes part in condition 2.
                    PlaceRoot::Constant(_) => continue,
                },
                places: vec![read.place.clone()],
            });
        }
        for write in &footprint.writes {
            if self.is_iteration_own(&write.place) {
                continue;
            }
            if self.record_range_write(&write.place) {
                continue;
            }
            self.shared.get_or_insert(write.argument.clone());
        }
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
        } else if self.element_writes.is_empty()
            && !self.range_references.iter().any(|range| range.written)
        {
            None
        } else {
            Some(LoopActualization::IndependentMap)
        };
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
        }
    }

    fn denial(&self) -> Option<LoopDenial> {
        if let Some(form) = self.form {
            return Some(LoopDenial::BodyForm { form });
        }
        if let Some(denial) = self.carried_state() {
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
        if let Some(argument) = &self.unresolved {
            return Some(LoopDenial::UnresolvedWrite {
                argument: argument.clone(),
            });
        }
        self.exit.map(|edge| LoopDenial::Exit { edge })
    }

    /// Written origins must have one map, and every access to one of those
    /// origins must stay in that map's current-iteration extent. Read-only
    /// origins carry no cross-iteration write conflict.
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
                    self.places.overlaps(&oracle, &reference.origin, place) && !covered(place)
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

    /// "Every operand read through that same root binding must be a direct
    /// `Array` or `Slots` subscript whose own discharged [OP-4] result
    /// retains exactly the same a and b." A matching read and write image
    /// select the same element in each iteration; any whole-root read,
    /// different image, or unproved subscript leaves the counts unequal.
    fn element_map_coverage(&self) -> Option<LoopDenial> {
        let oracle = UnprovedSeparations;
        self.element_writes
            .iter()
            .find(|written| {
                let reads = self
                    .reads
                    .iter()
                    .flat_map(|read| &read.places)
                    .filter(|place| self.places.overlaps(&oracle, place, &written.root))
                    .count();
                let matching = self
                    .element_reads
                    .iter()
                    .filter(|read| {
                        same_element_root(&read.root, &written.root) && read.map == written.map
                    })
                    .count();
                reads != matching
            })
            .map(|written| LoopDenial::SharedWrite {
                argument: written.statement.clone(),
            })
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
        | CheckedStatement::DropExpression { node_path, .. } => Some(node_path),
        CheckedStatement::Proof(proof) => Some(&proof.node_path),
        CheckedStatement::Match { .. }
        | CheckedStatement::Loop { .. }
        | CheckedStatement::Break { .. } => None,
    }
}

/// The resolved place above a trailing index step: the root an affine element
/// map is stated over.
fn element_root(place: &ResolvedPlace) -> Option<ResolvedPlace> {
    matches!(place.path.last(), Some(PlaceStep::Index(_))).then(|| ResolvedPlace {
        root: place.root,
        path: place.path[..place.path.len() - 1].to_vec(),
    })
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

/// The combine of `set acc = <op>(acc, rest)`, when `op` is one of the ten
/// [PAR-2] admits and `rest` reaches `acc` nowhere.
fn combine_of(accumulator: BindingId, value: &CheckedExpression) -> Option<LoopCombine> {
    let (combine, arguments) = match value {
        CheckedExpression::IntegerOperation {
            operation,
            arguments,
            ..
        } => (integer_combine(*operation)?, arguments),
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

/// The seven integer operations [PAR-2] admits.
///
/// The list is closed and every entry is here for the same stated reason:
/// each is total, associative, and commutative on the complete value set of
/// its type and carries a two-sided identity, so regrouping its applications
/// produces the same bits. `+`, `+defined`, and `+checked` are associative in
/// Z and are still absent, because each application attaches a domain
/// obligation or a `Result` route that regrouping moves. `+sat` fails
/// associativity outright.
const fn integer_combine(operation: CheckedIntegerOperation) -> Option<LoopCombine> {
    Some(match operation {
        CheckedIntegerOperation::AddWrap => LoopCombine::AddWrap,
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
            CheckedStatement::CountedRange { binder, .. } => out.push(*binder),
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
        CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
            vec![body.as_slice()]
        }
        _ => Vec::new(),
    }
}
