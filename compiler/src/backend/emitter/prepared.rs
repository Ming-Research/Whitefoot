//! Preparation of owned key sources and input-indexed prepared atomic entries.
//!
//! The source remains two ordinary boxed Slots owners. Its private order is
//! relocatable: only the optional heap allocation's pointer is stored, and
//! an inline permutation never caches its own address.

use super::*;
use crate::{IrRecord, IrRecordKind};

impl FunctionEmitter<'_, '_> {
    fn prepared_type(&self, prepared: IrValueId) -> Result<(IrNominalId, IrType), BackendFailure> {
        let nominal = match self.value_type(prepared) {
            Some(IrType::Nominal(id) | IrType::Address(IrAddressed::Nominal(id))) => id,
            _ => return Err(BackendFailure::InvalidIr),
        };
        let IrNominalKind::PreparedKeys { fields } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let [source] = fields.as_slice() else {
            return Err(BackendFailure::InvalidIr);
        };
        Ok((nominal, source.ty()))
    }

    fn prepared_address(&mut self, prepared: IrValueId) -> Result<String, BackendFailure> {
        match self.value_type(prepared) {
            Some(IrType::Address(_)) => Ok(self.value_name(prepared)),
            Some(IrType::Nominal(_)) => self.value_place(prepared),
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    /// Reads the two Slots owners without copying their storage. Both use
    /// the normal [len, cap, elements] representation.
    fn key_source_parts(
        &mut self,
        source_ty: IrType,
        source: &str,
    ) -> Result<(String, String, String, String), BackendFailure> {
        let IrType::Nominal(id) = source_ty else {
            return Err(BackendFailure::InvalidIr);
        };
        let IrNominalKind::Struct { fields } = self.nominal(id)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if fields.len() != 2 {
            return Err(BackendFailure::InvalidIr);
        }
        for field in fields {
            let IrType::Nominal(owner) = field.ty() else {
                return Err(BackendFailure::InvalidIr);
            };
            if !matches!(
                self.nominal(owner)?.kind(),
                IrNominalKind::Box {
                    referent: IrType::Window {
                        shape: IrWindowShape::Slots,
                        capacity: None,
                        ..
                    },
                    ..
                }
            ) {
                return Err(BackendFailure::InvalidIr);
            }
        }
        let source_llvm = self.output.type_name(self.program, source_ty)?;
        let bytes = self.next_temporary()?;
        let spans = self.next_temporary()?;
        let data = self.next_temporary()?;
        let span_data = self.next_temporary()?;
        writeln!(self.output, "  %{bytes} = extractvalue {source_llvm} {source}, 0\n  %{spans} = extractvalue {source_llvm} {source}, 1\n  %{data} = getelementptr inbounds i8, ptr %{bytes}, i64 16\n  %{span_data} = getelementptr inbounds i8, ptr %{spans}, i64 16")
            .map_err(|_| BackendFailure::TextEmission)?;
        Ok((
            format!("%{data}"),
            format!("%{bytes}"),
            format!("%{span_data}"),
            format!("%{spans}"),
        ))
    }

    pub(super) fn emit_key_prepare(
        &mut self,
        result: IrValueId,
        ty: IrType,
        source: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrType::Nominal(result_id) = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        let IrNominalKind::Enum { variants } = self.nominal(result_id)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let [ok, err] = variants.as_slice() else {
            return Err(BackendFailure::InvalidIr);
        };
        let ([prepared], [error]) = (ok.fields(), err.fields()) else {
            return Err(BackendFailure::InvalidIr);
        };
        let (ok_tag, err_tag) = (ok.tag(), err.tag());
        let (IrType::Nominal(prepared_id), IrType::Nominal(error_id)) = (prepared.ty(), error.ty())
        else {
            return Err(BackendFailure::InvalidIr);
        };
        let IrNominalKind::PreparedKeys { fields } = self.nominal(prepared_id)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let [source_field] = fields.as_slice() else {
            return Err(BackendFailure::InvalidIr);
        };
        let source_ty = source_field.ty();
        if self.value_type(source) != Some(source_ty) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Enum { variants } = self.nominal(error_id)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let [invalid, duplicate] = variants.as_slice() else {
            return Err(BackendFailure::InvalidIr);
        };
        let (invalid_tag, duplicate_tag) = (invalid.tag(), duplicate.tag());
        if invalid.fields().len() != 2
            || duplicate.fields().len() != 3
            || invalid.fields()[0].ty() != source_ty
            || duplicate.fields()[0].ty() != source_ty
        {
            return Err(BackendFailure::InvalidIr);
        }
        // Snapshot the source's two pointers before this operation writes a
        // result destination that storage planning may reuse.
        let source_operand = self.value_operand(source)?;
        let source_llvm = self.output.type_name(self.program, source_ty)?;
        let (bytes, bytes_owner, spans, spans_owner) =
            self.key_source_parts(source_ty, &source_operand)?;
        let length = self.next_temporary()?;
        let count = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{length} = load i64, ptr {bytes_owner}\n  %{count} = load i64, ptr {spans_owner}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let length = format!("%{length}");
        let count = format!("%{count}");
        let ordinal = result.ordinal();
        let scratch = format!("%wf.prepare.v{ordinal}.order");
        let first = format!("%wf.prepare.v{ordinal}.first");
        let second = format!("%wf.prepare.v{ordinal}.second");
        let status = self.next_temporary()?;
        let ok_label = format!("keys.prepare.ok.v{ordinal}");
        let invalid_label = format!("keys.prepare.invalid.v{ordinal}");
        let duplicate_label = format!("keys.prepare.duplicate.v{ordinal}");
        let done = format!("keys.prepare.done.v{ordinal}");
        let destination = self.value_place(result)?;
        let result_llvm = self.output.type_name(self.program, ty)?;
        self.names(&["wf__key_prepare"]);
        writeln!(self.output, "  %{status} = call i32 @wf__key_prepare(ptr {scratch}, ptr {bytes}, i64 {length}, ptr {spans}, i64 {count}, ptr {first}, ptr {second})\n  store {result_llvm} zeroinitializer, ptr {destination}\n  switch i32 %{status}, label %{duplicate_label} [ i32 0, label %{ok_label} i32 1, label %{invalid_label} ]")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(ok_label);
        writeln!(self.output, "  store i32 {ok_tag}, ptr {destination}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let prepared = self.variant_field_pointer(result_id, ok_tag, 0, &destination)?;
        let source_dest =
            self.aggregate_field_pointer(IrType::Nominal(prepared_id), &prepared, 0)?;
        let order = self.aggregate_field_pointer(IrType::Nominal(prepared_id), &prepared, 1)?;
        writeln!(
            self.output,
            "  store {source_llvm} {source_operand}, ptr {source_dest}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.intrinsics.insert(IntrinsicDeclaration::MemoryCopy);
        self.output.symbol("llvm.memcpy.p0.p0.i64");
        writeln!(self.output, "  call void @llvm.memcpy.p0.p0.i64(ptr {order}, ptr {scratch}, i64 144, i1 false)\n  br label %{done}").map_err(|_| BackendFailure::TextEmission)?;
        for (label, tag, fields) in [
            (invalid_label, invalid_tag, 2),
            (duplicate_label, duplicate_tag, 3),
        ] {
            self.output.open_block(label);
            writeln!(self.output, "  store i32 {err_tag}, ptr {destination}")
                .map_err(|_| BackendFailure::TextEmission)?;
            let error = self.variant_field_pointer(result_id, err_tag, 0, &destination)?;
            writeln!(self.output, "  store i32 {tag}, ptr {error}")
                .map_err(|_| BackendFailure::TextEmission)?;
            let source_dest = self.variant_field_pointer(error_id, tag, 0, &error)?;
            writeln!(
                self.output,
                "  store {source_llvm} {source_operand}, ptr {source_dest}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            for (field, index_pointer) in [(1, &first), (2, &second)].into_iter().take(fields - 1) {
                let target = self.variant_field_pointer(error_id, tag, field, &error)?;
                let value = self.next_temporary()?;
                writeln!(
                    self.output,
                    "  %{value} = load i64, ptr {index_pointer}\n  store i64 %{value}, ptr {target}"
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            writeln!(self.output, "  br label %{done}")
                .map_err(|_| BackendFailure::TextEmission)?;
        }
        self.output.open_block(done);
        Ok(())
    }

    pub(super) fn emit_prepared_order_release(
        &mut self,
        result: IrValueId,
        prepared: IrValueId,
    ) -> Result<(), BackendFailure> {
        let (nominal, _) = self.prepared_type(prepared)?;
        let address = self.prepared_address(prepared)?;
        let order = self.aggregate_field_pointer(IrType::Nominal(nominal), &address, 1)?;
        self.emit_unit_call(result, "wf__key_order_free", &format!("ptr {order}"))
    }

    pub(super) fn emit_table_hold_prepared(
        &mut self,
        result: IrValueId,
        record: IrRecord,
        prepared: IrValueId,
        entries: IrValueId,
    ) -> Result<(), BackendFailure> {
        if record.kind() != IrRecordKind::TableHold
            || !matches!(
                self.value_type(entries),
                Some(IrType::Address(IrAddressed::KeyedEntries { .. }))
            )
        {
            return Err(BackendFailure::InvalidIr);
        }
        let (nominal, source_ty) = self.prepared_type(prepared)?;
        let address = self.prepared_address(prepared)?;
        let source_pointer = self.aggregate_field_pointer(IrType::Nominal(nominal), &address, 0)?;
        let source = self.next_temporary()?;
        let source_llvm = self.output.type_name(self.program, source_ty)?;
        writeln!(
            self.output,
            "  %{source} = load {source_llvm}, ptr {source_pointer}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let (bytes, _, spans, _) = self.key_source_parts(source_ty, &format!("%{source}"))?;
        let order = self.aggregate_field_pointer(IrType::Nominal(nominal), &address, 1)?;
        self.emit_unit_call(
            result,
            "wf__table_hold_prepared",
            &format!(
                "ptr %wf.record.{}, ptr {bytes}, ptr {spans}, ptr {order}, ptr {}",
                record.index(),
                self.value_name(entries)
            ),
        )
    }
}
