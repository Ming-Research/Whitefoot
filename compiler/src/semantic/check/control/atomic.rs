//! [SHARE-2] the atomic statement: its target, its binding, its entry
//! bindings, its guard and its block.

use crate::semantic::check::FunctionContext;
use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, LexicalUseRole, Production,
    ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::super::model::{
    BindingId, CheckedEntryBinding, CheckedEntryIndex, CheckedExpression, CheckedMode,
    CheckedNominalKind, CheckedShared, CheckedStatePath, CheckedStatement, CheckedType,
    IntegerType, expression_children,
};
use super::super::super::places::{PlaceRoot, ResolvedPlace};
use super::super::expressions::calls::user::WAIT1_DECLARE_THE_CALLER_WAITING;
use super::super::references::{ReferenceInfo, ReferenceKind};
use super::super::{CheckStop, Checker, EffectSet, LocalBinding};
use super::{ControlCounters, ControlScope, StatementResult};

/// The repair for a target that is not a `Shared<T>` place [SHARE-2, DIAG-1].
pub(in crate::semantic::check) const SHARE2_NAME_A_SHARED_HANDLE: &str = "name a place of type `Shared<T>`: create the object with `shared_new`, and give each context its own handle made with `shared_share`";
/// The repair for an entry binding whose place is not an entry of a keyed
/// table reached through the statement's binding [SHARE-2].
pub(in crate::semantic::check) const SHARE2_NAME_A_TABLE_ENTRY: &str = "write the entry binding as `name = &s^.table[key]`, where `s` is the statement's binding and `table` a field of type `KeyedTable<V>` in its state";
/// The repair for an index atom that is neither a byte range nor a key set
/// [SHARE-2].
pub(in crate::semantic::check) const SHARE2_KEY_A_BYTE_RANGE: &str = "name one key as a `&[u8]` range, such as `&bytes[start..end]` or a reference variable holding one, or several keys as a place of type `KeySet` built before the statement";
/// The repair for a waiting call inside an atomic statement [SHARE-2].
pub(in crate::semantic::check) const SHARE2_WAIT_OUTSIDE_THE_BLOCK: &str = "move the waiting call out of the atomic statement: end the statement first, wait, and start another atomic statement for any update that depends on the outcome";
/// The repair for an atomic statement inside another [SHARE-2].
pub(in crate::semantic::check) const SHARE2_END_THE_OUTER_STATEMENT: &str = "end the outer atomic statement before starting the inner one, carrying what the inner one needs in a local; a state's tables and fields are all reached through the one statement's binding and its entry bindings";
/// The repair for a guard that writes [SHARE-2].
pub(in crate::semantic::check) const SHARE2_READ_ONLY_GUARD: &str = "make the guard read only, calling a function whose row writes nothing and moves no argument, and make the update in the block";
/// The repair for an entry binding the guard and block never use, and for a
/// statement whose guard and block reach nothing of the state [SHARE-2].
pub(in crate::semantic::check) const SHARE2_USE_THE_BINDING: &str = "remove the entry binding, or the whole statement when nothing in it reaches the state; an atomic statement holds what it names, so a binding nothing uses holds an entry for nothing";
/// The repair for a key set written while an entry binding names its keys
/// [SHARE-2, REF-2].
pub(in crate::semantic::check) const SHARE2_KEEP_THE_KEY_SET: &str = "build the key set completely before the statement and leave it unchanged inside: its keys were locked when the statement began";

/// One entry binding as the header writes it: its declaration, its place
/// and the index atom of the place's last step.
struct EntryHeader<'unit> {
    declaration: &'unit crate::DeclarationRecord,
    place: NodeId,
    atom: NodeId,
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
        let places = self
            .types
            .declarations
            .tree
            .children_with(node, Production::Place)?;
        let (target_place, entry_places) = places
            .split_first()
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // [SHARE-2] no atomic statement inside another's guard or block; the
        // inner statement is the offending one.
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
        // [WAIT-1, SHARE-2] every atomic statement counts as a waiting call.
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
        // [SHARE-2] the target, read when the statement begins.
        let target = self.check_place_borrow(
            context,
            node,
            node,
            *target_place,
            bindings,
            scope.loops.len(),
        )?;
        let state = match (target.mode, target.expression.ty()) {
            (CheckedMode::Reference, CheckedType::Nominal(nominal)) => {
                match self.types.nominal(nominal)?.kind.clone() {
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
        // [SHARE-2] a handle the caller lends through a reference parameter
        // whose row writes nothing below it stays live for the whole call,
        // so the statement needs none of its own.
        let borrowed = match target
            .reference
            .as_ref()
            .map(|reference| reference.paths.as_slice())
        {
            Some([place]) => match place.root {
                PlaceRoot::Binding(root) => bindings.values().any(|local| {
                    local.binding == root
                        && local.mode.is_reference()
                        && function
                            .parameters
                            .iter()
                            .any(|parameter| parameter.declaration == local.declaration)
                        && !function
                            .declared_effects
                            .writes
                            .iter()
                            .any(|path| path.root == local.declaration)
                }),
                PlaceRoot::Constant(_) => false,
            },
            _ => false,
        };

        // [SHARE-2] the binding and the entry bindings: the first IDENT
        // names the state, every later one an entry binding, in written
        // order.
        let declarations = self
            .types
            .declarations
            .declarations_at(node, DeclarationRole::AtomicBinder)?;
        let (declaration, entry_declarations) = declarations
            .split_first()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if entry_declarations.len() != entry_places.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let base_keys = bindings.keys().copied().collect::<Vec<_>>();
        let preserved = base_keys.iter().copied().collect::<HashSet<_>>();
        let mut block_bindings = bindings.clone();
        // The binding anchors at itself, as a reference parameter does: the
        // state belongs to no binding [SHARE-1], so no path reaches it except
        // through this binder.
        let binding = self.bind_atomic_reference(
            counters,
            &mut block_bindings,
            declaration,
            state,
            scope.loops.len(),
        )?;
        let headers = entry_declarations
            .iter()
            .zip(entry_places)
            .map(|(declaration, place)| {
                let suffixes = self
                    .types
                    .declarations
                    .tree
                    .children_with(*place, Production::Psuffix)?;
                let atom = match suffixes.last() {
                    Some(last) => self.types.declarations.tree.subscript_offset(*last)?,
                    None => None,
                };
                Ok(atom.map(|atom| EntryHeader {
                    declaration,
                    place: *place,
                    atom,
                }))
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        let mut entries = Vec::with_capacity(headers.len());
        let mut sets = Vec::new();
        for (header, place) in headers.into_iter().zip(entry_places) {
            let Some(header) = header else {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    *place,
                    SemanticIssueKind::AtomicTargetNotShared {
                        found: "a place whose last step is no index".to_owned(),
                        mechanical_fix: SHARE2_NAME_A_TABLE_ENTRY,
                    },
                );
            };
            let (entry, set_paths) = self.check_entry_binding(
                context,
                node,
                &header,
                binding,
                &mut block_bindings,
                counters,
                scope.loops.len(),
                &mut effects,
            )?;
            sets.extend(set_paths);
            entries.push(entry);
        }

        self.body.atomic_depth += 1;
        let checked = self.check_atomic_parts(context, node, &mut block_bindings, counters, scope);
        self.body.atomic_depth -= 1;
        let (mut guard, mut checked) = checked?;
        let state_root = declaration.id();
        let entry_roots = entry_declarations
            .iter()
            .map(|declaration| declaration.id())
            .collect::<Vec<_>>();
        let touched = |root: DeclarationId,
                       block: &EffectSet,
                       guard: &Option<(CheckedExpression, EffectSet)>| {
            block
                .reads
                .iter()
                .chain(&block.writes)
                .chain(
                    guard
                        .iter()
                        .flat_map(|guard| guard.1.reads.iter().chain(&guard.1.writes)),
                )
                .any(|path| path.root == root)
        };
        // [SHARE-2] the guard and block use every entry binding, and reach
        // the state through the binding or an entry binding.
        for ((entry, root), place) in entries.iter().zip(&entry_roots).zip(entry_places) {
            if !touched(*root, &checked.effects, &guard) {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    *place,
                    SemanticIssueKind::AtomicBindingUnused {
                        binding: counters
                            .binding_names
                            .get(entry.binding.0 as usize)
                            .cloned()
                            .unwrap_or_default(),
                        mechanical_fix: SHARE2_USE_THE_BINDING,
                    },
                );
            }
        }
        if entries.is_empty() && !touched(state_root, &checked.effects, &guard) {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                node,
                SemanticIssueKind::AtomicBindingUnused {
                    binding: declaration.spelling().to_owned(),
                    mechanical_fix: SHARE2_USE_THE_BINDING,
                },
            );
        }
        // [SHARE-2, REF-2] a key set an entry binding names stays unchanged
        // while the binding is valid, which is the whole block.
        let set_written = checked
            .effects
            .writes
            .iter()
            .chain(guard.iter().flat_map(|guard| guard.1.writes.iter()))
            .any(|write| sets.iter().any(|set| state_paths_overlap(write, set)));
        if set_written {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                node,
                SemanticIssueKind::AtomicKeySetWritten {
                    mechanical_fix: SHARE2_KEEP_THE_KEY_SET,
                },
            );
        }
        // [SHARE-3] an entry binding through which the guard and block write
        // nothing only reads its entries, so its reads take effect at one
        // point whichever other such statements on those keys run beside it.
        for (entry, root) in entries.iter_mut().zip(&entry_roots) {
            entry.reads = !checked
                .effects
                .writes
                .iter()
                .chain(guard.iter().flat_map(|guard| guard.1.writes.iter()))
                .any(|path| path.root == *root);
        }
        // The state belongs to no binding and no caller [SHARE-1], so no row
        // names a path rooted at the binding or an entry binding.
        let held = |path: &CheckedStatePath| {
            path.root == state_root || entry_roots.contains(&path.root)
        };
        for set in
            std::iter::once(&mut checked.effects).chain(guard.iter_mut().map(|guard| &mut guard.1))
        {
            set.reads.retain(|path| !held(path));
            set.writes.retain(|path| !held(path));
        }
        if let Some(guard) = &guard {
            effects = effects.union(guard.1.clone());
        }
        effects = effects.union(checked.effects);

        // [REF-2] the binders' roots leave scope when the block ends by any
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
        let invariants = self.atomic_invariants(state, binding);
        Ok(StatementResult {
            statement: CheckedStatement::Atomic {
                node_path,
                target: Box::new(target.expression),
                borrowed,
                binding,
                state,
                entries,
                guard: guard.map(|guard| Box::new(guard.0)),
                body: checked.statements,
                fallthrough_drops,
                continues: checked.can_continue,
                invariants,
            },
            can_continue: checked.can_continue,
            effects,
            all_paths_deliver: !checked.can_continue && checked.all_paths_deliver,
            direct_give: false,
            give_states: checked.give_states,
            break_states: checked.break_states,
        })
    }

    /// A reference binder of an atomic statement's header, anchored at
    /// itself, in scope for the guard and the block.
    fn bind_atomic_reference(
        &mut self,
        counters: &mut ControlCounters<'_>,
        block_bindings: &mut HashMap<DeclarationId, LocalBinding>,
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
        block_bindings.insert(
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

    /// [SHARE-2] one entry binding: its place is the statement's binding,
    /// `^`, fields ending at a `KeyedTable<V>`, and one index whose atom has
    /// type `&[u8]` or is a place of type `KeySet`. Returns the checked
    /// binding and the paths of the key set it names, if any.
    #[allow(clippy::too_many_arguments)]
    fn check_entry_binding(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        header: &EntryHeader<'_>,
        state_binding: BindingId,
        block_bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        loop_depth: usize,
        effects: &mut EffectSet,
    ) -> Result<(CheckedEntryBinding, Vec<CheckedStatePath>), CheckStop> {
        let FunctionContext { check_context, .. } = context;
        let shape = |found: &str| SemanticIssueKind::AtomicTargetNotShared {
            found: found.to_owned(),
            mechanical_fix: SHARE2_NAME_A_TABLE_ENTRY,
        };
        let table = self.check_place_borrow_prefix(
            context,
            header.place,
            header.place,
            header.place,
            block_bindings,
            loop_depth,
        )?;
        // The table part is reached through the statement's own binding,
        // `s^` followed by field selections.
        let through_state = matches!(
            table.reference.as_ref().map(|reference| reference.paths.as_slice()),
            Some([place]) if place.root == PlaceRoot::Binding(state_binding)
        );
        let entry_type = match (table.mode, table.expression.ty()) {
            (CheckedMode::Reference, CheckedType::Nominal(nominal)) => {
                match self.types.nominal(nominal)?.kind.clone() {
                    CheckedNominalKind::Shared {
                        shape: CheckedShared::Table { entry },
                        ..
                    } => Some(entry),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(entry_type) = entry_type else {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                header.place,
                shape("a place whose indexed part is no keyed table"),
            );
        };
        if !through_state {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                header.place,
                shape("a keyed table not reached through the statement's binding"),
            );
        }
        // The index atom: a reference variable or borrow of one key, or a
        // place holding a key set, which the statement borrows.
        let atom_place = self
            .types
            .declarations
            .tree
            .first_child_with(header.atom, Production::Place)?;
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
                let root = self
                    .types
                    .declarations
                    .use_at(check_context, pbase, LexicalUseRole::PlaceBase)?;
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
        let (index, referent, set_paths) = if let Some(place) = set_place {
            let set = self.check_place_borrow(
                context,
                header.atom,
                header.atom,
                place,
                block_bindings,
                loop_depth,
            )?;
            if set.expression.ty() != CheckedType::KeySet {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    header.atom,
                    SemanticIssueKind::AtomicKeyNotBytes {
                        found: self
                            .types
                            .checked_value_name(set.mode, set.expression.ty())?,
                        mechanical_fix: SHARE2_KEY_A_BYTE_RANGE,
                    },
                );
            }
            let mut paths = Vec::new();
            for place in set
                .reference
                .as_ref()
                .map(|reference| reference.paths.as_slice())
                .unwrap_or_default()
            {
                for path in self.effect_paths_for_place(node, place, block_bindings)? {
                    paths.push(path.path.clone());
                    effects.add_read(path);
                }
            }
            *effects = effects.clone().union(set.effects.clone());
            let referent = CheckedType::KeyedEntries {
                element: self.types.intern_element(entry_type)?,
            };
            (
                CheckedEntryIndex::Set(Box::new(set.expression)),
                referent,
                paths,
            )
        } else {
            let mut probe = block_bindings.clone();
            let key = self.check_atom(context, header.atom, &mut probe, loop_depth)?;
            let bytes = key.mode == CheckedMode::Range
                && key.expression.ty() == CheckedType::Integer(IntegerType::U8);
            if !bytes {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    header.atom,
                    SemanticIssueKind::AtomicKeyNotBytes {
                        found: self
                            .types
                            .checked_value_name(key.mode, key.expression.ty())?,
                        mechanical_fix: SHARE2_KEY_A_BYTE_RANGE,
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
                CheckedEntryIndex::Key(Box::new(key.expression)),
                entry_type,
                Vec::new(),
            )
        };
        let binding = self.bind_atomic_reference(
            counters,
            block_bindings,
            header.declaration,
            referent,
            loop_depth,
        )?;
        Ok((
            CheckedEntryBinding {
                node_path: self.types.declarations.tree.path(header.place)?.clone(),
                binding,
                table: Box::new(table.expression),
                entry: entry_type,
                index,
                referent,
                reads: false,
            },
            set_paths,
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

/// [OWN-7] whether two state paths overlap: one root, one path a prefix of
/// the other.
fn state_paths_overlap(left: &CheckedStatePath, right: &CheckedStatePath) -> bool {
    left.root == right.root
        && (left.steps.starts_with(&right.steps) || right.steps.starts_with(&left.steps))
}
