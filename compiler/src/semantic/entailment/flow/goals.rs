//! Goals formed from checked expressions: canonical goal expressions,
//! their origins and interning, signed Boolean decompositions and the
//! integer-domain goals of exact operations.

use super::*;

impl Input<'_, '_> {
    /// Converts one source expression to ENT-3's exact direct pure/total
    /// origin. Any excluded child excludes the whole expression.
    pub(super) fn direct_goal_expression(
        &self,
        expression: &CheckedExpression,
    ) -> Option<GoalExpression> {
        self.goal_expression(expression, false)
    }

    /// Converts a value whose nested obligations have already been discharged
    /// to its exact stable proof expression. In addition to the pure/total
    /// direct subset, this admits an exact integer result or indexed element
    /// only after `judge_expression` has checked that nested partial operation.
    pub(super) fn admitted_value_goal_expression(
        &self,
        expression: &CheckedExpression,
    ) -> Option<GoalExpression> {
        self.goal_expression(expression, true)
    }

    pub(super) fn goal_expression(
        &self,
        expression: &CheckedExpression,
        admitted_partial: bool,
    ) -> Option<GoalExpression> {
        if let Some(value) = self.constant_storage_scalar(expression) {
            return Some(GoalExpression::Datum(GoalDatum::Literal(value.clone())));
        }
        // A non-consuming place read is admitted by its final copy value, not
        // by the mode of every holder traversed on the way there. In
        // particular, reading through an owning box must retain the box's
        // explicit Deref projection even though the box binding itself is
        // affine and cannot be a standalone goal datum.
        if self.is_copy(expression.ty())
            && let Some(path) = self.read_place_path(expression)
            && let PlaceRoot::Binding(root) = path.root
        {
            return Some(GoalExpression::Datum(GoalDatum::Place {
                root,
                projections: path
                    .path
                    .iter()
                    .map(goal_projection_of_step)
                    .collect::<Option<Vec<_>>>()?,
                ty: expression.ty(),
            }));
        }
        let build_operation = |row, type_arguments, const_arguments, result, arguments: Vec<_>| {
            Some(GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            })
        };
        match expression {
            CheckedExpression::Constant(value) => {
                Some(GoalExpression::Datum(GoalDatum::Literal(value.clone())))
            }
            CheckedExpression::NamedConstant { declaration, value } => {
                Some(GoalExpression::Datum(GoalDatum::NamedConst {
                    declaration: *declaration,
                    projections: Vec::new(),
                    ty: value.ty(),
                }))
            }
            CheckedExpression::Binding { binding, ty, .. } if self.is_copy(*ty) => {
                Some(GoalExpression::Datum(GoalDatum::Place {
                    root: *binding,
                    projections: Vec::new(),
                    ty: *ty,
                }))
            }
            CheckedExpression::Project {
                binding,
                fields,
                ty,
                consume_root: false,
                ..
            } if self.is_copy(*ty) => Some(GoalExpression::Datum(GoalDatum::Place {
                root: *binding,
                projections: fields.iter().copied().map(GoalProjection::Field).collect(),
                ty: *ty,
            })),
            CheckedExpression::DerefAddressed { binding, ty, .. } if self.is_copy(*ty) => {
                Some(GoalExpression::Datum(GoalDatum::Place {
                    root: *binding,
                    projections: Vec::new(),
                    ty: *ty,
                }))
            }
            CheckedExpression::BoxDeref {
                referent, value, ..
            } if self.is_copy(*referent) => self
                .goal_expression(value, admitted_partial)?
                .with_projection(GoalProjection::Deref, *referent),
            CheckedExpression::ProjectValue {
                value, field, ty, ..
            } if self.is_copy(*ty) => self
                .goal_expression(value, admitted_partial)?
                .with_projection(GoalProjection::Field(*field), *ty),
            CheckedExpression::IntegerOperation {
                operation,
                operand_type,
                arguments,
                result,
                ..
            } if !operation.is_exact() || admitted_partial => build_operation(
                GoalOperation::Integer {
                    operation: *operation,
                    operand_type: *operand_type,
                },
                Vec::new(),
                Vec::new(),
                *result,
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::IntegerOperation { .. } => None,
            CheckedExpression::FloatOperation {
                operation: row,
                operand_type,
                arguments,
                ..
            } => build_operation(
                GoalOperation::Float {
                    operation: *row,
                    operand_type: *operand_type,
                },
                if matches!(
                    row,
                    CheckedFloatOperation::Infinity | CheckedFloatOperation::Nan
                ) {
                    vec![*operand_type]
                } else {
                    Vec::new()
                },
                Vec::new(),
                row.result_type(*operand_type),
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::NumericConversion {
                mode,
                source,
                destination,
                value,
                result,
                ..
            } => build_operation(
                GoalOperation::NumericConversion {
                    mode: *mode,
                    source: *source,
                    destination: *destination,
                },
                vec![source.ty(), destination.ty()],
                Vec::new(),
                *result,
                vec![self.goal_expression(value, admitted_partial)?],
            ),
            CheckedExpression::Reinterpret {
                source,
                destination,
                value,
                ..
            } => build_operation(
                GoalOperation::Reinterpret {
                    source: *source,
                    destination: *destination,
                },
                vec![source.ty(), destination.ty()],
                Vec::new(),
                destination.ty(),
                vec![self.goal_expression(value, admitted_partial)?],
            ),
            CheckedExpression::BooleanOperation {
                operation: row,
                arguments,
                ..
            } => build_operation(
                GoalOperation::Boolean(*row),
                Vec::new(),
                Vec::new(),
                CheckedType::Bool,
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::EnumEquality {
                equal,
                operand_type,
                arguments,
                ..
            } => build_operation(
                GoalOperation::EnumEquality {
                    equal: *equal,
                    operand_type: *operand_type,
                },
                Vec::new(),
                Vec::new(),
                CheckedType::Bool,
                arguments
                    .iter()
                    .map(|argument| self.goal_expression(argument, admitted_partial))
                    .collect::<Option<Vec<_>>>()?,
            ),
            CheckedExpression::ArrayMeasure {
                measure,
                root,
                length,
            } => {
                let argument = self.goal_array_root(root)?;
                let CheckedType::Array { element, .. } = argument.ty() else {
                    return None;
                };
                build_operation(
                    GoalOperation::ArrayMeasure {
                        measure: *measure,
                        element,
                        length: *length,
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::ArrayIndex {
                root,
                element_type,
                length,
                offset,
                ..
            } if admitted_partial => {
                let collection = self.goal_array_root(root)?;
                let CheckedType::Array {
                    element,
                    length: root_length,
                } = collection.ty()
                else {
                    return None;
                };
                if root_length != *length
                    || self.context.elements.get(element.index()) != Some(element_type)
                {
                    return None;
                }
                build_operation(
                    GoalOperation::ArrayIndex {
                        element,
                        length: *length,
                    },
                    Vec::new(),
                    Vec::new(),
                    *element_type,
                    vec![collection, self.goal_expression(offset, admitted_partial)?],
                )
            }
            // [MSR-1] a measure of a storage shape, read as the same
            // quantity the reader row loads. [ENT-2] a place whose offsets
            // are not all captured terms or constants names no one element,
            // so it has no goal identity either.
            CheckedExpression::ContainerMeasure { measure, root }
                if root.subscripted_term() == Some(SubscriptedTerm::Represented) =>
            {
                let measured = root.measured()?;
                let argument = self.goal_container_place(root)?;
                build_operation(
                    GoalOperation::ContainerMeasure {
                        measure: *measure,
                        measured,
                        element: root.element(),
                        constant: root.type_constant(),
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::ReadStorage { root, .. } if admitted_partial => {
                let Some((CheckedPlaceStep::Subscript(index), prefix)) = root.path.split_last()
                else {
                    if root
                        .place_path()
                        .contains(&PlaceStep::Index(CapturedValue::unknown()))
                    {
                        return None;
                    }
                    return self.goal_container_place(root);
                };
                let base = CheckedContainerRoot {
                    root: root.root,
                    path: prefix.to_vec(),
                    ty: index.base_type,
                };
                let row = match base.ty {
                    CheckedType::Array { element, length } => {
                        GoalOperation::ArrayIndex { element, length }
                    }
                    _ => GoalOperation::RunIndex {
                        measured: base.measured()?,
                        element: base.element()?,
                        constant: base.type_constant(),
                    },
                };
                let collection = self.goal_container_place(&base)?;
                build_operation(
                    row,
                    Vec::new(),
                    Vec::new(),
                    root.ty,
                    vec![
                        collection,
                        self.goal_expression(&index.offset, admitted_partial)?,
                    ],
                )
            }
            CheckedExpression::BufferMeasure { measure, root }
                if root.subscripted_term() == Some(SubscriptedTerm::Represented) =>
            {
                let argument = goal_binding_place(
                    root.binding,
                    root.path.iter().map(CheckedPlaceStep::goal_projection),
                    CheckedType::Buffer {
                        element: root.element,
                    },
                );
                build_operation(
                    GoalOperation::BufferMeasure {
                        measure: *measure,
                        element: root.element,
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            // [MSR-1, REF-4] the one measure a range reference has.
            CheckedExpression::RangeMeasure { measure, root } => {
                let argument =
                    goal_binding_place(root.binding, Vec::new(), root.element_type);
                build_operation(
                    // Clause formation uses the ordinary measured-place
                    // row. A body read must have that same structural goal
                    // identity, including after let-origin expansion.
                    GoalOperation::ContainerMeasure {
                        measure: *measure,
                        measured: MeasuredKind::Range,
                        element: Some(root.element),
                        constant: None,
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. }
                if place.subscripted_term() == Some(SubscriptedTerm::Represented) =>
            {
                let measured = place.measured()?;
                let argument = goal_binding_place(
                    place.root.binding,
                    place.goal_projections(),
                    place.ty,
                );
                build_operation(
                    GoalOperation::ContainerMeasure {
                        measure: *measure,
                        measured,
                        element: place.element(),
                        constant: place.type_constant(),
                    },
                    Vec::new(),
                    Vec::new(),
                    CheckedType::Integer(IntegerType::U64),
                    vec![argument],
                )
            }
            CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. }
                if admitted_partial && place.path.is_empty() =>
            {
                let collection = goal_binding_place(
                    place.root.binding,
                    Vec::new(),
                    place.root.element_type,
                );
                build_operation(
                    GoalOperation::RunIndex {
                        measured: MeasuredKind::Range,
                        element: place.root.element,
                        constant: None,
                    },
                    Vec::new(),
                    Vec::new(),
                    place.root.element_type,
                    vec![
                        collection,
                        self.goal_expression(&place.offset, admitted_partial)?,
                    ],
                )
            }
            CheckedExpression::BufferIndex { root, offset, .. } if admitted_partial => {
                let collection_type = CheckedType::Buffer {
                    element: root.element,
                };
                let collection = goal_binding_place(
                    root.binding,
                    root.path.iter().map(CheckedPlaceStep::goal_projection),
                    collection_type,
                );
                build_operation(
                    GoalOperation::BufferIndex {
                        element: root.element,
                    },
                    Vec::new(),
                    Vec::new(),
                    root.element_type,
                    vec![collection, self.goal_expression(offset, admitted_partial)?],
                )
            }
            CheckedExpression::Binding { .. }
            | CheckedExpression::Project { .. }
            | CheckedExpression::DerefAddressed { .. }
            | CheckedExpression::BoxDeref { .. }
            // [TYPE-9, WIN-3] an unbox consumes its owner, so the value it
            // produces is no longer a place any goal datum can name.
            | CheckedExpression::BoxTake { .. }
            | CheckedExpression::ProjectValue { .. }
            | CheckedExpression::UserCall { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::RangeElementMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. }
            | CheckedExpression::ReadStorage { .. }
            | CheckedExpression::ArrayIndex { .. }
            | CheckedExpression::BufferIndex { .. }
            | CheckedExpression::RangeIndex { .. }
            | CheckedExpression::BorrowRangeIndex { .. }
            | CheckedExpression::RangeOf { .. }
            | CheckedExpression::BorrowAddressed { .. }
            | CheckedExpression::ConstructStruct { .. }
            | CheckedExpression::ConstructEnum { .. } => None,
        }
    }

    pub(super) fn goal_array_root(&self, root: &CheckedArrayRoot) -> Option<GoalExpression> {
        match root {
            CheckedArrayRoot::Binding { binding, fields } => {
                let ty = self.projected_binding_type(*binding, fields)?;
                Some(goal_binding_place(
                    *binding,
                    fields.iter().copied().map(GoalProjection::Field),
                    ty,
                ))
            }
            CheckedArrayRoot::Constant(id) => {
                let declaration = self.context.constant_declaration(*id)?;
                let ty = self.context.constants.get(id.0 as usize)?.ty;
                Some(GoalExpression::Datum(GoalDatum::NamedConst {
                    declaration,
                    projections: Vec::new(),
                    ty,
                }))
            }
        }
    }

    pub(super) fn goal_container_place(
        &self,
        root: &CheckedContainerRoot,
    ) -> Option<GoalExpression> {
        Some(match root.root {
            PlaceRoot::Binding(binding) => {
                goal_binding_place(binding, root.goal_projections(), root.ty)
            }
            PlaceRoot::Constant(id) => GoalExpression::Datum(GoalDatum::NamedConst {
                declaration: self.context.constant_declaration(id)?,
                projections: root.goal_projections(),
                ty: root.ty,
            }),
        })
    }

    pub(super) fn projected_binding_type(
        &self,
        binding: BindingId,
        fields: &[u32],
    ) -> Option<CheckedType> {
        let mut ty = self.summary(binding)?.ty?;
        for field in fields {
            let CheckedType::Nominal(nominal) = ty else {
                return None;
            };
            let CheckedNominalKind::Struct { fields } =
                &self.context.nominals.get(nominal.0 as usize)?.kind
            else {
                return None;
            };
            ty = fields.get(*field as usize)?.ty;
        }
        Some(ty)
    }

    pub(super) fn goal_projection_type(
        &self,
        input: CheckedType,
        projection: GoalProjection,
    ) -> Option<CheckedType> {
        match projection {
            GoalProjection::Deref => match input {
                CheckedType::Nominal(nominal) => {
                    match self.context.nominals.get(nominal.0 as usize)?.kind {
                        CheckedNominalKind::Box { referent, .. } => Some(referent),
                        _ => Some(input),
                    }
                }
                // Borrow holders retain the referent type in checked form.
                _ => Some(input),
            },
            GoalProjection::Field(field) => {
                let CheckedType::Nominal(nominal) = input else {
                    return None;
                };
                let CheckedNominalKind::Struct { fields } =
                    &self.context.nominals.get(nominal.0 as usize)?.kind
                else {
                    return None;
                };
                fields.get(field as usize).map(|field| field.ty)
            }
            GoalProjection::Payload { variant, field } => {
                let CheckedType::Nominal(nominal) = input else {
                    return None;
                };
                let CheckedNominalKind::Enum { variants } =
                    &self.context.nominals.get(nominal.0 as usize)?.kind
                else {
                    return None;
                };
                variants
                    .iter()
                    .find(|candidate| candidate.tag == variant)?
                    .fields
                    .get(field as usize)
                    .map(|field| field.ty)
            }
            // [OP-4] a subscript selects the base's element type, which
            // [MSR-1] admits in a measure place and [WIN-1] gives the one
            // slot a run holds.
            GoalProjection::Subscript(_) => element_type(input, self.context.elements),
            // [REF-4, TYPE-8] a range step selects the run of T elements the
            // range names, and `&[T]` is a reference kind and not a type, so
            // that run's checked image is its element type, exactly as a
            // `&[T]` parameter's is.
            GoalProjection::Range(_) => element_type(input, self.context.elements),
            // [MSR-1] the same element selection a written subscript makes;
            // the offset is what the reader substitutes, not the type.
            GoalProjection::FormalSubscript { .. } => element_type(input, self.context.elements),
        }
    }

    pub(super) fn goal_place_path(&self, datum: &GoalDatum) -> Option<ResolvedPlace> {
        let (root, projections) = match datum {
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
        Some(ResolvedPlace {
            root,
            path: projections
                .iter()
                .map(|projection| projection.place_step())
                .collect(),
        })
    }

    pub(super) fn body_requirement_goal(
        &self,
        requirement: &CheckedRequirement,
    ) -> Option<GoalExpression> {
        self.body_goal_expression(&requirement.template.root)
    }

    pub(super) fn body_goal_expression(
        &self,
        expression: &GoalExpression,
    ) -> Option<GoalExpression> {
        match expression {
            GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ty,
            }) => {
                let parameter = self.function.parameters.get(*ordinal as usize)?;
                let binding = parameter.binding;
                // [MSR-1, ENT-2] a formal-valued subscript names a value
                // parameter of this same callable, so inside the body it is
                // that parameter's own binding: `deref(rows)[i].len` written
                // in the clause and written in the body are one term because
                // their canonical spellings are byte-identical there.
                let projections = self
                    .body_projections(PlaceRoot::Binding(binding), projections)
                    .iter()
                    .map(|projection| match projection {
                        GoalProjection::FormalSubscript { ordinal } => self
                            .function
                            .parameters
                            .get(*ordinal as usize)
                            .map(|offset| {
                                GoalProjection::Subscript(
                                    CapturedValue::new(
                                        CaptureId::source(u32::MAX),
                                        CapturedTerm::Binding(offset.binding),
                                    )
                                    .goal_identity(),
                                )
                            }),
                        other => Some(*other),
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(GoalExpression::Datum(GoalDatum::Place {
                    root: binding,
                    projections,
                    ty: *ty,
                }))
            }
            GoalExpression::Datum(GoalDatum::EvaluatedValue { .. }) => None,
            GoalExpression::Datum(datum) => Some(GoalExpression::Datum(datum.clone())),
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            } => Some(GoalExpression::Operation {
                row: *row,
                type_arguments: type_arguments.clone(),
                const_arguments: const_arguments.clone(),
                result: *result,
                arguments: arguments
                    .iter()
                    .map(|argument| self.body_goal_expression(argument))
                    .collect::<Option<Vec<_>>>()?,
            }),
        }
    }

    pub(super) fn goal_integer_constant(&self, expression: &GoalExpression) -> Option<i128> {
        match expression {
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { ty, bits })) => {
                Some(integer_value(*ty, *bits))
            }
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) if projections.is_empty() => {
                let CheckedValue::Integer {
                    ty: value_type,
                    bits,
                } = &self.context.constant(*declaration)?.value
                else {
                    return None;
                };
                (*ty == CheckedType::Integer(*value_type))
                    .then(|| integer_value(*value_type, *bits))
            }
            _ => None,
        }
    }
}

impl Vocabulary {
    /// The comparison one decomposition member's own binding recorded, for a
    /// member that is an unprojected `own Bool` place. This is `state.origins`,
    /// the [ENT-3] comparison-origin map [`Self::scrutinee_relation`] reads, so
    /// a conjunct and a direct comparison on the same binding deliver the
    /// same relation over the same terms.
    pub(super) fn member_binding_relation(
        &self,
        member: GoalId,
        state: &FactState,
    ) -> Option<Relation> {
        let GoalExpression::Datum(GoalDatum::Place {
            root,
            projections,
            ty: CheckedType::Bool,
        }) = self.goals.expression(member)
        else {
            return None;
        };
        projections
            .is_empty()
            .then(|| state.origins.get(root).cloned())
            .flatten()
    }

    pub(super) fn integer_domain_plan(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        operands: &[IntegerDomainOperand],
    ) -> Option<IntegerDomainPlan> {
        let fragment = fragment_type(operand_type)?;
        if matches!(
            operation,
            CheckedIntegerOperation::AddExact
                | CheckedIntegerOperation::AddDefined
                | CheckedIntegerOperation::SubtractExact
                | CheckedIntegerOperation::SubtractDefined
                | CheckedIntegerOperation::MultiplyExact
                | CheckedIntegerOperation::MultiplyDefined
        ) {
            let [left, right] = operands else {
                return None;
            };
            let conjuncts =
                overflow_conjuncts_for_values(operation, left.constant, right.constant, fragment)?;
            let operand = if conjuncts.ground {
                Some(ZERO)
            } else if left.constant.is_some() {
                right.term
            } else {
                left.term
            };
            return Some(IntegerDomainPlan {
                components: vec![
                    BoundsRequest {
                        left: conjuncts.ground.then_some(ZERO).or(operand),
                        right: ZERO,
                        bound: conjuncts.upper,
                        distinct: false,
                    },
                    BoundsRequest {
                        left: (conjuncts.ground || operand.is_some()).then_some(ZERO),
                        right: if conjuncts.ground {
                            ZERO
                        } else {
                            operand.unwrap_or(ZERO)
                        },
                        bound: conjuncts.lower,
                        distinct: false,
                    },
                ],
                kind: IntegerDomainPlanKind::Conjunction,
            });
        }

        if matches!(
            operation,
            CheckedIntegerOperation::DivideExact
                | CheckedIntegerOperation::DivideDefined
                | CheckedIntegerOperation::RemainderExact
                | CheckedIntegerOperation::RemainderDefined
        ) {
            let [dividend, divisor] = operands else {
                return None;
            };
            let mut components = vec![BoundsRequest {
                left: divisor.term,
                right: ZERO,
                bound: 0,
                distinct: true,
            }];
            let kind = if fragment.signed() {
                components.push(BoundsRequest {
                    left: dividend.term,
                    right: self
                        .terms
                        .intern(TermKind::Constant(type_range(fragment).0)),
                    bound: 0,
                    distinct: true,
                });
                components.push(BoundsRequest {
                    left: divisor.term,
                    right: self.terms.intern(TermKind::Constant(-1)),
                    bound: 0,
                    distinct: true,
                });
                IntegerDomainPlanKind::SignedDivision
            } else {
                components.push(BoundsRequest {
                    left: Some(ZERO),
                    right: ZERO,
                    bound: 0,
                    distinct: false,
                });
                IntegerDomainPlanKind::Conjunction
            };
            normalize_distinct_requests(&mut components);
            return Some(IntegerDomainPlan { components, kind });
        }

        if matches!(
            operation,
            CheckedIntegerOperation::AbsoluteExact
                | CheckedIntegerOperation::AbsoluteDefined
                | CheckedIntegerOperation::NegateExact
                | CheckedIntegerOperation::NegateDefined
        ) {
            let [operand] = operands else {
                return None;
            };
            let mut components = vec![BoundsRequest {
                left: operand.term,
                right: self
                    .terms
                    .intern(TermKind::Constant(type_range(fragment).0)),
                bound: 0,
                distinct: true,
            }];
            normalize_distinct_requests(&mut components);
            return Some(IntegerDomainPlan {
                components,
                kind: IntegerDomainPlanKind::Conjunction,
            });
        }

        if matches!(
            operation,
            CheckedIntegerOperation::ShiftLeftExact
                | CheckedIntegerOperation::ShiftLeftDefined
                | CheckedIntegerOperation::ShiftRightExact
                | CheckedIntegerOperation::ShiftRightDefined
        ) {
            let [_, amount] = operands else {
                return None;
            };
            return Some(IntegerDomainPlan {
                components: vec![BoundsRequest {
                    left: amount.term,
                    right: ZERO,
                    bound: i128::from(fragment.width()) - 1,
                    distinct: false,
                }],
                kind: IntegerDomainPlanKind::Conjunction,
            });
        }

        None
    }
}

impl Reasoning<'_, '_, '_> {
    /// [ENT-3] comparison-origin shape (a): a direct comparison call whose
    /// operands are each a term or constant. A measure is itself an [ENT-2]
    /// term, including when its place contains admitted subscripts.
    pub(super) fn direct_comparison(&mut self, expression: &CheckedExpression) -> Option<Relation> {
        let CheckedExpression::IntegerOperation {
            operation,
            operand_type,
            arguments,
            ..
        } = expression
        else {
            return None;
        };
        fragment_type(*operand_type)?;
        let [left_expression, right_expression] = arguments.as_slice() else {
            return None;
        };
        let left = self
            .measure_operand(left_expression)
            .or_else(|| self.read_operand(left_expression))?;
        let right = self
            .measure_operand(right_expression)
            .or_else(|| self.read_operand(right_expression))?;
        sources::comparison_relation(*operation, left, right, 0)
    }

    /// [ENT-3] comparison origin of a match scrutinee: shape (a) directly, or
    /// shape (b), a bare `own Bool` binding whose initializer comparison is
    /// still valid on every path to this use.
    pub(super) fn scrutinee_relation(
        &mut self,
        expression: &CheckedExpression,
        state: &FactState,
    ) -> Option<Relation> {
        if let Some(relation) = self.direct_comparison(expression) {
            return Some(relation);
        }
        if let CheckedExpression::Binding { binding, ty, .. } = expression
            && *ty == CheckedType::Bool
        {
            return state.origins.get(binding).cloned();
        }
        None
    }

    /// Exact value identity for one operand of an already-reached proof
    /// obligation. Stable structural expressions are preferred so a prior
    /// source fact can name the same value. The occurrence-local fallback is
    /// reserved for a value that cannot be safely replayed from source.
    pub(super) fn obligation_goal_operand(
        &mut self,
        site: &crate::NodePath,
        operand: usize,
        expression: &CheckedExpression,
        facts: &FactState,
    ) -> GoalExpression {
        self.input
            .admitted_value_goal_expression(expression)
            .map(|expression| self.expand_goal_expression(&expression, facts))
            .unwrap_or_else(|| {
                let operand =
                    u32::try_from(operand).expect("proof-obligation operand ordinal exceeds u32");
                GoalExpression::Datum(GoalDatum::EvaluatedValue {
                    function: self.input.function.id,
                    occurrence: EvaluatedValueOccurrence::ObligationOperand {
                        site: site.clone(),
                        operand,
                    },
                    captured_type: expression.ty(),
                    projections: Vec::new(),
                    ty: expression.ty(),
                })
            })
    }

    /// Replaces every still-valid ordinary-let leaf by its one complete
    /// origin. Leaves without a valid origin remain direct, so expansion is
    /// all-or-nothing over exactly the eligible leaves.
    pub(super) fn expand_goal_expression(
        &mut self,
        expression: &GoalExpression,
        state: &FactState,
    ) -> GoalExpression {
        self.expand_goal_expression_inner(expression, state, &mut HashSet::new(), false)
    }

    pub(super) fn expand_goal_expression_inner(
        &mut self,
        expression: &GoalExpression,
        state: &FactState,
        expanding: &mut HashSet<BindingId>,
        preserve_normalized_leaf: bool,
    ) -> GoalExpression {
        match expression {
            GoalExpression::Datum(GoalDatum::Place {
                root,
                projections,
                ty,
            }) => {
                let Some(origin) = state.goal_origins.get(root).copied() else {
                    return expression.clone();
                };
                if !expanding.insert(*root) {
                    return expression.clone();
                }
                let origin = self.vocabulary.goals.expression(origin).clone();
                let mut expanded = self.expand_goal_expression_inner(
                    &origin,
                    state,
                    expanding,
                    preserve_normalized_leaf,
                );
                expanding.remove(root);
                for projection in projections {
                    let Some(result) = self.input.goal_projection_type(expanded.ty(), *projection)
                    else {
                        return expression.clone();
                    };
                    let Some(next) = expanded.with_projection(*projection, result) else {
                        return expression.clone();
                    };
                    expanded = next;
                }
                if expanded.ty() == *ty {
                    expanded
                } else {
                    expression.clone()
                }
            }
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            } => {
                // Once an operation already has an exact L0 projection or
                // domain normalization, expanding one of its place operands
                // into a non-fragment expression would erase the checker
                // fact that Contrib(P) must classify. Boolean parents still
                // expand their children, so their normalized leaf predicates
                // remain visible without sacrificing those leaf identities.
                if preserve_normalized_leaf
                    && (self.goal_projection(expression).is_some()
                        || self.goal_normalization(expression).is_some())
                {
                    return expression.clone();
                }
                GoalExpression::Operation {
                    row: *row,
                    type_arguments: type_arguments.clone(),
                    const_arguments: const_arguments.clone(),
                    result: *result,
                    arguments: arguments
                        .iter()
                        .map(|argument| {
                            self.expand_goal_expression_inner(
                                argument,
                                state,
                                expanding,
                                preserve_normalized_leaf,
                            )
                        })
                        .collect(),
                }
            }
            GoalExpression::Datum(_) => expression.clone(),
        }
    }

    pub(super) fn goal_origin_set(
        &mut self,
        expression: &CheckedExpression,
        state: &FactState,
    ) -> Vec<GoalId> {
        let Some(direct) = self.input.admitted_value_goal_expression(expression) else {
            return Vec::new();
        };
        if direct.ty() != CheckedType::Bool {
            return Vec::new();
        }
        let complete = self.expand_goal_expression(&direct, state);
        let direct = self.intern_goal_expression(direct);
        let complete = self.intern_goal_expression(complete);
        if direct == complete {
            vec![direct]
        } else {
            vec![direct, complete]
        }
    }

    pub(super) fn record_goal_origin(
        &mut self,
        binding: BindingId,
        value: &CheckedExpression,
        state: &mut FactState,
    ) {
        let Some(direct) = self.input.admitted_value_goal_expression(value) else {
            return;
        };
        let origin = self.intern_goal_expression(direct);
        state.goal_origins.insert(binding, origin);
        state.ambiguous_goal_origins.remove(&binding);
    }

    pub(super) fn intern_goal_expression(&mut self, expression: GoalExpression) -> GoalId {
        if let GoalExpression::Operation {
            row: GoalOperation::Boolean(_),
            arguments,
            ..
        } = &expression
        {
            for argument in arguments {
                if argument.ty() == CheckedType::Bool {
                    self.intern_goal_expression(argument.clone());
                }
            }
        }
        let projection = self.goal_projection(&expression);
        let normalization = self.goal_normalization(&expression);
        let mut support = Vec::new();
        collect_goal_support(&expression, None, &mut support);
        self.vocabulary
            .goals
            .intern(expression, projection, normalization, support)
    }

    /// [O11 candidate] The signed Boolean decomposition set of one
    /// established goal: `+band` and `-bor` decompose into their signed
    /// children recursively, `bnot` flips the sign, and every other root —
    /// in particular `-band` and `+bor`, whose content is genuinely
    /// disjunctive, and `bxor` on either sign — contributes nothing.
    ///
    /// Members are interned so their exact identities, projections, and
    /// supports are retained in the inventory, but nothing establishes them
    /// as facts in this version: v0.30 acceptance is untouched. Design:
    /// `research/investigations/o11-composition/DESIGN.md`.
    pub(super) fn signed_boolean_decomposition(
        &mut self,
        parent: GoalId,
        sign: GoalSign,
        state: &FactState,
    ) -> Vec<(GoalId, GoalSign)> {
        let expression = self.vocabulary.goals.expression(parent).clone();
        let mut members = Vec::new();
        self.collect_decomposition_members(
            &expression,
            sign,
            state,
            &mut members,
            &mut HashSet::new(),
        );
        members
    }

    pub(super) fn collect_decomposition_members(
        &mut self,
        expression: &GoalExpression,
        sign: GoalSign,
        state: &FactState,
        members: &mut Vec<(GoalId, GoalSign)>,
        following: &mut HashSet<BindingId>,
    ) {
        // An unprojected `own Bool` leaf carrying a still-valid ordinary-let
        // origin stands for that origin under either sign, so the Boolean root
        // is read through the leaf and the leaf contributes no member of its
        // own. Reading the origin here is what keeps a conjunct in the operand
        // form its own binding recorded: the members of a `band` written over
        // comparison bindings are those bindings, whose relations
        // [`Self::establish_boolean_decomposition`] then takes from
        // `state.origins`, so both source spellings use the same relations.
        if let GoalExpression::Datum(GoalDatum::Place {
            root,
            projections,
            ty: CheckedType::Bool,
        }) = expression
            && projections.is_empty()
        {
            let Some(origin) = state.goal_origins.get(root).copied() else {
                return;
            };
            // Only a Boolean root has anything to decompose, and the ordinary
            // guard binding holds a comparison, so this settles the common case
            // without retaining the origin.
            if !matches!(
                self.vocabulary.goals.expression(origin),
                GoalExpression::Operation {
                    row: GoalOperation::Boolean(_),
                    ..
                }
            ) {
                return;
            }
            if !following.insert(*root) {
                return;
            }
            let origin = self.vocabulary.goals.expression(origin).clone();
            self.collect_decomposition_members(&origin, sign, state, members, following);
            following.remove(root);
            return;
        }
        let GoalExpression::Operation {
            row: GoalOperation::Boolean(operation),
            arguments,
            ..
        } = expression
        else {
            return;
        };
        let child_sign = match (operation, sign) {
            (CheckedBooleanOperation::And, GoalSign::Positive)
            | (CheckedBooleanOperation::Or, GoalSign::Negative) => sign,
            (CheckedBooleanOperation::Not, GoalSign::Positive) => GoalSign::Negative,
            (CheckedBooleanOperation::Not, GoalSign::Negative) => GoalSign::Positive,
            _ => return,
        };
        for argument in arguments {
            let member = self.intern_goal_expression(argument.clone());
            if !members.contains(&(member, child_sign)) {
                members.push((member, child_sign));
            }
            self.collect_decomposition_members(argument, child_sign, state, members, following);
        }
    }

    /// [ENT-3] Establishes the signed Boolean decomposition set of one
    /// just-established signed goal, at that same point and in whatever proof
    /// view the state carries.
    ///
    /// Each member enters as its own concrete opaque goal under [FN-8]
    /// structural identity, and a member whose complete root is one admitted
    /// comparison additionally delivers its exact L0 projection under `+` and
    /// that projection's exact negation under `-`. A member that is a bare
    /// comparison binding carries no projection of its own and delivers the
    /// relation that binding recorded instead, so a conjunct proves exactly
    /// what the same comparison proves at a direct branch.
    /// Decomposition never runs upward: this establishes children of an
    /// established parent only, so no child ever establishes or derives a
    /// parent.
    pub(super) fn establish_boolean_decomposition(
        &mut self,
        parent: GoalId,
        sign: GoalSign,
        state: &mut FactState,
        event: FlowEventId,
    ) {
        for (member, member_sign) in self.signed_boolean_decomposition(parent, sign, state) {
            state.establish_goal(member, member_sign, &mut self.vocabulary.derivations, event);
            let Some(relation) = self
                .vocabulary
                .goals
                .projection(member)
                .cloned()
                .or_else(|| self.vocabulary.member_binding_relation(member, state))
            else {
                continue;
            };
            let relation = match member_sign {
                GoalSign::Positive => relation,
                GoalSign::Negative => relation.negated(),
            };
            state.establish(&relation, &mut self.vocabulary.derivations, event);
        }
    }

    pub(super) fn goal_projection(&mut self, expression: &GoalExpression) -> Option<Relation> {
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation,
                    operand_type,
                },
            arguments,
            ..
        } = expression
        else {
            return None;
        };
        fragment_type(*operand_type)?;
        let [left, right] = arguments.as_slice() else {
            return None;
        };
        // [MSR-5] each side is an affine expression, so each projects to one
        // term displaced by a constant and the two displacements fold into
        // the one constant a difference bound carries.
        let (left, left_constant) = self.goal_side(left)?;
        let (right, right_constant) = self.goal_side(right)?;
        sources::comparison_relation(
            *operation,
            left,
            right,
            right_constant.checked_sub(left_constant)?,
        )
    }

    /// One clause side as a term displaced by a constant [MSR-5].
    ///
    /// A side with no term at all is one constant and keeps the constant term
    /// [ENT-2] folds it onto; a side carrying two terms, or a term with any
    /// coefficient other than one, is outside the difference-bound fragment
    /// and projects to nothing, which only under-derives [ENT-1].
    pub(super) fn goal_side(&mut self, expression: &GoalExpression) -> Option<(TermId, i128)> {
        let (term, constant) = self.goal_affine_side(expression)?;
        match term {
            Some(term) => Some((term, constant)),
            None => Some((
                self.vocabulary.terms.intern(TermKind::Constant(constant)),
                0,
            )),
        }
    }

    pub(super) fn goal_affine_side(
        &mut self,
        expression: &GoalExpression,
    ) -> Option<(Option<TermId>, i128)> {
        if let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation:
                        operation @ (CheckedIntegerOperation::AddExact
                        | CheckedIntegerOperation::SubtractExact
                        | CheckedIntegerOperation::MultiplyExact),
                    ..
                },
            arguments,
            ..
        } = expression
        {
            let [left, right] = arguments.as_slice() else {
                return None;
            };
            let (left_term, left_value) = self.goal_affine_side(left)?;
            let (right_term, right_value) = self.goal_affine_side(right)?;
            return match operation {
                CheckedIntegerOperation::AddExact => {
                    if left_term.is_some() && right_term.is_some() {
                        return None;
                    }
                    Some((
                        left_term.or(right_term),
                        left_value.checked_add(right_value)?,
                    ))
                }
                CheckedIntegerOperation::SubtractExact => {
                    if right_term.is_some() {
                        return None;
                    }
                    Some((left_term, left_value.checked_sub(right_value)?))
                }
                _ => {
                    if left_term.is_some() || right_term.is_some() {
                        return None;
                    }
                    Some((None, left_value.checked_mul(right_value)?))
                }
            };
        }
        if let GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { ty, bits })) =
            expression
        {
            return Some((None, integer_value(*ty, *bits)));
        }
        Some((Some(self.goal_operand(expression)?), 0))
    }

    pub(super) fn goal_operand(&mut self, expression: &GoalExpression) -> Option<TermId> {
        match expression {
            // [MSR-6] a const generic operand is the symbolic constant term.
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::ConstGeneric {
                declaration,
                ..
            })) => Some(self.const_parameter_term(*declaration)),
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { ty, bits })) => Some(
                self.vocabulary
                    .terms
                    .intern(TermKind::Constant(integer_value(*ty, *bits))),
            ),
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) if projections.is_empty() => {
                let CheckedValue::Integer {
                    ty: value_type,
                    bits,
                } = &self.input.context.constant(*declaration)?.value
                else {
                    return None;
                };
                (*ty == CheckedType::Integer(*value_type)).then(|| {
                    self.vocabulary
                        .terms
                        .intern(TermKind::Constant(integer_value(*value_type, *bits)))
                })
            }
            GoalExpression::Datum(datum) => {
                let fragment = fragment_type(datum.ty())?;
                let path = self.input.goal_place_path(datum)?;
                Some(
                    self.vocabulary
                        .terms
                        .intern(TermKind::Place(path, fragment)),
                )
            }
            GoalExpression::Operation { row, arguments, .. }
                if matches!(
                    row,
                    GoalOperation::ArrayMeasure { .. }
                        | GoalOperation::BufferMeasure { .. }
                        | GoalOperation::ContainerMeasure { .. }
                ) =>
            {
                let [place] = arguments.as_slice() else {
                    return None;
                };
                let GoalExpression::Datum(datum) = place else {
                    return None;
                };
                let path = self.input.goal_place_path(datum)?;
                let (measure, measured, array_length) = match row {
                    GoalOperation::ArrayMeasure {
                        measure, length, ..
                    } => (*measure, MeasuredKind::ConstantArray, Some(*length)),
                    GoalOperation::BufferMeasure { measure, .. } => {
                        (*measure, MeasuredKind::RuntimeArray, None)
                    }
                    // [MSR-1]'s row for a storage shape. The written
                    // constant is what `measure_term` reads for a cell the
                    // table fixes as the type's own constant [MSR-2].
                    GoalOperation::ContainerMeasure {
                        measure,
                        measured,
                        constant,
                        ..
                    } => (*measure, *measured, *constant),
                    _ => return None,
                };
                // [REF-4, MSR-1] a range whose path ends in a range step is
                // the anonymous one an actual formed at its call: it names no
                // binding, so its `len` is the mathematical difference of the
                // endpoint values the formation captured rather than a
                // measure of the storage below the step. Where both endpoints
                // are value-determined that difference is a constant, which
                // is the pre-transfer term [MSR-3] of that measure. An
                // endpoint this relation cannot name leaves the opaque
                // measure term, which carries no fact and only under-derives.
                if measure == CheckedMeasure::Length
                    && let Some(PlaceStep::Range(range)) = path.path.last()
                    && let Some(length) = range.constant_length()
                {
                    return Some(self.vocabulary.terms.intern(TermKind::Constant(length)));
                }
                Some(self.measure_term(measure, path, measured, array_length))
            }
            GoalExpression::Operation { .. } => None,
        }
    }

    /// [ENT-3.S6] the equality one range formation establishes on its
    /// binder's `len`: `deref(part).len = hi - lo`, read over the exact
    /// current-value images captured where the endpoints are evaluated.
    ///
    /// L0 is a difference-bound fragment [ENT-4], so the equality is stored
    /// exactly when the mathematical difference is one term displaced by a
    /// constant: two constant endpoints give the constant length, and an
    /// endpoint pair sharing a term gives the same. A pair naming two
    /// different terms — `&p[a..b]` — is outside the fragment and keeps only
    /// the affine image, which only under-derives [ENT-1].
    pub(super) fn range_length_relation(
        &mut self,
        length: TermId,
        start: &CheckedExpression,
        end: &CheckedExpression,
    ) -> Option<Relation> {
        let start_goal = self.input.admitted_value_goal_expression(start)?;
        let end_goal = self.input.admitted_value_goal_expression(end)?;
        let (start_term, start_constant) = self.goal_affine_side(&start_goal)?;
        let (end_term, end_constant) = self.goal_affine_side(&end_goal)?;
        let difference = end_constant.checked_sub(start_constant)?;
        let right = match (start_term, end_term) {
            (None, None) => self.vocabulary.terms.intern(TermKind::Constant(difference)),
            (None, Some(term)) => {
                return Some(Relation::Equal {
                    left: length,
                    right: term,
                    difference,
                });
            }
            (Some(start), Some(end)) if start == end => {
                self.vocabulary.terms.intern(TermKind::Constant(difference))
            }
            _ => return None,
        };
        Some(Relation::Equal {
            left: length,
            right,
            difference: 0,
        })
    }

    /// Installs one captured range image on the place that now names it.
    pub(super) fn establish_captured_range_length(
        &mut self,
        destination: ResolvedPlace,
        captured: CapturedRange,
        state: &mut AffineFlowState,
    ) -> Option<TermId> {
        let length = captured_range_length_image(captured, state)?;
        let term = self.place_measure_term(
            CheckedMeasure::Length,
            destination,
            MeasuredKind::Range,
            None,
        );
        state.measure_atoms.borrow_mut().insert(term, length);
        Some(term)
    }

    pub(super) fn integer_domain_goal(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        node_path: &crate::NodePath,
        facts: &FactState,
    ) -> GoalExpression {
        GoalExpression::Operation {
            row: GoalOperation::Integer {
                operation: operation
                    .defined_query()
                    .expect("every proof-required exact row has one total domain query"),
                operand_type,
            },
            type_arguments: Vec::new(),
            const_arguments: Vec::new(),
            result: CheckedType::Bool,
            arguments: arguments
                .iter()
                .enumerate()
                .map(|(ordinal, argument)| {
                    self.obligation_goal_operand(node_path, ordinal, argument, facts)
                })
                .collect(),
        }
    }

    pub(super) fn integer_domain_components(
        &mut self,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
    ) -> Vec<BoundsRequest> {
        let operands = arguments
            .iter()
            .map(|argument| IntegerDomainOperand {
                term: self.read_operand(argument),
                constant: checked_integer_constant(argument),
            })
            .collect::<Vec<_>>();
        self.vocabulary
            .integer_domain_plan(operation, operand_type, &operands)
            .map_or_else(Vec::new, |plan| plan.components)
    }

    /// The one normalization authority attached to any goal family that has
    /// a fixed L0 interpretation. Integer domains may use a small DNF;
    /// AllocationFit is one conjunction containing its ceiling comparison.
    pub(super) fn goal_normalization(
        &mut self,
        expression: &GoalExpression,
    ) -> Option<GoalNormalization> {
        if let Some(normalization) = self.conversion_goal_normalization(expression) {
            return Some(normalization);
        }
        if let Some(plan) = self.goal_integer_domain_plan(expression) {
            return Some(plan.normalization());
        }
        let GoalExpression::Operation {
            row: GoalOperation::BufferFits { maximum_length, .. },
            arguments,
            result: CheckedType::Bool,
            ..
        } = expression
        else {
            return None;
        };
        let [length] = arguments.as_slice() else {
            return None;
        };
        let threshold = self
            .vocabulary
            .terms
            .intern(TermKind::Constant(i128::from(*maximum_length)));
        Some(GoalNormalization::conjunction(vec![
            self.goal_operand(length).map(|length| Relation::Bound {
                left: length,
                right: threshold,
                bound: 0,
            }),
        ]))
    }

    pub(super) fn goal_integer_domain_plan(
        &mut self,
        expression: &GoalExpression,
    ) -> Option<IntegerDomainPlan> {
        let GoalExpression::Operation {
            row:
                GoalOperation::Integer {
                    operation,
                    operand_type,
                },
            arguments,
            result: CheckedType::Bool,
            ..
        } = expression
        else {
            return None;
        };
        if !operation.is_defined_query() {
            return None;
        }
        let mut operands = Vec::with_capacity(arguments.len());
        for argument in arguments {
            operands.push(IntegerDomainOperand {
                term: self.goal_operand(argument),
                constant: self.input.goal_integer_constant(argument),
            });
        }
        self.vocabulary
            .integer_domain_plan(*operation, *operand_type, &operands)
    }
}

impl Judging<'_, '_, '_> {
    /// Records the O11 decomposition inventory entry for one signed-goal
    /// establishment. Entries deduplicate by parent and sign; this is
    /// retained metadata beside the facts
    /// [`Self::establish_boolean_decomposition`] establishes.
    pub(super) fn record_boolean_decomposition(
        &mut self,
        parent: GoalId,
        sign: GoalSign,
        state: &FactState,
    ) {
        if self
            .output
            .boolean_decompositions
            .iter()
            .any(|candidate| candidate.parent == parent && candidate.sign == sign)
        {
            return;
        }
        let members = self
            .reasoning()
            .signed_boolean_decomposition(parent, sign, state);
        if members.is_empty() {
            return;
        }
        self.output
            .boolean_decompositions
            .push(super::super::BooleanGoalDecomposition {
                parent,
                sign,
                members,
            });
    }
}

/// The [ENT-2] goal projection one resolved-path step spells, where that
/// language has one.
///
/// A goal datum's place is a tracked place [ENT-2] clause (a) or a measure
/// place clause (b): field selections, `deref` wrappings, subscripts, and the
/// one range step the image of an anonymous `&[T]` actual carries [REF-4]. A
/// payload step preserves the selected variant and field. A window part is
/// effect vocabulary, not a value projection. Support may conservatively
/// widen to a prefix, but a value identity must never be shortened that way.
pub(super) fn goal_projection_of_step(step: &PlaceStep) -> Option<GoalProjection> {
    match step {
        PlaceStep::Deref => Some(GoalProjection::Deref),
        PlaceStep::Field(field) => Some(GoalProjection::Field(*field)),
        PlaceStep::Payload { variant, field } => Some(GoalProjection::Payload {
            variant: *variant,
            field: *field,
        }),
        PlaceStep::Index(offset) => Some(GoalProjection::Subscript(offset.goal_identity())),
        // [REF-4] a range step's endpoint captures identify the formation
        // whose immutable affine image gives the anonymous range its length.
        // Canonicalizing those captures by endpoint spelling would merge two
        // formations that read the same binding at different times.
        PlaceStep::Range(range) => Some(GoalProjection::Range(*range)),
        PlaceStep::Part(_) | PlaceStep::Measure(_) | PlaceStep::Descendant(_) => None,
    }
}

/// Replaces one occurrence-local FN-8 actual with the same admitted
/// structural value used by the rest of ENT-2. The caller invokes this
/// only after every obligation in every actual expression has succeeded.
/// A projection that the admitted structural tree cannot represent keeps
/// the occurrence-local value instead of inventing a different identity.
pub(super) fn admitted_call_goal_expression(
    expression: &GoalExpression,
    call: &crate::NodePath,
    arguments: &[Option<GoalExpression>],
) -> GoalExpression {
    match expression {
        GoalExpression::Datum(
            original @ GoalDatum::EvaluatedValue {
                occurrence:
                    EvaluatedValueOccurrence::CallArgument {
                        call: occurrence_call,
                        argument,
                    },
                captured_type,
                projections,
                ty,
                ..
            },
        ) if occurrence_call == call => {
            let Some(mut admitted) = usize::try_from(*argument)
                .ok()
                .and_then(|index| arguments.get(index))
                .and_then(Option::as_ref)
                .filter(|argument| argument.ty() == *captured_type)
                .cloned()
            else {
                return GoalExpression::Datum(original.clone());
            };
            for projection in projections {
                let Some(projected) = admitted.with_projection(*projection, *ty) else {
                    return GoalExpression::Datum(original.clone());
                };
                admitted = projected;
            }
            if admitted.ty() == *ty {
                admitted
            } else {
                GoalExpression::Datum(original.clone())
            }
        }
        GoalExpression::Operation {
            row,
            type_arguments,
            const_arguments,
            result,
            arguments: operands,
        } => GoalExpression::Operation {
            row: *row,
            type_arguments: type_arguments.clone(),
            const_arguments: const_arguments.clone(),
            result: *result,
            arguments: operands
                .iter()
                .map(|operand| admitted_call_goal_expression(operand, call, arguments))
                .collect(),
        },
        GoalExpression::Datum(datum) => GoalExpression::Datum(datum.clone()),
    }
}

pub(super) fn goal_binding_place(
    binding: BindingId,
    projections: impl IntoIterator<Item = GoalProjection>,
    ty: CheckedType,
) -> GoalExpression {
    GoalExpression::Datum(GoalDatum::Place {
        root: binding,
        projections: projections.into_iter().collect(),
        ty,
    })
}

pub(super) fn record_value_initializer_origin(frame: &GiveFrame, state: &mut FactState) {
    let mut origins =
        frame
            .gives
            .iter()
            .zip(&frame.give_goal_origins)
            .filter_map(|(edge, origin)| {
                let edge = &edge.facts;
                (!edge.all_derivable).then_some(*origin)
            });
    let Some(first) = origins.next() else {
        return;
    };
    if origins.any(|origin| origin != first) {
        state.ambiguous_goal_origins.insert(frame.binding);
    }
}

pub(super) fn collect_goal_support(
    expression: &GoalExpression,
    measure: Option<CheckedMeasure>,
    support: &mut Vec<GoalSupport>,
) {
    match expression {
        GoalExpression::Datum(GoalDatum::Place {
            root, projections, ..
        }) => support.push(GoalSupport {
            root: *root,
            projections: projections.clone(),
            measure,
        }),
        GoalExpression::Datum(
            GoalDatum::Parameter { .. }
            | GoalDatum::NamedConst { .. }
            | GoalDatum::EvaluatedValue { .. }
            | GoalDatum::Literal(_),
        ) => {}
        GoalExpression::Operation { row, arguments, .. } => {
            // [MSR-2] every measure of one place has the same support,
            // P's descriptor storage; the selected measure only says that
            // this node is a measure node rather than a place node.
            let node_measure = match row {
                GoalOperation::ArrayMeasure { measure, .. }
                | GoalOperation::BufferMeasure { measure, .. }
                | GoalOperation::ContainerMeasure { measure, .. } => Some(*measure),
                _ => None,
            };
            for argument in arguments {
                collect_goal_support(argument, node_measure, support);
            }
        }
    }
}

/// The immutable length image one [REF-4] formation captured.
///
/// The range-image table is keyed by the formation's original capture
/// occurrence. Reading it here keeps a later rebind from reconstructing
/// the range through endpoint bindings whose current values may differ
/// from the values the formation evaluated.
pub(super) fn captured_range_length_image(
    captured: CapturedRange,
    state: &AffineFlowState,
) -> Option<AffineForm> {
    let image = state.ranges.get(&captured.start.capture)?;
    image
        .end
        .subtract(&image.start, &mut AffineCheckState::new())
        .ok()
}

/// [EFF-5] one declared `epsuffix*` as resolved-path steps, with each
/// index and range position substituted by its own argument's value.
///
/// A signature never contains an index expression: an index position
/// names a value parameter of the same callable [EFF-1], and [EFF-5]
/// substitutes that parameter's actual at the call, evaluated once.
/// `offsets` carries the value each such parameter's argument holds. A
/// position whose argument is no value a place relation can name stays
/// the unknown offset, which no admitted family separates and which
/// therefore reaches every element of the indexed base. That is the
/// conservative direction for a kill.
pub(super) fn substituted_steps(
    steps: &[super::super::super::model::CheckedEffectStep],
    offsets: &HashMap<crate::DeclarationId, CapturedValue>,
) -> Vec<PlaceStep> {
    use super::super::super::model::CheckedEffectStep as Step;
    let offset = |declaration: &crate::DeclarationId| {
        offsets
            .get(declaration)
            .copied()
            .unwrap_or_else(CapturedValue::unknown)
    };
    steps
        .iter()
        .map(|step| match step {
            Step::Field(field) => PlaceStep::Field(*field),
            Step::Deref => PlaceStep::Deref,
            Step::Payload { variant, field } => PlaceStep::Payload {
                variant: *variant,
                field: *field,
            },
            Step::Index(declaration) => PlaceStep::Index(offset(declaration)),
            Step::Range { start, end } => PlaceStep::Range(CapturedRange {
                start: offset(start),
                end: offset(end),
            }),
            Step::Part(part) => PlaceStep::Part(*part),
            Step::Measure(measure) => PlaceStep::Measure(*measure),
        })
        .collect()
}

/// The value one call's argument supplies to an [EFF-5] substituted row
/// position, for every value parameter whose argument names one.
///
/// [REF-1] names a captured value by the occurrence it was evaluated at,
/// and a substituted position is evaluated at its argument rather than at
/// a `psuffix` of its own, so every position of one call shares the one
/// reserved identity below. Two positions holding two different written
/// literals are still separated by [OWN-7], which reads the value beside
/// the identity; a pair the identity alone leaves unseparated is treated
/// as one storage, which is the conservative direction for a kill.
pub(super) fn substituted_offsets(
    callee: Option<&super::super::EntailmentCallee>,
    captures: &[CapturedValue],
) -> HashMap<crate::DeclarationId, CapturedValue> {
    let mut offsets = HashMap::new();
    let Some(callee) = callee else {
        return offsets;
    };
    for (declaration, capture) in callee.parameter_declarations.iter().zip(captures) {
        if !matches!(capture.term, CapturedTerm::Opaque) {
            offsets.insert(*declaration, *capture);
        }
    }
    offsets
}

pub(super) fn checked_integer_constant(expression: &CheckedExpression) -> Option<i128> {
    match expression {
        CheckedExpression::Constant(CheckedValue::Integer { ty, bits })
        | CheckedExpression::NamedConstant {
            value: CheckedValue::Integer { ty, bits },
            ..
        } => Some(integer_value(*ty, *bits)),
        _ => None,
    }
}

/// A let-origin expansion is valid only while the bound value has no `set`
/// target on the path to its use. The target's projection does not narrow
/// this invalidation: changing one field or element invalidates the aggregate
/// value identity even when a separately established length fact survives.
pub(super) fn invalidate_goal_origin_for_set(state: &mut FactState, target: &CheckedSetTarget) {
    state.goal_origins.remove(&target.binding());
    state.ambiguous_goal_origins.remove(&target.binding());
}

/// The type one slot of an indexable base holds [OP-4, WIN-1].
pub(super) fn element_type(input: CheckedType, elements: &[CheckedType]) -> Option<CheckedType> {
    match input {
        CheckedType::Buffer { element } => elements.get(element.index()).copied(),
        CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
            elements.get(element.0 as usize).copied()
        }
        _ => None,
    }
}

/// The place one element write names [MSR-2]: the base it selects through,
/// with the element it writes appended.
///
/// [MSR-2] states the granularity over storage: a write at an element
/// position of P overlaps the descriptor storage of `P[i]` and none of P's
/// own, so the event carries `P[i]` and the overlap relation reads it.
pub(super) fn element_write_place(mut base: ResolvedPlace, offset: CapturedValue) -> ResolvedPlace {
    base.path.push(PlaceStep::Index(offset));
    base
}
