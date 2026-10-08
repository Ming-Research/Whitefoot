//! Call-offer grain: which permitted statement-group call offers are
//! published.
//!
//! A range split is priced by the runtime work unit and a recursive component's
//! offers are bounded by the recursion budget, but a statement-group call offer
//! had no price, and on a real program nearly all of them call a few
//! instructions: an offer costs more to publish, steal and join than such a
//! call runs. This keeps an offer exactly when its callee belongs to or reaches
//! a cyclic call component that offers its own calls, whose depth the recursion
//! budget governs, or when the callee's static work summary reaches
//! [`CALL_OFFER_WORK_UNIT`]. A component none of whose groups calls into it
//! spends no budget level at any depth, so a callee that reaches only such a
//! recursion is priced like any other callee. Every other member runs as the
//! ordinary call its refused edge already makes, at its original position; the
//! source-last join site stays, and no group gains a member. It reads only the
//! IR after checking, adds nothing to an offer at run time and changes no value
//! ([call-offer grain](../../../../research/investigations/call-offer-grain/DESIGN.md),
//! [recursive offers](../../../../research/investigations/recursive-offer-grain/DESIGN.md)).

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

/// The call each overlap member defines, by its result value.
fn member_callees(function: &IrFunction) -> HashMap<crate::ir::IrValueId, usize> {
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
    callee_of
}

/// What each function reaches: a cyclic call component that offers its own
/// calls, only components that offer none, or no cyclic component.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reach {
    Acyclic,
    Unoffered,
    Offered,
}

/// What recursion each function belongs to or reaches. A cyclic component
/// offers its own calls when a group of one of its members calls into it: only
/// such a call spends a level of the recursion budget, so only there does the
/// budget bound how deeply the component's offers nest.
fn reaches_recursion(functions: &[IrFunction], edges: &[Vec<usize>]) -> Vec<Reach> {
    let mut reaches = vec![Reach::Acyclic; edges.len()];
    // Components come callees first, so every callee outside a component is
    // settled before the component that calls it.
    for component in crate::cycles::components(edges) {
        let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
        let own = if !cyclic {
            Reach::Acyclic
        } else if component.iter().any(|&member| {
            let callee_of = member_callees(&functions[member]);
            functions[member].overlaps.iter().any(|overlap| {
                overlap.members.iter().any(|value| {
                    callee_of
                        .get(value)
                        .is_some_and(|callee| component.contains(callee))
                })
            })
        }) {
            Reach::Offered
        } else {
            Reach::Unoffered
        };
        let reached = component
            .iter()
            .flat_map(|&member| &edges[member])
            .filter(|callee| !component.contains(callee))
            .map(|&callee| reaches.get(callee).copied().unwrap_or(Reach::Offered))
            .fold(own, Reach::max);
        for member in component {
            reaches[member] = reached;
        }
    }
    reaches
}

/// Removes every published call offer below the grain, naming each in the
/// actualization ledger with its callee's static work.
///
/// Whether a component offers its own calls is read from the groups that
/// remain, so it is decided again after every pass: omitting a small member
/// can dissolve the group that made a recursion count as offering, and the
/// budget spends nothing in a recursion whose groups are gone. Passes repeat
/// until one omits nothing, which comes because each pass only removes.
pub(super) fn prune(functions: &mut [IrFunction], weights: &[u64], ledger: &mut Vec<String>) {
    let edges: Vec<_> = functions.iter().map(callees).collect();
    let names: Vec<_> = functions
        .iter()
        .map(|function| function.name.clone())
        .collect();
    loop {
        let recursive = reaches_recursion(functions, &edges);
        let mut omitted_any = false;
        for function in functions.iter_mut() {
            let callee_of = member_callees(function);
            let mut omitted = Vec::new();
            for overlap in &mut function.overlaps {
                let join = overlap.join_site();
                overlap.members.retain(|member| {
                    // The source-last member is the join site, which is never
                    // published; a member that is no call keeps its offer.
                    let Some(&callee) = callee_of.get(member).filter(|_| Some(*member) != join)
                    else {
                        return true;
                    };
                    let weight = weights.get(callee).copied().unwrap_or(u64::MAX);
                    let reach = recursive.get(callee).copied().unwrap_or(Reach::Offered);
                    let keep = reach == Reach::Offered || weight >= CALL_OFFER_WORK_UNIT;
                    if !keep {
                        omitted.push((callee, weight, reach));
                    }
                    keep
                });
            }
            function
                .overlaps
                .retain(|overlap| overlap.members.len() >= 2);
            omitted_any |= !omitted.is_empty();
            for (callee, weight, reach) in omitted {
                let recursion = if reach == Reach::Unoffered {
                    "reaches only recursion that offers none of its own calls"
                } else {
                    "no recursion"
                };
                ledger.push(format!(
                    "PAR actualization  {}  call grain: omitted offer of {} (static work {weight} below {CALL_OFFER_WORK_UNIT}, {recursion})",
                    function.name, names[callee]
                ));
            }
        }
        if !omitted_any {
            return;
        }
    }
}
