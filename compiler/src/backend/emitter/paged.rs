//! Paged windows and their noncontiguous run references.
//!
//! Growth can replace a small first page until it reaches the full page
//! extent. Full pages remain stable. All size checks precede their allocation
//! and use the selected target's element stride [STOR-6].

use crate::IrElement;

use super::*;

pub(super) const CELL: &str = "{ i64, i64, i64, [0 x ptr] }";
const HEADER: &str = "{ i64, i64, i64 }";

/// Round the capacity clamped to [1, B] up to a power of two. Capacity zero
/// has no page and receives the placeholder extent one. The same calculation
/// reconstructs page zero's allocation on release.
pub(super) fn first_page_extent(
    output: &mut FunctionBody,
    capacity: &str,
    b: u64,
    tag: &str,
) -> Result<String, BackendFailure> {
    if b == 1 {
        return Ok("1".to_owned());
    }
    writeln!(output, "  %{tag}.below = icmp ult i64 {capacity}, {b}\n  %{tag}.bounded = select i1 %{tag}.below, i64 {capacity}, i64 {b}\n  %{tag}.empty = icmp eq i64 %{tag}.bounded, 0\n  %{tag}.positive = select i1 %{tag}.empty, i64 1, i64 %{tag}.bounded\n  %{tag}.minus = sub nuw i64 %{tag}.positive, 1").map_err(|_| BackendFailure::TextEmission)?;
    let mut spread = format!("%{tag}.minus");
    for shift in [1, 2, 4, 8, 16, 32] {
        if shift >= b.trailing_zeros() {
            break;
        }
        writeln!(output, "  %{tag}.shift{shift} = lshr i64 {spread}, {shift}\n  %{tag}.spread{shift} = or i64 {spread}, %{tag}.shift{shift}").map_err(|_| BackendFailure::TextEmission)?;
        spread = format!("%{tag}.spread{shift}");
    }
    writeln!(output, "  %{tag}.extent = add nuw i64 {spread}, 1")
        .map_err(|_| BackendFailure::TextEmission)?;
    Ok(format!("%{tag}.extent"))
}

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
        self.output.symbol("wf__heap_take");
        writeln!(self.output, "  %{pointer} = call ptr @wf__heap_take(i64 {bytes})\n  %{valid} = icmp ne ptr %{pointer}, null\n  br i1 %{valid}, label %{ready}, label %{oom}").map_err(|_| BackendFailure::TextEmission)?;
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
        self.output.symbol("wf__empty_window");
        writeln!(self.output, "  %{tag}.is_empty = icmp eq i64 {capacity}, 0\n  br i1 %{tag}.is_empty, label %{tag}.empty, label %{tag}.positive")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.empty"));
        writeln!(self.output, "  br label %{tag}.complete")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.positive"));
        let pages = self.paged_count(&capacity, element)?;
        let dircap = self.paged_directory_capacity("1", &pages, &tag, &oom)?;
        let bytes = self.emit_allocation_size(
            &dircap,
            "8",
            &crate::target::PAGED_HEADER_BYTES.to_string(),
            &oom,
            &format!("{tag}.cell.allocate"),
        )?;
        let cell = self.paged_allocate(&bytes, &oom)?;
        // Positive capacity has at least one directory entry. The shared
        // empty header also contains its empty run's interior pointer.
        let header = self.next_temporary()?;
        writeln!(self.output, "  %{header} = insertvalue {HEADER} zeroinitializer, i64 {dircap}, 2\n  store {HEADER} %{header}, ptr {cell}").map_err(|_| BackendFailure::TextEmission)?;
        self.paged_allocate_pages(&cell, element, &capacity, ("0", &pages), &tag, &oom)?;
        writeln!(self.output, "  br label %{tag}.complete")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.complete"));
        writeln!(
            self.output,
            "  {} = phi ptr [ @wf__empty_window, %{tag}.empty ], [ {cell}, %{tag}.done ]",
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
        // The contract then also makes the old capacity zero. Leave the
        // immutable shared header and its owner untouched.
        writeln!(self.output, "  %{tag}.empty = icmp eq i64 {capacity}, 0\n  br i1 %{tag}.empty, label %{tag}.complete, label %{tag}.positive")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.positive"));
        let old = self.next_temporary()?;
        let header = self.next_temporary()?;
        let oldcap = self.next_temporary()?;
        let olddircap = self.next_temporary()?;
        writeln!(self.output, "  %{old} = load ptr, ptr {owner}\n  %{header} = load {HEADER}, ptr %{old}\n  %{oldcap} = extractvalue {HEADER} %{header}, 1\n  %{olddircap} = extractvalue {HEADER} %{header}, 2").map_err(|_| BackendFailure::TextEmission)?;
        let oldpages = self.paged_count(&format!("%{oldcap}"), element)?;
        let pages = self.paged_count(&capacity, element)?;
        let initial = self.next_temporary()?;
        writeln!(self.output, "  %{initial}.empty = icmp eq i64 %{olddircap}, 0\n  %{initial} = select i1 %{initial}.empty, i64 1, i64 %{olddircap}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let dircap = self.paged_directory_capacity(&format!("%{initial}"), &pages, &tag, &oom)?;
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
        let old_directory_bytes = self.next_temporary()?;
        let old_bytes = self.next_temporary()?;
        writeln!(self.output,
            "  %{old_directory_bytes} = mul nuw i64 %{olddircap}, 8\n  %{old_bytes} = add nuw i64 %{old_directory_bytes}, {}",
            crate::target::PAGED_HEADER_BYTES,
        ).map_err(|_| BackendFailure::TextEmission)?;
        self.output.symbol("wf__heap_give");
        writeln!(self.output, "  %{tag}.copied = mul nuw i64 {oldpages}, 8\n  call void @llvm.memmove.p0.p0.i64(ptr {freshdir}, ptr {olddir}, i64 %{tag}.copied, i1 false)\n  call void @wf__heap_give(ptr %{old}, i64 %{old_bytes})\n  store ptr {fresh}, ptr {owner}\n  br label %{tag}.resized").map_err(|_| BackendFailure::TextEmission)?;
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
        self.paged_grow_first_page(&cell, element, &format!("%{oldcap}"), &capacity, &tag, &oom)?;
        self.paged_allocate_pages(&cell, element, &capacity, (&oldpages, &pages), &tag, &oom)?;
        writeln!(self.output, "  br label %{tag}.complete")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.complete"));
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    /// Replace only an allocated page zero whose rounded extent increases.
    /// Its live prefix is copied before the old allocation is released.
    fn paged_grow_first_page(
        &mut self,
        cell: &str,
        element: IrType,
        old_capacity: &str,
        capacity: &str,
        tag: &str,
        oom: &str,
    ) -> Result<(), BackendFailure> {
        let (b, stride) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        let old_extent = first_page_extent(
            &mut self.output,
            old_capacity,
            b,
            &format!("{tag}.first.old"),
        )?;
        let extent = first_page_extent(&mut self.output, capacity, b, &format!("{tag}.first.new"))?;
        writeln!(self.output, "  %{tag}.first.allocated = icmp ne i64 {old_capacity}, 0\n  %{tag}.first.larger = icmp ugt i64 {extent}, {old_extent}\n  %{tag}.first.replace = and i1 %{tag}.first.allocated, %{tag}.first.larger\n  br i1 %{tag}.first.replace, label %{tag}.first.size, label %{tag}.first.done")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.first.size"));
        let bytes = self.emit_allocation_size(
            &extent,
            &stride.to_string(),
            "0",
            oom,
            &format!("{tag}.first.allocate"),
        )?;
        let nonzero = self.next_temporary()?;
        writeln!(self.output, "  %{nonzero}.empty = icmp eq i64 {bytes}, 0\n  %{nonzero} = select i1 %{nonzero}.empty, i64 1, i64 {bytes}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let fresh = self.paged_allocate(&format!("%{nonzero}"), oom)?;
        let directory = self.paged_directory(cell)?;
        self.intrinsics.insert(IntrinsicDeclaration::MemoryMove);
        self.output.symbol("llvm.memmove.p0.p0.i64");
        self.output.symbol("wf__heap_give");
        writeln!(self.output, "  %{tag}.first.pointer = load ptr, ptr {directory}\n  %{tag}.first.len = load i64, ptr {cell}\n  %{tag}.first.copied = mul nuw i64 %{tag}.first.len, {stride}\n  %{tag}.first.old.bytes = mul nuw i64 {old_extent}, {stride}\n  %{tag}.first.old.zero = icmp eq i64 %{tag}.first.old.bytes, 0\n  %{tag}.first.old.nonzero = select i1 %{tag}.first.old.zero, i64 1, i64 %{tag}.first.old.bytes\n  call void @llvm.memmove.p0.p0.i64(ptr {fresh}, ptr %{tag}.first.pointer, i64 %{tag}.first.copied, i1 false)\n  call void @wf__heap_give(ptr %{tag}.first.pointer, i64 %{tag}.first.old.nonzero)\n  store ptr {fresh}, ptr {directory}\n  br label %{tag}.first.done")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.first.done"));
        Ok(())
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

    /// Add only the missing pages and publish the exact requested capacity.
    /// The caller has already sized the cell and, on growth, replaced its owner.
    fn paged_allocate_pages(
        &mut self,
        cell: &str,
        element: IrType,
        capacity: &str,
        (oldpages, pages): (&str, &str),
        tag: &str,
        oom: &str,
    ) -> Result<(), BackendFailure> {
        let (b, stride) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        let directory = self.paged_directory(cell)?;
        let first_extent =
            first_page_extent(&mut self.output, capacity, b, &format!("{tag}.pages.first"))?;
        writeln!(self.output, "  %{tag}.no_pages = icmp eq i64 {oldpages}, {pages}\n  br i1 %{tag}.no_pages, label %{tag}.done, label %{tag}.pages.start").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.pages.start"));
        writeln!(self.output, "  br label %{tag}.pages")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.pages"));
        writeln!(self.output, "  %{tag}.page = phi i64 [ {oldpages}, %{tag}.pages.start ], [ %{tag}.next, %{tag}.stored ]\n  %{tag}.finished = icmp eq i64 %{tag}.page, {pages}\n  br i1 %{tag}.finished, label %{tag}.done, label %{tag}.page.size").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.page.size"));
        writeln!(self.output, "  %{tag}.first_page = icmp eq i64 %{tag}.page, 0\n  %{tag}.page.extent = select i1 %{tag}.first_page, i64 {first_extent}, i64 {b}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let page_bytes = self.emit_allocation_size(
            &format!("%{tag}.page.extent"),
            &stride.to_string(),
            "0",
            oom,
            &format!("{tag}.allocate"),
        )?;
        let nonzero = self.next_temporary()?;
        writeln!(self.output, "  %{nonzero}.empty = icmp eq i64 {page_bytes}, 0\n  %{nonzero} = select i1 %{nonzero}.empty, i64 1, i64 {page_bytes}").map_err(|_| BackendFailure::TextEmission)?;
        let page = self.paged_allocate(&format!("%{nonzero}"), oom)?;
        writeln!(self.output, "  %{tag}.slot = getelementptr inbounds ptr, ptr {directory}, i64 %{tag}.page\n  store ptr {page}, ptr %{tag}.slot\n  br label %{tag}.stored").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.stored"));
        writeln!(
            self.output,
            "  %{tag}.next = add nuw i64 %{tag}.page, 1\n  br label %{tag}.pages"
        )
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
