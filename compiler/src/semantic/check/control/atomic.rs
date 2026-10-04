//! [SHARE-2] the atomic statement: its target, its binding, its entry
//! bindings, its guard and its block.

use crate::semantic::check::FunctionContext;
use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::super::model::{
    BindingId, CheckedArrayRoot, CheckedEntryBinding, CheckedEntryIndex, CheckedExpression,
    CheckedMode, CheckedNominalKind, CheckedPlaceStep, CheckedRangeSource, CheckedSetTarget,
    CheckedShared, CheckedStatePath, CheckedStatement, CheckedType, IntegerType,
    expression_children,
};
use super::super::super::places::{CapturedValue, PlaceRoot, PlaceStep, ResolvedPlace};
use super::super::expressions::calls::user::WAIT1_DECLARE_THE_CALLER_WAITING;
use super::super::references::{ReferenceInfo, ReferenceKind};
use super::super::{CheckStop, Checker, EffectSet, LocalBinding};
use super::{ControlCounters, ControlScope, StatementResult};

/// The repair for a target that is not a `Shared<T>` place [SHARE-2, DIAG-1].
pub(in crate::semantic::check) const SHARE2_NAME_A_SHARED_HANDLE: &str = "name a place of type `Shared<T>`: create the object with `shared_new`, and give each context its own handle made with `shared_share`";
/// The repair for an entry binding whose place is not an entry of a keyed
/// table reached through the statement's binding [SHARE-2].
pub(in crate::semantic::check) const SHARE2_NAME_A_TABLE_ENTRY: &str = "write a table binding as `name = &s^.table[key]` for one entry, `name = &s^.table[keys]` for the entries under a key set, or `name = &s^.table` for the table whole, where `s` is the statement's binding and `table` a field of type `KeyedTable<V>` reached through no `Box`";
pub(in crate::semantic::check) const SHARE2_MOVE_HIDDEN_TABLE: &str = "move the table to a state field reached through no `Box` and no enum payload, then bind it whole as `t = &s^.field` in the header and reach it through `t`";
/// The repair for an index atom that is neither a byte range nor a key set
/// [SHARE-2].
pub(in crate::semantic::check) const SHARE2_KEY_A_BYTE_RANGE: &str = "name one key as a `&[u8]` range, such as `&bytes[start..end]` or a reference variable holding one, or several keys as a place of type `KeySet` built before the statement, such as `keys` or, through a reference to one, `keys^`";
/// [SHARE-2] repair for an index atom that reads the state.
pub(in crate::semantic::check) const SHARE2_KEY_BEFORE_THE_STATEMENT: &str = "compute the key into a local before the statement, through an earlier atomic statement if it comes from the state: a statement reads its keys when it begins, before it holds the state";
/// The repair for a waiting call inside an atomic statement [SHARE-2].
pub(in crate::semantic::check) const SHARE2_WAIT_OUTSIDE_THE_BLOCK: &str = "move the waiting call out of the atomic statement: end the statement first, wait, and start another atomic statement for any update that depends on the outcome";
/// The repair for an atomic statement inside another [SHARE-2].
pub(in crate::semantic::check) const SHARE2_END_THE_OUTER_STATEMENT: &str = "end the outer atomic statement before starting the inner one, carrying what the inner one needs in a local; a state's tables and fields are all reached through the one statement's binding and its entry bindings";
/// The repair for a guard that writes [SHARE-2].
pub(in crate::semantic::check) const SHARE2_READ_ONLY_GUARD: &str = "make the guard read only, calling a function whose row writes nothing and moves no argument, and make the update in the block";
/// The repair for an entry binding the guard and block never use, and for a
/// statement whose guard and block reach nothing of the state [SHARE-2].
pub(in crate::semantic::check) const SHARE2_USE_THE_BINDING: &str = "remove the table binding, or the whole statement when nothing in it reaches the state; an atomic statement holds what its header names, so a binding nothing uses holds a table or an entry for nothing";
/// One entry binding as the header writes it: its declaration, its place
/// and the index atom of the place's last step.
struct EntryHeader<'unit> {
    declaration: &'unit crate::DeclarationRecord,
    place: NodeId,
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
                Ok(EntryHeader {
                    declaration,
                    place: *place,
                    atom,
                })
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        let mut entries = Vec::with_capacity(headers.len());
        // The declarations an index atom may not read through: the binding's
        // and each earlier entry binding's.
        let mut statement_roots = vec![declaration.id()];
        for header in headers {
            let earlier = entries
                .iter()
                .map(|entry: &CheckedEntryBinding| entry.binding)
                .collect::<Vec<_>>();
            let entry = self.check_entry_binding(
                context,
                node,
                &header,
                binding,
                &earlier,
                &statement_roots,
                &mut block_bindings,
                counters,
                scope.loops.len(),
                &mut effects,
            )?;
            statement_roots.push(header.declaration.id());
            entries.push(entry);
        }

        let tables = super::super::super::model::state_parts(state, |ty| {
            use super::super::super::model::StateShape;
            let CheckedType::Nominal(nominal) = ty else {
                return StateShape::Plain;
            };
            match &self.types.nominals[nominal.0 as usize].kind {
                CheckedNominalKind::Shared {
                    shape: CheckedShared::Table { .. },
                    ..
                } => StateShape::Table(()),
                CheckedNominalKind::Struct { fields } => {
                    StateShape::Fields(fields.iter().map(|field| field.ty).collect())
                }
                _ => StateShape::Plain,
            }
        })
        .into_iter()
        .filter_map(|(path, table)| table.map(|_| path))
        .collect::<Vec<_>>();
        let names = tables
            .iter()
            .map(|path| self.atomic_table_name(state, declaration.spelling(), path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut whole = Vec::new();
        let mut named: Vec<(Vec<PlaceStep>, bool)> = Vec::new();
        for entry in &entries {
            let CheckedExpression::BorrowAddressed { root, .. } = entry.table.as_ref() else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let path = root
                .path
                .iter()
                .map(|step| step.place_step())
                .collect::<Vec<_>>();
            let is_whole = matches!(entry.index, CheckedEntryIndex::Whole);
            if named
                .iter()
                .any(|(prior, w)| *prior == path && (*w || is_whole))
            {
                let place = entry_places[entries
                    .iter()
                    .position(|e| e.binding == entry.binding)
                    .unwrap()];
                return self.types.declarations.issue_node(SemanticRule::Share2, place, SemanticIssueKind::AtomicTableBoundTwice {
                    table: self.atomic_table_name(state, declaration.spelling(), &root.path.iter().filter_map(|step| match step { CheckedPlaceStep::Field(field) => Some(*field), _ => None }).collect::<Vec<_>>())?, mechanical_fix: "name each table once: a whole binding `t = &s^.table` when the block computes its keys, reaching entries as `t^[key]` and `&t^[keys]`, or entry bindings for keys known before the statement",
                });
            }
            named.push((path.clone(), is_whole));
            if is_whole {
                whole.push((path, entry.binding));
            }
        }
        self.body.atomic_grant = Some(super::super::AtomicGrant {
            state: binding,
            tables,
            names,
            whole,
            touched: Vec::new(),
            refusals: Vec::new(),
        });
        self.body.atomic_depth += 1;
        let checked = self.check_atomic_parts(context, node, &mut block_bindings, counters, scope);
        self.body.atomic_depth -= 1;
        let grant = self
            .body
            .atomic_grant
            .take()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let (mut guard, mut checked) = checked?;
        if let Some((site, kind)) = grant.refusals.into_iter().next() {
            return self
                .types
                .declarations
                .issue_node(SemanticRule::Share2, site, kind);
        }
        let state_root = declaration.id();
        let entry_roots = entry_declarations
            .iter()
            .map(|declaration| declaration.id())
            .collect::<Vec<_>>();
        // A binder is used where the guard or the block names it: a place
        // through it, a reference formed through it, or the reference itself.
        let touched = |binding: BindingId| {
            grant.touched.contains(&binding)
                || guard
                    .as_ref()
                    .is_some_and(|guard| expression_mentions(&guard.0, binding))
                || checked
                    .statements
                    .iter()
                    .any(|statement| statement_mentions(statement, binding))
        };
        // [SHARE-2] the guard and block use every table binding, and reach
        // the state through the binding or a table binding.
        for (entry, place) in entries.iter().zip(entry_places) {
            if !touched(entry.binding) {
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
        if entries.is_empty() && !touched(binding) {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                node,
                SemanticIssueKind::AtomicBindingUnused {
                    binding: declaration.spelling().to_owned(),
                    mechanical_fix: SHARE2_USE_THE_BINDING,
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
        let held =
            |path: &CheckedStatePath| path.root == state_root || entry_roots.contains(&path.root);
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
        let mut reference =
            ReferenceInfo::formed(ReferenceKind::Single, ResolvedPlace::binding(binding));
        reference.atomic_sources.push(binding);
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
    /// type `&[u8]` or is a place of type `KeySet`.
    #[allow(clippy::too_many_arguments)]
    fn check_entry_binding(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        header: &EntryHeader<'_>,
        state_binding: BindingId,
        earlier: &[BindingId],
        statement_roots: &[DeclarationId],
        block_bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        loop_depth: usize,
        effects: &mut EffectSet,
    ) -> Result<CheckedEntryBinding, CheckStop> {
        let FunctionContext { check_context, .. } = context;
        let shape = |found: &str| SemanticIssueKind::AtomicTargetNotShared {
            found: found.to_owned(),
            mechanical_fix: SHARE2_NAME_A_TABLE_ENTRY,
        };
        let borrow = if header.atom.is_some() {
            Self::check_place_borrow_prefix
        } else {
            Self::check_place_borrow
        };
        let table = borrow(
            self,
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
        // Field selections alone reach the table, or none when the state is
        // the table: a table behind a `Box` or an index is no unit of the
        // state's layout (compiler/waiting-contexts/state-locks).
        let by_fields = table
            .reference
            .as_ref()
            .and_then(|reference| reference.paths.first())
            .is_some_and(|place| {
                place
                    .path
                    .iter()
                    .all(|step| matches!(step, PlaceStep::Field(_)))
            });
        if !by_fields {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                header.place,
                shape("a keyed table reached through a `Box` or an index"),
            );
        }
        let Some(atom) = header.atom else {
            let referent = table.expression.ty();
            let binding = self.bind_atomic_reference(
                counters,
                block_bindings,
                header.declaration,
                referent,
                loop_depth,
            )?;
            let selected = table
                .reference
                .as_ref()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .paths
                .clone();
            if let Some(reference) = block_bindings
                .get_mut(&header.declaration.id())
                .and_then(|local| local.reference.as_mut())
            {
                reference.paths.extend(selected.iter().cloned());
            }
            self.body.record_reference_origins(binding, &selected);
            return Ok(CheckedEntryBinding {
                node_path: self.types.declarations.tree.path(header.place)?.clone(),
                binding,
                table: Box::new(table.expression),
                entry: entry_type,
                index: CheckedEntryIndex::Whole,
                referent,
                reads: false,
            });
        };
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
                    mechanical_fix: SHARE2_KEY_A_BYTE_RANGE,
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
            places.iter().any(|place| {
                matches!(place.root, PlaceRoot::Binding(root) if root == state_binding || earlier.contains(&root))
            })
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
            let referent = CheckedType::KeyedEntries {
                element: self.types.intern_element(entry_type)?,
            };
            (
                CheckedEntryIndex::Set(Box::new(set.expression)),
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
        // [REF-2] the binder's selected path is an entry of the table, `s^.t`
        // followed by one index no other index is proved distinct from: a
        // write of the table, or of anything holding it, invalidates the
        // binder, and the entries of one table are taken as overlapping
        // [OWN-7], since two of the header's keys may be one. Entries over a
        // key set are anchored at its keys, so a write of the set while the
        // binder is valid invalidates it [SHARE-2], while a write through the
        // binder writes an entry and nothing of the set.
        let unknown = |mut place: ResolvedPlace| {
            place.path.push(PlaceStep::Index(CapturedValue::unknown()));
            place
        };
        let selected = vec![
            table
                .reference
                .as_ref()
                .and_then(|reference| reference.paths.first())
                .cloned()
                .map(unknown)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?,
        ];
        if let Some(reference) = block_bindings
            .get_mut(&header.declaration.id())
            .and_then(|local| local.reference.as_mut())
        {
            reference.paths.extend(selected.iter().cloned());
            reference
                .anchors
                .extend(set_places.into_iter().map(unknown));
        }
        self.body.record_reference_origins(binding, &selected);
        Ok(CheckedEntryBinding {
            node_path: self.types.declarations.tree.path(header.place)?.clone(),
            binding,
            table: Box::new(table.expression),
            entry: entry_type,
            index,
            referent,
            reads: false,
        })
    }

    fn atomic_table_name(
        &self,
        state: CheckedType,
        binder: &str,
        fields: &[u32],
    ) -> Result<String, CheckStop> {
        let mut ty = state;
        let mut name = format!("{binder}^");
        for index in fields {
            let CheckedType::Nominal(nominal) = ty else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let CheckedNominalKind::Struct { fields } =
                &self.types.nominals[nominal.0 as usize].kind
            else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let field = fields
                .get(*index as usize)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            name.push('.');
            name.push_str(&field.name);
            ty = field.ty;
        }
        Ok(name)
    }

    /// Record a formed place through the state binder for the grant judgment.
    /// Refusals are reported after ordinary reference and operation checks.
    pub(in crate::semantic::check) fn note_atomic_place(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        let Some(grant) = &self.body.atomic_grant else {
            return Ok(());
        };
        let state = grant.state;
        let Some(pbase) = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Pbase)?
        else {
            return Ok(());
        };
        let use_ = self.types.declarations.use_at(
            context.check_context,
            pbase,
            LexicalUseRole::PlaceBase,
        )?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Value,
        } = use_.target()
        else {
            return Ok(());
        };
        let Some(local) = bindings.get(&declaration) else {
            return Ok(());
        };
        if local.binding != state
            && !local
                .reference
                .as_ref()
                .is_some_and(|reference| reference.atomic_sources.contains(&state))
        {
            return Ok(());
        }
        let mut bases = local
            .reference
            .as_ref()
            .map(|reference| {
                reference
                    .paths
                    .iter()
                    .filter(|path| path.root == PlaceRoot::Binding(state))
                    .map(|path| {
                        path.path
                            .iter()
                            .take_while(|step| matches!(step, PlaceStep::Field(_)))
                            .filter_map(|step| match step {
                                PlaceStep::Field(field) => Some(*field),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if local.binding == state {
            bases.push(Vec::new());
        }
        let suffixes = self
            .types
            .declarations
            .tree
            .children_with(node, Production::Psuffix)?;
        if self.types.declarations.tree.reference_step(&suffixes)? != Some(0) {
            return Ok(());
        }
        let mut ty = local.ty;
        let mut fields = Vec::new();
        let mut reaches_table = self.table_entry_type(ty).is_some();
        for suffix in &suffixes[1..] {
            if reaches_table {
                break;
            }
            if self
                .types
                .declarations
                .tree
                .subscript_offset(*suffix)?
                .is_some()
            {
                break;
            }
            // Measures and window members are not fields leading to a table.
            // Leave their typing to the ordinary place judgment.
            if !matches!(ty, CheckedType::Nominal(nominal)
                if matches!(self.types.nominals[nominal.0 as usize].kind,
                    CheckedNominalKind::Struct { .. } | CheckedNominalKind::Box { .. }))
            {
                break;
            }
            let member = self
                .types
                .elaborate_place_member(context.check_context, *suffix, ty)?;
            let step = member.storage_step();
            match step {
                CheckedPlaceStep::Field(field) => fields.push(field),
                CheckedPlaceStep::BoxReferent(_) => {}
                CheckedPlaceStep::Subscript(_) => break,
            }
            ty = member.ty();
            reaches_table = self.table_entry_type(ty).is_some();
        }
        let grant = self.body.atomic_grant.as_mut().unwrap();
        if let Some(index) = grant.tables.iter().position(|table| {
            bases.iter().any(|base| {
                let reached = base.iter().chain(&fields).copied().collect::<Vec<_>>();
                reached.starts_with(table)
            })
        }) {
            grant.refusals.push((node, SemanticIssueKind::AtomicTableNotGranted {
                table: grant.names[index].clone(), mechanical_fix: "add a whole binding `t = &s^.table` to the header and reach the table through `t`; a statement's header names every table its guard and block reach",
            }));
        } else if reaches_table {
            grant.refusals.push((
                node,
                SemanticIssueKind::AtomicTableNotGranted {
                    table: "a table reached through the state binding".to_owned(),
                    mechanical_fix: SHARE2_MOVE_HIDDEN_TABLE,
                },
            ));
        }
        Ok(())
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
            target,
            entries,
            guard,
            body,
            ..
        } => {
            expression(target)
                || entries
                    .iter()
                    .flat_map(CheckedEntryBinding::expressions)
                    .any(expression)
                || guard.as_deref().is_some_and(expression)
                || block(body)
        }
        CheckedStatement::Proof(_) | CheckedStatement::Break { .. } => false,
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
