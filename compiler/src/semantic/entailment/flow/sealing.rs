//! [TYPE-11.S1] to [TYPE-11.S8]: sealed values.
//!
//! Beside its fact state, the walk carries the debts of the values the body
//! holds: the writes that may have broken an affine type invariant of a value
//! since the hand-off that delivered it to the body. A hand-off requires the
//! debts at or below what it hands on to be restored, and a read of a sealed
//! place establishes the instance the delivering hand-off guaranteed.

use super::super::super::model::{CheckedAffineInvariant, CheckedEffects, FunctionId};
use super::super::super::places::places_overlap;
use super::super::{CalleeSealing, SealingFailure, SealingRule};
use super::*;

/// One debt [TYPE-11.S2]: an anchor shape with one affine invariant of its
/// struct, or a cover standing for every anchor at or below its place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Debt {
    /// The anchor shape, or the place a cover stands below.
    anchor: ResolvedPlace,
    /// The invariant's position among the affine invariants of the anchor's
    /// struct; `None` for a cover.
    invariant: Option<usize>,
    /// The breaking events in walk order, which only the diagnostic reads.
    events: Vec<crate::NodePath>,
    /// Whether a widening lost an identity the debt was restorable through.
    widened: bool,
}

impl Debt {
    fn same_key(&self, other: &Self) -> bool {
        self.invariant == other.invariant && self.anchor == other.anchor
    }

    /// This debt with its anchor's first `depth` steps replaced by `onto`
    /// [TYPE-11.S3]; a cover above `onto` covers all of it.
    fn rebased(&self, depth: usize, onto: &ResolvedPlace) -> Self {
        let mut anchor = onto.clone();
        anchor
            .path
            .extend_from_slice(self.anchor.path.get(depth..).unwrap_or_default());
        Self {
            anchor,
            ..self.clone()
        }
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
    /// the same key [TYPE-11.S6].
    fn add(&mut self, debt: Debt) {
        if let Some(existing) = self.0.iter_mut().find(|known| known.same_key(&debt)) {
            for event in debt.events {
                if !existing.events.contains(&event) {
                    existing.events.push(event);
                }
            }
            existing.widened |= debt.widened;
        } else {
            self.0.push(debt);
        }
    }

    pub(super) fn union(&mut self, other: &Self) {
        for debt in &other.0 {
            self.add(debt.clone());
        }
    }

    /// Whether every key of these debts is a key of `other`.
    pub(super) fn keys_within(&self, other: &Self) -> bool {
        self.0
            .iter()
            .all(|debt| other.0.iter().any(|known| known.same_key(debt)))
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
}

/// [TYPE-11.S2] whether `debt` may be at or below `place`.
fn may_be_at_or_below(oracle: &dyn SeparationOracle, debt: &Debt, place: &ResolvedPlace) -> bool {
    (debt.invariant.is_none() || debt.anchor.path.len() >= place.path.len())
        && places_overlap(oracle, &debt.anchor, place)
}

/// [TYPE-11.S2] whether `debt` is proved at or below `place`: its anchor has
/// the place's root and begins with the place's steps, every captured offset
/// equal.
fn proved_at_or_below(debt: &Debt, place: &ResolvedPlace) -> bool {
    place.contains(&debt.anchor)
}

/// [TYPE-11.S2] whether `debt` may denote the invariant at `invariant` of
/// `anchor`.
fn may_denote(
    oracle: &dyn SeparationOracle,
    debt: &Debt,
    anchor: &ResolvedPlace,
    invariant: usize,
) -> bool {
    match debt.invariant {
        None => places_overlap(oracle, &debt.anchor, anchor),
        Some(own) => {
            own == invariant
                && debt.anchor.path.len() == anchor.path.len()
                && places_overlap(oracle, &debt.anchor, anchor)
        }
    }
}

/// Whether an offset of `place` reads the current value of `binding`.
fn offset_reads(place: &ResolvedPlace, binding: BindingId) -> bool {
    let reads = |value: &CapturedValue| value.term == CapturedTerm::Binding(binding);
    place.path.iter().any(|step| match step {
        PlaceStep::Index(value) | PlaceStep::Page(value) => reads(value),
        PlaceStep::Range(range) => reads(&range.start) || reads(&range.end),
        _ => false,
    })
}

/// The position of the step through which `place` carries the current
/// target identity of the reference `binding` [REF-1].
fn carried_by(place: &ResolvedPlace, binding: BindingId) -> Option<usize> {
    place
        .path
        .iter()
        .position(|step| matches!(step, PlaceStep::Descendant(target) if target.holder == binding))
}

/// Whether `place` has the unknown position `[?]` [TYPE-11.S2].
fn has_unknown_offset(place: &ResolvedPlace) -> bool {
    let unknown = |value: &CapturedValue| value.capture == CaptureId::Unknown;
    place.path.iter().any(|step| match step {
        PlaceStep::Index(value) | PlaceStep::Page(value) => unknown(value),
        PlaceStep::Range(range) => unknown(&range.start) || unknown(&range.end),
        _ => false,
    })
}

/// [TYPE-11.S3] `debt` after a kill batch wrote or ended `binding` while the
/// debt was not restored: an offset reading the binding widens to `[?]`, and
/// an anchor carrying the binding's current target identity widens to its
/// cover.
fn widened(mut debt: Debt, binding: BindingId) -> Debt {
    if let Some(position) = carried_by(&debt.anchor, binding) {
        debt.anchor.path.truncate(position);
        debt.invariant = None;
    }
    let current = CapturedTerm::Binding(binding);
    for step in &mut debt.anchor.path {
        match step {
            PlaceStep::Index(value) | PlaceStep::Page(value) if value.term == current => {
                *value = CapturedValue::unknown();
            }
            PlaceStep::Range(range) => {
                if range.start.term == current {
                    range.start = CapturedValue::unknown();
                }
                if range.end.term == current {
                    range.end = CapturedValue::unknown();
                }
            }
            _ => {}
        }
    }
    debt.widened = true;
    debt
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

    /// The type one step below a value of type `ty` reaches.
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
            PlaceStep::Index(_) | PlaceStep::Page(_) => self
                .context
                .elements
                .get(ty.storage_element()?.index())
                .copied(),
            PlaceStep::Range(_) | PlaceStep::Part(_) => Some(ty),
            PlaceStep::Descendant(target) => Some(target.ty),
            PlaceStep::Measure(_) => None,
        }
    }

    /// The type at each prefix of `place`, from its root to the whole place.
    fn sealed_prefix_types(&self, place: &ResolvedPlace) -> Vec<Option<CheckedType>> {
        let mut ty = match place.root {
            PlaceRoot::Binding(binding) => self.summary(binding).and_then(|summary| summary.ty),
            PlaceRoot::Constant(_) => None,
        };
        let mut types = Vec::with_capacity(place.path.len() + 1);
        types.push(ty);
        for step in &place.path {
            ty = ty.and_then(|ty| self.sealed_step_type(ty, step));
            types.push(ty);
        }
        types
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
        let owned: Vec<CheckedType> = match ty {
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
        };
        owned.into_iter().any(|ty| self.owns_anchor(ty, visited))
    }

    /// [TYPE-11.S1, ENT-5] whether a write at `written` reaches the read set
    /// of `invariant` at `anchor`: the storage of a fragment-integer datum,
    /// or the descriptor storage of a measured one, which a write at the
    /// measured place or above it replaces [MSR-2].
    fn write_reaches_read_set(
        &self,
        oracle: &dyn SeparationOracle,
        written: &ResolvedPlace,
        anchor: &ResolvedPlace,
        invariant: &CheckedAffineInvariant,
    ) -> bool {
        invariant.data.iter().any(|datum| {
            let mut place = anchor.clone();
            place.path.extend(datum_steps(datum));
            if matches!(place.path.last(), Some(PlaceStep::Measure(_))) {
                place.path.pop();
                written.path.len() <= place.path.len() && places_overlap(oracle, written, &place)
            } else {
                places_overlap(oracle, written, &place)
            }
        })
    }

    /// [TYPE-11.S3] the write row: a write at `written` adds a debt for each
    /// anchor strictly above the first `limit` steps of the written place and
    /// each affine invariant of the anchor whose read set the write reaches.
    pub(super) fn seal_write(
        &self,
        debts: &mut Debts,
        oracle: &dyn SeparationOracle,
        written: &ResolvedPlace,
        limit: usize,
        source: &crate::NodePath,
    ) {
        if !self.sealing {
            return;
        }
        let types = self.sealed_prefix_types(written);
        for depth in 0..limit.min(written.path.len()) {
            let Some(ty) = types[depth] else {
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
                if self.write_reaches_read_set(oracle, written, &anchor, invariant) {
                    debts.add(Debt {
                        anchor: anchor.clone(),
                        invariant: Some(index),
                        events: vec![source.clone()],
                        widened: false,
                    });
                }
            }
        }
        // A write through a cover `R.**` may reach an anchor the cover hides
        // whenever R owns one, so it adds that cover.
        if let Some(position) = written
            .path
            .iter()
            .position(|step| matches!(step, PlaceStep::Descendant(_)))
            && position < limit
            && let Some(ty) = types[position]
            && self.owns_anchor(ty, &mut Vec::new())
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
            resolved
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
                .map(|named| named.resolve(&self.places, true))
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
        let invariant = self.affine_invariants(ty).get(index)?;
        let image = self.sealed_image(anchor, ty)?;
        substitute_binder(&invariant.goal, &|projections, datum_ty| {
            let mut datum = image.clone();
            for projection in projections {
                datum = datum.with_projection(*projection, datum_ty)?;
            }
            Some(datum)
        })
    }

    /// [TYPE-11.S7] each anchor whose binder datum the read `expression`
    /// selects, with the invariant's position and the datum's projection.
    fn read_anchors(
        &self,
        expression: &CheckedExpression,
    ) -> Vec<(ResolvedPlace, usize, Vec<GoalProjection>)> {
        let places = self.sealed_places(expression);
        let [place] = places.as_slice() else {
            return Vec::new();
        };
        let types = self.sealed_prefix_types(place);
        let mut anchors = Vec::new();
        for depth in 0..place.path.len() {
            let Some(ty) = types[depth] else {
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
                            datum.clone(),
                        ));
                    }
                }
            }
        }
        anchors
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
                if may_be_at_or_below(&state.separations, debt, place) {
                    taken.push(debt.rebased(place.path.len(), onto));
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

    /// [TYPE-11.S2] the debts an atomic block's target starts with: every
    /// anchor strictly below its referent that the referent owns, at the
    /// unknown position for each element, and the cover of each place where
    /// an owned type recurs.
    fn owned_anchor_debts(
        &self,
        place: &mut ResolvedPlace,
        ty: CheckedType,
        ancestors: &mut Vec<CheckedType>,
        source: &crate::NodePath,
        debts: &mut Debts,
    ) {
        if ancestors.contains(&ty) {
            debts.add(Debt {
                anchor: place.clone(),
                invariant: None,
                events: vec![source.clone()],
                widened: false,
            });
            return;
        }
        if !place.path.is_empty() {
            for index in 0..self.affine_invariants(ty).len() {
                debts.add(Debt {
                    anchor: place.clone(),
                    invariant: Some(index),
                    events: vec![source.clone()],
                    widened: false,
                });
            }
        }
        let steps: Vec<(PlaceStep, CheckedType)> = match ty {
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
                            Some((PlaceStep::Field(u32::try_from(ordinal).ok()?), field.ty))
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
                                        PlaceStep::Payload {
                                            variant: variant.tag,
                                            field: u32::try_from(ordinal).ok()?,
                                        },
                                        field.ty,
                                    ))
                                })
                        })
                        .collect(),
                    Some(CheckedNominalKind::Box { referent, .. }) => {
                        vec![(PlaceStep::Deref, *referent)]
                    }
                    _ => Vec::new(),
                }
            }
            CheckedType::Array { .. }
            | CheckedType::Buffer { .. }
            | CheckedType::Window { .. }
            | CheckedType::Segments { .. } => ty
                .storage_element()
                .and_then(|element| self.context.elements.get(element.index()).copied())
                .map(|element| (PlaceStep::Index(CapturedValue::unknown()), element))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        ancestors.push(ty);
        for (step, below) in steps {
            if self.owns_anchor(below, &mut Vec::new()) {
                place.path.push(step);
                self.owned_anchor_debts(place, below, ancestors, source, debts);
                place.path.pop();
            }
        }
        ancestors.pop();
    }
}

impl Reasoning<'_, '_, '_> {
    /// [TYPE-11.S5] whether `debt` is restored in `state`: the instance of
    /// its invariant at its anchor is proved by [MSR-4]'s disposition of an
    /// [FN-8] goal. A cover and an anchor at `[?]` form no instance.
    fn sealed_debt_restored(&mut self, debt: &Debt, state: &ProofFlowState) -> bool {
        let Some(index) = debt.invariant else {
            return false;
        };
        let Some(goal) = self.input.sealed_instance(&debt.anchor, index) else {
            return false;
        };
        let (disposition, _, _) = self.call_goal_disposition(
            &ConcreteGoal::new(goal),
            ProofContext::new(&state.facts, &state.affine),
        );
        disposition == CallGoalDisposition::Discharged
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
    /// hand-off's own events and leaves the set. A debt left unrestored is a
    /// failure of `rule` at `site`; returns whether there was none.
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
        let unrestored = self.reasoning().unrestored_debts(state, |debt| {
            places
                .iter()
                .any(|place| may_be_at_or_below(&separations, debt, place))
        });
        let Some(first) = unrestored.first() else {
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
        let goal = first
            .invariant
            .and_then(|index| self.input.sealed_instance(&first.anchor, index));
        let anchor = self.input.render_anchor(&first.anchor);
        let rendered_goal = goal
            .as_ref()
            .map(|goal| self.input.render_concrete_goal(goal))
            .unwrap_or_else(|| format!("the type invariants of `{anchor}`"));
        self.output.sealing_failures.push(SealingFailure {
            rule,
            site: site.clone(),
            clause,
            anchor,
            goal: goal.map(ConcreteGoal::new),
            rendered_goal,
            events: first.events.clone(),
            widened: first.widened,
        });
        false
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
    /// parameter to it, which the call handed off.
    pub(super) fn seal_body_entry(&mut self, state: &mut ProofFlowState) {
        if !self.input.sealing {
            return;
        }
        let function = self.input.function;
        for parameter in &function.parameters {
            let Some(ty) = self
                .input
                .summary(parameter.binding)
                .and_then(|summary| summary.ty)
            else {
                continue;
            };
            let anchor = ResolvedPlace::binding(parameter.binding);
            for index in 0..self.input.affine_invariants(ty).len() {
                if let Some(goal) = self.input.sealed_instance(&anchor, index) {
                    self.establish_sealed_instance(&anchor, index, goal, state);
                }
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
        if !self.input.sealing || state.debts.is_empty() {
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
        if !self.input.sealing || state.debts.is_empty() {
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
        for (anchor, index, _) in self.input.read_anchors(expression) {
            if !self.sealed_anchor(&anchor, index, state) {
                continue;
            }
            let Some(goal) = self.input.sealed_instance(&anchor, index) else {
                continue;
            };
            self.establish_sealed_instance(&anchor, index, goal, state);
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
        if !self.input.sealing || self.input.read_place_path(value).is_some() {
            return;
        }
        for (anchor, index, datum) in self.input.read_anchors(value) {
            let Some(ty) = self
                .input
                .sealed_prefix_types(&anchor)
                .last()
                .copied()
                .flatten()
            else {
                continue;
            };
            let Some(invariant) = self.input.affine_invariants(ty).get(index) else {
                continue;
            };
            if invariant.data.len() != 1 || !self.sealed_anchor(&anchor, index, state) {
                continue;
            }
            let goal = substitute_binder(&invariant.goal, &|projections, datum_ty| {
                (projections == datum.as_slice())
                    .then(|| goal_binding_place(binding, Vec::new(), datum_ty))
            });
            if let Some(goal) = goal {
                self.establish_sealed_instance(&anchor, index, goal, state);
            }
        }
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
                .any(|debt| may_denote(&state.separations, debt, anchor, index))
    }

    fn establish_sealed_instance(
        &mut self,
        anchor: &ResolvedPlace,
        index: usize,
        goal: GoalExpression,
        state: &mut ProofFlowState,
    ) {
        let clause = self
            .input
            .sealed_prefix_types(anchor)
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.input.affine_invariants(ty).get(index))
            .map(|invariant| invariant.clause.clone());
        let event = self
            .vocabulary
            .proof_event(FlowEventKind::S18, clause.as_ref());
        self.judging()
            .establish_body_goal(goal, &mut state.facts, event);
    }

    /// [TYPE-11.S3] the widening row for a kill batch that writes or ends
    /// `binding`: each debt whose anchor reads it in an offset, or carries its
    /// current target identity, is offered for restoration first, and widened
    /// when it is not restored.
    pub(super) fn seal_binding_change(&mut self, state: &mut ProofFlowState, binding: BindingId) {
        if state.debts.is_empty() {
            return;
        }
        let unrestored = self.reasoning().unrestored_debts(state, |debt| {
            offset_reads(&debt.anchor, binding) || carried_by(&debt.anchor, binding).is_some()
        });
        for debt in unrestored {
            state.debts.add(widened(debt, binding));
        }
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
            self.seal_binding_change(state, *binding);
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
                self.seal_binding_change(&mut edge.state, binder);
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
    pub(super) fn seal_loop_header(&self, id: CheckedLoopId, state: &mut ProofFlowState) {
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
        if state.debts.is_empty() || self.input.places.is_reference(binding) {
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
        if state.debts.is_empty() || self.input.places.is_reference(binding) {
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
        if state.debts.is_empty() {
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
                self.seal_binding_change(state, place.binding);
                SealedSet::Rebinding
            }
            CheckedSetTarget::Place(place)
                if place.fields.is_empty() && place.mode == CheckedMode::Own =>
            {
                self.seal_binding_change(state, place.binding);
                SealedSet::Local {
                    binding: place.binding,
                    taken: self.sealed_continuation(value, place.binding, state),
                }
            }
            _ => {
                let places = self.input.sealed_places(value);
                let handed = !places.is_empty();
                self.judging()
                    .seal_hand_off(state, &places, SealingRule::Type11, node_path);
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
                for written in self.input.resolve_sealed(&place) {
                    self.input.seal_write(
                        &mut state.debts,
                        &state.separations,
                        &written,
                        written.path.len(),
                        node_path,
                    );
                    if sealed {
                        state.debts.remove_proved_at_or_below(&written);
                    }
                }
            }
        }
    }

    /// [TYPE-11.S3, TYPE-11.S4] one call: the hand-off of each argument, then
    /// the write row of each projected write, then a `swap`'s exchange.
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
        let offsets = substituted_offsets(callee, actual_captures);
        for (index, argument) in arguments.iter().enumerate() {
            let boundary_writes = formal_effects.and_then(|effects| {
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
            let transport = callee
                .and_then(|callee| callee.parameter_transports.get(index).copied())
                .unwrap_or_default();
            if transport == CallTransport::ReadOnlyReference || writes.is_empty() {
                continue;
            }
            for (referent, _) in self.input.argument_referents(argument) {
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
                        self.seal_binding_change(state, binding);
                    }
                    self.input.seal_write(
                        &mut state.debts,
                        &state.separations,
                        &written,
                        referent.path.len(),
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
    /// target moves onto the other, rebased; each that may be at or below one
    /// target without being proved there stays and is copied onto the other;
    /// when the two targets may be one place, each receives the union.
    fn seal_exchange(
        &self,
        first: &[ResolvedPlace],
        second: &[ResolvedPlace],
        state: &mut ProofFlowState,
    ) {
        let ([first], [second]) = (first, second) else {
            // A target with several resolved members is proved at none of
            // them: every debt there is copied and stays.
            let mut copies = Vec::new();
            for (from, onto) in [(first, second), (second, first)] {
                for place in onto {
                    copies.extend(self.input.taken_debts(state, from, place));
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
                if may_be_at_or_below(&state.separations, debt, from) {
                    moved.push((
                        !one_place && proved_at_or_below(debt, from),
                        debt.clone(),
                        debt.rebased(from.path.len(), onto),
                    ));
                }
            }
        }
        for (proved, original, _) in &moved {
            if *proved {
                state.debts.0.retain(|debt| !debt.same_key(original));
            }
        }
        for (_, _, rebased) in moved {
            state.debts.add(rebased);
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
                });
            }
        }
        self.input
            .owned_anchor_debts(&mut place, ty, &mut Vec::new(), node_path, &mut state.debts);
    }
}

/// What a `set` commit's sealing does after its own events.
pub(super) enum SealedSet {
    /// No struct in the program declares an affine invariant.
    Inert,
    /// A reference rebinding.
    Rebinding,
    /// A commit into a complete own local binding: a continuation, which
    /// takes nothing from a call result or a construction.
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

/// Whether `value` is a call result or a construction, which carry no debt.
fn sealed_value(value: &CheckedExpression) -> bool {
    matches!(
        value,
        CheckedExpression::UserCall { .. }
            | CheckedExpression::ConstructStruct { .. }
            | CheckedExpression::ConstructEnum { .. }
    )
}
