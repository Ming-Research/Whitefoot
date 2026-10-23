//! The atomic statement's one target and its ordinary reference, effect and
//! scope judgments [SHARE-2].
use super::super::super::model::{
    BindingId, CheckedExpression, CheckedMode, CheckedNominalKind, CheckedShared, CheckedStatePath,
    CheckedStatement, CheckedTarget, CheckedType, expression_children,
};
use super::super::super::places::{PlaceRoot, ResolvedPlace};
use super::super::expressions::calls::user::WAIT1_DECLARE_THE_CALLER_WAITING;
use super::super::references::{HandleWatch, ReferenceInfo, ReferenceKind};
use super::super::types::SelectedPlaceType;
use super::super::{CheckStop, Checker, EffectSet, LocalBinding, TypedExpression};
use super::{ControlCounters, ControlScope, StatementResult};
use crate::semantic::check::FunctionContext;
use crate::syntax::NodeId;
use crate::{
    DeclarationId, DeclarationRole, Production, ResolvedTarget, SemanticCompilerFailure,
    SemanticIssueKind, SemanticRule,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
pub(in crate::semantic::check) const SHARE2_NAME_A_SHARED_HANDLE: &str = "name a place of type `Shared<T>` or `SharedRead<T>`: create the object with `shared_new` and give each context its own handle made with `shared_share`";
pub(in crate::semantic::check) const SHARE2_WAIT_OUTSIDE_THE_BLOCK: &str = "move the waiting call out of the atomic statement: end the statement first, wait, and start another atomic statement for any update that depends on the outcome";
pub(in crate::semantic::check) const SHARE2_END_THE_OUTER_STATEMENT: &str = "end the outer statement before starting the inner one, or keep both parts in the state of one shared object and update them in one statement";
pub(in crate::semantic::check) const SHARE2_READ_ONLY_GUARD: &str = "make the guard read only, calling a function whose row writes nothing and moves no argument, and make the update in the block";
pub(in crate::semantic::check) const SHARE2_WAIT_FOR_THE_TIME_OUTSIDE_THE_GUARD: &str = "wait for the time outside the guard: call `std::time::sleep_until` with the deadline before the statement, or spawn a context that sleeps until the deadline and then fires a cancellation source, and read a watch of that source's `fired` in the guard";
pub(in crate::semantic::check) const SHARE2_USE_THE_BINDING: &str = "name the binding in the guard or the block, or remove the statement: a statement holds its object's state, so one that names nothing of it holds the state for nothing";
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
        let node_path = self.types.declarations.tree.path(node)?.clone();
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
        self.body.waiting.calls.push(node_path.clone());
        let place = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let declarations = self
            .types
            .declarations
            .declarations_at(node, DeclarationRole::AtomicBinder)?;
        let [declaration] = declarations.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let base_keys = bindings.keys().copied().collect::<Vec<_>>();
        let preserved = base_keys.iter().copied().collect::<HashSet<_>>();
        let mut block_bindings = bindings.clone();
        // [SHARE-2] the statement reads its handle place when it begins,
        // before it holds the state; a subscript of the place, such as
        // `&shards^[slot]`, is an ordinary subscript within it [OP-4].
        let handle = self.check_place_borrow(
            context,
            place,
            place,
            place,
            &block_bindings,
            scope.loops.len(),
        )?;
        let paths = handle
            .reference
            .as_ref()
            .map(|r| r.paths.clone())
            .unwrap_or_default();
        let Some((state, read_only)) = self.shared_handle_state(&handle)? else {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                place,
                SemanticIssueKind::AtomicTargetNotShared {
                    found: self
                        .types
                        .checked_value_name(handle.mode, handle.expression.ty())?,
                    mechanical_fix: SHARE2_NAME_A_SHARED_HANDLE,
                },
            );
        };
        // [SHARE-1, SHARE-2] a handle place that only the statement's own
        // guard and block can reach keeps the object live while they leave
        // it alone. Every such place is rooted at a binding: storage of
        // this activation, or, through a reference parameter, of a caller
        // that waits in this call. No other context reaches either, since
        // a spawn takes values alone [WAIT-3] and a state, which every
        // context reaches, is never a handle place, the statement reading
        // its handle before it holds the state and no atomic statement
        // enclosing another.
        let rooted = paths
            .iter()
            .all(|path| matches!(path.root, PlaceRoot::Binding(_)));
        let watch = if rooted && !paths.is_empty() {
            Some(Rc::new(HandleWatch::new(paths.clone())))
        } else {
            None
        };
        let mut effects = handle.effects.clone();
        for p in &paths {
            for path in self.effect_paths_for_place(node, p, &block_bindings)? {
                effects.add_read(path);
            }
        }
        let binding = self.bind_atomic_reference(
            counters,
            &mut block_bindings,
            declaration,
            state,
            scope.loops.len(),
        )?;
        let local = block_bindings
            .get_mut(&declaration.id())
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        local.read_only_state = read_only;
        local.handle_watch.clone_from(&watch);
        if let CheckedType::Nominal(nominal) = state
            && let Some(clauses) = self.types.range_type_invariants.get(&nominal)
        {
            self.body
                .range_facts
                .atomics
                .entry(node_path.clone())
                .or_default()
                .extend(clauses.iter().map(|clause| {
                    clause.with_subject(
                        crate::semantic::range_facts::CheckedRangeRoot::Binding(binding),
                        true,
                    )
                }));
        }
        let invariants = self.atomic_invariants(state, binding);
        let mut target = CheckedTarget {
            node_path: self.types.declarations.tree.path(place)?.clone(),
            ledger_name: declaration.spelling().to_owned(),
            ledger_state: self.types.checked_type_name(state)?,
            ledger_site: self
                .types
                .declarations
                .lock_ledger_site(declaration.origin().coordinate()),
            binding,
            handle: Box::new(handle.expression),
            kept_by_place: false,
            state,
            reads: false,
            writes: Vec::new(),
            written_roots: Vec::new(),
            invariants,
        };
        let root = declaration.id();
        self.body.atomic_depth += 1;
        self.body.held_atomic_roots = vec![binding];
        self.body.held_atomic_node = Some(node_path.clone());
        let held_call_start = self.body.held_calls.len();
        let held_write_start = self.body.held_writes.len();
        let checked = self.check_atomic_parts(context, node, &mut block_bindings, counters, scope);
        self.body.atomic_depth -= 1;
        self.body.held_atomic_roots.clear();
        self.body.held_atomic_node = None;
        let (mut guard, mut checked) = checked?;
        let held_calls = self.body.held_calls[held_call_start..].to_vec();
        let held_writes = self.body.held_writes[held_write_start..].to_vec();
        self.check_held_protection(&held_writes, &block_bindings, &held_calls)?;
        self.body.held_calls.truncate(held_call_start);
        self.body.held_writes.truncate(held_write_start);
        if !self.atomic_part_mentions(node)?.contains(&root) {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                place,
                SemanticIssueKind::AtomicBindingUnused {
                    binding: counters.binding_names[binding.0 as usize].clone(),
                    mechanical_fix: SHARE2_USE_THE_BINDING,
                },
            );
        }
        // No event the guard and block were checked with may have written,
        // moved or released the handle place, on any path through them. A
        // handle to a linear state ends only in a `shared_close`, a waiting
        // call no block contains, so every handle the block may move away
        // still names the object when the statement completes [SHARE-1,
        // PROV-6].
        target.kept_by_place = watch.as_ref().is_some_and(|watch| !watch.disturbed())
            || matches!(
                self.types.linearity_class(check_context, state)?,
                super::super::linearity::LinearityClass::Linear
            );
        // The paths below the state the guard and the block write, which
        // the lowering's lock units read (compiler/waiting-contexts/
        // state-locks). Every write, a variant write included, is recorded
        // at its path or a prefix of it, so a write of one map's entry
        // reaches that map's unit alone.
        let held = |path: &CheckedStatePath| path.root == root;
        let mut writes = checked
            .effects
            .writes
            .iter()
            .chain(guard.iter().flat_map(|g| g.1.writes.iter()))
            .filter(|path| held(path))
            .map(|path| path.steps.clone())
            .collect::<Vec<_>>();
        writes.sort();
        writes.dedup();
        target.reads = writes.is_empty();
        target.writes = writes;
        let mut written_roots = checked
            .effects
            .writes
            .iter()
            .chain(guard.iter().flat_map(|g| g.1.writes.iter()))
            .map(|path| path.root)
            .chain(
                checked
                    .effects
                    .variant_writes
                    .iter()
                    .chain(guard.iter().flat_map(|g| g.1.variant_writes.iter()))
                    .copied(),
            )
            .filter(|written| *written != root)
            .filter_map(|written| block_bindings.get(&written).map(|local| local.binding))
            .collect::<Vec<_>>();
        written_roots.sort_by_key(|binding| binding.0);
        written_roots.dedup();
        target.written_roots = written_roots;
        for set in std::iter::once(&mut checked.effects).chain(guard.iter_mut().map(|g| &mut g.1)) {
            set.reads.retain(|p| !held(p));
            set.writes.retain(|p| !held(p));
            set.variant_writes.retain(|written| *written != root);
        }
        if let Some(g) = &guard {
            effects = effects.union(g.1.clone());
        }
        effects = effects.union(checked.effects);
        let leaving = Checker::bindings_leaving_scope(&block_bindings, &base_keys);
        Checker::invalidate_control_exits(
            &mut block_bindings,
            &mut checked.give_states,
            &mut checked.loop_transfers,
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
                target: Box::new(target),
                guard: guard.map(|g| Box::new(g.0)),
                body: checked.statements,
                fallthrough_drops,
                continues: checked.can_continue,
            },
            can_continue: checked.can_continue,
            effects,
            all_paths_deliver: !checked.can_continue && checked.all_paths_deliver,
            direct_give: false,
            give_states: checked.give_states,
            loop_transfers: checked.loop_transfers,
        })
    }
    /// The state type of the handle place a borrow names, and whether the
    /// handle is a `SharedRead`, when the place has type `Shared<T>` or
    /// `SharedRead<T>` [SHARE-1].
    fn shared_handle_state(
        &self,
        borrow: &TypedExpression,
    ) -> Result<Option<(CheckedType, bool)>, CheckStop> {
        if borrow.mode != CheckedMode::Reference {
            return Ok(None);
        }
        self.shared_handle_type(borrow.expression.ty())
    }
    /// The state type and `SharedRead`-ness of a handle type `Shared<T>` or
    /// `SharedRead<T>` [SHARE-1], and `None` for any other type.
    fn shared_handle_type(
        &self,
        ty: CheckedType,
    ) -> Result<Option<(CheckedType, bool)>, CheckStop> {
        let CheckedType::Nominal(nominal) = ty else {
            return Ok(None);
        };
        if self.types.is_decision_cell(nominal)? {
            return Ok(None);
        }
        let CheckedNominalKind::Shared {
            state,
            shape: CheckedShared::Object,
        } = self.types.nominal(nominal)?.kind
        else {
            return Ok(None);
        };
        let read_only = self.types.source_nominal_instances[nominal.0 as usize]
            .as_ref()
            .is_some_and(|(template, _)| {
                self.types.nominal_templates[*template].name == "SharedRead"
            });
        Ok(Some((state, read_only)))
    }
    fn bind_atomic_reference(
        &mut self,
        counters: &mut ControlCounters<'_>,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        declaration: &crate::DeclarationRecord,
        referent: CheckedType,
        loop_depth: usize,
    ) -> Result<BindingId, CheckStop> {
        let binding = Checker::allocate_binding(counters.next_binding)?;
        counters
            .binding_names
            .push(declaration.spelling().to_owned());
        let reference =
            ReferenceInfo::formed(ReferenceKind::Single, ResolvedPlace::binding(binding));
        self.body
            .record_reference_origins(binding, &reference.paths);
        self.body.note_anchor(binding, declaration.id());
        bindings.insert(
            declaration.id(),
            LocalBinding {
                binding,
                declaration: declaration.id(),
                mode: CheckedMode::Reference,
                ty: referent,
                live: true,
                loop_depth,
                compiler_updated: false,
                read_only_state: false,
                reference: Some(reference),
                refinement_witnesses: Vec::new(),
                handle_watch: None,
                call_value: false,
            },
        );
        Ok(binding)
    }
    /// Source mentions survive proof erasure and inactive generic range clauses.
    /// Only the guard and block count; the target declarations and places do not.
    fn atomic_part_mentions(&self, node: NodeId) -> Result<HashSet<DeclarationId>, CheckStop> {
        let declarations = &self.types.declarations;
        let parts = declarations
            .tree
            .children_with(node, Production::Expr)?
            .into_iter()
            .chain(declarations.tree.children_with(node, Production::Stmt)?);
        let mut named = HashSet::new();
        for part in parts {
            let prefix = declarations.tree.path(part)?.components();
            for usage in declarations.resolved.lexical_uses_under(prefix) {
                if let ResolvedTarget::Source { declaration, .. } = usage.target() {
                    named.insert(declaration);
                }
            }
        }
        Ok(named)
    }
    /// The guard and the block, checked with the binders in scope and inside
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
                self.body.held_guard = true;
                let condition =
                    self.check_condition(context, expression_node, bindings, scope.loops.len());
                self.body.held_guard = false;
                let condition = condition?;
                if self.guard_has_decision_cell(&condition.expression)? {
                    return self.types.declarations.issue_node(
                        SemanticRule::Share2,
                        expression_node,
                        SemanticIssueKind::AtomicGuardDecisionCell {
                            mechanical_fix: "read or claim the decision cell in the atomic body; use ordinary shared state for a waiting guard dependency",
                        },
                    );
                }
                if let Some((call, handle)) =
                    self.guard_self_changing_part(&condition.expression)?
                {
                    let call = self
                        .types
                        .declarations
                        .tree
                        .node_with_path(&call)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    return self.types.declarations.issue_node(
                        SemanticRule::Share2,
                        call,
                        SemanticIssueKind::AtomicGuardReadsSelfChanging {
                            handle,
                            mechanical_fix: SHARE2_WAIT_FOR_THE_TIME_OUTSIDE_THE_GUARD,
                        },
                    );
                }
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

    /// [SHARE-2, PRE-2] the call of a guard whose row reads or writes a path
    /// whose type is or has a self-changing host handle part, with that part.
    /// A guard's `expr` holds at most one call, whose arguments are atoms
    /// [GRAM-5, GRAM-9], so the walk finds at most that call and the site is
    /// unique. A row path is rooted at one of the
    /// callee's reference parameters, so its type is the one the caller's
    /// projected path has [EFF-5]; a formal's row is rebased to the selected
    /// callee's parameters [FN-4].
    fn guard_self_changing_part(
        &self,
        expression: &CheckedExpression,
    ) -> Result<Option<(crate::NodePath, String)>, CheckStop> {
        for child in expression_children(expression) {
            if let Some(found) = self.guard_self_changing_part(child)? {
                return Ok(Some(found));
            }
        }
        let CheckedExpression::UserCall {
            function,
            formal_effects,
            call,
            ..
        } = expression
        else {
            return Ok(None);
        };
        let signature = self
            .types
            .signatures
            .get(function.0 as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let paths: Vec<&CheckedStatePath> = match formal_effects {
            Some(effects) => effects.reads.iter().chain(&effects.writes).collect(),
            None => signature
                .declared_effects
                .reads
                .iter()
                .chain(&signature.declared_effects.writes)
                .collect(),
        };
        for path in paths {
            let ty = match self.types.effect_path_selected(path, signature)? {
                SelectedPlaceType::Value(ty) | SelectedPlaceType::Range(ty) => ty,
                SelectedPlaceType::UnresolvedWindowElement => continue,
            };
            if let Some(handle) = self.types.first_self_changing_part(ty)? {
                return Ok(Some((call.clone(), handle)));
            }
        }
        Ok(None)
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

    fn guard_has_decision_cell(&self, expression: &CheckedExpression) -> Result<bool, CheckStop> {
        if let CheckedExpression::UserCall { function, .. } = expression
            && self.types.decision_cell_boundary(*function)?
        {
            return Ok(true);
        }
        for child in expression_children(expression) {
            if self.guard_has_decision_cell(child)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

