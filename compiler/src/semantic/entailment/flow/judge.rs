//! Judgments: each obligation, call requirement and separation is
//! judged where the walk reaches it and its outcome is recorded.

use super::*;

impl Input<'_, '_> {
    /// The [OWN-5] resolved place that root names.
    pub(super) fn container_root_place(&self, root: &CheckedContainerRoot) -> ResolvedPlace {
        let path = container_root_path(root);
        self.resolve(&path)
    }
}

impl Reasoning<'_, '_, '_> {
    /// Retains the external FN-4 authority that turns proofs of the formal
    /// requirements into permission to execute the selected actual [FN-5].
    /// The referenced query has its own proof arena; this caller-local node
    /// contains only the exact query ID and the caller proofs of its formal
    /// premises.
    pub(super) fn retain_contract_call_authorities(
        &mut self,
        call: &crate::NodePath,
        contract: &super::super::super::model::CheckedCallContract,
        parents: &[DerivationId],
    ) -> Option<Vec<DerivationId>> {
        let premise_paths = contract
            .requirements
            .iter()
            .map(|requirement| &requirement.clause)
            .collect::<Vec<_>>();
        if premise_paths.len() != parents.len() {
            return None;
        }
        let mut retained = Vec::with_capacity(contract.requirement_queries.len());
        for query_id in &contract.requirement_queries {
            let query = self.input.context.contract_query(*query_id)?;
            let [outcome] = query.proof.contract_goals.as_slice() else {
                return None;
            };
            if query.instance != Some(self.input.function.id)
                || outcome.disposition != CallGoalDisposition::Discharged
                || outcome.derivation.is_none()
                || query.premises.iter().ne(premise_paths.iter().copied())
            {
                return None;
            }
            let node = self.vocabulary.derivations.intern(
                super::super::state::DerivationNode::ContractCall {
                    call: call.clone(),
                    query: *query_id,
                    parents: parents.to_vec(),
                },
            );
            let occurrence = self.vocabulary.contract_call_roots;
            self.vocabulary.contract_call_roots = self
                .vocabulary
                .contract_call_roots
                .checked_add(1)
                .expect("FN-4 call roots exceed the u32 identity space");
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::CallContract(occurrence), node);
            retained.push(node);
        }
        Some(retained)
    }

    /// [OP-4, MSR-4] the obligation each subscript occurring *inside* a
    /// measured place owes, judged where the place is formed.
    ///
    /// `len_of(table[i])` names a place whose own subscript is an ordinary
    /// [OP-4] occurrence: it is discharged against `len_of(table)`, over the
    /// prefix of the path that reaches its base, and the measure term itself
    /// exists only where every one of them is discharged.
    /// Snapshot one binding-valued index at the point its place/call actual is
    /// evaluated. The immutable term lets pre-kill closure preserve exactly
    /// what was known about that occurrence without later rereading binding
    /// storage.
    pub(super) fn establish_index_capture(
        &mut self,
        captured: CapturedValue,
        expression: &CheckedExpression,
        state: &mut ProofFlowState,
    ) {
        if !matches!(captured.capture, CaptureId::Source(_))
            || !matches!(captured.term, CapturedTerm::Binding(_))
        {
            return;
        }
        let kind = TermKind::IndexCapture {
            capture: captured.capture,
        };
        if self.vocabulary.terms.interned(&kind).is_some() {
            return;
        }
        let image = self.affine_expression_form(expression, &mut state.affine);
        let Some(source) = self.read_operand(expression) else {
            return;
        };
        let datum = self.vocabulary.terms.intern(kind);
        if let Some(image) = image {
            state.affine.indices.insert(captured.capture, image);
        }
        let event = self
            .vocabulary
            .proof_event(FlowEventKind::S13, expression.carrier());
        state.facts.establish(
            &Relation::Equal {
                left: datum,
                right: source,
                difference: 0,
            },
            &mut self.vocabulary.derivations,
            event,
        );
    }
}

impl Judging<'_, '_, '_> {
    pub(super) fn obligations_since_discharged(&self, obligation_start: usize) -> bool {
        self.output.obligations[obligation_start..]
            .iter()
            .all(|outcome| {
                outcome.discharged
                    || matches!(outcome.family, ObligationFamily::ReferencePreservation(_))
            })
    }

    pub(super) fn judge_call_goal(
        &mut self,
        callee: super::super::super::model::FunctionId,
        node_path: &crate::NodePath,
        (requires_clause, subject): (crate::NodePath, Option<u32>),
        goal: ConcreteGoal,
        argument_count: usize,
        (context, written): (ProofContext<'_>, &ProofFlowState),
    ) -> (CallGoalDisposition, Option<DerivationId>) {
        let (disposition, evidence, derivation) =
            self.reasoning().call_goal_disposition(&goal, context);
        let ordinal = u32::try_from(self.output.call_goals.len())
            .expect("ENT call-root ordinal exceeds the u32 identity space");
        if let Some(root) = derivation {
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::CallGoal(ordinal), root);
        }
        let rendered_goal = self.input.render_concrete_goal(&goal.root);
        let range_lengths = if disposition == CallGoalDisposition::Discharged {
            Vec::new()
        } else {
            self.input.range_length_readings(&goal.root)
        };
        self.output.call_goals.push(CallGoalOutcome {
            node_path: node_path.clone(),
            callee,
            requires_clause,
            subject,
            goal,
            rendered_goal,
            argument_count: u32::try_from(argument_count)
                .expect("ENT call argument count exceeds the u32 identity space"),
            disposition,
            evidence,
            derivation,
            written_before: written.written_before(disposition == CallGoalDisposition::Discharged),
            range_lengths,
        });
        (disposition, derivation)
    }

    /// [OWN-7] the separations at this call or set commit: mandatory effect
    /// questions and preservation questions demanded by later reference uses.
    ///
    /// The checker owns the comparison — clauses 2 and 3 need the actual
    /// argument spellings and the live reference state — and hands over the
    /// pairs whose separation is a proof rather than syntax [OWN-7]. Each
    /// pair is discharged here by the fixed [ENT-6] families under [MSR-4]'s
    /// disposition, and a discharged pair is recorded on the current edge so
    /// that later dominated [OWN-7] questions can read it. A set reaches this
    /// judgment after RHS effects, using target captures evaluated before it.
    pub(super) fn judge_call_separations(
        &mut self,
        site: &crate::NodePath,
        state: &mut ProofFlowState,
    ) -> bool {
        let pending: Vec<_> = self
            .input
            .function
            .call_separations
            .iter()
            .enumerate()
            .filter(|(query, _)| !self.output.judged_separations.contains(query))
            .filter(|(_, separation)| separation.site.components().starts_with(site.components()))
            .map(|(query, separation)| (query, separation.clone()))
            .collect();
        let mut all_discharged = true;
        for (query, separation) in pending {
            self.output.judged_separations.insert(query);
            let discharged = self.judge_one_separation(query, &separation, state);
            // A preservation query is a later reference-use obligation. It
            // does not make this event unreachable or suppress its effects.
            all_discharged &= discharged || separation.reference_use.is_some();
        }
        all_discharged
    }

    /// One pair, by an ordered unresolved index candidate or by the four
    /// non-strict orderings [OWN-7] names for two range steps. Either empty-
    /// range ordering suffices because formation already proved start no
    /// greater than end [REF-4].
    pub(super) fn judge_one_separation(
        &mut self,
        query: usize,
        separation: &super::super::super::model::CheckedCallSeparation,
        state: &mut ProofFlowState,
    ) -> bool {
        use super::super::super::model::CheckedCallSeparationPositions;
        let proved = separation.positions.iter().copied().find_map(|positions| {
            let proof = match positions {
                CheckedCallSeparationPositions::Indices(left, right) => {
                    self.reasoning().prove_index_separation(left, right, state)
                }
                CheckedCallSeparationPositions::Ranges(left, right) => self
                    .reasoning()
                    .prove_range_separation(left, right, &state.facts, &state.affine),
                CheckedCallSeparationPositions::Live(index) => separation
                    .window
                    .as_ref()
                    .and_then(|window| self.reasoning().index_live_proof(window, index, state)),
                CheckedCallSeparationPositions::IndexOutsideRange(index, range) => self
                    .reasoning()
                    .prove_index_outside_range(index, range, state),
                CheckedCallSeparationPositions::RangeWithinLength(range) => {
                    separation.window.as_ref().and_then(|window| {
                        self.reasoning()
                            .range_within_length_proof(window, range, state)
                    })
                }
            };
            proof.map(|proof| (positions, proof))
        });
        let proof = proved.as_ref().map(|(_, proof)| proof);
        let discharged = proof.is_some();
        if let Some((positions, _)) = &proved {
            match *positions {
                CheckedCallSeparationPositions::Indices(left, right) => {
                    state.separations.record_indices_distinct(left, right);
                }
                CheckedCallSeparationPositions::Ranges(left, right) => {
                    state.separations.record_ranges_disjoint(left, right);
                }
                CheckedCallSeparationPositions::IndexOutsideRange(index, range) => {
                    state.separations.record_index_outside_range(index, range);
                }
                // A bound by the window's length holds at this call's entry
                // alone and is never carried along the edge [WIN-2].
                CheckedCallSeparationPositions::Live(_)
                | CheckedCallSeparationPositions::RangeWithinLength(_) => {}
            }
        }
        let derivation = proof.and_then(|proof| proof.derivation);
        let ordinal = u32::try_from(self.output.obligations.len())
            .expect("ENT obligation ordinal exceeds u32");
        if let Some(root) = derivation {
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::BoundsObligation(ordinal), root);
        }
        self.output.obligations.push(ObligationOutcome {
            node_path: separation.reference_use.as_ref()
                .map_or_else(|| separation.site.clone(), |use_site| use_site.site.clone()),
            family: if separation.reference_use.is_some() {
                ObligationFamily::ReferencePreservation(
                    u32::try_from(query).expect("reference-preservation queries exceed u32"),
                )
            } else if separation.exchange {
                ObligationFamily::ExchangeSeparation(
                    u32::try_from(query).expect("exchange-separation queries exceed u32"),
                )
            } else {
                ObligationFamily::CallSeparation(
                    u32::try_from(query).expect("call-separation queries exceed u32"),
                )
            },
            conjunct: 0,
            canonical_goal: None,
            components: Vec::new(),
            discharged,
            refuted: false,
            contradictory: proof.is_some_and(|proof| proof.route == Some(ProofRoute::Contradiction)),
            residual: (!discharged).then(|| match separation.positions.first() {
                Some(CheckedCallSeparationPositions::Indices(..)) => format!(
                    "{} and {} require their captured indices to be distinct",
                    separation.left_spelling, separation.right_spelling
                ),
                Some(CheckedCallSeparationPositions::Ranges(..)) => format!(
                    "{} and {} select different storage (one ends before the other starts, or one is empty)",
                    separation.left_spelling, separation.right_spelling
                ),
                Some(CheckedCallSeparationPositions::Live(..)) => format!(
                    "{} and {} select different storage (the index is below the window's length)",
                    separation.left_spelling, separation.right_spelling
                ),
                Some(CheckedCallSeparationPositions::IndexOutsideRange(..)) => format!(
                    "{} and {} select different storage (the index is before the range's start or at or after its end, or the range is empty)",
                    separation.left_spelling, separation.right_spelling
                ),
                Some(CheckedCallSeparationPositions::RangeWithinLength(..)) => format!(
                    "{} and {} select different storage (the range ends at or below the window's length, or is empty)",
                    separation.left_spelling, separation.right_spelling
                ),
                None => unreachable!("checker hands off at least one position candidate"),
            }),
            overlap_targets: Some((
                separation.left_spelling.clone(),
                separation.right_spelling.clone(),
            )),
            derivation,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: state.written_before(discharged),
            killed_by: Vec::new(),
        });
        discharged
    }

    /// Judges every optional [PAR-1] query whose first statement is this
    /// exact site. The incoming flow state is still the state before the
    /// statement: none of its effects, postconditions, or later branch facts
    /// have executed. Answers remain outside the flow separation ledger so a
    /// permission proof can never authorize an EFF-5 or kill judgment.
    pub(super) fn judge_permission_separations(
        &mut self,
        site: &crate::NodePath,
        state: &ProofFlowState,
    ) {
        let pending = self
            .output
            .permission_separations
            .iter()
            .enumerate()
            .filter_map(|(index, attempt)| (attempt.query.first == *site).then_some(index))
            .collect::<Vec<_>>();
        for index in pending {
            let query = self.output.permission_separations[index].query.clone();
            let derivation = self
                .reasoning()
                .prove_permission_separation(&query, state)
                .and_then(|proof| proof.derivation);
            let attempt = &mut self.output.permission_separations[index];
            attempt.attempted = true;
            attempt.discharged &= derivation.is_some();
            if let Some(derivation) = derivation {
                attempt.derivations.push(derivation);
            }
        }
    }

    pub(super) fn finalize_permission_separations(&mut self) -> Vec<PermissionSeparationProof> {
        let mut retained = Vec::with_capacity(self.output.permission_separations.len());
        let attempts = std::mem::take(&mut self.output.permission_separations);
        for (query, attempt) in attempts.into_iter().enumerate() {
            let PermissionSeparationAttempt {
                query: identity,
                attempted,
                discharged,
                mut derivations,
            } = attempt;
            let discharged = attempted && discharged;
            if discharged {
                for (occurrence, root) in derivations.iter().copied().enumerate() {
                    self.vocabulary.derivations.add_root(
                        DerivationRootKind::PermissionSeparation {
                            query: u32::try_from(query)
                                .expect("PAR-1 range queries exceed the u32 identity space"),
                            occurrence: u32::try_from(occurrence)
                                .expect("PAR-1 range-query visits exceed the u32 identity space"),
                        },
                        root,
                    );
                }
            } else {
                derivations.clear();
            }
            retained.push(PermissionSeparationProof {
                query: identity,
                discharged,
                derivations,
            });
        }
        retained
    }

    /// Every pair the checker handed over must be judged once, so a pair no
    /// walked call reached is recorded undischarged rather than dropped.
    pub(super) fn reject_unjudged_separations(&mut self) {
        let missing: Vec<_> = self
            .input
            .function
            .call_separations
            .iter()
            .enumerate()
            .filter(|(query, _)| !self.output.judged_separations.contains(query))
            .map(|(query, separation)| (query, separation.clone()))
            .collect();
        for (query, separation) in missing {
            self.output.judged_separations.insert(query);
            let mut state = ProofFlowState::default();
            self.judge_one_separation(query, &separation, &mut state);
        }
    }

    pub(super) fn judge_exact_relation_obligation(
        &mut self,
        (family, conjunct): (ObligationFamily, u8),
        node_path: crate::NodePath,
        root: GoalExpression,
        (left, right): (Option<TermId>, Option<TermId>),
        residual: String,
        states: &ProofFlowState,
    ) {
        let canonical_goal = root.clone();
        let request = left.zip(right).map(|(left, right)| BoundsRequest {
            left: Some(left),
            right,
            bound: 0,
            distinct: false,
        });
        let affine_left =
            left.and_then(|term| self.vocabulary.affine_term_value(term, &states.affine));
        // [MSR-4] the relation submits its own normalized target so the
        // affine route ranges over each side's own atom, exactly as a
        // subscript's bound does, rather than only over the goal expression
        // the two operands render to.
        let direct_affine = self
            .reasoning()
            .affine_goal_ordering_target(&root, &states.affine)
            .or_else(|| {
                let left = affine_left.clone()?;
                let right = right
                    .and_then(|term| self.vocabulary.affine_term_value(term, &states.affine))?;
                let mut check = AffineCheckState::new();
                AffineInequality::from_bounded_forms(&left, &right, 0, &mut check).ok()
            });
        let proof = self.reasoning().prove(
            ProofContext::new(&states.facts, &states.affine),
            ProofGoal::BoundedRelation(BoundedRelationGoal {
                canonical: Some(&root),
                request,
                direct_affine: direct_affine.as_ref(),
                fixed_affine_bridge: None,
                affine_left: affine_left.as_ref(),
            }),
        );
        let discharged = proof.disposition == ProofDisposition::Proved;
        let refuted = proof.disposition == ProofDisposition::Refuted;
        let contradictory = proof.route == Some(ProofRoute::Contradiction);
        let derivation = proof.derivation;
        let ordinal = u32::try_from(self.output.obligations.len())
            .expect("ENT obligation-root ordinal exceeds the u32 identity space");
        if let Some(root) = derivation {
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::BoundsObligation(ordinal), root);
        }
        self.output.obligations.push(ObligationOutcome {
            node_path: node_path.clone(),
            family,
            conjunct,
            canonical_goal: Some(canonical_goal),
            components: request.into_iter().collect(),
            discharged,
            refuted,
            contradictory,
            residual: (!discharged).then(|| residual.clone()),
            overlap_targets: None,
            derivation,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: states.written_before(discharged),
            killed_by: Vec::new(),
        });
    }

    pub(super) fn judge_view_range(
        &mut self,
        node_path: &crate::NodePath,
        start: &CheckedExpression,
        end: &CheckedExpression,
        length: &CheckedExpression,
        states: &ProofFlowState,
    ) {
        for (conjunct, (left, right)) in [(start, end), (end, length)].into_iter().enumerate() {
            let left_goal = self.reasoning().obligation_goal_operand(
                node_path,
                conjunct + 1,
                left,
                &states.facts,
            );
            let right_goal = self.reasoning().obligation_goal_operand(
                node_path,
                conjunct + 2,
                right,
                &states.facts,
            );
            let left_term = self.reasoning().read_operand(left);
            let right_term = self
                .reasoning()
                .read_operand(right)
                .or_else(|| self.reasoning().measure_operand(right));
            let goal = GoalExpression::Operation {
                row: GoalOperation::Integer {
                    operation: CheckedIntegerOperation::LessEqual,
                    operand_type: CheckedType::Integer(IntegerType::U64),
                },
                type_arguments: Vec::new(),
                const_arguments: Vec::new(),
                result: CheckedType::Bool,
                arguments: vec![left_goal, right_goal],
            };
            self.judge_exact_relation_obligation(
                (ObligationFamily::RangeFormation, conjunct as u8),
                node_path.clone(),
                goal,
                (left_term, right_term),
                format!(
                    "{} <= {}",
                    self.input.render_expression(left),
                    self.input.render_expression(right)
                ),
                states,
            );
        }
    }
}

impl Analyzer<'_, '_> {
    /// Judges every bounds obligation inside one expression against the
    /// state at this point, inner offsets before the sites they feed.
    ///
    /// A parent operation is reached only after every partial operation in
    /// its already-evaluated children succeeds.  The acceptance-dark test
    /// hook keeps a failed child outcome for inspection, but must not then
    /// manufacture an admitted exact/index value or a later obligation for
    /// the unreachable parent.
    pub(super) fn judge_children_reach_parent<'expression>(
        &mut self,
        children: impl IntoIterator<Item = &'expression CheckedExpression>,
        states: &mut ProofFlowState,
    ) -> bool {
        let mut reached = true;
        for child in children {
            reached &= self.judge_expression(child, states).reached;
        }
        reached
    }

    pub(super) fn judge_expression(
        &mut self,
        expression: &CheckedExpression,
        states: &mut ProofFlowState,
    ) -> ExpressionJudgment {
        let mut judgment = match expression {
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                actual_captures,
                goal_arguments,
                requirements,
                formal_contract,
                ..
            } => {
                let obligation_start = self.output.obligations.len();
                let mut actuals_reached = true;
                for argument in arguments {
                    actuals_reached &= self.judge_expression(argument, states).reached;
                }
                if actuals_reached {
                    let required_captures = self
                        .input
                        .function
                        .call_separations
                        .iter()
                        .filter(|separation| separation.site == *call)
                        .flat_map(|separation| separation.positions.iter())
                        .flat_map(|positions| {
                            use super::super::super::model::CheckedCallSeparationPositions as Positions;
                            match *positions {
                                Positions::Indices(left, right) => {
                                    vec![left.capture, right.capture]
                                }
                                Positions::Ranges(left, right) => vec![
                                    left.start.capture,
                                    left.end.capture,
                                    right.start.capture,
                                    right.end.capture,
                                ],
                                Positions::Live(index) => vec![index.capture],
                                Positions::IndexOutsideRange(index, range) => {
                                    vec![index.capture, range.start.capture, range.end.capture]
                                }
                                Positions::RangeWithinLength(range) => {
                                    vec![range.start.capture, range.end.capture]
                                }
                            }
                        })
                        .collect::<HashSet<_>>();
                    for (argument, captured) in arguments.iter().zip(actual_captures) {
                        if required_captures.contains(&captured.capture) {
                            self.reasoning()
                                .establish_index_capture(*captured, argument, states);
                        }
                    }
                }
                let actual_parents = self.output.obligations[obligation_start..]
                    .iter()
                    .filter(|outcome| {
                        !matches!(outcome.family, ObligationFamily::ReferencePreservation(_))
                    })
                    .map(|outcome| outcome.discharged.then_some(outcome.derivation).flatten())
                    .collect::<Option<Vec<_>>>();
                let mut goal_parents = Vec::with_capacity(requirements.len());
                let mut goals_ok = actuals_reached;
                let admitted_arguments = actuals_reached.then(|| {
                    arguments
                        .iter()
                        .zip(goal_arguments)
                        .map(|(argument, captured)| {
                            matches!(
                                captured,
                                GoalExpression::Datum(GoalDatum::EvaluatedValue {
                                    occurrence: EvaluatedValueOccurrence::CallArgument {
                                        call: occurrence_call,
                                        ..
                                    },
                                    ..
                                }) if occurrence_call == call
                            )
                            .then(|| self.input.admitted_value_goal_expression(argument))
                            .flatten()
                        })
                        .collect::<Vec<_>>()
                });
                // FN-8 begins only after every actual-expression obligation
                // succeeds. A failed OP-4 actual therefore publishes no call
                // judgment for diagnostic selection to reorder.
                for requirement in requirements {
                    if actuals_reached {
                        let goal = ConcreteGoal::new(admitted_call_goal_expression(
                            &requirement.goal.root,
                            call,
                            admitted_arguments
                                .as_deref()
                                .expect("reached actuals have admitted argument slots"),
                        ));
                        let (disposition, derivation) = self.judging().judge_call_goal(
                            *function,
                            call,
                            (requirement.requires_clause.clone(), requirement.subject),
                            goal,
                            arguments.len(),
                            (ProofContext::new(&states.facts, &states.affine), &*states),
                        );
                        goals_ok &= disposition == CallGoalDisposition::Discharged;
                        if let Some(derivation) = derivation {
                            goal_parents.push(derivation);
                        }
                    }
                }
                let reached = actuals_reached && goals_ok;
                let contract_parents = if reached {
                    formal_contract.as_deref().and_then(|contract| {
                        self.reasoning().retain_contract_call_authorities(
                            call,
                            contract,
                            &goal_parents,
                        )
                    })
                } else {
                    None
                };
                let reached = reached
                    && formal_contract
                        .as_ref()
                        .is_none_or(|_| contract_parents.as_ref().is_some());
                let postconditions = self
                    .input
                    .available_call_postconditions(*function, formal_contract.as_deref());
                let prepared_call = (|| {
                    let mut parents = actual_parents?;
                    if !reached || goal_parents.len() != requirements.len() {
                        return None;
                    }
                    parents.extend(goal_parents);
                    parents.extend(contract_parents.unwrap_or_default());
                    // A direct call needs an earlier-component verified FN-9
                    // summary. A bound call needs either those actual
                    // premises plus its retained FN-4 implication, or a
                    // zero-premise formal implication. Calls with no
                    // authorized relation retain the pre-S12 kill path.
                    if postconditions.is_empty() {
                        return None;
                    }
                    Some(PreparedCall {
                        callee: *function,
                        postconditions,
                        call: call.clone(),
                        parents,
                        transfer_events: Vec::new(),
                        kills: Vec::new(),
                        live: LiveBounds::default(),
                    })
                })();
                // [ENT-3.S13, MSR-3] the call datums are minted here, at the
                // pre-transfer point [ENT-5] fixes, and not at the later
                // establishment: instantiating at the call is what lets a
                // relation over an `own` operand outlive the consume the same
                // statement performs.
                if let Some(prepared) = &prepared_call {
                    self.reasoning().establish_call_datums(
                        *function,
                        call,
                        goal_arguments,
                        &prepared.postconditions,
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call,
                    reached,
                }
            }
            CheckedExpression::ArrayIndex {
                root,
                length,
                offset,
                obligation,
                ..
            } => {
                let reaches_index =
                    self.judge_children_reach_parent(std::iter::once(offset.as_ref()), states);
                let obligation_start = self.output.obligations.len();
                if reaches_index {
                    let base = array_root_place(root);
                    self.judge_obligation(
                        base,
                        MeasuredKind::ConstantArray,
                        Some(*length),
                        offset,
                        obligation.clone(),
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_index
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start),
                }
            }
            CheckedExpression::BufferIndex {
                root,
                offset,
                obligation,
                ..
            } => {
                let reaches_index =
                    self.judge_children_reach_parent(std::iter::once(offset.as_ref()), states);
                let obligation_start = self.output.obligations.len();
                if reaches_index {
                    let base = ResolvedPlace::from_path(root.binding, root.place_path());
                    self.judge_obligation(
                        base,
                        MeasuredKind::RuntimeArray,
                        None,
                        offset,
                        obligation.clone(),
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_index
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start),
                }
            }
            CheckedExpression::RangeElementMeasure { place, .. } => ExpressionJudgment {
                prepared_call: None,
                reached: self.judge_range_element_place(place, states),
            },
            // [OP-4, REF-4] one element of the run a range names owes
            // `i < p^.len`, the range's one measure [MSR-1].
            CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => ExpressionJudgment {
                prepared_call: None,
                reached: self.judge_range_element_place(place, states),
            },
            // [REF-4] the formation's two conjuncts, `lo <= hi` and
            // `hi <= x.len`, over the source's own one measure.
            CheckedExpression::RangeOf {
                carrier,
                source,
                start,
                end,
                obligation,
                captured,
                ..
            } => {
                let reaches_endpoints =
                    self.judge_children_reach_parent([start.as_ref(), end.as_ref()], states);
                // [OWN-7] the formation's two endpoint images, attached to
                // the occurrence that evaluated them. A range step names its
                // formation by that occurrence [REF-1], so a separation
                // submitted at a later call reads exactly these two forms and
                // never re-reads a spelling whose bindings may have moved on.
                self.reasoning().file_range_image(
                    carrier,
                    *captured,
                    start,
                    end,
                    &mut states.affine,
                );
                let obligation_start = self.output.obligations.len();
                let source_subscripts = match source {
                    CheckedRangeSource::Storage(root) => self.judge_place_subscripts(root, states),
                    CheckedRangeSource::Range(_) => true,
                    CheckedRangeSource::Element(place) => {
                        self.judge_range_element_place(place, states)
                    }
                };
                if reaches_endpoints && source_subscripts {
                    let length = match source {
                        CheckedRangeSource::Storage(root) => CheckedExpression::ContainerMeasure {
                            measure: CheckedMeasure::Length,
                            root: root.clone(),
                        },
                        CheckedRangeSource::Range(root) => CheckedExpression::RangeMeasure {
                            measure: CheckedMeasure::Length,
                            root: root.clone(),
                        },
                        CheckedRangeSource::Element(place) => {
                            CheckedExpression::RangeElementMeasure {
                                carrier: carrier.clone(),
                                measure: CheckedMeasure::Length,
                                place: place.clone(),
                            }
                        }
                    };
                    let formation_start = self.output.obligations.len();
                    self.judging()
                        .judge_view_range(obligation, start, end, &length, states);
                    // [PAR-2] retain the existing range-image proof only
                    // after both endpoint-domain obligations succeeded.
                    // Permission consumes these proofs; it cannot infer a
                    // partition merely from the shape of a range argument.
                    if self.output.obligations.len() == formation_start + 2
                        && self.judging().obligations_since_discharged(formation_start)
                        && let Some(image) =
                            states.affine.ranges.get(&captured.start.capture).cloned()
                    {
                        let partitions = self.proved_range_partitions(
                            captured.start.capture,
                            &image.start,
                            &image.end,
                            states,
                        );
                        let outcome = formation_start + 1;
                        for (index, partition) in partitions.iter().enumerate() {
                            for (base, parent) in [
                                (false, partition.stride_nonnegative),
                                (true, partition.base_nonnegative),
                            ] {
                                self.vocabulary.derivations.add_root(
                                    DerivationRootKind::RangePartition {
                                        obligation: u32::try_from(outcome)
                                            .expect("ENT obligation ordinal exceeds u32"),
                                        partition: u32::try_from(index)
                                            .expect("range partition ordinal exceeds u32"),
                                        base,
                                    },
                                    parent,
                                );
                            }
                        }
                        self.output.obligations[outcome].range_partitions = partitions;
                    }
                    // [REF-4, MSR-1] the range this formation names before
                    // any binding does: every resolved place of its source
                    // extended by its own range step, in the term identity a
                    // requirement instantiated over the argument reads.
                    if self.output.obligations.len() == formation_start + 2
                        && self.judging().obligations_since_discharged(formation_start)
                    {
                        let (root, steps) = source.place();
                        for mut place in self.input.places.resolve(root, &steps) {
                            place.path.push(PlaceStep::Range(*captured));
                            self.establish_range_length(
                                place.term_identity(),
                                *captured,
                                (start, end),
                                carrier,
                                states,
                            );
                        }
                    }
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_endpoints
                        && source_subscripts
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start),
                }
            }
            // that place's own subscripts are discharged [OP-4].
            CheckedExpression::ContainerMeasure { root, .. }
            | CheckedExpression::BorrowAddressed { root, .. } => {
                let obligation_start = self.output.obligations.len();
                let reached = self.judge_place_subscripts(root, states);
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reached
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start),
                }
            }
            // [OP-4, TYPE-9] a segment owes `i < len_of(s)` after the
            // `Segments` place's own subscripts; the run of every element owes
            // nothing more.
            CheckedExpression::BorrowSegment { root, segment, .. } => {
                let obligation_start = self.output.obligations.len();
                let mut reached = self.judge_place_subscripts(root, states);
                if let crate::semantic::CheckedSegmentSelect::One(index) = segment {
                    let reaches_offset =
                        self.judge_children_reach_parent(std::iter::once(&index.offset), states);
                    if reached && reaches_offset {
                        self.reasoning().establish_index_capture(
                            index.captured,
                            &index.offset,
                            states,
                        );
                        self.judge_obligation(
                            judged_place(root),
                            MeasuredKind::Segments,
                            None,
                            &index.offset,
                            index.obligation.clone(),
                            states,
                        );
                    }
                    reached &= reaches_offset;
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reached
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start),
                }
            }
            // [OP-4, WIN-1] a run's subscript owes `i < len_of(v)` wherever it
            // is written: the offset is a logical one and the window's length
            // bounds it, so the measured kind is the run's own and the written
            // capacity is not the bound. A read owes exactly what the
            // element-position target below owes, and is judged here.
            CheckedExpression::ReadStorage { root, .. } => {
                let reached = self.judge_place_subscripts(root, states);
                ExpressionJudgment {
                    prepared_call: None,
                    reached,
                }
            }
            CheckedExpression::IntegerOperation {
                carrier,
                operation,
                operand_type,
                arguments,
                ..
            } => {
                let reaches_operation = self.judge_children_reach_parent(arguments, states);
                let obligation_start = self.output.obligations.len();
                if operation.is_exact() && reaches_operation {
                    self.judge_integer_domain_obligation(
                        *operation,
                        *operand_type,
                        arguments,
                        carrier,
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_operation
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start),
                }
            }
            CheckedExpression::NumericConversion {
                carrier,
                mode: CheckedConversionMode::Exact,
                source,
                destination,
                value,
                ..
            } => {
                let reaches_operation = self.judge_children_reach_parent([value.as_ref()], states);
                let obligation_start = self.output.obligations.len();
                if reaches_operation {
                    self.judging().judge_conversion_domain_obligation(
                        *source,
                        *destination,
                        value,
                        carrier,
                        states,
                    );
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: reaches_operation
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start),
                }
            }
            // [TYPE-11] a construction owes its struct's type invariants over
            // its field operands, judged after them as a call's
            // requirements are, in the state before the construction.
            CheckedExpression::ConstructStruct {
                carrier,
                fields,
                invariants,
                invariant_arguments,
                ..
            } if !invariants.is_empty() => {
                let operands_reached = self.judge_children_reach_parent(fields.iter(), states);
                let mut goals_ok = operands_reached;
                if operands_reached {
                    let admitted_arguments = fields
                        .iter()
                        .zip(invariant_arguments)
                        .map(|(field, captured)| {
                            matches!(
                                captured,
                                GoalExpression::Datum(GoalDatum::EvaluatedValue {
                                    occurrence: EvaluatedValueOccurrence::CallArgument {
                                        call: occurrence_call,
                                        ..
                                    },
                                    ..
                                }) if occurrence_call == carrier
                            )
                            .then(|| self.input.admitted_value_goal_expression(field))
                            .flatten()
                        })
                        .collect::<Vec<_>>();
                    let constructing = self.input.function.id;
                    for invariant in invariants {
                        let goal = ConcreteGoal::new(admitted_call_goal_expression(
                            &invariant.goal.root,
                            carrier,
                            &admitted_arguments,
                        ));
                        let (disposition, _) = self.judging().judge_call_goal(
                            constructing,
                            carrier,
                            (invariant.requires_clause.clone(), invariant.subject),
                            goal,
                            fields.len(),
                            (ProofContext::new(&states.facts, &states.affine), &*states),
                        );
                        goals_ok &= disposition == CallGoalDisposition::Discharged;
                    }
                }
                ExpressionJudgment {
                    prepared_call: None,
                    reached: goals_ok,
                }
            }
            _ => {
                let reached =
                    self.judge_children_reach_parent(expression_children(expression), states);
                ExpressionJudgment {
                    prepared_call: None,
                    reached,
                }
            }
        };
        if let Some(carrier) = expression.carrier() {
            judgment.reached &= self.judging().judge_call_separations(carrier, states);
        }
        judgment
    }

    /// [OP-4, MSR-4, INV-1] the same obligation, over the measure places one
    /// written affine relation names.
    ///
    /// An invariant evaluates nothing and reads no storage, but
    /// `len_of(table[i])` is a term there on exactly the terms it is one at a
    /// measure former the program executes: a measure over a place whose
    /// subscripts are not all discharged is no term, so the relation names a
    /// slot the run has or it names nothing. The judgment is made once, at
    /// the point the relation is written — a `loop`'s header invariant in its
    /// entering context, a local `invariant` at its own statement.
    pub(super) fn judge_affine_relation_subscripts(
        &mut self,
        relation: &CheckedAffineRelation,
        states: &mut ProofFlowState,
    ) {
        for side in [&relation.left, &relation.right] {
            self.judge_affine_expression_subscripts(side, states);
        }
    }

    /// [ENT-2, FN-8] the subscripts of the clause (b) places one requirement
    /// forms, each owing [OP-4] against the base it indexes in the body-entry
    /// state holding the requirements written before it. A clause evaluates
    /// nothing, so only the obligations are judged, exactly as the same
    /// place's read or measure would judge them in the body.
    pub(super) fn judge_clause_places(
        &mut self,
        places: &[CheckedExpression],
        states: &mut ProofFlowState,
    ) {
        for place in places {
            match place {
                CheckedExpression::ContainerMeasure { root, .. }
                | CheckedExpression::ReadStorage { root, .. } => {
                    self.judge_place_subscripts(root, states);
                }
                CheckedExpression::RangeElementMeasure { place, .. }
                | CheckedExpression::RangeIndex { place, .. } => {
                    self.judge_range_element_place(place, states);
                }
                _ => {}
            }
        }
    }

    pub(super) fn judge_affine_expression_subscripts(
        &mut self,
        expression: &CheckedAffineExpression,
        states: &mut ProofFlowState,
    ) {
        for expression in expression.postorder() {
            if let CheckedAffineExpressionKind::Measure(measure) = &expression.kind {
                match measure.as_ref() {
                    CheckedExpression::ContainerMeasure { root, .. } => {
                        self.judge_place_subscripts(root, states);
                    }
                    CheckedExpression::RangeElementMeasure { place, .. } => {
                        self.judge_range_element_place(place, states);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(super) fn judge_place_subscripts(
        &mut self,
        root: &CheckedContainerRoot,
        states: &mut ProofFlowState,
    ) -> bool {
        let mut projections = Vec::new();
        if root.binding().is_some_and(is_holder) {
            projections.push(PlaceStep::Deref);
        }
        let mut reached = true;
        for step in &root.path {
            match step {
                CheckedPlaceStep::Field(field) => {
                    projections.push(PlaceStep::Field(*field));
                }
                CheckedPlaceStep::BoxReferent(_) => {
                    projections.push(PlaceStep::Deref);
                }
                CheckedPlaceStep::Subscript(subscript) => {
                    let Some(measured) = measured_kind(subscript.base_type) else {
                        return false;
                    };
                    let base = ResolvedPlace {
                        root: root.root,
                        path: projections.clone(),
                    };
                    let reaches_offset = self
                        .judge_children_reach_parent(std::iter::once(&subscript.offset), states);
                    let obligation_start = self.output.obligations.len();
                    if reaches_offset {
                        self.reasoning().establish_index_capture(
                            subscript.captured,
                            &subscript.offset,
                            states,
                        );
                        self.judge_obligation(
                            base,
                            measured,
                            type_constant(subscript.base_type),
                            &subscript.offset,
                            subscript.obligation.clone(),
                            states,
                        );
                    }
                    reached = reached
                        && reaches_offset
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start);
                    projections.push(PlaceStep::Index(subscript.captured));
                }
            }
        }
        reached
    }

    /// [OP-4, MSR-4] the outer range subscript and every nested subscript in
    /// one measured element place, in source order. The proof base remains
    /// the range holder until ordinary unique-origin resolution substitutes
    /// it; a joined holder is never reduced to one candidate here [ENT-5].
    pub(super) fn judge_range_element_place(
        &mut self,
        place: &super::super::super::model::CheckedRangeElementPlace,
        states: &mut ProofFlowState,
    ) -> bool {
        let mut base = ResolvedPlace::spelled(
            PlaceRoot::Binding(place.root.binding),
            is_holder(place.root.binding),
            Vec::new(),
        );
        let reaches_offset =
            self.judge_children_reach_parent(std::iter::once(&place.offset), states);
        let obligation_start = self.output.obligations.len();
        if reaches_offset {
            self.reasoning()
                .establish_index_capture(place.captured, &place.offset, states);
            self.judge_obligation(
                base.clone(),
                MeasuredKind::Range,
                None,
                &place.offset,
                place.obligation.clone(),
                states,
            );
        }
        let mut reached = reaches_offset
            && self
                .judging()
                .obligations_since_discharged(obligation_start);
        if !reached {
            return false;
        }
        base.path.push(PlaceStep::Index(place.captured));
        for step in &place.path {
            match step {
                CheckedPlaceStep::Field(field) => base.path.push(PlaceStep::Field(*field)),
                CheckedPlaceStep::BoxReferent(_) => base.path.push(PlaceStep::Deref),
                CheckedPlaceStep::Subscript(subscript) => {
                    let Some(measured) = measured_kind(subscript.base_type) else {
                        return false;
                    };
                    let reaches_offset = self
                        .judge_children_reach_parent(std::iter::once(&subscript.offset), states);
                    let obligation_start = self.output.obligations.len();
                    if reaches_offset {
                        self.reasoning().establish_index_capture(
                            subscript.captured,
                            &subscript.offset,
                            states,
                        );
                        self.judge_obligation(
                            base.clone(),
                            measured,
                            type_constant(subscript.base_type),
                            &subscript.offset,
                            subscript.obligation.clone(),
                            states,
                        );
                    }
                    reached = reached
                        && reaches_offset
                        && self
                            .judging()
                            .obligations_since_discharged(obligation_start);
                    if !reached {
                        return false;
                    }
                    base.path.push(PlaceStep::Index(subscript.captured));
                }
            }
        }
        reached
    }

    /// [ENT-6]: the bounds obligation `i < len_of(P)`, normalized
    /// `i - len_of(P) <= -1`, discharged exactly when the closed fact state at
    /// the node derives it.
    pub(super) fn judge_obligation(
        &mut self,
        base: ResolvedPlace,
        measured: MeasuredKind,
        array_length: Option<CheckedConst>,
        offset: &CheckedExpression,
        node_path: crate::NodePath,
        states: &ProofFlowState,
    ) {
        // [OP-4] the obligation is against `len_of(p)` in logical coordinates
        // [MSR-1], never against `cap_of(p)`.
        let length_term = self.reasoning().place_measure_term(
            CheckedMeasure::Length,
            base.clone(),
            measured,
            array_length,
        );
        let offset_term = self.reasoning().read_operand(offset);
        let affine_offset = self
            .input
            .direct_goal_expression(offset)
            .and_then(|offset| self.reasoning().affine_goal_value(&offset, &states.affine));
        // [MSR-4] the subscript submits its own normalized target to the one
        // disposition, so steps 4 and 5 range over the measure's own affine
        // atom instead of being reachable only through the L0-right bridge.
        let direct_affine = affine_offset.as_ref().and_then(|offset| {
            let length = self.vocabulary.measure_atom(length_term, &states.affine);
            let mut check = AffineCheckState::new();
            AffineInequality::from_bounded_forms(offset, &length, -1, &mut check).ok()
        });
        let fixed_array_affine =
            self.reasoning()
                .affine_fixed_array_index_target(offset, array_length, &states.affine);
        let rendered_residual = format!(
            "{} < {}.len",
            self.input.render_expression(offset),
            self.input.render_place(&base)
        );
        let request = BoundsRequest {
            left: offset_term,
            right: length_term,
            bound: -1,
            distinct: false,
        };
        let fixed_array_middle = match array_length {
            Some(CheckedConst::Value(length)) => Some(
                self.vocabulary
                    .terms
                    .intern(TermKind::Constant(i128::from(length))),
            ),
            Some(CheckedConst::Parameter(_) | CheckedConst::Derived(_)) | None => None,
        };
        let fixed_affine_bridge =
            fixed_array_affine
                .as_ref()
                .zip(fixed_array_middle)
                .map(|(target, middle)| FixedAffineBoundBridge {
                    target,
                    middle,
                    left_to_middle_bound: -1,
                });
        let goal = || {
            ProofGoal::BoundedRelation(BoundedRelationGoal {
                canonical: None,
                request: Some(request),
                direct_affine: direct_affine.as_ref(),
                fixed_affine_bridge,
                affine_left: affine_offset.as_ref(),
            })
        };
        let proof = self
            .reasoning()
            .prove(ProofContext::new(&states.facts, &states.affine), goal());
        let discharged = proof.disposition == ProofDisposition::Proved;
        let refuted = proof.disposition == ProofDisposition::Refuted;
        let contradictory = proof.route == Some(ProofRoute::Contradiction);
        let derivation = proof.derivation;
        let residual = (!discharged).then(|| rendered_residual.clone());
        // [DIAG-1] an unproved bound whose measure lost facts to writes on the
        // way here: when those facts, put back, discharge it, the repair
        // names the writes that removed them [ENT-5].
        let killed_by = if discharged || refuted {
            Vec::new()
        } else {
            let kills: Vec<&MeasureKill> = states
                .measure_kills
                .iter()
                .filter(|kill| kill.term == length_term)
                .collect();
            let mut restored = states.facts.clone();
            for kill in &kills {
                restored.restore_killed(&kill.cells, &self.vocabulary.derivations);
            }
            let counterfactual = if kills.iter().any(|kill| !kill.cells.is_empty()) {
                Some(
                    self.reasoning()
                        .prove(ProofContext::new(&restored, &states.affine), goal()),
                )
            } else {
                None
            };
            match counterfactual {
                Some(proof)
                    if proof.disposition == ProofDisposition::Proved
                        && proof.route != Some(ProofRoute::Contradiction) =>
                {
                    let measure = format!("{}.len", self.input.render_place(&base));
                    kills
                        .iter()
                        .map(|kill| MeasureKillNote {
                            source: kill.source.clone(),
                            written: self.input.render_place(&kill.written),
                            measure: measure.clone(),
                            narrowable: kill.narrowable,
                        })
                        .collect()
                }
                _ => Vec::new(),
            }
        };
        let ordinal = u32::try_from(self.output.obligations.len())
            .expect("ENT obligation-root ordinal exceeds the u32 identity space");
        if let Some(root) = derivation {
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::BoundsObligation(ordinal), root);
        }
        self.output.obligations.push(ObligationOutcome {
            node_path: node_path.clone(),
            family: ObligationFamily::Bounds,
            conjunct: 0,
            canonical_goal: None,
            components: vec![request],
            discharged,
            refuted,
            contradictory,
            residual,
            overlap_targets: None,
            derivation,
            affine_index_maps: if discharged {
                self.proved_affine_index_maps(affine_offset.as_ref(), states)
            } else {
                Vec::new()
            },
            range_partitions: Vec::new(),
            written_before: states.written_before(discharged),
            killed_by,
        });
    }

    /// Projects one already-computed exact offset value onto each active
    /// counted binder it depends on alone. The affine form is canonical, so
    /// the single-term test is complete for the deliberately small rule and
    /// its coefficient is nonzero by construction.
    pub(super) fn proved_affine_index_maps(
        &self,
        offset: Option<&AffineForm>,
        states: &ProofFlowState,
    ) -> Vec<super::super::ProvedAffineIndexMap> {
        let Some(offset) = offset else {
            return Vec::new();
        };
        let [coefficient] = offset.terms() else {
            return Vec::new();
        };
        self.frames
            .loops
            .iter()
            .filter_map(|frame| {
                let binder = frame.counted_binder?;
                let binder_term = states.affine.values.get(&binder)?.unit_term()?;
                (coefficient.term() == binder_term).then_some(super::super::ProvedAffineIndexMap {
                    loop_id: frame.id,
                    coefficient: coefficient.coefficient(),
                    constant: offset.constant_value(),
                })
            })
            .collect()
    }

    /// Retains PAR-2's adjacent-range family after both [REF-4] formation
    /// domain goals succeeded. Each active counted loop is considered once, and both sign
    /// goals run to completion for every matching exact image.
    pub(super) fn proved_range_partitions(
        &mut self,
        range: CaptureId,
        start: &AffineForm,
        end: &AffineForm,
        states: &ProofFlowState,
    ) -> Vec<super::super::ProvedRangePartition> {
        let candidates = self
            .frames
            .loops
            .iter()
            .filter_map(|frame| {
                let binder = states
                    .affine
                    .values
                    .get(&frame.counted_binder?)?
                    .unit_term()?;
                let mut visiting = HashSet::new();
                let start = self.vocabulary.counted_value_image(
                    start,
                    binder,
                    &frame.invariant_atoms,
                    &mut visiting,
                )?;
                let end = self.vocabulary.counted_value_image(
                    end,
                    binder,
                    &frame.invariant_atoms,
                    &mut visiting,
                )?;
                let after = start
                    .base
                    .add(&start.stride, &mut AffineCheckState::new())
                    .ok()?;
                (start.stride == end.stride && after == end.base).then_some((frame.id, start))
            })
            .collect::<Vec<_>>();
        let mut partitions = Vec::new();
        for (loop_id, image) in candidates {
            let Some(stride_target) = affine_less_equal(&AffineForm::constant(0), &image.stride)
            else {
                continue;
            };
            let Some(base_target) = affine_less_equal(&AffineForm::constant(0), &image.base) else {
                continue;
            };
            let stride = self.reasoning().prove(
                ProofContext::new(&states.facts, &states.affine),
                ProofGoal::Affine {
                    inequality: &stride_target,
                    right: None,
                },
            );
            let base = self.reasoning().prove(
                ProofContext::new(&states.facts, &states.affine),
                ProofGoal::Affine {
                    inequality: &base_target,
                    right: None,
                },
            );
            if stride.disposition == ProofDisposition::Proved
                && base.disposition == ProofDisposition::Proved
            {
                partitions.push(super::super::ProvedRangePartition {
                    loop_id,
                    range,
                    stride: image.stride,
                    base: image.base,
                    stride_nonnegative: stride
                        .derivation
                        .expect("a proved stride has a derivation"),
                    base_nonnegative: base.derivation.expect("a proved base has a derivation"),
                });
            }
        }
        partitions
    }

    /// [ENT-6] judges one proof-required exact integer operation. The source
    /// occurrence owns one canonical `.defined` goal and one obligation
    /// identity. Fixed L0 components are alternate derivations of that goal;
    /// they are never independent source obligations.
    pub(super) fn judge_integer_domain_obligation(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        node_path: &crate::NodePath,
        states: &mut ProofFlowState,
    ) {
        let canonical_goal = self.reasoning().integer_domain_goal(
            operation,
            operand_type,
            arguments,
            node_path,
            &states.facts,
        );
        let goal = Some(
            self.reasoning()
                .intern_goal_expression(canonical_goal.clone()),
        );
        let components =
            self.reasoning()
                .integer_domain_components(operation, operand_type, arguments);
        let residual = self.input.render_integer_domain_goal(operation, arguments);
        // Goal preparation may need to install a missing binding value image
        // before it can spell the fixed affine alternatives. Keep that work
        // local until the one proof query selects an affine route (or leaves
        // the goal unknown, matching the prior attempted-route state change).
        let candidate_atom_start = self.vocabulary.affine_atoms.len();
        let mut prepared_affine = states.affine.clone();
        let affine_clauses = self.reasoning().affine_integer_domain_clauses(
            operation,
            operand_type,
            arguments,
            &mut prepared_affine,
        );
        let affine_product = self.reasoning().affine_integer_product(
            operation,
            operand_type,
            arguments,
            &mut prepared_affine,
        );

        let outcome = self.reasoning().prove(
            ProofContext::new(&states.facts, &prepared_affine),
            ProofGoal::IntegerDomain(IntegerDomainGoal {
                canonical: goal,
                operation,
                operand_type,
                components: &components,
                affine_clauses: affine_clauses.as_deref(),
                affine_product: affine_product.as_ref(),
            }),
        );
        if outcome.route == Some(ProofRoute::Affine) || outcome.route.is_none() {
            states.affine = prepared_affine;
        } else {
            self.vocabulary.affine_atoms.truncate(candidate_atom_start);
        }
        let discharged = outcome.disposition == ProofDisposition::Proved;
        let refuted = outcome.disposition == ProofDisposition::Refuted;
        let contradictory = outcome.route == Some(ProofRoute::Contradiction);
        // Both records below describe this walk of this operation. A loop body
        // is walked more than once and the same node then carries different
        // operand values each time, so the previous walk's measurement is
        // dropped before this one decides: a judgment that does not discharge
        // must leave nothing behind for the binding to read.
        self.frames.product_intervals.remove(node_path);
        self.frames.product_operands.remove(node_path);
        // [ENT-3.S7]'s multiplication row reads the interval-product rule's
        // intervals only when that rule discharged the domain, so the
        // interval is retained exactly when this obligation discharged
        // through the interval-product route.
        if discharged && let Some(interval) = outcome.product_interval.clone() {
            self.frames
                .product_intervals
                .insert(node_path.clone(), (interval, outcome.derivation));
        }
        // That the exact multiplication's domain held, for [PRF-1] to fold a
        // term-scaled premise against. Recorded only when the domain
        // discharged through an affine route, which is what committed
        // `prepared_affine` and so fixed the images the judgment read.
        let ordinal = u32::try_from(self.output.obligations.len())
            .expect("ENT obligation-root ordinal exceeds the u32 identity space");
        if discharged
            && outcome.route == Some(ProofRoute::Affine)
            && operation == CheckedIntegerOperation::MultiplyExact
            && let Some(parent) = outcome.derivation
        {
            self.frames
                .product_operands
                .insert(node_path.clone(), (parent, ordinal));
        }
        if let Some(root) = outcome.derivation {
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::IntegerDomainObligation(ordinal), root);
        }
        self.output.obligations.push(ObligationOutcome {
            node_path: node_path.clone(),
            family: ObligationFamily::IntegerDomain,
            conjunct: 0,
            canonical_goal: Some(canonical_goal),
            components,
            discharged,
            refuted,
            contradictory,
            residual: (!discharged).then_some(residual),
            overlap_targets: None,
            derivation: outcome.derivation,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: states.written_before(discharged),
            killed_by: Vec::new(),
        });
    }

    pub(super) fn judge_set_target(
        &mut self,
        target: &CheckedSetTarget,
        states: &mut ProofFlowState,
    ) -> bool {
        match target {
            CheckedSetTarget::Place(_) => true,
            // [OP-4, REF-4] judge the outer range position and every nested
            // subscript in source order before the commit may execute.
            CheckedSetTarget::RangeIndex(target) => self.judge_range_element_place(target, states),
            // [OP-4, WIN-1] the run's own obligation is `i < len_of(v)`: the
            // offset is a logical one and the window's length bounds it, so
            // the measured kind is the run's and the written capacity is not
            // the bound.
            CheckedSetTarget::Storage(target) => self.judge_place_subscripts(target, states),
        }
    }
}

pub(super) fn array_root_place(root: &CheckedArrayRoot) -> ResolvedPlace {
    match root {
        CheckedArrayRoot::Binding { binding, fields } => ResolvedPlace::spelled(
            PlaceRoot::Binding(*binding),
            is_holder(*binding),
            fields.clone(),
        ),
        CheckedArrayRoot::Constant(id) => {
            ResolvedPlace::spelled(PlaceRoot::Constant(*id), false, Vec::new())
        }
    }
}

/// The exact place one measured or subscripted root names [MSR-2].
///
/// A run's path may carry subscripts of its own — `len_of(table[i])` is a
/// term [MSR-1] — so it is a source-order projection path and never a
/// field list.
pub(super) fn container_root_path(root: &CheckedContainerRoot) -> ResolvedPlace {
    let mut place = ResolvedPlace {
        root: root.root,
        path: Vec::new(),
    };
    place.path.extend(root.place_path());
    place
}

/// The place one [SET-1] commit writes, as every measure term over it is
/// stated [MSR-1]: a plain place, or one element position of a run.
///
/// An element position is a place only where its offset is one a place
/// relation can name [ENT-2] — a written literal, a live `own` integer
/// binding, or an in-scope const generic. An offset of any other form is
/// provably distinct from nothing, itself included, so a measure over it
/// would relate two elements as one term [OWN-7] and there is no place to
/// carry a measure to.
pub(super) fn set_target_place(target: &CheckedSetTarget) -> Option<ResolvedPlace> {
    match target {
        CheckedSetTarget::Place(place) => Some(ResolvedPlace::spelled(
            PlaceRoot::Binding(place.binding),
            is_holder(place.binding),
            place.fields.clone(),
        )),
        CheckedSetTarget::Storage(target) => {
            if target
                .place_path()
                .contains(&PlaceStep::Index(CapturedValue::unknown()))
            {
                return None;
            }
            Some(container_root_path(target))
        }
        CheckedSetTarget::RangeIndex(target) => {
            let path = target.place_path();
            if path.contains(&PlaceStep::Index(CapturedValue::unknown())) {
                return None;
            }
            let mut place = ResolvedPlace::spelled(
                PlaceRoot::Binding(target.root.binding),
                is_holder(target.root.binding),
                Vec::new(),
            );
            place.path.extend(path);
            Some(place)
        }
    }
}

/// The place one checked storage root names, as [`Analyzer::judge_place_subscripts`]
/// walks it: a holder's referent, each field, each `Box` content and each
/// subscript by its captured offset [REF-1].
fn judged_place(root: &CheckedContainerRoot) -> ResolvedPlace {
    let mut path = Vec::new();
    if root.binding().is_some_and(is_holder) {
        path.push(PlaceStep::Deref);
    }
    for step in &root.path {
        path.push(match step {
            CheckedPlaceStep::Field(field) => PlaceStep::Field(*field),
            CheckedPlaceStep::BoxReferent(_) => PlaceStep::Deref,
            CheckedPlaceStep::Subscript(subscript) => PlaceStep::Index(subscript.captured),
        });
    }
    ResolvedPlace {
        root: root.root,
        path,
    }
}
