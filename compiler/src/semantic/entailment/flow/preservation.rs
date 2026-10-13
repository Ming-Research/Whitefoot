//! Optional [ENT-5] preservation questions at the write that may kill support.

use super::*;

fn containing_place(place: &ResolvedPlace, depth: usize) -> ResolvedPlace {
    ResolvedPlace {
        atomic_aliases: place.atomic_aliases.clone(),
        root: place.root,
        path: place.path[..depth].to_vec(),
    }
    .term_identity()
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum SeparationQuestion {
    Indices(CapturedValue, CapturedValue),
    Ranges(CapturedRange, CapturedRange),
    Outside(CapturedValue, CapturedRange),
}

/// Captured relations from the edge plus answers valid only at this event.
/// Canonical support offsets can name current bindings, so their complete
/// identities must not enter the persistent capture-only ledger.
pub(super) struct WriteSeparations {
    inherited: SeparationLedger,
    indices: HashSet<(CapturedValue, CapturedValue)>,
    ranges: HashSet<(CapturedRange, CapturedRange)>,
    outside: HashSet<(CapturedValue, CapturedRange)>,
}

impl SeparationOracle for WriteSeparations {
    fn indices_distinct(&self, left: CapturedValue, right: CapturedValue) -> bool {
        self.indices.contains(&(left, right)) || self.inherited.indices_distinct(left, right)
    }

    fn ranges_disjoint(&self, left: CapturedRange, right: CapturedRange) -> bool {
        self.ranges.contains(&(left, right)) || self.inherited.ranges_disjoint(left, right)
    }

    fn index_outside_range(&self, index: CapturedValue, range: CapturedRange) -> bool {
        self.outside.contains(&(index, range)) || self.inherited.index_outside_range(index, range)
    }

    fn index_is_live(&self, _window: &ResolvedPlace, _index: CapturedValue) -> bool {
        false
    }

    fn range_within_length(&self, _window: &ResolvedPlace, _range: CapturedRange) -> bool {
        false
    }

    fn window_length_is_shared(&self, _window: &ResolvedPlace) -> bool {
        true
    }
}

impl Reasoning<'_, '_, '_> {
    fn fact_support_places(&self, facts: &FactState, places: &mut Vec<ResolvedPlace>) {
        let mut terms = facts
            .bounds
            .cells()
            .flat_map(|(left, right, _, _)| [left, right])
            .chain(facts.distinct.iter().flat_map(|(left, right, _)| [*left, *right]))
            .chain(facts.origins.values().flat_map(Relation::terms))
            .collect::<Vec<_>>();
        terms.sort_unstable();
        terms.dedup();
        let mut seen = HashSet::new();
        while let Some(term) = terms.pop() {
            if !seen.insert(term) {
                continue;
            }
            match self.vocabulary.terms.kind(term) {
                TermKind::Sum(left, right) => terms.extend([*left, *right]),
                TermKind::Place(place, _) | TermKind::Measure(_, place) => {
                    places.push(place.clone());
                }
                _ => {}
            }
        }
        let mut goals = facts.opaque.iter().map(|(goal, _)| *goal).collect::<Vec<_>>();
        goals.extend(facts.goal_origins.values().map(|origin| origin.goal));
        goals.extend(
            facts
                .opaque_sources
                .values()
                .flat_map(|views| views.keys().copied()),
        );
        goals.sort_unstable();
        goals.dedup();
        for goal in goals {
            for support in self.vocabulary.goals.support(goal) {
                places.push(self.input.resolve_goal_support(support).0);
            }
        }
    }

    /// Ask only pairs whose index/range steps have exactly the same resolved
    /// containing place. Deduplication is local to this event: failed answers
    /// and proofs about current binding values never escape its context.
    pub(super) fn event_write_separations(
        &mut self,
        state: &ProofFlowState,
        events: &[KillEvent],
        pending_results: &[results::ResultEvidence],
    ) -> WriteSeparations {
        let mut answers = WriteSeparations {
            inherited: state.separations.clone(),
            indices: HashSet::new(),
            ranges: HashSet::new(),
            outside: HashSet::new(),
        };
        let mut written = HashMap::<ResolvedPlace, Vec<PlaceStep>>::new();
        for event in events {
            let (KillEvent::Write { place, .. } | KillEvent::EntryImageHolderWrite { place, .. }) =
                event
            else {
                continue;
            };
            for place in self.input.places.resolve(place.root, &place.path) {
                for (depth, step) in place.path.iter().enumerate() {
                    if matches!(step, PlaceStep::Index(_) | PlaceStep::Range(_)) {
                        let base = containing_place(&place, depth);
                        written.entry(base).or_default().push(*step);
                    }
                }
            }
        }
        if written.is_empty() {
            return answers;
        }
        let mut places = Vec::new();
        self.fact_support_places(&state.facts, &mut places);
        for result in state.results.values().flatten().chain(pending_results) {
            self.fact_support_places(&result.facts, &mut places);
        }
        for term in state.affine.measure_atoms.borrow().keys() {
            if let TermKind::Place(place, _) | TermKind::Measure(_, place) =
                self.vocabulary.terms.kind(*term)
            {
                places.push(place.clone());
            }
        }
        places.extend(
            self.input
                .entry_images
                .iter()
                .zip(&state.entry_images)
                .filter(|(_, killed)| killed.is_none())
                .map(|(image, _)| image.place.clone()),
        );
        let mut pairs = Vec::new();
        for place in places {
            for place in self.input.places.resolve(place.root, &place.path) {
                for (depth, support) in place.path.iter().enumerate() {
                    if !matches!(support, PlaceStep::Index(_) | PlaceStep::Range(_)) {
                        continue;
                    }
                    let base = containing_place(&place, depth);
                    let Some(writes) = written.get(&base) else {
                        continue;
                    };
                    for write in writes {
                        let pair = match (*write, *support) {
                            (PlaceStep::Index(left), PlaceStep::Index(right)) => {
                                SeparationQuestion::Indices(left.min(right), left.max(right))
                            }
                            (PlaceStep::Range(left), PlaceStep::Range(right)) => {
                                SeparationQuestion::Ranges(left.min(right), left.max(right))
                            }
                            (PlaceStep::Index(index), PlaceStep::Range(range))
                            | (PlaceStep::Range(range), PlaceStep::Index(index)) => {
                                SeparationQuestion::Outside(index, range)
                            }
                            _ => unreachable!("only index and range positions are collected"),
                        };
                        pairs.push(pair);
                    }
                }
            }
        }
        pairs.sort_unstable();
        pairs.dedup();
        for pair in pairs {
            let proof = match pair {
                SeparationQuestion::Indices(left, right) => {
                    if left.provably_same(right)
                        || left.literals_distinct(right)
                        || answers.indices_distinct(left, right)
                    {
                        continue;
                    }
                    let proof = self.prove_index_separation(left, right, state);
                    if proof.is_some() {
                        answers.indices.insert((left, right));
                        answers.indices.insert((right, left));
                    }
                    proof
                }
                SeparationQuestion::Ranges(left, right) => {
                    if left.literally_disjoint(right) || answers.ranges_disjoint(left, right) {
                        continue;
                    }
                    let proof = self.prove_range_separation(left, right, &state.facts, &state.affine);
                    if proof.is_some() {
                        answers.ranges.insert((left, right));
                        answers.ranges.insert((right, left));
                    }
                    proof
                }
                SeparationQuestion::Outside(index, range) => {
                    if range.literally_excludes(index) || answers.index_outside_range(index, range) {
                        continue;
                    }
                    let proof = self.prove_index_outside_range(index, range, state);
                    if proof.is_some() {
                        answers.outside.insert((index, range));
                    }
                    proof
                }
            };
            if let Some(proof) = proof {
                let root = proof
                    .derivation
                    .expect("proved separation retains its derivation");
                let occurrence = self.vocabulary.fact_preservation_roots;
                self.vocabulary.fact_preservation_roots = occurrence
                    .checked_add(1)
                    .expect("fact-preservation roots exceed the u32 identity space");
                self.vocabulary
                    .derivations
                    .add_root(DerivationRootKind::FactPreservation { occurrence }, root);
            }
        }
        answers
    }
}
