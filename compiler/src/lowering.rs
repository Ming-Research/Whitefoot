//! Typed control-flow lowering from the semantically checked Whitefoot
//! specification into the IR of [`crate::ir`].
//!
//! Lowering performs no source admission, label lookup, exhaustiveness
//! decision, or ownership judgment. Optional loop actualization uses the
//! selected target's lane-frame layout; the value and control-flow
//! representation remains target-neutral.

use crate::ir::*;
use crate::semantic::{
    CheckedBooleanOperation, CheckedConversionMode, CheckedElement, CheckedEnumType,
    CheckedFloatOperation, CheckedIntegerOperation, CheckedLayoutCeiling, CheckedLayoutMagnitude,
    CheckedNumericType, CheckedTargetDomainObligation, CheckedType,
};

mod operands;
mod physical_types;
mod specialize;

/// A closed executable instance's type interpretation. Source region identity
/// has already been checked; only its storage reclamation remains in the IR.
#[derive(Clone, Copy)]
struct TypeLowering<'a> {
    nominals: &'a [IrNominalId],
    elements: &'a [Option<IrElement>],
}

impl TypeLowering<'_> {
    const EMPTY: Self = Self {
        nominals: &[],
        elements: &[],
    };
}

pub(crate) const fn lower_window_shape(value: crate::semantic::WindowShape) -> IrWindowShape {
    match value {
        crate::semantic::WindowShape::Slots => IrWindowShape::Slots,
        crate::semantic::WindowShape::Ring => IrWindowShape::Ring,
    }
}

pub(crate) const fn lower_release_class(
    value: crate::semantic::CheckedReleaseClass,
) -> IrReleaseClass {
    match value {
        crate::semantic::CheckedReleaseClass::General => IrReleaseClass::General,
    }
}

/// One nominal's lowered identity: instances of one physical family share an
/// IR nominal when their complete reclamation graphs agree.
fn erased_nominal(erasure: TypeLowering<'_>, id: crate::NominalId) -> IrNominalId {
    erasure
        .nominals
        .get(id.0 as usize)
        .copied()
        .unwrap_or(IrNominalId(id.0))
}

fn lower_element(
    erasure: TypeLowering<'_>,
    value: CheckedElement,
) -> Result<IrElement, LoweringFailure> {
    erasure
        .elements
        .get(value.index())
        .copied()
        .flatten()
        .ok_or(LoweringFailure::InvalidCheckedProgram)
}

fn lower_type(erasure: TypeLowering<'_>, value: CheckedType) -> Result<IrType, LoweringFailure> {
    Ok(match value {
        CheckedType::Unit => IrType::Unit,
        CheckedType::Bool => IrType::Bool,
        CheckedType::Integer(integer) => IrType::Integer {
            width: integer.width(),
            signed: integer.signed(),
        },
        CheckedType::Float(float) => IrType::Float {
            width: float.width(),
        },
        CheckedType::Generic(_) | CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        CheckedType::Nominal(id) => IrType::Nominal(erased_nominal(erasure, id)),
        CheckedType::Array { element, length } => IrType::Array {
            element: lower_element(erasure, element)?,
            length: length
                .value()
                .ok_or(LoweringFailure::InvalidCheckedProgram)?,
        },
        CheckedType::Buffer { element } => IrType::Buffer {
            element: lower_element(erasure, element)?,
        },
        CheckedType::Window {
            shape,
            element,
            capacity,
        } => IrType::Window {
            shape: lower_window_shape(shape),
            element: lower_element(erasure, element)?,
            capacity: capacity
                .map(|capacity| {
                    capacity
                        .value()
                        .ok_or(LoweringFailure::InvalidCheckedProgram)
                })
                .transpose()?,
        },
    })
}

const fn lower_numeric_type(value: CheckedNumericType) -> Result<IrType, LoweringFailure> {
    Ok(match value {
        CheckedNumericType::Integer(integer) => IrType::Integer {
            width: integer.width(),
            signed: integer.signed(),
        },
        CheckedNumericType::Float(float) => IrType::Float {
            width: float.width(),
        },
        CheckedNumericType::GenericInteger(_) | CheckedNumericType::GenericFloat(_) => {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
    })
}

fn lower_enum_type(erasure: TypeLowering<'_>, value: CheckedEnumType) -> IrEnumType {
    match value {
        CheckedEnumType::Bool => IrEnumType::Bool,
        CheckedEnumType::Nominal(id) => IrEnumType::Nominal(erased_nominal(erasure, id)),
    }
}

impl From<CheckedIntegerOperation> for IrIntegerOperation {
    fn from(value: CheckedIntegerOperation) -> Self {
        match value {
            CheckedIntegerOperation::AddWrap => Self::AddWrap,
            CheckedIntegerOperation::SubtractWrap => Self::SubtractWrap,
            CheckedIntegerOperation::MultiplyWrap => Self::MultiplyWrap,
            CheckedIntegerOperation::AddExact => Self::AddExact,
            CheckedIntegerOperation::SubtractExact => Self::SubtractExact,
            CheckedIntegerOperation::MultiplyExact => Self::MultiplyExact,
            CheckedIntegerOperation::AddDefined => Self::AddDefined,
            CheckedIntegerOperation::SubtractDefined => Self::SubtractDefined,
            CheckedIntegerOperation::MultiplyDefined => Self::MultiplyDefined,
            CheckedIntegerOperation::AddChecked => Self::AddChecked,
            CheckedIntegerOperation::SubtractChecked => Self::SubtractChecked,
            CheckedIntegerOperation::MultiplyChecked => Self::MultiplyChecked,
            CheckedIntegerOperation::DivideChecked => Self::DivideChecked,
            CheckedIntegerOperation::RemainderChecked => Self::RemainderChecked,
            CheckedIntegerOperation::DivideExact => Self::DivideExact,
            CheckedIntegerOperation::RemainderExact => Self::RemainderExact,
            CheckedIntegerOperation::DivideDefined => Self::DivideDefined,
            CheckedIntegerOperation::RemainderDefined => Self::RemainderDefined,
            CheckedIntegerOperation::AbsoluteWrap => Self::AbsoluteWrap,
            CheckedIntegerOperation::AbsoluteExact => Self::AbsoluteExact,
            CheckedIntegerOperation::AbsoluteDefined => Self::AbsoluteDefined,
            CheckedIntegerOperation::AbsoluteChecked => Self::AbsoluteChecked,
            CheckedIntegerOperation::NegateWrap => Self::NegateWrap,
            CheckedIntegerOperation::NegateExact => Self::NegateExact,
            CheckedIntegerOperation::NegateDefined => Self::NegateDefined,
            CheckedIntegerOperation::NegateChecked => Self::NegateChecked,
            CheckedIntegerOperation::BitAnd => Self::BitAnd,
            CheckedIntegerOperation::BitOr => Self::BitOr,
            CheckedIntegerOperation::BitXor => Self::BitXor,
            CheckedIntegerOperation::BitNot => Self::BitNot,
            CheckedIntegerOperation::ShiftLeftWrap => Self::ShiftLeftWrap,
            CheckedIntegerOperation::ShiftRightWrap => Self::ShiftRightWrap,
            CheckedIntegerOperation::ShiftLeftExact => Self::ShiftLeftExact,
            CheckedIntegerOperation::ShiftRightExact => Self::ShiftRightExact,
            CheckedIntegerOperation::ShiftLeftDefined => Self::ShiftLeftDefined,
            CheckedIntegerOperation::ShiftRightDefined => Self::ShiftRightDefined,
            CheckedIntegerOperation::RotateLeft => Self::RotateLeft,
            CheckedIntegerOperation::RotateRight => Self::RotateRight,
            CheckedIntegerOperation::PopulationCount => Self::PopulationCount,
            CheckedIntegerOperation::LeadingZeros => Self::LeadingZeros,
            CheckedIntegerOperation::TrailingZeros => Self::TrailingZeros,
            CheckedIntegerOperation::ByteSwap => Self::ByteSwap,
            CheckedIntegerOperation::MultiplyHigh => Self::MultiplyHigh,
            CheckedIntegerOperation::AddSaturating => Self::AddSaturating,
            CheckedIntegerOperation::SubtractSaturating => Self::SubtractSaturating,
            CheckedIntegerOperation::MultiplySaturating => Self::MultiplySaturating,
            CheckedIntegerOperation::Minimum => Self::Minimum,
            CheckedIntegerOperation::Maximum => Self::Maximum,
            CheckedIntegerOperation::Equal => Self::Equal,
            CheckedIntegerOperation::NotEqual => Self::NotEqual,
            CheckedIntegerOperation::Less => Self::Less,
            CheckedIntegerOperation::LessEqual => Self::LessEqual,
            CheckedIntegerOperation::Greater => Self::Greater,
            CheckedIntegerOperation::GreaterEqual => Self::GreaterEqual,
        }
    }
}

impl From<CheckedConversionMode> for IrConversionMode {
    fn from(value: CheckedConversionMode) -> Self {
        match value {
            CheckedConversionMode::Exact => Self::Exact,
            CheckedConversionMode::Checked => Self::Checked,
            CheckedConversionMode::Defined => Self::Defined,
        }
    }
}

impl From<CheckedFloatOperation> for IrFloatOperation {
    fn from(value: CheckedFloatOperation) -> Self {
        match value {
            CheckedFloatOperation::AddStrict => Self::AddStrict,
            CheckedFloatOperation::SubtractStrict => Self::SubtractStrict,
            CheckedFloatOperation::MultiplyStrict => Self::MultiplyStrict,
            CheckedFloatOperation::DivideStrict => Self::DivideStrict,
            CheckedFloatOperation::Equal => Self::Equal,
            CheckedFloatOperation::Less => Self::Less,
            CheckedFloatOperation::LessEqual => Self::LessEqual,
            CheckedFloatOperation::Greater => Self::Greater,
            CheckedFloatOperation::GreaterEqual => Self::GreaterEqual,
            CheckedFloatOperation::NotEqual => Self::NotEqual,
            CheckedFloatOperation::Negate => Self::Negate,
            CheckedFloatOperation::Absolute => Self::Absolute,
            CheckedFloatOperation::CopySign => Self::CopySign,
            CheckedFloatOperation::Minimum => Self::Minimum,
            CheckedFloatOperation::Maximum => Self::Maximum,
            CheckedFloatOperation::Floor => Self::Floor,
            CheckedFloatOperation::Ceil => Self::Ceil,
            CheckedFloatOperation::Truncate => Self::Truncate,
            CheckedFloatOperation::RoundEven => Self::RoundEven,
            CheckedFloatOperation::Remainder => Self::Remainder,
            CheckedFloatOperation::SquareRootStrict => Self::SquareRootStrict,
            CheckedFloatOperation::FusedMultiplyAddStrict => Self::FusedMultiplyAddStrict,
            CheckedFloatOperation::Infinity => Self::Infinity,
            CheckedFloatOperation::Nan => Self::Nan,
        }
    }
}

impl From<CheckedBooleanOperation> for IrBooleanOperation {
    fn from(value: CheckedBooleanOperation) -> Self {
        match value {
            CheckedBooleanOperation::And => Self::And,
            CheckedBooleanOperation::Or => Self::Or,
            CheckedBooleanOperation::ExclusiveOr => Self::ExclusiveOr,
            CheckedBooleanOperation::Not => Self::Not,
        }
    }
}

impl From<CheckedLayoutMagnitude> for IrLayoutMagnitude {
    fn from(value: CheckedLayoutMagnitude) -> Self {
        match value {
            CheckedLayoutMagnitude::Finite(value) => Self::Finite(value),
            CheckedLayoutMagnitude::AboveU64 => Self::AboveU64,
        }
    }
}

impl From<CheckedLayoutCeiling> for IrLayoutCeiling {
    fn from(value: CheckedLayoutCeiling) -> Self {
        Self {
            size: value.size.into(),
            align: value.align,
            stride: value.stride.into(),
        }
    }
}

impl From<CheckedTargetDomainObligation> for IrTargetDomainObligation {
    fn from(value: CheckedTargetDomainObligation) -> Self {
        match value {
            CheckedTargetDomainObligation::ElementAddress => Self::ElementAddress,
        }
    }
}

/// Whether lowering actualizes ordinary permission-derived overlap.
///
/// Every mode runs the same permission judgment and preserves source acceptance.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OverlapLowering {
    /// Emit sequential ordinary calls.
    #[default]
    Off,
    /// Outline eligible ordinary calls and counted-loop groups.
    On,
    /// Control over the recursion budget every other `On` form derives from
    /// the runtime: pin its starting value, or emit no budget family at all.
    OnWithRecursionBudget {
        /// Where one call into a recursive component starts counting.
        budget: RecursionBudget,
        /// Optional scalar-leaf offer suppression.
        maximum_scalar_leaf_operations: Option<u32>,
        /// Also select sequential clones on refused compute offers.
        sequential_refusal: bool,
    },
    /// Optional control: an ungranted ordinary call may enter
    /// its existing ordinary-ABI sequential clone at the original join.
    OnWithSequentialRefusal {
        /// Optional suppression of small scalar leaf offers, as in the leaf control.
        maximum_scalar_leaf_operations: Option<u32>,
    },
    /// Retain `On` except for offers of straight-line scalar
    /// leaves with at most this many nonconstant IR operations. This is an
    /// actualization heuristic, not an acceptance bound or machine-cost claim.
    OnWithoutSmallScalarLeaves {
        /// Maximum nonconstant operations in a scalar leaf whose offer is omitted.
        maximum_operations: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweringFailure {
    InvalidCheckedProgram,
    CounterOverflow,
    /// A target-stage layout check during optional loop actualization failed.
    /// Semantic acceptance has already completed; this is no source verdict.
    TargetLayout(crate::target::TargetLayoutFailure),
    /// Capability stop: one [PRE-1] record the compiler itself owns -- a
    /// window operation [OP-10], `swap` [OP-11], a construction function
    /// [OP-13] or `free_empty` [OP-14] -- whose body this version does not
    /// build.
    ///
    /// Every row of [`COMPILER_OWNED_PRELUDE_ROWS`] is built today, so no
    /// program reaches this. It remains the stop a row added to that list
    /// ahead of its body would take.
    ///
    /// These records are declared body-less like the host functions [PRE-2],
    /// but unlike a host function no trusted-base object defines them: the compiler is supposed
    /// to emit their bodies. Reaching here means a program called one, and
    /// stopping is what keeps an unimplemented capability from becoming a
    /// module that names a symbol nothing defines. It is never a source
    /// verdict, so the driver reports it as the unsupported capability it is.
    UnimplementedPreludeRow(&'static str),
}

impl From<crate::target::TargetLayoutFailure> for LoweringFailure {
    fn from(failure: crate::target::TargetLayoutFailure) -> Self {
        match failure {
            crate::target::TargetLayoutFailure::InvalidIr => Self::InvalidCheckedProgram,
            other => Self::TargetLayout(other),
        }
    }
}

/// The [PRE-1] records whose bodies the compiler itself emits: the nine
/// construction functions [OP-13], the nine window operations [OP-10],
/// `swap` [OP-11] and `free_empty` [OP-14].
///
/// The host functions [PRE-2] are deliberately absent: those are body-less
/// because the trusted base defines them, and calling one emits an ordinary
/// external call. A name that is on this list but that `lower_prelude_row` does not
/// build reaches [`LoweringFailure::UnimplementedPreludeRow`], so a row this
/// version has not built can never become a module that names a symbol
/// nothing defines.
pub(crate) const COMPILER_OWNED_PRELUDE_ROWS: [&str; 20] = [
    // [OP-13] the nine construction functions.
    "box_new",
    "array_filled",
    "slots_new",
    "ring_new",
    "box_array_filled",
    "box_slots_new",
    "box_ring_new",
    "slots_from_array",
    "slots_into_array",
    // [OP-10] the nine window operations.
    "place_back",
    "take_back",
    "insert_at",
    "remove_at",
    "append",
    "split_off",
    "grow",
    "place_front",
    "take_front",
    // [OP-11] `swap` and [OP-14] `free_empty`.
    "swap",
    "free_empty",
];

mod builder;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use builder::lower_checked;
pub(crate) use builder::lower_checked_from;
#[cfg(test)]
pub(crate) use builder::lower_checked_with_layout;
