//! Explanations for an indexed row whose value parameter was rewritten.

use std::collections::HashMap;

use super::{CheckContext, CheckStop, Checker, FunctionContext, LocalBinding, repairs};
use crate::semantic::places::{CapturedTerm, PlaceStep, ResolvedPlace};
use crate::syntax::NodeId;
use crate::{DeclarationId, Production};

impl Checker<'_, '_> {
    /// Restoring a call-value classification on this private copy only asks
    /// whether the lost parameter identity explains the uncovered path. The
    /// original binding state and every acceptance result remain unchanged.
    pub(super) fn rewritten_index_repair(
        &self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        place: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        call: bool,
    ) -> Result<Option<String>, CheckStop> {
        if self.reference_row_writes(context.function, place, bindings)? {
            return Ok(None);
        }
        for step in &place.path {
            let PlaceStep::Index(captured) = step else {
                continue;
            };
            let CapturedTerm::Binding(binding) = captured.term else {
                continue;
            };
            let Some(local) = bindings.values().find(|local| local.binding == binding) else {
                continue;
            };
            if local.call_value
                || !context
                    .function
                    .parameters
                    .iter()
                    .any(|parameter| parameter.declaration == local.declaration)
            {
                continue;
            }
            let mut restored = bindings.clone();
            if let Some(local) = restored.get_mut(&local.declaration) {
                local.call_value = true;
            }
            if !self.reference_row_writes(context.function, place, &restored)? {
                continue;
            }
            let Some(write) = self.preceding_parameter_write(
                context.check_context,
                context.function.node,
                node,
                local.declaration,
            )? else {
                continue;
            };
            let tree = &self.types.declarations.tree;
            let (_, line) = tree.source_line(tree.path(write)?)?;
            let parameter = self
                .types
                .declarations
                .declaration_spelling(local.declaration)?;
            let row = self
                .types
                .render_effect_row(&context.function.declared_effects, context.function)?;
            let access = self.types.render_resolved_place(place, bindings)?;
            let call = call.then(|| tree.source_spelling(node)).transpose()?;
            return Ok(Some(repairs::rewritten_effect_index(
                &row,
                &parameter,
                &tree.source_spelling(write)?,
                line,
                &access,
                call.as_deref(),
            )));
        }
        Ok(None)
    }

    /// Only preceding direct statements in an enclosing statement list can
    /// supply the line. A write in a sibling arm, a later statement, or a
    /// previous loop iteration is never guessed from source order alone.
    fn preceding_parameter_write(
        &self,
        context: &CheckContext<'_>,
        function: NodeId,
        mut node: NodeId,
        parameter: DeclarationId,
    ) -> Result<Option<NodeId>, CheckStop> {
        let declarations = self.types.declarations;
        let tree = &declarations.tree;
        while node != function {
            let Some(parent) = tree.parent(node)? else {
                return Ok(None);
            };
            if tree.production(node)? == Production::Stmt {
                let preceding = tree
                    .children(parent)?
                    .iter()
                    .take_while(|child| **child != node)
                    .copied()
                    .collect::<Vec<_>>();
                for sibling in preceding.into_iter().rev() {
                    if tree.production(sibling)? != Production::Stmt {
                        continue;
                    }
                    let Some(set) = tree.first_child_with(sibling, Production::SetStmt)? else {
                        continue;
                    };
                    let Some(target) = tree.first_child_with(set, Production::Place)? else {
                        continue;
                    };
                    if tree.children_with(target, Production::Psuffix)?.is_empty()
                        && declarations.complete_binding_target(context, target)? == Some(parameter)
                    {
                        return Ok(Some(set));
                    }
                }
            }
            node = parent;
        }
        Ok(None)
    }
}
