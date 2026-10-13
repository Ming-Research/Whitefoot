//! Private indexed contributions of a granted far range half.
//!
//! A range driver passes its current destinations to both halves. When the
//! lane for the far half is granted, the offering activation acquires one
//! neutral contribution per storage group (a dense slab per family, or one
//! root-shaped block shared by a call-updated root's families), stores it in
//! the lane frame in place of the destination, and publishes. After the join
//! it combines each contribution into its destination in ascending cell
//! order and frees it before releasing the lane. A refused lane runs the far
//! half inline after the near half, on the destinations themselves, and
//! allocates nothing. The far task and its descendants only borrow the
//! contribution; only its acquiring activation releases it.

use super::parallel::U64;
use super::*;
use crate::ir::{IrIndexedFamilyKind, IrIndexedProjection, IrIndexedReduction};

/// The first argument of a range driver's captures: seed, lower, upper.
const CAPTURES: usize = 3;

/// One storage group of a granted far half: the family that owns its
/// capture position, and the raw allocation acquired for it.
pub(super) struct FarGroup {
    /// Ordinal of the group's first family; every family of the group
    /// shares its `capture` and `private` positions.
    first: usize,
    /// The allocation's base and its byte size, for the acquired-prefix
    /// release on a later group's failure.
    pointer: String,
    bytes: String,
}

/// The granted far half's private storage, replacing frame fields.
pub(super) struct FarBundle {
    groups: Vec<FarGroup>,
    /// `(argument index, frame operand)` written instead of the argument.
    pub(super) fields: Vec<(usize, String)>,
}

/// The ordinals of each storage group's first family, in family order.
fn group_leaders(families: &[IrIndexedReduction]) -> Vec<usize> {
    let mut leaders: Vec<usize> = Vec::new();
    for (ordinal, family) in families.iter().enumerate() {
        if !leaders
            .iter()
            .any(|leader| families[*leader].capture == family.capture)
        {
            leaders.push(ordinal);
        }
    }
    leaders
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn far_argument(
        arguments: &[IrValueId],
        capture: usize,
    ) -> Result<IrValueId, BackendFailure> {
        capture
            .checked_add(CAPTURES)
            .and_then(|index| arguments.get(index))
            .copied()
            .ok_or(BackendFailure::InvalidIr)
    }

    /// Acquires and initializes the far half's contributions on the granted
    /// edge, before any frame field is written or the task is published.
    ///
    /// Sizes use checked allocation arithmetic [OP-9]. A size or allocator
    /// failure frees the groups this bundle already acquired and enters the
    /// shared heap-exhaustion termination [STOR-8]; the reserved lane was
    /// never published, so no worker can observe the abandoned slot.
    pub(super) fn emit_far_bundle(
        &mut self,
        families: &[IrIndexedReduction],
        arguments: &[IrValueId],
    ) -> Result<FarBundle, BackendFailure> {
        let id = self.next_temporary()?;
        let mut bundle = FarBundle {
            groups: Vec::new(),
            fields: Vec::new(),
        };
        for (group, first) in group_leaders(families).into_iter().enumerate() {
            let family = &families[first];
            let destination = Self::far_argument(arguments, family.capture)?;
            let range_type = self
                .value_type(destination)
                .ok_or(BackendFailure::InvalidIr)?;
            let IrType::Range { element } = range_type else {
                return Err(BackendFailure::InvalidIr);
            };
            let prefix = format!("indexed.far.{id}.{group}");
            let acquired = if family.root.is_some() {
                self.emit_far_block(families, first, arguments, &bundle.groups, &prefix)?
            } else {
                if self.program.element(element) != Some(family.projection.root_element) {
                    return Err(BackendFailure::InvalidIr);
                }
                self.emit_far_slab(family, arguments, &bundle.groups, &prefix)?
            };
            let (pointer, bytes, length) = acquired;
            let ty = self.output.type_name(self.program, range_type)?;
            writeln!(
                self.output,
                "  %{prefix}.descriptor.pointer = insertvalue {ty} zeroinitializer, ptr {pointer}, 0\n  %{prefix}.descriptor = insertvalue {ty} %{prefix}.descriptor.pointer, i64 {length}, 1"
            )?;
            bundle.fields.push((
                CAPTURES + family.capture,
                format!("{ty} %{prefix}.descriptor"),
            ));
            for member in families
                .iter()
                .filter(|member| member.capture == family.capture)
            {
                if Self::far_argument(arguments, member.private)
                    .ok()
                    .and_then(|value| self.value_type(value))
                    != Some(IrType::Bool)
                {
                    return Err(BackendFailure::InvalidIr);
                }
                let field = CAPTURES + member.private;
                if !bundle.fields.iter().any(|(index, _)| *index == field) {
                    bundle.fields.push((field, "i1 true".to_owned()));
                }
            }
            bundle.groups.push(FarGroup {
                first,
                pointer,
                bytes,
            });
        }
        Ok(bundle)
    }

    /// The initialized-cell count a family's count position carries.
    pub(super) fn far_count(
        &self,
        family: &IrIndexedReduction,
        arguments: &[IrValueId],
    ) -> Result<String, BackendFailure> {
        let count = Self::far_argument(arguments, family.count)?;
        if self.value_type(count) != Some(U64) {
            return Err(BackendFailure::InvalidIr);
        }
        Ok(self.value_name(count))
    }

    /// Frees every group acquired before a failing one, then terminates.
    pub(super) fn emit_far_failure(
        &mut self,
        acquired: &[FarGroup],
        failed: &str,
    ) -> Result<(), BackendFailure> {
        self.output.open_block(failed.to_owned());
        self.output.symbol("wf__heap_give");
        for group in acquired {
            writeln!(
                self.output,
                "  call void @wf__heap_give(ptr {}, i64 {})",
                group.pointer, group.bytes
            )?;
        }
        self.output.symbol("wf_resource_abort");
        writeln!(
            self.output,
            "  call void @wf_resource_abort()\n  unreachable"
        )?;
        Ok(())
    }

    /// A dense family's contribution: its initialized cell count of
    /// operation identities or false marks. Returns base, bytes and length.
    fn emit_far_slab(
        &mut self,
        family: &IrIndexedReduction,
        arguments: &[IrValueId],
        acquired: &[FarGroup],
        prefix: &str,
    ) -> Result<(String, String, String), BackendFailure> {
        let ty = self.output.type_name(self.program, family.private_type())?;
        let count = self.far_count(family, arguments)?;
        let failed = format!("{prefix}.failed");
        let stride = format!("ptrtoint (ptr getelementptr ({ty}, ptr null, i64 1) to i64)");
        let bytes =
            self.emit_allocation_size(&count, &stride, "0", &failed, &format!("{prefix}.size"))?;
        let pointer = format!("%{prefix}.allocation");
        self.output.symbol("wf__heap_take");
        writeln!(
            self.output,
            "  %{prefix}.empty = icmp eq i64 {bytes}, 0\n  %{prefix}.bytes = select i1 %{prefix}.empty, i64 1, i64 {bytes}\n  {pointer} = call ptr @wf__heap_take(i64 %{prefix}.bytes)\n  %{prefix}.ok = icmp ne ptr {pointer}, null\n  br i1 %{prefix}.ok, label %{prefix}.init, label %{failed}"
        )?;
        self.emit_far_failure(acquired, &failed)?;
        self.output.open_block(format!("{prefix}.init"));
        writeln!(self.output, "  br label %{prefix}.fill.head")?;
        self.output.open_block(format!("{prefix}.fill.head"));
        writeln!(
            self.output,
            "  %{prefix}.i = phi i64 [ 0, %{prefix}.init ], [ %{prefix}.next, %{prefix}.fill.body ]\n  %{prefix}.more = icmp ult i64 %{prefix}.i, {count}\n  br i1 %{prefix}.more, label %{prefix}.fill.body, label %{prefix}.fill.done"
        )?;
        self.output.open_block(format!("{prefix}.fill.body"));
        let identity = constant_operand(family.private_identity(), family.private_type())?;
        writeln!(
            self.output,
            "  %{prefix}.cell = getelementptr inbounds {ty}, ptr {pointer}, i64 %{prefix}.i\n  store {ty} {identity}, ptr %{prefix}.cell\n  %{prefix}.next = add i64 %{prefix}.i, 1\n  br label %{prefix}.fill.head"
        )?;
        self.output.open_block(format!("{prefix}.fill.done"));
        Ok((pointer, format!("%{prefix}.bytes"), count))
    }

    /// On the granted edge after the join, before the lane is released:
    /// combine each contribution into the destination the far call named,
    /// in ascending cell order, then free it once.
    pub(super) fn emit_far_combine(
        &mut self,
        families: &[IrIndexedReduction],
        arguments: &[IrValueId],
        frame: &str,
        frame_type: &str,
    ) -> Result<(), BackendFailure> {
        let id = self.next_temporary()?;
        for (group, first) in group_leaders(families).into_iter().enumerate() {
            let family = &families[first];
            let destination = Self::far_argument(arguments, family.capture)?;
            let range_type = self
                .value_type(destination)
                .ok_or(BackendFailure::InvalidIr)?;
            let range = self.output.type_name(self.program, range_type)?;
            let prefix = format!("indexed.join.{id}.{group}");
            let field = CAPTURES + family.capture;
            writeln!(
                self.output,
                "  %{prefix}.field = getelementptr inbounds {frame_type}, ptr {frame}, i32 0, i32 {field}\n  %{prefix}.descriptor = load {range}, ptr %{prefix}.field\n  %{prefix}.pointer = extractvalue {range} %{prefix}.descriptor, 0"
            )?;
            let pointer = format!("%{prefix}.pointer");
            if family.root.is_some() {
                self.emit_far_block_combine(families, first, arguments, &pointer, &prefix)?;
                continue;
            }
            let count = self.far_count(family, arguments)?;
            let destination_private =
                self.value_name(Self::far_argument(arguments, family.private)?);
            let destination = self.value_name(destination);
            writeln!(self.output, "  br label %{prefix}.entry")?;
            self.output.open_block(format!("{prefix}.entry"));
            writeln!(self.output, "  br label %{prefix}.combine.head")?;
            self.output.open_block(format!("{prefix}.combine.head"));
            writeln!(
                self.output,
                "  %{prefix}.i = phi i64 [ 0, %{prefix}.entry ], [ %{prefix}.next, %{prefix}.advance ]\n  %{prefix}.more = icmp ult i64 %{prefix}.i, {count}\n  br i1 %{prefix}.more, label %{prefix}.combine.body, label %{prefix}.free"
            )?;
            self.output.open_block(format!("{prefix}.combine.body"));
            let address = self.indexed_pointer(
                &format!("{prefix}.destination"),
                (&destination, range_type),
                &format!("%{prefix}.i"),
                &destination_private,
                &family.projection,
                family.private_type(),
            )?;
            let private_ty = self.output.type_name(self.program, family.private_type())?;
            writeln!(
                self.output,
                "  %{prefix}.from = getelementptr inbounds {private_ty}, ptr {pointer}, i64 %{prefix}.i\n  %{prefix}.right = load {private_ty}, ptr %{prefix}.from"
            )?;
            match family.kind {
                IrIndexedFamilyKind::Reduce { .. } => {
                    writeln!(
                        self.output,
                        "  %{prefix}.left = load {private_ty}, ptr {address}"
                    )?;
                    self.emit_indexed_combine_value(family, &prefix, &private_ty)?;
                    writeln!(
                        self.output,
                        "  store {private_ty} %{prefix}.value, ptr {address}\n  br label %{prefix}.advance"
                    )?;
                }
                // A true far mask marks the source cell with its constant or
                // sets the enclosing private mask, by the destination's mode.
                IrIndexedFamilyKind::Mark { constant } => {
                    writeln!(
                        self.output,
                        "  br i1 %{prefix}.right, label %{prefix}.mark, label %{prefix}.advance"
                    )?;
                    self.output.open_block(format!("{prefix}.mark"));
                    self.emit_indexed_mark(
                        &address,
                        &destination_private,
                        constant,
                        family.projection.value_type,
                    )?;
                    writeln!(self.output, "  br label %{prefix}.advance")?;
                }
            }
            self.output.open_block(format!("{prefix}.advance"));
            writeln!(
                self.output,
                "  %{prefix}.next = add i64 %{prefix}.i, 1\n  br label %{prefix}.combine.head"
            )?;
            self.output.open_block(format!("{prefix}.free"));
            // Acquisition computed this product without overflow.
            self.output.symbol("wf__heap_give");
            writeln!(
                self.output,
                "  %{prefix}.size = mul nuw i64 {count}, ptrtoint (ptr getelementptr ({private_ty}, ptr null, i64 1) to i64)\n  %{prefix}.empty = icmp eq i64 %{prefix}.size, 0\n  %{prefix}.bytes = select i1 %{prefix}.empty, i64 1, i64 %{prefix}.size\n  call void @wf__heap_give(ptr {pointer}, i64 %{prefix}.bytes)"
            )?;
        }
        Ok(())
    }

    pub(super) fn emit_indexed_combine_value(
        &mut self,
        spec: &crate::ir::IrIndexedReduction,
        prefix: &str,
        ty: &str,
    ) -> Result<(), BackendFailure> {
        let IrIndexedFamilyKind::Reduce { op, .. } = spec.kind else {
            return Err(BackendFailure::InvalidIr);
        };
        let opcode = match op {
            Ok(IrIntegerOperation::AddWrap) => "add",
            Ok(IrIntegerOperation::MultiplyWrap) => "mul",
            Ok(IrIntegerOperation::AddSaturating) => {
                let IrType::Integer {
                    width,
                    signed: false,
                } = spec.projection.value_type
                else {
                    return Err(BackendFailure::InvalidIr);
                };
                let intrinsic = format!("llvm.uadd.sat.i{width}");
                self.intrinsics.insert(IntrinsicDeclaration::Binary {
                    name: intrinsic.clone(),
                    ty: ty.to_owned(),
                });
                self.output.symbol(&intrinsic);
                writeln!(
                    self.output,
                    "  %{prefix}.value = call {ty} @{intrinsic}({ty} %{prefix}.left, {ty} %{prefix}.right)"
                )?;
                ""
            }
            Ok(IrIntegerOperation::BitAnd) | Err(IrBooleanOperation::And) => "and",
            Ok(IrIntegerOperation::BitOr) | Err(IrBooleanOperation::Or) => "or",
            Ok(IrIntegerOperation::BitXor) | Err(IrBooleanOperation::ExclusiveOr) => "xor",
            Ok(operation @ (IrIntegerOperation::Minimum | IrIntegerOperation::Maximum)) => {
                let IrType::Integer { signed, .. } = spec.projection.value_type else {
                    return Err(BackendFailure::InvalidIr);
                };
                let comparison = match (operation, signed) {
                    (IrIntegerOperation::Minimum, true) => "slt",
                    (IrIntegerOperation::Minimum, false) => "ult",
                    (IrIntegerOperation::Maximum, true) => "sgt",
                    _ => "ugt",
                };
                writeln!(
                    self.output,
                    "  %{prefix}.choose = icmp {comparison} {ty} %{prefix}.left, %{prefix}.right\n  %{prefix}.value = select i1 %{prefix}.choose, {ty} %{prefix}.left, {ty} %{prefix}.right"
                )?;
                ""
            }
            _ => return Err(BackendFailure::InvalidIr),
        };
        if !opcode.is_empty() {
            writeln!(
                self.output,
                "  %{prefix}.value = {opcode} {ty} %{prefix}.left, %{prefix}.right"
            )?;
        }
        Ok(())
    }

    /// Both layouts are compiler-owned: select the byte stride and field
    /// offset before forming an inbounds pointer, never form an out-of-range
    /// source-layout pointer into a dense slab even on an untaken path.
    pub(super) fn indexed_pointer(
        &mut self,
        prefix: &str,
        range: (&str, IrType),
        index: &str,
        private: &str,
        projection: &IrIndexedProjection,
        private_type: IrType,
    ) -> Result<String, BackendFailure> {
        let (range, range_type) = range;
        let root_ty = self
            .output
            .type_name(self.program, projection.root_element)?;
        let dense_ty = self.output.type_name(self.program, private_type)?;
        let range_ty = self.output.type_name(self.program, range_type)?;
        let mut ty = projection.root_element;
        let mut indices = String::new();
        for field in &projection.fields {
            let IrType::Nominal(nominal) = ty else {
                return Err(BackendFailure::InvalidIr);
            };
            let IrNominalKind::Struct { fields } = &self
                .program
                .nominal(nominal)
                .ok_or(BackendFailure::InvalidIr)?
                .kind
            else {
                return Err(BackendFailure::InvalidIr);
            };
            ty = fields
                .get(*field as usize)
                .ok_or(BackendFailure::InvalidIr)?
                .ty;
            write!(indices, ", i32 {field}")?;
        }
        if ty != projection.value_type {
            return Err(BackendFailure::InvalidIr);
        }
        let field_offset = if indices.is_empty() {
            "0".to_owned()
        } else {
            format!("ptrtoint (ptr getelementptr ({root_ty}, ptr null, i64 0{indices}) to i64)")
        };
        writeln!(
            self.output,
            "  %{prefix}.base = extractvalue {range_ty} {range}, 0\n  %{prefix}.stride = select i1 {private}, i64 ptrtoint (ptr getelementptr ({dense_ty}, ptr null, i64 1) to i64), i64 ptrtoint (ptr getelementptr ({root_ty}, ptr null, i64 1) to i64)\n  %{prefix}.field = select i1 {private}, i64 0, i64 {field_offset}\n  %{prefix}.scaled = mul i64 {index}, %{prefix}.stride\n  %{prefix}.offset = add i64 %{prefix}.scaled, %{prefix}.field\n  %{prefix}.address = getelementptr inbounds i8, ptr %{prefix}.base, i64 %{prefix}.offset"
        )?;
        Ok(format!("%{prefix}.address"))
    }

    pub(super) fn emit_indexed_mark(
        &mut self,
        address: &str,
        private: &str,
        constant: IrConstant,
        value_type: IrType,
    ) -> Result<(), BackendFailure> {
        let prefix = format!("indexed.mark.{}", self.next_temporary()?);
        let ty = self.output.type_name(self.program, value_type)?;
        let constant = constant_operand(constant, value_type)?;
        writeln!(
            self.output,
            "  br i1 {private}, label %{prefix}.mask, label %{prefix}.source"
        )?;
        self.output.open_block(format!("{prefix}.mask"));
        writeln!(
            self.output,
            "  store i1 true, ptr {address}\n  br label %{prefix}.done"
        )?;
        self.output.open_block(format!("{prefix}.source"));
        writeln!(
            self.output,
            "  store {ty} {constant}, ptr {address}\n  br label %{prefix}.done"
        )?;
        self.output.open_block(format!("{prefix}.done"));
        Ok(())
    }
}
