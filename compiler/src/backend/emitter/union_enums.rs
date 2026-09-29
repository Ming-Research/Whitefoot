//! Emission of union-laid-out payload enums (compiler/payload-enum-layout).
//!
//! An enum with at least two payload-carrying variants whose product
//! representation would not return in registers is laid out as a union of
//! per-variant views: variant `k` is `%wf.t.<link>.v<k> = type { i32,
//! <fields of k> }`, and the value is `%wf.t.<link> = type { i32, [S - 4 x
//! i8], [0 x %wf.t.<link>.v<m>] }`, where `S` is the size target layout
//! computes and `v<m>` a view of the value's alignment. The zero-length array
//! at offset `S` gives the value that alignment without adding a byte, and
//! the tag stays field 0, where the product layout has it too.
//!
//! LLVM has no union type, so such a value, and any aggregate holding one
//! inline, is memory-only: it always lives in a slot, its payload fields are
//! read and written with their own types through view addresses, whole values
//! move by memmove, calls pass its address and return it through a
//! destination, and its release helper takes its address. Every first-class
//! carrier would copy it element by element, move pointers and padding
//! through integers, or read a one-bit leaf through a type it was not stored
//! with.

use super::*;

/// Whether `id` is a union-laid-out enum; target layout owns the rule.
pub(super) fn is_union_enum(program: &IrProgram, id: IrNominalId) -> Result<bool, BackendFailure> {
    crate::target::is_union_enum(program.nominals(), program.elements(), id)
        .map_err(|_| BackendFailure::InvalidIr)
}

/// Whether a value of `ty` holds a union-laid-out enum inline, and so is
/// never an LLVM first-class value.
pub(super) fn is_memory_only(program: &IrProgram, ty: IrType) -> Result<bool, BackendFailure> {
    crate::target::is_memory_only(program.nominals(), program.elements(), ty)
        .map_err(|_| BackendFailure::InvalidIr)
}

/// The named type of variant `tag`'s view of a union-laid-out enum.
fn view_name(nominal: &IrNominal, tag: u32) -> String {
    format!("wf.t.{}.v{tag}", nominal.link_name())
}

/// Declares one union-laid-out enum's views and value type. The value's size
/// is target layout's, which a backend test compares with LLVM's own size of
/// every emitted value and view type.
pub(super) fn emit_union_declarations(
    module: &mut Module,
    program: &IrProgram,
    target: TargetLayout,
    nominal: &IrNominal,
) -> Result<(), BackendFailure> {
    let IrNominalKind::Enum { variants } = nominal.kind() else {
        return Err(BackendFailure::InvalidIr);
    };
    let layout = crate::target::union_enum_layout(target, program, nominal.id())
        .map_err(BackendFailure::TargetLayout)?;
    for variant in variants
        .iter()
        .filter(|variant| !variant.fields().is_empty())
    {
        let mut references = References::default();
        let mut body = String::from("{ i32");
        for field in variant.fields() {
            body.push_str(", ");
            body.push_str(&llvm_type_with_references(
                program,
                field.ty(),
                &mut references.types,
            )?);
        }
        body.push_str(" }");
        module.named_type(view_name(nominal, variant.tag()), body, references);
    }
    let payload = layout
        .size()
        .checked_sub(4)
        .ok_or(BackendFailure::InvalidIr)?;
    let aligning = view_name(nominal, layout.aligning_variant());
    let body = format!("{{ i32, [{payload} x i8], [0 x %{aligning}] }}");
    let mut references = References::default();
    references.types.insert(aligning);
    module.named_type(format!("wf.t.{}", nominal.link_name()), body, references);
    Ok(())
}

/// The type and indices of a `getelementptr` from an enum value's address to
/// payload field `field` of variant `variant`: a view of a union-laid-out
/// enum, and the flattened product field otherwise. The named types the
/// address names are recorded in `references`.
pub(super) fn variant_field_gep(
    program: &IrProgram,
    references: &mut BTreeSet<String>,
    nominal: IrNominalId,
    variant: u32,
    field: u32,
) -> Result<(String, String), BackendFailure> {
    let data = program.nominal(nominal).ok_or(BackendFailure::InvalidIr)?;
    let IrNominalKind::Enum { variants } = data.kind() else {
        return Err(BackendFailure::InvalidIr);
    };
    let selected = variants
        .iter()
        .find(|candidate| candidate.tag() == variant)
        .ok_or(BackendFailure::InvalidIr)?;
    if field as usize >= selected.fields().len() {
        return Err(BackendFailure::InvalidIr);
    }
    if is_union_enum(program, nominal)? {
        let view = view_name(data, variant);
        references.insert(view.clone());
        return Ok((
            format!("%{view}"),
            format!("i32 0, i32 {}", field as usize + 1),
        ));
    }
    let index = variant_field_base(variants, variant)?
        .checked_add(field as usize)
        .ok_or(BackendFailure::CounterOverflow)?;
    Ok((
        llvm_type_with_references(program, IrType::Nominal(nominal), references)?,
        format!("i32 0, i32 {index}"),
    ))
}

impl FunctionEmitter<'_, '_> {
    /// The address of payload field `field` of variant `variant` in the enum
    /// value at `address`.
    pub(super) fn variant_field_pointer(
        &mut self,
        nominal: IrNominalId,
        variant: u32,
        field: u32,
        address: &str,
    ) -> Result<String, BackendFailure> {
        let (ty, indices) = variant_field_gep(
            self.program,
            &mut self.output.references.types,
            nominal,
            variant,
            field,
        )?;
        let pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = getelementptr inbounds {ty}, ptr {address}, {indices}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{pointer}"))
    }

    pub(super) fn is_memory_only(&self, ty: IrType) -> Result<bool, BackendFailure> {
        is_memory_only(self.program, ty)
    }

    /// Whether a value is memory-only; such a value always has a slot.
    pub(super) fn value_is_memory_only(&self, value: IrValueId) -> Result<bool, BackendFailure> {
        let ty = self.value_type(value).ok_or(BackendFailure::InvalidIr)?;
        self.is_memory_only(ty)
    }

    /// Copies a memory-only value from `source` into its own slot: the
    /// memory form of an operation that reads an element or a referent.
    pub(super) fn copy_into_result(
        &mut self,
        result: IrValueId,
        ty: IrType,
        source: &str,
    ) -> Result<(), BackendFailure> {
        let destination = self.value_place(result)?;
        self.copy_storage(ty, source, &destination)
    }
}

/// The positions among an edge's memory-only transfers whose source must be
/// copied aside before any of them writes its destination, because another
/// transfer of the same edge writes the allocation that source lives in.
/// Frame planning reserves one snapshot slot for each, and edge emission
/// uses exactly these.
pub(super) fn edge_snapshot_positions(
    program: &IrProgram,
    storage: &FunctionStoragePlan,
    parameters: &[(IrValueId, IrType)],
    arguments: &[IrValueId],
) -> Result<Vec<usize>, BackendFailure> {
    let mut moves = Vec::new();
    for (position, ((parameter, ty), argument)) in parameters.iter().zip(arguments).enumerate() {
        let Some(destination) = storage.slot(*parameter) else {
            continue;
        };
        let source = storage.slot(*argument);
        if Some(destination) == source || !is_memory_only(program, *ty)? {
            continue;
        }
        let source = source.ok_or(BackendFailure::InvalidIr)?;
        moves.push((
            position,
            storage.allocation_root(destination),
            storage.allocation_root(source),
        ));
    }
    Ok(moves
        .iter()
        .filter(|(position, _, source)| {
            moves
                .iter()
                .any(|(other, destination, _)| other != position && destination == source)
        })
        .map(|(position, _, _)| *position)
        .collect())
}

/// One argument a hand-out or context start writes into its frame: a
/// first-class operand, or a memory-only value copied from its slot.
pub(super) enum FrameOperand {
    Value(String),
    Stored { ty: IrType, value: IrValueId },
}

impl FunctionEmitter<'_, '_> {
    /// The frame operand of one argument whose frame field has type `ty`.
    pub(super) fn frame_operand(
        &mut self,
        ty: &str,
        argument: IrValueId,
    ) -> Result<FrameOperand, BackendFailure> {
        let value_ty = self.value_type(argument).ok_or(BackendFailure::InvalidIr)?;
        if self.is_memory_only(value_ty)? {
            return Ok(FrameOperand::Stored {
                ty: value_ty,
                value: argument,
            });
        }
        let operand = self.value_operand(argument)?;
        Ok(FrameOperand::Value(format!("{ty} {operand}")))
    }

    /// Writes one frame operand into the frame field at `field`.
    pub(super) fn store_frame_operand(
        &mut self,
        operand: &FrameOperand,
        field: &str,
    ) -> Result<(), BackendFailure> {
        match operand {
            FrameOperand::Value(operand) => writeln!(self.output, "  store {operand}, ptr {field}")
                .map_err(|_| BackendFailure::TextEmission),
            FrameOperand::Stored { ty, value } => {
                let source = self.value_place(*value)?;
                self.copy_storage(*ty, &source, field)
            }
        }
    }
}
