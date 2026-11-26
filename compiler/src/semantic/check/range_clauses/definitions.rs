//! [FN-8, RANGE-1] expand erased definitions before forming range terms.

use super::*;
use crate::semantic::model::CheckedIntegerOperation;

impl Checker<'_, '_> {
    pub(super) fn range_definition(
        &self,
        declaration: DeclarationId,
    ) -> Result<Option<NodeId>, CheckStop> {
        let declaration = self
            .types
            .declarations
            .resolved
            .declaration(declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if declaration.role() != DeclarationRole::Let {
            return Ok(None);
        }
        let tree = &self.types.declarations.tree;
        let node = tree
            .node_with_path(declaration.origin().node())
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if tree.production(node)? != Production::ContractDefine {
            return Ok(None);
        }
        tree.first_child_with(node, Production::Expr)?
            .map(Some)
            .ok_or_else(|| SemanticCompilerFailure::InvalidCanonicalTree.into())
    }

    pub(super) fn range_definition_expression(
        &mut self,
        context: FunctionContext<'_, '_>,
        expression: NodeId,
        suffixes: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
        names: &RangeNames,
    ) -> Result<RangeOperand, CheckStop> {
        let tree = &self.types.declarations.tree;
        let atom = tree.first_child_with(expression, Production::Atom)?;
        let tail = tree.first_child_with(expression, Production::InfixTail)?;
        if let Some(atom) = atom {
            if let Some(tail) = tail {
                if !suffixes.is_empty() {
                    return self.invalid_range(
                        SemanticRule::Range1,
                        suffixes[0],
                        "a range term projects from an arithmetic expression",
                        "project from a value or an element selection",
                    );
                }
                let operator = self.types.declarations.infix_operator_node(tail)?;
                let operation = self.types.declarations.infix_operation(operator)?;
                if !matches!(
                    operation,
                    CheckedIntegerOperation::AddExact
                        | CheckedIntegerOperation::SubtractExact
                        | CheckedIntegerOperation::MultiplyExact
                ) {
                    return self.invalid_range(
                        SemanticRule::Range1,
                        operator,
                        "a range term uses an operation other than exact addition, subtraction or constant multiplication",
                        "write range terms with `+`, `-`, and multiplication by an integer literal or named const",
                    );
                }
                let right = self
                    .types
                    .declarations
                    .tree
                    .first_child_with(tail, Production::Atom)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let left = self
                    .range_atom(context, atom, bindings, names)?
                    .integer(self, atom, names)?;
                let right = self
                    .range_atom(context, right, bindings, names)?
                    .integer(self, right, names)?;
                return match operation {
                    CheckedIntegerOperation::AddExact => {
                        Ok(range_sum(vec![(1, left), (1, right)]).into())
                    }
                    CheckedIntegerOperation::SubtractExact => {
                        Ok(range_sum(vec![(1, left), (-1, right)]).into())
                    }
                    CheckedIntegerOperation::MultiplyExact => {
                        self.range_multiply(expression, left, right)
                    }
                    _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
                };
            }
            if suffixes.is_empty() {
                return self.range_atom(context, atom, bindings, names);
            }
            if let Some(place) = self
                .types
                .declarations
                .tree
                .first_child_with(atom, Production::Place)?
            {
                return self.range_place_with_suffixes(context, place, suffixes, bindings, names);
            }
        }
        self.invalid_range(
            SemanticRule::Range1,
            expression,
            "a contract definition expands to an expression that is not a range term",
            "define a range term from literals, consts, integer values, measures and element reads using `+`, `-` and constant multiplication",
        )
    }

    pub(super) fn range_multiply(
        &self,
        node: NodeId,
        left: CheckedRangeTerm,
        right: CheckedRangeTerm,
    ) -> Result<RangeOperand, CheckStop> {
        let (constant, value) = match (&left, &right) {
            (CheckedRangeTerm::Constant(constant), _) => (*constant, right),
            (_, CheckedRangeTerm::Constant(constant)) => (*constant, left),
            _ => {
                return self.invalid_range(
                    SemanticRule::Range1,
                    node,
                    "a range term multiplies two terms neither of which is a constant",
                    "multiply one term by an integer literal or a named integer const",
                );
            }
        };
        Ok(range_sum(vec![(constant, value)]).into())
    }
}
