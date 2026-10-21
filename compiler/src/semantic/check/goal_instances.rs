//! A callee's goals instantiated at one call: each formal replaced by its
//! argument, each formal region and type parameter by the caller's [FN-2].

use super::*;

impl Checker<'_, '_> {
    pub(super) fn instantiate_goal_expression(
        &mut self,
        check_context: &CheckContext<'_>,
        expression: &GoalExpression,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
        arguments: &[GoalExpression],
    ) -> Result<GoalExpression, CheckStop> {
        match expression {
            GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ty,
            }) => {
                let index = usize::try_from(*ordinal)
                    .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                let parameter = signature
                    .parameters
                    .get(index)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let mut image = arguments
                    .get(index)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let parameter_type =
                    self.instantiate_goal_type(check_context, parameter.ty, signature, regions)?;
                if image.ty() != parameter_type {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                let remaining = if parameter.mode == CheckedMode::Own {
                    projections.as_slice()
                } else {
                    let Some((GoalProjection::Deref, remaining)) = projections.split_first() else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    remaining
                };
                let final_type =
                    self.instantiate_goal_type(check_context, *ty, signature, regions)?;
                for projection in remaining {
                    // [MSR-1, EFF-5] a formal-valued subscript names a value
                    // parameter of the callee, and the caller substitutes its
                    // own actual for it exactly as it substitutes a row's
                    // index positions: the offset the place is identified
                    // over is the value that argument names here.
                    let projection = match projection {
                        GoalProjection::FormalSubscript { ordinal } => GoalProjection::Subscript(
                            self.goal_argument_offset(arguments.get(*ordinal as usize), None)?,
                        ),
                        GoalProjection::FormalPage { ordinal } => GoalProjection::Page(
                            self.goal_argument_offset(arguments.get(*ordinal as usize), None)?,
                        ),
                        GoalProjection::FormalPlaceSubscript { ordinal, place, ty }
                        | GoalProjection::FormalPlacePage { ordinal, place, ty } => {
                            let ty =
                                self.instantiate_goal_type(check_context, *ty, signature, regions)?;
                            let offset = self.goal_argument_offset(
                                arguments.get(*ordinal as usize),
                                Some((*place, ty)),
                            )?;
                            if matches!(projection, GoalProjection::FormalPlacePage { .. }) {
                                GoalProjection::Page(offset)
                            } else {
                                GoalProjection::Subscript(offset)
                            }
                        }
                        other => *other,
                    };
                    image = image
                        .with_projection(projection, final_type)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                }
                if image.ty() != final_type {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                Ok(image)
            }
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
                admission,
            }) => Ok(GoalExpression::Datum(GoalDatum::NamedConst {
                declaration: *declaration,
                projections: projections.clone(),
                ty: self.instantiate_goal_type(check_context, *ty, signature, regions)?,
                admission: admission.clone(),
            })),
            GoalExpression::Datum(GoalDatum::Literal(value)) => {
                Ok(GoalExpression::Datum(GoalDatum::Literal(
                    self.instantiate_goal_value(check_context, value, signature, regions)?,
                )))
            }
            GoalExpression::Datum(GoalDatum::Place { .. } | GoalDatum::EvaluatedValue { .. }) => {
                Err(SemanticCompilerFailure::InvalidResolution.into())
            }
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments: operands,
            } => Ok(GoalExpression::Operation {
                row: self.instantiate_goal_operation(check_context, *row, signature, regions)?,
                type_arguments: type_arguments
                    .iter()
                    .map(|ty| self.instantiate_goal_type(check_context, *ty, signature, regions))
                    .collect::<Result<Vec<_>, _>>()?,
                const_arguments: const_arguments
                    .iter()
                    .map(|value| self.types.instantiate_goal_const(*value, signature))
                    .collect::<Result<Vec<_>, _>>()?,
                result: self.instantiate_goal_type(check_context, *result, signature, regions)?,
                arguments: operands
                    .iter()
                    .map(|operand| {
                        self.instantiate_goal_expression(
                            check_context,
                            operand,
                            signature,
                            regions,
                            arguments,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            }),
        }
    }

    /// The offset one actual supplies to a formal-valued subscript [MSR-1].
    ///
    /// [ENT-2] decides a place's identity by its proof path, so an
    /// argument [`super::super::goal::actual_offset`] names supplies exactly that,
    /// and an argument it names no offset for supplies the unknown offset,
    /// which no admitted family separates -- the conservative reading in
    /// both directions [OWN-7].
    pub(super) fn goal_argument_offset(
        &self,
        argument: Option<&GoalExpression>,
        projection: Option<(super::super::places::FormalOffsetId, CheckedType)>,
    ) -> Result<super::super::places::CapturedValue, CheckStop> {
        let named = |declaration| {
            let constant = self
                .types
                .constant(*self.types.constants.get(&declaration)?);
            match constant.ok()?.value {
                CheckedValue::Integer { bits, .. } => Some(bits),
                _ => None,
            }
        };
        Ok(argument
            .and_then(|argument| match projection {
                Some((place, ty)) => super::super::goal::projected_actual_offset(
                    argument,
                    place,
                    ty,
                    &self.types.captured_places,
                    &named,
                ),
                None => super::super::goal::actual_offset(
                    argument,
                    &self.types.captured_places,
                    &named,
                ),
            })
            .unwrap_or_else(super::super::places::CapturedValue::unknown))
    }

    pub(super) fn instantiate_goal_operation(
        &mut self,
        check_context: &CheckContext<'_>,
        operation: GoalOperation,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<GoalOperation, CheckStop> {
        Ok(match operation {
            GoalOperation::Integer {
                operation,
                operand_type,
            } => GoalOperation::Integer {
                operation,
                operand_type: self.instantiate_goal_type(
                    check_context,
                    operand_type,
                    signature,
                    regions,
                )?,
            },
            GoalOperation::Float {
                operation,
                operand_type,
            } => GoalOperation::Float {
                operation,
                operand_type: self.instantiate_goal_type(
                    check_context,
                    operand_type,
                    signature,
                    regions,
                )?,
            },
            GoalOperation::NumericConversion {
                mode,
                source,
                destination,
            } => GoalOperation::NumericConversion {
                mode,
                source: self.instantiate_goal_numeric_type(
                    check_context,
                    source,
                    signature,
                    regions,
                )?,
                destination: self.instantiate_goal_numeric_type(
                    check_context,
                    destination,
                    signature,
                    regions,
                )?,
            },
            GoalOperation::Reinterpret {
                source,
                destination,
            } => GoalOperation::Reinterpret {
                source: self.instantiate_goal_numeric_type(
                    check_context,
                    source,
                    signature,
                    regions,
                )?,
                destination: self.instantiate_goal_numeric_type(
                    check_context,
                    destination,
                    signature,
                    regions,
                )?,
            },
            GoalOperation::Boolean(operation) => GoalOperation::Boolean(operation),
            GoalOperation::ValueEquality {
                equal,
                operand_type,
            } => GoalOperation::ValueEquality {
                equal,
                operand_type: self.instantiate_goal_type(
                    check_context,
                    operand_type,
                    signature,
                    regions,
                )?,
            },
            GoalOperation::ArrayMeasure {
                measure,
                element,
                length,
            } => GoalOperation::ArrayMeasure {
                measure,
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
                length: self.types.instantiate_goal_const(length, signature)?,
            },
            GoalOperation::ArrayIndex { element, length } => GoalOperation::ArrayIndex {
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
                length: self.types.instantiate_goal_const(length, signature)?,
            },
            GoalOperation::BufferMeasure { measure, element } => GoalOperation::BufferMeasure {
                measure,
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
            },
            GoalOperation::BufferIndex { element } => GoalOperation::BufferIndex {
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
            },
            GoalOperation::ContainerMeasure {
                measure,
                measured,
                element,
                constant,
            } => GoalOperation::ContainerMeasure {
                measure,
                measured,
                element: element
                    .map(|element| {
                        self.instantiate_goal_element(check_context, element, signature, regions)
                    })
                    .transpose()?,
                constant: constant
                    .map(|constant| self.types.instantiate_goal_const(constant, signature))
                    .transpose()?,
            },
            GoalOperation::RunIndex {
                measured,
                element,
                constant,
            } => GoalOperation::RunIndex {
                measured,
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
                constant: constant
                    .map(|constant| self.types.instantiate_goal_const(constant, signature))
                    .transpose()?,
            },
        })
    }

    pub(super) fn instantiate_goal_numeric_type(
        &mut self,
        check_context: &CheckContext<'_>,
        ty: CheckedNumericType,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<CheckedNumericType, CheckStop> {
        CheckedNumericType::from_type(self.instantiate_goal_type(
            check_context,
            ty.ty(),
            signature,
            regions,
        )?)
        .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    pub(super) fn instantiate_goal_type(
        &mut self,
        check_context: &CheckContext<'_>,
        ty: CheckedType,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<CheckedType, CheckStop> {
        Ok(match ty {
            CheckedType::Generic(declaration)
            | CheckedType::GenericInt(declaration)
            | CheckedType::GenericFloat(declaration) => signature
                .substitution
                .type_argument(declaration)
                .unwrap_or(ty),
            CheckedType::Array { element, length } => CheckedType::Array {
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
                length: self.types.instantiate_goal_const(length, signature)?,
            },
            CheckedType::Segments { element } => CheckedType::Segments {
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
            },
            CheckedType::Buffer { element } => CheckedType::Buffer {
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
            },
            CheckedType::Window {
                shape,
                element,
                capacity,
            } => CheckedType::Window {
                shape,
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
                capacity: capacity
                    .map(|capacity| self.types.instantiate_goal_const(capacity, signature))
                    .transpose()?,
            },
            CheckedType::Entries { element } => CheckedType::Entries {
                element: self.instantiate_goal_element(
                    check_context,
                    element,
                    signature,
                    regions,
                )?,
            },
            CheckedType::Nominal(id) => {
                self.instantiate_goal_nominal(check_context, id, signature, regions)?
            }
            CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_)
            | CheckedType::KeySet => ty,
        })
    }
}
