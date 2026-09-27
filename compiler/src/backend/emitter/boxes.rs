use std::fmt::Write;

use super::*;

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    pub(super) fn emit_box_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if self.value_type(value) != Some(*referent) {
            return Err(BackendFailure::InvalidIr);
        }
        let referent_type = self.output.type_name(self.program, *referent)?;
        let nonnull = self.next_temporary()?;
        let oom = format!("box.new.oom.v{}", result.ordinal());
        // The shared label helper keeps this block split visible to
        // `block_exit_label`, so a phi in a successor names the right
        // predecessor.
        let ready = box_new_ready_label(result);
        {
            let emission_argument_0 = self.value_name(result);
            let emission_argument_1 = self.value_name(result);

            {
                self.output.symbol("malloc");
                write!(
                    self.output,
                    "  {emission_argument_0} = call ptr @malloc(i64 ptrtoint (ptr getelementptr ({referent_type}, ptr null, i64 1) to i64))\n  %{nonnull} = icmp ne ptr {emission_argument_1}, null\n  br i1 %{nonnull}, label %{ready}, label %{oom}\n"
                )
            }?;
            self.output.open_block(oom.to_string());
            {
                self.output.symbol("wf_resource_abort");
                write!(
                    self.output,
                    "  call void @wf_resource_abort()\n  unreachable\n"
                )
            }?;
            self.output.open_block(ready.to_string());
        };
        let destination = self.value_name(result);
        self.store_value_at(value, &destination)
    }

    /// [S39, PROV-6] the destructuring consume of one cell: the referent is
    /// loaded out and the cell's own storage is released, which is a free on
    /// a general store and nothing at all on a bump extent, whose region
    /// reset reclaims the whole reservation.
    pub(super) fn emit_box_take(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrNominalKind::Box { referent, release } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if ty != *referent || self.value_type(value) != Some(IrType::Nominal(nominal)) {
            return Err(BackendFailure::InvalidIr);
        }
        let release = *release;
        {
            let emitted_type_1 = self.output.type_name(self.program, ty)?;
            writeln!(
                self.output,
                "  {} = load {}, ptr {}",
                self.value_name(result),
                emitted_type_1,
                self.value_name(value)
            )
        }
        .map_err(|_| BackendFailure::TextEmission)?;
        if release == crate::IrReleaseClass::General {
            {
                self.output.symbol("free");
                writeln!(
                    self.output,
                    "  call void @free(ptr {})",
                    self.value_name(value)
                )
            }
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(())
    }

    pub(super) fn emit_box_deref(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if ty != *referent || self.value_type(value) != Some(IrType::Nominal(nominal)) {
            return Err(BackendFailure::InvalidIr);
        }
        {
            let emitted_type_1 = self.output.type_name(self.program, ty)?;
            writeln!(
                self.output,
                "  {} = load {}, ptr {}",
                self.value_name(result),
                emitted_type_1,
                self.value_name(value)
            )
        }
        .map_err(|_| BackendFailure::TextEmission)
    }
}
