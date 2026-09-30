//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{
    IdentityKind, Reader, Record, Writer, record_enum, record_struct, record_tuple,
};

impl Record for FunctionId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Function, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        reader.identity(IdentityKind::Function).map(Self)
    }
}

record_tuple!(BindingId, 0);

impl Record for ContractQueryId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::ContractQuery, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        reader.identity(IdentityKind::ContractQuery).map(Self)
    }
}

record_enum!(CheckedMode {
    0 => Own,
    1 => Reference,
    2 => Range,
});

record_tuple!(CheckedLoopId, 0);

// Affine syntax can be deeply grouped. Use the model's iterative postorder
// rather than adding a recursive serialization walk beside its iterative
// clone/drop operations.
impl Record for CheckedAffineExpression {
    fn write(&self, writer: &mut Writer) {
        self.postorder().count().write(writer);
        for expression in self.postorder() {
            expression.node_path.write(writer);
            match &expression.kind {
                CheckedAffineExpressionKind::Constant { value, ty } => {
                    0_u8.write(writer);
                    value.write(writer);
                    ty.write(writer);
                }
                CheckedAffineExpressionKind::Local { binding, ty } => {
                    1_u8.write(writer);
                    binding.write(writer);
                    ty.write(writer);
                }
                CheckedAffineExpressionKind::ConstGeneric {
                    declaration,
                    ty,
                    name,
                } => {
                    2_u8.write(writer);
                    declaration.write(writer);
                    ty.write(writer);
                    name.write(writer);
                }
                CheckedAffineExpressionKind::Measure(value) => {
                    3_u8.write(writer);
                    value.write(writer);
                }
                CheckedAffineExpressionKind::Add(_, _) => 4_u8.write(writer),
                CheckedAffineExpressionKind::Subtract(_, _) => 5_u8.write(writer),
                CheckedAffineExpressionKind::MultiplyByConstant {
                    constant,
                    constant_ty,
                    value: _,
                } => {
                    6_u8.write(writer);
                    constant.write(writer);
                    constant_ty.write(writer);
                }
            }
        }
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let count = usize::read(reader)?;
        let mut values = Vec::new();
        for _ in 0..count {
            let node_path = Record::read(reader)?;
            let tag = u8::read(reader)?;
            let kind = match tag {
                0 => CheckedAffineExpressionKind::Constant {
                    value: Record::read(reader)?,
                    ty: Record::read(reader)?,
                },
                1 => CheckedAffineExpressionKind::Local {
                    binding: Record::read(reader)?,
                    ty: Record::read(reader)?,
                },
                2 => CheckedAffineExpressionKind::ConstGeneric {
                    declaration: Record::read(reader)?,
                    ty: Record::read(reader)?,
                    name: Record::read(reader)?,
                },
                3 => CheckedAffineExpressionKind::Measure(Record::read(reader)?),
                4 | 5 => {
                    let right = Box::new(values.pop()?);
                    let left = Box::new(values.pop()?);
                    if tag == 4 {
                        CheckedAffineExpressionKind::Add(left, right)
                    } else {
                        CheckedAffineExpressionKind::Subtract(left, right)
                    }
                }
                6 => CheckedAffineExpressionKind::MultiplyByConstant {
                    constant: Record::read(reader)?,
                    constant_ty: Record::read(reader)?,
                    value: Box::new(values.pop()?),
                },
                _ => return None,
            };
            values.push(Self { node_path, kind });
        }
        if values.len() != 1 {
            return None;
        }
        values.pop()
    }
}

record_struct!(CheckedAffineRelation {
    node_path,
    left,
    right,
    bound,
    equality
});

record_struct!(CheckedLoopInvariant {
    loop_id,
    declaration,
    name,
    relation
});

record_struct!(CheckedProofUse {
    node_path,
    multiplicity,
    source
});

record_enum!(CheckedProofMultiplicity {
    0 => Literal(f0),
    1 => Value { binding, ty },
});

record_enum!(CheckedProofUseSource {
    0 => Named(f0),
    1 => Relation(f0),
});

record_struct!(CheckedSourceProof {
    node_path,
    declaration,
    name,
    target,
    uses
});

record_enum!(ValueInitializerKind {
    0 => ValueIf,
    1 => ValueMatch,
});

impl Record for NominalId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Nominal, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        reader.identity(IdentityKind::Nominal).map(Self)
    }
}

impl Record for CheckedConstantId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Constant, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        reader.identity(IdentityKind::Constant).map(Self)
    }
}

impl Record for DerivedConstId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::DerivedConst, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        reader.identity(IdentityKind::DerivedConst).map(Self)
    }
}

record_enum!(ConstOperation {
    0 => Add,
    1 => Subtract,
    2 => Multiply,
    3 => Divide,
    4 => Remainder,
});

record_struct!(DerivedConst {
    operation,
    left,
    right
});

record_enum!(CheckedConst {
    0 => Value(f0),
    1 => Parameter(f0),
    2 => Derived(f0),
});

record_enum!(IntegerType {
    0 => I8,
    1 => I16,
    2 => I32,
    3 => I64,
    4 => U8,
    5 => U16,
    6 => U32,
    7 => U64,
});

record_enum!(FloatType {
    0 => F32,
    1 => F64,
});

record_enum!(CheckedConversionMode {
    0 => Exact,
    1 => Checked,
    2 => Defined,
    3 => Wrap,
    4 => Nearest,
});

record_enum!(CheckedNumericType {
    0 => Integer(f0),
    1 => Float(f0),
    2 => GenericInteger(f0),
    3 => GenericFloat(f0),
});

record_enum!(CheckedReleaseClass {
    0 => General,
});

impl Record for CheckedElement {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Element, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        reader.identity(IdentityKind::Element).map(Self)
    }
}

record_enum!(CheckedType {
    0 => Unit,
    1 => Bool,
    2 => Integer(f0),
    3 => Float(f0),
    4 => Generic(f0),
    5 => GenericInt(f0),
    6 => GenericFloat(f0),
    7 => Nominal(f0),
    8 => Array { element, length },
    9 => Buffer { element },
    10 => Window { shape, element, capacity },
    11 => Segments { element },
});

record_enum!(WindowShape {
    0 => Slots,
    1 => Ring,
});

record_enum!(CheckedMeasure {
    0 => Length,
    1 => Capacity,
    2 => Head,
});

record_enum!(MeasureCell {
    0 => ExactExtent,
    1 => ExactConstant(f0),
    2 => ExactTypeConstant,
    3 => ExactRuntime,
    4 => Bounded,
    5 => Absent,
});

record_enum!(MeasuredKind {
    0 => ConstantArray,
    1 => RuntimeArray,
    2 => ConstantSlots,
    3 => RuntimeSlots,
    4 => ConstantRing,
    5 => RuntimeRing,
    6 => Range,
    7 => Segments,
});

record_enum!(CheckedValue {
    0 => Unit,
    1 => Bool(f0),
    2 => Integer { ty, bits },
    3 => Float { ty, bits },
    4 => ConstGeneric { declaration, ty },
    5 => NumericIdentity { ty, one },
    6 => Array { ty, elements },
    7 => Struct { ty, fields },
});

record_struct!(CheckedConstant {
    id,
    declaration,
    name,
    declared_type,
    ty,
    value
});

record_struct!(CheckedField { name, ty, readonly });

record_struct!(CheckedVariant {
    name,
    constructor,
    tag,
    fields
});

record_enum!(CheckedConstructor {
    0 => Source(f0),
    1 => Prelude(f0),
});

record_enum!(CheckedNominalKind {
    0 => Struct { fields },
    1 => Enum { variants },
    2 => Box { referent, region, release },
    3 => Opaque,
    4 => Shared { state },
});

record_struct!(CheckedNominal {
    id,
    name,
    kind,
    linear,
    nocopy
});

record_enum!(CheckedIntegerOperation {
    0 => AddWrap,
    1 => SubtractWrap,
    2 => MultiplyWrap,
    3 => AddExact,
    4 => SubtractExact,
    5 => MultiplyExact,
    6 => AddDefined,
    7 => SubtractDefined,
    8 => MultiplyDefined,
    9 => AddChecked,
    10 => SubtractChecked,
    11 => MultiplyChecked,
    12 => DivideChecked,
    13 => RemainderChecked,
    14 => DivideExact,
    15 => RemainderExact,
    16 => DivideDefined,
    17 => RemainderDefined,
    18 => AbsoluteWrap,
    19 => AbsoluteExact,
    20 => AbsoluteDefined,
    21 => AbsoluteChecked,
    22 => NegateWrap,
    23 => NegateExact,
    24 => NegateDefined,
    25 => NegateChecked,
    26 => BitAnd,
    27 => BitOr,
    28 => BitXor,
    29 => BitNot,
    30 => ShiftLeftWrap,
    31 => ShiftRightWrap,
    32 => ShiftLeftExact,
    33 => ShiftRightExact,
    34 => ShiftLeftDefined,
    35 => ShiftRightDefined,
    36 => RotateLeft,
    37 => RotateRight,
    38 => PopulationCount,
    39 => LeadingZeros,
    40 => TrailingZeros,
    41 => ByteSwap,
    42 => MultiplyHigh,
    43 => AddSaturating,
    44 => SubtractSaturating,
    45 => MultiplySaturating,
    46 => Minimum,
    47 => Maximum,
    48 => Equal,
    49 => NotEqual,
    50 => Less,
    51 => LessEqual,
    52 => Greater,
    53 => GreaterEqual,
});

record_enum!(CheckedBooleanOperation {
    0 => And,
    1 => Or,
    2 => ExclusiveOr,
    3 => Not,
});

record_enum!(CheckedFloatOperation {
    0 => AddStrict,
    1 => SubtractStrict,
    2 => MultiplyStrict,
    3 => DivideStrict,
    4 => Equal,
    5 => Less,
    6 => LessEqual,
    7 => Greater,
    8 => GreaterEqual,
    9 => NotEqual,
    10 => Negate,
    11 => Absolute,
    12 => CopySign,
    13 => Minimum,
    14 => Maximum,
    15 => Floor,
    16 => Ceil,
    17 => Truncate,
    18 => RoundEven,
    19 => Remainder,
    20 => SquareRootStrict,
    21 => FusedMultiplyAddStrict,
    22 => Infinity,
    23 => Nan,
});

record_enum!(CheckedIntegerErrorClass {
    0 => Overflow,
    1 => DivError,
});

record_enum!(CheckedTargetDomainObligation {
    0 => ElementAddress,
});

record_enum!(CheckedLayoutMagnitude {
    0 => Finite(f0),
    1 => AboveU64,
});

record_struct!(CheckedLayoutCeiling {
    size,
    align,
    stride
});

record_struct!(CheckedAllocationFit {
    cell,
    element,
    layout_ceiling,
    count,
    source_length_upper_bound,
    site,
    count_site
});

record_enum!(CheckedArrayRoot {
    0 => Binding { binding, fields },
    1 => Constant(f0),
});

record_struct!(CheckedBufferRoot {
    binding,
    path,
    element,
    element_type
});

record_struct!(CheckedRangeRoot {
    binding,
    element,
    element_type
});

record_enum!(CheckedRangeSource {
    0 => Storage(f0),
    1 => Range(f0),
    2 => Element(f0),
});

record_struct!(CheckedRangeElementPlace {
    root,
    offset,
    captured,
    path,
    ty,
    obligation,
    target_domain
});

record_enum!(CheckedSegmentSelect {
    0 => One(f0),
    1 => All(f0),
});

record_struct!(CheckedSegmentIndex {
    offset,
    obligation,
    captured
});

record_struct!(CheckedContainerRoot { root, path, ty });

record_enum!(CheckedPlaceStep {
    0 => Field(f0),
    1 => BoxReferent(f0),
    2 => Subscript(f0),
});

record_struct!(CheckedPlaceSubscript {
    base_type,
    element_type,
    offset,
    obligation,
    target_domain,
    captured
});

record_enum!(SubscriptedTerm {
    0 => Represented,
    1 => Unrepresented,
});

record_enum!(CheckedIntegerArgumentSource {
    0 => TypedLiteral,
    1 => GenericNumericIdentity,
    2 => NamedConstant { declaration },
    3 => Other,
});

record_struct!(CheckedIntegerArgument { node_path, source });

record_struct!(CheckedResultBorrow {
    argument,
    root,
    path
});

record_enum!(CheckedExpression {
    0 => Constant(f0),
    1 => NamedConstant { declaration, value },
    2 => Binding { carrier, binding, ty, consume_root },
    3 => UserCall { function, tail_transfer, formal_effects, formal_contract, call, argument_nodes, arguments, actual_captures, goal_arguments, goal_regions, requirements, result, result_borrow, allocation },
    4 => IntegerOperation { carrier, operation, operand_type, argument_metadata, arguments, result },
    5 => FloatOperation { carrier, operation, operand_type, arguments },
    6 => NumericConversion { carrier, mode, source, destination, value, result },
    7 => Reinterpret { carrier, source, destination, value },
    8 => BooleanOperation { carrier, operation, arguments },
    9 => EnumEquality { carrier, equal, operand_type, arguments },
    10 => ArrayMeasure { measure, root, length },
    11 => ArrayIndex { carrier, root, element_type, length, offset, obligation, target_domain },
    12 => BoxTake { carrier, referent, binding, path, cleanup },
    13 => BufferMeasure { measure, root },
    14 => RangeOf { carrier, source, element, element_type, start, end, obligation, captured },
    15 => RangeMeasure { measure, root },
    16 => RangeElementMeasure { carrier, measure, place },
    17 => RangeIndex { carrier, place },
    18 => BorrowRangeIndex { carrier, place },
    19 => ContainerMeasure { measure, root },
    20 => ReadStorage { carrier, root },
    21 => BufferIndex { carrier, root, offset, obligation, target_domain },
    22 => BoxDeref { carrier, nominal, referent, value },
    23 => BorrowAddressed { carrier, root },
    24 => DerefAddressed { carrier, binding, ty },
    25 => ConstructStruct { carrier, nominal, fields, invariants, invariant_arguments },
    26 => ConstructEnum { carrier, nominal, variant, fields },
    27 => Project { carrier, binding, fields, ty, consume_root, residual_drops },
    28 => ProjectValue { carrier, value, nominal, field, ty },
    29 => BorrowSegment { carrier, root, segment, element, element_type },
});

record_enum!(CheckedEnumType {
    0 => Bool,
    1 => Nominal(f0),
});

record_struct!(CheckedMatchBinder {
    node_path,
    binding,
    field,
    mode,
    ty
});

record_struct!(CheckedMatchArm {
    tag,
    binders,
    covered,
    body,
    fallthrough_drops
});

record_struct!(CheckedDrop {
    binding,
    fields,
    ty
});

record_struct!(CheckedProjectedDrop { fields, ty });

record_enum!(CheckedOwnedTakeCleanup {
    0 => Drop { path, ty },
    1 => BoxShell { path, nominal, referent },
});

record_struct!(CheckedWritablePlace {
    binding,
    fields,
    mode,
    ty,
    declares
});

record_enum!(CheckedSetTarget {
    0 => Place(f0),
    1 => RangeIndex(f0),
    2 => Storage(f0),
});

record_struct!(PropagationContext {
    function,
    node_path
});

record_enum!(CheckedStatement {
    0 => Let { node_path, binding, value },
    1 => DestructuringLet { node_path, bindings, covered, nominal, value },
    2 => PropagateLet { node_path, binding, scrutinee, result_nominal, return_nominal, ok_type, error_type, error_drops, context },
    3 => Set { node_path, target, value, displaces_live_value },
    4 => Evaluate { node_path, value },
    5 => DropExpression { node_path, value, drops },
    6 => Proof(f0),
    7 => Return { node_path, value, drops },
    8 => Match { scrutinee, enum_type, arms, continues },
    9 => ValueMatchLet { node_path, kind, binding, result_type, result_mode, result_range_element, scrutinee, enum_type, arms, continues },
    10 => Give { node_path, value, drops },
    11 => Loop { id, invariants, body, backedge_drops },
    12 => CountedRange { id, node_path, binder, lower, upper, invariants, body, backedge_drops },
    13 => Break { node_path, target, drops },
    14 => Atomic { node_path, target, binding, state, guard, body, fallthrough_drops, continues, invariants },
});

record_struct!(CheckedParameter {
    name,
    declaration,
    node_path,
    binding,
    mode,
    ty,
    range_element
});

record_struct!(CheckedStatePath { root, steps });

record_enum!(CheckedEffectStep {
    0 => Field(f0),
    1 => Deref,
    2 => Payload { variant, field },
    3 => Index(f0),
    4 => Range { start, end },
    5 => Part(f0),
    6 => Measure(f0),
});

record_struct!(CheckedFunction {
    formal_hypothesis,
    id,
    declaration,
    module,
    name,
    symbol,
    function_actuals,
    region_parameters,
    parameters,
    result_mode,
    result,
    declared_state_writes,
    requirements,
    requirement_places,
    postconditions,
    body,
    reference_origins,
    body_disposition,
    allocates,
    call_separations,
    permission_separation_queries,
    waiting,
    obligations,
    entailment
});

record_struct!(CheckedWaiting {
    waits,
    calls,
    context_starts,
    context_awaits
});

record_struct!(CheckedContextAwait { statement, before });

record_struct!(CheckedCallSeparation {
    site,
    exchange,
    reference_use,
    positions,
    window,
    left_spelling,
    right_spelling,
    one_argument
});

record_struct!(CheckedReferencePreservationUse {
    site,
    binder,
    event
});

record_enum!(CheckedCallSeparationPositions {
    0 => Indices(f0, f1),
    1 => Ranges(f0, f1),
    2 => Live(f0),
    3 => IndexOutsideRange(f0, f1),
    4 => RangeWithinLength(f0),
});

record_enum!(CheckedBodyDisposition {
    0 => Inhabited,
    1 => Uninhabited { contradiction },
});

record_struct!(CheckedGenericRequirement {
    declaration,
    requirement
});

record_struct!(CheckedEffects {
    reads,
    writes,
    allocates
});

record_struct!(CheckedContractQuery {
    instance,
    site,
    premises,
    goal,
    proof
});

record_struct!(CheckedCallContract {
    requirements,
    requirement_queries,
    postconditions
});

record_struct!(CheckedBoundPostcondition {
    selector,
    relation,
    query,
    actual_premises
});
