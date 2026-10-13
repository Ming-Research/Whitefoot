//! Ordered final-L0 traversal and its immutable image index. Support metadata
//! omits only candidates whose DIRECT routes cannot succeed; retained entries
//! still execute the original checked candidate and proof construction.

use super::*;
use std::cell::Ref;

impl Reasoning<'_, '_, '_> {
    /// Execute the original final-family order, omitting only established
    /// failures. Selected entries retain every checked formation and parent.
    pub(super) fn affine_l0_then_direct_proof(
        &mut self,
        target: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Option<Vec<DerivationId>> {
        query.direct_metric = Metric::DirectFinalL0;
        work::affine_final_family_start();
        let l0 = query.l0;
        l0.promote(query.closed, check);
        let (total, selected) = {
            let complete = l0.complete.borrow();
            let index = complete.as_ref().expect("promoted above");
            (
                index.entries.len(),
                self.select_final_l0(target, query, index),
            )
        };
        work::affine_add(Metric::FinalImagesTotal, total as u64);
        let selected = match selected {
            Some(selected) => selected,
            None => {
                work::affine_metric(Metric::FinalFilterFallbacks);
                (0..total).collect()
            }
        };
        work::affine_add(Metric::FinalImagesSelected, selected.len() as u64);
        let mut next_ordinal = 0;
        for ordinal in selected {
            // Omitted suffixes after the first successful proof did not save
            // work: the original traversal would never have reached them.
            work::affine_add(Metric::FinalImagesSkipped, (ordinal - next_ordinal) as u64);
            next_ordinal = ordinal + 1;
            work::affine_metric(Metric::FinalImagesVisited);
            let entry = l0
                .ordered_entry(ordinal, query.closed, check)
                .expect("selected ordinal belongs to the complete index");
            let Some(mut parents) =
                self.affine_candidate_residual_proof(target, &entry.inequality, query, check)
            else {
                continue;
            };
            let Some(parent) = query.closed.bound_proof(
                entry.left,
                entry.right,
                entry.bound,
                &mut self.vocabulary.derivations,
            ) else {
                continue;
            };
            parents.push(parent);
            parents.sort_unstable_by_key(|parent| parent.0);
            parents.dedup();
            return Some(parents);
        }
        work::affine_add(Metric::FinalImagesSkipped, (total - next_ordinal) as u64);
        work::affine_final_family_exhausted();
        None
    }

    fn select_final_l0(
        &mut self,
        target: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        index: &AffineL0Index,
    ) -> Option<Vec<usize>> {
        if target.terms().is_empty() || query.closed.contradictory() {
            return None;
        }
        self.prepare_affine_intervals(target.terms(), query).ok()?;
        let maximum = interval_maximum(
            target.terms(),
            |atom| nonempty_interval(query, atom),
            &mut AffineCheckState::new(),
        )
        .ok()??;
        // A failed DIRECT can be an overflowing intermediate sum even when
        // the mathematical target holds. Disjoint terms can restore that sum's
        // representability, so only an exact violating maximum licenses this
        // filter. All other targets retain the complete original traversal.
        if maximum <= target.upper() {
            return None;
        }

        if query.final_interval_exceptions.is_none() {
            let mut exceptions = Vec::new();
            for (ordinal, entry) in index.entries.iter().enumerate() {
                let inequality = &entry.inequality;
                let minimum = self
                    .prepare_affine_intervals(inequality.terms(), query)
                    .ok()
                    .and_then(|()| interval_minimum(inequality.terms(), query));
                if minimum.is_none_or(|minimum| minimum > inequality.upper()) {
                    exceptions.push(ordinal);
                }
            }
            query.final_interval_exceptions = Some(exceptions);
        }
        let postings = index
            .postings
            .get_or_init(|| AffineL0Postings::new(&index.entries));
        let mut selected = std::collections::BTreeSet::new();
        for coefficient in target.terms() {
            if let Some(ordinals) = postings.by_atom.get(&coefficient.term()) {
                selected.extend(ordinals.iter().copied());
            }
        }
        // If a disjoint R (raw or tightened) has an exact DIRECT residual Q,
        // Q has exactly T's coefficients on T's atoms, and Q's foreign support
        // equals R's. Support equality deliberately retains a coefficient-
        // independent superset, covering integer tightenings without deriving
        // any new factor or changing checked residual formation.
        let overlapping = selected.iter().copied().collect::<Vec<_>>();
        let mut supports = HashSet::new();
        for ordinal in overlapping {
            if let Some(support) = complementary_support(target, &index.entries[ordinal].inequality)
                && supports.insert(support.clone())
                && let Some(ordinals) = postings.by_support.get(support.as_slice())
            {
                selected.extend(ordinals.iter().copied());
            }
        }
        selected.extend(
            query
                .final_interval_exceptions
                .as_ref()
                .expect("prepared above")
                .iter()
                .copied(),
        );
        Some(selected.into_iter().collect())
    }
}

impl AffineL0Postings {
    fn new(entries: &[AffineL0Entry]) -> Self {
        let mut postings = Self::default();
        for (ordinal, entry) in entries.iter().enumerate() {
            let support = entry
                .inequality
                .terms()
                .iter()
                .map(|term| {
                    postings
                        .by_atom
                        .entry(term.term())
                        .or_default()
                        .push(ordinal);
                    term.term()
                })
                .collect::<Box<[_]>>();
            postings
                .by_support
                .entry(support)
                .or_default()
                .push(ordinal);
        }
        postings
    }
}

fn nonempty_interval(query: &AffineDirectQuery<'_>, atom: AffineTermId) -> Option<(i128, i128)> {
    let interval = query.intervals.get(&atom)?;
    (interval.minimum <= interval.maximum).then_some((interval.minimum, interval.maximum))
}

/// An exact minimum of independent integer intervals. Missing, empty or
/// overflowing endpoints retain the candidate; they never establish failure.
fn interval_minimum(
    coefficients: &[AffineCoefficient],
    query: &AffineDirectQuery<'_>,
) -> Option<i128> {
    coefficients.iter().try_fold(0_i128, |sum, coefficient| {
        let (minimum, maximum) = nonempty_interval(query, coefficient.term())?;
        let endpoint = if coefficient.coefficient() > 0 {
            minimum
        } else {
            maximum
        };
        sum.checked_add(coefficient.coefficient().checked_mul(endpoint)?)
    })
}

fn complementary_support(
    target: &AffineInequality,
    image: &AffineInequality,
) -> Option<Vec<AffineTermId>> {
    let mut next = 0;
    let mut foreign = Vec::new();
    for coefficient in image.terms() {
        if let Some(target) = target.terms().get(next) {
            if coefficient.term() > target.term() {
                return None;
            }
            if coefficient.term() == target.term() {
                if coefficient.coefficient() != target.coefficient() {
                    return None;
                }
                next += 1;
                continue;
            }
        }
        foreign.push(coefficient.term());
    }
    (next == target.terms().len()).then_some(foreign)
}

impl LazyAffineL0Index {
    pub(super) fn new(candidates: Vec<AffineL0Candidate>) -> Self {
        let mut by_image: WordHashMap<Vec<(AffineTermId, i128)>, Vec<usize>> =
            WordHashMap::default();
        for (ordinal, candidate) in candidates.iter().enumerate() {
            let key = candidate
                .value
                .terms()
                .iter()
                .map(|term| (term.term(), term.coefficient()))
                .collect::<Vec<_>>();
            by_image.entry(key).or_default().push(ordinal);
        }
        Self {
            candidates,
            by_image,
            exact: RefCell::default(),
            complete: RefCell::default(),
        }
    }

    /// Only pairs with left coefficients - right coefficients == requested
    /// can contribute. Grouping ignores constants, but the original checked
    /// constructor still decides representability and the shifted bound.
    /// Visit matching pairs in original left/right order, replacing strictly
    /// stronger bounds only, so endpoints and equal-bound ties are identical.
    pub(super) fn entry(
        &self,
        terms: &[AffineCoefficient],
        closed: &ClosedState,
        check: &mut AffineCheckState,
    ) -> Option<Ref<'_, AffineL0Entry>> {
        if self.complete.borrow().is_some() {
            return self.complete_entry(terms);
        }
        if let Ok(entry) = Ref::filter_map(self.exact.borrow(), |exact| exact.get(terms)) {
            return Ref::filter_map(entry, Option::as_ref).ok();
        }
        // Cached absence counts as one cold demand too. Promote before the
        // Nth scan, bounding the total lazy left scans by N * (N - 1).
        if self.exact.borrow().len() + 1 >= self.candidates.len() {
            work::affine_cold_demand(0);
            self.promote(closed, check);
            return self.complete_entry(terms);
        }
        work::affine_cold_demand(self.candidates.len());
        let mut selected: Option<AffineL0Entry> = None;
        let mut right_key = Vec::new();
        for left in &self.candidates {
            if !affine_l0_right_key(left.value.terms(), terms, &mut right_key) {
                continue;
            }
            let Some(rights) = self.by_image.get(&right_key) else {
                continue;
            };
            for &right in rights {
                let right = &self.candidates[right];
                let Some(bound) = closed.tight_bound(left.term, right.term) else {
                    continue;
                };
                let Ok(inequality) = work::affine_result(
                    AffineInequality::from_bounded_forms(&left.value, &right.value, bound, check),
                    Metric::OverflowIndex,
                ) else {
                    continue;
                };
                if selected
                    .as_ref()
                    .is_none_or(|old| inequality.upper() < old.inequality.upper())
                {
                    selected = Some(AffineL0Entry {
                        inequality,
                        left: left.term,
                        right: right.term,
                        bound,
                    });
                }
            }
        }
        // None is also memoized: repeating an absent DIRECT vector should
        // not repeat the candidate search on an unchanged state.
        self.exact.borrow_mut().insert(terms.into(), selected);
        self.exact_entry(terms)
    }

    fn exact_entry(&self, terms: &[AffineCoefficient]) -> Option<Ref<'_, AffineL0Entry>> {
        Ref::filter_map(self.exact.borrow(), |exact| {
            exact.get(terms).and_then(Option::as_ref)
        })
        .ok()
    }

    fn complete_entry(&self, terms: &[AffineCoefficient]) -> Option<Ref<'_, AffineL0Entry>> {
        Ref::filter_map(self.complete.borrow(), |complete| {
            let index = complete.as_ref()?;
            index
                .by_terms
                .get(terms)
                .map(|&ordinal| &index.entries[ordinal])
        })
        .ok()
    }

    /// Build once per residency in original pair order. Prior exact demands
    /// cannot seed this order: a vector's strongest witness may appear after
    /// its first occurrence, and equal bounds keep the earlier witness.
    fn promote(&self, closed: &ClosedState, check: &mut AffineCheckState) {
        if self.complete.borrow().is_some() {
            return;
        }
        let mut index = AffineL0Index::default();
        for left in &self.candidates {
            for right in &self.candidates {
                let Some(bound) = closed.tight_bound(left.term, right.term) else {
                    continue;
                };
                let Ok(inequality) = work::affine_result(
                    AffineInequality::from_bounded_forms(&left.value, &right.value, bound, check),
                    Metric::OverflowIndex,
                ) else {
                    continue;
                };
                let ordinal = *index
                    .by_terms
                    .entry(inequality.terms().into())
                    .or_insert(index.entries.len());
                let entry = AffineL0Entry {
                    inequality,
                    left: left.term,
                    right: right.term,
                    bound,
                };
                if ordinal == index.entries.len() {
                    index.entries.push(entry);
                } else if entry.inequality.upper() < index.entries[ordinal].inequality.upper() {
                    index.entries[ordinal] = entry;
                }
            }
        }
        *self.complete.borrow_mut() = Some(index);
        self.exact.borrow_mut().clear();
        work::affine_promotion();
    }

    /// Entering the final AUTO family promotes even if its first member
    /// succeeds. Enumeration and all residual lookups then borrow the complete
    /// map, including negative answers, without rescanning left candidates.
    pub(super) fn ordered_entry(
        &self,
        ordinal: usize,
        closed: &ClosedState,
        check: &mut AffineCheckState,
    ) -> Option<Ref<'_, AffineL0Entry>> {
        self.promote(closed, check);
        Ref::filter_map(self.complete.borrow(), |complete| {
            complete.as_ref()?.entries.get(ordinal)
        })
        .ok()
    }
}

/// Solve right = left - requested in coefficient space only. This is a
/// lookup key, not an affine derivation: checked_sub accepts MIN - MIN = 0,
/// whereas negating requested first would lose a representable right image.
/// An out-of-range result cannot equal any candidate's i128 coefficient.
/// The actual pair always passes the original affine constructor afterwards,
/// including its stricter intermediate-overflow and formation-limit checks.
fn affine_l0_right_key(
    left: &[AffineCoefficient],
    requested: &[AffineCoefficient],
    result: &mut Vec<(AffineTermId, i128)>,
) -> bool {
    result.clear();
    let (mut l, mut r) = (0, 0);
    while l < left.len() || r < requested.len() {
        let term = match (left.get(l), requested.get(r)) {
            (Some(a), Some(b)) => a.term().min(b.term()),
            (Some(a), None) => a.term(),
            (None, Some(b)) => b.term(),
            (None, None) => break,
        };
        let a = if left.get(l).is_some_and(|a| a.term() == term) {
            let coefficient = left[l].coefficient();
            l += 1;
            coefficient
        } else {
            0
        };
        let b = if requested.get(r).is_some_and(|b| b.term() == term) {
            let coefficient = requested[r].coefficient();
            r += 1;
            coefficient
        } else {
            0
        };
        let Some(coefficient) = a.checked_sub(b) else {
            return false;
        };
        if coefficient != 0 {
            result.push((term, coefficient));
        }
    }
    true
}
