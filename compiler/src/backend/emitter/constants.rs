//! Read-only globals: the shared map's zeroed none entries and each
//! program constant rendered as rodata [CONST-2 candidate].

use super::*;

pub(super) fn emit_global_constants(
    output: &mut Module,
    program: &IrProgram,
) -> Result<(), BackendFailure> {
    let mut none_types = BTreeSet::new();
    for nominal in program.nominals() {
        if let IrNominalKind::Shared {
            shape: IrShared::Map {
                entry: IrType::Nominal(option),
            },
            ..
        } = nominal.kind()
            && none_types.insert(option.index())
        {
            let option = program.nominal(*option).ok_or(BackendFailure::InvalidIr)?;
            let mut references = References::default();
            let ty = llvm_type_with_references(
                program,
                IrType::Nominal(option.id()),
                &mut references.types,
            )?;
            output.global(
                format!(".wf_table_none.{}", option.link_name()),
                "unnamed_addr constant",
                ty,
                "zeroinitializer".to_owned(),
                None,
                references,
            );
        }
    }
    places::emit_measure_constants(output, program);
    for constant in program.constants() {
        output.text(format!("; const {}\n", constant.name()));
        let mut references = References::default();
        let ty = llvm_type_with_references(program, constant.ty(), &mut references.types)?;
        let value =
            global_constant_value(program, constant.value(), constant.ty(), &mut references)?;
        output.global(
            format!(".wf_const.{}", constant.link_name()),
            "unnamed_addr constant",
            ty,
            value,
            None,
            references,
        );
    }
    if !program.constants().is_empty() {
        output.text("\n");
    }
    Ok(())
}

/// Renders one rodata constant value of one exact type: a scalar operand, a
/// complete array, or a complete struct aggregate with each field rendered
/// recursively [CONST-2 candidate].
fn global_constant_value(
    program: &IrProgram,
    value: &IrGlobalValue,
    ty: IrType,
    references: &mut References,
) -> Result<String, BackendFailure> {
    match (value, ty) {
        (IrGlobalValue::Scalar(value), ty) => constant_operand(*value, ty),
        (IrGlobalValue::Array(elements), IrType::Array { element, length }) => {
            if u64::try_from(elements.len()).map_err(|_| BackendFailure::CounterOverflow)? != length
            {
                return Err(BackendFailure::InvalidIr);
            }
            if elements.is_empty() {
                return Ok("zeroinitializer".to_owned());
            }
            let mut text = String::from("[");
            let element_type = program.element(element).ok_or(BackendFailure::InvalidIr)?;
            let llvm_element_type =
                llvm_type_with_references(program, element_type, &mut references.types)?;
            for (index, value) in elements.iter().enumerate() {
                if index != 0 {
                    text.push_str(", ");
                }
                write!(
                    text,
                    "{llvm_element_type} {}",
                    global_constant_value(program, value, element_type, references)?
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            text.push(']');
            Ok(text)
        }
        (IrGlobalValue::Struct(fields), IrType::Nominal(id)) => {
            let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
            let IrNominalKind::Struct { fields: declared } = nominal.kind() else {
                return Err(BackendFailure::InvalidIr);
            };
            if fields.len() != declared.len() {
                return Err(BackendFailure::InvalidIr);
            }
            if fields.is_empty() {
                return Ok("zeroinitializer".to_owned());
            }
            let mut text = String::from("{ ");
            for (index, (value, field)) in fields.iter().zip(declared).enumerate() {
                if index != 0 {
                    text.push_str(", ");
                }
                write!(
                    text,
                    "{} {}",
                    llvm_type_with_references(program, field.ty(), &mut references.types)?,
                    global_constant_value(program, value, field.ty(), references)?
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            text.push_str(" }");
            Ok(text)
        }
        _ => Err(BackendFailure::InvalidIr),
    }
}
