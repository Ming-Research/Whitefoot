//! Value-associated conditional Result evidence. Each context assumes only
//! its own value is Ok; no operation combines guards of different outcomes.

use super::*;

#[derive(Clone, Debug)]
pub(super) struct ResultEvidence {
    pub(super) payload: TermId,
    pub(super) facts: FactState,
    pub(super) definitely_err: bool,
}

impl Input<'_, '_> {
    fn result_payload_type(&self, ty: CheckedType) -> Option<IntegerType> {
        let CheckedType::Nominal(id) = ty else {
            return None;
        };
        let CheckedNominalKind::Enum { variants } = &self.context.nominals.get(id.0 as usize)?.kind
        else {
            return None;
        };
        let ok = variants.iter().find(|variant| {
            variant.constructor == CheckedConstructor::Prelude(crate::BuiltinPreludeId::OK)
        })?;
        let [field] = ok.fields.as_slice() else {
            return None;
        };
        fragment_type(field.ty)
    }
}

impl Reasoning<'_, '_, '_> {
    fn result_context(&mut self, ty: CheckedType) -> Option<ResultEvidence> {
        let ty = self.input.result_payload_type(ty)?;
        Some(ResultEvidence {
            payload: self.vocabulary.terms.intern(TermKind::ResultPayload(ty)),
            facts: FactState::new(),
            definitely_err: false,
        })
    }
}

impl Vocabulary {
    /// Merge ordinary facts into exactly one conditional context. A guard's
    /// contradiction remains local until that value's success is selected.
    pub(super) fn refresh_result(&mut self, result: &mut ResultEvidence, ordinary: &FactState) {
        let ordinary = self.result_ordinary_snapshot(ordinary);
        self.refresh_result_from_snapshot(result, &ordinary);
    }

    fn result_ordinary_snapshot(&mut self, ordinary: &FactState) -> FactState {
        let mut ordinary = ordinary.clone();
        materialize_closure_before_kill(
            &mut ordinary,
            &self.terms,
            &self.goals,
            &mut self.derivations,
        );
        ordinary
    }

    /// Every Result at one flow point reads the same completed ordinary
    /// snapshot. Sharing its preparation does not combine their guards.
    fn refresh_result_from_snapshot(&self, result: &mut ResultEvidence, ordinary: &FactState) {
        if ordinary.all_derivable {
            result
                .facts
                .promote_to_contradiction(ordinary.contradiction);
        }
        if !result.facts.all_derivable
            && ((result.facts.bounds.is_empty() && result.facts.distinct.is_empty())
                || result.facts.numeric_core_terms() < ordinary.numeric_core_terms())
        {
            // Reuse the ordinary core when it covers more registered terms.
            // Importing all conditional candidates gives the same union as
            // importing ordinary facts into the old context; a different
            // valid witness can win an equal-bound tie.
            let conditional = result.facts.l0_candidates();
            result.facts = ordinary.numeric_snapshot();
            result
                .facts
                .kill(|term| matches!(self.terms.kind(term), TermKind::ResultPayload(_)));
            for (relation, parent) in conditional {
                result
                    .facts
                    .establish_from_proof(&relation, parent, &self.derivations);
            }
            return;
        }
        for (relation, parent) in ordinary.l0_candidates() {
            if relation
                .terms()
                .iter()
                .any(|term| matches!(self.terms.kind(*term), TermKind::ResultPayload(_)))
            {
                continue;
            }
            result
                .facts
                .establish_from_proof(&relation, parent, &self.derivations);
        }
    }

    fn substitute_result_facts(
        &mut self,
        statement: &crate::NodePath,
        source: &FactState,
        from: TermId,
        to: TermId,
    ) -> FactState {
        let mut closed = source.clone();
        materialize_closure_before_kill(
            &mut closed,
            &self.terms,
            &self.goals,
            &mut self.derivations,
        );
        if closed.all_derivable {
            return FactState::contradictory(closed.contradiction.expect("closed contradiction"));
        }
        let mut target = closed.numeric_snapshot();
        target.kill(|term| term == from);
        for (relation, parent) in closed.l0_candidates() {
            if !relation.terms().contains(&from) {
                continue;
            }
            let substituted = replace_relation_term(&relation, from, to);
            let parent = self.derivations.intern(DerivationNode::ResultTransport {
                statement: statement.clone(),
                from,
                to,
                relation: Box::new(substituted.clone()),
                parent,
            });
            target.establish_from_proof(&substituted, parent, &self.derivations);
        }
        target
    }
}

impl Analyzer<'_, '_> {
    /// Read the value before its own consuming transfer. Calls supply their
    /// evidence separately, after the successful call's ordered effects.
    pub(super) fn capture_result(
        &mut self,
        statement: &crate::NodePath,
        expression: &CheckedExpression,
        state: &ProofFlowState,
    ) -> Option<ResultEvidence> {
        if matches!(expression, CheckedExpression::Binding { binding, .. } if is_holder(*binding))
            || matches!(
                expression,
                CheckedExpression::BorrowAddressed { .. }
                    | CheckedExpression::BorrowRangeIndex { .. }
            )
        {
            return None;
        }
        let mut result = self.reasoning().result_context(expression.ty())?;
        match expression {
            CheckedExpression::Binding { binding, .. } if !is_holder(*binding) => {
                if let Some(held) = state.results.get(binding) {
                    result = held.clone();
                }
                self.vocabulary.refresh_result(&mut result, &state.facts);
            }
            CheckedExpression::ConstructEnum {
                variant, fields, ..
            } if *variant == 0 => {
                let [value] = fields.as_slice() else {
                    return Some(result);
                };
                if let Some(term) = self.reasoning().read_operand(value) {
                    result.facts = self.vocabulary.substitute_result_facts(
                        statement,
                        &state.facts,
                        term,
                        result.payload,
                    );
                    // A literal or a previously unmentioned binding needs
                    // its exact value equality as well as its consequences.
                    let event = self
                        .vocabulary
                        .proof_event(FlowEventKind::S5, Some(statement));
                    result.facts.establish(
                        &Relation::Equal {
                            left: result.payload,
                            right: term,
                            difference: 0,
                        },
                        &mut self.vocabulary.derivations,
                        event,
                    );
                }
            }
            CheckedExpression::ConstructEnum { variant, .. } if *variant == 1 => {
                let parent = self
                    .vocabulary
                    .derivations
                    .intern(DerivationNode::ResultErr {
                        statement: statement.clone(),
                    });
                result.facts = FactState::contradictory(parent);
                result.definitely_err = true;
            }
            CheckedExpression::NumericConversion {
                mode: CheckedConversionMode::Checked,
                source: CheckedNumericType::Integer(source),
                destination: CheckedNumericType::Integer(_),
                value,
                ..
            } => {
                // The conditional payload is exactly this evaluated integer,
                // even when its input is outside the tracked-place vocabulary.
                // Existing sources publish only the input's admitted image;
                // no identity is invented for an indirect mutable read.
                self.vocabulary.refresh_result(&mut result, &state.facts);
                let (minimum, maximum) = type_range(*source);
                let event = self
                    .vocabulary
                    .proof_event(FlowEventKind::S5, Some(statement));
                for relation in [
                    Relation::Bound {
                        left: result.payload,
                        right: ZERO,
                        bound: maximum,
                    },
                    Relation::Bound {
                        left: ZERO,
                        right: result.payload,
                        bound: -minimum,
                    },
                ] {
                    result
                        .facts
                        .establish(&relation, &mut self.vocabulary.derivations, event);
                }
                self.establish_value_image(
                    statement,
                    ValueImage::ResultPayload(result.payload),
                    value,
                    &mut result.facts,
                    &mut None,
                );
            }
            // [ENT-5] a checked integer row's success payload is the exact
            // row's mathematical result, with that row's [ENT-3.S7] facts.
            CheckedExpression::IntegerOperation { .. } => {
                self.vocabulary.refresh_result(&mut result, &state.facts);
                self.establish_checked_payload(
                    statement,
                    result.payload,
                    expression,
                    &mut result.facts,
                );
            }
            _ => {}
        }
        Some(result)
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn finish_result(
        &mut self,
        expression: &CheckedExpression,
        judgment: &ExpressionJudgment,
        result: &mut Option<ResultEvidence>,
        state: &ProofFlowState,
    ) {
        if !judgment.reached {
            *result = None;
            return;
        }
        let Some(result) = result else { return };
        let mut kills = Vec::new();
        self.input.collect_expression_kills(expression, &mut kills);
        self.apply_kills_one(&state.separations, &mut result.facts, &kills);
        self.vocabulary.refresh_result(result, &state.facts);
        let (
            Some(prepared),
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                goal_arguments,
                ..
            },
        ) = (&judgment.prepared_call, expression)
        else {
            return;
        };
        for available in &prepared.postconditions {
            if available.variant != Some(crate::BuiltinPreludeId::OK)
                || available.field != Some(crate::BuiltinPreludeId::OK_VALUE)
            {
                continue;
            }
            let Some(instantiated) = self.instantiate_call_postcondition_relation(
                *function,
                call,
                &available.relation,
                arguments,
                goal_arguments,
                &[Some(result.payload)],
                &[],
            ) else {
                continue;
            };
            if !self.s12_substitutions_survive(
                &state.separations,
                &instantiated.substitutions,
                &prepared.kills,
                true,
            ) {
                continue;
            }
            if let Some(parent) =
                self.vocabulary
                    .retain_postcondition_call(&instantiated, available, prepared)
            {
                let occurrence = self.vocabulary.s12_roots;
                self.vocabulary.s12_roots = self
                    .vocabulary
                    .s12_roots
                    .checked_add(1)
                    .expect("S12 root identity fits u32");
                self.vocabulary.derivations.add_root(
                    DerivationRootKind::PostconditionConditional { occurrence },
                    parent,
                );
                result.facts.establish_from_proof(
                    &instantiated.relation,
                    parent,
                    &self.vocabulary.derivations,
                );
            }
        }
    }
}

impl Vocabulary {
    pub(super) fn select_result(
        &mut self,
        statement: &crate::NodePath,
        result: &ResultEvidence,
        binding: BindingId,
        ty: CheckedType,
        state: &mut ProofFlowState,
    ) {
        let Some(receiver) = self.postcondition_place_term(PlaceRoot::Binding(binding), &[], ty)
        else {
            return;
        };
        let mut result = result.clone();
        self.refresh_result(&mut result, &state.facts);
        let selected =
            self.substitute_result_facts(statement, &result.facts, result.payload, receiver);
        if selected.all_derivable {
            state.facts.promote_to_contradiction(selected.contradiction);
        }
        for (relation, parent) in selected.l0_candidates() {
            if relation
                .terms()
                .iter()
                .any(|term| matches!(self.terms.kind(*term), TermKind::ResultPayload(_)))
            {
                continue;
            }
            state
                .facts
                .establish_from_proof(&relation, parent, &self.derivations);
        }
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn kill_result_evidence(
        &mut self,
        state: &mut ProofFlowState,
        events: &[KillEvent],
    ) {
        if events.is_empty() {
            return;
        }
        let mut held = std::mem::take(&mut state.results);
        held.retain(|binding, _| {
            let place = bound_place(*binding);
            if events.iter().any(|event| match event {
                KillEvent::Consume {
                    binding: source, ..
                }
                | KillEvent::EntryImageHolderConsume {
                    binding: source, ..
                } => binding == source,
                KillEvent::Write { place: written, .. }
                | KillEvent::EntryImageHolderWrite { place: written, .. } => self
                    .input
                    .resolved_places_overlap(&state.separations, &place, written),
            }) {
                return false;
            }
            true
        });
        if !held.is_empty() {
            let ordinary = self.vocabulary.result_ordinary_snapshot(&state.facts);
            for result in held.values_mut() {
                self.vocabulary
                    .refresh_result_from_snapshot(result, &ordinary);
                self.apply_kills_one(&state.separations, &mut result.facts, events);
            }
        }
        state.results = held;
    }
}

impl Analyzer<'_, '_> {
    pub(super) fn exit_result_scopes(&mut self, state: &mut ProofFlowState, depth: usize) {
        let exited = self
            .frames
            .scopes
            .iter()
            .skip(depth)
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        let mut held = std::mem::take(&mut state.results);
        held.retain(|binding, _| !exited.contains(binding));
        if !held.is_empty() {
            let ordinary = self.vocabulary.result_ordinary_snapshot(&state.facts);
            for result in held.values_mut() {
                self.vocabulary
                    .refresh_result_from_snapshot(result, &ordinary);
                materialize_closure_before_kill(
                    &mut result.facts,
                    &self.vocabulary.terms,
                    &self.vocabulary.goals,
                    &mut self.vocabulary.derivations,
                );
                self.exit_scopes_to_one(&mut result.facts, depth);
            }
        }
        state.results = held;
    }
}

impl Vocabulary {
    pub(super) fn join_result_evidence(
        &mut self,
        states: &[ProofFlowState],
    ) -> BTreeMap<BindingId, ResultEvidence> {
        let mut bindings = BTreeMap::new();
        for state in states {
            for (&binding, result) in &state.results {
                bindings.entry(binding).or_insert(result.payload);
            }
        }
        let mut results = BTreeMap::new();
        if bindings.is_empty() {
            return results;
        }
        let ordinary = states
            .iter()
            .map(|state| self.result_ordinary_snapshot(&state.facts))
            .collect::<Vec<_>>();
        for (binding, payload) in bindings {
            let mut images = Vec::new();
            for (state, ordinary) in states.iter().zip(&ordinary) {
                let mut result = state
                    .results
                    .get(&binding)
                    .cloned()
                    .unwrap_or(ResultEvidence {
                        payload,
                        facts: FactState::new(),
                        definitely_err: false,
                    });
                self.refresh_result_from_snapshot(&mut result, ordinary);
                images.push(result.facts);
            }
            let event = self.derivations.event(FlowEventKind::Join, None);
            let facts = join_at(
                &images,
                &self.terms,
                &self.goals,
                &mut self.derivations,
                event,
            );
            let definitely_err = states.iter().all(|state| {
                state
                    .results
                    .get(&binding)
                    .is_some_and(|result| result.definitely_err)
            });
            results.insert(
                binding,
                ResultEvidence {
                    payload,
                    facts,
                    definitely_err,
                },
            );
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::entailment::state::PostconditionCallDetail;
    use crate::semantic::entailment::{
        RelationProvenance, VerifiedPostconditionSummary, VerifiedPostconditionSummaryRef,
    };
    use crate::semantic::model::FunctionId;

    /// Compare independently live proofs, not only selected bounds, through
    /// destination collisions, refresh imports and later S12 removal.
    #[test]
    fn numeric_result_transport_preserves_the_complete_candidate_set() {
        let constant_ids = HashMap::new();
        let const_parameter_types = HashMap::new();
        let context = EntailmentContext {
            declarations: &[],
            callees: &[],
            constants: &[],
            constant_ids: &constant_ids,
            const_parameter_types: &const_parameter_types,
            nominals: &[],
            elements: &[],
            contract_queries: &[],
            verified_postconditions: &[],
            verified_postcondition_proofs: &[],
            binding_names: &[],
        };
        let function = CheckedFunction {
            formal_hypothesis: false,
            id: FunctionId(0),
            declaration: crate::DeclarationId::from_index(0).unwrap(),
            module: crate::ModuleId::BUNDLE_ROOT,
            name: String::new(),
            symbol: String::new(),
            function_actuals: Vec::new(),
            region_parameters: Vec::new(),
            parameters: Vec::new(),
            result_mode: CheckedMode::Own,
            result: CheckedType::Unit,
            declared_state_writes: Vec::new(),
            requirements: Vec::new(),
            requirement_places: Vec::new(),
            postconditions: Vec::new(),
            body: None,
            reference_origins: Vec::new(),
            body_disposition: Default::default(),
            allocates: false,
            call_separations: Vec::new(),
            permission_separation_queries: Vec::new(),
            obligations: Vec::new(),
            entailment: FunctionEntailment::default(),
        };
        let mut analyzer = Analyzer::new(&context, &function);
        let from = analyzer
            .vocabulary
            .terms
            .intern(TermKind::ResultPayload(IntegerType::I32));
        let [middle, to] = [0, 1].map(|binding| {
            analyzer.vocabulary.terms.intern(TermKind::Place(
                ResolvedPlace::binding(BindingId(binding)),
                IntegerType::I32,
            ))
        });
        let statement = crate::NodePath {
            components: vec![0],
        };
        let event = analyzer
            .vocabulary
            .derivations
            .event(FlowEventKind::S1, None);
        let mut source = FactState::new();
        for (left, right, bound) in [(from, middle, 5), (to, middle, 10)] {
            source.establish(
                &Relation::Bound { left, right, bound },
                &mut analyzer.vocabulary.derivations,
                event,
            );
        }
        let distinct = Relation::Distinct {
            left: from,
            right: middle,
            difference: 0,
        };
        source.establish(&distinct, &mut analyzer.vocabulary.derivations, event);
        for relation in [
            Relation::Bound {
                left: from,
                right: middle,
                bound: 0,
            },
            distinct,
        ] {
            let proof = analyzer
                .vocabulary
                .derivations
                .intern(DerivationNode::PostconditionCall {
                    detail: Box::new(PostconditionCallDetail {
                        call: statement.clone(),
                        relation: relation.clone(),
                        summary: VerifiedPostconditionSummaryRef {
                            summary: RelationProvenance::Verified(VerifiedPostconditionSummary {
                                function: FunctionId(0),
                                block: statement.clone(),
                                relation_ordinal: 0,
                                component: 0,
                            }),
                        },
                        substitutions: Vec::new(),
                        transfer_events: Vec::new(),
                        parents: Vec::new(),
                    }),
                });
            source.establish_from_proof(&relation, proof, &analyzer.vocabulary.derivations);
        }
        let unseeded = source.clone();
        materialize_closure_before_kill(
            &mut source,
            &analyzer.vocabulary.terms,
            &analyzer.vocabulary.goals,
            &mut analyzer.vocabulary.derivations,
        );
        // Retain alternate witnesses as well as the materialized selection,
        // so both transport paths must keep every distinct proof candidate.
        for (relation, proof) in unseeded.l0_candidates() {
            source.establish_from_proof(&relation, proof, &analyzer.vocabulary.derivations);
        }
        let candidates =
            |state: &FactState| state.l0_candidates().into_iter().collect::<HashSet<_>>();
        assert_eq!(candidates(&source), candidates(&source.numeric_snapshot()));
        let mut transported = analyzer
            .vocabulary
            .substitute_result_facts(&statement, &source, from, to);
        // Reference the previous full import, independent of closed-core reuse.
        let mut rebuilt = FactState::new();
        for (relation, parent) in source.l0_candidates() {
            let substituted = replace_relation_term(&relation, from, to);
            let parent = if relation.terms().contains(&from) {
                analyzer
                    .vocabulary
                    .derivations
                    .intern(DerivationNode::ResultTransport {
                        statement: statement.clone(),
                        from,
                        to,
                        relation: Box::new(substituted.clone()),
                        parent,
                    })
            } else {
                parent
            };
            rebuilt.establish_from_proof(&substituted, parent, &analyzer.vocabulary.derivations);
        }
        for remove_calls in [false, true] {
            if remove_calls {
                transported.retain_non_postcondition_candidates(&analyzer.vocabulary.derivations);
                rebuilt.retain_non_postcondition_candidates(&analyzer.vocabulary.derivations);
            }
            assert_eq!(candidates(&transported), candidates(&rebuilt));
            let actual = close(
                &transported,
                &analyzer.vocabulary.terms,
                &analyzer.vocabulary.goals,
                &mut analyzer.vocabulary.derivations,
            );
            let expected = close(
                &rebuilt,
                &analyzer.vocabulary.terms,
                &analyzer.vocabulary.goals,
                &mut analyzer.vocabulary.derivations,
            );
            assert!(!actual.contradictory());
            assert_eq!(actual.contradictory(), expected.contradictory());
            for left in analyzer.vocabulary.terms.ids() {
                for right in analyzer.vocabulary.terms.ids() {
                    assert_eq!(
                        actual.tight_bound(left, right),
                        expected.tight_bound(left, right)
                    );
                    let distinct = Relation::Distinct {
                        left,
                        right,
                        difference: 0,
                    };
                    assert_eq!(actual.derives(&distinct), expected.derives(&distinct));
                }
            }
            assert!(actual.derives_bound(to, middle, if remove_calls { 5 } else { -1 }));
        }

        let foreign = analyzer
            .vocabulary
            .terms
            .intern(TermKind::ResultPayload(IntegerType::U8));
        let mut ordinary = FactState::new();
        ordinary.establish(
            &Relation::Bound {
                left: to,
                right: middle,
                bound: 8,
            },
            &mut analyzer.vocabulary.derivations,
            event,
        );
        let ordinary = analyzer.vocabulary.result_ordinary_snapshot(&ordinary);
        // Exercise both an unseeded conditional state and a previously
        // closed context whose core predates a newly registered term.
        for conditional in [unseeded, source] {
            let mut refreshed = ResultEvidence {
                payload: from,
                facts: conditional.clone(),
                definitely_err: false,
            };
            assert!(refreshed.facts.numeric_core_terms() < ordinary.numeric_core_terms());
            let mut imported = conditional;
            // The former refresh loop is an independent reference for the union.
            for (relation, parent) in ordinary.l0_candidates() {
                if !relation.terms().iter().any(|term| {
                    matches!(
                        analyzer.vocabulary.terms.kind(*term),
                        TermKind::ResultPayload(_)
                    )
                }) {
                    imported.establish_from_proof(
                        &relation,
                        parent,
                        &analyzer.vocabulary.derivations,
                    );
                }
            }
            analyzer
                .vocabulary
                .refresh_result_from_snapshot(&mut refreshed, &ordinary);
            assert!(closure_is_seeded(&refreshed.facts));
            assert_eq!(
                refreshed.facts.numeric_core_terms(),
                ordinary.numeric_core_terms()
            );
            assert!(
                refreshed
                    .facts
                    .l0_candidates()
                    .iter()
                    .all(|(relation, _)| { !relation.terms().contains(&foreign) })
            );
            for remove_calls in [false, true] {
                if remove_calls {
                    refreshed
                        .facts
                        .retain_non_postcondition_candidates(&analyzer.vocabulary.derivations);
                    imported.retain_non_postcondition_candidates(&analyzer.vocabulary.derivations);
                }
                assert_eq!(candidates(&refreshed.facts), candidates(&imported));
                let actual = close(
                    &refreshed.facts,
                    &analyzer.vocabulary.terms,
                    &analyzer.vocabulary.goals,
                    &mut analyzer.vocabulary.derivations,
                );
                let expected = close(
                    &imported,
                    &analyzer.vocabulary.terms,
                    &analyzer.vocabulary.goals,
                    &mut analyzer.vocabulary.derivations,
                );
                assert!(!actual.contradictory());
                for left in analyzer.vocabulary.terms.ids() {
                    for right in analyzer.vocabulary.terms.ids() {
                        assert_eq!(
                            actual.tight_bound(left, right),
                            expected.tight_bound(left, right)
                        );
                        let distinct = Relation::Distinct {
                            left,
                            right,
                            difference: 0,
                        };
                        assert_eq!(actual.derives(&distinct), expected.derives(&distinct));
                    }
                }
                assert_eq!(actual.tight_bound(to, middle), Some(8));
                assert_eq!(
                    actual.tight_bound(from, middle),
                    Some(if remove_calls { 5 } else { -1 })
                );
            }
            let contradiction = analyzer
                .vocabulary
                .derivations
                .intern(DerivationNode::ResultErr {
                    statement: statement.clone(),
                });
            refreshed.facts = FactState::contradictory(contradiction);
            refreshed.definitely_err = true;
            analyzer
                .vocabulary
                .refresh_result_from_snapshot(&mut refreshed, &ordinary);
            assert!(refreshed.facts.all_derivable);
            assert_eq!(refreshed.facts.contradiction, Some(contradiction));
            assert!(refreshed.definitely_err);
        }
    }
}
