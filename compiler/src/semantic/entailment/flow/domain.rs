//! The per-path fact domain: joining the fact and affine states of
//! converging paths, promoting a contradiction, and forming the affine value
//! images and atoms the facts are stated over.

use super::*;

impl Input<'_, '_> {
    /// CONST-2's total field selection substitutes the immutable initializer.
    /// Preserve that known scalar while reads still carry the constant's
    /// actual storage identity. A subscript remains an OP-4 operation rather
    /// than adding a new automatic constant-evaluation family here.
    pub(super) fn constant_storage_scalar(
        &self,
        expression: &CheckedExpression,
    ) -> Option<&CheckedValue> {
        let CheckedExpression::ReadStorage { root, .. } = expression else {
            return None;
        };
        let PlaceRoot::Constant(id) = root.root else {
            return None;
        };
        let declaration = self.context.constant_declaration(id)?;
        let mut value = &self.context.constant(declaration)?.value;
        for step in &root.path {
            let CheckedPlaceStep::Field(index) = step else {
                return None;
            };
            let CheckedValue::Struct { fields, .. } = value else {
                return None;
            };
            value = fields.get(*index as usize)?;
        }
        (self.is_copy(root.ty) && value.ty() == root.ty).then_some(value)
    }

    /// Reconstructs the exact source-order place path retained by the checked
    /// expression. This is deliberately recursive: field selection may occur
    /// before or after a deref, and nested boxes may introduce more than one
    /// deref. [ENT-2] distinguishes those canonical spellings.
    pub(super) fn read_place_path(&self, expression: &CheckedExpression) -> Option<ResolvedPlace> {
        match expression {
            CheckedExpression::Binding { binding, .. } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                path: Vec::new(),
            }),
            CheckedExpression::Project {
                binding,
                fields,
                consume_root: false,
                ..
            } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                path: fields.iter().copied().map(PlaceStep::Field).collect(),
            }),
            CheckedExpression::DerefAddressed { binding, .. } => Some(ResolvedPlace {
                root: PlaceRoot::Binding(*binding),
                // A reference binding is the body-local name of its referent
                // path [REF-1]. The written `deref` is that boundary wrapper;
                // concrete Box content steps are appended below.
                path: Vec::new(),
            }),
            CheckedExpression::BoxDeref { value, .. } => {
                let mut path = self.read_place_path(value)?;
                path.path.push(PlaceStep::Deref);
                Some(path)
            }
            CheckedExpression::ProjectValue { value, field, .. } => {
                let mut path = self.read_place_path(value)?;
                path.path.push(PlaceStep::Field(*field));
                Some(path)
            }
            // [ENT-2] clause (b): a subscripted place is a term exactly when
            // its final step selects a readonly field of one fragment type.
            // Its offsets are part of its identity, so only a place whose
            // every offset was captured is one; the measure former keeps its
            // own row in `measure_operand`.
            CheckedExpression::ReadStorage { root, .. }
                if root.readonly_field_term(self.context.nominals)
                    == Some(SubscriptedTerm::Represented) =>
            {
                Some(container_root_path(root))
            }
            CheckedExpression::RangeIndex { place, .. }
                if place.readonly_field_term(self.context.nominals)
                    == Some(SubscriptedTerm::Represented) =>
            {
                Some(ResolvedPlace::from_path(
                    place.root.binding,
                    place.place_path(),
                ))
            }
            _ => None,
        }
    }

    /// [OWN-1] whether a value of this type is read without being consumed,
    /// so a place of it is an ordinary goal datum. A type parameter standing
    /// for itself answers false here: the flow state keeps no fact about a
    /// value of a type it cannot see.
    pub(super) fn is_copy(&self, ty: CheckedType) -> bool {
        crate::semantic::model::type_has_copy_capability(
            ty,
            self.context.nominals,
            self.context.elements,
            &|_| Some(false),
        )
        .unwrap_or(false)
    }

    /// Walks one block in its own lexical scope. Returns the fall-through:
    /// `true` when control continues past the block with `state` holding the
    /// post-scope-exit facts.
    pub(super) fn affine_binding_type(&self, binding: BindingId) -> Option<IntegerType> {
        match self.summary(binding)?.ty? {
            CheckedType::Integer(ty) if !is_holder(binding) => Some(ty),
            _ => None,
        }
    }
}

impl Vocabulary {
    /// Contradiction is absorbing. Promote the complete combined closure
    /// before every kill entry so a write cannot erase one premise and make
    /// an unreachable point reachable again.
    pub(super) fn promote_contradiction(&mut self, state: &mut FactState) {
        if state.all_derivable {
            return;
        }
        // A seeded closure costs about what the proof-free probe does over a
        // closed core and answers the same question directly; the probe stays
        // the cheaper test for a state with no closed part.
        if !closure_is_seeded(state)
            && !contradiction_without_proofs(state, &self.terms, &self.goals)
        {
            return;
        }
        let closed = close(state, &self.terms, &self.goals, &mut self.derivations);
        if closed.contradictory() {
            state.promote_to_contradiction(closed.contradiction_proof());
        }
    }

    pub(super) fn promote_flow_contradiction(&mut self, states: &mut ProofFlowState) {
        self.promote_contradiction(&mut states.facts);
    }

    /// Decomposes a finite checked value graph, expanding handles and exact
    /// product records only. Multiplication allows one binder-dependent
    /// operand and one invariant operand; its two resulting components must
    /// stay affine. Unknown body values and nonlinear binder uses fail closed.
    pub(super) fn counted_value_image(
        &self,
        form: &AffineForm,
        binder: AffineTermId,
        invariant: &HashSet<AffineTermId>,
        visiting: &mut HashSet<AffineTermId>,
    ) -> Option<CountedValueImage> {
        let mut image = CountedValueImage {
            stride: AffineForm::constant(0),
            base: AffineForm::constant(form.constant_value()),
        };
        let mut check = AffineCheckState::new();
        for coefficient in form.terms() {
            let term = coefficient.term();
            if !visiting.insert(term) {
                return None;
            }
            let term_image = self.counted_atom_image(term, binder, invariant, visiting)?;
            visiting.remove(&term);
            image.stride = image
                .stride
                .add(
                    &term_image
                        .stride
                        .scale(coefficient.coefficient(), &mut check)
                        .ok()?,
                    &mut check,
                )
                .ok()?;
            image.base = image
                .base
                .add(
                    &term_image
                        .base
                        .scale(coefficient.coefficient(), &mut check)
                        .ok()?,
                    &mut check,
                )
                .ok()?;
        }
        Some(image)
    }

    pub(super) fn counted_atom_image(
        &self,
        term: AffineTermId,
        binder: AffineTermId,
        invariant: &HashSet<AffineTermId>,
        visiting: &mut HashSet<AffineTermId>,
    ) -> Option<CountedValueImage> {
        if term == binder {
            return Some(CountedValueImage {
                stride: AffineForm::constant(1),
                base: AffineForm::constant(0),
            });
        }
        // A preheader product can mint a handle for a transparent sum. The
        // handle and its image name the same value, so expand it before the
        // invariant-atom case: otherwise the product's stride and an endpoint
        // that reads the sum directly would have different canonical images.
        if let Some(image) = self.handle_images.get(&term) {
            return self.counted_value_image(image, binder, invariant, visiting);
        }
        if invariant.contains(&term) {
            return Some(CountedValueImage {
                stride: AffineForm::constant(0),
                base: AffineForm::term(term),
            });
        }
        let (left, right) = *self.product_atoms.get(&term)?;
        let left =
            self.counted_value_image(&AffineForm::term(left), binder, invariant, visiting)?;
        let right =
            self.counted_value_image(&AffineForm::term(right), binder, invariant, visiting)?;
        let zero = AffineForm::constant(0);
        let (varying, fixed) = if left.stride == zero && right.stride == zero {
            // A pure exact product of fixed values is itself fixed, even
            // when the source computes it in the body. Keep its exact atom.
            return Some(CountedValueImage {
                stride: zero,
                base: AffineForm::term(term),
            });
        } else if right.stride == zero {
            (left, right.base)
        } else if left.stride == zero {
            (right, left.base)
        } else {
            return None;
        };
        let multiply = |left: &AffineForm, right: &AffineForm| {
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
        };
        Some(CountedValueImage {
            stride: multiply(&varying.stride, &fixed)?,
            base: multiply(&varying.base, &fixed)?,
        })
    }

    pub(super) fn affine_term_value(
        &mut self,
        term: TermId,
        state: &AffineFlowState,
    ) -> Option<AffineForm> {
        match self.terms.kind(term).clone() {
            TermKind::Zero => Some(AffineForm::constant(0)),
            TermKind::Constant(value) => Some(AffineForm::constant(value)),
            TermKind::Place(place, _) if place.path.is_empty() => match place.root {
                PlaceRoot::Binding(binding) => state.values.get(&binding).cloned(),
                _ => None,
            },
            // [MSR-4] a measure term's image is its own compiler-owned atom,
            // [MSR-3] a measure datum inherits the atom of the term it
            // denotes, and [MSR-6] gives one immutable image to a symbolic
            // const parameter throughout the generic body.
            TermKind::ConstParameter(..)
            | TermKind::Measure(..)
            | TermKind::EntryDatum { .. }
            | TermKind::MeasureDatum { .. }
            | TermKind::CallDatum {
                measure: Some(_), ..
            } => Some(self.measure_atom(term, state)),
            TermKind::Place(_, _)
            | TermKind::CountedCapture { .. }
            | TermKind::IndexCapture { .. }
            | TermKind::ResultPayload(_)
            | TermKind::CommitValue { .. }
            | TermKind::CallDatum { .. } => None,
        }
    }

    pub(super) fn new_affine_atom(&mut self, ty: IntegerType) -> AffineForm {
        let (minimum, maximum) = type_range(ty);
        self.new_affine_atom_with_interval(ty, minimum, maximum, false)
    }

    pub(super) fn new_affine_atom_with_interval(
        &mut self,
        ty: IntegerType,
        minimum: i128,
        maximum: i128,
        join_delta: bool,
    ) -> AffineForm {
        let index = u32::try_from(self.affine_atoms.len())
            .expect("affine value atoms exceed the u32 identity space");
        self.affine_atoms.push(AffineAtom {
            ty,
            minimum,
            maximum,
            join_delta,
        });
        AffineForm::term(AffineTermId::from_index(index))
    }

    /// Folds every delta atom an earlier join minted back into the constant
    /// interval it stands for [ENT-6].
    ///
    /// Without this, a delta atom counts as an ordinary nonconstant term at
    /// the next join, so two joins in sequence lose an image one join over the
    /// same branches keeps: acceptance would depend on whether the writer
    /// spelled the branch set as nested conditionals or as one flat match.
    pub(super) fn fold_join_deltas(&self, value: &AffineForm) -> Option<FoldedJoinImage> {
        let mut form = value.nonconstant_part();
        let mut minimum = value.constant_value();
        let mut maximum = minimum;
        let mut check = AffineCheckState::new();
        for coefficient in value.terms() {
            let atom = self.affine_atoms.get(coefficient.term().index() as usize)?;
            if !atom.join_delta {
                continue;
            }
            let low = coefficient.coefficient().checked_mul(atom.minimum)?;
            let high = coefficient.coefficient().checked_mul(atom.maximum)?;
            minimum = minimum.checked_add(low.min(high))?;
            maximum = maximum.checked_add(low.max(high))?;
            let folded = AffineForm::term(coefficient.term())
                .scale(coefficient.coefficient(), &mut check)
                .ok()?;
            form = form.subtract(&folded, &mut check).ok()?;
        }
        Some(FoldedJoinImage {
            form,
            minimum,
            maximum,
        })
    }

    pub(super) fn affine_unknown_integer(&mut self, ty: CheckedType) -> Option<AffineForm> {
        let CheckedType::Integer(ty) = ty else {
            return None;
        };
        Some(self.new_affine_atom(ty))
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn initialize_affine_parameters(&mut self, state: &mut AffineFlowState) {
        let parameters = self
            .input
            .function
            .parameters
            .iter()
            .filter_map(|parameter| match (parameter.mode, parameter.ty) {
                (CheckedMode::Own, CheckedType::Integer(ty)) => Some((parameter.binding, ty)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (binding, ty) in parameters {
            let value = self.vocabulary.new_affine_atom(ty);
            state.values.insert(binding, value);
        }
    }

    /// Reads an expression as a term or constant [ENT-2]; anything else is
    /// no operand and establishes or derives nothing.
    pub(super) fn read_operand(&mut self, expression: &CheckedExpression) -> Option<TermId> {
        if let Some(value) = self.input.constant_storage_scalar(expression).cloned() {
            return self.read_operand(&CheckedExpression::Constant(value));
        }
        match expression {
            // [MSR-6] a const generic read as a value is the symbolic
            // constant term [ENT-2] clause (c) fixes; a concrete [FN-2]
            // instance has already folded it to an integer constant.
            CheckedExpression::Constant(CheckedValue::ConstGeneric { declaration, .. }) => {
                return Some(self.const_parameter_term(*declaration));
            }
            CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
            | CheckedExpression::NamedConstant {
                value: CheckedValue::Integer { ty, bits },
                ..
            } => {
                return Some(
                    self.vocabulary
                        .terms
                        .intern(TermKind::Constant(integer_value(*ty, *bits))),
                );
            }
            _ => {}
        }
        let fragment = fragment_type(expression.ty())?;
        let path = self.input.read_place_path(expression)?;
        let kind = match path.path.as_slice() {
            projections
                if projections
                    .iter()
                    .all(|projection| matches!(projection, PlaceStep::Field(_))) =>
            {
                TermKind::Place(path, fragment)
            }
            _ => TermKind::Place(path, fragment),
        };
        Some(self.vocabulary.terms.intern(kind))
    }

    /// Files one [REF-4] formation's endpoint images under its start capture,
    /// the key every reader of the range-image table looks them up by.
    ///
    /// Only a source occurrence names one formation, and the checker gives
    /// every endpoint the occurrence that evaluated it. An image filed under
    /// any other capture would answer for every range carrying that capture
    /// [OWN-7], so nothing is filed for it and no ordering separates it.
    pub(super) fn file_range_image(
        &mut self,
        carrier: &crate::NodePath,
        captured: CapturedRange,
        start: &CheckedExpression,
        end: &CheckedExpression,
        affine: &mut AffineFlowState,
    ) {
        if !matches!(captured.start.capture, CaptureId::Source(_)) {
            return;
        }
        if let Some(image) = self.range_formation_image(carrier, start, end, affine) {
            affine.ranges.insert(captured.start.capture, image);
        }
    }

    /// The image one [REF-4] formation publishes: its two endpoints' affine
    /// forms in `affine`, attached to the formation node.
    pub(super) fn range_formation_image(
        &mut self,
        carrier: &crate::NodePath,
        start: &CheckedExpression,
        end: &CheckedExpression,
        affine: &mut AffineFlowState,
    ) -> Option<AffineRangeImage> {
        self.affine_expression_form(start, affine)
            .zip(self.affine_expression_form(end, affine))
            .map(|(start, end)| AffineRangeImage {
                source: carrier.clone(),
                start,
                end,
            })
    }

    pub(super) fn new_affine_binding_atom(&mut self, binding: BindingId) -> Option<AffineForm> {
        let ty = self.input.affine_binding_type(binding)?;
        Some(self.vocabulary.new_affine_atom(ty))
    }

    /// The one atom that stands for a binding's whole value, for the length of
    /// one certificate.
    ///
    /// A binding whose image is already a single atom is its own handle and
    /// mints nothing. Otherwise a fresh atom is minted and the image it stands
    /// for is remembered, so the handle can be unfolded again before anything
    /// is proved.
    ///
    /// [PRF-1]'s fold needs this because a local's image is transparent by
    /// design. `let stride = width + padding; let base = stride * row;` gives
    /// the product the operands `width + padding` and `row`, so a certificate
    /// scaling by `stride` distributes into pieces no admitted multiplication
    /// matches. Naming the binding on both sides — the product it forms and
    /// the multiplicity that scales by it — is what makes the two agree, and
    /// it is the rule [PRF-1] already states one sentence away for a named
    /// premise: resolve by the declaration the writer wrote, not by whatever
    /// that declaration currently expands to.
    ///
    /// The handle is deliberately not published as a fact and does not replace
    /// the binding's image. Both were tried and both cost more than they
    /// bought: a published equality is invisible to the residual, which is the
    /// direct L0 route by rule, and replacing the image makes every ordinary
    /// premise about the binding need that equality to prove. Keeping it a
    /// name that exists between the fold and the residual leaves the rest of
    /// the checker reading exactly what it read before.
    pub(super) fn affine_opaque_handle(
        &mut self,
        binding: BindingId,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        if let Some(image) = state.values.get(&binding)
            && image.unit_term().is_some()
        {
            return Some(image.clone());
        }
        if let Some(handle) = state.opaque_values.get(&binding) {
            return Some(handle.clone());
        }
        let handle = self.new_affine_binding_atom(binding)?;
        let Some(image) = state.values.get(&binding).cloned() else {
            state.values.insert(binding, handle.clone());
            state.opaque_values.insert(binding, handle.clone());
            return Some(handle);
        };
        let atom = handle.unit_term()?;
        self.vocabulary.handle_images.insert(atom, image);
        state.opaque_values.insert(binding, handle.clone());
        Some(handle)
    }

    pub(super) fn affine_expression_form(
        &mut self,
        expression: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        let mut events = Vec::new();
        self.input.collect_expression_kills(expression, &mut events);
        if !events.is_empty() {
            return self.vocabulary.affine_unknown_integer(expression.ty());
        }
        self.affine_pure_expression_form(expression, state)
    }

    /// The operand value images of one admitted S7 unsigned division, read
    /// where it was evaluated. A `set` commit reads both before its target
    /// kill, since an operand naming that place has a new image afterwards.
    pub(super) fn unsigned_division_operand_forms(
        &mut self,
        value: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<(AffineForm, AffineForm)> {
        let CheckedExpression::IntegerOperation { arguments, .. } = value else {
            return None;
        };
        let [dividend, divisor] = arguments.as_slice() else {
            return None;
        };
        Some((
            self.affine_pre_domain_form(dividend, state)?,
            self.affine_pre_domain_form(divisor, state)?,
        ))
    }

    /// The atom a multiplication's operand contributes to the fold: the
    /// binding's opaque handle when the operand is a plain read of one, and
    /// nothing otherwise.
    pub(super) fn affine_operand_handle(
        &mut self,
        operand: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineTermId> {
        let CheckedExpression::Binding { binding, ty, .. } = operand else {
            return None;
        };
        let CheckedType::Integer(integer) = *ty else {
            return None;
        };
        if self.input.affine_binding_type(*binding) != Some(integer) {
            return None;
        }
        self.affine_opaque_handle(*binding, state)?.unit_term()
    }

    pub(super) fn affine_pure_expression_form(
        &mut self,
        expression: &CheckedExpression,
        state: &mut AffineFlowState,
    ) -> Option<AffineForm> {
        if let Some(value) = self.input.constant_storage_scalar(expression).cloned() {
            return self.affine_pure_expression_form(&CheckedExpression::Constant(value), state);
        }
        let formed = match expression {
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
            // [MSR-6] endpoint and initializer value flow must use the same
            // declaration-anchored image as an invariant or contract
            // spelling of the symbolic const parameter.
            CheckedExpression::Constant(CheckedValue::ConstGeneric { declaration, .. }) => {
                let term = self.const_parameter_term(*declaration);
                Some(self.vocabulary.measure_atom(term, state))
            }
            CheckedExpression::Binding { binding, ty, .. } => {
                let CheckedType::Integer(integer) = *ty else {
                    return None;
                };
                if self.input.affine_binding_type(*binding) != Some(integer) {
                    Some(self.vocabulary.new_affine_atom(integer))
                } else if let Some(value) = state.values.get(binding) {
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
            } => self.affine_pure_expression_form(value, state),
            CheckedExpression::IntegerOperation {
                operation,
                arguments,
                result: CheckedType::Integer(_),
                ..
            } => {
                let [left, right] = arguments.as_slice() else {
                    return self.vocabulary.affine_unknown_integer(expression.ty());
                };
                let left = self.affine_pure_expression_form(left, state)?;
                let right = self.affine_pure_expression_form(right, state)?;
                let mut check = AffineCheckState::new();
                match operation {
                    CheckedIntegerOperation::AddExact | CheckedIntegerOperation::AddDefined => {
                        left.add(&right, &mut check).ok()
                    }
                    CheckedIntegerOperation::SubtractExact
                    | CheckedIntegerOperation::SubtractDefined => {
                        left.subtract(&right, &mut check).ok()
                    }
                    CheckedIntegerOperation::MultiplyExact
                    | CheckedIntegerOperation::MultiplyDefined => {
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
            _ => None,
        };
        formed.or_else(|| self.vocabulary.affine_unknown_integer(expression.ty()))
    }
}

impl Judging<'_, '_, '_> {
    /// Intersects every published affine fact by canonical numeric content,
    /// then records representative predecessor evidence for diagnostics.
    ///
    /// The first phase deliberately does not inspect evidence or the source
    /// category that published a fact: an inequality survives exactly when
    /// every predecessor contains the same relation over the same immutable
    /// value images. Loop dependencies are then unioned conservatively, so a
    /// cross-category match cannot carry an assumption beyond its loop.
    pub(super) fn join_affine_facts(&mut self, states: &[ProofFlowState]) -> Vec<ActiveAffineFact> {
        let Some(first) = states.first() else {
            return Vec::new();
        };

        // This is deliberately an ordered intersection, not a set iteration:
        // the first contributing structural predecessor fixes the retained
        // order, and the first canonical occurrence in that predecessor fixes
        // each fact's position. Evidence category never participates.
        let mut common_inequalities = Vec::new();
        for candidate in &first.affine.facts {
            if common_inequalities.contains(&candidate.inequality) {
                continue;
            }
            if states.iter().skip(1).all(|state| {
                state
                    .affine
                    .facts
                    .iter()
                    .any(|fact| fact.inequality == candidate.inequality)
            }) {
                common_inequalities.push(candidate.inequality.clone());
            }
        }

        common_inequalities
            .into_iter()
            .map(|inequality| {
                let witnesses = states
                    .iter()
                    .map(|state| {
                        state
                            .affine
                            .facts
                            .iter()
                            .filter(|fact| fact.inequality == inequality)
                            // A stable witness is strictly preferable to an
                            // active-loop assumption for the same canonical
                            // fact. Remaining ties keep deterministic fact
                            // insertion order and do not affect acceptance.
                            .min_by_key(|fact| fact.active_loops.len())
                            .expect("a common affine inequality has one witness")
                    })
                    .collect::<Vec<_>>();

                let mut active_loops = witnesses
                    .iter()
                    .flat_map(|fact| fact.active_loops.iter().copied())
                    .collect::<Vec<_>>();
                active_loops.sort_unstable_by_key(|loop_id| loop_id.0);
                active_loops.dedup();

                let first_evidence = witnesses[0].evidence;
                let evidence = if witnesses.iter().all(|fact| fact.evidence == first_evidence) {
                    first_evidence
                } else if witnesses
                    .iter()
                    .all(|fact| matches!(fact.evidence, AffineFactEvidence::Source(_)))
                {
                    let predecessors = witnesses
                        .iter()
                        .map(|fact| match fact.evidence {
                            AffineFactEvidence::Source(source) => source,
                            AffineFactEvidence::Derivation(_) => unreachable!(),
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice();
                    let join_ordinal = u32::try_from(self.output.joined_source_proofs.len())
                        .expect("joined affine fact count exceeds the u32 identity space");
                    self.output
                        .joined_source_proofs
                        .push(JoinedSourceProofProvenance { predecessors });
                    AffineFactEvidence::Source(SourceAffineFactRef::JoinedSourceProof {
                        join_ordinal,
                    })
                } else {
                    // Derivation and source identities are explanation only.
                    // The canonical all-predecessor intersection above is the
                    // sole authority for retaining this conclusion.
                    first_evidence
                };
                ActiveAffineFact {
                    inequality,
                    evidence,
                    active_loops,
                }
            })
            .collect()
    }

    pub(super) fn join_affine_states(&mut self, states: &[ProofFlowState]) -> AffineFlowState {
        let Some(first) = states.first() else {
            return AffineFlowState::default();
        };
        let mut bindings = first.affine.values.keys().copied().collect::<Vec<_>>();
        bindings.sort_by_key(|binding| binding.0);
        let mut values = WordHashMap::default();
        for binding in bindings {
            let Some(first_value) = first.affine.values.get(&binding) else {
                continue;
            };
            if !states
                .iter()
                .skip(1)
                .all(|state| state.affine.values.contains_key(&binding))
            {
                continue;
            }
            let value = if states.iter().skip(1).all(|state| {
                state
                    .affine
                    .values
                    .get(&binding)
                    .is_some_and(|value| value == first_value)
            }) {
                first_value.clone()
            } else if let Some(ty) = self.input.affine_binding_type(binding) {
                // [ENT-6] every input is normalized before the comparison: a
                // delta atom an earlier join minted folds back into the
                // constant interval it stands for, so what is compared is the
                // part of the image no join invented. Nested joins therefore
                // reach exactly the image one flat join over the same branches
                // reaches, and acceptance stops depending on the join shape.
                let folded = states
                    .iter()
                    .map(|state| {
                        state
                            .affine
                            .values
                            .get(&binding)
                            .and_then(|value| self.vocabulary.fold_join_deltas(value))
                    })
                    .collect::<Option<Vec<_>>>();
                match folded {
                    Some(folded)
                        if folded
                            .iter()
                            .skip(1)
                            .all(|image| image.form == folded[0].form) =>
                    {
                        let minimum = folded
                            .iter()
                            .map(|image| image.minimum)
                            .min()
                            .expect("one join input exists");
                        let maximum = folded
                            .iter()
                            .map(|image| image.maximum)
                            .max()
                            .expect("one join input exists");
                        let atom_start = self.vocabulary.affine_atoms.len();
                        let delta = self
                            .vocabulary
                            .new_affine_atom_with_interval(ty, minimum, maximum, true);
                        match folded[0].form.add(&delta, &mut AffineCheckState::new()) {
                            Ok(value) => value,
                            Err(_) => {
                                self.vocabulary.affine_atoms.truncate(atom_start);
                                self.vocabulary.new_affine_atom(ty)
                            }
                        }
                    }
                    _ => self.vocabulary.new_affine_atom(ty),
                }
            } else {
                continue;
            };
            values.insert(binding, value);
        }
        // An opaque handle is a convenience for one certificate, not a fact,
        // so a join keeps none: the next demand re-mints against whatever the
        // joined image is.
        let opaque_values = WordHashMap::default();

        // A measure keeps its current image only when every predecessor
        // carries that exact image. Otherwise a later query mints a fresh
        // image; no branch-local equality is promoted through this join.
        let mut measure_atoms = first.affine.measure_atoms.borrow().clone();
        measure_atoms.retain(|term, image| {
            states
                .iter()
                .skip(1)
                .all(|state| state.affine.measure_atoms.borrow().get(term) == Some(image))
        });

        AffineFlowState {
            values,
            measure_atoms: RefCell::new(measure_atoms),
            opaque_values,
            ranges: first
                .affine
                .ranges
                .iter()
                .filter(|(id, range)| {
                    states
                        .iter()
                        .skip(1)
                        .all(|state| state.affine.ranges.get(id) == Some(*range))
                })
                .map(|(id, range)| (*id, range.clone()))
                .collect(),
            indices: first
                .affine
                .indices
                .iter()
                .filter(|(id, image)| {
                    states
                        .iter()
                        .skip(1)
                        .all(|state| state.affine.indices.get(id) == Some(*image))
                })
                .map(|(id, image)| (*id, image.clone()))
                .collect(),
            facts: self.join_affine_facts(states),
            published_invariants: first
                .affine
                .published_invariants
                .iter()
                .filter(|(declaration, inequality)| {
                    states.iter().skip(1).all(|state| {
                        state.affine.published_invariants.get(declaration) == Some(*inequality)
                    })
                })
                .map(|(declaration, inequality)| (*declaration, inequality.clone()))
                .collect(),
        }
    }

    pub(super) fn join_flows(&mut self, states: &[ProofFlowState]) -> ProofFlowState {
        // Close L0 contradiction before any structural intersection. An
        // unreachable predecessor is neutral: it cannot erase a live affine
        // value, published invariant name, or canonical fact. Keeping all
        // promoted L0 states in `join_at` still records their contradiction
        // proofs as join parents. When every predecessor is contradictory,
        // the affine component is deliberately empty because L0 proves every
        // downstream goal.
        let mut promoted = states.to_vec();
        for state in &mut promoted {
            self.vocabulary.promote_flow_contradiction(state);
        }
        let contributing = promoted
            .iter()
            .filter(|state| !state.facts.all_derivable)
            .cloned()
            .collect::<Vec<_>>();
        let facts = promoted
            .iter()
            .map(|states| states.facts.clone())
            .collect::<Vec<_>>();
        let event = self.vocabulary.derivations.event(FlowEventKind::Join, None);
        let entry_images = (0..self.input.entry_images.len())
            .map(|index| {
                contributing
                    .iter()
                    .filter_map(|state| state.entry_images[index])
                    .min()
            })
            .collect();
        let results = self.vocabulary.join_result_evidence(&contributing);
        let mut continuing = Vec::new();
        for state in states {
            record_continuing(&mut continuing, &state.continuing);
        }
        ProofFlowState {
            results,
            facts: join_at(
                &facts,
                &self.vocabulary.terms,
                &self.vocabulary.goals,
                &mut self.vocabulary.derivations,
                event,
            ),
            entry_images,
            separations: SeparationLedger::intersection(
                contributing.iter().map(|state| &state.separations),
            ),
            affine: self.join_affine_states(&contributing),
            written: contributing
                .iter()
                .flat_map(|state| state.written.iter().copied())
                .collect(),
            continuing,
        }
    }
}

/// Returns the one deterministic premise traversal used by every affine
/// consumer: insertion order with later occurrences of the same canonical
/// inequality removed, regardless of evidence category.
pub(super) fn canonical_affine_facts(facts: &[ActiveAffineFact]) -> Vec<&ActiveAffineFact> {
    let mut seen = HashSet::new();
    facts
        .iter()
        .filter(|fact| seen.insert(fact.inequality.clone()))
        .collect()
}

pub(super) fn affine_facts(state: &AffineFlowState) -> Vec<ActiveAffineFact> {
    canonical_affine_facts(&state.facts)
        .into_iter()
        .cloned()
        .collect()
}

pub(super) fn affine_fact_uses_only_outer_values(
    inequality: &AffineInequality,
    state: &AffineFlowState,
    binder: BindingId,
) -> bool {
    let mut live_terms = state
        .values
        .iter()
        .filter(|(binding, _)| **binding != binder)
        .flat_map(|(_, value)| value.terms().iter().map(|coefficient| coefficient.term()))
        .collect::<HashSet<_>>();
    // [MSR-1, MSR-4] a measure of a place live at the continuation is an
    // outer value exactly as an integer binding is: it has one atom, that
    // atom is retargeted only by the events that kill the term [MSR-2],
    // and a conclusion over it therefore says the same thing after the
    // loop that it said inside. Without this every filling loop's exit
    // exports nothing and the `ensures` its body was written for is
    // unproved at the return.
    live_terms.extend(
        state
            .measure_atoms
            .borrow()
            .values()
            .flat_map(|value| value.terms().iter().map(|coefficient| coefficient.term())),
    );
    inequality
        .terms()
        .iter()
        .all(|coefficient| live_terms.contains(&coefficient.term()))
}

pub(super) fn affine_less_equal(left: &AffineForm, right: &AffineForm) -> Option<AffineInequality> {
    AffineInequality::from_forms(left, right, &mut AffineCheckState::new()).ok()
}
