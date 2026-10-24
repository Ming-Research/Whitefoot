//! [TYPE-11.S1] to [TYPE-11.S8]: sealed values.
//!
//! Beside its fact state, the walk carries the debts of the values the body
//! holds: the writes that may have broken an affine type invariant of a value
//! since the hand-off that delivered it to the body. A hand-off requires the
//! debts at or below what it hands on to be restored, or one at an unknown
//! position to hold at the position handed on, and a read of a sealed place
//! establishes the instance the delivering hand-off guaranteed.

use super::super::super::model::{
    CheckedAffineInvariant, CheckedEffectStep, CheckedEffects, CheckedShared, FunctionId,
};
use super::super::super::places::places_overlap;
use super::super::{CalleeSealing, EntailmentCallee, SealingFailure, SealingOrigin, SealingRule};
use super::*;
use std::borrow::Cow;

/// [TYPE-11.S7] one binder datum a read selects.
struct ReadAnchor {
    /// The anchor at each resolved member of the read place.
    members: Vec<ResolvedPlace>,
    /// The invariant's position among the affine invariants of `ty`.
    index: usize,
    /// The anchors' struct.
    ty: CheckedType,
    /// The datum's projection from the anchor.
    datum: Vec<GoalProjection>,
    /// What the instance reads its binder data from; `None` when the read
    /// forms no term to read them from.
    image: Option<GoalExpression>,
}

/// One debt [TYPE-11.S2]: an anchor shape with one affine invariant of its
/// struct, or a cover standing for every anchor at or below its place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Debt {
    /// The anchor shape, or the place a cover stands below.
    anchor: ResolvedPlace,
    /// The invariant's position among the affine invariants of the anchor's
    /// struct; `None` for a cover.
    invariant: Option<usize>,
    /// The breaking events in walk order, which the diagnostic reads.
    events: Vec<crate::NodePath>,
    /// Whether a widening lost an identity the debt was restorable through.
    widened: bool,
    /// The reference every breaking event wrote through, while no kill
    /// batch has rebound or ended it [TYPE-11.S5].
    through: Option<Through>,
    /// What created the debt, which the diagnostic's repair reads: an
    /// atomic block's entry only when every breaking event is one.
    origin: SealingOrigin,
}

/// [TYPE-11.S5] the reference a debt's breaking events all wrote through,
/// and the projection each formed from its referent to the debt's anchor:
/// the terms the debt is restored over.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Through {
    holder: BindingId,
    steps: Vec<PlaceStep>,
}

/// What one prefix of a place selects [TYPE-11.S2, REF-4].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Selected {
    /// A value of this type.
    Value(CheckedType),
    /// A run of elements of this type, which is no value of its own: what a
    /// range step, a page step or a segment subscript of a `Segments`
    /// selects, and the referent of a range or run parameter or of a range
    /// reference's current target.
    Run(CheckedType),
    /// A selection the walk does not follow.
    Unknown,
}

impl Debt {
    fn same_key(&self, other: &Self) -> bool {
        self.invariant == other.invariant && self.anchor == other.anchor
    }
}

/// The debts of one flow edge [TYPE-11.S2], each key once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Debts(Vec<Debt>);

impl Debts {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Adds one debt, uniting its breaking events with those of the debt of
    /// the same key [TYPE-11.S6]; the united events wrote through one
    /// reference only where both debts' did [TYPE-11.S5].
    fn add(&mut self, debt: Debt) {
        if let Some(existing) = self.0.iter_mut().find(|known| known.same_key(&debt)) {
            for event in debt.events {
                if !existing.events.contains(&event) {
                    existing.events.push(event);
                }
            }
            existing.widened |= debt.widened;
            if existing.origin != debt.origin {
                existing.origin = SealingOrigin::Written;
            }
            if existing.through != debt.through {
                existing.through = None;
            }
        } else {
            self.0.push(debt);
        }
    }

    pub(super) fn union(&mut self, other: &Self) {
        for debt in &other.0 {
            self.add(debt.clone());
        }
    }

    /// Whether every debt of these is a debt of `other` that is restored over
    /// the same reference's terms or over its anchor's own [TYPE-11.S5].
    pub(super) fn settled_within(&self, other: &Self) -> bool {
        self.0.iter().all(|debt| {
            other.0.iter().any(|known| {
                known.same_key(debt) && (known.through.is_none() || known.through == debt.through)
            })
        })
    }

    /// Removes and returns the debts `select` picks.
    fn take(&mut self, mut select: impl FnMut(&Debt) -> bool) -> Vec<Debt> {
        let (taken, kept) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|debt| select(debt));
        self.0 = kept;
        taken
    }

    /// [TYPE-11.S3] the death of a root binding removes every debt rooted at
    /// it.
    pub(super) fn remove_rooted(&mut self, binding: BindingId) {
        self.0
            .retain(|debt| debt.anchor.root != PlaceRoot::Binding(binding));
    }

    /// [TYPE-11.S3] a sealed store at `place` removes each debt proved at or
    /// below it.
    fn remove_proved_at_or_below(&mut self, place: &ResolvedPlace) {
        self.0.retain(|debt| !proved_at_or_below(debt, place));
    }

    /// [TYPE-11.S3] a sealed store at `place`, one member of a place spelled
    /// through `holder` that resolves to several, whose first `referent`
    /// steps are the reference's referent at that member: removes each debt
    /// proved at or below it that records `holder` and is anchored at that
    /// referent followed by the recorded projection. Its instance reads
    /// `holder^` followed by that projection [TYPE-11.S5], a place within the
    /// stored value; any other debt there may lie at a member the store did
    /// not write.
    fn remove_recorded_at_or_below(
        &mut self,
        place: &ResolvedPlace,
        holder: BindingId,
        referent: usize,
    ) {
        self.0.retain(|debt| {
            !(proved_at_or_below(debt, place)
                && debt.through.as_ref().is_some_and(|through| {
                    through.holder == holder
                        && debt.anchor.path.get(referent..) == Some(through.steps.as_slice())
                }))
        });
    }
}

/// The position of the cover step `**` of `place` [REF-1]; a summary has at
/// most one.
fn cover_step(place: &ResolvedPlace) -> Option<usize> {
    place
        .path
        .iter()
        .position(|step| matches!(step, PlaceStep::Descendant(_)))
}

/// [TYPE-11.S2] whether `debt` is proved at or below `place`: its anchor has
/// the place's root and begins with the place's steps, every captured offset
/// equal.
fn proved_at_or_below(debt: &Debt, place: &ResolvedPlace) -> bool {
    place.contains(&debt.anchor)
}

/// [REF-1] the members of one resolution that no other member covers. A
/// member without a cover step that begins with the prefix R of another
/// member's cover `R.**` and ends with that member's suffix lies within the
/// cover, whose current target identity names it as well while that
/// target is current.
fn uncovered(members: Vec<ResolvedPlace>) -> Vec<ResolvedPlace> {
    let covered = |member: &ResolvedPlace| {
        cover_step(member).is_none()
            && members.iter().any(|cover| {
                cover_step(cover).is_some_and(|at| {
                    let suffix = &cover.path[at + 1..];
                    cover.root == member.root
                        && member.path.len() >= at + suffix.len()
                        && member.path.starts_with(&cover.path[..at])
                        && member.path.ends_with(suffix)
                })
            })
    };
    members
        .iter()
        .filter(|member| !covered(member))
        .cloned()
        .collect()
}

/// The offsets of `step`: an index's or a page's, or a range's endpoints.
fn step_offsets(step: &PlaceStep) -> impl Iterator<Item = &CapturedValue> {
    let (first, second) = match step {
        PlaceStep::Index(value) | PlaceStep::Page(value) => (Some(value), None),
        PlaceStep::Range(range) => (Some(&range.start), Some(&range.end)),
        _ => (None, None),
    };
    first.into_iter().chain(second)
}

/// The offsets of `step`, to be widened.
fn step_offsets_mut(step: &mut PlaceStep) -> impl Iterator<Item = &mut CapturedValue> {
    let (first, second) = match step {
        PlaceStep::Index(value) | PlaceStep::Page(value) => (Some(value), None),
        PlaceStep::Range(range) => (Some(&mut range.start), Some(&mut range.end)),
        _ => (None, None),
    };
    first.into_iter().chain(second)
}

/// Whether `value` reads the current value of a tracked place [ENT-2], a
/// binding or a place below one, which a later write can change.
const fn reads_current(value: &CapturedValue) -> bool {
    matches!(value.term, CapturedTerm::Binding(_) | CapturedTerm::Place(_))
}

/// The position of the step through which `place` carries the current
/// target identity of the reference `binding` [REF-1].
fn carried_by(place: &ResolvedPlace, binding: BindingId) -> Option<usize> {
    place
        .path
        .iter()
        .position(|step| matches!(step, PlaceStep::Descendant(target) if target.holder == binding))
}

/// Whether `step` has the unknown position `[?]` [TYPE-11.S2].
fn unknown_step(step: &PlaceStep) -> bool {
    let unknown = |value: &CapturedValue| value.capture == CaptureId::Unknown;
    match step {
        PlaceStep::Index(value) | PlaceStep::Page(value) => unknown(value),
        PlaceStep::Range(range) => unknown(&range.start) || unknown(&range.end),
        _ => false,
    }
}

/// Whether `place` has the unknown position `[?]` [TYPE-11.S2].
fn has_unknown_offset(place: &ResolvedPlace) -> bool {
    place.path.iter().any(unknown_step)
}

/// [TYPE-11.S5] the anchor at which `debt`, at the unknown position, is
/// satisfied at the handed `place`: its anchor with each `[?]` taking the
/// captured offset the place has at that step, every other step of the
/// place's equal to the anchor's. None for a cover, and when some `[?]`
/// lies where the place has no captured offset.
fn handed_member(debt: &Debt, place: &ResolvedPlace) -> Option<ResolvedPlace> {
    if debt.invariant.is_none()
        || debt.anchor.root != place.root
        || cover_step(&debt.anchor).is_some()
        || debt.anchor.path.len() < place.path.len()
    {
        return None;
    }
    let known = |value: &CapturedValue| value.capture != CaptureId::Unknown;
    let mut member = debt.anchor.clone();
    for (step, handed) in member.path.iter_mut().zip(&place.path) {
        let fixed = match (&*step, handed) {
            (PlaceStep::Index(value), PlaceStep::Index(offset))
            | (PlaceStep::Page(value), PlaceStep::Page(offset)) => !known(value) && known(offset),
            _ => false,
        };
        if fixed {
            *step = *handed;
        } else if *step != *handed {
            return None;
        }
    }
    (!has_unknown_offset(&member)).then_some(member)
}

/// [TYPE-11.S3] `debt` after a kill batch wrote or ended `binding` while the
/// debt was not restored: the offsets from the first that `reads` picks are
/// widened as [`widen_offsets_from`] widens them; an anchor carrying the
/// binding's current target identity widens to its cover; and a debt
/// restored through the binding is restored over its anchor's own terms
/// from then on [TYPE-11.S5].
fn widened(mut debt: Debt, binding: BindingId, reads: impl Fn(&PlaceStep) -> bool) -> Debt {
    if debt
        .through
        .as_ref()
        .is_some_and(|through| through.holder == binding)
    {
        debt.through = None;
    }
    if let Some(position) = carried_by(&debt.anchor, binding) {
        debt.anchor.path.truncate(position);
        debt.invariant = None;
        debt.widened = true;
    }
    if let Some(first) = debt.anchor.path.iter().position(reads) {
        widen_offsets_from(&mut debt, first);
    }
    debt
}

/// [TYPE-11.S3] widens the offset at step `first` of `debt`'s anchor and
/// every later offset that reads a tracked place to `[?]`, so one anchor
/// widens to at most one more shape than it has such offsets.
fn widen_offsets_from(debt: &mut Debt, first: usize) {
    for step in &mut debt.anchor.path[first..] {
        for value in step_offsets_mut(step) {
            if reads_current(value) {
                *value = CapturedValue::unknown();
                debt.widened = true;
            }
        }
    }
}

/// The goal `expression` with each binder datum replaced by what `datum`
/// forms from the datum's projections and type; `None` when one forms
/// nothing [TYPE-11.S1].
fn substitute_binder(
    expression: &GoalExpression,
    datum: &dyn Fn(&[GoalProjection], CheckedType) -> Option<GoalExpression>,
) -> Option<GoalExpression> {
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: 0,
            projections,
            ty,
        }) => datum(projections, *ty),
        GoalExpression::Datum(GoalDatum::Parameter { .. }) => None,
        GoalExpression::Datum(other) => Some(GoalExpression::Datum(other.clone())),
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
                .map(|argument| substitute_binder(argument, datum))
                .collect::<Option<Vec<_>>>()?,
        }),
    }
}

/// The goal `expression` with each read of the binder datum `datum`, a
/// measure member's whole measure row included, replaced by what `value`
/// forms from the datum's type; `None` when it reads another binder datum
/// [TYPE-11.S7].
fn substitute_datum(
    expression: &GoalExpression,
    datum: &[GoalProjection],
    value: &dyn Fn(CheckedType) -> GoalExpression,
) -> Option<GoalExpression> {
    if let Some((read, ty)) = expression.binder_datum() {
        return (read == datum).then(|| value(ty));
    }
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter { .. }) => None,
        GoalExpression::Datum(other) => Some(GoalExpression::Datum(other.clone())),
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
                .map(|argument| substitute_datum(argument, datum, value))
                .collect::<Option<Vec<_>>>()?,
        }),
    }
}

/// The storage steps of one binder datum's projection.
fn datum_steps(datum: &[GoalProjection]) -> impl Iterator<Item = PlaceStep> + '_ {
    datum.iter().map(|projection| projection.place_step())
}

impl Input<'_, '_> {
    /// The affine type invariants of `ty`'s struct [TYPE-11].
    pub(super) fn affine_invariants(&self, ty: CheckedType) -> &[CheckedAffineInvariant] {
        match ty {
            CheckedType::Nominal(nominal) => self
                .context
                .nominals
                .get(nominal.0 as usize)
                .map_or(&[][..], |record| record.affine_invariants.as_slice()),
            _ => &[],
        }
    }

    /// The type of the value one field selection, `Box` content step or
    /// payload step below a value of type `ty` selects.
    fn sealed_step_type(&self, ty: CheckedType, step: &PlaceStep) -> Option<CheckedType> {
        let kind = |ty: CheckedType| match ty {
            CheckedType::Nominal(nominal) => self
                .context
                .nominals
                .get(nominal.0 as usize)
                .map(|record| &record.kind),
            _ => None,
        };
        match step {
            PlaceStep::Field(field) => match kind(ty)? {
                CheckedNominalKind::Struct { fields } => Some(fields.get(*field as usize)?.ty),
                _ => None,
            },
            PlaceStep::Deref => match kind(ty)? {
                CheckedNominalKind::Box { referent, .. } => Some(*referent),
                _ => None,
            },
            PlaceStep::Payload { variant, field } => match kind(ty)? {
                CheckedNominalKind::Enum { variants } => Some(
                    variants
                        .iter()
                        .find(|candidate| candidate.tag == *variant)?
                        .fields
                        .get(*field as usize)?
                        .ty,
                ),
                _ => None,
            },
            _ => None,
        }
    }

    /// The type of each entry of a map of type `ty`, the prelude's
    /// `Option<V>` [SHARE-1]; `None` for every other type.
    fn map_entry(&self, ty: CheckedType) -> Option<CheckedType> {
        let CheckedType::Nominal(nominal) = ty else {
            return None;
        };
        match self
            .context
            .nominals
            .get(nominal.0 as usize)
            .map(|record| &record.kind)
        {
            Some(CheckedNominalKind::Shared {
                shape: CheckedShared::Map { entry },
                ..
            }) => Some(*entry),
            _ => None,
        }
    }

    /// [TYPE-11.S1] whether a value of type `ty` is a map or an `Entries`,
    /// whose entries are no part of its value: no hand-off of it delivers
    /// them valid.
    fn holds_entries(&self, ty: CheckedType) -> bool {
        self.map_entry(ty).is_some() || matches!(ty, CheckedType::Entries { .. })
    }

    /// [TYPE-11.S4] whether `debt`'s anchor passes an entry of a map or an
    /// `Entries` that lies below `place`'s level: handing `place` on owes
    /// nothing of what that entry holds.
    fn below_entry(&self, debt: &Debt, place: &ResolvedPlace) -> bool {
        let (level, _) = self.sealed_levels(place);
        let selections = self.sealed_selections(&debt.anchor);
        let mut levels = 0;
        for (step, pair) in debt.anchor.path.iter().zip(selections.windows(2)) {
            let [above, below] = pair else {
                return false;
            };
            if !matches!(step, PlaceStep::Descendant(_)) && matches!(below, Selected::Value(_)) {
                levels += 1;
            }
            let entry = matches!(step, PlaceStep::Index(_))
                && matches!(above, Selected::Value(ty) if self.holds_entries(*ty));
            if entry && levels > level {
                return true;
            }
        }
        false
    }

    /// What each prefix of `place` selects, from its root to the whole place
    /// [TYPE-11.S2]. A range or run parameter is a run of elements of its
    /// declared type and owns no value itself [REF-4].
    fn sealed_selections(&self, place: &ResolvedPlace) -> Vec<Selected> {
        let mut selected = match place.root {
            PlaceRoot::Binding(binding) => {
                let run = self.function.parameters.iter().any(|parameter| {
                    parameter.binding == binding
                        && matches!(parameter.mode, CheckedMode::Range | CheckedMode::Run)
                });
                match self.summary(binding).and_then(|summary| summary.ty) {
                    Some(ty) if run => Selected::Run(ty),
                    Some(ty) => Selected::Value(ty),
                    None => Selected::Unknown,
                }
            }
            PlaceRoot::Constant(_) => Selected::Unknown,
        };
        let mut selections = Vec::with_capacity(place.path.len() + 1);
        selections.push(selected);
        for step in &place.path {
            selected = self.selected_below(selected, step);
            selections.push(selected);
        }
        selections
    }

    /// What `step` selects below a prefix that selects `selected`. A range
    /// step, a page step and a segment subscript of a `Segments` select a run
    /// of its elements, and a subscript of a run selects one of them, so
    /// `x[a..b][i]` selects an element as `x[k]` does [REF-4]; a reference's
    /// current target is its referent, a run for a range reference [REF-1].
    fn selected_below(&self, selected: Selected, step: &PlaceStep) -> Selected {
        let element = |ty: CheckedType| {
            ty.storage_element()
                .and_then(|element| self.context.elements.get(element.index()).copied())
        };
        let selection = |ty: Option<CheckedType>, run: bool| match ty {
            Some(ty) if run => Selected::Run(ty),
            Some(ty) => Selected::Value(ty),
            None => Selected::Unknown,
        };
        match (selected, step) {
            (_, PlaceStep::Descendant(target)) => selection(Some(target.ty), target.range),
            (Selected::Run(ty), PlaceStep::Index(_) | PlaceStep::Page(_)) => Selected::Value(ty),
            (Selected::Run(ty), PlaceStep::Range(_) | PlaceStep::Part(_)) => Selected::Run(ty),
            (Selected::Value(ty), PlaceStep::Index(_)) => match self.map_entry(ty) {
                Some(entry) => Selected::Value(entry),
                None => selection(element(ty), matches!(ty, CheckedType::Segments { .. })),
            },
            (
                Selected::Value(ty),
                PlaceStep::Range(_) | PlaceStep::Page(_) | PlaceStep::Part(_),
            ) => selection(element(ty), true),
            (Selected::Value(ty), _) => selection(self.sealed_step_type(ty, step), false),
            (Selected::Run(_) | Selected::Unknown, _) => Selected::Unknown,
        }
    }

    /// The type of the value each prefix of `place` selects, from its root
    /// to the whole place; `None` where the prefix selects a run, or a
    /// selection the walk does not follow.
    fn sealed_prefix_types(&self, place: &ResolvedPlace) -> Vec<Option<CheckedType>> {
        self.sealed_selections(place)
            .into_iter()
            .map(|selected| match selected {
                Selected::Value(ty) => Some(ty),
                Selected::Run(_) | Selected::Unknown => None,
            })
            .collect()
    }

    /// [TYPE-11.S2] the fewest and the most levels of `place`: each step that
    /// selects a value adds one, and a step that selects a run adds none. A
    /// cover step adds none here, [`Self::level_bounds`] reading the number
    /// it stands for; a subscript whose base the walk does not follow may
    /// select a run or a value, so it adds either.
    fn sealed_levels(&self, place: &ResolvedPlace) -> (usize, usize) {
        let selections = self.sealed_selections(place);
        let mut levels = (0, 0);
        for (step, selected) in place.path.iter().zip(&selections[1..]) {
            let (least, most) = match (step, selected) {
                (PlaceStep::Descendant(_), _) | (_, Selected::Run(_)) => (0, 0),
                (PlaceStep::Index(_) | PlaceStep::Page(_), Selected::Unknown) => (0, 1),
                (PlaceStep::Range(_) | PlaceStep::Part(_), Selected::Unknown) => (0, 0),
                (_, Selected::Value(_) | Selected::Unknown) => (1, 1),
            };
            levels.0 += least;
            levels.1 += most;
        }
        levels
    }

    /// [TYPE-11.S2] the fewest and the most levels the places `left` and
    /// `right` may have: a cover step `**` both pass through adds the same
    /// number to each, and one that only one of them passes through adds any
    /// number to it, none included [REF-1].
    fn level_bounds(&self, left: &ResolvedPlace, right: &ResolvedPlace) -> [(usize, usize); 2] {
        let shared = match (cover_step(left), cover_step(right)) {
            (Some(at), Some(other)) => at == other && left.path[at] == right.path[other],
            _ => false,
        };
        [left, right].map(|place| {
            let (least, most) = self.sealed_levels(place);
            if cover_step(place).is_some() && !shared {
                (least, usize::MAX)
            } else {
                (least, most)
            }
        })
    }

    /// [TYPE-11.S2] whether `debt` may be at or below `place`: its anchor may
    /// have at least the place's level and may overlap it, or it is a cover
    /// whose place may overlap it.
    fn may_be_at_or_below(
        &self,
        oracle: &dyn SeparationOracle,
        debt: &Debt,
        place: &ResolvedPlace,
    ) -> bool {
        let below = debt.invariant.is_none() || {
            let [(_, most), (least, _)] = self.level_bounds(&debt.anchor, place);
            most >= least
        };
        below && places_overlap(oracle, &debt.anchor, place)
    }

    /// [TYPE-11.S2] whether `debt` may denote the invariant at `invariant` of
    /// `anchor`: it is a cover that may overlap the anchor, or names that
    /// invariant at an anchor that may have the anchor's level and may
    /// overlap it.
    fn may_denote(
        &self,
        oracle: &dyn SeparationOracle,
        debt: &Debt,
        anchor: &ResolvedPlace,
        invariant: usize,
    ) -> bool {
        let named = match debt.invariant {
            None => true,
            Some(own) => {
                let [(least, most), (other_least, other_most)] =
                    self.level_bounds(&debt.anchor, anchor);
                own == invariant && least <= other_most && other_least <= most
            }
        };
        named && places_overlap(oracle, &debt.anchor, anchor)
    }

    /// [TYPE-11.S2] the steps of `anchor` below the value at `place`'s level
    /// on its path: those after `place`'s own steps when the anchor begins
    /// with them, every captured offset equal, and otherwise, when `place`
    /// selects a value at one level, those after the anchor's shortest prefix
    /// that selects a value at that level with no cover step on it. `None`
    /// when neither gives them.
    fn steps_below<'a>(
        &self,
        anchor: &'a ResolvedPlace,
        place: &ResolvedPlace,
    ) -> Option<&'a [PlaceStep]> {
        if place.contains(anchor) {
            return anchor.path.get(place.path.len()..);
        }
        let (level, most) = self.sealed_levels(place);
        let value = matches!(self.sealed_selections(place).last(), Some(Selected::Value(_)));
        if !value || level != most || cover_step(place).is_some() || anchor.root != place.root {
            return None;
        }
        let selections = self.sealed_selections(anchor);
        let mut at = 0;
        for (position, selected) in selections.iter().enumerate() {
            if at == level && matches!(selected, Selected::Value(_)) {
                return anchor.path.get(position..);
            }
            let (Some(step), Some(below)) =
                (anchor.path.get(position), selections.get(position + 1))
            else {
                return None;
            };
            match (step, below) {
                (PlaceStep::Descendant(_), _) | (_, Selected::Unknown) => return None,
                (_, Selected::Value(_)) => at += 1,
                (_, Selected::Run(_)) => {}
            }
            if at > level {
                return None;
            }
        }
        None
    }

    /// [TYPE-11.S2] `debt`, which may be at or below `from`, rebased onto
    /// `onto`: its anchor's steps down to the value at `from`'s level are
    /// replaced by `onto`'s. Where those steps are not known, or what is left
    /// passes a reference's current target identity, which no reference to
    /// `onto` carries, the debt is the cover of the place it is known down
    /// to. The rebased debt records no reference: no reference names the
    /// place its value moved to.
    fn rebased(&self, debt: &Debt, from: &ResolvedPlace, onto: &ResolvedPlace) -> Debt {
        let below = self.steps_below(&debt.anchor, from);
        let steps = below.unwrap_or_default();
        let kept = steps
            .iter()
            .position(|step| matches!(step, PlaceStep::Descendant(_)))
            .unwrap_or(steps.len());
        let mut anchor = onto.clone();
        anchor.path.extend_from_slice(&steps[..kept]);
        let covered = below.is_none() || kept < steps.len();
        Debt {
            anchor,
            invariant: debt.invariant.filter(|_| !covered),
            through: None,
            origin: if covered && debt.invariant.is_some() {
                SealingOrigin::Moved
            } else {
                debt.origin
            },
            ..debt.clone()
        }
    }

    /// [TYPE-11.S3] `debt`, which may be at or below the `swap` target
    /// `from`, exchanged onto the other target `onto`: its invariant at
    /// `onto` when its anchor is at `from`'s level, and otherwise the cover
    /// of `onto`. An exchange so appends no steps below `onto`, and the
    /// debts a loop of exchanges forms are polynomially many [TYPE-11.S6].
    /// The exchanged debt records no reference [TYPE-11.S2]: the value its
    /// breaking events wrote through one now lies at `onto`, which that
    /// reference need not name.
    fn exchanged(&self, debt: &Debt, from: &ResolvedPlace, onto: &ResolvedPlace) -> Debt {
        let at_level = debt.invariant.is_some()
            && self
                .steps_below(&debt.anchor, from)
                .is_some_and(<[PlaceStep]>::is_empty);
        Debt {
            anchor: onto.clone(),
            invariant: debt.invariant.filter(|_| at_level),
            through: None,
            origin: if debt.invariant.is_some() && !at_level {
                SealingOrigin::Moved
            } else {
                debt.origin
            },
            ..debt.clone()
        }
    }

    /// Whether an offset of `step` reads the current value of `binding`: the
    /// binding itself, or a tracked place below it or read through it as a
    /// reference [ENT-2, ENT-5].
    fn step_reads(&self, step: &PlaceStep, binding: BindingId) -> bool {
        step_offsets(step).any(|value| {
            reads_current(value)
                && self.term_support(value.term).iter().any(|(read, holders)| {
                    read.root == PlaceRoot::Binding(binding) || holders.contains(&binding)
                })
        })
    }

    /// Whether `event` writes or ends a place an offset of `step` reads
    /// [ENT-5].
    fn step_killed(
        &self,
        separations: &dyn SeparationOracle,
        step: &PlaceStep,
        event: &KillEvent,
    ) -> bool {
        step_offsets(step).any(|value| {
            reads_current(value)
                && self
                    .term_support(value.term)
                    .iter()
                    .any(|(read, holders)| self.event_kills_read(separations, read, holders, event))
        })
    }

    /// Whether a value of type `ty` owns a struct value with an affine type
    /// invariant [TYPE-11.S1].
    fn owns_anchor(&self, ty: CheckedType, visited: &mut Vec<CheckedType>) -> bool {
        if !self.affine_invariants(ty).is_empty() {
            return true;
        }
        if visited.contains(&ty) {
            return false;
        }
        visited.push(ty);
        self.owned_types(ty)
            .into_iter()
            .any(|ty| self.owns_anchor(ty, visited))
    }

    /// [TYPE-11.S3] whether an affine invariant of a value of type `ty`, or
    /// of a struct value it owns, reads a binder datum through a value of
    /// type `target` strictly below its anchor. Only such an invariant can
    /// read a place at or below a reference's current target from an anchor
    /// that the reference's cover hides above that target.
    fn reads_through(&self, ty: CheckedType, target: CheckedType) -> bool {
        let mut pending = vec![ty];
        let mut visited = Vec::new();
        while let Some(owned) = pending.pop() {
            if visited.contains(&owned) {
                continue;
            }
            visited.push(owned);
            let reads = self.affine_invariants(owned).iter().any(|invariant| {
                invariant.data.iter().any(|datum| {
                    let mut at = Some(owned);
                    datum_steps(datum).any(|step| {
                        at = at.and_then(|ty| self.sealed_step_type(ty, &step));
                        at == Some(target)
                    })
                })
            });
            if reads {
                return true;
            }
            pending.extend(self.owned_types(owned));
        }
        false
    }

    /// The types of the values a value of type `ty` owns one step below it
    /// [TYPE-11.S1].
    fn owned_types(&self, ty: CheckedType) -> Vec<CheckedType> {
        match ty {
            CheckedType::Nominal(nominal) => {
                match self
                    .context
                    .nominals
                    .get(nominal.0 as usize)
                    .map(|record| &record.kind)
                {
                    Some(CheckedNominalKind::Struct { fields }) => {
                        fields.iter().map(|field| field.ty).collect()
                    }
                    Some(CheckedNominalKind::Enum { variants }) => variants
                        .iter()
                        .flat_map(|variant| variant.fields.iter().map(|field| field.ty))
                        .collect(),
                    Some(CheckedNominalKind::Box { referent, .. }) => vec![*referent],
                    _ => Vec::new(),
                }
            }
            CheckedType::Array { .. }
            | CheckedType::Buffer { .. }
            | CheckedType::Window { .. }
            | CheckedType::Segments { .. } => ty
                .storage_element()
                .and_then(|element| self.context.elements.get(element.index()).copied())
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// [TYPE-11.S1, ENT-5] whether a write at `written` reaches the read set
    /// of `invariant` at `anchor`: the storage of a fragment-integer datum,
    /// or the descriptor word a measure member names, which a write of that
    /// word, of the measured place or above it replaces, and a write of an
    /// element or of another word does not [MSR-2]. A `relocation` write of
    /// `grow` or `grow_paged` at a cell reaches no datum its classification
    /// keeps, such as the length of the cell's window.
    fn write_reaches_read_set(
        &self,
        oracle: &dyn SeparationOracle,
        written: &ResolvedPlace,
        anchor: &ResolvedPlace,
        invariant: &CheckedAffineInvariant,
        relocation: bool,
    ) -> bool {
        invariant.data.iter().any(|datum| {
            let mut place = anchor.clone();
            place.path.extend(datum_steps(datum));
            if !relocation {
                return places_overlap(oracle, written, &place);
            }
            // A measure reads its supporting word [MSR-2].
            if let Some(PlaceStep::Measure(measure)) = place.path.last_mut() {
                *measure = measure.support_word();
            }
            self.relocation_reaches(oracle, &place, written)
        })
    }

    /// [TYPE-11.S3] the write row: a write at `written` adds a debt for each
    /// anchor strictly above the first `limit` steps of the written place and
    /// each affine invariant of the anchor whose read set the write reaches,
    /// a `relocation` write under its [ENT-5] classification. A write through
    /// the reference `through` names, whose referent is the written place's
    /// first steps up to the given count, records that reference on each
    /// debt at or below the referent [TYPE-11.S5].
    #[allow(clippy::too_many_arguments)]
    pub(super) fn seal_write(
        &self,
        debts: &mut Debts,
        oracle: &dyn SeparationOracle,
        written: &ResolvedPlace,
        limit: usize,
        through: Option<(BindingId, usize)>,
        relocation: bool,
        source: &crate::NodePath,
    ) {
        if !self.sealing {
            return;
        }
        let types = self.sealed_prefix_types(written);
        let cover = cover_step(written);
        for (depth, ty) in types.iter().enumerate().take(limit.min(written.path.len())) {
            // A cover's prefix R is the reference's current target, whose
            // anchor is named through its identity one step below, or lies
            // above that target, where the cover stands for it.
            let Some(ty) = ty.filter(|_| cover != Some(depth)) else {
                continue;
            };
            let invariants = self.affine_invariants(ty);
            if invariants.is_empty() {
                continue;
            }
            let anchor = ResolvedPlace {
                atomic_aliases: written.atomic_aliases.clone(),
                root: written.root,
                path: written.path[..depth].to_vec(),
            };
            for (index, invariant) in invariants.iter().enumerate() {
                if self.write_reaches_read_set(oracle, written, &anchor, invariant, relocation) {
                    debts.add(Debt {
                        anchor: anchor.clone(),
                        invariant: Some(index),
                        events: vec![source.clone()],
                        widened: false,
                        through: through.filter(|(_, referent)| depth >= *referent).map(
                            |(holder, referent)| Through {
                                holder,
                                steps: written.path[referent..depth].to_vec(),
                            },
                        ),
                        origin: SealingOrigin::Written,
                    });
                }
            }
        }
        // The anchor at the reference's current target and those below it
        // were named above. One the cover `R.**` hides strictly above that
        // target, R included, reads the written place only through a value
        // of the target's type, so the write adds the cover only when an
        // invariant of R, or of each element of a run R, or of a struct value
        // they own, reads through that type.
        if let Some(position) = cover
            && position < limit
            && let PlaceStep::Descendant(target) = written.path[position]
            && let Some(Selected::Value(ty) | Selected::Run(ty)) =
                self.sealed_selections(written).get(position).copied()
            && self.reads_through(ty, target.ty)
        {
            debts.add(Debt {
                anchor: ResolvedPlace {
                    atomic_aliases: written.atomic_aliases.clone(),
                    root: written.root,
                    path: written.path[..position].to_vec(),
                },
                invariant: None,
                events: vec![source.clone()],
                widened: false,
                through: None,
                origin: SealingOrigin::Written,
            });
        }
    }

    /// [REF-1] the resolved members of `place`, or the place as spelled when
    /// its root resolves to none.
    fn resolve_sealed(&self, place: &ResolvedPlace) -> Vec<ResolvedPlace> {
        let resolved = self.places.resolve(place.root, &place.path);
        if resolved.is_empty() {
            vec![place.clone()]
        } else {
            uncovered(resolved)
        }
    }

    /// [REF-1] the resolved places `expression` reads or names, through the
    /// references it reads them by; none for a value no place holds.
    pub(super) fn sealed_places(&self, expression: &CheckedExpression) -> Vec<ResolvedPlace> {
        let extend = |places: Vec<ResolvedPlace>, step: PlaceStep| {
            places
                .into_iter()
                .map(|mut place| {
                    place.path.push(step);
                    place
                })
                .collect()
        };
        match expression {
            CheckedExpression::Binding { binding, .. } => {
                self.resolve_sealed(&ResolvedPlace::binding(*binding))
            }
            CheckedExpression::DerefAddressed { binding, proof, .. } => match proof {
                Some(proof) => vec![proof.clone()],
                None => self.resolve_sealed(&ResolvedPlace::binding(*binding)),
            },
            CheckedExpression::Project {
                binding, fields, ..
            } => {
                let mut place = ResolvedPlace::binding(*binding);
                place
                    .path
                    .extend(fields.iter().copied().map(PlaceStep::Field));
                self.resolve_sealed(&place)
            }
            // [TYPE-9] a take moves the selected content out of its box.
            CheckedExpression::BoxTake { binding, path, .. } => {
                let mut place = ResolvedPlace::binding(*binding);
                place
                    .path
                    .extend(path.iter().map(CheckedPlaceStep::place_step));
                self.resolve_sealed(&place)
            }
            CheckedExpression::ProjectValue { value, field, .. } => {
                extend(self.sealed_places(value), PlaceStep::Field(*field))
            }
            CheckedExpression::BoxDeref { value, .. } => {
                extend(self.sealed_places(value), PlaceStep::Deref)
            }
            CheckedExpression::ReadStorage { root, .. } => self.resolve_sealed(&root.proof_place()),
            CheckedExpression::RangeIndex { place, .. } => {
                self.resolve_sealed(&place.proof_place())
            }
            _ => named_place(expression)
                .map(|named| uncovered(named.resolve(&self.places, true)))
                .unwrap_or_default(),
        }
    }

    /// [TYPE-11.S5] the image of `anchor`'s value the instance reads its
    /// binder data from: the anchor's own place, or, for an anchor that
    /// carries a reference's current target identity, that reference's.
    fn sealed_image(&self, anchor: &ResolvedPlace, ty: CheckedType) -> Option<GoalExpression> {
        if has_unknown_offset(anchor) {
            return None;
        }
        match anchor
            .path
            .iter()
            .rposition(|step| matches!(step, PlaceStep::Descendant(_)))
        {
            Some(position) => {
                let PlaceStep::Descendant(target) = anchor.path[position] else {
                    return None;
                };
                let projections = anchor.path[position + 1..]
                    .iter()
                    .map(goal_projection_of_step)
                    .collect::<Option<Vec<_>>>()?;
                Some(goal_binding_place(target.holder, projections, ty))
            }
            None => self.goal_place_datum(anchor, ty),
        }
    }

    /// [TYPE-11.S1] the instance of the affine invariant at `index` at
    /// `anchor`, each binder datum read from the anchor's image.
    fn sealed_instance(&self, anchor: &ResolvedPlace, index: usize) -> Option<GoalExpression> {
        let ty = (*self.sealed_prefix_types(anchor).last()?)?;
        let image = self.sealed_image(anchor, ty)?;
        self.instance_over(&image, ty, index)
    }

    /// [TYPE-11.S5] the instance that restores `debt`: read over the terms
    /// the reference its breaking events all wrote through forms, or else
    /// from its anchor's image. A cover and an anchor at `[?]` form none.
    fn restoration_instance(&self, debt: &Debt) -> Option<GoalExpression> {
        let index = debt.invariant?;
        if has_unknown_offset(&debt.anchor) {
            return None;
        }
        let Some(through) = &debt.through else {
            return self.sealed_instance(&debt.anchor, index);
        };
        let ty = (*self.sealed_prefix_types(&debt.anchor).last()?)?;
        let referent = ResolvedPlace {
            atomic_aliases: Vec::new(),
            root: PlaceRoot::Binding(through.holder),
            path: through.steps.clone(),
        };
        let image = self.goal_place_datum(&referent, ty)?;
        self.instance_over(&image, ty, index)
    }

    /// [TYPE-11.S1] the instance of the affine invariant at `index` of the
    /// struct `ty`, each binder datum read from `image`.
    fn instance_over(
        &self,
        image: &GoalExpression,
        ty: CheckedType,
        index: usize,
    ) -> Option<GoalExpression> {
        let invariant = self.affine_invariants(ty).get(index)?;
        substitute_binder(&invariant.goal, &|projections, datum_ty| {
            let mut datum = image.clone();
            for projection in projections {
                datum = datum.with_projection(*projection, datum_ty)?;
            }
            Some(datum)
        })
    }

    /// Each anchor whose binder datum the resolved read place `place`
    /// selects, with the invariant's position, the anchor's struct and the
    /// datum's projection.
    fn place_anchors(
        &self,
        place: &ResolvedPlace,
    ) -> Vec<(ResolvedPlace, usize, CheckedType, Vec<GoalProjection>)> {
        let types = self.sealed_prefix_types(place);
        let mut anchors = Vec::new();
        for (depth, ty) in types.iter().enumerate().take(place.path.len()) {
            let Some(ty) = *ty else {
                continue;
            };
            for (index, invariant) in self.affine_invariants(ty).iter().enumerate() {
                for datum in &invariant.data {
                    if place.path[depth..].iter().copied().eq(datum_steps(datum)) {
                        anchors.push((
                            ResolvedPlace {
                                atomic_aliases: place.atomic_aliases.clone(),
                                root: place.root,
                                path: place.path[..depth].to_vec(),
                            },
                            index,
                            ty,
                            datum.clone(),
                        ));
                    }
                }
            }
        }
        anchors
    }

    /// [TYPE-11.S7] the resolved places the read `expression` reads: for a
    /// measure read, the measure member of each resolved member of the
    /// place it measures [OP-15], and for any other read the places
    /// [`Self::sealed_places`] gives.
    fn read_places(&self, expression: &CheckedExpression) -> Vec<ResolvedPlace> {
        let Some(read) = measure_read(expression) else {
            return self.sealed_places(expression);
        };
        self.resolve_sealed(&read.place)
            .into_iter()
            .map(|mut place| {
                place.path.push(PlaceStep::Measure(read.measure));
                place
            })
            .collect()
    }

    /// [ENT-2] the place the read `expression` spells, when it is a term: a
    /// measure read's measured place followed by its measure, or the place
    /// [`Self::read_place_path`] gives.
    fn read_term_place(&self, expression: &CheckedExpression) -> Option<ResolvedPlace> {
        let Some(read) = measure_read(expression) else {
            return self.read_place_path(expression);
        };
        read.term.is_some().then(|| {
            let mut place = read.place;
            place.path.push(PlaceStep::Measure(read.measure));
            place
        })
    }

    /// [TYPE-11.S7] each binder datum the read `expression` selects at every
    /// resolved member of its place. The instance reads its binder data from
    /// the one member's anchor, or, for a read that resolves through a
    /// reference to several members, from the place the read spells through
    /// that reference, which names the one member it reads [ENT-2, REF-1].
    fn read_anchors(&self, expression: &CheckedExpression) -> Vec<ReadAnchor> {
        let places = self.read_places(expression);
        let Some((first, rest)) = places.split_first() else {
            return Vec::new();
        };
        let spelled = if rest.is_empty() {
            None
        } else {
            self.read_term_place(expression)
        };
        let mut anchors = Vec::new();
        for (anchor, index, ty, datum) in self.place_anchors(first) {
            let steps = first.path.len() - anchor.path.len();
            let mut members = vec![anchor];
            for place in rest {
                let found = self.place_anchors(place).into_iter().find(
                    |(other, other_index, other_ty, other_datum)| {
                        *other_index == index
                            && *other_ty == ty
                            && *other_datum == datum
                            && place.path.len() - other.path.len() == steps
                    },
                );
                if let Some((other, ..)) = found {
                    members.push(other);
                }
            }
            if members.len() != places.len() {
                continue;
            }
            let image = if rest.is_empty() {
                self.sealed_image(&members[0], ty)
            } else {
                spelled
                    .as_ref()
                    .and_then(|spelled| self.spelled_image(spelled, &datum, ty))
            };
            anchors.push(ReadAnchor {
                members,
                index,
                ty,
                datum,
                image,
            });
        }
        anchors
    }

    /// The image of the anchor whose binder datum `datum` a read spelled as
    /// `spelled` selects: that spelling without the datum's own steps.
    fn spelled_image(
        &self,
        spelled: &ResolvedPlace,
        datum: &[GoalProjection],
        ty: CheckedType,
    ) -> Option<GoalExpression> {
        let keep = spelled.path.len().checked_sub(datum.len())?;
        if !spelled.path[keep..].iter().copied().eq(datum_steps(datum)) {
            return None;
        }
        let image = ResolvedPlace {
            atomic_aliases: spelled.atomic_aliases.clone(),
            root: spelled.root,
            path: spelled.path[..keep].to_vec(),
        };
        self.goal_place_datum(&image, ty)
    }

    /// [TYPE-11.S7] whether an event of `kills` may have ended what the
    /// anchor `member` named when a call's return left its value sealed: it
    /// consumes the anchor's root or rebinds a reference whose target
    /// identity the anchor carries, replaces what one of its offsets reads
    /// [ENT-5], or writes, at or above its level, the enum a payload step of
    /// the anchor selects from, which ends the refinement that step stands on
    /// [REF-2, ENT-3.S15].
    fn delivery_ended(
        &self,
        separations: &dyn SeparationOracle,
        member: &ResolvedPlace,
        kills: &[KillEvent],
    ) -> bool {
        let enums = member
            .path
            .iter()
            .enumerate()
            .filter(|(_, step)| matches!(step, PlaceStep::Payload { .. }))
            .map(|(at, _)| ResolvedPlace {
                atomic_aliases: member.atomic_aliases.clone(),
                root: member.root,
                path: member.path[..at].to_vec(),
            })
            .collect::<Vec<_>>();
        kills.iter().any(|event| {
            self.event_kills_offset_support(separations, member, event)
                || match event {
                    KillEvent::Rebind { binding, .. } | KillEvent::Consume { binding, .. } => {
                        member.root == PlaceRoot::Binding(*binding)
                            || carried_by(member, *binding).is_some()
                    }
                    KillEvent::Write { place, .. }
                    | KillEvent::EntryImageHolderWrite { place, .. } => enums.iter().any(|owner| {
                        let [(least, _), (_, most)] = self.level_bounds(place, owner);
                        least <= most && places_overlap(separations, place, owner)
                    }),
                    KillEvent::EntryImageHolderConsume { .. } => false,
                }
        })
    }

    /// [TYPE-11.S3] the debts a continuation from `from` into `onto` takes:
    /// each debt that may be at or below a member of `from`, rebased.
    fn taken_debts(
        &self,
        state: &ProofFlowState,
        from: &[ResolvedPlace],
        onto: &ResolvedPlace,
    ) -> Vec<Debt> {
        let mut taken = Vec::new();
        for place in from {
            for debt in &state.debts.0 {
                if self.may_be_at_or_below(&state.separations, debt, place) {
                    taken.push(self.rebased(debt, place, onto));
                }
            }
        }
        taken
    }

    /// [TYPE-11.S4] the referent of each reference parameter whose row
    /// declares a write rooted at it, which every exit hands off.
    pub(super) fn written_referents(&self) -> Vec<ResolvedPlace> {
        let Some(callee) = self.context.callee(self.function.id) else {
            return Vec::new();
        };
        self.function
            .parameters
            .iter()
            .zip(&callee.parameter_writes)
            .filter(|(parameter, writes)| parameter.mode != CheckedMode::Own && !writes.is_empty())
            .flat_map(|(parameter, _)| {
                self.resolve_sealed(&ResolvedPlace::binding(parameter.binding))
            })
            .collect()
    }

    /// The anchor as a residual renders it.
    fn render_anchor(&self, anchor: &ResolvedPlace) -> String {
        self.sealed_prefix_types(anchor)
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.goal_place_datum(anchor, ty))
            .map(|image| self.render_concrete_goal(&image))
            .unwrap_or_else(|| match anchor.root {
                PlaceRoot::Binding(binding) => self
                    .context
                    .binding_names
                    .get(binding.0 as usize)
                    .cloned()
                    .unwrap_or_default(),
                PlaceRoot::Constant(_) => String::new(),
            })
    }

    /// [TYPE-11.S2] the unfolding below `place`, whose value has type `ty`,
    /// under `ancestors`: the debts an atomic block's target starts with, a
    /// map or `Entries` parameter at body entry, and a continuation of a
    /// `shared_close` result. Every anchor strictly below `place` that its
    /// value holds, at the unknown position for each element and entry, and
    /// the cover of each place where a held type recurs.
    fn unfolding_below(
        &self,
        place: &mut ResolvedPlace,
        ty: CheckedType,
        ancestors: &mut Vec<CheckedType>,
        source: &crate::NodePath,
        origin: SealingOrigin,
        debts: &mut Debts,
    ) {
        ancestors.push(ty);
        for (steps, below) in self.unfolded_parts(ty) {
            if self.owns_anchor(below, &mut Vec::new()) {
                let depth = place.path.len();
                place.path.extend(steps);
                self.unfolding_at(place, below, ancestors, source, origin, debts);
                place.path.truncate(depth);
            }
        }
        ancestors.pop();
    }

    /// [TYPE-11.S2] the unfolding of the value at `place`, of type `ty`,
    /// under `ancestors`: the cover of `place` when `ty` is among them, and
    /// otherwise a debt for each affine invariant of `ty` with the unfolding
    /// below `place`.
    fn unfolding_at(
        &self,
        place: &mut ResolvedPlace,
        ty: CheckedType,
        ancestors: &mut Vec<CheckedType>,
        source: &crate::NodePath,
        origin: SealingOrigin,
        debts: &mut Debts,
    ) {
        let recurs = ancestors.contains(&ty);
        let invariants = if recurs {
            vec![None]
        } else {
            (0..self.affine_invariants(ty).len()).map(Some).collect()
        };
        for invariant in invariants {
            debts.add(Debt {
                anchor: place.clone(),
                invariant,
                events: vec![source.clone()],
                widened: false,
                through: None,
                origin,
            });
        }
        if !recurs {
            self.unfolding_below(place, ty, ancestors, source, origin, debts);
        }
    }

    /// [TYPE-11.S2] the steps to each value one level below a value of type
    /// `ty` that it owns [TYPE-11.S1] or, as a map or an `Entries`, holds as
    /// an entry, with that value's type; a segment of a `Segments` is a run
    /// its element subscript selects from [REF-4].
    fn unfolded_parts(&self, ty: CheckedType) -> Vec<(Vec<PlaceStep>, CheckedType)> {
        let unknown = PlaceStep::Index(CapturedValue::unknown());
        match ty {
            CheckedType::Nominal(nominal) => {
                match self
                    .context
                    .nominals
                    .get(nominal.0 as usize)
                    .map(|record| &record.kind)
                {
                    Some(CheckedNominalKind::Struct { fields }) => fields
                        .iter()
                        .enumerate()
                        .filter_map(|(ordinal, field)| {
                            Some((vec![PlaceStep::Field(u32::try_from(ordinal).ok()?)], field.ty))
                        })
                        .collect(),
                    Some(CheckedNominalKind::Enum { variants }) => variants
                        .iter()
                        .flat_map(|variant| {
                            variant
                                .fields
                                .iter()
                                .enumerate()
                                .filter_map(|(ordinal, field)| {
                                    Some((
                                        vec![PlaceStep::Payload {
                                            variant: variant.tag,
                                            field: u32::try_from(ordinal).ok()?,
                                        }],
                                        field.ty,
                                    ))
                                })
                        })
                        .collect(),
                    Some(CheckedNominalKind::Box { referent, .. }) => {
                        vec![(vec![PlaceStep::Deref], *referent)]
                    }
                    Some(CheckedNominalKind::Shared {
                        shape: CheckedShared::Map { entry },
                        ..
                    }) => vec![(vec![unknown], *entry)],
                    _ => Vec::new(),
                }
            }
            CheckedType::Array { .. }
            | CheckedType::Buffer { .. }
            | CheckedType::Window { .. }
            | CheckedType::Entries { .. } => ty
                .storage_element()
                .and_then(|element| self.context.elements.get(element.index()).copied())
                .map(|element| (vec![unknown], element))
                .into_iter()
                .collect(),
            CheckedType::Segments { .. } => ty
                .storage_element()
                .and_then(|element| self.context.elements.get(element.index()).copied())
                .map(|element| (vec![unknown, unknown], element))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// [TYPE-11.S2] the `shared_close` call `value` is, with its result's
    /// type.
    fn closed_state<'value>(
        &self,
        value: &'value CheckedExpression,
    ) -> Option<(&'value crate::NodePath, CheckedType)> {
        let CheckedExpression::UserCall {
            function,
            call,
            result,
            ..
        } = value
        else {
            return None;
        };
        self.context
            .callee(*function)
            .is_some_and(|callee| callee.closes_state)
            .then_some((call, *result))
    }

    /// [TYPE-11.S2] the debts a continuation of `value` into `binding` adds
    /// when `value` is a `shared_close` result: the unfolding below the place
    /// its state takes, the `Some` payload of a binding of the whole result
    /// or the binding itself when it binds that payload. `None` for any other
    /// value, which carries no debt of its own [TYPE-11.S3].
    fn closed_state_debts(
        &self,
        value: &CheckedExpression,
        binding: BindingId,
    ) -> Option<Vec<Debt>> {
        let (call, result) = self.closed_state(value)?;
        let mut debts = Debts::default();
        let Some(ty) = self.summary(binding).and_then(|summary| summary.ty) else {
            return Some(debts.0);
        };
        let mut place = ResolvedPlace::binding(binding);
        let states = if ty == result {
            self.unfolded_parts(result)
        } else {
            vec![(Vec::new(), ty)]
        };
        for (steps, state) in states {
            place.path.extend(steps);
            self.unfolding_below(
                &mut place,
                state,
                &mut Vec::new(),
                call,
                SealingOrigin::Closed,
                &mut debts,
            );
            place.path.clear();
        }
        Some(debts.0)
    }

    /// [TYPE-11.S4] the `shared_close` call whose result `value` is, when its
    /// state may own a struct value with an affine type invariant, which no
    /// relation names while no place holds the result.
    fn unsealed_closed_result<'value>(
        &self,
        value: &'value CheckedExpression,
    ) -> Option<&'value crate::NodePath> {
        let (call, result) = self.closed_state(value)?;
        self.owned_types(result)
            .into_iter()
            .flat_map(|state| self.owned_types(state))
            .any(|below| self.owns_anchor(below, &mut Vec::new()))
            .then_some(call)
    }
}

impl Reasoning<'_, '_, '_> {
    /// [TYPE-11.S5] whether `debt` is restored in `state`: its restoring
    /// instance is proved by [MSR-4]'s disposition of an [FN-8] goal.
    fn sealed_debt_restored(&mut self, debt: &Debt, state: &ProofFlowState) -> bool {
        self.input
            .restoration_instance(debt)
            .is_some_and(|goal| self.instance_proved(goal, state))
    }

    /// [TYPE-11.S5] whether `debt`, at the unknown position, is satisfied at
    /// the handed `place`: the instance at the member the place fixes is
    /// proved, so the handed value is valid.
    fn debt_satisfied_at(
        &mut self,
        debt: &Debt,
        place: &ResolvedPlace,
        state: &ProofFlowState,
    ) -> bool {
        handed_member(debt, place)
            .zip(debt.invariant)
            .and_then(|(member, index)| self.input.sealed_instance(&member, index))
            .is_some_and(|goal| self.instance_proved(goal, state))
    }

    /// Whether `goal` is proved in `state` by [MSR-4]'s disposition of an
    /// [FN-8] goal.
    fn instance_proved(&mut self, goal: GoalExpression, state: &ProofFlowState) -> bool {
        let (disposition, _, _) = self.call_goal_disposition(
            &ConcreteGoal::new(goal),
            ProofContext::new(&state.facts, &state.affine),
        );
        disposition == CallGoalDisposition::Discharged
    }

    /// [TYPE-11.S3] the widening row for a kill batch that writes or ends
    /// `binding`: each debt whose anchor reads it in an offset, or carries its
    /// current target identity, or which is restored through it, is offered
    /// for restoration first, and widened when it is not restored.
    pub(super) fn seal_binding_change(&mut self, state: &mut ProofFlowState, binding: BindingId) {
        if state.debts.is_empty() {
            return;
        }
        let input = self.input;
        let reads = |step: &PlaceStep| input.step_reads(step, binding);
        let unrestored = self.unrestored_debts(state, |debt| {
            debt.anchor.path.iter().any(reads)
                || carried_by(&debt.anchor, binding).is_some()
                || debt
                    .through
                    .as_ref()
                    .is_some_and(|through| through.holder == binding)
        });
        for debt in unrestored {
            state.debts.add(widened(debt, binding, reads));
        }
    }

    /// [TYPE-11.S3] the widening row for the places a kill batch's `events`
    /// write or end [ENT-5]: each debt with an offset that reads one is
    /// offered for restoration first, in the state before the batch, and has
    /// its offsets widened from the first such one when it is not restored.
    pub(super) fn seal_offset_kills(
        &mut self,
        separations: &dyn SeparationOracle,
        state: &mut ProofFlowState,
        events: &[KillEvent],
    ) {
        if state.debts.is_empty() || events.is_empty() {
            return;
        }
        let input = self.input;
        let killed = |step: &PlaceStep| {
            events
                .iter()
                .any(|event| input.step_killed(separations, step, event))
        };
        let unrestored = self.unrestored_debts(state, |debt| debt.anchor.path.iter().any(killed));
        for mut debt in unrestored {
            if let Some(first) = debt.anchor.path.iter().position(killed) {
                widen_offsets_from(&mut debt, first);
            }
            state.debts.add(debt);
        }
    }

    /// [TYPE-11.S5] the restoration row of an exchange: a value repaired
    /// before the exchange moves sealed, so each debt at or below one of its
    /// `targets` is offered in the state before the swap's events, which kill
    /// the facts that restore it.
    pub(super) fn restore_before_exchange(
        &mut self,
        state: &mut ProofFlowState,
        targets: &[&ResolvedPlace],
    ) {
        let input = self.input;
        let separations = state.separations.clone();
        let unrestored = self.unrestored_debts(state, |debt| {
            targets
                .iter()
                .any(|place| input.may_be_at_or_below(&separations, debt, place))
        });
        for debt in unrestored {
            state.debts.add(debt);
        }
    }

    /// Offers each debt of `state` that `select` picks for restoration in
    /// `state`, removing the restored ones, and returns the others, taken out
    /// of the set.
    fn unrestored_debts(
        &mut self,
        state: &mut ProofFlowState,
        select: impl FnMut(&Debt) -> bool,
    ) -> Vec<Debt> {
        let offered = state.debts.take(select);
        offered
            .into_iter()
            .filter(|debt| !self.sealed_debt_restored(debt, state))
            .collect()
    }
}

impl Judging<'_, '_, '_> {
    /// [TYPE-11.S4] one hand-off of `places`: each debt that may be at or
    /// below one of them is offered for restoration in the state before the
    /// hand-off's own events and leaves the set when restored; one at the
    /// unknown position that each such place satisfies stays [TYPE-11.S5]. A
    /// debt neither restored nor satisfied is a failure of `rule` at `site`;
    /// returns whether there was none.
    pub(super) fn seal_hand_off(
        &mut self,
        state: &mut ProofFlowState,
        places: &[ResolvedPlace],
        rule: SealingRule,
        site: &crate::NodePath,
    ) -> bool {
        if state.debts.is_empty() || places.is_empty() {
            return true;
        }
        let separations = state.separations.clone();
        let input = self.input;
        let owed = |debt: &Debt, place: &ResolvedPlace| {
            input.may_be_at_or_below(&separations, debt, place) && !input.below_entry(debt, place)
        };
        let unrestored = self
            .reasoning()
            .unrestored_debts(state, |debt| places.iter().any(|place| owed(debt, place)));
        let mut failed = Vec::new();
        for debt in unrestored {
            let handed: Vec<&ResolvedPlace> =
                places.iter().filter(|place| owed(&debt, place)).collect();
            let satisfied = has_unknown_offset(&debt.anchor)
                && handed
                    .iter()
                    .all(|place| self.reasoning().debt_satisfied_at(&debt, place, state));
            if satisfied {
                state.debts.add(debt);
            } else {
                failed.push((debt, handed.first().map(|place| (*place).clone())));
            }
        }
        let Some((first, handed)) = failed.first() else {
            return true;
        };
        let ty = self
            .input
            .sealed_prefix_types(&first.anchor)
            .last()
            .copied()
            .flatten();
        let clause = ty.and_then(|ty| {
            let invariants = self.input.affine_invariants(ty);
            first
                .invariant
                .and_then(|index| invariants.get(index))
                .or_else(|| invariants.first())
                .map(|invariant| invariant.clause.clone())
        });
        // A debt at the unknown position that the handed place fixes names
        // the instance there, which is what the hand-off owes.
        let member = handed
            .as_ref()
            .filter(|_| has_unknown_offset(&first.anchor))
            .and_then(|place| handed_member(first, place))
            .and_then(|member| {
                let goal = self.input.sealed_instance(&member, first.invariant?)?;
                Some((member, goal))
            });
        let (anchor, goal, widened, member) = match member {
            Some((member, goal)) => (member, Some(goal), false, true),
            None => (
                first.anchor.clone(),
                self.input.restoration_instance(first),
                first.widened,
                false,
            ),
        };
        // A replacement removes the debt at the place an unknown position
        // lies in [TYPE-11.S3].
        let mut replaced = anchor.clone();
        if let Some(at) = replaced.path.iter().position(unknown_step) {
            replaced.path.truncate(at);
        }
        let replaced = self.input.render_anchor(&replaced);
        let anchor = self.input.render_anchor(&anchor);
        let rendered_goal = goal
            .as_ref()
            .map(|goal| self.input.render_concrete_goal(goal))
            .unwrap_or_else(|| format!("the type invariants of `{anchor}`"));
        self.output.sealing_failures.push(SealingFailure {
            rule,
            site: site.clone(),
            clause,
            anchor,
            replaced,
            goal: goal.map(ConcreteGoal::new),
            rendered_goal,
            events: first.events.clone(),
            widened,
            origin: first.origin,
            member,
        });
        false
    }

    /// [TYPE-11.S4] a hand-off of `value` itself at `site`: a failure of
    /// `rule` when `value` is a `shared_close` result whose state may own a
    /// struct value with an affine type invariant, which no relation names
    /// while no place holds the result.
    pub(super) fn seal_closed_hand_off(
        &mut self,
        value: &CheckedExpression,
        rule: SealingRule,
        site: &crate::NodePath,
    ) {
        let Some(call) = self.input.unsealed_closed_result(value) else {
            return;
        };
        let anchor = String::from("the state `shared_close` returns");
        self.output.sealing_failures.push(SealingFailure {
            rule,
            site: site.clone(),
            clause: None,
            replaced: anchor.clone(),
            goal: None,
            rendered_goal: format!("the type invariants of each value {anchor} owns"),
            anchor,
            events: vec![call.clone()],
            widened: false,
            origin: SealingOrigin::ClosedResult,
            member: false,
        });
    }
}

impl Analyzer<'_, '_> {
    /// [TYPE-11.S6] the debts after a merge of `states`: each incoming edge
    /// first offers each of its debts for restoration in its own state, and
    /// the debts that remain on some edge are united; then each key some edge
    /// carried before restoration and none carries after it has its instance
    /// established on each edge where its anchor meets [TYPE-11.S7]'s
    /// premises, before the fact states are joined.
    pub(super) fn seal_join(&mut self, states: &mut [ProofFlowState]) -> Debts {
        let mut joined = Debts::default();
        let mut offered: Vec<Debt> = Vec::new();
        for state in states.iter_mut() {
            if state.debts.is_empty() {
                continue;
            }
            for debt in &state.debts.0 {
                if !offered.iter().any(|known| known.same_key(debt)) {
                    offered.push(debt.clone());
                }
            }
            for debt in self.reasoning().unrestored_debts(state, |_| true) {
                state.debts.add(debt.clone());
                joined.add(debt);
            }
        }
        for debt in offered {
            let Some(index) = debt.invariant else {
                continue;
            };
            // A payload step's refinement may hold on one edge and not on
            // another, so no instance below one is established here.
            if joined.0.iter().any(|left| left.same_key(&debt))
                || debt
                    .anchor
                    .path
                    .iter()
                    .any(|step| matches!(step, PlaceStep::Payload { .. }))
            {
                continue;
            }
            let Some(goal) = self.input.sealed_instance(&debt.anchor, index) else {
                continue;
            };
            for state in states.iter_mut() {
                if !state.facts.all_derivable && self.sealed_anchor(&debt.anchor, index, state) {
                    self.establish_sealed_instance(&debt.anchor, index, goal.clone(), state);
                }
            }
        }
        joined
    }

    /// [TYPE-11.S7] at body entry, the instance of each affine invariant at
    /// each parameter of its struct and at the referent of each reference
    /// parameter to it, which the call handed off. A range parameter's type
    /// is its element's, so it takes none here. [TYPE-11.S2] A reference
    /// parameter to a map or an `Entries` starts with a debt at each anchor
    /// its entries hold.
    pub(super) fn seal_body_entry(&mut self, state: &mut ProofFlowState) {
        if !self.input.sealing {
            return;
        }
        let function = self.input.function;
        for parameter in &function.parameters {
            if !matches!(parameter.mode, CheckedMode::Own | CheckedMode::Reference) {
                continue;
            }
            let Some(ty) = self
                .input
                .summary(parameter.binding)
                .and_then(|summary| summary.ty)
            else {
                continue;
            };
            let mut anchor = ResolvedPlace::binding(parameter.binding);
            for index in 0..self.input.affine_invariants(ty).len() {
                if let Some(goal) = self.input.sealed_instance(&anchor, index) {
                    self.establish_sealed_instance(&anchor, index, goal, state);
                }
            }
            // [TYPE-11.S2] the entries of a map or an `Entries` referent are
            // no part of the value its hand-off delivered.
            if parameter.mode == CheckedMode::Reference && self.input.holds_entries(ty) {
                self.input.unfolding_below(
                    &mut anchor,
                    ty,
                    &mut Vec::new(),
                    &parameter.node_path,
                    SealingOrigin::Unsealed,
                    &mut state.debts,
                );
            }
        }
    }

    /// [TYPE-11.S4] a `return_stmt` or a propagated error exit hands off the
    /// places of the values it hands back and, with `referents`, each written
    /// referent.
    pub(super) fn seal_exit(
        &mut self,
        values: &[&CheckedExpression],
        referents: bool,
        site: &crate::NodePath,
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        for value in values {
            self.judging()
                .seal_closed_hand_off(value, SealingRule::Fn9, site);
        }
        if state.debts.is_empty() {
            return;
        }
        let mut places = values
            .iter()
            .flat_map(|value| self.input.sealed_places(value))
            .collect::<Vec<_>>();
        if referents {
            places.extend(self.input.written_referents());
        }
        self.judging()
            .seal_hand_off(state, &places, SealingRule::Fn9, site);
    }

    /// [TYPE-11.S4] a propagated error exit hands back the `Err` payload of
    /// the places its operand names, before the operand's own events.
    pub(super) fn seal_error_payload(
        &mut self,
        scrutinee: &CheckedExpression,
        site: &crate::NodePath,
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing || state.debts.is_empty() {
            return;
        }
        let places = self
            .input
            .sealed_places(scrutinee)
            .into_iter()
            .map(|mut place| {
                place.path.push(RESULT_ERR);
                place
            })
            .collect::<Vec<_>>();
        self.judging()
            .seal_hand_off(state, &places, SealingRule::Fn9, site);
    }

    /// [TYPE-11.S4] a `give_stmt`'s value, or a construction's operand, is
    /// handed off at `site`.
    pub(super) fn seal_value_hand_off(
        &mut self,
        value: &CheckedExpression,
        site: &crate::NodePath,
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        self.judging()
            .seal_closed_hand_off(value, SealingRule::Type11, site);
        if state.debts.is_empty() {
            return;
        }
        let places = self.input.sealed_places(value);
        self.judging()
            .seal_hand_off(state, &places, SealingRule::Type11, site);
    }

    /// [TYPE-11.S3, TYPE-11.S4, TYPE-11.S7] the sealing events of one judged
    /// expression, after its operands' and before its own kills: a call's
    /// hand-offs and write rows, a construction's operand hand-offs, and the
    /// instance a read of a binder datum establishes.
    pub(super) fn seal_expression(
        &mut self,
        expression: &CheckedExpression,
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        match expression {
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                actual_captures,
                formal_effects,
                ..
            } => self.seal_call(
                *function,
                call,
                arguments,
                actual_captures,
                formal_effects.as_deref(),
                state,
            ),
            CheckedExpression::ConstructStruct {
                carrier, fields, ..
            }
            | CheckedExpression::ConstructEnum {
                carrier, fields, ..
            } => {
                for field in fields {
                    self.seal_value_hand_off(field, carrier, state);
                }
            }
            _ => self.seal_read(expression, state),
        }
    }

    /// [TYPE-11.S7] after a read of a binder datum at a sealed anchor, the
    /// instance that anchor's delivering hand-off guaranteed.
    fn seal_read(&mut self, expression: &CheckedExpression, state: &mut ProofFlowState) {
        if !self.input.sealing {
            return;
        }
        for read in self.input.read_anchors(expression) {
            if !self.sealed_members(&read, state) {
                continue;
            }
            let Some(goal) = read
                .image
                .as_ref()
                .and_then(|image| self.input.instance_over(image, read.ty, read.index))
            else {
                continue;
            };
            self.establish_instance(Some(read.ty), read.index, goal, state);
        }
    }

    /// [TYPE-11.S7] the one-datum `let` form: when the initializer reads the
    /// only binder datum of an invariant at a sealed anchor whose data form no
    /// [ENT-2] term, the instance holds over the binding the `let` binds.
    pub(super) fn seal_let_datum(
        &mut self,
        binding: BindingId,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing || self.input.read_term_place(value).is_some() {
            return;
        }
        for read in self.input.read_anchors(value) {
            let Some(invariant) = self.input.affine_invariants(read.ty).get(read.index) else {
                continue;
            };
            if invariant.data.len() != 1 || !self.sealed_members(&read, state) {
                continue;
            }
            let goal = substitute_datum(&invariant.goal, &read.datum, &|datum_ty| {
                goal_binding_place(binding, Vec::new(), datum_ty)
            });
            if let Some(goal) = goal {
                self.establish_instance(Some(read.ty), read.index, goal, state);
            }
        }
    }

    /// [TYPE-11.S7] once the kills `kills` of the statement expression
    /// `expression` have run, the instance of each affine invariant at each
    /// referent of the struct one of its calls received in a hand-off at a
    /// reference parameter whose row writes it: the call's return left that
    /// referent sealed [TYPE-11.S4].
    pub(super) fn seal_call_returns(
        &mut self,
        expression: &CheckedExpression,
        kills: &[KillEvent],
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        for child in expression_children(expression) {
            self.seal_call_returns(child, kills, state);
        }
        let CheckedExpression::UserCall {
            function,
            arguments,
            formal_effects,
            ..
        } = expression
        else {
            return;
        };
        let context = self.input.context;
        let callee = context.callee(*function);
        if callee.map(|callee| callee.sealing).unwrap_or_default() != CalleeSealing::HandsOff {
            return;
        }
        for (index, argument) in arguments.iter().enumerate() {
            let writes = call_parameter_writes(callee, formal_effects.as_deref(), index);
            let transport = callee
                .and_then(|callee| callee.parameter_transports.get(index).copied())
                .unwrap_or_default();
            // A range reference's referent is a run, whose elements take no
            // instance here.
            if writes.is_none_or(|writes| writes.is_empty())
                || matches!(
                    transport,
                    CallTransport::ReadOnlyReference | CallTransport::ViewedRange
                )
            {
                continue;
            }
            let Some(named) = named_place(argument) else {
                continue;
            };
            let members = self
                .input
                .argument_referents(argument)
                .into_iter()
                .map(|(referent, _)| referent)
                .collect();
            let spelled = ResolvedPlace {
                atomic_aliases: Vec::new(),
                root: named.root,
                path: named.steps.iter().chain(&named.suffix).cloned().collect(),
            };
            self.seal_delivered(&uncovered(members), &spelled, kills, state);
        }
    }

    /// [TYPE-11.S7] once the statement that gave `binding` its value has run
    /// its kills, the instance of each affine invariant of the struct the
    /// binding holds. A reference binding receives no value of the struct:
    /// its place names the referent, which the statement did not give one.
    pub(super) fn seal_binding(&mut self, binding: BindingId, state: &mut ProofFlowState) {
        if self.input.places.is_reference(binding) {
            return;
        }
        self.seal_received(&ResolvedPlace::binding(binding), &[], state);
    }

    /// [TYPE-11.S7] once the kills `kills` of the statement that gave the
    /// place `spelled` names a value have run, the instance of each affine
    /// invariant of its struct there: a binding, a binder or a commit's
    /// target holds the value it received, which a debt it took [TYPE-11.S3]
    /// is the only thing that can keep unsealed.
    fn seal_received(
        &mut self,
        spelled: &ResolvedPlace,
        kills: &[KillEvent],
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        let members = self.input.resolve_sealed(spelled);
        self.seal_delivered(&members, spelled, kills, state);
    }

    /// [TYPE-11.S7] a [SET-1] commit: the instance at the place the commit
    /// wrote, after the commit's own kills `kills`. A reference rebind
    /// writes no value of the struct.
    pub(super) fn seal_set_target(
        &mut self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        kills: &[KillEvent],
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        if let KillEvent::Write { place, .. } = self.input.commit_kill(node_path, target) {
            self.seal_received(&place, kills, state);
        }
    }

    /// [TYPE-11.S7] the instance of each affine invariant of the struct at
    /// the place `spelled` names, whose resolved members are `members`, which
    /// a call's return or a statement's delivery left holding a received
    /// value: over the one member's anchor, or over the terms the place forms
    /// through a reference to several [ENT-2, REF-1], when every member meets
    /// the premises and no kill of `kills` ended what it names.
    fn seal_delivered(
        &mut self,
        members: &[ResolvedPlace],
        spelled: &ResolvedPlace,
        kills: &[KillEvent],
        state: &mut ProofFlowState,
    ) {
        let Some((first, rest)) = members.split_first() else {
            return;
        };
        let member_type = |member: &ResolvedPlace| {
            self.input
                .sealed_prefix_types(member)
                .last()
                .copied()
                .flatten()
        };
        let Some(ty) = member_type(first) else {
            return;
        };
        let count = self.input.affine_invariants(ty).len();
        if count == 0
            || rest.iter().any(|member| member_type(member) != Some(ty))
            || members
                .iter()
                .any(|member| self.input.delivery_ended(&state.separations, member, kills))
        {
            return;
        }
        for index in 0..count {
            if !members
                .iter()
                .all(|member| self.sealed_anchor(member, index, state))
            {
                continue;
            }
            let goal = if rest.is_empty() {
                self.input.sealed_instance(first, index)
            } else {
                self.input
                    .goal_place_datum(spelled, ty)
                    .and_then(|image| self.input.instance_over(&image, ty, index))
            };
            if let Some(goal) = goal {
                self.establish_instance(Some(ty), index, goal, state);
            }
        }
    }

    /// [TYPE-11.S7] the premises of the instance a read selects, at the
    /// anchor of each member of its place.
    fn sealed_members(&self, read: &ReadAnchor, state: &ProofFlowState) -> bool {
        read.members
            .iter()
            .all(|anchor| self.sealed_anchor(anchor, read.index, state))
    }

    /// [TYPE-11.S7] the premises of an instance at `anchor`: no debt may
    /// denote it, and its root binding is no atomic statement's target
    /// [SHARE-2]. A payload step of a read place is available only while its
    /// refinement holds [REF-1, ENT-3.S15].
    fn sealed_anchor(&self, anchor: &ResolvedPlace, index: usize, state: &ProofFlowState) -> bool {
        let targeted = matches!(anchor.root, PlaceRoot::Binding(binding)
            if self.frames.atomic.as_ref().is_some_and(|frame| frame.targets.contains(&binding)));
        !targeted
            && !state
                .debts
                .0
                .iter()
                .any(|debt| self.input.may_denote(&state.separations, debt, anchor, index))
    }

    fn establish_sealed_instance(
        &mut self,
        anchor: &ResolvedPlace,
        index: usize,
        goal: GoalExpression,
        state: &mut ProofFlowState,
    ) {
        let ty = self
            .input
            .sealed_prefix_types(anchor)
            .last()
            .copied()
            .flatten();
        self.establish_instance(ty, index, goal, state);
    }

    /// [ENT-3.S18] establishes `goal`, the instance of the affine invariant at
    /// `index` of the struct `ty`, with S4's affine images.
    fn establish_instance(
        &mut self,
        ty: Option<CheckedType>,
        index: usize,
        goal: GoalExpression,
        state: &mut ProofFlowState,
    ) {
        let clause = ty
            .and_then(|ty| self.input.affine_invariants(ty).get(index))
            .map(|invariant| invariant.clause.clone());
        let event = self
            .vocabulary
            .proof_event(FlowEventKind::S18, clause.as_ref());
        self.judging()
            .establish_body_goal(goal.clone(), &mut state.facts, event);
        self.reasoning().establish_affine_images(goal, None, state);
    }

    /// [TYPE-11.S3] the bindings whose scope ends below `depth`: the widening
    /// row for each, then the death of each as a root.
    pub(super) fn seal_scope_exit(&mut self, state: &mut ProofFlowState, depth: usize) {
        if state.debts.is_empty() {
            return;
        }
        let exited = self
            .frames
            .scopes
            .iter()
            .skip(depth)
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        for binding in &exited {
            self.reasoning().seal_binding_change(state, *binding);
        }
        for binding in exited {
            state.debts.remove_rooted(binding);
        }
    }

    /// [TYPE-11.S5, TYPE-11.S6] each backedge and `continue` of loop `id`
    /// offers every debt for restoration, after the widening row of a counted
    /// loop's `binder`; the debts left join the loop header's on the next
    /// iteration of the walk.
    pub(super) fn seal_backedges(
        &mut self,
        id: CheckedLoopId,
        binder: Option<BindingId>,
        edges: &mut [FlowEdge],
    ) {
        if !self.input.sealing {
            return;
        }
        let mut left = Debts::default();
        for edge in edges {
            if let Some(binder) = binder {
                self.reasoning().seal_binding_change(&mut edge.state, binder);
            }
            for debt in self.reasoning().unrestored_debts(&mut edge.state, |_| true) {
                edge.state.debts.add(debt.clone());
                left.add(debt);
            }
        }
        if !left.is_empty() {
            self.output
                .debt_backedges
                .entry(id)
                .or_default()
                .union(&left);
        }
    }

    /// [TYPE-11.S6] a loop header's debts: the entering edge's and those an
    /// earlier iteration of the walk found on the loop's backedges.
    pub(super) fn seal_loop_header(&mut self, id: CheckedLoopId, state: &mut ProofFlowState) {
        if let Some(seed) = self.input.debt_seeds.get(&id) {
            state.debts.union(seed);
        }
    }

    /// [TYPE-11.S3] a continuation from the places `value` names into the
    /// complete own local `binding`: the debts it takes, before the
    /// statement's own events.
    pub(super) fn sealed_continuation(
        &self,
        value: &CheckedExpression,
        binding: BindingId,
        state: &ProofFlowState,
    ) -> Vec<Debt> {
        if self.input.places.is_reference(binding) {
            return Vec::new();
        }
        if let Some(debts) = self.input.closed_state_debts(value, binding) {
            return debts;
        }
        if state.debts.is_empty() {
            return Vec::new();
        }
        let from = self.input.sealed_places(value);
        self.input
            .taken_debts(state, &from, &ResolvedPlace::binding(binding))
    }

    /// [TYPE-11.S3] a continuation from the part `step` selects of the
    /// places `value` names into the own binder `binding`: the debts it
    /// takes, before the statement's own events.
    pub(super) fn sealed_part_continuation(
        &self,
        value: &CheckedExpression,
        step: PlaceStep,
        binding: BindingId,
        state: &ProofFlowState,
    ) -> Vec<Debt> {
        if self.input.places.is_reference(binding) {
            return Vec::new();
        }
        if let Some(debts) = self.input.closed_state_debts(value, binding) {
            return debts;
        }
        if state.debts.is_empty() {
            return Vec::new();
        }
        let from = self
            .input
            .sealed_places(value)
            .into_iter()
            .map(|mut place| {
                place.path.push(step);
                place
            })
            .collect::<Vec<_>>();
        self.input
            .taken_debts(state, &from, &ResolvedPlace::binding(binding))
    }

    /// [TYPE-11.S3] the continuations of a `match` on `scrutinee`: each own
    /// binder of a variant arm takes the debts of the payload field it binds.
    /// A reference binder names that field in place and takes none.
    pub(super) fn sealed_binders(
        &self,
        scrutinee: &CheckedExpression,
        arms: &[CheckedMatchArm],
        state: &ProofFlowState,
    ) -> Vec<(BindingId, Vec<Debt>)> {
        if state.debts.is_empty() && self.input.closed_state(scrutinee).is_none() {
            return Vec::new();
        }
        let mut taken = Vec::new();
        for arm in arms {
            let CheckedMatchSelection::Variant(variant) = arm.selection else {
                continue;
            };
            for binder in &arm.binders {
                if binder.mode == CheckedMode::Own {
                    let step = PlaceStep::Payload {
                        variant,
                        field: binder.field,
                    };
                    let debts =
                        self.sealed_part_continuation(scrutinee, step, binder.binding, state);
                    if !debts.is_empty() {
                        taken.push((binder.binding, debts));
                    }
                }
            }
        }
        taken
    }

    /// [TYPE-11.S3] completes the continuations into the binders of `arm`.
    pub(super) fn bind_sealed(
        &self,
        arm: &CheckedMatchArm,
        taken: &[(BindingId, Vec<Debt>)],
        state: &mut ProofFlowState,
    ) {
        for binder in &arm.binders {
            for (binding, debts) in taken {
                if *binding == binder.binding {
                    self.finish_continuation(*binding, debts.clone(), state);
                }
            }
        }
    }

    /// [TYPE-11.S3] completes a continuation into `binding` after the
    /// statement's own events: removes the debts proved at or below the
    /// binding, then adds the taken ones.
    pub(super) fn finish_continuation(
        &self,
        binding: BindingId,
        taken: Vec<Debt>,
        state: &mut ProofFlowState,
    ) {
        state
            .debts
            .remove_proved_at_or_below(&ResolvedPlace::binding(binding));
        for debt in taken {
            state.debts.add(debt);
        }
    }

    /// [TYPE-11.S3, TYPE-11.S4] one `set` commit: `before` runs before the
    /// statement's own events and returns what `after` needs.
    pub(super) fn seal_set_before(
        &mut self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> SealedSet {
        if !self.input.sealing {
            return SealedSet::Inert;
        }
        let names_reference = match named_place(value).map(|named| named.form) {
            Some(NamingForm::Borrow | NamingForm::Range) => true,
            Some(NamingForm::Binding(named)) => self.input.places.is_reference(named),
            Some(NamingForm::Read) | None => false,
        };
        match target {
            CheckedSetTarget::Place(place)
                if place.fields.is_empty() && place.mode != CheckedMode::Own && names_reference =>
            {
                // A reference rebinding writes no storage [SET-1]; it ends the
                // current target identity the reference carried [REF-1].
                self.reasoning().seal_binding_change(state, place.binding);
                SealedSet::Rebinding
            }
            CheckedSetTarget::Place(place)
                if place.fields.is_empty() && place.mode == CheckedMode::Own =>
            {
                self.reasoning().seal_binding_change(state, place.binding);
                SealedSet::Local {
                    binding: place.binding,
                    taken: self.sealed_continuation(value, place.binding, state),
                }
            }
            _ => {
                let places = self.input.sealed_places(value);
                let handed = !places.is_empty();
                let mut judging = self.judging();
                judging.seal_closed_hand_off(value, SealingRule::Type11, node_path);
                judging.seal_hand_off(state, &places, SealingRule::Type11, node_path);
                SealedSet::Stored {
                    sealed: handed || sealed_value(value),
                }
            }
        }
    }

    pub(super) fn seal_set_after(
        &mut self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        sealed: SealedSet,
        state: &mut ProofFlowState,
    ) {
        match sealed {
            SealedSet::Inert | SealedSet::Rebinding => {}
            SealedSet::Local { binding, taken } => self.finish_continuation(binding, taken, state),
            SealedSet::Stored { sealed } => {
                let KillEvent::Write { place, .. } = self.input.commit_kill(node_path, target)
                else {
                    return;
                };
                // A write through a rebindable reference names its place
                // through that reference while it is not rebound.
                let holder = match place.root {
                    PlaceRoot::Binding(binding)
                        if self.input.places.reference_holder(binding).is_some() =>
                    {
                        Some(binding)
                    }
                    _ => None,
                };
                let members = self.input.resolve_sealed(&place);
                // A store at a place that resolves to several members is
                // proved at none of them, so it removes only the debts its
                // reference recorded [TYPE-11.S3].
                let several = members.len() > 1;
                for written in members {
                    let through = holder.map(|binding| {
                        (
                            binding,
                            written.path.len().saturating_sub(place.path.len()),
                        )
                    });
                    self.input.seal_write(
                        &mut state.debts,
                        &state.separations,
                        &written,
                        written.path.len(),
                        through,
                        false,
                        node_path,
                    );
                    if !sealed {
                        continue;
                    }
                    if !several {
                        state.debts.remove_proved_at_or_below(&written);
                    } else if let Some((binding, referent)) = through {
                        state
                            .debts
                            .remove_recorded_at_or_below(&written, binding, referent);
                    }
                }
            }
        }
    }

    /// [TYPE-11.S3, TYPE-11.S4] one call: the hand-off of each argument, or
    /// for a `swap` the restoration of each debt that may be at or below a
    /// target [TYPE-11.S5], then the write row of each projected write, then
    /// a `swap`'s exchange.
    pub(super) fn seal_call(
        &mut self,
        function: FunctionId,
        call: &crate::NodePath,
        arguments: &[CheckedExpression],
        actual_captures: &[CapturedValue],
        formal_effects: Option<&CheckedEffects>,
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        let context = self.input.context;
        let callee = context.callee(function);
        let sealing = callee.map(|callee| callee.sealing).unwrap_or_default();
        let handed = arguments
            .iter()
            .map(|argument| self.input.sealed_places(argument))
            .collect::<Vec<_>>();
        if sealing == CalleeSealing::HandsOff {
            for places in &handed {
                self.judging()
                    .seal_hand_off(state, places, SealingRule::Fn8, call);
            }
        }
        if sealing == CalleeSealing::Exchange {
            let targets: Vec<&ResolvedPlace> = handed.iter().flatten().collect();
            self.reasoning().restore_before_exchange(state, &targets);
        }
        let offsets = substituted_offsets(callee, actual_captures);
        for (index, argument) in arguments.iter().enumerate() {
            let Some(writes) = call_parameter_writes(callee, formal_effects, index) else {
                continue;
            };
            let transport = callee
                .and_then(|callee| callee.parameter_transports.get(index).copied())
                .unwrap_or_default();
            if transport == CallTransport::ReadOnlyReference || writes.is_empty() {
                continue;
            }
            // [ENT-5] the cell write of `grow` or `grow_paged`, as the kill
            // judgment classifies it.
            let relocation = formal_effects.is_none()
                && callee.is_some_and(|callee| callee.relocation)
                && index == 0;
            let referents = self
                .input
                .argument_referents(argument)
                .into_iter()
                .map(|(referent, _)| referent)
                .collect();
            for referent in uncovered(referents) {
                let written_places = if transport.writes_element_storage() {
                    vec![element_write_place(
                        referent.clone(),
                        CapturedValue::unknown(),
                    )]
                } else {
                    writes
                        .iter()
                        .map(|steps| {
                            let mut written = referent.clone();
                            written.path.extend(substituted_steps(steps, &offsets));
                            written
                        })
                        .collect()
                };
                for written in written_places {
                    if written.path.is_empty()
                        && let PlaceRoot::Binding(binding) = written.root
                    {
                        self.reasoning().seal_binding_change(state, binding);
                    }
                    self.input.seal_write(
                        &mut state.debts,
                        &state.separations,
                        &written,
                        referent.path.len(),
                        None,
                        relocation,
                        call,
                    );
                }
            }
        }
        if sealing == CalleeSealing::Exchange
            && let [first, second] = handed.as_slice()
        {
            self.seal_exchange(first, second, state);
        }
    }

    /// [TYPE-11.S3] `swap`'s exchange: each debt proved at or below one
    /// target moves onto the other, exchanged; each that may be at or below
    /// one target without being proved there stays and is copied onto the
    /// other the same way; when the two targets may be one place, each
    /// receives the union.
    fn seal_exchange(
        &self,
        first: &[ResolvedPlace],
        second: &[ResolvedPlace],
        state: &mut ProofFlowState,
    ) {
        let input = &self.input;
        let ([first], [second]) = (first, second) else {
            // A target with several resolved members is proved at none of
            // them: every debt there is copied and stays.
            let mut copies = Vec::new();
            for (from, onto) in [(first, second), (second, first)] {
                for source in from {
                    for target in onto {
                        for debt in &state.debts.0 {
                            if input.may_be_at_or_below(&state.separations, debt, source) {
                                copies.push(input.exchanged(debt, source, target));
                            }
                        }
                    }
                }
            }
            for debt in copies {
                state.debts.add(debt);
            }
            return;
        };
        let one_place = places_overlap(&state.separations, first, second);
        let mut moved = Vec::new();
        for (from, onto) in [(first, second), (second, first)] {
            for debt in &state.debts.0 {
                if input.may_be_at_or_below(&state.separations, debt, from) {
                    moved.push((
                        !one_place && proved_at_or_below(debt, from),
                        debt.clone(),
                        input.exchanged(debt, from, onto),
                    ));
                }
            }
        }
        for (proved, original, _) in &moved {
            if *proved {
                state.debts.0.retain(|debt| !debt.same_key(original));
            }
        }
        for (_, _, exchanged) in moved {
            state.debts.add(exchanged);
        }
    }

    /// [TYPE-11.S2] the debts an atomic block's `binding` starts with: the
    /// referent itself when the target `establishes` none of its type
    /// invariants, and every anchor strictly below it that it owns.
    pub(super) fn seal_atomic_entry(
        &self,
        binding: BindingId,
        establishes: bool,
        node_path: &crate::NodePath,
        state: &mut ProofFlowState,
    ) {
        if !self.input.sealing {
            return;
        }
        let Some(ty) = self.input.summary(binding).and_then(|summary| summary.ty) else {
            return;
        };
        let mut place = ResolvedPlace::binding(binding);
        if !establishes {
            for index in 0..self.input.affine_invariants(ty).len() {
                state.debts.add(Debt {
                    anchor: place.clone(),
                    invariant: Some(index),
                    events: vec![node_path.clone()],
                    widened: false,
                    through: None,
                    origin: SealingOrigin::Entered,
                });
            }
        }
        self.input.unfolding_below(
            &mut place,
            ty,
            &mut Vec::new(),
            node_path,
            SealingOrigin::Entered,
            &mut state.debts,
        );
    }
}

/// What a `set` commit's sealing does after its own events.
pub(super) enum SealedSet {
    /// No struct in the program declares an affine invariant.
    Inert,
    /// A reference rebinding.
    Rebinding,
    /// A commit into a complete own local binding: a continuation, which
    /// takes nothing from a construction or a call result other than a
    /// `shared_close` result [TYPE-11.S2].
    Local {
        binding: BindingId,
        taken: Vec<Debt>,
    },
    /// A commit into any other place, whose right-hand side is handed off
    /// when it is a place, and which is a sealed store when it is a hand-off,
    /// a call result or a construction.
    Stored { sealed: bool },
}

/// The payload field of the prelude `Result`'s `Ok` variant, tag 0.
pub(super) const RESULT_OK: PlaceStep = PlaceStep::Payload {
    variant: 0,
    field: 0,
};

/// The payload field of the prelude `Result`'s `Err` variant, tag 1.
const RESULT_ERR: PlaceStep = PlaceStep::Payload {
    variant: 1,
    field: 0,
};

/// Whether `value` is a call result or a construction, which carry no debt;
/// the hand-off of a `shared_close` result whose state may owe one fails
/// instead [TYPE-11.S4].
fn sealed_value(value: &CheckedExpression) -> bool {
    matches!(
        value,
        CheckedExpression::UserCall { .. }
            | CheckedExpression::ConstructStruct { .. }
            | CheckedExpression::ConstructEnum { .. }
    )
}

/// [EFF-5] the declared writes rooted at a call's parameter `index`: the
/// boundary row's for a call through a function-type parameter, otherwise
/// the callee's own; none when the callee is unknown.
fn call_parameter_writes<'callee>(
    callee: Option<&'callee EntailmentCallee>,
    formal_effects: Option<&CheckedEffects>,
    index: usize,
) -> Option<Cow<'callee, [Vec<CheckedEffectStep>]>> {
    let boundary = formal_effects.and_then(|effects| {
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
    match boundary {
        Some(writes) => Some(Cow::Owned(writes)),
        None => callee?
            .parameter_writes
            .get(index)
            .map(|writes| Cow::Borrowed(writes.as_slice())),
    }
}
