//! The selected target's layout of IR values and runtime objects, shared by
//! lowering's optional loop actualization and by the backend's qualification
//! and emission.

use std::collections::{HashMap, HashSet};

use crate::{
    IrArrayRoot, IrElement, IrFunction, IrInstruction, IrLayoutCeiling, IrNominal, IrNominalId,
    IrNominalKind, IrOperation, IrProgram, IrTargetDomainObligation, IrType, IrValueId,
    IrWindowShape,
};

/// How large a lane frame a handed-out call is granted, in bytes.
///
/// This restates `WF_SCHED_FRAME_BYTES` in `backend/sched/core.h`, because the
/// decision to emit a [`IrOperation::LoopSplit`] at all has to be made long
/// before a runtime exists — and a split whose frame is over the bound would be
/// refused every lane at run time and sequentialize with no report. The two
/// numbers live in two languages and are pinned to each other by
/// `ordinary_lane_frame_limits_match_the_runtime_slot`.
pub(crate) const LANE_FRAME_BYTES: u64 = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetObject {
    Representation,
    RuntimeSizedAllocation,
    Static,
    FunctionAbi,
    StackFrame,
    ParallelLaneFrame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetLayoutFailure {
    UnsupportedHost,
    InvalidIr,
    Unrepresentable(TargetObject),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TargetLayout {
    triple: &'static str,
    data_layout: &'static str,
    address_index_max: u64,
    allocator_parameter_max: u64,
    allocator_alignment: u64,
    stack_probe: &'static str,
}

impl TargetLayout {
    /// The closed set of target ABI records the backend can describe.
    ///
    /// Keeping selection explicit lets tests inspect a target which is not the
    /// machine running them. A spelling not listed here is unsupported rather
    /// than being approximated by a nearby ABI: in particular, the Windows GNU
    /// and MSVC targets do not share a mangling or runtime contract.
    pub(crate) fn for_triple(triple: &str) -> Result<Self, TargetLayoutFailure> {
        match triple {
            "aarch64-apple-darwin" => Ok(Self {
                triple: "aarch64-apple-darwin",
                stack_probe: "__chkstk_darwin",
                data_layout: "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32",
                address_index_max: i64::MAX as u64,
                allocator_parameter_max: u64::MAX,
                allocator_alignment: 8,
            }),
            "x86_64-apple-darwin" => Ok(Self {
                triple: "x86_64-apple-darwin",
                stack_probe: "__chkstk_darwin",
                data_layout: "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
                address_index_max: i64::MAX as u64,
                allocator_parameter_max: u64::MAX,
                allocator_alignment: 8,
            }),
            "aarch64-unknown-linux-gnu" => Ok(Self {
                triple: "aarch64-unknown-linux-gnu",
                stack_probe: "inline-asm",
                data_layout: "e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128",
                address_index_max: i64::MAX as u64,
                allocator_parameter_max: u64::MAX,
                allocator_alignment: 8,
            }),
            "x86_64-unknown-linux-gnu" => Ok(Self {
                triple: "x86_64-unknown-linux-gnu",
                stack_probe: "inline-asm",
                data_layout: "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
                address_index_max: i64::MAX as u64,
                allocator_parameter_max: u64::MAX,
                allocator_alignment: 8,
            }),
            "x86_64-pc-windows-msvc" => Ok(Self {
                triple: "x86_64-pc-windows-msvc",
                // The x86-64 MSVC ABI probes a large downward-growing frame
                // through __chkstk before adjusting RSP. This is the symbol
                // clang emits for the target, and the MSVC runtime supplies it;
                // the GNU target's differently named helper is not compatible.
                stack_probe: "__chkstk",
                data_layout: "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
                address_index_max: i64::MAX as u64,
                allocator_parameter_max: u64::MAX,
                allocator_alignment: 8,
            }),
            _ => Err(TargetLayoutFailure::UnsupportedHost),
        }
    }

    pub(crate) fn host() -> Result<Self, TargetLayoutFailure> {
        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        {
            return Self::for_triple("aarch64-apple-darwin");
        }
        #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
        {
            return Self::for_triple("x86_64-apple-darwin");
        }
        #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
        {
            return Self::for_triple("aarch64-unknown-linux-gnu");
        }
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            return Self::for_triple("x86_64-unknown-linux-gnu");
        }
        #[cfg(all(
            target_arch = "x86_64",
            target_os = "windows",
            target_env = "msvc",
            target_vendor = "pc"
        ))]
        {
            return Self::for_triple("x86_64-pc-windows-msvc");
        }
        #[allow(unreachable_code)]
        Err(TargetLayoutFailure::UnsupportedHost)
    }

    pub(super) const fn triple(self) -> &'static str {
        self.triple
    }

    pub(super) const fn data_layout(self) -> &'static str {
        self.data_layout
    }

    /// The `probe-stack` value every generated function carries: the
    /// ABI-mandated helper an Apple or Windows target already names from its
    /// own C translation units, and the target-independent spelling — an
    /// inline page walk — on the supported Linux targets.
    ///
    /// A frame larger than the guard region must touch each page on its way
    /// down. Without that, the frame's first store can land past the guard in
    /// whatever is mapped below — on a pool build, another lane's live stack —
    /// and the write succeeds silently. The backend emits the walk only for a
    /// frame past the page threshold, so an ordinary frame pays nothing.
    pub(super) const fn stack_probe(self) -> &'static str {
        self.stack_probe
    }

    pub(super) const fn address_index_max(self) -> u64 {
        self.address_index_max
    }

    pub(super) const fn runtime_allocation_max(self) -> u64 {
        if self.address_index_max < self.allocator_parameter_max {
            self.address_index_max
        } else {
            self.allocator_parameter_max
        }
    }

    /// Minimum alignment guaranteed by the selected target's heap allocator.
    /// Keeping this explicit checks wider element representations against the
    /// allocator promise rather than assuming the address ABI guarantees it.
    pub(super) const fn runtime_allocation_alignment(self) -> u64 {
        self.allocator_alignment
    }

    /// Retains the selected target ABI while replacing only the heap-domain
    /// limits used by exact boundary tests.
    #[cfg(test)]
    pub(super) const fn with_runtime_allocation_limits_for_test(
        mut self,
        byte_maximum: u64,
        alignment: u64,
    ) -> Self {
        self.allocator_parameter_max = byte_maximum;
        self.allocator_alignment = alignment;
        self
    }

    /// Retains the selected target ABI while replacing only the address-index
    /// domain used by exact aggregate-layout boundary tests.
    #[cfg(test)]
    pub(crate) const fn with_address_index_max_for_test(mut self, maximum: u64) -> Self {
        self.address_index_max = maximum;
        self
    }
}

#[derive(Clone, Copy)]
struct Layout {
    size: u64,
    align: u64,
}

const POINTER_LAYOUT: Layout = Layout { size: 8, align: 8 };

/// One concrete type owned by target lowering rather than by Whitefoot's
/// source type system.
///
/// The emitter consumes this same tree when it renders an LLVM type. Keeping
/// source types and compiler-owned arrays/records in one closed vocabulary is
/// what lets layout and emission share one materialization plan instead of
/// maintaining parallel lists of strings and sizes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum TargetStorageType {
    Source(IrType),
    Integer(u16),
    Array {
        element: Box<TargetStorageType>,
        length: u64,
    },
}

impl TargetStorageType {
    pub(super) const fn source(ty: IrType) -> Self {
        Self::Source(ty)
    }

    pub(super) const fn integer(width: u16) -> Self {
        Self::Integer(width)
    }

    pub(super) fn array(element: Self, length: u64) -> Self {
        Self::Array {
            element: Box::new(element),
            length,
        }
    }

    pub(super) fn bytes(length: u64) -> Self {
        Self::array(Self::integer(8), length)
    }
}

/// One logical slot in a generated frame.
///
/// `alignment` is the alignment the emitter will state for the slot's
/// address. It may be stronger than the type's natural alignment for target
/// ABI byte records such as `stat`. The frame constructor inserts explicit
/// byte padding so the stated alignment is true rather than an LLVM hint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TargetFrameSlot {
    ty: TargetStorageType,
    alignment: Option<u64>,
}

impl TargetFrameSlot {
    pub(super) const fn natural(ty: TargetStorageType) -> Self {
        Self {
            ty,
            alignment: None,
        }
    }

    pub(super) const fn aligned(ty: TargetStorageType, alignment: u64) -> Self {
        Self {
            ty,
            alignment: Some(alignment),
        }
    }

    pub(super) const fn ty(&self) -> &TargetStorageType {
        &self.ty
    }
}

/// A logical slot's field and offset in the complete qualification layout.
/// When slots are emitted independently, each pointer is allocation-relative
/// zero; this offset is then footprint accounting, not a physical address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TargetFrameField {
    physical_index: u32,
    offset: u64,
}

impl TargetFrameField {
    pub(super) const fn physical_index(self) -> u32 {
        self.physical_index
    }

    #[cfg(test)]
    pub(super) const fn offset(self) -> u64 {
        self.offset
    }
}

/// A complete generated frame, qualified before choosing how to expose its
/// independent allocation roots to LLVM.
///
/// `physical_fields` includes explicit inter-slot and tail padding. Therefore
/// the LLVM struct rendered from it has exactly `layout`, even for a logical
/// byte array whose requested address alignment is stronger than its natural
/// type alignment. `logical_fields` maps each source/emitter slot, in the
/// caller's order, to the physical field that owns it. Positive-sized roots
/// with one common natural alignment and no padding may instead be separate
/// allocations: every ordering has the same complete extent. Other frames
/// keep the struct allocation, including zero-sized or over-aligned roots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TargetFramePlan {
    physical_fields: Vec<TargetStorageType>,
    logical_fields: Vec<TargetFrameField>,
    layout: TargetAggregateLayout,
    independent_slot_alignment: Option<u64>,
}

impl TargetFramePlan {
    pub(super) fn physical_fields(&self) -> &[TargetStorageType] {
        &self.physical_fields
    }

    pub(super) fn logical_field(&self, index: usize) -> Option<TargetFrameField> {
        self.logical_fields.get(index).copied()
    }

    pub(super) const fn layout(&self) -> TargetAggregateLayout {
        self.layout
    }

    pub(super) const fn independent_slot_alignment(&self) -> Option<u64> {
        self.independent_slot_alignment
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.logical_fields.is_empty()
    }
}

/// Constructs and validates one complete compiler-generated frame before its
/// allocation is rendered.
///
/// This consumes the emitter's actual slot descriptions. It does not inspect
/// generated LLVM and it is not an acceptance replay: the resulting physical
/// field list is the sole input from which the emitter may form that frame.
pub(super) fn plan_target_frame(
    target: TargetLayout,
    program: &IrProgram,
    slots: &[TargetFrameSlot],
) -> Result<TargetFramePlan, TargetLayoutFailure> {
    let mut layouts = LayoutComputer::new(target, program.nominals(), program.elements());
    let mut physical_fields = Vec::new();
    let mut logical_fields = Vec::with_capacity(slots.len());
    let mut size = 0_u64;
    let mut frame_alignment = 1_u64;
    let mut common_slot_alignment = None;
    let mut independent_slots = true;

    for slot in slots {
        let layout = layouts
            .storage_layout(slot.ty())
            .map_err(|failure| as_object(failure, TargetObject::StackFrame))?;
        let requested = slot.alignment.unwrap_or(layout.align);
        if !requested.is_power_of_two() || requested < layout.align {
            return Err(TargetLayoutFailure::InvalidIr);
        }
        let start = align_up(target, size, requested, TargetObject::StackFrame)?;
        independent_slots &= requested == layout.align
            && layout.size > 0
            && layout.size % requested == 0
            && start == size
            && common_slot_alignment.is_none_or(|alignment| alignment == requested);
        common_slot_alignment = Some(requested);
        if start != size {
            physical_fields.push(TargetStorageType::bytes(start - size));
        }
        let physical_index = u32::try_from(physical_fields.len())
            .map_err(|_| TargetLayoutFailure::Unrepresentable(TargetObject::StackFrame))?;
        physical_fields.push(slot.ty().clone());
        logical_fields.push(TargetFrameField {
            physical_index,
            offset: start,
        });
        size = checked_add(start, layout.size, target, TargetObject::StackFrame)?;
        frame_alignment = frame_alignment.max(requested);
    }

    let complete = align_up(target, size, frame_alignment, TargetObject::StackFrame)?;
    independent_slots &= complete == size;
    if complete != size {
        physical_fields.push(TargetStorageType::bytes(complete - size));
    }

    Ok(TargetFramePlan {
        physical_fields,
        logical_fields,
        layout: TargetAggregateLayout {
            size: complete,
            align: frame_alignment,
        },
        independent_slot_alignment: independent_slots.then_some(common_slot_alignment).flatten(),
    })
}

/// Whether element-address scaling vanishes on the selected target. This is
/// the same checked layout calculation used during program qualification,
/// not a source-type or optional optimizer-fact approximation.
pub(super) fn element_has_zero_stride(
    target: TargetLayout,
    program: &IrProgram,
    element: IrType,
) -> Result<bool, TargetLayoutFailure> {
    let mut layouts = LayoutComputer::new(target, program.nominals(), program.elements());
    Ok(layouts.layout(element)?.size == 0)
}

pub(super) fn validate_static_storage(
    target: TargetLayout,
    program: &IrProgram,
    ty: &TargetStorageType,
) -> Result<TargetAggregateLayout, TargetLayoutFailure> {
    let mut layouts = LayoutComputer::new(target, program.nominals(), program.elements());
    let layout = layouts
        .storage_layout(ty)
        .map_err(|failure| as_object(failure, TargetObject::Static))?;
    if layout.size > target.address_index_max() {
        return Err(TargetLayoutFailure::Unrepresentable(TargetObject::Static));
    }
    Ok(TargetAggregateLayout {
        size: layout.size,
        align: layout.align,
    })
}

/// The selected-target layout of one fully assembled backend aggregate.
///
/// A consumer retains the part of the result its emitted form needs: an
/// ordinary parallel hand-out passes the validated byte size to the lane
/// runtime. Tests inspect both values at exact target and runtime boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TargetAggregateLayout {
    size: u64,
    align: u64,
}

impl TargetAggregateLayout {
    pub(super) const fn size(self) -> u64 {
        self.size
    }

    pub(super) const fn align(self) -> u64 {
        self.align
    }
}

/// Alignment the compiler-owned parallel runtime guarantees for the byte
/// storage at the start of every worker slot.
///
/// This is a backend ABI constant, not a source-language limit. The matching
/// C declaration is pinned by a backend test beside the size contract.
pub(super) const PARALLEL_LANE_FRAME_ALIGNMENT: u64 = 16;

/// Computes the selected-target layout of the exact aggregate an ordinary
/// handed-out call would place in a runtime lane: every declared argument in
/// ABI order, followed by the result.
///
/// `Some` means both address formation and the runtime slot contract can hold
/// the aggregate. `None` is an ordinary optimization decline for a layout
/// which is representable on the selected target but wider or more aligned
/// than the lane slot. An address-domain failure remains a target-layout
/// failure, and malformed IR remains a compiler failure.
/// The lane frame one handed-out call needs: `{ arguments..., result }`, and
/// one `u64` more where the published callback enters a budget-carrying
/// variant and must carry that budget across the hand-out.
/// Only the lowered type tables and signature are needed, so the loop builder
/// can ask the same question before it reserves or files synthesized functions.
pub(crate) fn parallel_lane_frame_layout(
    target: TargetLayout,
    nominals: &[IrNominal],
    elements: &[IrType],
    parameters: impl IntoIterator<Item = IrType>,
    result: IrType,
    carries_budget: bool,
) -> Result<Option<TargetAggregateLayout>, TargetLayoutFailure> {
    let mut layouts = LayoutComputer::new(target, nominals, elements);
    let mut fields = Vec::new();
    for ty in parameters {
        fields.push(
            layouts
                .layout(ty)
                .map_err(|failure| as_object(failure, TargetObject::ParallelLaneFrame))?,
        );
    }
    fields.push(
        layouts
            .layout(result)
            .map_err(|failure| as_object(failure, TargetObject::ParallelLaneFrame))?,
    );
    if carries_budget {
        fields.push(
            layouts
                .layout(IrType::Integer {
                    width: 64,
                    signed: false,
                })
                .map_err(|failure| as_object(failure, TargetObject::ParallelLaneFrame))?,
        );
    }
    let layout = layouts.aggregate_layout(fields, TargetObject::ParallelLaneFrame)?;
    if layout.size > LANE_FRAME_BYTES || layout.align > PARALLEL_LANE_FRAME_ALIGNMENT {
        return Ok(None);
    }
    Ok(Some(TargetAggregateLayout {
        size: layout.size,
        align: layout.align,
    }))
}

pub(super) fn validate_program(
    target: TargetLayout,
    program: &IrProgram,
) -> Result<(), TargetLayoutFailure> {
    let mut layouts = LayoutComputer::new(target, program.nominals(), program.elements());

    for nominal in program.nominals() {
        layouts.layout(IrType::Nominal(nominal.id()))?;
    }
    for nominal in program.nominals() {
        if let IrNominalKind::Box { referent, .. } = nominal.kind() {
            layouts.layout(*referent)?;
        }
    }
    // A run descriptor's representation does not contain its elements.
    // Validate their complete layouts independently, so an ownership cycle
    // through a Vector does not become a false inline-layout cycle.
    for element in program.elements() {
        layouts.layout(*element)?;
    }
    for constant in program.constants() {
        layouts
            .layout(constant.ty())
            .map_err(|failure| as_object(failure, TargetObject::Static))?;
    }
    for function in program.functions() {
        validate_function(&mut layouts, program, function)?;
    }
    Ok(())
}

fn validate_function(
    layouts: &mut LayoutComputer<'_>,
    program: &IrProgram,
    function: &IrFunction,
) -> Result<(), TargetLayoutFailure> {
    layouts
        .layout(function.result())
        .map_err(|failure| as_object(failure, TargetObject::FunctionAbi))?;
    for (_, ty) in function.parameters() {
        layouts
            .layout(*ty)
            .map_err(|failure| as_object(failure, TargetObject::FunctionAbi))?;
    }
    let integer_upper_bounds = target_integer_result_bounds(layouts, function)?;
    validate_source_call_allocations(layouts, program, function, &integer_upper_bounds)?;

    for block in function.blocks() {
        for (_, ty) in block.parameters() {
            layouts.layout(*ty)?;
        }
        for instruction in block.instructions() {
            let IrInstruction::Define {
                result: _,
                ty,
                operation,
            } = instruction
            else {
                continue;
            };
            layouts.layout(*ty)?;
            validate_target_obligation(
                layouts,
                program,
                function,
                &integer_upper_bounds,
                *ty,
                operation,
            )?;
        }
    }
    Ok(())
}

/// Qualifies each ordinary call whose compiler-owned callee performs a
/// runtime-capacity allocation. The callee stays one out-of-line instance;
/// the accepted OP-9 upper bound stays on the call whose proof established it.
fn validate_source_call_allocations(
    layouts: &mut LayoutComputer<'_>,
    program: &IrProgram,
    function: &IrFunction,
    integer_upper_bounds: &HashMap<IrValueId, u64>,
) -> Result<(), TargetLayoutFailure> {
    let mut validated = HashSet::new();
    for block in function.blocks() {
        for instruction in block.instructions() {
            let IrInstruction::Define {
                result,
                operation:
                    IrOperation::Call {
                        function: callee,
                        arguments,
                    },
                ..
            } = instruction
            else {
                continue;
            };
            let callee = program
                .functions()
                .get(*callee as usize)
                .ok_or(TargetLayoutFailure::InvalidIr)?;
            let allocation_cell = function_runtime_allocation_cell(callee)?;
            let mut source_calls = function
                .source_calls()
                .iter()
                .filter(|source| source.result() == *result);
            let source_call = source_calls.next();
            if source_calls.next().is_some() {
                return Err(TargetLayoutFailure::InvalidIr);
            }
            match (
                allocation_cell,
                source_call.and_then(|call| call.allocation()),
            ) {
                (None, None) => {}
                (None, Some(_)) | (Some(_), None) => {
                    return Err(TargetLayoutFailure::InvalidIr);
                }
                (Some(cell), Some(allocation)) => {
                    if allocation.cell() != cell {
                        return Err(TargetLayoutFailure::InvalidIr);
                    }
                    let count = arguments
                        .get(allocation.count_argument())
                        .copied()
                        .ok_or(TargetLayoutFailure::InvalidIr)?;
                    if function.value_type(count)
                        != Some(IrType::Integer {
                            width: 64,
                            signed: false,
                        })
                    {
                        return Err(TargetLayoutFailure::InvalidIr);
                    }
                    let IrNominalKind::Box { referent, .. } = program
                        .nominal(cell)
                        .ok_or(TargetLayoutFailure::InvalidIr)?
                        .kind()
                    else {
                        return Err(TargetLayoutFailure::InvalidIr);
                    };
                    let allocation_layout = runtime_capacity_allocation_layout(
                        layouts,
                        *referent,
                        allocation.layout_ceiling(),
                    )?;
                    // A count read from an already represented allocation
                    // also has that allocation's selected-target bound.
                    // This is the same SSA-value qualification used for a
                    // direct allocation node; an out-of-line row must not
                    // lose it at the ordinary call boundary.
                    let source_upper_bound = allocation.source_length_upper_bound();
                    let length_upper_bound = integer_upper_bounds
                        .get(&count)
                        .copied()
                        .map_or(source_upper_bound, |bound| source_upper_bound.min(bound));
                    let byte_upper_bound = length_upper_bound
                        .checked_mul(allocation_layout.stride)
                        .and_then(|slots| slots.checked_add(allocation_layout.header))
                        .ok_or(TargetLayoutFailure::Unrepresentable(
                            TargetObject::RuntimeSizedAllocation,
                        ))?;
                    if byte_upper_bound > layouts.target.runtime_allocation_max() {
                        return Err(TargetLayoutFailure::Unrepresentable(
                            TargetObject::RuntimeSizedAllocation,
                        ));
                    }
                    validated.insert(*result);
                }
            }
        }
    }
    if function
        .source_calls()
        .iter()
        .any(|call| call.allocation().is_some() && !validated.contains(&call.result()))
    {
        return Err(TargetLayoutFailure::InvalidIr);
    }
    Ok(())
}

/// The cell allocated by one compiler-owned out-of-line row, if any. A row
/// has exactly one such operation; its callers carry that operation's OP-9
/// bound rather than specializing or cloning this function.
fn function_runtime_allocation_cell(
    function: &IrFunction,
) -> Result<Option<IrNominalId>, TargetLayoutFailure> {
    let mut cell = None;
    for block in function.blocks() {
        for instruction in block.instructions() {
            let IrInstruction::Define { operation, .. } = instruction else {
                continue;
            };
            let candidate = match operation {
                IrOperation::BufferFill { nominal, .. }
                | IrOperation::WindowBlockNew { nominal, .. }
                | IrOperation::WindowGrow { nominal, .. } => Some(*nominal),
                _ => None,
            };
            if let Some(candidate) = candidate
                && cell.replace(candidate).is_some()
            {
                return Err(TargetLayoutFailure::InvalidIr);
            }
        }
    }
    Ok(cell)
}

#[derive(Clone, Copy)]
struct RuntimeCapacityAllocationLayout {
    stride: u64,
    header: u64,
}

/// Computes the exact terms the emitter uses for `header + count * stride`.
/// The zero-length tail array can require padding after the fixed words, so
/// the header is its selected-target field offset rather than merely its word
/// count.
fn runtime_capacity_allocation_layout(
    layouts: &mut LayoutComputer<'_>,
    content: IrType,
    ceiling: IrLayoutCeiling,
) -> Result<RuntimeCapacityAllocationLayout, TargetLayoutFailure> {
    let (actual, allocation) = runtime_capacity_layout(layouts, content)?;
    if !ceiling.size.permits(actual.size)
        || actual.align > ceiling.align
        || !ceiling.stride.permits(allocation.stride)
    {
        return Err(TargetLayoutFailure::Unrepresentable(
            TargetObject::Representation,
        ));
    }
    Ok(allocation)
}

fn runtime_capacity_layout(
    layouts: &mut LayoutComputer<'_>,
    content: IrType,
) -> Result<(Layout, RuntimeCapacityAllocationLayout), TargetLayoutFailure> {
    let (element, header_words) = match content {
        IrType::Buffer { element } => (
            layouts
                .elements
                .get(element.index())
                .copied()
                .ok_or(TargetLayoutFailure::InvalidIr)?,
            1_u64,
        ),
        IrType::Window {
            shape,
            element,
            capacity: None,
        } => (
            layouts
                .elements
                .get(element.index())
                .copied()
                .ok_or(TargetLayoutFailure::InvalidIr)?,
            match shape {
                IrWindowShape::Slots => 2,
                IrWindowShape::Ring => 3,
            },
        ),
        _ => return Err(TargetLayoutFailure::InvalidIr),
    };
    let actual = layouts.layout(element)?;
    let stride = align_up(
        layouts.target,
        actual.size,
        actual.align,
        TargetObject::Representation,
    )?;
    if actual.align.max(8) > layouts.target.runtime_allocation_alignment() {
        return Err(TargetLayoutFailure::Unrepresentable(
            TargetObject::RuntimeSizedAllocation,
        ));
    }
    let fixed = header_words
        .checked_mul(8)
        .ok_or(TargetLayoutFailure::Unrepresentable(
            TargetObject::RuntimeSizedAllocation,
        ))?;
    let header = align_up(
        layouts.target,
        fixed,
        actual.align,
        TargetObject::RuntimeSizedAllocation,
    )?;
    Ok((actual, RuntimeCapacityAllocationLayout { stride, header }))
}

/// Attaches selected-target integer bounds to the exact SSA values that carry
/// them. Buffer lengths contribute the representation invariant established by
/// target validation. This metadata never becomes an ambient source fact.
fn target_integer_result_bounds(
    layouts: &mut LayoutComputer<'_>,
    function: &IrFunction,
) -> Result<HashMap<IrValueId, u64>, TargetLayoutFailure> {
    let mut bounds = HashMap::new();
    let u64_type = IrType::Integer {
        width: 64,
        signed: false,
    };
    for block in function.blocks() {
        for instruction in block.instructions() {
            let IrInstruction::Define {
                result,
                ty,
                operation,
            } = instruction
            else {
                continue;
            };
            let measured = match operation {
                IrOperation::BufferMeasure { buffer } => Some(*buffer),
                IrOperation::ContainerMeasure {
                    measure: crate::IrMeasure::Length,
                    container,
                } => Some(*container),
                _ => None,
            };
            let content = measured
                .and_then(|value| function.value_type(value))
                .map(|ty| {
                    if let IrType::Address(addressed) = ty {
                        addressed.ty()
                    } else {
                        ty
                    }
                });
            let upper_bound = match content {
                Some(content @ (IrType::Buffer { .. } | IrType::Window { capacity: None, .. })) => {
                    let (_, allocation) = runtime_capacity_layout(layouts, content)?;
                    let payload_max = layouts
                        .target
                        .runtime_allocation_max()
                        .checked_sub(allocation.header)
                        .ok_or(TargetLayoutFailure::Unrepresentable(
                            TargetObject::RuntimeSizedAllocation,
                        ))?;
                    Some(element_count_max(payload_max, allocation.stride))
                }
                _ => None,
            };
            let Some(upper_bound) = upper_bound else {
                continue;
            };
            if *ty != u64_type || function.value_type(*result) != Some(u64_type) {
                return Err(TargetLayoutFailure::InvalidIr);
            }
            if bounds.insert(*result, upper_bound).is_some() {
                return Err(TargetLayoutFailure::InvalidIr);
            }
        }
    }
    Ok(bounds)
}

const fn element_count_max(byte_maximum: u64, stride: u64) -> u64 {
    match byte_maximum.checked_div(stride) {
        Some(maximum) => maximum,
        None => u64::MAX,
    }
}

fn validate_target_obligation(
    layouts: &mut LayoutComputer<'_>,
    program: &IrProgram,
    function: &IrFunction,
    integer_upper_bounds: &HashMap<IrValueId, u64>,
    result_type: IrType,
    operation: &IrOperation,
) -> Result<(), TargetLayoutFailure> {
    match operation {
        IrOperation::BoxNew { nominal, value } => {
            if result_type != IrType::Nominal(*nominal) {
                return Err(TargetLayoutFailure::InvalidIr);
            }
            let referent = match program
                .nominal(*nominal)
                .ok_or(TargetLayoutFailure::InvalidIr)?
                .kind()
            {
                IrNominalKind::Box { referent, .. } => *referent,
                _ => return Err(TargetLayoutFailure::InvalidIr),
            };
            if function.value_type(*value) != Some(referent) {
                return Err(TargetLayoutFailure::InvalidIr);
            }
            let allocation = layouts
                .layout(referent)
                .map_err(|failure| as_object(failure, TargetObject::RuntimeSizedAllocation))?;
            if allocation.size > layouts.target.runtime_allocation_max()
                || allocation.align > layouts.target.runtime_allocation_alignment()
            {
                return Err(TargetLayoutFailure::Unrepresentable(
                    TargetObject::RuntimeSizedAllocation,
                ));
            }
        }
        IrOperation::ArrayFill { target_domain, .. }
            if *target_domain == IrTargetDomainObligation::ElementAddress => {}
        IrOperation::BufferFill {
            nominal,
            length,
            target_domains,
            layout_ceiling,
            ..
        } if target_domains.is_complete() => {
            if result_type != IrType::Nominal(*nominal) {
                return Err(TargetLayoutFailure::InvalidIr);
            }
            let IrNominalKind::Box { referent, .. } = program
                .nominal(*nominal)
                .ok_or(TargetLayoutFailure::InvalidIr)?
                .kind()
            else {
                return Err(TargetLayoutFailure::InvalidIr);
            };
            let allocation_layout =
                runtime_capacity_allocation_layout(layouts, *referent, *layout_ceiling)?;
            if function.value_type(*length)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
            {
                return Err(TargetLayoutFailure::InvalidIr);
            }
            // A direct allocation node retains its own call-site bound here.
            // A compiler-owned out-of-line row carries the language ceiling
            // instead. validate_function unconditionally invokes
            // validate_source_call_allocations, which requires an allocation
            // record for EVERY call of this row and checks its exact
            // bound * stride + header against the selected target. Missing
            // records are InvalidIr, never an exemption. Do not scale the
            // unrelated language maximum here: MAX/stride plus a header
            // can overflow even when every actual call allocates one byte.
            // The direct-node check and the per-call check both retain the
            // full header and fail on arithmetic overflow or an excess.
            if target_domains.has_call_site_bound() {
                let source_upper_bound = target_domains.source_length_upper_bound();
                let length_upper_bound = integer_upper_bounds
                    .get(length)
                    .copied()
                    .map_or(source_upper_bound, |target_upper_bound| {
                        source_upper_bound.min(target_upper_bound)
                    });
                let byte_upper_bound = length_upper_bound
                    .checked_mul(allocation_layout.stride)
                    .and_then(|slots| slots.checked_add(allocation_layout.header))
                    .ok_or(TargetLayoutFailure::Unrepresentable(
                        TargetObject::RuntimeSizedAllocation,
                    ))?;
                if byte_upper_bound > layouts.target.runtime_allocation_max() {
                    return Err(TargetLayoutFailure::Unrepresentable(
                        TargetObject::RuntimeSizedAllocation,
                    ));
                }
            }
        }
        // [OP-13, OP-10] the runtime-capacity window block and `grow`, on the
        // same terms as the fill above: the element's actual layout against
        // [OP-9]'s language ceilings, its alignment against what the one heap
        // can promise [STOR-8], and the retained bound scaled by the actual
        // stride [STOR-6] wherever the allocation site's own discharge is the
        // bound.
        IrOperation::WindowBlockNew {
            nominal,
            capacity: length,
            obligations,
        }
        | IrOperation::WindowGrow {
            nominal,
            capacity: length,
            obligations,
            ..
        } if obligations.target_domains.is_complete() => {
            let IrNominalKind::Box { referent, .. } = program
                .nominal(*nominal)
                .ok_or(TargetLayoutFailure::InvalidIr)?
                .kind()
            else {
                return Err(TargetLayoutFailure::InvalidIr);
            };
            let allocation_layout =
                runtime_capacity_allocation_layout(layouts, *referent, obligations.layout_ceiling)?;
            if function.value_type(*length)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
            {
                return Err(TargetLayoutFailure::InvalidIr);
            }
            // Direct allocation nodes retain their own call-site bound here;
            // the shared compiler-owned row is qualified at each source call
            // above and therefore does not reuse one caller's bound here.
            if obligations.target_domains.has_call_site_bound() {
                let source_upper_bound = obligations.target_domains.source_length_upper_bound();
                let length_upper_bound = integer_upper_bounds
                    .get(length)
                    .copied()
                    .map_or(source_upper_bound, |target_upper_bound| {
                        source_upper_bound.min(target_upper_bound)
                    });
                let byte_upper_bound = length_upper_bound
                    .checked_mul(allocation_layout.stride)
                    .and_then(|slots| slots.checked_add(allocation_layout.header))
                    .ok_or(TargetLayoutFailure::Unrepresentable(
                        TargetObject::RuntimeSizedAllocation,
                    ))?;
                if byte_upper_bound > layouts.target.runtime_allocation_max() {
                    return Err(TargetLayoutFailure::Unrepresentable(
                        TargetObject::RuntimeSizedAllocation,
                    ));
                }
            }
        }
        // [BLK-2] the run's own take from a store, validated on the same terms
        // the retiring fill was: the element's actual layout against [OP-9]'s
        // language ceilings, its alignment against what the storage can
        // promise, and the retained source bound scaled by the actual stride
        // [STOR-6].
        //
        // What it does *not* carry is the retiring row's byte ceiling against
        // the allocator parameter domain, and the difference is the refusal.
        // A take that the store cannot satisfy hands back `None`, which is an
        // arm of the source program [BLK-2], so an unproved runtime count is
        // an ordinary program rather than a target stop; every run that is
        // materialized at all satisfies the successful-allocation invariant
        // [STOR-6], and a bump take is bounded by its extent's own byte
        // constant. The scaling is still checked for representability, because
        // that is the joint fact [OP-9]'s obligation and this qualification
        // establish together and neither establishes alone.
        IrOperation::ArrayIndex {
            root,
            target_domain,
            ..
        } if *target_domain == IrTargetDomainObligation::ElementAddress => {
            let root_type = match root {
                IrArrayRoot::Value(value) => function
                    .value_type(*value)
                    .ok_or(TargetLayoutFailure::InvalidIr)?,
                IrArrayRoot::Constant(id) => program
                    .constant(*id)
                    .ok_or(TargetLayoutFailure::InvalidIr)?
                    .ty(),
            };
            layouts.layout(root_type)?;
        }
        IrOperation::BufferIndex { target_domain, .. }
        | IrOperation::SliceIndex { target_domain, .. }
        | IrOperation::SliceAddress { target_domain, .. }
            if *target_domain == IrTargetDomainObligation::ElementAddress => {}
        IrOperation::ArrayFill { .. }
        | IrOperation::BufferFill { .. }
        | IrOperation::ArrayIndex { .. }
        | IrOperation::BufferIndex { .. }
        | IrOperation::SliceIndex { .. }
        | IrOperation::SliceAddress { .. } => {
            return Err(TargetLayoutFailure::InvalidIr);
        }
        _ => {}
    }
    Ok(())
}

struct LayoutComputer<'types> {
    target: TargetLayout,
    nominals: &'types [IrNominal],
    elements: &'types [IrType],
    nominal: HashMap<IrNominalId, Layout>,
    visiting: HashSet<IrNominalId>,
    visiting_elements: HashSet<IrElement>,
}

impl<'types> LayoutComputer<'types> {
    fn new(
        target: TargetLayout,
        nominals: &'types [IrNominal],
        elements: &'types [IrType],
    ) -> Self {
        Self {
            target,
            nominals,
            elements,
            nominal: HashMap::new(),
            visiting: HashSet::new(),
            visiting_elements: HashSet::new(),
        }
    }

    fn storage_layout(&mut self, ty: &TargetStorageType) -> Result<Layout, TargetLayoutFailure> {
        match ty {
            TargetStorageType::Source(ty) => self.layout(*ty),
            TargetStorageType::Integer(1) | TargetStorageType::Integer(8) => {
                Ok(Layout { size: 1, align: 1 })
            }
            TargetStorageType::Integer(width) if matches!(width, 16 | 32 | 64) => {
                let bytes = u64::from(width / 8);
                Ok(Layout {
                    size: bytes,
                    align: bytes,
                })
            }
            TargetStorageType::Integer(_) => Err(TargetLayoutFailure::InvalidIr),
            TargetStorageType::Array { element, length } => {
                let element = self.storage_layout(element)?;
                let stride = align_up(
                    self.target,
                    element.size,
                    element.align,
                    TargetObject::StackFrame,
                )?;
                Ok(Layout {
                    size: checked_mul(stride, *length, self.target, TargetObject::StackFrame)?,
                    align: element.align,
                })
            }
        }
    }

    fn layout(&mut self, ty: IrType) -> Result<Layout, TargetLayoutFailure> {
        match ty {
            IrType::Unit | IrType::Bool => Ok(Layout { size: 1, align: 1 }),
            IrType::Integer { width, .. } if matches!(width, 8 | 16 | 32 | 64) => {
                let bytes = u64::from(width / 8);
                Ok(Layout {
                    size: bytes,
                    align: bytes,
                })
            }
            IrType::Integer { .. } => Err(TargetLayoutFailure::InvalidIr),
            IrType::Float { width } if matches!(width, 32 | 64) => {
                let bytes = u64::from(width / 8);
                Ok(Layout {
                    size: bytes,
                    align: bytes,
                })
            }
            IrType::Float { .. } => Err(TargetLayoutFailure::InvalidIr),
            IrType::Nominal(id) => self.nominal_layout(id),
            IrType::Address(_) | IrType::RuntimeBoxPayload { .. } => Ok(POINTER_LAYOUT),
            IrType::Array { length: 0, .. } => Ok(Layout { size: 0, align: 1 }),
            IrType::Array { element, length } => {
                let element = self.element(element)?;
                let stride = align_up(
                    self.target,
                    element.size,
                    element.align,
                    TargetObject::Representation,
                )?;
                let size = checked_mul(stride, length, self.target, TargetObject::Representation)?;
                Ok(Layout {
                    size,
                    align: element.align,
                })
            }
            // [TYPE-9] a runtime-capacity `Array<T>` block is reached only
            // through the `Box` that owns it, so it never occupies inline
            // storage; its own layout is the `len` word that heads it
            // (compiler/storage-representation).
            IrType::Buffer { element } => {
                let element = self.element(element)?;
                Ok(Layout {
                    size: 8,
                    align: element.align.max(8),
                })
            }
            IrType::Range { element } => {
                self.element(element)?;
                Ok(Layout { size: 16, align: 8 })
            }
            // [TYPE-9] a runtime-capacity block is reached only through the
            // `Box` that owns it, so it never occupies inline storage.
            IrType::Window { capacity: None, .. } => Ok(Layout { size: 8, align: 8 }),
            // compiler/storage-representation: header first, `len` always,
            // `head` only for a `Ring`, and no capacity word where the type
            // constant already fixes it.
            IrType::Window {
                shape,
                element,
                capacity: Some(0),
            } => {
                self.element(element)?;
                let header = if shape == IrWindowShape::Ring { 16 } else { 8 };
                Ok(Layout {
                    size: header,
                    align: 8,
                })
            }
            IrType::Window {
                shape,
                element,
                capacity: Some(length),
            } => {
                let element = self.element(element)?;
                let stride = align_up(
                    self.target,
                    element.size,
                    element.align,
                    TargetObject::Representation,
                )?;
                let slots = checked_mul(stride, length, self.target, TargetObject::Representation)?;
                let align = element.align.max(8);
                let header = if shape == IrWindowShape::Ring { 16 } else { 8 };
                let body = align_up(
                    self.target,
                    header,
                    element.align,
                    TargetObject::Representation,
                )?;
                let size = align_up(
                    self.target,
                    body.checked_add(slots)
                        .ok_or(TargetLayoutFailure::Unrepresentable(
                            TargetObject::Representation,
                        ))?,
                    align,
                    TargetObject::Representation,
                )?;
                Ok(Layout { size, align })
            }
        }
    }

    /// One run slot's layout [WIN-1, OP-9]. A slot holding a run holds that
    /// run's complete representation -- a `FixedVector`'s slots and two
    /// descriptor words inline, a `Vector`'s four-word descriptor -- so the
    /// slot layout is that type's own.
    fn element(&mut self, element: IrElement) -> Result<Layout, TargetLayoutFailure> {
        if !self.visiting_elements.insert(element) {
            return Err(TargetLayoutFailure::InvalidIr);
        }
        let ty = self
            .elements
            .get(element.index())
            .copied()
            .ok_or(TargetLayoutFailure::InvalidIr)?;
        let layout = self.layout(ty);
        self.visiting_elements.remove(&element);
        layout
    }

    fn nominal_layout(&mut self, id: IrNominalId) -> Result<Layout, TargetLayoutFailure> {
        if let Some(layout) = self.nominal.get(&id) {
            return Ok(*layout);
        }
        if !self.visiting.insert(id) {
            return Err(TargetLayoutFailure::InvalidIr);
        }
        let nominal = self
            .nominals
            .get(id.index())
            .ok_or(TargetLayoutFailure::InvalidIr)?;
        if matches!(nominal.kind(), IrNominalKind::Opaque) {
            let layout = Layout {
                size: 32,
                align: 16,
            };
            self.visiting.remove(&id);
            self.nominal.insert(id, layout);
            return Ok(layout);
        }
        let layout = if matches!(nominal.kind(), IrNominalKind::Box { .. }) {
            POINTER_LAYOUT
        } else if nominal.is_tag_only_enum() {
            let IrNominalKind::Enum { variants } = nominal.kind() else {
                return Err(TargetLayoutFailure::InvalidIr);
            };
            if variants.len() <= 2 {
                Layout { size: 1, align: 1 }
            } else {
                Layout { size: 4, align: 4 }
            }
        } else {
            let mut fields = Vec::new();
            match nominal.kind() {
                IrNominalKind::Struct {
                    fields: declarations,
                } => fields.extend(declarations.iter().map(|field| field.ty())),
                IrNominalKind::Enum { variants } => {
                    fields.push(IrType::Integer {
                        width: 32,
                        signed: false,
                    });
                    fields.extend(
                        variants
                            .iter()
                            .flat_map(|variant| variant.fields())
                            .map(|field| field.ty()),
                    );
                }
                // A box has its own pointer layout above, and an opaque
                // nominal returned with its uniform representation
                // before this match; none reaches the field walk.
                IrNominalKind::Box { .. } | IrNominalKind::Opaque => {
                    return Err(TargetLayoutFailure::InvalidIr);
                }
            }
            self.struct_layout(fields)?
        };
        self.visiting.remove(&id);
        self.nominal.insert(id, layout);
        Ok(layout)
    }

    fn struct_layout(&mut self, fields: Vec<IrType>) -> Result<Layout, TargetLayoutFailure> {
        let mut layouts = Vec::with_capacity(fields.len());
        for field in fields {
            layouts.push(self.layout(field)?);
        }
        self.aggregate_layout(layouts, TargetObject::Representation)
    }

    fn aggregate_layout(
        &self,
        fields: impl IntoIterator<Item = Layout>,
        object: TargetObject,
    ) -> Result<Layout, TargetLayoutFailure> {
        let mut size = 0_u64;
        let mut alignment = 1_u64;
        for field in fields {
            size = align_up(self.target, size, field.align, object)?;
            size = checked_add(size, field.size, self.target, object)?;
            alignment = alignment.max(field.align);
        }
        size = align_up(self.target, size, alignment, object)?;
        Ok(Layout {
            size,
            align: alignment,
        })
    }
}

fn checked_add(
    left: u64,
    right: u64,
    target: TargetLayout,
    object: TargetObject,
) -> Result<u64, TargetLayoutFailure> {
    let value = left
        .checked_add(right)
        .ok_or(TargetLayoutFailure::Unrepresentable(object))?;
    if value > target.address_index_max() {
        return Err(TargetLayoutFailure::Unrepresentable(object));
    }
    Ok(value)
}

fn checked_mul(
    left: u64,
    right: u64,
    target: TargetLayout,
    object: TargetObject,
) -> Result<u64, TargetLayoutFailure> {
    let value = left
        .checked_mul(right)
        .ok_or(TargetLayoutFailure::Unrepresentable(object))?;
    if value > target.address_index_max() {
        return Err(TargetLayoutFailure::Unrepresentable(object));
    }
    Ok(value)
}

fn align_up(
    target: TargetLayout,
    value: u64,
    alignment: u64,
    object: TargetObject,
) -> Result<u64, TargetLayoutFailure> {
    let mask = alignment
        .checked_sub(1)
        .ok_or(TargetLayoutFailure::InvalidIr)?;
    let aligned = value
        .checked_add(mask)
        .map(|sum| sum & !mask)
        .ok_or(TargetLayoutFailure::Unrepresentable(object))?;
    if aligned > target.address_index_max() {
        return Err(TargetLayoutFailure::Unrepresentable(object));
    }
    Ok(aligned)
}

fn as_object(failure: TargetLayoutFailure, object: TargetObject) -> TargetLayoutFailure {
    match failure {
        TargetLayoutFailure::Unrepresentable(_) => TargetLayoutFailure::Unrepresentable(object),
        other => other,
    }
}
