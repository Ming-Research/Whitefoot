//! Shared activation-replacement conditions. Only a written [FN-10] marker
//! requires them; an unmarked call keeps ordinary lowering when they fail.

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, LexicalUseRole, NodePath, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssue, SemanticIssueKind, SemanticLocation, SemanticRule,
};

use super::super::model::CheckedMode;
use super::super::places::{PlaceRoot, ResolvedPlace};
use super::{CheckStop, Checker, FunctionSignature, LocalBinding};

impl Checker<'_> {
    pub(super) fn is_musttail_call(&self, node: NodeId) -> Result<bool, CheckStop> {
        for token in self.tree.direct_token_indices(node)? {
            if self.tree.token_bytes(*token)? == b"musttail" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn is_sole_return_call(&self, call: NodeId) -> Result<bool, CheckStop> {
        let expression = self
            .tree
            .parent(call)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let parent = self
            .tree
            .parent(expression)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        Ok(self.tree.production(expression)? == Production::Expr
            && self.tree.production(parent)? == Production::ReturnStmt
            && self.tree.children_with(parent, Production::Expr)?.len() == 1)
    }

    fn record_musttail_rejection(
        &self,
        node: NodeId,
        condition: &'static str,
        subject: Option<String>,
    ) -> Result<(), CheckStop> {
        self.musttail_rejections.borrow_mut().push(SemanticIssue {
            rule: SemanticRule::Fn10,
            location: SemanticLocation::SourceNode(
                self.tree.path(node)?.clone(),
                self.tree.coordinate(node)?,
            ),
            kind: SemanticIssueKind::InvalidMusttail { condition, subject },
            request: None,
        });
        Ok(())
    }

    /// A tail marker changes no ordinary call judgment. Collect its refusals
    /// without publishing a checked program, and let an earlier same-call
    /// rule win even when its proof is checked after the structural body pass.
    /// Errors at distinct nodes retain the ordinary checker's deterministic
    /// order. A later same-call rule, such as EFF-5, cannot hide FN-10 either.
    pub(super) fn finish_musttail_checks<T>(
        &self,
        result: Result<T, CheckStop>,
    ) -> Result<T, CheckStop> {
        let mut pending = std::mem::take(&mut *self.musttail_rejections.borrow_mut()).into_iter();
        match result {
            Ok(value) => match pending.next() {
                Some(issue) => Err(CheckStop::source_issue(issue)),
                None => Ok(value),
            },
            Err(CheckStop::Issue(ordinary)) => {
                let earlier = pending.find(|tail| {
                    tail.rule.definition_rank() < ordinary.rule.definition_rank()
                        && matches!(
                            (&tail.location, &ordinary.location),
                            (SemanticLocation::SourceNode(left, _), SemanticLocation::SourceNode(right, _))
                                if left == right
                        )
                });
                Err(match earlier {
                    Some(issue) => CheckStop::source_issue(issue),
                    None => CheckStop::Issue(ordinary),
                })
            }
            Err(stop) => Err(stop),
        }
    }

    /// Check the written position even in clauses and constants, where a call
    /// does not reach the ordinary expression checker.
    pub(super) fn check_musttail_positions(&self) -> Result<(), CheckStop> {
        for call in self
            .tree
            .descendants_with(self.tree.root(), Production::Call)?
        {
            if !self.is_musttail_call(call)? {
                continue;
            }
            if !self.is_sole_return_call(call)? {
                self.record_musttail_rejection(
                    call,
                    "musttail must be the sole expression of return",
                    None,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn check_musttail_callees(
        &self,
        function: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        for call in self
            .tree
            .descendants_with(function.node, Production::Call)?
        {
            if !self.is_musttail_call(call)? {
                continue;
            }
            if self.tree.is_constructor_call(call)? || self.behavior_call_key(call)?.is_some() {
                self.record_musttail_rejection(
                    call,
                    "musttail requires a direct call to the enclosing function",
                    None,
                )?;
                continue;
            }
            let callee = self
                .tree
                .first_child_with(call, Production::Callee)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let usage = self.use_at_roles(
                callee,
                &[
                    LexicalUseRole::IdentifierCallee,
                    LexicalUseRole::OperationCallee,
                ],
            )?;
            if !matches!(usage.target(), ResolvedTarget::Source { declaration, class: DeclarationClass::Function } if declaration == function.declaration)
            {
                self.record_musttail_rejection(
                    call,
                    "musttail requires a direct call to the enclosing function",
                    None,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn check_self_tail_arguments(
        &self,
        node: NodeId,
        function: &FunctionSignature,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        paths: &[Vec<ResolvedPlace>],
        modes: &[CheckedMode],
    ) -> Result<bool, CheckStop> {
        for (ordinal, (paths, mode)) in paths.iter().zip(modes).enumerate() {
            if !mode.is_reference() {
                continue;
            }
            if paths.is_empty()
                || paths.iter().any(|path| {
                    !function.parameters.iter().any(|parameter| {
                        parameter.mode.is_reference()
                            && bindings
                                .get(&parameter.declaration)
                                .is_some_and(|local| path.root == PlaceRoot::Binding(local.binding))
                    })
                })
            {
                if self.is_musttail_call(node)? {
                    self.record_musttail_rejection(node, "every musttail reference argument must be rooted at a reference parameter, not current-activation storage", Some(function.parameters[ordinal].name.clone()))?;
                }
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn check_self_tail_releases(
        &self,
        call: &NodePath,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<bool, CheckStop> {
        let mut owners = bindings
            .values()
            .filter(|local| local.live && local.mode == CheckedMode::Own)
            .collect::<Vec<_>>();
        owners.sort_by_key(|owner| std::cmp::Reverse(owner.binding.0));
        for owner in owners {
            if !self.has_nonempty_release(owner.ty)? {
                continue;
            }
            let referenced = bindings.values().any(|local| {
                local.live
                    && local.reference.as_ref().is_some_and(|reference| {
                        reference.is_valid()
                            && reference
                                .paths
                                .iter()
                                .any(|path| path.root == PlaceRoot::Binding(owner.binding))
                    })
            });
            if referenced {
                let node = self
                    .tree
                    .node_with_path(call)
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                if self.is_musttail_call(node)? {
                    let name = self
                        .resolved
                        .declarations()
                        .iter()
                        .find(|declaration| declaration.id() == owner.declaration)
                        .map(|declaration| declaration.spelling().to_owned());
                    self.record_musttail_rejection(
                        node,
                        "a live reference prevents releasing this owner before the musttail transfer",
                        name,
                    )?;
                }
                return Ok(false);
            }
        }
        Ok(true)
    }
}
