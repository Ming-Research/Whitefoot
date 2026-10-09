//! Select FN-8 goals before ordinary substitution loses projected types or
//! replaces an actual's unrepresented subscript with an opaque capture.
use super::super::super::goal::{GoalDatum, GoalExpression, GoalOperation, GoalProjection};
use super::*;

pub(super) struct Terms<'a> {
    parameters: &'a [CheckedParameter],
    arguments: &'a [CheckedExpression],
    nominals: &'a [CheckedNominal],
    elements: &'a [CheckedType],
    places: &'a PlaceMap,
}

impl<'a> Terms<'a> {
    pub(super) fn new(
        parameters: &'a [CheckedParameter],
        arguments: &'a [CheckedExpression],
        nominals: &'a [CheckedNominal],
        elements: &'a [CheckedType],
        places: &'a PlaceMap,
    ) -> Self {
        Self {
            parameters,
            arguments,
            nominals,
            elements,
            places,
        }
    }

    pub(super) fn goal(&self, goal: &GoalExpression) -> bool {
        use CheckedIntegerOperation as Op;
        let GoalExpression::Operation { row, arguments, .. } = goal else {
            return false;
        };
        match (row, arguments.as_slice()) {
            (GoalOperation::Boolean(CheckedBooleanOperation::And), [left, right]) => {
                self.goal(left) && self.goal(right)
            }
            (
                GoalOperation::Integer {
                    operation:
                        Op::Equal
                        | Op::NotEqual
                        | Op::Less
                        | Op::LessEqual
                        | Op::Greater
                        | Op::GreaterEqual,
                    ..
                },
                [left, right],
            ) => self.term(left) && self.term(right),
            _ => false,
        }
    }

    fn constant(&self, value: &GoalExpression) -> bool {
        match value {
            GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer { .. })) => true,
            GoalExpression::Datum(GoalDatum::NamedConst {
                projections,
                ty: CheckedType::Integer(_),
                ..
            }) => projections.is_empty(),
            GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ..
            }) if projections.is_empty() => {
                self.arguments.get(*ordinal as usize).is_some_and(constant)
            }
            _ => false,
        }
    }

    fn term(&self, value: &GoalExpression) -> bool {
        use CheckedIntegerOperation as Op;
        if !matches!(
            value.ty(),
            CheckedType::Integer(_) | CheckedType::GenericInt(_)
        ) {
            return false;
        }
        match value {
            GoalExpression::Datum(GoalDatum::Literal(_)) => true,
            GoalExpression::Datum(GoalDatum::NamedConst { projections, .. }) => {
                projections.is_empty()
            }
            GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ..
            }) => {
                if projections.is_empty() {
                    self.arguments.get(*ordinal as usize).is_some_and(term)
                } else {
                    // An integer field/dereference is a range term only below
                    // an element. A substituted bare own scalar remains one.
                    self.place(value)
                        .is_some_and(|place| place.element || place.scalar)
                }
            }
            GoalExpression::Datum(_) => false,
            GoalExpression::Operation { row, arguments, .. } => match (row, arguments.as_slice()) {
                (
                    GoalOperation::Integer {
                        operation: Op::AddExact | Op::SubtractExact,
                        ..
                    },
                    [left, right],
                ) => self.term(left) && self.term(right),
                (
                    GoalOperation::Integer {
                        operation: Op::MultiplyExact,
                        ..
                    },
                    [left, right],
                ) => {
                    self.term(left)
                        && self.term(right)
                        && (self.constant(left) || self.constant(right))
                }
                (
                    GoalOperation::ArrayMeasure { measure, .. }
                    | GoalOperation::BufferMeasure { measure, .. }
                    | GoalOperation::ContainerMeasure { measure, .. },
                    [place],
                ) => *measure != CheckedMeasure::Head && self.place(place).is_some(),
                (
                    GoalOperation::ArrayIndex { .. } | GoalOperation::BufferIndex { .. },
                    [place, index],
                ) => self.place(place).is_some() && self.term(index),
                (GoalOperation::RunIndex { measured, .. }, [place, index]) => {
                    !matches!(measured, MeasuredKind::Entries | MeasuredKind::KeySet)
                        && self.place(place).is_some_and(|place| {
                            !matches!(
                                measured,
                                MeasuredKind::ConstantRing | MeasuredKind::RuntimeRing
                            ) || place.element
                        })
                        && self.term(index)
                }
                _ => false,
            },
        }
    }

    fn place(&self, expression: &GoalExpression) -> Option<Place> {
        let GoalExpression::Datum(GoalDatum::Parameter {
            ordinal,
            projections,
            ..
        }) = expression
        else {
            return None;
        };
        let parameter = self.parameters.get(*ordinal as usize)?;
        let argument = self.arguments.get(*ordinal as usize)?;
        let mut place = actual_place(argument, self.places)?;
        let mut range_referent = parameter.mode.is_range();
        let mut ty = parameter.ty;
        // A reference formal's first dereference is replaced by the actual
        // place; it is not a Box projection of that place's referent.
        let projections = if parameter.mode.is_reference() {
            projections
                .strip_prefix(&[GoalProjection::Deref])
                .unwrap_or(projections)
        } else {
            projections
        };
        for projection in projections {
            place.scalar = false;
            ty = match *projection {
                GoalProjection::Deref => {
                    let CheckedType::Nominal(nominal) = ty else {
                        return None;
                    };
                    let CheckedNominalKind::Box { referent, .. } =
                        self.nominals.get(nominal.0 as usize)?.kind
                    else {
                        return None;
                    };
                    referent
                }
                GoalProjection::Field(field) => {
                    let CheckedType::Nominal(nominal) = ty else {
                        return None;
                    };
                    let CheckedNominalKind::Struct { fields } =
                        &self.nominals.get(nominal.0 as usize)?.kind
                    else {
                        return None;
                    };
                    fields.get(field as usize)?.ty
                }
                GoalProjection::Payload { variant, field } => {
                    if !place.element {
                        return None;
                    }
                    let CheckedType::Nominal(nominal) = ty else {
                        return None;
                    };
                    let CheckedNominalKind::Enum { variants } =
                        &self.nominals.get(nominal.0 as usize)?.kind
                    else {
                        return None;
                    };
                    variants
                        .iter()
                        .find(|item| item.tag == variant)?
                        .fields
                        .get(field as usize)?
                        .ty
                }
                GoalProjection::Subscript(_) | GoalProjection::FormalSubscript { .. } => {
                    if let GoalProjection::FormalSubscript { ordinal } = projection
                        && !term(self.arguments.get(*ordinal as usize)?)
                    {
                        return None;
                    }
                    // Original template captures are literals or consts;
                    // a parameter's source shape is checked before erasure.
                    let selected = if range_referent {
                        range_referent = false;
                        ty
                    } else {
                        if !element_base(ty, place.element) {
                            return None;
                        }
                        match ty {
                            CheckedType::Segments { element } => CheckedType::Buffer { element },
                            CheckedType::Array { element, .. }
                            | CheckedType::Buffer { element }
                            | CheckedType::Window { element, .. } => {
                                *self.elements.get(element.0 as usize)?
                            }
                            _ => return None,
                        }
                    };
                    place.element = true;
                    selected
                }
                GoalProjection::Range(_)
                | GoalProjection::Page(_)
                | GoalProjection::FormalPage { .. } => return None,
            };
        }
        Some(place)
    }
}

struct Place {
    element: bool,
    scalar: bool,
}

fn actual_place(expression: &CheckedExpression, places: &PlaceMap) -> Option<Place> {
    let mut scalar = false;
    let element = match expression {
        CheckedExpression::Binding { binding, .. } => {
            scalar = !places.is_reference(*binding);
            false
        }
        CheckedExpression::BorrowAddressed { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => {
            let PlaceRoot::Binding(binding) = root.root else {
                return None;
            };
            if !range_path(&root.path, false) {
                return None;
            }
            scalar = root.path.is_empty() && !places.is_reference(binding);
            root.path
                .iter()
                .any(|step| matches!(step, CheckedPlaceStep::Subscript(_)))
        }
        CheckedExpression::BorrowRangeIndex { place, .. }
        | CheckedExpression::RangeIndex { place, .. } => {
            if !term(&place.offset) || !range_path(&place.path, true) {
                return None;
            }
            true
        }
        CheckedExpression::ArrayIndex {
            root: CheckedArrayRoot::Binding { .. },
            offset,
            ..
        } => {
            if !term(offset) {
                return None;
            }
            true
        }
        CheckedExpression::BufferIndex { root, offset, .. } => {
            if !term(offset) || !range_path(&root.path, false) {
                return None;
            }
            true
        }
        CheckedExpression::ProjectValue { value, .. }
        | CheckedExpression::BoxDeref { value, .. } => actual_place(value, places)?.element,
        CheckedExpression::Project { .. } | CheckedExpression::DerefAddressed { .. } => false,
        _ => return None,
    };
    Some(Place { element, scalar })
}
