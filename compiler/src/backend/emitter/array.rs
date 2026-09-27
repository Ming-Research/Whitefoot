use super::*;

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    /// [OP-13] `slots_from_array` and `slots_into_array`: the consuming
    /// conversion between a full fixed window and its dense array.
    ///
    /// Both rows are declared on `Slots` alone, whose window begins at slot
    /// zero [WIN-1], and both sides are full, so the elements are one
    /// contiguous extent on each side and the transfer is one copy of the
    /// slots plus, in the array-to-window direction, the one descriptor word
    /// the block carries. The header is first and the slots follow it
    /// (compiler/storage-representation), so the slot extent is reached
    /// through the window's own slots field rather than from the block's
    /// base.
    pub(super) fn emit_full_array_conversion(
        &mut self,
        result: IrValueId,
        ty: IrType,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let source_type = self.value_type(value).ok_or(BackendFailure::InvalidIr)?;
        let (element, length, window_type, to_array) = match (source_type, ty) {
            (
                IrType::Window {
                    shape: crate::IrWindowShape::Slots,
                    element: source,
                    capacity: Some(source_length),
                },
                IrType::Array { element, length },
            ) if source == element && source_length == length => {
                (element, length, source_type, true)
            }
            (
                IrType::Array {
                    element: source,
                    length: source_length,
                },
                IrType::Window {
                    shape: crate::IrWindowShape::Slots,
                    element,
                    capacity: Some(length),
                },
            ) if source == element && source_length == length => (element, length, ty, false),
            _ => return Err(BackendFailure::InvalidIr),
        };
        let source = self.value_place(value)?;
        let destination = self.value_place(result)?;
        let window_llvm = llvm_type(self.program, window_type)?;
        // A `Slots<T, N>` is `{ i64 len, [N x T] }`, so the slots are field
        // one and the length is field zero.
        let (window_place, slots) = if to_array {
            (source.clone(), self.next_temporary()?)
        } else {
            (destination.clone(), self.next_temporary()?)
        };
        writeln!(
            self.output,
            "  %{slots} = getelementptr inbounds {window_llvm}, ptr {window_place}, i64 0, i32 1"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if !to_array {
            let filled = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{filled} = getelementptr inbounds {window_llvm}, ptr {destination}, i64 0, i32 0\n  store i64 {length}, ptr %{filled}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        // No element access is needed for the empty case.
        if length == 0 {
            return Ok(());
        }
        let element_type = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        let element_llvm = llvm_type(self.program, element_type)?;
        let length = length.to_string();
        let count = self.element_address_index(element_type, &length)?;
        self.intrinsics.insert(IntrinsicDeclaration::MemoryMove);
        if to_array {
            self.copy_element_range(&element_llvm, &format!("%{slots}"), &destination, count)
        } else {
            self.copy_element_range(&element_llvm, &source, &format!("%{slots}"), count)
        }
    }

    /// A bounded count of complete stride-spaced representations. Target
    /// layout supplies the stride; zero-byte elements transfer no bytes while
    /// ownership still moves with the whole source value.
    fn copy_element_range(
        &mut self,
        element_llvm: &str,
        source: &str,
        destination: &str,
        count: &str,
    ) -> Result<(), BackendFailure> {
        let end = self.next_temporary()?;
        let bytes = self.next_temporary()?;
        writeln!(self.output, "  %{end} = getelementptr {element_llvm}, ptr null, i64 {count}\n  %{bytes} = ptrtoint ptr %{end} to i64\n  call void @llvm.memmove.p0.p0.i64(ptr {destination}, ptr {source}, i64 %{bytes}, i1 false)")
            .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_array_fill(
        &mut self,
        result: IrValueId,
        ty: IrType,
        value: IrValueId,
        target_domain: IrTargetDomainObligation,
    ) -> Result<(), BackendFailure> {
        if target_domain != IrTargetDomainObligation::ElementAddress {
            return Err(BackendFailure::InvalidIr);
        }
        let IrType::Array { element, length } = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        let element_type = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        if self.value_type(value) != Some(element_type) {
            return Err(BackendFailure::InvalidIr);
        }

        let array_type = llvm_type(self.program, ty)?;
        let array_slot = self.value_place(result)?;
        let index_slot = self.entry_slot(FunctionSlot::ArrayFillIndex(result))?;
        let index = self.next_temporary()?;
        let in_range = self.next_temporary()?;
        let element_pointer = self.next_temporary()?;
        let next_index = self.next_temporary()?;
        let logical_index = format!("%{index}");
        let address_index = self.element_address_index(element_type, &logical_index)?;
        writeln!(
            self.output,
            "  store i64 0, ptr {index_slot}\n  br label %{}\n{}:\n  %{index} = load i64, ptr {index_slot}\n  %{in_range} = icmp ult i64 %{index}, {length}\n  br i1 %{in_range}, label %{}, label %{}\n{}:\n  %{element_pointer} = getelementptr inbounds {array_type}, ptr {array_slot}, i64 0, i64 {address_index}",
            array_fill_head_label(result),
            array_fill_head_label(result),
            array_fill_body_label(result),
            array_fill_done_label(result),
            array_fill_body_label(result),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.store_value_at(value, &format!("%{element_pointer}"))?;
        writeln!(
            self.output,
            "  %{next_index} = add i64 %{index}, 1\n  store i64 %{next_index}, ptr {index_slot}\n  br label %{}\n{}:",
            array_fill_head_label(result),
            array_fill_done_label(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Emits a discharged source subscript read [OP-4]: the checker derived
    /// the bounds obligation, so no compare, branch, or trap is emitted in
    /// any build mode.
    pub(super) fn emit_array_index(
        &mut self,
        result: IrValueId,
        ty: IrType,
        root: IrArrayRoot,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    ) -> Result<(), BackendFailure> {
        if target_domain != IrTargetDomainObligation::ElementAddress {
            return Err(BackendFailure::InvalidIr);
        }
        if self.value_type(offset)
            != Some(IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let root_type = match root {
            IrArrayRoot::Value(value) => self
                .function
                .value_type(value)
                .ok_or(BackendFailure::InvalidIr)?,
            IrArrayRoot::Constant(id) => self
                .program
                .constant(id)
                .ok_or(BackendFailure::InvalidIr)?
                .ty(),
        };
        let IrType::Array { element, .. } = root_type else {
            return Err(BackendFailure::InvalidIr);
        };
        if self.program.element(element) != Some(ty) {
            return Err(BackendFailure::InvalidIr);
        }

        let array_type = llvm_type(self.program, root_type)?;
        let root_pointer = match root {
            IrArrayRoot::Value(value) => self.value_place(value)?,
            IrArrayRoot::Constant(id) => {
                constant_symbol(self.program.constant(id).ok_or(BackendFailure::InvalidIr)?)
            }
        };
        let element_pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{element_pointer} = getelementptr inbounds {array_type}, ptr {root_pointer}, i64 0, i64 {}",
            self.element_address_index(ty, &self.value_name(offset))?,
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.load_place_result(result, ty, &format!("%{element_pointer}"))
    }
}
