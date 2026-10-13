//! Integer points that show the fixed affine search [MSR-4, ENT-6] has no
//! witness, so that it need not exhaust its families to find that out.
//!
//! Every `DIRECT` and `AUTO` step adds premises with positive integer factors
//! and divides by a common integer factor rounding down, so whatever it
//! derives holds at every integer point that satisfies all the premises it
//! reads. Those premises are, for one query view: the closed L0 image
//! `left - right <= bound` of every ordered candidate pair, the explicit
//! affine premises, and each atom's interval, which is its intrinsic range
//! narrowed by closed bounds of candidates whose image is that atom alone and
//! is therefore implied by the range and the pair images through `Z`. Step 6
//! also reads the bridge images to the goal's right term. A point satisfying
//! all of them at which the target is false is a counterexample: no step can
//! derive the target, and the ordered traversal would exhaust every family
//! and return no proof.
//!
//! The point is only ever used to skip that exhaustion. It proves nothing and
//! selects no witness, and a query for which no point is found runs the
//! unchanged traversal, so every disposition and retained derivation is the
//! traversal's. Every value is checked `i128` arithmetic; an overflow or an
//! atom outside the view finds no point. The search is a fixed monotone
//! repair, moving atoms only up from their minimums (the view's least point)
//! or only down from their maximums (its greatest point): its round limit
//! and the number of points kept decide only whether a point is found, never
//! a result, and a point is returned only after one complete round in which
//! every premise and the falsified target held without change.

use super::*;

/// Ordinary and origin-transport queries can alternate between two numeric
/// premise sets; two slots avoid rebuilding either while they remain resident.
const MODEL_VIEWS: usize = 2;

/// Points kept per view, the least and greatest included; a point found once
/// the view is full still serves the query that found it.
const MODEL_POINTS: usize = 10;

/// Repair rounds per search. Closed pair images need one round per change in
/// the common case; the limit decides only how long a point is sought.
const MODEL_ROUNDS: usize = 8;

/// The premises of one query view, in atom positions, with the points found.
pub(super) struct AffineModelCache {
    index: Rc<LazyAffineL0Index>,
    assumptions: Vec<AffineInequality>,
    /// `None` when the upward search from every atom's minimum finds no
    /// point, so no query in the view is refuted.
    view: Option<ModelView>,
}

struct ModelView {
    atoms: Vec<AffineTermId>,
    minimum: Vec<i128>,
    maximum: Vec<i128>,
    /// Each candidate image as positioned coefficients and its constant.
    forms: Vec<(Vec<(usize, i128)>, i128)>,
    /// Per atom position, the candidates whose image reads it.
    uses: Vec<Vec<(usize, i128)>>,
    /// `forms[left] - forms[right] <= bound` for every closed pair bound.
    pairs: Vec<(usize, usize, i128)>,
    /// Explicit affine premises, `sum <= upper`.
    facts: Vec<(Vec<(usize, i128)>, i128)>,
    /// The least point every view has, and the greatest when the downward
    /// search finds one; directed searches start from them.
    least: Vec<i128>,
    greatest: Option<Vec<i128>>,
    /// Every verified point kept, the least and greatest included.
    points: Vec<Vec<i128>>,
    /// The latest target falsified in this view, with its point and the
    /// values chosen for target atoms outside the view.
    refutation: Option<Refutation>,
}

struct Refutation {
    target: AffineInequality,
    point: Vec<i128>,
    extension: Vec<(AffineTermId, i128)>,
}

/// `sum(coefficient * atom) >= least`, the falsified target.
#[derive(Clone, Copy)]
struct Goal<'a> {
    terms: &'a [(usize, i128)],
    least: i128,
}

impl Reasoning<'_, '_, '_> {
    /// Whether an integer point satisfying every premise of this view
    /// falsifies `target`; when it does, `AUTO` over the view has no witness.
    pub(super) fn affine_model_refutes(
        &mut self,
        target: &AffineInequality,
        assumptions: &[ActiveAffineFact],
        closed: &Rc<ClosedState>,
        index: &Rc<LazyAffineL0Index>,
    ) -> bool {
        if closed.contradictory() {
            return false;
        }
        let slot = self.affine_model_slot(closed, index, assumptions);
        let atoms = &self.vocabulary.affine_atoms;
        let Some(view) = self.vocabulary.affine_models[slot].view.as_mut() else {
            super::super::work::affine_refutation(false);
            return false;
        };
        let refuted = view.refute(target, atoms);
        super::super::work::affine_refutation(refuted);
        refuted
    }

    /// Whether the point that falsified `target` in this view also satisfies
    /// every step-6 bridge image to `right`, so that every residual step 6
    /// would submit is false there too.
    pub(super) fn affine_bridges_refuted(
        &self,
        target: &AffineInequality,
        assumptions: &[ActiveAffineFact],
        closed: &Rc<ClosedState>,
        right: TermId,
        right_value: &AffineForm,
        candidates: &[AffineL0Candidate],
    ) -> bool {
        let Some(index) = self
            .vocabulary
            .affine_l0_cache
            .as_ref()
            .map(|cache| &cache.index)
        else {
            return false;
        };
        let Some(refutation) = self
            .vocabulary
            .affine_models
            .iter()
            .find(|cache| cache.matches(closed, index, assumptions))
            .and_then(|cache| cache.view.as_ref())
            .and_then(|view| {
                view.refutation
                    .as_ref()
                    .filter(|refutation| refutation.target == *target)
                    .map(|refutation| (view, refutation))
            })
        else {
            return false;
        };
        let (view, refutation) = refutation;
        let holds = view
            .bridges_hold(refutation, closed, right, right_value, candidates)
            .is_some();
        if holds {
            super::super::work::affine_bridge_refutation();
        }
        holds
    }

    fn affine_model_slot(
        &mut self,
        closed: &Rc<ClosedState>,
        index: &Rc<LazyAffineL0Index>,
        assumptions: &[ActiveAffineFact],
    ) -> usize {
        let models = &mut self.vocabulary.affine_models;
        if let Some(slot) = models
            .iter()
            .position(|cache| cache.matches(closed, index, assumptions))
        {
            return slot;
        }
        super::super::work::affine_model_view();
        let timed = super::super::work::timed(super::super::work::Phase::ModelViewBuild);
        let view = ModelView::new(
            &index.candidates,
            closed,
            assumptions,
            &self.vocabulary.affine_atoms,
        );
        drop(timed);
        let models = &mut self.vocabulary.affine_models;
        if models.len() == MODEL_VIEWS {
            models.remove(0);
        }
        models.push(AffineModelCache {
            index: Rc::clone(index),
            assumptions: assumptions
                .iter()
                .map(|fact| fact.inequality.clone())
                .collect(),
            view,
        });
        models.len() - 1
    }
}

impl AffineModelCache {
    /// The latest target a point falsified in this view.
    #[cfg(test)]
    pub(super) fn refuted_target(&self) -> Option<&AffineInequality> {
        let view = self.view.as_ref()?;
        view.refutation
            .as_ref()
            .map(|refutation| &refutation.target)
    }

    /// An index residency fixes the retained L0 content and ordered value
    /// images (`AffineL0Content`), so its identity and the explicit numerical
    /// premises fix every model constraint. Signed-only origin additions may
    /// change the closed-state allocation but cannot change a satisfying
    /// point. Models retain no derivations; a refutation only skips a search
    /// whose exhaustive result is already unproved. Atom rollbacks still
    /// clear every model, preventing reuse with changed intrinsic ranges.
    fn matches(
        &self,
        closed: &Rc<ClosedState>,
        index: &Rc<LazyAffineL0Index>,
        assumptions: &[ActiveAffineFact],
    ) -> bool {
        !closed.contradictory()
            && Rc::ptr_eq(&self.index, index)
            && self.assumptions.len() == assumptions.len()
            && self
                .assumptions
                .iter()
                .zip(assumptions)
                .all(|(kept, fact)| *kept == fact.inequality)
    }
}

impl ModelView {
    fn new(
        candidates: &[AffineL0Candidate],
        closed: &ClosedState,
        assumptions: &[ActiveAffineFact],
        table: &[AffineAtom],
    ) -> Option<Self> {
        let mut atoms = candidates
            .iter()
            .flat_map(|candidate| candidate.value.terms())
            .chain(assumptions.iter().flat_map(|fact| fact.inequality.terms()))
            .map(|coefficient| coefficient.term())
            .collect::<Vec<_>>();
        atoms.sort_unstable();
        atoms.dedup();
        let mut minimum = Vec::with_capacity(atoms.len());
        let mut maximum = Vec::with_capacity(atoms.len());
        for atom in &atoms {
            let atom = table.get(atom.index() as usize)?;
            if atom.minimum > atom.maximum {
                return None;
            }
            minimum.push(atom.minimum);
            maximum.push(atom.maximum);
        }
        let position = |terms: &[AffineCoefficient]| {
            terms
                .iter()
                .map(|coefficient| {
                    let at = atoms.binary_search(&coefficient.term()).ok()?;
                    Some((at, coefficient.coefficient()))
                })
                .collect::<Option<Vec<_>>>()
        };
        let forms = candidates
            .iter()
            .map(|candidate| {
                Some((
                    position(candidate.value.terms())?,
                    candidate.value.constant_value(),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let facts = assumptions
            .iter()
            .map(|fact| Some((position(fact.inequality.terms())?, fact.inequality.upper())))
            .collect::<Option<Vec<_>>>()?;
        let mut uses = vec![Vec::new(); atoms.len()];
        for (candidate, (terms, _)) in forms.iter().enumerate() {
            for &(at, coefficient) in terms {
                uses[at].push((candidate, coefficient));
            }
        }
        let mut pairs = Vec::new();
        for (left, left_candidate) in candidates.iter().enumerate() {
            for (right, right_candidate) in candidates.iter().enumerate() {
                if let Some(bound) = closed.tight_bound(left_candidate.term, right_candidate.term) {
                    pairs.push((left, right, bound));
                }
            }
        }
        let mut view = Self {
            atoms,
            minimum,
            maximum,
            forms,
            uses,
            pairs,
            facts,
            least: Vec::new(),
            greatest: None,
            points: Vec::new(),
            refutation: None,
        };
        view.least = view.search(view.minimum.clone(), Direction::Up, None)?;
        view.greatest = view.search(view.maximum.clone(), Direction::Down, None);
        view.points.push(view.least.clone());
        view.points.extend(view.greatest.clone());
        Some(view)
    }

    /// Falsifies `target` at a kept point or at a new one found up from the
    /// least point or down from the greatest, recording it for step 6.
    fn refute(&mut self, target: &AffineInequality, table: &[AffineAtom]) -> bool {
        // Atoms no premise reads are bounded by their range alone; each takes
        // the endpoint that makes the target's left side largest.
        let mut inside = Vec::new();
        let mut extension = Vec::new();
        let mut outside = 0_i128;
        for coefficient in target.terms() {
            let (term, factor) = (coefficient.term(), coefficient.coefficient());
            if let Ok(at) = self.atoms.binary_search(&term) {
                inside.push((at, factor));
                continue;
            }
            let Some(atom) = table.get(term.index() as usize) else {
                return false;
            };
            if atom.minimum > atom.maximum {
                return false;
            }
            let value = if factor > 0 {
                atom.maximum
            } else {
                atom.minimum
            };
            let Some(sum) = factor
                .checked_mul(value)
                .and_then(|part| outside.checked_add(part))
            else {
                return false;
            };
            outside = sum;
            extension.push((term, value));
        }
        let Some(least) = target
            .upper()
            .checked_add(1)
            .and_then(|least| least.checked_sub(outside))
        else {
            return false;
        };
        let goal = Goal {
            terms: &inside,
            least,
        };
        let found = self
            .points
            .iter()
            .find(|point| goal.holds(point))
            .cloned()
            .or_else(|| {
                let point = self
                    .search(self.least.clone(), Direction::Up, Some(goal))
                    .or_else(|| {
                        let greatest = self.greatest.clone()?;
                        self.search(greatest, Direction::Down, Some(goal))
                    })?;
                if self.points.len() < MODEL_POINTS {
                    self.points.push(point.clone());
                }
                Some(point)
            });
        let Some(point) = found else {
            return false;
        };
        self.refutation = Some(Refutation {
            target: target.clone(),
            point,
            extension,
        });
        true
    }

    /// One form's value at a refutation's point, or `None` when it reads an
    /// atom the point does not fix.
    fn value_at(&self, form: &AffineForm, refutation: &Refutation) -> Option<i128> {
        let mut value = form.constant_value();
        for coefficient in form.terms() {
            let atom = match self.atoms.binary_search(&coefficient.term()) {
                Ok(at) => refutation.point[at],
                Err(_) => {
                    refutation
                        .extension
                        .iter()
                        .find(|(term, _)| *term == coefficient.term())?
                        .1
                }
            };
            value = value.checked_add(coefficient.coefficient().checked_mul(atom)?)?;
        }
        Some(value)
    }

    /// Whether every closed bridge image `candidate - right <= bound` holds
    /// at the refutation's point.
    fn bridges_hold(
        &self,
        refutation: &Refutation,
        closed: &ClosedState,
        right: TermId,
        right_value: &AffineForm,
        candidates: &[AffineL0Candidate],
    ) -> Option<()> {
        let right_value = self.value_at(right_value, refutation)?;
        for candidate in candidates {
            let Some(bound) = closed.tight_bound(candidate.term, right) else {
                continue;
            };
            let left = self.value_at(&candidate.value, refutation)?;
            if left.checked_sub(right_value)? > bound {
                return None;
            }
        }
        Some(())
    }

    /// Moves atoms from `start` in one direction until every pair image,
    /// every explicit premise and `goal` hold, and returns that point. A
    /// violated premise moves one atom whose move lowers its left side,
    /// preferring one whose move does not lower the goal's sum; an unmet goal
    /// moves one atom whose move raises that sum. Atoms only rise from the
    /// least point and only fall from the greatest, each stopping at its
    /// range. The point is returned only after a whole round changed nothing.
    fn search(
        &self,
        start: Vec<i128>,
        direction: Direction,
        goal: Option<Goal<'_>>,
    ) -> Option<Vec<i128>> {
        let mut point = start;
        let mut values = self
            .forms
            .iter()
            .map(|(terms, constant)| sum_at(terms, *constant, &point))
            .collect::<Option<Vec<_>>>()?;
        for _ in 0..MODEL_ROUNDS {
            let mut changed = false;
            if let Some(goal) = goal {
                let reached = sum_at(goal.terms, 0, &point)?;
                if reached < goal.least {
                    let (at, rate) = goal.terms.iter().find_map(|&(at, factor)| {
                        let rate = direction.rate(factor)?;
                        (rate > 0 && self.movable(&point, at, direction)).then_some((at, rate))
                    })?;
                    let deficit = goal.least.checked_sub(reached)?;
                    self.shift(&mut point, &mut values, at, deficit, rate, direction)?;
                    changed = true;
                }
            }
            for &(left, right, bound) in &self.pairs {
                let excess = values[left]
                    .checked_sub(values[right])?
                    .checked_sub(bound)?;
                if excess > 0 {
                    let lowering = difference(&self.forms[left].0, &self.forms[right].0);
                    let (at, rate) = self.lowering_atom(&lowering, &point, direction, goal)?;
                    self.shift(&mut point, &mut values, at, excess, rate, direction)?;
                    changed = true;
                }
            }
            for (terms, upper) in &self.facts {
                let excess = sum_at(terms, 0, &point)?.checked_sub(*upper)?;
                if excess > 0 {
                    let (at, rate) = self.lowering_atom(terms, &point, direction, goal)?;
                    self.shift(&mut point, &mut values, at, excess, rate, direction)?;
                    changed = true;
                }
            }
            if !changed {
                return Some(point);
            }
        }
        None
    }

    /// Whether an atom can still move in `direction` within its range.
    fn movable(&self, point: &[i128], at: usize, direction: Direction) -> bool {
        match direction {
            Direction::Up => point[at] < self.maximum[at],
            Direction::Down => point[at] > self.minimum[at],
        }
    }

    /// The atom to move for a violated `sum <= upper`: one whose move in
    /// `direction` lowers the sum and that can still move, one whose move
    /// keeps the goal's sum first, with the rate its move lowers the sum.
    fn lowering_atom(
        &self,
        terms: &[(usize, i128)],
        point: &[i128],
        direction: Direction,
        goal: Option<Goal<'_>>,
    ) -> Option<(usize, i128)> {
        let lowers_goal = |at: usize| {
            goal.is_some_and(|goal| {
                goal.terms.iter().any(|&(goal_at, factor)| {
                    goal_at == at && direction.rate(factor).is_some_and(|rate| rate < 0)
                })
            })
        };
        let mut eligible = terms.iter().filter_map(|&(at, factor)| {
            let rate = direction.rate(factor)?.checked_neg()?;
            (rate > 0 && self.movable(point, at, direction)).then_some((at, rate))
        });
        let first = eligible.clone().find(|&(at, _)| !lowers_goal(at));
        first.or_else(|| eligible.next())
    }

    /// Moves one atom by `ceil(excess / rate)` in `direction`, stopping at
    /// its range.
    fn shift(
        &self,
        point: &mut [i128],
        values: &mut [i128],
        at: usize,
        excess: i128,
        rate: i128,
        direction: Direction,
    ) -> Option<()> {
        let mut step = excess / rate;
        if excess % rate != 0 {
            step = step.checked_add(1)?;
        }
        let moved = match direction {
            Direction::Up => point[at]
                .checked_add(step)
                .map_or(self.maximum[at], |moved| moved.min(self.maximum[at])),
            Direction::Down => point[at]
                .checked_sub(step)
                .map_or(self.minimum[at], |moved| moved.max(self.minimum[at])),
        };
        let delta = moved.checked_sub(point[at])?;
        for &(candidate, coefficient) in &self.uses[at] {
            values[candidate] = values[candidate].checked_add(coefficient.checked_mul(delta)?)?;
        }
        point[at] = moved;
        Some(())
    }
}

/// Which way a search moves atoms: up from the least point or down from the
/// greatest.
#[derive(Clone, Copy)]
enum Direction {
    Up,
    Down,
}

impl Direction {
    /// How much one unit move of an atom with this coefficient raises a sum.
    fn rate(self, coefficient: i128) -> Option<i128> {
        match self {
            Self::Up => Some(coefficient),
            Self::Down => coefficient.checked_neg(),
        }
    }
}

impl Goal<'_> {
    fn holds(self, point: &[i128]) -> bool {
        sum_at(self.terms, 0, point).is_some_and(|reached| reached >= self.least)
    }
}

fn sum_at(terms: &[(usize, i128)], constant: i128, point: &[i128]) -> Option<i128> {
    let mut value = constant;
    for &(at, coefficient) in terms {
        value = value.checked_add(coefficient.checked_mul(point[at])?)?;
    }
    Some(value)
}

/// `left - right` over positions, both sorted by position.
fn difference(left: &[(usize, i128)], right: &[(usize, i128)]) -> Vec<(usize, i128)> {
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut l, mut r) = (0, 0);
    while l < left.len() || r < right.len() {
        match (left.get(l), right.get(r)) {
            (Some(&(la, lc)), Some(&(ra, rc))) if la == ra => {
                if let Some(coefficient) = lc.checked_sub(rc).filter(|value| *value != 0) {
                    merged.push((la, coefficient));
                }
                l += 1;
                r += 1;
            }
            (Some(&(la, lc)), Some(&(ra, _))) if la < ra => {
                merged.push((la, lc));
                l += 1;
            }
            (Some(&(la, lc)), None) => {
                merged.push((la, lc));
                l += 1;
            }
            (_, Some(&(ra, rc))) => {
                if let Some(coefficient) = rc.checked_neg() {
                    merged.push((ra, coefficient));
                }
                r += 1;
            }
            (None, None) => break,
        }
    }
    merged
}
