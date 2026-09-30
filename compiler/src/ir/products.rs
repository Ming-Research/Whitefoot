//! Compiler-private typed records of lowered module fragments.

use super::*;
use crate::semantic::products::{
    IdentityKind, Reader, Record, Writer, record_enum, record_struct, record_tuple,
};

record_tuple!(IrValueId, 0);

record_tuple!(IrBlockId, 0);

impl Record for IrNominalId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Nominal, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(Self(reader.identity(IdentityKind::Nominal)?))
    }
}

impl Record for IrConstantId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Constant, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(Self(reader.identity(IdentityKind::Constant)?))
    }
}

impl Record for IrElement {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Element, self.0);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(Self(reader.identity(IdentityKind::Element)?))
    }
}

record_enum!(IrAddressed {
    0 => Unit,
    1 => Bool,
    2 => Integer { width, signed },
    3 => Float { width },
    4 => Nominal(f0),
    5 => Buffer { element },
    6 => Array { element, length },
    7 => Window { shape, element, capacity },
    8 => Segments { element },
});

record_enum!(IrReleaseClass {
    0 => General,
});

record_enum!(IrType {
    0 => Unit,
    1 => Bool,
    2 => Integer { width, signed },
    3 => Float { width },
    4 => Nominal(f0),
    5 => Address(f0),
    6 => Array { element, length },
    7 => Buffer { element },
    8 => Range { element },
    9 => RuntimeBoxPayload { nominal },
    10 => Window { shape, element, capacity },
    11 => Segments { element },
});

record_enum!(IrWindowShape {
    0 => Slots,
    1 => Ring,
});

record_struct!(IrField { ty });

record_struct!(IrVariant { tag, fields });

record_enum!(IrNominalKind {
    0 => Struct { fields },
    1 => Enum { variants },
    2 => Box { referent, release },
    3 => Opaque,
    4 => Shared { state },
});

record_struct!(IrNominal {
    name,
    link_name,
    stable,
    id,
    kind
});

record_enum!(IrEnumType {
    0 => Bool,
    1 => Nominal(f0),
});

record_enum!(IrIntegerOperation {
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

record_enum!(IrBooleanOperation {
    0 => And,
    1 => Or,
    2 => ExclusiveOr,
    3 => Not,
});

record_enum!(IrConversionMode {
    0 => Exact,
    1 => Checked,
    2 => Defined,
    3 => Wrap,
    4 => Nearest,
});

record_enum!(IrFloatOperation {
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

record_enum!(IrConstant {
    0 => Unit,
    1 => Bool(f0),
    2 => Integer { ty, bits },
    3 => Float { ty, bits },
});

record_enum!(IrGlobalValue {
    0 => Scalar(f0),
    1 => Array(f0),
    2 => Struct(f0),
});

record_struct!(IrGlobalConstant {
    id,
    name,
    link_name,
    ty,
    value
});

record_enum!(IrArrayRoot {
    0 => Value(f0),
    1 => Constant(f0),
});

record_enum!(IrTargetDomainObligation {
    0 => RuntimeSizedAllocation,
    1 => ElementAddress,
});

record_struct!(IrRuntimeTargetObligations {
    allocation,
    element_address,
    source_length_upper_bound,
    call_site_bound
});

record_enum!(IrLayoutMagnitude {
    0 => Finite(f0),
    1 => AboveU64,
});

record_struct!(IrAllocationObligations {
    layout_ceiling,
    target_domains
});

record_struct!(IrLayoutCeiling {
    size,
    align,
    stride
});

record_enum!(IrMeasure {
    0 => Length,
    1 => Capacity,
    2 => Head,
});

record_enum!(IrBoundary {
    0 => PlaceBack,
    1 => PlaceFront,
    2 => TakeBack,
    3 => TakeFront,
});

record_enum!(IrPlaceStep {
    0 => Field { nominal, field },
    1 => BoxReferent { nominal },
    2 => EnumVariant { nominal, variant, field },
    3 => RunElement { offset, target_domain },
    4 => ArrayElement { offset, target_domain },
    5 => BufferElement { offset, target_domain },
});

record_enum!(IrWorkEstimate {
    0 => Constant(f0),
    1 => Value(f0),
    2 => Length(f0),
    3 => BoxArrayLength(f0),
    4 => Sum(f0),
    5 => Product(f0, f1),
    6 => Difference(f0, f1),
    7 => Quotient(f0, f1),
});

record_enum!(IrOperation {
    0 => Constant(f0),
    1 => Call { function: Function, arguments },
    2 => Integer { operation, operand_type, arguments },
    3 => Float { operation, operand_type, arguments },
    4 => NumericConversion { mode, source_type, destination_type, value },
    5 => Reinterpret { source_type, destination_type, value },
    6 => Boolean { operation, arguments },
    7 => EnumEquality { equal, operand_type, arguments },
    8 => ArrayFill { value, target_domain },
    9 => FullArrayConversion { value },
    10 => ArrayIndex { root, offset, target_domain },
    11 => BufferFill { nominal, length, value, layout_ceiling, target_domains },
    12 => BufferMeasure { buffer },
    13 => Window,
    14 => ContainerMeasure { measure, container },
    15 => RunIndex { run, offset, target_domain },
    16 => RunBoundary { row, run, value },
    17 => RunTaken { row, run },
    18 => RunShift { run, index, open },
    19 => RunInsert { run, index, value },
    20 => RunTransfer { destination, source, index },
    21 => WindowBlockNew { nominal, capacity, obligations },
    22 => WindowGrow { nominal, cell, capacity, obligations },
    23 => CellFree { nominal, value },
    24 => BufferIndex { buffer, offset, target_domain },
    25 => BufferProbeSkip { buffer, index, limit, needles },
    26 => SliceFromBuffer { buffer },
    27 => SliceFromRun { run },
    28 => SliceRange { slice, start, end },
    29 => SliceMeasure { slice },
    30 => SliceIndex { slice, offset, target_domain },
    31 => SliceAddress { slice, offset, target_domain },
    32 => BoxNew { nominal, value },
    33 => BoxTake { nominal, value },
    34 => BoxDeref { nominal, value },
    35 => RuntimeBoxPayload { nominal, owner },
    36 => RuntimeBoxOwner { nominal, payload },
    37 => ConstructStruct { nominal, fields },
    38 => ConstructEnum { nominal, variant, fields },
    39 => ProjectStruct { aggregate, nominal, field, consume_root },
    40 => InsertStruct { aggregate, nominal, field, value },
    41 => ProjectVariant { aggregate, nominal, variant, field },
    42 => AddressOf { value, referent },
    43 => ConstantAddress { constant },
    44 => ProjectAddress { address, projection },
    45 => Load { address, referent },
    46 => LoopSplit { splitter: Function, chunk: Function, seed, lower, upper, captures, weight, work },
    47 => ContextStart { function: Function, arguments },
    48 => ContextJoin,
    49 => ContextStartBound { function: Function, arguments },
    50 => ContextAwait { start },
    51 => SegmentsTotal { lengths },
    52 => SegmentsFits { nominal, lengths, total, layout_ceiling },
    53 => SegmentsFill { nominal, lengths, total, value },
    54 => SegmentsMeasure { segments },
    55 => SegmentSlice { segments, index },
    56 => SegmentsAll { segments },
    57 => SharedNew { nominal },
    58 => SharedState { nominal, object },
    59 => SharedRetain { nominal, object },
    60 => SharedAcquire { object },
    61 => SharedWatch { object },
    62 => SharedUnlock { object },
});

record_enum!(IrInstruction {
    0 => Define { result, ty, operation },
    1 => StoreSlice { slice, index, value },
    2 => Store { address, value, referent },
    3 => Drops(f0),
});

record_enum!(IrDropSubject {
    0 => Value(f0),
    1 => Place(f0),
});

record_struct!(IrDrop { subject, ty });

record_struct!(IrMatchTarget { tag, block });

record_enum!(IrTerminator {
    0 => Unreachable,
    1 => Jump { target, arguments, drops },
    2 => Match { scrutinee, enum_type, targets },
    3 => Return { value, drops },
});

record_struct!(IrBlock {
    parameters,
    instructions,
    terminator
});

record_struct!(IrOverlap { members });

record_enum!(IrSynthesis {
    0 => Splitter,
    1 => Chunk,
});

record_enum!(IrSourceMode {
    0 => Own,
    1 => Reference,
    2 => Range,
});

record_struct!(IrSourceSignature { parameters, result });

record_enum!(IrSourceArgument {
    0 => Binding { consume_root },
    1 => Projection { consume_root },
    2 => Borrow,
    3 => PlaceRead,
    4 => Value,
});

record_struct!(IrSourceAllocation {
    cell,
    count_argument,
    layout_ceiling,
    source_length_upper_bound,
    site,
    count_site
});

record_struct!(IrSourceCall {
    result,
    arguments,
    returned_borrow_argument,
    allocation
});

record_struct!(IrCountedRange {
    blocks,
    continuation,
    lower,
    upper
});

record_struct!(IrFunction {
    name,
    parameters,
    readonly_reference_parameters,
    source_signature,
    source_calls,
    result,
    values,
    blocks,
    counted_ranges,
    overlaps,
    synthesis,
    waits
});

impl Record for std::ops::Range<usize> {
    fn write(&self, writer: &mut Writer) {
        self.start.write(writer);
        self.end.write(writer);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(usize::read(reader)?..usize::read(reader)?)
    }
}
