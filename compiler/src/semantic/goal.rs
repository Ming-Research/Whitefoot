use crate::{DeclarationId, NodePath};

use super::model::{
    BindingId, CheckedBooleanOperation, CheckedConst, CheckedConversionMode, CheckedElement,
    CheckedFloatOperation, CheckedIntegerOperation, CheckedMeasure, CheckedNumericType,
    CheckedPlaceStep, CheckedSetTarget, CheckedType, CheckedValue, FunctionId, MeasuredKind,
    SubscriptedTerm,
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
    /// [TYPE-11] the parameter ordinal a type invariant is taken over, which
    /// tells apart the requirements one `type_invariant` gives two
    /// parameters; `None` for a written `requires_clause`.
    pub(crate) subject: Option<u32>,
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
    /// [TYPE-11] the requirement's subject ordinal [`CheckedRequirement::subject`].
    pub(crate) subject: Option<u32>,
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
    /// The ordinary term selecting a contract position. No new snapshot is
    /// created: projected selectors retain storage and offset support.
    pub(crate) fn captured_offset(
        &self,
        constant_ids: &std::collections::HashMap<DeclarationId, super::model::CheckedConstantId>,
        constants: &[super::model::CheckedConstant],
    ) -> Option<super::places::CapturedValue> {
        use super::places::{CapturedTerm, CapturedValue, ResolvedPlace};
        if let Self::Operation { row, arguments, .. } = self {
            let measure = match row {
                GoalOperation::ArrayMeasure { measure, .. }
                | GoalOperation::BufferMeasure { measure, .. }
                | GoalOperation::ContainerMeasure { measure, .. }
                    if *measure != CheckedMeasure::Pages =>
                {
                    *measure
                }
                _ => return None,
            };
            let [base] = arguments.as_slice() else {
                return None;
            };
            if let Self::Datum(datum) = base {
                let admission = datum.admission()?;
                let projections = match datum {
                    GoalDatum::Place { projections, .. }
                    | GoalDatum::NamedConst { projections, .. } => projections,
                    _ => return None,
                };
                if admission.written.is_none()
                    && admission.root_type.is_none()
                    && projections.iter().any(|step| {
                        matches!(step, GoalProjection::Subscript(_) | GoalProjection::Page(_))
                    })
                {
                    return None;
                }
            }
            let selected = base.clone().with_projection(
                GoalProjection::Measure(measure),
                CheckedType::Integer(super::model::IntegerType::U64),
            )?;
            return selected.captured_offset(constant_ids, constants);
        }
        if let Self::Datum(datum) = self {
            let projections: &[GoalProjection] = match datum {
                GoalDatum::Place { projections, .. }
                | GoalDatum::NamedConst { projections, .. } => projections,
                _ => &[],
            };
            if let Some(GoalProjection::Measure(measure)) = projections.last() {
                if *measure == CheckedMeasure::Pages {
                    return None;
                }
                let admission = datum.admission()?;
                let indexed = |projections: &[GoalProjection]| {
                    projections.iter().any(|step| {
                        matches!(
                            step,
                            GoalProjection::Subscript(_)
                                | GoalProjection::Page(_)
                                | GoalProjection::FormalSubscript { .. }
                                | GoalProjection::FormalPage { .. }
                        )
                    })
                };
                let written_indexed = match admission.written.as_deref() {
                    Some(CheckedSetTarget::Storage(root)) => root
                        .path
                        .iter()
                        .any(|step| matches!(step, CheckedPlaceStep::Subscript(_))),
                    Some(CheckedSetTarget::RangeIndex(_)) => true,
                    Some(CheckedSetTarget::Place(_)) => false,
                    None if admission.root_type.is_some() => false,
                    None => indexed(projections),
                };
                if written_indexed || indexed(&admission.projections) {
                    return None;
                }
            }
        }
        let term = match self {
            Self::Datum(GoalDatum::Literal(CheckedValue::Integer { bits, .. })) => {
                CapturedTerm::Literal(*bits)
            }
            Self::Datum(GoalDatum::Literal(CheckedValue::ConstGeneric { declaration, .. })) => {
                CapturedTerm::Const(*declaration)
            }
            Self::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
                ..
            }) if matches!(ty, CheckedType::Integer(_)) => {
                let constant = *constant_ids.get(declaration)?;
                if projections.is_empty() {
                    let CheckedValue::Integer { bits, .. } =
                        &constants.get(constant.0 as usize)?.value
                    else {
                        return None;
                    };
                    CapturedTerm::Literal(*bits)
                } else {
                    if projections.iter().any(|projection| {
                        matches!(
                            projection,
                            GoalProjection::FormalSubscript { .. }
                                | GoalProjection::FormalPage { .. }
                        )
                    }) {
                        return None;
                    }
                    let place = ResolvedPlace {
                        root: super::places::PlaceRoot::Constant(constant),
                        path: projections.iter().map(GoalProjection::place_step).collect(),
                        atomic_aliases: Vec::new(),
                    };
                    CapturedTerm::Place(super::places::ProjectedSelector::new(
                        place.term_identity(),
                    ))
                }
            }
            Self::Datum(GoalDatum::Place {
                root,
                projections,
                ty,
                ..
            }) if matches!(ty, CheckedType::Integer(_)) => {
                if projections.iter().any(|projection| {
                    matches!(
                        projection,
                        GoalProjection::FormalSubscript { .. } | GoalProjection::FormalPage { .. }
                    )
                }) {
                    return None;
                }
                if projections.is_empty() {
                    CapturedTerm::Binding(*root)
                } else {
                    let mut place = ResolvedPlace::binding(*root);
                    place.path.extend(
                        projections
                            .iter()
                            .cloned()
                            .map(|projection| projection.place_step()),
                    );
                    CapturedTerm::Place(super::places::ProjectedSelector::new(
                        place.term_identity(),
                    ))
                }
            }
            _ => return None,
        };
        Some(CapturedValue::new(CapturedValue::unknown().capture, term).goal_identity())
    }

    /// The written-place annotations of one interned admission view. They
    /// do not change the exact typed predicate identity [ENT-2, FN-8].
    pub(crate) fn same_admission_view(&self, other: &Self) -> bool {
        if self != other {
            return false;
        }
        match (self, other) {
            (Self::Datum(left), Self::Datum(right)) => {
                match (left.admission(), right.admission()) {
                    (Some(left), Some(right)) => left.same_view(right),
                    (None, None) => true,
                    _ => false,
                }
            }
            (
                Self::Operation {
                    arguments: left, ..
                },
                Self::Operation {
                    arguments: right, ..
                },
            ) => left
                .iter()
                .zip(right)
                .all(|(left, right)| left.same_admission_view(right)),
            _ => false,
        }
    }

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
        let prior_type = datum.ty();
        let projections = datum.projections_mut();
        // [ENT-2] extending a page referent selects storage below it; its
        // page step no longer identifies the borrow's captured extent.
        if let Some(GoalProjection::Page(offset)) = projections.last_mut() {
            *offset = offset.goal_identity();
        }
        projections.push(projection.clone());
        if let Some(admission) = datum.admission_mut() {
            if admission.written.is_none() && admission.root_type.is_none() {
                admission.root_type = Some(prior_type);
            }
            admission.projections.push(projection);
        }
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
        admission: GoalAdmission,
    },
    /// One caller place after lexical resolution. Borrow and reborrow actuals
    /// use their ultimate referent root rather than the temporary holder.
    Place {
        root: BindingId,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
        admission: GoalAdmission,
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
    pub(crate) const fn admission(&self) -> Option<&GoalAdmission> {
        match self {
            Self::Place { admission, .. } | Self::NamedConst { admission, .. } => Some(admission),
            _ => None,
        }
    }

    pub(crate) fn admission_mut(&mut self) -> Option<&mut GoalAdmission> {
        match self {
            Self::Place { admission, .. } | Self::NamedConst { admission, .. } => Some(admission),
            _ => None,
        }
    }

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

/// Written-place provenance for L0 admission at the complete projected
/// endpoint [ENT-2]. A default annotation denotes only its current admitted
/// endpoint; extending it records the prior type and every appended step.
/// This annotation is outside exact goal identity [FN-8]; admission views
/// compare its contents separately before sharing numeric metadata.
#[derive(Clone, Debug, Default)]
pub(crate) struct GoalAdmission {
    pub(crate) written: Option<Box<CheckedSetTarget>>,
    pub(crate) root_type: Option<CheckedType>,
    pub(crate) root_is_range: bool,
    pub(crate) projections: Vec<GoalProjection>,
}

impl GoalAdmission {
    pub(crate) fn written(target: CheckedSetTarget) -> Self {
        Self {
            written: Some(Box::new(target)),
            ..Self::default()
        }
    }

    pub(crate) fn plain(ty: CheckedType) -> Self {
        Self {
            root_type: Some(ty),
            ..Self::default()
        }
    }

    pub(crate) fn plain_range(ty: CheckedType) -> Self {
        Self {
            root_is_range: true,
            ..Self::plain(ty)
        }
    }

    fn same_view(&self, other: &Self) -> bool {
        self.root_type == other.root_type
            && self.root_is_range == other.root_is_range
            && self.written_view() == other.written_view()
            && self
                .projections
                .iter()
                .cloned()
                .map(admission_projection)
                .eq(other.projections.iter().cloned().map(admission_projection))
    }

    fn written_view(&self) -> Option<(CheckedType, Vec<GoalProjection>, Option<SubscriptedTerm>)> {
        let target = self.written.as_deref()?;
        let (path, offsets) = match target {
            CheckedSetTarget::Place(_) => (Vec::new(), None),
            CheckedSetTarget::Storage(place) => (
                place
                    .path
                    .iter()
                    .map(CheckedPlaceStep::goal_projection)
                    .collect(),
                place.subscripted_term(),
            ),
            CheckedSetTarget::RangeIndex(place) => {
                (place.goal_projections(), place.subscripted_term())
            }
        };
        // Source occurrence paths and capture sites are diagnostic identity,
        // not a different admission view of the same typed predicate [ENT-2].
        let indexed = matches!(target, CheckedSetTarget::RangeIndex(_))
            || path
                .iter()
                .any(|step| matches!(step, GoalProjection::Subscript(_)));
        Some((
            target.ty(),
            if indexed {
                path.into_iter().map(admission_projection).collect()
            } else {
                Vec::new()
            },
            indexed.then_some(offsets).flatten(),
        ))
    }
}

fn admission_projection(projection: GoalProjection) -> GoalProjection {
    match projection {
        GoalProjection::Subscript(offset) => GoalProjection::Subscript(offset.goal_identity()),
        GoalProjection::Page(offset) => GoalProjection::Page(offset.goal_identity()),
        other => other,
    }
}

impl PartialEq for GoalAdmission {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for GoalAdmission {}

impl std::hash::Hash for GoalAdmission {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

/// Structural identity for an already-evaluated, occurrence-local value.
///
/// Call actuals remain distinguishable from proof-obligation operands so
/// FN-8's bind-first diagnostic cannot be selected for an [ENT-6] obligation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum EvaluatedValueOccurrence {
    CallArgument { call: NodePath, argument: u32 },
    ObligationOperand { site: NodePath, operand: u32 },
}

/// Ordered projection identity within one formal or named-constant datum.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum GoalProjection {
    Deref,
    /// One declared readonly descriptor field [OP-15, MSR-2].
    Measure(CheckedMeasure),
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
    Page(super::places::CapturedValue),
    /// One [REF-4] range step of the base reached so far, both endpoints
    /// captured where the range was formed [REF-1].
    ///
    /// It occurs in exactly one position: the image of a `&[T]` actual that
    /// formed its range at the call and therefore names no binding. A range
    /// reference a binding names carries no step of its own, that binding
    /// being the [ENT-2] measure place `view^`; this projection is what
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
        projections: Vec<GoalProjection>,
    },
    /// A page selection whose offset names a value parameter, instantiated
    /// like `FormalSubscript` while retaining the distinct [REF-4] page path.
    FormalPage {
        ordinal: u32,
        projections: Vec<GoalProjection>,
    },
}

impl GoalProjection {
    /// Convert an exact proof-path step without widening value identity.
    pub(crate) fn from_place_step(step: &super::places::PlaceStep) -> Option<Self> {
        use super::places::PlaceStep;
        match step {
            PlaceStep::Deref => Some(GoalProjection::Deref),
            PlaceStep::Measure(measure) => Some(GoalProjection::Measure(*measure)),
            PlaceStep::Field(field) => Some(GoalProjection::Field(*field)),
            PlaceStep::Payload { variant, field } => Some(GoalProjection::Payload {
                variant: *variant,
                field: *field,
            }),
            PlaceStep::Index(offset) => Some(GoalProjection::Subscript(offset.goal_identity())),
            // [ENT-2] retain the supplied page identity: borrowed pages carry
            // their formation capture; direct selectors are already canonical.
            PlaceStep::Page(offset) => Some(GoalProjection::Page(offset.clone())),
            // [REF-4] a range step's endpoint captures identify the formation
            // whose immutable affine image gives the anonymous range its length.
            // Canonicalizing those captures by endpoint spelling would merge two
            // formations that read the same binding at different times.
            PlaceStep::Range(range) => Some(GoalProjection::Range(range.clone())),
            PlaceStep::Part(_) | PlaceStep::Descendant(_) => None,
        }
    }

    /// The complete storage projection used by ENT-2 terms and ENT-5 kills.
    /// A formal subscript must be instantiated before it can prove separation.
    pub(crate) fn place_step(&self) -> super::places::PlaceStep {
        use super::places::{CapturedValue, PlaceStep};
        match self.clone() {
            Self::Deref => PlaceStep::Deref,
            Self::Measure(measure) => PlaceStep::Measure(measure),
            Self::Field(field) => PlaceStep::Field(field),
            Self::Payload { variant, field } => PlaceStep::Payload { variant, field },
            Self::Subscript(offset) => PlaceStep::Index(offset),
            Self::Page(offset) => PlaceStep::Page(offset),
            Self::Range(range) => PlaceStep::Range(range),
            Self::FormalSubscript { .. } => PlaceStep::Index(CapturedValue::unknown()),
            Self::FormalPage { .. } => PlaceStep::Page(CapturedValue::unknown()),
        }
    }
}

/// [ENT-2] whether one row is a widening conversion: a bare `cvt::<S, D>`
/// with integer S and D whose pair is whole-type total [OP-6]. Such a
/// conversion denotes its operand's mathematical value, so a relation term
/// or a comparison-origin operand over one is that operand itself.
pub(crate) fn widening_integer_conversion(row: &GoalOperation) -> bool {
    matches!(
        row,
        GoalOperation::NumericConversion {
            mode: CheckedConversionMode::Exact,
            source: source @ (CheckedNumericType::Integer(_) | CheckedNumericType::GenericInteger(_)),
            destination: destination @ (CheckedNumericType::Integer(_) | CheckedNumericType::GenericInteger(_)),
        } if source.converts_totally_to(*destination)
    )
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
    ValueEquality {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::model::IntegerType;
    use crate::semantic::places::{CaptureId, CapturedTerm, CapturedValue};

    #[test]
    fn range_of_arrays_keeps_run_kind_when_appending_element_projection() {
        let array = CheckedType::Array {
            element: CheckedElement(0),
            length: CheckedConst::Value(1),
        };
        let offset = GoalProjection::Subscript(CapturedValue::new(
            CaptureId::source(0),
            CapturedTerm::Literal(0),
        ));
        let base = |admission| {
            GoalExpression::Datum(GoalDatum::Place {
                root: BindingId(0),
                projections: Vec::new(),
                ty: array,
                admission,
            })
        };
        let range = base(GoalAdmission::plain_range(array))
            .with_projection(offset.clone(), array)
            .unwrap();
        let own = base(GoalAdmission::default())
            .with_projection(offset.clone(), CheckedType::Integer(IntegerType::U8))
            .unwrap();
        let GoalExpression::Datum(range) = range else {
            unreachable!()
        };
        let GoalExpression::Datum(own) = own else {
            unreachable!()
        };
        let range = range.admission().unwrap();
        let own = own.admission().unwrap();
        assert_eq!(range.root_type, Some(array));
        assert!(range.root_is_range);
        assert_eq!(range.projections, vec![offset.clone()]);
        assert_eq!(own.root_type, Some(array));
        assert!(!own.root_is_range);
        assert_eq!(own.projections, vec![offset.clone()]);
        assert!(!range.same_view(own));
    }
}
