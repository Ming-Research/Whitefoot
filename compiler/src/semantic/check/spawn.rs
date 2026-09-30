//! [WAIT-3] spawns: a `spawn` stands as the call of an expression statement
//! or of an ordinary `let` right-hand side, and its callee waits, takes every
//! argument by value and, for a discarded result, has the drop capability.
//! Every admitted spawn starts a context [WAIT-2]; a call that is not spawned
//! executes in its caller's context.

use crate::syntax::NodeId;
use crate::{
    FixedTerminal, Production, SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
    TerminalPredicate,
};

use super::{CheckContext, CheckStop, Checker, DeclarationInventory, FunctionSignature};

/// Where a call stands, read from its parents.
enum CallPosition {
    /// The call of an `expr_stmt`: the statement node.
    ExpressionStatement(NodeId),
    /// The call of an `ordinary_let_rhs`: the `let_stmt` node.
    LetRightHandSide(NodeId),
    /// Anywhere else.
    Other,
}

/// The condition a spawn written anywhere but a statement's own call fails.
const SPAWN_POSITION: &str =
    "a spawn is the call of an expression statement or of an ordinary let right-hand side";

impl DeclarationInventory<'_> {
    pub(super) fn is_spawn(&self, node: NodeId) -> Result<bool, CheckStop> {
        Ok(self
            .tree
            .direct_token_with(node, TerminalPredicate::Fixed(FixedTerminal::Spawn))?
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

    fn invalid_spawn<Value>(&self, node: NodeId, condition: &str) -> Result<Value, CheckStop> {
        self.issue_node(
            SemanticRule::Wait3,
            node,
            SemanticIssueKind::InvalidSpawn {
                condition: condition.to_owned(),
            },
        )
    }
}

impl Checker<'_, '_> {
    /// Reject a spawn written where [WAIT-3] admits none: outside a
    /// statement's own call, including a contract clause and a constant,
    /// which the body check never reaches.
    pub(super) fn check_spawn_positions(&mut self) -> Result<(), CheckStop> {
        for call in self
            .types
            .declarations
            .tree
            .descendants_with(self.types.declarations.tree.root(), Production::Call)?
        {
            if self.types.declarations.is_spawn(call)?
                && matches!(
                    self.types.declarations.call_position(call)?,
                    CallPosition::Other
                )
            {
                return self.types.declarations.invalid_spawn(call, SPAWN_POSITION);
            }
        }
        Ok(())
    }

    /// [WAIT-3] at a spawn whose selected callee waits: the call stands as an
    /// expression statement or an ordinary `let` right-hand side and takes
    /// every argument by value, so the started context shares no storage
    /// with its starter. A discarded result must also have the drop
    /// capability, since the context releases it; a bound one is the
    /// binding's from the join on.
    pub(super) fn check_spawn(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        let (statement, bound) = match self.types.declarations.call_position(node)? {
            CallPosition::ExpressionStatement(statement) => (statement, false),
            CallPosition::LetRightHandSide(statement) => (statement, true),
            CallPosition::Other => {
                return self.types.declarations.invalid_spawn(node, SPAWN_POSITION);
            }
        };
        if let Some(parameter) = signature
            .parameters
            .iter()
            .find(|parameter| parameter.mode.is_reference())
        {
            return self.types.declarations.invalid_spawn(
                node,
                &format!(
                    "every parameter of a spawned callee is a value parameter, and `{}` is a reference parameter",
                    parameter.name
                ),
            );
        }
        if !bound
            && self
                .types
                .linear_release_obligation(check_context, signature.result)?
                .is_some()
        {
            return self.types.declarations.invalid_spawn(
                node,
                "the discarded result of a spawned callee has the drop capability",
            );
        }
        let path = self.types.declarations.tree.path(statement)?.clone();
        self.body.waiting.context_starts.push(path);
        Ok(())
    }

    /// [WAIT-3] every spawn of one body calls a waiting callee: a spawn the
    /// body check did not record as a waiting call names a function, an
    /// operation or a constructor that does not wait, which would start no
    /// context anything could observe.
    pub(super) fn check_spawned_callees_wait(
        &mut self,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        for call in self
            .types
            .declarations
            .tree
            .descendants_with(signature.node, Production::Call)?
        {
            if !self.types.declarations.is_spawn(call)? {
                continue;
            }
            let path = self.types.declarations.tree.path(call)?;
            if self.body.waiting.calls.contains(path) {
                continue;
            }
            return self
                .types
                .declarations
                .invalid_spawn(call, "the callee of a spawn waits");
        }
        Ok(())
    }
}
