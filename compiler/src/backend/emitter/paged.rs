//! Address-stable paged windows and their noncontiguous run references.
//!
//! Only directory pointers move during growth. All size checks precede their
//! allocation and use the selected target's element stride [STOR-6].

use super::*;

const DESCRIPTOR: &str = "{ i64, i64, ptr, i64 }";

impl FunctionEmitter<'_, '_> {
    pub(super) fn paged_directory(&mut self, paged: &str) -> Result<String, BackendFailure> {
        let address = self.next_temporary()?;
        let directory = self.next_temporary()?;
        writeln!(self.output, "  %{address} = getelementptr inbounds {DESCRIPTOR}, ptr {paged}, i32 0, i32 2\n  %{directory} = load ptr, ptr %{address}").map_err(|_| BackendFailure::TextEmission)?;
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
        let cell = self.paged_allocate("32", &oom)?;
        // A nonnull directory also represents an empty run without a sentinel
        // pointer or an element allocation. Its first pointer remains raw.
        let directory = self.paged_allocate("8", &oom)?;
        let header = self.next_temporary()?;
        writeln!(self.output, "  %{header} = insertvalue {DESCRIPTOR} {{ i64 0, i64 0, ptr null, i64 1 }}, ptr {directory}, 2\n  store {DESCRIPTOR} %{header}, ptr {cell}").map_err(|_| BackendFailure::TextEmission)?;
        self.paged_reserve(&cell, element, &self.value_name(capacity), result, &oom)?;
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
        let address = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{address} = load ptr, ptr {}",
            self.value_name(cell)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let oom = format!("paged.oom.v{}", result.ordinal());
        self.paged_reserve(
            &format!("%{address}"),
            element,
            &self.value_name(capacity),
            result,
            &oom,
        )?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    /// Reserve pages up to the requested capacity. The directory's capacity
    /// doubles until it fits, with one copy of its initialized pointers.
    fn paged_reserve(
        &mut self,
        cell: &str,
        element: IrType,
        capacity: &str,
        result: IrValueId,
        oom: &str,
    ) -> Result<(), BackendFailure> {
        let (b, stride) = crate::target::paged_geometry(self.target, self.program, element)
            .map_err(BackendFailure::TargetLayout)?;
        let tag = format!("paged.v{}", result.ordinal());
        let cap_address = self.next_temporary()?;
        let dir_address = self.next_temporary()?;
        let dircap_address = self.next_temporary()?;
        let oldcap = self.next_temporary()?;
        let olddir = self.next_temporary()?;
        let olddircap = self.next_temporary()?;
        writeln!(self.output, "  %{cap_address} = getelementptr inbounds {DESCRIPTOR}, ptr {cell}, i32 0, i32 1\n  %{dir_address} = getelementptr inbounds {DESCRIPTOR}, ptr {cell}, i32 0, i32 2\n  %{dircap_address} = getelementptr inbounds {DESCRIPTOR}, ptr {cell}, i32 0, i32 3\n  %{oldcap} = load i64, ptr %{cap_address}\n  %{olddir} = load ptr, ptr %{dir_address}\n  %{olddircap} = load i64, ptr %{dircap_address}").map_err(|_| BackendFailure::TextEmission)?;
        let oldpages = self.paged_count(&format!("%{oldcap}"), element)?;
        let pages = self.paged_count(capacity, element)?;
        writeln!(self.output, "  br label %{tag}.start")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.start"));
        writeln!(self.output, "  br label %{tag}.size")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.size"));
        let limit = self.target.runtime_allocation_max() / 16;
        writeln!(self.output, "  %{tag}.capacity = phi i64 [ %{olddircap}, %{tag}.start ], [ %{tag}.doubled, %{tag}.double ]\n  %{tag}.fits = icmp uge i64 %{tag}.capacity, {pages}\n  br i1 %{tag}.fits, label %{tag}.sized, label %{tag}.limit").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.limit"));
        writeln!(self.output, "  %{tag}.overflow = icmp ugt i64 %{tag}.capacity, {limit}\n  br i1 %{tag}.overflow, label %{oom}, label %{tag}.double").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.double"));
        writeln!(
            self.output,
            "  %{tag}.doubled = shl nuw i64 %{tag}.capacity, 1\n  br label %{tag}.size"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.sized"));
        writeln!(self.output, "  %{tag}.unchanged = icmp eq i64 %{tag}.capacity, %{olddircap}\n  br i1 %{tag}.unchanged, label %{tag}.existing, label %{tag}.resize").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.resize"));
        let bytes = self.emit_allocation_size(
            &format!("%{tag}.capacity"),
            "8",
            "0",
            oom,
            &format!("{tag}.dir.allocate"),
        )?;
        let fresh = self.paged_allocate(&bytes, oom)?;
        self.intrinsics.insert(IntrinsicDeclaration::MemoryMove);
        self.output.symbol("llvm.memmove.p0.p0.i64");
        self.output.symbol("free");
        writeln!(self.output, "  %{tag}.copied = mul nuw i64 {oldpages}, 8\n  call void @llvm.memmove.p0.p0.i64(ptr {fresh}, ptr %{olddir}, i64 %{tag}.copied, i1 false)\n  call void @free(ptr %{olddir})\n  br label %{tag}.resized").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.resized"));
        writeln!(self.output, "  br label %{tag}.directory")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.existing"));
        writeln!(self.output, "  br label %{tag}.directory")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.directory"));
        writeln!(self.output, "  %{tag}.dir = phi ptr [ {fresh}, %{tag}.resized ], [ %{olddir}, %{tag}.existing ]\n  store ptr %{tag}.dir, ptr %{dir_address}\n  store i64 %{tag}.capacity, ptr %{dircap_address}").map_err(|_| BackendFailure::TextEmission)?;
        writeln!(self.output, "  %{tag}.no_pages = icmp eq i64 {oldpages}, {pages}\n  br i1 %{tag}.no_pages, label %{tag}.done, label %{tag}.page.size").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.page.size"));
        let page_bytes = self.emit_allocation_size(
            &b.to_string(),
            &stride.to_string(),
            "0",
            oom,
            &format!("{tag}.pages.start"),
        )?;
        let nonzero = self.next_temporary()?;
        writeln!(self.output, "  %{nonzero}.empty = icmp eq i64 {page_bytes}, 0\n  %{nonzero} = select i1 %{nonzero}.empty, i64 1, i64 {page_bytes}").map_err(|_| BackendFailure::TextEmission)?;
        writeln!(self.output, "  br label %{tag}.pages")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.pages"));
        writeln!(self.output, "  %{tag}.page = phi i64 [ {oldpages}, %{tag}.pages.start ], [ %{tag}.next, %{tag}.stored ]\n  %{tag}.finished = icmp eq i64 %{tag}.page, {pages}\n  br i1 %{tag}.finished, label %{tag}.done, label %{tag}.allocate").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(format!("{tag}.allocate"));
        let page = self.paged_allocate(&format!("%{nonzero}"), oom)?;
        writeln!(self.output, "  %{tag}.slot = getelementptr inbounds ptr, ptr %{tag}.dir, i64 %{tag}.page\n  store ptr {page}, ptr %{tag}.slot\n  br label %{tag}.stored").map_err(|_| BackendFailure::TextEmission)?;
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
        writeln!(self.output, "  store i64 {capacity}, ptr %{cap_address}")
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
