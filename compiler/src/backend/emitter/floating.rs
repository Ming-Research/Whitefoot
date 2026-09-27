use super::*;

impl FunctionEmitter<'_, '_> {
    pub(super) fn emit_float(
        &mut self,
        result: IrValueId,
        result_type: IrType,
        operation: IrFloatOperation,
        operand_type: IrType,
        arguments: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        let width = match operand_type {
            IrType::Float { width } if matches!(width, 32 | 64) => width,
            _ => return Err(BackendFailure::InvalidIr),
        };
        let expected_arguments = float_operand_count(operation);
        if arguments.len() != expected_arguments
            || arguments
                .iter()
                .any(|argument| self.value_type(*argument) != Some(operand_type))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let comparison = matches!(
            operation,
            IrFloatOperation::Equal
                | IrFloatOperation::Less
                | IrFloatOperation::LessEqual
                | IrFloatOperation::Greater
                | IrFloatOperation::GreaterEqual
                | IrFloatOperation::NotEqual
        );
        if result_type
            != if comparison {
                IrType::Bool
            } else {
                operand_type
            }
        {
            return Err(BackendFailure::InvalidIr);
        }
        let llvm_ty = self.output.type_name(self.program, operand_type)?;
        let rendered_arguments = arguments
            .iter()
            .map(|argument| self.value_name(*argument))
            .collect::<Vec<_>>();
        match operation {
            IrFloatOperation::AddStrict
            | IrFloatOperation::SubtractStrict
            | IrFloatOperation::MultiplyStrict
            | IrFloatOperation::DivideStrict
            | IrFloatOperation::Remainder => {
                let opcode = match operation {
                    IrFloatOperation::AddStrict => "fadd",
                    IrFloatOperation::SubtractStrict => "fsub",
                    IrFloatOperation::MultiplyStrict => "fmul",
                    IrFloatOperation::DivideStrict => "fdiv",
                    IrFloatOperation::Remainder => "frem",
                    _ => return Err(BackendFailure::InvalidIr),
                };
                writeln!(
                    self.output,
                    "  {} = {opcode} {llvm_ty} {}, {}",
                    self.value_name(result),
                    rendered_arguments[0],
                    rendered_arguments[1]
                )
                .map_err(|_| BackendFailure::TextEmission)
            }
            IrFloatOperation::Equal
            | IrFloatOperation::Less
            | IrFloatOperation::LessEqual
            | IrFloatOperation::Greater
            | IrFloatOperation::GreaterEqual
            | IrFloatOperation::NotEqual => {
                let predicate = match operation {
                    IrFloatOperation::Equal => "oeq",
                    IrFloatOperation::Less => "olt",
                    IrFloatOperation::LessEqual => "ole",
                    IrFloatOperation::Greater => "ogt",
                    IrFloatOperation::GreaterEqual => "oge",
                    IrFloatOperation::NotEqual => "une",
                    _ => return Err(BackendFailure::InvalidIr),
                };
                writeln!(
                    self.output,
                    "  {} = fcmp {predicate} {llvm_ty} {}, {}",
                    self.value_name(result),
                    rendered_arguments[0],
                    rendered_arguments[1]
                )
                .map_err(|_| BackendFailure::TextEmission)
            }
            IrFloatOperation::Negate => writeln!(
                self.output,
                "  {} = fneg {llvm_ty} {}",
                self.value_name(result),
                rendered_arguments[0]
            )
            .map_err(|_| BackendFailure::TextEmission),
            IrFloatOperation::Absolute
            | IrFloatOperation::Floor
            | IrFloatOperation::Ceil
            | IrFloatOperation::Truncate
            | IrFloatOperation::RoundEven
            | IrFloatOperation::SquareRootStrict => {
                let intrinsic = match operation {
                    IrFloatOperation::Absolute => "fabs",
                    IrFloatOperation::Floor => "floor",
                    IrFloatOperation::Ceil => "ceil",
                    IrFloatOperation::Truncate => "trunc",
                    IrFloatOperation::RoundEven => "roundeven",
                    IrFloatOperation::SquareRootStrict => "sqrt",
                    _ => return Err(BackendFailure::InvalidIr),
                };
                let name = format!("llvm.{intrinsic}.f{width}");
                self.intrinsics.insert(IntrinsicDeclaration::Unary {
                    name: name.clone(),
                    ty: llvm_ty.clone(),
                });
                {
                    self.output.symbol(name.to_string());
                    writeln!(
                        self.output,
                        "  {} = call {llvm_ty} @{name}({llvm_ty} {})",
                        self.value_name(result),
                        rendered_arguments[0]
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)
            }
            IrFloatOperation::CopySign | IrFloatOperation::Minimum | IrFloatOperation::Maximum => {
                let intrinsic = match operation {
                    IrFloatOperation::CopySign => "copysign",
                    IrFloatOperation::Minimum => "minimum",
                    IrFloatOperation::Maximum => "maximum",
                    _ => return Err(BackendFailure::InvalidIr),
                };
                let name = format!("llvm.{intrinsic}.f{width}");
                self.intrinsics.insert(IntrinsicDeclaration::Binary {
                    name: name.clone(),
                    ty: llvm_ty.clone(),
                });
                {
                    self.output.symbol(name.to_string());
                    writeln!(
                        self.output,
                        "  {} = call {llvm_ty} @{name}({llvm_ty} {}, {llvm_ty} {})",
                        self.value_name(result),
                        rendered_arguments[0],
                        rendered_arguments[1]
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)
            }
            IrFloatOperation::FusedMultiplyAddStrict => {
                let name = format!("llvm.fma.f{width}");
                self.intrinsics.insert(IntrinsicDeclaration::Ternary {
                    name: name.clone(),
                    ty: llvm_ty.clone(),
                });
                {
                    self.output.symbol(name.to_string());
                    writeln!(
                        self.output,
                        "  {} = call {llvm_ty} @{name}({llvm_ty} {}, {llvm_ty} {}, {llvm_ty} {})",
                        self.value_name(result),
                        rendered_arguments[0],
                        rendered_arguments[1],
                        rendered_arguments[2]
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)
            }
            IrFloatOperation::Infinity | IrFloatOperation::Nan => {
                let bits = match (width, operation) {
                    (32, IrFloatOperation::Infinity) => 0x7f80_0000,
                    (32, IrFloatOperation::Nan) => 0x7fc0_0000,
                    (64, IrFloatOperation::Infinity) => 0x7ff0_0000_0000_0000,
                    (64, IrFloatOperation::Nan) => 0x7ff8_0000_0000_0000,
                    _ => return Err(BackendFailure::InvalidIr),
                };
                let constant = constant_operand(
                    IrConstant::Float {
                        ty: operand_type,
                        bits,
                    },
                    operand_type,
                )?;
                writeln!(
                    self.output,
                    "  {} = select i1 true, {llvm_ty} {constant}, {llvm_ty} {constant}",
                    self.value_name(result)
                )
                .map_err(|_| BackendFailure::TextEmission)
            }
        }
    }
}

const fn float_operand_count(operation: IrFloatOperation) -> usize {
    match operation {
        IrFloatOperation::Infinity | IrFloatOperation::Nan => 0,
        IrFloatOperation::Negate
        | IrFloatOperation::Absolute
        | IrFloatOperation::Floor
        | IrFloatOperation::Ceil
        | IrFloatOperation::Truncate
        | IrFloatOperation::RoundEven
        | IrFloatOperation::SquareRootStrict => 1,
        IrFloatOperation::FusedMultiplyAddStrict => 3,
        _ => 2,
    }
}
