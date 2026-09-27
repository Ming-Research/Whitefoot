//! The one structural walk, which owns event order: statements,
//! `set` commits, match arms, and the effects of an evaluated expression.

use super::*;

impl Reasoning<'_, '_, '_> {
    /// A relation over exclusive exit state needs no result destination.
    /// It is established after the call's effects; enclosing commits and
    /// scope exits kill its ordinary place support in the normal flow.
    pub(super) fn establish_call_state(
        &mut self,
        expression: &CheckedExpression,
        prepared: &PreparedCall,
        state: &mut ProofFlowState,
    ) {
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            goal_arguments,
            ..
        } = expression
        else {
            return;
        };
        for available in prepared.postconditions.iter().cloned() {
            if available.variant.is_some()
                || available
                    .relation
                    .operands
                    .iter()
                    .any(|term| term.contains_result())
            {
                continue;
            }
            let Some(instantiated) = self.instantiate_call_postcondition_relation(
                *function,
                call,
                &available.relation,
                arguments,
                goal_arguments,
                &[],
                &[],
            ) else {
                continue;
            };
            if !self.s12_substitutions_survive(
                &prepared.entry_separations(&state.separations),
                &instantiated.substitutions,
                &prepared.kills,
                true,
            ) {
                continue;
            }
            let Some(proof) =
                self.vocabulary
                    .retain_postcondition_call(&instantiated, &available, prepared)
            else {
                continue;
            };
            let occurrence = self.vocabulary.s12_roots;
            self.vocabulary.s12_roots = self
                .vocabulary
                .s12_roots
                .checked_add(1)
                .expect("S12 roots exceed the u32 identity space");
            self.vocabulary
                .derivations
                .add_root(DerivationRootKind::PostconditionState { occurrence }, proof);
            state.facts.establish_from_proof(
                &instantiated.relation,
                proof,
                &self.vocabulary.derivations,
            );
        }
    }
}

impl Judging<'_, '_, '_> {
    pub(super) fn establish_arm_entry(
        &mut self,
        arm: &CheckedMatchArm,
        facts: &ArmFacts,
        state: &mut FactState,
        event: Option<FlowEventId>,
    ) {
        if let Some(relation) = &facts.comparison {
            // Bool arms: tag 1 is `True()`, tag 0 is `False()`; the False
            // arm takes the exact negation [ENT-3].
            if arm.tag == 1 {
                state.establish(
                    relation,
                    &mut self.vocabulary.derivations,
                    event.expect("comparison arm has an S1 proof event"),
                );
            } else if arm.tag == 0 {
                state.establish(
                    &relation.negated(),
                    &mut self.vocabulary.derivations,
                    event.expect("comparison arm has an S1 proof event"),
                );
            }
        }
        for goal in &facts.goals {
            if arm.tag == 1 {
                state.establish_goal(
                    *goal,
                    GoalSign::Positive,
                    &mut self.vocabulary.derivations,
                    event.expect("goal arm has an S1 proof event"),
                );
                // [ENT-3] Signed Boolean decomposition of the established goal.
                self.reasoning().establish_boolean_decomposition(
                    *goal,
                    GoalSign::Positive,
                    state,
                    event.expect("goal arm has an S1 proof event"),
                );
                self.record_boolean_decomposition(*goal, GoalSign::Positive, state);
            } else if arm.tag == 0 {
                state.establish_goal(
                    *goal,
                    GoalSign::Negative,
                    &mut self.vocabulary.derivations,
                    event.expect("goal arm has an S1 proof event"),
                );
                // [ENT-3] Signed Boolean decomposition of the established goal.
                self.reasoning().establish_boolean_decomposition(
                    *goal,
                    GoalSign::Negative,
                    state,
                    event.expect("goal arm has an S1 proof event"),
                );
                self.record_boolean_decomposition(*goal, GoalSign::Negative, state);
            }
        }
    }
}

impl Analyzer<'_, '_> {
    pub(super) fn walk_block(
        &mut self,
        statements: &[CheckedStatement],
        state: &mut ProofFlowState,
    ) -> bool {
        self.frames.scopes.push(Vec::new());
        let mut continues = true;
        for statement in statements {
            if !continues {
                break;
            }
            continues = self.walk_statement(statement, state);
        }
        if continues {
            let depth = self.frames.scopes.len() - 1;
            self.exit_scopes_to(state, depth);
        }
        self.frames.scopes.pop();
        continues
    }

    pub(super) fn declare(&mut self, binding: BindingId) {
        if let Some(scope) = self.frames.scopes.last_mut() {
            scope.push(binding);
        }
    }

    pub(super) fn expression_effects(
        &mut self,
        expression: &CheckedExpression,
        state: &mut ProofFlowState,
    ) -> ExpressionJudgment {
        let mut judgment = self.judge_expression(expression, state);
        let mut events = Vec::new();
        self.input.collect_expression_kills(expression, &mut events);
        if let Some(prepared) = &mut judgment.prepared_call {
            let transfer_events = &mut prepared.transfer_events;
            let live = self.apply_kills_each(state, &events, |analyzer, event| {
                let kind = match event {
                    KillEvent::Consume { .. } | KillEvent::EntryImageHolderConsume { .. } => {
                        FlowEventKind::PostconditionCallConsume
                    }
                    KillEvent::Write { .. } | KillEvent::EntryImageHolderWrite { .. } => {
                        FlowEventKind::PostconditionCallWrite
                    }
                };
                let proof_event = analyzer.vocabulary.proof_event(kind, Some(event.source()));
                transfer_events.push(proof_event);
                proof_event
            });
            prepared.kills = events;
            prepared.live = live;
        } else {
            self.apply_kills(state, &events);
        }
        if let Some(prepared) = &judgment.prepared_call {
            self.reasoning()
                .establish_call_state(expression, prepared, state);
        }
        judgment
    }

    pub(super) fn walk_set(
        &mut self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        state: &mut ProofFlowState,
    ) {
        // [MSR-3] the [LIV-2] `set`-target placement is minted before the
        // statement's own kills, because the datum it forms is the value the
        // transferred place had immediately before them. The destination is
        // the place this commit writes, which is a plain place or one element
        // position of a run.
        let placement = self
            .reasoning()
            .mint_commit_placement(node_path, 0, target, value, state);
        let constructed = if set_target_place(target).is_some() {
            self.reasoning()
                .mint_construct_placements(node_path, value, state)
        } else {
            Vec::new()
        };
        // [SET-1]: the target's base and offset are evaluated before the
        // right-hand side; both are judged at this point, then the commit
        // kill applies.
        let target_reached = self.judge_set_target(target, state);
        let mut result = self.capture_result(node_path, value, state);
        let affine_value = self
            .reasoning()
            .affine_expression_form(value, &mut state.affine);
        let judgment = self.expression_effects(value, state);
        self.reasoning()
            .finish_result(value, &judgment, &mut result, state);
        let ExpressionJudgment {
            prepared_call: prepared,
            reached: value_reached,
        } = judgment;
        let ranges_reached = self.judging().judge_call_separations(node_path, state);
        let commit_reached = target_reached && value_reached && ranges_reached;
        let receiver_route = commit_reached
            .then(|| {
                prepared.as_ref().and_then(|prepared| {
                    self.input
                        .direct_receiver_route(&state.separations, target, value, prepared)
                })
            })
            .flatten();
        invalidate_goal_origin_for_set(&mut state.facts, target);
        // [SET-1, ENT-3.S5]: the right-hand side is now evaluated, so its
        // image is established here, on this occurrence's own commit value,
        // under exactly the rules a `let` initializer uses. Establishing it
        // before the target kill is what lets [ENT-5]'s pre-kill closure
        // carry the surviving consequences of that value past the write,
        // instead of losing them with the old target value.
        //
        // Only a direct fragment place can receive that image, so only such a
        // commit forms a value term: with no destination to carry it to, the
        // image would relate nothing to the program's later state.
        let commit_carries_image = self.vocabulary.commit_target_term(target).is_some();
        let mut commit_event = None;
        let commit_division = (commit_reached && commit_carries_image)
            .then(|| {
                self.establish_value_image(
                    node_path,
                    ValueImage::Commit(node_path),
                    value,
                    &mut state.facts,
                    &mut commit_event,
                )
            })
            .flatten();
        let commit_operands = commit_division.as_ref().and_then(|_| {
            self.reasoning()
                .unsigned_division_operand_forms(value, &mut state.affine)
        });
        if commit_reached && let Some(product) = &affine_value {
            self.establish_unsigned_division_product(product, value, &mut state.affine);
        }
        let mut target_kills = Vec::new();
        self.input
            .collect_target_kill(node_path, target, state, &mut target_kills);
        let receivers =
            if let (Some(prepared), Some(receiver_route)) = (prepared.as_ref(), receiver_route) {
                self.reasoning().prepare_direct_receiver(
                    &state.separations,
                    receiver_route,
                    value,
                    prepared,
                    &target_kills,
                )
            } else {
                Vec::new()
            };
        let target_event = (!receivers.is_empty()).then(|| {
            self.vocabulary
                .proof_event(FlowEventKind::PostconditionReceiverWrite, Some(node_path))
        });
        if let Some(result) = &mut result {
            let live = self.reasoning().event_live_bounds(state, &target_kills);
            let separations = EventSeparations {
                ledger: &state.separations,
                live: &live,
            };
            self.reasoning()
                .apply_kills_one(&separations, &mut result.facts, &target_kills);
        }
        if let Some(target_event) = target_event {
            self.apply_kills_each(state, &target_kills, |_, _| target_event);
        } else {
            self.apply_kills(state, &target_kills);
        }
        // [MSR-3] the placement's second half, after the target's own kills:
        // the committed place's measures are the datums minted before them.
        if commit_reached
            && let Some(carry) = &placement
            && let Some(destination) = set_target_place(target)
        {
            self.reasoning().establish_measure_datums(
                node_path,
                destination,
                carry,
                &mut state.facts,
            );
        }
        // [REF-4, MSR-1] rebinding a range holder installs the new
        // formation's captured length only after the old holder facts have
        // been killed. Read the formation image by capture occurrence; the
        // endpoint bindings may no longer denote the evaluated values.
        if commit_reached
            && let CheckedSetTarget::Place(place) = target
            && place.fields.is_empty()
            && let CheckedExpression::RangeOf { captured, .. } = value
        {
            let destination = bound_place(place.binding);
            let _ = self.reasoning().establish_captured_range_length(
                destination,
                *captured,
                &mut state.affine,
            );
        }
        if commit_reached
            && !constructed.is_empty()
            && let Some(base) = set_target_place(target)
        {
            self.reasoning().establish_construct_placements(
                node_path,
                &base,
                &constructed,
                &mut state.facts,
            );
        }
        // [CALL-4] a `set` target is an S12 destination, and [CALL-6] puts
        // the establishment after the call's own transfer, consumes and the
        // target's commit and kills — which is exactly this point. The
        // destination list is one entry long because a single-target `set`
        // takes result ordinal zero, and the route is the same one a `let`
        // binder and a `set` target list take. `PreparedCall` already holds
        // the exact direct or formal-boundary publication surface, with the
        // target's own kills the events every substitution must survive.
        if commit_reached
            && let Some(prepared) = prepared.as_ref()
            && let CheckedSetTarget::Place(place) = target
        {
            let destinations = vec![Some((
                place.binding,
                place
                    .fields
                    .iter()
                    .map(|field| GoalProjection::Field(*field))
                    .collect::<Vec<_>>(),
                place.ty,
            ))];
            self.reasoning().establish_result_list_destinations(
                node_path,
                &destinations,
                value,
                prepared,
                &target_kills,
                state,
            );
        }
        // [ENT-3.S5, ENT-5]: the committed value exists only after the old
        // target facts have died. The equality names the commit value formed
        // above; when no source recognized the right-hand side, no commit
        // value was interned and this commit contributes no fact either.
        let mut set_image_event = None;
        if commit_reached {
            if let Some(commit) = self.vocabulary.interned_commit_value_term(node_path, value) {
                self.vocabulary.establish_commit_copy_fact(
                    node_path,
                    target,
                    commit,
                    &mut state.facts,
                    &mut set_image_event,
                );
            }
            let mut committed_affine = None;
            if let CheckedSetTarget::Place(place) = target
                && place.fields.is_empty()
                && self.input.affine_binding_type(place.binding).is_some()
                && let Some(value) = affine_value
            {
                committed_affine = Some(value.clone());
                state.affine.values.insert(place.binding, value);
            }
            // The scaled quotient image binds the committed value to the
            // dividend image read before the kill [ENT-3.S7].
            if let (Some(established), Some((dividend, divisor)), Some(quotient)) =
                (commit_division, commit_operands, committed_affine)
            {
                self.vocabulary.establish_unsigned_division_image(
                    &quotient,
                    &dividend,
                    &divisor,
                    established,
                    &mut state.affine,
                );
            }
        }
        if let (Some(prepared), Some(target_event)) = (&prepared, target_event) {
            for receiver in &receivers {
                self.vocabulary.establish_direct_receiver(
                    node_path,
                    receiver,
                    prepared,
                    target_event,
                    state,
                );
            }
        }
        if commit_reached
            && let Some(result) = result
            && let CheckedSetTarget::Place(place) = target
            && place.fields.is_empty()
            && !is_holder(place.binding)
        {
            state.results.insert(place.binding, result);
        }
    }

    pub(super) fn walk_statement(
        &mut self,
        statement: &CheckedStatement,
        state: &mut ProofFlowState,
    ) -> bool {
        let permission_site = match statement {
            CheckedStatement::Proof(proof) => Some(&proof.node_path),
            CheckedStatement::Let { node_path, .. }
            | CheckedStatement::Set { node_path, .. }
            | CheckedStatement::Evaluate { node_path, .. }
            | CheckedStatement::DropExpression { node_path, .. } => Some(node_path),
            CheckedStatement::Match {
                scrutinee: CheckedExpression::UserCall { call, .. },
                ..
            } => Some(call),
            _ => None,
        };
        if let Some(site) = permission_site {
            self.judging().judge_permission_separations(site, state);
        }
        match statement {
            CheckedStatement::Let {
                node_path,
                binding,
                value,
            } => {
                let affine_value = self
                    .reasoning()
                    .affine_expression_form(value, &mut state.affine);
                // [MSR-3] the rebind placement is minted before the
                // initializer's own kills, because the datum it forms is the
                // value the transferred place had immediately before them.
                let rebind = self
                    .reasoning()
                    .mint_rebind_datums(node_path, 0, value, state);
                // [MSR-3] the construct placement is minted at the same
                // point and for the same reason: a field operand is consumed
                // by the construct that fills the field with it.
                let constructed = self
                    .reasoning()
                    .mint_construct_placements(node_path, value, state);
                let mut result = self.capture_result(node_path, value, state);
                let judgment = self.expression_effects(value, state);
                self.reasoning()
                    .finish_result(value, &judgment, &mut result, state);
                if let Some(result) = result {
                    state.results.insert(*binding, result);
                }
                self.declare(*binding);
                if judgment.reached
                    && self.input.affine_binding_type(*binding).is_some()
                    && let Some(value) = affine_value
                {
                    state.affine.values.insert(*binding, value);
                }
                if let Some(prepared) = &judgment.prepared_call {
                    self.reasoning()
                        .establish_direct_result(node_path, *binding, value, prepared, state);
                }
                if judgment.reached
                    && value.ty() == CheckedType::Bool
                    && let Some(relation) = self.reasoning().direct_comparison(value)
                {
                    state.facts.origins.insert(*binding, relation);
                }
                if judgment.reached {
                    self.reasoning()
                        .record_goal_origin(*binding, value, &mut state.facts);
                }
                // Sources S5, S6, S7, and S9 establish at the binding, after
                // the initializer's own kills [ENT-3, ENT-5].
                let mut event = None;
                let unsigned_division = if judgment.reached {
                    self.establish_value_image(
                        node_path,
                        ValueImage::Binding(*binding),
                        value,
                        &mut state.facts,
                        &mut event,
                    )
                } else {
                    None
                };
                if judgment.reached
                    && let Some(rebind) = &rebind
                {
                    self.reasoning().establish_rebind_datums(
                        node_path,
                        *binding,
                        rebind,
                        &mut state.facts,
                    );
                }
                if judgment.reached && !constructed.is_empty() {
                    let base = bound_place(*binding);
                    self.reasoning().establish_construct_placements(
                        node_path,
                        &base,
                        &constructed,
                        &mut state.facts,
                    );
                }
                if let Some(established) = unsigned_division
                    && let Some(quotient) = state.affine.values.get(binding).cloned()
                    && let Some((dividend, divisor)) = self
                        .reasoning()
                        .unsigned_division_operand_forms(value, &mut state.affine)
                {
                    self.vocabulary.establish_unsigned_division_image(
                        &quotient,
                        &dividend,
                        &divisor,
                        established,
                        &mut state.affine,
                    );
                }
                if judgment.reached {
                    self.record_product_atom(*binding, value, &mut state.affine);
                    if let Some(product) = state.affine.values.get(binding).cloned() {
                        self.establish_unsigned_division_product(
                            &product,
                            value,
                            &mut state.affine,
                        );
                    }
                    // [REF-4, MSR-1] a range reference's one measure is
                    // `len`, equal to the immutable endpoint images the
                    // formation recorded while evaluating this initializer.
                    if let CheckedExpression::RangeOf {
                        start,
                        end,
                        captured,
                        ..
                    } = value
                    {
                        let destination = bound_place(*binding);
                        let term = self.reasoning().establish_captured_range_length(
                            destination,
                            *captured,
                            &mut state.affine,
                        );
                        // [ENT-3.S6] the same formation establishes
                        // `deref(part).len = hi - lo` as an ordinary fact, so
                        // a requirement stated over the range's length is
                        // judged against the length the range has and not
                        // merely against an affine premise.
                        if let Some(relation) = term.and_then(|term| {
                            captured_range_length_image(*captured, &state.affine)
                                .filter(|image| image.terms().is_empty())
                                .map(|image| Relation::Equal {
                                    left: term,
                                    right: self
                                        .vocabulary
                                        .terms
                                        .intern(TermKind::Constant(image.constant_value())),
                                    difference: 0,
                                })
                                .or_else(|| {
                                    self.reasoning().range_length_relation(term, start, end)
                                })
                        }) {
                            let formation = self
                                .vocabulary
                                .proof_event(FlowEventKind::S6, Some(node_path));
                            state.facts.establish(
                                &relation,
                                &mut self.vocabulary.derivations,
                                formation,
                            );
                        }
                    }
                }
                true
            }
            // [GRAM-4, CALL-4] `let (a, b) = f(...);`. The call is judged once;
            // each binder is declared and receives the published relations
            // naming its own result ordinal [ENT-3.S12].
            CheckedStatement::DestructuringLet {
                node_path,
                bindings,
                value,
                ..
            } => {
                // [MSR-3] the destructuring placement is minted before the
                // consume the statement performs, because the datums it
                // forms are the measures the taken-apart value's fields had
                // immediately before it.
                let taken = self
                    .reasoning()
                    .mint_destructuring_placements(node_path, bindings, value, state);
                let judgment = self.expression_effects(value, state);
                let mut destinations = Vec::with_capacity(bindings.len());
                for (binding, ty, _) in bindings {
                    self.declare(*binding);
                    // A call's result list has no single expression image to
                    // copy into each destination. Each fragment result still
                    // receives its own current-value atom, so S12's ordinary
                    // result relation can bridge published affine facts to
                    // that ordinal without conflating sibling results.
                    if judgment.reached
                        && let Some(value) = self.vocabulary.affine_unknown_integer(*ty)
                    {
                        state.affine.values.insert(*binding, value);
                    }
                    destinations.push(Some((*binding, Vec::new(), *ty)));
                }
                if judgment.reached {
                    for (ordinal, carry) in &taken {
                        let Some((binding, _, _)) = bindings.get(*ordinal as usize) else {
                            continue;
                        };
                        let destination = bound_place(*binding);
                        self.reasoning().establish_measure_datums(
                            node_path,
                            destination,
                            carry,
                            &mut state.facts,
                        );
                    }
                }
                if let Some(prepared) = &judgment.prepared_call
                    && judgment.reached
                {
                    self.reasoning().establish_result_list_destinations(
                        node_path,
                        &destinations,
                        value,
                        prepared,
                        &[],
                        state,
                    );
                }
                true
            }
            CheckedStatement::PropagateLet {
                node_path,
                binding,
                scrutinee,
                ok_type,
                ..
            } => {
                let mut result = self.capture_result(node_path, scrutinee, state);
                let judgment = self.expression_effects(scrutinee, state);
                self.reasoning()
                    .finish_result(scrutinee, &judgment, &mut result, state);
                self.declare(*binding);
                if let Some(result) = result {
                    self.vocabulary
                        .select_result(node_path, &result, *binding, *ok_type, state);
                }
                if self.input.affine_binding_type(*binding).is_some()
                    && let Some(value) = self.vocabulary.affine_unknown_integer(*ok_type)
                {
                    state.affine.values.insert(*binding, value);
                }
                true
            }
            CheckedStatement::Set {
                node_path,
                target,
                value,
                ..
            } => {
                self.walk_set(node_path, target, value, state);
                true
            }
            CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. } => {
                let _ = self.expression_effects(value, state);
                true
            }
            CheckedStatement::Proof(proof) => {
                self.judge_affine_relation_subscripts(&proof.target, state);
                for written_use in &proof.uses {
                    if let CheckedProofUseSource::Relation(relation) = &written_use.source {
                        self.judge_affine_relation_subscripts(relation, state);
                    }
                }
                let source_ordinal = u32::try_from(self.output.source_proofs.len())
                    .expect("local invariant count exceeds the u32 identity space");
                let target_result = self.reasoning().checked_affine_relation_inequality(
                    &proof.target,
                    &mut state.affine,
                    &mut AffineCheckState::new(),
                );
                let target_failure = target_result
                    .as_ref()
                    .err()
                    .copied()
                    .map(source_proof_formation_failure);
                let target = target_result.as_ref().ok().cloned();
                // [INV-1] an `==` statement target is one batch of two
                // bounds: both are proved here, and both are published.
                let partner_result = self.reasoning().checked_affine_relation_partner(
                    &proof.target,
                    &mut state.affine,
                    &mut AffineCheckState::new(),
                );
                let partner_failure = partner_result
                    .as_ref()
                    .and_then(|partner| partner.as_ref().err().copied())
                    .map(source_proof_formation_failure);
                let partner = partner_result
                    .as_ref()
                    .and_then(|partner| partner.as_ref().ok().cloned());
                let partner_written = partner_result.is_some();
                let target_failure = target_failure.or(partner_failure);
                self.vocabulary
                    .invariant_targets
                    .insert(proof.declaration, target_result);
                let formed_premises = proof
                    .uses
                    .iter()
                    .map(|written_use| match &written_use.source {
                        CheckedProofUseSource::Named(declaration) => self
                            .vocabulary
                            .invariant_targets
                            .get(declaration)
                            .cloned()
                            .unwrap_or(Err(AffineCheckError::CoefficientMismatch)),
                        CheckedProofUseSource::Relation(relation) => {
                            self.reasoning().checked_affine_relation_inequality(
                                relation,
                                &mut state.affine,
                                &mut AffineCheckState::new(),
                            )
                        }
                    })
                    .collect::<Vec<_>>();
                let source_failure_use_index = formed_premises
                    .iter()
                    .position(|premise| premise.is_err())
                    .map(|index| {
                        u32::try_from(index).expect("source-proof use index fits the u32 identity")
                    });
                let source_failure = source_failure_use_index.map(|index| {
                    let error = formed_premises
                        [usize::try_from(index).expect("source-proof use index fits usize")]
                    .as_ref()
                    .expect_err("source failure index names a failed source")
                    .to_owned();
                    source_proof_formation_failure(error)
                });
                let premises = formed_premises
                    .iter()
                    .map(|premise| premise.as_ref().ok().cloned())
                    .collect::<Vec<_>>();
                let published_premises = proof
                    .uses
                    .iter()
                    .map(|written_use| match &written_use.source {
                        CheckedProofUseSource::Named(declaration) => self
                            .vocabulary
                            .invariant_targets
                            .get(declaration)
                            .and_then(|formed| formed.as_ref().ok())
                            .zip(state.affine.published_invariants.get(declaration))
                            .is_some_and(|(declared, published)| declared == published),
                        CheckedProofUseSource::Relation(_) => false,
                    })
                    .collect::<Vec<_>>();
                let named_premises = proof
                    .uses
                    .iter()
                    .map(|written_use| {
                        matches!(&written_use.source, CheckedProofUseSource::Named(_))
                    })
                    .collect::<Vec<_>>();
                let multiplicities = proof
                    .uses
                    .iter()
                    .map(|written_use| {
                        self.reasoning()
                            .certificate_multiplicity(written_use.multiplicity, &mut state.affine)
                    })
                    .collect::<Vec<_>>();
                let certificate_premises = premises
                    .iter()
                    .zip(&multiplicities)
                    .map(|(premise, multiplicity)| premise.clone().zip(multiplicity.clone()))
                    .collect::<Option<Vec<_>>>();

                // [INV-1] a blockless target receives the complete MSR-4
                // disposition. [PRF-1] instead judges a written certificate's
                // redundancy by AUTO alone: Step 6 may prove a target without
                // making its explicitly written certificate redundant.
                let (target_right, partner_right) = if proof.uses.is_empty() {
                    (
                        self.reasoning()
                            .checked_affine_right_term(&proof.target.right),
                        self.reasoning()
                            .checked_affine_right_term(&proof.target.left),
                    )
                } else {
                    (None, None)
                };
                let target_goal = |inequality, right| {
                    if proof.uses.is_empty() {
                        ProofGoal::Affine { inequality, right }
                    } else {
                        ProofGoal::AutomaticAffine { inequality }
                    }
                };
                let target_proved = target.as_ref().is_some_and(|target| {
                    self.reasoning()
                        .prove(
                            ProofContext::new(&state.facts, &state.affine),
                            target_goal(target, target_right),
                        )
                        .disposition
                        == ProofDisposition::Proved
                }) && (!partner_written
                    || partner.as_ref().is_some_and(|partner| {
                        self.reasoning()
                            .prove(
                                ProofContext::new(&state.facts, &state.affine),
                                target_goal(partner, partner_right),
                            )
                            .disposition
                            == ProofDisposition::Proved
                    }));
                let redundant = !proof.uses.is_empty() && target_proved;
                // [MSR-4] a blockless target no step discharged is refuted
                // when the entering context derives the negation of one of
                // its bounds.
                let target_refuted =
                    proof.uses.is_empty() && !target_proved && target_failure.is_none() && {
                        let mut members = vec![(target.clone(), target_right, partner_right)];
                        if partner_written {
                            members.push((partner.clone(), partner_right, target_right));
                        }
                        self.reasoning().affine_target_disposition(
                            &members,
                            &state.facts,
                            &state.affine,
                        ) == TargetDisposition::Refuted
                    };
                let certificate_sum = if proof.uses.is_empty() || source_failure.is_some() {
                    None
                } else {
                    Some(certificate_premises.as_deref().map_or_else(
                        || Err((SourceProofCertificateFailure::FormationCapacity, 0)),
                        source_proof_sum,
                    ))
                };

                // Every written `use` is proved against the same pre-proof
                // program point. No premise established by this statement can
                // help another premise in the same statement.
                let premise_results = self.reasoning().source_proof_premise_results(
                    &premises,
                    &named_premises,
                    &published_premises,
                    &state.affine,
                    &state.facts,
                );
                let certificate_failure = certificate_sum
                    .as_ref()
                    .and_then(|sum| sum.as_ref().err().copied());
                let certificate_failure_kind = certificate_failure.map(|(failure, _)| failure);
                let certificate_failure_use_index =
                    certificate_failure.map(|(_, use_index)| use_index);
                let first_unproved_premise =
                    premise_results
                        .iter()
                        .position(|proved| !proved)
                        .map(|index| {
                            u32::try_from(index)
                                .expect("source-proof use index fits the u32 identity")
                        });
                let residual = if proof.uses.is_empty() {
                    Ok(target_proved)
                } else if target_failure.is_some()
                    || source_failure.is_some()
                    || certificate_failure_kind.is_some()
                    || first_unproved_premise.is_some()
                {
                    Ok(false)
                } else {
                    match (
                        target.as_ref(),
                        certificate_sum.as_ref().and_then(|sum| sum.as_ref().ok()),
                    ) {
                        (Some(target), Some(sum)) => {
                            // [INV-1] every member of the batch is closed by
                            // the one written certificate.
                            let forward = self.reasoning().source_proof_certificate_residual(
                                target,
                                sum,
                                &state.affine,
                                &state.facts,
                            );
                            match (&forward, partner.as_ref()) {
                                (Ok(true), Some(partner)) => {
                                    self.reasoning().source_proof_certificate_residual(
                                        partner,
                                        sum,
                                        &state.affine,
                                        &state.facts,
                                    )
                                }
                                _ => forward,
                            }
                        }
                        _ => Err(SourceProofCertificateFailure::FormationCapacity),
                    }
                };
                let residual_failure = residual.as_ref().err().copied();
                let check = SourceProofCheck {
                    premises: premise_results,
                    first_unproved_premise,
                    combination: residual.unwrap_or(false),
                    target_failure,
                    source_failure,
                    source_failure_use_index,
                    certificate_failure: certificate_failure_kind,
                    certificate_failure_use_index,
                    residual_failure,
                    redundant,
                    target_refuted,
                };

                if let Some(target) = target
                    && check.discharged()
                {
                    for inequality in std::iter::once(target.clone()).chain(partner) {
                        state.affine.facts.push(ActiveAffineFact {
                            inequality,
                            evidence: AffineFactEvidence::Source(
                                SourceAffineFactRef::SourceProof { source_ordinal },
                            ),
                            active_loops: Vec::new(),
                        });
                    }
                    state
                        .affine
                        .published_invariants
                        .insert(proof.declaration, target);
                }
                self.output.source_proofs.push(SourceProofOutcome {
                    node_path: proof.node_path.clone(),
                    use_node_paths: proof
                        .uses
                        .iter()
                        .map(|written_use| written_use.node_path.clone())
                        .collect(),
                    source_ordinal,
                    name: proof.name.clone(),
                    certificate_written: !proof.uses.is_empty(),
                    check,
                });
                true
            }
            CheckedStatement::Return {
                node_path,
                value,
                drops: _,
            } => {
                let multiple = self.input.function.postconditions.iter().any(|clause| {
                    clause.selected_returns.iter().any(|selected| {
                        selected.statement == *node_path && selected.values.len() > 1
                    })
                });
                let returned = match value {
                    CheckedExpression::ConstructStruct { fields, .. } if multiple => {
                        fields.iter().collect::<Vec<_>>()
                    }
                    _ => vec![value],
                };
                let mut results = returned
                    .iter()
                    .map(|value| self.capture_result(node_path, value, state))
                    .collect::<Vec<_>>();

                let affine_result = self
                    .reasoning()
                    .affine_pure_expression_form(value, &mut state.affine);
                // [FN-9] the relation is queried "immediately before return
                // transfer and edge cleanup": the returned value's own
                // consume is that transfer, so it has not happened at the
                // query point and its kills are applied after. Nothing reads
                // the state between the two, because a return has no normal
                // continuation.
                //
                // [MSR-3] a call in return position is not that transfer. The
                // exit-state measure of a written reference parameter
                // "evaluates over that parameter's resolved referent
                // immediately before each selected return, after the return's
                // ordinary effects and kills", and a call's projected writes
                // and its published exit relation are exactly those effects
                // [CALL-6]. Judging the clause before them read the referent's
                // entry state at the exit, which made `deref(p).len ==
                // deref(entry(p)).len` hold over a callee that had just
                // changed it.
                let judgment = if matches!(value, CheckedExpression::UserCall { .. }) {
                    self.expression_effects(value, state)
                } else {
                    self.judge_expression(value, state)
                };
                let mut events = Vec::new();
                if !matches!(value, CheckedExpression::UserCall { .. }) {
                    self.input.collect_expression_kills(value, &mut events);
                }
                for result in &mut results {
                    // Every ordinal sees the complete return expression's
                    // effects. A clause selects only its own Result context.
                    self.reasoning()
                        .finish_result(value, &judgment, result, state);
                }

                self.judging().judge_postcondition_return(
                    node_path,
                    state,
                    affine_result.as_ref(),
                    judgment.reached,
                    &results,
                );
                self.apply_kills(state, &events);
                false
            }
            CheckedStatement::Give {
                node_path,
                value,
                drops: _,
            } => {
                let mut result = self.capture_result(node_path, value, state);
                let judgment = self.expression_effects(value, state);
                self.reasoning()
                    .finish_result(value, &judgment, &mut result, state);
                if let Some((scope_depth, loop_depth, binding, result_type)) =
                    self.frames.gives.last().map(|frame| {
                        (
                            frame.scope_depth,
                            frame.loop_depth,
                            frame.binding,
                            frame.result_type,
                        )
                    })
                {
                    let give_goal_origin = if judgment.reached && result_type == CheckedType::Bool {
                        self.input
                            .admitted_value_goal_expression(value)
                            .map(|origin| self.reasoning().intern_goal_expression(origin))
                    } else {
                        None
                    };
                    let delivery = Some({
                        self.value_delivery_image(
                            value,
                            state,
                            DeliveryImageContext {
                                statement: node_path,
                                receiver_binding: binding,
                                receiver_type: result_type,
                                scope_depth,
                                loop_depth,
                            },
                        )
                    });
                    let mut exit = state.clone();
                    if let Some(result) = result {
                        exit.results.insert(binding, result);
                    }
                    self.exit_scopes_to(&mut exit, scope_depth);
                    self.exit_counted_loops_from(&mut exit, loop_depth);
                    if let Some(frame) = self.frames.gives.last_mut() {
                        frame.gives.push(exit);
                        frame.give_goal_origins.push(give_goal_origin);
                        if let Some(delivery) = delivery {
                            frame.delivery_images.push(delivery);
                            frame.delivery_edges.push(node_path.clone());
                        }
                    }
                }
                false
            }
            CheckedStatement::Break { target, drops: _ } => {
                if let Some(position) = self
                    .frames
                    .loops
                    .iter()
                    .rposition(|frame| frame.id == *target)
                {
                    let depth = self.frames.loops[position].scope_depth;
                    let mut exit = state.clone();
                    self.exit_scopes_to(&mut exit, depth);
                    self.exit_counted_loops_from(&mut exit, position);
                    self.frames.loops[position].breaks.push(exit);
                }
                false
            }
            CheckedStatement::Match {
                scrutinee,
                enum_type,
                arms,
                ..
            } => {
                // [MSR-3] the payload placement is minted before the `match`
                // consumes its scrutinee, because the datums it forms are the
                // measures the payload had immediately before that consume.
                let payload = self
                    .reasoning()
                    .mint_payload_placements(scrutinee, *enum_type, state);
                let mut result = expression_node_path(scrutinee)
                    .cloned()
                    .and_then(|site| self.capture_result(&site, scrutinee, state));
                let judgment = self.expression_effects(scrutinee, state);
                self.reasoning()
                    .finish_result(scrutinee, &judgment, &mut result, state);
                let facts = if judgment.reached {
                    self.reasoning()
                        .arm_facts(scrutinee, *enum_type, &state.facts)
                } else {
                    ArmFacts::default()
                };
                let mut exits = Vec::new();
                for arm in arms {
                    let payload = payload.iter().find(|payload| payload.tag == arm.tag);
                    if let Some(exit) = self.walk_arm(arm, state, &facts, payload, result.as_ref())
                    {
                        exits.push(exit);
                    }
                }
                if exits.is_empty() {
                    false
                } else {
                    *state = self.judging().join_flows(&exits);
                    true
                }
            }
            CheckedStatement::ValueMatchLet {
                node_path,
                binding,
                result_type,
                scrutinee,
                enum_type,
                arms,
                ..
            } => {
                // [MSR-3] the payload placement is minted before the `match`
                // consumes its scrutinee, because the datums it forms are the
                // measures the payload had immediately before that consume.
                let payload = self
                    .reasoning()
                    .mint_payload_placements(scrutinee, *enum_type, state);
                let mut result = self.capture_result(node_path, scrutinee, state);
                let judgment = self.expression_effects(scrutinee, state);
                self.reasoning()
                    .finish_result(scrutinee, &judgment, &mut result, state);
                let facts = if judgment.reached {
                    self.reasoning()
                        .arm_facts(scrutinee, *enum_type, &state.facts)
                } else {
                    ArmFacts::default()
                };
                self.frames.gives.push(GiveFrame {
                    scope_depth: self.frames.scopes.len(),
                    loop_depth: self.frames.loops.len(),
                    node_path: node_path.clone(),
                    binding: *binding,
                    result_type: *result_type,
                    gives: Vec::new(),
                    give_goal_origins: Vec::new(),
                    delivery_images: Vec::new(),
                    delivery_edges: Vec::new(),
                });
                for arm in arms {
                    // Every delivering path leaves by `give`; an arm's
                    // fall-through state contributes nothing [GIVE-1].
                    let payload = payload.iter().find(|payload| payload.tag == arm.tag);
                    let _ = self.walk_arm(arm, state, &facts, payload, result.as_ref());
                }
                let frame = self
                    .frames
                    .gives
                    .pop()
                    .expect("checked value initializer has one active give frame");
                self.declare(*binding);
                if frame.gives.is_empty() {
                    return false;
                }
                *state = self.judging().join_flows(&frame.gives);
                if self.input.affine_binding_type(*binding).is_some()
                    && let Some(value) = self.vocabulary.affine_unknown_integer(*result_type)
                {
                    state.affine.values.insert(*binding, value);
                }
                record_value_initializer_origin(&frame, &mut state.facts);
                self.vocabulary.establish_value_delivery_join(&frame, state);
                true
            }
            CheckedStatement::Loop {
                id,
                invariants,
                body,
                backedge_drops: _,
            } => {
                for invariant in invariants {
                    self.judge_affine_relation_subscripts(&invariant.relation, state);
                }
                let base = self
                    .reasoning()
                    .prove_loop_invariant_bases(invariants, state);
                let base_batch = base
                    .iter()
                    .all(|disposition| *disposition == TargetDisposition::Proved);

                // The generic header starts from the preheader minus every
                // fact a continuing kill may invalidate. Invariants then add
                // precisely the author-written induction hypotheses whose
                // complete base batch succeeded.
                let mut kills = LoopKills::default();
                self.input.collect_continuing_loop_kills(
                    body,
                    true,
                    &mut LoopReachability::default(),
                    &mut kills,
                );
                self.reasoning().apply_loop_kills(state, &kills, None);
                self.reasoning().activate_loop_invariant_batch(
                    *id,
                    invariants,
                    base_batch,
                    &mut state.affine,
                );
                let invariant_declarations = invariants
                    .iter()
                    .map(|invariant| invariant.declaration)
                    .collect::<Vec<_>>();
                let head_entry_images = state.entry_images.clone();
                self.frames.loops.push(LoopFrame {
                    id: *id,
                    invariant_declarations: invariant_declarations.clone().into_boxed_slice(),
                    scope_depth: self.frames.scopes.len(),
                    counted_binder: None,
                    invariant_atoms: HashSet::new(),
                    capture_path: None,
                    breaks: Vec::new(),
                });
                let mut body_state = state.clone();
                let outer_continuing = std::mem::take(&mut body_state.continuing);
                let body_falls_through = self.walk_block(body, &mut body_state);
                if body_falls_through {
                    debug_assert_summarized(&body_state, &kills);
                }

                let mut step = vec![None; invariants.len()];
                if body_falls_through {
                    for (index, invariant) in invariants.iter().enumerate() {
                        step[index] = Some(
                            self.reasoning()
                                .prove_affine_relation_batch(&invariant.relation, &mut body_state),
                        );
                    }
                }
                self.judging()
                    .record_loop_invariant_outcomes(*id, invariants, &base, &step, None);

                let frame = self.frames.loops.pop();
                let mut breaks = frame.map(|frame| frame.breaks).unwrap_or_default();
                for break_state in &mut breaks {
                    remove_active_loop_invariants(
                        &mut break_state.affine,
                        *id,
                        &invariant_declarations,
                    );
                }
                let has_breaks = !breaks.is_empty();
                // The continuation is the join over the break edges; with no
                // break it is the contradictory all-derivable state, matching
                // an unreachable-in-truth continuation the conservative graph
                // keeps reachable [ENT-5].
                *state = self.judging().join_flows(&breaks);
                if !has_breaks {
                    state.entry_images = head_entry_images;
                }
                record_continuing(&mut state.continuing, &outer_continuing);
                true
            }
            CheckedStatement::CountedRange {
                id,
                node_path,
                binder,
                lower,
                upper,
                invariants,
                body,
                backedge_drops: _,
            } => {
                let occurrence = self.vocabulary.encountered_counted;
                self.vocabulary.encountered_counted = self
                    .vocabulary
                    .encountered_counted
                    .checked_add(1)
                    .expect("counted statements exceed the u32 identity space");
                // [FN-1, ENT-3 S11]: evaluate each endpoint exactly once,
                // left to right, then install the private captures and the
                // compiler-updated binder in a construct-owned fact scope.
                let lower_affine = self
                    .reasoning()
                    .affine_expression_form(lower, &mut state.affine);
                let _ = self.expression_effects(lower, state);
                // Capture the upper endpoint after lower-endpoint effects even
                // when no current proof consumes its affine image.  This
                // preserves FN-1 evaluation order and therefore deterministic
                // atom identities for every later program-point value.
                let upper_affine = self
                    .reasoning()
                    .affine_expression_form(upper, &mut state.affine);
                let _ = self.expression_effects(upper, state);
                let outer_scope_depth = self.frames.scopes.len();
                self.frames.scopes.push(vec![*binder]);
                let range_path = node_path.components().to_vec();
                let preheader_event = self
                    .vocabulary
                    .proof_event(FlowEventKind::S11, Some(node_path));
                let counted_terms = self.reasoning().establish_counted_preheader(
                    &range_path,
                    *binder,
                    lower,
                    upper,
                    &mut state.facts,
                    preheader_event,
                );
                // S11 fixes the complete post-capture closure before
                // continuing kills are subtracted. This preserves sound
                // snapshot consequences without rereading a mutable endpoint
                // on later iterations.
                let snapshot = self
                    .vocabulary
                    .derivations
                    .event(FlowEventKind::Snapshot, None);
                state.facts = materialize_closure_at(
                    &state.facts,
                    &self.vocabulary.terms,
                    &self.vocabulary.goals,
                    &mut self.vocabulary.derivations,
                    snapshot,
                );
                let counted = capture_counted_preheader(counted_terms, &state.facts);
                let binder_affine = lower_affine
                    .clone()
                    .or_else(|| self.reasoning().new_affine_binding_atom(*binder))
                    .expect("a checked counted binder has one u64 affine value");
                state.affine.values.insert(*binder, binder_affine);

                let lower_le_upper = lower_affine.as_ref().and_then(|lower| {
                    upper_affine.as_ref().and_then(|upper| {
                        AffineInequality::from_forms(lower, upper, &mut AffineCheckState::new())
                            .ok()
                    })
                });
                let lower_le_upper = lower_le_upper.as_ref().is_some_and(|target| {
                    self.reasoning()
                        .prove(
                            ProofContext::new(&state.facts, &state.affine),
                            ProofGoal::Affine {
                                inequality: target,
                                right: None,
                            },
                        )
                        .disposition
                        == ProofDisposition::Proved
                });

                for invariant in invariants {
                    self.judge_affine_relation_subscripts(&invariant.relation, state);
                }
                let base = self
                    .reasoning()
                    .prove_loop_invariant_bases(invariants, state);
                let base_batch = base
                    .iter()
                    .all(|disposition| *disposition == TargetDisposition::Proved);

                let mut kills = LoopKills::default();
                let body_reaches_head = self.input.collect_continuing_loop_kills(
                    body,
                    true,
                    &mut LoopReachability::default(),
                    &mut kills,
                );
                if body_reaches_head {
                    // The hidden update is a continuing write exactly when
                    // normal body fallthrough can reach it.
                    kills.push_event_group(vec![KillEvent::Write {
                        place: ResolvedPlace {
                            root: PlaceRoot::Binding(*binder),
                            path: Vec::new(),
                        },
                        element: false,
                        source: node_path.clone(),
                    }]);
                    kills.set_bindings.insert(*binder);
                }
                self.reasoning()
                    .apply_loop_kills(state, &kills, Some(snapshot));

                let invariant_atoms = state
                    .affine
                    .values
                    .values()
                    .chain(state.affine.opaque_values.values())
                    .chain(state.affine.measure_atoms.borrow().values())
                    .flat_map(|form| form.terms().iter().map(|coefficient| coefficient.term()))
                    .collect();

                // Values killed by a possible continuing iteration now read
                // as fresh header atoms.  The written expressions themselves
                // are unchanged; only their program-point value images differ
                // from the base targets above.
                let header_binder = self
                    .reasoning()
                    .new_affine_binding_atom(*binder)
                    .expect("a checked counted binder has one u64 affine value");
                state.affine.values.insert(*binder, header_binder);
                self.reasoning().activate_loop_invariant_batch(
                    *id,
                    invariants,
                    base_batch,
                    &mut state.affine,
                );

                let head = state.clone();
                let invariant_declarations = invariants
                    .iter()
                    .map(|invariant| invariant.declaration)
                    .collect::<Vec<_>>();
                self.frames.loops.push(LoopFrame {
                    id: *id,
                    invariant_declarations: invariant_declarations.clone().into_boxed_slice(),
                    scope_depth: outer_scope_depth,
                    counted_binder: Some(*binder),
                    invariant_atoms,
                    capture_path: Some(range_path.clone()),
                    breaks: Vec::new(),
                });
                let mut body_state = head.clone();
                let outer_continuing = std::mem::take(&mut body_state.continuing);
                let body_event = self
                    .vocabulary
                    .proof_event(FlowEventKind::S11, Some(node_path));
                let counted = self.vocabulary.establish_counted_body_entry(
                    node_path,
                    counted,
                    &mut body_state.facts,
                    body_event,
                );
                self.judging()
                    .retain_counted_derivations(occurrence, counted);
                let body_falls_through = self.walk_block(body, &mut body_state);
                if body_falls_through {
                    debug_assert_summarized(&body_state, &kills);
                }

                let mut step = vec![None; invariants.len()];
                let mut hidden_update = !body_falls_through;
                // A body reaching the backedge normally should still carry the
                // header binder's affine image. Where this walk has lost it,
                // the hidden `binder + 1` update is unproved and every
                // next-header target with it: [OWN-8]'s conservative reading,
                // which withholds the exhaustion rule and the step batch and
                // never widens acceptance.
                let current_binder = body_state.affine.values.get(binder).cloned();
                if body_falls_through && current_binder.is_none() {
                    step = vec![Some(TargetDisposition::Unproved); invariants.len()];
                }
                if let (true, Some(current_binder)) = (body_falls_through, current_binder) {
                    let next_binder = current_binder
                        .add(&AffineForm::constant(1), &mut AffineCheckState::new())
                        .ok();
                    let hidden_target = next_binder.as_ref().and_then(|next| {
                        AffineInequality::from_forms(
                            next,
                            &AffineForm::constant(u64::MAX as i128),
                            &mut AffineCheckState::new(),
                        )
                        .ok()
                    });
                    let counter_limit = self
                        .vocabulary
                        .terms
                        .intern(TermKind::Constant(u64::MAX as i128));
                    hidden_update = hidden_target.as_ref().is_some_and(|target| {
                        self.reasoning()
                            .prove(
                                ProofContext::new(&body_state.facts, &body_state.affine),
                                ProofGoal::Affine {
                                    inequality: target,
                                    right: Some(counter_limit),
                                },
                            )
                            .disposition
                            == ProofDisposition::Proved
                    });
                    // Normalize the next-header target with `binder :=
                    // binder_head + 1`, but retain the old header binding in
                    // the proof state.  The true-header S11 relation constrains
                    // `binder_head`; replacing the live binding first would
                    // make that exact old value unreachable while proving the
                    // backedge target.
                    let mut next_affine = body_state.affine.clone();
                    if let Some(next_binder) = next_binder {
                        next_affine.values.insert(*binder, next_binder);
                    }

                    for (index, invariant) in invariants.iter().enumerate() {
                        let next_target = self.reasoning().checked_loop_invariant_inequality(
                            invariant,
                            &mut next_affine,
                            &mut AffineCheckState::new(),
                        );
                        // [INV-1] both bounds of an `==` next-header target
                        // are proved, in the same substituted state.
                        let next_partner = self
                            .reasoning()
                            .checked_affine_relation_partner(
                                &invariant.relation,
                                &mut next_affine,
                                &mut AffineCheckState::new(),
                            )
                            .map(|partner| partner.ok());
                        let right = self
                            .reasoning()
                            .checked_affine_right_term(&invariant.relation.right);
                        let left = self
                            .reasoning()
                            .checked_affine_right_term(&invariant.relation.left);
                        let mut members = vec![(next_target, right, left)];
                        if let Some(partner) = next_partner {
                            members.push((partner, left, right));
                        }
                        let disposition = self.reasoning().affine_target_disposition(
                            &members,
                            &body_state.facts,
                            &body_state.affine,
                        );
                        // An unrepresentable hidden update fails the step
                        // without refuting the target it would reach.
                        step[index] = Some(if hidden_update {
                            disposition
                        } else {
                            TargetDisposition::Unproved
                        });
                    }
                }

                self.judging().record_loop_invariant_outcomes(
                    *id,
                    invariants,
                    &base,
                    &step,
                    Some(*binder),
                );
                let step_batch = step.iter().all(|disposition| {
                    disposition.is_none_or(|disposition| disposition == TargetDisposition::Proved)
                });
                let export = lower_le_upper && base_batch && step_batch && hidden_update;
                let frame = self.frames.loops.pop();
                let mut breaks = frame.map(|frame| frame.breaks).unwrap_or_default();
                for break_state in &mut breaks {
                    remove_active_loop_invariants(
                        &mut break_state.affine,
                        *id,
                        &invariant_declarations,
                    );
                }

                // Unlike an ordinary loop, the real false-header edge always
                // contributes. Binder and captures leave scope before it or
                // a matching break reaches the continuation.
                let mut exhaustion = head;
                if let Some(upper_affine) = &upper_affine
                    && export
                {
                    let mut normalized = exhaustion.affine.clone();
                    normalized.values.insert(*binder, upper_affine.clone());
                    for (source_ordinal, invariant) in invariants.iter().enumerate() {
                        let Some(inequality) = self.reasoning().checked_loop_invariant_inequality(
                            invariant,
                            &mut normalized,
                            &mut AffineCheckState::new(),
                        ) else {
                            continue;
                        };
                        let partner = self
                            .reasoning()
                            .checked_affine_relation_partner(
                                &invariant.relation,
                                &mut normalized,
                                &mut AffineCheckState::new(),
                            )
                            .and_then(Result::ok);
                        for inequality in std::iter::once(inequality).chain(partner) {
                            if !affine_fact_uses_only_outer_values(
                                &inequality,
                                &normalized,
                                *binder,
                            ) {
                                continue;
                            }
                            let fact = ActiveAffineFact {
                                inequality,
                                evidence: AffineFactEvidence::Source(
                                    SourceAffineFactRef::LoopInvariant(SourceLoopInvariantRef {
                                        loop_id: *id,
                                        source_ordinal: u32::try_from(source_ordinal)
                                            .expect("loop invariant ordinal exceeds u32"),
                                    }),
                                ),
                                active_loops: Vec::new(),
                            };
                            exhaustion.affine.facts.push(fact);
                        }
                    }
                }
                remove_active_loop_invariants(&mut exhaustion.affine, *id, &invariant_declarations);
                self.exit_scopes_to(&mut exhaustion, outer_scope_depth);
                self.vocabulary
                    .exit_counted_capture_scope(&mut exhaustion, &range_path);
                let mut exits = Vec::with_capacity(1 + breaks.len());
                exits.push(exhaustion);
                exits.extend(breaks);
                self.frames.scopes.pop();
                *state = self.judging().join_flows(&exits);
                record_continuing(&mut state.continuing, &outer_continuing);
                true
            }
        }
    }

    /// Walks one match arm from `entry`; establishes the arm-entry facts the
    /// scrutinee admits, applies the arm's scope-exit kills on fall-through,
    /// and returns the arm-exit state when the arm reaches the continuation.
    pub(super) fn walk_arm(
        &mut self,
        arm: &CheckedMatchArm,
        entry: &ProofFlowState,
        facts: &ArmFacts,
        payload: Option<&PayloadPlacement>,
        result: Option<&ResultEvidence>,
    ) -> Option<ProofFlowState> {
        let mut state = entry.clone();
        let s1_event = (!facts.goals.is_empty() || facts.comparison.is_some()).then(|| {
            self.vocabulary
                .proof_event(FlowEventKind::S1, facts.node_path.as_ref())
        });
        self.judging()
            .establish_arm_entry(arm, facts, &mut state.facts, s1_event);
        // [MSR-3] the payload placement's second half: on the arm whose
        // variant carries the payload, the binder that names it has the
        // measures the payload had before the consume.
        if let Some(payload) = payload {
            for (field, carry) in &payload.carried {
                let Some(binder) = arm.binders.iter().find(|binder| binder.field == *field) else {
                    continue;
                };
                let destination = bound_place(binder.binding);
                self.reasoning().establish_measure_datums(
                    &binder.node_path,
                    destination,
                    carry,
                    &mut state.facts,
                );
            }
        }
        if arm.tag == 0
            && let Some(result) = result
            && let Some(binder) = arm
                .binders
                .iter()
                .find(|binder| binder.field == 0 && binder.mode == CheckedMode::Own)
        {
            self.vocabulary.select_result(
                &binder.node_path,
                result,
                binder.binding,
                binder.ty,
                &mut state,
            );
        }
        self.frames
            .scopes
            .push(arm.binders.iter().map(|b| b.binding).collect());
        for binder in &arm.binders {
            if let CheckedType::Integer(ty) = binder.ty
                && matches!(binder.mode, CheckedMode::Own)
            {
                let value = self.vocabulary.new_affine_atom(ty);
                state.affine.values.insert(binder.binding, value);
            }
        }
        let mut continues = true;
        for statement in &arm.body {
            if !continues {
                break;
            }
            continues = self.walk_statement(statement, &mut state);
        }
        if continues {
            let depth = self.frames.scopes.len() - 1;
            self.exit_scopes_to(&mut state, depth);
        }
        self.frames.scopes.pop();
        continues.then_some(state)
    }
}

pub(super) fn expression_node_path(expression: &CheckedExpression) -> Option<&crate::NodePath> {
    expression.carrier()
}
