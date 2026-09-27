//! [FN-9] postconditions: the relation proved at every selected exit,
//! the entry images it reads, and the [ENT-3] S12 publication of a
//! verified relation at a call.

use super::*;

impl Input<'_, '_> {
    pub(super) fn postcondition_affine_target(
        &self,
        postcondition: &CheckedPostcondition,
        result: &AffineForm,
        state: &AffineFlowState,
    ) -> Option<Vec<AffineInequality>> {
        let operands = postcondition
            .relation
            .operands
            .iter()
            .map(|operand| {
                self.postcondition_affine_datum(&operand.datum, result, state)?
                    .add(
                        &AffineForm::constant(operand.displacement),
                        &mut AffineCheckState::new(),
                    )
                    .ok()
            })
            .collect::<Option<Vec<_>>>()?;
        let (left, right, strict, equal) = match postcondition.relation.normalized {
            NormalizedRelation::UpperBound {
                left,
                right,
                strict,
            } => (left, right, strict, false),
            NormalizedRelation::Equal => (0, 1, false, true),
            NormalizedRelation::NotEqual => return None,
        };
        let left = operands.get(left as usize)?;
        let right = operands.get(right as usize)?;
        let right = if strict {
            right
                .subtract(&AffineForm::constant(1), &mut AffineCheckState::new())
                .ok()?
        } else {
            right.clone()
        };
        affine_ordering_targets(left, &right, equal)
    }

    pub(super) fn postcondition_affine_datum(
        &self,
        datum: &RelationDatum,
        result: &AffineForm,
        state: &AffineFlowState,
    ) -> Option<AffineForm> {
        match datum {
            RelationDatum::Result {
                ty: CheckedType::Integer(_),
                ..
            } => Some(result.clone()),
            RelationDatum::Parameter {
                ordinal,
                projections,
                ty: CheckedType::Integer(_),
            } if projections.is_empty() => {
                let binding = self.function.parameters.get(*ordinal as usize)?.binding;
                state.values.get(&binding).cloned()
            }
            RelationDatum::NamedConst {
                declaration,
                projections,
                ty: CheckedType::Integer(_),
            } if projections.is_empty() => self
                .context
                .constant(*declaration)
                .and_then(|constant| postcondition_affine_constant(&constant.value)),
            RelationDatum::Literal { value, .. } => postcondition_affine_constant(value),
            RelationDatum::Result { .. }
            | RelationDatum::Parameter { .. }
            | RelationDatum::NamedConst { .. }
            | RelationDatum::Measure(..) => None,
        }
    }

    pub(super) fn postcondition_return_place_root(
        &self,
        root: PostconditionReturnPlaceRoot,
    ) -> Option<PlaceRoot> {
        match root {
            PostconditionReturnPlaceRoot::Binding(binding) => Some(PlaceRoot::Binding(binding)),
            PostconditionReturnPlaceRoot::NamedConst(declaration) => Some(PlaceRoot::Constant(
                *self.context.constant_ids.get(&declaration)?,
            )),
        }
    }

    /// [TYPE-8, REF-4] whether the formal at this ordinal is a `&[T]`, whose
    /// one [MSR-1] row is the range's and not the element type's.
    pub(super) fn formal_is_range(&self, ordinal: u32) -> bool {
        self.function
            .parameters
            .get(ordinal as usize)
            .is_some_and(|parameter| parameter.mode == CheckedMode::Range)
    }

    pub(super) fn available_postconditions(
        &self,
        function: super::super::super::model::FunctionId,
    ) -> Vec<AvailablePostcondition> {
        self.context
            .verified_postconditions(function)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(postcondition, proof)| {
                Some(AvailablePostcondition {
                    relation: postcondition.relation.clone(),
                    variant: postcondition.selector.variant,
                    field: postcondition
                        .selector
                        .field
                        .as_ref()
                        .map(|field| field.declaration),
                    authority: super::super::RelationProvenance::Verified(proof.summary.clone()?),
                })
            })
            .collect()
    }

    /// The relations one exact call may publish [FN-5]. A direct call uses
    /// its actual's verified FN-9 surface. A bound call uses the retained
    /// formal surface only when its exact FN-4 query is discharged and every
    /// actual premise selected by that query has an earlier-component FN-9
    /// summary. A zero-premise implication needs only its retained query.
    pub(super) fn available_call_postconditions(
        &self,
        function: super::super::super::model::FunctionId,
        formal: Option<&super::super::super::model::CheckedCallContract>,
    ) -> Vec<AvailablePostcondition> {
        let Some(formal) = formal else {
            return self.available_postconditions(function);
        };
        formal
            .postconditions
            .iter()
            .filter_map(|boundary| {
                let query = self.context.contract_query(boundary.query)?;
                let [outcome] = query.proof.contract_goals.as_slice() else {
                    return None;
                };
                if outcome.disposition != CallGoalDisposition::Discharged
                    || outcome.derivation.is_none()
                    || query.instance != Some(self.function.id)
                    || query.goal != boundary.selector.block
                    || query.premises.len() != boundary.actual_premises.len()
                {
                    return None;
                }
                let premises = boundary
                    .actual_premises
                    .iter()
                    .zip(&query.premises)
                    .map(|(ordinal, clause)| {
                        let (postcondition, proof) =
                            self.context.verified_postcondition(function, *ordinal)?;
                        let summary = proof.summary.as_ref()?;
                        (postcondition.selector.block == *clause
                            && summary.function == function
                            && summary.relation_ordinal == *ordinal)
                            .then(|| summary.clone())
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(AvailablePostcondition {
                    relation: boundary.relation.clone(),
                    variant: boundary.selector.variant,
                    field: boundary
                        .selector
                        .field
                        .as_ref()
                        .map(|field| field.declaration),
                    authority: super::super::RelationProvenance::FormalBoundary {
                        query: boundary.query,
                        actual: function,
                        premises,
                    },
                })
            })
            .collect()
    }

    /// [REF-1, MSR-2] the reference variables a place reads through.
    ///
    /// A measure term's support contains every reference variable any prefix
    /// of its place reads through, so ending one of those bindings ends the
    /// fact. v0.59 walked a holder chain; v0.60 asks the reference summary,
    /// which is where a reference's path set now lives.
    pub(super) fn append_holder_chain(&self, binding: BindingId, holders: &mut Vec<BindingId>) {
        if !self.places.is_reference(binding) {
            return;
        }
        if !holders.contains(&binding) {
            holders.push(binding);
        }
    }

    pub(super) fn collect_checked_argument_holders(
        &self,
        argument: &CheckedExpression,
        holders: &mut Vec<BindingId>,
    ) {
        match argument {
            CheckedExpression::Binding { binding, .. }
            | CheckedExpression::Project { binding, .. }
            | CheckedExpression::DerefAddressed { binding, .. } => {
                self.append_holder_chain(*binding, holders);
            }
            CheckedExpression::BorrowAddressed { root, .. } => {
                if let Some(binding) = root.binding() {
                    self.append_holder_chain(binding, holders);
                }
            }
            CheckedExpression::BufferMeasure { root, .. } => {
                self.append_holder_chain(root.binding, holders);
            }
            CheckedExpression::RangeMeasure { root, .. } => {
                self.append_holder_chain(root.binding, holders);
            }
            CheckedExpression::RangeElementMeasure { place, .. }
            | CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                self.append_holder_chain(place.root.binding, holders);
            }
            CheckedExpression::RangeOf { source, .. } => {
                if let Some(binding) = source.binding() {
                    self.append_holder_chain(binding, holders);
                }
            }
            CheckedExpression::ArrayMeasure {
                root: CheckedArrayRoot::Binding { binding, .. },
                ..
            } => self.append_holder_chain(*binding, holders),
            // These checked wrappers are one read of their nested place. They
            // do not create a second consume, but M must retain the holder on
            // which the resulting caller image depends.
            CheckedExpression::BoxDeref { value, .. }
            | CheckedExpression::ProjectValue { value, .. } => {
                self.collect_checked_argument_holders(value, holders);
            }
            _ => {}
        }
    }

    pub(super) fn collect_goal_image_holders(
        &self,
        argument: &GoalExpression,
        holders: &mut Vec<BindingId>,
    ) {
        match argument {
            GoalExpression::Datum(GoalDatum::Place {
                root, projections, ..
            }) => {
                let support = GoalSupport {
                    root: *root,
                    projections: projections.clone(),
                    measure: None,
                };
                let (_, image_holders) = self.resolve_goal_support(&support);
                for holder in image_holders {
                    if !holders.contains(&holder) {
                        holders.push(holder);
                    }
                }
            }
            GoalExpression::Operation { arguments, .. } => {
                for argument in arguments {
                    self.collect_goal_image_holders(argument, holders);
                }
            }
            GoalExpression::Datum(_) => {}
        }
    }

    pub(super) fn call_argument_holder_chain(
        &self,
        argument: &CheckedExpression,
        goal_argument: &GoalExpression,
    ) -> Vec<BindingId> {
        let mut holders = Vec::new();
        self.collect_checked_argument_holders(argument, &mut holders);
        self.collect_goal_image_holders(goal_argument, &mut holders);
        holders
    }

    /// Whether one write definitely replaces the target selected by a live
    /// reference holder.
    ///
    /// A postcondition substitution that still reads a holder dies when that
    /// holder is rebound. A write below its referent does not rebind it: the
    /// ordinary term-support judgment below decides whether that field,
    /// element, descriptor word, or whole-value write reaches the term. The
    /// former prefix-overlap test conflated those questions and discarded a
    /// window-length relation on `swap(&deref(holder)[i], ...)` even though
    /// [MSR-2] makes the indexed element disjoint from the descriptor.
    pub(super) fn write_replaces_live_holder(
        &self,
        holder: BindingId,
        written: &ResolvedPlace,
    ) -> bool {
        if written.root == PlaceRoot::Binding(holder) && written.path.is_empty() {
            return true;
        }
        let holder_targets = self.places.resolve(PlaceRoot::Binding(holder), &[]);
        let written_targets = self.places.resolve(written.root, &written.path);
        if holder_targets.is_empty() || written_targets.is_empty() {
            return true;
        }
        let same_target = |left: &ResolvedPlace, right: &ResolvedPlace| {
            left.contains(right) && right.contains(left)
        };
        holder_targets.len() == written_targets.len()
            && holder_targets.iter().all(|holder| {
                written_targets
                    .iter()
                    .any(|write| same_target(holder, write))
            })
            && written_targets.iter().all(|write| {
                holder_targets
                    .iter()
                    .any(|holder| same_target(holder, write))
            })
    }

    pub(super) fn call_parameter_place(
        &self,
        actual: &GoalExpression,
        projections: &[GoalProjection],
    ) -> Option<(PlaceRoot, Vec<GoalProjection>)> {
        let GoalExpression::Datum(datum) = actual else {
            return None;
        };
        let (root, actual_projections) = match datum {
            GoalDatum::Place {
                root, projections, ..
            } => (PlaceRoot::Binding(*root), projections),
            GoalDatum::NamedConst {
                declaration,
                projections,
                ..
            } => (
                PlaceRoot::Constant(*self.context.constant_ids.get(declaration)?),
                projections,
            ),
            GoalDatum::Parameter { .. }
            | GoalDatum::EvaluatedValue { .. }
            | GoalDatum::Literal(_) => return None,
        };
        Some((
            root,
            actual_projections
                .iter()
                .chain(projections)
                .copied()
                .collect(),
        ))
    }

    pub(super) fn receiver_argument_overlaps(
        &self,
        separations: &dyn SeparationOracle,
        expression: &CheckedExpression,
        receiver: &ResolvedPlace,
    ) -> bool {
        if self
            .read_place_path(expression)
            .is_some_and(|place| self.resolved_places_overlap(separations, &place, receiver))
        {
            return true;
        }
        if self
            .argument_referents(expression)
            .iter()
            .any(|(place, _)| self.resolved_places_overlap(separations, place, receiver))
        {
            return true;
        }
        expression_children(expression)
            .into_iter()
            .any(|child| self.receiver_argument_overlaps(separations, child, receiver))
    }

    pub(super) fn direct_receiver_route(
        &self,
        separations: &dyn SeparationOracle,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        prepared: &PreparedCall,
    ) -> Option<DirectReceiverRoute> {
        let CheckedSetTarget::Place(target) = target else {
            return None;
        };
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            result,
            ..
        } = value
        else {
            return None;
        };
        if *function != prepared.callee
            || *call != prepared.call
            || !target.fields.is_empty()
            || is_holder(target.binding)
            || *result != target.ty
            || fragment_type(target.ty).is_none()
        {
            return None;
        }
        let receiver = ResolvedPlace {
            root: PlaceRoot::Binding(target.binding),
            path: Vec::new(),
        };
        let mut selected = None;
        for (formal, argument) in arguments.iter().enumerate() {
            let exact = matches!(
                argument,
                CheckedExpression::Binding {
                    binding,
                    ty,
                    consume_root: false,
                    ..
                } if *binding == target.binding && *ty == target.ty
            );
            if exact {
                if selected.is_some() {
                    return None;
                }
                selected = Some(
                    u32::try_from(formal)
                        .expect("call argument ordinal exceeds the u32 identity space"),
                );
            } else if self.receiver_argument_overlaps(separations, argument, &receiver) {
                return None;
            }
        }
        Some(DirectReceiverRoute {
            binding: target.binding,
            formal: selected?,
            ty: target.ty,
        })
    }

    pub(super) fn collect_postcondition_entry_images(&mut self) {
        let mut data = Vec::new();
        let mut relation_images = Vec::with_capacity(self.function.postconditions.len());
        for postcondition in &self.function.postconditions {
            let mut indices = Vec::new();
            for operand in &postcondition.relation.operands {
                let datum = match &operand.datum {
                    RelationDatum::Parameter {
                        ordinal,
                        projections,
                        ty,
                    } => Some((
                        PostconditionEntryImage {
                            parameter: *ordinal,
                            projections: projections.clone(),
                            measure: None,
                        },
                        *ty,
                    )),
                    RelationDatum::Measure(measure, place) => match place.root {
                        PostconditionPlaceRoot::Parameter { ordinal } => Some((
                            PostconditionEntryImage {
                                parameter: ordinal,
                                projections: place.projections.clone(),
                                measure: Some(*measure),
                            },
                            place.ty,
                        )),
                        // A result place is not a parameter entry image.
                        PostconditionPlaceRoot::Result { .. }
                        | PostconditionPlaceRoot::ExitParameter { .. } => None,
                    },
                    RelationDatum::Result { .. }
                    | RelationDatum::NamedConst { .. }
                    | RelationDatum::Literal { .. } => None,
                };
                if let Some(datum) = datum {
                    let index = data
                        .iter()
                        .position(|existing: &(PostconditionEntryImage, CheckedType)| {
                            existing.0 == datum.0
                        })
                        .unwrap_or_else(|| {
                            let index = data.len();
                            data.push(datum);
                            index
                        });
                    if !indices.contains(&index) {
                        indices.push(index);
                    }
                }
            }
            relation_images.push(indices);
        }
        self.entry_images = data
            .into_iter()
            .map(|(datum, ty)| {
                let parameter = self
                    .function
                    .parameters
                    .get(datum.parameter as usize)
                    .expect("checked postcondition parameter ordinal must resolve");
                let projections = self
                    .body_projections(PlaceRoot::Binding(parameter.binding), &datum.projections)
                    .to_vec();
                let support = GoalSupport {
                    root: parameter.binding,
                    projections,
                    measure: datum.measure,
                };
                let (place, holders) = self.resolve_goal_support(&support);
                EntryImageRecord {
                    datum,
                    ty,
                    place,
                    holders,
                }
            })
            .collect();
        self.postcondition_entry_images = relation_images;
    }
}

impl Vocabulary {
    /// [MSR-4] the affine target of one already-instantiated ordering
    /// relation, read through the affine image of each of its two terms.
    ///
    /// A measure term, a measure datum and an integer binding each have an
    /// image, so this reaches every relation whose operands the affine domain
    /// carries — which is what a clause over a run's measures is.
    pub(super) fn affine_relation_target(
        &mut self,
        relation: &Relation,
        state: &AffineFlowState,
    ) -> Option<Vec<AffineInequality>> {
        let (left, right, difference, equal) = match relation {
            Relation::Bound { left, right, bound } => (*left, *right, *bound, false),
            Relation::Equal {
                left,
                right,
                difference,
            } => (*left, *right, *difference, true),
            _ => return None,
        };
        let left = self.affine_term_value(left, state)?;
        let right = self.affine_term_value(right, state)?;
        let mut check = AffineCheckState::new();
        let right = right
            .add(&AffineForm::constant(difference), &mut check)
            .ok()?;
        affine_ordering_targets(&left, &right, equal)
    }

    pub(super) fn postcondition_place_term(
        &mut self,
        root: PlaceRoot,
        projections: &[GoalProjection],
        ty: CheckedType,
    ) -> Option<TermId> {
        let fragment = fragment_type(ty)?;
        let projections = projections
            .iter()
            .map(|projection| projection.place_step())
            .collect::<Vec<_>>();
        let path = ResolvedPlace {
            root,
            path: projections,
        };
        Some(self.terms.intern(TermKind::Place(path, fragment)))
    }

    /// Whether one term is already immutable with empty support, so that no
    /// [ENT-5] event can change what it denotes [ENT-2, MSR-3].
    pub(super) fn immortal_term(&self, term: TermId) -> bool {
        matches!(
            self.terms.kind(term),
            TermKind::Zero
                | TermKind::Constant(_)
                | TermKind::ConstParameter(..)
                | TermKind::CountedCapture { .. }
                | TermKind::IndexCapture { .. }
                | TermKind::ResultPayload(_)
                | TermKind::CommitValue { .. }
                | TermKind::CallDatum { .. }
        )
    }

    /// The already-minted call datum of one `own` operand, when this call
    /// established one [MSR-3].
    pub(super) fn interned_call_datum(
        &self,
        call: &crate::NodePath,
        formal: u32,
        projections: &[GoalProjection],
        measure: Option<CheckedMeasure>,
        ty: CheckedType,
    ) -> Option<TermId> {
        let datum_type = if measure.is_some() {
            super::super::super::model::IntegerType::U64
        } else {
            fragment_type(ty)?
        };
        self.terms.interned(&call_datum_kind(
            call,
            formal,
            projections,
            measure,
            datum_type,
        ))
    }

    pub(super) fn retain_postcondition_call(
        &mut self,
        instantiated: &InstantiatedPostcondition,
        available: &AvailablePostcondition,
        prepared: &PreparedCall,
    ) -> Option<DerivationId> {
        let summary = selected_call_summary(available)?;
        Some(
            self.derivations
                .intern(super::super::state::DerivationNode::PostconditionCall {
                    detail: Box::new(super::super::state::PostconditionCallDetail {
                        call: prepared.call.clone(),
                        relation: instantiated.relation.clone(),
                        summary,
                        substitutions: instantiated.substitutions.clone(),
                        transfer_events: prepared.transfer_events.clone(),
                        parents: prepared.parents.clone(),
                    }),
                }),
        )
    }

    pub(super) fn retain_direct_result(
        &mut self,
        statement: &crate::NodePath,
        binding: BindingId,
        instantiated: &InstantiatedPostcondition,
        available: &AvailablePostcondition,
        prepared: &PreparedCall,
        state: &mut FactState,
    ) {
        let Some(call) = self.retain_postcondition_call(instantiated, available, prepared) else {
            return;
        };
        let route = self.derivations.intern(
            super::super::state::DerivationNode::PostconditionDirectResult {
                statement: statement.clone(),
                binding,
                relation: Box::new(instantiated.relation.clone()),
                parent: call,
            },
        );
        let occurrence = self.s12_roots;
        self.s12_roots = self
            .s12_roots
            .checked_add(1)
            .expect("S12 roots exceed the u32 identity space");
        self.derivations.add_root(
            DerivationRootKind::PostconditionDirectResult { occurrence },
            route,
        );
        state.establish_from_proof(&instantiated.relation, route, &self.derivations);
    }

    pub(super) fn retain_direct_receiver(
        &mut self,
        statement: &crate::NodePath,
        candidate: &DirectReceiverCandidate,
        target_event: FlowEventId,
        prepared: &PreparedCall,
        state: &mut FactState,
    ) {
        let Some(call) =
            self.retain_postcondition_call(&candidate.instantiated, &candidate.available, prepared)
        else {
            return;
        };
        let proof = self.derivations.intern(
            super::super::state::DerivationNode::PostconditionDirectReceiver {
                statement: statement.clone(),
                binding: candidate.route.binding,
                receiver_formal: candidate.route.formal,
                relation: Box::new(candidate.instantiated.relation.clone()),
                target_event,
                parent: call,
            },
        );
        let occurrence = self.s12_roots;
        self.s12_roots = self
            .s12_roots
            .checked_add(1)
            .expect("S12 roots exceed the u32 identity space");
        self.derivations.add_root(
            DerivationRootKind::PostconditionDirectReceiver { occurrence },
            proof,
        );
        state.establish_from_proof(&candidate.instantiated.relation, proof, &self.derivations);
    }

    pub(super) fn establish_direct_receiver(
        &mut self,
        statement: &crate::NodePath,
        candidate: &DirectReceiverCandidate,
        prepared: &PreparedCall,
        target_event: FlowEventId,
        states: &mut ProofFlowState,
    ) {
        self.retain_direct_receiver(
            statement,
            candidate,
            target_event,
            prepared,
            &mut states.facts,
        );
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn retain_postcondition_aggregate(
        &mut self,
        block: &crate::NodePath,
        relation_ordinal: u32,
        parents: Option<Vec<DerivationId>>,
    ) -> PostconditionAggregate {
        if self.input.function.formal_hypothesis || self.input.function.body.is_none() {
            let node = self.vocabulary.derivations.intern(
                super::super::state::DerivationNode::SignatureContract {
                    block: block.clone(),
                    relation_ordinal,
                },
            );
            self.vocabulary.derivations.add_root(
                DerivationRootKind::PostconditionAggregate { relation_ordinal },
                node,
            );
            return PostconditionAggregate {
                discharged: true,
                derivation: Some(node),
            };
        }
        let Some(parents) = parents.filter(|parents| !parents.is_empty()) else {
            return PostconditionAggregate {
                discharged: false,
                derivation: None,
            };
        };
        let node = self.vocabulary.derivations.intern(
            super::super::state::DerivationNode::PostconditionAggregate {
                block: block.clone(),
                relation_ordinal,
                parents,
            },
        );
        self.vocabulary.derivations.add_root(
            DerivationRootKind::PostconditionAggregate { relation_ordinal },
            node,
        );
        PostconditionAggregate {
            discharged: true,
            derivation: Some(node),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn judge_postcondition(
        &mut self,
        relation_ordinal: u32,
        occurrence: usize,
        statement: &crate::NodePath,
        relation: &Relation,
        state: &FactState,
        affine_target: Option<&[AffineInequality]>,
        affine_state: &AffineFlowState,
        unavailable: bool,
    ) -> PostconditionExitProof {
        let context = ProofContext::new(state, affine_state);
        if unavailable {
            return PostconditionExitProof {
                disposition: PostconditionDisposition::Unproved,
                derivation: None,
            };
        }
        let proof = self.prove(
            context,
            ProofGoal::Ordering {
                relation,
                affine: affine_target,
            },
        );
        if proof.disposition == ProofDisposition::Proved {
            let parent = proof
                .derivation
                .expect("a proved postcondition relation must retain its local derivation");
            let node = self.vocabulary.derivations.intern(
                super::super::state::DerivationNode::PostconditionExit {
                    statement: statement.clone(),
                    relation_ordinal,
                    relation: Box::new(relation.clone()),
                    parent,
                },
            );
            self.vocabulary.derivations.add_root(
                DerivationRootKind::PostconditionExit {
                    relation_ordinal,
                    occurrence: u32::try_from(occurrence)
                        .expect("postcondition exits exceed the u32 identity space"),
                },
                node,
            );
            PostconditionExitProof {
                disposition: PostconditionDisposition::Discharged,
                derivation: Some(node),
            }
        } else {
            PostconditionExitProof {
                disposition: if proof.disposition == ProofDisposition::Refuted {
                    PostconditionDisposition::Refuted
                } else {
                    PostconditionDisposition::Unproved
                },
                derivation: None,
            }
        }
    }

    /// Converts one already-substituted callable-boundary ordering predicate
    /// to its unique affine inequality. Unsupported goal shapes simply retain
    /// the ordinary L0 result; no alternate formula is guessed.
    pub(super) fn affine_goal_ordering_target(
        &mut self,
        expression: &GoalExpression,
        state: &AffineFlowState,
    ) -> Option<AffineInequality> {
        self.affine_signed_goal_ordering_target(expression, state, GoalSign::Positive)
    }

    /// Retains the written right operand after the comparison and truth sign
    /// choose their fixed orientation. A compound operand without an L0 term
    /// has no right bridge; its coefficient vector cannot invent one.
    pub(super) fn signed_goal_right_term(
        &mut self,
        expression: &GoalExpression,
        sign: GoalSign,
    ) -> Option<TermId> {
        let GoalExpression::Operation {
            row: GoalOperation::Integer { operation, .. },
            arguments,
            ..
        } = expression
        else {
            return None;
        };
        let [left, right] = arguments.as_slice() else {
            return None;
        };
        let reversed = match operation {
            CheckedIntegerOperation::Less | CheckedIntegerOperation::LessEqual => false,
            CheckedIntegerOperation::Greater | CheckedIntegerOperation::GreaterEqual => true,
            _ => return None,
        } ^ (sign == GoalSign::Negative);
        self.goal_side(if reversed { left } else { right })
            .map(|(term, _)| term)
    }

    /// Converts either truth sign of one callable-boundary ordering leaf to
    /// its unique affine inequality. Boolean composition uses this same leaf
    /// normalization instead of adding a call-specific affine fallback.
    pub(super) fn affine_signed_goal_ordering_target(
        &mut self,
        expression: &GoalExpression,
        state: &AffineFlowState,
        sign: GoalSign,
    ) -> Option<AffineInequality> {
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation,
                    operand_type: CheckedType::Integer(_),
                },
            arguments,
            result: CheckedType::Bool,
            ..
        } = expression
        else {
            return None;
        };
        let [written_left, written_right] = arguments.as_slice() else {
            return None;
        };
        let left = self.affine_goal_value(written_left, state)?;
        let right = self.affine_goal_value(written_right, state)?;
        let mut check = AffineCheckState::new();
        let (left, right, strict) = match operation {
            CheckedIntegerOperation::LessEqual => (left, right, false),
            CheckedIntegerOperation::Less => (left, right, true),
            CheckedIntegerOperation::GreaterEqual => (right, left, false),
            CheckedIntegerOperation::Greater => (right, left, true),
            _ => return None,
        };
        let (left, right, strict) = match sign {
            GoalSign::Positive => (left, right, strict),
            // Integer order is total: not(left <= right) is right < left,
            // while not(left < right) is right <= left.
            GoalSign::Negative => (right, left, !strict),
        };
        let right = if strict {
            right.subtract(&AffineForm::constant(1), &mut check).ok()?
        } else {
            right
        };
        AffineInequality::from_forms(&left, &right, &mut check).ok()
    }

    /// Reads the mathematical value of the fixed affine subset admitted in a
    /// concrete call goal. Every place must be an unprojected current integer
    /// binding, and multiplication must have a literal/constant side.
    pub(super) fn affine_goal_value(
        &mut self,
        expression: &GoalExpression,
        state: &AffineFlowState,
    ) -> Option<AffineForm> {
        match expression {
            GoalExpression::Datum(GoalDatum::Literal(value)) => {
                postcondition_affine_constant(value)
            }
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty: CheckedType::Integer(_),
            }) if projections.is_empty() => self
                .input
                .context
                .constant(*declaration)
                .and_then(|constant| postcondition_affine_constant(&constant.value)),
            GoalExpression::Datum(GoalDatum::Place {
                root,
                projections,
                ty: CheckedType::Integer(_),
            }) if projections.is_empty() => state.values.get(root).cloned(),
            GoalExpression::Operation {
                row:
                    GoalOperation::NumericConversion {
                        mode: CheckedConversionMode::Exact,
                        source: CheckedNumericType::Integer(_),
                        destination: CheckedNumericType::Integer(_),
                    },
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => {
                let [value] = arguments.as_slice() else {
                    return None;
                };
                self.affine_goal_value(value, state)
            }
            GoalExpression::Operation {
                row: GoalOperation::Integer { operation, .. },
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => {
                let [left, right] = arguments.as_slice() else {
                    return None;
                };
                let left = self.affine_goal_value(left, state)?;
                let right = self.affine_goal_value(right, state)?;
                let mut check = AffineCheckState::new();
                match operation {
                    CheckedIntegerOperation::AddExact => left.add(&right, &mut check).ok(),
                    CheckedIntegerOperation::SubtractExact => {
                        left.subtract(&right, &mut check).ok()
                    }
                    CheckedIntegerOperation::MultiplyExact => {
                        if left.terms().is_empty() {
                            right.scale(left.constant_value(), &mut check).ok()
                        } else if right.terms().is_empty() {
                            left.scale(right.constant_value(), &mut check).ok()
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            // [MSR-4] a measure term's image in the affine domain is its own
            // compiler-owned atom, exactly as it is for a term the flow
            // already carries. Without this a goal over a measure reaches
            // only the L0 route, and every filling loop's row requirement —
            // `built.len < built.cap` under `built.len + at >= n` — is
            // unproved for want of the domain the rule names.
            GoalExpression::Operation {
                row:
                    GoalOperation::ArrayMeasure { .. }
                    | GoalOperation::BufferMeasure { .. }
                    | GoalOperation::ContainerMeasure { .. },
                ..
            } => {
                let term = self.goal_operand(expression)?;
                Some(self.vocabulary.measure_atom(term, state))
            }
            GoalExpression::Datum(
                GoalDatum::Parameter { .. } | GoalDatum::EvaluatedValue { .. },
            )
            | GoalExpression::Datum(GoalDatum::NamedConst { .. })
            | GoalExpression::Datum(GoalDatum::Place { .. })
            | GoalExpression::Operation { .. } => None,
        }
    }

    pub(super) fn instantiate_postcondition_relation(
        &mut self,
        postcondition: &CheckedPostcondition,
        results: &[Option<TermId>],
        returns: &[Option<PostconditionReturnDatum>],
    ) -> Option<Relation> {
        let operands = postcondition
            .relation
            .operands
            .iter()
            .map(|operand| {
                Some((
                    self.postcondition_relation_term(&operand.datum, results, returns)?,
                    operand.displacement,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let [first, second] = operands.as_slice() else {
            return None;
        };
        // [FN-9] each side's displacement folds into the one constant a
        // difference bound carries: `l + a <cmp> r + b` is
        // `l - r <cmp> b - a`.
        let gap = second.1.checked_sub(first.1)?;
        match postcondition.relation.normalized {
            NormalizedRelation::Equal => Some(Relation::Equal {
                left: first.0,
                right: second.0,
                difference: gap,
            }),
            NormalizedRelation::NotEqual => Some(if first.0 <= second.0 {
                Relation::Distinct {
                    left: first.0,
                    right: second.0,
                    difference: gap,
                }
            } else {
                Relation::Distinct {
                    left: second.0,
                    right: first.0,
                    difference: gap.checked_neg()?,
                }
            }),
            NormalizedRelation::UpperBound {
                left,
                right,
                strict,
            } => {
                let lower = *operands.get(left as usize)?;
                let upper = *operands.get(right as usize)?;
                Some(Relation::Bound {
                    left: lower.0,
                    right: upper.0,
                    bound: upper
                        .1
                        .checked_sub(lower.1)?
                        .checked_sub(i128::from(strict))?,
                })
            }
        }
    }

    pub(super) fn postcondition_relation_term(
        &mut self,
        datum: &RelationDatum,
        results: &[Option<TermId>],
        returns: &[Option<PostconditionReturnDatum>],
    ) -> Option<TermId> {
        match datum {
            // [CALL-4] the datum names one declared result ordinal, and the
            // destination supplies that ordinal's term.
            RelationDatum::Result { ordinal, .. } => *results.get(*ordinal as usize)?,
            RelationDatum::Parameter {
                ordinal,
                projections,
                ty,
            } => {
                let binding = self
                    .input
                    .function
                    .parameters
                    .get(*ordinal as usize)?
                    .binding;
                let projections = self
                    .input
                    .body_projections(PlaceRoot::Binding(binding), projections);
                self.vocabulary.postcondition_place_term(
                    PlaceRoot::Binding(binding),
                    projections,
                    *ty,
                )
            }
            RelationDatum::NamedConst {
                declaration,
                projections,
                ty,
            } => self.postcondition_named_const_term(*declaration, projections, *ty),
            RelationDatum::Literal { value, .. } => self.postcondition_constant_term(value),
            RelationDatum::Measure(measure, place) => match place.root {
                // [MSR-3] an `own` or shared-borrow parameter's measure in an
                // `ensures` denotes that parameter's entry datum, which the
                // entry placement minted and which nothing kills. The live
                // term is not read here: a body that writes the parameter
                // back still means the entry value.
                PostconditionPlaceRoot::Parameter { ordinal } => {
                    let kind = entry_datum_kind(ordinal, &place.projections, *measure);
                    if let Some(datum) = self.vocabulary.terms.interned(&kind) {
                        return Some(datum);
                    }
                    let binding = self
                        .input
                        .function
                        .parameters
                        .get(ordinal as usize)?
                        .binding;
                    let projections = self
                        .input
                        .body_projections(PlaceRoot::Binding(binding), &place.projections);
                    self.postcondition_measure_term(
                        *measure,
                        PlaceRoot::Binding(binding),
                        projections,
                        place.ty,
                        self.input.formal_is_range(ordinal),
                    )
                }
                PostconditionPlaceRoot::ExitParameter { ordinal } => {
                    let binding = self
                        .input
                        .function
                        .parameters
                        .get(ordinal as usize)?
                        .binding;
                    let projections = self
                        .input
                        .body_projections(PlaceRoot::Binding(binding), &place.projections);
                    self.postcondition_measure_term(
                        *measure,
                        PlaceRoot::Binding(binding),
                        projections,
                        place.ty,
                        self.input.formal_is_range(ordinal),
                    )
                }
                // [CALL-4] a measure over a result place is instantiated at
                // that ordinal's own destination: at an exit, the place the
                // selected return hands back.
                PostconditionPlaceRoot::Result { ordinal } => {
                    let datum = returns.get(ordinal as usize)?.as_ref()?;
                    let PostconditionReturnDatum::Place(returned) = datum else {
                        return None;
                    };
                    let root = self.input.postcondition_return_place_root(returned.root)?;
                    let mut projections = returned.projections.clone();
                    projections.extend_from_slice(&place.projections);
                    self.postcondition_measure_term(*measure, root, &projections, place.ty, false)
                }
            },
        }
    }

    pub(super) fn postcondition_return_term(
        &mut self,
        datum: &PostconditionReturnDatum,
    ) -> Option<TermId> {
        match datum {
            PostconditionReturnDatum::ResultPayload { ty } => fragment_type(*ty)
                .map(|ty| self.vocabulary.terms.intern(TermKind::ResultPayload(ty))),
            PostconditionReturnDatum::Place(place) => self.postcondition_return_place_term(place),
            PostconditionReturnDatum::Literal { value, .. } => {
                self.postcondition_constant_term(value)
            }
            PostconditionReturnDatum::Measure(measure, place) => {
                let root = self.input.postcondition_return_place_root(place.root)?;
                self.postcondition_measure_term(
                    *measure,
                    root,
                    &place.projections,
                    place.ty,
                    place.range_referent,
                )
            }
        }
    }

    pub(super) fn postcondition_return_place_term(
        &mut self,
        place: &PostconditionReturnPlace,
    ) -> Option<TermId> {
        if let PostconditionReturnPlaceRoot::NamedConst(declaration) = place.root {
            return self.postcondition_named_const_term(declaration, &place.projections, place.ty);
        }
        let root = self.input.postcondition_return_place_root(place.root)?;
        self.vocabulary
            .postcondition_place_term(root, &place.projections, place.ty)
    }

    pub(super) fn postcondition_named_const_term(
        &mut self,
        declaration: crate::DeclarationId,
        projections: &[GoalProjection],
        ty: CheckedType,
    ) -> Option<TermId> {
        if projections.is_empty()
            && let Some(term) = self
                .input
                .context
                .constant(declaration)
                .and_then(|constant| self.postcondition_constant_term(&constant.value))
        {
            return Some(term);
        }
        let root = PlaceRoot::Constant(*self.input.context.constant_ids.get(&declaration)?);
        self.vocabulary
            .postcondition_place_term(root, projections, ty)
    }

    /// [ENT-2, MSR-6] every occurrence of a symbolic const parameter names
    /// its original declaration and keeps that declaration's written type.
    /// A storage extent or a differently typed formal does not create another
    /// constant identity or grant that parameter a different interval.
    pub(super) fn const_parameter_term(&mut self, declaration: crate::DeclarationId) -> TermId {
        let ty = self.input.context.const_parameter_types[&declaration];
        self.vocabulary
            .terms
            .intern(TermKind::ConstParameter(declaration, ty))
    }

    pub(super) fn postcondition_constant_term(&mut self, value: &CheckedValue) -> Option<TermId> {
        if let CheckedValue::ConstGeneric { declaration, .. } = value {
            return Some(self.const_parameter_term(*declaration));
        }
        let value = match value {
            CheckedValue::Integer { ty, bits } => integer_value(*ty, *bits),
            CheckedValue::NumericIdentity {
                ty: CheckedType::Integer(_),
                one,
            } => i128::from(*one),
            _ => return None,
        };
        Some(self.vocabulary.terms.intern(TermKind::Constant(value)))
    }

    pub(super) fn postcondition_measure_term(
        &mut self,
        measure: CheckedMeasure,
        root: PlaceRoot,
        projections: &[GoalProjection],
        ty: CheckedType,
        range_referent: bool,
    ) -> Option<TermId> {
        let projections = projections
            .iter()
            .map(|projection| projection.place_step())
            .collect::<Vec<_>>();
        // [MSR-1] gives `&[T]` a row of its own, and [TYPE-8] makes the
        // range kind a mode and not a type: the checked type of a `&[T]`
        // parameter is its element type, so the measured row cannot be
        // recovered from it and comes from the parameter's mode instead.
        // Without this a clause naming `deref(part).len` of a range
        // parameter had no term at all, so [FN-9] selected no exit for it.
        let measured = if range_referent && projections.is_empty() {
            MeasuredKind::Range
        } else {
            measured_kind(ty)?
        };
        // [MSR-2] the written constant a cell the table fixes as the type's
        // own reads: an `Array`'s length or a constant window's capacity.
        let array_length = type_constant(ty);
        Some(self.measure_term(
            measure,
            ResolvedPlace {
                root,
                path: projections,
            },
            measured,
            array_length,
        ))
    }

    pub(super) fn postcondition_term_live_holders(&self, term: TermId) -> Vec<BindingId> {
        let mut holders = Vec::new();
        match self.vocabulary.terms.kind(term) {
            TermKind::Place(place, _) | TermKind::Measure(_, place) => {
                let PlaceRoot::Binding(root) = place.root else {
                    return holders;
                };
                let support = GoalSupport {
                    root,
                    projections: place
                        .path
                        .iter()
                        .map_while(goal_projection_of_step)
                        .collect(),
                    measure: match self.vocabulary.terms.kind(term) {
                        TermKind::Measure(measure, _) => Some(*measure),
                        _ => None,
                    },
                };
                let (_, projected_holders) = self.input.resolve_goal_support(&support);
                holders.extend(projected_holders);
            }
            TermKind::Zero | TermKind::Constant(_) | TermKind::ConstParameter(..) => {}
            TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. }
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. } => {}
        }
        holders
    }

    pub(super) fn s12_transfer_event_kills_substitution(
        &self,
        separations: &dyn SeparationOracle,
        substitution: &PostconditionCallSubstitution,
        event: &KillEvent,
    ) -> bool {
        // [MSR-3] a call datum contains no place and denotes the operand's
        // value at the pre-transfer point, so no event at or after the call
        // can invalidate a relation stated over it.
        if substitution.datum {
            return false;
        }
        let holder_consumed = match event {
            KillEvent::Consume { binding, .. }
            | KillEvent::EntryImageHolderConsume { binding, .. } => {
                substitution.transfer_holders.contains(binding)
            }
            _ => false,
        };
        if holder_consumed {
            return true;
        }
        match event {
            KillEvent::EntryImageHolderWrite {
                place,
                element,
                source,
            } => self.event_kills_term(
                separations,
                substitution.term,
                &KillEvent::Write {
                    place: place.clone(),
                    element: *element,
                    source: source.clone(),
                },
            ),
            _ => self.event_kills_term(separations, substitution.term, event),
        }
    }

    pub(super) fn s12_candidate_term_killed(
        &self,
        separations: &dyn SeparationOracle,
        term: TermId,
        event: &KillEvent,
    ) -> bool {
        let live_holders = self.postcondition_term_live_holders(term);
        let live_holder_killed = match event {
            KillEvent::Consume { binding, .. }
            | KillEvent::EntryImageHolderConsume { binding, .. } => live_holders.contains(binding),
            KillEvent::Write {
                place,
                element: false,
                ..
            }
            | KillEvent::EntryImageHolderWrite {
                place,
                element: false,
                ..
            } => live_holders
                .iter()
                .any(|holder| self.input.write_replaces_live_holder(*holder, place)),
            KillEvent::Write { element: true, .. }
            | KillEvent::EntryImageHolderWrite { element: true, .. } => false,
        };
        if live_holder_killed {
            return true;
        }
        match event {
            KillEvent::EntryImageHolderConsume { binding, source } => self.event_kills_term(
                separations,
                term,
                &KillEvent::Consume {
                    binding: *binding,
                    source: source.clone(),
                },
            ),
            KillEvent::EntryImageHolderWrite {
                place,
                element,
                source,
            } => self.event_kills_term(
                separations,
                term,
                &KillEvent::Write {
                    place: place.clone(),
                    element: *element,
                    source: source.clone(),
                },
            ),
            _ => self.event_kills_term(separations, term, event),
        }
    }

    pub(super) fn s12_candidate_scope_kills_term(
        &self,
        term: TermId,
        exited: &HashSet<BindingId>,
    ) -> bool {
        self.vocabulary.scope_kills_term(term, exited)
            || self
                .postcondition_term_live_holders(term)
                .iter()
                .any(|holder| exited.contains(holder))
    }

    pub(super) fn s12_substitutions_survive(
        &self,
        separations: &dyn SeparationOracle,
        substitutions: &[PostconditionCallSubstitution],
        events: &[KillEvent],
        call_transfer: bool,
    ) -> bool {
        substitutions.iter().all(|substitution| {
            if call_transfer && substitution.exit_state {
                return true;
            }
            events.iter().all(|event| {
                !self.s12_transfer_event_kills_substitution(separations, substitution, event)
            })
        })
    }

    pub(super) fn kill_s12_candidates_for_event(
        &self,
        separations: &dyn SeparationOracle,
        state: &mut FactState,
        event: &KillEvent,
    ) {
        if !state.may_hold_postcondition_candidates() {
            return;
        }
        state.kill_proof_candidates(&self.vocabulary.derivations, |left, right, proof| {
            self.vocabulary
                .derivations
                .depends_on_postcondition_call(proof)
                && (self.s12_candidate_term_killed(separations, left, event)
                    || self.s12_candidate_term_killed(separations, right, event))
        });
    }

    pub(super) fn kill_s12_candidates_for_scope(
        &self,
        state: &mut FactState,
        exited: &HashSet<BindingId>,
    ) {
        if !state.may_hold_postcondition_candidates() {
            return;
        }
        state.kill_proof_candidates(&self.vocabulary.derivations, |left, right, proof| {
            self.vocabulary
                .derivations
                .depends_on_postcondition_call(proof)
                && (self.s12_candidate_scope_kills_term(left, exited)
                    || self.s12_candidate_scope_kills_term(right, exited))
        });
    }

    pub(super) fn call_parameter_term(
        &mut self,
        actual: &GoalExpression,
        projections: &[GoalProjection],
        ty: CheckedType,
        measure: Option<CheckedMeasure>,
        mode: CheckedMode,
    ) -> Option<TermId> {
        let projections = if mode == CheckedMode::Own {
            projections
        } else {
            let (GoalProjection::Deref, remaining) = projections.split_first()? else {
                return None;
            };
            remaining
        };
        if projections.is_empty() && measure.is_none() {
            return (actual.ty() == ty)
                .then(|| self.goal_operand(actual))
                .flatten();
        }
        let (root, projections) = self.input.call_parameter_place(actual, projections)?;
        if let Some(measure) = measure {
            // [TYPE-8, REF-4] the callee's own parameter kind supplies the
            // [MSR-1] row: the caller's actual may be any place that names a
            // range, and the element type it carries has no row at all.
            self.postcondition_measure_term(
                measure,
                root,
                &projections,
                ty,
                mode == CheckedMode::Range,
            )
        } else {
            self.vocabulary
                .postcondition_place_term(root, &projections, ty)
        }
    }

    /// [ENT-3.S13, MSR-3] mints, at one call's pre-transfer point, the call
    /// datum of every `own` operand any declared relation of the resolved
    /// callee names, and establishes it equal to that operand's pre-transfer
    /// term.
    ///
    /// The equality is stated here, before the call's own consumes and
    /// kills, so [ENT-5]'s pre-kill closure carries the datum's consequences
    /// across them. The datum itself contains no place, so nothing kills it:
    /// that is exactly why a relation naming a consumed `own` operand's
    /// measure means what it reads as at the caller, and why the consume the
    /// same statement performs cannot delete it.
    pub(super) fn establish_call_datums(
        &mut self,
        function: super::super::super::model::FunctionId,
        call: &crate::NodePath,
        goal_arguments: &[GoalExpression],
        postconditions: &[AvailablePostcondition],
        state: &mut ProofFlowState,
    ) {
        let Some(callee) = self.input.context.callee(function) else {
            return;
        };
        let parameter_modes = callee.parameter_modes.clone();
        let mut operands: Vec<(
            u32,
            Vec<GoalProjection>,
            Option<CheckedMeasure>,
            CheckedType,
        )> = Vec::new();
        for available in postconditions {
            for operand in &available.relation.operands {
                let datum = &operand.datum;
                match datum {
                    RelationDatum::Parameter {
                        ordinal,
                        projections,
                        ty,
                    } => operands.push((*ordinal, projections.clone(), None, *ty)),
                    // A result-rooted measure names no operand and mints no
                    // call datum [CALL-4].
                    RelationDatum::Measure(measure, place) => {
                        if let PostconditionPlaceRoot::Parameter { ordinal } = place.root {
                            operands.push((
                                ordinal,
                                place.projections.clone(),
                                Some(*measure),
                                place.ty,
                            ));
                        }
                    }
                    RelationDatum::Result { .. }
                    | RelationDatum::NamedConst { .. }
                    | RelationDatum::Literal { .. } => {}
                }
            }
        }
        let event = self.vocabulary.proof_event(FlowEventKind::S13, Some(call));
        for (ordinal, projections, measure, ty) in operands {
            let Some(mode) = parameter_modes.get(ordinal as usize).copied() else {
                continue;
            };
            if mode != CheckedMode::Own
                && !(measure.is_some() && matches!(mode, CheckedMode::Reference))
            {
                continue;
            }
            let Some(datum_type) = (if measure.is_some() {
                Some(super::super::super::model::IntegerType::U64)
            } else {
                fragment_type(ty)
            }) else {
                continue;
            };
            let kind = call_datum_kind(call, ordinal, &projections, measure, datum_type);
            if self.vocabulary.terms.interned(&kind).is_some() {
                continue;
            }
            let Some(actual) = goal_arguments.get(ordinal as usize) else {
                continue;
            };
            let Some(term) = self.call_parameter_term(actual, &projections, ty, measure, mode)
            else {
                continue;
            };
            // A datum denotes the operand's value at this point. When the
            // pre-transfer term is already immutable and has empty support,
            // it denotes exactly that and nothing can retarget it, so the
            // datum is that term: minting a second one would add an
            // indirection to every derivation and hide the writer's own
            // constant behind it in a diagnostic.
            if self.vocabulary.immortal_term(term) {
                continue;
            }
            let datum = self.vocabulary.terms.intern(kind);
            self.vocabulary
                .adopt_measure_atom(datum, term, &state.affine);
            state.facts.establish(
                &Relation::Equal {
                    left: datum,
                    right: term,
                    difference: 0,
                },
                &mut self.vocabulary.derivations,
                event,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_call_postcondition_relation(
        &mut self,
        function: super::super::super::model::FunctionId,
        call_path: &crate::NodePath,
        template: &RelationTemplate,
        checked_arguments: &[CheckedExpression],
        arguments: &[GoalExpression],
        results: &[Option<TermId>],
        result_places: &[Option<(PlaceRoot, Vec<GoalProjection>, CheckedType)>],
    ) -> Option<InstantiatedPostcondition> {
        let parameter_modes = self.input.context.callee(function)?.parameter_modes.clone();
        let mut substitutions = Vec::new();
        let mut operands = Vec::with_capacity(template.operands.len());
        for (operand, term_operand) in template.operands.iter().enumerate() {
            let datum = &term_operand.datum;
            let (term, formal) = match datum {
                // [CALL-4] the destination supplies one term per declared
                // result ordinal; an ordinal with none makes only this
                // relation unavailable.
                RelationDatum::Result { ordinal, .. } => {
                    ((*results.get(*ordinal as usize)?)?, None)
                }
                RelationDatum::Parameter {
                    ordinal,
                    projections,
                    ty,
                } => match self.vocabulary.interned_call_datum(
                    call_path,
                    *ordinal,
                    projections,
                    None,
                    *ty,
                ) {
                    // [MSR-3] an `own` operand denotes this call's call
                    // datum, which has empty support.
                    Some(datum) => (datum, Some((*ordinal, true))),
                    None => (
                        self.call_parameter_term(
                            arguments.get(*ordinal as usize)?,
                            projections,
                            *ty,
                            None,
                            *parameter_modes.get(*ordinal as usize)?,
                        )?,
                        Some((*ordinal, false)),
                    ),
                },
                RelationDatum::NamedConst {
                    declaration,
                    projections,
                    ty,
                } => (
                    self.postcondition_named_const_term(*declaration, projections, *ty)?,
                    None,
                ),
                RelationDatum::Literal { value, .. } => {
                    (self.postcondition_constant_term(value)?, None)
                }
                RelationDatum::Measure(measure, place) => match place.root {
                    PostconditionPlaceRoot::Parameter { ordinal } => {
                        match self.vocabulary.interned_call_datum(
                            call_path,
                            ordinal,
                            &place.projections,
                            Some(*measure),
                            place.ty,
                        ) {
                            Some(datum) => (datum, Some((ordinal, true))),
                            None => (
                                self.call_parameter_term(
                                    arguments.get(ordinal as usize)?,
                                    &place.projections,
                                    place.ty,
                                    Some(*measure),
                                    *parameter_modes.get(ordinal as usize)?,
                                )?,
                                Some((ordinal, false)),
                            ),
                        }
                    }
                    PostconditionPlaceRoot::ExitParameter { ordinal } => (
                        self.call_parameter_term(
                            arguments.get(ordinal as usize)?,
                            &place.projections,
                            place.ty,
                            Some(*measure),
                            *parameter_modes.get(ordinal as usize)?,
                        )?,
                        Some((ordinal, false)),
                    ),
                    // [CALL-4] the destination supplies one place per
                    // declared result ordinal, and this operand is that
                    // place's measure rather than its value.
                    PostconditionPlaceRoot::Result { ordinal } => {
                        let (root, destination, _) =
                            result_places.get(ordinal as usize)?.as_ref()?;
                        // [CALL-4, MSR-1] the clause's own projections below
                        // the result ordinal continue the destination's path:
                        // `result.inner.len` names the measure of the cell
                        // content the binder holds [TYPE-9] and not a measure
                        // of the cell, which has no row at all.
                        let mut projections = destination.clone();
                        projections.extend(place.projections.iter().cloned());
                        (
                            self.postcondition_measure_term(
                                *measure,
                                *root,
                                &projections,
                                place.ty,
                                false,
                            )?,
                            None,
                        )
                    }
                },
            };
            if let Some((formal, datum)) = formal {
                substitutions.push(PostconditionCallSubstitution {
                    operand: u32::try_from(operand)
                        .expect("postcondition operands exceed the u32 identity space"),
                    formal,
                    term,
                    transfer_holders: self.input.call_argument_holder_chain(
                        checked_arguments.get(formal as usize)?,
                        arguments.get(formal as usize)?,
                    ),
                    datum,
                    exit_state: matches!(
                        &term_operand.datum,
                        RelationDatum::Measure(
                            _,
                            PostconditionPlace {
                                root: PostconditionPlaceRoot::ExitParameter { .. },
                                ..
                            }
                        )
                    ),
                });
            }
            operands.push((term, term_operand.displacement));
        }
        let [first, second] = operands.as_slice() else {
            return None;
        };
        // [FN-9] each side's displacement folds into the one constant a
        // difference bound carries.
        let gap = second.1.checked_sub(first.1)?;
        let relation = match template.normalized {
            NormalizedRelation::Equal => Relation::Equal {
                left: first.0,
                right: second.0,
                difference: gap,
            },
            NormalizedRelation::NotEqual => {
                if first.0 <= second.0 {
                    Relation::Distinct {
                        left: first.0,
                        right: second.0,
                        difference: gap,
                    }
                } else {
                    Relation::Distinct {
                        left: second.0,
                        right: first.0,
                        difference: gap.checked_neg()?,
                    }
                }
            }
            NormalizedRelation::UpperBound {
                left,
                right,
                strict,
            } => {
                let lower = *operands.get(left as usize)?;
                let upper = *operands.get(right as usize)?;
                Relation::Bound {
                    left: lower.0,
                    right: upper.0,
                    bound: upper
                        .1
                        .checked_sub(lower.1)?
                        .checked_sub(i128::from(strict))?,
                }
            }
        };
        Some(InstantiatedPostcondition {
            relation,
            substitutions,
        })
    }

    pub(super) fn establish_direct_result(
        &mut self,
        statement: &crate::NodePath,
        binding: BindingId,
        value: &CheckedExpression,
        prepared: &PreparedCall,
        states: &mut ProofFlowState,
    ) {
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            goal_arguments,
            result,
            ..
        } = value
        else {
            return;
        };
        if *function != prepared.callee || *call != prepared.call {
            return;
        }
        // [CALL-4] the destination is one term when the ordinal's value is an
        // [ENT-2] term, and is always the place a measure over that ordinal is
        // taken over. A measured result has the second and not the first.
        let result_term = fragment_type(*result).and_then(|_| {
            self.vocabulary
                .postcondition_place_term(PlaceRoot::Binding(binding), &[], *result)
        });
        let result_place = Some((PlaceRoot::Binding(binding), Vec::new(), *result));
        for available in prepared.postconditions.iter().cloned() {
            if available.variant.is_some()
                || !available
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
                &[result_term],
                std::slice::from_ref(&result_place),
            ) else {
                continue;
            };
            if !self.s12_substitutions_survive(
                &prepared.entry_separations(&states.separations),
                &instantiated.substitutions,
                &prepared.kills,
                true,
            ) {
                continue;
            }
            self.vocabulary.retain_direct_result(
                statement,
                binding,
                &instantiated,
                &available,
                prepared,
                &mut states.facts,
            );
        }
    }

    /// [ENT-3.S12, CALL-4] establishes, at each destination of a binder or
    /// target list, every published relation naming that destination's result
    /// ordinal.
    ///
    /// The destinations are given in written order, so destination i is
    /// result ordinal i; `extra_kills` are the events the same statement's
    /// commits contribute, which a substitution must survive exactly as it
    /// must survive the call's own.
    pub(super) fn establish_result_list_destinations(
        &mut self,
        statement: &crate::NodePath,
        destinations: &[Option<(BindingId, Vec<GoalProjection>, CheckedType)>],
        value: &CheckedExpression,
        prepared: &PreparedCall,
        extra_kills: &[KillEvent],
        states: &mut ProofFlowState,
    ) {
        let CheckedExpression::UserCall {
            function,
            call,
            arguments,
            goal_arguments,
            ..
        } = value
        else {
            return;
        };
        if *function != prepared.callee || *call != prepared.call {
            return;
        }
        // One term per result ordinal, in written order. A subscript place is
        // no [ENT-2] term and a non-fragment ordinal carries no relation
        // datum, so either leaves its ordinal without a term and makes only
        // the relations naming it unavailable.
        let mut result_terms = Vec::with_capacity(destinations.len());
        // [CALL-4] the same destination is also the place a measure over that
        // result ordinal is taken over, which a measured ordinal has and a
        // fragment-integer value term does not.
        let mut result_places = Vec::with_capacity(destinations.len());
        let mut anchor = None;
        for destination in destinations {
            let term = destination.as_ref().and_then(|(binding, fields, ty)| {
                fragment_type(*ty)?;
                let term = self.vocabulary.postcondition_place_term(
                    PlaceRoot::Binding(*binding),
                    fields,
                    *ty,
                );
                if term.is_some() && anchor.is_none() {
                    anchor = Some(*binding);
                }
                term
            });
            result_places.push(destination.as_ref().map(|(binding, fields, ty)| {
                if anchor.is_none() {
                    anchor = Some(*binding);
                }
                (PlaceRoot::Binding(*binding), fields.clone(), *ty)
            }));
            result_terms.push(term);
        }
        let Some(anchor) = anchor else {
            return;
        };
        for available in prepared.postconditions.iter().cloned() {
            // A variant-routed relation is restricted to its arm [CALL-6];
            // a binder or target list enters no arm.
            if available.variant.is_some()
                || !available
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
                &result_terms,
                &result_places,
            ) else {
                continue;
            };
            if !self.s12_substitutions_survive(
                &prepared.entry_separations(&states.separations),
                &instantiated.substitutions,
                &prepared.kills,
                true,
            ) || !self.s12_substitutions_survive(
                &states.separations,
                &instantiated.substitutions,
                extra_kills,
                false,
            ) {
                continue;
            }
            self.vocabulary.retain_direct_result(
                statement,
                anchor,
                &instantiated,
                &available,
                prepared,
                &mut states.facts,
            );
        }
    }

    pub(super) fn prepare_direct_receiver(
        &mut self,
        separations: &SeparationLedger,
        route: DirectReceiverRoute,
        value: &CheckedExpression,
        prepared: &PreparedCall,
        target_events: &[KillEvent],
    ) -> Vec<DirectReceiverCandidate> {
        let CheckedExpression::UserCall {
            function,
            arguments,
            goal_arguments,
            ..
        } = value
        else {
            return Vec::new();
        };
        let Some(result_term) = self.vocabulary.postcondition_place_term(
            PlaceRoot::Binding(route.binding),
            &[],
            route.ty,
        ) else {
            return Vec::new();
        };
        prepared
            .postconditions
            .iter()
            .cloned()
            .filter_map(|available| {
                if available.variant.is_some()
                    || !available
                        .relation
                        .operands
                        .iter()
                        .any(|term| term.contains_result())
                {
                    return None;
                }
                let instantiated = self.instantiate_call_postcondition_relation(
                    *function,
                    &prepared.call,
                    &available.relation,
                    arguments,
                    goal_arguments,
                    &[Some(result_term)],
                    &[],
                )?;
                if instantiated
                    .substitutions
                    .iter()
                    .any(|substitution| substitution.formal == route.formal)
                    || !self.s12_substitutions_survive(
                        &prepared.entry_separations(separations),
                        &instantiated.substitutions,
                        &prepared.kills,
                        true,
                    )
                    || !self.s12_substitutions_survive(
                        separations,
                        &instantiated.substitutions,
                        target_events,
                        false,
                    )
                {
                    return None;
                }
                Some(DirectReceiverCandidate {
                    route,
                    available,
                    instantiated,
                })
            })
            .collect()
    }

    /// [MSR-3] the entry placement: at body entry, per parameter of measured
    /// type and per measure any declared relation names, one compiler-owned
    /// immutable datum established equal to that measure.
    ///
    /// The datum contains no place, so no [ENT-5] event kills it. That is
    /// what makes an `ensures` naming an `own` parameter's measure denote the
    /// entry value even where the body writes that parameter back with a
    /// [LIV-2] `set`, and it is the callee-side half of the denotation
    /// [MSR-3]'s table gives the same operand at a caller.
    pub(super) fn establish_entry_datums(&mut self, state: &mut ProofFlowState) {
        if self.input.entry_images.is_empty() {
            return;
        }
        let event = self.vocabulary.proof_event(FlowEventKind::Entry, None);
        for index in 0..self.input.entry_images.len() {
            let image = self.input.entry_images[index].datum.clone();
            let ty = self.input.entry_images[index].ty;
            let Some(measure) = image.measure else {
                continue;
            };
            let Some(parameter) = self.input.function.parameters.get(image.parameter as usize)
            else {
                continue;
            };
            let binding = parameter.binding;
            let projections = self
                .input
                .body_projections(PlaceRoot::Binding(binding), &image.projections);
            let Some(live) = self.postcondition_measure_term(
                measure,
                PlaceRoot::Binding(binding),
                projections,
                ty,
                self.input.formal_is_range(image.parameter),
            ) else {
                continue;
            };
            let datum = self.vocabulary.terms.intern(entry_datum_kind(
                image.parameter,
                &image.projections,
                measure,
            ));
            self.vocabulary
                .adopt_measure_atom(datum, live, &state.affine);
            state.facts.establish(
                &Relation::Equal {
                    left: datum,
                    right: live,
                    difference: 0,
                },
                &mut self.vocabulary.derivations,
                event,
            );
        }
    }
}

impl Judging<'_, '_, '_> {
    pub(super) fn initialize_postcondition_proofs(&mut self) {
        let aggregate = || PostconditionAggregate {
            discharged: false,
            derivation: None,
        };
        self.output.postconditions = self
            .input
            .function
            .postconditions
            .iter()
            .enumerate()
            .map(|(ordinal, postcondition)| FunctionPostconditionProof {
                block: postcondition.selector.block.clone(),
                selector: postcondition.selector.selector.clone(),
                relation_ordinal: u32::try_from(ordinal)
                    .expect("postcondition relation ordinal exceeds u32"),
                summary: None,
                exits: Vec::new(),
                aggregate: aggregate(),
            })
            .collect();
    }

    pub(super) fn finalize_postcondition_aggregates(&mut self) {
        for index in 0..self.output.postconditions.len() {
            let block = self.output.postconditions[index].block.clone();
            let relation_ordinal = self.output.postconditions[index].relation_ordinal;
            let parents = self.output.postconditions[index]
                .exits
                .iter()
                .map(|exit| {
                    (exit.disposition == PostconditionDisposition::Discharged)
                        .then_some(exit.derivation)
                        .flatten()
                })
                .collect::<Option<Vec<_>>>();
            self.output.postconditions[index].aggregate = self
                .reasoning()
                .retain_postcondition_aggregate(&block, relation_ordinal, parents);
        }
    }

    pub(super) fn judge_postcondition_return(
        &mut self,
        statement: &crate::NodePath,
        states: &ProofFlowState,
        affine_result: Option<&AffineForm>,
        value_reached: bool,
        forwarded: &[Option<ResultEvidence>],
    ) {
        if self.output.postconditions.is_empty() {
            return;
        }
        for index in 0..self.input.function.postconditions.len() {
            let postcondition = &self.input.function.postconditions[index];
            let Some(selected) = postcondition
                .selected_returns
                .iter()
                .find(|selected| selected.statement == *statement)
                .cloned()
            else {
                continue;
            };
            let forwarded = forwarded
                .get(postcondition.selector.ordinal as usize)
                .and_then(Option::as_ref);
            let conditional = selected
                .values
                .iter()
                .any(|value| matches!(value, Some(PostconditionReturnDatum::ResultPayload { .. })));
            if conditional && forwarded.is_some_and(|result| result.definitely_err) {
                // An outcome with no possible success supplies no selected
                // success return, including after copies and value joins.
                continue;
            }
            let mut conditional_facts = states.facts.clone();
            if conditional && let Some(result) = forwarded {
                if result.facts.all_derivable {
                    conditional_facts.promote_to_contradiction(result.facts.contradiction);
                }
                for (relation, parent) in result.facts.l0_candidates() {
                    conditional_facts.establish_from_proof(
                        &relation,
                        parent,
                        &self.vocabulary.derivations,
                    );
                }
            }
            // [CALL-4] one term per declared result ordinal, in written order.
            let results = selected
                .values
                .iter()
                .map(|value| {
                    value
                        .as_ref()
                        .and_then(|value| self.reasoning().postcondition_return_term(value))
                })
                .collect::<Vec<_>>();
            // [CALL-4] an ordinal whose destination is no [ENT-2] place
            // makes only the relations naming it unavailable, which is what a
            // measured result's value term is: the clause names its measure
            // and never the value.
            let Some(relation) = self.reasoning().instantiate_postcondition_relation(
                postcondition,
                &results,
                &selected.values,
            ) else {
                continue;
            };
            // [MSR-4] the affine route over the relation's own instantiated
            // terms, which is what carries a measure operand: a measure has
            // an affine atom [MSR-4] and no result value image, so the datum
            // route below reaches it nowhere. The datum route stays for a
            // fragment result whose returned expression has a richer image
            // than its place.
            let affine_target = self
                .vocabulary
                .affine_relation_target(&relation, &states.affine)
                .or_else(|| {
                    affine_result.and_then(|result| {
                        self.input.postcondition_affine_target(
                            postcondition,
                            result,
                            &states.affine,
                        )
                    })
                });
            let residual = self.reasoning().render_relation(&relation);

            let entry_images = self.input.postcondition_entry_images[index]
                .iter()
                .map(|entry_index| PostconditionEntryImageOutcome {
                    datum: self.input.entry_images[*entry_index].datum.clone(),
                    invalidation: states.entry_images[*entry_index],
                })
                .collect::<Vec<_>>();
            let occurrence = self.output.postconditions[index].exits.len();
            let relation_ordinal = self.output.postconditions[index].relation_ordinal;
            let unavailable = !value_reached
                || entry_images
                    .iter()
                    .any(|image| image.invalidation.is_some());
            let complete = self.reasoning().judge_postcondition(
                relation_ordinal,
                occurrence,
                statement,
                &relation,
                &conditional_facts,
                affine_target.as_deref(),
                &states.affine,
                unavailable,
            );
            self.output.postconditions[index]
                .exits
                .push(PostconditionExit {
                    statement: statement.clone(),
                    relation,
                    residual,
                    entry_images,
                    disposition: complete.disposition,
                    derivation: complete.derivation,
                });
        }
    }
}

/// Equality is the conjunction of its two ordinary affine bounds. Both
/// are checked by the same deterministic numeric derivation as <=.
pub(super) fn affine_ordering_targets(
    left: &AffineForm,
    right: &AffineForm,
    equal: bool,
) -> Option<Vec<AffineInequality>> {
    let mut check = AffineCheckState::new();
    let mut targets = vec![AffineInequality::from_forms(left, right, &mut check).ok()?];
    if equal {
        targets.push(AffineInequality::from_forms(right, left, &mut check).ok()?);
    }
    Some(targets)
}

pub(super) fn postcondition_affine_constant(value: &CheckedValue) -> Option<AffineForm> {
    let value = match value {
        CheckedValue::Integer { ty, bits } => integer_value(*ty, *bits),
        CheckedValue::NumericIdentity {
            ty: CheckedType::Integer(_),
            one,
        } => i128::from(*one),
        _ => return None,
    };
    Some(AffineForm::constant(value))
}

/// [MSR-3] the identity of one call datum: the call, the formal ordinal,
/// the operand's ordered projections, and which [MSR-1] measure of the
/// operand the datum denotes, if any.
pub(super) fn call_datum_kind(
    call: &crate::NodePath,
    formal: u32,
    projections: &[GoalProjection],
    measure: Option<CheckedMeasure>,
    ty: super::super::super::model::IntegerType,
) -> TermKind {
    TermKind::CallDatum {
        call_path: call.components().to_vec(),
        formal,
        projections: projections
            .iter()
            .map(|projection| projection.place_step())
            .collect(),
        measure,
        ty,
    }
}

pub(super) fn selected_call_summary(
    available: &AvailablePostcondition,
) -> Option<VerifiedPostconditionSummaryRef> {
    Some(VerifiedPostconditionSummaryRef {
        summary: available.authority.clone(),
    })
}

pub(super) fn replace_relation_term(relation: &Relation, from: TermId, to: TermId) -> Relation {
    let replace = |term| if term == from { to } else { term };
    match relation {
        Relation::Bound { left, right, bound } => Relation::Bound {
            left: replace(*left),
            right: replace(*right),
            bound: *bound,
        },
        Relation::Equal {
            left,
            right,
            difference,
        } => Relation::Equal {
            left: replace(*left),
            right: replace(*right),
            difference: *difference,
        },
        Relation::Distinct {
            left,
            right,
            difference,
        } => {
            let (left, right) = (replace(*left), replace(*right));
            // Ordering the pair reverses the difference with it.
            if left <= right {
                Relation::Distinct {
                    left,
                    right,
                    difference: *difference,
                }
            } else {
                Relation::Distinct {
                    left: right,
                    right: left,
                    difference: -difference,
                }
            }
        }
    }
}

/// [MSR-3] the identity of one entry datum: the formal ordinal, the
/// operand's ordered projections, and which [MSR-1] measure of it the
/// datum denotes.
pub(super) fn entry_datum_kind(
    formal: u32,
    projections: &[GoalProjection],
    measure: CheckedMeasure,
) -> TermKind {
    TermKind::EntryDatum {
        formal,
        projections: projections
            .iter()
            .map(|projection| projection.place_step())
            .collect(),
        measure,
    }
}
