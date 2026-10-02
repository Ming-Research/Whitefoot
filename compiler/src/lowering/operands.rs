//! Runtime value uses shared by capture selection, CFG transfer, storage and emission.

use super::{
    IrArrayRoot, IrDrop, IrDropSubject, IrInstruction, IrOperation, IrTerminator, IrValueId,
};

fn copied(value: &IrValueId) -> IrValueId {
    *value
}

impl IrDrop {
    fn operand_mut(&mut self) -> &mut IrValueId {
        match &mut self.subject {
            IrDropSubject::Value(value) | IrDropSubject::Place(value) => value,
        }
    }
}

// The read and rewrite views share one exhaustive operand classification.
// Rewriting definitions, block targets and non-executable metadata belongs to
// the CFG transfer, not to this inventory of runtime uses.
macro_rules! instruction_operands {
    ($instruction:expr, $iter:ident, $value:path, $drop:ident, $operation:ident) => {
        match $instruction {
            IrInstruction::Define { operation, .. } => operation.$operation(),
            IrInstruction::StoreSlice {
                slice,
                index,
                value,
            } => {
                vec![$value(slice), $value(index), $value(value)]
            }
            IrInstruction::Store { address, value, .. } => {
                vec![$value(address), $value(value)]
            }
            IrInstruction::Drops(drops) => drops.$iter().map(|drop| drop.$drop()).collect(),
        }
    };
}

impl IrInstruction {
    pub(crate) fn operands(&self) -> Vec<IrValueId> {
        instruction_operands!(self, iter, copied, operand, operands)
    }

    pub(super) fn remap_operands(&mut self, mut remap: impl FnMut(IrValueId) -> IrValueId) {
        let operands: Vec<&mut IrValueId> = instruction_operands!(
            self,
            iter_mut,
            std::convert::identity,
            operand_mut,
            operands_mut
        );
        for value in operands {
            *value = remap(*value);
        }
    }
}

macro_rules! terminator_operands {
    ($terminator:expr, $iter:ident, $value:path, $drop:ident) => {
        match $terminator {
            IrTerminator::Unreachable => Vec::new(),
            IrTerminator::Jump {
                arguments, drops, ..
            } => arguments
                .$iter()
                .map($value)
                .chain(drops.$iter().map(|drop| drop.$drop()))
                .collect(),
            IrTerminator::Match { scrutinee, .. } => vec![$value(scrutinee)],
            IrTerminator::Return { value, drops } => std::iter::once($value(value))
                .chain(drops.$iter().map(|drop| drop.$drop()))
                .collect(),
        }
    };
}

impl IrTerminator {
    pub(crate) fn operands(&self) -> Vec<IrValueId> {
        terminator_operands!(self, iter, copied, operand)
    }

    pub(super) fn remap_operands(&mut self, mut remap: impl FnMut(IrValueId) -> IrValueId) {
        let operands: Vec<&mut IrValueId> =
            terminator_operands!(self, iter_mut, std::convert::identity, operand_mut);
        for value in operands {
            *value = remap(*value);
        }
    }
}

macro_rules! operation_operands {
    ($operation:expr, $iter:ident, $value:path) => {
        match $operation {
            IrOperation::Constant(_)
            | IrOperation::ConstantAddress { .. }
            | IrOperation::Window
            | IrOperation::ContextJoin
            | IrOperation::SharedNew { .. } => Vec::new(),
            IrOperation::Call { arguments, .. }
            | IrOperation::ContextStart { arguments, .. }
            | IrOperation::ContextStartBound { arguments, .. }
            | IrOperation::Integer { arguments, .. }
            | IrOperation::Float { arguments, .. }
            | IrOperation::Boolean { arguments, .. } => arguments.$iter().map($value).collect(),
            IrOperation::EnumEquality { arguments, .. } => arguments.$iter().map($value).collect(),
            IrOperation::NumericConversion { value, .. }
            | IrOperation::Reinterpret { value, .. }
            | IrOperation::ArrayFill { value, .. }
            | IrOperation::FullArrayConversion { value }
            | IrOperation::BoxNew { value, .. }
            | IrOperation::BoxTake { value, .. }
            | IrOperation::BoxDeref { value, .. }
            | IrOperation::RuntimeBoxPayload { owner: value, .. }
            | IrOperation::RuntimeBoxOwner { payload: value, .. }
            | IrOperation::AddressOf { value, .. }
            | IrOperation::ContextAwait { start: value }
            | IrOperation::SharedState { object: value, .. }
            | IrOperation::SharedRetain { object: value, .. }
            | IrOperation::SharedAcquire { object: value }
            | IrOperation::SharedWatch { object: value }
            | IrOperation::SharedTake { object: value }
            | IrOperation::SharedUnlock { object: value }
            | IrOperation::SharedMapNew {
                capacity: value, ..
            }
            | IrOperation::SharedMapState { object: value, .. }
            | IrOperation::SharedMapHold { object: value }
            | IrOperation::SharedMapUnhold { object: value }
            | IrOperation::SharedMapCount { state: value } => vec![$value(value)],
            IrOperation::SharedMapLock { object, key, .. } => vec![$value(object), $value(key)],
            IrOperation::SharedMapUnlock { object, entry, .. } => {
                vec![$value(object), $value(entry)]
            }
            IrOperation::ArrayIndex { root, offset, .. } => match root {
                IrArrayRoot::Value(value) => vec![$value(value), $value(offset)],
                IrArrayRoot::Constant(_) => vec![$value(offset)],
            },
            IrOperation::BufferFill { length, value, .. } => vec![$value(length), $value(value)],
            IrOperation::SegmentsTotal { lengths } => vec![$value(lengths)],
            IrOperation::SegmentsFits { lengths, total, .. } => {
                vec![$value(lengths), $value(total)]
            }
            IrOperation::SegmentsFill {
                lengths,
                total,
                value,
                ..
            } => vec![$value(lengths), $value(total), $value(value)],
            IrOperation::SegmentsMeasure { segments } | IrOperation::SegmentsAll { segments } => {
                vec![$value(segments)]
            }
            IrOperation::SegmentSlice { segments, index } => vec![$value(segments), $value(index)],
            IrOperation::BufferMeasure { buffer } | IrOperation::SliceFromBuffer { buffer } => {
                vec![$value(buffer)]
            }
            IrOperation::ContainerMeasure { container, .. } => vec![$value(container)],
            IrOperation::RunIndex { run, offset, .. } => vec![$value(run), $value(offset)],
            IrOperation::RunConsumeFinish { run, retained } => vec![$value(run), $value(retained)],
            IrOperation::RunLengthCommit { run, length } => vec![$value(run), $value(length)],
            IrOperation::RunBoundaryResident { run, value, length } => {
                vec![$value(run), $value(value), $value(length)]
            }
            IrOperation::RunBoundary { run, value, .. } => std::iter::once($value(run))
                .chain(value.$iter().map($value))
                .collect(),
            IrOperation::RunTaken { run, .. } | IrOperation::SliceFromRun { run } => {
                vec![$value(run)]
            }
            IrOperation::RunShift { run, index, .. } => vec![$value(run), $value(index)],
            IrOperation::RunInsert { run, index, value } => {
                vec![$value(run), $value(index), $value(value)]
            }
            IrOperation::RunTransfer {
                destination,
                source,
                index,
            } => {
                vec![$value(destination), $value(source), $value(index)]
            }
            IrOperation::WindowBlockNew { capacity, .. } => vec![$value(capacity)],
            IrOperation::WindowGrow { cell, capacity, .. } => vec![$value(cell), $value(capacity)],
            IrOperation::CellFree { value, .. } => vec![$value(value)],
            IrOperation::SliceRange { slice, start, end } => {
                vec![$value(slice), $value(start), $value(end)]
            }
            IrOperation::BufferIndex { buffer, offset, .. } => vec![$value(buffer), $value(offset)],
            IrOperation::BufferProbeSkip {
                buffer,
                index,
                limit,
                needles,
            } => [$value(buffer), $value(index), $value(limit)]
                .into_iter()
                .chain(needles.$iter().map($value))
                .collect(),
            IrOperation::SliceMeasure { slice } => vec![$value(slice)],
            IrOperation::SliceIndex { slice, offset, .. }
            | IrOperation::SliceAddress { slice, offset, .. } => {
                vec![$value(slice), $value(offset)]
            }
            IrOperation::ConstructStruct { fields, .. }
            | IrOperation::ConstructEnum { fields, .. } => fields.$iter().map($value).collect(),
            IrOperation::ProjectStruct { aggregate, .. }
            | IrOperation::ProjectVariant { aggregate, .. } => {
                vec![$value(aggregate)]
            }
            IrOperation::InsertStruct {
                aggregate, value, ..
            } => vec![$value(aggregate), $value(value)],
            IrOperation::Load { address, .. } => vec![$value(address)],
            IrOperation::RuntimeContentSwap { first, second } => {
                vec![$value(first), $value(second)]
            }
            IrOperation::ProjectAddress {
                address,
                projection,
            } => match projection {
                super::IrPlaceStep::Field { .. }
                | super::IrPlaceStep::BoxReferent { .. }
                | super::IrPlaceStep::EnumVariant { .. } => vec![$value(address)],
                super::IrPlaceStep::RunElement { offset, .. }
                | super::IrPlaceStep::ArrayElement { offset, .. }
                | super::IrPlaceStep::BufferElement { offset, .. } => {
                    vec![$value(address), $value(offset)]
                }
            },
            IrOperation::LoopSplit {
                seed,
                lower,
                upper,
                captures,
                ..
            } => [$value(seed), $value(lower), $value(upper)]
                .into_iter()
                .chain(captures.$iter().map($value))
                .collect(),
        }
    };
}

impl IrOperation {
    /// Every runtime value read, including allocation inputs and loop captures.
    /// New operations must specify their uses before any consumer compiles.
    pub(crate) fn operands(&self) -> Vec<IrValueId> {
        operation_operands!(self, iter, copied)
    }

    fn operands_mut(&mut self) -> Vec<&mut IrValueId> {
        operation_operands!(self, iter_mut, std::convert::identity)
    }
}
