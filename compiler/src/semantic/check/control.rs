use super::super::model::CheckedExpression;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

mod commit;
mod loops;
mod matches;
mod proofs;
mod results;

use crate::syntax::NodeId;
use crate::{
    DeclarationId, DeclarationRole, Production, SemanticCompilerFailure, SemanticIssue,
    SemanticIssueKind, SemanticLocation, SemanticRule,
};

use super::super::model::{
    BindingId, CheckedDrop, CheckedLoopId, CheckedMode, CheckedProjectedDrop, CheckedStatement,
    CheckedType, ValueInitializerKind,
};
use super::references::{InvalidationEvent, REF3_RETURN_AN_INDEX, ReferenceInfo};
use super::{CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding};
use crate::semantic::places::PlaceRoot;
pub(super) use commit::CommitReadOut;
use loops::{BreakState, LoopContext};

pub(super) struct BlockResult {
    pub(super) statements: Vec<CheckedStatement>,
    pub(super) can_continue: bool,
    pub(super) effects: EffectSet,
    all_paths_deliver: bool,
    give_states: Vec<HashMap<DeclarationId, LocalBinding>>,
    break_states: Vec<BreakState>,
}

pub(super) struct StatementResult {
    pub(super) statement: CheckedStatement,
    pub(super) can_continue: bool,
    pub(super) effects: EffectSet,
    all_paths_deliver: bool,
    direct_give: bool,
    give_states: Vec<HashMap<DeclarationId, LocalBinding>>,
    break_states: Vec<BreakState>,
}

pub(super) struct GiveContext {
    /// [GIVE-1] the binding's mode and type are derived from the delivery
    /// set, not written: the first delivering `give` of this initializer
    /// produces them and every later one must agree exactly. `None` until
    /// that first `give` is checked; still `None` afterwards exactly when
    /// the delivery set is empty.
    delivered: Cell<Option<(CheckedMode, CheckedType)>>,
    /// [REF-1] the union of the path sets the delivering `give`s name, where
    /// every delivering `give` of this initializer delivers a reference. The
    /// binder is then itself a reference variable naming that union, and
    /// every check on it must hold for every member of the set.
    delivered_reference: std::cell::RefCell<Option<ReferenceInfo>>,
    preserved: HashSet<DeclarationId>,
    enclosing_loops: HashSet<CheckedLoopId>,
}

impl GiveContext {
    pub(super) fn empty(preserved: &HashSet<DeclarationId>, scope: ControlScope<'_>) -> Self {
        Self {
            delivered: Cell::new(None),
            delivered_reference: std::cell::RefCell::new(None),
            preserved: preserved.clone(),
            enclosing_loops: scope.loops.iter().map(|context| context.id).collect(),
        }
    }

    pub(super) fn delivered(&self) -> Option<(CheckedMode, CheckedType)> {
        self.delivered.get()
    }

    /// [REF-1] the reference the delivery set names, which is the union of
    /// the incoming path sets.
    pub(super) fn delivered_reference(&self) -> Option<ReferenceInfo> {
        self.delivered_reference.borrow().clone()
    }

    /// [REF-1] one delivering `give` of a reference joins its path set into
    /// the set the binder will name.
    fn deliver_reference(&self, delivered: &ReferenceInfo) {
        let mut current = self.delivered_reference.borrow_mut();
        match current.as_mut() {
            Some(existing) => existing.join(delivered),
            None => *current = Some(delivered.clone()),
        }
    }

    /// [REF-2] a delivered reference is stored separately from the ownership
    /// state copied onto its `give` edge. Scope exit must invalidate both
    /// representations before the value initializer publishes its binder.
    fn invalidate_reference_roots_leaving_scope(&self, leaving: &[BindingId]) {
        let mut delivered = self.delivered_reference.borrow_mut();
        let Some(reference) = delivered.as_mut() else {
            return;
        };
        if reference.paths.iter().any(|path| match path.root {
            PlaceRoot::Binding(binding) => leaving.contains(&binding),
            PlaceRoot::Constant(_) => false,
        }) {
            reference.invalidate(InvalidationEvent::RootScopeEnded);
        }
    }
}

pub(super) struct ControlCounters<'state> {
    pub(super) next_binding: &'state mut u32,
    pub(super) next_loop: &'state mut u32,
    /// Source spelling of every allocated binding, indexed by [`BindingId`],
    /// for ordinary proof and effect diagnostics. Every binding allocation
    /// site pushes exactly one name.
    pub(super) binding_names: &'state mut Vec<String>,
}

#[derive(Clone, Copy)]
pub(super) struct ControlScope<'state> {
    pub(super) loops: &'state [LoopContext],
    pub(super) give_context: Option<&'state GiveContext>,
}

impl<'unit> Checker<'unit> {
    pub(super) fn check_block(
        &self,
        function: &FunctionSignature,
        statement_wrappers: &[NodeId],
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<BlockResult, CheckStop> {
        let mut statements = Vec::with_capacity(statement_wrappers.len());
        let mut can_continue = true;
        let mut effects = EffectSet::NONE;
        let mut all_paths_deliver = false;
        let mut direct_give = false;
        let mut give_states = Vec::new();
        let mut break_states = Vec::new();
        for wrapper in statement_wrappers {
            let statement = self.tree.only_child(*wrapper)?;
            if !can_continue {
                return self.issue_node(
                    if direct_give {
                        SemanticRule::Give1
                    } else {
                        SemanticRule::Fn1
                    },
                    statement,
                    if direct_give {
                        SemanticIssueKind::InvalidGive
                    } else {
                        SemanticIssueKind::UnreachableStatement
                    },
                );
            }
            let checked = self.check_statement(function, statement, bindings, counters, scope)?;
            can_continue = checked.can_continue;
            effects = effects.union(checked.effects);
            all_paths_deliver = checked.all_paths_deliver;
            direct_give = checked.direct_give;
            give_states.extend(checked.give_states);
            break_states.extend(checked.break_states);
            statements.push(checked.statement);
        }
        if can_continue {
            all_paths_deliver = false;
        }
        Ok(BlockResult {
            statements,
            can_continue,
            effects,
            all_paths_deliver,
            give_states,
            break_states,
        })
    }

    pub(super) fn check_statement(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        self.check_statement_body(function, node, bindings, counters, scope)
    }

    fn check_statement_body(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        match self.tree.production(node)? {
            Production::LetStmt | Production::ContractDefine => {
                self.check_let(function, node, bindings, counters, scope)
            }
            Production::ExprStmt => {
                let call = self
                    .tree
                    .first_child_with(node, Production::Call)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let value = self.check_call(function, call, bindings, scope.loops.len())?;
                let node_path = self.tree.path(node)?.clone();
                // A discarded borrow-mode result is a reference, never the
                // owner of its referent: no drop or release may run for it
                // [REF-1, STOR-3]. Only an own-mode affine result is dropped.
                let statement = if value.mode != CheckedMode::Own
                    || self.is_copy_type(value.expression.ty())?
                {
                    CheckedStatement::Evaluate {
                        node_path,
                        value: value.expression,
                    }
                } else {
                    self.validate_scope_release(value.expression.ty(), "discarded result", node)?;
                    let drops = self
                        .drop_paths(value.expression.ty(), Vec::new())?
                        .into_iter()
                        .map(|(fields, ty)| CheckedProjectedDrop { fields, ty })
                        .collect();
                    CheckedStatement::DropExpression {
                        node_path,
                        value: value.expression,
                        drops,
                    }
                };
                Ok(Self::continuing_statement(statement, value.effects))
            }
            Production::InvariantStmt => {
                self.check_local_invariant(node, bindings, function, scope.loops.len())
            }
            // [FN-1, GRAM-4] a `return` writes exactly as many expressions as
            // the enclosing declaration writes results, and expression i
            // produces result ordinal i. A count mismatch is the ordinary
            // FN-1 return-shape rejection at the `return_stmt`.
            Production::ReturnStmt
                if self.tree.children_with(node, Production::Expr)?.len()
                    != function.results.len() =>
            {
                self.issue_node(SemanticRule::Fn1, node, SemanticIssueKind::ReturnMismatch)
            }
            Production::ReturnStmt if function.results.len() > 1 => {
                self.check_result_list_return(function, node, bindings, scope)
            }
            Production::ReturnStmt => {
                let expression_node = self
                    .tree
                    .first_child_with(node, Production::Expr)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                // [REF-3] "a `return_stmt` whose selected expression is a
                // reference is that violation": a `borrow_expr` [GRAM-5] is
                // that expression whatever place it names, so the escape is
                // settled from the written form before the place is
                // resolved. Resolving it first would report whatever the
                // named place happens to owe — an unproved subscript bound,
                // a base [OP-4] does not admit — as though repairing that
                // could make the return legal, when the restructuring
                // [REF-3] names is to return an index instead.
                if self.complete_borrow_expression(expression_node)?.is_some() {
                    return self.issue_node(
                        SemanticRule::Ref3,
                        expression_node,
                        SemanticIssueKind::EscapingReference {
                            mechanical_fix: REF3_RETURN_AN_INDEX,
                        },
                    );
                }
                self.check_return_implicit_read(function, expression_node, bindings)?;
                let mut value =
                    self.check_expression(function, expression_node, bindings, scope.loops.len())?;
                // [REF-3] a `return_stmt` whose selected expression is a
                // reference is the escape violation itself, and [FN-1] forms
                // no candidate there.
                self.reject_escaping_reference(&value, expression_node)?;
                if value.expression.ty() != function.result {
                    return Err(CheckStop::source_issue(SemanticIssue {
                        rule: SemanticRule::Fn1,
                        location: SemanticLocation::SourceNode(
                            self.tree.path(node)?.clone(),
                            self.tree.coordinate(expression_node)?,
                        ),
                        kind: SemanticIssueKind::ReturnMismatch,
                        request: None,
                    }));
                }
                let drops = self.live_affine_drops(bindings, &HashSet::new(), node)?;
                if let CheckedExpression::UserCall {
                    tail_transfer,
                    call,
                    ..
                } = &mut value.expression
                    && *tail_transfer
                {
                    *tail_transfer = self.check_self_tail_releases(call, bindings)?;
                }
                Ok(StatementResult {
                    statement: CheckedStatement::Return {
                        node_path: self.tree.path(node)?.clone(),
                        value: value.expression,
                        drops,
                    },
                    can_continue: false,
                    effects: value.effects,
                    all_paths_deliver: true,
                    direct_give: false,
                    give_states: Vec::new(),
                    break_states: Vec::new(),
                })
            }
            // [GRAM-6] the Bool conditional checks into the same two-armed
            // Bool match the `match` spelling produced, so everything below
            // the checker sees one statement kind for both.
            Production::IfStmt => {
                let matched = self.check_if(function, node, bindings, counters, scope, false)?;
                Ok(StatementResult {
                    statement: CheckedStatement::Match {
                        scrutinee: matched.scrutinee,
                        enum_type: matched.enum_type,
                        arms: matched.arms,
                        continues: matched.can_continue,
                    },
                    can_continue: matched.can_continue,
                    effects: matched.effects,
                    all_paths_deliver: matched.all_paths_deliver,
                    direct_give: false,
                    give_states: matched.give_states,
                    break_states: matched.break_states,
                })
            }
            Production::MatchStmt => {
                let matched = self.check_match(function, node, bindings, counters, scope, false)?;
                Ok(StatementResult {
                    statement: CheckedStatement::Match {
                        scrutinee: matched.scrutinee,
                        enum_type: matched.enum_type,
                        arms: matched.arms,
                        continues: matched.can_continue,
                    },
                    can_continue: matched.can_continue,
                    effects: matched.effects,
                    all_paths_deliver: matched.all_paths_deliver,
                    direct_give: false,
                    give_states: matched.give_states,
                    break_states: matched.break_states,
                })
            }
            Production::GiveStmt => {
                let Some(context) = scope.give_context else {
                    return self.issue_node(
                        SemanticRule::Give1,
                        node,
                        SemanticIssueKind::InvalidGive,
                    );
                };
                let expression_node = self
                    .tree
                    .first_child_with(node, Production::Expr)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let value =
                    self.check_expression(function, expression_node, bindings, scope.loops.len())?;
                // [GIVE-1] derivation is agreement over the closed delivery
                // set: the first delivering `give` produces the binding's
                // exact mode and type, and every later one must match them.
                let delivered = (value.mode, value.expression.ty());
                // [REF-1, GIVE-1] when every delivering `give` delivers a
                // reference, the agreement above is agreement of reference
                // kind and the binder's path set is the union over the
                // delivery set.
                if let Some(reference) = &value.reference {
                    context.deliver_reference(reference);
                }
                match context.delivered.get() {
                    None => context.delivered.set(Some(delivered)),
                    Some(earlier) if earlier == delivered => {}
                    Some((mode, ty)) => {
                        return self.issue_node(
                            SemanticRule::Give1,
                            node,
                            SemanticIssueKind::type_mismatch(
                                self.checked_value_name(mode, ty)?,
                                self.checked_value_name(value.mode, value.expression.ty())?,
                            ),
                        );
                    }
                }
                Ok(StatementResult {
                    statement: CheckedStatement::Give {
                        node_path: self.tree.path(node)?.clone(),
                        value: value.expression,
                        drops: self.live_affine_drops(bindings, &context.preserved, node)?,
                    },
                    can_continue: false,
                    effects: value.effects,
                    all_paths_deliver: true,
                    direct_give: true,
                    give_states: vec![bindings.clone()],
                    break_states: Vec::new(),
                })
            }
            // [GRAM-4, SET-1] every written `set` is one commit: the
            // targets are resolved and judged first, then the whole
            // right-hand side, then the three admission conditions.
            Production::SetStmt => self.check_commit(function, node, bindings, counters, scope),
            Production::LoopStmt => self.check_loop(function, node, bindings, counters, scope),
            Production::ForStmt => {
                self.check_counted_range(function, node, bindings, counters, scope)
            }
            Production::BreakStmt => self.check_break(node, bindings, scope),
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }

    fn check_let(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let first_binding = *counters.next_binding;
        let result = self.check_let_body(function, node, bindings, counters, scope)?;
        // Every binding form shares the same destination judgment. In
        // particular, a value initializer can deliver storage allocated in
        // a region nested inside its own destination's scope.
        let mut destinations = bindings
            .values()
            .filter(|local| local.binding.0 >= first_binding)
            .collect::<Vec<_>>();
        destinations.sort_by_key(|local| local.binding.0);
        for local in destinations {
            self.check_confined_destination(function, local.ty, Some(local.declaration), node)?;
        }
        Ok(result)
    }

    fn check_let_body(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        // [PROV-6, GRAM-4] the destructuring consume is the one `let`
        // alternative whose operand place is a direct child of the statement.
        if let Some(place) = self.tree.first_child_with(node, Production::Place)? {
            return self
                .check_destructuring_consume(function, node, place, bindings, counters, scope);
        }
        // [GRAM-4, CALL-4] a binder list takes its right-hand side's result
        // ordinals; a `call` directly under the `let_stmt` is that form and
        // no other selects it.
        if let Some(call) = self.tree.first_child_with(node, Production::Call)? {
            return self.check_destructuring_let(function, node, call, bindings, counters, scope);
        }
        // [TYPE-5] a `let` binder's mode and type are derived, never written:
        // exactly what its selected right-hand side produces. Each arm below
        // therefore checks that right-hand side first and reads the binding's
        // mode and type off the result.
        let declaration = self.declaration_at(node, DeclarationRole::Let)?;
        let declaration_id = declaration.id();
        let binding = Self::allocate_binding(counters.next_binding)?;
        counters
            .binding_names
            .push(declaration.spelling().to_owned());

        // [GIVE-1] a value initializer is a `match` or an `if`. Both derive the
        // binder from their delivery set and share every judgment below, so
        // only the checker that produces the delivery set differs.
        let value_match = self.tree.first_child_with(node, Production::ValueMatch)?;
        let value_if = self.tree.first_child_with(node, Production::ValueIf)?;
        if let Some(initializer) = value_match.or(value_if) {
            let matched = if value_if.is_some() {
                self.check_if(function, initializer, bindings, counters, scope, true)?
            } else {
                self.check_match(function, initializer, bindings, counters, scope, true)?
            };
            if !matched.all_paths_deliver {
                return self.issue_node(
                    SemanticRule::Give1,
                    initializer,
                    SemanticIssueKind::InvalidGive,
                );
            }
            // [GIVE-1] an empty delivery set — every arm leaves by `return`
            // or by `break` — rejects at the `let_stmt` node: its repair is
            // the statement form with the binding dropped, and the statements
            // after it, which no path reaches [FN-1], go with it.
            let Some((mode, expected)) = matched.delivered else {
                let binding = declaration.spelling().to_owned();
                let form = if value_if.is_some() { "if" } else { "match" };
                return self.issue_node(
                    SemanticRule::Give1,
                    node,
                    SemanticIssueKind::EmptyDeliverySet {
                        mechanical_fix: format!(
                            "every arm leaves by `return` or `break`, so no value reaches `{binding}`: drop `let {binding} =`, write the `{form}` as a statement, and delete the statements after it in this block, which no path reaches"
                        ),
                        binding,
                    },
                );
            };
            let result_range_element = if mode == CheckedMode::Range {
                Some(self.intern_element(expected)?)
            } else {
                None
            };
            // [REF-1] a binder every arm of which delivers a reference is
            // itself a reference variable, naming the union of the path sets
            // its delivering arms name, rather than taking a type [TYPE-5].
            let reference = if mode.is_reference() {
                Some(
                    matched
                        .delivered_reference
                        .clone()
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                )
            } else {
                None
            };
            if matched.can_continue
                && let Some(reference) = &reference
            {
                self.record_reference_origins(binding, &reference.paths);
            }
            if matched.can_continue
                && bindings
                    .insert(
                        declaration_id,
                        LocalBinding {
                            binding,
                            declaration: declaration_id,
                            mode,
                            ty: expected,
                            live: true,
                            loop_depth: scope.loops.len(),
                            compiler_updated: false,
                            reference,
                            refinement_witnesses: Vec::new(),
                        },
                    )
                    .is_some()
            {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            return Ok(StatementResult {
                statement: CheckedStatement::ValueMatchLet {
                    node_path: self.tree.path(node)?.clone(),
                    kind: if value_if.is_some() {
                        ValueInitializerKind::ValueIf
                    } else {
                        ValueInitializerKind::ValueMatch
                    },
                    binding,
                    result_type: expected,
                    result_mode: mode,
                    result_range_element,
                    scrutinee: matched.scrutinee,
                    enum_type: matched.enum_type,
                    arms: matched.arms,
                    continues: matched.can_continue,
                },
                can_continue: matched.can_continue,
                effects: matched.effects,
                all_paths_deliver: !matched.can_continue,
                direct_give: false,
                give_states: Vec::new(),
                break_states: matched.break_states,
            });
        }
        if let Some(propagate) = self
            .tree
            .first_child_with(node, Production::PropagateLetRhs)?
        {
            return self.check_propagate_let(
                function,
                node,
                propagate,
                declaration_id,
                binding,
                bindings,
                scope,
            );
        }
        let contract_definition = self.tree.production(node)? == Production::ContractDefine;
        let expression_owner = if contract_definition {
            node
        } else {
            self.tree
                .first_child_with(node, Production::OrdinaryLetRhs)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?
        };
        let expression_node = self
            .tree
            .first_child_with(expression_owner, Production::Expr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // An `ordinary_let_rhs` is always self-typed [TYPE-5], so it is
        // checked with no expectation and the binder takes what it produces.
        let value =
            self.check_expression(function, expression_node, bindings, scope.loops.len())?;
        let mode = value.mode;
        let expected = value.expression.ty();
        // [REF-1] a binder whose initializer is a `borrow_expr`, and a binder
        // that reads a reference variable, is itself a reference variable
        // naming the same path — an alias, not a copy of the referent. The
        // binder takes that reference kind rather than a type [TYPE-5].
        let reference = value.reference.clone();
        if mode.is_reference() && reference.is_none() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        // Contract definitions use temporary binding namespaces and are
        // erased, so they do not contribute to the executable body's map.
        if !contract_definition && let Some(reference) = &reference {
            self.record_reference_origins(binding, &reference.paths);
        }
        if bindings
            .insert(
                declaration_id,
                LocalBinding {
                    binding,
                    declaration: declaration_id,
                    mode,
                    ty: expected,
                    live: true,
                    loop_depth: scope.loops.len(),
                    compiler_updated: false,
                    reference,
                    refinement_witnesses: Vec::new(),
                },
            )
            .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(Self::continuing_statement(
            CheckedStatement::Let {
                node_path: self.tree.path(node)?.clone(),
                binding,
                value: value.expression,
            },
            value.effects,
        ))
    }

    /// The compiler-derived releases one edge leaving a scope carries
    /// [STOR-3, LIV-1], and the [PROV-6] refusal of a value that is linear in
    /// this scope and has no derived release to carry it there.
    fn live_affine_drops(
        &self,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        preserved: &HashSet<DeclarationId>,
        edge: NodeId,
    ) -> Result<Vec<CheckedDrop>, CheckStop> {
        let mut live = bindings
            .iter()
            .filter_map(|(declaration, local)| {
                (local.live && local.mode == CheckedMode::Own && !preserved.contains(declaration))
                    .then_some((*declaration, local.clone()))
            })
            .collect::<Vec<_>>();
        live.sort_by_key(|entry| std::cmp::Reverse(entry.1.binding.0));
        let mut drops = Vec::new();
        for (_, local) in &live {
            let name = self
                .resolved
                .declarations()
                .iter()
                .find(|declaration| declaration.id() == local.declaration)
                .map_or_else(String::new, |declaration| declaration.spelling().to_owned());
            self.validate_scope_release(local.ty, &name, edge)?;
        }
        for (_, local) in live {
            if !self.is_copy_type(local.ty)? {
                let paths = self.drop_paths(local.ty, Vec::new())?;
                for (fields, ty) in paths {
                    drops.push(CheckedDrop {
                        binding: local.binding,
                        fields,
                        ty,
                    });
                }
            }
        }
        Ok(drops)
    }

    fn allocate_binding(next_binding: &mut u32) -> Result<BindingId, CheckStop> {
        let binding = BindingId(*next_binding);
        *next_binding = next_binding
            .checked_add(1)
            .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        Ok(binding)
    }

    fn continuing_statement(statement: CheckedStatement, effects: EffectSet) -> StatementResult {
        StatementResult {
            statement,
            can_continue: true,
            effects,
            all_paths_deliver: false,
            direct_give: false,
            give_states: Vec::new(),
            break_states: Vec::new(),
        }
    }
}
