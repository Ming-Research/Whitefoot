//! Physical destinations for immutable aggregate IR values.
//!
//! Source ownership and initialization have already been checked. The storage
//! plan can reuse dead value storage, but reads still produce value snapshots;
//! only explicit address operations expose a mutable place. This module owns
//! the bridge between those two representations and the internal call ABI.

use super::*;

pub(in crate::backend) fn returned_storage_slot(
    function: &IrFunction,
    storage: &FunctionStoragePlan,
) -> Option<usize> {
    let mut returned = None;
    for block in function.blocks() {
        if let IrTerminator::Return { value, .. } = block.terminator() {
            let slot = storage.slot(*value)?;
            if returned.is_some_and(|previous| previous != slot) {
                return None;
            }
            returned = Some(slot);
        }
    }
    let returned = returned?;
    // Keep the complete parent allocation when returning one of its fields;
    // the caller's result contract supplies only the returned child's extent.
    if storage.allocation_root(returned) != returned {
        return None;
    }
    let mut parameters = function
        .parameters()
        .iter()
        .enumerate()
        .filter(|(_, (value, _))| {
            storage
                .slot(*value)
                .is_some_and(|slot| storage.allocation_root(slot) == returned)
        });
    let Some((ordinal, _)) = parameters.next() else {
        return Some(returned);
    };
    // All other indirect inputs reach private storage before this group's
    // entry transfer writes the result or a field within it. The result may alias any consumed
    // argument, not necessarily this parameter. Keep that last transfer:
    // the same ABI also admits an independent result destination.
    // Source roles, complete CFG interference and exposed-storage exclusion
    // remain independent prerequisites; a matching representation grants none.
    let signature = function.source_signature()?;
    if parameters.next().is_some()
        || signature.parameters().get(ordinal) != Some(&crate::IrSourceMode::Own)
        || signature.result() != crate::IrSourceMode::Own
        || storage.is_exposed(returned)
        || !function.overlaps().is_empty()
    {
        return None;
    }
    Some(returned)
}

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    /// Preserve the logical index everywhere except address formation. A
    /// zero-stride step uses zero even in facts-off emission, so the actual
    /// GEP operand has an exact target-domain representation [STOR-6].
    pub(super) fn element_address_index<'index>(
        &self,
        element: IrType,
        index: &'index str,
    ) -> Result<&'index str, BackendFailure> {
        if crate::target::element_has_zero_stride(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?
        {
            Ok("0")
        } else {
            Ok(index)
        }
    }

    pub(super) fn emit_place_definition(
        &mut self,
        result: IrValueId,
        ty: IrType,
        operation: &IrOperation,
    ) -> Result<bool, BackendFailure> {
        match operation {
            IrOperation::AddressOf { value, referent } => {
                self.emit_address_of(result, ty, *value, *referent)?;
            }
            IrOperation::Call {
                function,
                arguments,
            } if !self.overlap_handed_out.contains(&result) => {
                self.emit_call(result, ty, *function, arguments)?;
            }
            IrOperation::Window => self.emit_fixed_vector(result, ty)?,
            IrOperation::ArrayFill {
                value,
                target_domain,
            } => {
                self.emit_array_fill(result, ty, *value, *target_domain)?;
            }
            IrOperation::ArrayIndex {
                root,
                offset,
                target_domain,
            } => {
                self.emit_array_index(result, ty, *root, *offset, *target_domain)?;
            }
            IrOperation::FullArrayConversion { value } => {
                self.emit_full_array_conversion(result, ty, *value)?;
            }
            IrOperation::RunIndex {
                run,
                offset,
                target_domain,
            } => {
                self.emit_run_index(result, ty, *run, *offset, *target_domain)?;
            }
            IrOperation::RunTaken { row, run } => self.emit_run_taken(result, ty, *row, *run)?,
            IrOperation::RunBoundary { row, run, value } => {
                self.emit_run_boundary(result, ty, *row, *run, *value)?;
            }
            IrOperation::ContainerMeasure { measure, container } => {
                self.emit_container_measure(result, ty, *measure, *container)?;
            }
            IrOperation::SliceFromRun { run } => self.emit_slice_from_run(result, ty, *run)?,
            IrOperation::ProjectAddress {
                address,
                projection,
            } => {
                self.emit_project_address(result, ty, *address, projection)?;
            }
            IrOperation::Load { address, referent } if self.storage.slot(result).is_some() => {
                if ty != referent.ty()
                    || self.value_type(*address) != Some(IrType::Address(*referent))
                {
                    return Err(BackendFailure::InvalidIr);
                }
                self.load_place_result(result, ty, &self.value_name(*address))?;
            }
            IrOperation::ConstructStruct { nominal, fields } => {
                if ty != IrType::Nominal(*nominal) {
                    return Err(BackendFailure::InvalidIr);
                }
                let IrNominalKind::Struct { fields: declared } = self.nominal(*nominal)?.kind()
                else {
                    return Err(BackendFailure::InvalidIr);
                };
                if declared.len() != fields.len()
                    || declared
                        .iter()
                        .zip(fields)
                        .any(|(field, value)| self.value_type(*value) != Some(field.ty()))
                {
                    return Err(BackendFailure::InvalidIr);
                }
                self.construct_at(
                    result,
                    ty,
                    None,
                    fields.iter().copied().enumerate().collect(),
                )?;
            }
            IrOperation::ConstructEnum {
                nominal,
                variant,
                fields,
            } if self.storage.slot(result).is_some() => {
                if ty != IrType::Nominal(*nominal) {
                    return Err(BackendFailure::InvalidIr);
                }
                let IrNominalKind::Enum { variants } = self.nominal(*nominal)?.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                let selected = variants
                    .iter()
                    .find(|candidate| candidate.tag() == *variant)
                    .ok_or(BackendFailure::InvalidIr)?;
                if selected.fields().len() != fields.len()
                    || selected
                        .fields()
                        .iter()
                        .zip(fields)
                        .any(|(field, value)| self.value_type(*value) != Some(field.ty()))
                {
                    return Err(BackendFailure::InvalidIr);
                }
                let base = variant_field_base(variants, *variant)?;
                self.construct_at(
                    result,
                    ty,
                    Some(*variant),
                    fields
                        .iter()
                        .enumerate()
                        .map(|(index, value)| (base + index, *value))
                        .collect(),
                )?;
            }
            IrOperation::ProjectStruct {
                aggregate,
                nominal,
                field,
                consume_root,
            } => {
                let IrNominalKind::Struct { fields } = self.nominal(*nominal)?.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                if self.value_type(*aggregate) != Some(IrType::Nominal(*nominal))
                    || fields.get(*field as usize).map(|field| field.ty()) != Some(ty)
                {
                    return Err(BackendFailure::InvalidIr);
                }
                let source = self.value_place(*aggregate)?;
                let address = self.aggregate_field_pointer(
                    IrType::Nominal(*nominal),
                    &source,
                    *field as usize,
                )?;
                self.load_place_result(result, ty, &address)?;
                if *consume_root {
                    writeln!(self.output, "  ; ownership-consuming projection")
                        .map_err(|_| BackendFailure::TextEmission)?;
                }
            }
            IrOperation::ProjectVariant {
                aggregate,
                nominal,
                variant,
                field,
            } => {
                let IrNominalKind::Enum { variants } = self.nominal(*nominal)?.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                let selected = variants
                    .iter()
                    .find(|candidate| candidate.tag() == *variant)
                    .ok_or(BackendFailure::InvalidIr)?;
                if self.value_type(*aggregate) != Some(IrType::Nominal(*nominal))
                    || selected
                        .fields()
                        .get(*field as usize)
                        .map(|field| field.ty())
                        != Some(ty)
                {
                    return Err(BackendFailure::InvalidIr);
                }
                let index = variant_field_base(variants, *variant)? + *field as usize;
                let source = self.value_place(*aggregate)?;
                let address =
                    self.aggregate_field_pointer(IrType::Nominal(*nominal), &source, index)?;
                self.load_place_result(result, ty, &address)?;
            }
            IrOperation::InsertStruct {
                aggregate,
                nominal,
                field,
                value,
            } => {
                let IrNominalKind::Struct { fields } = self.nominal(*nominal)?.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                if ty != IrType::Nominal(*nominal)
                    || self.value_type(*aggregate) != Some(ty)
                    || fields.get(*field as usize).map(|field| field.ty())
                        != self.value_type(*value)
                {
                    return Err(BackendFailure::InvalidIr);
                }
                // Read the replacement before a coalesced destination write.
                let replacement = self.value_operand(*value)?;
                let destination = self.value_place(result)?;
                let source = self.value_place(*aggregate)?;
                self.copy_storage(ty, &source, &destination)?;
                let field_address =
                    self.aggregate_field_pointer(ty, &destination, *field as usize)?;
                let field_type = self.value_type(*value).ok_or(BackendFailure::InvalidIr)?;
                writeln!(
                    self.output,
                    "  store {} {replacement}, ptr {field_address}",
                    llvm_type(self.program, field_type)?
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn construct_at(
        &mut self,
        result: IrValueId,
        ty: IrType,
        tag: Option<u32>,
        fields: Vec<(usize, IrValueId)>,
    ) -> Result<(), BackendFailure> {
        let destination = self.value_place(result)?;
        writeln!(
            self.output,
            "  store {} zeroinitializer, ptr {destination}",
            llvm_type(self.program, ty)?
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if let Some(tag) = tag {
            let address = self.aggregate_field_pointer(ty, &destination, 0)?;
            writeln!(self.output, "  store i32 {tag}, ptr {address}")
                .map_err(|_| BackendFailure::TextEmission)?;
        }
        for (field, value) in fields {
            let address = self.aggregate_field_pointer(ty, &destination, field)?;
            self.store_value_at(value, &address)?;
        }
        Ok(())
    }

    pub(super) fn emit_place_edge(
        &mut self,
        target: IrBlockId,
        arguments: &[IrValueId],
        drops: &[IrDrop],
    ) -> Result<(), BackendFailure> {
        let parameters = self.block(target)?.parameters().to_vec();
        let mut transfers = Vec::new();
        for ((parameter, ty), argument) in parameters.iter().zip(arguments) {
            if self.storage.slot(*parameter).is_some()
                && self.storage.slot(*parameter) != self.storage.slot(*argument)
            {
                let operand = self.value_operand(*argument)?;
                transfers.push((*parameter, *ty, operand));
            }
        }
        // Cleanup still reads predecessor snapshots. A phi destination may
        // reuse their storage only after those final reads have completed.
        self.emit_drops(drops)?;
        for (parameter, ty, operand) in transfers {
            let destination = self.value_place(parameter)?;
            writeln!(
                self.output,
                "  store {} {operand}, ptr {destination}",
                llvm_type(self.program, ty)?,
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(())
    }

    pub(super) fn emit_project_address(
        &mut self,
        result: IrValueId,
        ty: IrType,
        address: IrValueId,
        projection: &crate::IrPlaceStep,
    ) -> Result<(), BackendFailure> {
        let pointer = self.projected_address_pointer(ty, address, projection)?;
        writeln!(
            self.output,
            "  {} = getelementptr i8, ptr {pointer}, i64 0",
            value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn projected_address_pointer(
        &mut self,
        ty: IrType,
        address: IrValueId,
        projection: &crate::IrPlaceStep,
    ) -> Result<String, BackendFailure> {
        let Some(IrType::Address(base)) = self.value_type(address) else {
            return Err(BackendFailure::InvalidIr);
        };
        let IrType::Address(referent) = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        let pointer = match projection {
            crate::IrPlaceStep::Field { nominal, field } => {
                let IrNominalKind::Struct { fields } = self.nominal(*nominal)?.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                if base.ty() != IrType::Nominal(*nominal)
                    || fields.get(*field as usize).map(|field| field.ty()) != Some(referent.ty())
                {
                    return Err(BackendFailure::InvalidIr);
                }
                self.aggregate_field_pointer(base.ty(), &self.value_name(address), *field as usize)?
            }
            crate::IrPlaceStep::BoxReferent { nominal } => {
                let IrNominalKind::Box {
                    referent: boxed, ..
                } = self.nominal(*nominal)?.kind()
                else {
                    return Err(BackendFailure::InvalidIr);
                };
                if base.ty() != IrType::Nominal(*nominal) || *boxed != referent.ty() {
                    return Err(BackendFailure::InvalidIr);
                }
                let pointer = self.next_temporary()?;
                writeln!(
                    self.output,
                    "  %{pointer} = load ptr, ptr {}",
                    self.value_name(address)
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                format!("%{pointer}")
            }
            crate::IrPlaceStep::EnumVariant {
                nominal,
                variant,
                field,
            } => {
                let IrNominalKind::Enum { variants } = self.nominal(*nominal)?.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                let selected = variants
                    .iter()
                    .find(|candidate| candidate.tag() == *variant)
                    .ok_or(BackendFailure::InvalidIr)?;
                if base.ty() != IrType::Nominal(*nominal)
                    || selected
                        .fields()
                        .get(*field as usize)
                        .map(|field| field.ty())
                        != Some(referent.ty())
                {
                    return Err(BackendFailure::InvalidIr);
                }
                let index = variant_field_base(variants, *variant)? + *field as usize;
                self.aggregate_field_pointer(base.ty(), &self.value_name(address), index)?
            }
            crate::IrPlaceStep::RunElement {
                offset,
                target_domain,
            } => self.run_element_place(address, *offset, referent.ty(), *target_domain)?,
            crate::IrPlaceStep::ArrayElement {
                offset,
                target_domain,
            } => {
                let IrType::Array { element, .. } = base.ty() else {
                    return Err(BackendFailure::InvalidIr);
                };
                if self.program.element(element) != Some(referent.ty())
                    || *target_domain != IrTargetDomainObligation::ElementAddress
                    || self.value_type(*offset)
                        != Some(IrType::Integer {
                            width: 64,
                            signed: false,
                        })
                {
                    return Err(BackendFailure::InvalidIr);
                }
                let pointer = self.next_temporary()?;
                writeln!(
                    self.output,
                    "  %{pointer} = getelementptr inbounds {}, ptr {}, i64 0, i64 {}",
                    llvm_type(self.program, base.ty())?,
                    self.value_name(address),
                    self.element_address_index(referent.ty(), &self.value_name(*offset))?
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                format!("%{pointer}")
            }
            crate::IrPlaceStep::BufferElement {
                offset,
                target_domain,
            } => {
                let IrType::Buffer { element } = base.ty() else {
                    return Err(BackendFailure::InvalidIr);
                };
                if self.program.element(element) != Some(referent.ty())
                    || *target_domain != IrTargetDomainObligation::ElementAddress
                    || self.value_type(*offset)
                        != Some(IrType::Integer {
                            width: 64,
                            signed: false,
                        })
                {
                    return Err(BackendFailure::InvalidIr);
                }
                let (block, _) = self.buffer_block(address)?;
                self.buffer_element_pointer(
                    block,
                    &self.value_name(address),
                    &self.value_name(*offset),
                )?
            }
        };
        Ok(pointer)
    }

    /// Resolve the binding's ordinary backing storage.
    pub(super) fn binding_place(&mut self, value: IrValueId) -> Result<String, BackendFailure> {
        self.entry_slot(FunctionSlot::Address(value))
    }

    pub(super) fn value_place(&mut self, value: IrValueId) -> Result<String, BackendFailure> {
        let slot = self.storage.slot(value).ok_or(BackendFailure::InvalidIr)?;
        if let Some(field) = self.storage.field_destination(slot) {
            let parent = self.slot_place(field.parent_slot)?;
            return self.aggregate_field_pointer(
                IrType::Nominal(field.nominal),
                &parent,
                field.field as usize,
            );
        }
        self.slot_place(slot)
    }

    fn slot_place(&mut self, slot: usize) -> Result<String, BackendFailure> {
        if let Some(destination) = self.storage.destination(slot) {
            self.binding_place(destination)
        } else if Some(slot) == self.result_slot {
            Ok(RESULT_POINTER.to_owned())
        } else {
            self.entry_slot(FunctionSlot::OwnedValue(slot))
        }
    }

    pub(super) fn value_operand(&mut self, value: IrValueId) -> Result<String, BackendFailure> {
        if self.storage.slot(value).is_none() {
            return Ok(self.value_name(value));
        }
        let temporary = self.next_temporary()?;
        let ty = self.value_type(value).ok_or(BackendFailure::InvalidIr)?;
        let address = self.value_place(value)?;
        writeln!(
            self.output,
            "  %{temporary} = load {}, ptr {address}",
            llvm_type(self.program, ty)?,
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{temporary}"))
    }

    pub(super) fn materialize_operands(
        &mut self,
        values: impl IntoIterator<Item = IrValueId>,
    ) -> Result<(), BackendFailure> {
        self.materialized.clear();
        for value in values {
            if self.storage.slot(value).is_some() && !self.materialized.contains_key(&value) {
                let operand = self.value_operand(value)?;
                self.materialized.insert(value, operand);
            }
        }
        Ok(())
    }

    pub(super) fn copy_storage(
        &mut self,
        ty: IrType,
        source: &str,
        destination: &str,
    ) -> Result<(), BackendFailure> {
        if source == destination {
            return Ok(());
        }
        let llvm = llvm_type(self.program, ty)?;
        // Keep the checked snapshot and its ordering, but do not expand an
        // aggregate into SSA fields merely to copy it. The target's allocated
        // type size includes representation padding and is not the source
        // layout ceiling or a run's initialized length. The closed OP-11 body
        // copies only between its equal-or-disjoint reference targets and its
        // private snapshots. Ordinary llvm.memcpy permits equal pointers (the
        // stricter memcpy.inline does not), so it preserves same-place swap
        // without withholding the proved exclusion of partial overlap. This
        // grants no noalias attribute to swap's reference parameters.
        // Other bodies may reuse overlapping aggregate result storage and keep
        // memmove's snapshot semantics.
        let operation = if aliasing_admitted_row(self.function.name()) {
            self.intrinsics.insert(IntrinsicDeclaration::MemoryCopy);
            "memcpy"
        } else {
            self.intrinsics.insert(IntrinsicDeclaration::MemoryMove);
            "memmove"
        };
        writeln!(
            self.output,
            "  call void @llvm.{operation}.p0.p0.i64(ptr {destination}, ptr {source}, i64 ptrtoint (ptr getelementptr ({llvm}, ptr null, i32 1) to i64), i1 false)"
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn store_value_at(
        &mut self,
        value: IrValueId,
        destination: &str,
    ) -> Result<(), BackendFailure> {
        let ty = self.value_type(value).ok_or(BackendFailure::InvalidIr)?;
        if is_stored_aggregate(self.program, ty)? {
            let source = self.value_place(value)?;
            return self.copy_storage(ty, &source, destination);
        }
        writeln!(
            self.output,
            "  store {} {}, ptr {destination}",
            llvm_type(self.program, ty)?,
            self.value_name(value),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn save_value_result(&mut self, result: IrValueId) -> Result<(), BackendFailure> {
        if self.storage.slot(result).is_none() {
            return Ok(());
        }
        let ty = self.value_type(result).ok_or(BackendFailure::InvalidIr)?;
        let destination = self.value_place(result)?;
        writeln!(
            self.output,
            "  store {} {}, ptr {destination}",
            llvm_type(self.program, ty)?,
            value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn aggregate_field_pointer(
        &mut self,
        ty: IrType,
        address: &str,
        field: usize,
    ) -> Result<String, BackendFailure> {
        let pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = getelementptr inbounds {}, ptr {address}, i32 0, i32 {field}",
            llvm_type(self.program, ty)?,
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{pointer}"))
    }

    pub(super) fn load_place_result(
        &mut self,
        result: IrValueId,
        ty: IrType,
        address: &str,
    ) -> Result<(), BackendFailure> {
        if self.storage.slot(result).is_some() {
            let destination = self.value_place(result)?;
            return self.copy_storage(ty, address, &destination);
        }
        writeln!(
            self.output,
            "  {} = load {}, ptr {address}",
            value_name(result),
            llvm_type(self.program, ty)?,
        )
        .map_err(|_| BackendFailure::TextEmission)
    }
}
