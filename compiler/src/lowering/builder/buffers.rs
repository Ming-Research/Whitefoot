//! Lowering of the runtime-capacity `Array<T>` [TYPE-9] and its readers.
//!
//! The shape is one heap block `[len | elements]` reached only through the
//! `Box` that owns it (compiler/storage-representation), so every root here
//! is the block's *address*: `b.inner` is the cell's own pointer, a measure
//! reads the `len` word at the head of the block, and an element address is
//! one `inbounds` step past that header.

use crate::semantic::{CheckedBufferRoot, CheckedExpression, CheckedTargetDomainObligation};

use super::*;

impl IrBuilder<'_> {
    pub(super) fn lower_buffer_length(
        &mut self,
        root: &CheckedBufferRoot,
    ) -> Result<IrValueId, LoweringFailure> {
        let buffer = self.buffer_root(root)?;
        self.define(
            IrType::Integer {
                width: 64,
                signed: false,
            },
            IrOperation::BufferMeasure { buffer },
        )
    }

    pub(super) fn lower_buffer_index(
        &mut self,
        root: &CheckedBufferRoot,
        offset: &CheckedExpression,
        target_domain: CheckedTargetDomainObligation,
    ) -> Result<IrValueId, LoweringFailure> {
        if let Some(family) = self.indexed_family(
            crate::semantic::CheckedPlaceRoot::Binding(root.binding),
            &root.path,
            &[],
        ) {
            let offset = self.expression(offset)?;
            let address = self.indexed_address(family, offset, target_domain.into())?;
            return self.load_storage_value(address);
        }
        let buffer = self.buffer_root(root)?;
        let IrType::Address(IrAddressed::Buffer { element }) = self.value_type(buffer)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
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
            IrOperation::BufferIndex {
                buffer,
                offset,
                target_domain: target_domain.into(),
            },
        )
    }

    /// The address of the block one buffer place names [TYPE-9].
    ///
    /// A runtime-capacity shape is only ever `Box` content, so the path that
    /// reaches it carries that content step and the binding rooting it is
    /// storage-backed; the cell's own pointer slot is loaded once and what
    /// that yields is the block.
    pub(super) fn buffer_root(
        &mut self,
        root: &CheckedBufferRoot,
    ) -> Result<IrValueId, LoweringFailure> {
        let base = self
            .bindings
            .get(&root.binding)
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        if !matches!(self.value_type(base)?, IrType::Address(_)) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let address = self.project_address_path(base, &root.path)?;
        if self.value_type(address)?
            != IrType::Address(IrAddressed::Buffer {
                element: lower_element(self.erasure, root.element)?,
            })
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(address)
    }
}
