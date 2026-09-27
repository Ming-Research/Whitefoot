//! The one `prove` dispatcher, whose route order [MSR-4] fixes, and
//! its routes: the L0 closure, the fixed affine and interval rules, the
//! integer-domain and separation proofs, and the measure terms they read.

use super::*;

impl Vocabulary {
    pub(super) fn intern_measure(
        &mut self,
        measure: CheckedMeasure,
        path: &ResolvedPlace,
    ) -> TermId {
        self.terms.intern(TermKind::Measure(measure, path.clone()))
    }

    pub(super) fn goal_numeric_derivation(
        &mut self,
        goal: Option<GoalId>,
        relation: Option<&Relation>,
        parent: DerivationId,
    ) -> DerivationId {
        let Some(goal) = goal else {
            return parent;
        };
        if let Some(relation) = relation
            && self.goals.projection(goal) == Some(relation)
        {
            return self.derivations.intern(DerivationNode::GoalProjection {
                goal,
                sign: GoalSign::Positive,
                relation: relation.clone(),
                parent,
            });
        }
        if let Some(relation) = relation
            && self.goals.normalization(goal).is_some_and(|normalization| {
                normalization.clause_is_single_relation(GoalSign::Positive, 0, relation)
            })
        {
            return self.derivations.intern(DerivationNode::GoalNormalization {
                goal,
                sign: GoalSign::Positive,
                clause: 0,
                parents: vec![parent],
            });
        }
        // A bounded obligation may have an exact complete goal whose source
        // occurrence is represented in L0 only through an evaluated alias.
        // The relation remains the obligation's direct proof root; recording
        // it as this globally interned goal's normalization would attach
        // occurrence-local data to a shared identity.
        if relation.is_some() {
            return parent;
        }
        self.derivations
            .intern(DerivationNode::GoalAffineConsequence {
                goal,
                sign: GoalSign::Positive,
                parent,
            })
    }

    pub(super) fn prove_integer_domain_finite(
        &mut self,
        context: ProofContext<'_>,
        goal: &IntegerDomainGoal<'_>,
    ) -> ProofResult {
        let closed = close(
            context.facts,
            &self.terms,
            &self.goals,
            &mut self.derivations,
        );
        let contradictory = closed.contradictory();
        let signed_division = matches!(
            goal.operation,
            CheckedIntegerOperation::DivideExact | CheckedIntegerOperation::RemainderExact
        ) && fragment_type(goal.operand_type)
            .is_some_and(IntegerType::signed);
        let component_proof =
            |index: usize, derivations: &mut DerivationLedger| -> Option<DerivationId> {
                request_relation(goal.components.get(index)?)
                    .and_then(|relation| closed.relation_proof(&relation, derivations))
            };
        if contradictory {
            let parents = closed.contradiction_proof().map(|proof| vec![proof]);
            let Some(parents) = parents else {
                unreachable!("a contradictory closure retains its proof");
            };
            let derivation = self.derivations.intern(DerivationNode::IntegerDomain {
                goal: goal.canonical,
                parents,
            });
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        if let Some(canonical) = goal.canonical {
            if closed.holds_opaque(canonical, GoalSign::Positive) {
                let parent = closed
                    .opaque_proof(canonical, GoalSign::Positive)
                    .expect("an opaque goal fact retains its proof");
                let derivation = self.derivations.intern(DerivationNode::IntegerDomain {
                    goal: Some(canonical),
                    parents: vec![parent],
                });
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.holds_opaque(canonical, GoalSign::Negative) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let normalization_parents = if signed_division && goal.components.len() == 3 {
            component_proof(0, &mut self.derivations).and_then(|nonzero| {
                component_proof(1, &mut self.derivations)
                    .or_else(|| component_proof(2, &mut self.derivations))
                    .map(|witness| vec![nonzero, witness])
            })
        } else if !goal.components.is_empty() {
            goal.components
                .iter()
                .map(|request| {
                    request_relation(request).and_then(|relation| {
                        closed.relation_proof(&relation, &mut self.derivations)
                    })
                })
                .collect::<Option<Vec<_>>>()
        } else {
            None
        };
        if let Some(parents) = normalization_parents {
            let parents = if let Some(canonical) = goal.canonical {
                if let Some(normalization) = closed.normalization_proof(
                    canonical,
                    GoalSign::Positive,
                    &self.goals,
                    &mut self.derivations,
                ) {
                    vec![normalization]
                } else {
                    // A complete admitted Goal may expand an ordinary-let
                    // operand into an exact operation that is not an L0 term.
                    // Its source occurrence can still have fixed L0
                    // components over the already evaluated alias. Those
                    // occurrence-local parents prove this IntegerDomain
                    // judgment directly; they must not become a normalization
                    // on the globally interned complete Goal identity.
                    parents
                }
            } else {
                parents
            };
            let derivation = self.derivations.intern(DerivationNode::IntegerDomain {
                goal: goal.canonical,
                parents,
            });
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::L0),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let component_false = |index: usize| {
            goal.components
                .get(index)
                .and_then(request_relation)
                .is_some_and(|relation| closed.derives(&relation.negated()))
        };
        let normalization_refuted = if signed_division && goal.components.len() == 3 {
            component_false(0) || (component_false(1) && component_false(2))
        } else {
            goal.components
                .iter()
                .filter_map(request_relation)
                .any(|relation| closed.derives(&relation.negated()))
        };
        if normalization_refuted {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::L0),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        ProofResult {
            disposition: ProofDisposition::Unknown,
            route: None,
            derivation: None,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Builds the fixed L0 vocabulary before closure. Each live integer
    /// binding contributes its ordinary term and its exact current affine
    /// value; Z is the fixed zero candidate. Later matching never invents a
    /// term after the closed state was formed.
    /// The image of one measure term [MSR-4].
    ///
    /// A measure whose table cell [MSR-1] fixes its value is a standing fact
    /// [MSR-2], and its image is that fact rather than a free atom: a cell
    /// with a constant value has that constant, and a cell the table equates
    /// to another term shares that term's image. Every other measure gets one
    /// compiler-owned immutable atom, minted on first use and stable for the
    /// rest of the function walk.
    /// The binding one measure term's place is rooted in, where it has one.
    pub(super) fn measure_term_root(&self, term: TermId) -> Option<BindingId> {
        let root = match self.terms.kind(term) {
            TermKind::Measure(_, place) => place.root,
            _ => return None,
        };
        match root {
            PlaceRoot::Binding(binding) => Some(binding),
            PlaceRoot::Constant(_) => None,
        }
    }

    /// The image this program point holds for one measure or const-generic term.
    ///
    /// [MSR-4]'s automatic derivation reads the current edge's immutable
    /// value image. A written invariant captures that image in its theorem;
    /// a later kill removes only the affected edge's current mapping, so a
    /// new image cannot reuse that theorem without a surviving relation.
    /// A measure the table fixes reads as its standing [MSR-2] fact.
    pub(super) fn measure_atom(&mut self, term: TermId, state: &AffineFlowState) -> AffineForm {
        let mut anchor = term;
        // The table relates a cell to a constant or to one other term, and
        // this version's rows chain at most once. The bound keeps a future
        // row from looping.
        for _ in 0..4 {
            match self.terms.measure_bound(anchor) {
                Some(MeasureBound::Constant(value)) => return AffineForm::constant(value),
                Some(MeasureBound::Equal(other)) => anchor = other,
                None => break,
            }
        }
        if let Some(atom) = state.measure_atoms.borrow().get(&anchor) {
            return atom.clone();
        }
        // [REF-4, MSR-1] an anonymous range actual has no binding on which
        // S6 can install its length image. Its measure term retains the exact
        // range step, whose original capture occurrence selects the image
        // published when the formation evaluated its endpoints.
        let captured = match self.terms.kind(anchor) {
            TermKind::Measure(CheckedMeasure::Length, place) => {
                place.path.last().and_then(|step| match step {
                    PlaceStep::Range(range) => Some(*range),
                    _ => None,
                })
            }
            _ => None,
        };
        if let Some(atom) =
            captured.and_then(|captured| captured_range_length_image(captured, state))
        {
            state
                .measure_atoms
                .borrow_mut()
                .insert(anchor, atom.clone());
            return atom;
        }
        // [MSR-6] a symbolic const parameter has its declaration's integer
        // type; measures and their immutable datums instead have type u64.
        // Sharing the image path cannot give a signed parameter the unsigned
        // nonnegativity bound or widen a narrower parameter's range.
        let ty = match self.terms.kind(anchor) {
            TermKind::ConstParameter(_, ty) => *ty,
            _ => IntegerType::U64,
        };
        let atom = self.new_affine_atom(ty);
        state
            .measure_atoms
            .borrow_mut()
            .insert(anchor, atom.clone());
        atom
    }

    /// [MSR-3] one measure datum inherits the atom the term it is established
    /// equal to holds at that point.
    ///
    /// The datum denotes that value, so it is that value in the affine domain
    /// too. Because nothing kills a datum, its atom outlives the write that
    /// retargets the term's: a header conclusion published over the old atom
    /// stays anchored to a live term, which is what lets one published
    /// relation preserve an invariant across a [SET-1] commit.
    pub(super) fn adopt_measure_atom(
        &mut self,
        datum: TermId,
        live: TermId,
        state: &AffineFlowState,
    ) {
        if state.measure_atoms.borrow().contains_key(&datum) {
            return;
        }
        let atom = self.measure_atom(live, state);
        // A constant image is still the exact immutable value this datum
        // captures. Retaining it is what carries an affine exact measure
        // across the transfer that kills the live place.
        state.measure_atoms.borrow_mut().insert(datum, atom);
    }

    /// Every registered measure term, in term order.
    ///
    /// The registry only grows during the forward walk, so this scans just
    /// the terms interned since the last call and keeps the answer. Every
    /// numeric goal queries it, and rescanning the whole registry per query
    /// made that quadratic in the size of the function.
    pub(super) fn measure_terms(&mut self) -> Vec<TermId> {
        let registered = self.terms.ids().count();
        for index in self.measure_terms_scanned..registered {
            let id = TermId(
                u32::try_from(index).expect("ENT term inventory exceeds the u32 identity space"),
            );
            // [MSR-3] a measure datum is a measure of the affine domain's
            // kind: it denotes one measure's value at a point, it is of
            // fragment type u64, and nothing kills it. It participates in
            // step 6's bridge exactly as a live measure term does, which is
            // what carries a header conclusion across the write that kills
            // the term the conclusion was published over.
            if matches!(
                self.terms.kind(id),
                TermKind::Measure(..)
                    | TermKind::CallDatum {
                        measure: Some(_),
                        ..
                    }
                    | TermKind::EntryDatum { .. }
                    | TermKind::MeasureDatum { .. }
            ) {
                self.measure_terms_seen.push(id);
            }
        }
        self.measure_terms_scanned = registered;
        self.measure_terms_seen.clone()
    }

    /// Every live measure term, grouped by the affine atom it images.
    ///
    /// [MSR-4]'s interval step starts from each atom's *direct closed L0*
    /// interval, so it has to be able to name the term whose value the atom
    /// stands for. A local's atom is named through the binding that denotes
    /// it; a measure atom's own name is its measure term [MSR-2], and without
    /// this map a measure entered every interval substitution at its complete
    /// `u64` range however tightly the closed state had already bounded it.
    pub(super) fn measure_terms_by_atom(
        &mut self,
        state: &AffineFlowState,
    ) -> WordHashMap<AffineTermId, Vec<TermId>> {
        let mut grouped: WordHashMap<AffineTermId, Vec<TermId>> = WordHashMap::default();
        for term in self.measure_terms() {
            if let Some(atom) = self.measure_atom(term, state).unit_term() {
                grouped.entry(atom).or_default().push(term);
            }
        }
        for terms in grouped.values_mut() {
            terms.sort_unstable_by_key(|term| term.0);
            terms.dedup();
        }
        grouped
    }

    /// Queries the strongest closed L0 image with exactly this affine vector.
    pub(super) fn affine_l0_proof(
        &mut self,
        inequality: &AffineInequality,
        index: &AffineL0Index,
        closed: &ClosedState,
    ) -> Result<Option<Vec<DerivationId>>, AffineCheckError> {
        let Some(entry) = index.entry(inequality.terms()) else {
            return Ok(None);
        };
        if entry.inequality.upper() > inequality.upper() {
            return Ok(None);
        }
        let parent = closed
            .bound_proof(entry.left, entry.right, entry.bound, &mut self.derivations)
            .ok_or(AffineCheckError::CoefficientMismatch)?;
        Ok(Some(vec![parent]))
    }
}

impl Reasoning<'_, '_, '_> {
    /// The one former of every [MSR-1] measure term.
    ///
    /// Every measure of one place is formed together, because [MSR-2]'s
    /// standing facts relate them to each other: the value the table fixes
    /// for a cell, the equality of a table cell to another measure, and the
    /// orderings `P.len <= P.cap` and `P.head <= P.cap`. A site that
    /// names only one measure still needs the others to exist for those
    /// facts to have terms to relate, and all three have empty support beyond
    /// P's own, so forming them together costs nothing a program can observe.
    pub(super) fn measure_term(
        &mut self,
        measure: CheckedMeasure,
        path: ResolvedPlace,
        measured: MeasuredKind,
        array_length: Option<CheckedConst>,
    ) -> TermId {
        let extent = self
            .vocabulary
            .intern_measure(CheckedMeasure::Length, &path);
        // [MSR-1]'s table, read once per cell.
        for cell_measure in [
            CheckedMeasure::Length,
            CheckedMeasure::Capacity,
            CheckedMeasure::Head,
        ] {
            let term = self.vocabulary.intern_measure(cell_measure, &path);
            let bound = match cell_measure.cell(measured) {
                MeasureCell::ExactConstant(value) => {
                    Some(MeasureBound::Constant(i128::from(value)))
                }
                MeasureCell::ExactExtent => match array_length {
                    Some(CheckedConst::Value(value)) => {
                        Some(MeasureBound::Constant(i128::from(value)))
                    }
                    Some(CheckedConst::Parameter(declaration)) => {
                        Some(MeasureBound::Equal(self.const_parameter_term(declaration)))
                    }
                    // A symbolic derived length has no [ENT-2] term form; the
                    // concrete instance, whose length is a value, restates the
                    // constant bound.
                    Some(CheckedConst::Derived(_)) => None,
                    // A runtime extent: `cap` is equal to it, `len` is it.
                    None => (cell_measure != CheckedMeasure::Length)
                        .then_some(MeasureBound::Equal(extent)),
                },
                // [MSR-2]: a measure the table fixes as the type's own
                // written constant is a standing fact with empty support; a
                // run's `cap` is that constant and a `Vector`'s is not.
                MeasureCell::ExactTypeConstant => match array_length {
                    Some(CheckedConst::Value(value)) => {
                        Some(MeasureBound::Constant(i128::from(value)))
                    }
                    Some(CheckedConst::Parameter(declaration)) => {
                        Some(MeasureBound::Equal(self.const_parameter_term(declaration)))
                    }
                    Some(CheckedConst::Derived(_)) | None => None,
                },
                // An independent runtime quantity of the value's own
                // descriptor: the standing facts [MSR-2] already publishes
                // relate it to the others, and it carries no bound of its own.
                MeasureCell::ExactRuntime | MeasureCell::Bounded | MeasureCell::Absent => None,
            };
            if let Some(bound) = bound {
                self.vocabulary.terms.set_measure_bound(term, bound);
            }
        }
        self.vocabulary.intern_measure(measure, &path)
    }

    pub(super) fn call_goal_disposition(
        &mut self,
        goal: &ConcreteGoal,
        context: ProofContext<'_>,
    ) -> (
        CallGoalDisposition,
        Vec<CallGoalEvidence>,
        Option<DerivationId>,
    ) {
        let affine_target = self.affine_goal_ordering_target(&goal.root, context.affine);
        let result = self.prove(
            context,
            ProofGoal::Signed {
                expression: &goal.root,
                affine: affine_target.as_ref(),
            },
        );
        let disposition = match result.disposition {
            ProofDisposition::Proved => CallGoalDisposition::Discharged,
            ProofDisposition::Refuted => CallGoalDisposition::Refuted,
            ProofDisposition::Unknown => CallGoalDisposition::Unproved,
        };
        let evidence = match (result.disposition, result.route) {
            (ProofDisposition::Proved, Some(ProofRoute::Contradiction)) => {
                vec![CallGoalEvidence::AllDerivable]
            }
            (
                sign @ (ProofDisposition::Proved | ProofDisposition::Refuted),
                Some(ProofRoute::SignedOrdinary {
                    opaque,
                    projection,
                    normalization,
                    introduction,
                }),
            ) => {
                let mut evidence = Vec::with_capacity(4);
                match sign {
                    ProofDisposition::Proved => {
                        if opaque {
                            evidence.push(CallGoalEvidence::OpaquePositive);
                        }
                        if projection {
                            evidence.push(CallGoalEvidence::ExactL0Projection);
                        }
                        if normalization {
                            evidence.push(CallGoalEvidence::NormalizationPositive);
                        }
                        if introduction {
                            evidence.push(CallGoalEvidence::BooleanIntroductionPositive);
                        }
                    }
                    ProofDisposition::Refuted => {
                        if opaque {
                            evidence.push(CallGoalEvidence::OpaqueNegative);
                        }
                        if projection {
                            evidence.push(CallGoalEvidence::NegatedL0Projection);
                        }
                        if normalization {
                            evidence.push(CallGoalEvidence::NormalizationNegative);
                        }
                        if introduction {
                            evidence.push(CallGoalEvidence::BooleanIntroductionNegative);
                        }
                    }
                    ProofDisposition::Unknown => unreachable!(),
                }
                evidence
            }
            (ProofDisposition::Proved, Some(ProofRoute::Affine)) => {
                vec![CallGoalEvidence::AffinePositive]
            }
            (ProofDisposition::Unknown, None) => Vec::new(),
            _ => unreachable!("a signed proof returned an incompatible route"),
        };
        (disposition, evidence, result.derivation)
    }

    /// Unified deterministic entry for numeric/logical entailment.  The
    /// consumer supplies one normalized proposition; this function tries the
    /// fixed ordinary closure before the fixed affine rule and constructs the
    /// selected derivation during that same query.
    pub(super) fn prove(&mut self, context: ProofContext<'_>, goal: ProofGoal<'_>) -> ProofResult {
        match goal {
            ProofGoal::Affine { inequality, right } => {
                self.prove_affine(context, inequality, right)
            }
            ProofGoal::AutomaticAffine { inequality } => {
                self.prove_affine(context, inequality, None)
            }
            ProofGoal::Signed { expression, affine } => {
                self.prove_signed(context, expression, affine)
            }
            ProofGoal::Ordering { relation, affine } => {
                self.prove_ordering(context, relation, affine)
            }
            ProofGoal::IntegerDomain(goal) => self.prove_integer_domain(context, goal),
            ProofGoal::ConversionDomain {
                canonical,
                operand,
                image,
            } => self.prove_conversion_domain(context, canonical, operand, image),
            ProofGoal::BoundedRelation(goal) => self.prove_bounded_relation(context, goal),
            ProofGoal::NormalizedOrdering {
                goal,
                relation,
                affine,
                right,
                upper_bound,
            } => {
                let proof = self.prove_normalized_ordering(&context, goal, relation, affine, right);
                self.project_numeric_upper_bound(&context, proof, upper_bound)
            }
        }
    }

    pub(super) fn prove_affine(
        &mut self,
        context: ProofContext<'_>,
        inequality: &AffineInequality,
        right: Option<TermId>,
    ) -> ProofResult {
        let closed = context.close(
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        let Some(proof) = self.numeric_affine_proof(inequality, right, context) else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let derivation = self
            .vocabulary
            .derivations
            .intern(DerivationNode::AffineConsequence {
                relation: None,
                premises: proof.premises.into_boxed_slice(),
                parents: proof.parents,
            });
        ProofResult {
            disposition: ProofDisposition::Proved,
            route: Some(ProofRoute::Affine),
            derivation: Some(derivation),
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    pub(super) fn prove_signed(
        &mut self,
        context: ProofContext<'_>,
        expression: &GoalExpression,
        affine_target: Option<&AffineInequality>,
    ) -> ProofResult {
        let goal = self.intern_goal_expression(expression.clone());
        let closed = close(
            context.facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        // Conversion domains give an established negative identity priority
        // over the independent sufficient-range normalization.
        if matches!(
            expression,
            GoalExpression::Operation {
                row: GoalOperation::NumericConversion {
                    mode: CheckedConversionMode::Defined,
                    ..
                },
                ..
            }
        ) && closed.holds_opaque(goal, GoalSign::Negative)
            && !closed.holds_opaque(goal, GoalSign::Positive)
        {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::SignedOrdinary {
                    opaque: true,
                    projection: false,
                    normalization: false,
                    introduction: false,
                }),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let positive_opaque = closed.holds_opaque(goal, GoalSign::Positive);
        let positive_projection = self
            .vocabulary
            .goals
            .projection(goal)
            .is_some_and(|relation| closed.derives(relation));
        let positive_normalization =
            closed.derives_normalized_goal(goal, GoalSign::Positive, &self.vocabulary.goals);
        let positive_introduction = !positive_opaque
            && !positive_projection
            && !positive_normalization
            && closed.derives_goal(goal, GoalSign::Positive, &self.vocabulary.goals);
        if positive_opaque || positive_projection || positive_normalization || positive_introduction
        {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::SignedOrdinary {
                    opaque: positive_opaque,
                    projection: positive_projection,
                    normalization: positive_normalization,
                    introduction: positive_introduction,
                }),
                derivation: closed.goal_proof(
                    goal,
                    GoalSign::Positive,
                    &self.vocabulary.goals,
                    &mut self.vocabulary.derivations,
                ),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let negative_opaque = closed.holds_opaque(goal, GoalSign::Negative);
        let negative_projection = self
            .vocabulary
            .goals
            .projection(goal)
            .is_some_and(|relation| closed.derives(&relation.negated()));
        let negative_normalization =
            closed.derives_normalized_goal(goal, GoalSign::Negative, &self.vocabulary.goals);
        let negative_introduction = !negative_opaque
            && !negative_projection
            && !negative_normalization
            && closed.derives_goal(goal, GoalSign::Negative, &self.vocabulary.goals);
        if negative_opaque || negative_projection || negative_normalization || negative_introduction
        {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::SignedOrdinary {
                    opaque: negative_opaque,
                    projection: negative_projection,
                    normalization: negative_normalization,
                    introduction: negative_introduction,
                }),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        // The affine target is this goal's own comparison, normalized. Proving
        // it proves the goal, so the L0 projection is what the evidence names,
        // not what the route needs: a goal that carries a coefficient has no
        // two-term projection to name and instead retains the exact signed
        // goal above its affine consequence.
        if let Some(target) = affine_target {
            let projection = self.vocabulary.goals.projection(goal).cloned();
            let right = self.signed_goal_right_term(expression, GoalSign::Positive);
            if let Some(proof) = self.numeric_affine_proof(target, right, context) {
                let consequence =
                    self.vocabulary
                        .derivations
                        .intern(DerivationNode::AffineConsequence {
                            relation: projection.clone().map(Box::new),
                            premises: proof.premises.into_boxed_slice(),
                            parents: proof.parents,
                        });
                let derivation =
                    match projection {
                        Some(relation) => {
                            self.vocabulary
                                .derivations
                                .intern(DerivationNode::GoalProjection {
                                    goal,
                                    sign: GoalSign::Positive,
                                    relation,
                                    parent: consequence,
                                })
                        }
                        None => self.vocabulary.derivations.intern(
                            DerivationNode::GoalAffineConsequence {
                                goal,
                                sign: GoalSign::Positive,
                                parent: consequence,
                            },
                        ),
                    };
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let derivation = self.signed_goal_affine_proof(
            context,
            expression,
            GoalSign::Positive,
            &closed,
            &mut HashSet::new(),
        );
        ProofResult {
            disposition: if derivation.is_some() {
                ProofDisposition::Proved
            } else {
                ProofDisposition::Unknown
            },
            route: derivation.map(|_| ProofRoute::Affine),
            derivation,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Proves one signed ordering leaf through the ordinary affine entry and
    /// projects the result back to the exact interned Goal. This is the leaf
    /// case used by the fixed Boolean recursion below.
    pub(super) fn affine_signed_goal_leaf_proof(
        &mut self,
        context: ProofContext<'_>,
        expression: &GoalExpression,
        goal: GoalId,
        sign: GoalSign,
    ) -> Option<DerivationId> {
        let target = self.affine_signed_goal_ordering_target(expression, context.affine, sign)?;
        let mut relation = self.vocabulary.goals.projection(goal).cloned();
        if sign == GoalSign::Negative {
            relation = relation.map(|relation| relation.negated());
        }
        let right = self.signed_goal_right_term(expression, sign);
        let proof = self.numeric_affine_proof(&target, right, context)?;
        let consequence = self
            .vocabulary
            .derivations
            .intern(DerivationNode::AffineConsequence {
                relation: relation.clone().map(Box::new),
                premises: proof.premises.into_boxed_slice(),
                parents: proof.parents,
            });
        Some(self.vocabulary.derivations.intern(match relation {
            Some(relation) => DerivationNode::GoalProjection {
                goal,
                sign,
                relation,
                parent: consequence,
            },
            None => DerivationNode::GoalAffineConsequence {
                goal,
                sign,
                parent: consequence,
            },
        }))
    }

    /// Extends the existing finite Boolean introduction rule with affine
    /// ordering leaves. The recursion follows the closed truth table exactly:
    /// conjunction requires every positive child, disjunction every negative
    /// child, the opposite signs require one witness, and `not` flips sign.
    /// It performs no premise, coefficient, or path search.
    pub(super) fn signed_goal_affine_proof(
        &mut self,
        context: ProofContext<'_>,
        expression: &GoalExpression,
        sign: GoalSign,
        closed: &ClosedState,
        visiting: &mut HashSet<(GoalId, GoalSign)>,
    ) -> Option<DerivationId> {
        let goal = self.intern_goal_expression(expression.clone());
        if let Some(proof) = closed.goal_proof(
            goal,
            sign,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        ) {
            return Some(proof);
        }
        if !visiting.insert((goal, sign)) {
            return None;
        }

        let proof = match expression {
            GoalExpression::Operation {
                row:
                    GoalOperation::NumericConversion {
                        mode: CheckedConversionMode::Defined,
                        ..
                    },
                ..
            } if sign == GoalSign::Positive => {
                self.conversion_goal_bound_proof(context, expression, goal)
            }
            GoalExpression::Operation {
                row: GoalOperation::Boolean(operation),
                arguments,
                ..
            } => {
                let child_sign = match (operation, sign) {
                    (CheckedBooleanOperation::And, GoalSign::Positive)
                    | (CheckedBooleanOperation::Or, GoalSign::Positive) => GoalSign::Positive,
                    (CheckedBooleanOperation::And, GoalSign::Negative)
                    | (CheckedBooleanOperation::Or, GoalSign::Negative) => GoalSign::Negative,
                    (CheckedBooleanOperation::Not, GoalSign::Positive) => GoalSign::Negative,
                    (CheckedBooleanOperation::Not, GoalSign::Negative) => GoalSign::Positive,
                    (CheckedBooleanOperation::ExclusiveOr, _) => {
                        visiting.remove(&(goal, sign));
                        return None;
                    }
                };
                let requires_all = matches!(
                    (operation, sign),
                    (CheckedBooleanOperation::And, GoalSign::Positive)
                        | (CheckedBooleanOperation::Or, GoalSign::Negative)
                        | (CheckedBooleanOperation::Not, _)
                );
                let parents = if requires_all {
                    let mut parents = Vec::with_capacity(arguments.len());
                    let mut complete = true;
                    for argument in arguments {
                        let Some(parent) = self.signed_goal_affine_proof(
                            context, argument, child_sign, closed, visiting,
                        ) else {
                            complete = false;
                            break;
                        };
                        parents.push(parent);
                    }
                    complete.then_some(parents)
                } else {
                    let mut best = None;
                    for argument in arguments {
                        let Some(candidate) = self.signed_goal_affine_proof(
                            context, argument, child_sign, closed, visiting,
                        ) else {
                            continue;
                        };
                        // Existential Boolean introductions use the first
                        // successful child in source order. Later witnesses
                        // cannot change acceptance, only diagnostics.
                        if best.is_none() {
                            best = Some(candidate);
                        }
                    }
                    best.map(|parent| vec![parent])
                };
                parents.map(|parents| {
                    self.vocabulary
                        .derivations
                        .intern(DerivationNode::BooleanIntroduction {
                            goal,
                            sign,
                            parents,
                        })
                })
            }
            GoalExpression::Operation { .. } => {
                self.affine_signed_goal_leaf_proof(context, expression, goal, sign)
            }
            GoalExpression::Datum(_) => None,
        };
        visiting.remove(&(goal, sign));
        proof
    }

    pub(super) fn prove_ordering(
        &mut self,
        context: ProofContext<'_>,
        relation: &Relation,
        affine_target: Option<&[AffineInequality]>,
    ) -> ProofResult {
        let closed = context.close(
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if closed.derives(relation) {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::L0),
                derivation: Some(
                    closed
                        .relation_proof(relation, &mut self.vocabulary.derivations)
                        .expect("a proved L0 relation must retain its local derivation"),
                ),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if closed.derives(&relation.negated()) {
            return ProofResult {
                disposition: ProofDisposition::Refuted,
                route: Some(ProofRoute::L0),
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        let Some(targets) = affine_target else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let mut premises = Vec::new();
        let mut parents = Vec::new();
        for (ordinal, target) in targets.iter().enumerate() {
            let right = match relation {
                Relation::Bound { right, .. } if ordinal == 0 => Some(*right),
                Relation::Equal { left, right, .. } => match ordinal {
                    0 => Some(*right),
                    1 => Some(*left),
                    _ => None,
                },
                _ => None,
            };
            let Some(proof) = self.numeric_affine_proof(target, right, context) else {
                return ProofResult {
                    disposition: ProofDisposition::Unknown,
                    route: None,
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            };
            premises.extend(proof.premises);
            parents.extend(proof.parents);
        }
        let derivation = self
            .vocabulary
            .derivations
            .intern(DerivationNode::AffineConsequence {
                relation: None,
                premises: premises.into_boxed_slice(),
                parents,
            });
        ProofResult {
            disposition: ProofDisposition::Proved,
            route: Some(ProofRoute::Affine),
            derivation: Some(derivation),
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    pub(super) fn prove_bounded_relation(
        &mut self,
        context: ProofContext<'_>,
        goal: BoundedRelationGoal<'_>,
    ) -> ProofResult {
        let canonical = goal
            .canonical
            .map(|expression| self.intern_goal_expression(expression.clone()));
        let closed = close(
            context.facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if let Some(canonical) = canonical {
            if closed.holds_opaque(canonical, GoalSign::Positive) {
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: closed.opaque_proof(canonical, GoalSign::Positive),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.holds_opaque(canonical, GoalSign::Negative) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let relation = goal.request.as_ref().and_then(request_relation);
        if let Some(relation) = relation.as_ref() {
            if closed.derives(relation) {
                let parent = closed
                    .relation_proof(relation, &mut self.vocabulary.derivations)
                    .expect("a proved L0 relation must retain its local derivation");
                let derivation =
                    self.vocabulary
                        .goal_numeric_derivation(canonical, Some(relation), parent);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::L0),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.derives(&relation.negated()) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::L0),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        if let Some(target) = goal.direct_affine {
            let assumptions = affine_facts(context.affine);
            if let Some(proof) = self.affine_target_proof(target, &assumptions, context) {
                let parent =
                    self.vocabulary
                        .derivations
                        .intern(DerivationNode::AffineConsequence {
                            relation: relation.clone().map(Box::new),
                            premises: proof.premises.into_boxed_slice(),
                            parents: proof.parents,
                        });
                let derivation =
                    self.vocabulary
                        .goal_numeric_derivation(canonical, relation.as_ref(), parent);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        if let Some(request) = goal.request
            && let Some(left) = request.left
        {
            if let Some(derivation) = goal.fixed_affine_bridge.and_then(|bridge| {
                self.fixed_affine_bound_derivation(
                    bridge,
                    left,
                    request.right,
                    request.bound,
                    context.affine,
                    context.facts,
                )
            }) {
                let derivation = self.vocabulary.goal_numeric_derivation(
                    canonical,
                    relation.as_ref(),
                    derivation,
                );
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }

            if let Some(derivation) = goal.affine_left.and_then(|affine_left| {
                self.affine_bound_via_l0_right(
                    affine_left,
                    left,
                    request.right,
                    request.bound,
                    context.affine,
                    context.facts,
                )
            }) {
                let derivation = self.vocabulary.goal_numeric_derivation(
                    canonical,
                    relation.as_ref(),
                    derivation,
                );
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::Affine),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        ProofResult {
            disposition: ProofDisposition::Unknown,
            route: None,
            derivation: None,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Proves one canonical ordering that may also be represented by a finite
    /// normalized goal. The goal identity has ordinary priority when present;
    /// the bare L0 relation is the fallback only for source operands outside
    /// that goal fragment. The affine route proves the same written relation
    /// and, when needed, concludes the exact goal normalization in the same
    /// call.
    pub(super) fn prove_normalized_ordering(
        &mut self,
        context: &ProofContext<'_>,
        goal: Option<GoalId>,
        relation: Option<&Relation>,
        affine_target: Option<&AffineInequality>,
        right: Option<TermId>,
    ) -> ProofResult {
        let closed = close(
            context.facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if closed.contradictory() {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Contradiction),
                derivation: closed.contradiction_proof(),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }
        if let Some(goal) = goal {
            if closed.holds_opaque(goal, GoalSign::Positive) {
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: closed.opaque_proof(goal, GoalSign::Positive),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.holds_opaque(goal, GoalSign::Negative) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::FiniteGoal),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }
        if let Some(relation) = relation {
            if closed.derives(relation) {
                let parent = closed
                    .relation_proof(relation, &mut self.vocabulary.derivations)
                    .expect("a proved L0 relation must retain its local derivation");
                let derivation =
                    self.vocabulary
                        .goal_numeric_derivation(goal, Some(relation), parent);
                return ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::L0),
                    derivation: Some(derivation),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
            if closed.derives(&relation.negated()) {
                return ProofResult {
                    disposition: ProofDisposition::Refuted,
                    route: Some(ProofRoute::L0),
                    derivation: None,
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }

        let Some(target) = affine_target else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let Some(proof) = self.numeric_affine_proof(target, right, *context) else {
            return ProofResult {
                disposition: ProofDisposition::Unknown,
                route: None,
                derivation: None,
                numeric_upper_bound: None,
                product_interval: None,
            };
        };
        let consequence = self
            .vocabulary
            .derivations
            .intern(DerivationNode::AffineConsequence {
                relation: relation.cloned().map(Box::new),
                premises: proof.premises.into_boxed_slice(),
                parents: proof.parents,
            });
        let derivation = self
            .vocabulary
            .goal_numeric_derivation(goal, relation, consequence);
        ProofResult {
            disposition: ProofDisposition::Proved,
            route: Some(ProofRoute::Affine),
            derivation: Some(derivation),
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Projects the tightest numeric ceiling available from the same proof
    /// context after, and only after, the normalized ordering was proved.
    /// This is not another admission query: the selected `ProofResult` remains
    /// the sole acceptance authority. The projection merely chooses between
    /// the ordering's own admitted ceiling, the ordinary closure, and the
    /// fixed affine interval rule, retaining the derivation for the chosen
    /// number.
    pub(super) fn project_numeric_upper_bound(
        &mut self,
        context: &ProofContext<'_>,
        mut proof: ProofResult,
        request: Option<NumericUpperBoundRequest<'_>>,
    ) -> ProofResult {
        let Some(request) = request else {
            return proof;
        };
        if proof.disposition != ProofDisposition::Proved {
            return proof;
        }
        let Some(admission_derivation) = proof.derivation else {
            return proof;
        };
        if proof.route == Some(ProofRoute::Contradiction) {
            proof.numeric_upper_bound = Some(ProvedNumericUpperBound {
                value: 0,
                derivation: admission_derivation,
            });
            return proof;
        }

        let mut selected = ProvedNumericUpperBound {
            value: request.admitted,
            derivation: admission_derivation,
        };

        if let Some(term) = request.term {
            let closed = close(
                context.facts,
                &self.vocabulary.terms,
                &self.vocabulary.goals,
                &mut self.vocabulary.derivations,
            );
            if let Some(candidate) = closed.tight_bound(term, ZERO)
                && candidate < selected.value
                && let Some(derivation) =
                    closed.bound_proof(term, ZERO, candidate, &mut self.vocabulary.derivations)
            {
                selected = ProvedNumericUpperBound {
                    value: candidate,
                    derivation,
                };
            }
        }

        if let Some(form) = request.affine {
            let assumptions = affine_facts(context.affine);
            if let Some(endpoint) = self
                .affine_closed_interval_proof(form, &assumptions, context.affine, context.facts)
                .map(|interval| interval.maximum)
                && endpoint.value < selected.value
            {
                let relation = request.term.map(|left| {
                    Box::new(Relation::Bound {
                        left,
                        right: ZERO,
                        bound: endpoint.value,
                    })
                });
                let derivation =
                    self.vocabulary
                        .derivations
                        .intern(DerivationNode::AffineConsequence {
                            relation,
                            premises: endpoint.consequence.premises.into_boxed_slice(),
                            parents: endpoint.consequence.parents,
                        });
                selected = ProvedNumericUpperBound {
                    value: endpoint.value,
                    derivation,
                };
            }
        }

        proof.numeric_upper_bound = Some(selected);
        proof
    }

    /// Interns one measure term over a place spelled in the compact
    /// [`ResolvedPlace`] form, with [MSR-2]'s standing facts.
    pub(super) fn place_measure_term(
        &mut self,
        measure: CheckedMeasure,
        base: ResolvedPlace,
        measured: MeasuredKind,
        array_length: Option<CheckedConst>,
    ) -> TermId {
        self.measure_term(measure, base, measured, array_length)
    }

    /// Forms the exact `offset <= N - 1` target for a fixed-size array.
    /// Dynamic buffer and slice lengths remain on the ordinary L0 route until
    /// their length term is connected to an affine value by a fixed rule.
    pub(super) fn affine_fixed_array_index_target(
        &mut self,
        offset: &CheckedExpression,
        array_length: Option<CheckedConst>,
        state: &AffineFlowState,
    ) -> Option<AffineInequality> {
        let CheckedConst::Value(length) = array_length? else {
            return None;
        };
        let offset = self.input.direct_goal_expression(offset)?;
        let left = self.affine_goal_value(&offset, state)?;
        let right = AffineForm::constant(i128::from(length).checked_sub(1)?);
        AffineInequality::from_forms(&left, &right, &mut AffineCheckState::new()).ok()
    }

    pub(super) fn affine_consequence_derivation(
        &mut self,
        target: &AffineInequality,
        relation: Option<Relation>,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<DerivationId> {
        let assumptions = affine_facts(affine);
        let proof =
            self.affine_target_proof(target, &assumptions, ProofContext::new(facts, affine))?;
        Some(
            self.vocabulary
                .derivations
                .intern(DerivationNode::AffineConsequence {
                    relation: relation.map(Box::new),
                    premises: proof.premises.into_boxed_slice(),
                    parents: proof.parents,
                }),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fixed_affine_bound_derivation(
        &mut self,
        bridge: FixedAffineBoundBridge<'_>,
        left: TermId,
        right: TermId,
        requested: i128,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<DerivationId> {
        let remaining = requested.checked_sub(bridge.left_to_middle_bound)?;
        let first_relation = Relation::Bound {
            left,
            right: bridge.middle,
            bound: bridge.left_to_middle_bound,
        };
        let first =
            self.affine_consequence_derivation(bridge.target, Some(first_relation), affine, facts)?;
        let closed = close(
            facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        let second = closed.bound_proof(
            bridge.middle,
            right,
            remaining,
            &mut self.vocabulary.derivations,
        )?;
        Some(
            self.vocabulary
                .derivations
                .intern(DerivationNode::TransitiveBound {
                    left,
                    middle: bridge.middle,
                    right,
                    bound: requested,
                    first,
                    second,
                }),
        )
    }

    /// The complete Step 6 inventory. Querying it must not recreate a measure
    /// image removed by an intervening write. Fixed measures and aliases use
    /// the same immutable anchor as ordinary measure reads.
    pub(super) fn affine_right_bridge_candidates(
        &mut self,
        affine: &AffineFlowState,
    ) -> Vec<AffineL0Candidate> {
        let mut candidates = Vec::new();
        for term in self.vocabulary.measure_terms() {
            let mut anchor = term;
            let mut fixed = None;
            for _ in 0..4 {
                match self.vocabulary.terms.measure_bound(anchor) {
                    Some(MeasureBound::Constant(value)) => {
                        fixed = Some(AffineForm::constant(value));
                        break;
                    }
                    Some(MeasureBound::Equal(other)) => anchor = other,
                    None => break,
                }
            }
            if let Some(value) =
                fixed.or_else(|| affine.measure_atoms.borrow().get(&anchor).cloned())
            {
                candidates.push(AffineL0Candidate { term, value });
            }
        }
        let mut bindings = affine.values.keys().copied().collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.0);
        for binding in bindings {
            let Some(ty) = self.input.affine_binding_type(binding) else {
                continue;
            };
            let term = self.vocabulary.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                ty,
            ));
            candidates.push(AffineL0Candidate {
                term,
                value: affine.values[&binding].clone(),
            });
        }
        candidates
    }

    /// The shared affine portion of MSR-4. AUTO remains nonrecursive: after
    /// it fails, Step 6 subtracts exactly one closed `m-r <= c` image and
    /// submits exactly that residual to AUTO. The normalization supplies r;
    /// a normalized coefficient vector never chooses a new right operand.
    pub(super) fn numeric_affine_proof(
        &mut self,
        target: &AffineInequality,
        right: Option<TermId>,
        context: ProofContext<'_>,
    ) -> Option<AffineConsequenceProof> {
        let assumptions = affine_facts(context.affine);
        if let Some(proof) = self.affine_target_proof(target, &assumptions, context) {
            return Some(proof);
        }
        let right = right?;
        let right_value = self.vocabulary.affine_term_value(right, context.affine)?;
        let candidates = self.affine_right_bridge_candidates(context.affine);
        let closed = context.close(
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        for candidate in candidates {
            let Some(bound) = closed.tight_bound(candidate.term, right) else {
                continue;
            };
            let mut check = AffineCheckState::new();
            let Ok(image) = AffineInequality::from_bounded_forms(
                &candidate.value,
                &right_value,
                bound,
                &mut check,
            ) else {
                continue;
            };
            let Ok(residual) = AffineInequality::residual_after(target, &image, &mut check) else {
                continue;
            };
            let Some(mut proof) = self.affine_target_proof(&residual, &assumptions, context) else {
                continue;
            };
            let Some(bridge) = closed.bound_proof(
                candidate.term,
                right,
                bound,
                &mut self.vocabulary.derivations,
            ) else {
                continue;
            };
            proof.parents.push(bridge);
            return Some(proof);
        }
        None
    }

    /// Combines one affine left-hand value with one already-live L0 bridge to
    /// the requested right-hand term. For a candidate middle term `m`, L0
    /// fixes `m - right <= c`; the single affine target is therefore
    /// `left - m <= requested - c`. It uses the same complete Step 6 inventory
    /// as the general numeric entry and retains an exact transitive L0 node.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn affine_bound_via_l0_right(
        &mut self,
        left: &AffineForm,
        left_term: TermId,
        right_term: TermId,
        requested: i128,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<DerivationId> {
        let candidates = self.affine_right_bridge_candidates(affine);
        let closed = close(
            facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if closed.contradictory() {
            return closed.contradiction_proof();
        }
        for AffineL0Candidate {
            term: middle,
            value: middle_value,
        } in candidates
        {
            let Some(bridge) = closed.tight_bound(middle, right_term) else {
                continue;
            };
            let Some(affine_bound) = requested.checked_sub(bridge) else {
                continue;
            };
            let Ok(right) = middle_value.add(
                &AffineForm::constant(affine_bound),
                &mut AffineCheckState::new(),
            ) else {
                continue;
            };
            let Ok(target) =
                AffineInequality::from_forms(left, &right, &mut AffineCheckState::new())
            else {
                continue;
            };
            let relation = Relation::Bound {
                left: left_term,
                right: middle,
                bound: affine_bound,
            };
            let Some(first) =
                self.affine_consequence_derivation(&target, Some(relation), affine, facts)
            else {
                continue;
            };
            let Some(second) =
                closed.bound_proof(middle, right_term, bridge, &mut self.vocabulary.derivations)
            else {
                continue;
            };
            return Some(
                self.vocabulary
                    .derivations
                    .intern(DerivationNode::TransitiveBound {
                        left: left_term,
                        middle,
                        right: right_term,
                        bound: requested,
                        first,
                        second,
                    }),
            );
        }
        None
    }

    pub(super) fn captured_index_term(&mut self, value: CapturedValue) -> Option<TermId> {
        match value.term {
            CapturedTerm::Literal(value) => Some(
                self.vocabulary
                    .terms
                    .intern(TermKind::Constant(i128::from(value))),
            ),
            CapturedTerm::Const(declaration) => Some(self.const_parameter_term(declaration)),
            // A superseded binding's capture still names the value its
            // formation read, which is the immutable term the capture minted.
            CapturedTerm::Binding(_) | CapturedTerm::Superseded(_)
                if matches!(value.capture, CaptureId::Source(_)) =>
            {
                self.vocabulary.terms.interned(&TermKind::IndexCapture {
                    capture: value.capture,
                })
            }
            CapturedTerm::Binding(_) | CapturedTerm::Superseded(_) | CapturedTerm::Opaque => None,
        }
    }

    pub(super) fn prove_index_separation(
        &mut self,
        left: CapturedValue,
        right: CapturedValue,
        state: &ProofFlowState,
    ) -> Option<ProofResult> {
        let left_term = self.captured_index_term(left)?;
        let right_term = self.captured_index_term(right)?;
        let relation = if left_term <= right_term {
            Relation::Distinct {
                left: left_term,
                right: right_term,
                difference: 0,
            }
        } else {
            Relation::Distinct {
                left: right_term,
                right: left_term,
                difference: 0,
            }
        };
        let left_image = state
            .affine
            .indices
            .get(&left.capture)
            .cloned()
            .or_else(|| self.vocabulary.affine_term_value(left_term, &state.affine));
        let right_image = state
            .affine
            .indices
            .get(&right.capture)
            .cloned()
            .or_else(|| self.vocabulary.affine_term_value(right_term, &state.affine));
        let mut inequalities = Vec::new();
        if let Some((left_image, right_image)) = left_image.as_ref().zip(right_image.as_ref()) {
            for (lower, upper) in [(left_image, right_image), (right_image, left_image)] {
                if let Ok(inequality) = AffineInequality::from_bounded_forms(
                    lower,
                    upper,
                    -1,
                    &mut AffineCheckState::new(),
                ) {
                    inequalities.push(inequality);
                }
            }
        }
        let mut substitution = None;
        let mut proof = self.prove(
            ProofContext::new(&state.facts, &state.affine),
            ProofGoal::Ordering {
                relation: &relation,
                affine: None,
            },
        );
        if proof.disposition != ProofDisposition::Proved
            && let (CapturedTerm::Binding(left_binding), CapturedTerm::Binding(right_binding)) =
                (left.term, right.term)
        {
            let source_left = self.vocabulary.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(left_binding), false, Vec::new()),
                super::super::super::model::IntegerType::U64,
            ));
            let source_right = self.vocabulary.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(right_binding), false, Vec::new()),
                super::super::super::model::IntegerType::U64,
            ));
            let source_relation = if source_left <= source_right {
                Relation::Distinct {
                    left: source_left,
                    right: source_right,
                    difference: 0,
                }
            } else {
                Relation::Distinct {
                    left: source_right,
                    right: source_left,
                    difference: 0,
                }
            };
            let left_identity = Relation::Equal {
                left: left_term,
                right: source_left,
                difference: 0,
            };
            let right_identity = Relation::Equal {
                left: right_term,
                right: source_right,
                difference: 0,
            };
            let closed = ProofContext::new(&state.facts, &state.affine).close(
                &self.vocabulary.terms,
                &self.vocabulary.goals,
                &mut self.vocabulary.derivations,
            );
            if closed.derives(&source_relation)
                && closed.derives(&left_identity)
                && closed.derives(&right_identity)
            {
                let parent = closed
                    .relation_proof(&source_relation, &mut self.vocabulary.derivations)
                    .expect("a proved source disequality retains its proof");
                let left_identity = closed
                    .relation_proof(&left_identity, &mut self.vocabulary.derivations)
                    .expect("a live left capture identity retains its proof");
                let right_identity = closed
                    .relation_proof(&right_identity, &mut self.vocabulary.derivations)
                    .expect("a live right capture identity retains its proof");
                substitution = Some(Box::new(IndexCaptureSubstitution {
                    source_left,
                    source_right,
                    left_identity,
                    right_identity,
                }));
                proof = ProofResult {
                    disposition: ProofDisposition::Proved,
                    route: Some(ProofRoute::L0),
                    derivation: Some(parent),
                    numeric_upper_bound: None,
                    product_interval: None,
                };
            }
        }
        let mut affine_target = None;
        let mut affine_images = None;
        if proof.disposition != ProofDisposition::Proved {
            for inequality in &inequalities {
                proof = self.prove(
                    ProofContext::new(&state.facts, &state.affine),
                    ProofGoal::Ordering {
                        relation: &relation,
                        affine: Some(std::slice::from_ref(inequality)),
                    },
                );
                if proof.disposition == ProofDisposition::Proved {
                    if proof.route == Some(ProofRoute::Affine) {
                        affine_target = Some(Box::new(inequality.clone()));
                        affine_images = Some(Box::new((
                            left_image
                                .clone()
                                .expect("an affine candidate has a left image"),
                            right_image
                                .clone()
                                .expect("an affine candidate has a right image"),
                        )));
                    }
                    break;
                }
            }
        }
        if proof.disposition != ProofDisposition::Proved {
            return None;
        }
        let parent = proof
            .derivation
            .expect("a proved index separation retains its L0 or affine parent");
        proof.derivation = Some(self.vocabulary.derivations.intern(
            DerivationNode::IndexSeparation {
                detail: Box::new(IndexSeparationDetail {
                    left,
                    right,
                    parent,
                    affine_target,
                    affine_images,
                    substitution,
                }),
            },
        ));
        Some(proof)
    }

    /// [OWN-7] the proof `affine` gives that two ranges under containing
    /// paths that are identical step for step or differ only in index steps
    /// are disjoint, over the images their formations filed or, for a range a
    /// row takes from a call's arguments, its endpoints' own.
    pub(super) fn prove_range_separation(
        &mut self,
        left: CapturedRange,
        right: CapturedRange,
        facts: &FactState,
        affine: &AffineFlowState,
    ) -> Option<ProofResult> {
        let (left_start, left_end) = self.range_endpoint_images(left, affine)?;
        let (right_start, right_end) = self.range_endpoint_images(right, affine)?;
        for (ordering, end, start) in [
            (
                RangeSeparationOrdering::LeftBeforeRight,
                &left_end,
                &right_start,
            ),
            (
                RangeSeparationOrdering::RightBeforeLeft,
                &right_end,
                &left_start,
            ),
            (RangeSeparationOrdering::LeftEmpty, &left_end, &left_start),
            (
                RangeSeparationOrdering::RightEmpty,
                &right_end,
                &right_start,
            ),
        ] {
            let Ok(inequality) =
                AffineInequality::from_forms(end, start, &mut AffineCheckState::new())
            else {
                continue;
            };
            let mut proof = self.prove(
                ProofContext::new(facts, affine),
                ProofGoal::Affine {
                    inequality: &inequality,
                    right: None,
                },
            );
            if proof.disposition == ProofDisposition::Proved {
                let parent = proof
                    .derivation
                    .expect("a proved range ordering retains its affine or contradiction parent");
                proof.derivation = Some(self.vocabulary.derivations.intern(
                    DerivationNode::RangeSeparation {
                        detail: Box::new(RangeSeparationDetail {
                            left,
                            right,
                            ordering,
                            parent,
                        }),
                    },
                ));
                return Some(proof);
            }
        }
        None
    }

    /// The affine image of one captured index or endpoint: its own capture's
    /// image where the call or formation filed one, else the value its term
    /// has on this edge.
    fn captured_value_image(
        &mut self,
        value: CapturedValue,
        affine: &AffineFlowState,
    ) -> Option<AffineForm> {
        if let Some(image) = affine.indices.get(&value.capture) {
            return Some(image.clone());
        }
        let term = self.captured_index_term(value)?;
        self.vocabulary.affine_term_value(term, affine)
    }

    /// A range's two endpoint images: the ones its formation filed [REF-4],
    /// or each endpoint's own, for a range a row takes from the call's
    /// arguments [EFF-5].
    fn range_endpoint_images(
        &mut self,
        range: CapturedRange,
        affine: &AffineFlowState,
    ) -> Option<(AffineForm, AffineForm)> {
        if let Some(image) = affine.ranges.get(&range.start.capture) {
            return Some((image.start.clone(), image.end.clone()));
        }
        Some((
            self.captured_value_image(range.start, affine)?,
            self.captured_value_image(range.end, affine)?,
        ))
    }

    /// The first of `inequalities` the facts on this edge prove.
    fn prove_first_affine(
        &mut self,
        inequalities: impl IntoIterator<Item = Result<AffineInequality, AffineCheckError>>,
        state: &ProofFlowState,
    ) -> Option<ProofResult> {
        inequalities.into_iter().flatten().find_map(|inequality| {
            let proof = self.prove(
                ProofContext::new(&state.facts, &state.affine),
                ProofGoal::Affine {
                    inequality: &inequality,
                    right: None,
                },
            );
            (proof.disposition == ProofDisposition::Proved).then_some(proof)
        })
    }

    /// [OWN-7] the proof `state` gives that an index lies outside a range of
    /// the same base: `index < start`, `end <= index`, or `end <= start`.
    pub(super) fn prove_index_outside_range(
        &mut self,
        index: CapturedValue,
        range: CapturedRange,
        state: &ProofFlowState,
    ) -> Option<ProofResult> {
        let index = self.captured_value_image(index, &state.affine)?;
        let (start, end) = self.range_endpoint_images(range, &state.affine)?;
        self.prove_first_affine(
            [
                AffineInequality::from_bounded_forms(
                    &index,
                    &start,
                    -1,
                    &mut AffineCheckState::new(),
                ),
                AffineInequality::from_forms(&end, &index, &mut AffineCheckState::new()),
                AffineInequality::from_forms(&end, &start, &mut AffineCheckState::new()),
            ],
            state,
        )
    }

    /// [WIN-2] the proof `state` gives that a range of `window` ends at or
    /// below the window's length, `end <= len(window)`, or is empty.
    pub(super) fn range_within_length_proof(
        &mut self,
        window: &ResolvedPlace,
        range: CapturedRange,
        state: &ProofFlowState,
    ) -> Option<ProofResult> {
        let (start, end) = self.range_endpoint_images(range, &state.affine)?;
        let length = self
            .vocabulary
            .terms
            .interned(&TermKind::Measure(CheckedMeasure::Length, window.clone()))
            .map(|length| self.vocabulary.measure_atom(length, &state.affine));
        let within = length.map(|length| {
            AffineInequality::from_forms(&end, &length, &mut AffineCheckState::new())
        });
        self.prove_first_affine(
            within.into_iter().chain([AffineInequality::from_forms(
                &end,
                &start,
                &mut AffineCheckState::new(),
            )]),
            state,
        )
    }

    /// One [PAR-1] range question in the state before its first statement.
    ///
    /// A range bound before that statement has the image its formation
    /// published. A range one of the pair's calls forms as an actual has
    /// none yet, because its formation runs with its call; it names the same
    /// storage as that range bound immediately before the first statement
    /// [REF-4], so its endpoints are evaluated here, in a copy of this state,
    /// into exactly the image such a binding would publish. The planner lists
    /// a later statement's formation only when nothing before it writes what
    /// its endpoints read, so these are the values that formation evaluates.
    /// The copy keeps the atoms this evaluation mints out of the ordinary
    /// walk and replaces any image an earlier evaluation left under the same
    /// capture.
    pub(super) fn prove_permission_separation(
        &mut self,
        query: &PermissionSeparationQuery,
        state: &ProofFlowState,
    ) -> Option<ProofResult> {
        if query.formations.is_empty() {
            return self.prove_range_separation(
                query.left,
                query.right,
                &state.facts,
                &state.affine,
            );
        }
        let mut affine = state.affine.clone();
        for formation in &query.formations {
            let capture = formation.captured.start.capture;
            match self.range_formation_image(
                &formation.carrier,
                &formation.start,
                &formation.end,
                &mut affine,
            ) {
                Some(image) => affine.ranges.insert(capture, image),
                None => affine.ranges.remove(&capture),
            };
        }
        self.prove_range_separation(query.left, query.right, &state.facts, &affine)
    }

    pub(super) fn prove_integer_domain(
        &mut self,
        context: ProofContext<'_>,
        goal: IntegerDomainGoal<'_>,
    ) -> ProofResult {
        let finite = self.vocabulary.prove_integer_domain_finite(context, &goal);
        if finite.disposition != ProofDisposition::Unknown {
            return finite;
        }

        if let Some(derivation) = goal.affine_clauses.and_then(|clauses| {
            self.affine_integer_domain_derivation(
                clauses,
                context.affine,
                context.facts,
                goal.canonical,
            )
        }) {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Affine),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                product_interval: None,
            };
        }

        if let Some((derivation, interval)) = goal.affine_product.and_then(|product| {
            self.affine_integer_product_derivation(
                product,
                context.affine,
                context.facts,
                goal.canonical,
            )
        }) {
            return ProofResult {
                disposition: ProofDisposition::Proved,
                route: Some(ProofRoute::Affine),
                derivation: Some(derivation),
                numeric_upper_bound: None,
                // [ENT-3.S7]'s multiplication row establishes this interval on
                // whatever value the multiplication binds. It travels with the
                // judgment because only this route proved it: a domain
                // discharged by the finite L0 or affine-clause route leaves
                // that row to read the closed operand intervals instead.
                product_interval: Some(interval),
            };
        }

        ProofResult {
            disposition: ProofDisposition::Unknown,
            route: None,
            derivation: None,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// Builds and proves the fixed affine range normalization of one exact
    /// integer operation from its already-evaluated operands.  This function
    /// never asks for the current operation's result image: using that image's
    /// result type here would circularly assume the domain being checked.
    pub(super) fn affine_integer_domain_derivation(
        &mut self,
        clauses: &[Vec<NumericAffineTarget>],
        affine: &AffineFlowState,
        facts: &FactState,
        goal: Option<GoalId>,
    ) -> Option<DerivationId> {
        for clause in clauses {
            let mut consequences = Vec::with_capacity(clause.len());
            let mut proved = true;
            for target in clause {
                let Some(proof) = self.numeric_affine_proof(
                    &target.inequality,
                    target.right,
                    ProofContext::new(facts, affine),
                ) else {
                    proved = false;
                    break;
                };
                consequences.push(self.vocabulary.derivations.intern(
                    DerivationNode::AffineConsequence {
                        relation: None,
                        premises: proof.premises.into_boxed_slice(),
                        parents: proof.parents,
                    },
                ));
            }
            if proved {
                return Some(
                    self.vocabulary
                        .derivations
                        .intern(DerivationNode::IntegerDomain {
                            goal,
                            parents: consequences,
                        }),
                );
            }
        }
        None
    }

    /// Selects the one nonlinear integer-domain rule. Both operands must be
    /// genuine affine values: constant multiplication stays on the ordinary
    /// affine path above, while every other nonlinear expression remains
    /// unavailable to this checker.
    pub(super) fn affine_integer_product(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        state: &mut AffineFlowState,
    ) -> Option<AffineIntegerProduct> {
        if !matches!(
            operation,
            CheckedIntegerOperation::MultiplyExact | CheckedIntegerOperation::MultiplyDefined
        ) {
            return None;
        }
        let CheckedType::Integer(ty) = operand_type else {
            return None;
        };
        let [left, right] = arguments else {
            return None;
        };
        let left = self.affine_pre_domain_form(left, state)?;
        let right = self.affine_pre_domain_form(right, state)?;
        if left.terms().is_empty() || right.terms().is_empty() {
            return None;
        }
        Some(AffineIntegerProduct { left, right, ty })
    }

    /// Applies the fixed interval-product rule. Once independent inclusive
    /// intervals are proved for the two affine operands, the product's extrema
    /// occur among exactly four endpoint pairs. All four products are formed
    /// with checked `i128` arithmetic before any range decision is made.
    pub(super) fn affine_integer_product_derivation(
        &mut self,
        product: &AffineIntegerProduct,
        affine: &AffineFlowState,
        facts: &FactState,
        goal: Option<GoalId>,
    ) -> Option<(DerivationId, AffineProductInterval)> {
        let interval = self.affine_integer_product_interval(product, affine, facts)?;
        let derivation = self
            .vocabulary
            .derivations
            .intern(DerivationNode::IntegerDomain {
                goal,
                parents: interval.consequences.to_vec(),
            });
        Some((derivation, interval))
    }

    /// The one measurement the fixed interval-product rule performs. The four
    /// endpoint products decide [ENT-6]'s domain admission and bound the
    /// interval [ENT-3.S7]'s multiplication row publishes, so both read this
    /// result rather than proving the same endpoints twice: the admitted
    /// range and the published bound then cannot disagree by construction.
    pub(super) fn affine_integer_product_interval(
        &mut self,
        product: &AffineIntegerProduct,
        affine: &AffineFlowState,
        facts: &FactState,
    ) -> Option<AffineProductInterval> {
        let assumptions = affine_facts(affine);
        let left = self.affine_closed_interval_proof(&product.left, &assumptions, affine, facts)?;
        let right =
            self.affine_closed_interval_proof(&product.right, &assumptions, affine, facts)?;

        let products = [
            left.minimum.value.checked_mul(right.minimum.value)?,
            left.minimum.value.checked_mul(right.maximum.value)?,
            left.maximum.value.checked_mul(right.minimum.value)?,
            left.maximum.value.checked_mul(right.maximum.value)?,
        ];
        let (type_minimum, type_maximum) = type_range(product.ty);
        if products
            .iter()
            .any(|value| *value < type_minimum || *value > type_maximum)
        {
            return None;
        }
        // The extrema of a product over two inclusive intervals occur among
        // exactly these four pairs, so the tightest interval the rule can
        // state is their own minimum and maximum.
        let minimum = *products.iter().min()?;
        let maximum = *products.iter().max()?;

        let consequences: Vec<DerivationId> = [
            left.minimum.consequence,
            left.maximum.consequence,
            right.minimum.consequence,
            right.maximum.consequence,
        ]
        .into_iter()
        .map(|proof| {
            self.vocabulary
                .derivations
                .intern(DerivationNode::AffineConsequence {
                    relation: None,
                    premises: proof.premises.into_boxed_slice(),
                    parents: proof.parents,
                })
        })
        .collect();
        Some(AffineProductInterval {
            minimum,
            maximum,
            consequences: consequences.into_boxed_slice(),
        })
    }

    /// Computes the tightest endpoint found by the existing coefficient-one
    /// rule: first the L0/type interval alone, then each source invariant once
    /// in deterministic order. The final endpoint is reproved through
    /// `affine_target_proof`, so the retained consequence names the actual
    /// invariant premise and every selected L0 endpoint.
    pub(super) fn affine_closed_interval_proof(
        &mut self,
        form: &AffineForm,
        assumptions: &[ActiveAffineFact],
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Option<AffineClosedIntervalProof> {
        let zero = AffineForm::constant(0);
        let upper_zero = affine_less_equal(form, &zero)?;
        let lower_zero = affine_less_equal(&zero, form)?;
        let constant = form.constant_value();

        let mut maximum = self
            .affine_lhs_maximum(&upper_zero, values, facts, &mut AffineCheckState::new())
            .ok()
            .flatten()
            .and_then(|terms| constant.checked_add(terms));
        let mut minimum = self
            .affine_lhs_maximum(&lower_zero, values, facts, &mut AffineCheckState::new())
            .ok()
            .flatten()
            .and_then(|terms| constant.checked_sub(terms));

        for assumption in canonical_affine_facts(assumptions) {
            if let Ok(residual) = AffineInequality::residual_after(
                &upper_zero,
                &assumption.inequality,
                &mut AffineCheckState::new(),
            ) && let Some(candidate) = self
                .affine_lhs_maximum(&residual, values, facts, &mut AffineCheckState::new())
                .ok()
                .flatten()
                .and_then(|residual_maximum| {
                    constant
                        .checked_add(assumption.inequality.upper())?
                        .checked_add(residual_maximum)
                })
                && maximum.is_none_or(|current| candidate < current)
            {
                maximum = Some(candidate);
            }

            if let Ok(residual) = AffineInequality::residual_after(
                &lower_zero,
                &assumption.inequality,
                &mut AffineCheckState::new(),
            ) && let Some(candidate) = self
                .affine_lhs_maximum(&residual, values, facts, &mut AffineCheckState::new())
                .ok()
                .flatten()
                .and_then(|residual_maximum| {
                    constant
                        .checked_sub(assumption.inequality.upper())?
                        .checked_sub(residual_maximum)
                })
                && minimum.is_none_or(|current| candidate > current)
            {
                minimum = Some(candidate);
            }
        }

        let minimum = minimum?;
        let maximum = maximum?;
        if minimum > maximum {
            return None;
        }
        let minimum_target = affine_less_equal(&AffineForm::constant(minimum), form)?;
        let maximum_target = affine_less_equal(form, &AffineForm::constant(maximum))?;
        let minimum_proof = self.affine_target_proof(
            &minimum_target,
            assumptions,
            ProofContext::new(facts, values),
        )?;
        let maximum_proof = self.affine_target_proof(
            &maximum_target,
            assumptions,
            ProofContext::new(facts, values),
        )?;
        Some(AffineClosedIntervalProof {
            minimum: AffineIntervalEndpointProof {
                value: minimum,
                consequence: minimum_proof,
            },
            maximum: AffineIntervalEndpointProof {
                value: maximum,
                consequence: maximum_proof,
            },
        })
    }

    pub(super) fn affine_integer_domain_clauses(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        state: &mut AffineFlowState,
    ) -> Option<Vec<Vec<NumericAffineTarget>>> {
        let CheckedType::Integer(ty) = operand_type else {
            return None;
        };
        if matches!(
            operation,
            CheckedIntegerOperation::ShiftLeftExact
                | CheckedIntegerOperation::ShiftLeftDefined
                | CheckedIntegerOperation::ShiftRightExact
                | CheckedIntegerOperation::ShiftRightDefined
        ) {
            let [_, amount] = arguments else {
                return None;
            };
            let amount = self.affine_pre_domain_form(amount, state)?;
            let target =
                affine_less_equal(&amount, &AffineForm::constant(i128::from(ty.width()) - 1))?;
            let right = self
                .vocabulary
                .terms
                .intern(TermKind::Constant(i128::from(ty.width()) - 1));
            return Some(vec![vec![NumericAffineTarget {
                inequality: target,
                right: Some(right),
            }]]);
        }
        if matches!(
            operation,
            CheckedIntegerOperation::AbsoluteExact | CheckedIntegerOperation::AbsoluteDefined
        ) {
            let [value] = arguments else {
                return None;
            };
            let right = self
                .read_operand(value)
                .or_else(|| self.measure_operand(value));
            let value = self.affine_pre_domain_form(value, state)?;
            let minimum = type_range(ty).0;
            let target = affine_less_equal(&AffineForm::constant(minimum.checked_add(1)?), &value)?;
            return Some(vec![vec![NumericAffineTarget {
                inequality: target,
                right,
            }]]);
        }
        if matches!(
            operation,
            CheckedIntegerOperation::DivideExact
                | CheckedIntegerOperation::DivideDefined
                | CheckedIntegerOperation::RemainderExact
                | CheckedIntegerOperation::RemainderDefined
        ) {
            let [dividend, divisor] = arguments else {
                return None;
            };
            let dividend_right = self
                .read_operand(dividend)
                .or_else(|| self.measure_operand(dividend));
            let divisor_right = self
                .read_operand(divisor)
                .or_else(|| self.measure_operand(divisor));
            let dividend = self.affine_pre_domain_form(dividend, state)?;
            let divisor = self.affine_pre_domain_form(divisor, state)?;
            let positive = NumericAffineTarget {
                inequality: affine_less_equal(&AffineForm::constant(1), &divisor)?,
                right: divisor_right,
            };
            if !ty.signed() {
                return Some(vec![vec![positive]]);
            }
            let minus_one = self.vocabulary.terms.intern(TermKind::Constant(-1));
            let minus_two = self.vocabulary.terms.intern(TermKind::Constant(-2));
            let negative = NumericAffineTarget {
                inequality: affine_less_equal(&divisor, &AffineForm::constant(-1))?,
                right: Some(minus_one),
            };
            let dividend_not_min = NumericAffineTarget {
                inequality: affine_less_equal(
                    &AffineForm::constant(type_range(ty).0.checked_add(1)?),
                    &dividend,
                )?,
                right: dividend_right,
            };
            let divisor_below_minus_one = NumericAffineTarget {
                inequality: affine_less_equal(&divisor, &AffineForm::constant(-2))?,
                right: Some(minus_two),
            };
            let divisor_above_minus_one = NumericAffineTarget {
                inequality: affine_less_equal(&AffineForm::constant(0), &divisor)?,
                right: divisor_right,
            };
            let nonzero = [negative, positive];
            let overflow_safe = [
                dividend_not_min,
                divisor_below_minus_one,
                divisor_above_minus_one,
            ];
            let mut clauses = Vec::with_capacity(nonzero.len() * overflow_safe.len());
            for nonzero in &nonzero {
                for overflow_safe in &overflow_safe {
                    clauses.push(vec![nonzero.clone(), overflow_safe.clone()]);
                }
            }
            return Some(clauses);
        }
        let result = match operation {
            CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
                let [left, right] = arguments else {
                    return None;
                };
                let left = self.affine_pre_domain_form(left, state)?;
                let right = self.affine_pre_domain_form(right, state)?;
                left.add(&right, &mut AffineCheckState::new()).ok()?
            }
            CheckedIntegerOperation::SubtractExact | CheckedIntegerOperation::SubtractDefined => {
                let [left, right] = arguments else {
                    return None;
                };
                let left = self.affine_pre_domain_form(left, state)?;
                let right = self.affine_pre_domain_form(right, state)?;
                left.subtract(&right, &mut AffineCheckState::new()).ok()?
            }
            CheckedIntegerOperation::MultiplyExact | CheckedIntegerOperation::MultiplyDefined => {
                let [left, right] = arguments else {
                    return None;
                };
                let left = self.affine_pre_domain_form(left, state)?;
                let right = self.affine_pre_domain_form(right, state)?;
                if left.terms().is_empty() {
                    right
                        .scale(left.constant_value(), &mut AffineCheckState::new())
                        .ok()?
                } else if right.terms().is_empty() {
                    left.scale(right.constant_value(), &mut AffineCheckState::new())
                        .ok()?
                } else {
                    return None;
                }
            }
            CheckedIntegerOperation::NegateExact | CheckedIntegerOperation::NegateDefined => {
                let [value] = arguments else {
                    return None;
                };
                self.affine_pre_domain_form(value, state)?
                    .scale(-1, &mut AffineCheckState::new())
                    .ok()?
            }
            _ => return None,
        };
        let (minimum, maximum) = type_range(ty);
        let mut check = AffineCheckState::new();
        let maximum_term = self.vocabulary.terms.intern(TermKind::Constant(maximum));
        Some(vec![vec![
            NumericAffineTarget {
                inequality: AffineInequality::from_forms(
                    &result,
                    &AffineForm::constant(maximum),
                    &mut check,
                )
                .ok()?,
                right: Some(maximum_term),
            },
            NumericAffineTarget {
                inequality: AffineInequality::from_forms(
                    &AffineForm::constant(minimum),
                    &result,
                    &mut check,
                )
                .ok()?,
                right: None,
            },
        ]])
    }

    /// Exact pre-domain value construction.  Unlike ordinary value flow it
    /// has no fresh-result fallback: failure to reconstruct the mathematical
    /// operands simply makes the affine proof route unavailable.
    pub(super) fn affine_pre_domain_form(
        &mut self,
        expression: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        if let Some(value) = self.input.constant_storage_scalar(expression).cloned() {
            return self.affine_pre_domain_form(&CheckedExpression::Constant(value), state);
        }
        let mut events = Vec::new();
        self.input.collect_expression_kills(expression, &mut events);
        if !events.is_empty() {
            return None;
        }
        match expression {
            CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::RangeElementMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. } => self
                .checked_measure_term(expression)
                .map(|term| self.vocabulary.measure_atom(term, state)),
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
            | CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { ty, bits },
                ..
            } => Some(AffineForm::constant(integer_value(*ty, *bits))),
            // [MSR-6] a symbolic const generic is one declaration-anchored
            // value throughout the generic body. Counted-range endpoint
            // capture must use that same image as an invariant or contract
            // spelling of the parameter; treating the endpoint as an
            // unrelated unknown loses the exhaustion fact at the return.
            CheckedExpression::Constant(CheckedValue::ConstGeneric { declaration, .. }) => {
                let term = self.const_parameter_term(*declaration);
                Some(self.vocabulary.measure_atom(term, state))
            }
            CheckedExpression::Binding { binding, ty, .. } => {
                let CheckedType::Integer(integer) = *ty else {
                    return None;
                };
                if self.input.affine_binding_type(*binding) != Some(integer) {
                    return None;
                }
                if let Some(value) = state.values.get(binding) {
                    Some(value.clone())
                } else {
                    let value = self.vocabulary.new_affine_atom(integer);
                    state.values.insert(*binding, value.clone());
                    Some(value)
                }
            }
            CheckedExpression::NumericConversion {
                mode: CheckedConversionMode::Exact,
                source: CheckedNumericType::Integer(_),
                destination: CheckedNumericType::Integer(_),
                value,
                ..
            } => self.affine_pre_domain_form(value, state),
            CheckedExpression::IntegerOperation {
                operation,
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => match operation {
                CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
                    let [left, right] = arguments.as_slice() else {
                        return None;
                    };
                    self.affine_pre_domain_form(left, state)?
                        .add(
                            &self.affine_pre_domain_form(right, state)?,
                            &mut AffineCheckState::new(),
                        )
                        .ok()
                }
                CheckedIntegerOperation::SubtractExact
                | CheckedIntegerOperation::SubtractDefined => {
                    let [left, right] = arguments.as_slice() else {
                        return None;
                    };
                    self.affine_pre_domain_form(left, state)?
                        .subtract(
                            &self.affine_pre_domain_form(right, state)?,
                            &mut AffineCheckState::new(),
                        )
                        .ok()
                }
                CheckedIntegerOperation::MultiplyExact
                | CheckedIntegerOperation::MultiplyDefined => {
                    let [left, right] = arguments.as_slice() else {
                        return None;
                    };
                    let left = self.affine_pre_domain_form(left, state)?;
                    let right = self.affine_pre_domain_form(right, state)?;
                    if left.terms().is_empty() {
                        right
                            .scale(left.constant_value(), &mut AffineCheckState::new())
                            .ok()
                    } else if right.terms().is_empty() {
                        left.scale(right.constant_value(), &mut AffineCheckState::new())
                            .ok()
                    } else {
                        None
                    }
                }
                CheckedIntegerOperation::NegateExact | CheckedIntegerOperation::NegateDefined => {
                    let [value] = arguments.as_slice() else {
                        return None;
                    };
                    self.affine_pre_domain_form(value, state)?
                        .scale(-1, &mut AffineCheckState::new())
                        .ok()
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Exact maximum of one affine left-hand side under the independently
    /// known L0/type interval of each atom. This is numeric discovery only;
    /// callers must subsequently prove any selected endpoint with
    /// `affine_target_proof` before it can discharge a source obligation.
    pub(super) fn affine_lhs_maximum(
        &mut self,
        inequality: &AffineInequality,
        values: &AffineFlowState,
        facts: &FactState,
        check: &mut AffineCheckState,
    ) -> Result<Option<i128>, AffineCheckError> {
        let mut requested = inequality
            .terms()
            .iter()
            .map(|coefficient| coefficient.term())
            .collect::<Vec<_>>();
        requested.sort_unstable();
        requested.dedup();

        let measure_terms_by_atom = self.vocabulary.measure_terms_by_atom(values);
        let mut term_intervals = HashMap::new();
        for atom_id in requested {
            let atom = *self
                .vocabulary
                .affine_atoms
                .get(atom_id.index() as usize)
                .ok_or(AffineCheckError::CoefficientMismatch)?;
            let (minimum, maximum) = (atom.minimum, atom.maximum);
            let mut bindings = values
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
            if let Some(measures) = measure_terms_by_atom.get(&atom_id) {
                terms.extend(measures.iter().copied());
            }
            term_intervals.insert(atom_id, (minimum, maximum, terms));
        }

        let closed = close(
            facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if closed.contradictory() {
            return Ok(None);
        }
        let intervals = term_intervals
            .into_iter()
            .map(|(atom, (mut minimum, mut maximum, terms))| {
                for term in terms {
                    if let Some(upper) = closed.tight_bound(term, ZERO) {
                        maximum = maximum.min(upper);
                    }
                    if let Some(negative_lower) = closed.tight_bound(ZERO, term)
                        && let Some(lower) = negative_lower.checked_neg()
                    {
                        minimum = minimum.max(lower);
                    }
                }
                (atom, (minimum, maximum))
            })
            .collect::<HashMap<_, _>>();
        interval_maximum(
            inequality.terms(),
            |term| intervals.get(&term).copied(),
            check,
        )
    }

    /// Every [INV-1] measure factor of one written affine expression, in the
    /// order a left-to-right walk reaches it, interned as its [ENT-2] term.
    pub(super) fn collect_affine_measure_terms(
        &mut self,
        expression: &CheckedAffineExpression,
        out: &mut Vec<TermId>,
    ) -> Option<()> {
        for expression in expression.postorder() {
            match &expression.kind {
                CheckedAffineExpressionKind::Constant { .. }
                | CheckedAffineExpressionKind::Local { .. }
                | CheckedAffineExpressionKind::Add(_, _)
                | CheckedAffineExpressionKind::Subtract(_, _)
                | CheckedAffineExpressionKind::MultiplyByConstant { .. } => {}
                CheckedAffineExpressionKind::Measure(measure) => {
                    out.push(self.checked_measure_term(measure)?);
                }
                CheckedAffineExpressionKind::ConstGeneric { declaration, .. } => {
                    out.push(self.const_parameter_term(*declaration));
                }
            }
        }
        Some(())
    }

    /// The [ENT-2] measure term one [INV-1] affine measure factor names.
    pub(super) fn checked_measure_term(
        &mut self,
        expression: &CheckedExpression,
    ) -> Option<TermId> {
        let goal = self.input.goal_expression(expression, false)?;
        self.goal_operand(&goal)
    }

    pub(super) fn affine_l0_candidates(
        &mut self,
        values: &AffineFlowState,
    ) -> Vec<AffineL0Candidate> {
        let mut candidates = vec![AffineL0Candidate {
            term: ZERO,
            value: AffineForm::constant(0),
        }];
        // [MSR-4] step 6 ranges over every live measure term as well as every
        // own integer binding with an image, so a measure participates in the
        // affine domain through its own atom.
        for term in self.vocabulary.measure_terms() {
            let value = self.vocabulary.measure_atom(term, values);
            // A measure whose image is a constant is Z displaced by that
            // constant, and Z is already the fixed zero candidate, so its
            // index entries would duplicate Z's under one coefficient vector.
            if value.terms().is_empty() {
                continue;
            }
            candidates.push(AffineL0Candidate { term, value });
        }
        let mut bindings = values.values.keys().copied().collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.0);
        for binding in bindings {
            let Some(ty) = self.input.affine_binding_type(binding) else {
                continue;
            };
            let term = self.vocabulary.terms.intern(TermKind::Place(
                ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                ty,
            ));
            candidates.push(AffineL0Candidate {
                term,
                value: values.values[&binding].clone(),
            });
        }
        candidates
    }

    pub(super) fn affine_interval_proof(
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

    pub(super) fn affine_residual_proof(
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
                .affine_l0_proof(inequality, query.l0, query.closed)?
        {
            return Ok(Some(parents));
        }
        self.affine_interval_proof(inequality, query, check)
    }

    /// Checks the fixed `DIRECT(T - S)` residual of one accumulated candidate
    /// `S`, and then the same residual against each integer tightening of `S`.
    ///
    /// Every affine atom denotes a mathematical integer, so an accumulated
    /// `k * v <= u` with a positive integer `k` dividing every coefficient
    /// also proves `v <= floor(u / k)`. The tightening factors are functions
    /// of the candidate and the target alone: this step selects no additional
    /// premise, guesses no multiplier, and leaves the candidate families
    /// exactly as fixed by the specification. Each tightening is formed on its
    /// own: an unrepresentable one is skipped and removes neither the other
    /// tightening nor the untightened candidate.
    pub(super) fn affine_candidate_residual_proof(
        &mut self,
        target: &AffineInequality,
        candidate: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Option<Vec<DerivationId>> {
        let tightenings = integer_tightenings(candidate, target, check);
        for accumulated in std::iter::once(candidate).chain(tightenings.iter()) {
            let Ok(residual) = AffineInequality::residual_after(target, accumulated, check) else {
                continue;
            };
            if let Ok(Some(parents)) = self.affine_residual_proof(&residual, query, check) {
                return Some(parents);
            }
        }
        None
    }

    /// Exhausts one coefficient-one L0 premise followed by the direct
    /// L0/interval residual rule. The L0 index contains one strongest entry
    /// per coefficient vector, so strengthening ordinary facts can only make
    /// a residual easier and never removes an earlier witness.
    pub(super) fn affine_l0_then_direct_proof(
        &mut self,
        target: &AffineInequality,
        query: &mut AffineDirectQuery<'_>,
        check: &mut AffineCheckState,
    ) -> Option<Vec<DerivationId>> {
        for entry in &query.l0.entries {
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
        None
    }

    pub(super) fn affine_target_proof(
        &mut self,
        target: &AffineInequality,
        assumptions: &[ActiveAffineFact],
        context: ProofContext<'_>,
    ) -> Option<AffineConsequenceProof> {
        let values = context.affine;
        let mut check = AffineCheckState::new();
        let candidates = self.affine_l0_candidates(values);
        let closed = context.close(
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        // Every relation-form use in a certificate sees the same entering
        // facts and value images. Its target and residual still run through
        // all ordinary rules; only the unchanged ordered query index is
        // shared. Candidate formation precedes the revision check because it
        // may register a previously unseen term.
        let l0 = context
            .closed
            .and_then(|view| view.affine_index(&self.vocabulary.terms, &self.vocabulary.goals))
            .unwrap_or_else(|| {
                let index = Rc::new(affine_l0_index(&candidates, &closed, &mut check));
                if let Some(view) = context
                    .closed
                    .filter(|view| view.matches(&self.vocabulary.terms, &self.vocabulary.goals))
                {
                    *view.affine_index.borrow_mut() = Some(Rc::clone(&index));
                }
                index
            });
        let mut query = AffineDirectQuery::new(&l0, values, &closed);
        if let Ok(Some(parents)) = self.affine_residual_proof(target, &mut query, &mut check) {
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
            if let Some(parents) = self.affine_candidate_residual_proof(
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
            first_two_premise_candidate(&automatic, &mut check, |sum, check| {
                self.affine_candidate_residual_proof(target, sum, &mut query, check)
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
        self.affine_l0_then_direct_proof(target, &mut query, &mut check)
            .map(|parents| AffineConsequenceProof {
                premises: Vec::new(),
                parents,
            })
    }
}

/// Exhausts the unordered coefficient-one premise pairs, including `(p, p)`.
///
/// The callback receives each accumulated pair sum and owns the residual
/// against the target. Candidate arithmetic is isolated: an unrepresentable
/// pair is skipped and cannot hide a later representable witness. Returning
/// after a successful callback is acceptance-order independent because every
/// earlier candidate has already failed and adding another premise cannot
/// remove an existing pair from this source-shaped finite set.
pub(super) fn first_two_premise_candidate<T>(
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

/// Builds the goal-query index for ordinary difference bounds.
///
/// This is an ephemeral view over the already-closed L0 state, not a copy
/// of `FactState::bounds` in the affine premise set. For each canonical
/// affine coefficient vector it retains the strongest live L0 image. A
/// target or residual can therefore query exactly its own vector without
/// making every L0 edge participate in affine premise enumeration.
pub(super) fn affine_l0_index(
    candidates: &[AffineL0Candidate],
    closed: &ClosedState,
    check: &mut AffineCheckState,
) -> AffineL0Index {
    let mut index = AffineL0Index::default();
    for left in candidates {
        for right in candidates {
            let Some(bound) = closed.tight_bound(left.term, right.term) else {
                continue;
            };
            let Ok(inequality) =
                AffineInequality::from_bounded_forms(&left.value, &right.value, bound, check)
            else {
                // This L0 image is outside the affine i128 vocabulary.
                // It cannot suppress another representable image.
                continue;
            };
            let key: Box<[AffineCoefficient]> = inequality.terms().into();
            if let Some(existing) = index.by_terms.get(&key).copied() {
                if inequality.upper() < index.entries[existing].inequality.upper() {
                    index.entries[existing] = AffineL0Entry {
                        inequality,
                        left: left.term,
                        right: right.term,
                        bound,
                    };
                }
                continue;
            }
            let entry = index.entries.len();
            index.by_terms.insert(key, entry);
            index.entries.push(AffineL0Entry {
                inequality,
                left: left.term,
                right: right.term,
                bound,
            });
        }
    }
    index
}

/// Collects only explicit source-affine facts and automatic value images.
/// Ordinary difference bounds remain in L0 and are queried through
/// [`Self::affine_l0_index`] for the concrete target or residual.
/// x1 retires the capacity identity. [MSR-2] used to make
/// `P.len + P.room = P.cap` a standing fact of every window and [ENT-6]
/// appended it here as two inequalities over the place's three measure
/// atoms. The `room` measure is gone, so the identity has no third term
/// to relate and the fact system carries only the orderings
/// `Z <= P.len`, `Z <= P.head`, `P.len <= P.cap` and `P.head <= P.cap`
/// that [`Self::measure_term`] and the implicit bounds publish. Nothing
/// else was appended by that route, so the sequence now starts empty.
pub(super) fn automatic_affine_premises(
    facts: &[ActiveAffineFact],
    check: &mut AffineCheckState,
) -> Result<Vec<AutomaticAffinePremise>, AffineCheckError> {
    let mut premises = Vec::new();
    for fact in canonical_affine_facts(facts) {
        check.charge(1)?;
        let (source, parent) = match fact.evidence {
            AffineFactEvidence::Source(source) => (Some(source), None),
            AffineFactEvidence::Derivation(parent) => (None, Some(parent)),
        };
        premises.push(AutomaticAffinePremise {
            inequality: fact.inequality.clone(),
            source,
            parent,
        });
    }
    Ok(premises)
}

pub(super) fn affine_consequence_from_residual(
    selected: &[(usize, i128)],
    automatic: &[AutomaticAffinePremise],
    mut parents: Vec<DerivationId>,
) -> AffineConsequenceProof {
    let mut premises = Vec::new();
    for &(index, factor) in selected {
        let premise = &automatic[index];
        if let Some(source) = premise.source {
            premises.push(AffinePremiseUse { source, factor });
        }
        if let Some(parent) = premise.parent {
            parents.push(parent);
        }
    }
    parents.sort_unstable_by_key(|parent| parent.0);
    parents.dedup();
    AffineConsequenceProof { premises, parents }
}

pub(super) fn normalize_distinct_requests(requests: &mut [BoundsRequest]) {
    for request in requests {
        if request.distinct
            && let Some(left) = request.left
            && request.right < left
        {
            request.left = Some(request.right);
            request.right = left;
        }
    }
}

pub(super) fn request_relation(request: &BoundsRequest) -> Option<Relation> {
    let left = request.left?;
    Some(if request.distinct {
        Relation::Distinct {
            left,
            right: request.right,
            difference: 0,
        }
    } else {
        Relation::Bound {
            left,
            right: request.right,
            bound: request.bound,
        }
    })
}
