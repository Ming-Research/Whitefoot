mod float_endpoint;

use super::*;

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    pub(super) fn emit_numeric_conversion(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        mode: IrConversionMode,
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        if self.value_type(value) != Some(source_type)
            || (matches!(mode, IrConversionMode::Exact | IrConversionMode::Wrap)
                && result_type != destination_type)
            || (mode == IrConversionMode::Defined && result_type != IrType::Bool)
            || (mode == IrConversionMode::Wrap
                && !matches!(
                    (source_type, destination_type),
                    (IrType::Integer { .. }, IrType::Integer { .. })
                ))
        {
            return Err(BackendFailure::InvalidIr);
        }
        if source_type == destination_type {
            if !matches!(source_type, IrType::Integer { .. } | IrType::Float { .. }) {
                return Err(BackendFailure::InvalidIr);
            }
            if matches!(mode, IrConversionMode::Exact | IrConversionMode::Wrap) {
                let ty = llvm_type(self.program, source_type)?;
                // `select` is a representation copy. Floating arithmetic here
                // could quiet a signaling NaN or change a signed zero.
                return writeln!(
                    self.output,
                    "  {} = select i1 true, {ty} {}, {ty} {}",
                    self.value_name(result),
                    self.value_name(value),
                    self.value_name(value)
                )
                .map_err(|_| BackendFailure::TextEmission);
            }
            return self.emit_conversion_outcome(
                result,
                result_type,
                mode,
                destination_type,
                &self.value_name(value),
                "true",
            );
        }
        match (source_type, destination_type) {
            (IrType::Integer { .. }, IrType::Integer { .. }) => self.emit_integer_conversion(
                result,
                result_type,
                mode,
                source_type,
                destination_type,
                value,
            ),
            (IrType::Integer { .. }, IrType::Float { .. })
            | (IrType::Float { .. }, IrType::Integer { .. })
            | (IrType::Float { .. }, IrType::Float { .. }) => self.emit_float_endpoint_conversion(
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

    fn emit_integer_conversion(
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
            IrType::Integer {
                width: destination_width,
                signed: destination_signed,
            },
        ) = (source_type, destination_type)
        else {
            return Err(BackendFailure::InvalidIr);
        };
        let total = source_width < destination_width
            && (source_signed == destination_signed || (!source_signed && destination_signed));
        if matches!(mode, IrConversionMode::Exact | IrConversionMode::Wrap) {
            return self.emit_integer_cast(
                &self.value_name(result),
                value,
                source_width,
                source_signed,
                destination_width,
            );
        }
        let valid = if total {
            "true".to_owned()
        } else {
            format!(
                "%{}",
                self.emit_integer_conversion_validity(
                    value,
                    source_width,
                    source_signed,
                    destination_width,
                    destination_signed,
                )?
            )
        };
        let converted = if mode == IrConversionMode::Checked {
            let converted = format!("%{}", self.next_temporary()?);
            self.emit_integer_cast(
                &converted,
                value,
                source_width,
                source_signed,
                destination_width,
            )?;
            converted
        } else {
            // A domain-only integer conversion needs no converted payload.
            String::new()
        };
        self.emit_conversion_outcome(
            result,
            result_type,
            mode,
            destination_type,
            &converted,
            &valid,
        )
    }

    fn emit_conversion_outcome(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        mode: IrConversionMode,
        destination_type: IrType,
        converted: &str,
        valid: &str,
    ) -> Result<(), BackendFailure> {
        match mode {
            IrConversionMode::Checked => {
                self.emit_narrow_result(result, result_type, destination_type, converted, valid)
            }
            IrConversionMode::Defined if result_type == IrType::Bool => writeln!(
                self.output,
                "  {} = or i1 {valid}, false",
                self.value_name(result)
            )
            .map_err(|_| BackendFailure::TextEmission),
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    fn emit_narrow_result(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        destination_type: IrType,
        converted: &str,
        valid: &str,
    ) -> Result<(), BackendFailure> {
        let error_type = self.checked_result_error_type(result_type, destination_type, &[0])?;
        let result_ty = llvm_type(self.program, result_type)?;
        let destination_ty = llvm_type(self.program, destination_type)?;
        let error_ty = llvm_type(self.program, error_type)?;
        let ok_tag = self.next_temporary()?;
        let ok_value = self.next_temporary()?;
        let error_tag = self.next_temporary()?;
        let error_value = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{ok_tag} = insertvalue {result_ty} zeroinitializer, i32 0, 0\n  %{ok_value} = insertvalue {result_ty} %{ok_tag}, {destination_ty} {converted}, 1\n  %{error_tag} = insertvalue {result_ty} zeroinitializer, i32 1, 0\n  %{error_value} = insertvalue {result_ty} %{error_tag}, {error_ty} 0, 2\n  {} = select i1 {valid}, {result_ty} %{ok_value}, {result_ty} %{error_value}",
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    fn emit_integer_cast(
        &mut self,
        result: &str,
        value: IrValueId,
        source_width: u8,
        source_signed: bool,
        destination_width: u8,
    ) -> Result<(), BackendFailure> {
        let source = self.value_name(value);
        let opcode = if source_width > destination_width {
            "trunc"
        } else if source_width < destination_width && source_signed {
            "sext"
        } else if source_width < destination_width {
            "zext"
        } else {
            return writeln!(self.output, "  {result} = or i{source_width} {source}, 0")
                .map_err(|_| BackendFailure::TextEmission);
        };
        writeln!(
            self.output,
            "  {result} = {opcode} i{source_width} {source} to i{destination_width}"
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    fn emit_integer_conversion_validity(
        &mut self,
        value: IrValueId,
        source_width: u8,
        source_signed: bool,
        destination_width: u8,
        destination_signed: bool,
    ) -> Result<String, BackendFailure> {
        let value = self.value_name(value);
        if source_signed == destination_signed {
            if source_width <= destination_width {
                return Err(BackendFailure::InvalidIr);
            }
            if source_signed {
                let minimum = -(1_i128 << (destination_width - 1));
                let maximum = (1_i128 << (destination_width - 1)) - 1;
                let above_minimum = self.next_temporary()?;
                let below_maximum = self.next_temporary()?;
                let valid = self.next_temporary()?;
                writeln!(
                    self.output,
                    "  %{above_minimum} = icmp sge i{source_width} {value}, {minimum}\n  %{below_maximum} = icmp sle i{source_width} {value}, {maximum}\n  %{valid} = and i1 %{above_minimum}, %{below_maximum}"
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                return Ok(valid);
            }
            let maximum = (1_u128 << destination_width) - 1;
            let valid = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{valid} = icmp ule i{source_width} {value}, {maximum}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            return Ok(valid);
        }
        if source_signed {
            let nonnegative = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{nonnegative} = icmp sge i{source_width} {value}, 0"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            if destination_width >= source_width {
                return Ok(nonnegative);
            }
            let maximum = (1_u128 << destination_width) - 1;
            let below_maximum = self.next_temporary()?;
            let valid = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{below_maximum} = icmp sle i{source_width} {value}, {maximum}\n  %{valid} = and i1 %{nonnegative}, %{below_maximum}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            return Ok(valid);
        }
        if destination_width > source_width {
            return Err(BackendFailure::InvalidIr);
        }
        let maximum = (1_u128 << (destination_width - 1)) - 1;
        let valid = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{valid} = icmp ule i{source_width} {value}, {maximum}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(valid)
    }
}
