use crate::semantic::check::FunctionContext;
use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{DeclarationId, Production, SemanticCompilerFailure, SemanticIssueKind, SemanticRule};

use super::super::super::super::model::{
    CheckedConversionMode, CheckedExpression, CheckedMode, CheckedNumericType, CheckedType,
};
use super::super::super::{
    CheckStop, Checker, EffectSet, LocalBinding, PreludeType, TypedExpression,
};

impl<'unit> Checker<'_, 'unit> {
    pub(super) fn check_conversion(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        mode: CheckedConversionMode,
        spelling: &str,
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
                    callee: spelling.to_owned(),
                    declared_parameters: Vec::new(),
                },
            );
        }
        let [source, destination] = self.numeric_type_arguments(context, node, true)?;
        // [OP-6] endpoint domains: wrapping admits integer endpoints only, and
        // rounding admits any numeric source with a float destination.
        let integer = |endpoint: CheckedNumericType| {
            matches!(
                endpoint,
                CheckedNumericType::Integer(_) | CheckedNumericType::GenericInteger(_)
            )
        };
        let outside_endpoint_domain = match mode {
            CheckedConversionMode::Wrap => !integer(source) || !integer(destination),
            CheckedConversionMode::Nearest => integer(destination),
            CheckedConversionMode::Exact
            | CheckedConversionMode::Checked
            | CheckedConversionMode::Defined => false,
        };
        if outside_endpoint_domain {
            return self.types.declarations.issue_node(
                SemanticRule::Op1,
                node,
                SemanticIssueKind::InvalidOperation,
            );
        }
        let result = match mode {
            CheckedConversionMode::Exact
            | CheckedConversionMode::Wrap
            | CheckedConversionMode::Nearest => destination.ty(),
            CheckedConversionMode::Defined => CheckedType::Bool,
            CheckedConversionMode::Checked => {
                let error =
                    CheckedType::Nominal(self.types.prelude_nominal(PreludeType::NarrowError)?);
                CheckedType::Nominal(
                    self.types
                        .prelude_nominal(PreludeType::Result(destination.ty(), error))?,
                )
            }
        };
        let atom = self
            .types
            .declarations
            .operation_atoms(node, 1)?
            .into_iter()
            .next()
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
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
            CheckedExpression::NumericConversion {
                carrier: self.types.declarations.tree.path(node)?.clone(),
                mode,
                source,
                destination,
                value: Box::new(argument.expression),
                result,
            },
            EffectSet::NONE.union(argument.effects),
        ))
    }
}
