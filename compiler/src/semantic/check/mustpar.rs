//! [PAR-4] the `mustpar` marker: its position, its waiting form's conditions,
//! and the validation of its first two forms against the finished permission
//! table. The marker adds no permission; it only requires one.

use crate::syntax::NodeId;
use crate::{
    FixedTerminal, NodePath, Production, SemanticCompilerFailure, SemanticIssue,
    SemanticIssueKind, SemanticLocation, SemanticRule, TerminalPredicate,
};

use super::super::loop_permission::LoopVerdict;
use super::super::model::{CheckedFunction, CheckedMustpar};
use super::super::permission::{FunctionPermissions, PermissionVerdict};
use super::super::permission_ledger::{denied_detail, loop_denied_detail};
use super::{CheckStop, Checker, FunctionSignature, PermissionLedgerSource};

/// Where a marked call stands, read from its parents.
enum CallPosition {
    /// The call of an `expr_stmt`: the statement node.
    ExpressionStatement(NodeId),
    /// The call of an `ordinary_let_rhs`: the `let_stmt` node.
    LetRightHandSide(NodeId),
    /// Anywhere else.
    Other,
}

impl Checker<'_, '_, '_, '_> {
    pub(super) fn is_mustpar_marked(&self, node: NodeId) -> Result<bool, CheckStop> {
        Ok(self
            .tree
            .direct_token_with(node, TerminalPredicate::Fixed(FixedTerminal::Mustpar))?
            .is_some())
    }

    fn call_position(&self, call: NodeId) -> Result<CallPosition, CheckStop> {
        let parent = self
            .tree
            .parent(call)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        match self.tree.production(parent)? {
            Production::ExprStmt => Ok(CallPosition::ExpressionStatement(parent)),
            Production::Expr => {
                let Some(owner) = self.tree.parent(parent)? else {
                    return Ok(CallPosition::Other);
                };
                if self.tree.production(owner)? != Production::OrdinaryLetRhs {
                    return Ok(CallPosition::Other);
                }
                let statement = self
                    .tree
                    .parent(owner)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                Ok(CallPosition::LetRightHandSide(statement))
            }
            _ => Ok(CallPosition::Other),
        }
    }

    fn invalid_mustpar<Value>(&self, node: NodeId, condition: &str) -> Result<Value, CheckStop> {
        self.issue_node(
            SemanticRule::Par4,
            node,
            SemanticIssueKind::InvalidMustpar {
                condition: condition.to_owned(),
            },
        )
    }

    /// Reject a marked call written where no form of [PAR-4] admits one:
    /// outside a statement's own call, including a contract clause and a
    /// constant, which the body check never reaches.
    pub(super) fn check_mustpar_positions(&self) -> Result<(), CheckStop> {
        for call in self
            .tree
            .descendants_with(self.tree.root(), Production::Call)?
        {
            if self.is_mustpar_marked(call)?
                && matches!(self.call_position(call)?, CallPosition::Other)
            {
                return self.invalid_mustpar(
                    call,
                    "mustpar marks the call of an expression statement or of an ordinary let right-hand side",
                );
            }
        }
        Ok(())
    }

    /// [PAR-4] form 3, at a marked call whose selected callee waits: the
    /// call stands as an expression statement, takes every argument by value
    /// and returns a droppable result, so the started context shares no
    /// storage with the context that starts it.
    pub(super) fn check_waiting_mustpar(
        &self,
        node: NodeId,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        let CallPosition::ExpressionStatement(statement) = self.call_position(node)? else {
            return self.invalid_mustpar(
                node,
                "a mustpar call whose callee waits is the call of an expression statement",
            );
        };
        if let Some(parameter) = signature
            .parameters
            .iter()
            .find(|parameter| parameter.mode.is_reference())
        {
            return self.invalid_mustpar(
                node,
                &format!(
                    "every parameter of a waiting callee started by mustpar is a value parameter, and `{}` is a reference parameter",
                    parameter.name
                ),
            );
        }
        if self
            .linear_release_obligation(signature.result)?
            .is_some()
        {
            return self.invalid_mustpar(
                node,
                "the result of a waiting callee started by mustpar has the drop capability",
            );
        }
        let path = self.tree.path(statement)?.clone();
        self.waiting.borrow_mut().context_starts.push(path);
        Ok(())
    }

    /// Record every `mustpar` of one body that its first two forms prove
    /// against the permission table: a marked `for_stmt`, and a marked call
    /// whose callee does not wait. A waiting callee's marker was judged when
    /// its call was checked.
    pub(super) fn collect_mustpar_markers(
        &self,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        let mut independent = Vec::new();
        for node in self.tree.descendants_with(signature.node, Production::ForStmt)? {
            if self.is_mustpar_marked(node)? {
                let path = self.tree.path(node)?.clone();
                independent.push(CheckedMustpar {
                    statement: path.clone(),
                    marker: path,
                    counted_loop: true,
                });
            }
        }
        for call in self.tree.descendants_with(signature.node, Production::Call)? {
            if !self.is_mustpar_marked(call)? {
                continue;
            }
            let marker = self.tree.path(call)?.clone();
            if self.waiting.borrow().calls.contains(&marker) {
                continue;
            }
            let statement = match self.call_position(call)? {
                CallPosition::ExpressionStatement(statement)
                | CallPosition::LetRightHandSide(statement) => statement,
                CallPosition::Other => {
                    return self.invalid_mustpar(
                        call,
                        "mustpar marks the call of an expression statement or of an ordinary let right-hand side",
                    );
                }
            };
            independent.push(CheckedMustpar {
                statement: self.tree.path(statement)?.clone(),
                marker,
                counted_loop: false,
            });
        }
        independent.sort_by(|left, right| left.marker.components().cmp(right.marker.components()));
        self.waiting.borrow_mut().independent = independent;
        Ok(())
    }

    /// [PAR-4] forms 1 and 2 against the finished permission table: the
    /// first marker in source order whose statement the judgment does not
    /// permit is refused, carrying the denial the ledger would print.
    pub(super) fn validate_mustpar(
        &self,
        functions: &[CheckedFunction],
        permissions: &[FunctionPermissions],
    ) -> Result<(), CheckStop> {
        let source = PermissionLedgerSource { tree: &self.tree };
        let mut refused: Vec<(NodePath, String)> = Vec::new();
        for (function, table) in functions.iter().zip(permissions) {
            for marked in &function.waiting.independent {
                let condition = if marked.counted_loop {
                    match table
                        .loops
                        .iter()
                        .find(|judged| judged.statement == marked.statement)
                        .map(|judged| &judged.verdict)
                    {
                        Some(LoopVerdict::PermittedEligible) => continue,
                        Some(LoopVerdict::Denied(denial)) => format!(
                            "PAR-2 permission for the marked loop is denied: {}",
                            loop_denied_detail(denial, &source)?
                        ),
                        None => "PAR-2 judges the marked loop".to_owned(),
                    }
                } else {
                    match table
                        .marked
                        .iter()
                        .find(|(statement, _)| *statement == marked.statement)
                        .map(|(_, verdict)| verdict)
                    {
                        Some(Some(PermissionVerdict::PermittedEligible)) => continue,
                        Some(Some(PermissionVerdict::Denied(denial))) => format!(
                            "PAR-1 permission for the marked statement and the next is denied: {}",
                            denied_detail(denial, &source)?
                        ),
                        Some(None) | None => {
                            "a next statement of the same block follows the marked statement"
                                .to_owned()
                        }
                    }
                };
                refused.push((marked.marker.clone(), condition));
            }
        }
        refused.sort_by(|left, right| left.0.components().cmp(right.0.components()));
        let Some((marker, condition)) = refused.into_iter().next() else {
            return Ok(());
        };
        let node = self
            .tree
            .node_with_path(&marker)
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        Err(CheckStop::source_issue(SemanticIssue {
            rule: SemanticRule::Par4,
            location: SemanticLocation::SourceNode(marker, self.tree.coordinate(node)?),
            kind: SemanticIssueKind::InvalidMustpar { condition },
            request: None,
        }))
    }
}
