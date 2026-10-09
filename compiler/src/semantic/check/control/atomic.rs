//! Atomic targets and their ordinary reference, effect and scope judgments [SHARE-2].
use super::super::super::model::{
    BindingId, CheckedArrayRoot, CheckedExpression, CheckedMode, CheckedNominalKind,
    CheckedRangeSource, CheckedSetTarget, CheckedShared, CheckedStatePath, CheckedStatement,
    CheckedTarget, CheckedTargetKind, CheckedType, IntegerType, expression_children,
};
use super::super::super::places::{CapturedValue, PlaceRoot, PlaceStep, ResolvedPlace};
use super::super::expressions::calls::user::WAIT1_DECLARE_THE_CALLER_WAITING;
use super::super::references::{ReferenceInfo, ReferenceKind};
use super::super::{CheckStop, Checker, EffectSet, LocalBinding};
use super::{ControlCounters, ControlScope, StatementResult};
use crate::semantic::check::FunctionContext;
use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};
use std::collections::{HashMap, HashSet};
pub(in crate::semantic::check) const SHARE2_NAME_A_SHARED_HANDLE: &str = "name a place of type `Shared<T>`: create the object with `shared_new`, or with `shared_map_new` for a map, and give each context its own handle made with `shared_share`";
pub(in crate::semantic::check) const SHARE2_KEY_WITHOUT_MOVE: &str = "name the key set without `move`: the statement reads it when it begins and the binding over it stays valid while the set is not written";
pub(in crate::semantic::check) const SHARE2_KEY_A_BYTE_RANGE: &str = "name one key as a `&[u8]` range, such as `&bytes[start..end]` or a reference variable holding one, or several keys as a place of type `KeySet` built before the statement, such as `keys` or, through a reference to one, `keys^`";
pub(in crate::semantic::check) const SHARE2_KEY_BEFORE_THE_STATEMENT: &str = "read handles and keys before the statement: copy a handle held in a state out with `shared_share` in an earlier atomic statement, and compute a key that comes from a state into a local the same way; a statement reads its handles and keys when it begins, before it holds any state";
pub(in crate::semantic::check) const SHARE2_WAIT_OUTSIDE_THE_BLOCK: &str = "move the waiting call out of the atomic statement: end the statement first, wait, and start another atomic statement for any update that depends on the outcome";
pub(in crate::semantic::check) const SHARE2_END_THE_OUTER_STATEMENT: &str = "name both handles as targets of one statement, `atomic outer = &first, inner = &second { … }`, or end the outer statement before starting the inner one";
pub(in crate::semantic::check) const SHARE2_READ_ONLY_GUARD: &str = "make the guard read only, calling a function whose row writes nothing and moves no argument, and make the update in the block";
pub(in crate::semantic::check) const SHARE2_USE_THE_BINDING: &str = "remove the target, or the whole statement when no target is used; a statement holds what its targets name, so a target nothing uses holds a state or entries for nothing";
struct IndexHeader {
    atom: Option<NodeId>,
}
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
        let places = self
            .types
            .declarations
            .tree
            .children_with(node, Production::Place)?;
        let declarations = self
            .types
            .declarations
            .declarations_at(node, DeclarationRole::AtomicBinder)?;
        if places.len() != declarations.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let base_keys = bindings.keys().copied().collect::<Vec<_>>();
        let preserved = base_keys.iter().copied().collect::<HashSet<_>>();
        let mut block_bindings = bindings.clone();
        let mut effects = EffectSet::NONE;
        let mut targets: Vec<CheckedTarget> = Vec::new();
        let mut handles: Vec<Vec<ResolvedPlace>> = Vec::new();
        let mut statement_roots = Vec::new();
        for (place, declaration) in places.iter().zip(&declarations) {
            let suffixes = self
                .types
                .declarations
                .tree
                .children_with(*place, Production::Psuffix)?;
            let atom = suffixes
                .last()
                .map(|last| self.types.declarations.tree.subscript_offset(*last))
                .transpose()?
                .flatten();
            let header = IndexHeader { atom };
            let borrow = if atom.is_some() {
                Self::check_place_borrow_prefix
            } else {
                Self::check_place_borrow
            };
            let target = borrow(
                self,
                context,
                *place,
                *place,
                *place,
                &block_bindings,
                scope.loops.len(),
            )?;
            let paths = target
                .reference
                .as_ref()
                .map(|r| r.paths.clone())
                .unwrap_or_default();
            if paths.iter().any(|p| matches!(p.root, PlaceRoot::Binding(root) if targets.iter().any(|t| t.binding == root))) || target.effects.reads.iter().any(|p| statement_roots.contains(&p.root)) {
                return self.types.declarations.issue_node(SemanticRule::Share2, *place, SemanticIssueKind::AtomicKeyReadsTheState { mechanical_fix: SHARE2_KEY_BEFORE_THE_STATEMENT });
            }
            let state = match (target.mode, target.expression.ty()) {
                (CheckedMode::Reference, CheckedType::Nominal(nominal)) => {
                    match self.types.nominal(nominal)?.kind {
                        CheckedNominalKind::Shared {
                            state,
                            shape: CheckedShared::Object,
                        } => Some(state),
                        _ => None,
                    }
                }
                _ => None,
            };
            let Some(state) = state else {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    *place,
                    SemanticIssueKind::AtomicTargetNotShared {
                        found: self
                            .types
                            .checked_value_name(target.mode, target.expression.ty())?,
                        mechanical_fix: SHARE2_NAME_A_SHARED_HANDLE,
                    },
                );
            };
            for (prior, prior_target) in handles.iter().zip(&targets) {
                if (atom.is_none()
                    || matches!(
                        prior_target.kind,
                        CheckedTargetKind::Object | CheckedTargetKind::MapWhole
                    ))
                    && prior.iter().any(|a| {
                        paths.iter().any(|b| {
                            super::super::super::places::places_overlap(
                                &super::super::super::places::UnprovedSeparations,
                                a,
                                b,
                            )
                        })
                    })
                {
                    return self.types.declarations.issue_node(SemanticRule::Share2, *place, SemanticIssueKind::AtomicTargetNotShared { found: "a whole target beside another target on an overlapping handle place".to_owned(), mechanical_fix: "hold a handle whole once, or name its entries with entry and key-set targets" });
                }
            }
            let borrowed = match paths.as_slice() {
                [p] => match p.root {
                    PlaceRoot::Binding(root) => bindings.values().any(|l| {
                        l.binding == root
                            && l.mode.is_reference()
                            && function
                                .parameters
                                .iter()
                                .any(|p| p.declaration == l.declaration)
                            && !function
                                .declared_effects
                                .writes
                                .iter()
                                .any(|p| p.root == l.declaration)
                    }),
                    PlaceRoot::Constant(_) => false,
                },
                _ => false,
            };
            effects = effects.union(target.effects.clone());
            for p in &paths {
                for path in self.effect_paths_for_place(node, p, &block_bindings)? {
                    effects.add_read(path);
                }
            }
            let map_entry = self.table_entry_type(state);
            let earlier = targets.iter().map(|t| t.binding).collect::<Vec<_>>();
            let (kind, referent, anchors) = match (map_entry, atom) {
                (Some(entry), Some(_)) => self.check_target_index(context, node, &header, &earlier, &statement_roots, &mut block_bindings, scope.loops.len(), &mut effects, entry)?,
                (Some(_), None) => (CheckedTargetKind::MapWhole, state, Vec::new()),
                (None, None) => (CheckedTargetKind::Object, state, Vec::new()),
                (None, Some(_)) => return self.types.declarations.issue_node(SemanticRule::Share2, *place, SemanticIssueKind::AtomicTargetNotShared { found: "an indexed handle whose state is no concurrent hash map".to_owned(), mechanical_fix: "write the target as `name = &handle` and reach the state through `name^`; an index step names entries of a `ConcurrentHashMap<V>` state alone" }),
            };
            let binding = self.bind_atomic_reference(
                counters,
                &mut block_bindings,
                declaration,
                referent,
                scope.loops.len(),
            )?;
            if let Some(reference) = block_bindings
                .get_mut(&declaration.id())
                .and_then(|l| l.reference.as_mut())
            {
                reference.anchors = anchors;
                if matches!(
                    kind,
                    CheckedTargetKind::MapEntry(_) | CheckedTargetKind::MapSet(_)
                ) {
                    reference.paths[0]
                        .path
                        .push(PlaceStep::Index(CapturedValue::unknown()));
                }
                self.body
                    .record_reference_origins(binding, &reference.paths);
            }
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
            targets.push(CheckedTarget {
                lock_order: self.types.atomic_type_order(state)?,
                node_path: self.types.declarations.tree.path(*place)?.clone(),
                binding,
                handle: Box::new(target.expression),
                borrowed,
                state,
                kind,
                referent,
                reads: false,
                invariants,
            });
            handles.push(paths);
            statement_roots.push(declaration.id());
        }
        // Pairwise root relation, never a transitive class [SHARE-2].
        for (declaration, target) in declarations.iter().zip(&targets) {
            let mut aliases = Vec::new();
            for other in &targets {
                if self.types.types_unify(target.state, other.state)? {
                    aliases.push(other.binding);
                }
            }
            if aliases.len() > 1 {
                self.body
                    .range_facts
                    .atomic_aliases
                    .insert(node_path.clone());
            }
            let reference = block_bindings
                .get_mut(&declaration.id())
                .and_then(|l| l.reference.as_mut())
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for path in &mut reference.paths {
                path.atomic_aliases.clone_from(&aliases);
            }
            self.body.reference_origins[target.binding.0 as usize].clear();
            self.body
                .record_reference_origins(target.binding, &reference.paths);
        }
        self.body.atomic_depth += 1;
        let checked = self.check_atomic_parts(context, node, &mut block_bindings, counters, scope);
        self.body.atomic_depth -= 1;
        let (mut guard, mut checked) = checked?;
        for (target, place) in targets.iter_mut().zip(&places) {
            if !guard
                .as_ref()
                .is_some_and(|g| expression_mentions(&g.0, target.binding))
                && !checked
                    .statements
                    .iter()
                    .any(|s| statement_mentions(s, target.binding))
            {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    *place,
                    SemanticIssueKind::AtomicBindingUnused {
                        binding: counters.binding_names[target.binding.0 as usize].clone(),
                        mechanical_fix: SHARE2_USE_THE_BINDING,
                    },
                );
            }
            let aliases =
                targets_alias_declarations(&declarations, &block_bindings, target.binding);
            target.reads = !checked
                .effects
                .writes
                .iter()
                .chain(guard.iter().flat_map(|g| g.1.writes.iter()))
                .any(|p| aliases.contains(&p.root));
        }
        let held = |path: &CheckedStatePath| statement_roots.contains(&path.root);
        for set in std::iter::once(&mut checked.effects).chain(guard.iter_mut().map(|g| &mut g.1)) {
            set.reads.retain(|p| !held(p));
            set.writes.retain(|p| !held(p));
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
                targets,
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
                reference: Some(reference),
                refinement_witnesses: Vec::new(),
                call_value: false,
            },
        );
        Ok(binding)
    }
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn check_target_index(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        header: &IndexHeader,
        earlier: &[BindingId],
        statement_roots: &[DeclarationId],
        block_bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        effects: &mut EffectSet,
        entry_type: CheckedType,
    ) -> Result<(CheckedTargetKind, CheckedType, Vec<ResolvedPlace>), CheckStop> {
        let FunctionContext { check_context, .. } = context;
        let atom = header
            .atom
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        // An index atom is a place or a range the statement reads when it
        // begins; a `move` is neither.
        if self
            .types
            .declarations
            .tree
            .has_fixed(atom, crate::syntax::terminal::FixedTerminal::Move)?
        {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                atom,
                SemanticIssueKind::AtomicKeyNotBytes {
                    found: "a value `move` takes".to_owned(),
                    mechanical_fix: SHARE2_KEY_WITHOUT_MOVE,
                },
            );
        }
        // The statement reads its index atoms when it begins, before it holds
        // the state, so none reads through its bindings.
        // A place the atom names, and anything its range's endpoints read.
        let touches_the_state = |effects: &EffectSet| {
            effects
                .reads
                .iter()
                .chain(&effects.writes)
                .any(|path| statement_roots.contains(&path.root))
        };
        let reads_the_state = |places: &[ResolvedPlace]| {
            places.iter().any(
                |place| matches!(place.root, PlaceRoot::Binding(root) if earlier.contains(&root)),
            )
        };
        // The index atom: a reference variable or borrow of one key, or a
        // place holding a key set, which the statement borrows.
        let atom_place = self
            .types
            .declarations
            .tree
            .first_child_with(atom, Production::Place)?;
        let set_place = match atom_place {
            Some(place) => {
                let suffixes = self
                    .types
                    .declarations
                    .tree
                    .children_with(place, Production::Psuffix)?;
                let pbase = self
                    .types
                    .declarations
                    .tree
                    .first_child_with(place, Production::Pbase)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let root = self.types.declarations.use_at(
                    check_context,
                    pbase,
                    LexicalUseRole::PlaceBase,
                )?;
                let reference_root = match root.target() {
                    ResolvedTarget::Source {
                        declaration,
                        class: DeclarationClass::Value,
                    } => block_bindings
                        .get(&declaration)
                        .is_some_and(|local| local.reference.is_some()),
                    _ => false,
                };
                (!(reference_root && suffixes.is_empty())).then_some(place)
            }
            None => None,
        };
        let (index, referent, set_places) = if let Some(place) = set_place {
            let set =
                self.check_place_borrow(context, atom, atom, place, block_bindings, loop_depth)?;
            if set.expression.ty() != CheckedType::KeySet {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    atom,
                    SemanticIssueKind::AtomicKeyNotBytes {
                        found: self
                            .types
                            .checked_value_name(set.mode, set.expression.ty())?,
                        mechanical_fix: SHARE2_KEY_A_BYTE_RANGE,
                    },
                );
            }
            let places = set
                .reference
                .as_ref()
                .map(|reference| reference.paths.clone())
                .unwrap_or_default();
            if reads_the_state(&places) || touches_the_state(&set.effects) {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    atom,
                    SemanticIssueKind::AtomicKeyReadsTheState {
                        mechanical_fix: SHARE2_KEY_BEFORE_THE_STATEMENT,
                    },
                );
            }
            for place in &places {
                for path in self.effect_paths_for_place(node, place, block_bindings)? {
                    effects.add_read(path);
                }
            }
            *effects = effects.clone().union(set.effects.clone());
            let referent = CheckedType::Entries {
                element: self.types.intern_element(entry_type)?,
            };
            (
                CheckedTargetKind::MapSet(Box::new(set.expression)),
                referent,
                places,
            )
        } else {
            let mut probe = block_bindings.clone();
            let key = self.check_atom(context, atom, &mut probe, loop_depth)?;
            let bytes = key.mode == CheckedMode::Range
                && key.expression.ty() == CheckedType::Integer(IntegerType::U8);
            if !bytes {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    atom,
                    SemanticIssueKind::AtomicKeyNotBytes {
                        found: self
                            .types
                            .checked_value_name(key.mode, key.expression.ty())?,
                        mechanical_fix: SHARE2_KEY_A_BYTE_RANGE,
                    },
                );
            }
            if reads_the_state(
                key.reference
                    .as_ref()
                    .map(|reference| reference.paths.as_slice())
                    .unwrap_or_default(),
            ) || touches_the_state(&key.effects)
            {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    atom,
                    SemanticIssueKind::AtomicKeyReadsTheState {
                        mechanical_fix: SHARE2_KEY_BEFORE_THE_STATEMENT,
                    },
                );
            }
            // [SHARE-2] the statement reads the bytes the key names.
            for place in key
                .reference
                .as_ref()
                .map(|reference| reference.paths.as_slice())
                .unwrap_or_default()
            {
                for path in self.effect_paths_for_place(node, place, block_bindings)? {
                    effects.add_read(path);
                }
            }
            *effects = effects.clone().union(key.effects.clone());
            (
                CheckedTargetKind::MapEntry(Box::new(key.expression)),
                entry_type,
                Vec::new(),
            )
        };
        Ok((
            index,
            referent,
            set_places
                .into_iter()
                .map(|mut p| {
                    p.path.push(PlaceStep::Index(CapturedValue::unknown()));
                    p
                })
                .collect(),
        ))
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

/// Whether a checked statement, or a block it holds, names `binding`.
fn statement_mentions(statement: &CheckedStatement, binding: BindingId) -> bool {
    let expression = |value: &CheckedExpression| expression_mentions(value, binding);
    let block = |statements: &[CheckedStatement]| {
        statements
            .iter()
            .any(|statement| statement_mentions(statement, binding))
    };
    match statement {
        CheckedStatement::Let { value, .. }
        | CheckedStatement::DestructuringLet { value, .. }
        | CheckedStatement::Evaluate { value, .. }
        | CheckedStatement::DropExpression { value, .. }
        | CheckedStatement::Return { value, .. }
        | CheckedStatement::Give { value, .. } => expression(value),
        CheckedStatement::PropagateLet { scrutinee, .. } => expression(scrutinee),
        CheckedStatement::Set { target, value, .. } => {
            let target = match target {
                CheckedSetTarget::Place(place) => place.binding == binding,
                CheckedSetTarget::RangeIndex(place) => {
                    place.root.binding == binding || place.offsets().any(expression)
                }
                CheckedSetTarget::Storage(root) => {
                    root.binding() == Some(binding) || root.offsets().any(expression)
                }
            };
            target || expression(value)
        }
        CheckedStatement::Match {
            scrutinee, arms, ..
        }
        | CheckedStatement::ValueMatchLet {
            scrutinee, arms, ..
        } => expression(scrutinee) || arms.iter().any(|arm| block(&arm.body)),
        CheckedStatement::Loop { body, .. } => block(body),
        CheckedStatement::CountedRange {
            lower, upper, body, ..
        } => expression(lower) || expression(upper) || block(body),
        CheckedStatement::Atomic {
            targets,
            guard,
            body,
            ..
        } => {
            targets
                .iter()
                .flat_map(CheckedTarget::expressions)
                .any(expression)
                || guard.as_deref().is_some_and(expression)
                || block(body)
        }
        CheckedStatement::Proof(_)
        | CheckedStatement::Break { .. }
        | CheckedStatement::Continue { .. } => false,
    }
}

/// Whether a checked expression names `binding` as a place's root or as a
/// value.
fn expression_mentions(expression: &CheckedExpression, binding: BindingId) -> bool {
    let names = match expression {
        CheckedExpression::Binding { binding: named, .. }
        | CheckedExpression::DerefAddressed { binding: named, .. }
        | CheckedExpression::Project { binding: named, .. }
        | CheckedExpression::BoxTake { binding: named, .. } => *named == binding,
        CheckedExpression::BorrowAddressed { root, .. }
        | CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. }
        | CheckedExpression::BorrowSegment { root, .. } => root.binding() == Some(binding),
        CheckedExpression::ArrayMeasure { root, .. }
        | CheckedExpression::ArrayIndex { root, .. } => {
            matches!(root, CheckedArrayRoot::Binding { binding: named, .. } if *named == binding)
        }
        CheckedExpression::BufferMeasure { root, .. }
        | CheckedExpression::BufferIndex { root, .. } => root.binding == binding,
        CheckedExpression::RangeMeasure { root, .. } => root.binding == binding,
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. }
        | CheckedExpression::BorrowRangeIndex { place, .. } => place.root.binding == binding,
        CheckedExpression::RangeOf { source, .. } => match source {
            CheckedRangeSource::Storage(root) => root.binding() == Some(binding),
            CheckedRangeSource::Range(root) => root.binding == binding,
            CheckedRangeSource::Element(place) => place.root.binding == binding,
        },
        _ => false,
    };
    names
        || expression_children(expression)
            .into_iter()
            .any(|child| expression_mentions(child, binding))
}

fn targets_alias_declarations(
    declarations: &[&crate::DeclarationRecord],
    bindings: &HashMap<DeclarationId, LocalBinding>,
    binding: BindingId,
) -> Vec<DeclarationId> {
    let aliases = declarations
        .iter()
        .filter_map(|d| bindings.get(&d.id()))
        .find(|l| l.binding == binding)
        .and_then(|l| l.reference.as_ref())
        .and_then(|r| r.paths.first())
        .map(|p| p.atomic_aliases.as_slice())
        .unwrap_or_default();
    declarations
        .iter()
        .filter(|d| {
            bindings
                .get(&d.id())
                .is_some_and(|l| aliases.contains(&l.binding))
        })
        .map(|d| d.id())
        .collect()
}
