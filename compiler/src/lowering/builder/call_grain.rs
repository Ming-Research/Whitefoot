//! Call-offer grain: which permitted statement-group call offers are
//! published.
//!
//! A range split is priced by the runtime work unit and a recursive component's
//! offers are bounded by the recursion budget, but a statement-group call offer
//! had no price, and on a real program nearly all of them call a few
//! instructions: an offer costs more to publish, steal and join than such a
//! call runs. This keeps an offer exactly when its callee belongs to or reaches
//! a cyclic call component that offers its own calls, whose depth the recursion
//! budget governs, or when its price reaches [`CALL_OFFER_WORK_UNIT`]. A
//! component none of whose groups calls into it spends no budget level at any
//! depth, so a callee that reaches only such a recursion is priced like any
//! other callee.
//!
//! The price is the callee's whole-call work summary (the one range pricing
//! builds) bound to the call's own arguments. A price that does not depend on
//! a run-time value, or a static work that already reaches the unit, decides
//! the offer here. A price that depends on the arguments keeps the offer and
//! is compared with the unit at the call, before a lane is acquired, folded to
//! one compare of the argument-dependent part; a price that would need more
//! than one arithmetic operation there loses its offer as before. A function
//! whose every reachable hand-out is such a price over its own entry values
//! also compares them once at entry and, when all are below the unit, runs its
//! sequential version. Every omitted or declined member runs as the ordinary
//! call its refused edge already makes, at its original position; the
//! source-last join site stays, and no group gains a member. It reads only the
//! IR after checking and changes no value
//! ([call-offer grain](../../../../research/investigations/call-offer-grain/DESIGN.md),
//! [recursive offers](../../../../research/investigations/recursive-offer-grain/DESIGN.md)).

use std::collections::{BTreeSet, HashMap};

use super::work::MemberPrices;
use crate::IrWorkEstimate as Work;
use crate::{IrFunction, IrInstruction, IrOfferPrice, IrOperation, IrSynthesis, IrValueId};

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

/// What the grain decides for one handed-out member.
enum Decision {
    /// Offered whenever it is handed out.
    Keep,
    /// The ordinary call; the reason completes its ledger line.
    Omit(&'static str),
    /// Offered when its price at the call reaches the unit; `entry` is the
    /// same price over the caller's entry values, when it is one.
    Priced {
        site: IrOfferPrice,
        entry: Option<IrOfferPrice>,
    },
}

/// A price folded against the work unit.
enum Verdict {
    /// Reaches the unit whatever the arguments.
    Reaches,
    /// Below the unit whatever the arguments.
    Below,
    /// Reaches the unit exactly when `work >= at_least`.
    Runtime(IrOfferPrice),
}

/// Folds `coefficient * work + constant >= unit` to `work >= at_least`, with
/// `at_least = ceil((unit - constant) / coefficient)`: for whole numbers
/// `k * w < m` holds exactly when `w < ceil(m / k)`, and the price's saturating
/// arithmetic agrees because the unit is far below its ceiling. Several
/// argument-dependent terms keep their coefficients in `work`.
fn verdict(price: &Work) -> Verdict {
    let terms = match price {
        Work::Constant(value) if *value >= CALL_OFFER_WORK_UNIT => return Verdict::Reaches,
        Work::Constant(_) => return Verdict::Below,
        Work::Sum(parts) => parts.clone(),
        other => vec![other.clone()],
    };
    let mut constant = 0_u64;
    let mut variable = Vec::new();
    for term in terms {
        match term {
            Work::Constant(value) => constant = constant.saturating_add(value),
            term => variable.push(term),
        }
    }
    if constant >= CALL_OFFER_WORK_UNIT {
        return Verdict::Reaches;
    }
    if variable.is_empty() {
        return Verdict::Below;
    }
    let (coefficient, work) = if variable.len() == 1 {
        match variable.remove(0) {
            Work::Product(left, right) if matches!(*left, Work::Constant(_)) => {
                let Work::Constant(coefficient) = *left else {
                    unreachable!()
                };
                (coefficient, *right)
            }
            term => (1, term),
        }
    } else {
        (1, Work::Sum(variable))
    };
    if coefficient == 0 {
        return Verdict::Below;
    }
    Verdict::Runtime(IrOfferPrice {
        work,
        at_least: (CALL_OFFER_WORK_UNIT - constant).div_ceil(coefficient),
    })
}

/// The arithmetic operations evaluating `work` takes; a leaf is a register or
/// one descriptor or header read.
fn operations(work: &Work) -> usize {
    match work {
        Work::Constant(_) | Work::Value(_) | Work::Length(_) | Work::BoxArrayLength(_) => 0,
        Work::Sum(parts) => parts
            .iter()
            .map(operations)
            .sum::<usize>()
            .saturating_add(parts.len().saturating_sub(1)),
        Work::Product(left, right) | Work::Difference(left, right) => operations(left)
            .saturating_add(operations(right))
            .saturating_add(1),
        Work::Quotient(value, _) => operations(value).saturating_add(1),
    }
}

/// How the ledger spells a price's argument-dependent part, naming IR values
/// by their ordinals.
fn spell(work: &Work) -> String {
    match work {
        Work::Constant(value) => value.to_string(),
        Work::Value(value) => format!("v{}", value.ordinal()),
        Work::Length(value) => format!("len(v{})", value.ordinal()),
        Work::BoxArrayLength(value) => format!("box_len(v{})", value.ordinal()),
        Work::Sum(parts) => format!(
            "({})",
            parts.iter().map(spell).collect::<Vec<_>>().join(" + ")
        ),
        Work::Product(left, right) => format!("{} * {}", spell(left), spell(right)),
        Work::Difference(left, right) => format!("({} - {})", spell(left), spell(right)),
        Work::Quotient(value, divisor) => format!("{} / {divisor}", spell(value)),
    }
}

/// One handed-out member's decision, from its callee's static weight, the
/// recursion its callee reaches and its argument-bound price.
fn decide(weight: u64, reach: Reach, price: Option<&super::work::MemberPrice>) -> Decision {
    if reach == Reach::Offered || weight >= CALL_OFFER_WORK_UNIT {
        return Decision::Keep;
    }
    let Some(price) = price else {
        return Decision::Omit("");
    };
    let entry = match verdict(&price.traced) {
        Verdict::Reaches => return Decision::Keep,
        Verdict::Below => return Decision::Omit(""),
        Verdict::Runtime(entry) => entry,
    };
    match verdict(&price.site) {
        Verdict::Reaches => Decision::Keep,
        Verdict::Below => Decision::Omit(""),
        Verdict::Runtime(site) if operations(&site.work) <= 1 => Decision::Priced {
            site,
            entry: price.at_entry.then_some(entry),
        },
        Verdict::Runtime(_) => Decision::Omit(
            ", and its price over the arguments needs more than one operation and a compare at the call",
        ),
    }
}

/// Every function a function can enter: its calls, the contexts it starts
/// and both halves of its splits.
fn entered(function: &IrFunction) -> Vec<usize> {
    let mut entered = callees(function);
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        if let IrInstruction::Define {
            operation:
                IrOperation::ContextStart { function, .. }
                | IrOperation::ContextStartBound { function, .. },
            ..
        } = instruction
        {
            entered.push(*function as usize);
        }
    }
    entered
}

/// Which functions can reach a hand-out: a remaining group's handed-out
/// member or a range split, in themselves or anything they enter.
fn reaches_hand_out(functions: &[IrFunction], edges: &[Vec<usize>]) -> Vec<bool> {
    let mut reaches: Vec<bool> = functions
        .iter()
        .map(|function| {
            function
                .overlaps
                .iter()
                .any(|overlap| !overlap.handed_out().is_empty())
                || function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|instruction| {
                        matches!(
                            instruction,
                            IrInstruction::Define {
                                operation: IrOperation::LoopSplit { .. },
                                ..
                            }
                        )
                    })
        })
        .collect();
    let mut callers = vec![Vec::new(); functions.len()];
    for (caller, called) in edges.iter().enumerate() {
        for &callee in called {
            if let Some(entry) = callers.get_mut(callee) {
                entry.push(caller);
            }
        }
    }
    let mut pending: Vec<_> = (0..functions.len()).filter(|&f| reaches[f]).collect();
    while let Some(function) = pending.pop() {
        for &caller in &callers[function] {
            if !reaches[caller] {
                reaches[caller] = true;
                pending.push(caller);
            }
        }
    }
    reaches
}

/// Removes every published call offer below the grain, naming each in the
/// actualization ledger with its callee's static work, and records the
/// run-time price of every offer whose price depends on its arguments.
///
/// Whether a component offers its own calls is read from the groups that
/// remain, so it is decided again after every pass: omitting a small member
/// can dissolve the group that made a recursion count as offering, and the
/// budget spends nothing in a recursion whose groups are gone. Passes repeat
/// until one omits nothing, which comes because each pass only removes.
pub(super) fn prune(
    functions: &mut [IrFunction],
    weights: &[u64],
    prices: &MemberPrices,
    ledger: &mut Vec<String>,
) {
    let edges: Vec<_> = functions.iter().map(callees).collect();
    let names: Vec<_> = functions
        .iter()
        .map(|function| function.name.clone())
        .collect();
    let recursive = loop {
        let recursive = reaches_recursion(functions, &edges);
        let mut omitted_any = false;
        for (ordinal, function) in functions.iter_mut().enumerate() {
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
                    match decide(weight, reach, prices.get(&(ordinal, *member))) {
                        Decision::Omit(why) => {
                            omitted.push((callee, weight, reach, why));
                            false
                        }
                        Decision::Keep | Decision::Priced { .. } => true,
                    }
                });
            }
            function
                .overlaps
                .retain(|overlap| overlap.members.len() >= 2);
            omitted_any |= !omitted.is_empty();
            for (callee, weight, reach, why) in omitted {
                let recursion = if reach == Reach::Unoffered {
                    "reaches only recursion that offers none of its own calls"
                } else {
                    "no recursion"
                };
                ledger.push(format!(
                    "PAR actualization  {}  call grain: omitted offer of {} (static work {weight} below {CALL_OFFER_WORK_UNIT}{why}, {recursion})",
                    function.name, names[callee]
                ));
            }
        }
        if !omitted_any {
            break recursive;
        }
    };
    record_prices(functions, weights, prices, &recursive, &names, ledger);
}

/// Records the run-time price of every remaining member whose price depends
/// on its arguments, and the entry selection of every function where those
/// prices, over its entry values, are every hand-out it can reach.
///
/// The sequential version such a selection enters declines every offer below
/// it, so a function qualifies only when it neither splits a range nor enters
/// a function that can reach a hand-out, itself included, and when every
/// handed-out member of its own groups is priced at entry. It must also have
/// a sequential version to enter and a body that can be entered at its start:
/// a range splitter has no clone and a waiting function is a resumable frame.
fn record_prices(
    functions: &mut [IrFunction],
    weights: &[u64],
    prices: &MemberPrices,
    recursive: &[Reach],
    names: &[String],
    ledger: &mut Vec<String>,
) {
    let entered: Vec<_> = functions.iter().map(entered).collect();
    let reaches = reaches_hand_out(functions, &entered);
    for (ordinal, function) in functions.iter_mut().enumerate() {
        let callee_of = member_callees(function);
        let mut priced = Vec::new();
        let mut entry = Vec::new();
        let mut every_member_at_entry = true;
        let handed_out: BTreeSet<IrValueId> = function
            .overlaps
            .iter()
            .flat_map(|overlap| overlap.handed_out().iter().copied())
            .collect();
        for member in &handed_out {
            let Some(&callee) = callee_of.get(member) else {
                every_member_at_entry = false;
                continue;
            };
            let weight = weights.get(callee).copied().unwrap_or(u64::MAX);
            let reach = recursive.get(callee).copied().unwrap_or(Reach::Offered);
            match decide(weight, reach, prices.get(&(ordinal, *member))) {
                Decision::Priced {
                    site,
                    entry: at_entry,
                } => {
                    ledger.push(format!(
                        "PAR actualization  {}  call grain: priced offer of {} (static work {weight} below {CALL_OFFER_WORK_UNIT}; offered when {} >= {}, its work over the arguments reaching {CALL_OFFER_WORK_UNIT})",
                        function.name,
                        names[callee],
                        spell(&site.work),
                        site.at_least
                    ));
                    priced.push((*member, site));
                    match at_entry {
                        Some(price) => entry.push(price),
                        None => every_member_at_entry = false,
                    }
                }
                Decision::Keep | Decision::Omit(_) => every_member_at_entry = false,
            }
        }
        let enters_hand_out = entered[ordinal]
            .iter()
            .any(|&callee| reaches.get(callee).copied().unwrap_or(true));
        let splits = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| {
                matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::LoopSplit { .. },
                        ..
                    }
                )
            });
        if every_member_at_entry
            && !entry.is_empty()
            && !enters_hand_out
            && !splits
            && !function.waits
            && function.synthesis != Some(IrSynthesis::Splitter)
        {
            ledger.push(format!(
                "PAR actualization  {}  call grain: entry runs the sequential version when {}",
                function.name,
                entry
                    .iter()
                    .map(|price| format!("{} < {}", spell(&price.work), price.at_least))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ));
            function.entry_selection = entry;
        }
        function.priced_offers = priced;
    }
}
