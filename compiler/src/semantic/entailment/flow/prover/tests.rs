//! Internal proof-route and aggregate-evidence checks. Source verdicts live
//! in conformance; these tests isolate finite normalization from the later
//! affine-result fallback so that bypassing the shared dispatcher fails.

use super::*;

fn with_analyzer(check: impl FnOnce(&mut Analyzer<'_, '_>)) {
    with_integer_parameters(&[], check);
}

fn with_integer_parameters(types: &[IntegerType], check: impl FnOnce(&mut Analyzer<'_, '_>)) {
    let constant_ids = HashMap::new();
    let const_parameter_types = HashMap::new();
    let context = EntailmentContext {
        declarations: &[],
        callees: &[],
        constants: &[],
        constant_ids: &constant_ids,
        const_parameter_types: &const_parameter_types,
        copy_type_parameters: &[],
        nominals: &[],
        elements: &[],
        contract_queries: &[],
        verified_postconditions: &[],
        verified_postcondition_proofs: &[],
        binding_names: &[],
    };
    let function = CheckedFunction {
        formal_hypothesis: false,
        summary_source: None,
        prelude_element: None,
        id: crate::semantic::model::FunctionId(0),
        declaration: crate::DeclarationId::from_index(0).unwrap(),
        module: crate::ModuleId::BUNDLE_ROOT,
        name: String::new(),
        symbol: String::new(),
        function_actuals: Vec::new(),
        region_parameters: Vec::new(),
        parameters: types
            .iter()
            .enumerate()
            .map(|(index, ty)| crate::semantic::model::CheckedParameter {
                name: format!("p{index}"),
                declaration: crate::DeclarationId::from_index(index).unwrap(),
                node_path: crate::NodePath { components: vec![] },
                binding: BindingId(u32::try_from(index).unwrap()),
                mode: CheckedMode::Own,
                ty: CheckedType::Integer(*ty),
                range_element: None,
            })
            .collect(),
        result_mode: CheckedMode::Own,
        result: CheckedType::Unit,
        declared_state_writes: Vec::new(),
        declared_state_reads: Vec::new(),
        requirements: Vec::new(),
        requirement_places: Vec::new(),
        postconditions: Vec::new(),
        range_facts: Default::default(),
        body: None,
        reference_origins: Vec::new(),
        body_disposition: Default::default(),
        allocates: false,
        call_separations: Vec::new(),
        permission_separation_queries: Vec::new(),
        waiting: crate::semantic::model::CheckedWaiting::default(),
        obligations: Vec::new(),
        entailment: FunctionEntailment::default(),
    };
    let mut analyzer = Analyzer::new(&context, &function);
    analyzer.input.collect_bindings();
    check(&mut analyzer);
}

#[test]
fn measure_term_forms_pages_only_for_paged() {
    for (measured, has_pages) in [
        (MeasuredKind::RuntimeRing, false),
        (MeasuredKind::RuntimeSlots, false),
        (MeasuredKind::Paged, true),
    ] {
        with_analyzer(|analyzer| {
            let path = ResolvedPlace::binding(BindingId(0));
            analyzer
                .reasoning()
                .measure_term(CheckedMeasure::Length, path.clone(), measured, None);
            for measure in [
                CheckedMeasure::Length,
                CheckedMeasure::Capacity,
                CheckedMeasure::Head,
                CheckedMeasure::Pages,
            ] {
                assert_eq!(
                    analyzer
                        .vocabulary
                        .terms
                        .interned(&TermKind::Measure(measure, path.clone()))
                        .is_some(),
                    measure != CheckedMeasure::Pages || has_pages,
                    "{measured:?}: {measure:?}",
                );
            }
        });
    }
}

/// The oracle is the unchanged full pair enumeration, called without the
/// memo. Compare both ordered witnesses and every exact-vector boundary
/// query, so losing even an image that another AUTO route can recover fails.
fn assert_affine_index_matches_rebuild(
    analyzer: &mut Analyzer<'_, '_>,
    context: ProofContext<'_>,
) -> (Rc<ClosedState>, Rc<LazyAffineL0Index>) {
    let candidates = analyzer.reasoning().affine_l0_candidates(context.affine);
    let closed = context.close(
        &analyzer.vocabulary.terms,
        &analyzer.vocabulary.goals,
        &mut analyzer.vocabulary.derivations,
    );
    let full = affine_l0_index(&candidates, &closed, &mut AffineCheckState::new());
    assert_affine_promotion_matches_rebuild(&candidates, &closed, &full);
    let (actual_closed, actual) = analyzer.reasoning().affine_query_view(context);
    assert!(Rc::ptr_eq(&closed, &actual_closed));
    // Demand exact vectors in reverse order first. The final family's order
    // must still be the full builder's first-occurrence order, not demand order.
    let mut check = AffineCheckState::new();
    for entry in full.entries.iter().rev() {
        assert_eq!(
            actual
                .entry(entry.inequality.terms(), &closed, &mut check)
                .as_deref(),
            Some(entry)
        );
    }
    let mut entries = Vec::new();
    while let Some(entry) = actual.ordered_entry(entries.len(), &closed, &mut check) {
        entries.push(entry.clone());
    }
    assert_eq!(entries, full.entries);
    let by_terms: WordHashMap<Box<[AffineCoefficient]>, usize> = entries
        .iter()
        .enumerate()
        .map(|(ordinal, entry)| (entry.inequality.terms().into(), ordinal))
        .collect();
    assert_eq!(by_terms, full.by_terms);
    let oracle = full_affine_query_index(candidates, &full);
    let (_, repeated) = analyzer.reasoning().affine_query_view(context);
    assert!(
        Rc::ptr_eq(&actual, &repeated),
        "unchanged queries must reuse"
    );
    for entry in &full.entries {
        let coefficients = entry
            .inequality
            .terms()
            .iter()
            .map(|term| (term.term(), term.coefficient()))
            .collect::<Vec<_>>();
        for delta in [-1, 0, 1] {
            let Some(upper) = entry.inequality.upper().checked_add(delta) else {
                continue;
            };
            let target =
                AffineInequality::from_terms(&coefficients, upper, &mut AffineCheckState::new())
                    .unwrap();
            let expected = analyzer
                .vocabulary
                .affine_l0_proof(&target, &oracle, &closed, &mut check);
            let observed = analyzer
                .vocabulary
                .affine_l0_proof(&target, &actual, &closed, &mut check);
            assert_eq!(observed, expected, "{target:?}");
            assert_eq!(observed.unwrap().is_some(), delta >= 0, "{target:?}");
        }
    }
    (closed, actual)
}

/// Each trigger must produce the eager map, even when exact requests visit
/// vectors in a different order, repeat a cached absence, or ask for a new
/// absent residual after promotion. Run this over the same alias, overflow,
/// state-change and inventory fixtures as the memo differential above.
fn assert_affine_promotion_matches_rebuild(
    candidates: &[AffineL0Candidate],
    closed: &ClosedState,
    full: &AffineL0Index,
) {
    let mut check = AffineCheckState::new();
    let absent = AffineForm::term(AffineTermId::from_index(u32::MAX));
    assert!(candidates.iter().all(|candidate| {
        candidate
            .value
            .terms()
            .iter()
            .all(|term| term.term() != absent.unit_term().unwrap())
    }));
    let mut demands = full
        .entries
        .iter()
        .map(|entry| entry.inequality.terms().to_vec())
        .collect::<Vec<_>>();
    // Ensure at least N unique requests, including when aliases leave fewer
    // than N present vectors. Two more guarantee new absences after promotion.
    for factor in 1..=candidates.len() + 2 {
        demands.push(
            absent
                .scale(factor as i128, &mut check)
                .unwrap()
                .terms()
                .to_vec(),
        );
    }
    let forward = (0..demands.len()).collect::<Vec<_>>();
    let reverse = forward.iter().copied().rev().collect::<Vec<_>>();
    let mut interleaved = Vec::new();
    for first in 0..demands.len().div_ceil(2) {
        interleaved.push(first);
        let last = demands.len() - 1 - first;
        if first != last {
            interleaved.push(last);
        }
    }
    for order in [&forward, &reverse, &interleaved] {
        for family_trigger in [false, true] {
            let index = LazyAffineL0Index::new(candidates.to_vec());
            let prefix = if family_trigger {
                candidates.len().saturating_sub(1).min(2)
            } else {
                0
            };
            for (position, &demand) in order.iter().enumerate() {
                if family_trigger && position == prefix {
                    assert!(index.complete.borrow().is_none());
                    assert_eq!(
                        index.ordered_entry(0, closed, &mut check).as_deref(),
                        full.entries.first()
                    );
                    assert!(index.complete.borrow().is_some(), "family entry promotes");
                }
                let terms = &demands[demand];
                let expected = full
                    .by_terms
                    .get(terms.as_slice())
                    .map(|&i| &full.entries[i]);
                assert_eq!(index.entry(terms, closed, &mut check).as_deref(), expected);
                let work = check.used();
                assert_eq!(index.entry(terms, closed, &mut check).as_deref(), expected);
                assert_eq!(check.used(), work, "warm hits must not rebuild witnesses");
                let promoted = if family_trigger {
                    position >= prefix
                } else {
                    position + 1 >= candidates.len()
                };
                assert_eq!(index.complete.borrow().is_some(), promoted);
                if promoted {
                    assert!(
                        index.exact.borrow().is_empty(),
                        "complete misses stay map lookups"
                    );
                } else {
                    assert_eq!(index.exact.borrow().len(), position + 1);
                }
            }
            let complete = index.complete.borrow();
            let complete = complete.as_ref().unwrap();
            assert_eq!(complete.entries, full.entries);
            assert_eq!(complete.by_terms, full.by_terms);
            // Repeated family walks and residual lookups may coexist with a
            // borrowed entry and must not rebuild or mutate the complete map.
            let target = AffineInequality::from_bounded_forms(
                &absent,
                &AffineForm::constant(0),
                0,
                &mut check,
            )
            .unwrap();
            let work = check.used();
            for _ in 0..2 {
                for (ordinal, expected) in full.entries.iter().enumerate() {
                    let entry = index.ordered_entry(ordinal, closed, &mut check).unwrap();
                    assert_eq!(&*entry, expected);
                    let exact = index
                        .entry(entry.inequality.terms(), closed, &mut check)
                        .unwrap();
                    assert!(std::ptr::eq(&*entry, &*exact), "warm entries are borrowed");
                    assert!(index.entry(absent.terms(), closed, &mut check).is_none());
                    // A residual containing the fresh atom cannot be an L0
                    // image. Overflowed residuals keep their ordinary skip.
                    if let Ok(residual) = AffineInequality::residual_after(
                        &target,
                        &entry.inequality,
                        &mut AffineCheckState::new(),
                    ) {
                        assert!(!full.by_terms.contains_key(residual.terms()));
                        assert!(index.entry(residual.terms(), closed, &mut check).is_none());
                        assert!(index.exact.borrow().is_empty());
                    }
                }
                assert!(
                    index
                        .ordered_entry(full.entries.len(), closed, &mut check)
                        .is_none()
                );
            }
            assert_eq!(check.used(), work, "promotion happens only once");
        }
    }
}

/// Inject the original full rebuild into the same proof-family traversal.
/// All lookups (including absent vectors) use that complete result; no lazy
/// pair search is allowed to repair a missing oracle entry.
fn full_affine_query_index(
    candidates: Vec<AffineL0Candidate>,
    full: &AffineL0Index,
) -> LazyAffineL0Index {
    LazyAffineL0Index {
        candidates,
        by_image: WordHashMap::default(),
        exact: RefCell::default(),
        complete: RefCell::new(Some(AffineL0Index {
            entries: full.entries.clone(),
            by_terms: full.by_terms.clone(),
        })),
    }
}

fn cache_measure(analyzer: &mut Analyzer<'_, '_>, binding: u32) -> TermId {
    analyzer.vocabulary.terms.intern(TermKind::Measure(
        CheckedMeasure::Length,
        ResolvedPlace::binding(BindingId(binding)),
    ))
}

fn cache_bound(
    analyzer: &mut Analyzer<'_, '_>,
    facts: &mut FactState,
    left: TermId,
    right: TermId,
    bound: i128,
) {
    let event = analyzer
        .vocabulary
        .derivations
        .event(FlowEventKind::S1, None);
    facts.establish(
        &Relation::Bound { left, right, bound },
        &mut analyzer.vocabulary.derivations,
        event,
    );
}

#[test]
fn affine_index_cache_matches_full_rebuild_for_ordered_images() {
    with_integer_parameters(&[IntegerType::I32; 3], |analyzer| {
        let mut affine = AffineFlowState::default();
        let [a, b] = [0, 1].map(|_| analyzer.vocabulary.new_affine_atom(IntegerType::I32));
        let mut check = AffineCheckState::new();
        let shifted = a.add(&AffineForm::constant(5), &mut check).unwrap();
        let mixed = a
            .scale(-2, &mut check)
            .unwrap()
            .add(&b, &mut check)
            .unwrap();
        // Insert out of source order; candidate order must remain term order
        // then binding order, including aliases and a multi-atom image.
        for (binding, value) in [(2, mixed), (0, a.clone()), (1, shifted)] {
            affine.values.insert(BindingId(binding), value);
        }
        let measure = cache_measure(analyzer, 10);
        affine.measure_atoms.borrow_mut().insert(measure, b.clone());
        let candidates = analyzer.reasoning().affine_l0_candidates(&affine);
        assert_eq!(candidates.len(), 5);
        assert_eq!(candidates[1].term, measure);
        let [x, alias] = [candidates[2].term, candidates[3].term];
        let mut facts = FactState::new();
        cache_bound(analyzer, &mut facts, x, ZERO, 9);
        cache_bound(analyzer, &mut facts, alias, ZERO, 11);
        let (closed, index) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        let selected = index.entry(a.terms(), &closed, &mut check).unwrap();
        assert_eq!(selected.inequality.upper(), 6);
        assert_eq!(
            selected.left, alias,
            "later strictly stronger image replaces the first"
        );
        cache_bound(analyzer, &mut facts, x, ZERO, 6);
        let (closed, tied) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert_eq!(
            tied.entry(a.terms(), &closed, &mut check).unwrap().left,
            x,
            "first equal image wins"
        );
        assert!(!Rc::ptr_eq(&index, &tied));

        // A coefficient negation overflows for some ordered pairs. Other
        // representable pairs, including the independent b image, survive.
        affine
            .values
            .insert(BindingId(0), a.scale(i128::MIN, &mut check).unwrap());
        let (closed, overflow) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert!(overflow.entry(b.terms(), &closed, &mut check).is_some());
        // A constant difference of MIN cannot be negated; another shifted
        // alias of the same coefficient vector must still be considered.
        affine.values.insert(
            BindingId(0),
            a.add(&AffineForm::constant(i128::MIN), &mut check).unwrap(),
        );
        assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
    });
}

#[test]
fn affine_index_cache_rebuilds_after_facts_kills_and_joins() {
    with_analyzer(|analyzer| {
        let affine = AffineFlowState::default();
        let [x, y] = [0, 1].map(|binding| cache_measure(analyzer, binding));
        let mut facts = FactState::new();
        cache_bound(analyzer, &mut facts, x, y, 3);
        let (_, first) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        let mut fork = facts.clone();
        let (_, shared) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&fork, &affine));
        assert!(Rc::ptr_eq(&first, &shared));
        cache_bound(analyzer, &mut fork, x, y, 1);
        let (closed, stronger) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&fork, &affine));
        assert_eq!(closed.tight_bound(x, y), Some(1));
        assert!(!Rc::ptr_eq(&first, &stronger));
        materialize_closure_before_kill(
            &mut fork,
            &analyzer.vocabulary.terms,
            &analyzer.vocabulary.goals,
            &mut analyzer.vocabulary.derivations,
        );
        let (_, snapshot) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&fork, &affine));
        assert!(!Rc::ptr_eq(&stronger, &snapshot));
        fork.kill(|term| term == y);
        let (closed, killed) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&fork, &affine));
        assert!(!closed.derives_bound(x, y, 3));
        assert!(!Rc::ptr_eq(&snapshot, &killed));
        // The sibling keeps the old relation; a cache keyed by the address
        // of a reused local state or by its candidate count gets this wrong.
        let (closed, _) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert_eq!(closed.tight_bound(x, y), Some(3));
        let event = analyzer
            .vocabulary
            .derivations
            .event(FlowEventKind::Join, None);
        let joined = join_at(
            &[facts, fork],
            &analyzer.vocabulary.terms,
            &analyzer.vocabulary.goals,
            &mut analyzer.vocabulary.derivations,
            event,
        );
        let (closed, _) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&joined, &affine));
        assert!(!closed.derives_bound(x, y, 3));
    });
}

#[test]
fn affine_index_cache_keys_complete_current_images_even_with_a_closed_context() {
    with_integer_parameters(&[IntegerType::I32; 2], |analyzer| {
        let facts = FactState::new();
        let mut affine = AffineFlowState::default();
        let measure = cache_measure(analyzer, 10);
        let a = analyzer.vocabulary.new_affine_atom(IntegerType::I32);
        let b = analyzer.vocabulary.new_affine_atom(IntegerType::I32);
        affine.values.insert(BindingId(0), a.clone());
        analyzer.vocabulary.terms.intern(TermKind::Place(
            ResolvedPlace::binding(BindingId(1)),
            IntegerType::I32,
        ));
        let _ = analyzer.reasoning().affine_l0_candidates(&affine);
        let closed = ProofClosure::new(
            &facts,
            &analyzer.vocabulary.terms,
            &analyzer.vocabulary.goals,
            &mut analyzer.vocabulary.derivations,
        );
        let mut previous = None;
        let mut check = AffineCheckState::new();
        for image in [
            a.clone(),
            a.add(&AffineForm::constant(7), &mut check).unwrap(),
            a.scale(2, &mut check).unwrap(),
            b,
        ] {
            affine.values.insert(BindingId(0), image);
            let context = ProofContext {
                facts: &facts,
                affine: &affine,
                closed: Some(&closed),
                origin_view: OriginView::Pending,
            };
            let (view, index) = assert_affine_index_matches_rebuild(analyzer, context);
            assert!(
                Rc::ptr_eq(&view, &closed.state),
                "only value images changed"
            );
            if let Some(old) = previous.replace(index.clone()) {
                assert!(!Rc::ptr_eq(&old, &index));
            }
        }
        // A measure kill can replace its atom with unchanged L0 facts and
        // term/goal revisions. The candidate list, not those revisions,
        // distinguishes this current value from the one at the old point.
        affine.measure_atoms.borrow_mut().remove(&measure);
        let (view, reminted) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert!(Rc::ptr_eq(&view, &closed.state));
        assert!(!Rc::ptr_eq(previous.as_ref().unwrap(), &reminted));
        let image = affine.values.remove(&BindingId(0)).unwrap();
        affine.values.insert(BindingId(1), image);
        let (view, renamed) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert!(Rc::ptr_eq(&view, &closed.state));
        assert!(
            !Rc::ptr_eq(&reminted, &renamed),
            "same images, different L0 terms"
        );
        affine.values.remove(&BindingId(1));
        let (_, removed) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert!(!Rc::ptr_eq(&renamed, &removed));
    });
}

#[test]
fn affine_index_cache_rebuilds_on_inventory_revisions_and_contradiction() {
    with_analyzer(|analyzer| {
        let mut facts = FactState::new();
        let affine = AffineFlowState::default();
        let measure = cache_measure(analyzer, 0);
        let (old_closed, old) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        // A newly registered measure gets an image before closing the view.
        let fresh = cache_measure(analyzer, 1);
        let (view, grown) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert!(!Rc::ptr_eq(&old_closed, &view));
        assert!(!Rc::ptr_eq(&old, &grown));
        let image = analyzer.vocabulary.measure_atom(fresh, &affine);
        assert!(
            grown
                .entry(image.terms(), &view, &mut AffineCheckState::new())
                .is_some()
        );
        let count = analyzer.vocabulary.terms.ids().count();
        analyzer
            .vocabulary
            .terms
            .set_measure_bound(measure, MeasureBound::Constant(7));
        let (_, fixed) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        assert_eq!(count, analyzer.vocabulary.terms.ids().count());
        assert!(!Rc::ptr_eq(&grown, &fixed));

        let expression = GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Bool(true)));
        let goal = analyzer
            .vocabulary
            .goals
            .intern(expression.clone(), None, None, vec![]);
        let event = analyzer
            .vocabulary
            .derivations
            .event(FlowEventKind::S1, None);
        facts.establish_goal(
            goal,
            GoalSign::Positive,
            &mut analyzer.vocabulary.derivations,
            event,
        );
        let (_, before) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        let count = analyzer.vocabulary.goals.ids().count();
        let same = analyzer.vocabulary.goals.intern(
            expression,
            Some(Relation::Bound {
                left: fresh,
                right: ZERO,
                bound: 2,
            }),
            None,
            vec![],
        );
        assert_eq!(goal, same);
        assert_eq!(count, analyzer.vocabulary.goals.ids().count());
        let (view, projected) =
            assert_affine_index_matches_rebuild(analyzer, ProofContext::new(&facts, &affine));
        // A projection lets L0 prove a signed goal, not the reverse. S1
        // publishes a comparison's relation separately; attaching metadata
        // to an already-established opaque goal cannot establish fresh <= 2.
        assert_eq!(view.tight_bound(fresh, ZERO), Some(i128::from(u64::MAX)));
        assert!(!view.derives_bound(fresh, ZERO, 2));
        assert!(view.derives_goal(goal, GoalSign::Positive, &analyzer.vocabulary.goals));
        assert!(!Rc::ptr_eq(&before, &projected));
        facts.establish_goal(
            goal,
            GoalSign::Negative,
            &mut analyzer.vocabulary.derivations,
            event,
        );
        // Contradiction bypasses exact-vector lookup in DIRECT; compare the
        // selected contradiction proof with and without the memo below.
        let (view, index) = analyzer
            .reasoning()
            .affine_query_view(ProofContext::new(&facts, &affine));
        assert!(view.contradictory());
        assert!(!Rc::ptr_eq(&projected, &index));
        let impossible =
            AffineInequality::from_terms(&[], -1, &mut AffineCheckState::new()).unwrap();
        let answer = analyzer
            .reasoning()
            .affine_target_proof(&impossible, &[], ProofContext::new(&facts, &affine))
            .unwrap();
        assert_eq!(answer.parents, vec![view.contradiction_proof().unwrap()]);
        analyzer.vocabulary.affine_l0_cache = None;
        let rebuilt = analyzer
            .reasoning()
            .affine_target_proof(&impossible, &[], ProofContext::new(&facts, &affine))
            .unwrap();
        assert_eq!(answer.parents, rebuilt.parents);
    });
}

#[test]
fn affine_index_cache_preserves_direct_auto_families_and_selected_parents() {
    with_analyzer(|analyzer| {
        let mut facts = FactState::new();
        let affine = AffineFlowState::default();
        let [x, y] = [0, 1].map(|binding| cache_measure(analyzer, binding));
        let [a, b, c, d, p, q] =
            [0; 6].map(|_| analyzer.vocabulary.new_affine_atom(IntegerType::U64));
        let mut check = AffineCheckState::new();
        let ab = a.add(&b, &mut check).unwrap();
        let cd = c.add(&d, &mut check).unwrap();
        affine.measure_atoms.borrow_mut().insert(x, ab.clone());
        affine.measure_atoms.borrow_mut().insert(y, cd.clone());
        cache_bound(analyzer, &mut facts, x, ZERO, 2);
        cache_bound(analyzer, &mut facts, y, ZERO, 3);
        let target = |form: &AffineForm, upper| {
            AffineInequality::from_bounded_forms(
                form,
                &AffineForm::constant(0),
                upper,
                &mut AffineCheckState::new(),
            )
            .unwrap()
        };
        let premise = |inequality, ordinal| ActiveAffineFact {
            inequality,
            evidence: AffineFactEvidence::Source(SourceAffineFactRef::LoopInvariant(
                SourceLoopInvariantRef {
                    loop_id: CheckedLoopId(0),
                    source_ordinal: ordinal,
                },
            )),
        };
        let sum = ab.add(&cd, &mut check).unwrap();
        let pq = p.add(&q, &mut check).unwrap();
        let cases = [
            (target(&ab, 2), vec![], true), // DIRECT exact image.
            (target(&ab, 1), vec![], false),
            (target(&sum, 5), vec![], true), // Final L0 image + DIRECT.
            (target(&sum, 4), vec![], false),
            (target(&p, 4), vec![premise(target(&p, 4), 0)], true),
            (
                target(&pq, 10),
                vec![premise(target(&p, 4), 0), premise(target(&q, 6), 1)],
                true,
            ),
            (
                target(&pq, 9),
                vec![premise(target(&p, 4), 0), premise(target(&q, 6), 1)],
                false,
            ),
            (
                target(&p, 4),
                vec![premise(target(&p.scale(6, &mut check).unwrap(), 27), 0)],
                true,
            ),
        ];
        for (target, assumptions, expected) in cases {
            let context = ProofContext::new(&facts, &affine);
            analyzer.vocabulary.affine_l0_cache = None;
            let (closed, primed) = analyzer.reasoning().affine_query_view(context);
            assert!(primed.exact.borrow().is_empty());
            assert!(primed.complete.borrow().is_none());
            let observed = analyzer
                .reasoning()
                .affine_target_proof(&target, &assumptions, context)
                .map(|proof| (proof.premises, proof.parents));
            assert!(Rc::ptr_eq(
                &primed,
                &analyzer.vocabulary.affine_l0_cache.as_ref().unwrap().index
            ));
            // Force the unchanged full builder, not a second lookup of the
            // same memo. Compare the selected source and L0 parents as well
            // as success; an incomplete final-image family must fail here.
            let full = affine_l0_index(&primed.candidates, &closed, &mut check);
            analyzer.vocabulary.affine_l0_cache = Some(AffineL0Cache {
                closed: Rc::clone(&closed),
                index: Rc::new(full_affine_query_index(primed.candidates.clone(), &full)),
            });
            let rebuilt = analyzer
                .reasoning()
                .affine_target_proof(&target, &assumptions, context)
                .map(|proof| (proof.premises, proof.parents));
            assert_eq!(observed, rebuilt, "{target:?}");
            analyzer.vocabulary.affine_l0_cache = None;
            let reference = analyzer
                .reasoning()
                .reference_affine_target_proof(&target, &assumptions, context)
                .map(|proof| (proof.premises, proof.parents));
            assert_eq!(observed, reference, "allocating route: {target:?}");
            assert_eq!(observed.is_some(), expected, "{target:?}");
        }
    });
}

#[test]
fn affine_scratch_preserves_late_winners_cancellation_and_boundary_routes() {
    with_analyzer(|analyzer| {
        let [a, b] = [0; 2].map(|_| {
            analyzer
                .vocabulary
                .new_affine_atom(IntegerType::I32)
                .unit_term()
                .unwrap()
        });
        let inequality = |terms: &[(AffineTermId, i128)], upper| {
            AffineInequality::from_terms(terms, upper, &mut AffineCheckState::new()).unwrap()
        };
        let source = |ordinal| {
            SourceAffineFactRef::LoopInvariant(SourceLoopInvariantRef {
                loop_id: CheckedLoopId(0),
                source_ordinal: ordinal,
            })
        };
        let cases = [
            (
                "unrepresentable single before first of two successes",
                inequality(&[(a, 1)], 4),
                vec![
                    inequality(&[(a, i128::MIN), (b, 1)], 0),
                    inequality(&[(a, 1)], 4),
                    inequality(&[(a, 1)], 3),
                ],
                Some(vec![(1, 1)]),
            ),
            (
                "failed and overflowing pairs before the successful pair",
                inequality(&[(a, 1), (b, 1)], 10),
                vec![
                    inequality(&[(a, i128::MAX)], 0),
                    inequality(&[(a, 1)], 4),
                    inequality(&[(b, 1)], 6),
                ],
                Some(vec![(1, 1), (2, 1)]),
            ),
            (
                "pair cancellation then zero residual",
                inequality(&[(a, 1)], 0),
                vec![
                    inequality(&[(a, 2), (b, -2)], 0),
                    inequality(&[(a, -1), (b, 2)], 0),
                ],
                Some(vec![(0, 1), (1, 1)]),
            ),
            (
                "MIN coefficient cancellation still fails before addition",
                inequality(&[(a, i128::MIN)], 0),
                vec![inequality(&[(a, i128::MIN)], 0)],
                None,
            ),
            (
                "residual coefficient overflow is not a proof",
                inequality(&[(a, i128::MAX)], 0),
                vec![inequality(&[(a, -1)], 0)],
                None,
            ),
            (
                "upper overflow does not hide the later exact MIN bound",
                inequality(&[(a, 1)], i128::MIN),
                vec![inequality(&[(a, 1)], 1), inequality(&[(a, 1)], i128::MIN)],
                Some(vec![(1, 1)]),
            ),
        ];
        let facts = FactState::new();
        let affine = AffineFlowState::default();
        for (name, target, premises, expected_sources) in cases {
            let assumptions = premises
                .into_iter()
                .enumerate()
                .map(|(ordinal, inequality)| ActiveAffineFact {
                    inequality,
                    evidence: AffineFactEvidence::Source(source(u32::try_from(ordinal).unwrap())),
                })
                .collect::<Vec<_>>();
            let context = ProofContext::new(&facts, &affine);
            analyzer.vocabulary.affine_l0_cache = None;
            let actual = analyzer
                .reasoning()
                .affine_target_proof(&target, &assumptions, context)
                .map(|proof| (proof.premises, proof.parents));
            analyzer.vocabulary.affine_l0_cache = None;
            let expected = analyzer
                .reasoning()
                .reference_affine_target_proof(&target, &assumptions, context)
                .map(|proof| (proof.premises, proof.parents));
            assert_eq!(actual, expected, "{name}");
            assert_eq!(
                actual.as_ref().map(|(premises, _)| premises.clone()),
                expected_sources.map(|selected| selected
                    .into_iter()
                    .map(|(ordinal, factor)| {
                        AffinePremiseUse {
                            source: source(ordinal),
                            factor,
                        }
                    })
                    .collect::<Vec<_>>()),
                "source selection pins the single/pair route: {name}"
            );
        }
    });
}

#[test]
fn canonical_interval_requests_keep_endpoint_ties_and_selected_parents() {
    with_integer_parameters(&[IntegerType::I32; 3], |analyzer| {
        let mut affine = AffineFlowState::default();
        let a = analyzer.vocabulary.new_affine_atom(IntegerType::I32);
        let b = analyzer.vocabulary.new_affine_atom(IntegerType::I32);
        affine.values.insert(BindingId(0), a.clone());
        affine.values.insert(BindingId(1), a.clone());
        affine.values.insert(BindingId(2), b.clone());
        let candidates = analyzer.reasoning().affine_l0_candidates(&affine);
        let mut facts = FactState::new();
        cache_bound(analyzer, &mut facts, candidates[1].term, ZERO, 5);
        cache_bound(analyzer, &mut facts, candidates[2].term, ZERO, 5);
        cache_bound(analyzer, &mut facts, ZERO, candidates[3].term, 2);
        let (closed, l0) = analyzer
            .reasoning()
            .affine_query_view(ProofContext::new(&facts, &affine));
        let mut selected = vec![
            closed
                .bound_proof(
                    candidates[1].term,
                    ZERO,
                    5,
                    &mut analyzer.vocabulary.derivations,
                )
                .unwrap(),
            closed
                .bound_proof(
                    ZERO,
                    candidates[3].term,
                    2,
                    &mut analyzer.vocabulary.derivations,
                )
                .unwrap(),
        ];
        selected.sort_unstable_by_key(|parent| parent.0);
        selected.dedup();
        let mut query = AffineDirectQuery::new(&l0, &affine, &closed);
        let mut reference = AffineDirectQuery::new(&l0, &affine, &closed);
        for upper in [9, 8, 9] {
            let target = AffineInequality::from_terms(
                &[(b.unit_term().unwrap(), -2), (a.unit_term().unwrap(), 1)],
                upper,
                &mut AffineCheckState::new(),
            )
            .unwrap();
            let actual = analyzer.reasoning().affine_interval_proof(
                &target,
                &mut query,
                &mut AffineCheckState::new(),
            );
            let expected = analyzer.reasoning().reference_affine_interval_proof(
                &target,
                &mut reference,
                &mut AffineCheckState::new(),
            );
            assert_eq!(actual, expected);
            assert_eq!(actual, Ok((upper == 9).then(|| selected.clone())));
        }
    });
}

#[test]
fn affine_index_cache_demands_only_requested_vectors_and_memoizes_absence() {
    with_analyzer(|analyzer| {
        let affine = AffineFlowState::default();
        let mut facts = FactState::new();
        let x = cache_measure(analyzer, 0);
        for binding in 1..16 {
            cache_measure(analyzer, binding);
        }
        cache_bound(analyzer, &mut facts, x, ZERO, 7);
        let a = analyzer.vocabulary.measure_atom(x, &affine);
        let target = AffineInequality::from_bounded_forms(
            &a,
            &AffineForm::constant(0),
            7,
            &mut AffineCheckState::new(),
        )
        .unwrap();
        let context = ProofContext::new(&facts, &affine);
        let (closed, index) = analyzer.reasoning().affine_query_view(context);
        assert!(index.exact.borrow().is_empty());
        assert!(
            analyzer
                .reasoning()
                .affine_target_proof(&target, &[], context)
                .is_some()
        );
        assert_eq!(index.exact.borrow().len(), 1);
        assert!(index.complete.borrow().is_none());
        {
            let mut check = AffineCheckState::new();
            let first = index.entry(a.terms(), &closed, &mut check).unwrap();
            let repeated = index.entry(a.terms(), &closed, &mut check).unwrap();
            assert!(
                std::ptr::eq(&*first, &*repeated),
                "lazy warm hits are borrowed"
            );
            assert_eq!(check.used(), 0);
        }
        let absent = analyzer.vocabulary.new_affine_atom(IntegerType::I32);
        for _ in 0..2 {
            assert!(
                index
                    .entry(absent.terms(), &closed, &mut AffineCheckState::new())
                    .is_none()
            );
            assert_eq!(index.exact.borrow().len(), 2);
            assert!(matches!(
                index.exact.borrow().get(absent.terms()),
                Some(None)
            ));
        }
        let (_, repeated) = analyzer.reasoning().affine_query_view(context);
        assert!(Rc::ptr_eq(&index, &repeated));
    });
}

#[test]
fn affine_index_cache_promoted_final_family_keeps_disjoint_images_and_late_winners() {
    with_integer_parameters(&[IntegerType::I32; 3], |analyzer| {
        let mut affine = AffineFlowState::default();
        let a = analyzer.vocabulary.new_affine_atom(IntegerType::I32);
        let b = analyzer.vocabulary.new_affine_atom(IntegerType::I32);
        let mut check = AffineCheckState::new();
        affine.values.insert(BindingId(0), b.clone());
        affine
            .values
            .insert(BindingId(1), a.subtract(&b, &mut check).unwrap());
        affine.values.insert(
            BindingId(2),
            b.add(&AffineForm::constant(5), &mut check).unwrap(),
        );
        let candidates = analyzer.reasoning().affine_l0_candidates(&affine);
        let mut facts = FactState::new();
        cache_bound(analyzer, &mut facts, candidates[1].term, ZERO, 0);
        cache_bound(analyzer, &mut facts, candidates[2].term, ZERO, 1);
        // The later alias supplies b <= -1; b <= 0 cannot prove a <= 0.
        cache_bound(analyzer, &mut facts, candidates[3].term, ZERO, 4);
        let context = ProofContext::new(&facts, &affine);
        let (closed, lazy) = analyzer.reasoning().affine_query_view(context);
        let full = affine_l0_index(&candidates, &closed, &mut check);
        for upper in [0, -1] {
            analyzer.vocabulary.affine_l0_cache = Some(AffineL0Cache {
                closed: Rc::clone(&closed),
                index: Rc::clone(&lazy),
            });
            let target = AffineInequality::from_bounded_forms(
                &a,
                &AffineForm::constant(0),
                upper,
                &mut check,
            )
            .unwrap();
            let observed = analyzer
                .reasoning()
                .affine_target_proof(&target, &[], context)
                .map(|proof| (proof.premises, proof.parents));
            assert_eq!(observed.is_some(), upper == 0);
            assert!(
                lazy.complete.borrow().is_some(),
                "success and exhaustion promote"
            );
            assert!(lazy.exact.borrow().is_empty());
            analyzer.vocabulary.affine_l0_cache = Some(AffineL0Cache {
                closed: Rc::clone(&closed),
                index: Rc::new(full_affine_query_index(candidates.clone(), &full)),
            });
            let expected = analyzer
                .reasoning()
                .affine_target_proof(&target, &[], context)
                .map(|proof| (proof.premises, proof.parents));
            assert_eq!(observed, expected);
        }
        assert_eq!(
            lazy.entry(b.terms(), &closed, &mut check).unwrap().left,
            candidates[3].term
        );
    });
}

#[test]
fn finite_arithmetic_components_use_current_images_in_component_order() {
    use CheckedIntegerOperation::{
        AddExact, MultiplyExact, ShiftLeftExact, ShiftRightExact, SubtractExact,
    };
    // Operation, operand type, constant, constant first, current value, bad value.
    let cases = [
        (AddExact, IntegerType::U64, 4, false, 8, u64::MAX as i128),
        (AddExact, IntegerType::U64, 4, true, 8, u64::MAX as i128),
        (SubtractExact, IntegerType::U64, 4, false, 8, 3),
        (SubtractExact, IntegerType::U64, 20, true, 8, 21),
        (
            MultiplyExact,
            IntegerType::U64,
            2,
            false,
            8,
            u64::MAX as i128,
        ),
        (
            MultiplyExact,
            IntegerType::I64,
            -2,
            true,
            8,
            i64::MIN as i128,
        ),
        (
            AddExact,
            IntegerType::I64,
            -4,
            false,
            i64::MIN as i128 + 4,
            i64::MIN as i128 + 3,
        ),
        (
            SubtractExact,
            IntegerType::I64,
            4,
            false,
            i64::MIN as i128 + 4,
            i64::MIN as i128 + 3,
        ),
        (ShiftLeftExact, IntegerType::U64, 1, true, 8, 64),
        (ShiftRightExact, IntegerType::U64, 1, true, 8, 64),
    ];
    for (operation, ty, constant, constant_first, value, bad_value) in cases {
        with_analyzer(|analyzer| {
            let binding = BindingId(0);
            let value_ty = if matches!(operation, ShiftLeftExact | ShiftRightExact) {
                IntegerType::U32
            } else {
                ty
            };
            let term = analyzer
                .vocabulary
                .terms
                .intern(TermKind::Place(ResolvedPlace::binding(binding), value_ty));
            let variable = IntegerDomainOperand {
                term: Some(term),
                constant: None,
            };
            let literal = IntegerDomainOperand {
                term: Some(
                    analyzer
                        .vocabulary
                        .terms
                        .intern(TermKind::Constant(constant)),
                ),
                constant: Some(constant),
            };
            let operands = if constant_first {
                [literal, variable]
            } else {
                [variable, literal]
            };
            let operand_type = CheckedType::Integer(ty);
            let plan = analyzer
                .vocabulary
                .integer_domain_plan(operation, operand_type, &operands)
                .unwrap();
            let goal = IntegerDomainGoal {
                canonical: None,
                operation,
                operand_type,
                components: &plan.components,
                affine_clauses: None,
                affine_product: None,
            };
            let facts = FactState::new();
            let mut affine = AffineFlowState::default();
            affine.values.insert(binding, AffineForm::constant(value));
            let result = analyzer
                .reasoning()
                .prove_integer_domain_finite(ProofContext::new(&facts, &affine), &goal);
            assert_eq!(
                result.disposition,
                ProofDisposition::Proved,
                "{operation:?} {ty:?}"
            );
            assert_eq!(result.route, Some(ProofRoute::Affine));
            let DerivationNode::IntegerDomain { parents, .. } =
                &analyzer.vocabulary.derivations.nodes[result.derivation.unwrap().0 as usize]
            else {
                panic!("finite components must retain one aggregate root");
            };
            assert_eq!(parents.len(), plan.components.len());
            for (parent, request) in parents.iter().zip(&plan.components) {
                if let DerivationNode::AffineConsequence { relation, .. } =
                    &analyzer.vocabulary.derivations.nodes[parent.0 as usize]
                {
                    assert_eq!(relation.as_deref(), request_relation(request).as_ref());
                }
            }
            // A successful query publishes no fact usable with another image.
            affine
                .values
                .insert(binding, AffineForm::constant(bad_value));
            let failed = analyzer
                .reasoning()
                .prove_integer_domain_finite(ProofContext::new(&facts, &affine), &goal);
            assert_ne!(
                failed.disposition,
                ProofDisposition::Proved,
                "{operation:?} {ty:?}"
            );
            assert!(failed.derivation.is_none());
        });
    }
}

#[test]
fn finite_lower_component_uses_the_original_right_term_for_step_six() {
    with_analyzer(|analyzer| {
        let mut affine = AffineFlowState::default();
        let old = analyzer.vocabulary.terms.intern(TermKind::Measure(
            CheckedMeasure::Length,
            ResolvedPlace::binding(BindingId(0)),
        ));
        let current = analyzer.vocabulary.terms.intern(TermKind::Measure(
            CheckedMeasure::Length,
            ResolvedPlace::binding(BindingId(1)),
        ));
        let old_image = analyzer.vocabulary.measure_atom(old, &affine);
        let _ = analyzer.vocabulary.measure_atom(current, &affine);
        let fp = analyzer.vocabulary.new_affine_atom(IntegerType::U64);
        affine.facts.push(ActiveAffineFact {
            inequality: AffineInequality::from_bounded_forms(
                &fp,
                &old_image,
                -4,
                &mut AffineCheckState::new(),
            )
            .unwrap(),
            evidence: AffineFactEvidence::Source(SourceAffineFactRef::LoopInvariant(
                SourceLoopInvariantRef {
                    loop_id: CheckedLoopId(0),
                    source_ordinal: 0,
                },
            )),
        });
        let mut facts = FactState::new();
        let event = analyzer
            .vocabulary
            .derivations
            .event(FlowEventKind::S1, None);
        facts.establish(
            &Relation::Equal {
                left: old,
                right: current,
                difference: 0,
            },
            &mut analyzer.vocabulary.derivations,
            event,
        );
        let four = analyzer.vocabulary.terms.intern(TermKind::Constant(4));
        let operation = CheckedIntegerOperation::SubtractExact;
        let operand_type = CheckedType::Integer(IntegerType::U64);
        let plan = analyzer
            .vocabulary
            .integer_domain_plan(
                operation,
                operand_type,
                &[
                    IntegerDomainOperand {
                        term: Some(current),
                        constant: None,
                    },
                    IntegerDomainOperand {
                        term: Some(four),
                        constant: Some(4),
                    },
                ],
            )
            .unwrap();
        let goal = IntegerDomainGoal {
            canonical: None,
            operation,
            operand_type,
            components: &plan.components,
            affine_clauses: None,
            affine_product: None,
        };
        let result = analyzer
            .reasoning()
            .prove_integer_domain_finite(ProofContext::new(&facts, &affine), &goal);
        assert_eq!(result.disposition, ProofDisposition::Proved);
        let DerivationNode::IntegerDomain { parents, .. } =
            &analyzer.vocabulary.derivations.nodes[result.derivation.unwrap().0 as usize]
        else {
            panic!("one integer-domain root must retain both components");
        };
        assert_eq!(parents.len(), 2);
        assert!(matches!(
            analyzer.vocabulary.derivations.nodes[parents[1].0 as usize],
            DerivationNode::TransitiveBound { left: ZERO, middle, right, bound: -4, .. }
                if middle == old && right == current
        ));
        let mut unavailable = plan.components[1];
        unavailable.left = None;
        let unknown = analyzer
            .reasoning()
            .prove_finite_component(ProofContext::new(&facts, &affine), unavailable);
        assert_eq!(unknown.disposition, ProofDisposition::Unknown);
        assert!(unknown.derivation.is_none());
    });
}

// Before the repair, the first request panics while folding MAX + 1.
// This tests the existing checked-folding policy, not a source verdict.
#[test]
fn offset_request_folding_uses_the_existing_checked_constant_domain() {
    let mut terms = TermTable::new();
    let one = terms.intern(TermKind::Constant(1));
    let x = terms.intern(TermKind::Place(
        ResolvedPlace::binding(BindingId(0)),
        IntegerType::I64,
    ));
    let mut requests = [
        BoundsRequest {
            left: Some(x),
            right: one,
            bound: i128::MAX,
            distinct: true,
        },
        BoundsRequest {
            left: Some(one),
            right: x,
            bound: i128::MIN,
            distinct: true,
        },
        BoundsRequest {
            left: Some(x),
            right: ZERO,
            bound: i128::MIN,
            distinct: true,
        },
    ];
    normalize_distinct_requests(&mut requests, &terms);
    assert!(request_relation(&requests[0]).is_none());
    assert!(request_relation(&requests[1]).is_none());
    assert_eq!(
        request_relation(&requests[2]),
        Some(Relation::Distinct {
            left: ZERO,
            right: x,
            difference: i128::MAX,
        })
    );
}

// The replacement crosses term ordering without changing the mathematical
// relation. Previously both transport helpers evaluated -MIN unchecked.
#[test]
fn offset_extremes_survive_delivery_and_call_term_replacement() {
    let first = TermId(1);
    let second = TermId(2);
    let replacement = TermId(3);
    for (offset, reversed) in [
        (i128::MIN, i128::MAX),
        (i128::MIN + 1, i128::MAX),
        (i128::MAX - 1, i128::MIN + 2),
        (i128::MAX, i128::MIN + 1),
    ] {
        for (left, right, expected) in [
            (
                first,
                second,
                Relation::Distinct {
                    left: second,
                    right: replacement,
                    difference: reversed,
                },
            ),
            (
                second,
                first,
                Relation::Distinct {
                    left: second,
                    right: replacement,
                    difference: offset,
                },
            ),
        ] {
            let relation = Relation::Distinct {
                left,
                right,
                difference: offset,
            };
            assert_eq!(
                sources::substitute_delivery_relation(&relation, first, replacement),
                expected
            );
            assert_eq!(
                postconditions::replace_relation_term(&relation, first, replacement),
                expected
            );
        }
    }
}

// Allocating traversal retained from the base revision. It deliberately keeps
// the original pair clones, residual formation and sorted atom requests so
// comparisons do not simply run the new scratch path twice.
use crate::semantic::entailment::affine::{
    integer_tightenings, reference_residual_after, sum_explicit_inequalities,
};

impl Reasoning<'_, '_, '_> {
    pub(super) fn reference_affine_interval_proof(
        &mut self,
        inequality: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Result<Option<Vec<DerivationId>>, AffineCheckError> {
        let mut requested = inequality
            .terms()
            .iter()
            .map(|coefficient| coefficient.term())
            .collect::<Vec<_>>();
        requested.sort_unstable();
        requested.dedup();

        if query.measures.is_none() {
            query.measures = Some(self.vocabulary.measure_terms_by_atom(query.values));
        }
        let measures = query
            .measures
            .as_ref()
            .expect("measure index prepared above");
        for atom_id in requested {
            if query.intervals.contains_key(&atom_id) {
                continue;
            }
            let atom = *self
                .vocabulary
                .affine_atoms
                .get(atom_id.index() as usize)
                .ok_or(AffineCheckError::CoefficientMismatch)?;
            let mut interval = AffineAtomInterval {
                minimum: atom.minimum,
                maximum: atom.maximum,
                minimum_parent: None,
                maximum_parent: None,
            };
            let mut bindings = query
                .values
                .values
                .iter()
                .filter_map(|(binding, value)| {
                    (value.unit_term() == Some(atom_id)).then_some(*binding)
                })
                .collect::<Vec<_>>();
            bindings.sort_by_key(|binding| binding.0);
            let mut terms = bindings
                .into_iter()
                .filter_map(|binding| {
                    if self.input.affine_binding_type(binding) != Some(atom.ty) {
                        return None;
                    }
                    Some(self.vocabulary.terms.intern(TermKind::Place(
                        ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                        atom.ty,
                    )))
                })
                .collect::<Vec<_>>();
            if let Some(measures) = measures.get(&atom_id) {
                terms.extend(measures.iter().copied());
            }
            for term in terms {
                if let Some(upper) = query.closed.tight_bound(term, ZERO)
                    && upper < interval.maximum
                {
                    interval.maximum = upper;
                    interval.maximum_parent = Some((term, ZERO, upper));
                }
                if let Some(negative_lower) = query.closed.tight_bound(ZERO, term)
                    && let Some(lower) = negative_lower.checked_neg()
                    && lower > interval.minimum
                {
                    interval.minimum = lower;
                    interval.minimum_parent = Some((ZERO, term, negative_lower));
                }
            }
            query.intervals.insert(atom_id, interval);
        }

        if query.closed.contradictory() {
            return Ok(query.closed.contradiction_proof().map(|proof| vec![proof]));
        }
        let proved = interval_proves(
            inequality,
            |term| {
                query
                    .intervals
                    .get(&term)
                    .map(|interval| (interval.minimum, interval.maximum))
            },
            check,
        )?;
        if !proved {
            return Ok(None);
        }
        let mut parents = Vec::new();
        for coefficient in inequality.terms() {
            let interval = query
                .intervals
                .get(&coefficient.term())
                .ok_or(AffineCheckError::CoefficientMismatch)?;
            let selected = if coefficient.coefficient() > 0 {
                interval.maximum_parent
            } else {
                interval.minimum_parent
            };
            if let Some((left, right, bound)) = selected {
                let parent = query
                    .closed
                    .bound_proof(left, right, bound, &mut self.vocabulary.derivations)
                    .ok_or(AffineCheckError::CoefficientMismatch)?;
                parents.push(parent);
            }
        }
        parents.sort_unstable_by_key(|parent| parent.0);
        parents.dedup();
        Ok(Some(parents))
    }

    pub(super) fn reference_affine_residual_proof(
        &mut self,
        inequality: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Result<Option<Vec<DerivationId>>, AffineCheckError> {
        if query.closed.contradictory() {
            return Ok(query.closed.contradiction_proof().map(|proof| vec![proof]));
        }
        if let Some(parents) =
            self.vocabulary
                .affine_l0_proof(inequality, query.l0, query.closed, check)?
        {
            return Ok(Some(parents));
        }
        self.reference_affine_interval_proof(inequality, query, check)
    }

    pub(super) fn reference_affine_candidate_residual_proof(
        &mut self,
        target: &AffineInequality,
        candidate: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Option<Vec<DerivationId>> {
        let tightenings = integer_tightenings(candidate, target, check);
        for accumulated in std::iter::once(candidate).chain(tightenings.iter()) {
            let Ok(residual) = reference_residual_after(target, accumulated, check) else {
                continue;
            };
            if let Ok(Some(parents)) = self.reference_affine_residual_proof(&residual, query, check)
            {
                return Some(parents);
            }
        }
        None
    }

    pub(super) fn reference_affine_l0_then_direct_proof(
        &mut self,
        target: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Option<Vec<DerivationId>> {
        super::super::super::work::affine_final_family_start();
        let l0 = query.l0;
        let mut ordinal = 0;
        while let Some(entry) = l0.ordered_entry(ordinal, query.closed, check) {
            ordinal += 1;
            let Some(mut parents) = self.reference_affine_candidate_residual_proof(
                target,
                &entry.inequality,
                query,
                check,
            ) else {
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
        super::super::super::work::affine_final_family_exhausted();
        None
    }

    pub(super) fn reference_affine_target_proof(
        &mut self,
        target: &AffineInequality,
        assumptions: &[ActiveAffineFact],
        context: ProofContext<'_>,
    ) -> Option<AffineConsequenceProof> {
        let values = context.affine;
        let mut check = AffineCheckState::new();
        let (closed, l0) = self.affine_query_view(context);
        let mut query = AffineDirectQuery::new(&l0, values, &closed);
        if let Ok(Some(parents)) =
            self.reference_affine_residual_proof(target, &mut query, &mut check)
        {
            return Some(AffineConsequenceProof {
                premises: Vec::new(),
                parents,
            });
        }
        let automatic = automatic_affine_premises(assumptions, &mut check).ok()?;

        // Preserve the complete coefficient-one single-premise route. Every
        // premise is tried independently; an arithmetic error in one candidate
        // cannot suppress a later source or value-image fact.
        for (index, assumption) in automatic.iter().enumerate() {
            // A candidate that cannot participate in an i128 residual grants
            // no authority, but it must not hide a later independently
            // representable source fact in the same deterministic order.
            if let Some(parents) = self.reference_affine_candidate_residual_proof(
                target,
                &assumption.inequality,
                &mut query,
                &mut check,
            ) {
                return Some(affine_consequence_from_residual(
                    &[(index, 1)],
                    &automatic,
                    parents,
                ));
            }
        }

        // R2 exhausts the source-shaped set of unordered coefficient-one
        // pairs, including one premise used twice. There is no greedy state,
        // backtracking cutoff, or cumulative work budget: fact order changes
        // only which successful derivation is retained, never acceptance.
        if let Some((first, second, parents)) =
            reference_first_two_premise_candidate(&automatic, &mut check, |sum, check| {
                self.reference_affine_candidate_residual_proof(target, sum, &mut query, check)
            })
        {
            let selected = if first == second {
                vec![(first, 2)]
            } else {
                vec![(first, 1), (second, 1)]
            };
            return Some(affine_consequence_from_residual(
                &selected, &automatic, parents,
            ));
        }

        // Ordinary L0 relations remain outside the affine premise set. This
        // is the specification's final `DIRECT(T - R)` family: subtract each
        // strongest indexed L0 image once, then run the ordinary DIRECT check
        // on the residual. DIRECT may itself close an exact L0 image, but the
        // route never publishes or recursively saturates either relation.
        self.reference_affine_l0_then_direct_proof(target, &mut query, &mut check)
            .map(|parents| AffineConsequenceProof {
                premises: Vec::new(),
                parents,
            })
    }
}

pub(super) fn reference_first_two_premise_candidate<T>(
    premises: &[AutomaticAffinePremise],
    check: &mut AffineCheckState,
    mut prove: impl FnMut(&AffineInequality, &mut AffineCheckState) -> Option<T>,
) -> Option<(usize, usize, T)> {
    for first in 0..premises.len() {
        for second in first..premises.len() {
            let pair = [
                premises[first].inequality.clone(),
                premises[second].inequality.clone(),
            ];
            let Ok(sum) = sum_explicit_inequalities(&pair, check) else {
                continue;
            };
            if let Some(proof) = prove(&sum, check) {
                return Some((first, second, proof));
            }
        }
    }
    None
}
