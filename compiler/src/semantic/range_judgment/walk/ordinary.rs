//! Deferred ordinary records, proved before the operation or assumption they authorize.
use super::super::super::model::{CheckedLoopInvariant, CheckedProofUse, CheckedProofUseSource};
use super::*;

/// One written instance: the fact and the terms it is instantiated at.
type WrittenInstance = (FactId, Vec<Linear>, Vec<Linear>);
type Written = Vec<WrittenInstance>;

impl Walker<'_> {
    /// Project a formal over the already evaluated actual value, without
    /// substituting caller binding identities into the callee's namespace.
    pub(super) fn goal_parameter(
        &mut self,
        state: &mut State,
        function: &CheckedFunction,
        values: &[Value],
        ordinal: usize,
        projections: &[super::super::super::goal::GoalProjection],
        ty: CheckedType,
    ) -> Option<Value> {
        use super::super::super::goal::GoalProjection;
        use super::super::super::places::CapturedTerm;
        let parameter = function.parameters.get(ordinal)?;
        let mut value = values.get(ordinal)?.clone();
        let mut selected_type = parameter.ty;
        let mut range_referent = parameter.mode.is_range();
        // Only the formal reference's first dereference selects the actual
        // referent. Later dereferences select Box contents below that place.
        let projections = if parameter.mode.is_reference() {
            if let Some(rest) = projections.strip_prefix(&[GoalProjection::Deref]) {
                value = match value {
                    Value::Ref(View::Scalar(binding)) => state.values.get(&binding)?.clone(),
                    Value::Ref(View::Place(location)) => Value::Owned(state.resolve(&location)),
                    value @ Value::Ref(View::Run { .. } | View::Element { .. }) => value,
                    _ => return None,
                };
                rest
            } else {
                projections
            }
        } else {
            projections
        };
        for projection in projections {
            let index = match projection {
                GoalProjection::FormalSubscript { ordinal } => {
                    match values.get(*ordinal as usize)? {
                        Value::Int(value) => Some(value.clone()),
                        _ => return None,
                    }
                }
                GoalProjection::Subscript(index) => Some(match index.term {
                    CapturedTerm::Literal(value) => Linear::constant(i128::from(value)),
                    CapturedTerm::Binding(binding) => {
                        let ordinal = function
                            .parameters
                            .iter()
                            .position(|parameter| parameter.binding == binding)?;
                        let Value::Int(value) = values.get(ordinal)? else {
                            return None;
                        };
                        value.clone()
                    }
                    CapturedTerm::Const(declaration) => {
                        let value = &self
                            .constants
                            .iter()
                            .find(|item| item.declaration == declaration)?
                            .value;
                        let Value::Int(value) = constant(value) else {
                            return None;
                        };
                        value
                    }
                    CapturedTerm::Superseded(_) | CapturedTerm::Opaque => return None,
                }),
                _ => None,
            };
            if let Some(index) = index {
                value = self.goal_subscript(state, value, selected_type, index)?;
                selected_type = if range_referent {
                    range_referent = false;
                    selected_type
                } else {
                    match selected_type {
                        CheckedType::Segments { element } => CheckedType::Buffer { element },
                        CheckedType::Array { element, .. }
                        | CheckedType::Buffer { element }
                        | CheckedType::Window { element, .. } => {
                            *self.elements.get(element.0 as usize)?
                        }
                        _ => return None,
                    }
                };
                continue;
            }
            let (step, projected) = match (projection, selected_type) {
                (GoalProjection::Deref, CheckedType::Nominal(nominal)) => {
                    let CheckedNominalKind::Box { referent, .. } =
                        self.nominals.get(nominal.0 as usize)?.kind
                    else {
                        return None;
                    };
                    (Step::BoxContent, referent)
                }
                (GoalProjection::Field(field), CheckedType::Nominal(nominal)) => {
                    let CheckedNominalKind::Struct { fields } =
                        &self.nominals.get(nominal.0 as usize)?.kind
                    else {
                        return None;
                    };
                    (Step::Field(*field), fields.get(*field as usize)?.ty)
                }
                (GoalProjection::Payload { variant, field }, CheckedType::Nominal(nominal)) => {
                    let CheckedNominalKind::Enum { variants } =
                        &self.nominals.get(nominal.0 as usize)?.kind
                    else {
                        return None;
                    };
                    let ty = variants
                        .iter()
                        .find(|item| item.tag == *variant)?
                        .fields
                        .get(*field as usize)?
                        .ty;
                    (
                        Step::Payload {
                            variant: *variant,
                            field: *field,
                            variants: variants.len() as u32,
                        },
                        ty,
                    )
                }
                _ => return None,
            };
            selected_type = projected;
            value = match (value, step) {
                (
                    Value::Ref(View::Element {
                        container,
                        indices,
                        projection: Some(mut projection),
                    }),
                    step,
                ) => {
                    projection.push(match step {
                        Step::Field(field) => CheckedRangeProjection::Field(field),
                        Step::BoxContent => CheckedRangeProjection::BoxContent,
                        Step::Payload {
                            variant,
                            field,
                            variants,
                        } => CheckedRangeProjection::Payload {
                            variant,
                            field,
                            variants,
                        },
                        _ => return None,
                    });
                    Value::Ref(View::Element {
                        container,
                        indices,
                        projection: Some(projection),
                    })
                }
                (Value::Owned(location), step) => Value::Owned(location.child(step)),
                (Value::Struct(fields), Step::Field(field)) => fields.get(field as usize)?.clone(),
                (
                    Value::Variant {
                        variant, fields, ..
                    },
                    Step::Payload {
                        variant: selected,
                        field,
                        ..
                    },
                ) if variant == selected => fields.get(field as usize)?.clone(),
                _ => return None,
            };
        }
        match (value, ty) {
            (Value::Owned(location), CheckedType::Integer(_)) => {
                Some(self.read_location(state, &location, ty))
            }
            (
                Value::Ref(View::Element {
                    container,
                    indices,
                    projection: Some(projection),
                }),
                CheckedType::Integer(ty),
            ) => {
                let version = state.version(&mut self.world, container);
                Some(Value::Int(self.world.read(
                    version,
                    indices,
                    projection,
                    Some(ty),
                )))
            }
            (value, _) => Some(value),
        }
    }

    /// Both template encodings of a subscript select the same current element.
    pub(super) fn goal_subscript(
        &mut self,
        state: &mut State,
        value: Value,
        ty: CheckedType,
        index: Linear,
    ) -> Option<Value> {
        Some(match value {
            Value::Ref(View::Run {
                container,
                mut prefix,
                offset,
                ..
            }) => {
                prefix.push(offset.plus(&index)?);
                Value::Ref(View::Element {
                    container,
                    indices: prefix,
                    projection: Some(Vec::new()),
                })
            }
            Value::Ref(View::Element {
                container,
                mut indices,
                projection: Some(mut projection),
            }) => {
                projection.push(CheckedRangeProjection::Index(indices.len() as u32));
                indices.push(index);
                Value::Ref(View::Element {
                    container,
                    indices,
                    projection: Some(projection),
                })
            }
            Value::Owned(location) | Value::Ref(View::Place(location)) => {
                let segments = matches!(ty, CheckedType::Segments { .. });
                let location = state.resolve(&location);
                let container =
                    state.container(&mut self.world, location, if segments { 2 } else { 1 })?;
                if segments {
                    let length = self.world.segment_length(
                        container,
                        state.generation(container),
                        index.clone(),
                    );
                    Value::Ref(View::Run {
                        container,
                        prefix: vec![index],
                        offset: Linear::constant(0),
                        length,
                    })
                } else {
                    Value::Ref(View::Element {
                        container,
                        indices: vec![index],
                        projection: Some(Vec::new()),
                    })
                }
            }
            _ => return None,
        })
    }

    /// RANGE-2 has already selected comparisons and their conjunctions.
    /// Failure to reconstruct one here is an unrepresentable site, not an
    /// unsupported source goal shape.
    pub(super) fn goal_comparisons_with(
        &mut self,
        state: &mut State,
        expression: &super::super::super::goal::GoalExpression,
        function: &CheckedFunction,
        values: &[Value],
    ) -> Option<Vec<Literal>> {
        use super::super::super::goal::{GoalExpression, GoalOperation};
        match expression {
            GoalExpression::Operation {
                row: GoalOperation::Boolean(CheckedBooleanOperation::And),
                arguments,
                ..
            } if arguments.len() == 2 => {
                let mut goals = Vec::new();
                for argument in arguments {
                    goals.extend(self.goal_comparisons_with(state, argument, function, values)?);
                }
                Some(goals)
            }
            _ => self
                .goal_literal_with(state, expression, function, values)
                .map(|goal| vec![goal]),
        }
    }

    pub(super) fn has_ordinary(&self, site: &NodePath, subject: &ObligationSubject) -> bool {
        self.dry == 0
            && self.deferred.iter().any(|(index, _)| {
                let record = &self.function.obligations[*index];
                record.site == *site && record.subject == *subject
            })
    }

    /// Each comparison is one conclusion of the ordinary clause.
    pub(super) fn ordinary_goal(
        &mut self,
        state: &State,
        site: &NodePath,
        subject: &ObligationSubject,
        goals: Option<&[Literal]>,
        written: Option<&[WrittenInstance]>,
    ) {
        if !self.has_ordinary(site, subject) {
            return;
        }
        let counterexamples =
            goals.map(|goals| goals.iter().map(|goal| vec![negated(goal)]).collect());
        self.ordinary_counterexamples(state, site, subject, counterexamples, written);
    }

    pub(super) fn ordinary_counterexamples(
        &mut self,
        state: &State,
        site: &NodePath,
        subject: &ObligationSubject,
        counterexamples: Option<Vec<Vec<Literal>>>,
        written: Option<&[WrittenInstance]>,
    ) {
        if !self.has_ordinary(site, subject) {
            return;
        }
        let proved = match (counterexamples, written) {
            (Some(counterexamples), Some(written)) => {
                let (units, mut choices) = state.premises(&self.world);
                choices.push(counterexamples);
                let query = Query {
                    units,
                    choices,
                    ..Query::default()
                };
                match facts::judge(&mut self.world, &self.facts, &state.facts, written, query) {
                    Ok(Verdict::Refuted) => DeferredAnswer::Proved,
                    Ok(Verdict::Open) if self.imprecise.is_some() => DeferredAnswer::Inconclusive,
                    Ok(Verdict::Open) => DeferredAnswer::Unproved,
                    Err(Capacity::Arithmetic) => {
                        self.issues.push(RangeIssue::Unsupported {
                            node: site.clone(),
                            feature: UnsupportedSemanticFeature::RangeArithmetic,
                        });
                        DeferredAnswer::Inconclusive
                    }
                    Err(capacity) => {
                        self.issues.push(RangeIssue::Undischarged {
                            node: site.clone(),
                            fact: "the deferred ordinary obligation".to_owned(),
                            site: "an ordinary obligation",
                            relation: None,
                            capacity: Some(capacity.describe()),
                        });
                        DeferredAnswer::Inconclusive
                    }
                }
            }
            _ => {
                self.issues.push(RangeIssue::Unsupported {
                    node: site.clone(),
                    feature: UnsupportedSemanticFeature::RangeOrdinaryGoal,
                });
                DeferredAnswer::Inconclusive
            }
        };
        for (index, answer) in &mut self.deferred {
            let record = &self.function.obligations[*index];
            if record.site == *site && record.subject == *subject {
                *answer = match (*answer, proved) {
                    // A definite failure on any incoming edge remains a
                    // rejection even if another edge cannot be judged.
                    (DeferredAnswer::Unproved, _) | (_, DeferredAnswer::Unproved) => {
                        DeferredAnswer::Unproved
                    }
                    (DeferredAnswer::Inconclusive, _) | (_, DeferredAnswer::Inconclusive) => {
                        DeferredAnswer::Inconclusive
                    }
                    _ => proved,
                };
            }
        }
    }

    pub(super) fn ordinary_bound(
        &mut self,
        state: &State,
        site: &NodePath,
        index: &Linear,
        length: Option<Linear>,
    ) {
        let subject = ObligationSubject::Source {
            family: ObligationFamily::Bounds,
            conjunct: 0,
        };
        if !self.has_ordinary(site, &subject) {
            return;
        }
        let goals = length.map(|length| vec![literal(index.clone(), Relation::Less, length)]);
        self.ordinary_goal(state, site, &subject, goals.as_deref(), Some(&[]));
    }

    pub(super) fn ordinary_domain(
        &mut self,
        state: &State,
        site: &NodePath,
        family: ObligationFamily,
        value: &Value,
        ty: CheckedType,
    ) {
        let subject = ObligationSubject::Source {
            family,
            conjunct: 0,
        };
        if !self.has_ordinary(site, &subject) {
            return;
        }
        let goals = match (value, ty) {
            (Value::Int(value), CheckedType::Integer(ty)) => {
                let (low, high) = integer_range(ty);
                Some(vec![
                    literal(value.clone(), Relation::GreaterEqual, Linear::constant(low)),
                    literal(value.clone(), Relation::LessEqual, Linear::constant(high)),
                ])
            }
            _ => None,
        };
        self.ordinary_goal(state, site, &subject, goals.as_deref(), Some(&[]));
    }

    pub(super) fn target_length(
        &mut self,
        state: &mut State,
        target: &Target,
        ty: CheckedType,
    ) -> Option<Linear> {
        self.target_measure(state, target, ty, CheckedMeasure::Length)
    }

    pub(super) fn target_measure(
        &mut self,
        state: &mut State,
        target: &Target,
        ty: CheckedType,
        measure: CheckedMeasure,
    ) -> Option<Linear> {
        let observed = match target {
            Target::Run { length, .. } if measure == CheckedMeasure::Length => Some(length.clone()),
            Target::Location(location) => {
                let location = state.resolve(location);
                let arity = if matches!(ty, CheckedType::Segments { .. }) {
                    2
                } else {
                    1
                };
                let container = state.container(&mut self.world, location, arity)?;
                Some(
                    self.world
                        .measure(container, state.generation(container), measure),
                )
            }
            Target::Row { container, row } if measure == CheckedMeasure::Length => Some(
                self.world
                    .segment_length(*container, state.generation(*container), row.clone()),
            ),
            Target::Element {
                container,
                indices,
                projection: Some(projection),
            } => {
                let mut projection = projection.clone();
                projection.push(CheckedRangeProjection::Measure(measure));
                let version = state.version(&mut self.world, *container);
                Some(
                    self.world
                        .read(version, indices.clone(), projection, Some(IntegerType::U64)),
                )
            }
            _ => None,
        };
        if let CheckedType::Array { length, .. } = ty
            && measure == CheckedMeasure::Length
        {
            let fixed = Linear::constant(i128::from(length.value()?));
            if let Some(observed) = observed {
                state
                    .conds
                    .push(literal(observed, Relation::Equal, fixed.clone()));
            }
            Some(fixed)
        } else {
            observed
        }
    }

    pub(super) fn ordinary_invariants(
        &mut self,
        state: &mut State,
        invariants: &[CheckedLoopInvariant],
    ) {
        if self.dry != 0 || self.deferred.is_empty() {
            return;
        }
        for invariant in invariants {
            let subject = self.deferred.iter().find_map(|(index, _)| {
                let record = &self.function.obligations[*index];
                (record.site == invariant.relation.node_path
                    && matches!(record.subject, ObligationSubject::LoopInvariant { .. }))
                .then(|| record.subject.clone())
            });
            if let Some(subject) = subject {
                let goals = self.affine_relation(state, &invariant.relation);
                self.ordinary_goal(
                    state,
                    &invariant.relation.node_path,
                    &subject,
                    goals.as_deref(),
                    Some(&[]),
                );
            }
        }
    }

    pub(super) fn assume_affine(&mut self, state: &mut State, invariants: &[CheckedLoopInvariant]) {
        for invariant in invariants {
            if let Some(goals) = self.affine_relation(state, &invariant.relation) {
                state.conds.extend(goals);
            }
        }
    }

    pub(super) fn written_instances(
        &mut self,
        state: &State,
        uses: &[CheckedProofUse],
    ) -> Option<Written> {
        let mut written = Vec::new();
        for step in uses {
            let CheckedProofUseSource::Range(step) = &step.source else {
                continue;
            };
            let fact = state
                .facts
                .iter()
                .copied()
                .find(|fact| self.facts[*fact as usize].clause.declaration == step.fact)?;
            let frame = self.facts[fact as usize].frame.clone();
            let arguments = step
                .arguments
                .iter()
                .map(|argument| self.certificate_term(state, &frame, argument, &[]))
                .collect::<Option<Vec<_>>>()?;
            written.push((fact, arguments, Vec::new()));
        }
        Some(written)
    }
}

/// Boolean operators keep comparisons intact through arbitrary nesting.
pub(super) fn boolean(operation: CheckedBooleanOperation, parts: Vec<Cond>) -> Option<Cond> {
    match (operation, parts.as_slice()) {
        (CheckedBooleanOperation::And, [_, _]) => Some(Cond::And(parts)),
        (CheckedBooleanOperation::Or, [_, _]) => Some(Cond::Or(parts)),
        (CheckedBooleanOperation::Not, [part]) => Some(Cond::Not(Box::new(part.clone()))),
        (CheckedBooleanOperation::ExclusiveOr, [left, right]) => Some(Cond::Or(vec![
            Cond::And(vec![left.clone(), Cond::Not(Box::new(right.clone()))]),
            Cond::And(vec![Cond::Not(Box::new(left.clone())), right.clone()]),
        ])),
        _ => None,
    }
}

/// Counterexamples to the fixed OP-2 domains whose results RANGE-2 leaves
/// opaque. These domain rows do not introduce written Boolean goals.
pub(super) fn integer_domain(
    operation: CheckedIntegerOperation,
    ty: IntegerType,
    values: &[Linear],
) -> Option<Vec<Vec<Literal>>> {
    let equals =
        |value: &Linear, bound| literal(value.clone(), Relation::Equal, Linear::constant(bound));
    match (operation, values) {
        (
            CheckedIntegerOperation::DivideExact | CheckedIntegerOperation::RemainderExact,
            [dividend, divisor],
        ) => {
            let mut failures = vec![vec![equals(divisor, 0)]];
            if ty.signed() {
                failures.push(vec![
                    equals(dividend, integer_range(ty).0),
                    equals(divisor, -1),
                ]);
            }
            Some(failures)
        }
        (
            CheckedIntegerOperation::NegateExact | CheckedIntegerOperation::AbsoluteExact,
            [value],
        ) => Some(vec![vec![equals(value, integer_range(ty).0)]]),
        (
            CheckedIntegerOperation::ShiftLeftExact | CheckedIntegerOperation::ShiftRightExact,
            [_, amount],
        ) => Some(vec![vec![literal(
            amount.clone(),
            Relation::GreaterEqual,
            Linear::constant(i128::from(ty.width())),
        )]]),
        _ => None,
    }
}
