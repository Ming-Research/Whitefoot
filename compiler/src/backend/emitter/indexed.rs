//! Private indexed families for a structured range split. One slab per
//! family contains the leaves' dense scalar cells or masks in leaf order. Only this site owns those
//! allocations; chunks borrow disjoint ranges and cannot release them.

use super::parallel::LoopSplitSite;
use super::*;
use crate::ir::{IrIndexedFamilyKind, IrIndexedProjection};

pub(super) struct IndexedPrivate {
    pub(super) budget: String,
    active: Option<String>,
    roots: Vec<PrivateRoot>,
}

struct PrivateRoot {
    pointer: String,
    cells: String,
    total: String,
    original: String,
}

impl FunctionEmitter<'_, '_> {
    /// Scheduling arithmetic saturates; allocation arithmetic below fails
    /// through STOR-8 instead. Overflow must never make a costly split cheap.
    fn indexed_saturating(
        &mut self,
        opcode: &str,
        left: &str,
        right: &str,
    ) -> Result<String, BackendFailure> {
        let name = format!("llvm.{opcode}.with.overflow.i64");
        self.intrinsics.insert(IntrinsicDeclaration::Overflow {
            name: name.clone(),
            ty: "i64".to_owned(),
        });
        self.output.symbol(&name);
        let pair = self.next_temporary()?;
        let value = self.next_temporary()?;
        let overflow = self.next_temporary()?;
        let result = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pair} = call {{ i64, i1 }} @{name}(i64 {left}, i64 {right})\n  %{value} = extractvalue {{ i64, i1 }} %{pair}, 0\n  %{overflow} = extractvalue {{ i64, i1 }} %{pair}, 1\n  %{result} = select i1 %{overflow}, i64 -1, i64 %{value}"
        )?;
        Ok(format!("%{result}"))
    }

    pub(super) fn emit_indexed_prepare(
        &mut self,
        split: &LoopSplitSite<'_>,
        span: &str,
        weight: &str,
        budget: &str,
        arguments: &mut [String],
    ) -> Result<IndexedPrivate, BackendFailure> {
        if split.indexed.is_empty() {
            return Ok(IndexedPrivate {
                budget: budget.to_owned(),
                active: None,
                roots: Vec::new(),
            });
        }
        let id = self.next_temporary()?;
        let allocate = format!("indexed.{id}.allocate");
        let sequential = format!("indexed.{id}.sequential");
        let ready = format!("indexed.{id}.ready");
        let join = format!("indexed.{id}.join");
        // The runtime's budget is small; guarding the shift also makes this
        // internal boundary total for a future budget policy. A leaf must
        // receive at least one source iteration.
        writeln!(
            self.output,
            "  %indexed.{id}.bounded = icmp ult i64 {budget}, 64\n  %indexed.{id}.shift = select i1 %indexed.{id}.bounded, i64 {budget}, i64 0\n  %indexed.{id}.leaves = shl i64 1, %indexed.{id}.shift\n  %indexed.{id}.nontrivial = icmp ugt i64 %indexed.{id}.leaves, 1\n  %indexed.{id}.fits = icmp ule i64 %indexed.{id}.leaves, {span}\n  %indexed.{id}.shape = and i1 %indexed.{id}.nontrivial, %indexed.{id}.fits"
        )?;
        let leaves = format!("%indexed.{id}.leaves");
        let mut cells = "0".to_owned();
        for root in split.indexed {
            let count = *split
                .captures
                .get(root.count)
                .ok_or(BackendFailure::InvalidIr)?;
            if self.value_type(count)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
            {
                return Err(BackendFailure::InvalidIr);
            }
            cells = self.indexed_saturating("uadd", &cells, &self.value_name(count))?;
        }
        // Charge one copy-equivalent traversal, identity fill, and combine
        // per leaf cell. No copy of a source seed is actually necessary.
        let replicated = self.indexed_saturating("umul", &cells, &leaves)?;
        let overhead = self.indexed_saturating("umul", &replicated, "3")?;
        let work = self.indexed_saturating("umul", span, weight)?;
        let active = format!("%indexed.{id}.active");
        let selected_budget = format!("%indexed.{id}.budget");
        writeln!(
            self.output,
            "  %indexed.{id}.pays = icmp ugt i64 {work}, {overhead}\n  {active} = and i1 %indexed.{id}.shape, %indexed.{id}.pays\n  {selected_budget} = select i1 {active}, i64 {budget}, i64 0\n  br i1 {active}, label %{allocate}, label %{sequential}"
        )?;
        self.output.open_block(allocate);
        let mut roots: Vec<PrivateRoot> = Vec::new();
        for (ordinal, root) in split.indexed.iter().enumerate() {
            let range = *split
                .captures
                .get(root.capture)
                .ok_or(BackendFailure::InvalidIr)?;
            let Some(IrType::Range { element }) = self.value_type(range) else {
                return Err(BackendFailure::InvalidIr);
            };
            if self.program.element(element) != Some(root.projection.root_element) {
                return Err(BackendFailure::InvalidIr);
            }
            let ty = self.output.type_name(self.program, root.private_type())?;
            let count = self.value_name(split.captures[root.count]);
            let prefix = format!("indexed.{id}.{ordinal}");
            let failed = format!("{prefix}.failed");
            let size = format!("{prefix}.size");
            let malloc = format!("{prefix}.malloc");
            let init = format!("{prefix}.init");
            let head = format!("{prefix}.fill.head");
            let body = format!("{prefix}.fill.body");
            let done = format!("{prefix}.fill.done");
            let total = self.emit_allocation_size(&count, &leaves, "0", &failed, &size)?;
            let stride = format!("ptrtoint (ptr getelementptr ({ty}, ptr null, i64 1) to i64)");
            let bytes = self.emit_allocation_size(&total, &stride, "0", &failed, &malloc)?;
            let pointer = format!("%{prefix}.allocation");
            self.output.symbol("malloc");
            writeln!(
                self.output,
                "  %{prefix}.empty = icmp eq i64 {bytes}, 0\n  %{prefix}.bytes = select i1 %{prefix}.empty, i64 1, i64 {bytes}\n  {pointer} = call ptr @malloc(i64 %{prefix}.bytes)\n  %{prefix}.ok = icmp ne ptr {pointer}, null\n  br i1 %{prefix}.ok, label %{init}, label %{failed}"
            )?;
            self.output.open_block(failed);
            for previous in &roots {
                self.output.symbol("free");
                writeln!(self.output, "  call void @free(ptr {})", previous.pointer)?;
            }
            self.output.symbol("wf_resource_abort");
            writeln!(
                self.output,
                "  call void @wf_resource_abort()\n  unreachable"
            )?;
            self.output.open_block(init.clone());
            writeln!(self.output, "  br label %{head}")?;
            self.output.open_block(head.clone());
            writeln!(
                self.output,
                "  %{prefix}.i = phi i64 [ 0, %{init} ], [ %{prefix}.next, %{body} ]\n  %{prefix}.more = icmp ult i64 %{prefix}.i, {total}\n  br i1 %{prefix}.more, label %{body}, label %{done}"
            )?;
            self.output.open_block(body);
            let identity = match root.kind {
                IrIndexedFamilyKind::Reduce { identity, .. } => identity,
                IrIndexedFamilyKind::Mark { .. } => IrConstant::Bool(false),
            };
            let identity = constant_operand(identity, root.private_type())?;
            writeln!(
                self.output,
                "  %{prefix}.cell = getelementptr inbounds {ty}, ptr {pointer}, i64 %{prefix}.i\n  store {ty} {identity}, ptr %{prefix}.cell\n  %{prefix}.next = add i64 %{prefix}.i, 1\n  br label %{head}"
            )?;
            self.output.open_block(done);
            roots.push(PrivateRoot {
                pointer,
                cells: count,
                total,
                original: self.value_name(range),
            });
        }
        writeln!(self.output, "  br label %{ready}")?;
        self.output.open_block(ready.clone());
        writeln!(self.output, "  br label %{join}")?;
        self.output.open_block(sequential.clone());
        writeln!(self.output, "  br label %{join}")?;
        self.output.open_block(join);
        // Emit every phi before descriptor construction or ABI projections.
        for (ordinal, root) in roots.iter_mut().enumerate() {
            let pointer = format!("%indexed.{id}.{ordinal}.private");
            let total = format!("%indexed.{id}.{ordinal}.total");
            writeln!(
                self.output,
                "  {pointer} = phi ptr [ {}, %{ready} ], [ null, %{sequential} ]\n  {total} = phi i64 [ {}, %{ready} ], [ 0, %{sequential} ]",
                root.pointer, root.total
            )?;
            root.pointer = pointer;
            root.total = total;
        }
        for (ordinal, (root, spec)) in roots.iter().zip(split.indexed).enumerate() {
            let range = split.captures[spec.capture];
            let range_type = self.value_type(range).ok_or(BackendFailure::InvalidIr)?;
            let ty = self.output.type_name(self.program, range_type)?;
            let prefix = format!("indexed.{id}.{ordinal}");
            writeln!(
                self.output,
                "  %{prefix}.pointer = insertvalue {ty} zeroinitializer, ptr {}, 0\n  %{prefix}.range = insertvalue {ty} %{prefix}.pointer, i64 {}, 1\n  %{prefix}.selected = select i1 {active}, {ty} %{prefix}.range, {ty} {}",
                root.pointer, root.total, root.original
            )?;
            arguments[3 + spec.capture] = self.value_argument(
                ParameterAbi::Value(range_type),
                &format!("%{prefix}.selected"),
            )?;
            let original_private = self.value_name(split.captures[spec.private]);
            writeln!(
                self.output,
                "  %{prefix}.is_private = or i1 {active}, {original_private}"
            )?;
            arguments[3 + spec.private] = self.value_argument(
                ParameterAbi::Value(IrType::Bool),
                &format!("%{prefix}.is_private"),
            )?;
        }
        Ok(IndexedPrivate {
            budget: selected_budget,
            active: Some(active),
            roots,
        })
    }

    pub(super) fn emit_indexed_finish(
        &mut self,
        split: &LoopSplitSite<'_>,
        private: IndexedPrivate,
    ) -> Result<(), BackendFailure> {
        let Some(active) = private.active else {
            return Ok(());
        };
        let id = self.next_temporary()?;
        let combine = format!("indexed.{id}.combine");
        let finished = format!("indexed.{id}.finished");
        writeln!(
            self.output,
            "  br i1 {active}, label %{combine}, label %{finished}"
        )?;
        self.output.open_block(combine);
        for (ordinal, (root, spec)) in private.roots.iter().zip(split.indexed).enumerate() {
            let prefix = format!("indexed.{id}.{ordinal}");
            let entry = format!("{prefix}.entry");
            let head = format!("{prefix}.combine.head");
            let body = format!("{prefix}.combine.body");
            let done = format!("{prefix}.free");
            if let IrIndexedFamilyKind::Mark { constant } = spec.kind {
                self.emit_indexed_mark_finish(split, root, spec, &prefix, constant)?;
                continue;
            }
            let ty = self
                .output
                .type_name(self.program, spec.projection.value_type)?;
            writeln!(self.output, "  br label %{entry}")?;
            self.output.open_block(entry.clone());
            writeln!(self.output, "  br label %{head}")?;
            self.output.open_block(head.clone());
            // Flat ascending index is leaf-major, cell-minor. Empty roots
            // never enter the body, so the remainder always has nonzero cells.
            writeln!(
                self.output,
                "  %{prefix}.i = phi i64 [ 0, %{entry} ], [ %{prefix}.next, %{body} ]\n  %{prefix}.more = icmp ult i64 %{prefix}.i, {}\n  br i1 %{prefix}.more, label %{body}, label %{done}",
                root.total
            )?;
            self.output.open_block(body);
            writeln!(
                self.output,
                "  %{prefix}.index = urem i64 %{prefix}.i, {}",
                root.cells
            )?;
            let destination = self.indexed_pointer(
                &format!("{prefix}.destination"),
                (
                    &root.original,
                    self.value_type(split.captures[spec.capture])
                        .ok_or(BackendFailure::InvalidIr)?,
                ),
                &format!("%{prefix}.index"),
                &self.value_name(split.captures[spec.private]),
                &spec.projection,
                spec.private_type(),
            )?;
            writeln!(
                self.output,
                "  %{prefix}.from = getelementptr inbounds {ty}, ptr {}, i64 %{prefix}.i\n  %{prefix}.left = load {ty}, ptr {destination}\n  %{prefix}.right = load {ty}, ptr %{prefix}.from",
                root.pointer
            )?;
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
                        ty: ty.clone(),
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
            writeln!(
                self.output,
                "  store {ty} %{prefix}.value, ptr {destination}\n  %{prefix}.next = add i64 %{prefix}.i, 1\n  br label %{head}"
            )?;
            self.output.open_block(done);
            self.output.symbol("free");
            writeln!(self.output, "  call void @free(ptr {})", root.pointer)?;
        }
        writeln!(self.output, "  br label %{finished}")?;
        self.output.open_block(finished);
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

    fn emit_indexed_mark_finish(
        &mut self,
        split: &LoopSplitSite<'_>,
        root: &PrivateRoot,
        spec: &crate::ir::IrIndexedReduction,
        prefix: &str,
        constant: IrConstant,
    ) -> Result<(), BackendFailure> {
        let entry = format!("{prefix}.entry");
        let head = format!("{prefix}.cells");
        let leaves = format!("{prefix}.leaves");
        let scan = format!("{prefix}.scan");
        let decide = format!("{prefix}.decide");
        let store = format!("{prefix}.store");
        let next = format!("{prefix}.next");
        let done = format!("{prefix}.free");
        writeln!(self.output, "  br label %{entry}")?;
        self.output.open_block(entry.clone());
        writeln!(self.output, "  br label %{head}")?;
        self.output.open_block(head.clone());
        writeln!(
            self.output,
            "  %{prefix}.cell = phi i64 [ 0, %{entry} ], [ %{prefix}.nextcell, %{next} ]\n  %{prefix}.more = icmp ult i64 %{prefix}.cell, {}\n  br i1 %{prefix}.more, label %{leaves}, label %{done}",
            root.cells
        )?;
        self.output.open_block(leaves.clone());
        writeln!(
            self.output,
            "  %{prefix}.i = phi i64 [ %{prefix}.cell, %{head} ], [ %{prefix}.nextleaf, %{scan} ]\n  %{prefix}.any = phi i1 [ false, %{head} ], [ %{prefix}.merged, %{scan} ]\n  %{prefix}.hasleaf = icmp ult i64 %{prefix}.i, {}\n  br i1 %{prefix}.hasleaf, label %{scan}, label %{decide}",
            root.total
        )?;
        self.output.open_block(scan);
        writeln!(
            self.output,
            "  %{prefix}.from = getelementptr inbounds i1, ptr {}, i64 %{prefix}.i\n  %{prefix}.marked = load i1, ptr %{prefix}.from\n  %{prefix}.merged = or i1 %{prefix}.any, %{prefix}.marked\n  %{prefix}.nextleaf = add i64 %{prefix}.i, {}\n  br label %{leaves}",
            root.pointer, root.cells
        )?;
        self.output.open_block(decide);
        writeln!(
            self.output,
            "  br i1 %{prefix}.any, label %{store}, label %{next}"
        )?;
        self.output.open_block(store);
        let destination = self.indexed_pointer(
            &format!("{prefix}.destination"),
            (
                &root.original,
                self.value_type(split.captures[spec.capture])
                    .ok_or(BackendFailure::InvalidIr)?,
            ),
            &format!("%{prefix}.cell"),
            &self.value_name(split.captures[spec.private]),
            &spec.projection,
            IrType::Bool,
        )?;
        self.emit_indexed_mark(
            &destination,
            &self.value_name(split.captures[spec.private]),
            constant,
            spec.projection.value_type,
        )?;
        writeln!(self.output, "  br label %{next}")?;
        self.output.open_block(next);
        writeln!(
            self.output,
            "  %{prefix}.nextcell = add i64 %{prefix}.cell, 1\n  br label %{head}"
        )?;
        self.output.open_block(done);
        self.output.symbol("free");
        writeln!(self.output, "  call void @free(ptr {})", root.pointer)?;
        Ok(())
    }
}
