//! Frozen relation instances shared by optional transport and mandatory
//! induction [ENT-2(j), ENT-5, INV-1]. Formation never publishes a theorem.

use super::super::state::AffineRelationInstance;
use super::super::{LoopFormationFailure, LoopRelationEvidence};
use super::*;

pub(super) struct NextHeader<'a> {
    pub(super) loop_id: CheckedLoopId,
    pub(super) edge: &'a crate::NodePath,
    pub(super) binder: BindingId,
    pub(super) value: AffineForm,
}

pub(super) struct RelationBatch {
    pub(super) disposition: TargetDisposition,
    pub(super) evidence: LoopRelationEvidence,
}

impl RelationBatch {
    fn unformed(failure: Option<LoopFormationFailure>) -> Self {
        Self {
            disposition: TargetDisposition::Unproved,
            evidence: LoopRelationEvidence {
                instance: None,
                components: Vec::new(),
                contradiction: None,
                failing_component: None,
                formation_failure: failure,
            },
        }
    }
}

impl Reasoning<'_, '_, '_> {
    /// Terms and current images are shared at a program point, but a query's
    /// target namespace and proofs remain private to that query's view.
    pub(super) fn relation_instance(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &ProofFlowState,
        next: Option<&NextHeader<'_>>,
    ) -> Result<(AffineRelationInstance, ProofFlowState), Option<LoopFormationFailure>> {
        let mut view = state.clone();
        let mut targets = HashMap::new();
        let mut formation = Vec::new();
        for side in [&relation.left, &relation.right] {
            for leaf in side.postorder() {
                match &leaf.kind {
                    CheckedAffineExpressionKind::Local { binding, .. } => {
                        if !state.affine.values.contains_key(binding) {
                            return Err(None);
                        }
                    }
                    CheckedAffineExpressionKind::Measure(expression) => {
                        self.form_relation_measure(
                            expression,
                            &mut view,
                            next,
                            &mut targets,
                            &mut formation,
                        )?;
                        let term = self.checked_measure_term(expression).ok_or(None)?;
                        // An ordinary current measure has one image shared by
                        // all frozen candidate views. This adds no premise.
                        let image = self.vocabulary.measure_atom(term, &state.affine);
                        view.affine.measure_atoms.borrow_mut().insert(term, image);
                        self.target_measure(term, &mut view, next, &mut targets);
                    }
                    _ => {}
                }
            }
        }
        let mut substitution = view.affine.clone();
        if let Some(next) = next {
            substitution.values.insert(next.binder, next.value.clone());
        }
        for (source, target) in &targets {
            let image = self.vocabulary.measure_atom(*target, &view.affine);
            substitution
                .measure_atoms
                .borrow_mut()
                .insert(*source, image);
        }
        let mut operands = Vec::new();
        for side in [&relation.left, &relation.right] {
            for leaf in side.postorder() {
                if matches!(
                    leaf.kind,
                    CheckedAffineExpressionKind::Local { .. }
                        | CheckedAffineExpressionKind::Measure(_)
                        | CheckedAffineExpressionKind::Constant { .. }
                        | CheckedAffineExpressionKind::ConstGeneric { .. }
                ) {
                    operands.push(
                        self.checked_affine_form(
                            leaf,
                            &mut substitution,
                            &mut AffineCheckState::new(),
                        )
                        .map_err(|_| None)?,
                    );
                }
            }
        }
        let forward = self
            .checked_affine_relation_inequality(
                relation,
                &mut substitution,
                &mut AffineCheckState::new(),
            )
            .map_err(|_| None)?;
        let right = self
            .checked_affine_right_term(&relation.right)
            .map(|term| targets.get(&term).copied().unwrap_or(term));
        let left = self
            .checked_affine_right_term(&relation.left)
            .map(|term| targets.get(&term).copied().unwrap_or(term));
        let mut components = vec![forward];
        let mut sides = vec![(right, left)];
        if let Some(partner) = self.checked_affine_relation_partner(
            relation,
            &mut substitution,
            &mut AffineCheckState::new(),
        ) {
            components.push(partner.map_err(|_| None)?);
            sides.push((left, right));
        }
        Ok((
            AffineRelationInstance {
                operands,
                components,
                sides,
                formation,
            },
            view,
        ))
    }

    pub(super) fn prove_relation_instance(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &ProofFlowState,
        next: Option<&NextHeader<'_>>,
    ) -> RelationBatch {
        let closed = close(
            &state.facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        if let Some(contradiction) = closed.contradiction_proof() {
            return RelationBatch {
                disposition: TargetDisposition::Proved,
                evidence: LoopRelationEvidence {
                    instance: None,
                    components: Vec::new(),
                    contradiction: Some(contradiction),
                    failing_component: None,
                    formation_failure: None,
                },
            };
        }
        let (instance, view) = match self.relation_instance(relation, state, next) {
            Ok(formed) => formed,
            Err(failure) => return RelationBatch::unformed(failure),
        };
        let mut evidence = LoopRelationEvidence {
            instance: Some(instance.clone()),
            components: Vec::new(),
            contradiction: None,
            failing_component: None,
            formation_failure: None,
        };
        for (component, (inequality, (right, opposite))) in
            instance.components.iter().zip(&instance.sides).enumerate()
        {
            let proof = self.prove(
                ProofContext::new(&view.facts, &view.affine),
                ProofGoal::Affine {
                    inequality,
                    right: *right,
                },
            );
            if proof.disposition == ProofDisposition::Proved {
                evidence
                    .components
                    .push(proof.derivation.expect("proved component has evidence"));
            } else {
                evidence.failing_component = Some(component as u8);
                let refuted = inequality
                    .negated(&mut AffineCheckState::new())
                    .ok()
                    .is_some_and(|negation| {
                        self.prove(
                            ProofContext::new(&view.facts, &view.affine),
                            ProofGoal::Affine {
                                inequality: &negation,
                                right: *opposite,
                            },
                        )
                        .disposition
                            == ProofDisposition::Proved
                    });
                return RelationBatch {
                    disposition: if refuted {
                        TargetDisposition::Refuted
                    } else {
                        TargetDisposition::Unproved
                    },
                    evidence,
                };
            }
        }
        RelationBatch {
            disposition: TargetDisposition::Proved,
            evidence,
        }
    }

    fn target_measure(
        &mut self,
        source: TermId,
        view: &mut ProofFlowState,
        next: Option<&NextHeader<'_>>,
        targets: &mut HashMap<TermId, TermId>,
    ) -> TermId {
        let Some(next) = next else {
            return source;
        };
        let TermKind::Measure(_, path) = self.vocabulary.terms.kind(source) else {
            return source;
        };
        if !path.path.iter().any(|step| matches!(step, PlaceStep::Index(offset) if offset.support() == Some(next.binder))) { return source; }
        if let Some(target) = targets.get(&source) {
            return *target;
        }
        // The source former already registered exactly this measured place's
        // standing siblings. Clone the table facts with all siblings renamed.
        let mut pairs = Vec::new();
        for measure in [
            CheckedMeasure::Length,
            CheckedMeasure::Capacity,
            CheckedMeasure::Head,
        ] {
            if let Some(sibling) = self.vocabulary.terms.sibling_measure(source, measure) {
                let target = self.vocabulary.terms.intern(TermKind::TargetMeasure {
                    measure,
                    source: sibling,
                    loop_id: next.loop_id,
                    edge: next.edge.clone(),
                });
                targets.insert(sibling, target);
                pairs.push((sibling, target));
            }
        }
        for (sibling, target) in &pairs {
            if let Some(bound) = self.vocabulary.terms.measure_bound(*sibling) {
                let bound = match bound {
                    MeasureBound::Constant(value) => MeasureBound::Constant(value),
                    MeasureBound::Equal(term) => {
                        MeasureBound::Equal(targets.get(&term).copied().unwrap_or(term))
                    }
                };
                self.vocabulary.terms.set_measure_bound(*target, bound);
            }
        }
        for (_, target) in pairs {
            self.vocabulary.measure_atom(target, &view.affine);
        }
        targets[&source]
    }

    fn form_relation_measure(
        &mut self,
        expression: &CheckedExpression,
        view: &mut ProofFlowState,
        next: Option<&NextHeader<'_>>,
        targets: &mut HashMap<TermId, TermId>,
        proofs: &mut Vec<DerivationId>,
    ) -> Result<(), Option<LoopFormationFailure>> {
        let (mut base, steps) = match expression {
            CheckedExpression::ContainerMeasure { root, .. } => (
                ResolvedPlace {
                    root: root.root,
                    path: Vec::new(),
                    atomic_aliases: Vec::new(),
                },
                root.path.as_slice(),
            ),
            CheckedExpression::RangeElementMeasure { place, .. } => {
                let mut base = bound_place(place.root.binding);
                self.form_relation_subscript(
                    &base,
                    MeasuredKind::Range,
                    None,
                    &place.offset,
                    &place.obligation,
                    view,
                    next,
                    targets,
                    proofs,
                )?;
                base.path.push(PlaceStep::Index(place.captured));
                (base, place.path.as_slice())
            }
            _ => return Ok(()),
        };
        for step in steps {
            match step {
                CheckedPlaceStep::Field(field) => base.path.push(PlaceStep::Field(*field)),
                CheckedPlaceStep::BoxReferent(_) => base.path.push(PlaceStep::Deref),
                CheckedPlaceStep::Subscript(subscript) => {
                    // Nominal subscripts are ordinary checked operations;
                    // affine measure terms admit only their tracked offsets.
                    if let Some(measured) = measured_kind(subscript.base_type) {
                        self.form_relation_subscript(
                            &base,
                            measured,
                            type_constant(subscript.base_type),
                            &subscript.offset,
                            &subscript.obligation,
                            view,
                            next,
                            targets,
                            proofs,
                        )?;
                    }
                    base.path.push(PlaceStep::Index(subscript.captured));
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn form_relation_subscript(
        &mut self,
        base: &ResolvedPlace,
        measured: MeasuredKind,
        length: Option<CheckedConst>,
        offset: &CheckedExpression,
        site: &crate::NodePath,
        view: &mut ProofFlowState,
        next: Option<&NextHeader<'_>>,
        targets: &mut HashMap<TermId, TermId>,
        proofs: &mut Vec<DerivationId>,
    ) -> Result<(), Option<LoopFormationFailure>> {
        let source =
            self.place_measure_term(CheckedMeasure::Length, base.clone(), measured, length);
        let term = self.target_measure(source, view, next, targets);
        let mut target_values = view.affine.clone();
        if let Some(next) = next {
            target_values.values.insert(next.binder, next.value.clone());
        }
        let value = self
            .input
            .direct_goal_expression(offset)
            .and_then(|offset| self.affine_goal_value(&offset, &target_values))
            .ok_or(None)?;
        let extent = self.vocabulary.measure_atom(term, &view.affine);
        let inequality =
            AffineInequality::from_bounded_forms(&value, &extent, -1, &mut AffineCheckState::new())
                .map_err(|_| None)?;
        let proof = self.prove(
            ProofContext::new(&view.facts, &view.affine),
            ProofGoal::Affine {
                inequality: &inequality,
                right: Some(term),
            },
        );
        if proof.disposition == ProofDisposition::Proved {
            proofs.push(proof.derivation.expect("formed subscript has evidence"));
            Ok(())
        } else {
            let mut required = format!(
                "{} < {}.len",
                self.input.render_expression(offset),
                self.input.render_place(base)
            );
            if let Some(next) = next {
                required = substitute_next_name(&required, &self.input.binding_name(next.binder));
            }
            Err(Some(LoopFormationFailure {
                site: site.clone(),
                required,
            }))
        }
    }
}

/// Render a checked binder occurrence, preserving every other identifier.
/// The checked renderer is canonical, so identifier boundaries suffice here.
pub(super) fn substitute_next_name(rendered: &str, binder: &str) -> String {
    let mut output = String::new();
    let mut word = String::new();
    for ch in rendered.chars().chain(std::iter::once('\0')) {
        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            if word == binder {
                output.push_str(&format!("({binder} + 1_u64)"));
            } else {
                output.push_str(&word);
            }
            word.clear();
            if ch != '\0' {
                output.push(ch);
            }
        }
    }
    output
}
