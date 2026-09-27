use crate::{DeclarationId, NodePath};

use super::model::{
    BindingId, CheckedBooleanOperation, CheckedConst, CheckedConversionMode, CheckedElement,
    CheckedFloatOperation, CheckedIntegerOperation, CheckedMeasure, CheckedNumericType,
    CheckedType, CheckedValue, FunctionId, MeasuredKind,
};

/// One function requirement, split into predicate and occurrence identity.
///
/// The requires-clause path belongs to diagnostics and retained metadata. It is
/// deliberately outside [`GoalTemplate`]'s equality, so two requirements with
/// the same alpha-expanded typed predicate compare equal even when their
/// source occurrences differ.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRequirement {
    pub(crate) template: GoalTemplate,
    /// Exact `requires_clause` occurrence. It is diagnostic/provenance
    /// identity only and never an executable trap record.
    pub(crate) clause: NodePath,
}

/// The finite typed predicate carried by one [FN-8] callable boundary.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GoalTemplate {
    pub(crate) root: GoalExpression,
}

impl GoalTemplate {
    pub(crate) fn new(root: GoalExpression) -> Self {
        Self { root }
    }
}

/// One fully substituted goal at a concrete call occurrence.
///
/// This wrapper prevents a caller from accidentally treating a formal-bearing
/// template as a caller-state predicate. Construction succeeds only after
/// every formal datum has been replaced by its pre-transfer actual image.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ConcreteGoal {
    pub(crate) root: GoalExpression,
}

impl ConcreteGoal {
    pub(crate) fn new(root: GoalExpression) -> Self {
        Self { root }
    }
}

/// The concrete predicate and source occurrence retained for one user call.
///
/// The callee instance is the surrounding `CheckedExpression::UserCall`
/// function id. The requires-clause path is occurrence/provenance identity only;
/// it is deliberately absent from `ConcreteGoal` equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedCallRequirement {
    pub(crate) requires_clause: NodePath,
    pub(crate) goal: ConcreteGoal,
}

/// One node of an alpha-expanded requirement predicate.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GoalExpression {
    Datum(GoalDatum),
    Operation {
        row: GoalOperation,
        /// Written operation type arguments after generic substitution.
        type_arguments: Vec<CheckedType>,
        /// Written operation const arguments after generic substitution.
        const_arguments: Vec<CheckedConst>,
        result: CheckedType,
        arguments: Vec<GoalExpression>,
    },
}

impl GoalExpression {
    pub(crate) const fn ty(&self) -> CheckedType {
        match self {
            Self::Datum(datum) => datum.ty(),
            Self::Operation { result, .. } => *result,
        }
    }

    pub(crate) fn with_projection(
        mut self,
        projection: GoalProjection,
        ty: CheckedType,
    ) -> Option<Self> {
        let Self::Datum(datum) = &mut self else {
            return None;
        };
        datum.projections_mut().push(projection);
        datum.set_ty(ty);
        Some(self)
    }
}

/// One typed leaf in a requirement template or concrete proof goal.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GoalDatum {
    /// A formal is identified by position, not source spelling or declaration.
    Parameter {
        ordinal: u32,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
    /// Named constants retain declaration identity instead of collapsing to
    /// their evaluated value.
    NamedConst {
        declaration: DeclarationId,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
    /// One caller place after lexical resolution. Borrow and reborrow actuals
    /// use their ultimate referent root rather than the temporary holder.
    Place {
        root: BindingId,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
    /// One already-evaluated value that source cannot safely name again.
    ///
    /// The value is scoped to its exact source occurrence and contributes no
    /// place support: it is the immutable mathematical result already
    /// produced at that point, not permission to reread the source expression.
    EvaluatedValue {
        function: FunctionId,
        occurrence: EvaluatedValueOccurrence,
        captured_type: CheckedType,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
    /// Literals retain their exact checked type and mathematical/nominal value.
    Literal(CheckedValue),
}

impl GoalDatum {
    pub(crate) const fn ty(&self) -> CheckedType {
        match self {
            Self::Parameter { ty, .. }
            | Self::NamedConst { ty, .. }
            | Self::Place { ty, .. }
            | Self::EvaluatedValue { ty, .. } => *ty,
            Self::Literal(value) => value.ty(),
        }
    }

    fn projections_mut(&mut self) -> &mut Vec<GoalProjection> {
        match self {
            Self::Parameter { projections, .. }
            | Self::NamedConst { projections, .. }
            | Self::Place { projections, .. }
            | Self::EvaluatedValue { projections, .. } => projections,
            Self::Literal(_) => unreachable!("a literal cannot carry a place projection"),
        }
    }

    fn set_ty(&mut self, new_ty: CheckedType) {
        match self {
            Self::Parameter { ty, .. }
            | Self::NamedConst { ty, .. }
            | Self::Place { ty, .. }
            | Self::EvaluatedValue { ty, .. } => *ty = new_ty,
            Self::Literal(_) => unreachable!("a literal cannot carry a place projection"),
        }
    }
}

/// Structural identity for an already-evaluated, occurrence-local value.
///
/// Call actuals remain distinguishable from proof-obligation operands so
/// FN-8's bind-first diagnostic cannot be selected for OP-2 or OP-9.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum EvaluatedValueOccurrence {
    CallArgument { call: NodePath, argument: u32 },
    ObligationOperand { site: NodePath, operand: u32 },
}

/// Ordered projection identity within one formal or named-constant datum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GoalProjection {
    Deref,
    Field(u32),
    /// One refined enum payload, preserving both the variant and field
    /// identity when a reference actual is substituted [REF-1, ENT-2].
    Payload {
        variant: u32,
        field: u32,
    },
    /// One [OP-4] subscript of the base reached so far, which [MSR-1] admits
    /// in a measure place so that `table[i].len` is a term. The offset is a
    /// logical one, its captured value is immutable once the place is formed
    /// [REF-1], and the obligation it owes is discharged there [MSR-4].
    Subscript(super::places::CapturedValue),
    /// One [REF-4] range step of the base reached so far, both endpoints
    /// captured where the range was formed [REF-1].
    ///
    /// It occurs in exactly one position: the image of a `&[T]` actual that
    /// formed its range at the call and therefore names no binding. A range
    /// reference a binding names carries no step of its own, that binding
    /// being the [ENT-2] measure place `deref(view)`; this projection is what
    /// keeps an anonymous range distinct from the storage it was formed over,
    /// whose `len` is a different quantity [MSR-1].
    Range(super::places::CapturedRange),
    /// One [OP-4] subscript of an [ENT-2] clause (b) place inside a
    /// `contract_block`, whose written offset is a value parameter of the same
    /// callable.
    ///
    /// [ENT-2] admits an offset that is "a written integer literal, a live
    /// `own` fragment-integer tracked place, or an in-scope const generic", and in a
    /// declaration-boundary template a parameter is named by its ordinal and
    /// not by any binding: the caller substitutes its own actual for it, as
    /// [EFF-5] already substitutes a row's index positions. It occurs only
    /// under [`GoalDatum::Parameter`], and both readers -- the caller's
    /// instantiation and the callee body's own reading -- replace it with an
    /// ordinary [`GoalProjection::Subscript`] before any term is interned.
    FormalSubscript {
        ordinal: u32,
    },
}

impl GoalProjection {
    /// The complete storage projection used by ENT-2 terms and ENT-5 kills.
    /// A formal subscript must be instantiated before it can prove separation.
    pub(crate) fn place_step(self) -> super::places::PlaceStep {
        use super::places::{CapturedValue, PlaceStep};
        match self {
            Self::Deref => PlaceStep::Deref,
            Self::Field(field) => PlaceStep::Field(field),
            Self::Payload { variant, field } => PlaceStep::Payload { variant, field },
            Self::Subscript(offset) => PlaceStep::Index(offset),
            Self::Range(range) => PlaceStep::Range(range),
            Self::FormalSubscript { .. } => PlaceStep::Index(CapturedValue::unknown()),
        }
    }
}

/// One structural goal row and its exact selected type/domain identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GoalOperation {
    Integer {
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
    },
    Float {
        operation: CheckedFloatOperation,
        operand_type: CheckedType,
    },
    NumericConversion {
        mode: CheckedConversionMode,
        source: CheckedNumericType,
        destination: CheckedNumericType,
    },
    Reinterpret {
        source: CheckedNumericType,
        destination: CheckedNumericType,
    },
    Boolean(CheckedBooleanOperation),
    EnumEquality {
        equal: bool,
        operand_type: CheckedType,
    },
    ArrayMeasure {
        measure: CheckedMeasure,
        element: CheckedElement,
        length: CheckedConst,
    },
    /// One array element value whose own OP-4 obligation has already been
    /// discharged before this expression is used as a proof operand.
    ArrayIndex {
        element: CheckedElement,
        length: CheckedConst,
    },
    BufferMeasure {
        measure: CheckedMeasure,
        element: CheckedElement,
    },
    /// One buffer element value whose own OP-4 obligation has already been
    /// discharged before this expression is used as a proof operand.
    BufferIndex {
        element: CheckedElement,
    },
    /// Canonical total allocation-domain predicate [OP-9]. The ceiling is
    /// part of the row identity so a proof cannot be reused across a layout
    /// rule change or across distinct element representations.
    BufferFits {
        element: CheckedType,
        maximum_length: u64,
    },
    /// One [MSR-1] measure of a storage shape [TYPE-9]. The measured kind is
    /// part of the row identity because the measure table gives each its own
    /// row, and the written constant is what a fixed-capacity row carries
    /// [MSR-2].
    ContainerMeasure {
        measure: CheckedMeasure,
        measured: MeasuredKind,
        /// The element type of the measured storage shape.
        element: Option<CheckedElement>,
        /// An `Array` length or constant window capacity; a runtime-capacity
        /// shape stores the corresponding measure and has none here.
        constant: Option<CheckedConst>,
    },
    /// One run element value whose own [OP-4] obligation has already been
    /// discharged before this expression is used as a proof operand.
    RunIndex {
        measured: MeasuredKind,
        element: CheckedElement,
        constant: Option<CheckedConst>,
    },
}
