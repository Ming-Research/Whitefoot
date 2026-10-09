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
        let mut value = values.get(ordinal)?.clone();
        let mut selected_type = Some(function.parameters.get(ordinal)?.ty);
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
                value = match value {
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
                    Value::Owned(location) => {
                        let segments = matches!(selected_type, Some(CheckedType::Segments { .. }));
                        let container = state.container(
                            &mut self.world,
                            location,
                            if segments { 2 } else { 1 },
                        )?;
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
                };
                selected_type = None;
                continue;
            }
            if matches!(projection, GoalProjection::Field(_)) {
                selected_type = None;
            }
            value = match (value, projection) {
                (Value::Ref(View::Place(location)), GoalProjection::Deref) => {
                    Value::Owned(state.resolve(&location))
                }
                (
                    value @ Value::Ref(View::Run { .. } | View::Element { .. }),
                    GoalProjection::Deref,
                ) => value,
                (
                    Value::Ref(View::Element {
                        container,
                        indices,
                        projection: Some(mut projection),
                    }),
                    GoalProjection::Field(field),
                ) => {
                    projection.push(CheckedRangeProjection::Field(*field));
                    Value::Ref(View::Element {
                        container,
                        indices,
                        projection: Some(projection),
                    })
                }
                (Value::Owned(location), GoalProjection::Field(field)) => {
                    Value::Owned(location.child(Step::Field(*field)))
                }
                (Value::Struct(fields), GoalProjection::Field(field)) => {
                    fields.get(*field as usize)?.clone()
                }
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

    pub(super) fn goal_condition_with(
        &mut self,
        state: &mut State,
        expression: &super::super::super::goal::GoalExpression,
        function: &CheckedFunction,
        values: &[Value],
    ) -> Option<Cond> {
        use super::super::super::goal::{GoalDatum, GoalExpression, GoalOperation};
        match expression {
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Bool(value))) => {
                Some(Cond::Constant(*value))
            }
            GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ty,
            }) => {
                match self.goal_parameter(
                    state,
                    function,
                    values,
                    *ordinal as usize,
                    projections,
                    *ty,
                )? {
                    Value::Bool(condition) => Some(condition),
                    _ => None,
                }
            }
            GoalExpression::Operation {
                row: GoalOperation::Boolean(operation),
                arguments,
                ..
            } => {
                let parts = arguments
                    .iter()
                    .map(|argument| self.goal_condition_with(state, argument, function, values))
                    .collect::<Option<Vec<_>>>()?;
                boolean(*operation, parts)
            }
            _ => self
                .goal_literal_with(state, expression, function, values)
                .map(Cond::Literal),
        }
    }

    pub(super) fn has_ordinary(&self, site: &NodePath, subject: &ObligationSubject) -> bool {
        self.dry == 0
            && self.deferred.iter().any(|(index, _)| {
                let record = &self.function.obligations[*index];
                record.site == *site && record.subject == *subject
            })
    }

    /// None means a site was never visited; false sticks across all incoming edges.
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
        let goal = goals.map(|goals| Cond::And(goals.iter().cloned().map(Cond::Literal).collect()));
        self.ordinary_condition(state, site, subject, goal.as_ref(), written);
    }

    pub(super) fn ordinary_condition(
        &mut self,
        state: &State,
        site: &NodePath,
        subject: &ObligationSubject,
        goal: Option<&Cond>,
        written: Option<&[WrittenInstance]>,
    ) {
        if !self.has_ordinary(site, subject) {
            return;
        }
        let negation = goal.and_then(|goal| alternatives(goal, false));
        let proved = match (negation, written) {
            (Some(negation), Some(written)) => {
                let (units, mut choices) = state.premises(&self.world);
                choices.push(negation);
                let query = Query {
                    units,
                    choices,
                    ..Query::default()
                };
                match facts::judge(&mut self.world, &self.facts, &state.facts, written, query) {
                    Ok(Verdict::Refuted) => true,
                    Ok(Verdict::Open) => false,
                    Err(Capacity::Arithmetic) => {
                        self.issues.push(RangeIssue::Unsupported {
                            node: site.clone(),
                            feature: UnsupportedSemanticFeature::RangeArithmetic,
                        });
                        false
                    }
                    Err(capacity) => {
                        self.issues.push(RangeIssue::Undischarged {
                            node: site.clone(),
                            fact: "the deferred ordinary obligation".to_owned(),
                            site: "an ordinary obligation",
                            relation: None,
                            capacity: Some(capacity.describe()),
                        });
                        false
                    }
                }
            }
            _ => {
                self.issues.push(RangeIssue::Unsupported {
                    node: site.clone(),
                    feature: UnsupportedSemanticFeature::RangeOrdinaryGoal,
                });
                false
            }
        };
        for (index, answer) in &mut self.deferred {
            let record = &self.function.obligations[*index];
            if record.site == *site && record.subject == *subject {
                *answer = Some(answer.unwrap_or(true) && proved);
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

/// Alternatives of conjunctions, preserving the complete Boolean formula.
/// In particular, negating a disjunction conjoins each operand's negation.
fn alternatives(condition: &Cond, truth: bool) -> Option<Vec<Vec<Literal>>> {
    match condition {
        Cond::Literal(literal) => Some(vec![vec![if truth {
            literal.clone()
        } else {
            negated(literal)
        }]]),
        Cond::Constant(value) => Some(if *value == truth {
            vec![Vec::new()]
        } else {
            Vec::new()
        }),
        Cond::Not(inner) => alternatives(inner, !truth),
        Cond::And(parts) | Cond::Or(parts) => {
            let conjunction = matches!(condition, Cond::And(_)) == truth;
            let mut result = if conjunction {
                vec![Vec::new()]
            } else {
                Vec::new()
            };
            for part in parts {
                let next = alternatives(part, truth)?;
                if conjunction {
                    result = result
                        .into_iter()
                        .flat_map(|prefix| {
                            next.iter().map(move |suffix| {
                                let mut row = prefix.clone();
                                row.extend(suffix.iter().cloned());
                                row
                            })
                        })
                        .collect();
                } else {
                    result.extend(next);
                }
            }
            Some(result)
        }
        Cond::Unknown => None,
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

/// OP-2 domains whose results RANGE-2 intentionally leaves opaque.
pub(super) fn integer_domain(
    operation: CheckedIntegerOperation,
    ty: IntegerType,
    values: &[Linear],
) -> Option<Cond> {
    let differs = |value: &Linear, bound| {
        Cond::Literal(literal(
            value.clone(),
            Relation::NotEqual,
            Linear::constant(bound),
        ))
    };
    match (operation, values) {
        (
            CheckedIntegerOperation::DivideExact | CheckedIntegerOperation::RemainderExact,
            [dividend, divisor],
        ) => {
            let nonzero = differs(divisor, 0);
            Some(if ty.signed() {
                Cond::And(vec![
                    nonzero,
                    Cond::Or(vec![
                        differs(dividend, integer_range(ty).0),
                        differs(divisor, -1),
                    ]),
                ])
            } else {
                nonzero
            })
        }
        (
            CheckedIntegerOperation::NegateExact | CheckedIntegerOperation::AbsoluteExact,
            [value],
        ) => Some(differs(value, integer_range(ty).0)),
        (
            CheckedIntegerOperation::ShiftLeftExact | CheckedIntegerOperation::ShiftRightExact,
            [_, amount],
        ) => Some(Cond::Literal(literal(
            amount.clone(),
            Relation::Less,
            Linear::constant(i128::from(ty.width())),
        ))),
        _ => None,
    }
}
