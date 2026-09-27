//! [REF-1] resolved places and the [OWN-7] overlap relation.
//!
//! A resolved place is a root — a local variable, a parameter, or a named
//! const [CONST-2] — and the ordered steps below it: field selections,
//! `deref` of `Box` content [TYPE-7], enum payload steps, index steps, range
//! steps [REF-4], window parts [WIN-2] and measure reads [OP-15]. That is the
//! one path type in the checker; a reference variable is not storage of its
//! own, so resolving a place rooted at one replaces that root by the path the
//! reference names, recursively [REF-1].
//!
//! [OWN-7] is the single relation every consumer asks: the call-site pairwise
//! comparison [EFF-5], reference invalidation [REF-2], the [ENT-5] kills, and
//! the [PAR-1] adjacency footprints. Two places fail to overlap exactly when
//! some step of their common prefix provably selects two different storages,
//! and a pair no admitted family separates is overlapping. Nothing here may
//! grow a private second copy of that relation.
//!
//! Three of the separations [OWN-7] admits are proofs rather than syntax —
//! two index steps proved distinct by the fixed [ENT-6] families under
//! [MSR-4]'s disposition, two range steps proved disjoint by the four
//! non-strict orderings, and an index step proved outside a range step by
//! three. Those reach the entailment fragment through [`SeparationOracle`]
//! and nothing else. A path's captured index and endpoint values are immutable
//! once captured [REF-1, OWN-7], while the proof that separates two such
//! values is available only along the control-flow edges it dominates.

use crate::DeclarationId;

use super::model::{
    BindingId, CheckedConstantId, CheckedExpression, CheckedFunction, CheckedLoopId,
    CheckedMatchArm, CheckedMeasure, CheckedStatement, CheckedType, IntegerType,
};

/// The generation in which an index expression or range endpoint was
/// evaluated [REF-1].
///
/// An ordinary formation uses its source occurrence. A loop-header value
/// carried from an arbitrary earlier iteration uses a compiler-owned header
/// identity instead, because a source occurrence may evaluate to a different
/// value on every backedge. Within one generation the captured value is
/// immutable: later assignments to the variables used to form it do not
/// retarget the path. Two steps carrying one `CaptureId` therefore hold one
/// value only when that identity denotes the same generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CaptureId {
    Source(u32),
    LoopHeader {
        loop_id: CheckedLoopId,
        holder: BindingId,
        path: u32,
        position: u32,
    },
    ValueDetermined,
    SpellingDetermined,
    Unknown,
}

impl CaptureId {
    /// The identity of one source expression occurrence.
    pub(crate) const fn source(occurrence: u32) -> Self {
        Self::Source(occurrence)
    }

    /// One opaque value carried from an arbitrary prior iteration.
    const fn loop_carried(
        loop_id: CheckedLoopId,
        holder: BindingId,
        path: u32,
        position: u32,
    ) -> Self {
        Self::LoopHeader {
            loop_id,
            holder,
            path,
            position,
        }
    }
}

/// The occurrence a value-determined capture carries in a goal datum.
///
/// It stands for no source node: a literal and a const denote one value at
/// every occurrence, so the identity of the place they index is their value
/// and not where it was written [REF-1, ENT-2].
const VALUE_DETERMINED_CAPTURE: CaptureId = CaptureId::ValueDetermined;

/// The nonidentity marker for a value this relation cannot name.
///
/// Unlike a source occurrence, two uses of this marker are no evidence that
/// they evaluated to the same value.
const UNKNOWN_CAPTURE: CaptureId = CaptureId::Unknown;

/// The occurrence a binding-valued capture carries in a goal datum.
///
/// [ENT-2] decides term identity by spelling: "Two places are the same term
/// exactly when their roots resolve to the same declaration event and their
/// canonical source spellings are byte-identical." `rows[i].len` written in a
/// clause and written again in the body is therefore one term, and the
/// occurrence that evaluated `i` carries no identity of its own. What keeps
/// that sound is [MSR-2]: the offset's own support is part of every enclosing
/// measure term's support, so a write to `i` kills every term `i` occurs in
/// rather than silently retargeting one.
const SPELLING_DETERMINED_CAPTURE: CaptureId = CaptureId::SpellingDetermined;

/// How the entailment fragment reads one captured value [ENT-2].
///
/// A written literal and a named or generic const are values this relation
/// decides by itself. A binding read at the formation is a term the fragment
/// can reason about but this module cannot, and everything else is opaque to
/// both; each of the latter two is decided, if at all, by the fixed [ENT-6]
/// families through [`SeparationOracle`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CapturedTerm {
    /// A written integer literal.
    Literal(u64),
    /// The value a live `own` integer binding held at the formation.
    Binding(BindingId),
    /// The value a binding held at the formation, once a later write of that
    /// binding has made its spelling name another value [REF-1].
    Superseded(BindingId),
    /// An in-scope const [CONST-1, CONST-2], fixed at instantiation [FN-2].
    Const(DeclarationId),
    /// A computed value: the fragment sees the goal, not a term this module
    /// can name.
    Opaque,
}

/// The immutable value one index step or range endpoint captured [REF-1].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CapturedValue {
    pub(crate) capture: CaptureId,
    pub(crate) term: CapturedTerm,
}

impl CapturedValue {
    pub(crate) const fn new(capture: CaptureId, term: CapturedTerm) -> Self {
        Self { capture, term }
    }

    /// The offset of a write at an element position this version cannot name
    /// [MSR-3].
    ///
    /// This is a nonidentity marker rather than a shared occurrence: two
    /// unknown values never prove equality. No admitted family proves one
    /// distinct from anything either, so [OWN-7] still leaves indexed places
    /// overlapping, while two unknown range frames cannot expose later steps
    /// as though both were relative to one proved-identical frame.
    pub(crate) const fn unknown() -> Self {
        Self::new(UNKNOWN_CAPTURE, CapturedTerm::Opaque)
    }

    /// The identity this capture carries inside a goal datum [ENT-2].
    ///
    /// A goal datum is compared structurally, so a place written twice is one
    /// term only when its subscripts are one value there. A written literal
    /// and an in-scope const are value-determined: `rows[0_u64]` denotes one
    /// storage wherever it is written, exactly as [`Self::provably_same`]
    /// already decides. The occurrence that evaluated them therefore carries
    /// no identity of its own and is dropped, which is what makes
    /// `rows[0_u64].len` and the bound of `rows[0_u64][i]` one term [MSR-1].
    /// A binding read uses its declaration/spelling identity here, with writes
    /// invalidating facts supported by that binding. Captured storage identity
    /// in [`Self::provably_same`] still distinguishes its evaluations. Only an
    /// opaque offset, and a binding's offset after a later write of that
    /// binding, keep occurrence identity inside a goal datum.
    pub(crate) const fn goal_identity(self) -> Self {
        match self.term {
            CapturedTerm::Literal(_) | CapturedTerm::Const(_) => {
                Self::new(VALUE_DETERMINED_CAPTURE, self.term)
            }
            // [ENT-2] a binding is a spelling, and two byte-identical
            // spellings rooted at one declaration are one term. The
            // occurrence is dropped here and not in [`Self::provably_same`]:
            // term identity under-approximates aliasing while [OWN-7]'s
            // overlap relation over-approximates it, and a binding's two
            // reads may still straddle a write when the question is which
            // storage each selects.
            CapturedTerm::Binding(_) => Self::new(SPELLING_DETERMINED_CAPTURE, self.term),
            // The binding's spelling now names a later value, and only the
            // occurrence still names the one this capture read.
            CapturedTerm::Superseded(_) | CapturedTerm::Opaque => self,
        }
    }

    /// Whether the two captures hold one value on every execution.
    ///
    /// One source capture occurrence is one evaluation, but its identity is
    /// evidence only between equal term classes: a collision between a
    /// binding and a literal, or between two different bindings, proves
    /// nothing. Across two occurrences only the value-determined terms decide
    /// themselves; a binding's two reads may straddle a write to that binding,
    /// and an opaque value names no term at all.
    /// A value-determined term decides the pair by its own value, before the
    /// occurrence is consulted at all: two literals hold one value exactly
    /// when they are the same literal, whether or not one evaluation produced
    /// both, and [`Self::goal_identity`] drops the occurrence from such a
    /// capture, so the occurrence is no longer evidence there.
    pub(crate) fn provably_same(self, other: Self) -> bool {
        match (self.term, other.term) {
            (CapturedTerm::Literal(left), CapturedTerm::Literal(right)) => left == right,
            (CapturedTerm::Const(left), CapturedTerm::Const(right)) => left == right,
            (
                CapturedTerm::Binding(left) | CapturedTerm::Superseded(left),
                CapturedTerm::Binding(right) | CapturedTerm::Superseded(right),
            ) => left == right && self.capture == other.capture,
            (CapturedTerm::Opaque, CapturedTerm::Opaque) => {
                self.capture != UNKNOWN_CAPTURE && self.capture == other.capture
            }
            _ => false,
        }
    }

    /// The half of [OWN-7]'s index separation this module settles without a
    /// proof: two written literals of different value select two slots.
    /// Every other pair is the fixed [ENT-6] families' question.
    const fn literals_distinct(self, other: Self) -> bool {
        matches!(
            (self.term, other.term),
            (CapturedTerm::Literal(left), CapturedTerm::Literal(right)) if left != right
        )
    }

    /// The support one captured value contributes to every measure term of
    /// the place it occurs in [ENT-5]: the binding it read, where the
    /// binding's spelling still names that value.
    pub(crate) const fn support(self) -> Option<BindingId> {
        match self.term {
            CapturedTerm::Binding(binding) => Some(binding),
            CapturedTerm::Literal(_)
            | CapturedTerm::Const(_)
            | CapturedTerm::Superseded(_)
            | CapturedTerm::Opaque => None,
        }
    }
}

/// One captured range step's two endpoints [REF-4], both evaluated at the
/// formation under the obligation `lo <= hi` and `hi <= x.len`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CapturedRange {
    pub(crate) start: CapturedValue,
    pub(crate) end: CapturedValue,
}

impl CapturedRange {
    /// The range's one [MSR-1] measure as a mathematical value, where both
    /// endpoints are value-determined [REF-4]: `hi - lo`.
    ///
    /// A binding or computed endpoint denotes no value this relation can
    /// name, so the length is no constant there and the caller falls back to
    /// the opaque measure term, which only under-derives [ENT-1].
    pub(crate) const fn constant_length(self) -> Option<i128> {
        let (CapturedTerm::Literal(start), CapturedTerm::Literal(end)) =
            (self.start.term, self.end.term)
        else {
            return None;
        };
        (end as i128).checked_sub(start as i128)
    }

    /// The half of [OWN-7]'s index-and-range separation this module settles
    /// without a proof: written literals that put the index before the start
    /// or at or after the end, or make the range empty.
    const fn literally_excludes(self, index: CapturedValue) -> bool {
        match (self.start.term, self.end.term, index.term) {
            (CapturedTerm::Literal(start), CapturedTerm::Literal(end), _) if end <= start => true,
            (_, CapturedTerm::Literal(end), CapturedTerm::Literal(index)) if end <= index => true,
            (CapturedTerm::Literal(start), _, CapturedTerm::Literal(index)) => index < start,
            _ => false,
        }
    }
}

/// The four window parts [WIN-2]: vocabulary for effect rows and this
/// relation only, never a place a program reads, writes, or references
/// [TYPE-10].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum WindowPart {
    /// `r.next`, the append slot at index `r.len`.
    Next,
    /// `r.last`, the last filled slot at index `r.len - 1`.
    Last,
    /// `r.filled`, every slot below `r.len`.
    Filled,
    /// `r.free`, every slot from `r.len` up.
    Free,
}

impl WindowPart {
    /// The [WIN-2] member spelling of this part, which is how an effect row
    /// and every diagnostic over one name it.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Last => "last",
            Self::Filled => "filled",
            Self::Free => "free",
        }
    }
}

/// Root of a resolved place: a function-local binding — parameters, `let`
/// bindings of every right-hand form, counted binders and match binders share
/// the dense [`BindingId`] space — or a named const [CONST-2].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlaceRoot {
    Binding(BindingId),
    Constant(CheckedConstantId),
}

/// One step below a resolved place's root [REF-1, OWN-7].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlaceStep {
    /// One current target somewhere at or below the preceding anchor.
    /// The identity permits suffix comparison only for this same target;
    /// its cover is never an equality of independently selected places.
    Descendant(DescendantTarget),
    /// One struct field selection, by source ordinal.
    Field(u32),
    /// `deref` of `Box` content [TYPE-7]. It is an ordinary path step and is
    /// not erased: `deref(h).value` and `deref(h.value)` are two paths.
    Deref,
    /// One enum payload step, available under the refinement fact that the
    /// enum currently holds this variant [REF-1, ENT-3.S15].
    Payload { variant: u32, field: u32 },
    /// One [OP-4] subscript. The offset is logical; the slot it selects is
    /// `(r.head + i) mod r.cap`, which is [WIN-1]'s business and no rule of
    /// the overlap relation mentions.
    Index(CapturedValue),
    /// One range step [REF-4] with both endpoints captured at formation.
    Range(CapturedRange),
    /// One of [WIN-2]'s four named window parts.
    Part(WindowPart),
    /// One measure read [OP-15, MSR-1]: descriptor storage, never a slot.
    Measure(CheckedMeasure),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DescendantTarget {
    pub(crate) loop_id: CheckedLoopId,
    pub(crate) holder: BindingId,
    pub(crate) ty: CheckedType,
    pub(crate) range: bool,
    pub(crate) readonly: bool,
}

/// What one step pair establishes about the two places below it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StepSeparation {
    /// The two steps provably select two different storages, so the complete
    /// places do not overlap however their later steps read.
    Separate,
    /// The two steps select one storage, so the walk continues and a later
    /// step may still separate them.
    Same,
    /// The two steps may select one storage and no later step can separate
    /// them: either the pair is unseparated under two different frames, or
    /// one step is a region containing the other.
    Overlapping,
}

/// The three [OWN-7] separations that are proofs rather than syntax,
/// [WIN-2]'s conditional answers for an index or a range beside a window
/// part, and whether [WIN-2]'s fixed answers apply at all.
///
/// [OWN-7] decides two index steps "by the fixed [ENT-6] families under
/// [MSR-4]'s disposition", two range steps by four named non-strict
/// orderings and an index beside a range by three, so the relation cannot be
/// purely syntactic. This trait is the
/// one seam through which it reaches the entailment fragment; every
/// implementation answers from the fixed families and from nothing else, and
/// answering `false` to any of its questions is always sound because a pair no
/// admitted family discharges is overlapping. [WIN-2]'s fixed answers hold
/// only between places that read one `r.len`, so whether they apply is a
/// question every implementation answers for its own consumer rather than a
/// default this trait assumes.
///
/// The caller supplies the proof evidence available at the current program
/// point. Nothing in a resolved path can move under it: the captured index
/// and endpoint values are immutable mathematical values [OWN-7, REF-1], but
/// a proof relating those values is available only on edges it dominates.
pub(crate) trait SeparationOracle {
    /// Two index steps of one base whose offsets the fixed [ENT-6] families
    /// prove distinct.
    fn indices_distinct(&self, left: CapturedValue, right: CapturedValue) -> bool;

    /// Two range steps under containing paths that are identical step for
    /// step or differ only in index steps, proved disjoint by one of the four
    /// non-strict orderings `left.end <= right.start`,
    /// `right.end <= left.start`, `left.end <= left.start`, and
    /// `right.end <= right.start`; either empty-range ordering suffices
    /// because formation already proved start no greater than end [OWN-7].
    fn ranges_disjoint(&self, left: CapturedRange, right: CapturedRange) -> bool;

    /// [WIN-2]'s liveness: whether `i < r.len` is proved in the state the two
    /// places are compared in. Only a live `r[i]` is separated from `r.next`
    /// or `r.free`: an index at or past the length may name the append slot
    /// or a free one, and past the capacity of a `Ring` it wraps onto a
    /// filled one [WIN-1]. `window` is the resolved place the two steps hang
    /// below, which is the `r` the relation `r.len` is read of.
    ///
    /// Answering `false` is always sound. A subscript formed in the compared
    /// state discharged its [OP-4] obligation there, and a reference into a
    /// window stays valid only while that bound holds [OP-10], so both are
    /// live; a fact's place is live where the event's entry state derives the
    /// bound [ENT-5], and an index an effect row takes from another argument
    /// where the call's entry state derives it [EFF-5].
    fn index_is_live(&self, window: &ResolvedPlace, index: CapturedValue) -> bool;

    /// An index step and a range step under containing paths that are
    /// identical step for step or differ only in index steps, proved disjoint
    /// by one of the three orderings `index < range.start`,
    /// `range.end <= index` and `range.end <= range.start`: the index lies
    /// before the range, at or after its end, or the range is empty [OWN-7].
    fn index_outside_range(&self, index: CapturedValue, range: CapturedRange) -> bool;

    /// [WIN-2]'s row for a range of a window beside `r.next` or `r.free`:
    /// whether `range.end <= r.len`, or `range.end <= range.start`, is proved
    /// in the state the two places are compared in. `window` is as for
    /// [`Self::index_is_live`].
    fn range_within_length(&self, window: &ResolvedPlace, range: CapturedRange) -> bool;

    /// Whether both places are interpreted against one value of `r.len`.
    ///
    /// Every [WIN-2] part is defined relative to `r.len`, and a subscript is
    /// live only below it, so each fixed answer that separates a part from an
    /// index or from another part holds only when both steps read the same
    /// length. A consumer comparing places of one program state answers
    /// `true`. [PAR-1] compares two statements' places in the state before
    /// the first, where the second's subscript is live only after the
    /// statements before it have run; when one of those writes `r.len`, an
    /// index live only after an append is not distinct from the append slot,
    /// and the answer is `false`. Answering `false` is sound: the pair then
    /// overlaps. Answering `true` is sound only for places of one state.
    fn window_length_is_shared(&self, window: &ResolvedPlace) -> bool;
}

/// One resolved place [OWN-7]: its root and the ordered steps below it.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolvedPlace {
    pub(crate) root: PlaceRoot,
    pub(crate) path: Vec<PlaceStep>,
}

impl ResolvedPlace {
    /// The same root and static path shape at an arbitrary loop header.
    ///
    /// A loop-carried rebinding may evaluate every written index and range
    /// endpoint again on each backedge [REF-1]. The source occurrence is not
    /// the runtime generation: retaining it would let a prior iteration's
    /// path consume the affine image formed by the current iteration at that
    /// same source node. Give every captured position a deterministic opaque
    /// header identity instead. The root and every non-captured step remain
    /// exact, so aliases still resolve to the real storage and never become a
    /// fresh local place. A reference which is not rebound across this loop
    /// never takes this conversion and keeps its original captures.
    pub(crate) fn loop_carried(
        &self,
        loop_id: CheckedLoopId,
        holder: BindingId,
        path: u32,
    ) -> Result<Self, crate::SemanticCompilerFailure> {
        let mut carried = self.clone();
        let mut position = 0_u32;
        let mut next = || {
            let capture = CaptureId::loop_carried(loop_id, holder, path, position);
            position = position
                .checked_add(1)
                .ok_or(crate::SemanticCompilerFailure::CounterOverflow)?;
            Ok(CapturedValue::new(capture, CapturedTerm::Opaque))
        };
        for step in &mut carried.path {
            match step {
                PlaceStep::Index(index) => *index = next()?,
                PlaceStep::Range(range) => {
                    range.start = next()?;
                    range.end = next()?;
                }
                PlaceStep::Deref
                | PlaceStep::Descendant(_)
                | PlaceStep::Field(_)
                | PlaceStep::Payload { .. }
                | PlaceStep::Part(_)
                | PlaceStep::Measure(_) => {}
            }
        }
        Ok(carried)
    }

    /// The same path once `binding` has been written.
    ///
    /// An index this path captured from `binding` still holds the value the
    /// binding had when the path was formed [REF-1], but the binding's
    /// spelling now names another value, and an index term is interned by
    /// that spelling [`CapturedValue::goal_identity`]. Such an index keeps
    /// its occurrence, which still names the value it read, and loses the
    /// spelling, so no fact about the binding's new value reaches a term over
    /// this path. A range endpoint needs no conversion: its goal identity is
    /// already its formation capture.
    pub(crate) fn supersede_binding(&mut self, binding: BindingId) {
        for step in &mut self.path {
            if let PlaceStep::Index(index) = step
                && index.term == CapturedTerm::Binding(binding)
            {
                *index = CapturedValue::new(index.capture, CapturedTerm::Superseded(binding));
            }
        }
    }

    /// The same path once the index `capture` read from `binding` is
    /// superseded on another incoming edge of a join: the join cannot tell
    /// the edges apart, so after it the spelling names that value on none
    /// [`Self::supersede_binding`].
    pub(crate) fn supersede_capture(&mut self, capture: CaptureId, binding: BindingId) {
        for step in &mut self.path {
            if let PlaceStep::Index(index) = step
                && index.capture == capture
                && index.term == CapturedTerm::Binding(binding)
            {
                *index = CapturedValue::new(capture, CapturedTerm::Superseded(binding));
            }
        }
    }

    /// The indices of this path whose binding's spelling still names them,
    /// each as its capture and that binding, which a later write of the
    /// binding supersedes [`Self::supersede_binding`].
    pub(crate) fn spelled_indices(&self) -> impl Iterator<Item = (CaptureId, BindingId)> + '_ {
        self.path.iter().filter_map(|step| match step {
            PlaceStep::Index(CapturedValue {
                capture,
                term: CapturedTerm::Binding(binding),
            }) => Some((*capture, *binding)),
            _ => None,
        })
    }

    /// The indices of this path a write of their binding superseded, each as
    /// its capture and that binding.
    pub(crate) fn superseded_indices(&self) -> impl Iterator<Item = (CaptureId, BindingId)> + '_ {
        self.path.iter().filter_map(|step| match step {
            PlaceStep::Index(CapturedValue {
                capture,
                term: CapturedTerm::Superseded(binding),
            }) => Some((*capture, *binding)),
            _ => None,
        })
    }

    /// The identity this place carries as an [ENT-2] term.
    ///
    /// A term is interned by its path, so `rows[0_u64].len` written twice is
    /// one measure term only when the two paths are equal there. A written
    /// literal and an in-scope const are value-determined, so their
    /// occurrence carries no identity of its own and
    /// [`CapturedValue::goal_identity`] drops it for an index. A range keeps
    /// its formation captures: retained affine endpoint images are keyed by
    /// that occurrence, so canonicalizing its binding endpoints would erase
    /// the key and could collide across formations.
    pub(crate) fn term_identity(mut self) -> Self {
        for step in &mut self.path {
            match step {
                PlaceStep::Index(offset) => *offset = offset.goal_identity(),
                PlaceStep::Range(_) => {}
                PlaceStep::Field(_)
                | PlaceStep::Descendant(_)
                | PlaceStep::Deref
                | PlaceStep::Payload { .. }
                | PlaceStep::Part(_)
                | PlaceStep::Measure(_) => {}
            }
        }
        self
    }

    /// The whole storage of one binding, with no selection below the root.
    pub(crate) const fn binding(binding: BindingId) -> Self {
        Self {
            root: PlaceRoot::Binding(binding),
            path: Vec::new(),
        }
    }

    /// One binding's storage with field selections below it, which is the
    /// shape a bare or projected place resolves to [REF-1].
    pub(crate) fn fields(binding: BindingId, fields: Vec<u32>) -> Self {
        Self {
            root: PlaceRoot::Binding(binding),
            path: fields.into_iter().map(PlaceStep::Field).collect(),
        }
    }

    /// One binding's storage with an already-resolved step path below it.
    pub(crate) const fn from_path(binding: BindingId, path: Vec<PlaceStep>) -> Self {
        Self {
            root: PlaceRoot::Binding(binding),
            path,
        }
    }

    /// One further [OP-4] subscript below this place, at the value its index
    /// expression captured [REF-1].
    pub(crate) fn push_subscript(&mut self, captured: CapturedValue) {
        self.path.push(PlaceStep::Index(captured));
    }

    /// The same place with field selections appended, for a caller that holds
    /// a plain field path.
    pub(crate) fn extend_fields(&mut self, fields: &[u32]) {
        self.path
            .extend(fields.iter().copied().map(PlaceStep::Field));
    }

    /// One place spelled as an optional leading `deref` [TYPE-7] followed by
    /// field selections, which is the shape most checked place nodes carry.
    pub(crate) fn spelled(root: PlaceRoot, deref: bool, fields: Vec<u32>) -> Self {
        let mut path = Vec::with_capacity(usize::from(deref) + fields.len());
        if deref {
            path.push(PlaceStep::Deref);
        }
        path.extend(fields.into_iter().map(PlaceStep::Field));
        Self { root, path }
    }

    /// Whether this place's path positively selects the same storage as, or a
    /// storage containing, `other`'s.
    ///
    /// This is positive target containment, used to preserve a captured
    /// target after a write below it. Invalidation instead asks the
    /// conservative question in [`Self::may_be_prefix_of`].
    pub(crate) fn contains(&self, other: &Self) -> bool {
        self.root == other.root
            && self.path.len() <= other.path.len()
            && self
                .path
                .iter()
                .zip(&other.path)
                .all(|(left, right)| steps_provably_same(*left, *right))
    }

    /// [REF-2]'s own question: whether this place is a *proper* prefix of
    /// `other`, which is what invalidates a reference whose path is `other`.
    /// Writing the storage at `other` or below it is a content write and
    /// invalidates nothing.
    #[cfg(test)]
    pub(crate) fn is_proper_prefix_of(&self, other: &Self) -> bool {
        self.path.len() < other.path.len() && self.contains(other)
    }

    /// Whether this place may select the same storage as a prefix of
    /// `other`, under [OWN-7]'s one overlap relation.
    ///
    /// Unlike [`Self::contains`], this is the conservative question used by
    /// [REF-2]: two indexed positions are one candidate prefix unless the
    /// supplied oracle proves them distinct. `include_equal` distinguishes a
    /// write, which preserves a reference to its exact target, from a move or
    /// release, which removes that target's value as well as its descendants.
    pub(crate) fn may_be_prefix_of(
        &self,
        oracle: &dyn SeparationOracle,
        other: &Self,
        include_equal: bool,
    ) -> bool {
        if self.has_descendant() || other.has_descendant() {
            if !places_overlap(oracle, self, other) {
                return false;
            }
            // A write at or below a captured target cannot remove that
            // target. This is positive target identity, not cover equality.
            if other.contains(self) {
                return include_equal && self.contains(other);
            }
            return true;
        }
        let depth_admitted = if include_equal {
            self.storage_depth() <= other.storage_depth()
        } else {
            self.storage_depth() < other.storage_depth()
        };
        depth_admitted && places_overlap(oracle, self, other)
    }

    /// How far below its root this path's storage lies, in half levels.
    ///
    /// A range selects a run of its base's elements, and the step after it
    /// selects one of those elements, so `x[a..b][i]` lies at the level of
    /// `x[k]` although it is one step longer; a run a path ends at lies
    /// between its base and the base's elements [REF-4]. Counting steps would
    /// let a write through a range frame miss a reference to the element it
    /// replaces [REF-2].
    fn storage_depth(&self) -> usize {
        let last = self.path.len().saturating_sub(1);
        self.path
            .iter()
            .enumerate()
            .map(|(position, step)| match step {
                PlaceStep::Range(_) => usize::from(position == last),
                _ => 2,
            })
            .sum()
    }

    pub(crate) fn has_descendant(&self) -> bool {
        self.path
            .iter()
            .any(|step| matches!(step, PlaceStep::Descendant(_)))
    }

    /// Exchange may tolerate equality, never possible proper ancestry.
    /// Positions in one fixed slot shape are equal or disjoint even when
    /// their index values are unknown. Unknown targets need actual identity.
    pub(crate) fn exchange_safe(&self, oracle: &dyn SeparationOracle, other: &Self) -> bool {
        !places_overlap(oracle, self, other)
            || (self.root == other.root
                && self.path.len() == other.path.len()
                && self.path.iter().zip(&other.path).all(|(left, right)| {
                    matches!((left, right), (PlaceStep::Index(_), PlaceStep::Index(_)))
                        || steps_provably_same(*left, *right)
                }))
    }

    /// The known prefix of the possible-location cover. Relative suffixes
    /// describe the selected target, not a narrower subtree anchor.
    pub(crate) fn cover_prefix(&self) -> &[PlaceStep] {
        let end = self
            .path
            .iter()
            .position(|step| matches!(step, PlaceStep::Descendant(_)))
            .unwrap_or(self.path.len());
        &self.path[..end]
    }
}

/// Whether two steps positively select one storage on every execution.
fn steps_provably_same(left: PlaceStep, right: PlaceStep) -> bool {
    match (left, right) {
        (PlaceStep::Descendant(left), PlaceStep::Descendant(right)) => left == right,
        (PlaceStep::Field(left), PlaceStep::Field(right)) => left == right,
        (PlaceStep::Deref, PlaceStep::Deref) => true,
        (
            PlaceStep::Payload {
                variant: left_variant,
                field: left_field,
            },
            PlaceStep::Payload {
                variant: right_variant,
                field: right_field,
            },
        ) => left_variant == right_variant && left_field == right_field,
        (PlaceStep::Index(left), PlaceStep::Index(right)) => left.provably_same(right),
        (PlaceStep::Range(left), PlaceStep::Range(right)) => {
            left.start.provably_same(right.start) && left.end.provably_same(right.end)
        }
        (PlaceStep::Part(left), PlaceStep::Part(right)) => left == right,
        (PlaceStep::Measure(left), PlaceStep::Measure(right)) => left == right,
        _ => false,
    }
}

/// [WIN-2]'s fixed answers for two window parts of one window.
///
/// `next` is the slot at index `len`, `last` the slot at index `len - 1`,
/// `filled` the slots below `len` and `free` the slots from `len` up, and
/// every cell below follows from those four definitions: `filled` and `free`
/// do not overlap, `next` is contained in `free`, `last` is contained in
/// `filled`, and the two singletons sit one index apart.
const fn parts_overlap(left: WindowPart, right: WindowPart) -> bool {
    use WindowPart::{Filled, Free, Last, Next};
    matches!(
        (left, right),
        (Next, Next)
            | (Last, Last)
            | (Filled, Filled)
            | (Free, Free)
            | (Next, Free)
            | (Free, Next)
            | (Last, Filled)
            | (Filled, Last)
    )
}

/// The [OWN-7] and [WIN-2] answer for one step pair.
fn separation(
    oracle: &dyn SeparationOracle,
    window: &ResolvedPlace,
    left: PlaceStep,
    right: PlaceStep,
) -> StepSeparation {
    match (left, right) {
        (PlaceStep::Descendant(left), PlaceStep::Descendant(right)) if left == right => {
            StepSeparation::Same
        }
        // Two field selections of different fields select two storages.
        (PlaceStep::Field(left), PlaceStep::Field(right)) => {
            if left == right {
                StepSeparation::Same
            } else {
                StepSeparation::Separate
            }
        }
        (PlaceStep::Deref, PlaceStep::Deref) => StepSeparation::Same,
        // Two payload steps of one variant selecting different fields of that
        // payload separate; two payload steps naming different variants of
        // one enum select the same storage and therefore overlap.
        (
            PlaceStep::Payload {
                variant: left_variant,
                field: left_field,
            },
            PlaceStep::Payload {
                variant: right_variant,
                field: right_field,
            },
        ) => {
            if left_variant != right_variant {
                StepSeparation::Overlapping
            } else if left_field == right_field {
                StepSeparation::Same
            } else {
                StepSeparation::Separate
            }
        }
        // Two index steps of one base. Unproved distinctness is not overlap:
        // both bases are the same storage, so a later step still separates
        // the two places under either answer, which is why the walk goes on.
        (PlaceStep::Index(left), PlaceStep::Index(right)) => {
            if left.literals_distinct(right) || oracle.indices_distinct(left, right) {
                StepSeparation::Separate
            } else {
                StepSeparation::Same
            }
        }
        // Two range steps. Every subsequent step is relative to its
        // containing range, so an unseparated pair of different frames stops
        // the walk: unequal subscripts below two different range steps never
        // establish separation by themselves.
        (PlaceStep::Range(left), PlaceStep::Range(right)) => {
            if left.start.provably_same(right.start) && left.end.provably_same(right.end) {
                StepSeparation::Same
            } else if oracle.ranges_disjoint(left, right) {
                StepSeparation::Separate
            } else {
                StepSeparation::Overlapping
            }
        }
        // Two parts are separated by their definitions against one `r.len`,
        // and by nothing when the two places read different lengths.
        (PlaceStep::Part(left), PlaceStep::Part(right)) => {
            if left == right {
                StepSeparation::Same
            } else if parts_overlap(left, right) || !oracle.window_length_is_shared(window) {
                StepSeparation::Overlapping
            } else {
                StepSeparation::Separate
            }
        }
        // A live `r[i]`, one whose `i < r.len` is proved where the two places
        // are compared, never overlaps `r.next` or `r.free`; any other `r[i]`
        // overlaps both, and every `r[i]` overlaps `r.last` and `r.filled`,
        // because a row's `r.last` stands for every slot its callee's
        // successive `take_back` calls reach [WIN-2]. Liveness bounds the
        // index by the length the part is defined against only when both
        // read one length.
        (PlaceStep::Index(_), PlaceStep::Part(_)) | (PlaceStep::Part(_), PlaceStep::Index(_))
            if !oracle.window_length_is_shared(window) =>
        {
            StepSeparation::Overlapping
        }
        (PlaceStep::Index(index), PlaceStep::Part(part))
        | (PlaceStep::Part(part), PlaceStep::Index(index)) => {
            let separate = match part {
                WindowPart::Next | WindowPart::Free => oracle.index_is_live(window, index),
                WindowPart::Last | WindowPart::Filled => false,
            };
            if separate {
                StepSeparation::Separate
            } else {
                StepSeparation::Overlapping
            }
        }
        // An index and a range of one base: separated when the index is
        // proved before the range or at or after its end, or the range
        // empty [OWN-7]. Unproved, the index may lie inside the range.
        (PlaceStep::Index(index), PlaceStep::Range(range))
        | (PlaceStep::Range(range), PlaceStep::Index(index)) => {
            if range.literally_excludes(index) || oracle.index_outside_range(index, range) {
                StepSeparation::Separate
            } else {
                StepSeparation::Overlapping
            }
        }
        // A range of a window proved to end at or below `r.len`, or proved
        // empty, never overlaps `r.next` or `r.free`; every range overlaps
        // `r.last` and `r.filled`, as every slot does [WIN-2]. The bound is
        // by the length the part is defined against only when both places
        // read one length.
        (PlaceStep::Range(_), PlaceStep::Part(_)) | (PlaceStep::Part(_), PlaceStep::Range(_))
            if !oracle.window_length_is_shared(window) =>
        {
            StepSeparation::Overlapping
        }
        (PlaceStep::Range(range), PlaceStep::Part(part))
        | (PlaceStep::Part(part), PlaceStep::Range(range)) => {
            let separate = match part {
                WindowPart::Next | WindowPart::Free => oracle.range_within_length(window, range),
                WindowPart::Last | WindowPart::Filled => false,
            };
            if separate {
                StepSeparation::Separate
            } else {
                StepSeparation::Overlapping
            }
        }
        // Each measure is a distinct descriptor word and overlaps no slot
        // [WIN-2, MSR-2]. A write of len does not write cap or head.
        (PlaceStep::Measure(left), PlaceStep::Measure(right)) => {
            if left == right {
                StepSeparation::Same
            } else {
                StepSeparation::Separate
            }
        }
        (PlaceStep::Measure(_), PlaceStep::Index(_) | PlaceStep::Range(_) | PlaceStep::Part(_))
        | (PlaceStep::Index(_) | PlaceStep::Range(_) | PlaceStep::Part(_), PlaceStep::Measure(_)) => {
            StepSeparation::Separate
        }
        // Everything left is a pair no admitted family discharges, so the
        // pair is overlapping [OWN-7].
        _ => StepSeparation::Overlapping,
    }
}

/// [OWN-7] over two complete resolved paths that share a root.
fn paths_overlap(
    oracle: &dyn SeparationOracle,
    place: &ResolvedPlace,
    left: &[PlaceStep],
    right: &[PlaceStep],
) -> bool {
    for (depth, (left, right)) in left.iter().zip(right).enumerate() {
        // The window a [WIN-2] answer is read of is the place the two steps
        // hang below, which is the common prefix walked so far.
        let window = ResolvedPlace {
            root: place.root,
            path: place.path[..depth].to_vec(),
        };
        match separation(oracle, &window, *left, *right) {
            StepSeparation::Separate => return false,
            StepSeparation::Overlapping => return true,
            StepSeparation::Same => {}
        }
    }
    // One path is a prefix of the other, which is exactly [OWN-7]'s overlap.
    true
}

/// [OWN-7]: whether two resolved places overlap, without a memo.
///
/// Two places rooted at different local variables, parameters or consts are
/// two storages, which is the first separation [OWN-7] admits; below a common
/// root the walk asks each step pair in turn.
pub(crate) fn places_overlap(
    oracle: &dyn SeparationOracle,
    left: &ResolvedPlace,
    right: &ResolvedPlace,
) -> bool {
    left.root == right.root && paths_overlap(oracle, left, &left.path, &right.path)
}

/// [EFF-5] whether two places overlap whatever values their index and range
/// positions take.
///
/// This is the ordinary [OWN-7] walk asked with every position question
/// answered by the values alone: two positions that are not provably the
/// same value may be distinct, a slot may lie below `r.len` [WIN-2], and a
/// range may be empty or lie apart from an index, from another range, or
/// from `r.len` [OWN-7, WIN-2]. The places overlap at every position exactly
/// when the walk still finds them overlapping: one is a prefix of the other,
/// or they first differ at a pair OWN-7 and WIN-2 fix as overlapping, such as
/// two payload steps naming different variants or a slot or a range against
/// `r.last` or `r.filled`.
pub(crate) fn overlaps_at_every_position(left: &ResolvedPlace, right: &ResolvedPlace) -> bool {
    if left.root != right.root {
        return false;
    }
    for (depth, (left_step, right_step)) in left.path.iter().zip(&right.path).enumerate() {
        let window = ResolvedPlace {
            root: left.root,
            path: left.path[..depth].to_vec(),
        };
        match separation(&EveryPositionSeparable, &window, *left_step, *right_step) {
            StepSeparation::Separate => return false,
            StepSeparation::Overlapping => return true,
            StepSeparation::Same => {}
        }
    }
    true
}

/// The oracle of [`overlaps_at_every_position`]: every question a position's
/// value could answer is answered by some values as separating.
///
/// It is not a proof oracle and grants no separation to any consumer that
/// judges one program state; it asks only whether an overlap depends on
/// position values at all.
struct EveryPositionSeparable;

impl SeparationOracle for EveryPositionSeparable {
    fn indices_distinct(&self, left: CapturedValue, right: CapturedValue) -> bool {
        !left.provably_same(right)
    }

    fn ranges_disjoint(&self, _left: CapturedRange, _right: CapturedRange) -> bool {
        true
    }

    /// Some values of an index lie below `r.len` [WIN-2].
    fn index_is_live(&self, _window: &ResolvedPlace, _index: CapturedValue) -> bool {
        true
    }

    fn index_outside_range(&self, _index: CapturedValue, _range: CapturedRange) -> bool {
        true
    }

    fn range_within_length(&self, _window: &ResolvedPlace, _range: CapturedRange) -> bool {
        true
    }

    /// The two places are one call's declared paths, both read at its entry
    /// [WIN-2].
    fn window_length_is_shared(&self, _window: &ResolvedPlace) -> bool {
        true
    }
}

/// The exact range pair that could separate two otherwise-overlapping paths.
///
/// This follows the ordinary [OWN-7] walk with no proof oracle. A candidate
/// exists only when the first unresolved divergence is two range frames; an
/// earlier syntactic separation needs no proof, while every other unresolved
/// divergence and every prefix overlap has no answer in the bounded range
/// family. Keeping this beside [`separation`] prevents a permission planner
/// from reimplementing the path relation with subtly different prefix or
/// nested-frame rules.
pub(crate) fn range_separation_candidate(
    left: &ResolvedPlace,
    right: &ResolvedPlace,
) -> Option<(CapturedRange, CapturedRange)> {
    if left.root != right.root {
        return None;
    }
    let oracle = UnprovedSeparations;
    for (depth, (left_step, right_step)) in left.path.iter().zip(&right.path).enumerate() {
        let window = ResolvedPlace {
            root: left.root,
            path: left.path[..depth].to_vec(),
        };
        match separation(&oracle, &window, *left_step, *right_step) {
            StepSeparation::Separate => return None,
            StepSeparation::Same => {}
            StepSeparation::Overlapping => {
                return match (*left_step, *right_step) {
                    (PlaceStep::Range(left), PlaceStep::Range(right)) => Some((left, right)),
                    _ => None,
                };
            }
        }
    }
    None
}

/// The oracle of a consumer that holds no proof state [OWN-7].
///
/// Every proof answer is `false`, which is the relation's own default: a pair
/// no admitted family discharges overlaps. The bounds by `r.len` are the
/// exception: every index or range these consumers meet beside a window part
/// was formed in the compared state under its bound, or is named by a
/// reference valid only while that bound holds. A consumer that needs the
/// proved half records the pair for the entailment fragment instead of
/// guessing here.
///
/// Every consumer compares places of one program state: an [OP-12] target
/// against its callee's row, a live reference against a write at that
/// write's entry [REF-2], the planner's candidate search, and one counted
/// body's own accesses [PAR-2], where a write of an enclosing window's
/// `r.len` has already denied permission. [WIN-2]'s fixed answers therefore
/// apply. A consumer comparing two states supplies its own oracle, as
/// [PAR-1] does, and so does [EFF-5]'s pairwise comparison, the one consumer
/// whose index may come from an effect row.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnprovedSeparations;

impl SeparationOracle for UnprovedSeparations {
    fn indices_distinct(&self, _left: CapturedValue, _right: CapturedValue) -> bool {
        false
    }

    fn ranges_disjoint(&self, _left: CapturedRange, _right: CapturedRange) -> bool {
        false
    }

    /// Every index these consumers meet beside a window part is live in the
    /// compared state [WIN-2]. A part is named only by an effect row, so the
    /// index is the other place's: an [OP-12] target and a counted body's
    /// access are subscripts formed in that state, which discharged [OP-4]
    /// there, and a reference target is valid only while the bound it was
    /// formed under holds [OP-10, REF-2].
    fn index_is_live(&self, _window: &ResolvedPlace, _index: CapturedValue) -> bool {
        true
    }

    fn index_outside_range(&self, _index: CapturedValue, _range: CapturedRange) -> bool {
        false
    }

    /// Every range these consumers meet beside a window part lies within the
    /// length of the compared state, as every such index is live there: a
    /// range formed in that state discharged its [REF-4] bound
    /// `hi <= r.len` there, and a range reference stays valid only while
    /// that bound holds [REF-2].
    fn range_within_length(&self, _window: &ResolvedPlace, _range: CapturedRange) -> bool {
        true
    }

    /// Every consumer of this oracle compares places of one state.
    fn window_length_is_shared(&self, _window: &ResolvedPlace) -> bool {
        true
    }
}

/// What one binding names when it is a reference variable [REF-1].
///
/// Origins are already resolved storage paths, including immutable incoming
/// parameter anchors. They are not edges to another reference holder's summary.
#[derive(Clone, Debug, Default)]
pub(crate) struct BindingSummary {
    pub(crate) ty: Option<CheckedType>,
    /// Every target established for this holder during the final structural
    /// walk. This function-wide inventory is not its target at a particular use.
    pub(crate) reference_paths: Vec<ResolvedPlace>,
    /// References have no storage of their own. Keep their kind even when an
    /// incomplete summary supplies no target, so resolution fails closed.
    pub(crate) reference: bool,
    pub(crate) reference_unknown: bool,
}

/// How one expression names caller storage [REF-1, REF-4]: the written root
/// and steps a consumer resolves through its own place map, the steps each
/// resolved member takes below that, and the form that names them.
///
/// The entailment flow, the permission judgments and the place map read this
/// one classification instead of each matching expression shapes, and it
/// lists every expression form, so a new form is classified here before any
/// consumer can miss it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NamedPlace {
    pub(crate) root: PlaceRoot,
    pub(crate) steps: Vec<PlaceStep>,
    /// Appended to each resolved member: a range formation's own step, or a
    /// range element's index and path below its range reference.
    pub(crate) suffix: Vec<PlaceStep>,
    pub(crate) form: NamingForm,
}

/// Which written form names a [`NamedPlace`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NamingForm {
    /// A binding named whole: a reference names its referent, any other
    /// binding its own storage.
    Binding(BindingId),
    /// `deref(r)`, a field projection or a Box-content take, reading the
    /// storage it names.
    Read,
    /// A `borrow_expr` written at the expression, a range element's included
    /// [REF-1].
    Borrow,
    /// A range formed at the expression [REF-4].
    Range,
}

impl NamedPlace {
    /// Each resolved member with the suffix below it. An unresolved written
    /// place is kept as written when `keep_unresolved` asks for it: an
    /// overlap treats that unknown target conservatively, where an empty
    /// candidate set would lose it.
    pub(crate) fn resolve(&self, places: &PlaceMap, keep_unresolved: bool) -> Vec<ResolvedPlace> {
        let mut resolved = places.resolve(self.root, &self.steps);
        if resolved.is_empty() && keep_unresolved {
            resolved.push(ResolvedPlace {
                root: self.root,
                path: self.steps.clone(),
            });
        }
        for place in &mut resolved {
            place.path.extend(self.suffix.iter().cloned());
        }
        resolved
    }
}

/// The caller storage `expression` names, or `None` for a form that names
/// none.
pub(crate) fn named_place(expression: &CheckedExpression) -> Option<NamedPlace> {
    let named = |root, steps, suffix, form| {
        Some(NamedPlace {
            root,
            steps,
            suffix,
            form,
        })
    };
    match expression {
        CheckedExpression::Binding { binding, .. } => named(
            PlaceRoot::Binding(*binding),
            Vec::new(),
            Vec::new(),
            NamingForm::Binding(*binding),
        ),
        CheckedExpression::DerefAddressed { binding, .. }
        | CheckedExpression::BoxTake { binding, .. } => named(
            PlaceRoot::Binding(*binding),
            Vec::new(),
            Vec::new(),
            NamingForm::Read,
        ),
        CheckedExpression::Project {
            binding, fields, ..
        } => named(
            PlaceRoot::Binding(*binding),
            fields.iter().copied().map(PlaceStep::Field).collect(),
            Vec::new(),
            NamingForm::Read,
        ),
        CheckedExpression::BorrowAddressed { root, .. } => {
            named(root.root, root.place_path(), Vec::new(), NamingForm::Borrow)
        }
        CheckedExpression::BorrowRangeIndex { place, .. } => named(
            PlaceRoot::Binding(place.root.binding),
            Vec::new(),
            place.place_path(),
            NamingForm::Borrow,
        ),
        CheckedExpression::RangeOf {
            source, captured, ..
        } => {
            let (root, steps) = source.place();
            named(
                root,
                steps,
                vec![PlaceStep::Range(*captured)],
                NamingForm::Range,
            )
        }
        CheckedExpression::Constant(_)
        | CheckedExpression::NamedConstant { .. }
        | CheckedExpression::UserCall { .. }
        | CheckedExpression::IntegerOperation { .. }
        | CheckedExpression::FloatOperation { .. }
        | CheckedExpression::NumericConversion { .. }
        | CheckedExpression::Reinterpret { .. }
        | CheckedExpression::BooleanOperation { .. }
        | CheckedExpression::EnumEquality { .. }
        | CheckedExpression::ArrayMeasure { .. }
        | CheckedExpression::ArrayIndex { .. }
        | CheckedExpression::BufferMeasure { .. }
        | CheckedExpression::RangeMeasure { .. }
        | CheckedExpression::RangeElementMeasure { .. }
        | CheckedExpression::RangeIndex { .. }
        | CheckedExpression::ContainerMeasure { .. }
        | CheckedExpression::ReadStorage { .. }
        | CheckedExpression::BufferIndex { .. }
        | CheckedExpression::BoxDeref { .. }
        | CheckedExpression::ConstructStruct { .. }
        | CheckedExpression::ConstructEnum { .. }
        | CheckedExpression::ProjectValue { .. } => None,
    }
}

/// Dense per-binding types and the structural checker's resolved origins.
#[derive(Debug, Default)]
pub(crate) struct PlaceMap {
    bindings: Vec<BindingSummary>,
}

impl PlaceMap {
    pub(crate) fn for_function(function: &CheckedFunction) -> Self {
        let mut map = Self::default();
        for parameter in &function.parameters {
            let summary = map.summary_mut(parameter.binding);
            summary.ty = Some(parameter.ty);
            if parameter.mode.is_reference() {
                summary.reference = true;
                summary.reference_paths = vec![ResolvedPlace::binding(parameter.binding)];
            }
        }
        map.bindings.resize_with(
            map.bindings.len().max(function.reference_origins.len()),
            BindingSummary::default,
        );
        for (summary, paths) in map.bindings.iter_mut().zip(&function.reference_origins) {
            if !paths.is_empty() {
                summary.reference = true;
                summary.reference_paths.clone_from(paths);
            }
        }
        map.collect_block_bindings(function.body.as_deref().unwrap_or_default());
        for summary in &mut map.bindings {
            summary.reference_unknown = summary.reference && summary.reference_paths.is_empty();
        }
        map
    }

    /// [OWN-7]: whether the two resolved places overlap.
    pub(crate) fn overlaps(
        &self,
        oracle: &dyn SeparationOracle,
        left: &ResolvedPlace,
        right: &ResolvedPlace,
    ) -> bool {
        if left.root != right.root {
            return false;
        }
        places_overlap(oracle, left, right)
    }

    fn summary_mut(&mut self, binding: BindingId) -> &mut BindingSummary {
        let index = binding.0 as usize;
        if self.bindings.len() <= index {
            self.bindings.resize(index + 1, BindingSummary::default());
        }
        &mut self.bindings[index]
    }

    pub(crate) fn summary(&self, binding: BindingId) -> Option<&BindingSummary> {
        self.bindings.get(binding.0 as usize)
    }

    pub(crate) fn is_reference(&self, binding: BindingId) -> bool {
        self.summary(binding).is_some_and(|summary| {
            summary.reference || summary.reference_unknown || !summary.reference_paths.is_empty()
        })
    }

    /// The binding a use of a reference variable reads besides the path it
    /// names: the variable holds what its formation captured, the range
    /// endpoints and index values among them, and the `let` that forms it
    /// or a `set` that rebinds it writes it [REF-1, PAR-1].
    ///
    /// A reference parameter that is never rebound names exactly the place
    /// its own binding stands for, so it has no separate holder, and neither
    /// has an owned binding. A rebound parameter's binding is both its holder
    /// and the entry place it names, so reading it overlaps every access
    /// through the parameter, which denies more than PAR-1 asks. A reference
    /// whose inventory is unknown is read as well, failing closed.
    pub(crate) fn reference_holder(&self, binding: BindingId) -> Option<ResolvedPlace> {
        let summary = self.summary(binding)?;
        let holder = ResolvedPlace::binding(binding);
        (summary.reference && summary.reference_paths.as_slice() != std::slice::from_ref(&holder))
            .then_some(holder)
    }

    /// Resolves a written holder and suffix through its complete origin inventory.
    ///
    /// Stored paths are already resolved. Looking their roots up again would
    /// reinterpret an incoming parameter's entry storage through the same
    /// parameter's later rebindings. The structural walk owns reference flow;
    /// neither recursive expansion nor another origin fixed point belongs here.
    pub(crate) fn resolve(&self, root: PlaceRoot, steps: &[PlaceStep]) -> Vec<ResolvedPlace> {
        let mut resolved = match root {
            PlaceRoot::Constant(_) => vec![ResolvedPlace {
                root,
                path: Vec::new(),
            }],
            PlaceRoot::Binding(binding) => match self.summary(binding) {
                Some(summary) if summary.reference_unknown => Vec::new(),
                Some(summary) if summary.reference || !summary.reference_paths.is_empty() => {
                    summary.reference_paths.clone()
                }
                _ => vec![ResolvedPlace::binding(binding)],
            },
        };
        for place in &mut resolved {
            place.path.extend_from_slice(steps);
        }
        resolved
    }

    /// Collects types and reference kinds only. Origin paths come from the
    /// final structural traversal, including every payload and delivery edge.
    fn collect_block_bindings(&mut self, statements: &[CheckedStatement]) {
        for statement in statements {
            match statement {
                CheckedStatement::Let { binding, value, .. } => {
                    let names_reference = match named_place(value).map(|named| named.form) {
                        Some(NamingForm::Borrow | NamingForm::Range) => true,
                        Some(NamingForm::Binding(named)) => self.is_reference(named),
                        Some(NamingForm::Read) | None => false,
                    };
                    let summary = self.summary_mut(*binding);
                    summary.ty = Some(value.ty());
                    summary.reference |= names_reference;
                }
                CheckedStatement::DestructuringLet { bindings, .. } => {
                    for (binding, ty, _) in bindings {
                        self.summary_mut(*binding).ty = Some(*ty);
                    }
                }
                CheckedStatement::PropagateLet {
                    binding, ok_type, ..
                } => {
                    self.summary_mut(*binding).ty = Some(*ok_type);
                }
                CheckedStatement::ValueMatchLet {
                    binding,
                    result_type,
                    result_mode,
                    arms,
                    ..
                } => {
                    let summary = self.summary_mut(*binding);
                    summary.ty = Some(*result_type);
                    summary.reference |= result_mode.is_reference();
                    self.collect_arm_bindings(arms);
                }
                CheckedStatement::Match { arms, .. } => {
                    self.collect_arm_bindings(arms);
                }
                CheckedStatement::Loop { body, .. } => self.collect_block_bindings(body),
                CheckedStatement::CountedRange { binder, body, .. } => {
                    self.summary_mut(*binder).ty = Some(CheckedType::Integer(IntegerType::U64));
                    self.collect_block_bindings(body);
                }
                _ => {}
            }
        }
    }

    fn collect_arm_bindings(&mut self, arms: &[CheckedMatchArm]) {
        for arm in arms {
            for binder in &arm.binders {
                let summary = self.summary_mut(binder.binding);
                summary.ty = Some(binder.ty);
                summary.reference |= binder.mode.is_reference();
            }
            self.collect_block_bindings(&arm.body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaptureId, CapturedRange, CapturedTerm, CapturedValue, PlaceMap, PlaceRoot, PlaceStep,
        ResolvedPlace, SeparationOracle, WindowPart,
    };
    use crate::semantic::model::{BindingId, CheckedLoopId, CheckedMeasure};

    fn literal(capture: u32, value: u64) -> CapturedValue {
        CapturedValue::new(CaptureId::source(capture), CapturedTerm::Literal(value))
    }

    fn opaque(capture: u32) -> CapturedValue {
        CapturedValue::new(CaptureId::source(capture), CapturedTerm::Opaque)
    }

    fn binding(capture: u32, binding: u32) -> CapturedValue {
        CapturedValue::new(
            CaptureId::source(capture),
            CapturedTerm::Binding(BindingId(binding)),
        )
    }

    fn place(binding: u32, path: &[PlaceStep]) -> ResolvedPlace {
        ResolvedPlace {
            root: PlaceRoot::Binding(BindingId(binding)),
            path: path.to_vec(),
        }
    }

    #[test]
    fn loop_carried_paths_keep_shape_but_not_source_capture_generation() {
        let original = place(
            7,
            &[
                PlaceStep::Field(2),
                PlaceStep::Index(literal(11, 0)),
                PlaceStep::Range(CapturedRange {
                    start: binding(12, 4),
                    end: opaque(13),
                }),
            ],
        );
        let carried = original
            .loop_carried(CheckedLoopId(3), BindingId(9), 1)
            .expect("small carried identity");
        let repeated = original
            .loop_carried(CheckedLoopId(3), BindingId(9), 1)
            .expect("the same header identity is deterministic");
        let other_holder = original
            .loop_carried(CheckedLoopId(3), BindingId(10), 1)
            .expect("small carried identity");

        assert_eq!(carried, repeated);
        assert_ne!(carried, other_holder);
        assert_eq!(carried.root, original.root);
        assert!(matches!(carried.path[0], PlaceStep::Field(2)));
        let PlaceStep::Index(carried_index) = carried.path[1] else {
            panic!("the index step must remain an index");
        };
        let PlaceStep::Index(original_index) = original.path[1] else {
            panic!("the source step must be an index");
        };
        assert_eq!(carried_index.term, CapturedTerm::Opaque);
        assert!(!carried_index.provably_same(original_index));
        let PlaceStep::Range(carried_range) = carried.path[2] else {
            panic!("the range step must remain a range");
        };
        assert_eq!(carried_range.start.term, CapturedTerm::Opaque);
        assert_eq!(carried_range.end.term, CapturedTerm::Opaque);
        assert_ne!(carried_range.start.capture, carried_range.end.capture);
    }

    /// Payload construction is covered through checked source by
    /// tests::references::nested_reference_payload_origins_keep_all_roots_and_suffixes.
    /// Missing or explicitly incomplete origins still fail closed here.
    #[test]
    fn unresolved_reference_summary_does_not_publish_a_partial_origin_set() {
        let mut map = PlaceMap::default();
        let binding = BindingId(0);
        map.summary_mut(binding).reference = true;
        assert!(map.resolve(PlaceRoot::Binding(binding), &[]).is_empty());

        let summary = map.summary_mut(binding);
        summary.reference_paths = vec![place(9, &[PlaceStep::Field(2)])];
        summary.reference_unknown = true;
        assert!(map.resolve(PlaceRoot::Binding(binding), &[]).is_empty());
    }

    /// An oracle that discharges exactly the separations a test names, so a
    /// test can distinguish "the relation asked the fragment" from "the
    /// relation decided it by syntax".
    struct Proves {
        indices: bool,
        ranges: bool,
        live: bool,
        index_outside: bool,
        range_within: bool,
        /// Whether the two places read one `r.len`, which is a question of
        /// state rather than proof: `true` for places of one state.
        one_length: bool,
    }

    impl SeparationOracle for Proves {
        fn indices_distinct(&self, _left: CapturedValue, _right: CapturedValue) -> bool {
            self.indices
        }

        fn ranges_disjoint(&self, _left: CapturedRange, _right: CapturedRange) -> bool {
            self.ranges
        }

        fn index_is_live(&self, _window: &ResolvedPlace, _index: CapturedValue) -> bool {
            self.live
        }

        fn index_outside_range(&self, _index: CapturedValue, _range: CapturedRange) -> bool {
            self.index_outside
        }

        fn range_within_length(&self, _window: &ResolvedPlace, _range: CapturedRange) -> bool {
            self.range_within
        }

        fn window_length_is_shared(&self, _window: &ResolvedPlace) -> bool {
            self.one_length
        }
    }

    /// The oracle a consumer with no ProofContext in hand would supply
    /// [OWN-8] for two places of one state: every proof answer `false`, which
    /// denies each separation the fixed families might have discharged, so
    /// the relation degrades to "overlapping" and never to "disjoint".
    const DENIED: Proves = Proves {
        indices: false,
        ranges: false,
        live: false,
        index_outside: false,
        range_within: false,
        one_length: true,
    };
    /// [OWN-7] two places fail to overlap when some step of their common
    /// prefix provably selects two different storages, and one place that is
    /// a prefix of the other overlaps it.
    #[test]
    fn different_roots_and_different_fields_separate() {
        let map = PlaceMap::default();
        let denied = DENIED;
        assert!(!map.overlaps(&denied, &place(0, &[]), &place(1, &[])));
        assert!(!map.overlaps(
            &denied,
            &place(0, &[PlaceStep::Field(0)]),
            &place(0, &[PlaceStep::Field(1)]),
        ));
        assert!(map.overlaps(&denied, &place(0, &[]), &place(0, &[PlaceStep::Field(1)])));
        assert!(map.overlaps(
            &denied,
            &place(0, &[PlaceStep::Field(1)]),
            &place(0, &[PlaceStep::Field(1), PlaceStep::Deref]),
        ));
    }

    /// [OWN-7] two payload steps of one variant selecting different fields of
    /// that payload separate; two payload steps naming different variants of
    /// one enum select the same storage and therefore overlap.
    #[test]
    fn payload_steps_separate_only_within_one_variant() {
        let map = PlaceMap::default();
        let denied = DENIED;
        let one = place(
            0,
            &[PlaceStep::Payload {
                variant: 0,
                field: 0,
            }],
        );
        let two = place(
            0,
            &[PlaceStep::Payload {
                variant: 0,
                field: 1,
            }],
        );
        let other = place(
            0,
            &[PlaceStep::Payload {
                variant: 1,
                field: 0,
            }],
        );
        assert!(!map.overlaps(&denied, &one, &two));
        assert!(map.overlaps(&denied, &one, &other));
    }

    /// [OWN-7] two index steps separate when two written literals differ, and
    /// otherwise only when the fixed [ENT-6] families discharge it.
    #[test]
    fn index_steps_reach_the_entailment_families() {
        let denied = DENIED;
        let proving = Proves {
            indices: true,
            ranges: false,
            live: false,
            index_outside: false,
            range_within: false,
            one_length: true,
        };
        let left = place(0, &[PlaceStep::Index(literal(0, 3))]);
        let right = place(0, &[PlaceStep::Index(literal(1, 4))]);
        assert!(!PlaceMap::default().overlaps(&denied, &left, &right));

        let left = place(0, &[PlaceStep::Index(opaque(0))]);
        let right = place(0, &[PlaceStep::Index(opaque(1))]);
        assert!(PlaceMap::default().overlaps(&denied, &left, &right));
        assert!(!PlaceMap::default().overlaps(&proving, &left, &right));
    }

    /// [OWN-7] an unproved index pair does not stop the walk: both steps
    /// select one base, so a later field selection still separates the two
    /// complete places whichever way the index question would have gone.
    #[test]
    fn a_field_below_an_unproved_index_still_separates() {
        let map = PlaceMap::default();
        let left = place(0, &[PlaceStep::Index(opaque(0)), PlaceStep::Field(0)]);
        let right = place(0, &[PlaceStep::Index(opaque(1)), PlaceStep::Field(1)]);
        assert!(!map.overlaps(&DENIED, &left, &right));
    }

    /// [OWN-7] every subsequent step is relative to its containing range, so
    /// unequal subscripts below two different range steps never establish
    /// separation by themselves, while a proved separation of the containing
    /// ranges separates all their descendants.
    #[test]
    fn steps_below_two_unseparated_ranges_never_separate() {
        let frame = |capture: u32| {
            PlaceStep::Range(CapturedRange {
                start: opaque(capture),
                end: opaque(capture + 100),
            })
        };
        let left = place(0, &[frame(0), PlaceStep::Index(literal(1, 0))]);
        let right = place(0, &[frame(1), PlaceStep::Index(literal(2, 7))]);
        assert!(PlaceMap::default().overlaps(&DENIED, &left, &right));

        let proving = Proves {
            indices: false,
            ranges: true,
            live: false,
            index_outside: false,
            range_within: false,
            one_length: true,
        };
        assert!(!PlaceMap::default().overlaps(&proving, &left, &right));
    }

    /// Two unresolved range frames can start at different offsets even when
    /// their endpoint representation uses the same unknown capture. Relative
    /// indices below them therefore cannot establish separation.
    #[test]
    fn relative_indices_do_not_separate_unresolved_range_frames() {
        let frame = PlaceStep::Range(CapturedRange {
            start: CapturedValue::unknown(),
            end: CapturedValue::unknown(),
        });
        let left = place(0, &[frame, PlaceStep::Index(literal(0, 0))]);
        let right = place(0, &[frame, PlaceStep::Index(literal(1, 1))]);

        assert!(PlaceMap::default().overlaps(&DENIED, &left, &right));
    }

    /// Goal canonicalization gives binding-valued endpoints one capture id,
    /// but different binding terms are still different unresolved frames.
    /// Their relative indices cannot be used as absolute separation evidence.
    #[test]
    fn canonical_capture_ids_do_not_merge_distinct_binding_range_frames() {
        let left_frame = PlaceStep::Range(CapturedRange {
            start: binding(0, 1).goal_identity(),
            end: binding(1, 2).goal_identity(),
        });
        let right_frame = PlaceStep::Range(CapturedRange {
            start: binding(2, 3).goal_identity(),
            end: binding(3, 4).goal_identity(),
        });
        let left = place(0, &[left_frame, PlaceStep::Index(literal(4, 0))]);
        let right = place(0, &[right_frame, PlaceStep::Index(literal(5, 1))]);

        assert!(PlaceMap::default().overlaps(&DENIED, &left, &right));
    }

    /// [WIN-2] a live `r[i]` never overlaps `r.next` or `r.free`; every
    /// `r[i]` overlaps `r.last` and `r.filled`, because a row's `r.last`
    /// stands for every slot its callee's successive `take_back` calls reach;
    /// an `r[i]` not proved live overlaps every part, because its index may
    /// be the length itself.
    #[test]
    fn window_part_answers_are_the_fixed_table() {
        let map = PlaceMap::default();
        let denied = DENIED;
        let live = Proves {
            live: true,
            ..DENIED
        };
        let index = place(0, &[PlaceStep::Index(opaque(0))]);
        let part = |part| place(0, &[PlaceStep::Part(part)]);

        assert!(!map.overlaps(&live, &index, &part(WindowPart::Next)));
        assert!(!map.overlaps(&live, &index, &part(WindowPart::Free)));
        assert!(map.overlaps(&live, &index, &part(WindowPart::Filled)));
        assert!(map.overlaps(&live, &index, &part(WindowPart::Last)));
        for window_part in [
            WindowPart::Next,
            WindowPart::Free,
            WindowPart::Filled,
            WindowPart::Last,
        ] {
            assert!(map.overlaps(&denied, &index, &part(window_part)));
            assert!(map.overlaps(&denied, &part(window_part), &index));
        }

        assert!(!map.overlaps(&denied, &part(WindowPart::Filled), &part(WindowPart::Free)));
        assert!(!map.overlaps(&denied, &part(WindowPart::Next), &part(WindowPart::Last)));
        assert!(map.overlaps(&denied, &part(WindowPart::Next), &part(WindowPart::Free)));
        assert!(map.overlaps(&denied, &part(WindowPart::Last), &part(WindowPart::Filled)));
    }

    /// [OWN-7] an index beside a range of one base is separated only when
    /// the fixed family proves the index outside the range, or written
    /// literals put it there. [WIN-2] a range proved within `r.len` is
    /// separated from `r.next` and `r.free`, never from `r.last` or
    /// `r.filled`, and from none of them when the two places read different
    /// lengths.
    #[test]
    fn an_index_or_a_part_beside_a_range_is_separated_by_its_bounds() {
        let map = PlaceMap::default();
        let range = |start, end| PlaceStep::Range(CapturedRange { start, end });
        let index = place(0, &[PlaceStep::Index(opaque(0))]);
        let span = place(0, &[range(opaque(1), opaque(2))]);
        assert!(map.overlaps(&DENIED, &index, &span));
        assert!(map.overlaps(&DENIED, &span, &index));
        let outside = Proves {
            index_outside: true,
            ..DENIED
        };
        assert!(!map.overlaps(&outside, &index, &span));
        assert!(!map.overlaps(&outside, &span, &index));
        let below_index = place(0, &[PlaceStep::Index(opaque(0)), PlaceStep::Field(1)]);
        assert!(!map.overlaps(&outside, &below_index, &span));

        // Written literals decide without a proof: an index at or after the
        // end, one before the start, and an empty range separate; an index
        // inside does not.
        let written = |start, end| place(0, &[range(literal(1, start), literal(2, end))]);
        let slot = |at| place(0, &[PlaceStep::Index(literal(0, at))]);
        assert!(!map.overlaps(&DENIED, &slot(3), &written(0, 2)));
        assert!(!map.overlaps(&DENIED, &slot(2), &written(0, 2)));
        assert!(!map.overlaps(&DENIED, &slot(0), &written(1, 4)));
        assert!(!map.overlaps(&DENIED, &slot(2), &written(2, 2)));
        assert!(map.overlaps(&DENIED, &slot(1), &written(0, 2)));

        let part = |part| place(0, &[PlaceStep::Part(part)]);
        let within = Proves {
            range_within: true,
            ..DENIED
        };
        let two_lengths = Proves {
            range_within: true,
            one_length: false,
            ..DENIED
        };
        for window_part in [
            WindowPart::Next,
            WindowPart::Free,
            WindowPart::Filled,
            WindowPart::Last,
        ] {
            assert!(map.overlaps(&DENIED, &span, &part(window_part)));
            assert!(map.overlaps(&DENIED, &part(window_part), &span));
            assert!(map.overlaps(&two_lengths, &span, &part(window_part)));
        }
        assert!(!map.overlaps(&within, &span, &part(WindowPart::Next)));
        assert!(!map.overlaps(&within, &part(WindowPart::Free), &span));
        assert!(map.overlaps(&within, &span, &part(WindowPart::Last)));
        assert!(map.overlaps(&within, &span, &part(WindowPart::Filled)));
    }

    /// [EFF-5] a range beside an index or beside `r.next` or `r.free`
    /// overlaps or not by its positions' values, so the pair is compared;
    /// beside `r.last` or `r.filled` it overlaps at every position, as a slot
    /// does, so it is not.
    #[test]
    fn a_range_overlaps_at_every_position_only_beside_last_or_filled() {
        let span = place(
            0,
            &[PlaceStep::Range(CapturedRange {
                start: opaque(1),
                end: opaque(2),
            })],
        );
        let index = place(0, &[PlaceStep::Index(opaque(0))]);
        let part = |part| place(0, &[PlaceStep::Part(part)]);
        assert!(!super::overlaps_at_every_position(&span, &index));
        assert!(!super::overlaps_at_every_position(&index, &span));
        for window_part in [WindowPart::Next, WindowPart::Free] {
            assert!(!super::overlaps_at_every_position(
                &span,
                &part(window_part)
            ));
            assert!(!super::overlaps_at_every_position(
                &part(window_part),
                &span
            ));
        }
        for window_part in [WindowPart::Last, WindowPart::Filled] {
            assert!(super::overlaps_at_every_position(&span, &part(window_part)));
            assert!(super::overlaps_at_every_position(&part(window_part), &span));
            assert!(super::overlaps_at_every_position(
                &index,
                &part(window_part)
            ));
        }
        assert!(super::overlaps_at_every_position(&span, &span));
    }

    /// Every [WIN-2] part is defined against `r.len`, and a live index is live
    /// below it, so none of the fixed answers separates two places that read
    /// different lengths [PAR-1]: an index live only after an append is the
    /// append slot. Two different indices are unaffected.
    #[test]
    fn window_parts_do_not_separate_places_of_two_lengths() {
        let map = PlaceMap::default();
        let two_lengths = Proves {
            indices: false,
            ranges: false,
            live: true,
            index_outside: false,
            range_within: false,
            one_length: false,
        };
        let index = place(0, &[PlaceStep::Index(opaque(0))]);
        let part = |part| place(0, &[PlaceStep::Part(part)]);

        for window_part in [
            WindowPart::Next,
            WindowPart::Free,
            WindowPart::Filled,
            WindowPart::Last,
        ] {
            assert!(map.overlaps(&two_lengths, &index, &part(window_part)));
            assert!(map.overlaps(&two_lengths, &part(window_part), &index));
        }
        assert!(map.overlaps(
            &two_lengths,
            &part(WindowPart::Filled),
            &part(WindowPart::Free)
        ));
        assert!(map.overlaps(
            &two_lengths,
            &part(WindowPart::Next),
            &part(WindowPart::Last)
        ));
        assert!(!map.overlaps(
            &two_lengths,
            &place(0, &[PlaceStep::Index(literal(0, 3))]),
            &place(0, &[PlaceStep::Index(literal(1, 4))]),
        ));
    }

    /// [WIN-2] the measure `r.len` is itself a write target and overlaps no
    /// slot.
    #[test]
    fn a_measure_overlaps_no_slot() {
        let map = PlaceMap::default();
        let denied = DENIED;
        let length = place(0, &[PlaceStep::Measure(CheckedMeasure::Length)]);
        assert!(!map.overlaps(&denied, &length, &place(0, &[PlaceStep::Index(opaque(0))])));
        assert!(!map.overlaps(
            &denied,
            &length,
            &place(0, &[PlaceStep::Part(WindowPart::Filled)]),
        ));
        assert!(map.overlaps(&denied, &length, &length));
    }

    /// [REF-2] writing the storage at a reference's path or below it is a
    /// content write; only a proper prefix invalidates it.
    #[test]
    fn only_a_proper_prefix_invalidates() {
        let reference = place(0, &[PlaceStep::Field(0), PlaceStep::Deref]);
        assert!(place(0, &[PlaceStep::Field(0)]).is_proper_prefix_of(&reference));
        assert!(!reference.is_proper_prefix_of(&reference));
        assert!(
            !place(
                0,
                &[PlaceStep::Field(0), PlaceStep::Deref, PlaceStep::Field(2)]
            )
            .is_proper_prefix_of(&reference)
        );
        assert!(!place(0, &[PlaceStep::Field(1)]).is_proper_prefix_of(&reference));
    }

    /// [REF-2, REF-4] a range frame descends no level of its own: `x[0..2][1]`
    /// selects `x[1]`, so it is a proper prefix of `x[1][0]` although both
    /// paths have two steps, and it is `x[1]`'s own level, not its prefix. A
    /// run a path ends at lies between its base and the base's elements.
    #[test]
    fn a_range_frame_descends_no_level_of_its_own() {
        let frame = PlaceStep::Range(CapturedRange {
            start: literal(0, 0),
            end: literal(1, 2),
        });
        let through_frame = place(0, &[frame, PlaceStep::Index(literal(2, 1))]);
        let element = place(0, &[PlaceStep::Index(literal(3, 1))]);
        let below = place(
            0,
            &[
                PlaceStep::Index(literal(4, 1)),
                PlaceStep::Index(literal(5, 0)),
            ],
        );
        assert!(through_frame.may_be_prefix_of(&DENIED, &below, false));
        assert!(!through_frame.may_be_prefix_of(&DENIED, &element, false));
        assert!(through_frame.may_be_prefix_of(&DENIED, &element, true));

        let run = place(0, &[frame]);
        assert!(run.may_be_prefix_of(&DENIED, &element, false));
        assert!(!element.may_be_prefix_of(&DENIED, &run, true));
    }

    /// [REF-1, ENT-2] after its binding is written, an index a path captured
    /// keeps the evaluation it read and loses the binding's spelling: its goal
    /// identity is its own occurrence, it is still the same value as that
    /// occurrence, and it no longer depends on the binding. An index of
    /// another binding keeps its spelling.
    #[test]
    fn a_written_binding_supersedes_the_index_a_path_captured_from_it() {
        let mut path = place(
            0,
            &[
                PlaceStep::Index(binding(0, 1)),
                PlaceStep::Index(binding(1, 2)),
            ],
        );
        assert_eq!(
            path.spelled_indices().collect::<Vec<_>>(),
            [
                (CaptureId::source(0), BindingId(1)),
                (CaptureId::source(1), BindingId(2))
            ]
        );
        path.supersede_binding(BindingId(1));
        let PlaceStep::Index(superseded) = path.path[0] else {
            unreachable!("the first step is an index");
        };
        let PlaceStep::Index(kept) = path.path[1] else {
            unreachable!("the second step is an index");
        };
        assert_eq!(superseded.term, CapturedTerm::Superseded(BindingId(1)));
        assert_eq!(superseded.goal_identity(), superseded);
        assert!(superseded.provably_same(binding(0, 1)));
        assert_eq!(superseded.support(), None);
        assert_eq!(kept, binding(1, 2));
        assert_ne!(kept.goal_identity(), kept);
        assert_eq!(
            path.spelled_indices().collect::<Vec<_>>(),
            [(CaptureId::source(1), BindingId(2))]
        );
    }
}
