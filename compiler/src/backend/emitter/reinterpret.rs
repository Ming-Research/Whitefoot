use super::*;

impl FunctionEmitter<'_, '_> {
    pub(super) fn emit_reinterpret(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        if result_type != destination_type
            || source_type == destination_type
            || self.value_type(value) != Some(source_type)
        {
            return Err(BackendFailure::InvalidIr);
        }
        let opcode = match (source_type, destination_type) {
            (
                IrType::Integer {
                    width: source_width,
                    signed: source_signed,
                },
                IrType::Integer {
                    width: destination_width,
                    signed: destination_signed,
                },
            ) if source_width == destination_width && source_signed != destination_signed => {
                return writeln!(
                    self.output,
                    "  {} = or i{source_width} {}, 0",
                    self.value_name(result),
                    self.value_name(value)
                )
                .map_err(|_| BackendFailure::TextEmission);
            }
            (
                IrType::Integer {
                    width: source_width,
                    ..
                },
                IrType::Float {
                    width: destination_width,
                },
            )
            | (
                IrType::Float {
                    width: source_width,
                },
                IrType::Integer {
                    width: destination_width,
                    ..
                },
            ) if source_width == destination_width && matches!(source_width, 32 | 64) => "bitcast",
            _ => return Err(BackendFailure::InvalidIr),
        };
        {
            let emitted_type_1 = self.output.type_name(self.program, source_type)?;
            let emitted_type_3 = self.output.type_name(self.program, destination_type)?;
            writeln!(
                self.output,
                "  {} = {opcode} {} {} to {}",
                self.value_name(result),
                emitted_type_1,
                self.value_name(value),
                emitted_type_3
            )
        }
        .map_err(|_| BackendFailure::TextEmission)
    }
}
