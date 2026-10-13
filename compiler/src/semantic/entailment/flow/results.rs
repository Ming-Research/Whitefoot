//! Value-associated conditional evidence of an enum value: for each of its
//! routes, a variant field, the relations that hold of that field if the
//! value holds that variant. Each context assumes only its own value holds
//! its own variant; no operation combines guards of different outcomes.

use super::*;
use crate::semantic::postcondition::PayloadRoute;

#[derive(Clone, Debug)]
pub(super) struct ResultEvidence {
    /// The routed field's type, which the context's private root is typed
    /// by [ENT-2] clause (i).
    pub(super) payload: CheckedType,
    /// The variant field this context is restricted to; an arm for the
    /// variant's tag that binds the field selects it.
    pub(super) route: PayloadRoute,
    /// Whether this is the success route of a Result or an Option, which
    /// `propagate` selects [ERR-3].
    pub(super) success: bool,
    pub(super) facts: FactState,
    /// The value definitely holds another variant.
    pub(super) definitely_err: bool,
}

/// [ENT-5] one route of an own enum type: a variant field whose type supplies
/// data under [CALL-4] or is a struct with type invariants [TYPE-11].
#[derive(Clone, Copy)]
pub(super) struct SuccessRoute {
    pub(super) payload: CheckedType,
    pub(super) route: PayloadRoute,
    pub(super) success: bool,
    /// An integer-payload Result has a context for every value, as it always
    /// has. Every other admitted type receives one where evidence is created:
    /// an empty context and a missing association mean the same [ENT-5], so
    /// a value no success construction or routed call reaches needs none.
    pub(super) eager: bool,
}

impl SuccessRoute {
    /// Whether `evidence` is this route's context.
    pub(super) fn holds(&self, evidence: &ResultEvidence) -> bool {
        evidence.route.variant == self.route.variant && evidence.route.field == self.route.field
    }
}

impl Input<'_, '_> {
    /// [ENT-5] the routes of a local own enum value, in variant and then
    /// field order: each field whose type supplies data under [CALL-4] (a
    /// fragment integer, measured type, or aggregate reaching one through
    /// struct fields and `Box` content), and each field whose type is a
    /// struct with type invariants [TYPE-11].
    pub(super) fn payload_routes(&self, ty: CheckedType) -> Vec<SuccessRoute> {
        let CheckedType::Nominal(id) = ty else {
            return Vec::new();
        };
        let Some(CheckedNominalKind::Enum { variants }) = self
            .context
            .nominals
            .get(id.0 as usize)
            .map(|record| &record.kind)
        else {
            return Vec::new();
        };
        let mut routes = Vec::new();
        for (index, variant) in variants.iter().enumerate() {
            let Ok(variant_index) = u32::try_from(index) else {
                return Vec::new();
            };
            let success = matches!(
                variant.constructor,
                CheckedConstructor::Prelude(
                    crate::BuiltinPreludeId::OK | crate::BuiltinPreludeId::SOME
                )
            );
            for (position, field) in variant.fields.iter().enumerate() {
                let Ok(field_index) = u32::try_from(position) else {
                    return Vec::new();
                };
                let success = success && variant.fields.len() == 1;
                let invariant = matches!(
                    field.ty,
                    CheckedType::Nominal(payload)
                        if self
                            .context
                            .nominals
                            .get(payload.0 as usize)
                            .is_some_and(|record| record.type_invariant)
                );
                if !invariant && !self.payload_supplies_data(field.ty) {
                    continue;
                }
                routes.push(SuccessRoute {
                    payload: field.ty,
                    route: PayloadRoute {
                        variant: variant_index,
                        field: field_index,
                        tag: variant.tag,
                    },
                    success,
                    eager: success
                        && variant.constructor
                            == CheckedConstructor::Prelude(crate::BuiltinPreludeId::OK)
                        && fragment_type(field.ty).is_some(),
                });
            }
        }
        routes
    }

    /// [CALL-4] whether a value of this type supplies a datum.
    fn payload_supplies_data(&self, ty: CheckedType) -> bool {
        if fragment_type(ty).is_some() || measured_kind(ty).is_some() {
            return true;
        }
        let mut pending = vec![ty];
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            let CheckedType::Nominal(nominal) = ty else {
                continue;
            };
            if !seen.insert(nominal) {
                continue;
            }
            let children = match self
                .context
                .nominals
                .get(nominal.0 as usize)
                .map(|record| &record.kind)
            {
                Some(CheckedNominalKind::Struct { fields }) => {
                    fields.iter().map(|field| field.ty).collect::<Vec<_>>()
                }
                Some(CheckedNominalKind::Box { referent, .. }) => vec![*referent],
                _ => Vec::new(),
            };
            for child in children {
                if fragment_type(child).is_some() || measured_kind(child).is_some() {
                    return true;
                }
                pending.push(child);
            }
        }
        false
    }

    /// The type an owned descendant projection of struct-field and `Box`
    /// content steps reaches below a payload type [CALL-4], if every step
    /// selects one.
    pub(super) fn payload_path_type(
        &self,
        mut ty: CheckedType,
        path: &[PlaceStep],
    ) -> Option<CheckedType> {
        for step in path {
            let CheckedType::Nominal(nominal) = ty else {
                return None;
            };
            ty = match (&self.context.nominals.get(nominal.0 as usize)?.kind, step) {
                (CheckedNominalKind::Struct { fields }, PlaceStep::Field(field)) => {
                    fields.get(*field as usize)?.ty
                }
                (CheckedNominalKind::Box { referent, .. }, PlaceStep::Deref) => *referent,
                _ => return None,
            };
        }
        Some(ty)
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn result_context(&mut self, route: SuccessRoute) -> ResultEvidence {
        ResultEvidence {
            payload: route.payload,
            route: route.route,
            success: route.success,
            facts: FactState::new(),
            definitely_err: false,
        }
    }

    /// [ENT-2] clause (i) the term of a payload root that stands for the
    /// value of the fragment-integer place `path` reaches, or for one
    /// measure of the measured place it reaches.
    pub(super) fn payload_root_term(
        &mut self,
        payload: CheckedType,
        path: &[PlaceStep],
        measure: Option<CheckedMeasure>,
    ) -> Option<TermId> {
        let reached = self.input.payload_path_type(payload, path)?;
        let ty = match measure {
            Some(_) => {
                measured_kind(reached)?;
                IntegerType::U64
            }
            None => fragment_type(reached)?,
        };
        Some(self.vocabulary.terms.intern(TermKind::ResultPayload {
            payload,
            path: path.to_vec(),
            measure,
            ty,
        }))
    }

    /// The pairs a variant construction substitutes [ENT-5]: each already
    /// formed term of the payload place, its own value or a place or measure
    /// its owned descendant projection reaches, with the root term standing
    /// for it. A term outside the existing vocabulary contributes nothing.
    fn payload_pairs(
        &mut self,
        payload: CheckedType,
        value: &CheckedExpression,
    ) -> Vec<(TermId, TermId)> {
        if fragment_type(payload).is_some() {
            let Some(term) = self.read_operand(value) else {
                return Vec::new();
            };
            return self
                .payload_root_term(payload, &[], None)
                .map(|root| vec![(term, root)])
                .unwrap_or_default();
        }
        let Some(source) = self.input.placement_source_place(value) else {
            return Vec::new();
        };
        let source = source.term_identity();
        let mut found = Vec::new();
        for term in self.vocabulary.terms.ids() {
            let (place, measure) = match self.vocabulary.terms.kind(term) {
                TermKind::Place(place, _) => (place, None),
                TermKind::Measure(measure, place) => (place, Some(*measure)),
                _ => continue,
            };
            if place.root != source.root {
                continue;
            }
            let Some(path) = place.path.strip_prefix(source.path.as_slice()) else {
                continue;
            };
            if !path
                .iter()
                .all(|step| matches!(step, PlaceStep::Field(_) | PlaceStep::Deref))
            {
                continue;
            }
            found.push((term, path.to_vec(), measure));
        }
        let mut pairs = Vec::with_capacity(found.len());
        for (term, path, measure) in found {
            if let Some(root) = self.payload_root_term(payload, &path, measure) {
                pairs.push((term, root));
            }
        }
        pairs
    }
}

impl Vocabulary {
    /// Merge ordinary facts into exactly one conditional context. A guard's
    /// contradiction remains local until that value's success is selected.
    pub(super) fn refresh_result(&mut self, result: &mut ResultEvidence, ordinary: &FactState) {
        // Contradiction already proves every conditional relation. In particular,
        // an excluded variant needs no ordinary closure prepared on its behalf.
        if result.facts.all_derivable {
            return;
        }
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
        if result.facts.all_derivable {
            return;
        }
        if ordinary.all_derivable {
            result
                .facts
                .promote_to_contradiction(ordinary.contradiction);
            return;
        }
        if (result.facts.bounds.is_empty() && result.facts.distinct.is_empty())
            || result.facts.numeric_core_terms() < ordinary.numeric_core_terms()
        {
            // Reuse the ordinary core when it covers more registered terms.
            // Importing all conditional candidates gives the same union as
            // importing ordinary facts into the old context; a different
            // valid witness can win an equal-bound tie.
            let conditional = result.facts.l0_candidates();
            result.facts = ordinary.numeric_snapshot();
            result.facts.kill(|term| {
                self.terms.any_leaf(term, &|leaf| {
                    matches!(self.terms.kind(leaf), TermKind::ResultPayload { .. })
                })
            });
            for (relation, parent) in conditional {
                result
                    .facts
                    .establish_from_proof(&relation, parent, &self.derivations);
            }
            return;
        }
        for (relation, parent) in ordinary.l0_candidates() {
            if relation.terms().iter().any(|term| {
                self.terms.any_leaf(*term, &|leaf| {
                    matches!(self.terms.kind(leaf), TermKind::ResultPayload { .. })
                })
            }) {
                continue;
            }
            result
                .facts
                .establish_from_proof(&relation, parent, &self.derivations);
        }
    }

    /// Substitutes every pair's second term for its first in the closed
    /// facts of `source`, recording one transport step per substituted term.
    fn substitute_result_facts(
        &mut self,
        statement: &crate::NodePath,
        source: &FactState,
        pairs: &[(TermId, TermId)],
    ) -> FactState {
        let pairs = self.terms.expand_substitutions(pairs);
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
        let substituted_term = |term: TermId| pairs.iter().any(|(from, _)| *from == term);
        let mut target = closed.numeric_snapshot();
        target.kill(substituted_term);
        for (relation, parent) in closed.l0_candidates() {
            if !relation.terms().iter().copied().any(substituted_term) {
                continue;
            }
            let mut substituted = relation;
            let mut parent = parent;
            for (from, to) in &pairs {
                if !substituted.terms().contains(from) {
                    continue;
                }
                substituted = replace_relation_term(&substituted, *from, *to);
                parent = self.derivations.intern(DerivationNode::ResultTransport {
                    statement: statement.clone(),
                    from: *from,
                    to: *to,
                    relation: Box::new(substituted.clone()),
                    parent,
                });
            }
            target.establish_from_proof(&substituted, parent, &self.derivations);
        }
        target
    }
}

impl Analyzer<'_, '_> {
    /// Read the value before its own consuming transfer, one context per
    /// route of its type. Calls supply their evidence separately, after the
    /// successful call's ordered effects.
    pub(super) fn capture_result(
        &mut self,
        statement: &crate::NodePath,
        expression: &CheckedExpression,
        state: &ProofFlowState,
    ) -> Vec<ResultEvidence> {
        if matches!(expression, CheckedExpression::Binding { binding, .. } if is_holder(*binding))
            || matches!(
                expression,
                CheckedExpression::BorrowAddressed { .. }
                    | CheckedExpression::BorrowRangeIndex { .. }
            )
        {
            return Vec::new();
        }
        let mut captured = Vec::new();
        for route in self.input.payload_routes(expression.ty()) {
            if let Some(result) = self.capture_route(statement, expression, state, route) {
                captured.push(result);
            }
        }
        captured
    }

    fn capture_route(
        &mut self,
        statement: &crate::NodePath,
        expression: &CheckedExpression,
        state: &ProofFlowState,
        route: SuccessRoute,
    ) -> Option<ResultEvidence> {
        let mut result = self.reasoning().result_context(route);
        match expression {
            CheckedExpression::Binding { binding, .. } => {
                if let Some(held) = state
                    .results
                    .get(binding)
                    .and_then(|held| held.iter().find(|held| route.holds(held)))
                {
                    result = held.clone();
                } else if !route.eager {
                    return None;
                }
                self.vocabulary.refresh_result(&mut result, &state.facts);
            }
            CheckedExpression::ConstructEnum {
                variant, fields, ..
            } if *variant == route.route.variant => {
                let Some(value) = fields.get(route.route.field as usize) else {
                    return Some(result);
                };
                let pairs = self.reasoning().payload_pairs(route.payload, value);
                if pairs.is_empty() {
                    return route.eager.then_some(result);
                }
                result.facts =
                    self.vocabulary
                        .substitute_result_facts(statement, &state.facts, &pairs);
                // A literal or a previously unmentioned binding needs its
                // exact value equality as well as its consequences.
                let event = self
                    .vocabulary
                    .proof_event(FlowEventKind::S5, Some(statement));
                for (term, root) in pairs {
                    result.facts.establish(
                        &Relation::Equal {
                            left: root,
                            right: term,
                            difference: 0,
                        },
                        &mut self.vocabulary.derivations,
                        event,
                    );
                }
            }
            CheckedExpression::ConstructEnum { .. } => {
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
            } if route.success => {
                // The conditional payload is exactly this evaluated integer,
                // even when its input is outside the tracked-place vocabulary.
                // Existing sources publish only the input's admitted image;
                // no identity is invented for an indirect mutable read.
                let payload = self
                    .reasoning()
                    .payload_root_term(route.payload, &[], None)?;
                self.vocabulary.refresh_result(&mut result, &state.facts);
                let (minimum, maximum) = type_range(*source);
                let event = self
                    .vocabulary
                    .proof_event(FlowEventKind::S5, Some(statement));
                for relation in [
                    Relation::Bound {
                        left: payload,
                        right: ZERO,
                        bound: maximum,
                    },
                    Relation::Bound {
                        left: ZERO,
                        right: payload,
                        bound: -minimum,
                    },
                ] {
                    result
                        .facts
                        .establish(&relation, &mut self.vocabulary.derivations, event);
                }
                self.establish_value_image(
                    statement,
                    ValueImage::ResultPayload(payload),
                    value,
                    &mut result.facts,
                    &mut None,
                );
            }
            // [ENT-5] a checked integer row's success payload is the exact
            // row's mathematical result, with that row's [ENT-3.S7] facts.
            CheckedExpression::IntegerOperation { .. } if route.success => {
                let payload = self
                    .reasoning()
                    .payload_root_term(route.payload, &[], None)?;
                self.vocabulary.refresh_result(&mut result, &state.facts);
                self.establish_checked_payload(statement, payload, expression, &mut result.facts);
            }
            _ if !route.eager => return None,
            _ => {}
        }
        Some(result)
    }
}

impl Analyzer<'_, '_> {
    /// [TYPE-11, ENT-3.S4] each routed requirement enters the body in its
    /// parameter's own conditional context for its route, never as an
    /// ordinary fact: the condition is that the argument holds the route's
    /// variant, and a contradictory invariant stays local to that context.
    pub(super) fn establish_routed_requirements(&mut self, state: &mut ProofFlowState) {
        let function = self.input.function;
        for requirement in &function.routed_requirements {
            let Some(parameter) = function.parameters.get(requirement.subject as usize) else {
                continue;
            };
            let Some(route) = self
                .input
                .payload_routes(parameter.ty)
                .into_iter()
                .find(|route| {
                    route.route.variant == requirement.route.variant
                        && route.route.field == requirement.route.field
                })
            else {
                continue;
            };
            let results = [self
                .reasoning()
                .payload_root_term(requirement.payload, &[], None)];
            let returns = [Some(PostconditionReturnDatum::ResultPayload {
                ty: requirement.payload,
            })];
            let Some(relation) = self.reasoning().instantiate_relation_template(
                &requirement.relation,
                &results,
                &returns,
            ) else {
                continue;
            };
            let event = self
                .vocabulary
                .proof_event(FlowEventKind::S4, Some(&requirement.clause));
            let context = self.reasoning().result_context(route);
            let held = state.results.entry(parameter.binding).or_default();
            if !held.iter().any(|result| route.holds(result)) {
                held.push(context);
                held.sort_by_key(|result| (result.route.variant, result.route.field));
            }
            if let Some(result) = held.iter_mut().find(|result| route.holds(result)) {
                result
                    .facts
                    .establish(&relation, &mut self.vocabulary.derivations, event);
            }
        }
    }
}

impl Analyzer<'_, '_> {
    /// [TYPE-11, FN-8] the goal one routed requirement poses at a call and
    /// the facts it is judged in: the invariant over the place below the
    /// argument's payload step, and the ordinary facts with the argument's
    /// context for the route, each private root term standing for that
    /// place's corresponding term [ENT-5]. An argument with no context for
    /// the route contributes the ordinary facts alone.
    ///
    /// A bare binding is the root its context is selected onto. Any other
    /// argument, a field or element read or a moved field, is rooted where
    /// the call's ordinary requirements root it: `image`, the argument's
    /// admitted pre-transfer image [FN-8], or, when that image cannot carry
    /// the payload step, the argument's own occurrence-local value, which
    /// no fact names.
    pub(super) fn routed_requirement_goal(
        &mut self,
        call: &crate::NodePath,
        argument: &CheckedExpression,
        image: Option<GoalExpression>,
        requirement: &crate::semantic::postcondition::CheckedRoutedRequirement,
        state: &ProofFlowState,
    ) -> (ConcreteGoal, FactState) {
        let binding = match argument {
            CheckedExpression::Binding { binding, .. } => Some(*binding),
            _ => None,
        };
        let step = GoalProjection::Payload {
            variant: requirement.route.tag,
            field: requirement.route.field,
        };
        let root = match binding {
            Some(binding) => PayloadGoalRoot::Binding(binding),
            None => PayloadGoalRoot::Image(
                image
                    .filter(|image| {
                        matches!(
                            image,
                            GoalExpression::Datum(
                                GoalDatum::Place { .. } | GoalDatum::EvaluatedValue { .. }
                            )
                        )
                    })
                    .unwrap_or_else(|| {
                        GoalExpression::Datum(GoalDatum::EvaluatedValue {
                            function: self.input.function.id,
                            occurrence: EvaluatedValueOccurrence::CallArgument {
                                call: call.clone(),
                                argument: requirement.subject,
                            },
                            captured_type: argument.ty(),
                            projections: Vec::new(),
                            ty: argument.ty(),
                        })
                    }),
            ),
        };
        let goal = ConcreteGoal::new(payload_goal(&requirement.goal.root, &root, step));
        let context = self
            .capture_result(call, argument, state)
            .into_iter()
            .find(|result| {
                result.route.variant == requirement.route.variant
                    && result.route.field == requirement.route.field
            });
        let (Some(binding), Some(context)) = (binding, context) else {
            return (goal, state.facts.clone());
        };
        let facts = self
            .reasoning()
            .payload_place_facts(call, &context, binding, &state.facts);
        (goal, facts)
    }
}

/// What a routed requirement's payload step is taken below at a call.
enum PayloadGoalRoot {
    /// A bare binding argument, the place its context is selected onto.
    Binding(BindingId),
    /// Any other argument's admitted image or occurrence-local value: a
    /// place or an evaluated-value datum.
    Image(GoalExpression),
}

/// [TYPE-11] a type invariant's predicate over parameter zero, read over
/// the place `step` reaches below `root` instead.
fn payload_goal(
    expression: &GoalExpression,
    root: &PayloadGoalRoot,
    step: GoalProjection,
) -> GoalExpression {
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: 0,
            projections,
            ty,
        }) => match root {
            PayloadGoalRoot::Binding(binding) => GoalExpression::Datum(GoalDatum::Place {
                root: *binding,
                projections: std::iter::once(step)
                    .chain(projections.iter().copied())
                    .collect(),
                ty: *ty,
                admission: GoalAdmission::default(),
            }),
            // Each step is appended as an ordinary requirement's projection
            // is appended to its admitted actual [FN-8]; only the last
            // step's type is the datum's.
            PayloadGoalRoot::Image(image) => std::iter::once(step)
                .chain(projections.iter().copied())
                .try_fold(image.clone(), |datum, projection| {
                    datum.with_projection(projection, *ty)
                })
                .expect("a place or evaluated value takes every projection"),
        },
        GoalExpression::Datum(datum) => GoalExpression::Datum(datum.clone()),
        GoalExpression::Operation {
            row,
            type_arguments,
            const_arguments,
            result,
            arguments,
        } => GoalExpression::Operation {
            row: *row,
            type_arguments: type_arguments.clone(),
            const_arguments: const_arguments.clone(),
            result: *result,
            arguments: arguments
                .iter()
                .map(|argument| payload_goal(argument, root, step))
                .collect(),
        },
    }
}

impl Reasoning<'_, '_, '_> {
    /// [TYPE-11, ENT-5] one context's facts with each private root term
    /// replaced by the corresponding term of the place below `binding`'s
    /// payload step for the context's route, over the current ordinary
    /// facts. The result is a scratch state for one judgment; nothing in it
    /// becomes an ordinary fact.
    fn payload_place_facts(
        &mut self,
        statement: &crate::NodePath,
        result: &ResultEvidence,
        binding: BindingId,
        ordinary: &FactState,
    ) -> FactState {
        let mut result = result.clone();
        self.vocabulary.refresh_result(&mut result, ordinary);
        let step = PlaceStep::Payload {
            variant: result.route.tag,
            field: result.route.field,
        };
        let pairs = self.payload_receiver_pairs(&result, binding, Some(step));
        self.vocabulary
            .substitute_result_facts(statement, &result.facts, &pairs)
    }

    /// Each private root term of `result`'s payload with the corresponding
    /// term of the receiving place: `binding` itself, or the place `step`
    /// reaches below it, extended by the root term's projection.
    fn payload_receiver_pairs(
        &mut self,
        result: &ResultEvidence,
        binding: BindingId,
        step: Option<PlaceStep>,
    ) -> Vec<(TermId, TermId)> {
        let mut roots = Vec::new();
        for term in self.vocabulary.terms.ids() {
            if let TermKind::ResultPayload {
                payload,
                path,
                measure,
                ty,
            } = self.vocabulary.terms.kind(term)
                && *payload == result.payload
            {
                roots.push((term, path.clone(), *measure, *ty));
            }
        }
        let mut pairs = Vec::with_capacity(roots.len());
        for (root, path, measure, ty) in roots {
            let place = ResolvedPlace {
                atomic_aliases: Vec::new(),
                root: PlaceRoot::Binding(binding),
                path: step.into_iter().chain(path.iter().cloned()).collect(),
            };
            let receiver = match measure {
                None => Some(self.vocabulary.terms.intern(TermKind::Place(place, ty))),
                Some(measure) => self
                    .input
                    .payload_path_type(result.payload, &path)
                    .and_then(|reached| {
                        let measured = measured_kind(reached)?;
                        Some(self.place_measure_term(
                            measure,
                            place,
                            measured,
                            type_constant(reached),
                        ))
                    }),
            };
            if let Some(receiver) = receiver {
                pairs.push((root, receiver));
            }
        }
        pairs
    }

    pub(super) fn finish_result(
        &mut self,
        expression: &CheckedExpression,
        judgment: &ExpressionJudgment,
        results: &mut Vec<ResultEvidence>,
        state: &ProofFlowState,
    ) {
        if !judgment.reached {
            results.clear();
            return;
        }
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
            for result in results.iter_mut() {
                let mut kills = Vec::new();
                self.input.collect_expression_kills(expression, &mut kills);
                self.apply_kills_one(&state.separations, &mut result.facts, &kills);
                self.vocabulary.refresh_result(result, &state.facts);
            }
            return;
        };
        let routes = self.input.payload_routes(expression.ty());
        // [ENT-5] each routed relation is restricted to the context of the
        // variant field it is routed by, whether written or generated by
        // a type invariant [TYPE-11].
        let routed = prepared
            .postconditions
            .iter()
            .filter_map(|available| {
                let wanted = available.route?;
                let route = routes.iter().find(|route| {
                    route.route.variant == wanted.variant && route.route.field == wanted.field
                })?;
                Some((*route, available.clone()))
            })
            .collect::<Vec<_>>();
        // [ENT-5] a routed call's value receives its context here, where the
        // call establishes the relations it restricts to it; a type whose
        // contexts are created only by evidence has none before.
        for (route, _) in &routed {
            if !results.iter().any(|result| route.holds(result)) {
                let context = self.result_context(*route);
                results.push(context);
            }
        }
        results.sort_by_key(|result| (result.route.variant, result.route.field));
        let mut kills = Vec::new();
        self.input.collect_expression_kills(expression, &mut kills);
        for result in results.iter_mut() {
            self.apply_kills_one(&state.separations, &mut result.facts, &kills);
            self.vocabulary.refresh_result(result, &state.facts);
        }
        for (route, available) in &routed {
            let Some(result) = results.iter_mut().find(|result| route.holds(result)) else {
                continue;
            };
            let destination = [Some(ResultDestination::PayloadRoot(result.payload))];
            let Some(instantiated) = self.instantiate_call_postcondition_relation(
                *function,
                call,
                &available.relation,
                arguments,
                goal_arguments,
                &destination,
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

    /// [ENT-5] an own match's success arm and propagate's successful
    /// continuation select the evaluated outcome's context: each root term
    /// is replaced by the corresponding term of the receiving binding, the
    /// binding itself for the payload's own value and the place or measure
    /// its projection reaches otherwise.
    pub(super) fn select_result(
        &mut self,
        statement: &crate::NodePath,
        result: &ResultEvidence,
        binding: BindingId,
        state: &mut ProofFlowState,
    ) {
        let mut result = result.clone();
        self.vocabulary.refresh_result(&mut result, &state.facts);
        let pairs = self.payload_receiver_pairs(&result, binding, None);
        let selected = self
            .vocabulary
            .substitute_result_facts(statement, &result.facts, &pairs);
        if selected.all_derivable {
            state.facts.promote_to_contradiction(selected.contradiction);
        }
        for (relation, parent) in selected.l0_candidates() {
            if relation.terms().iter().any(|term| {
                self.vocabulary.terms.any_leaf(*term, &|leaf| {
                    matches!(
                        self.vocabulary.terms.kind(leaf),
                        TermKind::ResultPayload { .. }
                    )
                })
            }) {
                continue;
            }
            state
                .facts
                .establish_from_proof(&relation, parent, &self.vocabulary.derivations);
            self.vocabulary
                .establish_postcondition_affine(&relation, parent, &mut state.affine);
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
                KillEvent::Write {
                    place: written,
                    relocation,
                    ..
                }
                | KillEvent::EntryImageHolderWrite {
                    place: written,
                    relocation,
                    ..
                } => {
                    self.input
                        .write_kills_support(&state.separations, &place, written, *relocation)
                }
            }) {
                return false;
            }
            true
        });
        if held
            .values()
            .flatten()
            .any(|result| !result.facts.all_derivable)
        {
            let ordinary = self.vocabulary.result_ordinary_snapshot(&state.facts);
            for result in held.values_mut().flatten() {
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
        if held
            .values()
            .flatten()
            .any(|result| !result.facts.all_derivable)
        {
            let ordinary = self.vocabulary.result_ordinary_snapshot(&state.facts);
            for result in held.values_mut().flatten() {
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
    ) -> BTreeMap<BindingId, Vec<ResultEvidence>> {
        let mut keys = BTreeMap::new();
        for state in states {
            for (&binding, held) in &state.results {
                for result in held {
                    keys.entry((binding, result.route.variant, result.route.field))
                        .or_insert((result.payload, result.route, result.success));
                }
            }
        }
        let mut results: BTreeMap<BindingId, Vec<ResultEvidence>> = BTreeMap::new();
        if keys.is_empty() {
            return results;
        }
        // A predecessor contributes its ordinary closure only to routes that
        // are not already contradictory there. Keep excluded route images for
        // the join's neutral predecessor and contradiction-parent semantics.
        let mut ordinary = vec![None; states.len()];
        fn find(
            state: &ProofFlowState,
            binding: BindingId,
            route: PayloadRoute,
        ) -> Option<&ResultEvidence> {
            state.results.get(&binding).and_then(|held| {
                held.iter().find(|result| {
                    result.route.variant == route.variant && result.route.field == route.field
                })
            })
        }
        for ((binding, _, _), (payload, route, success)) in keys {
            let mut images = Vec::new();
            for (state, ordinary) in states.iter().zip(&mut ordinary) {
                let mut result = find(state, binding, route)
                    .cloned()
                    .unwrap_or(ResultEvidence {
                        payload,
                        route,
                        success,
                        facts: FactState::new(),
                        definitely_err: false,
                    });
                if !result.facts.all_derivable {
                    let ordinary =
                        ordinary.get_or_insert_with(|| self.result_ordinary_snapshot(&state.facts));
                    self.refresh_result_from_snapshot(&mut result, ordinary);
                }
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
                find(state, binding, route).is_some_and(|result| result.definitely_err)
            });
            results.entry(binding).or_default().push(ResultEvidence {
                payload,
                route,
                success,
                facts,
                definitely_err,
            });
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
        with_vocabulary(|vocabulary| {
            let from = vocabulary.terms.intern(TermKind::ResultPayload {
                payload: CheckedType::Integer(IntegerType::I32),
                path: Vec::new(),
                measure: None,
                ty: IntegerType::I32,
            });
            let [middle, to] = [0, 1].map(|binding| {
                vocabulary.terms.intern(TermKind::Place(
                    ResolvedPlace::binding(BindingId(binding)),
                    IntegerType::I32,
                ))
            });
            let statement = crate::NodePath {
                components: vec![0],
            };
            let event = vocabulary.derivations.event(FlowEventKind::S1, None);
            let mut source = FactState::new();
            for (left, right, bound) in [(from, middle, 5), (to, middle, 10)] {
                source.establish(
                    &Relation::Bound { left, right, bound },
                    &mut vocabulary.derivations,
                    event,
                );
            }
            let distinct = Relation::Distinct {
                left: from,
                right: middle,
                difference: 0,
            };
            source.establish(&distinct, &mut vocabulary.derivations, event);
            for relation in [
                Relation::Bound {
                    left: from,
                    right: middle,
                    bound: 0,
                },
                distinct,
            ] {
                let proof = vocabulary
                    .derivations
                    .intern(DerivationNode::PostconditionCall {
                        detail: Box::new(PostconditionCallDetail {
                            call: statement.clone(),
                            relation: relation.clone(),
                            summary: VerifiedPostconditionSummaryRef {
                                summary: RelationProvenance::Verified(
                                    VerifiedPostconditionSummary {
                                        function: FunctionId(0),
                                        block: statement.clone(),
                                        relation_ordinal: 0,
                                        component: 0,
                                    },
                                ),
                            },
                            substitutions: Vec::new(),
                            transfer_events: Vec::new(),
                            parents: Vec::new(),
                        }),
                    });
                source.establish_from_proof(&relation, proof, &vocabulary.derivations);
            }
            let unseeded = source.clone();
            materialize_closure_before_kill(
                &mut source,
                &vocabulary.terms,
                &vocabulary.goals,
                &mut vocabulary.derivations,
            );
            // Retain alternate witnesses as well as the materialized selection,
            // so both transport paths must keep every distinct proof candidate.
            for (relation, proof) in unseeded.l0_candidates() {
                source.establish_from_proof(&relation, proof, &vocabulary.derivations);
            }
            let candidates =
                |state: &FactState| state.l0_candidates().into_iter().collect::<HashSet<_>>();
            assert_eq!(candidates(&source), candidates(&source.numeric_snapshot()));
            let mut transported =
                vocabulary.substitute_result_facts(&statement, &source, &[(from, to)]);
            // Reference the previous full import, independent of closed-core reuse.
            let mut rebuilt = FactState::new();
            for (relation, parent) in source.l0_candidates() {
                let substituted = replace_relation_term(&relation, from, to);
                let parent = if relation.terms().contains(&from) {
                    vocabulary
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
                rebuilt.establish_from_proof(&substituted, parent, &vocabulary.derivations);
            }
            for remove_calls in [false, true] {
                if remove_calls {
                    transported.retain_non_postcondition_candidates(&vocabulary.derivations);
                    rebuilt.retain_non_postcondition_candidates(&vocabulary.derivations);
                }
                assert_eq!(candidates(&transported), candidates(&rebuilt));
                let actual = close(
                    &transported,
                    &vocabulary.terms,
                    &vocabulary.goals,
                    &mut vocabulary.derivations,
                );
                let expected = close(
                    &rebuilt,
                    &vocabulary.terms,
                    &vocabulary.goals,
                    &mut vocabulary.derivations,
                );
                assert!(!actual.contradictory());
                assert_eq!(actual.contradictory(), expected.contradictory());
                for left in vocabulary.terms.ids() {
                    for right in vocabulary.terms.ids() {
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

            let foreign = vocabulary.terms.intern(TermKind::ResultPayload {
                payload: CheckedType::Integer(IntegerType::U8),
                path: Vec::new(),
                measure: None,
                ty: IntegerType::U8,
            });
            let mut ordinary = FactState::new();
            ordinary.establish(
                &Relation::Bound {
                    left: to,
                    right: middle,
                    bound: 8,
                },
                &mut vocabulary.derivations,
                event,
            );
            let ordinary = vocabulary.result_ordinary_snapshot(&ordinary);
            // Exercise both an unseeded conditional state and a previously
            // closed context whose core predates a newly registered term.
            for conditional in [unseeded, source] {
                let mut refreshed = ResultEvidence {
                    payload: CheckedType::Integer(IntegerType::I32),
                    route: PayloadRoute {
                        variant: 0,
                        field: 0,
                        tag: 0,
                    },
                    success: true,
                    facts: conditional.clone(),
                    definitely_err: false,
                };
                assert!(refreshed.facts.numeric_core_terms() < ordinary.numeric_core_terms());
                let mut imported = conditional;
                // The former refresh loop is an independent reference for the union.
                for (relation, parent) in ordinary.l0_candidates() {
                    if !relation.terms().iter().any(|term| {
                        matches!(vocabulary.terms.kind(*term), TermKind::ResultPayload { .. })
                    }) {
                        imported.establish_from_proof(&relation, parent, &vocabulary.derivations);
                    }
                }
                vocabulary.refresh_result_from_snapshot(&mut refreshed, &ordinary);
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
                            .retain_non_postcondition_candidates(&vocabulary.derivations);
                        imported.retain_non_postcondition_candidates(&vocabulary.derivations);
                    }
                    assert_eq!(candidates(&refreshed.facts), candidates(&imported));
                    let actual = close(
                        &refreshed.facts,
                        &vocabulary.terms,
                        &vocabulary.goals,
                        &mut vocabulary.derivations,
                    );
                    let expected = close(
                        &imported,
                        &vocabulary.terms,
                        &vocabulary.goals,
                        &mut vocabulary.derivations,
                    );
                    assert!(!actual.contradictory());
                    for left in vocabulary.terms.ids() {
                        for right in vocabulary.terms.ids() {
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
                let contradiction = vocabulary.derivations.intern(DerivationNode::ResultErr {
                    statement: statement.clone(),
                });
                refreshed.facts = FactState::contradictory(contradiction);
                refreshed.definitely_err = true;
                vocabulary.refresh_result_from_snapshot(&mut refreshed, &ordinary);
                assert!(refreshed.facts.all_derivable);
                assert_eq!(refreshed.facts.contradiction, Some(contradiction));
                assert!(refreshed.definitely_err);
            }
        });
    }

    #[test]
    fn excluded_routes_skip_ordinary_snapshots_and_remain_neutral_at_joins() {
        with_vocabulary(|vocabulary| {
            let [left, middle, right] = [0, 1, 2].map(|binding| {
                vocabulary.terms.intern(TermKind::Place(
                    ResolvedPlace::binding(BindingId(binding)),
                    IntegerType::I32,
                ))
            });
            let payload = vocabulary.terms.intern(TermKind::ResultPayload {
                payload: CheckedType::Integer(IntegerType::I32),
                path: Vec::new(),
                measure: None,
                ty: IntegerType::I32,
            });
            let event = vocabulary.derivations.event(FlowEventKind::S1, None);
            let mut ordinary = FactState::new();
            for (left, right, bound) in [(left, middle, 2), (middle, right, 3)] {
                ordinary.establish(
                    &Relation::Bound { left, right, bound },
                    &mut vocabulary.derivations,
                    event,
                );
            }
            let contradiction = vocabulary.derivations.intern(DerivationNode::ResultErr {
                statement: crate::NodePath {
                    components: vec![0],
                },
            });
            let mut excluded = ResultEvidence {
                payload: CheckedType::Integer(IntegerType::I32),
                route: PayloadRoute {
                    variant: 0,
                    field: 0,
                    tag: 0,
                },
                success: false,
                facts: FactState::contradictory(contradiction),
                definitely_err: true,
            };
            let snapshots = |vocabulary: &Vocabulary| {
                vocabulary
                    .derivations
                    .events
                    .iter()
                    .filter(|event| event.kind == FlowEventKind::Snapshot)
                    .count()
            };
            let before = snapshots(vocabulary);
            vocabulary.refresh_result(&mut excluded, &ordinary);
            assert_eq!(snapshots(vocabulary), before);
            assert_eq!(excluded.facts.contradiction, Some(contradiction));
            assert!(excluded.facts.l0_candidates().is_empty());
            assert!(excluded.definitely_err);

            let holder = BindingId(3);
            let edge = |result: Option<ResultEvidence>| ProofFlowState {
                facts: ordinary.clone(),
                results: result
                    .map(|result| BTreeMap::from([(holder, vec![result])]))
                    .unwrap_or_default(),
                ..ProofFlowState::default()
            };
            let excluded_edge = edge(Some(excluded.clone()));
            let joined =
                vocabulary.join_result_evidence(&[excluded_edge.clone(), excluded_edge.clone()]);
            assert_eq!(snapshots(vocabulary), before);
            let joined = &joined[&holder][0];
            assert!(joined.facts.all_derivable);
            assert!(joined.definitely_err);
            assert!(matches!(
                &vocabulary.derivations.nodes[joined.facts.contradiction.unwrap().0 as usize],
                DerivationNode::JoinContradiction { parents, .. }
                    if parents.len() == 2
                        && parents.iter().all(|parent| parent.parent == contradiction)
            ));

            let mut live = excluded;
            live.facts = FactState::new();
            live.definitely_err = false;
            live.facts.establish(
                &Relation::Bound {
                    left: payload,
                    right: ZERO,
                    bound: 5,
                },
                &mut vocabulary.derivations,
                event,
            );
            let live_edge = edge(Some(live));
            for missing in [false, true] {
                let mut edges = vec![live_edge.clone(), excluded_edge.clone()];
                if missing {
                    edges.push(edge(None));
                }
                let joined = vocabulary.join_result_evidence(&edges);
                let joined = &joined[&holder][0];
                assert!(!joined.definitely_err);
                let closed = close(
                    &joined.facts,
                    &vocabulary.terms,
                    &vocabulary.goals,
                    &mut vocabulary.derivations,
                );
                assert!(!closed.contradictory());
                assert_eq!(closed.derives_bound(payload, ZERO, 5), !missing);
                assert!(closed.derives_bound(left, right, 5));
            }
            // The ordinary chain really needs materialization: the zero-snapshot
            // assertions above cannot pass merely because their input was empty.
            assert!(snapshots(vocabulary) > before);

            let mut live = edge(None);
            let mut selected = ResultEvidence {
                facts: FactState::new(),
                definitely_err: false,
                ..joined.clone()
            };
            live.facts = FactState::contradictory(contradiction);
            vocabulary.refresh_result(&mut selected, &live.facts);
            assert!(selected.facts.all_derivable);
            assert_eq!(selected.facts.contradiction, Some(contradiction));
            assert!(!selected.definitely_err);
        });
    }

    fn with_vocabulary(test: impl FnOnce(&mut Vocabulary)) {
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
            component_postconditions: &[],
            binding_names: &[],
        };
        let function = CheckedFunction {
            formal_hypothesis: false,
            summary_source: None,
            prelude_element: None,
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
            declared_state_reads: Vec::new(),
            requirements: Vec::new(),
            requirement_places: Vec::new(),
            routed_requirements: Vec::new(),
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
        test(&mut analyzer.vocabulary);
    }
}
