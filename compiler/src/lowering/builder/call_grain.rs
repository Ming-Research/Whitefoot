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
//!
//! The same grain prices the other side of each hand-off: the join site, the
//! member the caller runs itself before it joins. One below the grain leaves
//! the group and runs as the ordinary call it already is, after the join of
//! the members before it; the member before it becomes the join site and is
//! priced in turn, and a group left with one member dissolves. A single costly
//! chain whose caller keeps only a tiny call before the join so offers
//! nothing, while a balanced recursion's kept call reaches the same offered
//! recursion and keeps its group, and a range driver's halves both call the
//! driver itself
//! ([both-sides grain](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#experiment-6b-both-sides-grain-and-the-fir-single-thread-criterion-fixed-before-it-measures)).

use std::collections::HashMap;

use super::{IrFunction, IrInstruction, IrOperation};

/// The work a callee must reach to be offered, in the static summary's units:
/// the estimated work of one range driver slice (`SLICE_WORK` in
/// `lowering/builder/demand.rs`).
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

/// Removes every published call offer below the grain and every join site
/// below it, naming each in the actualization ledger with its callee's static
/// work.
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
            // A member below the grain, with its callee's work and reach; a
            // member that is no call is never below it.
            let below = |member: &crate::ir::IrValueId| {
                let &callee = callee_of.get(member)?;
                let weight = weights.get(callee).copied().unwrap_or(u64::MAX);
                let reach = recursive.get(callee).copied().unwrap_or(Reach::Offered);
                (reach != Reach::Offered && weight < CALL_OFFER_WORK_UNIT)
                    .then_some((callee, weight, reach))
            };
            let mut omitted = Vec::new();
            let mut unkept = Vec::new();
            for overlap in &mut function.overlaps {
                // The caller runs the join site itself before it joins. One
                // below the grain leaves the group and runs after the join,
                // and the member before it is priced next, until one member
                // is left, which hands nothing off.
                while overlap.members.len() >= 2 {
                    let Some(found) = overlap.join_site().and_then(|join| below(&join)) else {
                        break;
                    };
                    unkept.push(found);
                    overlap.members.pop();
                }
                let join = overlap.join_site();
                overlap.members.retain(|member| {
                    // The source-last member is the join site, which is never
                    // published.
                    let Some(found) = below(member).filter(|_| Some(*member) != join) else {
                        return true;
                    };
                    omitted.push(found);
                    false
                });
            }
            function
                .overlaps
                .retain(|overlap| overlap.members.len() >= 2);
            omitted_any |= !omitted.is_empty() || !unkept.is_empty();
            let recursion = |reach: Reach| {
                if reach == Reach::Unoffered {
                    "reaches only recursion that offers none of its own calls"
                } else {
                    "no recursion"
                }
            };
            for (callee, weight, reach) in omitted {
                ledger.push(format!(
                    "PAR actualization  {}  call grain: omitted offer of {} (static work {weight} below {CALL_OFFER_WORK_UNIT}, {})",
                    function.name, names[callee], recursion(reach)
                ));
            }
            for (callee, weight, reach) in unkept {
                ledger.push(format!(
                    "PAR actualization  {}  call grain: join site {} runs after the join (kept static work {weight} below {CALL_OFFER_WORK_UNIT}, {})",
                    function.name, names[callee], recursion(reach)
                ));
            }
        }
        if !omitted_any {
            return;
        }
    }
}
