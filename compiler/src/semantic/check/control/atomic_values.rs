//! Direct atomic-value use and one operation on each statement path [SHARE-2].

use super::super::super::model::{
    BindingId, CheckedExpression, CheckedSetTarget, CheckedStatement, CheckedTarget,
    CheckedTargetKind, expression_children,
};
use super::super::LocalBinding;
use super::super::{CheckStop, Checker};
use crate::syntax::NodeId;
use crate::{
    DeclarationId, NodePath, ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind,
    SemanticRule,
};
use std::collections::HashMap;

/// Only the paths that continue can meet the next operation. A `give` leaves
/// an arm and resumes its own value initializer; a return or loop transfer
/// leaves this flow altogether. Counts saturate by rejection at the second call.
#[derive(Default)]
struct Paths {
    next: Option<u8>,
    gives: Option<u8>,
}

fn join(left: Option<u8>, right: Option<u8>) -> Option<u8> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (left, right) => left.or(right),
    }
}

impl Checker<'_, '_> {
    /// Whether `expression` names one of the enclosing atomic statements'
    /// atomic-value bindings, the only occurrences SHARE-2 restricts.
    pub(in crate::semantic::check) fn mentions_atomic_value_root(
        &self,
        expression: &CheckedExpression,
    ) -> bool {
        self.body
            .atomic_value_roots
            .iter()
            .any(|root| super::atomic::expression_mentions(expression, *root))
    }

    /// Guards and erased proofs never acquire a readable scalar from the
    /// atomic-value state. Refuse the source use before ordinary typing.
    pub(in crate::semantic::check) fn reject_atomic_value_names(
        &self,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        reason: &'static str,
        mechanical_fix: &'static str,
    ) -> Result<(), CheckStop> {
        if self.body.atomic_value_roots.is_empty() {
            return Ok(());
        }
        let owner = self.types.declarations.tree.path(node)?.components();
        for usage in self.types.declarations.resolved.lexical_uses() {
            if usage.origin().node().components().starts_with(owner)
                && let ResolvedTarget::Source { declaration, .. } = usage.target()
                && let Some(local) = bindings.get(&declaration)
                && self.body.atomic_value_roots.contains(&local.binding)
            {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    node,
                    SemanticIssueKind::AtomicValueUse {
                        reason,
                        mechanical_fix,
                    },
                );
            }
        }
        Ok(())
    }

    pub(super) fn check_atomic_value_paths(
        &self,
        targets: &[CheckedTarget],
        guard: Option<&CheckedExpression>,
        statements: &[CheckedStatement],
    ) -> Result<(), CheckStop> {
        let bindings = targets
            .iter()
            .filter(|target| matches!(target.kind, CheckedTargetKind::AtomicValue))
            .map(|target| target.binding)
            .collect::<Vec<_>>();
        if bindings.is_empty() {
            return Ok(());
        }
        if let Some(guard) = guard {
            for target in targets {
                if bindings.contains(&target.binding)
                    && super::atomic::expression_mentions(guard, target.binding)
                {
                    return self.atomic_value_issue(
                        &target.node_path,
                        "an atomic-value target binding cannot occur in a guard",
                        "read the atomic value once in the block; use ordinary shared state for a waiting guard",
                    );
                }
            }
        }
        self.atomic_value_block(statements, &bindings, 0, false)?;
        Ok(())
    }

    fn atomic_value_issue<T>(
        &self,
        path: &NodePath,
        reason: &'static str,
        mechanical_fix: &'static str,
    ) -> Result<T, CheckStop> {
        let node = self
            .types
            .declarations
            .tree
            .node_with_path(path)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.types.declarations.issue_node(
            SemanticRule::Share2,
            node,
            SemanticIssueKind::AtomicValueUse {
                reason,
                mechanical_fix,
            },
        )
    }

    fn atomic_value_expression(
        &self,
        expression: &CheckedExpression,
        bindings: &[BindingId],
        count: u8,
        in_loop: bool,
        site: &NodePath,
    ) -> Result<u8, CheckStop> {
        if let CheckedExpression::UserCall {
            function,
            arguments,
            call,
            ..
        } = expression
        {
            let signature = &self.types.signatures[function.0 as usize];
            if matches!(
                signature.name.as_str(),
                "atomic_load" | "atomic_add" | "atomic_raise"
            ) && self
                .types
                .declarations
                .tree
                .is_prelude_node(signature.node)?
            {
                if in_loop {
                    return self.atomic_value_issue(
                        call,
                        "an atomic-value operation cannot occur inside a loop of the atomic block",
                        "perform the operation outside the loop, or make each iteration its own atomic statement",
                    );
                }
                if count != 0 {
                    return self.atomic_value_issue(
                        call,
                        "a path through this statement performs more than one atomic-value operation",
                        "keep at most one operation across all atomic-value targets on each path, or use separate atomic statements",
                    );
                }
                // The ordinary call checker established that the first argument
                // is this statement's direct atomic-value binder.
                // An argument is an atom [GRAM-9], so it holds no nested
                // operation; each starts from count 0.
                for argument in arguments.iter().skip(1) {
                    self.atomic_value_expression(argument, bindings, 0, in_loop, call)?;
                }
                return Ok(1);
            }
        }
        // An atomic-value binding is never an alias, helper argument, borrowed
        // place, storage projection or another expression's value.
        if bindings
            .iter()
            .any(|binding| super::atomic::expression_mentions(expression, *binding))
        {
            return self.atomic_value_issue(
                site,
                "an atomic-value target binding is used outside the cell argument of its direct operation",
                "pass the header binding only as `cell: binding` to one `atomic_load`, `atomic_add` or `atomic_raise` call",
            );
        }
        let mut count = count;
        for child in expression_children(expression) {
            count = self.atomic_value_expression(child, bindings, count, in_loop, site)?;
        }
        Ok(count)
    }

    fn atomic_value_block(
        &self,
        statements: &[CheckedStatement],
        bindings: &[BindingId],
        initial: u8,
        in_loop: bool,
    ) -> Result<Paths, CheckStop> {
        let mut paths = Paths {
            next: Some(initial),
            gives: None,
        };
        for statement in statements {
            let Some(count) = paths.next else { break };
            let Some(site) = statement.ledger_path() else {
                continue;
            };
            let expression =
                |value, count| self.atomic_value_expression(value, bindings, count, in_loop, site);
            match statement {
                CheckedStatement::Let { value, .. }
                | CheckedStatement::DestructuringLet { value, .. }
                | CheckedStatement::Evaluate { value, .. }
                | CheckedStatement::DropExpression { value, .. } => {
                    paths.next = Some(expression(value, count)?);
                }
                CheckedStatement::PropagateLet { scrutinee, .. } => {
                    // The error edge leaves; the success edge retains its count.
                    paths.next = Some(expression(scrutinee, count)?);
                }
                CheckedStatement::Set { target, value, .. } => {
                    let root = match target {
                        CheckedSetTarget::Place(place) => Some(place.binding),
                        CheckedSetTarget::RangeIndex(place) => Some(place.root.binding),
                        CheckedSetTarget::Storage(root) => root.binding(),
                    };
                    if root.is_some_and(|root| bindings.contains(&root)) {
                        return self.atomic_value_issue(
                            site,
                            "an atomic-value target binding cannot be assigned or written as a place",
                            "use one atomic-value operation directly on the header binding",
                        );
                    }
                    // Offsets can contain only atoms, and are still checked for
                    // forbidden uses of the atomic-value reference itself.
                    match target {
                        CheckedSetTarget::RangeIndex(place) => {
                            for offset in place.offsets() {
                                expression(offset, count)?;
                            }
                        }
                        CheckedSetTarget::Storage(root) => {
                            for offset in root.offsets() {
                                expression(offset, count)?;
                            }
                        }
                        CheckedSetTarget::Place(_) => {}
                    }
                    paths.next = Some(expression(value, count)?);
                }
                CheckedStatement::Return { value, .. } => {
                    expression(value, count)?;
                    paths.next = None;
                }
                CheckedStatement::Give { value, .. } => {
                    paths.gives = join(paths.gives, Some(expression(value, count)?));
                    paths.next = None;
                }
                CheckedStatement::Match {
                    scrutinee, arms, ..
                }
                | CheckedStatement::ValueMatchLet {
                    scrutinee, arms, ..
                } => {
                    let count = expression(scrutinee, count)?;
                    let mut next = None;
                    let mut gives = None;
                    for arm in arms {
                        let branch =
                            self.atomic_value_block(&arm.body, bindings, count, in_loop)?;
                        next = join(next, branch.next);
                        gives = join(gives, branch.gives);
                    }
                    if matches!(statement, CheckedStatement::ValueMatchLet { .. }) {
                        next = join(next, gives);
                    } else {
                        paths.gives = join(paths.gives, gives);
                    }
                    paths.next = next;
                }
                CheckedStatement::Loop {
                    body, continues, ..
                } => {
                    let nested = self.atomic_value_block(body, bindings, count, true)?;
                    paths.gives = join(paths.gives, nested.gives);
                    paths.next = continues.then_some(count);
                }
                CheckedStatement::CountedRange {
                    lower, upper, body, ..
                } => {
                    self.atomic_value_expression(lower, bindings, count, true, site)?;
                    self.atomic_value_expression(upper, bindings, count, true, site)?;
                    let nested = self.atomic_value_block(body, bindings, count, true)?;
                    paths.gives = join(paths.gives, nested.gives);
                    paths.next = Some(count);
                }
                CheckedStatement::Break { .. } | CheckedStatement::Continue { .. } => {
                    paths.next = None;
                }
                CheckedStatement::Atomic { .. } => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                CheckedStatement::Proof(_) => {}
            }
        }
        Ok(paths)
    }
}
