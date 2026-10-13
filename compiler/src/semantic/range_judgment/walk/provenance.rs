//! Diagnostic read origins attached to evaluations and current scalar bindings.
use super::*;

impl Walker<'_> {
    pub(super) fn eval(&mut self, state: &mut State, expression: &CheckedExpression) -> Value {
        let previous = self.goal_reads.replace(Vec::new());
        let value = self.eval_inner(state, expression);
        self.last_reads = self.finish_read_scope(previous);
        value
    }

    pub(super) fn finish_read_scope(
        &mut self,
        previous: Option<Vec<facts::ReadProvenance>>,
    ) -> Vec<facts::ReadProvenance> {
        let reads = self.goal_reads.take().unwrap_or_default();
        self.goal_reads = previous.map(|mut parent| {
            parent.extend(reads.iter().cloned());
            parent
        });
        reads
    }

    pub(super) fn read_binding_provenance(&mut self, state: &State, binding: BindingId) {
        if let Some(reads) = &mut self.goal_reads
            && let Some(stored) = state.binding_reads.get(&binding)
        {
            reads.extend(stored.iter().cloned());
        }
    }

    pub(super) fn bind_provenance(
        state: &mut State,
        binding: BindingId,
        reads: Vec<facts::ReadProvenance>,
    ) {
        if matches!(
            state.values.get(&binding),
            Some(Value::Int(_) | Value::Bool(..))
        ) && !reads.is_empty()
        {
            state.binding_reads.insert(binding, reads);
        }
    }

    pub(super) fn affine_with_provenance(
        &mut self,
        state: &mut State,
        relation: &CheckedAffineRelation,
    ) -> (
        Result<Vec<Literal>, GoalFailure>,
        Vec<facts::ReadProvenance>,
    ) {
        let previous = self.goal_reads.replace(Vec::new());
        let goals = self.affine_relation(state, relation);
        let reads = self.finish_read_scope(previous);
        (goals, reads)
    }
}
