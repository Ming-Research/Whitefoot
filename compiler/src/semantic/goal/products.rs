//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{record_enum, record_struct};

record_struct!(CheckedRequirement {
    template,
    clause,
    subject
});

record_struct!(GoalTemplate { root });

record_struct!(ConcreteGoal { root });

record_struct!(CheckedCallRequirement {
    requires_clause,
    subject,
    goal
});

record_enum!(GoalExpression {
    0 => Datum(f0),
    1 => Operation { row, type_arguments, const_arguments, result, arguments },
});

record_enum!(GoalDatum {
    0 => Parameter { ordinal, projections, ty },
    1 => NamedConst { declaration, projections, ty },
    2 => Place { root, projections, ty },
    3 => EvaluatedValue { function, occurrence, captured_type, projections, ty },
    4 => Literal(f0),
});

record_enum!(EvaluatedValueOccurrence {
    0 => CallArgument { call, argument },
    1 => ObligationOperand { site, operand },
});

record_enum!(GoalProjection {
    0 => Deref,
    1 => Field(f0),
    2 => Payload { variant, field },
    3 => Subscript(f0),
    4 => Range(f0),
    5 => FormalSubscript { ordinal },
});

record_enum!(GoalOperation {
    0 => Integer { operation, operand_type },
    1 => Float { operation, operand_type },
    2 => NumericConversion { mode, source, destination },
    3 => Reinterpret { source, destination },
    4 => Boolean(f0),
    5 => EnumEquality { equal, operand_type },
    6 => ArrayMeasure { measure, element, length },
    7 => ArrayIndex { element, length },
    8 => BufferMeasure { measure, element },
    9 => BufferIndex { element },
    10 => BufferFits { element, maximum_length },
    11 => ContainerMeasure { measure, measured, element, constant },
    12 => RunIndex { measured, element, constant },
});
