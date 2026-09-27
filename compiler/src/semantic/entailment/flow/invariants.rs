//! [INV-1] loop invariants: the checked affine forms of an invariant,
//! its base and backedge batches, and its recorded outcome.

use super::*;

impl Reasoning<'_, '_, '_> {
    pub(super) fn checked_affine_form(
        &mut self,
        expression: &CheckedAffineExpression,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Result<AffineForm, AffineCheckError> {
        enum Pending<'expression> {
            Visit(&'expression CheckedAffineExpression),
            Add,
            Subtract,
            Scale(i128),
        }

        let mut pending = vec![Pending::Visit(expression)];
        let mut values = Vec::new();
        while let Some(next) = pending.pop() {
            match next {
                Pending::Visit(expression) => match &expression.kind {
                    CheckedAffineExpressionKind::Constant { value, .. } => {
                        values.push(AffineForm::constant(*value));
                    }
                    CheckedAffineExpressionKind::Local { binding, .. } => {
                        let value = if let Some(value) = state.values.get(binding) {
                            value.clone()
                        } else {
                            let value = self
                                .new_affine_binding_atom(*binding)
                                .ok_or(AffineCheckError::CoefficientMismatch)?;
                            state.values.insert(*binding, value.clone());
                            value
                        };
                        values.push(value);
                    }
                    // [INV-1, MSR-2] a measure factor's image is the one this
                    // program point holds for that term. It is retargeted by
                    // exactly the events that kill the term, so a relation
                    // proved before a write says nothing after it.
                    CheckedAffineExpressionKind::Measure(measure) => {
                        let term = self
                            .checked_measure_term(measure)
                            .ok_or(AffineCheckError::CoefficientMismatch)?;
                        values.push(self.vocabulary.measure_atom(term, state));
                    }
                    // [INV-1, MSR-6, ENT-2] a const generic at the symbolic
                    // instance is the declaration-anchored constant term, and
                    // no [ENT-5] event kills it, so its image is one
                    // immutable atom for the whole walk.
                    CheckedAffineExpressionKind::ConstGeneric { declaration, .. } => {
                        let term = self.const_parameter_term(*declaration);
                        values.push(self.vocabulary.measure_atom(term, state));
                    }
                    CheckedAffineExpressionKind::Add(left, right) => {
                        pending.push(Pending::Add);
                        pending.push(Pending::Visit(right));
                        pending.push(Pending::Visit(left));
                    }
                    CheckedAffineExpressionKind::Subtract(left, right) => {
                        pending.push(Pending::Subtract);
                        pending.push(Pending::Visit(right));
                        pending.push(Pending::Visit(left));
                    }
                    CheckedAffineExpressionKind::MultiplyByConstant {
                        constant, value, ..
                    } => {
                        pending.push(Pending::Scale(*constant));
                        pending.push(Pending::Visit(value));
                    }
                },
                Pending::Add => {
                    let right = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    let left = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    values.push(left.add(&right, check)?);
                }
                Pending::Subtract => {
                    let right = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    let left = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    values.push(left.subtract(&right, check)?);
                }
                Pending::Scale(constant) => {
                    let value = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                    values.push(value.scale(constant, check)?);
                }
            }
        }
        let result = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
        if values.is_empty() {
            Ok(result)
        } else {
            Err(AffineCheckError::CoefficientMismatch)
        }
    }

    pub(super) fn checked_loop_invariant_inequality(
        &mut self,
        invariant: &CheckedLoopInvariant,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Option<AffineInequality> {
        self.checked_affine_relation_inequality(&invariant.relation, state, check)
            .ok()
    }

    /// [INV-1] the second member of an `==` target, `b-a <= 0` beside the
    /// `a-b <= 0` the record carries. A relation written with one of the four
    /// ordered symbols has no second member and answers `None`.
    pub(super) fn checked_affine_relation_partner(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Option<Result<AffineInequality, AffineCheckError>> {
        if !relation.equality {
            return None;
        }
        let left = match self.checked_affine_form(&relation.left, state, check) {
            Ok(form) => form,
            Err(error) => return Some(Err(error)),
        };
        let right = match self.checked_affine_form(&relation.right, state, check) {
            Ok(form) => form,
            Err(error) => return Some(Err(error)),
        };
        Some(AffineInequality::from_bounded_forms(
            &right, &left, 0, check,
        ))
    }

    /// The disposition of one written invariant's batch in this state
    /// [INV-1]: one inequality, or both bounds of an equality.
    pub(super) fn prove_affine_relation_batch(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &mut ProofFlowState,
    ) -> TargetDisposition {
        let target = self
            .checked_affine_relation_inequality(
                relation,
                &mut state.affine,
                &mut AffineCheckState::new(),
            )
            .ok();
        let partner = self
            .checked_affine_relation_partner(
                relation,
                &mut state.affine,
                &mut AffineCheckState::new(),
            )
            .map(|partner| partner.ok());
        let right = self.checked_affine_right_term(&relation.right);
        let left = self.checked_affine_right_term(&relation.left);
        let mut members = vec![(target, right, left)];
        if let Some(partner) = partner {
            members.push((partner, left, right));
        }
        self.affine_target_disposition(&members, &state.facts, &state.affine)
    }

    /// [MSR-4] the disposition of one [INV-1] target's bounds in one state:
    /// proved when every bound is, refuted when the state derives the
    /// negation of one bound, and unproved otherwise. Each member carries its
    /// bound, that bound's own right-hand term, and the opposite side's term,
    /// which is the right-hand term of the bound's negation.
    pub(super) fn affine_target_disposition(
        &mut self,
        members: &[(Option<AffineInequality>, Option<TermId>, Option<TermId>)],
        facts: &FactState,
        affine: &AffineFlowState,
    ) -> TargetDisposition {
        let proved = members.iter().all(|(member, right, _)| {
            member.as_ref().is_some_and(|inequality| {
                self.prove(
                    ProofContext::new(facts, affine),
                    ProofGoal::Affine {
                        inequality,
                        right: *right,
                    },
                )
                .disposition
                    == ProofDisposition::Proved
            })
        });
        if proved {
            return TargetDisposition::Proved;
        }
        // A contradictory state proves every bound, so no negation below is
        // proved from a contradiction.
        let refuted = members.iter().any(|(member, _, opposite)| {
            member
                .as_ref()
                .and_then(|inequality| inequality.negated(&mut AffineCheckState::new()).ok())
                .is_some_and(|negation| {
                    self.prove(
                        ProofContext::new(facts, affine),
                        ProofGoal::Affine {
                            inequality: &negation,
                            right: *opposite,
                        },
                    )
                    .disposition
                        == ProofDisposition::Proved
                })
        });
        if refuted {
            TargetDisposition::Refuted
        } else {
            TargetDisposition::Unproved
        }
    }

    /// INV-1 base is a simultaneous batch: every target is checked against
    /// the same preheader state before any invariant from the batch becomes an
    /// assumption.
    pub(super) fn prove_loop_invariant_bases(
        &mut self,
        invariants: &[CheckedLoopInvariant],
        state: &mut ProofFlowState,
    ) -> Vec<TargetDisposition> {
        invariants
            .iter()
            .map(|invariant| self.prove_affine_relation_batch(&invariant.relation, state))
            .collect()
    }

    /// Installs the complete invariant batch at a generic loop header only
    /// after every base judgment succeeded. No source-order prefix can lend
    /// authority to a later base case.
    pub(super) fn activate_loop_invariant_batch(
        &mut self,
        loop_id: CheckedLoopId,
        invariants: &[CheckedLoopInvariant],
        base_batch: bool,
        state: &mut AffineFlowState,
    ) {
        for (source_ordinal, invariant) in invariants.iter().enumerate() {
            let target = self.checked_affine_relation_inequality(
                &invariant.relation,
                state,
                &mut AffineCheckState::new(),
            );
            let partner = self
                .checked_affine_relation_partner(
                    &invariant.relation,
                    state,
                    &mut AffineCheckState::new(),
                )
                .and_then(Result::ok);
            self.vocabulary
                .invariant_targets
                .insert(invariant.declaration, target.clone());
            if base_batch && let Ok(inequality) = target {
                state
                    .published_invariants
                    .insert(invariant.declaration, inequality.clone());
                // [INV-1] an `==` target is one batch of two bounds, and both
                // become assumptions together once the base batch succeeded.
                for inequality in std::iter::once(inequality).chain(partner) {
                    state.facts.push(ActiveAffineFact {
                        inequality,
                        evidence: AffineFactEvidence::Source(SourceAffineFactRef::LoopInvariant(
                            SourceLoopInvariantRef {
                                loop_id,
                                source_ordinal: u32::try_from(source_ordinal)
                                    .expect("loop invariant ordinal exceeds u32"),
                            },
                        )),
                        active_loops: vec![loop_id],
                    });
                }
            }
        }
    }

    pub(super) fn checked_affine_relation_inequality(
        &mut self,
        relation: &CheckedAffineRelation,
        state: &mut AffineFlowState,
        check: &mut AffineCheckState,
    ) -> Result<AffineInequality, AffineCheckError> {
        let left = self.checked_affine_form(&relation.left, state, check)?;
        let right = self.checked_affine_form(&relation.right, state, check)?;
        AffineInequality::from_bounded_forms(&left, &right, relation.bound, check)
    }

    /// Recognizes the source right side's one L0 term plus displacement,
    /// using the existing source normalizer rather than the current value's
    /// coefficient vector. The displacement already belongs to the target
    /// inequality, so only the term is needed by Step 6.
    pub(super) fn checked_affine_right_term(
        &mut self,
        expression: &CheckedAffineExpression,
    ) -> Option<TermId> {
        let source = CheckedAffineRelation {
            node_path: expression.node_path.clone(),
            left: CheckedAffineExpression {
                node_path: expression.node_path.clone(),
                kind: CheckedAffineExpressionKind::Constant {
                    value: 0,
                    ty: IntegerType::U64,
                },
            },
            right: expression.clone(),
            bound: 0,
            equality: false,
        };
        let Relation::Bound {
            left: ZERO,
            right,
            bound,
        } = self.checked_affine_relation_l0(&source)?
        else {
            return None;
        };
        if right == ZERO {
            Some(self.vocabulary.terms.intern(TermKind::Constant(bound)))
        } else {
            Some(right)
        }
    }

    /// Projects the exact source relation into L0 when its normalized binding
    /// coefficients have one of the fixed difference-bound shapes. This does
    /// no discovery: it only recognizes `x - y <= c`, `x <= c`, `c <= x`, or
    /// a constant proposition after the source-written affine arithmetic has
    /// been normalized.
    pub(super) fn checked_affine_relation_l0(
        &mut self,
        relation: &CheckedAffineRelation,
    ) -> Option<Relation> {
        /// One leaf of the written relation, in the order the walk reaches it.
        enum SourceLeaf {
            Local(BindingId),
            /// [INV-1] one measure factor, already interned as its [ENT-2]
            /// term by the pre-pass below.
            Measure(TermId),
        }

        fn source_form(
            expression: &CheckedAffineExpression,
            leaves: &mut Vec<SourceLeaf>,
            measures: &[TermId],
            visited: &mut usize,
            check: &mut AffineCheckState,
        ) -> Option<AffineForm> {
            let mut values: Vec<AffineForm> = Vec::new();
            for expression in expression.postorder() {
                let value = match &expression.kind {
                    CheckedAffineExpressionKind::Constant { value, .. } => {
                        AffineForm::constant(*value)
                    }
                    CheckedAffineExpressionKind::Local { binding, .. } => {
                        let index = leaves
                            .iter()
                            .position(|candidate| {
                                matches!(candidate, SourceLeaf::Local(other) if other == binding)
                            })
                            .unwrap_or_else(|| {
                                leaves.push(SourceLeaf::Local(*binding));
                                leaves.len() - 1
                            });
                        let index = u32::try_from(index).ok()?;
                        AffineForm::term(AffineTermId::from_index(index))
                    }
                    CheckedAffineExpressionKind::Measure(_)
                    | CheckedAffineExpressionKind::ConstGeneric { .. } => {
                        let term = *measures.get(*visited)?;
                        *visited = visited.checked_add(1)?;
                        let index = leaves
                            .iter()
                            .position(|candidate| {
                                matches!(candidate, SourceLeaf::Measure(other) if *other == term)
                            })
                            .unwrap_or_else(|| {
                                leaves.push(SourceLeaf::Measure(term));
                                leaves.len() - 1
                            });
                        let index = u32::try_from(index).ok()?;
                        AffineForm::term(AffineTermId::from_index(index))
                    }
                    CheckedAffineExpressionKind::Add(_, _) => {
                        let right = values.pop()?;
                        let left = values.pop()?;
                        left.add(&right, check).ok()?
                    }
                    CheckedAffineExpressionKind::Subtract(_, _) => {
                        let right = values.pop()?;
                        let left = values.pop()?;
                        left.subtract(&right, check).ok()?
                    }
                    CheckedAffineExpressionKind::MultiplyByConstant { constant, .. } => {
                        values.pop()?.scale(*constant, check).ok()?
                    }
                };
                values.push(value);
            }
            values.pop()
        }

        // Interning needs `&mut self`, and the walk above does not have it, so
        // the measure terms are resolved first in exactly the order that walk
        // reaches them.
        let mut measures = Vec::new();
        self.collect_affine_measure_terms(&relation.left, &mut measures)?;
        self.collect_affine_measure_terms(&relation.right, &mut measures)?;
        let mut leaves = Vec::new();
        let mut visited = 0;
        let mut check = AffineCheckState::new();
        let left = source_form(
            &relation.left,
            &mut leaves,
            &measures,
            &mut visited,
            &mut check,
        )?;
        let right = source_form(
            &relation.right,
            &mut leaves,
            &measures,
            &mut visited,
            &mut check,
        )?;
        let inequality =
            AffineInequality::from_bounded_forms(&left, &right, relation.bound, &mut check).ok()?;
        let mut term = |coefficient: super::super::affine::AffineCoefficient| match leaves
            .get(coefficient.term().index() as usize)?
        {
            SourceLeaf::Measure(term) => Some(*term),
            SourceLeaf::Local(binding) => {
                let binding = *binding;
                let fragment = fragment_type(CheckedType::Integer(
                    self.input.affine_binding_type(binding)?,
                ))?;
                Some(self.vocabulary.terms.intern(TermKind::Place(
                    ResolvedPlace::spelled(PlaceRoot::Binding(binding), false, Vec::new()),
                    fragment,
                )))
            }
        };
        let (left, right) = match inequality.terms() {
            [] => (ZERO, ZERO),
            [coefficient] if coefficient.coefficient() == 1 => (term(*coefficient)?, ZERO),
            [coefficient] if coefficient.coefficient() == -1 => (ZERO, term(*coefficient)?),
            [first, second] => match (first.coefficient(), second.coefficient()) {
                (1, -1) => (term(*first)?, term(*second)?),
                (-1, 1) => (term(*second)?, term(*first)?),
                _ => return None,
            },
            _ => return None,
        };
        Some(Relation::Bound {
            left,
            right,
            bound: inequality.upper(),
        })
    }
}

impl Judging<'_, '_, '_> {
    pub(super) fn record_loop_invariant_outcomes(
        &mut self,
        loop_id: CheckedLoopId,
        invariants: &[CheckedLoopInvariant],
        base: &[TargetDisposition],
        step: &[Option<TargetDisposition>],
        counted_binder: Option<BindingId>,
    ) {
        for (index, invariant) in invariants.iter().enumerate() {
            self.output.loop_invariants.push(LoopInvariantOutcome {
                node_path: invariant.relation.node_path.clone(),
                loop_id,
                source_ordinal: u32::try_from(index).expect("loop invariant ordinal exceeds u32"),
                name: invariant.name.clone(),
                base_target: self
                    .input
                    .render_checked_invariant_relation(&invariant.relation, None),
                backedge_target: self
                    .input
                    .render_checked_invariant_relation(&invariant.relation, counted_binder),
                proof: LoopInvariantProof {
                    base: base[index] == TargetDisposition::Proved,
                    step: step[index].map(|step| step == TargetDisposition::Proved),
                    base_refuted: base[index] == TargetDisposition::Refuted,
                    step_refuted: step[index] == Some(TargetDisposition::Refuted),
                },
            });
        }
    }
}

pub(super) fn remove_active_loop_invariants(
    state: &mut AffineFlowState,
    loop_id: CheckedLoopId,
    declarations: &[crate::DeclarationId],
) {
    state
        .facts
        .retain(|fact| !fact.active_loops.contains(&loop_id));
    for declaration in declarations {
        state.published_invariants.remove(declaration);
    }
}
