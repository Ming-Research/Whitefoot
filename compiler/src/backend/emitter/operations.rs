use super::*;

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    /// The planned backing that gives a binding its stable address.
    ///
    /// This includes a Box owner's pointer slot: replacing through its borrow
    /// must update that slot, rather than only changing a callee's pointer.
    pub(super) fn emit_address_of(
        &mut self,
        result: IrValueId,
        ty: IrType,
        value: IrValueId,
        referent: IrAddressed,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Address(referent)
            || self.value_type(value) != Some(referent.ty())
            || !self.referent_is_stored(referent)?
        {
            return Err(BackendFailure::InvalidIr);
        }
        let address = self.binding_place(result)?;
        if self
            .storage
            .slot(value)
            .and_then(|slot| self.storage.destination(slot))
            != Some(result)
        {
            self.store_value_at(value, &address)?;
        }
        Ok(())
    }

    pub(super) fn emit_load(
        &mut self,
        result: IrValueId,
        ty: IrType,
        address: IrValueId,
        referent: IrAddressed,
    ) -> Result<(), BackendFailure> {
        if ty != referent.ty() || self.value_type(address) != Some(IrType::Address(referent)) {
            return Err(BackendFailure::InvalidIr);
        }
        writeln!(
            self.output,
            "  {} = load {}, ptr {}",
            self.value_name(result),
            llvm_type(self.program, ty)?,
            self.value_name(address)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_store(
        &mut self,
        address: IrValueId,
        value: IrValueId,
        referent: IrAddressed,
    ) -> Result<(), BackendFailure> {
        if self.value_type(address) != Some(IrType::Address(referent))
            || self.value_type(value) != Some(referent.ty())
        {
            return Err(BackendFailure::InvalidIr);
        }
        let destination = self.value_name(address);
        self.store_value_at(value, &destination)
    }

    fn referent_is_stored(&self, referent: IrAddressed) -> Result<bool, BackendFailure> {
        Ok(match referent {
            IrAddressed::Nominal(nominal) => matches!(
                self.nominal(nominal)?.kind(),
                IrNominalKind::Struct { .. }
                    | IrNominalKind::Enum { .. }
                    | IrNominalKind::Box { .. }
                    | IrNominalKind::Opaque
            ),
            IrAddressed::Unit
            | IrAddressed::Bool
            | IrAddressed::Integer { .. }
            | IrAddressed::Float { .. }
            | IrAddressed::Buffer { .. }
            // An inline window's storage lives in its owner, so a reference
            // to one addresses that storage [TYPE-9, REF-1].
            | IrAddressed::Array { .. }
            | IrAddressed::Window { .. } => true,
        })
    }

    pub(super) fn emit_constant(
        &mut self,
        result: IrValueId,
        ty: IrType,
        constant: IrConstant,
    ) -> Result<(), BackendFailure> {
        let rendered = constant_operand(constant, ty)?;
        let llvm_ty = llvm_type(self.program, ty)?;
        writeln!(
            self.output,
            "  {} = select i1 true, {llvm_ty} {rendered}, {llvm_ty} {rendered}",
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_call(
        &mut self,
        result: IrValueId,
        ty: IrType,
        function: u32,
        arguments: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        let target = self
            .program
            .functions()
            .get(function as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        let abi = FunctionAbi::build(self.program, target)?;
        if abi.result().ty() != ty || abi.parameters().len() != arguments.len() {
            return Err(BackendFailure::InvalidIr);
        }
        let mut rendered = Vec::with_capacity(arguments.len());
        let stored_result = abi.result().uses_destination();
        if stored_result {
            let destination = self.value_place(result)?;
            rendered.push(format!("ptr {destination}"));
        }
        for (argument, parameter) in arguments.iter().zip(abi.parameters()) {
            if self.value_type(*argument) != Some(parameter.ty()) {
                return Err(BackendFailure::InvalidIr);
            }
            if parameter.is_indirect() {
                let address = self.value_place(*argument)?;
                rendered.push(format!("ptr {address}"));
            } else {
                let operand = self.value_name(*argument);
                rendered.push(self.value_argument(*parameter, &operand)?);
            }
        }
        // A call that stays inside a budgeted component carries the caller's
        // remaining levels as the callee variant's trailing parameter, the way
        // a split carries its allowance into the splitter.
        let (callee, budget) = self.callee_target(function, target.name());
        if let Some(budget) = budget {
            rendered.push(format!("i64 {budget}"));
        }
        if stored_result {
            return writeln!(
                self.output,
                "  call void @{callee}({})",
                rendered.join(", ")
            )
            .map_err(|_| BackendFailure::TextEmission);
        }
        writeln!(
            self.output,
            "  {} = call {} @{callee}({})",
            self.value_name(result),
            llvm_type(self.program, ty)?,
            rendered.join(", ")
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        // A stored aggregate returned in registers enters the storage the
        // plan selected for it. A scalar result has no storage.
        self.save_value_result(result)
    }

    /// One by-value operand as its callee's parameter receives it.
    ///
    /// A range reference crosses every call boundary as its element pointer
    /// and count (see [`crate::backend::abi`]), so its pair is split here,
    /// immediately before the call that passes it; any other value passes as
    /// itself.
    pub(super) fn value_argument(
        &mut self,
        parameter: ParameterAbi,
        operand: &str,
    ) -> Result<String, BackendFailure> {
        let ty = llvm_type(self.program, parameter.ty())?;
        if !parameter.is_range() {
            return Ok(format!("{ty} {operand}"));
        }
        let pointer = self.next_temporary()?;
        let count = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = extractvalue {ty} {operand}, 0\n  %{count} = extractvalue {ty} {operand}, 1"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("ptr %{pointer}, i64 %{count}"))
    }

    pub(super) fn emit_boolean(
        &mut self,
        result: IrValueId,
        ty: IrType,
        operation: IrBooleanOperation,
        arguments: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Bool
            || arguments
                .iter()
                .any(|argument| self.value_type(*argument) != Some(IrType::Bool))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let (opcode, left, right) = match (operation, arguments) {
            (IrBooleanOperation::And, [left, right]) => ("and", *left, self.value_name(*right)),
            (IrBooleanOperation::Or, [left, right]) => ("or", *left, self.value_name(*right)),
            (IrBooleanOperation::ExclusiveOr, [left, right]) => {
                ("xor", *left, self.value_name(*right))
            }
            (IrBooleanOperation::Not, [value]) => ("xor", *value, "true".to_owned()),
            _ => return Err(BackendFailure::InvalidIr),
        };
        writeln!(
            self.output,
            "  {} = {opcode} i1 {}, {right}",
            self.value_name(result),
            self.value_name(left)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_enum_equality(
        &mut self,
        result: IrValueId,
        ty: IrType,
        equal: bool,
        operand_type: IrType,
        arguments: [IrValueId; 2],
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Bool
            || !is_tag_only_type(self.program, operand_type)?
            || arguments
                .iter()
                .any(|argument| self.value_type(*argument) != Some(operand_type))
        {
            return Err(BackendFailure::InvalidIr);
        }
        writeln!(
            self.output,
            "  {} = icmp {} {} {}, {}",
            self.value_name(result),
            if equal { "eq" } else { "ne" },
            llvm_type(self.program, operand_type)?,
            self.value_name(arguments[0]),
            self.value_name(arguments[1])
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_struct(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        fields: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Struct {
            fields: declared_fields,
        } = self.nominal(nominal)?.kind()
        else {
            return Err(BackendFailure::InvalidIr);
        };
        if fields.len() != declared_fields.len() {
            return Err(BackendFailure::InvalidIr);
        }
        for (value, field) in fields.iter().zip(declared_fields) {
            if self.value_type(*value) != Some(field.ty()) {
                return Err(BackendFailure::InvalidIr);
            }
        }
        self.emit_insert_sequence(
            result,
            ty,
            fields
                .iter()
                .enumerate()
                .map(|(index, value)| (index, *value))
                .collect(),
        )
    }

    pub(super) fn emit_enum(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        variant: u32,
        fields: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let nominal_data = self.nominal(nominal)?;
        let IrNominalKind::Enum { variants } = nominal_data.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let selected = variants
            .iter()
            .find(|candidate| candidate.tag() == variant)
            .ok_or(BackendFailure::InvalidIr)?;
        if fields.len() != selected.fields().len() {
            return Err(BackendFailure::InvalidIr);
        }
        for (value, field) in fields.iter().zip(selected.fields()) {
            if self.value_type(*value) != Some(field.ty()) {
                return Err(BackendFailure::InvalidIr);
            }
        }
        if nominal_data.is_tag_only_enum() {
            let llvm_ty = llvm_type(self.program, ty)?;
            writeln!(
                self.output,
                "  {} = or {llvm_ty} 0, {variant}",
                self.value_name(result)
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            return Ok(());
        }
        let mut inserts = vec![(0_usize, None)];
        let base = variant_field_base(variants, variant)?;
        inserts.extend(
            fields
                .iter()
                .enumerate()
                .map(|(index, value)| (base + index, Some(*value))),
        );
        self.emit_enum_insert_sequence(result, ty, variant, inserts)
    }

    pub(super) fn emit_insert_sequence(
        &mut self,
        result: IrValueId,
        ty: IrType,
        fields: Vec<(usize, IrValueId)>,
    ) -> Result<(), BackendFailure> {
        let aggregate_ty = llvm_type(self.program, ty)?;
        if fields.is_empty() {
            writeln!(
                self.output,
                "  {} = select i1 true, {aggregate_ty} zeroinitializer, {aggregate_ty} zeroinitializer",
                self.value_name(result)
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            return Ok(());
        }
        let mut base = "zeroinitializer".to_owned();
        let total = fields.len();
        for (ordinal, (index, value)) in fields.into_iter().enumerate() {
            let field_ty = self
                .function
                .value_type(value)
                .ok_or(BackendFailure::InvalidIr)?;
            let output = if ordinal + 1 == total {
                self.value_name(result)
            } else {
                format!("%{}", self.next_temporary()?)
            };
            writeln!(
                self.output,
                "  {output} = insertvalue {aggregate_ty} {base}, {} {}, {index}",
                llvm_type(self.program, field_ty)?,
                self.value_name(value)
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            base = output;
        }
        Ok(())
    }

    pub(super) fn emit_enum_insert_sequence(
        &mut self,
        result: IrValueId,
        ty: IrType,
        tag: u32,
        inserts: Vec<(usize, Option<IrValueId>)>,
    ) -> Result<(), BackendFailure> {
        let aggregate_ty = llvm_type(self.program, ty)?;
        let mut base = "zeroinitializer".to_owned();
        let total = inserts.len();
        for (ordinal, (index, value)) in inserts.into_iter().enumerate() {
            let output = if ordinal + 1 == total {
                self.value_name(result)
            } else {
                format!("%{}", self.next_temporary()?)
            };
            match value {
                Some(value) => {
                    let field_ty = self
                        .function
                        .value_type(value)
                        .ok_or(BackendFailure::InvalidIr)?;
                    writeln!(
                        self.output,
                        "  {output} = insertvalue {aggregate_ty} {base}, {} {}, {index}",
                        llvm_type(self.program, field_ty)?,
                        self.value_name(value)
                    )
                    .map_err(|_| BackendFailure::TextEmission)?;
                }
                None => {
                    writeln!(
                        self.output,
                        "  {output} = insertvalue {aggregate_ty} {base}, i32 {tag}, {index}"
                    )
                    .map_err(|_| BackendFailure::TextEmission)?;
                }
            }
            base = output;
        }
        Ok(())
    }

    pub(super) fn emit_struct_projection(
        &mut self,
        result: IrValueId,
        ty: IrType,
        aggregate: IrValueId,
        nominal: IrNominalId,
        field: u32,
        consume_root: bool,
    ) -> Result<(), BackendFailure> {
        if self.value_type(aggregate) != Some(IrType::Nominal(nominal)) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Struct { fields } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if fields.get(field as usize).map(|field| field.ty()) != Some(ty) {
            return Err(BackendFailure::InvalidIr);
        }
        if consume_root {
            writeln!(self.output, "  ; ownership-consuming projection")
                .map_err(|_| BackendFailure::TextEmission)?;
        }
        writeln!(
            self.output,
            "  {} = extractvalue {} {}, {field}",
            self.value_name(result),
            llvm_type(self.program, IrType::Nominal(nominal))?,
            self.value_name(aggregate)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_struct_insertion(
        &mut self,
        result: IrValueId,
        ty: IrType,
        aggregate: IrValueId,
        nominal: IrNominalId,
        field: u32,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) || self.value_type(aggregate) != Some(ty) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Struct { fields } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let field_ty = fields
            .get(field as usize)
            .map(|field| field.ty())
            .ok_or(BackendFailure::InvalidIr)?;
        if self.value_type(value) != Some(field_ty) {
            return Err(BackendFailure::InvalidIr);
        }
        writeln!(
            self.output,
            "  {} = insertvalue {} {}, {} {}, {field}",
            self.value_name(result),
            llvm_type(self.program, ty)?,
            self.value_name(aggregate),
            llvm_type(self.program, field_ty)?,
            self.value_name(value)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_variant_projection(
        &mut self,
        result: IrValueId,
        ty: IrType,
        aggregate: IrValueId,
        nominal: IrNominalId,
        variant: u32,
        field: u32,
    ) -> Result<(), BackendFailure> {
        if self.value_type(aggregate) != Some(IrType::Nominal(nominal)) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Enum { variants } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let selected = variants
            .iter()
            .find(|candidate| candidate.tag() == variant)
            .ok_or(BackendFailure::InvalidIr)?;
        if selected
            .fields()
            .get(field as usize)
            .map(|field| field.ty())
            != Some(ty)
        {
            return Err(BackendFailure::InvalidIr);
        }
        let index = variant_field_base(variants, variant)? + field as usize;
        writeln!(
            self.output,
            "  {} = extractvalue {} {}, {index}",
            self.value_name(result),
            llvm_type(self.program, IrType::Nominal(nominal))?,
            self.value_name(aggregate)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }
}
