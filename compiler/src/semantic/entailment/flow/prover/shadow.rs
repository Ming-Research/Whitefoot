//! Bounded, read-only countermodel observation. This module never prepares a
//! semantic view, promotes an index, constructs a proof, or changes AUTO work.
//! Unknown includes stale/incomplete snapshots, arithmetic overflow and either
//! observer cap. All such outcomes leave the real traversal untouched.

use super::*;

const GROUP_OPERATIONS: u64 = 100_000;

#[derive(Default)]
struct Budget {
    used: u64,
    stopped: bool,
}

impl Budget {
    fn charge(&mut self, amount: usize) -> Option<()> {
        if self.stopped {
            return None;
        }
        let amount = u64::try_from(amount).ok()?;
        let cutoff = if amount > GROUP_OPERATIONS.saturating_sub(self.used) {
            Some(Metric::ShadowGroupCutoffs)
        } else if !work::shadow_charge(amount) {
            Some(Metric::ShadowFunctionCutoffs)
        } else {
            None
        };
        if let Some(metric) = cutoff {
            self.stopped = true;
            work::affine_metric(metric);
            return None;
        }
        self.used += amount;
        Some(())
    }
}

struct Stamp {
    terms: usize,
    goals: usize,
    atoms: Vec<AffineAtom>,
    measures: Vec<(TermId, AffineForm)>,
    closed: Rc<ClosedState>,
    l0: Rc<LazyAffineL0Index>,
}

impl Stamp {
    fn capture(
        vocabulary: &Vocabulary,
        auto: &AffineAutoQuery<'_>,
        target: &AffineInequality,
        budget: &mut Budget,
    ) -> Option<Self> {
        budget.charge(1 + target.terms().len())?;
        let prepared = auto.prepared.as_ref()?;
        if !prepared.failed.contains(target) || prepared.l0.complete.borrow().is_none() {
            return None;
        }
        budget.charge(vocabulary.affine_atoms.len())?;
        let atoms = vocabulary.affine_atoms.clone();
        let images = auto.values.measure_atoms.borrow();
        // Charge a conservative comparison allowance before sorting keys. The
        // subsequent coefficient copies are charged before each allocation.
        let allowance = images
            .len()
            .checked_mul(usize::try_from(images.len().max(1).ilog2()).ok()? + 2)?;
        budget.charge(allowance)?;
        let mut keys = images.keys().copied().collect::<Vec<_>>();
        keys.sort_unstable();
        let mut measures = Vec::new();
        for term in keys {
            let image = &images[&term];
            budget.charge(1 + image.terms().len())?;
            measures.push((term, image.clone()));
        }
        Some(Self {
            terms: vocabulary.terms.revision(),
            goals: vocabulary.goals.revision(),
            atoms,
            measures,
            closed: Rc::clone(&prepared.closed),
            l0: Rc::clone(&prepared.l0),
        })
    }

    fn matches(
        &self,
        vocabulary: &Vocabulary,
        auto: &AffineAutoQuery<'_>,
        closed: &Rc<ClosedState>,
        budget: &mut Budget,
    ) -> Option<()> {
        budget.charge(1)?;
        let prepared = auto.prepared.as_ref()?;
        if self.terms != vocabulary.terms.revision()
            || self.goals != vocabulary.goals.revision()
            || !Rc::ptr_eq(&self.closed, closed)
            || !Rc::ptr_eq(&self.closed, &prepared.closed)
            || !Rc::ptr_eq(&self.l0, &prepared.l0)
            || self.atoms.len() != vocabulary.affine_atoms.len()
        {
            return None;
        }
        budget.charge(self.atoms.len())?;
        if self.atoms != vocabulary.affine_atoms {
            return None;
        }
        let images = auto.values.measure_atoms.borrow();
        if images.len() != self.measures.len() {
            return None;
        }
        for (term, image) in &self.measures {
            budget.charge(1 + image.terms().len())?;
            if images.get(term) != Some(image) {
                return None;
            }
        }
        Some(())
    }
}

/// One observer accompanies the original Step-6 traversal. Certification is
/// reported only at its end, after all actually submitted bridges and view
/// identities have been checked. Counters for covered calls include reuse
/// hits; covered exhaustions count actual final-family exhaustion only.
pub(super) struct Group {
    budget: Budget,
    stamp: Option<Stamp>,
    models: Vec<Vec<i128>>,
    unknown: bool,
    uncertain_assignment: bool,
    calls: u64,
    exhaustions: u64,
}

impl Group {
    pub(super) fn start(
        vocabulary: &Vocabulary,
        auto: &AffineAutoQuery<'_>,
        target: &AffineInequality,
    ) -> Option<Self> {
        if !work::affine_observing() {
            return None;
        }
        if !auto
            .prepared
            .as_ref()
            .is_some_and(|prepared| prepared.failed.contains(target))
        {
            return None;
        }
        work::affine_metric(Metric::ShadowGroups);
        let mut budget = Budget::default();
        let stamp = Stamp::capture(vocabulary, auto, target, &mut budget);
        let unknown = stamp.is_none();
        Some(Self {
            budget,
            stamp,
            models: Vec::new(),
            unknown,
            uncertain_assignment: false,
            calls: 0,
            exhaustions: work::affine_exhaustions(),
        })
    }

    /// Called after the real right image, bridge inventory and closure exist.
    /// It reads the already complete index and reconstructs endpoint values in
    /// detached storage, never adding an endpoint to the real query cache.
    pub(super) fn prepare(
        &mut self,
        reasoning: &Reasoning<'_, '_, '_>,
        auto: &AffineAutoQuery<'_>,
        target: &AffineInequality,
        closed: &Rc<ClosedState>,
    ) {
        if self
            .prepare_inner(reasoning, auto, target, closed)
            .is_none()
        {
            self.unknown = true;
        }
    }

    fn prepare_inner(
        &mut self,
        reasoning: &Reasoning<'_, '_, '_>,
        auto: &AffineAutoQuery<'_>,
        target: &AffineInequality,
        closed: &Rc<ClosedState>,
    ) -> Option<()> {
        let stamp = self.stamp.as_ref()?;
        stamp.matches(reasoning.vocabulary, auto, closed, &mut self.budget)?;
        if closed.contradictory() {
            return None;
        }
        let prepared = auto.prepared.as_ref()?;
        let measures = prepared.measures.as_ref()?;
        self.budget.charge(stamp.atoms.len())?;
        let mut intervals = stamp
            .atoms
            .iter()
            .map(|atom| (atom.minimum, atom.maximum))
            .collect::<Vec<_>>();
        // DIRECT's measure names come from its existing atom association map.
        // In particular, an imaged sum is not itself an interval name.
        for (index, interval) in intervals.iter_mut().enumerate() {
            self.budget.charge(1)?;
            let atom = AffineTermId::from_index(u32::try_from(index).ok()?);
            if let Some(terms) = measures.get(&atom) {
                for &term in terms {
                    strengthen(interval, term, closed, &mut self.budget)?;
                }
            }
        }
        // Binding-place candidates are already interned by ordinary query
        // preparation. The same type/image conditions as DIRECT select them.
        for candidate in &stamp.l0.candidates {
            self.budget.charge(1)?;
            let Some(atom) = candidate.value.unit_term() else {
                continue;
            };
            let TermKind::Place(place, ty) = reasoning.vocabulary.terms.kind(candidate.term) else {
                continue;
            };
            let PlaceRoot::Binding(binding) = place.root else {
                continue;
            };
            if !place.path.is_empty()
                || !place.atomic_aliases.is_empty()
                || reasoning.input.affine_binding_type(binding) != Some(*ty)
                || stamp.atoms.get(atom.index() as usize)?.ty != *ty
                || auto
                    .values
                    .values
                    .get(&binding)
                    .and_then(AffineForm::unit_term)
                    != Some(atom)
            {
                continue;
            }
            strengthen(
                intervals.get_mut(atom.index() as usize)?,
                candidate.term,
                closed,
                &mut self.budget,
            )?;
        }
        let complete = stamp.l0.complete.borrow();
        let complete = complete.as_ref()?;
        // numeric_affine_proof borrowed this canonical, first-occurrence list
        // from affine_facts. No premise normalization or proof construction is
        // performed by the observer.
        for choice in 0..3 {
            work::affine_metric(Metric::ShadowAssignments);
            self.budget.charge(intervals.len())?;
            let mut model = Vec::with_capacity(intervals.len());
            for &(minimum, maximum) in &intervals {
                if minimum > maximum {
                    return None;
                }
                model.push(match choice {
                    0 => 0_i128.clamp(minimum, maximum),
                    1 => minimum,
                    _ => maximum,
                });
            }
            let valid = (|| {
                for premise in auto.assumptions {
                    if !satisfies(&premise.inequality, &model, &mut self.budget)? {
                        return Some(false);
                    }
                }
                for entry in &complete.entries {
                    if !satisfies(&entry.inequality, &model, &mut self.budget)? {
                        return Some(false);
                    }
                }
                satisfies(target, &model, &mut self.budget).map(|holds| !holds)
            })();
            match valid {
                Some(true) => self.models.push(model),
                Some(false) => {}
                None if self.budget.stopped => return None,
                None => self.uncertain_assignment = true,
            }
        }
        Some(())
    }

    pub(super) fn bridge(&mut self, image: &AffineInequality, residual: &AffineInequality) {
        self.calls = self.calls.saturating_add(1);
        if self.unknown {
            return;
        }
        let mut index = 0;
        while index < self.models.len() {
            let valid = satisfies(image, &self.models[index], &mut self.budget).and_then(|holds| {
                if !holds {
                    return Some(false);
                }
                satisfies(residual, &self.models[index], &mut self.budget).map(|holds| !holds)
            });
            match valid {
                Some(true) => index += 1,
                Some(false) => {
                    self.models.remove(index);
                }
                None if self.budget.stopped => {
                    self.unknown = true;
                    return;
                }
                None => {
                    self.uncertain_assignment = true;
                    self.models.remove(index);
                }
            }
        }
    }

    pub(super) fn observe_view(
        &mut self,
        vocabulary: &Vocabulary,
        auto: &AffineAutoQuery<'_>,
        closed: &Rc<ClosedState>,
    ) {
        if !self.unknown
            && self
                .stamp
                .as_ref()
                .and_then(|stamp| stamp.matches(vocabulary, auto, closed, &mut self.budget))
                .is_none()
        {
            self.unknown = true;
        }
    }

    pub(super) fn unavailable(&mut self) {
        self.unknown = true;
    }

    pub(super) fn finish(self) {
        if self.unknown || (self.models.is_empty() && self.uncertain_assignment) {
            work::affine_metric(Metric::ShadowUnknown);
        } else if self.models.is_empty() {
            work::affine_metric(Metric::ShadowRejected);
        } else {
            work::affine_metric(Metric::ShadowWouldCertify);
            work::affine_add(Metric::ShadowBridgeCalls, self.calls);
            work::affine_add(
                Metric::ShadowFinalExhaustions,
                work::affine_exhaustions().saturating_sub(self.exhaustions),
            );
        }
    }
}

fn strengthen(
    interval: &mut (i128, i128),
    term: TermId,
    closed: &ClosedState,
    budget: &mut Budget,
) -> Option<()> {
    budget.charge(2)?;
    if let Some(upper) = closed.tight_bound(term, ZERO) {
        interval.1 = interval.1.min(upper);
    }
    if let Some(negative_lower) = closed.tight_bound(ZERO, term) {
        budget.charge(1)?;
        // Match DIRECT: an unrepresentable negative endpoint supplies no
        // stronger lower bound, so the type bound remains available.
        if let Some(lower) = negative_lower.checked_neg() {
            interval.0 = interval.0.max(lower);
        }
    }
    Some(())
}

fn satisfies(inequality: &AffineInequality, model: &[i128], budget: &mut Budget) -> Option<bool> {
    budget.charge(1)?;
    let mut value = 0_i128;
    for coefficient in inequality.terms() {
        // One atom lookup and checked multiply/add. Failure is unknown, never
        // a countermodel and never a result returned to the actual prover.
        budget.charge(3)?;
        let atom = *model.get(coefficient.term().index() as usize)?;
        value = value.checked_add(coefficient.coefficient().checked_mul(atom)?)?;
    }
    Some(value <= inequality.upper())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(scope: &work::AffineTestScope, metric: &str) -> u64 {
        super::super::tests::affine_metric_value(&scope.rows(), metric)
    }

    #[test]
    fn observer_arithmetic_overflow_and_both_caps_are_unknown() {
        let scope = work::AffineTestScope::new();
        let target = AffineInequality::from_terms(
            &[(AffineTermId::from_index(0), i128::MAX)],
            i128::MAX,
            &mut AffineCheckState::new(),
        )
        .unwrap();
        let mut budget = Budget::default();
        assert_eq!(satisfies(&target, &[2], &mut budget), None);
        assert_eq!(count(&scope, "affine_shadow_operations"), 4);
        assert_eq!(budget.charge(GROUP_OPERATIONS as usize - 4), Some(()));
        assert_eq!(budget.charge(1), None);
        assert_eq!(budget.charge(1), None, "the cutoff is counted once");
        assert_eq!(count(&scope, "affine_shadow_group_cutoffs"), 1);
        for _ in 1..20 {
            assert_eq!(
                Budget::default().charge(GROUP_OPERATIONS as usize),
                Some(())
            );
        }
        assert_eq!(Budget::default().charge(1), None);
        assert_eq!(count(&scope, "affine_shadow_function_cutoffs"), 1);
        assert_eq!(
            count(&scope, "affine_shadow_operations"),
            work::SHADOW_FUNCTION_OPERATIONS
        );
    }
}
