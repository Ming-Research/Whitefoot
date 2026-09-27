//! Emission of the [TYPE-9] windows: the window itself, its subscript, and
//! the [OP-10] boundary operations over it.
//!
//! A window carries no per-slot tag and no runtime discriminant: `len` and,
//! on a `Ring`, `head` are the complete typestate [WIN-1]. A boundary
//! operation updates those words and the selected slot through the reference
//! the operand names.
//!
//! The layout is header-first, so the inline and the boxed placement of one
//! shape share one address computation
//! (compiler/storage-representation): a `Slots` is `{ len, slots }` and a
//! `Ring` is `{ len, head, slots }`, each with a `cap` word after `len` in
//! the runtime-capacity placement and none at all where the type constant
//! already fixes it.
//!
//! Slots uses its proved logical offset directly. A Ring window is `len`
//! slots beginning at `head` modulo `cap` [WIN-1], so its subscript at
//! logical offset `i` reads slot `(head + i) mod cap`. Because
//! `head < cap` and `i < len <= cap`, the sum is below `2 * cap` and the
//! modulus is one conditional subtract; no division is emitted.

use crate::{IrBoundary, IrElement, IrMeasure, IrWindowShape};

use super::*;

/// One emitted window's field layout [TYPE-9, WIN-1].
#[derive(Clone, Copy)]
struct RunShape {
    shape: IrWindowShape,
    element: IrElement,
    /// `Some` is the constant-capacity placement, whose capacity is the type
    /// constant and is stored nowhere.
    capacity: Option<u64>,
}

impl RunShape {
    const fn of(ty: IrType) -> Option<Self> {
        match ty {
            IrType::Window {
                shape,
                element,
                capacity,
            } => Some(Self {
                shape,
                element,
                capacity,
            }),
            _ => None,
        }
    }

    const fn element(self) -> IrElement {
        self.element
    }

    fn element_type(self, program: &IrProgram) -> Result<IrType, BackendFailure> {
        program
            .element(self.element())
            .ok_or(BackendFailure::InvalidIr)
    }

    /// The aggregate field index of `len`, which every row of [MSR-1]'s
    /// table has and which the header-first layout puts first.
    const fn length_field(self) -> u32 {
        0
    }

    /// The aggregate field index of `cap`, which only a runtime-capacity
    /// block stores.
    const fn capacity_field(self) -> Option<u32> {
        match self.capacity {
            Some(_) => None,
            None => Some(1),
        }
    }

    /// The aggregate field index of `head`, which only a `Ring` has [WIN-1].
    const fn head_field(self) -> Option<u32> {
        match (self.shape, self.capacity) {
            (IrWindowShape::Slots, _) => None,
            (IrWindowShape::Ring, Some(_)) => Some(1),
            (IrWindowShape::Ring, None) => Some(2),
        }
    }

    /// The aggregate field index of the slots.
    const fn slots_field(self) -> u32 {
        match (self.shape, self.capacity) {
            (IrWindowShape::Slots, Some(_)) => 1,
            (IrWindowShape::Slots, None) | (IrWindowShape::Ring, Some(_)) => 2,
            (IrWindowShape::Ring, None) => 3,
        }
    }
}

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    fn run_value_type(&self, run: IrValueId) -> Result<IrType, BackendFailure> {
        Ok(
            match self.value_type(run).ok_or(BackendFailure::InvalidIr)? {
                IrType::Address(referent) => referent.ty(),
                ty => ty,
            },
        )
    }

    fn run_storage(&mut self, run: IrValueId) -> Result<Option<String>, BackendFailure> {
        if matches!(self.value_type(run), Some(IrType::Address(_))) {
            Ok(Some(self.value_name(run)))
        } else if self.storage.slot(run).is_some() {
            self.value_place(run).map(Some)
        } else {
            Ok(None)
        }
    }

    fn prepare_run_update(
        &mut self,
        result: IrValueId,
        run: IrValueId,
        ty: IrType,
    ) -> Result<IrValueId, BackendFailure> {
        if matches!(self.value_type(run), Some(IrType::Address(_))) {
            return Ok(run);
        }
        if self.storage.slot(result).is_some() {
            let source = self.value_place(run)?;
            let destination = self.value_place(result)?;
            self.copy_storage(ty, &source, &destination)?;
            Ok(result)
        } else {
            Ok(run)
        }
    }

    pub(super) fn run_element_place(
        &mut self,
        run: IrValueId,
        offset: IrValueId,
        element: IrType,
        target_domain: IrTargetDomainObligation,
    ) -> Result<String, BackendFailure> {
        let run_type = self.run_value_type(run)?;
        let Some(shape) = RunShape::of(run_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        if target_domain != IrTargetDomainObligation::ElementAddress
            || shape.element_type(self.program)? != element
            || self.value_type(offset)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let head = self.window_origin(shape, run_type, run)?;
        let physical = self.wrap_offset(shape, run_type, run, &head, &self.value_name(offset))?;
        self.element_pointer(run, shape, run_type, run, &physical)
            .map(|pointer| format!("%{pointer}"))
    }

    /// [BLK-2] `fixed_vector`: the empty window over `n` raw slots.
    ///
    /// The value is the zero aggregate, so both descriptor words start at
    /// zero, which is exactly the row's four published relations.
    pub(super) fn emit_fixed_vector(
        &mut self,
        result: IrValueId,
        ty: IrType,
    ) -> Result<(), BackendFailure> {
        let Some(_) = RunShape::of(ty) else {
            return Err(BackendFailure::InvalidIr);
        };
        let run_type = llvm_type(self.program, ty)?;
        let destination = self.value_place(result)?;
        writeln!(
            self.output,
            "  store {run_type} zeroinitializer, ptr {destination}",
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [MSR-1] one measure of a storage shape, read at run time.
    pub(super) fn emit_container_measure(
        &mut self,
        result: IrValueId,
        ty: IrType,
        measure: IrMeasure,
        container: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let container_type = self.run_value_type(container)?;
        // A runtime-capacity `Array<T>` has no window at all: every slot
        // holds a value, so the one stored count is its `len`, and x1's
        // [MSR-1] table gives the two `Array` rows neither a `cap` cell nor a
        // `head` cell [WIN-1]. That word heads its block, exactly as a boxed
        // window's does (compiler/storage-representation), and the block is
        // reached only by pointer [TYPE-9].
        if matches!(container_type, IrType::Buffer { .. }) {
            return match measure {
                IrMeasure::Length => {
                    let address = self
                        .run_storage(container)?
                        .ok_or(BackendFailure::InvalidIr)?;
                    let pointer = self.aggregate_field_pointer(container_type, &address, 0)?;
                    writeln!(
                        self.output,
                        "  {} = load i64, ptr {pointer}",
                        self.value_name(result)
                    )
                    .map_err(|_| BackendFailure::TextEmission)
                }
                IrMeasure::Capacity | IrMeasure::Head => Err(BackendFailure::InvalidIr),
            };
        }
        let Some(shape) = RunShape::of(container_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        let value = match measure {
            IrMeasure::Length => self.run_word(container_type, container, shape.length_field())?,
            // `head` is a `Ring`'s alone [WIN-1]; a `Slots` window begins at
            // slot zero and the measure table gives it no cell at all.
            IrMeasure::Head => {
                let field = shape.head_field().ok_or(BackendFailure::InvalidIr)?;
                self.run_word(container_type, container, field)?
            }
            // A constant capacity is the type constant and never reaches
            // emission; a runtime one is the block's own word.
            IrMeasure::Capacity => {
                let field = shape.capacity_field().ok_or(BackendFailure::InvalidIr)?;
                self.run_word(container_type, container, field)?
            }
        };
        writeln!(
            self.output,
            "  {} = add i64 {value}, 0",
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [REF-4] one range reference formed over typed owner storage.
    ///
    /// The window is `len` slots beginning at `head`, and the row's own
    /// requirement `vector.head <= vector.cap` is discharged before
    /// this operation exists [BLK-0], so `head + len <= cap` and the window
    /// is one contiguous range: the descriptor is the address of slot `head`
    /// together with `len`, and no modulus is emitted.
    ///
    /// A complete array instead contributes its type's length and the address
    /// of its first slot, without descriptor metadata in the owner.
    /// Both view modes point into the checked owner's stable storage. A
    /// descriptor copy does not create storage or prolong its lifetime.
    pub(super) fn emit_slice_from_run(
        &mut self,
        result: IrValueId,
        ty: IrType,
        run: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrType::Range { element } = ty else {
            return Err(BackendFailure::InvalidIr);
        };
        let run_type = self.run_value_type(run)?;
        if let IrType::Array {
            element: actual,
            length,
        } = run_type
        {
            if actual != element || !matches!(self.value_type(run), Some(IrType::Address(_))) {
                return Err(BackendFailure::InvalidIr);
            }
            let pointer = self.value_name(run);
            return self.emit_slice_descriptor(result, ty, &pointer, length);
        }
        let Some(shape) = RunShape::of(run_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        if shape.element_type(self.program)?
            != self
                .program
                .element(element)
                .ok_or(BackendFailure::InvalidIr)?
        {
            return Err(BackendFailure::InvalidIr);
        }
        let head = self.window_origin(shape, run_type, run)?;
        let length = self.run_word(run_type, run, shape.length_field())?;
        let pointer = self.element_pointer(result, shape, run_type, run, &head)?;
        let descriptor_type = llvm_type(self.program, ty)?;
        let partial = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{partial} = insertvalue {descriptor_type} zeroinitializer, ptr %{pointer}, 0\n  {} = insertvalue {descriptor_type} %{partial}, i64 {length}, 1",
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [OP-4, WIN-1] one discharged subscript read at logical offset `i`.
    pub(super) fn emit_run_index(
        &mut self,
        result: IrValueId,
        ty: IrType,
        run: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    ) -> Result<(), BackendFailure> {
        if target_domain != IrTargetDomainObligation::ElementAddress {
            return Err(BackendFailure::InvalidIr);
        }
        let run_type = self.run_value_type(run)?;
        let Some(shape) = RunShape::of(run_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        if shape.element_type(self.program)? != ty
            || self.value_type(offset)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let head = self.window_origin(shape, run_type, run)?;
        let offset = self.value_name(offset);
        let physical = self.wrap_offset(shape, run_type, run, &head, &offset)?;
        let element_pointer = self.element_pointer(result, shape, run_type, run, &physical)?;
        self.load_place_result(result, ty, &format!("%{element_pointer}"))
    }

    /// [OP-10] capture the old physical slot, move the descriptor and return
    /// its element. Descriptor words and a nonempty element's bytes are
    /// disjoint in both Slots and Ring; a zero-sized element touches no bytes.
    /// No call, release or source observation intervenes. Writing the header
    /// first lets LLVM forward the element through later aggregate moves.
    pub(super) fn emit_run_taken(
        &mut self,
        result: IrValueId,
        ty: IrType,
        row: IrBoundary,
        run: IrValueId,
    ) -> Result<(), BackendFailure> {
        if row.places() || !matches!(self.value_type(run), Some(IrType::Address(_))) {
            return Err(BackendFailure::InvalidIr);
        }
        let run_type = self.run_value_type(run)?;
        let Some(shape) = RunShape::of(run_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        if shape.element_type(self.program)? != ty {
            return Err(BackendFailure::InvalidIr);
        }
        let physical = self.boundary_slot(shape, run_type, run, row)?;
        let element_pointer = self.element_pointer(result, shape, run_type, run, &physical)?;
        self.move_run_boundary(shape, run_type, run, row)?;
        self.load_place_result(result, ty, &format!("%{element_pointer}"))
    }

    /// [OP-10] place an element, then move the boundary.
    pub(super) fn emit_run_boundary(
        &mut self,
        result: IrValueId,
        ty: IrType,
        row: IrBoundary,
        run: IrValueId,
        value: Option<IrValueId>,
    ) -> Result<(), BackendFailure> {
        let run_type = self.run_value_type(run)?;
        if ty != IrType::Unit || !matches!(self.value_type(run), Some(IrType::Address(_))) {
            return Err(BackendFailure::InvalidIr);
        }
        let Some(shape) = RunShape::of(run_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        let Some(value) = value else {
            return Err(BackendFailure::InvalidIr);
        };
        if !row.places() || self.value_type(value) != Some(shape.element_type(self.program)?) {
            return Err(BackendFailure::InvalidIr);
        }
        let updated = self.prepare_run_update(result, run, run_type)?;
        let physical = self.boundary_slot(shape, run_type, run, row)?;
        let element_pointer = self.element_pointer(result, shape, run_type, updated, &physical)?;
        self.store_value_at(value, &format!("%{element_pointer}"))?;
        self.move_run_boundary(shape, run_type, run, row)?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    fn move_run_boundary(
        &mut self,
        shape: RunShape,
        run_type: IrType,
        run: IrValueId,
        row: IrBoundary,
    ) -> Result<(), BackendFailure> {
        let length = self.run_word(run_type, run, shape.length_field())?;
        let head = self.window_origin(shape, run_type, run)?;
        // The new descriptor words. A back operation leaves `head` where it
        // was; a front operation moves it by one, modulo the capacity.
        let new_length = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{new_length} = {} i64 {length}, 1",
            if row.places() { "add" } else { "sub" },
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        // A front row's new window origin is exactly the slot it just
        // touched: a front placement's is the slot it wrote, and a front
        // removal's is one past the slot it read.
        let new_head = if row.front() {
            if row.places() {
                self.boundary_slot(shape, run_type, run, row)?
            } else {
                self.wrap_offset(shape, run_type, run, &head, "1")?
            }
        } else {
            head
        };
        let destination = self.run_storage(run)?.ok_or(BackendFailure::InvalidIr)?;
        let length_address =
            self.aggregate_field_pointer(run_type, &destination, shape.length_field() as usize)?;
        writeln!(
            self.output,
            "  store i64 %{new_length}, ptr {length_address}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        // Only a `Ring` stores a window origin [WIN-1]; a `Slots` window
        // begins at slot zero and no operation moves it, so a front
        // operation over one is no row of [OP-10] this emitter can serve.
        if let Some(field) = shape.head_field() {
            let head_address =
                self.aggregate_field_pointer(run_type, &destination, field as usize)?;
            writeln!(self.output, "  store i64 {new_head}, ptr {head_address}")
                .map_err(|_| BackendFailure::TextEmission)?;
        } else if row.front() {
            return Err(BackendFailure::InvalidIr);
        }
        Ok(())
    }

    /// The physical slot one boundary operation touches [WIN-1].
    ///
    /// A back operation touches the slot one past the window's last, which is
    /// `(head + len) mod cap` for a placement and `(head + len - 1) mod cap`
    /// for a removal; a front placement touches `(head + cap - 1) mod cap`
    /// and a front removal touches `head` itself.
    fn boundary_slot(
        &mut self,
        shape: RunShape,
        run_type: IrType,
        run: IrValueId,
        row: IrBoundary,
    ) -> Result<String, BackendFailure> {
        let head = self.window_origin(shape, run_type, run)?;
        match row {
            IrBoundary::TakeFront => Ok(head),
            // Placement proves cap > 0 and the Ring invariant gives head <
            // cap. Select a positive predecessor base before subtracting:
            // head + cap - 1 can overflow even for header-only storage.
            IrBoundary::PlaceFront => {
                let capacity = self.run_capacity(shape, run_type, run)?;
                let at_start = self.next_temporary()?;
                let predecessor = self.next_temporary()?;
                let physical = self.next_temporary()?;
                writeln!(
                    self.output,
                    "  %{at_start} = icmp eq i64 {head}, 0\n  %{predecessor} = select i1 %{at_start}, i64 {capacity}, i64 {head}\n  %{physical} = sub i64 %{predecessor}, 1",
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                Ok(format!("%{physical}"))
            }
            IrBoundary::PlaceBack | IrBoundary::TakeBack => {
                let length = self.run_word(run_type, run, shape.length_field())?;
                let offset = if row.places() {
                    length
                } else {
                    let previous = self.next_temporary()?;
                    writeln!(self.output, "  %{previous} = sub i64 {length}, 1")
                        .map_err(|_| BackendFailure::TextEmission)?;
                    format!("%{previous}")
                };
                self.wrap_offset(shape, run_type, run, &head, &offset)
            }
        }
    }

    /// A Slots offset already names its physical slot: OP-4 and OP-10 prove
    /// the selected element exists, or that a placement has spare capacity.
    /// Only Ring needs `(base + offset) mod cap` [WIN-1].
    fn wrap_offset(
        &mut self,
        shape: RunShape,
        run_type: IrType,
        run: IrValueId,
        base: &str,
        offset: &str,
    ) -> Result<String, BackendFailure> {
        if shape.shape == IrWindowShape::Slots {
            return Ok(offset.to_owned());
        }
        let capacity = self.run_capacity(shape, run_type, run)?;
        let sum = self.next_temporary()?;
        let over = self.next_temporary()?;
        let wrapped = self.next_temporary()?;
        let physical = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{sum} = add i64 {base}, {offset}\n  %{over} = icmp uge i64 %{sum}, {capacity}\n  %{wrapped} = sub i64 %{sum}, {capacity}\n  %{physical} = select i1 %{over}, i64 %{wrapped}, i64 %{sum}",
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{physical}"))
    }

    /// One descriptor word of a run.
    fn run_word(
        &mut self,
        run_type: IrType,
        run: IrValueId,
        field: u32,
    ) -> Result<String, BackendFailure> {
        let llvm = llvm_type(self.program, run_type)?;
        let word = self.next_temporary()?;
        if let Some(address) = self.run_storage(run)? {
            let pointer = self.aggregate_field_pointer(run_type, &address, field as usize)?;
            writeln!(self.output, "  %{word} = load i64, ptr {pointer}")
                .map_err(|_| BackendFailure::TextEmission)?;
            return Ok(format!("%{word}"));
        }
        writeln!(
            self.output,
            "  %{word} = extractvalue {llvm} {}, {field}",
            self.value_name(run),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{word}"))
    }

    /// The run's capacity: a `FixedVector`'s type constant, or a `Vector`'s
    /// own descriptor word.
    fn run_capacity(
        &mut self,
        shape: RunShape,
        run_type: IrType,
        run: IrValueId,
    ) -> Result<String, BackendFailure> {
        match (shape.capacity, shape.capacity_field()) {
            (Some(capacity), _) => Ok(capacity.to_string()),
            (None, Some(field)) => self.run_word(run_type, run, field),
            (None, None) => Err(BackendFailure::InvalidIr),
        }
    }

    /// The window origin [WIN-1]: a `Ring`'s stored `head`, and the constant
    /// zero for a `Slots`, whose window begins at slot zero and whose
    /// measure table gives it no `head` cell at all.
    fn window_origin(
        &mut self,
        shape: RunShape,
        run_type: IrType,
        run: IrValueId,
    ) -> Result<String, BackendFailure> {
        match shape.head_field() {
            Some(field) => self.run_word(run_type, run, field),
            None => Ok("0".to_owned()),
        }
    }

    /// The address of one physical slot of a window.
    ///
    /// The slots follow the header in the same block in both placements
    /// (compiler/storage-representation), so one `getelementptr` serves the
    /// inline window and the boxed one alike; only where the block address
    /// comes from differs.
    fn element_pointer(
        &mut self,
        _result: IrValueId,
        shape: RunShape,
        run_type: IrType,
        run: IrValueId,
        physical: &str,
    ) -> Result<String, BackendFailure> {
        let physical = self.element_address_index(shape.element_type(self.program)?, physical)?;
        let llvm = llvm_type(self.program, run_type)?;
        let slot = self.run_storage(run)?.ok_or(BackendFailure::InvalidIr)?;
        let pointer = self.next_temporary()?;
        if self.window_address_facts == WindowAddressFacts::Emit {
            // For positive stride S, the qualified complete object or
            // allocation has H + cap*S within the signed address domain.
            // OP-4/OP-10 and WIN-1 bound this physical index by cap, including
            // an empty range's one-past pointer. Its i64 value is therefore
            // nonnegative; for zero stride the actual operand above is zero.
            // Together with inbounds and the containing parent's qualified
            // extent, this states that the payload offset cannot reach back
            // into the header. It does not constrain logical Ring wrap sums.
            self.intrinsics.insert(IntrinsicDeclaration::Assume);
            writeln!(
                self.output,
                "  %{pointer}.nonnegative = icmp sge i64 {physical}, 0\n  call void @llvm.assume(i1 %{pointer}.nonnegative)"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        writeln!(
            self.output,
            "  %{pointer} = getelementptr inbounds {llvm}, ptr {slot}, i64 0, i32 {}, i64 {physical}",
            shape.slots_field(),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(pointer)
    }
}

/// The label one shift leaves its block at: the loop is three LLVM blocks, so
/// a later phi in the same IR block must name the block the walk fell out of.
pub(super) fn run_shift_done_label(result: IrValueId) -> String {
    format!("run.shift.done.v{}", result.ordinal())
}

fn run_shift_head_label(result: IrValueId) -> String {
    format!("run.shift.head.v{}", result.ordinal())
}

fn run_shift_body_label(result: IrValueId) -> String {
    format!("run.shift.body.v{}", result.ordinal())
}

fn run_shift_pre_label(result: IrValueId) -> String {
    format!("run.shift.pre.v{}", result.ordinal())
}

pub(super) fn run_transfer_done_label(result: IrValueId) -> String {
    format!("run.move.done.v{}", result.ordinal())
}

fn run_transfer_head_label(result: IrValueId) -> String {
    format!("run.move.head.v{}", result.ordinal())
}

fn run_transfer_body_label(result: IrValueId) -> String {
    format!("run.move.body.v{}", result.ordinal())
}

fn run_transfer_pre_label(result: IrValueId) -> String {
    format!("run.move.pre.v{}", result.ordinal())
}

pub(super) fn window_block_ready_label(result: IrValueId) -> String {
    format!("window.block.ready.v{}", result.ordinal())
}

fn window_block_oom_label(result: IrValueId) -> String {
    format!("window.block.oom.v{}", result.ordinal())
}

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    /// The byte size of one element of this window, as the target's own
    /// layout of it. Target qualification proved it no larger than the
    /// source ceiling [OP-9, STOR-6].
    fn window_element_size(&self, shape: RunShape) -> Result<String, BackendFailure> {
        let element = llvm_type(self.program, shape.element_type(self.program)?)?;
        Ok(format!(
            "ptrtoint (ptr getelementptr ({element}, ptr null, i64 1) to i64)"
        ))
    }

    /// The byte offset of the first slot: the block's header, which the
    /// header-first layout puts ahead of the elements
    /// (compiler/storage-representation).
    fn window_header_size(
        &self,
        shape: RunShape,
        run_type: IrType,
    ) -> Result<String, BackendFailure> {
        let block = llvm_type(self.program, run_type)?;
        Ok(format!(
            "ptrtoint (ptr getelementptr ({block}, ptr null, i64 0, i32 {}) to i64)",
            shape.slots_field()
        ))
    }

    /// Copies one element between two physical slots of two windows.
    fn copy_between_slots(
        &mut self,
        element: IrType,
        source: &str,
        destination: &str,
    ) -> Result<(), BackendFailure> {
        self.copy_storage(element, source, destination)
    }

    /// [OP-10] `insert_at`'s and `remove_at`'s one shift of `window.filled`,
    /// with the boundary move that shift makes room for or closes.
    ///
    /// The walk runs in the direction that never overwrites a slot it has
    /// not yet read, and every slot it touches is reached through the
    /// window's own coordinate system, so one walk serves a `Slots` and a
    /// wrapped `Ring` alike [WIN-1].
    pub(super) fn emit_run_shift(
        &mut self,
        result: IrValueId,
        ty: IrType,
        run: IrValueId,
        index: IrValueId,
        open: bool,
    ) -> Result<(), BackendFailure> {
        let run_type = self.run_value_type(run)?;
        if ty != IrType::Unit || !matches!(self.value_type(run), Some(IrType::Address(_))) {
            return Err(BackendFailure::InvalidIr);
        }
        let Some(shape) = RunShape::of(run_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        if self.value_type(index)
            != Some(IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let element = shape.element_type(self.program)?;
        let index = self.value_name(index);
        let pre = run_shift_pre_label(result);
        let head_label = run_shift_head_label(result);
        let body = run_shift_body_label(result);
        let done = run_shift_done_label(result);
        writeln!(self.output, "  br label %{pre}\n{pre}:")
            .map_err(|_| BackendFailure::TextEmission)?;
        let length = self.run_word(run_type, run, shape.length_field())?;
        let origin = self.window_origin(shape, run_type, run)?;
        // A closing shift stops one slot below the window's last.
        let limit = if open {
            index.clone()
        } else {
            let limit = self.next_temporary()?;
            writeln!(self.output, "  %{limit} = sub i64 {length}, 1")
                .map_err(|_| BackendFailure::TextEmission)?;
            format!("%{limit}")
        };
        let counter = self.next_temporary()?;
        let stepped = self.next_temporary()?;
        let more = self.next_temporary()?;
        let start = if open { length.clone() } else { index.clone() };
        let comparison = if open { "ugt" } else { "ult" };
        writeln!(
            self.output,
            "  br label %{head_label}\n{head_label}:\n  %{counter} = phi i64 [ {start}, %{pre} ], [ %{stepped}, %{body} ]\n  %{more} = icmp {comparison} i64 %{counter}, {limit}\n  br i1 %{more}, label %{body}, label %{done}\n{body}:"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        writeln!(
            self.output,
            "  %{stepped} = {} i64 %{counter}, 1",
            if open { "sub" } else { "add" }
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        // An opening shift reads the slot below and writes the one at the
        // counter; a closing shift reads the slot above and writes the one at
        // the counter.
        let destination_offset =
            self.wrap_offset(shape, run_type, run, &origin, &format!("%{counter}"))?;
        let destination =
            self.element_pointer(result, shape, run_type, run, &destination_offset)?;
        let source_offset =
            self.wrap_offset(shape, run_type, run, &origin, &format!("%{stepped}"))?;
        let source = self.element_pointer(result, shape, run_type, run, &source_offset)?;
        self.copy_between_slots(element, &format!("%{source}"), &format!("%{destination}"))?;
        writeln!(self.output, "  br label %{head_label}\n{done}:")
            .map_err(|_| BackendFailure::TextEmission)?;
        // The boundary move the shift opened or closed.
        let moved = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{moved} = {} i64 {length}, 1",
            if open { "add" } else { "sub" }
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let storage = self.run_storage(run)?.ok_or(BackendFailure::InvalidIr)?;
        let length_address =
            self.aggregate_field_pointer(run_type, &storage, shape.length_field() as usize)?;
        writeln!(self.output, "  store i64 %{moved}, ptr {length_address}")
            .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    /// [OP-10] `insert_at`'s placement into the slot the shift opened.
    pub(super) fn emit_run_insert(
        &mut self,
        result: IrValueId,
        ty: IrType,
        run: IrValueId,
        index: IrValueId,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        let run_type = self.run_value_type(run)?;
        if ty != IrType::Unit || !matches!(self.value_type(run), Some(IrType::Address(_))) {
            return Err(BackendFailure::InvalidIr);
        }
        let Some(shape) = RunShape::of(run_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        let element = shape.element_type(self.program)?;
        if self.value_type(value) != Some(element) {
            return Err(BackendFailure::InvalidIr);
        }
        let origin = self.window_origin(shape, run_type, run)?;
        let offset = self.value_name(index);
        let physical = self.wrap_offset(shape, run_type, run, &origin, &offset)?;
        let slot = self.element_pointer(result, shape, run_type, run, &physical)?;
        self.store_value_at(value, &format!("%{slot}"))?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    /// [OP-10] the run of elements `append` and `split_off` move between two
    /// windows, and the two boundary moves that go with it.
    pub(super) fn emit_run_transfer(
        &mut self,
        result: IrValueId,
        ty: IrType,
        destination: IrValueId,
        source: IrValueId,
        index: IrValueId,
    ) -> Result<(), BackendFailure> {
        let destination_type = self.run_value_type(destination)?;
        let source_type = self.run_value_type(source)?;
        if ty != IrType::Unit
            || !matches!(self.value_type(destination), Some(IrType::Address(_)))
            || !matches!(self.value_type(source), Some(IrType::Address(_)))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let (Some(destination_shape), Some(source_shape)) =
            (RunShape::of(destination_type), RunShape::of(source_type))
        else {
            return Err(BackendFailure::InvalidIr);
        };
        let element = source_shape.element_type(self.program)?;
        if destination_shape.element_type(self.program)? != element {
            return Err(BackendFailure::InvalidIr);
        }
        let index = self.value_name(index);
        let pre = run_transfer_pre_label(result);
        let head_label = run_transfer_head_label(result);
        let body = run_transfer_body_label(result);
        let done = run_transfer_done_label(result);
        writeln!(self.output, "  br label %{pre}\n{pre}:")
            .map_err(|_| BackendFailure::TextEmission)?;
        let source_length = self.run_word(source_type, source, source_shape.length_field())?;
        let destination_length = self.run_word(
            destination_type,
            destination,
            destination_shape.length_field(),
        )?;
        let source_origin = self.window_origin(source_shape, source_type, source)?;
        let destination_origin =
            self.window_origin(destination_shape, destination_type, destination)?;
        let count = self.next_temporary()?;
        let counter = self.next_temporary()?;
        let stepped = self.next_temporary()?;
        let more = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{count} = sub i64 {source_length}, {index}\n  br label %{head_label}\n{head_label}:\n  %{counter} = phi i64 [ 0, %{pre} ], [ %{stepped}, %{body} ]\n  %{more} = icmp ult i64 %{counter}, %{count}\n  br i1 %{more}, label %{body}, label %{done}\n{body}:\n  %{stepped} = add i64 %{counter}, 1"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let source_logical = self.next_temporary()?;
        let destination_logical = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{source_logical} = add i64 {index}, %{counter}\n  %{destination_logical} = add i64 {destination_length}, %{counter}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let source_offset = self.wrap_offset(
            source_shape,
            source_type,
            source,
            &source_origin,
            &format!("%{source_logical}"),
        )?;
        let source_slot =
            self.element_pointer(result, source_shape, source_type, source, &source_offset)?;
        let destination_offset = self.wrap_offset(
            destination_shape,
            destination_type,
            destination,
            &destination_origin,
            &format!("%{destination_logical}"),
        )?;
        let destination_slot = self.element_pointer(
            result,
            destination_shape,
            destination_type,
            destination,
            &destination_offset,
        )?;
        self.copy_between_slots(
            element,
            &format!("%{source_slot}"),
            &format!("%{destination_slot}"),
        )?;
        writeln!(self.output, "  br label %{head_label}\n{done}:")
            .map_err(|_| BackendFailure::TextEmission)?;
        let grown = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{grown} = add i64 {destination_length}, %{count}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let source_storage = self.run_storage(source)?.ok_or(BackendFailure::InvalidIr)?;
        let source_length_address = self.aggregate_field_pointer(
            source_type,
            &source_storage,
            source_shape.length_field() as usize,
        )?;
        writeln!(
            self.output,
            "  store i64 {index}, ptr {source_length_address}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let destination_storage = self
            .run_storage(destination)?
            .ok_or(BackendFailure::InvalidIr)?;
        let destination_length_address = self.aggregate_field_pointer(
            destination_type,
            &destination_storage,
            destination_shape.length_field() as usize,
        )?;
        writeln!(
            self.output,
            "  store i64 %{grown}, ptr {destination_length_address}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    /// [OP-13] one runtime-capacity window block and the cell that owns it.
    ///
    /// The block is `[len | cap | head? | slots]` in one allocation, so the
    /// cell pointer is the block pointer and every later access reaches the
    /// header and the slots through one address
    /// (compiler/storage-representation). The window starts empty, which is
    /// exactly what the row's `ensures` publishes.
    pub(super) fn emit_window_block_new(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        capacity: IrValueId,
        obligations: crate::IrAllocationObligations,
    ) -> Result<(), BackendFailure> {
        if !obligations.target_domains.is_complete() || ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let block_type = *referent;
        let Some(shape) = RunShape::of(block_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        if shape.capacity.is_some()
            || self.value_type(capacity)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let element_size = self.window_element_size(shape)?;
        let header_size = self.window_header_size(shape, block_type)?;
        let block = llvm_type(self.program, block_type)?;
        let slots_bytes = self.next_temporary()?;
        let bytes = self.next_temporary()?;
        let nonnull = self.next_temporary()?;
        let ready = window_block_ready_label(result);
        let oom = window_block_oom_label(result);
        writeln!(
            self.output,
            "  %{slots_bytes} = mul nuw i64 {}, {element_size}\n  %{bytes} = add nuw i64 %{slots_bytes}, {header_size}\n  {} = call ptr @malloc(i64 %{bytes})\n  %{nonnull} = icmp ne ptr {}, null\n  br i1 %{nonnull}, label %{ready}, label %{oom}\n{oom}:\n  call void @wf_resource_abort()\n  unreachable\n{ready}:",
            self.value_name(capacity),
            self.value_name(result),
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let block_address = self.value_name(result);
        let length_address = self.aggregate_field_pointer(
            block_type,
            &block_address,
            shape.length_field() as usize,
        )?;
        writeln!(self.output, "  store i64 0, ptr {length_address}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let capacity_field = shape.capacity_field().ok_or(BackendFailure::InvalidIr)?;
        let capacity_address =
            self.aggregate_field_pointer(block_type, &block_address, capacity_field as usize)?;
        writeln!(
            self.output,
            "  store i64 {}, ptr {capacity_address}",
            self.value_name(capacity)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if let Some(head) = shape.head_field() {
            let head_address =
                self.aggregate_field_pointer(block_type, &block_address, head as usize)?;
            writeln!(self.output, "  store i64 0, ptr {head_address}")
                .map_err(|_| BackendFailure::TextEmission)?;
        }
        let _ = block;
        Ok(())
    }

    /// [OP-10] `grow`: the cell's content is remade whole at the new
    /// capacity.
    ///
    /// One allocation, one copy of the header and the filled slots, one
    /// free, and the cell's pointer slot takes the new block. [STOR-7] makes
    /// the copying route legal at every value, because no judgment depends
    /// on the block's address.
    pub(super) fn emit_window_grow(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        cell: IrValueId,
        capacity: IrValueId,
        obligations: crate::IrAllocationObligations,
    ) -> Result<(), BackendFailure> {
        if !obligations.target_domains.is_complete()
            || ty != IrType::Unit
            || self.value_type(cell) != Some(IrType::Address(IrAddressed::Nominal(nominal)))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let block_type = *referent;
        let Some(shape) = RunShape::of(block_type) else {
            return Err(BackendFailure::InvalidIr);
        };
        if shape.capacity.is_some() || shape.shape != IrWindowShape::Slots {
            return Err(BackendFailure::InvalidIr);
        }
        let element_size = self.window_element_size(shape)?;
        let header_size = self.window_header_size(shape, block_type)?;
        let cell_address = self.value_name(cell);
        let old = self.next_temporary()?;
        writeln!(self.output, "  %{old} = load ptr, ptr {cell_address}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let old_block = format!("%{old}");
        let old_length_address =
            self.aggregate_field_pointer(block_type, &old_block, shape.length_field() as usize)?;
        let length = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{length} = load i64, ptr {old_length_address}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let slots_bytes = self.next_temporary()?;
        let bytes = self.next_temporary()?;
        let fresh = self.next_temporary()?;
        let nonnull = self.next_temporary()?;
        let ready = window_block_ready_label(result);
        let oom = window_block_oom_label(result);
        writeln!(
            self.output,
            "  %{slots_bytes} = mul nuw i64 {}, {element_size}\n  %{bytes} = add nuw i64 %{slots_bytes}, {header_size}\n  %{fresh} = call ptr @malloc(i64 %{bytes})\n  %{nonnull} = icmp ne ptr %{fresh}, null\n  br i1 %{nonnull}, label %{ready}, label %{oom}\n{oom}:\n  call void @wf_resource_abort()\n  unreachable\n{ready}:",
            self.value_name(capacity),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let fresh_block = format!("%{fresh}");
        let fresh_length_address =
            self.aggregate_field_pointer(block_type, &fresh_block, shape.length_field() as usize)?;
        writeln!(
            self.output,
            "  store i64 %{length}, ptr {fresh_length_address}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let capacity_field = shape.capacity_field().ok_or(BackendFailure::InvalidIr)?;
        let fresh_capacity_address =
            self.aggregate_field_pointer(block_type, &fresh_block, capacity_field as usize)?;
        writeln!(
            self.output,
            "  store i64 {}, ptr {fresh_capacity_address}",
            self.value_name(capacity)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        // The filled slots move as bytes: no value's judgment depends on its
        // address [STOR-7], and a `Slots` window begins at slot zero, so the
        // filled prefix is one contiguous extent.
        let old_slots = self.next_temporary()?;
        let fresh_slots = self.next_temporary()?;
        let moved = self.next_temporary()?;
        let block = llvm_type(self.program, block_type)?;
        writeln!(
            self.output,
            "  %{old_slots} = getelementptr inbounds {block}, ptr %{old}, i64 0, i32 {slots}, i64 0\n  %{fresh_slots} = getelementptr inbounds {block}, ptr %{fresh}, i64 0, i32 {slots}, i64 0\n  %{moved} = mul nuw i64 %{length}, {element_size}",
            slots = shape.slots_field(),
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.intrinsics.insert(IntrinsicDeclaration::MemoryMove);
        writeln!(
            self.output,
            "  call void @llvm.memmove.p0.p0.i64(ptr %{fresh_slots}, ptr %{old_slots}, i64 %{moved}, i1 false)\n  call void @free(ptr %{old})\n  store ptr %{fresh}, ptr {cell_address}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }

    /// [OP-14] the cell of a boxed window proved empty.
    pub(super) fn emit_cell_free(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Unit || self.value_type(value) != Some(IrType::Nominal(nominal)) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Box { .. } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        writeln!(
            self.output,
            "  call void @free(ptr {})",
            self.value_name(value)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.emit_constant(result, ty, IrConstant::Unit)
    }
}
