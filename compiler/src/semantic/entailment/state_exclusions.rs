//! Compact integer selection facts [ENT-3.S17, ENT-4, ENT-5].
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Exclusion {
    pub(crate) expression: GoalId,
    pub(crate) values: Arc<[i128]>,
    pub(crate) views: Vec<ExclusionView>,
    pub(crate) proof: DerivationId,
}

/// One actual admission view. Canonical truth alone grants no L0 projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExclusionView {
    pub(crate) expression: GoalId,
    pub(crate) projection: Option<(TermId, i128)>,
    pub(crate) proof: DerivationId,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ExclusionConclusion {
    Set,
    Relation(Relation),
    Goal(GoalId, GoalSign),
    Contradiction,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ExclusionProof {
    pub(crate) projection: Option<(TermId, i128)>,
    pub(crate) expression: GoalId,
    pub(crate) values: Arc<[i128]>,
    pub(crate) conclusion: ExclusionConclusion,
    pub(crate) event: Option<FlowEventId>,
    pub(crate) parents: Vec<DerivationId>,
}

impl ExclusionProof {
    pub(super) fn tie_component(&self, index: usize) -> Option<u32> {
        [
            self.expression.0,
            self.event.map_or(u32::MAX, |event| event.0),
        ]
        .get(index)
        .copied()
        .or_else(|| {
            self.parents
                .get(index.checked_sub(2)?)
                .map(|parent| parent.0)
        })
    }
}

impl Exclusion {
    pub(crate) fn from_view(
        expression: GoalId,
        values: Arc<[i128]>,
        projection: Option<(TermId, i128)>,
        proof: DerivationId,
        goals: &GoalTable,
    ) -> Self {
        Self {
            expression: goals.canonical(expression),
            values,
            views: vec![ExclusionView {
                expression,
                projection,
                proof,
            }],
            proof,
        }
    }

    fn numeric_consequence(
        &self,
        view: &ExclusionView,
        conclusion: ExclusionConclusion,
        ledger: &mut DerivationLedger,
    ) -> DerivationId {
        ledger.intern(DerivationNode::IntegerExclusion {
            detail: Box::new(ExclusionProof {
                projection: view.projection,
                expression: view.expression,
                values: Arc::clone(&self.values),
                conclusion,
                event: None,
                parents: vec![view.proof],
            }),
        })
    }

    pub(crate) fn consequence(
        &self,
        conclusion: ExclusionConclusion,
        ledger: &mut DerivationLedger,
    ) -> DerivationId {
        ledger.intern(DerivationNode::IntegerExclusion {
            detail: Box::new(ExclusionProof {
                projection: None,
                expression: self.expression,
                values: Arc::clone(&self.values),
                conclusion,
                event: None,
                parents: vec![self.proof],
            }),
        })
    }

    fn contains(&self, value: i128) -> bool {
        self.values.binary_search(&value).is_ok()
    }
}

impl FactState {
    pub(crate) fn establish_exclusion(&mut self, exclusion: Exclusion) {
        self.forget_closed_view();
        if let Some(held) = self
            .exclusions
            .iter_mut()
            .find(|held| held.expression == exclusion.expression && held.values == exclusion.values)
        {
            for view in exclusion.views {
                if !held
                    .views
                    .iter()
                    .any(|current| current.expression == view.expression)
                {
                    held.views.push(view);
                }
            }
        } else {
            self.exclusions.push(exclusion);
        }
    }
}

impl ClosedState {
    pub(super) fn exclusion_membership(
        &self,
        left: TermId,
        right: TermId,
        difference: i128,
    ) -> Option<(&Exclusion, &ExclusionView)> {
        let (term, value) = if right == ZERO {
            (left, difference)
        } else if left == ZERO {
            (right, difference.checked_neg()?)
        } else {
            return None;
        };
        self.exclusions.iter().find_map(|fact| {
            fact.views
                .iter()
                .find(|view| {
                    view.projection.is_some_and(|(at, offset)| {
                        at == term
                            && value
                                .checked_add(offset)
                                .is_some_and(|value| fact.contains(value))
                    })
                })
                .map(|view| (fact, view))
        })
    }

    pub(super) fn exclusion_relation_proof(
        &self,
        left: TermId,
        right: TermId,
        difference: i128,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        let (fact, view) = self.exclusion_membership(left, right, difference)?;
        Some(fact.numeric_consequence(
            view,
            ExclusionConclusion::Relation(Relation::Distinct {
                left,
                right,
                difference,
            }),
            ledger,
        ))
    }

    pub(super) fn exclusion_goal(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
    ) -> Option<&Exclusion> {
        let (expression, constant, required) =
            goals.integer_comparisons.get(&goals.canonical(goal))?;
        if *required != sign {
            return None;
        }
        self.exclusions.iter().find(|fact| {
            goals.expression(fact.expression) == expression && fact.contains(*constant)
        })
    }

    pub(super) fn exclusion_goal_proof(
        &self,
        goal: GoalId,
        sign: GoalSign,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
    ) -> Option<DerivationId> {
        Some(
            self.exclusion_goal(goal, sign, goals)?
                .consequence(ExclusionConclusion::Goal(goal, sign), ledger),
        )
    }
}

/// Reuse ordinary closure, adding only queried endpoint consequences. Each
/// changing round strictly tightens a bound to pass an excluded endpoint.
/// The sets are finite; no budget or independent member choices are involved.
pub(super) fn close_with_exclusions(
    state: &FactState,
    terms: &TermTable,
    goals: &GoalTable,
    ledger: &mut DerivationLedger,
    excluded: Option<TermId>,
) -> ClosedState {
    if state.exclusions.is_empty() {
        return close_with_row_pruning::<true, false>(state, terms, goals, ledger, excluded);
    }
    let mut working = state.clone();
    working.forget_closed_view();
    loop {
        #[cfg(test)]
        tests::CLOSURE_ROUNDS.with(|rounds| rounds.set(rounds.get() + 1));
        let mut closed =
            close_with_row_pruning::<true, false>(&working, terms, goals, ledger, excluded);
        closed.exclusions = state
            .exclusions
            .iter()
            .cloned()
            .map(|mut fact| {
                fact.views.retain(|view| {
                    view.projection
                        .is_none_or(|(term, _)| Some(term) != excluded)
                });
                fact
            })
            .collect();
        if closed.all_derivable {
            return closed;
        }
        let mut changed = false;
        let projected_terms = closed
            .exclusions
            .iter()
            .flat_map(|fact| {
                fact.views
                    .iter()
                    .filter_map(|view| view.projection.map(|(term, _)| term))
            })
            .collect::<std::collections::BTreeSet<_>>();
        for term in projected_terms {
            if term == ZERO {
                if let Some((fact, view)) = closed.exclusion_membership(ZERO, ZERO, 0) {
                    closed.contradiction = Some(fact.numeric_consequence(
                        view,
                        ExclusionConclusion::Contradiction,
                        ledger,
                    ));
                    closed.all_derivable = true;
                    return closed;
                }
                continue;
            }
            for (left, right) in [(term, ZERO), (ZERO, term)] {
                let Some((initial_bound, initial_proof)) = closed.cell(left, right, ledger) else {
                    continue;
                };
                let (mut bound, mut proof) = (initial_bound, initial_proof);
                // Follow the whole consecutive run, including membership
                // supplied by different sets, before reclosing the matrix.
                // Each step retains ENT-4's ordinary strengthening proof.
                while let Some((fact, view)) = closed.exclusion_membership(left, right, bound) {
                    let Some(stronger) = bound.checked_sub(1) else {
                        break;
                    };
                    let distinct = fact.numeric_consequence(
                        view,
                        ExclusionConclusion::Relation(Relation::Distinct {
                            left,
                            right,
                            difference: bound,
                        }),
                        ledger,
                    );
                    proof = ledger.intern(DerivationNode::StrengthenedBound {
                        left,
                        right,
                        bound: stronger,
                        weak: proof,
                        distinct,
                    });
                    bound = stronger;
                }
                if bound < initial_bound {
                    working.establish_from_proof(
                        &Relation::Bound { left, right, bound },
                        proof,
                        ledger,
                    );
                    changed = true;
                }
            }
        }
        if changed {
            continue;
        }
        // Materialize only signs of goals already in the function's finite
        // universe; membership does not mint a goal for a set member.
        for goal in goals.ids() {
            for sign in [GoalSign::Positive, GoalSign::Negative] {
                if let Some(proof) = closed.exclusion_goal_proof(goal, sign, goals, ledger) {
                    let key = (goals.canonical(goal), sign);
                    closed.opaque.insert(key);
                    closed.opaque_proofs.insert(key, proof);
                }
            }
        }
        return close_goal_contradictions(closed, goals, ledger);
    }
}

pub(super) fn join_exclusions(
    states: &[FactState],
    closed: &[Rc<ClosedState>],
    contributing: &[usize],
    ledger: &mut DerivationLedger,
    event: FlowEventId,
) -> Vec<Exclusion> {
    let Some(first) = contributing.first() else {
        return Vec::new();
    };
    states[*first]
        .exclusions
        .iter()
        .filter_map(|fact| {
            let held = states
                .iter()
                .enumerate()
                .map(|(index, state)| {
                    if closed[index].contradictory() {
                        Some(None)
                    } else {
                        state
                            .exclusions
                            .iter()
                            .find(|held| {
                                held.expression == fact.expression && held.values == fact.values
                            })
                            .map(Some)
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            let parents = held
                .iter()
                .enumerate()
                .map(|(index, held)| {
                    held.map_or_else(
                        || {
                            closed[index]
                                .contradiction_proof()
                                .expect("contradictory predecessor")
                        },
                        |held| held.proof,
                    )
                })
                .collect();
            let proof = ledger.intern(DerivationNode::IntegerExclusion {
                detail: Box::new(ExclusionProof {
                    projection: None,
                    expression: fact.expression,
                    values: Arc::clone(&fact.values),
                    conclusion: ExclusionConclusion::Set,
                    event: Some(event),
                    parents,
                }),
            });
            let views = fact
                .views
                .iter()
                .filter_map(|view| {
                    let parents = held
                        .iter()
                        .enumerate()
                        .map(|(index, held)| match held {
                            None => closed[index].contradiction_proof(),
                            Some(held) => held
                                .views
                                .iter()
                                .find(|other| {
                                    other.expression == view.expression
                                        && other.projection == view.projection
                                })
                                .map(|other| other.proof),
                        })
                        .collect::<Option<Vec<_>>>()?;
                    let proof = ledger.intern(DerivationNode::IntegerExclusion {
                        detail: Box::new(ExclusionProof {
                            projection: view.projection,
                            expression: view.expression,
                            values: Arc::clone(&fact.values),
                            conclusion: ExclusionConclusion::Set,
                            event: Some(event),
                            parents,
                        }),
                    });
                    Some(ExclusionView {
                        proof,
                        ..view.clone()
                    })
                })
                .collect();
            Some(Exclusion {
                expression: fact.expression,
                values: Arc::clone(&fact.values),
                views,
                proof,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::goal::{GoalAdmission, GoalDatum};
    use crate::semantic::model::CheckedType;
    use crate::semantic::places::{PlaceRoot, ResolvedPlace};

    thread_local! {
        pub(super) static CLOSURE_ROUNDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn fixture(
        values: &[i128],
    ) -> (
        FactState,
        TermTable,
        GoalTable,
        DerivationLedger,
        TermId,
        GoalId,
    ) {
        typed_fixture(values, IntegerType::U64)
    }

    fn typed_fixture(
        values: &[i128],
        ty: IntegerType,
    ) -> (
        FactState,
        TermTable,
        GoalTable,
        DerivationLedger,
        TermId,
        GoalId,
    ) {
        let mut terms = TermTable::new();
        let term = terms.intern(TermKind::Place(
            ResolvedPlace {
                atomic_aliases: Vec::new(),
                root: PlaceRoot::Binding(BindingId(0)),
                path: Vec::new(),
            },
            ty,
        ));
        let mut goals = GoalTable::default();
        let expression = goals.intern(
            GoalExpression::Datum(GoalDatum::Place {
                root: BindingId(0),
                projections: Vec::new(),
                ty: CheckedType::Integer(ty),
                admission: GoalAdmission::default(),
            }),
            None,
            None,
            vec![GoalSupport {
                root: BindingId(0),
                projections: Vec::new(),
                measure: None,
            }],
        );
        let values: Arc<[i128]> = values.into();
        let mut ledger = DerivationLedger::default();
        let event = ledger.event(FlowEventKind::S17, None);
        let proof = ledger.intern(DerivationNode::IntegerExclusion {
            detail: Box::new(ExclusionProof {
                projection: Some((term, 0)),
                expression,
                values: Arc::clone(&values),
                conclusion: ExclusionConclusion::Set,
                event: Some(event),
                parents: Vec::new(),
            }),
        });
        let mut state = FactState::new();
        state.establish_exclusion(Exclusion::from_view(
            expression,
            values,
            Some((term, 0)),
            proof,
            &goals,
        ));
        (state, terms, goals, ledger, term, expression)
    }

    fn source_exclusion(
        state: &mut FactState,
        goals: &GoalTable,
        ledger: &mut DerivationLedger,
        term: TermId,
        expression: GoalId,
        values: &[i128],
    ) {
        let values: Arc<[i128]> = values.into();
        let event = ledger.event(FlowEventKind::S17, None);
        let proof = ledger.intern(DerivationNode::IntegerExclusion {
            detail: Box::new(ExclusionProof {
                projection: Some((term, 0)),
                expression,
                values: Arc::clone(&values),
                conclusion: ExclusionConclusion::Set,
                event: Some(event),
                parents: Vec::new(),
            }),
        });
        state.establish_exclusion(Exclusion::from_view(
            expression,
            values,
            Some((term, 0)),
            proof,
            goals,
        ));
    }

    #[test]
    fn signed_join_keeps_common_zero_disequality_from_different_exclusion_sets() {
        let (first, mut terms, goals, mut ledger, term, expression) =
            typed_fixture(&[0, 10], IntegerType::I8);
        let mut second = FactState::new();
        source_exclusion(&mut second, &goals, &mut ledger, term, expression, &[0, 20]);
        let relation = Relation::Distinct {
            left: term,
            right: ZERO,
            difference: 0,
        };
        for state in [&first, &second] {
            let closed = close(state, &terms, &goals, &mut ledger);
            assert!(closed.derives(&relation));
            assert_eq!(closed.tight_bound(term, ZERO), Some(127));
            assert_eq!(closed.tight_bound(ZERO, term), Some(128));
        }
        let event = ledger.event(FlowEventKind::Join, None);
        let joined = join_at(&[first, second], &terms, &goals, &mut ledger, event);
        assert!(joined.exclusions.is_empty());
        assert_eq!(joined.distinct.len(), 1);
        let closed = close(&joined, &terms, &goals, &mut ledger);
        assert!(closed.derives(&relation));
        assert!(closed.relation_proof(&relation, &mut ledger).is_some());

        // ENT-4 supplies membership at Z, not disequality substitution
        // through an arbitrary second term whose value happens to be fixed.
        let other = terms.intern(TermKind::Place(
            ResolvedPlace {
                atomic_aliases: Vec::new(),
                root: PlaceRoot::Binding(BindingId(1)),
                path: Vec::new(),
            },
            IntegerType::I8,
        ));
        let mut input = FactState::new();
        source_exclusion(&mut input, &goals, &mut ledger, term, expression, &[10]);
        let source = ledger.event(FlowEventKind::S1, None);
        input.establish(
            &Relation::Equal {
                left: other,
                right: ZERO,
                difference: 10,
            },
            &mut ledger,
            source,
        );
        assert!(
            !close(&input, &terms, &goals, &mut ledger).derives(&Relation::Distinct {
                left: term,
                right: other,
                difference: 0,
            })
        );
    }

    #[test]
    fn killing_an_exclusion_or_its_view_discards_cached_endpoint_strengthening() {
        for view_only in [false, true] {
            let (mut state, terms, mut goals, mut ledger, term, expression) =
                typed_fixture(&[255], IntegerType::U8);
            let killed = if view_only {
                let mut spelling = goals.expression(expression).clone();
                let GoalExpression::Datum(GoalDatum::Place { admission, .. }) = &mut spelling
                else {
                    unreachable!()
                };
                *admission = GoalAdmission::plain(CheckedType::Integer(IntegerType::U8));
                let view = goals.intern(spelling, None, None, goals.support(expression).to_vec());
                assert_ne!(expression, view);
                assert_eq!(goals.canonical(view), expression);
                state = FactState::new();
                source_exclusion(&mut state, &goals, &mut ledger, term, view, &[255]);
                view
            } else {
                expression
            };
            assert_eq!(
                close(&state, &terms, &goals, &mut ledger).tight_bound(term, ZERO),
                Some(254)
            );
            assert!(state.closed_view.borrow().is_some());
            state.kill_goals(|goal| goal == killed);
            assert!(state.closed_view.borrow().is_none());
            assert!(state.view_seed.is_none());
            assert_eq!(state.exclusions.len(), usize::from(view_only));
            assert_eq!(
                close(&state, &terms, &goals, &mut ledger).tight_bound(term, ZERO),
                Some(255)
            );
        }
    }

    #[test]
    fn consecutive_excluded_endpoints_close_once_after_the_complete_run() {
        for upper in [false, true] {
            let values = if upper {
                (200..=255).collect::<Vec<i128>>()
            } else {
                (0..=55).collect()
            };
            let (_, terms, goals, mut ledger, term, expression) =
                typed_fixture(&[], IntegerType::U8);
            // Alternate the members between sets: the complete run must be
            // followed even when its adjacent steps come from different sets.
            let mut state = FactState::new();
            for parity in [0, 1] {
                let subset = values
                    .iter()
                    .copied()
                    .filter(|value| value % 2 == parity)
                    .collect::<Vec<_>>();
                source_exclusion(&mut state, &goals, &mut ledger, term, expression, &subset);
            }
            let before = CLOSURE_ROUNDS.with(std::cell::Cell::get);
            let closed = close(&state, &terms, &goals, &mut ledger);
            assert_eq!(
                CLOSURE_ROUNDS.with(std::cell::Cell::get) - before,
                2,
                "one initial closure and one after all 56 endpoint steps"
            );
            let (left, right, bound) = if upper {
                (term, ZERO, 199)
            } else {
                (ZERO, term, -56)
            };
            assert_eq!(closed.tight_bound(left, right), Some(bound));
            assert!(!closed.contradictory());
            assert_eq!(state.exclusions.len(), 2);
            assert!(state.distinct.is_empty());
            assert_eq!(
                ledger
                    .nodes
                    .iter()
                    .filter(|node| matches!(node,
                DerivationNode::StrengthenedBound { left: a, right: b, .. }
                if *a == left && *b == right))
                    .count(),
                56
            );
        }
    }

    #[test]
    fn a_long_else_is_one_source_and_membership_never_expands_it() {
        let values = (1..=130).collect::<Vec<i128>>();
        let (mut state, terms, goals, mut ledger, term, _) = fixture(&values);
        assert_eq!(state.exclusions.len(), 1);
        assert!(state.distinct.is_empty());
        let closed = close(&state, &terms, &goals, &mut ledger);
        for value in values {
            assert!(closed.derives(&Relation::Distinct {
                left: term,
                right: ZERO,
                difference: value
            }));
        }
        assert!(!closed.derives(&Relation::Distinct {
            left: term,
            right: ZERO,
            difference: 131
        }));
        materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
        assert_eq!(state.exclusions.len(), 1);
        assert!(state.distinct.is_empty());
        assert_eq!(ledger.nodes.iter().filter(|node| matches!(node, DerivationNode::IntegerExclusion { detail } if detail.event.is_some_and(|event| ledger.events[event.0 as usize].kind == FlowEventKind::S17))).count(), 1);
    }

    #[test]
    fn exclusion_support_dies_at_a_write_and_cannot_reappear_from_cached_closure() {
        let (mut state, terms, goals, mut ledger, term, expression) = fixture(&[5]);
        let relation = Relation::Distinct {
            left: term,
            right: ZERO,
            difference: 5,
        };
        assert!(close(&state, &terms, &goals, &mut ledger).derives(&relation));
        materialize_closure_before_kill(&mut state, &terms, &goals, &mut ledger);
        state.kill(|candidate| candidate == term);
        state.kill_goals(|goal| goal == expression);
        assert!(state.exclusions.is_empty());
        assert!(!close(&state, &terms, &goals, &mut ledger).derives(&relation));
    }

    #[test]
    fn excluded_endpoints_reach_the_fixed_point_and_only_common_sets_survive_a_join() {
        let (mut state, terms, goals, mut ledger, term, _) = fixture(&[0, 1, 2, 4]);
        let event = ledger.event(FlowEventKind::S1, None);
        state.establish(
            &Relation::Bound {
                left: term,
                right: ZERO,
                bound: 4,
            },
            &mut ledger,
            event,
        );
        let closed = close(&state, &terms, &goals, &mut ledger);
        assert!(closed.derives(&Relation::Equal {
            left: term,
            right: ZERO,
            difference: 3
        }));
        let join_event = ledger.event(FlowEventKind::Join, None);
        let joined = join_at(
            &[state.clone(), state.clone()],
            &terms,
            &goals,
            &mut ledger,
            join_event,
        );
        assert_eq!(joined.exclusions.len(), 1);
        let joined = join_at(
            &[state, FactState::new()],
            &terms,
            &goals,
            &mut ledger,
            join_event,
        );
        assert!(joined.exclusions.is_empty());
        assert!(
            !close(&joined, &terms, &goals, &mut ledger).derives(&Relation::Distinct {
                left: term,
                right: ZERO,
                difference: 4
            })
        );
    }
    #[test]
    fn canonical_exclusion_survives_distinct_admission_views_without_a_numeric_view() {
        use crate::semantic::model::CheckedIntegerOperation;
        let (mut first, terms, mut goals, mut ledger, term, canonical) = fixture(&[5]);
        let mut other_expression = goals.expression(canonical).clone();
        let GoalExpression::Datum(GoalDatum::Place { admission, .. }) = &mut other_expression
        else {
            unreachable!()
        };
        *admission = GoalAdmission::plain(CheckedType::Integer(IntegerType::U64));
        let other = goals.intern(
            other_expression,
            None,
            None,
            goals.support(canonical).to_vec(),
        );
        assert_ne!(canonical, other);
        assert_eq!(goals.canonical(other), canonical);
        let values = Arc::clone(&first.exclusions[0].values);
        let event = ledger.event(FlowEventKind::S17, None);
        let proof = ledger.intern(DerivationNode::IntegerExclusion {
            detail: Box::new(ExclusionProof {
                projection: None,
                expression: other,
                values: Arc::clone(&values),
                conclusion: ExclusionConclusion::Set,
                event: Some(event),
                parents: Vec::new(),
            }),
        });
        let second_fact = Exclusion::from_view(other, values, None, proof, &goals);
        let mut second = FactState::new();
        second.establish_exclusion(second_fact.clone());
        let comparison = GoalExpression::Operation {
            row: GoalOperation::Integer {
                operation: CheckedIntegerOperation::NotEqual,
                operand_type: CheckedType::Integer(IntegerType::U64),
            },
            type_arguments: Vec::new(),
            const_arguments: Vec::new(),
            result: CheckedType::Bool,
            arguments: vec![
                goals.expression(canonical).clone(),
                GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer {
                    ty: IntegerType::U64,
                    bits: 5,
                })),
            ],
        };
        let goal = goals.intern(comparison, None, None, goals.support(canonical).to_vec());
        let exact_expression = goals.expression(canonical).clone();
        goals
            .integer_comparisons
            .insert(goal, (exact_expression, 5, GoalSign::Positive));
        let event = ledger.event(FlowEventKind::Join, None);
        let joined = join_at(&[first.clone(), second], &terms, &goals, &mut ledger, event);
        assert_eq!(
            joined.exclusions.len(),
            1,
            "exact identity ignores admission metadata"
        );
        assert!(
            joined.exclusions[0].views.is_empty(),
            "neither source view is common to both inputs"
        );
        let closed = close(&joined, &terms, &goals, &mut ledger);
        assert!(closed.derives_goal(goal, GoalSign::Positive, &goals));
        assert!(
            !closed.derives(&Relation::Distinct {
                left: term,
                right: ZERO,
                difference: 5
            }),
            "canonical truth does not grant the first input's numeric projection"
        );
        first.establish_exclusion(second_fact);
        assert_eq!(first.exclusions.len(), 1);
        assert_eq!(
            first.exclusions[0].views.len(),
            2,
            "coalescing exact truth retains each actual admission source"
        );
        first.kill_goals(|id| id == other);
        assert_eq!(
            first.exclusions[0].views.len(),
            1,
            "view support is killed independently"
        );
    }

    /// An exclusion decides a comparison registered after the closure was
    /// remembered. The old closure would answer the new goal at query time
    /// but never materialized its opaque sign and proof, which a later
    /// snapshot reads, so the remembered view must be recomputed.
    #[test]
    fn an_appended_goal_an_exclusion_decides_recomputes_the_remembered_view() {
        use crate::semantic::model::CheckedIntegerOperation;
        let (state, terms, mut goals, mut ledger, _term, expression) = fixture(&[5]);
        goals.mark_metadata_complete(expression);
        let first = close(&state, &terms, &goals, &mut ledger);
        let comparison = GoalExpression::Operation {
            row: GoalOperation::Integer {
                operation: CheckedIntegerOperation::NotEqual,
                operand_type: CheckedType::Integer(IntegerType::U64),
            },
            type_arguments: Vec::new(),
            const_arguments: Vec::new(),
            result: CheckedType::Bool,
            arguments: vec![
                goals.expression(expression).clone(),
                GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer {
                    ty: IntegerType::U64,
                    bits: 5,
                })),
            ],
        };
        let goal = goals.intern(comparison, None, None, goals.support(expression).to_vec());
        let exact_expression = goals.expression(expression).clone();
        goals
            .integer_comparisons
            .insert(goal, (exact_expression, 5, GoalSign::Positive));
        goals.mark_metadata_complete(goal);
        assert!(first.derives_goal(goal, GoalSign::Positive, &goals));
        let key = (goals.canonical(goal), GoalSign::Positive);
        assert!(!first.opaque.contains(&key));

        let second = close(&state, &terms, &goals, &mut ledger);
        assert!(!Rc::ptr_eq(&first, &second));
        let fresh = close_with_excluded_term(&state, &terms, &goals, &mut ledger, None);
        assert!(second.opaque.contains(&key));
        assert_eq!(
            second.opaque_proofs.get(&key),
            fresh.opaque_proofs.get(&key)
        );
        assert!(second.opaque_proofs.contains_key(&key));
    }
}
