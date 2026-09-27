use crate::semantic::check::FunctionContext;
use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{DeclarationId, Production, SemanticIssueKind, SemanticRule};

use super::super::super::super::model::{CheckedExpression, CheckedMode};
use super::super::super::{CheckStop, Checker, EffectSet, LocalBinding, TypedExpression};

impl<'unit> Checker<'_, 'unit> {
    pub(super) fn check_reinterpret(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        if self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::FieldinitList)?
            .is_some()
        {
            return self.types.declarations.issue_node(
                SemanticRule::Gram11,
                node,
                SemanticIssueKind::InvalidNamedArguments {
                    callee: "reinterpret".to_owned(),
                    declared_parameters: Vec::new(),
                },
            );
        }
        let [source, destination] = self.numeric_type_arguments(context, node, false)?;
        if !source.reinterprets_to(destination) {
            return self.types.declarations.issue_node(
                SemanticRule::Op1,
                node,
                SemanticIssueKind::InvalidOperation,
            );
        }
        let atoms = self.types.declarations.operation_atoms(node, 1)?;
        let atom = atoms[0];
        let argument = self.check_atom(context, atom, bindings, loop_depth)?;
        if argument.expression.ty() != source.ty() || argument.mode != CheckedMode::Own {
            return self.types.declarations.issue_node(
                SemanticRule::Type5,
                atom,
                SemanticIssueKind::type_mismatch(
                    format!("own {}", self.types.checked_type_name(source.ty())?),
                    self.types
                        .checked_value_name(argument.mode, argument.expression.ty())?,
                ),
            );
        }
        Ok(TypedExpression::owned(
            CheckedExpression::Reinterpret {
                carrier: self.types.declarations.tree.path(node)?.clone(),
                source,
                destination,
                value: Box::new(argument.expression),
            },
            EffectSet::NONE.union(argument.effects),
        ))
    }
}
