//! Diagnostic read origins attached to evaluations and current scalar bindings.
use super::*;

impl Walker<'_> {
    /// What each clause place and value denotes in this state. Root values
    /// and their diagnostic read origins come from the same source scope.
    pub(super) fn frame(
        &mut self,
        state: &mut State,
        clause: &CheckedRangeClause,
        roots: &dyn Fn(CheckedRangeRoot) -> Option<Value>,
        root_reads: &dyn Fn(CheckedRangeRoot) -> Vec<facts::ReadProvenance>,
    ) -> Frame {
        let mut frame = Frame::default();
        let mut segments: BTreeSet<CheckedRangePlace> = BTreeSet::new();
        let mut terms: Vec<&CheckedRangeTerm> = Vec::new();
        for binder in &clause.binders {
            terms.push(&binder.start);
            terms.push(&binder.end);
        }
        for relation in clause.relations() {
            terms.push(&relation.left);
            terms.push(&relation.right);
        }
        for term in &terms {
            segment_places(term, &mut segments);
        }
        for place in clause.places() {
            let view = self.place_view(state, &place, segments.contains(&place), roots);
            frame.places.insert(place, view);
        }
        let mut values = Vec::new();
        for term in &terms {
            term.collect_values(&mut values);
        }
        values.sort();
        values.dedup();
        for root in values {
            match roots(root) {
                Some(Value::Int(value)) => {
                    frame.values.insert(root, value);
                    frame.value_reads.insert(root, root_reads(root));
                }
                Some(
                    value @ (Value::Owned(_)
                    | Value::Struct(_)
                    | Value::Variant { .. }
                    | Value::Bool(..)),
                ) => {
                    // Symbolic type-parameter values still form integer terms.
                    frame.values.insert(root, self.world.opaque(None));
                    let version = self.snapshot_aggregate(state, &value);
                    frame.aggregates.insert(root, version);
                }
                _ => {
                    frame.values.insert(root, self.world.opaque(None));
                }
            }
        }
        frame
    }

    /// Local range roots use exactly the bindings of their source state.
    pub(super) fn binding_frame(
        &mut self,
        state: &mut State,
        clause: &CheckedRangeClause,
        source: &State,
    ) -> Frame {
        self.frame(
            state,
            clause,
            &|root| binding_value(source, root),
            &|root| match root {
                CheckedRangeRoot::Binding(binding) => source
                    .binding_reads
                    .get(&binding)
                    .cloned()
                    .unwrap_or_default(),
                _ => Vec::new(),
            },
        )
    }

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
