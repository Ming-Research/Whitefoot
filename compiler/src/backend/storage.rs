//! Storage for immutable aggregate IR values.
//!
//! Source ownership is already checked and is not inferred here. A value gets
//! independent backing unless complete CFG liveness proves that a selected
//! update, edge transfer or alternative return can reuse backing whose old
//! contents are dead. A returned group containing one owned entry parameter
//! may use the result after every other indirect input reaches private storage.
//! Loads and ordinary projections remain snapshots. A consumed call input and
//! its consumed struct result field may occupy the same field of the complete
//! result allocation after a separate interference check. Exposed backing is
//! not coalesced, and schedules whose reads can outlive an IR call keep every
//! value separate until their actual retirement lifetimes are represented.

use std::collections::BTreeSet;

use crate::{
    IrFunction, IrInstruction, IrNominalId, IrNominalKind, IrOperation, IrProgram,
    IrSourceArgument, IrSourceMode, IrTerminator, IrType, IrValueId,
};

use super::BackendFailure;

/// These values contain their payload inline. Descriptors retain their
/// ordinary SSA representation: their payload is elsewhere. This
/// choice depends on representation, not source names or a size threshold.
pub(super) fn is_stored_aggregate(program: &IrProgram, ty: IrType) -> Result<bool, BackendFailure> {
    Ok(match ty {
        IrType::Array { .. }
        | IrType::Window {
            capacity: Some(_), ..
        } => true,
        IrType::Nominal(nominal) => {
            let nominal = program.nominal(nominal).ok_or(BackendFailure::InvalidIr)?;
            match nominal.kind() {
                IrNominalKind::Struct { .. } | IrNominalKind::Opaque => true,
                IrNominalKind::Enum { .. } => !nominal.is_tag_only_enum(),
                IrNominalKind::Box { .. } => false,
            }
        }
        IrType::Unit
        | IrType::Bool
        | IrType::Integer { .. }
        | IrType::Float { .. }
        | IrType::Buffer { .. }
        | IrType::Window { capacity: None, .. }
        | IrType::Range { .. }
        | IrType::RuntimeBoxPayload { .. }
        | IrType::Address(_) => false,
    })
}

pub(super) struct FunctionStoragePlan {
    values: Vec<Option<usize>>,
    slots: Vec<IrType>,
    exposed: BTreeSet<usize>,
    /// A fresh binding can be the destination of its initializing value.
    /// The frame plan supplies this address's static or per-iteration backing.
    destinations: Vec<Option<IrValueId>>,
    fields: Vec<Option<FieldDestination>>,
}

/// A logical slot occupies one field of a complete local struct allocation.
/// Logical slot identity and type are retained; this is not a value alias.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FieldDestination {
    pub(super) parent_slot: usize,
    pub(super) nominal: IrNominalId,
    pub(super) field: u32,
}

/// Only these two instruction sites have an established overlapping transfer.
/// Other occurrences of the same values still contribute normal interference.
struct FieldReuse {
    call: usize,
    argument: usize,
    projection: usize,
}

impl FunctionStoragePlan {
    #[cfg(test)]
    pub(super) fn build(
        program: &IrProgram,
        function: &IrFunction,
    ) -> Result<Self, BackendFailure> {
        Self::build_in_world(program, function, false)
    }

    /// Storage interference follows the world being emitted. A sequential
    /// clone has no deferred hand-out operands, including in its callees.
    pub(super) fn build_in_world(
        program: &IrProgram,
        function: &IrFunction,
        sequential: bool,
    ) -> Result<Self, BackendFailure> {
        // A signature has the same parameter/result ABI as a definition but
        // no activation or control-flow graph to allocate storage for.
        if function.blocks().is_empty() {
            return Ok(Self {
                values: vec![None; function.value_types().len()],
                slots: Vec::new(),
                exposed: BTreeSet::new(),
                destinations: Vec::new(),
                fields: Vec::new(),
            });
        }
        let graph = FlowGraph::from_function(program, function, sequential)?;
        // Capture pruning preserves value IDs and their type metadata. Only
        // definitions still present in the graph need backing; ordinary unused
        // definitions keep their storage just as used definitions do.
        let mut types = vec![None; function.value_types().len()];
        for value in graph.definitions() {
            let ty = *function
                .value_types()
                .get(value)
                .ok_or(BackendFailure::InvalidIr)?;
            types[value] = is_stored_aggregate(program, ty)?.then_some(ty);
        }
        let returned: Vec<_> = function
            .blocks()
            .iter()
            .filter_map(|block| match block.terminator() {
                IrTerminator::Return { value, .. } => Some(index(*value)),
                _ => None,
            })
            .collect();
        let mut plan = graph.plan(types, &returned)?;
        plan.select_destinations(function, &graph)?;
        plan.select_field_destinations(program, function, &graph, sequential)?;
        Ok(plan)
    }

    pub(super) fn slot(&self, value: IrValueId) -> Option<usize> {
        self.values.get(value.ordinal() as usize).copied().flatten()
    }

    /// The slot number is the index in this slice. Ordering follows the first
    /// value in each group and does not depend on hashing or traversal timing.
    pub(super) fn slots(&self) -> &[IrType] {
        &self.slots
    }

    pub(super) fn destination(&self, slot: usize) -> Option<IrValueId> {
        self.destinations.get(slot).copied().flatten()
    }

    pub(super) fn is_exposed(&self, slot: usize) -> bool {
        self.exposed.contains(&slot)
    }

    pub(super) fn field_destination(&self, slot: usize) -> Option<FieldDestination> {
        self.fields.get(slot).copied().flatten()
    }

    /// Field placements have depth one and never redirect an existing binding
    /// destination. The complete allocation, not its child, owns the frame slot.
    pub(super) fn allocation_root(&self, slot: usize) -> usize {
        self.field_destination(slot)
            .map_or(slot, |field| field.parent_slot)
    }

    fn select_field_destinations(
        &mut self,
        program: &IrProgram,
        function: &IrFunction,
        graph: &FlowGraph,
        sequential: bool,
    ) -> Result<(), BackendFailure> {
        if !graph.coalesce {
            return Ok(());
        }
        let types: Vec<_> = self
            .values
            .iter()
            .map(|slot| slot.map(|slot| self.slots[slot]))
            .collect();
        let mut members = vec![Vec::new(); self.slots.len()];
        for (value, slot) in self.values.iter().enumerate() {
            if let Some(slot) = slot {
                members[*slot].push(value);
            }
        }
        // Each selected allocation component is independent. In particular a
        // later candidate cannot place an existing parent inside its own child.
        let mut placed = BTreeSet::new();
        for (block_index, block) in function.blocks().iter().enumerate() {
            for (call_index, instruction) in block.instructions().iter().enumerate() {
                let IrInstruction::Define {
                    result: call,
                    ty: IrType::Nominal(nominal),
                    operation: operation @ IrOperation::Call { .. },
                } = instruction
                else {
                    continue;
                };
                let Some(parent) = self.slot(*call) else {
                    continue;
                };
                if members[parent].as_slice() != [index(*call)]
                    || placed.contains(&parent)
                    || self.destination(parent).is_some()
                    || self.is_exposed(parent)
                {
                    continue;
                }
                let IrNominalKind::Struct { fields } = program
                    .nominal(*nominal)
                    .ok_or(BackendFailure::InvalidIr)?
                    .kind()
                else {
                    continue;
                };
                let mut projections = Vec::new();
                let mut seen_fields = BTreeSet::new();
                let mut admitted = true;
                // The parent is only read by distinct, consuming field
                // projections after this call, in this same dynamic block.
                // Whole-parent reads, exposure, drops and CFG transports keep
                // the original allocation rather than guessing field liveness.
                for (other_block, flow) in graph.blocks.iter().enumerate() {
                    if flow.terminal_uses.contains(&index(*call)) {
                        admitted = false;
                    }
                    for (other_index, flow_instruction) in flow.instructions.iter().enumerate() {
                        if !flow_instruction.operands.contains(&index(*call)) {
                            continue;
                        }
                        let IrInstruction::Define {
                            result,
                            ty,
                            operation:
                                IrOperation::ProjectStruct {
                                    aggregate,
                                    nominal: source,
                                    field,
                                    consume_root: true,
                                },
                        } = &function.blocks()[other_block].instructions()[other_index]
                        else {
                            admitted = false;
                            continue;
                        };
                        if other_block != block_index
                            || other_index <= call_index
                            || aggregate != call
                            || source != nominal
                            || !seen_fields.insert(*field)
                            || fields.get(*field as usize).map(|field| field.ty()) != Some(*ty)
                        {
                            admitted = false;
                            continue;
                        }
                        projections.push((*result, *ty, *field));
                    }
                }
                if !admitted {
                    continue;
                }
                for (projection, ty, field) in projections {
                    let Some(child) = self.slot(projection) else {
                        continue;
                    };
                    let Some(argument) = call_reuse_operand_for_type(
                        program, function, *call, operation, ty, sequential,
                    )?
                    else {
                        continue;
                    };
                    let input = self.slot(argument).ok_or(BackendFailure::InvalidIr)?;
                    let children = BTreeSet::from([input, child]);
                    if children.contains(&parent)
                        || children.iter().any(|slot| {
                            placed.contains(slot)
                                || self.destination(*slot).is_some()
                                || self.is_exposed(*slot)
                        })
                    {
                        continue;
                    }
                    let reuse = FieldReuse {
                        call: index(*call),
                        argument: index(argument),
                        projection: index(projection),
                    };
                    let (conflicts, _) = graph.interference_with_field_reuse(&types, Some(&reuse));
                    let values: Vec<_> = children
                        .iter()
                        .chain(std::iter::once(&parent))
                        .flat_map(|slot| members[*slot].iter().copied())
                        .collect();
                    if values
                        .iter()
                        .any(|left| values.iter().any(|right| conflicts[*left].contains(right)))
                    {
                        continue;
                    }
                    let destination = FieldDestination {
                        parent_slot: parent,
                        nominal: *nominal,
                        field,
                    };
                    for child in children {
                        self.fields[child] = Some(destination);
                        placed.insert(child);
                    }
                    placed.insert(parent);
                    break;
                }
            }
        }
        // This also protects later address resolution from cycles and from a
        // child being backed by less storage than its complete parent requires.
        for (child, field) in self.fields.iter().enumerate() {
            if let Some(field) = field
                && (field.parent_slot == child
                    || self.field_destination(field.parent_slot).is_some()
                    || self.destination(child).is_some()
                    || self.destination(field.parent_slot).is_some()
                    || self.slots[field.parent_slot] != IrType::Nominal(field.nominal))
            {
                return Err(BackendFailure::InvalidIr);
            }
        }
        Ok(())
    }

    /// Redirect a value's construction into the fresh place that consumes it.
    /// This changes physical placement, not source ownership or initialization:
    /// no old content is overwritten. Input/result backing reuse is selected
    /// separately by `call_reuse_operand` and checked CFG liveness.
    ///
    /// The single-use condition preserves independent snapshots and exposed
    /// views. Both definitions must execute in the same block, hence the same
    /// dynamic iteration. A static destination must be in an acyclic block.
    /// Single-use alone cannot exclude a
    /// previous iteration's address alias during the producer's reads.
    /// AddressOf already freezes the value's storage group;
    /// no other definition can subsequently reuse it.
    fn select_destinations(
        &mut self,
        function: &IrFunction,
        graph: &FlowGraph,
    ) -> Result<(), BackendFailure> {
        let mut uses = vec![0_u8; self.values.len()];
        for block in &graph.blocks {
            for value in block
                .instructions
                .iter()
                .flat_map(|instruction| &instruction.operands)
                .chain(&block.terminal_uses)
            {
                uses[*value] = uses[*value].saturating_add(1);
            }
        }
        let mut members = vec![0_usize; self.slots.len()];
        for slot in self.values.iter().flatten() {
            members[*slot] += 1;
        }
        for (block_index, block) in function.blocks().iter().enumerate() {
            if graph.reentered(block_index) {
                continue;
            }
            let mut defined = BTreeSet::new();
            for instruction in block.instructions() {
                let IrInstruction::Define {
                    result,
                    ty,
                    operation,
                } = instruction
                else {
                    continue;
                };
                if let IrOperation::AddressOf { value, referent } = operation
                    && let Some(slot) = self.slot(*value)
                    && members[slot] == 1
                    && uses[index(*value)] == 1
                    && defined.contains(value)
                {
                    if *ty != IrType::Address(*referent) || self.slots[slot] != referent.ty() {
                        return Err(BackendFailure::InvalidIr);
                    }
                    self.destinations[slot] = Some(*result);
                }
                defined.insert(*result);
            }
        }
        Ok(())
    }
}

/// The emitter must read all incoming edge values before writing any aggregate
/// block parameter. Slot assignment does not turn parallel phi transfer into
/// sequential assignment; in particular two loop carries may exchange slots.
struct FlowGraph {
    entry_parameters: Vec<usize>,
    blocks: Vec<FlowBlock>,
    coalesce: bool,
}

struct FlowBlock {
    parameters: Vec<usize>,
    instructions: Vec<FlowInstruction>,
    terminal_uses: Vec<usize>,
    successors: Vec<usize>,
    transfers: Vec<(usize, usize)>,
}

struct FlowInstruction {
    result: Option<usize>,
    operands: Vec<usize>,
    /// Only these operations permit their result to overwrite this one input
    /// after every operand has been read. This is not an ownership annotation.
    reuse: Option<usize>,
    exposed: Option<usize>,
}

impl FlowGraph {
    fn definitions(&self) -> impl Iterator<Item = usize> + '_ {
        self.entry_parameters
            .iter()
            .copied()
            .chain(self.blocks.iter().flat_map(|block| {
                block.parameters.iter().copied().chain(
                    block
                        .instructions
                        .iter()
                        .filter_map(|instruction| instruction.result),
                )
            }))
    }

    /// Whether a later CFG visit can overwrite this block's static backing.
    /// No source ownership inference is needed for an acyclic initialization.
    fn reentered(&self, block: usize) -> bool {
        let mut pending = self.blocks[block].successors.clone();
        let mut seen = BTreeSet::new();
        while let Some(next) = pending.pop() {
            if next == block {
                return true;
            }
            if seen.insert(next) {
                pending.extend(self.blocks[next].successors.iter().copied());
            }
        }
        false
    }

    fn from_function(
        program: &IrProgram,
        function: &IrFunction,
        sequential: bool,
    ) -> Result<Self, BackendFailure> {
        let blocks = function
            .blocks()
            .iter()
            .map(|block| {
                let (successors, transfers) = match block.terminator() {
                    IrTerminator::Jump {
                        target, arguments, ..
                    } => (
                        vec![target.index()],
                        function
                            .blocks()
                            .get(target.index())
                            .map(|block| {
                                block
                                    .parameters()
                                    .iter()
                                    .zip(arguments)
                                    .map(|((parameter, _), argument)| {
                                        (index(*parameter), index(*argument))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    ),
                    IrTerminator::Match { targets, .. } => (
                        targets
                            .iter()
                            .map(|target| target.block().index())
                            .collect(),
                        Vec::new(),
                    ),
                    IrTerminator::Return { .. } | IrTerminator::Unreachable => {
                        (Vec::new(), Vec::new())
                    }
                };
                Ok(FlowBlock {
                    parameters: block
                        .parameters()
                        .iter()
                        .map(|(value, _)| index(*value))
                        .collect(),
                    instructions: block
                        .instructions()
                        .iter()
                        .map(|instruction| {
                            FlowInstruction::from_ir(program, function, instruction, sequential)
                        })
                        .collect::<Result<Vec<_>, BackendFailure>>()?,
                    terminal_uses: block
                        .terminator()
                        .operands()
                        .into_iter()
                        .map(index)
                        .collect(),
                    successors,
                    transfers,
                })
            })
            .collect::<Result<Vec<_>, BackendFailure>>()?;
        Ok(Self {
            entry_parameters: function
                .parameters()
                .iter()
                .map(|(value, _)| index(*value))
                .collect(),
            blocks,
            // A refused ordinary hand-out reads its arguments at join.
            coalesce: sequential || function.overlaps().is_empty(),
        })
    }

    fn plan(
        &self,
        types: Vec<Option<IrType>>,
        returned: &[usize],
    ) -> Result<FunctionStoragePlan, BackendFailure> {
        self.validate(types.len())?;
        if returned.iter().any(|value| *value >= types.len()) {
            return Err(BackendFailure::InvalidIr);
        }
        let mut groups: Vec<BTreeSet<usize>> = (0..types.len())
            .map(|value| BTreeSet::from([value]))
            .collect();
        let mut representative: Vec<usize> = (0..types.len()).collect();
        if self.coalesce {
            let (interference, frozen) = self.interference(&types);
            // Update and CFG transfer neighbors can reuse backing. Returned
            // values can also share their caller's destination, but returning
            // on different edges does not by itself prove their storage dead:
            // all definitions, reads, drops and exposed addresses still take
            // part in the same interference check. If every returned group
            // joins, the emitter can omit its frame slot and return copy.
            // A group containing an entry parameter additionally needs the
            // prologue's private-input-before-result ordering. Its input
            // transfer remains; no caller input/result equality is assumed.
            let candidates = self
                .blocks
                .iter()
                .flat_map(|block| {
                    block
                        .instructions
                        .iter()
                        .filter_map(|instruction| Some((instruction.result?, instruction.reuse?)))
                        .chain(block.transfers.iter().copied())
                })
                .chain(
                    returned.first().into_iter().flat_map(|first| {
                        returned.iter().skip(1).map(move |other| (*first, *other))
                    }),
                );
            for (left, right) in candidates {
                if types[left].is_none() || types[left] != types[right] {
                    continue;
                }
                let a = representative[left];
                let b = representative[right];
                if a == b
                    || groups[a]
                        .iter()
                        .chain(&groups[b])
                        .any(|value| frozen.contains(value))
                    || groups[a].iter().any(|left| {
                        groups[b]
                            .iter()
                            .any(|right| interference[*left].contains(right))
                    })
                {
                    continue;
                }
                let (keep, remove) = (a.min(b), a.max(b));
                for member in std::mem::take(&mut groups[remove]) {
                    representative[member] = keep;
                    groups[keep].insert(member);
                }
            }
        }
        let mut values = vec![None; types.len()];
        let mut slots = Vec::new();
        let mut assigned = vec![None; types.len()];
        for (value, ty) in types.into_iter().enumerate() {
            if let Some(ty) = ty {
                let slot = *assigned[representative[value]].get_or_insert_with(|| {
                    slots.push(ty);
                    slots.len() - 1
                });
                values[value] = Some(slot);
            }
        }
        let destinations = vec![None; slots.len()];
        let fields = vec![None; slots.len()];
        let exposed = self
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| instruction.exposed)
            .filter_map(|value| values[value])
            .collect();
        Ok(FunctionStoragePlan {
            values,
            slots,
            exposed,
            destinations,
            fields,
        })
    }

    fn validate(&self, values: usize) -> Result<(), BackendFailure> {
        if self.blocks.is_empty()
            || self.entry_parameters.iter().any(|value| *value >= values)
            || self.blocks.iter().any(|block| {
                block
                    .parameters
                    .iter()
                    .chain(&block.terminal_uses)
                    .any(|value| *value >= values)
                    || block
                        .successors
                        .iter()
                        .any(|target| *target >= self.blocks.len())
                    || block
                        .transfers
                        .iter()
                        .any(|(a, b)| *a >= values || *b >= values)
                    || block.instructions.iter().any(|instruction| {
                        instruction.result.is_some_and(|value| value >= values)
                            || instruction.operands.iter().any(|value| *value >= values)
                            || instruction.reuse.is_some_and(|value| value >= values)
                            || instruction.exposed.is_some_and(|value| value >= values)
                    })
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        Ok(())
    }

    fn live_in(&self) -> Vec<BTreeSet<usize>> {
        let mut entering = vec![BTreeSet::new(); self.blocks.len()];
        loop {
            let mut changed = false;
            for (ordinal, block) in self.blocks.iter().enumerate().rev() {
                let mut live = self.live_after(block, &entering);
                for instruction in block.instructions.iter().rev() {
                    if let Some(result) = instruction.result {
                        live.remove(&result);
                    }
                    live.extend(instruction.operands.iter().copied());
                }
                for parameter in &block.parameters {
                    live.remove(parameter);
                }
                // Function parameters are defined in the activation's
                // prologue, not by executing block zero. Keep their uses in
                // this fixed point: an edge back to block zero must preserve
                // an original parameter across the previous iteration.
                if entering[ordinal] != live {
                    entering[ordinal] = live;
                    changed = true;
                }
            }
            if !changed {
                return entering;
            }
        }
    }

    fn live_after(&self, block: &FlowBlock, entering: &[BTreeSet<usize>]) -> BTreeSet<usize> {
        let mut live: BTreeSet<_> = block.terminal_uses.iter().copied().collect();
        for successor in &block.successors {
            live.extend(entering[*successor].iter().copied());
        }
        live
    }

    fn interference(&self, types: &[Option<IrType>]) -> (Vec<BTreeSet<usize>>, BTreeSet<usize>) {
        self.interference_with_field_reuse(types, None)
    }

    fn interference_with_field_reuse(
        &self,
        types: &[Option<IrType>],
        reuse: Option<&FieldReuse>,
    ) -> (Vec<BTreeSet<usize>>, BTreeSet<usize>) {
        let entering = self.live_in();
        let mut conflicts = vec![BTreeSet::new(); types.len()];
        let mut frozen = BTreeSet::new();
        let mut conflict = |a: usize, b: usize| {
            if a != b && types[a].is_some() && types[b].is_some() {
                conflicts[a].insert(b);
                conflicts[b].insert(a);
            }
        };
        for (ordinal, block) in self.blocks.iter().enumerate() {
            let mut live = self.live_after(block, &entering);
            for instruction in block.instructions.iter().rev() {
                if let Some(exposed) = instruction.exposed {
                    frozen.insert(exposed);
                }
                if let Some(result) = instruction.result {
                    // Even an unused definition writes its assigned backing.
                    for other in &live {
                        if !reuse
                            .is_some_and(|reuse| result == reuse.projection && *other == reuse.call)
                        {
                            conflict(result, *other);
                        }
                    }
                    // An arbitrary result destination may be written before
                    // a call/constructor has finished reading its operands.
                    // Only a selected update has an explicit read-then-write
                    // contract permitting its base operand's reuse.
                    for operand in &instruction.operands {
                        let field_transfer = reuse.is_some_and(|reuse| {
                            (result == reuse.call && *operand == reuse.argument)
                                || (result == reuse.projection && *operand == reuse.call)
                        });
                        if Some(*operand) != instruction.reuse && !field_transfer {
                            conflict(result, *operand);
                        }
                    }
                    live.remove(&result);
                }
                live.extend(instruction.operands.iter().copied());
            }
            let parameters: Vec<_> = block
                .parameters
                .iter()
                .chain(self.entry_parameters.iter().filter(|_| ordinal == 0))
                .copied()
                .collect();
            // Phi and function parameter stores happen even for unused
            // parameters. Keep all destinations distinct, and protect every
            // value live across those writes, including outer loop values.
            for parameter in &parameters {
                for other in live.iter().chain(&parameters) {
                    conflict(*parameter, *other);
                }
            }
        }
        (conflicts, frozen)
    }
}

/// Selects the one checked owned binding whose dead backing may receive this
/// ordinary call's whole result. Stored parameters are snapshotted in the
/// callee prologue before any body or result write, so making its result
/// destination equal this one input address preserves argument evaluation.
/// Calls which can leave the current synchronous extent keep distinct storage.
fn call_reuse_operand(
    program: &IrProgram,
    caller: &IrFunction,
    result: IrValueId,
    operation: &IrOperation,
    sequential: bool,
) -> Result<Option<IrValueId>, BackendFailure> {
    let ty = caller.value_type(result).ok_or(BackendFailure::InvalidIr)?;
    call_reuse_operand_for_type(program, caller, result, operation, ty, sequential)
}

fn call_reuse_operand_for_type(
    program: &IrProgram,
    caller: &IrFunction,
    result: IrValueId,
    operation: &IrOperation,
    input_type: IrType,
    sequential: bool,
) -> Result<Option<IrValueId>, BackendFailure> {
    let IrOperation::Call {
        function,
        arguments,
    } = operation
    else {
        return Err(BackendFailure::InvalidIr);
    };
    if !sequential && !caller.overlaps().is_empty() {
        return Ok(None);
    }
    let mut source_calls = caller
        .source_calls()
        .iter()
        .filter(|call| call.result() == result);
    let Some(source_call) = source_calls.next() else {
        return Ok(None);
    };
    if source_calls.next().is_some() || source_call.arguments().len() != arguments.len() {
        return Err(BackendFailure::InvalidIr);
    }
    let callee = program
        .functions()
        .get(*function as usize)
        .ok_or(BackendFailure::InvalidIr)?;
    let Some(signature) = callee.source_signature() else {
        return Ok(None);
    };
    if (!sequential && !callee.overlaps().is_empty())
        || signature.result() != IrSourceMode::Own
        || signature.parameters().len() != arguments.len()
        || callee.result() != caller.value_type(result).ok_or(BackendFailure::InvalidIr)?
        || !is_stored_aggregate(program, callee.result())?
    {
        return Ok(None);
    }
    let mut candidate = None;
    for ((argument, source), mode) in arguments
        .iter()
        .zip(source_call.arguments())
        .zip(signature.parameters())
    {
        let consumes_owned_binding =
            matches!(source, IrSourceArgument::Binding { consume_root: true })
                && *mode == IrSourceMode::Own
                && caller.value_type(*argument) == Some(input_type);
        if consumes_owned_binding {
            if candidate.is_some() {
                return Ok(None);
            }
            candidate = Some(*argument);
        }
    }
    Ok(candidate)
}

impl FlowInstruction {
    fn from_ir(
        program: &IrProgram,
        function: &IrFunction,
        instruction: &IrInstruction,
        sequential: bool,
    ) -> Result<Self, BackendFailure> {
        let (result, reuse, exposed) = match instruction {
            IrInstruction::Define {
                result, operation, ..
            } => {
                let reuse = match operation {
                    IrOperation::InsertStruct { aggregate, .. } => Some(index(*aggregate)),
                    IrOperation::Call { .. } => {
                        call_reuse_operand(program, function, *result, operation, sequential)?
                            .map(index)
                    }
                    _ => None,
                };
                let exposed = match operation {
                    IrOperation::AddressOf { value, .. } => Some(index(*value)),
                    IrOperation::SliceFromRun { run } => Some(index(*run)),
                    _ => None,
                };
                (Some(index(*result)), reuse, exposed)
            }
            IrInstruction::StoreSlice { .. }
            | IrInstruction::Store { .. }
            | IrInstruction::Drops(_) => (None, None, None),
        };
        Ok(Self {
            result,
            operands: instruction.operands().into_iter().map(index).collect(),
            reuse,
            exposed,
        })
    }
}

fn index(value: IrValueId) -> usize {
    value.ordinal() as usize
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]

    use super::*;

    const AGGREGATE: IrType = IrType::Buffer {
        element: crate::IrElement(0),
    };

    fn define(result: usize, operands: &[usize], reuse: Option<usize>) -> FlowInstruction {
        FlowInstruction {
            result: Some(result),
            operands: operands.to_vec(),
            reuse,
            exposed: None,
        }
    }

    fn block(
        parameters: &[usize],
        instructions: Vec<FlowInstruction>,
        uses: &[usize],
    ) -> FlowBlock {
        FlowBlock {
            parameters: parameters.to_vec(),
            instructions,
            terminal_uses: uses.to_vec(),
            successors: Vec::new(),
            transfers: Vec::new(),
        }
    }

    fn plan(graph: &FlowGraph, values: usize) -> FunctionStoragePlan {
        plan_returning(graph, values, &[])
    }

    fn plan_returning(graph: &FlowGraph, values: usize, returned: &[usize]) -> FunctionStoragePlan {
        let types = vec![Some(AGGREGATE); values];
        let (conflicts, _) = graph.interference(&types);
        let plan = graph.plan(types, returned).expect("well-formed flow graph");
        for (value, others) in conflicts.iter().enumerate() {
            for other in others {
                assert_ne!(
                    plan.values[value], plan.values[*other],
                    "interfering values {value} and {other}"
                );
            }
        }
        compare_execution(graph, &plan);
        plan
    }

    #[test]
    fn field_transfer_exceptions_are_local_and_preserve_frozen_values() {
        let reuse = FieldReuse {
            call: 1,
            argument: 0,
            projection: 2,
        };
        let types = vec![Some(AGGREGATE); 5];
        let mut graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![block(
                &[],
                vec![
                    define(1, &[0], None),
                    define(2, &[1], None),
                    define(4, &[], None),
                    define(3, &[1], None),
                ],
                &[2, 3],
            )],
            coalesce: true,
        };
        let (ordinary, _) = graph.interference(&types);
        assert!(ordinary[1].contains(&0));
        assert!(ordinary[1].contains(&2));
        let (selected, _) = graph.interference_with_field_reuse(&types, Some(&reuse));
        assert!(!selected[1].contains(&0));
        assert!(!selected[1].contains(&2));
        assert!(selected[1].contains(&3), "other projection still writes");
        assert!(selected[1].contains(&4), "other definition still writes");

        graph.blocks[0].terminal_uses.push(0);
        graph.blocks[0].instructions[0].exposed = Some(0);
        let (selected, frozen) = graph.interference_with_field_reuse(&types, Some(&reuse));
        assert!(selected[1].contains(&0), "live input is not a dead operand");
        assert_eq!(frozen, BTreeSet::from([0]));
    }

    #[test]
    fn a_nonzero_result_field_can_back_an_owned_parameter_and_returned_child() {
        with_program(
            br#"nocopy struct Row {
  left: u64;
  right: u64;
}

fn split(value: Row) -> (observed: u64, updated: Row) pure {
  let observed = value.left;
  return observed, move value;
}

fn relay(value: Row) -> result: Row pure {
  let (observed, updated) = split(value: move value);
  return move updated;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = Row(left: 3_u64, right: 5_u64);
  let result = relay(value: move value);
  if result.right != 5_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            |program| {
                let relay = program
                    .functions()
                    .iter()
                    .find(|function| function.name() == "relay")
                    .expect("relay");
                let plan = FunctionStoragePlan::build(program, relay).expect("plan");
                let parameter = plan.slot(relay.parameters()[0].0).expect("owned input");
                let returned = relay
                    .blocks()
                    .iter()
                    .find_map(|block| match block.terminator() {
                        IrTerminator::Return { value, .. } => plan.slot(*value),
                        _ => None,
                    })
                    .expect("owned result");
                let field = plan.field_destination(parameter).expect("input field");
                assert_eq!(field.field, 1);
                assert_eq!(plan.field_destination(returned), Some(field));
                assert_eq!(plan.allocation_root(parameter), field.parent_slot);
                assert_eq!(plan.allocation_root(returned), field.parent_slot);
                assert_eq!(plan.field_destination(field.parent_slot), None);
                assert_eq!(plan.destination(parameter), None);
                assert_eq!(plan.destination(returned), None);
                assert_eq!(
                    super::super::emitter::places::returned_storage_slot(relay, &plan),
                    None,
                    "the complete parent cannot use the smaller caller result buffer"
                );
            },
        );
    }

    #[test]
    fn exclusive_returns_construct_in_one_destination() {
        for successors in [[1, 2], [2, 1]] {
            let mut entry = block(&[], Vec::new(), &[]);
            entry.successors.extend(successors);
            let graph = FlowGraph {
                entry_parameters: Vec::new(),
                blocks: vec![
                    entry,
                    block(&[], vec![define(0, &[], None)], &[0]),
                    block(&[], vec![define(1, &[], None)], &[1]),
                ],
                coalesce: true,
            };
            let plan = plan_returning(&graph, 2, &[0, 1]);
            assert_eq!(plan.slots.len(), 1);
        }
    }

    #[test]
    fn an_alternative_return_cannot_overwrite_a_value_read_in_a_successor() {
        let mut entry = block(&[], vec![define(0, &[], None)], &[]);
        entry.successors.extend([2, 1]);
        let mut alternative = block(&[], vec![define(1, &[], None)], &[]);
        alternative.successors.push(3);
        let graph = FlowGraph {
            entry_parameters: Vec::new(),
            blocks: vec![
                entry,
                block(&[], Vec::new(), &[0]),
                alternative,
                block(&[], Vec::new(), &[0, 1]),
            ],
            coalesce: true,
        };
        let plan = plan_returning(&graph, 2, &[0, 1]);
        assert_ne!(plan.values[0], plan.values[1]);
    }

    #[test]
    fn an_exposed_return_keeps_its_backing_after_its_last_direct_read() {
        let mut expose = define(1, &[0], None);
        expose.exposed = Some(0);
        let mut entry = block(&[], vec![define(0, &[], None), expose], &[]);
        entry.successors.extend([2, 1]);
        let graph = FlowGraph {
            entry_parameters: Vec::new(),
            blocks: vec![
                entry,
                block(&[], Vec::new(), &[0]),
                block(&[], vec![define(2, &[], None)], &[1, 2]),
            ],
            coalesce: true,
        };
        let plan = plan_returning(&graph, 3, &[0, 2]);
        assert_ne!(plan.values[0], plan.values[2]);
    }

    #[test]
    fn an_alternative_return_preserves_values_read_after_a_backedge() {
        let mut entry = block(&[], Vec::new(), &[]);
        entry.successors.extend([2, 1]);
        let mut body = block(&[], vec![define(1, &[0], None), define(2, &[], None)], &[]);
        body.successors.extend([2, 3]);
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![
                entry,
                block(&[], Vec::new(), &[0]),
                body,
                block(&[], Vec::new(), &[2]),
            ],
            coalesce: true,
        };
        let plan = plan_returning(&graph, 3, &[0, 2]);
        assert_ne!(plan.values[0], plan.values[2]);
    }

    #[test]
    fn deferred_return_values_keep_distinct_destinations() {
        let mut entry = block(&[], Vec::new(), &[]);
        entry.successors.extend([1, 2]);
        let graph = FlowGraph {
            entry_parameters: Vec::new(),
            blocks: vec![
                entry,
                block(&[], vec![define(0, &[], None)], &[0]),
                block(&[], vec![define(1, &[], None)], &[1]),
            ],
            coalesce: false,
        };
        let plan = plan_returning(&graph, 2, &[0, 1]);
        assert_ne!(plan.values[0], plan.values[1]);
    }

    /// A separate concrete oracle executes value snapshots and physical
    /// slots together. It does not read liveness or interference. Definitions
    /// write even when dead, every operand is checked, and loop transfers are
    /// simultaneous. Eight trips exercise reuse across dynamic definitions;
    /// this finite test trace is unrelated to source proof acceptance.
    fn compare_execution(graph: &FlowGraph, plan: &FunctionStoragePlan) {
        let mut values = vec![None; plan.values.len()];
        let mut storage = vec![None; plan.slots.len()];
        for parameter in &graph.entry_parameters {
            let value = 100 + *parameter as u64;
            values[*parameter] = Some(value);
            storage[plan.values[*parameter].expect("aggregate parameter")] = Some(value);
        }
        let read = |value: usize, values: &[Option<u64>], storage: &[Option<u64>]| {
            let expected = values[value].expect("read defined value");
            let slot = plan.values[value].expect("aggregate operand");
            assert_eq!(
                storage[slot],
                Some(expected),
                "value {value} changed in slot {slot}"
            );
            expected
        };
        let mut at = 0;
        let mut previous: Option<usize> = None;
        for _ in 0..graph.blocks.len() * 8 {
            let block = &graph.blocks[at];
            if let Some(previous) = previous {
                let transfers: Vec<_> = graph.blocks[previous]
                    .transfers
                    .iter()
                    .map(|(destination, source)| (*destination, read(*source, &values, &storage)))
                    .collect();
                for (destination, value) in transfers {
                    values[destination] = Some(value);
                    storage[plan.values[destination].expect("aggregate phi")] = Some(value);
                }
            }
            for instruction in &block.instructions {
                let answer = instruction.operands.iter().fold(1_u64, |answer, operand| {
                    answer
                        .wrapping_mul(31)
                        .wrapping_add(read(*operand, &values, &storage))
                });
                if let Some(result) = instruction.result {
                    let answer = answer.wrapping_mul(17).wrapping_add(result as u64);
                    values[result] = Some(answer);
                    storage[plan.values[result].expect("aggregate definition")] = Some(answer);
                }
            }
            for operand in &block.terminal_uses {
                read(*operand, &values, &storage);
            }
            let Some(successor) = block.successors.first() else {
                break;
            };
            previous = Some(at);
            at = *successor;
        }
    }

    #[test]
    fn dense_loop_updates_and_edges_share_one_backing() {
        // bb0: a = fixed_vector; jump bb1(a)
        // bb1(p): next = run_boundary(p); jump bb1(next)
        let mut entry = block(&[], vec![define(0, &[], None)], &[0]);
        entry.successors.push(1);
        entry.transfers.push((1, 0));
        let mut body = block(&[1], vec![define(2, &[1], Some(1))], &[2]);
        body.successors.push(1);
        body.transfers.push((1, 2));
        let plan = plan(
            &FlowGraph {
                entry_parameters: Vec::new(),
                blocks: vec![entry, body],
                coalesce: true,
            },
            3,
        );
        assert_eq!(plan.slots.len(), 1);
    }

    #[test]
    fn an_old_snapshot_live_after_an_update_stays_separate() {
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![block(&[], vec![define(1, &[0], Some(0))], &[0, 1])],
            coalesce: true,
        };
        assert_ne!(plan(&graph, 2).values[0], plan(&graph, 2).values[1]);
    }

    #[test]
    fn a_backedge_to_entry_preserves_original_function_parameters() {
        let mut body = block(&[], vec![define(1, &[0], Some(0))], &[1]);
        body.successors.push(0);
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![body],
            coalesce: true,
        };
        let plan = plan(&graph, 2);
        assert_ne!(plan.values[0], plan.values[1]);
    }

    #[test]
    fn an_unused_definition_still_cannot_clobber_a_live_value() {
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![block(&[], vec![define(1, &[0], Some(0))], &[0])],
            coalesce: true,
        };
        let plan = plan(&graph, 2);
        assert_ne!(plan.values[0], plan.values[1]);
    }

    #[test]
    fn swapped_loop_parameters_require_distinct_destinations() {
        // Both backedge sources must be snapshotted before either parameter
        // slot is overwritten. Storage coalescing cannot turn this into two
        // sequential copies, even though each transfer is individually legal.
        let mut entry = block(&[], Vec::new(), &[0, 1]);
        entry.successors.push(1);
        entry.transfers.extend([(2, 0), (3, 1)]);
        let mut body = block(&[2, 3], Vec::new(), &[3, 2]);
        body.successors.push(1);
        body.transfers.extend([(2, 3), (3, 2)]);
        let graph = FlowGraph {
            entry_parameters: vec![0, 1],
            blocks: vec![entry, body],
            coalesce: true,
        };
        let plan = plan(&graph, 4);
        assert_ne!(plan.values[2], plan.values[3]);
        assert_eq!(plan.values[0], plan.values[2]);
        assert_eq!(plan.values[1], plan.values[3]);
    }

    #[test]
    fn a_dead_phi_write_protects_an_outer_value() {
        let mut entry = block(&[], vec![define(0, &[], None)], &[0]);
        entry.successors.push(1);
        entry.transfers.push((1, 0));
        let body = block(&[1], Vec::new(), &[0]);
        let graph = FlowGraph {
            entry_parameters: Vec::new(),
            blocks: vec![entry, body],
            coalesce: true,
        };
        let plan = plan(&graph, 2);
        assert_ne!(plan.values[0], plan.values[1]);
    }

    #[test]
    fn a_phi_destination_may_reuse_an_owner_dropped_on_another_incoming_edge() {
        // bb0(old): branch left or right
        // left: jump joined(old)
        // right: fresh = construct; jump joined(fresh), dropping old
        // joined(answer): return answer
        // Coalescing answer with old is valid, but the right edge must read
        // old for cleanup before writing fresh to answer's backing. The
        // terminal-use oracle executes the right edge with that ordering.
        let mut entry = block(&[], Vec::new(), &[]);
        entry.successors.extend([2, 1]);
        let mut left = block(&[], Vec::new(), &[0]);
        left.successors.push(3);
        left.transfers.push((2, 0));
        let mut right = block(&[], vec![define(1, &[], None)], &[0, 1]);
        right.successors.push(3);
        right.transfers.push((2, 1));
        let joined = block(&[2], Vec::new(), &[2]);
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![entry, left, right, joined],
            coalesce: true,
        };
        let plan = plan(&graph, 3);
        assert_eq!(plan.values[0], plan.values[2]);
        assert_ne!(plan.values[1], plan.values[2]);
    }

    #[test]
    fn exposed_values_are_frozen_even_after_their_last_direct_use() {
        let mut expose = define(1, &[0], None);
        expose.exposed = Some(0);
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![block(&[], vec![expose, define(2, &[0], Some(0))], &[2])],
            coalesce: true,
        };
        let plan = plan(&graph, 3);
        assert_ne!(plan.values[0], plan.values[2]);
        assert!(plan.is_exposed(plan.values[0].expect("stored exposed input")));
        assert!(!plan.is_exposed(plan.values[2].expect("stored update result")));
    }

    #[test]
    fn a_call_cannot_alias_a_last_use_argument_via_transitive_edge_unions() {
        let mut entry = block(&[], vec![define(0, &[], None)], &[0]);
        entry.successors.push(1);
        entry.transfers.push((1, 0));
        let mut body = block(
            &[1],
            vec![define(2, &[1], Some(1)), define(3, &[2], None)],
            &[3],
        );
        body.successors.push(1);
        body.transfers.push((1, 3));
        let graph = FlowGraph {
            entry_parameters: Vec::new(),
            blocks: vec![entry, body],
            coalesce: true,
        };
        let plan = plan(&graph, 4);
        assert_eq!(plan.values[1], plan.values[2]);
        assert_ne!(plan.values[2], plan.values[3]);
    }

    #[test]
    fn deferred_schedules_keep_distinct_backing() {
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![block(&[], vec![define(1, &[0], Some(0))], &[1])],
            coalesce: false,
        };
        let plan = plan(&graph, 2);
        assert_ne!(plan.values[0], plan.values[1]);
    }

    #[test]
    fn projections_and_loads_keep_snapshot_storage() {
        let graph = FlowGraph {
            entry_parameters: vec![0],
            blocks: vec![block(
                &[],
                vec![define(1, &[0], None), define(2, &[0], Some(0))],
                &[1, 2],
            )],
            coalesce: true,
        };
        let plan = plan(&graph, 3);
        assert_eq!(plan.values[0], plan.values[2]);
        assert_ne!(plan.values[1], plan.values[2]);
    }

    fn with_program(source: &[u8], test: impl FnOnce(&IrProgram)) {
        use crate::*;

        let limits = CompilerLimits::default();
        let inputs = [SourceInput::new("storage.wf", source)];
        let bundle = SourceBundle::with_prelude(&inputs, limits.source).expect("valid source");
        let LexOutcome::Complete(lexed) = lex(&bundle, limits.lexer) else {
            panic!("lex")
        };
        let TerminalOutcome::Complete(classified) =
            classify_terminals(&lexed, ACTIVE_KERNEL_SPEC_HASH, limits.terminals)
        else {
            panic!("terminals")
        };
        let parsed = match parse(classified, limits.parser) {
            ParseOutcome::Complete(parsed) => parsed,
            other => panic!("parse: {other:?}"),
        };
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, limits.finalizer) else {
            panic!("finalize")
        };
        let CanonicalOutcome::Complete(canonical) = audit_canonical(*finalized, limits.canonical)
        else {
            panic!("canonical")
        };
        let ResolutionOutcome::Complete(resolved) = resolve(canonical) else {
            panic!("resolve")
        };
        let checked = match check_semantics(&resolved) {
            SemanticOutcome::Complete(checked) => checked,
            other => panic!("semantics: {other:?}"),
        };
        let program = lower_checked(*checked, OverlapLowering::Off).expect("lower");
        test(&program);
    }

    #[test]
    fn fresh_binding_destinations_keep_call_inputs_and_snapshots_separate() {
        with_program(
            br#"nocopy struct Row {
  left: u64;
  right: u64;
}

fn build(seed: u64) -> result: Row pure {
  let after = seed +wrap 1_u64;
  return Row(left: seed, right: after);
}

fn exchange(old: &Row) -> result: Row writes(old) {
  let previous = build(seed: 11_u64);
  swap(first: old, second: &previous);
  set deref(old).left = 99_u64;
  return move previous;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = build(seed: 11_u64);
  let previous = exchange(old: &first);
  if first.left != 99_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if previous.left != 11_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            |program| {
                let mut direct_calls = 0;
                for function in program.functions() {
                    let plan = FunctionStoragePlan::build(program, function).expect("plan");
                    for block in function.blocks() {
                        for instruction in block.instructions() {
                            let IrInstruction::Define {
                                result, operation, ..
                            } = instruction
                            else {
                                continue;
                            };
                            let Some(slot) = plan.slot(*result) else {
                                continue;
                            };
                            if let Some(address) = plan.destination(slot) {
                                assert_eq!(
                                    plan.values
                                        .iter()
                                        .filter(|member| **member == Some(slot))
                                        .count(),
                                    1
                                );
                                assert!(block.instructions().iter().any(|instruction| matches!(instruction,
                                    IrInstruction::Define { result: target, operation: IrOperation::AddressOf { value, .. }, .. }
                                    if *target == address && value == result
                                )));
                                if let IrOperation::Call { arguments, .. } = operation {
                                    direct_calls += 1;
                                    for argument in arguments {
                                        assert_ne!(plan.slot(*argument), Some(slot));
                                        assert_ne!(*argument, address);
                                    }
                                }
                            }
                            if let IrOperation::Load { address, .. } = operation {
                                // Reading an existing place remains a snapshot,
                                // even when a later fresh owner adopts that snapshot.
                                assert_ne!(plan.destination(slot), Some(*address));
                            }
                        }
                    }
                }
                assert_eq!(
                    direct_calls, 2,
                    "each fresh owner receives its helper result directly"
                );
            },
        );
    }

    #[test]
    fn repeated_storage_is_distinct_from_acyclic_entry_and_exit_storage() {
        let mut entry = block(&[], Vec::new(), &[]);
        entry.successors.push(1);
        let mut header = block(&[], Vec::new(), &[]);
        header.successors.extend([2, 3]);
        let mut body = block(&[], Vec::new(), &[]);
        body.successors.push(1);
        let graph = FlowGraph {
            entry_parameters: Vec::new(),
            blocks: vec![entry, header, body, block(&[], Vec::new(), &[])],
            coalesce: true,
        };
        assert!(!graph.reentered(0));
        assert!(graph.reentered(1));
        assert!(graph.reentered(2));
        assert!(!graph.reentered(3));
    }

    #[test]
    fn a_synchronous_whole_result_reuses_one_consumed_owned_binding() {
        with_program(
            br#"nocopy struct Row {
  left: u64;
  right: u64;
}

fn pass(value: Row) -> result: Row pure {
  return move value;
}

fn relay(value: Row) -> result: Row pure {
  return pass(value: move value);
}

fn main() -> status: std::process::ExitStatus pure {
  let row = Row(left: 3_u64, right: 5_u64);
  let kept = relay(value: move row);
  if kept.left != 3_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            |program| {
                let relay = program
                    .functions()
                    .iter()
                    .find(|function| function.name() == "relay")
                    .expect("relay");
                let plan = FunctionStoragePlan::build(program, relay).expect("plan");
                let (result, argument) = relay
                    .blocks()
                    .iter()
                    .flat_map(|block| block.instructions())
                    .find_map(|instruction| match instruction {
                        IrInstruction::Define {
                            result,
                            operation: IrOperation::Call { arguments, .. },
                            ..
                        } => Some((result.to_owned(), arguments[0])),
                        _ => None,
                    })
                    .expect("call");
                assert_eq!(plan.slot(result), plan.slot(argument));
            },
        );
    }

    #[test]
    fn several_same_typed_owned_inputs_do_not_choose_an_alias_candidate() {
        with_program(
            br#"nocopy struct Row {
  left: u64;
  right: u64;
}

fn choose(left: Row, right: Row) -> result: Row pure {
  return move right;
}

fn relay(left: Row, right: Row) -> result: Row pure {
  return choose(left: move left, right: move right);
}

fn main() -> status: std::process::ExitStatus pure {
  let left = Row(left: 1_u64, right: 2_u64);
  let right = Row(left: 3_u64, right: 4_u64);
  let kept = relay(left: move left, right: move right);
  if kept.left != 3_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            |program| {
                let relay = program
                    .functions()
                    .iter()
                    .find(|function| function.name() == "relay")
                    .expect("relay");
                let plan = FunctionStoragePlan::build(program, relay).expect("plan");
                let (result, arguments) = relay
                    .blocks()
                    .iter()
                    .flat_map(|block| block.instructions())
                    .find_map(|instruction| match instruction {
                        IrInstruction::Define {
                            result,
                            operation: IrOperation::Call { arguments, .. },
                            ..
                        } => Some((result.to_owned(), arguments.as_slice())),
                        _ => None,
                    })
                    .expect("call");
                assert!(
                    arguments
                        .iter()
                        .all(|argument| plan.slot(*argument) != plan.slot(result))
                );
            },
        );
    }

    #[test]
    fn a_multi_result_retains_its_complete_parent_allocation() {
        with_program(
            br#"nocopy struct Row {
  left: u64;
  right: u64;
}

fn split(value: Row) -> (updated: Row, observed: u64) pure {
  let observed = value.left;
  return move value, observed;
}

fn relay(value: Row) -> result: Row pure {
  let (updated, observed) = split(value: move value);
  return move updated;
}

fn main() -> status: std::process::ExitStatus pure {
  let row = Row(left: 3_u64, right: 5_u64);
  let kept = relay(value: move row);
  if kept.left != 3_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            |program| {
                let relay = program
                    .functions()
                    .iter()
                    .find(|function| function.name() == "relay")
                    .expect("relay");
                let plan = FunctionStoragePlan::build(program, relay).expect("plan");
                let (result, argument) = relay
                    .blocks()
                    .iter()
                    .flat_map(|block| block.instructions())
                    .find_map(|instruction| match instruction {
                        IrInstruction::Define {
                            result,
                            operation: IrOperation::Call { arguments, .. },
                            ..
                        } => Some((result.to_owned(), arguments[0])),
                        _ => None,
                    })
                    .expect("call");
                assert_ne!(plan.slot(result), plan.slot(argument));
                let parent = plan.slot(result).expect("complete result");
                let input = plan.slot(argument).expect("owned input");
                let field = plan.field_destination(input).expect("consumed input field");
                assert_eq!(field.parent_slot, parent);
                assert_eq!(field.field, 0);
                assert_eq!(plan.allocation_root(input), parent);
                assert_eq!(plan.field_destination(parent), None);
                assert_ne!(plan.slots[parent], plan.slots[input]);
            },
        );
    }

    #[test]
    fn checked_dense_ir_coalesces_without_changing_ownership() {
        with_program(
            br#"fn main() -> status: std::process::ExitStatus pure {
  let built = slots_new::<u64, 8>();
  for @fill (
    at in 0_u64..8_u64,
    invariant grown: built.len >= at,
    invariant spare: built.cap + at >= built.len + 8_u64
  ) {
    place_back(window: &built, value: 1_u64);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            |program| {
                let function = &program
                    .functions()
                    .iter()
                    .find(|function| function.name() == "main")
                    .expect("fixture main");
                let plan = FunctionStoragePlan::build(program, function).expect("plan");
                // The construction row and the boundary row are ordinary
                // [PRE-1] calls (compiler/prelude-records), so what this
                // fixture shows is that the window the construction returns
                // and the window each `place_back` addresses are one frame
                // slot: the construction's result, every value of window type,
                // and every address taken of one all share that backing.
                let slots: BTreeSet<_> = function
                    .value_types()
                    .iter()
                    .enumerate()
                    .filter(|(_, ty)| {
                        matches!(
                            ty,
                            IrType::Window { .. }
                                | IrType::Address(crate::IrAddressed::Window { .. })
                        )
                    })
                    .filter_map(|(index, _)| plan.values.get(index).copied().flatten())
                    .collect();
                assert_eq!(slots.len(), 1, "construction and append use one backing");
                // PRE-2's ExitStatus is an ordinary opaque nominal, so its
                // direct-call result owns backing independently of the run.
                let IrType::Nominal(result) = function.result() else {
                    panic!("the source entry returns an ordinary nominal");
                };
                assert!(matches!(
                    program.nominal(result).expect("result nominal").kind(),
                    IrNominalKind::Opaque
                ));
                assert_eq!(
                    plan.slots()
                        .iter()
                        .filter(|ty| **ty == function.result())
                        .count(),
                    1,
                    "the opaque result uses exactly one separate slot"
                );
                assert_ne!(
                    plan.slots()[*slots.first().expect("run slot")],
                    function.result()
                );
                assert_eq!(plan.slots().len(), 2);
            },
        );
    }
}
