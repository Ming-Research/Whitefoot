use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{DeclarationId, Production, SemanticIssueKind, SemanticRule};

use super::super::super::super::model::{CheckedExpression, CheckedMode};
use super::super::super::{
    CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, TypedExpression,
};

impl<'unit> Checker<'unit> {
    pub(super) fn check_reinterpret(
        &self,
        node: NodeId,
        function: &FunctionSignature,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        if self
            .tree
            .first_child_with(node, Production::FieldinitList)?
            .is_some()
        {
            return self.issue_node(
                SemanticRule::Gram11,
                node,
                SemanticIssueKind::InvalidNamedArguments {
                    callee: "reinterpret".to_owned(),
                    declared_parameters: Vec::new(),
                },
            );
        }
        let [source, destination] = self.numeric_type_arguments(node, function, false)?;
        if !source.reinterprets_to(destination) {
            return self.issue_node(SemanticRule::Op1, node, SemanticIssueKind::InvalidOperation);
        }
        let atoms = self.operation_atoms(node, 1)?;
        let atom = atoms[0];
        let argument = self.check_atom(function, atom, bindings, loop_depth)?;
        if argument.expression.ty() != source.ty() || argument.mode != CheckedMode::Own {
            return self.issue_node(
                SemanticRule::Type5,
                atom,
                SemanticIssueKind::type_mismatch(
                    format!("own {}", self.checked_type_name(source.ty())?),
                    self.checked_value_name(argument.mode, argument.expression.ty())?,
                ),
            );
        }
        Ok(TypedExpression::owned(
            CheckedExpression::Reinterpret {
                carrier: self.tree.path(node)?.clone(),
                source,
                destination,
                value: Box::new(argument.expression),
            },
            EffectSet::NONE.union(argument.effects),
        ))
    }
}
