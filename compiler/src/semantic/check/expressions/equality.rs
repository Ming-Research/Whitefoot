//! [OP-16] operand agreement and the structural equality-type judgment.

use std::collections::{HashMap, HashSet};

use super::super::{
    CheckContext, CheckStop, FunctionContext, LocalBinding, TypeContext, TypedExpression,
};
use crate::semantic::model::{
    CheckedIntegerOperation, CheckedMode, CheckedNominalKind, CheckedType,
};
use crate::syntax::NodeId;
use crate::syntax::terminal::TerminalPredicate;
use crate::{DeclarationId, Production, SemanticCompilerFailure, SemanticIssueKind, SemanticRule};

impl TypeContext<'_> {
    /// Reject a non-equality part before an affine operand can be consumed or
    /// rejected for its bare spelling. This reads only declared types; the
    /// ordinary operand judgment still checks every admitted use exactly once.
    pub(in crate::semantic::check) fn preflight_value_equality(
        &self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        operation: CheckedIntegerOperation,
        operands: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        if !matches!(
            operation,
            CheckedIntegerOperation::Equal | CheckedIntegerOperation::NotEqual
        ) {
            return Ok(());
        }
        let mut selected = None;
        for &operand in operands {
            if let Some(ty) = self.equality_operand_type(context, operand, bindings)? {
                if let Some(expected) = selected {
                    if ty != expected {
                        return self.declarations.issue_node(
                            SemanticRule::Type5,
                            operand,
                            SemanticIssueKind::type_mismatch(
                                format!("own {}", self.checked_type_name(expected)?),
                                format!("own {}", self.checked_type_name(ty)?),
                            ),
                        );
                    }
                } else {
                    selected = Some(ty);
                }
            }
        }
        if let Some(ty) = selected {
            self.require_equality_type(node, ty)?;
        }
        Ok(())
    }

    fn equality_operand_type(
        &self,
        context: FunctionContext<'_, '_>,
        mut node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<CheckedType>, CheckStop> {
        loop {
            match self.declarations.tree.production(node)? {
                Production::Place => {
                    return self.equality_place_type(context.check_context, node, bindings);
                }
                Production::Atom => {
                    if let Some(value) =
                        self.postcondition_result_placeholder(context.check_context, node)?
                    {
                        return Ok(Some(value.ty()));
                    }
                    if let Some(place) = self
                        .declarations
                        .tree
                        .first_child_with(node, Production::Place)?
                    {
                        return self.equality_place_type(context.check_context, place, bindings);
                    }
                    if let Some(literal) = self
                        .declarations
                        .tree
                        .direct_token_with(node, TerminalPredicate::Literal)?
                    {
                        if matches!(
                            self.declarations.tree.token_bytes(literal)?,
                            b"0_T" | b"1_T"
                        ) {
                            let one = self.declarations.tree.token_bytes(literal)? == b"1_T";
                            return Ok(Some(
                                self.check_generic_numeric_identity(context, node, one)?
                                    .expression
                                    .ty(),
                            ));
                        }
                        return Ok(Some(self.declarations.parse_literal(node, literal)?.ty()));
                    }
                    return Ok(None);
                }
                Production::AffineExpr | Production::AffineTerm | Production::AffineFactor => {
                    let [child] = self.declarations.tree.children(node)? else {
                        return Ok(None);
                    };
                    node = *child;
                }
                _ => return Ok(None),
            }
        }
    }

    fn equality_place_type(
        &self,
        context: &CheckContext<'_>,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<CheckedType>, CheckStop> {
        if let Some(value) = self.postcondition_result_placeholder(context, place)? {
            return Ok(Some(value.ty()));
        }
        let base = self
            .declarations
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // The body-only type oracle deliberately does not form entry(...) or
        // variant payload paths. Their owning operand judgment runs below;
        // preflight must not introduce an unsupported-capability stop first.
        if !self.declarations.tree.children(base)?.is_empty() {
            return Ok(None);
        }
        for suffix in self
            .declarations
            .tree
            .children_with(place, Production::Psuffix)?
        {
            if self
                .declarations
                .tree
                .direct_token_with(suffix, TerminalPredicate::TypeIdentifier)?
                .is_some()
            {
                return Ok(None);
            }
        }
        self.place_selected_type(context, place, bindings)
    }

    pub(in crate::semantic::check) fn check_equality_operand_types(
        &self,
        node: NodeId,
        operands: &[(NodeId, TypedExpression)],
    ) -> Result<(), CheckStop> {
        let [(left_node, left), (right_node, right)] = operands else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let selected = left.expression.ty();
        if right.expression.ty() != selected || right.mode != left.mode {
            return self.declarations.issue_node(
                SemanticRule::Type5,
                *right_node,
                SemanticIssueKind::type_mismatch(
                    self.checked_value_name(left.mode, selected)?,
                    self.checked_value_name(right.mode, right.expression.ty())?,
                ),
            );
        }
        if left.mode != CheckedMode::Own {
            return self.declarations.issue_node(
                SemanticRule::Op1,
                *left_node,
                SemanticIssueKind::InvalidEqualityType {
                    mechanical_fix: super::super::repairs::value_equality_repair(
                        &self.checked_value_name(left.mode, selected)?,
                        "reference kind",
                    ),
                },
            );
        }
        self.require_equality_type(node, selected)
    }

    fn require_equality_type(&self, node: NodeId, ty: CheckedType) -> Result<(), CheckStop> {
        if let Some((part, rejected)) = self.first_non_equality_part(ty)? {
            return self.declarations.issue_node(
                SemanticRule::Op1,
                node,
                SemanticIssueKind::InvalidEqualityType {
                    mechanical_fix: super::super::repairs::value_equality_repair(
                        &self.checked_type_name(rejected)?,
                        &part,
                    ),
                },
            );
        }
        Ok(())
    }

    /// Instances in the inventory already have their arguments substituted.
    /// Reverse insertion makes this depth-first walk visit fields, variants
    /// and payload fields in declaration order, including private fields.
    fn first_non_equality_part(
        &self,
        ty: CheckedType,
    ) -> Result<Option<(String, CheckedType)>, CheckStop> {
        let mut pending = vec![(String::from("operand type"), ty)];
        let mut visited = HashSet::new();
        while let Some((path, ty)) = pending.pop() {
            match ty {
                CheckedType::Unit
                | CheckedType::Bool
                | CheckedType::Integer(_)
                | CheckedType::GenericInt(_) => {}
                CheckedType::Array { element, .. } => {
                    pending.push((format!("{path}, element type"), self.element_type(element)?));
                }
                CheckedType::Nominal(id) => {
                    let nominal = self.nominal(id)?;
                    if nominal.linear || nominal.nocopy {
                        return Ok(Some((
                            format!("{path} (declaration removes a capability)"),
                            ty,
                        )));
                    }
                    if !visited.insert(id) {
                        continue;
                    }
                    match &nominal.kind {
                        CheckedNominalKind::Struct { fields } => {
                            for field in fields.iter().rev() {
                                pending.push((format!("{path}, field `{}`", field.name), field.ty));
                            }
                        }
                        CheckedNominalKind::Enum { variants } => {
                            for variant in variants.iter().rev() {
                                for field in variant.fields.iter().rev() {
                                    pending.push((
                                        format!(
                                            "{path}, payload field `{}.{}`",
                                            variant.name, field.name
                                        ),
                                        field.ty,
                                    ));
                                }
                            }
                        }
                        CheckedNominalKind::Opaque
                        | CheckedNominalKind::Box { .. }
                        | CheckedNominalKind::Shared { .. } => return Ok(Some((path, ty))),
                    }
                }
                _ => return Ok(Some((path, ty))),
            }
        }
        Ok(None)
    }
}
