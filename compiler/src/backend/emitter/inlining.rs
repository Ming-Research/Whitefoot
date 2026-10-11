//! A bounded source-IR inlining policy, independent of transfer expansion.
//!
//! These are provisional code-growth budgets, not LLVM cost thresholds. A
//! transfer counts once before layout lowering; construction charges each
//! supplied field. Block-parameter forwarding has no additional charge.

use super::{BackendFailure, FunctionAbi, ResultAbi, dispatch};
use crate::{IrFunction, IrInstruction, IrOperation, IrProgram, IrTerminator};

pub(in crate::backend) const ATTRIBUTE: &str = " alwaysinline \"wf-inline-policy\"=\"bounded-ir\"";

const LOCAL_LIMIT: usize = 64;
const EXPANDED_LIMIT: usize = 256;

#[derive(Clone, Copy)]
struct Candidate {
    cost: usize,
    cyclic: bool,
    stored_result: bool,
}

#[derive(Clone, Copy)]
struct Expansion {
    cost: usize,
    cyclic: bool,
}

pub(in crate::backend) struct InlinePlan {
    selected: Vec<bool>,
}

impl InlinePlan {
    pub(in crate::backend) fn build(program: &IrProgram) -> Result<Self, BackendFailure> {
        Self::build_in_world(program, false)
    }

    /// A clone erases overlap scheduling. Plan every callee in that same
    /// world, so selected expansion costs and loop summaries match ordinary
    /// sequential lowering rather than inheriting the overlapped plan.
    pub(in crate::backend) fn build_sequential(
        program: &IrProgram,
    ) -> Result<Self, BackendFailure> {
        Self::build_in_world(program, true)
    }

    fn build_in_world(program: &IrProgram, sequential: bool) -> Result<Self, BackendFailure> {
        let functions = program.functions();
        let mut edges = vec![Vec::new(); functions.len()];
        let mut costs = vec![None; functions.len()];
        for (index, function) in functions.iter().enumerate() {
            let reachable = dispatch::reachable(function)?;
            // Keep every direct edge, including calls in ineligible bodies:
            // otherwise a cycle through a large callee could be missed.
            for (block, live) in function.blocks().iter().zip(&reachable) {
                if !live {
                    continue;
                }
                for instruction in block.instructions() {
                    let IrInstruction::Define { operation, .. } = instruction else {
                        continue;
                    };
                    let callees: &[u32] = match operation {
                        IrOperation::Call { function, .. }
                        | IrOperation::ContextStart { function, .. }
                        | IrOperation::ContextStartBound { function, .. } => {
                            std::slice::from_ref(function)
                        }
                        IrOperation::LoopSplit {
                            splitter, chunk, ..
                        } => std::slice::from_ref(if sequential { chunk } else { splitter }),
                        _ => &[],
                    };
                    for &callee in callees {
                        if callee as usize >= functions.len() {
                            return Err(BackendFailure::InvalidIr);
                        }
                        edges[index].push(callee as usize);
                    }
                }
            }
            costs[index] = local_cost(program, function, &reachable, sequential)?;
        }
        Ok(Self {
            selected: select(&costs, &edges),
        })
    }

    pub(in crate::backend) fn contains(&self, ordinal: usize) -> bool {
        self.selected[ordinal]
    }
}

fn select(candidates: &[Option<Candidate>], edges: &[Vec<usize>]) -> Vec<bool> {
    let mut expanded: Vec<Option<Expansion>> = vec![None; candidates.len()];
    // The shared iterative decomposition visits callees before callers.
    for component in crate::cycles::components(edges) {
        let index = component[0];
        if component.len() != 1 || edges[index].contains(&index) {
            continue;
        }
        let Some(candidate) = candidates[index].filter(|candidate| candidate.cost <= LOCAL_LIMIT)
        else {
            continue;
        };
        let mut expansion = Expansion {
            cost: candidate.cost,
            cyclic: candidate.cyclic,
        };
        for &callee in &edges[index] {
            // Each call site is charged separately. Calls of an unselected
            // callee retain their local unit without absorbing its body.
            if let Some(callee) = expanded[callee] {
                expansion.cost = expansion.cost.saturating_add(callee.cost);
                expansion.cyclic |= callee.cyclic;
            }
        }
        // A register-result wrapper must not absorb a loop early, whether
        // written here or introduced by a selected scalar-return callee.
        if expansion.cost <= EXPANDED_LIMIT && !(candidate.stored_result && expansion.cyclic) {
            expanded[index] = Some(expansion);
        }
    }
    expanded.into_iter().map(|cost| cost.is_some()).collect()
}

fn local_cost(
    program: &IrProgram,
    function: &IrFunction,
    reachable: &[bool],
    sequential: bool,
) -> Result<Option<Candidate>, BackendFailure> {
    if function.blocks().is_empty()
        || function.waits()
        || (!sequential && !function.overlaps().is_empty())
        || function.synthesis().is_some()
    {
        return Ok(None);
    }
    let mut cost = 0_usize;
    for (block, live) in function.blocks().iter().zip(reachable) {
        if !live {
            continue;
        }
        cost = cost.saturating_add(1); // terminator, including each return
        match block.terminator() {
            IrTerminator::Jump { drops, .. } | IrTerminator::Return { drops, .. }
                if !drops.is_empty() =>
            {
                return Ok(None);
            }
            _ => {}
        }
        for instruction in block.instructions() {
            let units = match instruction {
                IrInstruction::Define { operation, .. } => operation_cost(operation),
                IrInstruction::Store { .. } | IrInstruction::StoreSlice { .. } => Some(1),
                IrInstruction::Drops(drops) if drops.is_empty() => Some(0),
                IrInstruction::Drops(_) | IrInstruction::IndexedMark { .. } => None,
            };
            let Some(units) = units else {
                return Ok(None);
            };
            cost = cost.saturating_add(units);
        }
    }
    if cost > LOCAL_LIMIT
        || dispatch::storage_parts(function)?
            .iter()
            .any(|part| *part != Some(0))
    {
        return Ok(None);
    }
    Ok(Some(Candidate {
        cost,
        cyclic: cyclic(function, reachable),
        stored_result: matches!(
            FunctionAbi::build(program, function)?.result(),
            ResultAbi::StoredValue(_)
        ),
    }))
}

fn operation_cost(operation: &IrOperation) -> Option<usize> {
    match operation {
        IrOperation::ConstructStruct { fields, .. } | IrOperation::ConstructEnum { fields, .. } => {
            Some(fields.len().saturating_add(1))
        }
        IrOperation::Constant(_)
        | IrOperation::Call { .. }
        | IrOperation::Integer { .. }
        | IrOperation::Float { .. }
        | IrOperation::NumericConversion { .. }
        | IrOperation::Reinterpret { .. }
        | IrOperation::Boolean { .. }
        | IrOperation::ArrayIndex { .. }
        | IrOperation::BufferIndex { .. }
        | IrOperation::BufferMeasure { .. }
        | IrOperation::ContainerMeasure { .. }
        | IrOperation::RunIndex { .. }
        | IrOperation::RunBoundary { .. }
        | IrOperation::RunTaken { .. }
        | IrOperation::RunInsert { .. }
        | IrOperation::SliceFromBuffer { .. }
        | IrOperation::SliceFromRun { .. }
        | IrOperation::SliceRange { .. }
        | IrOperation::SliceMeasure { .. }
        | IrOperation::SliceIndex { .. }
        | IrOperation::SliceAddress { .. }
        | IrOperation::BoxDeref { .. }
        | IrOperation::ProjectStruct { .. }
        | IrOperation::InsertStruct { .. }
        | IrOperation::ProjectVariant { .. }
        | IrOperation::AddressOf { .. }
        | IrOperation::ConstantAddress { .. }
        | IrOperation::ProjectAddress { .. }
        | IrOperation::Load { .. } => Some(1),
        // Allocations, fill/shift loops, aggregate equality, contexts,
        // shared-state machinery and unknown future forms have no small
        // expansion estimate here and keep the host's ordinary policy.
        _ => None,
    }
}

fn cyclic(function: &IrFunction, reachable: &[bool]) -> bool {
    let edges = function
        .blocks()
        .iter()
        .zip(reachable)
        .map(|(block, live)| {
            if !live {
                return Vec::new();
            }
            match block.terminator() {
                IrTerminator::Jump { target, .. } => vec![target.index()],
                IrTerminator::Match { targets, .. } => targets
                    .iter()
                    .map(|target| target.block().index())
                    .collect(),
                IrTerminator::Return { .. } | IrTerminator::Unreachable => Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    crate::cycles::components(&edges)
        .iter()
        .any(|component| component.len() > 1 || edges[component[0]].contains(&component[0]))
}

#[cfg(test)]
mod tests {
    use super::{Candidate, EXPANDED_LIMIT, LOCAL_LIMIT, operation_cost, select};
    use crate::{IrNominalId, IrOperation, IrValueId};

    fn selection(costs: &[Option<usize>], edges: &[Vec<usize>]) -> Vec<bool> {
        let candidates = costs
            .iter()
            .map(|cost| {
                cost.map(|cost| Candidate {
                    cost,
                    cyclic: false,
                    stored_result: false,
                })
            })
            .collect::<Vec<_>>();
        select(&candidates, edges)
    }

    #[test]
    fn budgets_include_boundaries_and_charge_each_selected_call() {
        assert_eq!(
            selection(
                &[Some(LOCAL_LIMIT), Some(LOCAL_LIMIT + 1)],
                &[vec![], vec![]]
            ),
            [true, false]
        );
        // 64 + three 64-unit bodies exactly fills the expansion allowance.
        let costs = [Some(64), Some(64), Some(64)];
        let edges = [vec![], vec![0, 0, 0], vec![0, 0, 0, 0]];
        assert_eq!(EXPANDED_LIMIT, 256);
        assert_eq!(selection(&costs, &edges), [true, true, false]);
        // A large retained callee does not charge its body to the caller.
        assert_eq!(
            selection(&[Some(65), Some(2)], &[vec![], vec![0]]),
            [false, true]
        );
    }

    #[test]
    fn all_recursive_members_are_excluded_even_through_large_bodies() {
        let costs = [Some(2), Some(100), Some(2), Some(2)];
        let edges = [vec![1], vec![0], vec![2], vec![0]];
        assert_eq!(selection(&costs, &edges), [false, false, false, true]);
    }

    #[test]
    fn constructors_charge_fields_and_unknown_expansions_are_excluded() {
        let operation = IrOperation::ConstructStruct {
            nominal: IrNominalId(0),
            fields: vec![IrValueId(0); LOCAL_LIMIT],
        };
        assert_eq!(operation_cost(&operation), Some(LOCAL_LIMIT + 1));
        assert_eq!(operation_cost(&IrOperation::ContextJoin), None);
    }
}
