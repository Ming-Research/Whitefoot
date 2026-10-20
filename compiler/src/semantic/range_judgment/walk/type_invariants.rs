//! Atomic entry facts and TYPE-11 obligations on edges that leave the block.
use super::*;

pub(super) struct AtomicFacts {
    pub(super) clauses: Vec<CheckedRangeClause>,
    /// Target references as the block entered: rebinding does not release
    /// the original object or move its leaving-edge obligation.
    pub(super) bindings: BTreeMap<BindingId, Value>,
    pub(super) loops: Vec<CheckedLoopId>,
    pub(super) gives: usize,
}

impl Walker<'_> {
    pub(super) fn enter_atomic(&mut self, state: &mut State, node: &NodePath) {
        let clauses = self
            .function
            .range_facts
            .atomics
            .get(node)
            .cloned()
            .unwrap_or_default();
        for clause in &clauses {
            let frame = self.frame(&mut state.clone(), clause, &|root| {
                binding_value(state, root)
            });
            let fact = self.add_fact(clause.clone(), frame);
            state.facts.push(fact);
        }
        self.atomic = Some(AtomicFacts {
            clauses,
            bindings: state.values.clone(),
            loops: self.active_loops.clone(),
            gives: self.gives.len(),
        });
    }

    pub(super) fn owe_atomic(&mut self, state: &State, node: &NodePath, leaves: bool) {
        if !leaves {
            return;
        }
        let Some(atomic) = &self.atomic else {
            return;
        };
        let bindings = atomic.bindings.clone();
        for clause in atomic.clauses.clone() {
            let frame = self.frame(&mut state.clone(), &clause, &|root| match root {
                CheckedRangeRoot::Binding(binding) => bindings.get(&binding).cloned(),
                _ => None,
            });
            self.require(state, &clause, &frame, node, "an atomic leaving edge");
        }
    }
}
