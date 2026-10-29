//! [SHARE-2] the one state an atomic statement holds shares the ordinary
//! storage/version machinery.
use super::*;
use crate::semantic::model::CheckedTarget;

impl Walker<'_> {
    pub(super) fn atomic_statement(
        &mut self,
        mut state: State,
        target: &CheckedTarget,
        node: &NodePath,
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        continues: bool,
    ) -> Option<State> {
        self.cite = node.clone();
        for handle in target.expressions() {
            let _ = self.eval(&mut state, handle);
        }
        state.havoc_everything(&mut self.world);
        self.unplaced(None, true, true);
        // The binding names the state, storage of no binding that no earlier
        // fact describes [SHARE-1].
        let mut held = state.clone();
        let view = View::Place(Location::root(Origin::Constructed(self.world.new_origin())));
        held.values.insert(target.binding, Value::Ref(view));
        self.enter_atomic(&mut held, node);
        if let Some(guard) = guard {
            let condition = match self.eval(&mut held, guard) {
                Value::Bool(condition, _) => condition,
                _ => Cond::Unknown,
            };
            if condition.excludes(true) {
                self.atomic = None;
                return join(&mut self.world, state.conds.len(), Vec::new());
            }
            condition.literals(true, &mut held.conds);
        }
        let after = self.block(held, body);
        if let Some(after) = &after {
            self.owe_atomic(after, node, true);
        }
        self.atomic = None;
        let mut afters = Vec::new();
        if continues && let Some(mut after) = after {
            after.havoc_everything(&mut self.world);
            afters.push(after);
        }
        join(&mut self.world, state.conds.len(), afters)
    }
}
