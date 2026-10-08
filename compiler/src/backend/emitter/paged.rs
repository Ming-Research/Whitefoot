//! Address-stable paged windows and their noncontiguous run references.
//!
//! Growth replaces the header-first cell, copying its header and page pointers
//! only. Every directory entry is initialized: pages covering len are allocated,
//! and later entries are null or retain pages emptied by take_back. Construction
//! and growth reserve directory space; place_back allocates a missing page at
//! its first slot. All size checks precede their allocation and use the selected
//! target's element stride [STOR-6].

use crate::IrElement;

use super::*;

pub(super) const CELL: &str = "{ i64, i64, i64, [0 x ptr] }";
const HEADER: &str = "{ i64, i64, i64 }";

impl FunctionEmitter<'_, '_> {
    pub(super) fn paged_directory(&mut self, paged: &str) -> Result<String, BackendFailure> {
        let directory = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{directory} = getelementptr inbounds {CELL}, ptr {paged}, i32 0, i32 3, i64 0"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{directory}"))
    }

    /// Overflow-free ceil(count/B), including count == u64::MAX and B == 1.
    pub(super) fn paged_count(
        &mut self,
        count: &str,
        element: IrType,
    ) -> Result<String, BackendFailure> {
        let (b, _) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        let q = self.next_temporary()?;
        let r = self.next_temporary()?;
        let tail = self.next_temporary()?;
        let carry = self.next_temporary()?;
        let pages = self.next_temporary()?;
        writeln!(self.output, "  %{q} = lshr i64 {count}, {shift}\n  %{r} = and i64 {count}, {mask}\n  %{tail} = icmp ne i64 %{r}, 0\n  %{carry} = zext i1 %{tail} to i64\n  %{pages} = add nuw i64 %{q}, %{carry}", shift=b.trailing_zeros(), mask=b-1).map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{pages}"))
    }

    /// Returns a temporary name, as the ordinary window element path does.
    pub(super) fn paged_element_pointer(
        &mut self,
        directory: &str,
        index: &str,
        element: IrType,
    ) -> Result<String, BackendFailure> {
        let (b, stride) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        let page = self.next_temporary()?;
        let offset = self.next_temporary()?;
        let slot = self.next_temporary()?;
        let pointer = self.next_temporary()?;
        let address = self.next_temporary()?;
        let llvm = self.output.type_name(self.program, element)?;
        let displacement = if stride == 0 {
            "0".to_owned()
        } else {
            format!("%{offset}")
        };
        writeln!(self.output, "  %{page} = lshr i64 {index}, {shift}\n  %{offset} = and i64 {index}, {mask}\n  %{slot} = getelementptr inbounds ptr, ptr {directory}, i64 %{page}\n  %{pointer} = load ptr, ptr %{slot}\n  %{address} = getelementptr inbounds {llvm}, ptr %{pointer}, i64 {displacement}", shift=b.trailing_zeros(), mask=b-1).map_err(|_| BackendFailure::TextEmission)?;
        Ok(address)
    }

    pub(super) fn emit_paged_page_len(
        &mut self,
        result: IrValueId,
        ty: IrType,
        element: IrElement,
    ) -> Result<(), BackendFailure> {
        let element = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        let (b, _) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        self.emit_constant(result, ty, IrConstant::Integer { ty, bits: b })
    }

    pub(super) fn emit_paged_run(
        &mut self,
        result: IrValueId,
        run: IrValueId,
        element: IrElement,
    ) -> Result<(), BackendFailure> {
        if !matches!(self.value_type(run), Some(IrType::Address(IrAddressed::Window { shape: IrWindowShape::Paged, element: actual, capacity: None })) if actual == element)
        {
            return Err(BackendFailure::InvalidIr);
        }
        let run = self.value_name(run);
        let directory = self.paged_directory(&run)?;
        let length = self.next_temporary()?;
        let partial = self.next_temporary()?;
        writeln!(self.output, "  %{length} = load i64, ptr {run}\n  %{partial} = insertvalue {{ ptr, i64, i64 }} zeroinitializer, ptr {directory}, 0\n  {} = insertvalue {{ ptr, i64, i64 }} %{partial}, i64 %{length}, 2", self.value_name(result)).map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_paged_page(
        &mut self,
        result: IrValueId,
        ty: IrType,
        paged: IrValueId,
        index: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrType::Range { element } = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        if !matches!(self.value_type(paged), Some(IrType::Address(IrAddressed::Window { shape: IrWindowShape::Paged, element: actual, capacity: None })) if actual == element)
        {
            return Err(BackendFailure::InvalidIr);
        }
        let element = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        let (b, _) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        let paged = self.value_name(paged);
        let index = self.value_name(index);
        let directory = self.paged_directory(&paged)?;
        let slot = self.next_temporary()?;
        let pointer = self.next_temporary()?;
        let length = self.next_temporary()?;
        let start = self.next_temporary()?;
        let remaining = self.next_temporary()?;
        let full = self.next_temporary()?;
        let count = self.next_temporary()?;
        let partial = self.next_temporary()?;
        writeln!(self.output, "  %{slot} = getelementptr inbounds ptr, ptr {directory}, i64 {index}\n  %{pointer} = load ptr, ptr %{slot}\n  %{length} = load i64, ptr {paged}\n  %{start} = shl nuw i64 {index}, {shift}\n  %{remaining} = sub nuw i64 %{length}, %{start}\n  %{full} = icmp uge i64 %{remaining}, {b}\n  %{count} = select i1 %{full}, i64 {b}, i64 %{remaining}\n  %{partial} = insertvalue {{ ptr, i64 }} poison, ptr %{pointer}, 0\n  {} = insertvalue {{ ptr, i64 }} %{partial}, i64 %{count}, 1", self.value_name(result), shift=b.trailing_zeros()).map_err(|_| BackendFailure::TextEmission)
    }

    /// Allocation failure shares the ordinary resource-exhaustion exit.
    fn paged_allocate(&mut self, bytes: &str, oom: &str) -> Result<String, BackendFailure> {
        let pointer = self.next_temporary()?;
        let valid = self.next_temporary()?;
        let ready = format!("paged.alloc.{pointer}");
        self.output.symbol("malloc");
        writeln!(self.output, "  %{pointer} = call ptr @malloc(i64 {bytes})\n  %{valid} = icmp ne ptr %{pointer}, null\n  br i1 %{valid}, label %{ready}, label %{oom}").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(ready);
        Ok(format!("%{pointer}"))
    }

    pub(super) fn emit_paged_new(
        &mut self,
        result: IrValueId,
        _ty: IrType,
        element: IrType,
        capacity: IrValueId,
    ) -> Result<(), BackendFailure> {
        let oom = format!("paged.oom.v{}", result.ordinal());
        let tag = format!("paged.v{}", result.ordinal());
        let capacity = self.value_name(capacity);
        let pages = self.paged_count(&capacity, element)?;
        let dircap = self.paged_directory_capacity("4", &pages, &tag, &oom)?;
        let bytes = self.emit_allocation_size(
            &dircap,
            "8",
            &crate::target::PAGED_HEADER_BYTES.to_string(),
            &oom,
            &format!("{tag}.cell.allocate"),
        )?;
        let cell = self.paged_allocate(&bytes, &oom)?;
        // Four directory entries avoid cell replacement for the first small grows
        // and keep an empty run's interior pointer within the cell.
        // Every unused entry is null, including the spare directory capacity.
        let header = self.next_temporary()?;
        writeln!(self.output, "  %{header} = insertvalue {HEADER} zeroinitializer, i64 {dircap}, 2\n  store {HEADER} %{header}, ptr {cell}").map_err(|_| BackendFailure::TextEmission)?;
        let directory = self.paged_directory(&cell)?;
        self.paged_initialize_entries(&directory, "0", &dircap, &tag)?;
        self.paged_publish_capacity(&cell, &capacity, &tag, &oom)?;
        writeln!(
            self.output,
            "  {} = getelementptr i8, ptr {cell}, i64 0",
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    pub(super) fn emit_paged_grow(
        &mut self,
        result: IrValueId,
        ty: IrType,
        element: IrType,
        cell: IrValueId,
        capacity: IrValueId,
    ) -> Result<(), BackendFailure> {
        let oom = format!("paged.oom.v{}", result.ordinal());
        let tag = format!("paged.v{}", result.ordinal());
        let owner = self.value_name(cell);
        let capacity = self.value_name(capacity);
        let old = self.next_temporary()?;
        let header = self.next_temporary()?;
        let olddircap = self.next_temporary()?;
        writeln!(self.output, "  %{old} = load ptr, ptr {owner}\n  %{header} = load {HEADER}, ptr %{old}\n  %{olddircap} = extractvalue {HEADER} %{header}, 2").map_err(|_| BackendFailure::TextEmission)?;
        let pages = self.paged_count(&capacity, element)?;
        let dircap = self.paged_directory_capacity(&format!("%{olddircap}"), &pages, &tag, &oom)?;
        writeln!(self.output, "  %{tag}.unchanged = icmp eq i64 {dircap}, %{olddircap}\n  br i1 %{tag}.unchanged, label %{tag}.existing, label %{tag}.resize").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.resize"));
        let bytes = self.emit_allocation_size(
            &dircap,
            "8",
            &crate::target::PAGED_HEADER_BYTES.to_string(),
            &oom,
            &format!("{tag}.cell.allocate"),
        )?;
        let fresh = self.paged_allocate(&bytes, &oom)?;
        let updated = self.next_temporary()?;
        writeln!(self.output, "  %{updated} = insertvalue {HEADER} %{header}, i64 {dircap}, 2\n  store {HEADER} %{updated}, ptr {fresh}").map_err(|_| BackendFailure::TextEmission)?;
        let olddir = self.paged_directory(&format!("%{old}"))?;
        let freshdir = self.paged_directory(&fresh)?;
        self.intrinsics.insert(IntrinsicDeclaration::MemoryMove);
        self.output.symbol("llvm.memmove.p0.p0.i64");
        self.output.symbol("free");
        writeln!(self.output, "  %{tag}.copied = mul nuw i64 %{olddircap}, 8\n  call void @llvm.memmove.p0.p0.i64(ptr {freshdir}, ptr {olddir}, i64 %{tag}.copied, i1 false)").map_err(|_| BackendFailure::TextEmission)?;
        self.paged_initialize_entries(&freshdir, &format!("%{olddircap}"), &dircap, &tag)?;
        writeln!(self.output, "  call void @free(ptr %{old})\n  store ptr {fresh}, ptr {owner}\n  br label %{tag}.resized").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.resized"));
        writeln!(self.output, "  br label %{tag}.ready")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.existing"));
        writeln!(self.output, "  br label %{tag}.ready")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.ready"));
        let cell = format!("%{tag}.cell");
        writeln!(
            self.output,
            "  {cell} = phi ptr [ {fresh}, %{tag}.resized ], [ %{old}, %{tag}.existing ]"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.paged_publish_capacity(&cell, &capacity, &tag, &oom)?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    /// Double from an existing positive directory capacity until it fits.
    /// The limit includes the header, so every doubled cell is allocatable.
    fn paged_directory_capacity(
        &mut self,
        initial: &str,
        pages: &str,
        tag: &str,
        oom: &str,
    ) -> Result<String, BackendFailure> {
        writeln!(self.output, "  br label %{tag}.start")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.start"));
        writeln!(self.output, "  br label %{tag}.size")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.size"));
        let limit = self
            .target
            .runtime_allocation_max()
            .saturating_sub(crate::target::PAGED_HEADER_BYTES)
            / 16;
        writeln!(self.output, "  %{tag}.capacity = phi i64 [ {initial}, %{tag}.start ], [ %{tag}.doubled, %{tag}.double ]\n  %{tag}.fits = icmp uge i64 %{tag}.capacity, {pages}\n  br i1 %{tag}.fits, label %{tag}.sized, label %{tag}.limit").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.limit"));
        writeln!(self.output, "  %{tag}.overflow = icmp ugt i64 %{tag}.capacity, {limit}\n  br i1 %{tag}.overflow, label %{oom}, label %{tag}.double").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.double"));
        writeln!(
            self.output,
            "  %{tag}.doubled = shl nuw i64 %{tag}.capacity, 1\n  br label %{tag}.size"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.sized"));
        Ok(format!("%{tag}.capacity"))
    }

    /// Initialize the new directory suffix; copied entries already hold either
    /// null or a page retained across growth and boundary removal.
    fn paged_initialize_entries(
        &mut self,
        directory: &str,
        start: &str,
        end: &str,
        tag: &str,
    ) -> Result<(), BackendFailure> {
        writeln!(self.output, "  br label %{tag}.entries.start")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.entries.start"));
        writeln!(self.output, "  br label %{tag}.entries")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.entries"));
        writeln!(self.output, "  %{tag}.entry = phi i64 [ {start}, %{tag}.entries.start ], [ %{tag}.next, %{tag}.entry.store ]\n  %{tag}.finished = icmp eq i64 %{tag}.entry, {end}\n  br i1 %{tag}.finished, label %{tag}.entries.done, label %{tag}.entry.store").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.entry.store"));
        writeln!(self.output, "  %{tag}.slot = getelementptr inbounds ptr, ptr {directory}, i64 %{tag}.entry\n  store ptr null, ptr %{tag}.slot\n  %{tag}.next = add nuw i64 %{tag}.entry, 1\n  br label %{tag}.entries").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.entries.done"));
        Ok(())
    }

    /// Publish the exact requested capacity after directory construction/growth.
    fn paged_publish_capacity(
        &mut self,
        cell: &str,
        capacity: &str,
        tag: &str,
        oom: &str,
    ) -> Result<(), BackendFailure> {
        writeln!(self.output, "  br label %{tag}.done")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(oom.to_owned());
        self.output.symbol("wf_resource_abort");
        self.output
            .push_str("  call void @wf_resource_abort()\n  unreachable\n");
        self.output.open_block(format!("{tag}.done"));
        let cap_address = self.next_temporary()?;
        writeln!(self.output, "  %{cap_address} = getelementptr inbounds {CELL}, ptr {cell}, i32 0, i32 1\n  store i64 {capacity}, ptr %{cap_address}")
            .map_err(|_| BackendFailure::TextEmission)
    }

    /// Only a page's first placement can encounter null. Interior placements
    /// use the allocated prefix; a page retained by take_back is reused.
    pub(super) fn paged_prepare_back(
        &mut self,
        result: IrValueId,
        cell: &str,
        length: &str,
        element: IrType,
    ) -> Result<(), BackendFailure> {
        let (b, stride) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        let tag = format!("paged.back.v{}", result.ordinal());
        let oom = format!("{tag}.oom");
        writeln!(self.output, "  %{tag}.offset = and i64 {length}, {mask}\n  %{tag}.boundary = icmp eq i64 %{tag}.offset, 0\n  br i1 %{tag}.boundary, label %{tag}.lookup, label %{tag}.ready", mask=b-1).map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.lookup"));
        let directory = self.paged_directory(cell)?;
        writeln!(self.output, "  %{tag}.page = lshr i64 {length}, {shift}\n  %{tag}.slot = getelementptr inbounds ptr, ptr {directory}, i64 %{tag}.page\n  %{tag}.pointer = load ptr, ptr %{tag}.slot\n  %{tag}.missing = icmp eq ptr %{tag}.pointer, null\n  br i1 %{tag}.missing, label %{tag}.size, label %{tag}.ready", shift=b.trailing_zeros()).map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.size"));
        let bytes = self.emit_allocation_size(
            &b.to_string(),
            &stride.to_string(),
            "0",
            &oom,
            &format!("{tag}.allocate"),
        )?;
        // Zero-stride pages still need distinct nonnull backing allocations.
        let nonzero = self.next_temporary()?;
        writeln!(self.output, "  %{nonzero}.empty = icmp eq i64 {bytes}, 0\n  %{nonzero} = select i1 %{nonzero}.empty, i64 1, i64 {bytes}").map_err(|_| BackendFailure::TextEmission)?;
        let page = self.paged_allocate(&format!("%{nonzero}"), &oom)?;
        writeln!(
            self.output,
            "  store ptr {page}, ptr %{tag}.slot\n  br label %{tag}.ready"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(oom);
        self.output.symbol("wf_resource_abort");
        self.output
            .push_str("  call void @wf_resource_abort()\n  unreachable\n");
        self.output.open_block(format!("{tag}.ready"));
        Ok(())
    }

    pub(super) fn run_reference_pointer(
        &mut self,
        run: IrValueId,
        offset: IrValueId,
        element: IrType,
    ) -> Result<String, BackendFailure> {
        let directory = self.next_temporary()?;
        let lo = self.next_temporary()?;
        let index = self.next_temporary()?;
        writeln!(self.output, "  %{directory} = extractvalue {{ ptr, i64, i64 }} {}, 0\n  %{lo} = extractvalue {{ ptr, i64, i64 }} {}, 1\n  %{index} = add nuw i64 %{lo}, {}", self.value_name(run), self.value_name(run), self.value_name(offset)).map_err(|_| BackendFailure::TextEmission)?;
        self.paged_element_pointer(&format!("%{directory}"), &format!("%{index}"), element)
    }
}
