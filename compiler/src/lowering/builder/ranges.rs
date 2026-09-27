//! Lowering of the [REF-4] range reference `&[T]`.
//!
//! compiler/storage-representation (range reference) lowers `&[T]` as a
//! pointer to the range's first element together with the element count,
//! which is the pair `IrType::Range` already names. Forming one addresses the
//! source place and narrows the descriptor to the written endpoints; the two
//! readers are the element count and one discharged element address.

use crate::semantic::{
    CheckedExpression, CheckedRangeElementPlace, CheckedRangeRoot, CheckedRangeSource,
    CheckedTargetDomainObligation, CheckedType, MeasureCell,
};

use super::*;

impl IrBuilder<'_> {
    /// [REF-4] `&x[lo..hi]`: the source descriptor, narrowed by the endpoints
    /// the checker already discharged `lo <= hi <= x.len` for.
    pub(super) fn lower_range_of(
        &mut self,
        source: &CheckedRangeSource,
        start: &CheckedExpression,
        end: &CheckedExpression,
        element: crate::semantic::CheckedElement,
    ) -> Result<IrValueId, LoweringFailure> {
        let element = lower_element(self.erasure, element)?;
        let ty = IrType::Range { element };
        let slice = match source {
            CheckedRangeSource::Storage(root) => {
                let address = self.lower_place_address(root)?;
                // A runtime-capacity `Array<T>` [TYPE-9] is one block
                // `[len | elements]` reached through its owner slot, so
                // emission resolves the current block for its length and
                // first element address; windows use ordinary run formation.
                if matches!(lower_type(self.erasure, root.ty)?, IrType::Buffer { .. }) {
                    self.define(ty, IrOperation::SliceFromBuffer { buffer: address })?
                } else {
                    self.define(ty, IrOperation::SliceFromRun { run: address })?
                }
            }
            CheckedRangeSource::Range(root) => self.range_root(root)?,
        };
        if self.value_type(slice)? != ty {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let start = self.expression(start)?;
        let end = self.expression(end)?;
        let index = IrType::Integer {
            width: 64,
            signed: false,
        };
        if self.value_type(start)? != index || self.value_type(end)? != index {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.define(ty, IrOperation::SliceRange { slice, start, end })
    }

    /// [MSR-1] the one measure a range reference has.
    pub(super) fn lower_range_measure(
        &mut self,
        root: &CheckedRangeRoot,
    ) -> Result<IrValueId, LoweringFailure> {
        let slice = self.range_root(root)?;
        self.define(
            IrType::Integer {
                width: 64,
                signed: false,
            },
            IrOperation::SliceMeasure { slice },
        )
    }

    /// [MSR-1] one measure of an element selected through a range reference.
    /// The selected element is addressed with the ordinary range machinery;
    /// its measure is then the same descriptor read as any stored measured
    /// place, so lowering adds no range-specific IR operation.
    pub(super) fn lower_range_element_measure(
        &mut self,
        measure: crate::semantic::CheckedMeasure,
        place: &CheckedRangeElementPlace,
    ) -> Result<IrValueId, LoweringFailure> {
        let measured = place
            .ty
            .measured()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        match measure.cell(measured) {
            MeasureCell::ExactConstant(value) => self.lower_fixed_measure(value),
            MeasureCell::ExactTypeConstant => {
                let constant = match place.ty {
                    CheckedType::Array { length, .. } => length.value(),
                    CheckedType::Window { capacity, .. } => {
                        capacity.and_then(crate::semantic::CheckedConst::value)
                    }
                    _ => None,
                }
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                self.lower_fixed_measure(constant)
            }
            MeasureCell::ExactExtent if matches!(place.ty, CheckedType::Array { .. }) => {
                let CheckedType::Array { length, .. } = place.ty else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                self.lower_fixed_measure(
                    length
                        .value()
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                )
            }
            MeasureCell::ExactExtent | MeasureCell::ExactRuntime | MeasureCell::Bounded => {
                let container = self.lower_range_address(
                    &place.root,
                    &place.offset,
                    &place.path,
                    place.target_domain,
                )?;
                self.define(
                    IrType::Integer {
                        width: 64,
                        signed: false,
                    },
                    IrOperation::ContainerMeasure {
                        measure: super::runs::lower_measure(measure),
                        container,
                    },
                )
            }
            MeasureCell::Absent => Err(LoweringFailure::InvalidCheckedProgram),
        }
    }

    /// [OP-4] one discharged element read of the run a range names.
    pub(super) fn lower_range_index(
        &mut self,
        root: &CheckedRangeRoot,
        offset: &CheckedExpression,
        path: &[crate::semantic::CheckedPlaceStep],
        target_domain: CheckedTargetDomainObligation,
    ) -> Result<IrValueId, LoweringFailure> {
        if !path.is_empty() {
            let address = self.lower_range_address(root, offset, path, target_domain)?;
            return self.load_storage_value(address);
        }
        let slice = self.range_root(root)?;
        let element = lower_element(self.erasure, root.element)?;
        let offset = self.expression(offset)?;
        if self.value_type(offset)?
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.define(
            self.element_type(element)?,
            IrOperation::SliceIndex {
                slice,
                offset,
                target_domain: target_domain.into(),
            },
        )
    }

    /// [REF-1, REF-4] the discharged address of one range element. This is
    /// the same pointer arithmetic as an element read, with the load omitted
    /// so the resulting source reference continues to name caller storage.
    pub(super) fn lower_range_address(
        &mut self,
        root: &CheckedRangeRoot,
        offset: &CheckedExpression,
        path: &[crate::semantic::CheckedPlaceStep],
        target_domain: CheckedTargetDomainObligation,
    ) -> Result<IrValueId, LoweringFailure> {
        let slice = self.range_root(root)?;
        let element = lower_element(self.erasure, root.element)?;
        let element_type = self.element_type(element)?;
        let offset = self.expression(offset)?;
        if self.value_type(offset)?
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let address = self.define(
            IrType::Address(
                IrAddressed::of(element_type).ok_or(LoweringFailure::InvalidCheckedProgram)?,
            ),
            IrOperation::SliceAddress {
                slice,
                offset,
                target_domain: target_domain.into(),
            },
        )?;
        self.project_address_path(address, path)
    }

    /// The descriptor value one range reference binding holds [REF-4].
    pub(super) fn range_root(
        &mut self,
        root: &CheckedRangeRoot,
    ) -> Result<IrValueId, LoweringFailure> {
        let slice = self.binding_value(root.binding)?;
        if self.value_type(slice)?
            != (IrType::Range {
                element: lower_element(self.erasure, root.element)?,
            })
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(slice)
    }
}
