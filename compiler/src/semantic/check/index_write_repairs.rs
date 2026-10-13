//! Explanations for an indexed row whose value parameter was rewritten.

use std::collections::HashMap;

use super::{
    CheckContext, CheckStop, Checker, FunctionContext, FunctionSignature, LocalBinding, repairs,
};
use crate::semantic::model::{CheckedConst, CheckedMode, CheckedType, IntegerType};
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
        callee: Option<&FunctionSignature>,
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
            let move_before_write = self.can_move_before_index_write(
                context.function,
                node,
                write,
                &parameter,
                callee,
            )?;
            let call = callee.map(|_| tree.source_spelling(node)).transpose()?;
            return Ok(Some(repairs::rewritten_effect_index(
                &row,
                &parameter,
                &tree.source_spelling(write)?,
                line,
                &access,
                call.as_deref(),
                move_before_write,
            )));
        }
        Ok(None)
    }

    /// This is a sufficient diagnostic certificate, not a motion judgment.
    /// Only the first two direct body statements are considered, so no local,
    /// guard, invariant or call result can be established between the write
    /// and the operation. The assignment itself must establish no premise the
    /// operation needs: a direct bound or requirement must already be present
    /// at entry. More involved cases retain only the fresh-local repair.
    fn can_move_before_index_write(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        write: NodeId,
        parameter: &str,
        callee: Option<&FunctionSignature>,
    ) -> Result<bool, CheckStop> {
        let tree = &self.types.declarations.tree;
        let statements = tree.children_with(function.node, Production::Stmt)?;
        let [first, second, ..] = statements.as_slice() else {
            return Ok(false);
        };
        if tree.first_child_with(*first, Production::SetStmt)? != Some(write)
            || !function.substitution.entries().is_empty()
            || !function.held_parameters.is_empty()
        {
            return Ok(false);
        }
        let Some(value) = tree.first_child_with(write, Production::Expr)? else {
            return Ok(false);
        };
        if integer_literal_type(&tree.source_spelling(value)?).is_none() {
            return Ok(false);
        }
        let Some(requirements) = self.motion_requirements(function)? else {
            return Ok(false);
        };
        if let Some(callee) = callee {
            let Some(statement) = tree.first_child_with(*second, Production::ExprStmt)? else {
                return Ok(false);
            };
            if tree.first_child_with(statement, Production::Call)? != Some(node)
                || !callee.substitution.entries().is_empty()
                || callee.formal_parameter.is_some()
                || !callee.held_parameters.is_empty()
                || callee
                    .parameters
                    .iter()
                    .filter(|p| p.mode.is_reference())
                    .count()
                    > 1
                // One reference can still supply two indexed effects whose
                // EFF-5 separation depends on the value this write establishes.
                // This certificate admits no declared-row pair to prove.
                || callee
                    .declared_effects
                    .reads
                    .iter()
                    .chain(&callee.declared_effects.writes)
                    .nth(1)
                    .is_some()
            {
                return Ok(false);
            }
            let Some(arguments) = tree.first_child_with(node, Production::FieldinitList)? else {
                return Ok(false);
            };
            let arguments = tree.children_with(arguments, Production::Fieldinit)?;
            if arguments.len() != callee.parameters.len() {
                return Ok(false);
            }
            for (argument, formal) in arguments.iter().zip(&callee.parameters) {
                let Some(actual) = function.parameters.iter().find(|p| p.name == formal.name) else {
                    return Ok(false);
                };
                if actual.ty != formal.ty
                    || actual.mode != formal.mode
                    || !self.motion_parameter_type(actual.mode, actual.ty)?
                {
                    return Ok(false);
                }
                let Some(atom) = tree.first_child_with(*argument, Production::Atom)? else {
                    return Ok(false);
                };
                if tree.source_spelling(atom)? != actual.name {
                    return Ok(false);
                }
            }
            let Some(callee_requirements) = self.motion_requirements(callee)? else {
                return Ok(false);
            };
            return Ok(callee_requirements.iter().all(|requirement| {
                requirements.contains(requirement)
                    && callee.parameters.iter().any(|formal| {
                        formal.mode == CheckedMode::Own
                            && simple_parameter_requirement(requirement, &formal.name)
                    })
            }));
        }
        let Some(set) = tree.first_child_with(*second, Production::SetStmt)? else {
            return Ok(false);
        };
        if tree.first_child_with(set, Production::Place)? != Some(node) {
            return Ok(false);
        }
        let Some(value) = tree.first_child_with(set, Production::Expr)? else {
            return Ok(false);
        };
        let Some(value_type) = integer_literal_type(&tree.source_spelling(value)?) else {
            return Ok(false);
        };
        let access = tree.source_spelling(node)?;
        for root in &function.parameters {
            let CheckedType::Array {
                element,
                length: CheckedConst::Value(length),
            } = root.ty
            else {
                continue;
            };
            if root.mode == CheckedMode::Reference
                && access == format!("{}^[{parameter}]", root.name)
                && self.types.element_type(element)? == CheckedType::Integer(value_type)
                && requirements.contains(&format!("requires {parameter} < {length}_u64;"))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn motion_parameter_type(&self, mode: CheckedMode, ty: CheckedType) -> Result<bool, CheckStop> {
        Ok(match (mode, ty) {
            (
                CheckedMode::Own,
                CheckedType::Integer(_) | CheckedType::Bool | CheckedType::Unit,
            ) => true,
            (
                CheckedMode::Reference,
                CheckedType::Array {
                    element,
                    length: CheckedConst::Value(_),
                },
            ) => matches!(
                self.types.element_type(element)?,
                CheckedType::Integer(_) | CheckedType::Bool | CheckedType::Unit
            ),
            _ => false,
        })
    }

    fn motion_requirements(
        &self,
        function: &FunctionSignature,
    ) -> Result<Option<Vec<String>>, CheckStop> {
        let tree = &self.types.declarations.tree;
        let Some(contract) = tree.first_child_with(function.node, Production::ContractBlock)? else {
            return Ok(Some(Vec::new()));
        };
        if !tree
            .children_with(contract, Production::ContractDefine)?
            .is_empty()
        {
            return Ok(None);
        }
        tree.children_with(contract, Production::RequiresClause)?
            .into_iter()
            .map(|clause| Ok(tree.source_spelling(clause)?))
            .collect::<Result<Vec<_>, CheckStop>>()
            .map(Some)
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

fn integer_literal_type(spelling: &str) -> Option<IntegerType> {
    let (digits, suffix) = spelling.rsplit_once('_')?;
    let digits = digits.strip_prefix('-').unwrap_or(digits);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match suffix {
        "i8" => Some(IntegerType::I8),
        "i16" => Some(IntegerType::I16),
        "i32" => Some(IntegerType::I32),
        "i64" => Some(IntegerType::I64),
        "u8" => Some(IntegerType::U8),
        "u16" => Some(IntegerType::U16),
        "u32" => Some(IntegerType::U32),
        "u64" => Some(IntegerType::U64),
        _ => None,
    }
}

fn simple_parameter_requirement(requirement: &str, parameter: &str) -> bool {
    let Some(relation) = requirement.strip_prefix(&format!("requires {parameter} ")) else {
        return false;
    };
    ["< ", "<= ", "== ", ">= ", "> "].iter().any(|operator| {
        relation
            .strip_prefix(*operator)
            .and_then(|literal| literal.strip_suffix(';'))
            .and_then(integer_literal_type)
            .is_some()
    })
}
