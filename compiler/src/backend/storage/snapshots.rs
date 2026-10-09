//! Read-through placement of logical by-value Load snapshots. Unknown roots,
//! operations and call contracts keep the ordinary copy. This analysis changes
//! neither IR values nor source ownership (compiler/storage-placement).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::{BackendFailure, FlowGraph, FunctionStoragePlan, index};
use crate::backend::abi::FunctionAbi;
use crate::{
    IrFunction, IrInstruction, IrNominalKind, IrOperation, IrProgram, IrSourceMode, IrTerminator,
    IrType, IrValueId,
};

type MaterializationSites = BTreeMap<(usize, usize), BTreeSet<usize>>;

struct SnapshotFacts<'a> {
    program: &'a IrProgram,
    function: &'a IrFunction,
    origins: &'a [Option<usize>],
    definitions: Vec<Option<&'a IrOperation>>,
    /// Checked source reference roots, and fresh local allocation identities.
    roots: Vec<Option<usize>>,
    /// Callee plans are deliberately built without snapshot selection, so
    /// recursion cannot recursively invoke this analysis.
    immutable: BTreeMap<u32, BTreeSet<usize>>,
    sequential: bool,
}

impl FunctionStoragePlan {
    pub(super) fn select_read_through(
        &mut self,
        program: &IrProgram,
        function: &IrFunction,
        sequential: bool,
    ) -> Result<(), BackendFailure> {
        // A sequential clone executes these groups and its callees in source
        // order. Only the overlapping world has deferred reads beyond a call.
        if function.blocks().is_empty()
            || function.waits()
            || (!sequential && !function.overlaps().is_empty())
        {
            return Ok(());
        }
        if !function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .any(|instruction| {
                matches!(instruction, IrInstruction::Define {
                    result, operation: IrOperation::Load { .. }, ..
                } if self.slot(*result).is_some())
            })
        {
            return Ok(());
        }
        let graph = FlowGraph::from_function(program, function, sequential)?;
        let parts = crate::backend::emitter::dispatch::storage_parts(function)?;
        let entering = graph.live_in();
        let mut facts = SnapshotFacts::new(program, function, &self.origins, sequential);
        for (block_index, block) in function.blocks().iter().enumerate() {
            for (instruction_index, instruction) in block.instructions().iter().enumerate() {
                let IrInstruction::Define {
                    result,
                    operation: IrOperation::Load { address, .. },
                    ..
                } = instruction
                else {
                    continue;
                };
                let Some(root) = facts.roots[index(*address)] else {
                    continue;
                };
                if self.slot(*result).is_none() || parts[block_index].is_none() {
                    continue;
                }
                let family: BTreeSet<_> = self
                    .origins
                    .iter()
                    .enumerate()
                    .filter_map(|(value, origin)| {
                        (*origin == Some(index(*result))).then_some(value)
                    })
                    .collect();
                let slots: BTreeSet<_> = family
                    .iter()
                    .filter_map(|value| self.values[*value])
                    .collect();
                if slots
                    .iter()
                    .any(|slot| !self.holds_origin(*slot, index(*result)))
                {
                    continue;
                }
                // A carry from an earlier dynamic execution must not become
                // the current execution's source pointer, even if both loads
                // have the same static definition identity.
                let mut live = graph.live_after(&graph.blocks[block_index], &entering);
                for instruction in graph.blocks[block_index].instructions[instruction_index..]
                    .iter()
                    .rev()
                {
                    if let Some(result) = instruction.result {
                        live.remove(&result);
                    }
                    live.extend(instruction.operands.iter().copied());
                }
                let Some(copies) = facts.materialization_sites(
                    &graph,
                    &parts,
                    parts[block_index],
                    root,
                    &family,
                )?
                else {
                    continue;
                };
                if !live.is_disjoint(&family)
                    || !facts.source_survives(
                        &graph,
                        (block_index, instruction_index),
                        root,
                        &family,
                        &copies,
                    )?
                {
                    continue;
                }
                for slot in slots {
                    self.read_through[slot] = Some(*address);
                }
                for (point, values) in copies {
                    for value in values {
                        if let Some(slot) = self.values[value] {
                            self.snapshot_copies.entry(point).or_default().insert(slot);
                            self.snapshot_backing.insert(slot);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'a> SnapshotFacts<'a> {
    fn new(
        program: &'a IrProgram,
        function: &'a IrFunction,
        origins: &'a [Option<usize>],
        sequential: bool,
    ) -> Self {
        let mut definitions = vec![None; origins.len()];
        for instruction in function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
        {
            if let IrInstruction::Define {
                result, operation, ..
            } = instruction
            {
                definitions[index(*result)] = Some(operation);
            }
        }
        let mut facts = Self {
            program,
            function,
            origins,
            definitions,
            roots: vec![None; origins.len()],
            immutable: BTreeMap::new(),
            sequential,
        };
        for value in 0..origins.len() {
            facts.roots[value] = facts.root(value, &mut BTreeSet::new());
        }
        facts
    }

    /// Distinct checked reference parameters separate a read from a write by
    /// EFF-5; typed projections and loads retain their containing root. Whole
    /// roots deliberately conflate different fields and indices. A changed or
    /// multiply-originating reference, shared state or unknown producer has no
    /// disjointness proof. Aggregates cannot contain references (TYPE-8).
    fn root(&self, value: usize, visiting: &mut BTreeSet<usize>) -> Option<usize> {
        let value = self.origins[value]?;
        if let Some(root) = self.roots[value] {
            return Some(root);
        }
        if !visiting.insert(value) {
            return None;
        }
        if let Some(position) = self
            .function
            .parameters()
            .iter()
            .position(|(parameter, _)| index(*parameter) == value)
        {
            return (self.function.source_signature()?.parameters().get(position)
                == Some(&IrSourceMode::Reference))
            .then_some(value);
        }
        match self.definitions[value]? {
            IrOperation::ProjectAddress { address, .. } | IrOperation::Load { address, .. } => {
                self.root(index(*address), visiting)
            }
            IrOperation::SliceFromRun { run } => self.root(index(*run), visiting),
            IrOperation::SliceFromBuffer { buffer } => self.root(index(*buffer), visiting),
            IrOperation::SliceRange { slice, .. } | IrOperation::SliceAddress { slice, .. } => {
                self.root(index(*slice), visiting)
            }
            IrOperation::AddressOf { value: source, .. } => {
                // Copying a box pointer into a binding does not make its
                // payload a distinct allocation. Retain a known input root.
                self.root(index(*source), visiting).or_else(|| {
                    matches!(
                        self.definitions[self.origins[index(*source)]?]?,
                        IrOperation::ConstructStruct { .. }
                            | IrOperation::ConstructEnum { .. }
                            | IrOperation::ArrayFill { .. }
                            | IrOperation::Window
                            | IrOperation::Constant(_)
                    )
                    .then_some(value)
                })
            }
            IrOperation::BoxNew { .. }
            | IrOperation::BufferFill { .. }
            | IrOperation::WindowBlockNew { .. }
            | IrOperation::SegmentsFill { .. } => Some(value),
            _ => None,
        }
    }

    fn scalar(&self, value: IrValueId) -> bool {
        match self.function.value_type(value) {
            Some(IrType::Unit | IrType::Bool | IrType::Integer { .. } | IrType::Float { .. }) => {
                true
            }
            Some(IrType::Nominal(id)) => self
                .program
                .nominal(id)
                .is_some_and(|nominal| nominal.is_tag_only_enum()),
            _ => false,
        }
    }

    /// A checked own argument with only inline data cannot reach another
    /// allocation, even when its value came from an unknown producer. Empty
    /// release alone proves no representation fact about opaque types or
    /// borrowed descriptors, which stay outside this closed inline subset.
    fn inline_value(&self, value: IrValueId) -> Result<bool, BackendFailure> {
        let mut pending = vec![
            self.function
                .value_type(value)
                .ok_or(BackendFailure::InvalidIr)?,
        ];
        let mut visited = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match ty {
                IrType::Unit | IrType::Bool | IrType::Integer { .. } | IrType::Float { .. } => {}
                IrType::Array { element, .. }
                | IrType::Window {
                    element,
                    capacity: Some(_),
                    ..
                } => {
                    pending.push(
                        self.program
                            .element(element)
                            .ok_or(BackendFailure::InvalidIr)?,
                    );
                }
                IrType::Nominal(id) => match self
                    .program
                    .nominal(id)
                    .ok_or(BackendFailure::InvalidIr)?
                    .kind()
                {
                    IrNominalKind::Struct { fields } => {
                        pending.extend(fields.iter().map(crate::IrField::ty))
                    }
                    IrNominalKind::Enum { variants } => pending.extend(
                        variants
                            .iter()
                            .flat_map(crate::IrVariant::fields)
                            .map(crate::IrField::ty),
                    ),
                    _ => return Ok(false),
                },
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    /// A sufficient subset of the emitter's immutable-parameter rule. The
    /// acyclic-body restriction ensures it cannot become a split definition;
    /// a declaration supplies no implementation evidence. Destination-result
    /// callees are also safe consumers: their prologue captures indirect inputs
    /// before writing a possibly aliased result. That ABI rule stays in place;
    /// it does not require a redundant snapshot in this caller.
    fn immutable_parameter(
        &mut self,
        callee: u32,
        position: usize,
    ) -> Result<bool, BackendFailure> {
        if let Some(positions) = self.immutable.get(&callee) {
            return Ok(positions.contains(&position));
        }
        let function = self
            .program
            .functions()
            .get(callee as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        let mut positions = BTreeSet::new();
        if !function.blocks().is_empty()
            && !function.waits()
            && (self.sequential || function.overlaps().is_empty())
        {
            let graph = FlowGraph::from_function(self.program, function, self.sequential)?;
            let abi = FunctionAbi::build(self.program, function)?;
            if !(0..graph.blocks.len()).any(|block| graph.reentered(block)) {
                let plan =
                    FunctionStoragePlan::build_copies(self.program, function, self.sequential)?;
                let returned =
                    crate::backend::emitter::places::returned_storage_slot(function, &plan);
                for (position, ((value, _), parameter)) in function
                    .parameters()
                    .iter()
                    .zip(abi.parameters())
                    .enumerate()
                {
                    if parameter.is_indirect()
                        && plan.holds_only(*value)
                        && plan.slot(*value) != returned
                        && function
                            .source_signature()
                            .and_then(|signature| signature.parameters().get(position))
                            == Some(&IrSourceMode::Own)
                    {
                        positions.insert(position);
                    }
                }
            }
        }
        let eligible = positions.contains(&position);
        self.immutable.insert(callee, positions);
        Ok(eligible)
    }

    /// An ineligible observation gets private backing just for that operation.
    /// Exposed or mixed-origin storage was excluded by holds_origin: letting an
    /// address escape here would require tracking later mutations of the copy.
    fn materialization_sites(
        &mut self,
        graph: &FlowGraph,
        parts: &[Option<usize>],
        part: Option<usize>,
        root: usize,
        family: &BTreeSet<usize>,
    ) -> Result<Option<MaterializationSites>, BackendFailure> {
        let mut copies = BTreeMap::new();
        for (block_index, block) in self.function.blocks().iter().enumerate() {
            for (at, instruction) in block.instructions().iter().enumerate() {
                let used: BTreeSet<_> = instruction
                    .operands()
                    .iter()
                    .map(|value| index(*value))
                    .filter(|value| family.contains(value))
                    .collect();
                if used.is_empty() {
                    continue;
                }
                if parts[block_index] != part {
                    return Ok(None);
                }
                let reads = match instruction {
                    IrInstruction::Define {
                        result,
                        operation:
                            IrOperation::ProjectVariant { .. }
                            | IrOperation::ProjectStruct {
                                consume_root: false,
                                ..
                            },
                        ..
                    } => self.scalar(*result),
                    IrInstruction::Define {
                        operation:
                            IrOperation::Call {
                                function,
                                arguments,
                            },
                        ..
                    } => {
                        let mut reads = true;
                        for (position, value) in arguments.iter().enumerate() {
                            if family.contains(&index(*value))
                                && !self.immutable_parameter(*function, position)?
                            {
                                reads = false;
                            }
                        }
                        reads
                    }
                    _ => false,
                };
                // Even an immutable formal cannot read through when another
                // argument lets this call change the source during the read.
                // Capturing immediately before the call protects that boundary.
                if !reads || self.invalidates(instruction, root)? {
                    copies.insert((block_index, at), used);
                }
            }
            let terminal = block.terminator();
            let used: BTreeSet<_> = terminal
                .operands()
                .iter()
                .map(|value| index(*value))
                .filter(|value| family.contains(value))
                .collect();
            if used.is_empty() {
                continue;
            }
            if parts[block_index] != part {
                return Ok(None);
            }
            let reads = match terminal {
                IrTerminator::Match { .. } => true,
                IrTerminator::Jump { drops, .. } => {
                    !drops
                        .iter()
                        .any(|drop| family.contains(&index(drop.operand())))
                        && graph.blocks[block_index]
                            .transfers
                            .iter()
                            .all(|(target, source)| {
                                !family.contains(source) || family.contains(target)
                            })
                }
                _ => false,
            };
            if !reads {
                copies.insert((block_index, block.instructions().len()), used);
            }
        }
        Ok(Some(copies))
    }

    /// Explore both clean and invalidated states at every instruction. A
    /// mutation after the last use is harmless; one on ANY path to a use is
    /// not. Re-executing the Load starts a new interval (old carries were
    /// excluded above). This finite walk also covers loops and cleanup edges.
    fn source_survives(
        &self,
        graph: &FlowGraph,
        load: (usize, usize),
        root: usize,
        family: &BTreeSet<usize>,
        copies: &MaterializationSites,
    ) -> Result<bool, BackendFailure> {
        let mut pending = vec![(load.0, load.1 + 1, false)];
        let mut seen = BTreeSet::new();
        while let Some((block, at, dirty)) = pending.pop() {
            if (block, at) == load || !seen.insert((block, at, dirty)) {
                continue;
            }
            let body = &self.function.blocks()[block];
            if let Some(instruction) = body.instructions().get(at) {
                let invalidates = self.invalidates(instruction, root)?;
                // A private copy precedes the operation and may survive its
                // writes. A read-through argument needs stability for the
                // complete call. Every later use still checks its own interval.
                if (dirty || (invalidates && !copies.contains_key(&(block, at))))
                    && instruction
                        .operands()
                        .iter()
                        .any(|value| family.contains(&index(*value)))
                {
                    return Ok(false);
                }
                pending.push((block, at + 1, dirty || invalidates));
            } else {
                if dirty
                    && graph.blocks[block]
                        .terminal_uses
                        .iter()
                        .any(|value| family.contains(value))
                {
                    return Ok(false);
                }
                let dirty = dirty
                    || match body.terminator() {
                        IrTerminator::Jump { drops, .. } | IrTerminator::Return { drops, .. } => {
                            drops
                                .iter()
                                .any(|drop| self.may_alias(drop.operand(), root))
                        }
                        _ => false,
                    };
                pending.extend(
                    graph.blocks[block]
                        .successors
                        .iter()
                        .map(|next| (*next, 0, dirty)),
                );
            }
        }
        Ok(true)
    }

    fn may_alias(&self, value: IrValueId, root: usize) -> bool {
        self.roots[index(value)].is_none_or(|other| other == root)
    }

    fn invalidates(
        &self,
        instruction: &IrInstruction,
        root: usize,
    ) -> Result<bool, BackendFailure> {
        // Moving an owner can hide this allocation under a different root.
        // Its later release would then look unrelated. Stop at the transfer,
        // using the IR's release graph rather than treating a fresh wrapper
        // as proof of disjointness. Pointer-free copies do not transfer any
        // allocation's release authority and still permit unrelated stores.
        let transferred = match instruction {
            IrInstruction::Store { value, .. } | IrInstruction::StoreSlice { value, .. } => {
                std::slice::from_ref(value)
            }
            IrInstruction::Define { operation, .. } => match operation {
                IrOperation::ConstructStruct { fields, .. }
                | IrOperation::ConstructEnum { fields, .. } => fields.as_slice(),
                IrOperation::AddressOf { value, .. }
                | IrOperation::BoxNew { value, .. }
                | IrOperation::ArrayFill { value, .. }
                | IrOperation::BufferFill { value, .. }
                | IrOperation::SegmentsFill { value, .. }
                | IrOperation::RunInsert { value, .. } => std::slice::from_ref(value),
                IrOperation::RunBoundary { value, .. } => value.as_slice(),
                _ => &[],
            },
            _ => &[],
        };
        for value in transferred {
            let ty = self
                .function
                .value_type(*value)
                .ok_or(BackendFailure::InvalidIr)?;
            if self.may_alias(*value, root)
                && crate::ir::type_derives_release(
                    self.program.nominals(),
                    self.program.elements(),
                    ty,
                )
                .ok_or(BackendFailure::InvalidIr)?
            {
                return Ok(true);
            }
        }
        Ok(match instruction {
            IrInstruction::Store { address, .. } => self.may_alias(*address, root),
            IrInstruction::StoreSlice { slice, .. } => self.may_alias(*slice, root),
            IrInstruction::Drops(drops) => drops
                .iter()
                .any(|drop| self.may_alias(drop.operand(), root)),
            IrInstruction::IndexedMark { .. } => true,
            IrInstruction::Define {
                result, operation, ..
            } => match operation {
                IrOperation::Call {
                    function,
                    arguments,
                } => {
                    let callee = self
                        .program
                        .functions()
                        .get(*function as usize)
                        .ok_or(BackendFailure::InvalidIr)?;
                    let Some(signature) = callee.source_signature() else {
                        return Ok(true);
                    };
                    if callee.waits() || (!self.sequential && !callee.overlaps().is_empty()) {
                        return Ok(true);
                    }
                    for (position, argument) in arguments.iter().enumerate() {
                        if signature.parameters().get(position) == Some(&IrSourceMode::Own)
                            && self.inline_value(*argument)?
                        {
                            continue;
                        }
                        let readonly = matches!(
                            signature.parameters().get(position),
                            Some(IrSourceMode::Reference | IrSourceMode::Range)
                        ) && callee.parameters().get(position).is_some_and(
                            |(formal, _)| callee.readonly_reference_parameters.contains(formal),
                        );
                        if !readonly && !self.scalar(*argument) && self.may_alias(*argument, root) {
                            return Ok(true);
                        }
                    }
                    false
                }
                IrOperation::RunBoundary { run, .. }
                | IrOperation::RunTaken { run, .. }
                | IrOperation::RunShift { run, .. }
                | IrOperation::RunInsert { run, .. } => self.may_alias(*run, root),
                IrOperation::RunTransfer {
                    destination,
                    source,
                    ..
                } => self.may_alias(*destination, root) || self.may_alias(*source, root),
                IrOperation::WindowGrow { cell, .. } => self.may_alias(*cell, root),
                IrOperation::CellFree { value, .. } | IrOperation::BoxTake { value, .. } => {
                    self.may_alias(*value, root)
                }
                // Reinitializing the containing allocation in a loop is a
                // write even though these operations produce a fresh value.
                IrOperation::AddressOf { .. }
                | IrOperation::BoxNew { .. }
                | IrOperation::BufferFill { .. }
                | IrOperation::WindowBlockNew { .. }
                | IrOperation::SegmentsFill { .. } => self.roots[index(*result)] == Some(root),
                IrOperation::Constant(_)
                | IrOperation::ConstantAddress { .. }
                | IrOperation::Integer { .. }
                | IrOperation::Float { .. }
                | IrOperation::NumericConversion { .. }
                | IrOperation::Reinterpret { .. }
                | IrOperation::Boolean { .. }
                | IrOperation::EnumEquality { .. }
                | IrOperation::ProjectAddress { .. }
                | IrOperation::Load { .. }
                | IrOperation::ProjectStruct {
                    consume_root: false,
                    ..
                }
                | IrOperation::ProjectVariant { .. }
                | IrOperation::ConstructStruct { .. }
                | IrOperation::ConstructEnum { .. }
                | IrOperation::ContainerMeasure { .. }
                | IrOperation::BufferMeasure { .. }
                | IrOperation::SliceMeasure { .. }
                | IrOperation::SliceFromRun { .. }
                | IrOperation::SliceFromBuffer { .. }
                | IrOperation::SliceRange { .. }
                | IrOperation::SliceAddress { .. } => false,
                // Includes waits, shared holds/releases, context transfers,
                // indexed/split work and any future operation without a rule.
                _ => true,
            },
        })
    }
}
