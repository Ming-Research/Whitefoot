//! Call results read at the storage they copy (compiler/storage-placement,
//! proposed).
//!
//! A definition whose result uses a caller destination also has an internal
//! `<symbol>.source` entry with the same parameters, which returns the
//! address holding its result. On a path whose result is a by-value Load
//! snapshot of a place under one of its reference parameters, that place
//! stays unchanged from the read through the activation's exit, cleanup
//! included, and the entry returns the place's address; a path that returns
//! a call it forwarded returns that call's address when the call received
//! this activation's own destination; every other path builds the result in
//! the destination and returns the destination.
//!
//! A caller reads a call's result through the address its source entry
//! returns only when every use of the result reads it in place, as a Load
//! snapshot's read-through uses do, and the snapshot's invalidating events
//! prove every place the callee's reference and range arguments supply
//! unchanged and valid from the call through each use. The callee's row names
//! only places under those arguments [EFF-2], so this covers what its
//! [EFF-5]-substituted row names; the call's destination stays the caller's
//! complete, unexposed slot, which storage interference keeps from every
//! other value while the result is live. Neither side reads another
//! function's body, so a caller's choice depends on its callee's signature
//! alone. The logical value is still the value at the return; this changes
//! placement, not a language rule.

use std::collections::{BTreeMap, BTreeSet};

use super::snapshots::SnapshotFacts;
use super::{BackendFailure, FlowGraph, FunctionStoragePlan, index};
use crate::backend::abi::FunctionAbi;
use crate::backend::emitter::places::returned_storage_slot;
use crate::{
    IrFunction, IrInstruction, IrOperation, IrPlaceStep, IrProgram, IrSourceMode, IrTerminator,
    IrValueId,
};

/// Where a value's reads find its bytes instead of its own slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::backend) enum ValueSource {
    /// The address the defining call's source entry returned, named by that
    /// call's result.
    Call(IrValueId),
    /// The address operand of a Load snapshot this definition's source entry
    /// returns, which therefore takes no copy.
    Load(IrValueId),
}

/// The address one `Return` of a source entry hands back.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::backend) enum ReturnSource {
    /// The address operand of a Load snapshot.
    Load(IrValueId),
    /// The address a forwarded call's own source entry returned.
    Call(IrValueId),
}

#[derive(Debug, Default)]
pub(in crate::backend) struct ResultSources {
    values: BTreeMap<usize, ValueSource>,
    /// The `Return` blocks of this definition's source entry that return a
    /// source; every other `Return` returns the destination.
    returns: BTreeMap<usize, ReturnSource>,
    /// Parameter positions whose own referent a source return may address,
    /// rather than storage behind an owner pointer loaded from it.
    returned_referents: BTreeSet<usize>,
}

/// The facts every check of one definition reads.
struct Selection<'plan, 'ir> {
    plan: &'plan FunctionStoragePlan,
    function: &'ir IrFunction,
    graph: FlowGraph,
    entering: Vec<BTreeSet<usize>>,
    parts: Vec<Option<usize>>,
    facts: SnapshotFacts<'plan>,
    /// Whether this emission defines the definition's source entry.
    returns: bool,
    /// The destination slot of this definition's own result, which a call
    /// it forwards must have received.
    forwarding_slot: Option<usize>,
}

impl FunctionStoragePlan {
    pub(in crate::backend) fn value_source(&self, value: IrValueId) -> Option<ValueSource> {
        self.result_sources.values.get(&index(value)).copied()
    }

    /// Whether this definition's source entry returns a source on some path;
    /// otherwise its source entry builds every result in the destination.
    pub(in crate::backend) fn returns_sources(&self) -> bool {
        !self.result_sources.returns.is_empty()
    }

    pub(in crate::backend) fn return_source(&self, block: usize) -> Option<ReturnSource> {
        self.result_sources.returns.get(&block).copied()
    }

    /// Whether a source return may address parameter `position`'s own
    /// referent, so that pointer is captured through the return.
    pub(in crate::backend) fn returns_referent(&self, position: usize) -> bool {
        self.result_sources.returned_referents.contains(&position)
    }

    /// Select the call results read at their source and, when `entry` says
    /// this emission defines the plain `<symbol>.source` entry, that entry's
    /// source returns. Waiting definitions, overlapping execution and
    /// possible split dispatch parts keep the Load snapshot's exclusions.
    pub(in crate::backend) fn select_result_sources(
        &mut self,
        program: &IrProgram,
        function: &IrFunction,
        sequential: bool,
        entry: bool,
    ) -> Result<(), BackendFailure> {
        if function.blocks().is_empty()
            || function.waits()
            || (!sequential && !function.overlaps().is_empty())
        {
            return Ok(());
        }
        let parts = crate::backend::emitter::dispatch::storage_parts(function)?;
        if parts.iter().any(|part| *part != Some(0)) {
            return Ok(());
        }
        let returns = entry
            && FunctionAbi::build(program, function)?
                .result()
                .uses_destination();
        let graph = FlowGraph::from_function(program, function, sequential)?;
        let entering = graph.live_in();
        let selected = {
            let plan: &Self = self;
            let mut selection = Selection {
                plan,
                function,
                graph,
                entering,
                parts,
                facts: SnapshotFacts::new(program, function, &plan.origins, sequential),
                returns,
                forwarding_slot: if returns {
                    returned_storage_slot(function, plan)
                } else {
                    None
                },
            };
            let mut selected = ResultSources::default();
            for (block, body) in function.blocks().iter().enumerate() {
                for (at, instruction) in body.instructions().iter().enumerate() {
                    if let IrInstruction::Define {
                        result,
                        operation:
                            IrOperation::Call {
                                function: callee,
                                arguments,
                            },
                        ..
                    } = instruction
                    {
                        selection.call(
                            program,
                            (block, at),
                            *result,
                            *callee,
                            arguments,
                            &mut selected,
                        )?;
                    }
                }
            }
            if returns {
                selection.load_returns(&mut selected)?;
            }
            selected
        };
        self.result_sources = selected;
        Ok(())
    }

    /// A complete allocation whose address nothing else observes: no binding
    /// destination, no field placement in a parent, no child placed in it.
    fn complete_unexposed(&self, slot: usize) -> bool {
        self.destination(slot).is_none()
            && self.field_destination(slot).is_none()
            && !self.is_exposed(slot)
            && !self
                .fields
                .iter()
                .flatten()
                .any(|field| field.parent_slot == slot)
    }
}

impl Selection<'_, '_> {
    /// The values carrying `origin`'s contents unchanged, when each lives in
    /// a complete, unexposed slot and none is carried into `point` from an
    /// earlier execution of it.
    fn family(&self, origin: IrValueId, point: (usize, usize)) -> Option<BTreeSet<usize>> {
        let family: BTreeSet<usize> = self
            .plan
            .origins
            .iter()
            .enumerate()
            .filter_map(|(value, from)| (*from == Some(index(origin))).then_some(value))
            .collect();
        if family
            .iter()
            .filter_map(|value| self.plan.values[*value])
            .any(|slot| !self.plan.complete_unexposed(slot))
        {
            return None;
        }
        let (block, at) = point;
        let mut live = self
            .graph
            .live_after(&self.graph.blocks[block], &self.entering);
        for instruction in self.graph.blocks[block].instructions[at..].iter().rev() {
            if let Some(defined) = instruction.result {
                live.remove(&defined);
            }
            live.extend(instruction.operands.iter().copied());
        }
        live.is_disjoint(&family).then_some(family)
    }

    /// The `Return` blocks among the uses of `family` when every other use
    /// reads it in place and the storage under `root` stays unchanged from
    /// `point` through each read, or `None`. A return qualifies only in a
    /// source entry and only when `returnable`.
    fn read_uses(
        &mut self,
        point: (usize, usize),
        root: Option<usize>,
        family: &BTreeSet<usize>,
        returnable: bool,
    ) -> Result<Option<BTreeSet<usize>>, BackendFailure> {
        let Some(copies) =
            self.facts
                .materialization_sites(&self.graph, &self.parts, Some(0), root, family)?
        else {
            return Ok(None);
        };
        let mut returning = BTreeSet::new();
        for (block, at) in copies.keys() {
            let body = &self.function.blocks()[*block];
            let returned = self.returns
                && returnable
                && *at == body.instructions().len()
                && matches!(body.terminator(), IrTerminator::Return { value, drops }
                    if family.contains(&index(*value))
                        && !drops.iter().any(|drop| family.contains(&index(drop.operand()))));
            if !returned {
                return Ok(None);
            }
            returning.insert(*block);
        }
        if !self
            .facts
            .source_survives(&self.graph, point, root, family, &BTreeMap::new())?
        {
            return Ok(None);
        }
        Ok(Some(returning))
    }

    /// Whether storage under `root` survives from `point` through every exit
    /// in `returning`, cleanup included.
    fn stable_to_exits(
        &self,
        point: (usize, usize),
        root: Option<usize>,
        returning: &BTreeSet<usize>,
    ) -> Result<bool, BackendFailure> {
        for exit in returning {
            if !self
                .facts
                .stable_to_return(&self.graph, point, root, *exit)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Select one call when its callee's signature gives it a source entry
    /// and every use of its result reads at the source while the source
    /// stays unchanged. Never a partial choice: a caller that cannot read
    /// through every use calls the public entry and copies nothing itself.
    fn call(
        &mut self,
        program: &IrProgram,
        point: (usize, usize),
        result: IrValueId,
        callee: u32,
        arguments: &[IrValueId],
        selected: &mut ResultSources,
    ) -> Result<(), BackendFailure> {
        let target = program
            .functions()
            .get(callee as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        if target.waits()
            || !FunctionAbi::build(program, target)?
                .result()
                .uses_destination()
        {
            return Ok(());
        }
        let Some(signature) = target.source_signature() else {
            return Ok(());
        };
        let Some(slot) = self.plan.slot(result) else {
            return Ok(());
        };
        let Some(family) = self.family(result, point) else {
            return Ok(());
        };
        // A forwarded call must have received this activation's own
        // destination, so the address it returns is never a local slot.
        let forwards = self.forwarding_slot == Some(slot);
        // The source set: every place the reference and range arguments
        // supply. An argument without a checked root conflates every place.
        let mut roots = BTreeSet::new();
        let mut unknown = false;
        for (position, argument) in arguments.iter().enumerate() {
            if matches!(
                signature.parameters().get(position),
                Some(IrSourceMode::Reference | IrSourceMode::Range)
            ) {
                match self.facts.roots[index(*argument)] {
                    Some(root) => {
                        roots.insert(Some(root));
                    }
                    None => unknown = true,
                }
            }
        }
        if unknown {
            roots = BTreeSet::from([None]);
        }
        // A callee without reference arguments builds every result.
        if roots.is_empty() {
            return Ok(());
        }
        let mut returning = BTreeSet::new();
        for root in &roots {
            let Some(returns) = self.read_uses(point, *root, &family, forwards)? else {
                return Ok(());
            };
            returning.extend(returns);
        }
        // A forwarded address must lie under this definition's own reference
        // parameters and survive its exit; a local allocation never does.
        let mut referents = BTreeSet::new();
        if !returning.is_empty() {
            for root in &roots {
                let Some(position) = root.and_then(|root| {
                    self.function
                        .parameters()
                        .iter()
                        .position(|(parameter, _)| index(*parameter) == root)
                }) else {
                    return Ok(());
                };
                if !self.stable_to_exits(point, *root, &returning)? {
                    return Ok(());
                }
                referents.insert(position);
            }
        }
        for value in family {
            selected.values.insert(value, ValueSource::Call(result));
        }
        for exit in returning {
            selected.returns.insert(exit, ReturnSource::Call(result));
        }
        selected.returned_referents.extend(referents);
        Ok(())
    }

    /// The source entry's returns of a by-value Load snapshot of a place
    /// reached from a reference parameter by place projections alone, whose
    /// source stays unchanged from the read through that return's cleanup.
    ///
    /// A snapshot already read through its address keeps that placement and
    /// skips only the copy before each such return. Any other snapshot whose
    /// every use reads it in place or returns it reads at its address
    /// throughout, which also covers a snapshot sharing the destination with
    /// a result another path builds.
    fn load_returns(&mut self, selected: &mut ResultSources) -> Result<(), BackendFailure> {
        let mut loads = Vec::new();
        let mut projections = BTreeMap::new();
        for (block, body) in self.function.blocks().iter().enumerate() {
            for (at, instruction) in body.instructions().iter().enumerate() {
                match instruction {
                    IrInstruction::Define {
                        result,
                        operation: IrOperation::Load { address, .. },
                        ..
                    } => loads.push(((block, at), *result, *address)),
                    IrInstruction::Define {
                        result,
                        operation:
                            IrOperation::ProjectAddress {
                                address,
                                projection,
                            },
                        ..
                    } => {
                        projections.insert(*result, (*address, projection));
                    }
                    _ => {}
                }
            }
        }
        for (point, load, address) in loads {
            // Walk the address back to the parameter it projects. Any other
            // producer, such as a local binding's address, is not a place
            // under a reference parameter.
            let mut current = address;
            let mut behind_owner = false;
            let position = loop {
                if let Some(position) = self
                    .function
                    .parameters()
                    .iter()
                    .position(|(parameter, _)| *parameter == current)
                {
                    break self
                        .function
                        .source_signature()
                        .and_then(|signature| signature.parameters().get(position))
                        .is_some_and(|mode| *mode == IrSourceMode::Reference)
                        .then_some(position);
                }
                match projections.get(&current) {
                    Some((_, IrPlaceStep::Measure { .. })) | None => break None,
                    Some((base, step)) => {
                        behind_owner |= matches!(step, IrPlaceStep::BoxReferent { .. });
                        current = *base;
                    }
                }
            };
            let root = self.facts.roots[index(address)];
            let Some(position) = position.filter(|_| root == Some(index(current))) else {
                continue;
            };
            let Some(slot) = self.plan.slot(load) else {
                continue;
            };
            let exits: BTreeSet<usize> = self
                .function
                .blocks()
                .iter()
                .enumerate()
                .filter(|(_, body)| {
                    matches!(body.terminator(), IrTerminator::Return { value, .. }
                        if self.plan.origins[index(*value)] == Some(index(load)))
                })
                .map(|(block, _)| block)
                .collect();
            if exits.is_empty() {
                continue;
            }
            let mut chosen = BTreeSet::new();
            if self.plan.read_through(slot) == Some(address) {
                for exit in exits {
                    if self
                        .facts
                        .stable_to_return(&self.graph, point, root, exit)?
                    {
                        chosen.insert(exit);
                    }
                }
            } else {
                let Some(family) = self.family(load, point) else {
                    continue;
                };
                let Some(returning) = self.read_uses(point, root, &family, true)? else {
                    continue;
                };
                if returning != exits || !self.stable_to_exits(point, root, &returning)? {
                    continue;
                }
                for value in family {
                    selected.values.insert(value, ValueSource::Load(address));
                }
                chosen = returning;
            }
            if !chosen.is_empty() && !behind_owner {
                selected.returned_referents.insert(position);
            }
            for exit in chosen {
                selected.returns.insert(exit, ReturnSource::Load(address));
            }
        }
        Ok(())
    }
}
