//! Lowering of the [TYPE-9] storage shapes and the [MSR-1] measures of them.
//!
//! A storage shape's value is its complete storage: a `Slots<T, n>` is the
//! descriptor word `len` followed by `n` inline slots, and an `Array<T, n>`
//! is `n` slots whose `len` and `cap` are both the type constant and are
//! stored nowhere [TYPE-9, WIN-1, OP-9]. No slot carries a tag: the window
//! is the complete typestate, so every measure here is one descriptor read
//! or a compile-time constant and every subscript is one address
//! computation.
//!
//! The nine window operations [OP-10], `swap` [OP-11] and `free_empty`
//! [OP-14] are ordinary [PRE-1] records and reach lowering as ordinary
//! calls; their bodies are synthesized in `windows.rs`.

use crate::IrMeasure;
use crate::semantic::{
    CheckedContainerRoot, CheckedMeasure, CheckedPlaceStep, CheckedType, MeasureCell,
};

use super::*;

/// The IR spelling of one [MSR-1] measure.
pub(super) const fn lower_measure(measure: CheckedMeasure) -> IrMeasure {
    match measure {
        CheckedMeasure::Length => IrMeasure::Length,
        CheckedMeasure::Capacity => IrMeasure::Capacity,
        CheckedMeasure::Head => IrMeasure::Head,
    }
}

impl IrBuilder<'_> {
    /// One [MSR-1] measure of a storage shape.
    ///
    /// A cell the table fixes as a compile-time constant is that constant and
    /// loads nothing; every other cell is one descriptor word [MSR-2].
    pub(super) fn lower_container_measure(
        &mut self,
        measure: CheckedMeasure,
        root: &CheckedContainerRoot,
    ) -> Result<IrValueId, LoweringFailure> {
        let measured = root
            .measured()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        match measure.cell(measured) {
            MeasureCell::ExactConstant(value) => self.lower_fixed_measure(value),
            // An `Array` length and a constant window capacity are the type's
            // own written constant and are stored nowhere.
            MeasureCell::ExactTypeConstant => {
                let constant = root
                    .type_constant()
                    .and_then(|constant| match constant {
                        crate::semantic::CheckedConst::Value(value) => Some(value),
                        _ => None,
                    })
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                self.lower_fixed_measure(constant)
            }
            MeasureCell::ExactExtent if matches!(root.ty, CheckedType::Array { .. }) => {
                let length = root
                    .type_constant()
                    .and_then(|constant| constant.value())
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                self.lower_fixed_measure(length)
            }
            // A `Segments` block is reached through its cell and its count
            // is the word heading it [TYPE-9].
            MeasureCell::ExactRuntime if matches!(root.ty, CheckedType::Segments { .. }) => {
                let segments = self.lower_place_address(root)?;
                self.define(
                    IrType::Integer {
                        width: 64,
                        signed: false,
                    },
                    IrOperation::SegmentsMeasure { segments },
                )
            }
            MeasureCell::ExactExtent | MeasureCell::ExactRuntime | MeasureCell::Bounded => {
                let container = self.container_root_value(root)?;
                self.define(
                    IrType::Integer {
                        width: 64,
                        signed: false,
                    },
                    IrOperation::ContainerMeasure {
                        measure: lower_measure(measure),
                        container,
                    },
                )
            }
            MeasureCell::Absent => Err(LoweringFailure::InvalidCheckedProgram),
        }
    }

    /// Reads one measured place's value out of the value at its root:
    /// a field step projects, and a subscript step reads the slot the offset
    /// selects [WIN-1, OP-4].
    fn project_place_path(
        &mut self,
        base: IrValueId,
        steps: &[CheckedPlaceStep],
    ) -> Result<IrValueId, LoweringFailure> {
        let mut value = base;
        for step in steps {
            value = match step {
                CheckedPlaceStep::Field(field) => {
                    self.project_struct_path(value, &[*field], false)?
                }
                // Box-referent places are promoted by storage planning and
                // lowered through their owner slot's address. Reaching this
                // value-only path would take an address from a copied Box.
                CheckedPlaceStep::BoxReferent(_) => {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                CheckedPlaceStep::Subscript(subscript) => {
                    let offset = self.expression(&subscript.offset)?;
                    let operation = match lower_type(self.erasure, subscript.base_type)? {
                        IrType::Array { .. } => IrOperation::ArrayIndex {
                            root: IrArrayRoot::Value(value),
                            offset,
                            target_domain: subscript.target_domain.into(),
                        },
                        IrType::Window { .. } => IrOperation::RunIndex {
                            run: value,
                            offset,
                            target_domain: subscript.target_domain.into(),
                        },
                        _ => return Err(LoweringFailure::InvalidCheckedProgram),
                    };
                    self.define(lower_type(self.erasure, subscript.element_type)?, operation)?
                }
            };
        }
        Ok(value)
    }

    /// The run or extent value one measured place reads, projected out of its
    /// root binding by the field selections that reach it [MSR-2].
    pub(super) fn container_root_value(
        &mut self,
        root: &CheckedContainerRoot,
    ) -> Result<IrValueId, LoweringFailure> {
        let Some(binding) = root.binding() else {
            return self.lower_place_address(root);
        };
        if self
            .bindings
            .get(&binding)
            .copied()
            .is_some_and(|storage| matches!(self.value_type(storage), Ok(IrType::Address(_))))
        {
            return self.lower_place_address(root);
        }
        let value = self.binding_value(binding)?;
        let value = self.project_place_path(value, &root.path)?;
        if self.value_type(value)? != lower_type(self.erasure, root.ty)? {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(value)
    }
}
