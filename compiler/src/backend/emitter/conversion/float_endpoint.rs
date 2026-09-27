use super::*;

impl FunctionEmitter<'_, '_> {
    pub(super) fn emit_float_endpoint_conversion(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        mode: IrConversionMode,
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        match (source_type, destination_type) {
            (IrType::Integer { .. }, IrType::Float { .. }) => self.emit_integer_to_float(
                result,
                result_type,
                mode,
                source_type,
                destination_type,
                value,
            ),
            (IrType::Float { .. }, IrType::Integer { .. }) => self.emit_float_to_integer(
                result,
                result_type,
                mode,
                source_type,
                destination_type,
                value,
            ),
            (IrType::Float { .. }, IrType::Float { .. }) => self.emit_float_to_float(
                result,
                result_type,
                mode,
                source_type,
                destination_type,
                value,
            ),
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    fn emit_integer_to_float(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        mode: IrConversionMode,
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let (
            IrType::Integer {
                width: source_width,
                signed: source_signed,
            },
            IrType::Float {
                width: destination_width,
            },
        ) = (source_type, destination_type)
        else {
            return Err(BackendFailure::InvalidIr);
        };
        if !matches!(source_width, 8 | 16 | 32 | 64) || !matches!(destination_width, 32 | 64) {
            return Err(BackendFailure::InvalidIr);
        }
        let total = (destination_width == 32 && source_width <= 16)
            || (destination_width == 64 && source_width <= 32);
        if total && mode == IrConversionMode::Defined {
            return self.emit_conversion_outcome(
                result,
                result_type,
                mode,
                destination_type,
                "",
                "true",
            );
        }
        let converted = if mode == IrConversionMode::Exact {
            self.value_name(result)
        } else {
            format!("%{}", self.next_temporary()?)
        };
        let source_ty = self.output.type_name(self.program, source_type)?;
        let destination_ty = self.output.type_name(self.program, destination_type)?;
        let opcode = if source_signed { "sitofp" } else { "uitofp" };
        writeln!(
            self.output,
            "  {converted} = {opcode} {source_ty} {} to {destination_ty}",
            self.value_name(value)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if mode == IrConversionMode::Exact {
            return Ok(());
        }
        if total {
            return self.emit_conversion_outcome(
                result,
                result_type,
                mode,
                destination_type,
                &converted,
                "true",
            );
        }

        let recovered = self.next_temporary()?;
        let equal = self.next_temporary()?;
        let below_maximum = self.next_temporary()?;
        let valid = self.next_temporary()?;
        let reverse = if source_signed { "fptosi" } else { "fptoui" };
        let intrinsic = format!("llvm.{reverse}.sat.i{source_width}.f{destination_width}");
        self.output.symbol(&intrinsic);
        self.intrinsics.insert(IntrinsicDeclaration::UnaryCast {
            name: intrinsic.clone(),
            result_ty: source_ty.clone(),
            argument_ty: destination_ty.clone(),
        });
        let maximum = if source_signed {
            (1_u128 << (source_width - 1)) - 1
        } else {
            (1_u128 << source_width) - 1
        };
        // A saturating reverse cast normally proves exactness. The one collision
        // is an unrepresentable integer maximum whose rounded float lies above
        // the source range and therefore saturates back to that same maximum.
        writeln!(
            self.output,
            "  %{recovered} = call {source_ty} @{intrinsic}({destination_ty} {converted})\n  %{equal} = icmp eq {source_ty} {}, %{recovered}\n  %{below_maximum} = icmp ne {source_ty} {}, {maximum}\n  %{valid} = and i1 %{equal}, %{below_maximum}",
            self.value_name(value),
            self.value_name(value)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_conversion_outcome(
            result,
            result_type,
            mode,
            destination_type,
            &converted,
            &format!("%{valid}"),
        )
    }

    fn emit_float_to_integer(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        mode: IrConversionMode,
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let (
            IrType::Float {
                width: source_width,
            },
            IrType::Integer {
                width: destination_width,
                signed: destination_signed,
            },
        ) = (source_type, destination_type)
        else {
            return Err(BackendFailure::InvalidIr);
        };
        if !matches!(source_width, 32 | 64) || !matches!(destination_width, 8 | 16 | 32 | 64) {
            return Err(BackendFailure::InvalidIr);
        }
        let source_ty = self.output.type_name(self.program, source_type)?;
        let destination_ty = self.output.type_name(self.program, destination_type)?;
        let opcode = if destination_signed {
            "fptosi"
        } else {
            "fptoui"
        };
        if mode == IrConversionMode::Exact {
            // The checked source obligation establishes the raw instruction's
            // complete finite, integral and in-range domain.
            return writeln!(
                self.output,
                "  {} = {opcode} {source_ty} {} to {destination_ty}",
                self.value_name(result),
                self.value_name(value)
            )
            .map_err(|_| BackendFailure::TextEmission);
        }
        let converted = self.next_temporary()?;
        let reverse = self.next_temporary()?;
        let equal = self.next_temporary()?;
        let intrinsic = format!("llvm.{opcode}.sat.i{destination_width}.f{source_width}");
        self.output.symbol(&intrinsic);
        self.intrinsics.insert(IntrinsicDeclaration::UnaryCast {
            name: intrinsic.clone(),
            result_ty: destination_ty.clone(),
            argument_ty: source_ty.clone(),
        });
        let return_opcode = if destination_signed {
            "sitofp"
        } else {
            "uitofp"
        };
        writeln!(
            self.output,
            "  %{converted} = call {destination_ty} @{intrinsic}({source_ty} {})\n  %{reverse} = {return_opcode} {destination_ty} %{converted} to {source_ty}\n  %{equal} = fcmp oeq {source_ty} {}, %{reverse}",
            self.value_name(value),
            self.value_name(value)
        )
        .map_err(|_| BackendFailure::TextEmission)?;

        let precision = if source_width == 32 { 24 } else { 53 };
        let maximum_bits = if destination_signed {
            destination_width - 1
        } else {
            destination_width
        };
        let valid = if maximum_bits <= precision {
            format!("%{equal}")
        } else {
            let below_maximum = self.next_temporary()?;
            let valid = self.next_temporary()?;
            let maximum = if destination_signed {
                (1_u128 << (destination_width - 1)) - 1
            } else {
                (1_u128 << destination_width) - 1
            };
            // When this maximum needs more significand bits than the source
            // float has, the first out-of-range power of two can saturate to it
            // and round back to the original float. Exclude that collision.
            writeln!(
                self.output,
                "  %{below_maximum} = icmp ne {destination_ty} %{converted}, {maximum}\n  %{valid} = and i1 %{equal}, %{below_maximum}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            format!("%{valid}")
        };
        self.emit_conversion_outcome(
            result,
            result_type,
            mode,
            destination_type,
            &format!("%{converted}"),
            &valid,
        )
    }

    fn emit_float_to_float(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        mode: IrConversionMode,
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let (
            IrType::Float {
                width: source_width,
            },
            IrType::Float {
                width: destination_width,
            },
        ) = (source_type, destination_type)
        else {
            return Err(BackendFailure::InvalidIr);
        };
        let opcode = match (source_width, destination_width) {
            (32, 64) => "fpext",
            (64, 32) => "fptrunc",
            _ => return Err(BackendFailure::InvalidIr),
        };
        let total = source_width < destination_width;
        if total && mode == IrConversionMode::Defined {
            return self.emit_conversion_outcome(
                result,
                result_type,
                mode,
                destination_type,
                "",
                "true",
            );
        }

        let source_ty = self.output.type_name(self.program, source_type)?;
        let destination_ty = self.output.type_name(self.program, destination_type)?;
        let converted = self.next_temporary()?;
        let nan = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{converted} = {opcode} {source_ty} {} to {destination_ty}\n  %{nan} = fcmp uno {source_ty} {}, {}",
            self.value_name(value),
            self.value_name(value),
            self.value_name(value)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let valid = if total || mode == IrConversionMode::Exact {
            "true".to_owned()
        } else {
            let widened = self.next_temporary()?;
            let exact = self.next_temporary()?;
            let valid = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{widened} = fpext {destination_ty} %{converted} to {source_ty}\n  %{exact} = fcmp oeq {source_ty} {}, %{widened}\n  %{valid} = or i1 %{nan}, %{exact}",
                self.value_name(value)
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            format!("%{valid}")
        };
        let selected = if mode == IrConversionMode::Defined {
            String::new()
        } else {
            let selected = if mode == IrConversionMode::Exact {
                self.value_name(result)
            } else {
                format!("%{}", self.next_temporary()?)
            };
            let canonical_nan = canonical_nan_operand(destination_type)?;
            writeln!(
                self.output,
                "  {selected} = select i1 %{nan}, {destination_ty} {canonical_nan}, {destination_ty} %{converted}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            selected
        };
        if mode == IrConversionMode::Exact {
            Ok(())
        } else {
            self.emit_conversion_outcome(
                result,
                result_type,
                mode,
                destination_type,
                &selected,
                &valid,
            )
        }
    }
}

fn canonical_nan_operand(ty: IrType) -> Result<String, BackendFailure> {
    let bits = match ty {
        IrType::Float { width: 32 } => 0x7fc0_0000,
        IrType::Float { width: 64 } => 0x7ff8_0000_0000_0000,
        _ => return Err(BackendFailure::InvalidIr),
    };
    constant_operand(IrConstant::Float { ty, bits }, ty)
}
