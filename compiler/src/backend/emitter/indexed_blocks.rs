//! Root-shaped private allocations and borrowed helper arguments.

use super::indexed::FarGroup;
use super::*;
use crate::ir::{IrIndexedFamilyKind, IrIndexedReduction, IrIndexedRootReference, IrPlaceStep};

/// Every proper ancestor is fixed-size storage. The block itself is borrowed,
/// never copied into an ancestor slot. Slots belong to the ordinary frame plan.
pub(super) fn reference_slots(
    program: &IrProgram,
    original: IrType,
    roots: &[IrIndexedRootReference],
) -> Result<Vec<(Vec<IrPlaceStep>, IrType)>, BackendFailure> {
    let mut slots = Vec::new();
    for root in roots {
        let mut ty = original;
        for (depth, step) in root.path.iter().enumerate() {
            let prefix = root.path[..depth].to_vec();
            if !slots.iter().any(|(path, _)| *path == prefix) {
                slots.push((prefix, ty));
            }
            ty = reference_step(program, ty, step)?;
        }
    }
    slots.sort_by_key(|(path, _)| path.len());
    Ok(slots)
}

fn reference_step(
    program: &IrProgram,
    ty: IrType,
    step: &IrPlaceStep,
) -> Result<IrType, BackendFailure> {
    let IrType::Nominal(nominal) = ty else {
        return Err(BackendFailure::InvalidIr);
    };
    match (
        step,
        program
            .nominal(nominal)
            .ok_or(BackendFailure::InvalidIr)?
            .kind(),
    ) {
        (
            IrPlaceStep::Field {
                nominal: expected,
                field,
            },
            IrNominalKind::Struct { fields },
        ) if *expected == nominal => Ok(fields
            .get(*field as usize)
            .ok_or(BackendFailure::InvalidIr)?
            .ty),
        (IrPlaceStep::BoxReferent { nominal: expected }, IrNominalKind::Box { referent, .. })
            if *expected == nominal =>
        {
            Ok(*referent)
        }
        _ => Err(BackendFailure::InvalidIr),
    }
}

fn has_measures(program: &IrProgram, ty: IrType) -> Result<bool, BackendFailure> {
    match ty {
        IrType::Buffer { .. } | IrType::Window { .. } | IrType::KeySet | IrType::Entries { .. } => {
            Ok(true)
        }
        IrType::Array { element, length } => Ok(length != 0
            && has_measures(
                program,
                program.element(element).ok_or(BackendFailure::InvalidIr)?,
            )?),
        IrType::Nominal(nominal) => match program
            .nominal(nominal)
            .ok_or(BackendFailure::InvalidIr)?
            .kind()
        {
            // A Box subtree is borrowed intact, not traversed or released.
            // This also terminates the type walk at recursive owner edges.
            IrNominalKind::Box { .. } => Ok(true),
            IrNominalKind::Struct { fields } => {
                for field in fields {
                    if has_measures(program, field.ty)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        },
        _ => Ok(false),
    }
}

impl FunctionEmitter<'_, '_> {
    /// PAR-2 summaries may inspect measures anywhere below their referent.
    /// Preserve inline headers and the borrowed pointers that reach read-only
    /// Box subtrees; all scalar cell bytes remain zero until family fill.
    /// Walk initialized elements only, including a Ring's physical window.
    fn emit_indexed_measures(
        &mut self,
        ty: IrType,
        destination: &str,
        source: &str,
        prefix: &str,
    ) -> Result<(), BackendFailure> {
        if !has_measures(self.program, ty)? {
            return Ok(());
        }
        if matches!(ty, IrType::KeySet | IrType::Entries { .. }) {
            let field = if ty == IrType::KeySet { 0 } else { 2 };
            let from = self.aggregate_field_pointer(ty, source, field)?;
            let to = self.aggregate_field_pointer(ty, destination, field)?;
            writeln!(
                self.output,
                "  %{prefix}.count = load i64, ptr {from}\n  store i64 %{prefix}.count, ptr {to}"
            )?;
            return Ok(());
        }
        if let IrType::Nominal(nominal) = ty {
            match self
                .program
                .nominal(nominal)
                .ok_or(BackendFailure::InvalidIr)?
                .kind()
            {
                IrNominalKind::Box { .. } => {
                    writeln!(
                        self.output,
                        "  %{prefix}.borrowed = load ptr, ptr {source}\n  store ptr %{prefix}.borrowed, ptr {destination}"
                    )?;
                    return Ok(());
                }
                IrNominalKind::Struct { fields } => {
                    let fields = fields.iter().map(|field| field.ty).collect::<Vec<_>>();
                    for (index, field) in fields.into_iter().enumerate() {
                        if has_measures(self.program, field)? {
                            let from = self.aggregate_field_pointer(ty, source, index)?;
                            let to = self.aggregate_field_pointer(ty, destination, index)?;
                            self.emit_indexed_measures(
                                field,
                                &to,
                                &from,
                                &format!("{prefix}.field.{index}"),
                            )?;
                        }
                    }
                    return Ok(());
                }
                _ => return Err(BackendFailure::InvalidIr),
            }
        }
        let (element, words, data, fixed_length, ring_capacity) = match ty {
            IrType::Array { element, length } => (element, 0, None, Some(length), None),
            IrType::Buffer { element } => (element, 1, Some(1), None, None),
            IrType::Window {
                element,
                shape,
                capacity,
            } if shape != IrWindowShape::Paged => {
                let words =
                    1 + usize::from(capacity.is_none()) + usize::from(shape == IrWindowShape::Ring);
                (
                    element,
                    words,
                    Some(words),
                    None,
                    (shape == IrWindowShape::Ring).then_some(capacity),
                )
            }
            _ => return Err(BackendFailure::InvalidIr),
        };
        let mut measures = Vec::new();
        for word in 0..words {
            let from = self.aggregate_field_pointer(ty, source, word)?;
            let to = self.aggregate_field_pointer(ty, destination, word)?;
            let value = format!("%{prefix}.word.{word}");
            writeln!(
                self.output,
                "  {value} = load i64, ptr {from}\n  store i64 {value}, ptr {to}"
            )?;
            measures.push(value);
        }
        let element = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        if !has_measures(self.program, element)? {
            return Ok(());
        }
        let count = fixed_length.map_or_else(|| measures[0].clone(), |length| length.to_string());
        writeln!(self.output, "  br label %{prefix}.entry")?;
        self.output.open_block(format!("{prefix}.entry"));
        writeln!(self.output, "  br label %{prefix}.head")?;
        self.output.open_block(format!("{prefix}.head"));
        writeln!(
            self.output,
            "  %{prefix}.i = phi i64 [ 0, %{prefix}.entry ], [ %{prefix}.next, %{prefix}.advance ]\n  %{prefix}.more = icmp ult i64 %{prefix}.i, {count}\n  br i1 %{prefix}.more, label %{prefix}.body, label %{prefix}.done"
        )?;
        self.output.open_block(format!("{prefix}.body"));
        let index = if let Some(capacity) = ring_capacity {
            let head = measures.last().ok_or(BackendFailure::InvalidIr)?;
            let capacity =
                capacity.map_or_else(|| measures[1].clone(), |capacity| capacity.to_string());
            writeln!(
                self.output,
                "  %{prefix}.position = add i64 {head}, %{prefix}.i\n  %{prefix}.physical = urem i64 %{prefix}.position, {capacity}"
            )?;
            format!("%{prefix}.physical")
        } else {
            format!("%{prefix}.i")
        };
        let data = data.map_or_else(String::new, |data| format!(", i32 {data}"));
        let name = self.output.type_name(self.program, ty)?;
        writeln!(
            self.output,
            "  %{prefix}.from = getelementptr inbounds {name}, ptr {source}, i64 0{data}, i64 {index}\n  %{prefix}.to = getelementptr inbounds {name}, ptr {destination}, i64 0{data}, i64 {index}"
        )?;
        self.emit_indexed_measures(
            element,
            &format!("%{prefix}.to"),
            &format!("%{prefix}.from"),
            &format!("{prefix}.element"),
        )?;
        writeln!(self.output, "  br label %{prefix}.advance")?;
        self.output.open_block(format!("{prefix}.advance"));
        writeln!(
            self.output,
            "  %{prefix}.next = add i64 %{prefix}.i, 1\n  br label %{prefix}.head"
        )?;
        self.output.open_block(format!("{prefix}.done"));
        Ok(())
    }

    pub(super) fn emit_indexed_blocks(
        &mut self,
        result: IrValueId,
        ty: IrType,
        address: IrValueId,
    ) -> Result<(), BackendFailure> {
        let slot = self.frame.slot(FunctionSlot::IndexedDirectory(result))?;
        let ty = self.output.type_name(self.program, ty)?;
        let temporary = self.next_temporary()?;
        writeln!(
            self.output,
            "  store ptr {}, ptr {slot}\n  %{temporary} = insertvalue {ty} zeroinitializer, ptr {slot}, 0\n  {} = insertvalue {ty} %{temporary}, i64 1, 1",
            self.value_name(address),
            self.value_name(result)
        )?;
        Ok(())
    }

    pub(super) fn emit_indexed_block(
        &mut self,
        result: IrValueId,
        blocks: IrValueId,
    ) -> Result<(), BackendFailure> {
        let ty = self.output.type_name(
            self.program,
            self.value_type(blocks).ok_or(BackendFailure::InvalidIr)?,
        )?;
        let temporary = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{temporary} = extractvalue {ty} {}, 0\n  {} = load ptr, ptr %{temporary}",
            self.value_name(blocks),
            self.value_name(result)
        )?;
        Ok(())
    }

    pub(super) fn emit_indexed_reference(
        &mut self,
        result: IrValueId,
        original: IrValueId,
        roots: &[IrIndexedRootReference],
    ) -> Result<(), BackendFailure> {
        let Some(IrType::Address(referent)) = self.value_type(original) else {
            return Err(BackendFailure::InvalidIr);
        };
        if let Some(root) = roots.iter().find(|root| root.path.is_empty()) {
            if roots.len() != 1 {
                return Err(BackendFailure::InvalidIr);
            }
            writeln!(
                self.output,
                "  {} = getelementptr i8, ptr {}, i64 0",
                self.value_name(result),
                self.value_name(root.block)
            )?;
            return Ok(());
        }
        let slots = reference_slots(self.program, referent.ty(), roots)?;
        // Copy only the fixed ancestors. Each Box edge below is replaced by
        // another borrowed ancestor or by the leaf's private allocation.
        for (index, (path, ty)) in slots.iter().enumerate() {
            let mut address = self.value_name(original);
            let mut source_type = referent.ty();
            for step in path {
                address = self.indexed_reference_step(source_type, &address, step)?;
                source_type = reference_step(self.program, source_type, step)?;
            }
            let slot = self
                .frame
                .slot(FunctionSlot::IndexedReference(result, index))?;
            let name = self.output.type_name(self.program, *ty)?;
            let loaded = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{loaded} = load {name}, ptr {address}\n  store {name} %{loaded}, ptr {slot}"
            )?;
        }
        for (index, (path, ty)) in slots.iter().enumerate().rev() {
            let slot = self
                .frame
                .slot(FunctionSlot::IndexedReference(result, index))?;
            let mut children: Vec<(Vec<IrPlaceStep>, String)> = roots
                .iter()
                .map(|root| (root.path.clone(), self.value_name(root.block)))
                .collect();
            for (child, (child_path, _)) in slots.iter().enumerate() {
                children.push((
                    child_path.clone(),
                    self.frame
                        .slot(FunctionSlot::IndexedReference(result, child))?,
                ));
            }
            for (child_path, address) in children {
                if child_path.len() != path.len() + 1 || !child_path.starts_with(path) {
                    continue;
                }
                let step = &child_path[path.len()];
                match step {
                    IrPlaceStep::BoxReferent { .. } => {
                        writeln!(self.output, "  store ptr {address}, ptr {slot}")?;
                    }
                    IrPlaceStep::Field { field, .. } => {
                        let destination =
                            self.aggregate_field_pointer(*ty, &slot, *field as usize)?;
                        let child_type = reference_step(self.program, *ty, step)?;
                        let child_type = self.output.type_name(self.program, child_type)?;
                        let loaded = self.next_temporary()?;
                        writeln!(
                            self.output,
                            "  %{loaded} = load {child_type}, ptr {address}\n  store {child_type} %{loaded}, ptr {destination}"
                        )?;
                    }
                    _ => return Err(BackendFailure::InvalidIr),
                }
            }
        }
        let slot = self.frame.slot(FunctionSlot::IndexedReference(result, 0))?;
        writeln!(
            self.output,
            "  {} = getelementptr i8, ptr {slot}, i64 0",
            self.value_name(result)
        )?;
        Ok(())
    }

    fn indexed_reference_step(
        &mut self,
        ty: IrType,
        address: &str,
        step: &IrPlaceStep,
    ) -> Result<String, BackendFailure> {
        match step {
            IrPlaceStep::Field { field, .. } => {
                self.aggregate_field_pointer(ty, address, *field as usize)
            }
            IrPlaceStep::BoxReferent { .. } => {
                let loaded = self.next_temporary()?;
                writeln!(self.output, "  %{loaded} = load ptr, ptr {address}")?;
                Ok(format!("%{loaded}"))
            }
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    /// A container projection within a block, before its logical subscript.
    fn indexed_container(
        &mut self,
        block: &str,
        spec: &IrIndexedReduction,
    ) -> Result<(String, IrType), BackendFailure> {
        let root = spec.root.as_ref().ok_or(BackendFailure::InvalidIr)?;
        let mut ty = root.block_type;
        let mut address = block.to_owned();
        for field in &root.fields {
            let IrType::Nominal(nominal) = ty else {
                return Err(BackendFailure::InvalidIr);
            };
            address = self.aggregate_field_pointer(ty, &address, *field as usize)?;
            ty = reference_step(
                self.program,
                ty,
                &IrPlaceStep::Field {
                    nominal,
                    field: *field,
                },
            )?;
        }
        Ok((address, ty))
    }

    fn indexed_block_cell(
        &mut self,
        block: &str,
        spec: &IrIndexedReduction,
        index: &str,
    ) -> Result<String, BackendFailure> {
        let (address, ty) = self.indexed_container(block, spec)?;
        let field = match ty {
            IrType::Array { .. } => None,
            IrType::Buffer { .. } => Some(1),
            IrType::Window {
                shape: crate::IrWindowShape::Slots,
                capacity,
                ..
            } => Some(if capacity.is_some() { 1 } else { 2 }),
            _ => return Err(BackendFailure::InvalidIr),
        };
        let mut data = String::new();
        if let Some(field) = field {
            write!(data, ", i32 {field}")?;
        }
        let mut indices = format!("i64 {index}");
        for field in &spec.projection.fields {
            write!(indices, ", i32 {field}")?;
        }
        let name = self.output.type_name(self.program, ty)?;
        let element = self
            .output
            .type_name(self.program, spec.projection.root_element)?;
        let base = self.next_temporary()?;
        let pointer = self.next_temporary()?;
        // Empty fixed containers have an alignment-free [0 x i8] tail.
        // Project their data base separately, then use the family's typed
        // element stride; no record field is ever projected through i8.
        writeln!(
            self.output,
            "  %{base} = getelementptr inbounds {name}, ptr {address}, i64 0{data}, i64 0\n  %{pointer} = getelementptr inbounds {element}, ptr %{base}, {indices}"
        )?;
        Ok(format!("%{pointer}"))
    }

    /// A call-updated root's contribution: one allocation holding a
    /// singleton directory slot, its own byte size, and one root-shaped block
    /// shared by the root's families. The block preserves the destination's
    /// initialized measures and borrowed measure-only Box subtrees; every
    /// family cell holds its operation identity or its mark sentinel.
    /// Returns base, bytes and the directory length.
    pub(super) fn emit_far_block(
        &mut self,
        families: &[IrIndexedReduction],
        first: usize,
        arguments: &[IrValueId],
        acquired: &[FarGroup],
        prefix: &str,
    ) -> Result<(String, String, String), BackendFailure> {
        let spec = &families[first];
        let root = spec.root.as_ref().ok_or(BackendFailure::InvalidIr)?;
        let destination = Self::far_argument(arguments, spec.capture)?;
        let range_type = self.output.type_name(
            self.program,
            self.value_type(destination)
                .ok_or(BackendFailure::InvalidIr)?,
        )?;
        let block_type = self.output.type_name(self.program, root.block_type)?;
        writeln!(
            self.output,
            "  %{prefix}.source.directory = extractvalue {range_type} {}, 0\n  %{prefix}.source = load ptr, ptr %{prefix}.source.directory",
            self.value_name(destination)
        )?;
        let source = format!("%{prefix}.source");
        let failed = format!("{prefix}.failed");
        let runtime = match root.block_type {
            IrType::Buffer { element } => Some((element, 0, 1)),
            IrType::Window {
                shape: crate::IrWindowShape::Slots,
                element,
                capacity: None,
            } => Some((element, 1, 2)),
            _ => None,
        };
        let (count, stride, header) = if let Some((element, measure, data)) = runtime {
            let capacity = self.aggregate_field_pointer(root.block_type, &source, measure)?;
            writeln!(
                self.output,
                "  %{prefix}.capacity = load i64, ptr {capacity}"
            )?;
            let element_type = self.output.type_name(
                self.program,
                self.program
                    .element(element)
                    .ok_or(BackendFailure::InvalidIr)?,
            )?;
            (
                format!("%{prefix}.capacity"),
                format!("ptrtoint (ptr getelementptr ({element_type}, ptr null, i64 1) to i64)"),
                format!(
                    "ptrtoint (ptr getelementptr ({block_type}, ptr null, i64 0, i32 {data}) to i64)"
                ),
            )
        } else {
            (
                "1".to_owned(),
                format!("ptrtoint (ptr getelementptr ({block_type}, ptr null, i64 1) to i64)"),
                "0".to_owned(),
            )
        };
        let block_bytes = self.emit_allocation_size(
            &count,
            &stride,
            &header,
            &failed,
            &format!("{prefix}.block.size"),
        )?;
        writeln!(
            self.output,
            "  %{prefix}.empty = icmp eq i64 {block_bytes}, 0\n  %{prefix}.block.bytes = select i1 %{prefix}.empty, i64 1, i64 {block_bytes}"
        )?;
        let block_bytes = format!("%{prefix}.block.bytes");
        // The block follows the directory slot and the size word at its
        // own alignment, so the allocator's alignment covers it.
        let offset = format!(
            "ptrtoint (ptr getelementptr ({{ ptr, i64, {block_type} }}, ptr null, i32 0, i32 2) to i64)"
        );
        let bytes = self.emit_allocation_size(
            "1",
            &block_bytes,
            &offset,
            &failed,
            &format!("{prefix}.size"),
        )?;
        let pointer = format!("%{prefix}.allocation");
        self.output.symbol("wf__heap_take");
        writeln!(
            self.output,
            "  {pointer} = call ptr @wf__heap_take(i64 {bytes})\n  %{prefix}.ok = icmp ne ptr {pointer}, null\n  br i1 %{prefix}.ok, label %{prefix}.init, label %{failed}"
        )?;
        self.emit_far_failure(acquired, &failed)?;
        self.output.open_block(format!("{prefix}.init"));
        let block = format!("%{prefix}.block");
        writeln!(
            self.output,
            "  {block} = getelementptr inbounds {{ ptr, i64, {block_type} }}, ptr {pointer}, i32 0, i32 2\n  store ptr {block}, ptr {pointer}\n  %{prefix}.size.slot = getelementptr inbounds {{ ptr, i64, {block_type} }}, ptr {pointer}, i32 0, i32 1\n  store i64 {bytes}, ptr %{prefix}.size.slot\n  br label %{prefix}.zero.head"
        )?;
        self.output.open_block(format!("{prefix}.zero.head"));
        writeln!(
            self.output,
            "  %{prefix}.byte = phi i64 [ 0, %{prefix}.init ], [ %{prefix}.nextbyte, %{prefix}.zero.body ]\n  %{prefix}.hasbyte = icmp ult i64 %{prefix}.byte, {block_bytes}\n  br i1 %{prefix}.hasbyte, label %{prefix}.zero.body, label %{prefix}.zero.done"
        )?;
        self.output.open_block(format!("{prefix}.zero.body"));
        writeln!(
            self.output,
            "  %{prefix}.zero = getelementptr inbounds i8, ptr {block}, i64 %{prefix}.byte\n  store i8 0, ptr %{prefix}.zero\n  %{prefix}.nextbyte = add i64 %{prefix}.byte, 1\n  br label %{prefix}.zero.head"
        )?;
        self.output.open_block(format!("{prefix}.zero.done"));
        self.emit_indexed_measures(
            root.block_type,
            &block,
            &source,
            &format!("{prefix}.measures"),
        )?;
        for (family, member) in families
            .iter()
            .enumerate()
            .filter(|(_, member)| member.capture == spec.capture)
        {
            let fill = format!("{prefix}.family.{family}");
            let cells = self.far_count(member, arguments)?;
            let ty = self.output.type_name(self.program, member.private_type())?;
            let identity = constant_operand(member.private_identity(), member.private_type())?;
            writeln!(self.output, "  br label %{fill}.entry")?;
            self.output.open_block(format!("{fill}.entry"));
            writeln!(self.output, "  br label %{fill}.head")?;
            self.output.open_block(format!("{fill}.head"));
            writeln!(
                self.output,
                "  %{fill}.cell = phi i64 [ 0, %{fill}.entry ], [ %{fill}.next, %{fill}.body ]\n  %{fill}.more = icmp ult i64 %{fill}.cell, {cells}\n  br i1 %{fill}.more, label %{fill}.body, label %{fill}.done"
            )?;
            self.output.open_block(format!("{fill}.body"));
            let address = self.indexed_block_cell(&block, member, &format!("%{fill}.cell"))?;
            writeln!(
                self.output,
                "  store {ty} {identity}, ptr {address}\n  %{fill}.next = add i64 %{fill}.cell, 1\n  br label %{fill}.head"
            )?;
            self.output.open_block(format!("{fill}.done"));
        }
        Ok((pointer, bytes, "1".to_owned()))
    }

    /// Combine a granted root-shaped contribution into the destination block
    /// its far call named, family by family in ascending cells, then free
    /// its one raw allocation. `pointer` is the contribution's directory.
    pub(super) fn emit_far_block_combine(
        &mut self,
        families: &[IrIndexedReduction],
        first: usize,
        arguments: &[IrValueId],
        pointer: &str,
        prefix: &str,
    ) -> Result<(), BackendFailure> {
        let capture = families[first].capture;
        let root = families[first]
            .root
            .as_ref()
            .ok_or(BackendFailure::InvalidIr)?;
        let block_type = self.output.type_name(self.program, root.block_type)?;
        let destination = Self::far_argument(arguments, capture)?;
        let range_type = self.output.type_name(
            self.program,
            self.value_type(destination)
                .ok_or(BackendFailure::InvalidIr)?,
        )?;
        writeln!(
            self.output,
            "  %{prefix}.far = load ptr, ptr {pointer}\n  %{prefix}.destination.directory = extractvalue {range_type} {}, 0\n  %{prefix}.destination = load ptr, ptr %{prefix}.destination.directory",
            self.value_name(destination)
        )?;
        for (ordinal, spec) in families
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.capture == capture)
        {
            let family = format!("{prefix}.family.{ordinal}");
            let cells = self.far_count(spec, arguments)?;
            let ty = self
                .output
                .type_name(self.program, spec.projection.value_type)?;
            writeln!(self.output, "  br label %{family}.entry")?;
            self.output.open_block(format!("{family}.entry"));
            writeln!(self.output, "  br label %{family}.combine.head")?;
            self.output.open_block(format!("{family}.combine.head"));
            writeln!(
                self.output,
                "  %{family}.cell = phi i64 [ 0, %{family}.entry ], [ %{family}.next, %{family}.advance ]\n  %{family}.more = icmp ult i64 %{family}.cell, {cells}\n  br i1 %{family}.more, label %{family}.body, label %{family}.done"
            )?;
            self.output.open_block(format!("{family}.body"));
            let from = self.indexed_block_cell(
                &format!("%{prefix}.far"),
                spec,
                &format!("%{family}.cell"),
            )?;
            let destination = self.indexed_block_cell(
                &format!("%{prefix}.destination"),
                spec,
                &format!("%{family}.cell"),
            )?;
            writeln!(self.output, "  %{family}.right = load {ty}, ptr {from}")?;
            match spec.kind {
                IrIndexedFamilyKind::Reduce { .. } => {
                    writeln!(
                        self.output,
                        "  %{family}.left = load {ty}, ptr {destination}"
                    )?;
                    self.emit_indexed_combine_value(spec, &family, &ty)?;
                    writeln!(
                        self.output,
                        "  store {ty} %{family}.value, ptr {destination}\n  br label %{family}.advance"
                    )?;
                }
                // The sentinel is the constant with its lowest bit inverted,
                // so only a touched cell equals the constant; zero is a
                // legitimate mark constant.
                IrIndexedFamilyKind::Mark { constant } => {
                    let constant = constant_operand(constant, spec.projection.value_type)?;
                    writeln!(
                        self.output,
                        "  %{family}.marked = icmp eq {ty} %{family}.right, {constant}\n  br i1 %{family}.marked, label %{family}.store, label %{family}.advance"
                    )?;
                    self.output.open_block(format!("{family}.store"));
                    writeln!(
                        self.output,
                        "  store {ty} {constant}, ptr {destination}\n  br label %{family}.advance"
                    )?;
                }
            }
            self.output.open_block(format!("{family}.advance"));
            writeln!(
                self.output,
                "  %{family}.next = add i64 %{family}.cell, 1\n  br label %{family}.combine.head"
            )?;
            self.output.open_block(format!("{family}.done"));
        }
        self.output.symbol("wf__heap_give");
        writeln!(
            self.output,
            "  %{prefix}.size.slot = getelementptr inbounds {{ ptr, i64, {block_type} }}, ptr {pointer}, i32 0, i32 1\n  %{prefix}.bytes = load i64, ptr %{prefix}.size.slot\n  call void @wf__heap_give(ptr {pointer}, i64 %{prefix}.bytes)"
        )?;
        Ok(())
    }
}
