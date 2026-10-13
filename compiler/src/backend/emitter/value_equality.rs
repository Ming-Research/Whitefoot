//! [OP-16] equality reads typed leaves, never padding or inactive payloads.
//! Both source operands have already been evaluated. Branching between their
//! parts has no observable effects, and a single failure edge complements the
//! entire equality for `!=`. Array extent does not multiply emitted code.

use super::*;

/// A whole evaluated operand keeps its storage binding deferred until an
/// active comparison block projects or reads it. Derived pointers are already
/// emitted SSA values and retain their dominating definition.
enum EqualityPlace {
    Value(IrValueId),
    Pointer(String),
}

impl FunctionEmitter<'_, '_> {
    fn equality_place(&mut self, place: &EqualityPlace) -> Result<String, BackendFailure> {
        match place {
            EqualityPlace::Value(value) => self.value_place(*value),
            EqualityPlace::Pointer(pointer) => Ok(pointer.clone()),
        }
    }

    pub(super) fn emit_value_equality(
        &mut self,
        result: IrValueId,
        ty: IrType,
        equal: bool,
        operand_type: IrType,
        arguments: [IrValueId; 2],
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Bool
            || arguments
                .iter()
                .any(|argument| self.value_type(*argument) != Some(operand_type))
        {
            return Err(BackendFailure::InvalidIr);
        }
        if operand_type == IrType::Unit
            || matches!(operand_type, IrType::Nominal(id)
                if matches!(self.nominal(id)?.kind(), IrNominalKind::Opaque))
        {
            writeln!(
                self.output,
                "  {} = icmp {} i1 false, false",
                self.value_name(result),
                if equal { "eq" } else { "ne" }
            )?;
            return Ok(());
        }
        if matches!(operand_type, IrType::Integer { .. })
            || is_tag_only_type(self.program, operand_type)?
        {
            let left = self.value_operand(arguments[0])?;
            let right = self.value_operand(arguments[1])?;
            let llvm = self.output.type_name(self.program, operand_type)?;
            writeln!(
                self.output,
                "  {} = icmp {} {llvm} {left}, {right}",
                self.value_name(result),
                if equal { "eq" } else { "ne" }
            )?;
            return Ok(());
        }
        let left = EqualityPlace::Value(arguments[0]);
        let right = EqualityPlace::Value(arguments[1]);
        let unique = self.next_temporary()?;
        let mismatch = format!("eq.{unique}.different");
        let matched = format!("eq.{unique}.same");
        let done = format!("eq.{unique}.done");
        self.compare_value_parts(operand_type, &left, &right, &mismatch)?;
        writeln!(self.output, "  br label %{matched}")?;
        self.output.open_block(matched.clone());
        writeln!(self.output, "  br label %{done}")?;
        self.output.open_block(mismatch.clone());
        writeln!(self.output, "  br label %{done}")?;
        self.output.open_block(done);
        writeln!(
            self.output,
            "  {} = phi i1 [{equal}, %{matched}], [{}, %{mismatch}]",
            self.value_name(result),
            !equal
        )?;
        Ok(())
    }

    /// Continue in a fresh block when these scalar values agree; otherwise
    /// branch to the enclosing comparison's one failure block.
    fn equality_guard(
        &mut self,
        llvm: &str,
        left: &str,
        right: &str,
        mismatch: &str,
    ) -> Result<(), BackendFailure> {
        let comparison = self.next_temporary()?;
        let next = format!("eq.{comparison}.next");
        writeln!(
            self.output,
            "  %{comparison} = icmp eq {llvm} {left}, {right}\n  br i1 %{comparison}, label %{next}, label %{mismatch}"
        )?;
        self.output.open_block(next);
        Ok(())
    }

    /// The two places select values of the same type. Only a scalar leaf is
    /// loaded; an aggregate's address is projected to its actual parts.
    fn compare_value_parts(
        &mut self,
        ty: IrType,
        left: &EqualityPlace,
        right: &EqualityPlace,
        mismatch: &str,
    ) -> Result<(), BackendFailure> {
        if ty == IrType::Unit {
            return Ok(());
        }
        if matches!(ty, IrType::Integer { .. }) || is_tag_only_type(self.program, ty)? {
            let left = self.equality_place(left)?;
            let right = self.equality_place(right)?;
            let llvm = self.output.type_name(self.program, ty)?;
            let a = self.next_temporary()?;
            let b = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{a} = load {llvm}, ptr {left}\n  %{b} = load {llvm}, ptr {right}"
            )?;
            return self.equality_guard(&llvm, &format!("%{a}"), &format!("%{b}"), mismatch);
        }
        match ty {
            IrType::Nominal(id) => {
                // Clone only the type description, so recursive emission may
                // record its named-type dependencies and open new blocks.
                match self.nominal(id)?.kind().clone() {
                    IrNominalKind::Struct { fields } => {
                        for (index, field) in fields.iter().enumerate() {
                            let left = self.equality_place(left)?;
                            let right = self.equality_place(right)?;
                            let a = self.aggregate_field_pointer(ty, &left, index)?;
                            let b = self.aggregate_field_pointer(ty, &right, index)?;
                            self.compare_value_parts(
                                field.ty(),
                                &EqualityPlace::Pointer(a),
                                &EqualityPlace::Pointer(b),
                                mismatch,
                            )?;
                        }
                    }
                    IrNominalKind::Enum { variants } => {
                        let tag_ty = enum_tag_type(self.nominal(id)?)?;
                        let left_tag = self.equality_place(left)?;
                        let right_tag = self.equality_place(right)?;
                        let a = self.aggregate_field_pointer(ty, &left_tag, 0)?;
                        let b = self.aggregate_field_pointer(ty, &right_tag, 0)?;
                        let tag_a = self.next_temporary()?;
                        let tag_b = self.next_temporary()?;
                        writeln!(
                            self.output,
                            "  %{tag_a} = load {tag_ty}, ptr {a}\n  %{tag_b} = load {tag_ty}, ptr {b}"
                        )?;
                        self.equality_guard(
                            &tag_ty,
                            &format!("%{tag_a}"),
                            &format!("%{tag_b}"),
                            mismatch,
                        )?;
                        let done = format!("eq.{tag_a}.done");
                        writeln!(self.output, "  switch {tag_ty} %{tag_a}, label %{mismatch} [")?;
                        for variant in &variants {
                            writeln!(
                                self.output,
                                "    {tag_ty} {}, label %eq.{tag_a}.v{}",
                                variant.tag(),
                                variant.tag()
                            )?;
                        }
                        writeln!(self.output, "  ]")?;
                        for variant in &variants {
                            self.output
                                .open_block(format!("eq.{tag_a}.v{}", variant.tag()));
                            for (index, field) in variant.fields().iter().enumerate() {
                                let index = u32::try_from(index)
                                    .map_err(|_| BackendFailure::CounterOverflow)?;
                                // The common helper selects a union's typed
                                // variant view or a product's field index.
                                let left = self.equality_place(left)?;
                                let right = self.equality_place(right)?;
                                let a =
                                    self.variant_field_pointer(id, variant.tag(), index, &left)?;
                                let b =
                                    self.variant_field_pointer(id, variant.tag(), index, &right)?;
                                self.compare_value_parts(
                                    field.ty(),
                                    &EqualityPlace::Pointer(a),
                                    &EqualityPlace::Pointer(b),
                                    mismatch,
                                )?;
                            }
                            writeln!(self.output, "  br label %{done}")?;
                        }
                        self.output.open_block(done);
                    }
                    // Fieldless opaque structs have no parts to compare;
                    // opaque structs with fields lower as Struct above.
                    IrNominalKind::Opaque => {}
                    IrNominalKind::Box { .. } | IrNominalKind::Shared { .. } => {
                        return Err(BackendFailure::InvalidIr);
                    }
                }
            }
            IrType::Array { element, length } => {
                // No element, including a recursively named zero-sized part,
                // is visited when the array is empty.
                if length == 0 {
                    return Ok(());
                }
                let element = self
                    .program
                    .element(element)
                    .ok_or(BackendFailure::InvalidIr)?;
                let llvm = self.output.type_name(self.program, ty)?;
                let index = self.next_temporary()?;
                let next = self.next_temporary()?;
                let in_range = self.next_temporary()?;
                let entry = format!("eq.{index}.entry");
                let head = format!("eq.{index}.head");
                let body = format!("eq.{index}.body");
                let latch = format!("eq.{index}.latch");
                let done = format!("eq.{index}.done");
                writeln!(self.output, "  br label %{entry}")?;
                self.output.open_block(entry.clone());
                writeln!(self.output, "  br label %{head}")?;
                self.output.open_block(head.clone());
                writeln!(
                    self.output,
                    "  %{index} = phi i64 [0, %{entry}], [%{next}, %{latch}]\n  %{in_range} = icmp ult i64 %{index}, {length}\n  br i1 %{in_range}, label %{body}, label %{done}"
                )?;
                self.output.open_block(body);
                let left = self.equality_place(left)?;
                let right = self.equality_place(right)?;
                let logical = format!("%{index}");
                let address_index = self.element_address_index(element, &logical)?;
                let a = self.next_temporary()?;
                let b = self.next_temporary()?;
                writeln!(
                    self.output,
                    "  %{a} = getelementptr inbounds {llvm}, ptr {left}, i64 0, i64 {address_index}\n  %{b} = getelementptr inbounds {llvm}, ptr {right}, i64 0, i64 {address_index}"
                )?;
                self.compare_value_parts(
                    element,
                    &EqualityPlace::Pointer(format!("%{a}")),
                    &EqualityPlace::Pointer(format!("%{b}")),
                    mismatch,
                )?;
                writeln!(self.output, "  br label %{latch}")?;
                self.output.open_block(latch);
                // The largest index entering this block is length - 1, so
                // this increment fits even when length is u64::MAX.
                writeln!(
                    self.output,
                    "  %{next} = add i64 %{index}, 1\n  br label %{head}"
                )?;
                self.output.open_block(done);
            }
            _ => return Err(BackendFailure::InvalidIr),
        }
        Ok(())
    }
}
