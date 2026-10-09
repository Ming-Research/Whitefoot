//! Atomic entry facts and TYPE-11 obligations on edges that leave the block.
use super::*;

pub(super) struct AtomicFacts {
    pub(super) clauses: Vec<CheckedRangeClause>,
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
        for clause in atomic.clauses.clone() {
            let frame = self.frame(&mut state.clone(), &clause, &|root| {
                binding_value(state, root)
            });
            self.require(state, &clause, &frame, node, "an atomic leaving edge");
        }
    }
}
