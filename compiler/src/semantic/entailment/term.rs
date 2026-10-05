//! [ENT-2] terms: the closed vocabulary the L0 fragment relates.
//!
//! A term is a tracked place, a length term over a place, one of the two
//! private endpoint captures of a written counted range, the private commit
//! value of one `set` occurrence, a constant, a symbolic const-generic
//! parameter, or the distinguished zero term Z. Term identity is
//! declaration-anchored: two places are the same term exactly when
//! their roots resolve to the same declaration event — one [`BindingId`] in
//! one checked function — and their canonical spellings agree, which the
//! structural representation below captures byte-for-byte for canonical
//! source. Identity deliberately under-approximates aliasing; kills use the
//! [OWN-7] overlap relation over resolved places instead, which
//! over-approximates it [ENT-5].

use std::cell::RefCell;
use std::rc::Rc;

use super::super::model::{CheckedMeasure, CheckedType, IntegerType};
use super::super::places::CaptureId;
pub(crate) use super::super::places::{PlaceRoot, PlaceStep, ResolvedPlace};
use super::state::WordHashMap;
use crate::DeclarationId;

/// Which once-captured endpoint one private counted-range term denotes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CountedCaptureSide {
    Lower,
    Upper,
}

/// One [ENT-2] term.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum TermKind {
    /// The distinguished zero term Z, carrying constant bounds.
    Zero,
    /// The mathematical value of an integer literal or integer-typed named
    /// const. Interning constants as terms lets disequalities and bounds share
    /// one representation; the implicit equality to Z folds them back.
    Constant(i128),
    /// An in-scope const-generic parameter with its exact written integer
    /// type [MSR-6], which supplies its implicit bounds under [ENT-2].
    ConstParameter(DeclarationId, IntegerType),
    /// One term of the private success-payload root of an isolated
    /// conditional context [ENT-2] clause (i): the payload's own value when
    /// `path` is empty and `measure` is `None`, and otherwise the
    /// fragment-integer place or the measure of the measured place its owned
    /// descendant projection reaches [CALL-4]. Contexts interpret these terms
    /// independently; selection substitutes them away before publishing
    /// anything into ordinary flow.
    ResultPayload {
        /// The success payload type the root is typed by.
        payload: CheckedType,
        path: Vec<PlaceStep>,
        measure: Option<CheckedMeasure>,
        ty: IntegerType,
    },
    /// A tracked place [ENT-2] clause (a) whose final selected type is one
    /// fragment type, carried as the one resolved path the checker has
    /// [REF-1].
    Place(ResolvedPlace, IntegerType),
    /// One measure term `P.len`, `P.cap` or `P.head` [MSR-1, OP-15],
    /// of fragment type u64. Its support is P's descriptor storage [MSR-2].
    Measure(CheckedMeasure, ResolvedPlace),
    /// One immutable compiler-owned endpoint capture [ENT-2, S11]. The
    /// finalized `for_stmt` path plus the endpoint side is its complete
    /// function-local identity; source can neither name nor mutate it.
    CountedCapture {
        range_path: Vec<u32>,
        side: CountedCaptureSide,
    },
    /// The immutable value of one index evaluation at formation.
    IndexCapture { capture: CaptureId },
    /// One immutable compiler-owned commit value [ENT-2]: the value the
    /// right-hand side of one `set` statement evaluated to at that
    /// occurrence, before its target kill. The statement's finalized
    /// NodePath plus the value's fragment type is its complete function-local
    /// identity; source can neither name nor mutate it. The flow visits that
    /// statement once, so this one term denotes its value in the single
    /// abstract evaluation the walk performs, as a counted header image does
    /// for an arbitrary iteration. A `give` of a carrier names its given
    /// value by the same kind at its own NodePath [ENT-2].
    CommitValue {
        commit_path: Vec<u32>,
        ty: IntegerType,
    },
    /// One immutable compiler-owned call datum [ENT-3.S13, MSR-3]: the value
    /// one `own` operand of a declared relation had at that call's
    /// pre-transfer point, or one measure of it. The call's finalized
    /// NodePath, the formal ordinal, the operand's ordered projections, and
    /// and which measure of it the datum denotes, if any, are its complete
    /// function-local identity [MSR-1]. No place occurs in it, so no
    /// [ENT-5] event kills it and a relation stated over it survives the
    /// consume the same statement performs.
    CallDatum {
        call_path: Vec<u32>,
        formal: u32,
        projections: Vec<PlaceStep>,
        measure: Option<CheckedMeasure>,
        ty: IntegerType,
    },
    /// One immutable compiler-owned entry datum [MSR-3]: the value one
    /// [MSR-1] measure of a parameter had at body entry, or the value of one
    /// fragment-integer place of a written reference parameter named under
    /// `entry(parameter)`. The formal ordinal, the operand's ordered
    /// projections and whether the datum denotes the value or one measure of
    /// it are its complete function-local identity. No place occurs in it, so
    /// no [ENT-5] event kills it: that is what makes an `ensures` naming a
    /// parameter's measure mean the entry value even where the body writes
    /// that parameter back [LIV-2], and it is the same datum a caller
    /// substitutes as that call's call datum.
    EntryDatum {
        formal: u32,
        projections: Vec<PlaceStep>,
        measure: Option<CheckedMeasure>,
        ty: IntegerType,
    },
    /// One immutable compiler-owned measure datum [MSR-3]: the value one
    /// [MSR-1] measure of a measured place had immediately before one
    /// statement carried that value across a naming event — a `let` or
    /// [LIV-2] `set` rebind, a construct's field operand, a destructuring
    /// binder, an element position, or an enum payload. The statement's
    /// finalized NodePath, the placement, the ordinal within that statement,
    /// the projection path from that ordinal's operand to the measured place, and
    /// the measure are its complete function-local identity. No place occurs
    /// in it, so neither the consume the statement performs nor the write it
    /// commits can kill it, which is what carries a measured value's measures
    /// across the event.
    ///
    /// `path` is empty where the operand is itself measured, and names the
    /// field, payload and Box-content selections that reach the measured
    /// place where the operand is an aggregate holding one: a placement
    /// carries every measured place under its operand, so one operand mints
    /// one datum set per such place [MSR-1]. With `measure` `None` the datum
    /// is the value of the fragment-integer place `path` reaches, which a
    /// placement carries when that source place is a term of the state it
    /// reads [MSR-3].
    MeasureDatum {
        statement: Vec<u32>,
        placement: MeasurePlacement,
        ordinal: u32,
        path: Vec<PlaceStep>,
        measure: Option<CheckedMeasure>,
        ty: IntegerType,
    },
}

/// Which naming event one measure datum stands at [MSR-3].
///
/// The placement is part of the datum's identity so that one statement
/// carrying two of them — a `replace`, whose displaced value leaves the
/// target as its stored value arrives — mints two terms and not one, and so
/// that a diagnostic can name the event the datum belongs to.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MeasurePlacement {
    /// One `let` binder or one [LIV-2] `set` target whose right-hand side is
    /// a bare use of a measured place.
    Rebind,
    /// One field operand of a `construct`, carried into the field of the
    /// value it builds.
    Construct,
    /// One binder of a destructuring consume, carried out of the field it
    /// names.
    Destructuring,
    /// One element position of a run, written by a [LIV-2] element-position
    /// commit or read out by the [SET-2] `replace` that displaces it.
    Element,
    /// One payload binder of a `match` arm, carried out of the payload of
    /// the enum place the arm consumes.
    Payload,
}

/// Dense identity of one interned term.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) struct TermId(pub(crate) u32);

/// One implicit [ENT-2] equality a measure term carries at every program
/// point: the `Array<T, N>` `len` equality to N, with concrete N a constant
/// and const-generic N a symbolic constant term, and [MSR-2]'s standing
/// constant for a table cell whose value is fixed. Implicit facts hold at
/// every program point and never die.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeasureBound {
    Constant(i128),
    Equal(TermId),
}

/// The zero term is always interned first.
pub(crate) const ZERO: TermId = TermId(0);

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

/// The two implicit bounds one term carries against Z, each with the kind of
/// the tightest implicit fact giving it: `term - Z <= to_zero` and
/// `Z - term <= from_zero`. Z itself carries neither. A term that holds no
/// stored relation and no implicit fact against another non-Z term has no
/// other fact, so a closure answers every pair involving it from these two
/// bounds and its Z row and column [ENT-4].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImplicitRange {
    pub(crate) to_zero: Option<(i128, ImplicitBoundKind)>,
    pub(crate) from_zero: Option<(i128, ImplicitBoundKind)>,
}

/// Function-scoped term registry. Only terms written in the function
/// participate [ENT-4]; the registry grows monotonically during the forward
/// walk. A term registered after a query cannot change that query's answer:
/// every implicit fact relates a term to Z (or, for an array length, to its
/// constant length term), so any derivation hopping through a later term's
/// implicit bounds factors through Z, and no other fact mentions an
/// unregistered term.
pub(crate) struct TermTable {
    terms: Vec<TermKind>,
    ids: WordHashMap<TermKind, TermId>,
    measure_bounds: WordHashMap<TermId, MeasureBound>,
    /// Terms with an implicit fact against another non-Z term: a measure the
    /// table equates to a symbolic constant and that constant, and a
    /// capacity measure with a registered length or head sibling. Sorted.
    relational: Vec<TermId>,
    /// The implicit ranges of every registered term at one revision.
    implicit_cache: RefCell<Option<(usize, Rc<Vec<ImplicitRange>>)>>,
    revision: usize,
}

impl TermTable {
    pub(crate) fn new() -> Self {
        let mut table = Self {
            terms: Vec::new(),
            ids: WordHashMap::default(),
            measure_bounds: WordHashMap::default(),
            relational: Vec::new(),
            implicit_cache: RefCell::new(None),
            revision: 0,
        };
        let zero = table.intern(TermKind::Zero);
        debug_assert_eq!(zero, ZERO);
        table
    }

    pub(crate) fn set_measure_bound(&mut self, term: TermId, bound: MeasureBound) {
        if self.measure_bounds.insert(term, bound) != Some(bound) {
            if let MeasureBound::Equal(other) = bound {
                self.note_relational(term, other);
            }
            self.revision = self
                .revision
                .checked_add(1)
                .expect("term revision fits usize");
        }
    }

    /// Terms that carry an implicit fact against a non-Z term. Such a term
    /// takes part in every closure even without a stored relation, because
    /// the fact it carries is not implied by its bounds through Z.
    pub(crate) fn relational_terms(&self) -> &[TermId] {
        &self.relational
    }

    fn note_relational(&mut self, first: TermId, second: TermId) {
        for term in [first, second] {
            if let Err(position) = self.relational.binary_search(&term) {
                self.relational.insert(position, term);
            }
        }
    }

    /// The implicit ranges of every registered term, shared until the table
    /// changes.
    pub(crate) fn implicit_ranges(&self) -> Rc<Vec<ImplicitRange>> {
        if let Some((revision, ranges)) = self.implicit_cache.borrow().as_ref()
            && *revision == self.revision
        {
            return Rc::clone(ranges);
        }
        let ranges = Rc::new(
            self.ids()
                .map(|id| self.implicit_range(id))
                .collect::<Vec<_>>(),
        );
        *self.implicit_cache.borrow_mut() = Some((self.revision, Rc::clone(&ranges)));
        ranges
    }

    /// The tightest implicit bound of one term in each direction against Z;
    /// of two equal bounds the first emitted, which is the one the complete
    /// closure's candidate order keeps.
    fn implicit_range(&self, id: TermId) -> ImplicitRange {
        let mut range = ImplicitRange {
            to_zero: None,
            from_zero: None,
        };
        if id == ZERO {
            return range;
        }
        self.for_each_implicit_bound(id, |left, right, bound, kind| {
            let held = if (left, right) == (id, ZERO) {
                &mut range.to_zero
            } else if (left, right) == (ZERO, id) {
                &mut range.from_zero
            } else {
                return;
            };
            if held.is_none_or(|(current, _)| bound < current) {
                *held = Some((bound, kind));
            }
        });
        range
    }

    /// Emits every [ENT-2] implicit bound carried by one term: the reflexive
    /// bound, the fragment-type range, the constant fold through Z, and the
    /// `len_of(P) = N` equality of an `array<T, N>` place.
    ///
    /// Implicit facts are a function of the term table and the place's type
    /// alone. They hold at every program point, so this is the single rule
    /// table every closure entry point re-emits; no [ENT-5] kill and no join
    /// can remove one, and a state that lost the materialized copy of one
    /// regains it here.
    pub(crate) fn for_each_implicit_bound(
        &self,
        id: TermId,
        mut emit: impl FnMut(TermId, TermId, i128, ImplicitBoundKind),
    ) {
        emit(id, id, 0, ImplicitBoundKind::Reflexive);
        match self.kind(id) {
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
            TermKind::Measure(measure, _) => {
                let (minimum, maximum) = type_range(IntegerType::U64);
                emit(id, ZERO, maximum, ImplicitBoundKind::TypeMaximum);
                emit(ZERO, id, -minimum, ImplicitBoundKind::TypeMinimum);
                match self.measure_bound(id) {
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
                        if let Some(other) = self.sibling_measure(id, bounded) {
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

    /// Changes whenever registered terms or their standing measure facts change.
    pub(crate) fn revision(&self) -> usize {
        self.revision
    }

    pub(crate) fn measure_bound(&self, term: TermId) -> Option<MeasureBound> {
        self.measure_bounds.get(&term).copied()
    }

    /// Another measure of the same place as `term`, when that measure term is
    /// registered. [MSR-2]'s standing orderings relate two measures of one
    /// place, so the table has to find the sibling by its place.
    pub(crate) fn sibling_measure(&self, term: TermId, measure: CheckedMeasure) -> Option<TermId> {
        let sibling = match self.kind(term) {
            TermKind::Measure(_, place) => TermKind::Measure(measure, place.clone()),
            _ => return None,
        };
        self.interned(&sibling)
    }

    /// Interns one term, canonicalizing the written constant zero to Z.
    ///
    /// Relations are over mathematical values [ENT-2], so a written `0_T`
    /// and the distinguished zero term denote the same value and must be
    /// one term. Kept apart, a disequality reaches Z only by a bound
    /// strengthened through the constant's implicit equality, which exists
    /// only where the fragment already bounds the operand on that side: a
    /// a source relation `d != 0_i32` then could not discharge an obligation stated
    /// against Z at a signed type, and [OP-2]'s own mechanical fix would be
    /// unwritable. Z carries exactly the bounds the constant zero would
    /// have contributed, so the merge loses no fact.
    pub(crate) fn intern(&mut self, kind: TermKind) -> TermId {
        let kind = Self::identity(kind);
        if let Some(id) = self.ids.get(&kind) {
            return *id;
        }
        let id = TermId(
            u32::try_from(self.terms.len())
                .expect("ENT term inventory exceeds the u32 identity space"),
        );
        self.terms.push(kind.clone());
        // [MSR-2]'s standing ordering relates a capacity measure to the
        // length and head measures of its place as soon as both are
        // registered; `for_each_implicit_bound` emits it from the capacity.
        if let TermKind::Measure(measure, place) = &kind {
            let siblings: &[CheckedMeasure] = match measure {
                CheckedMeasure::Capacity => &[CheckedMeasure::Length, CheckedMeasure::Head],
                CheckedMeasure::Length | CheckedMeasure::Head => &[CheckedMeasure::Capacity],
            };
            let siblings = siblings
                .iter()
                .filter_map(|sibling| self.interned(&TermKind::Measure(*sibling, place.clone())))
                .collect::<Vec<_>>();
            for sibling in siblings {
                self.note_relational(id, sibling);
            }
        }
        self.ids.insert(kind, id);
        self.revision = self
            .revision
            .checked_add(1)
            .expect("term revision fits usize");
        id
    }

    /// The identity of one already interned term, without interning it.
    pub(crate) fn interned(&self, kind: &TermKind) -> Option<TermId> {
        self.ids.get(&Self::identity(kind.clone())).copied()
    }

    /// The one canonical form of a term kind.
    ///
    /// The constant zero is the distinguished zero term, for the reason
    /// above. A place-carrying kind takes its path's term identity, so a
    /// place written twice with the same literal or const subscript is one
    /// term at both occurrences: `rows[0_u64].len` and the bound
    /// `i < rows[0_u64].len` owes are otherwise two unrelated quantities
    /// [MSR-1, ENT-2].
    fn identity(kind: TermKind) -> TermKind {
        match kind {
            TermKind::Constant(0) => TermKind::Zero,
            TermKind::Place(place, fragment) => TermKind::Place(place.term_identity(), fragment),
            TermKind::Measure(measure, place) => TermKind::Measure(measure, place.term_identity()),
            other => other,
        }
    }

    pub(crate) fn kind(&self, id: TermId) -> &TermKind {
        &self.terms[id.0 as usize]
    }

    /// Every registered term, for implicit-fact materialization [ENT-4].
    pub(crate) fn ids(&self) -> impl Iterator<Item = TermId> {
        (0..self.terms.len()).map(|index| {
            TermId(u32::try_from(index).expect("ENT term inventory exceeds the u32 identity space"))
        })
    }

    pub(crate) fn into_inventory(self) -> (Vec<TermKind>, Vec<Option<MeasureBound>>) {
        let measure_bounds = (0..self.terms.len())
            .map(|index| {
                let id = TermId(
                    u32::try_from(index)
                        .expect("ENT term inventory exceeds the u32 identity space"),
                );
                self.measure_bounds.get(&id).copied()
            })
            .collect();
        (self.terms, measure_bounds)
    }
}

/// Inclusive value range of one fragment type, as mathematical integers.
pub(crate) const fn type_range(ty: IntegerType) -> (i128, i128) {
    match ty {
        IntegerType::I8 => (i8::MIN as i128, i8::MAX as i128),
        IntegerType::I16 => (i16::MIN as i128, i16::MAX as i128),
        IntegerType::I32 => (i32::MIN as i128, i32::MAX as i128),
        IntegerType::I64 => (i64::MIN as i128, i64::MAX as i128),
        IntegerType::U8 => (0, u8::MAX as i128),
        IntegerType::U16 => (0, u16::MAX as i128),
        IntegerType::U32 => (0, u32::MAX as i128),
        IntegerType::U64 => (0, u64::MAX as i128),
    }
}

/// The mathematical value of one checked integer constant, whose `bits` hold
/// the type-width two's-complement pattern.
pub(crate) const fn integer_value(ty: IntegerType, bits: u64) -> i128 {
    let value = bits as i128;
    if ty.signed() {
        let width = ty.width() as u32;
        let sign_bit = 1_u64 << (width - 1);
        if bits & sign_bit != 0 {
            return value - (1_i128 << width);
        }
    }
    value
}
