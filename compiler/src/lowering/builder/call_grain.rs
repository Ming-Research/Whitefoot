//! Call-offer grain: which permitted statement-group call offers are
//! published.
//!
//! A range split is priced by the runtime work unit and a recursive component's
//! offers are bounded by the recursion budget, but a statement-group call offer
//! had no price, and on a real program nearly all of them call a few
//! instructions: an offer costs more to publish, steal and join than such a
//! call runs. This keeps an offer exactly when its callee belongs to or reaches
//! a cyclic call component, whose depth the recursion budget governs, or when
//! the callee's static work summary reaches [`CALL_OFFER_WORK_UNIT`]. Every
//! other member runs as the ordinary call its refused edge already makes, at
//! its original position; the source-last join site stays, and no group gains a
//! member. It reads only the IR after checking, adds nothing to an offer at run
//! time and changes no value
//! ([call-offer grain](../../../../research/investigations/call-offer-grain/DESIGN.md)).

use std::collections::HashMap;

use super::{IrFunction, IrInstruction, IrOperation};

/// The work a callee must reach to be offered, in the static summary's units:
/// the runtime's default split work unit (`WF_PAR_SPLIT_WORK_UNIT` in
/// `backend/sched/entry.c`), the price a range chunk must reach.
pub(super) const CALL_OFFER_WORK_UNIT: u64 = 150_000;

/// Every function each function calls, a split counting its splitter and chunk.
fn callees(function: &IrFunction) -> Vec<usize> {
    let mut called = Vec::new();
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        match instruction {
            IrInstruction::Define {
                operation: IrOperation::Call { function, .. },
                ..
            } => called.push(*function as usize),
            IrInstruction::Define {
                operation:
                    IrOperation::LoopSplit {
                        splitter, chunk, ..
                    },
                ..
            } => called.extend([*splitter as usize, *chunk as usize]),
            _ => {}
        }
    }
    called
}

/// Whether each function belongs to or reaches a cyclic call component.
fn reaches_recursion(edges: &[Vec<usize>]) -> Vec<bool> {
    let mut reaches = vec![false; edges.len()];
    // Components come callees first, so every callee outside a component is
    // settled before the component that calls it.
    for component in crate::cycles::components(edges) {
        let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
        let reached = cyclic
            || component
                .iter()
                .flat_map(|&member| &edges[member])
                .any(|&callee| reaches.get(callee).copied().unwrap_or(false));
        for member in component {
            reaches[member] = reached;
        }
    }
    reaches
}

/// Removes every published call offer below the grain, naming each in the
/// actualization ledger with its callee's static work.
pub(super) fn prune(functions: &mut [IrFunction], weights: &[u64], ledger: &mut Vec<String>) {
    let edges: Vec<_> = functions.iter().map(callees).collect();
    let recursive = reaches_recursion(&edges);
    let names: Vec<_> = functions
        .iter()
        .map(|function| function.name.clone())
        .collect();
    for function in functions.iter_mut() {
        let mut callee_of = HashMap::new();
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let IrInstruction::Define {
                result,
                operation: IrOperation::Call { function, .. },
                ..
            } = instruction
            {
                callee_of.insert(*result, *function as usize);
            }
        }
        let mut omitted = Vec::new();
        for overlap in &mut function.overlaps {
            let join = overlap.join_site();
            overlap.members.retain(|member| {
                // The source-last member is the join site, which is never
                // published; a member that is no call keeps its offer.
                let Some(&callee) = callee_of.get(member).filter(|_| Some(*member) != join) else {
                    return true;
                };
                let weight = weights.get(callee).copied().unwrap_or(u64::MAX);
                let keep = recursive.get(callee).copied().unwrap_or(true)
                    || weight >= CALL_OFFER_WORK_UNIT;
                if !keep {
                    omitted.push((callee, weight));
                }
                keep
            });
        }
        function
            .overlaps
            .retain(|overlap| overlap.members.len() >= 2);
        for (callee, weight) in omitted {
            ledger.push(format!(
                "PAR actualization  {}  call grain: omitted offer of {} (static work {weight} below {CALL_OFFER_WORK_UNIT}, no recursion)",
                function.name, names[callee]
            ));
        }
    }
}
