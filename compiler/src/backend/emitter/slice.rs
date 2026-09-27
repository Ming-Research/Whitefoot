use super::*;

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    /// [REF-4] the descriptor of the whole run a runtime-capacity `Array<T>`
    /// block holds: its first element's address and the `len` word at the
    /// head of the block (compiler/storage-representation).
    pub(super) fn emit_slice_from_buffer(
        &mut self,
        result: IrValueId,
        ty: IrType,
        buffer: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrType::Range { element } = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        let Some(IrType::Address(IrAddressed::Buffer {
            element: buffer_element,
        })) = self.value_type(buffer)
        else {
            return Err(BackendFailure::InvalidIr);
        };
        if element != buffer_element {
            return Err(BackendFailure::InvalidIr);
        }
        let block_type = self.output.type_name(
            self.program,
            IrType::Buffer {
                element: buffer_element,
            },
        )?;
        let descriptor_type = self.output.type_name(self.program, ty)?;
        let address = self.addressed_storage_pointer(buffer)?;
        let pointer = self.next_temporary()?;
        let length_address = self.next_temporary()?;
        let length = self.next_temporary()?;
        let partial = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = getelementptr inbounds {block_type}, ptr {address}, i64 0, i32 1, i64 0\n  %{length_address} = getelementptr inbounds {block_type}, ptr {address}, i32 0, i32 0\n  %{length} = load i64, ptr %{length_address}\n  %{partial} = insertvalue {descriptor_type} zeroinitializer, ptr %{pointer}, 0\n  {} = insertvalue {descriptor_type} %{partial}, i64 %{length}, 1",
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_slice_range(
        &mut self,
        result: IrValueId,
        ty: IrType,
        slice: IrValueId,
        start: IrValueId,
        end: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrType::Range { element } = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        let index_type = Some(IrType::Integer {
            width: 64,
            signed: false,
        });
        if self.value_type(slice) != Some(ty)
            || self.value_type(start) != index_type
            || self.value_type(end) != index_type
        {
            return Err(BackendFailure::InvalidIr);
        }
        let descriptor_type = self.output.type_name(self.program, ty)?;
        let element_type = self.output.type_name(
            self.program,
            self.program
                .element(element)
                .ok_or(BackendFailure::InvalidIr)?,
        )?;
        let pointer = self.next_temporary()?;
        let adjusted = self.next_temporary()?;
        let length = self.next_temporary()?;
        let partial = self.next_temporary()?;
        let logical_start = self.value_name(start);
        let address_start = self.element_address_index(
            self.program
                .element(element)
                .ok_or(BackendFailure::InvalidIr)?,
            &logical_start,
        )?;
        // [REF-4] discharged both domain conjuncts, `lo <= hi` and
        // `hi <= x.len`, before this descriptor exists, so the adjusted
        // address stays inside the extent the original descriptor names and
        // carries `inbounds` (compiler/backend-facts). Empty ranges,
        // including one at the end of an allocation, form descriptors
        // without a load.
        writeln!(
            self.output,
            "  %{pointer} = extractvalue {descriptor_type} {}, 0\n  %{adjusted} = getelementptr inbounds {element_type}, ptr %{pointer}, i64 {}\n  %{length} = sub nuw i64 {}, {}\n  %{partial} = insertvalue {descriptor_type} zeroinitializer, ptr %{adjusted}, 0\n  {} = insertvalue {descriptor_type} %{partial}, i64 %{length}, 1",
            self.value_name(slice), address_start, self.value_name(end),
            self.value_name(start), self.value_name(result),
        ).map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_slice_length(
        &mut self,
        result: IrValueId,
        ty: IrType,
        slice: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
            || !matches!(self.value_type(slice), Some(IrType::Range { .. }))
        {
            return Err(BackendFailure::InvalidIr);
        }
        {
            let emitted_type_1 = self.output.type_name(
                self.program,
                self.function
                    .value_type(slice)
                    .ok_or(BackendFailure::InvalidIr)?,
            )?;
            writeln!(
                self.output,
                "  {} = extractvalue {} {}, 1",
                self.value_name(result),
                emitted_type_1,
                self.value_name(slice)
            )
        }
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Emits a discharged source subscript read [OP-4]: the checker derived
    /// the bounds obligation, so no compare, branch, or trap is emitted in
    /// any build mode.
    pub(super) fn emit_slice_index(
        &mut self,
        result: IrValueId,
        ty: IrType,
        slice: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    ) -> Result<(), BackendFailure> {
        if target_domain != IrTargetDomainObligation::ElementAddress {
            return Err(BackendFailure::InvalidIr);
        }
        let Some(slice_type @ IrType::Range { element }) = self.value_type(slice) else {
            return Err(BackendFailure::InvalidIr);
        };
        if self.program.element(element) != Some(ty)
            || self.value_type(offset)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let descriptor_type = self.output.type_name(self.program, slice_type)?;
        let element_type = self.output.type_name(self.program, ty)?;
        let pointer = self.next_temporary()?;
        let element_pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = extractvalue {descriptor_type} {}, 0\n  %{element_pointer} = getelementptr inbounds {element_type}, ptr %{pointer}, i64 {}\n  {} = load {element_type}, ptr %{element_pointer}",
            self.value_name(slice),
            self.element_address_index(ty, &self.value_name(offset))?,
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [SET-1] one element-position store through an exclusive view.
    ///
    /// The descriptor is {data pointer, length}, so the store is the element
    /// address the read already computes and one `store` through it.
    pub(super) fn emit_slice_store(
        &mut self,
        slice: IrValueId,
        index: IrValueId,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let Some(slice_type @ IrType::Range { element }) = self.value_type(slice) else {
            return Err(BackendFailure::InvalidIr);
        };
        if self.value_type(index)
            != Some(IrType::Integer {
                width: 64,
                signed: false,
            })
            || self.value_type(value) != self.program.element(element)
        {
            return Err(BackendFailure::InvalidIr);
        }
        let descriptor_type = self.output.type_name(self.program, slice_type)?;
        let element_type = self.output.type_name(
            self.program,
            self.program
                .element(element)
                .ok_or(BackendFailure::InvalidIr)?,
        )?;
        let pointer = self.next_temporary()?;
        let element_pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = extractvalue {descriptor_type} {}, 0\n  %{element_pointer} = getelementptr inbounds {element_type}, ptr %{pointer}, i64 {}",
            self.value_name(slice),
            self.element_address_index(
                self.program.element(element).ok_or(BackendFailure::InvalidIr)?,
                &self.value_name(index),
            )?,
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.store_value_at(value, &format!("%{element_pointer}"))
    }

    pub(super) fn emit_slice_descriptor(
        &mut self,
        result: IrValueId,
        ty: IrType,
        pointer: &str,
        length: u64,
    ) -> Result<(), BackendFailure> {
        let descriptor_type = self.output.type_name(self.program, ty)?;
        let partial = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{partial} = insertvalue {descriptor_type} zeroinitializer, ptr {pointer}, 0\n  {} = insertvalue {descriptor_type} %{partial}, i64 {length}, 1",
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Emits the address of a discharged range element [REF-1, REF-4]. The
    /// range descriptor supplies the base pointer and [OP-4] supplies the
    /// proof that the inbounds step is valid; no element load occurs here.
    pub(super) fn emit_slice_address(
        &mut self,
        result: IrValueId,
        ty: IrType,
        slice: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    ) -> Result<(), BackendFailure> {
        if target_domain != IrTargetDomainObligation::ElementAddress {
            return Err(BackendFailure::InvalidIr);
        }
        let Some(slice_type @ IrType::Range { element }) = self.value_type(slice) else {
            return Err(BackendFailure::InvalidIr);
        };
        let element_type = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        if ty != IrType::Address(IrAddressed::of(element_type).ok_or(BackendFailure::InvalidIr)?)
            || self.value_type(offset)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let descriptor_type = self.output.type_name(self.program, slice_type)?;
        let llvm_element_type = self.output.type_name(self.program, element_type)?;
        let pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = extractvalue {descriptor_type} {}, 0\n  {} = getelementptr inbounds {llvm_element_type}, ptr %{pointer}, i64 {}",
            self.value_name(slice),
            self.value_name(result),
            self.element_address_index(element_type, &self.value_name(offset))?,
        )
        .map_err(|_| BackendFailure::TextEmission)
    }
}
