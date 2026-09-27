//! The budget-carrying clone family a compute-actualizing build gives each
//! ordinary cyclic call-graph component. Ordinary calls and runtime interfaces
//! stay shared.
//!
//! A component is read off the call graph and the permission table and nothing
//! else: no source name, signature, shape, project or test identity reaches
//! this decision. Neither does any acceptance judgment read its result — the
//! budget selects which permitted offers a run actualizes, exactly as the loop
//! splitter's allowance does, and the same programs are accepted and the same
//! values computed whichever way it is fixed.

use std::collections::HashSet;

use crate::{IrInstruction, IrOperation, IrProgram, RecursionBudget};

use super::parallel::sequential_clone_symbol;
use super::source_symbol;

/// The symbol one member's budget-carrying variant is emitted under.
///
/// It lives in the same reserved `wf__par_` namespace as the runtime's own
/// symbols, which [FORM-3] puts out of reach of any source IDENT, so the
/// variant can never collide with a function the writer declared.
pub(super) fn recursion_budget_symbol(name: &str) -> String {
    format!("wf__par_budget_{name}")
}

/// Recognize a synthesized budget variant for post-codegen stack attribution.
pub(crate) fn is_recursion_budget_symbol(symbol: &str) -> bool {
    symbol
        .strip_prefix("wf__par_budget_")
        .is_some_and(|name| !name.is_empty())
}

/// The component one budget-carrying variant belongs to.
///
/// A member's ordinary symbol is not emitted through this: it obtains the
/// initial budget and calls the variant, so the body exists once.
#[derive(Clone, Copy)]
pub(super) struct Grain {
    component: usize,
}

/// Every ordinary cyclic component of one module, and what was done with it.
#[derive(Default)]
pub(super) struct RecursiveFrontiers {
    /// Where a call into a component starts counting, or `None` when no
    /// family is emitted at all.
    budget: Option<RecursionBudget>,
    component_of: Vec<Option<usize>>,
    /// What this module did with each cyclic component it found, in the
    /// deterministic component order, for `--par-ledger`. A component that
    /// keeps its ordinary path says so here and names the member that put it
    /// there, so an unspecialized recursion is explicit rather than silent.
    ledger: Vec<String>,
}

impl RecursiveFrontiers {
    pub(super) fn new(program: &IrProgram, clones: &HashSet<u32>) -> Self {
        let Some(selected) = program.recursion_budget() else {
            return Self::default();
        };
        let mechanism = match selected {
            RecursionBudget::Off => "off".to_owned(),
            RecursionBudget::RuntimeDerived => "runtime-derived".to_owned(),
            RecursionBudget::Pinned(value) => format!("pinned {value}"),
        };
        let functions = program.functions();
        let mut edges = vec![Vec::new(); functions.len()];
        for (ordinal, function) in functions.iter().enumerate() {
            for instruction in function.blocks().iter().flat_map(|b| b.instructions()) {
                match instruction {
                    IrInstruction::Define {
                        operation: IrOperation::Call { function, .. },
                        ..
                    } => {
                        edges[ordinal].push(*function as usize);
                    }
                    IrInstruction::Define {
                        operation:
                            IrOperation::LoopSplit {
                                splitter, chunk, ..
                            },
                        ..
                    } => {
                        edges[ordinal].extend([*splitter as usize, *chunk as usize]);
                    }
                    _ => {}
                }
            }
        }
        let mut component_of = vec![None; functions.len()];
        let mut components = Vec::new();
        let mut families = 0_usize;
        for (id, mut component) in super::super::graph::components(&edges)
            .into_iter()
            .enumerate()
        {
            let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
            if !cyclic {
                continue;
            }
            // Ordinal order, so one component reads the same way whatever
            // order the decomposition popped its members in.
            component.sort_unstable();
            // The structural conditions the family needs, asked one member at a
            // time so the ledger can name the one that refused. A component
            // passes exactly when no member answers.
            let refusal = component.iter().find_map(|&ordinal| {
                let function = &functions[ordinal];
                let name = function.name();
                // Structural reasons first, so a member that is several of
                // these at once is named by the one a reader can act on: a
                // splitter is synthesized *and* has no clone, and the first of
                // those is why.
                if function.synthesis().is_some() {
                    Some(format!("{name} is a synthesized loop function"))
                } else if !u32::try_from(ordinal).is_ok_and(|i| clones.contains(&i)) {
                    Some(format!("{name} has no sequential clone"))
                } else {
                    None
                }
            });
            let members = component
                .iter()
                .map(|&ordinal| functions[ordinal].name())
                .collect::<Vec<_>>()
                .join(", ");
            match refusal {
                Some(reason) => components.push(format!(
                    "PAR frontier    component({members})  excluded: {reason}"
                )),
                None if selected == RecursionBudget::Off => components.push(format!(
                    "PAR frontier    component({members})  no family: recursion budget off"
                )),
                None => {
                    families += 1;
                    components.push(format!(
                        "PAR frontier    component({members})  budget-carrying clone family, entered with recursion budget {mechanism}"
                    ));
                    for ordinal in component {
                        component_of[ordinal] = Some(id);
                    }
                }
            }
        }
        let mut ledger = vec![format!(
            "PAR frontier    recursion budget {mechanism}  family emitted for {families} of {} cyclic components",
            components.len()
        )];
        ledger.extend(components);
        Self {
            budget: (selected != RecursionBudget::Off).then_some(selected),
            component_of,
            ledger,
        }
    }

    /// What this module did with the recursion budget, for `--par-ledger`.
    /// Empty when this lowering actualizes no compute at all.
    pub(super) fn ledger(&self) -> &[String] {
        &self.ledger
    }

    pub(super) fn is_used(&self) -> bool {
        self.component_of.iter().any(Option::is_some)
    }

    /// Where a call into a component starts counting. `None` exactly when no
    /// family is emitted.
    pub(super) const fn initial(&self) -> Option<RecursionBudget> {
        self.budget
    }

    /// The component holding this function, or `None` for a function no
    /// family holds — which is every function of a build that emits no family
    /// at all.
    pub(super) fn grain(&self, ordinal: usize) -> Option<Grain> {
        Some(Grain {
            component: self.component_of.get(ordinal).copied().flatten()?,
        })
    }

    /// The symbol a call made inside a budgeted world names, and whether that
    /// call spends one level of the caller's budget.
    ///
    /// A call that stays inside the component names the callee's variant and
    /// carries the remaining budget; a call that leaves it names the callee's
    /// ordinary entry, which asks for a budget of its own. Nothing here reads
    /// the budget's value: the world below the cut is selected by the callee's
    /// own entry test, so the caller emits one call and no branch.
    pub(super) fn callee(&self, ordinal: u32, name: &str, grain: Grain) -> (String, bool) {
        if self.spends(ordinal, grain) {
            (recursion_budget_symbol(name), true)
        } else {
            (source_symbol(name), false)
        }
    }

    /// Whether a call from this world to that callee stays inside the
    /// component, and so spends one level of the caller's budget. The frame of
    /// a published callback is sized from this, because such a callback
    /// carries the budget through the frame the offer fills.
    pub(super) fn spends(&self, ordinal: u32, grain: Grain) -> bool {
        self.component_of.get(ordinal as usize).copied().flatten() == Some(grain.component)
    }

    /// The callee one member enters when its budget is spent: its own
    /// sequential clone, the world with no scheduler test in it.
    pub(super) fn exhausted(name: &str) -> String {
        sequential_clone_symbol(name)
    }
}
