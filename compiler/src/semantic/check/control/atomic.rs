//! [SHARE-2] the atomic statement: its target, its binding, its guard and its
//! block.

use crate::semantic::check::FunctionContext;
use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationId, DeclarationRole, Production, SemanticCompilerFailure, SemanticIssueKind,
    SemanticRule,
};

use super::super::super::model::{
    CheckedExpression, CheckedMode, CheckedNominalKind, CheckedStatement, CheckedType,
    expression_children,
};
use super::super::super::places::ResolvedPlace;
use super::super::expressions::calls::user::WAIT1_DECLARE_THE_CALLER_WAITING;
use super::super::references::{ReferenceInfo, ReferenceKind};
use super::super::{CheckStop, Checker, EffectSet, LocalBinding};
use super::{ControlCounters, ControlScope, StatementResult};

/// The repair for a target that is not a `Shared<T>` place [SHARE-2, DIAG-1].
pub(in crate::semantic::check) const SHARE2_NAME_A_SHARED_HANDLE: &str = "name a place of type `Shared<T>`: create the object with `shared_new` and give each context its own handle made with `shared_share`";
/// The repair for a waiting call inside an atomic statement [SHARE-2].
pub(in crate::semantic::check) const SHARE2_WAIT_OUTSIDE_THE_BLOCK: &str = "move the waiting call out of the atomic statement: end the statement first, wait, and start another atomic statement for any update that depends on the outcome";
/// The repair for an atomic statement inside another [SHARE-2].
pub(in crate::semantic::check) const SHARE2_END_THE_OUTER_STATEMENT: &str = "end the outer atomic statement before starting the inner one, carrying what the inner one needs in a local";
/// The repair for a guard that writes [SHARE-2].
pub(in crate::semantic::check) const SHARE2_READ_ONLY_GUARD: &str = "make the guard read only, calling a function whose row writes nothing and moves no argument, and make the update in the block";

impl Checker<'_, '_> {
    pub(super) fn check_atomic(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let FunctionContext {
            check_context,
            function,
        } = context;
        // [SHARE-2] a guard or block contains no atomic statement; the inner
        // statement is the offending one.
        if self.body.atomic_depth > 0 {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                node,
                SemanticIssueKind::WaitInsideAtomic {
                    construct: "an atomic statement",
                    mechanical_fix: SHARE2_END_THE_OUTER_STATEMENT,
                },
            );
        }
        // [WAIT-1, SHARE-2] the statement counts as a waiting call.
        if !function.waits {
            return self.types.declarations.issue_node(
                SemanticRule::Wait1,
                node,
                SemanticIssueKind::WaitingCallOutsideWaitingFunction {
                    callee: "an atomic statement".to_owned(),
                    context: "a function that does not wait",
                    mechanical_fix: WAIT1_DECLARE_THE_CALLER_WAITING,
                },
            );
        }
        let node_path = self.types.declarations.tree.path(node)?.clone();
        self.body.waiting.calls.push(node_path.clone());

        // [SHARE-2] the target: `&place` of type `Shared<T>`, read when the
        // statement begins.
        let place_node = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let target =
            self.check_place_borrow(context, node, node, place_node, bindings, scope.loops.len())?;
        let state = match (target.mode, target.expression.ty()) {
            (CheckedMode::Reference, CheckedType::Nominal(nominal)) => {
                match &self.types.nominal(nominal)?.kind {
                    CheckedNominalKind::Shared { state } => Some(*state),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(state) = state else {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                node,
                SemanticIssueKind::AtomicTargetNotShared {
                    found: self
                        .types
                        .checked_value_name(target.mode, target.expression.ty())?,
                    mechanical_fix: SHARE2_NAME_A_SHARED_HANDLE,
                },
            );
        };
        let mut effects = target.effects.clone();
        for place in target
            .reference
            .as_ref()
            .map(|reference| reference.paths.as_slice())
            .unwrap_or_default()
        {
            for path in self.effect_paths_for_place(node, place, bindings)? {
                effects.add_read(path);
            }
        }

        // [SHARE-2] the binding: a reference variable of kind `&T` whose path
        // is the state. It anchors at itself, as a reference parameter does:
        // the state belongs to no binding [SHARE-1], so no path reaches it
        // except through this binder.
        let declaration = self
            .types
            .declarations
            .declaration_at(node, DeclarationRole::AtomicBinder)?;
        let binding = Checker::allocate_binding(counters.next_binding)?;
        counters
            .binding_names
            .push(declaration.spelling().to_owned());
        let base_keys = bindings.keys().copied().collect::<Vec<_>>();
        let preserved = base_keys.iter().copied().collect::<HashSet<_>>();
        let mut block_bindings = bindings.clone();
        let reference =
            ReferenceInfo::formed(ReferenceKind::Single, ResolvedPlace::binding(binding));
        self.body
            .record_reference_origins(binding, &reference.paths);
        block_bindings.insert(
            declaration.id(),
            LocalBinding {
                binding,
                declaration: declaration.id(),
                mode: CheckedMode::Reference,
                ty: state,
                live: true,
                loop_depth: scope.loops.len(),
                compiler_updated: false,
                reference: Some(reference),
                refinement_witnesses: Vec::new(),
                call_value: false,
            },
        );

        self.body.atomic_depth += 1;
        let checked = self.check_atomic_parts(context, node, &mut block_bindings, counters, scope);
        self.body.atomic_depth -= 1;
        let (guard, mut checked) = checked?;
        if let Some(guard) = &guard {
            effects = effects.union(guard.1.clone());
        }
        effects = effects.union(checked.effects);

        // [REF-2] the binder's root leaves scope when the block ends by any
        // edge, with the block's own bindings.
        let leaving = Checker::bindings_leaving_scope(&block_bindings, &base_keys);
        Checker::invalidate_control_exits(
            &mut block_bindings,
            &mut checked.give_states,
            &mut checked.break_states,
            scope.give_context,
            &leaving,
        );
        let fallthrough_drops = if checked.can_continue {
            self.types
                .live_affine_drops(check_context, &block_bindings, &preserved, node)?
        } else {
            Vec::new()
        };
        if checked.can_continue {
            self.types.declarations.join_states(
                &base_keys,
                std::slice::from_ref(&block_bindings),
                &["the atomic block".to_owned()],
                node,
                bindings,
            )?;
        }
        Ok(StatementResult {
            statement: CheckedStatement::Atomic {
                node_path,
                target: Box::new(target.expression),
                binding,
                state,
                guard: guard.map(|guard| Box::new(guard.0)),
                body: checked.statements,
                fallthrough_drops,
            },
            can_continue: checked.can_continue,
            effects,
            all_paths_deliver: !checked.can_continue && checked.all_paths_deliver,
            direct_give: false,
            give_states: checked.give_states,
            break_states: checked.break_states,
        })
    }

    /// The guard and the block, checked with the binder in scope and inside
    /// the atomic statement, so a waiting call or another atomic statement in
    /// either is refused [SHARE-2].
    #[allow(clippy::type_complexity)]
    fn check_atomic_parts(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<(Option<(CheckedExpression, EffectSet)>, super::BlockResult), CheckStop> {
        let guard = match self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Expr)?
        {
            Some(expression_node) => {
                let condition =
                    self.check_condition(context, expression_node, bindings, scope.loops.len())?;
                if self.guard_writes(&condition.expression) {
                    return self.types.declarations.issue_node(
                        SemanticRule::Share2,
                        expression_node,
                        SemanticIssueKind::AtomicGuardWrites {
                            mechanical_fix: SHARE2_READ_ONLY_GUARD,
                        },
                    );
                }
                Some((condition.expression, condition.effects))
            }
            None => None,
        };
        let statements = self
            .types
            .declarations
            .tree
            .children_with(node, Production::Stmt)?;
        let block = self.check_block(context, &statements, bindings, counters, scope)?;
        Ok((guard, block))
    }

    /// [SHARE-2, PAR-1] whether a guard's footprint writes a path: a call in
    /// it whose row writes, or which consumes an argument's place.
    fn guard_writes(&self, expression: &CheckedExpression) -> bool {
        if let CheckedExpression::UserCall {
            function,
            formal_effects,
            arguments,
            ..
        } = expression
        {
            let declared = formal_effects
                .as_ref()
                .map(|effects| !effects.writes.is_empty())
                .or_else(|| {
                    self.types
                        .signatures
                        .get(function.0 as usize)
                        .map(|signature| !signature.declared_effects.writes.is_empty())
                })
                .unwrap_or(true);
            let consumes = arguments.iter().any(|argument| {
                matches!(
                    argument,
                    CheckedExpression::Binding {
                        consume_root: true,
                        ..
                    } | CheckedExpression::Project {
                        consume_root: true,
                        ..
                    } | CheckedExpression::BoxTake { .. }
                )
            });
            if declared || consumes {
                return true;
            }
        }
        expression_children(expression)
            .into_iter()
            .any(|child| self.guard_writes(child))
    }
}
