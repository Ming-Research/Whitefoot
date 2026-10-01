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
    BindingId, CheckedAtomicForm, CheckedExpression, CheckedMode, CheckedNominalKind,
    CheckedShared, CheckedStatement, CheckedType, IntegerType, expression_children,
};
use super::super::super::places::{PlaceRoot, ResolvedPlace};
use super::super::expressions::calls::user::WAIT1_DECLARE_THE_CALLER_WAITING;
use super::super::references::{ReferenceInfo, ReferenceKind};
use super::super::{AtomicHold, CheckStop, Checker, EffectSet, LocalBinding, TypedExpression};
use super::{ControlCounters, ControlScope, StatementResult};

/// The repair for a target that is not a `Shared<T>` or `SharedMap<V>` place
/// [SHARE-2, DIAG-1].
pub(in crate::semantic::check) const SHARE2_NAME_A_SHARED_HANDLE: &str = "name a place of type `Shared<T>` or `SharedMap<V>`, or an entry `m[key]` of a map: create the object with `shared_new` or the map with `shared_map_new`, and give each context its own handle made with `shared_share` or `shared_map_share`";
/// The repair for an entry `s^[k]` of a state no enclosing statement holds
/// [SHARE-2].
pub(in crate::semantic::check) const SHARE2_HOLD_THE_STATE: &str = "name the entry `m[key]` of a `SharedMap<V>` handle, or hold the map's state in an enclosing `atomic s = &m` statement and name the entry `s^[key]` inside its block";
/// The repair for a key that is not a byte range [SHARE-2].
pub(in crate::semantic::check) const SHARE2_KEY_A_BYTE_RANGE: &str = "name the key as a `&[u8]` range, such as `&bytes[start..end]`, or a reference variable holding one";
/// The repair for a guard on a statement holding a map's state or entry, or
/// inside the block of one [SHARE-2].
pub(in crate::semantic::check) const SHARE2_NO_GUARD_ON_A_MAP: &str = "remove the guard and test the condition inside the block; only a statement on a `Shared<T>` object outside every map's and entry's statement waits for a guard";
/// The repair for a waiting call inside an atomic statement [SHARE-2].
pub(in crate::semantic::check) const SHARE2_WAIT_OUTSIDE_THE_BLOCK: &str = "move the waiting call out of the atomic statement: end the statement first, wait, and start another atomic statement for any update that depends on the outcome";
/// The repair for an atomic statement inside an object's statement
/// [SHARE-2].
pub(in crate::semantic::check) const SHARE2_END_THE_OUTER_STATEMENT: &str = "end the outer atomic statement before starting the inner one, carrying what the inner one needs in a local";
/// The repair for an atomic statement a map's or an entry's statement does
/// not admit inside it [SHARE-2].
pub(in crate::semantic::check) const SHARE2_ONLY_OBJECTS_OR_HELD_ENTRIES: &str = "end the outer atomic statement before starting the inner one, carrying what the inner one needs in a local; inside a statement on a map or an entry only a statement on a `Shared<T>` object may start, and inside one holding a map's state also one on an entry `s^[key]` through its binding";
/// The repair for a guard that writes [SHARE-2].
pub(in crate::semantic::check) const SHARE2_READ_ONLY_GUARD: &str = "make the guard read only, calling a function whose row writes nothing and moves no argument, and make the update in the block";

/// [SHARE-2] which of the four target forms an atomic statement has.
enum AtomicTarget {
    /// `&h`, `h` a `Shared<T>`: the state, of type `T`.
    Object { state: CheckedType },
    /// `&h`, `h` a `SharedMap<V>`: the map's state, a `Keyed<V>`.
    Map { state: CheckedType },
    /// `&m[k]` or `&s^[k]`: the entry, an `Option<V>`; `holder` is the
    /// binder of the enclosing statement holding the map's state, when the
    /// map is reached through it.
    Entry {
        entry: CheckedType,
        key: Box<TypedExpression>,
        holder: Option<BindingId>,
    },
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
        let place_node = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Place)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // [SHARE-2] the target, read when the statement begins.
        let (target, form) =
            self.check_atomic_target(context, node, place_node, bindings, scope.loops.len())?;
        // [SHARE-2] `s^[k]` names an entry of the state an enclosing
        // statement holds through that statement's binding.
        if let AtomicTarget::Entry {
            holder: Some(holder),
            ..
        } = &form
            && !self
                .body
                .atomic_holds
                .iter()
                .any(|hold| matches!(hold, AtomicHold::Map(held) if held == holder))
        {
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                node,
                SemanticIssueKind::AtomicTargetNotShared {
                    found: "an entry of a map state no enclosing atomic statement holds".to_owned(),
                    mechanical_fix: SHARE2_HOLD_THE_STATE,
                },
            );
        }
        // [SHARE-2] an object's statement contains no atomic statement; a
        // map's or an entry's contains one on an object, and a map's one on
        // an entry of the state it holds. The inner statement is the
        // offending one.
        let admitted = match (self.body.atomic_holds.last(), &form) {
            (None, AtomicTarget::Entry { holder: None, .. })
            | (None, AtomicTarget::Object { .. } | AtomicTarget::Map { .. }) => true,
            (Some(AtomicHold::Map(_) | AtomicHold::Entry), AtomicTarget::Object { .. }) => true,
            (
                Some(AtomicHold::Map(held)),
                AtomicTarget::Entry {
                    holder: Some(holder),
                    ..
                },
            ) => held == holder,
            _ => false,
        };
        if !admitted {
            let mechanical_fix = match self.body.atomic_holds.last() {
                Some(AtomicHold::Map(_) | AtomicHold::Entry) => SHARE2_ONLY_OBJECTS_OR_HELD_ENTRIES,
                _ => SHARE2_END_THE_OUTER_STATEMENT,
            };
            return self.types.declarations.issue_node(
                SemanticRule::Share2,
                node,
                SemanticIssueKind::WaitInsideAtomic {
                    construct: "an atomic statement",
                    mechanical_fix,
                },
            );
        }
        let node_path = self.types.declarations.tree.path(node)?.clone();
        // [WAIT-1, SHARE-2] every statement but one on an entry of a held
        // state counts as a waiting call.
        let waits = !matches!(
            form,
            AtomicTarget::Entry {
                holder: Some(_),
                ..
            }
        );
        if waits {
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
        }
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
        let (state, checked_form, key, hold) = match form {
            AtomicTarget::Object { state } => {
                (state, CheckedAtomicForm::Object, None, AtomicHold::Object)
            }
            AtomicTarget::Map { state } => (
                state,
                CheckedAtomicForm::Map,
                None,
                AtomicHold::Map(BindingId(0)),
            ),
            AtomicTarget::Entry { entry, key, holder } => {
                effects = effects.union(key.effects.clone());
                // [SHARE-2] the statement reads the bytes `k` names.
                for place in key
                    .reference
                    .as_ref()
                    .map(|reference| reference.paths.as_slice())
                    .unwrap_or_default()
                {
                    for path in self.effect_paths_for_place(node, place, bindings)? {
                        effects.add_read(path);
                    }
                }
                (
                    entry,
                    CheckedAtomicForm::Entry {
                        held: holder.is_some(),
                    },
                    Some(Box::new(key.expression)),
                    AtomicHold::Entry,
                )
            }
        };

        // [SHARE-2] the binding: a reference variable naming what the
        // statement holds. It anchors at itself, as a reference parameter
        // does: that belongs to no binding [SHARE-1], so no path reaches it
        // except through this binder.
        let declaration = self
            .types
            .declarations
            .declaration_at(node, DeclarationRole::AtomicBinder)?;
        let binding = Checker::allocate_binding(counters.next_binding)?;
        counters
            .binding_names
            .push(declaration.spelling().to_owned());
        let hold = match hold {
            AtomicHold::Map(_) => AtomicHold::Map(binding),
            other => other,
        };
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

        // [SHARE-2] only an object's statement outside every other atomic
        // statement waits for a guard: one inside a map's or an entry's
        // block would wait holding what that statement holds.
        let allow_guard =
            matches!(checked_form, CheckedAtomicForm::Object) && self.body.atomic_holds.is_empty();
        self.body.atomic_depth += 1;
        self.body.atomic_holds.push(hold);
        let checked = self.check_atomic_parts(
            context,
            node,
            &mut block_bindings,
            counters,
            scope,
            allow_guard,
        );
        self.body.atomic_holds.pop();
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
        let invariants = if matches!(checked_form, CheckedAtomicForm::Object) {
            self.atomic_invariants(state, binding)
        } else {
            Vec::new()
        };
        Ok(StatementResult {
            statement: CheckedStatement::Atomic {
                node_path,
                target: Box::new(target.expression),
                form: checked_form,
                key,
                binding,
                state,
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

    /// [SHARE-2] the target and what it holds: a place of type `Shared<T>`
    /// or `SharedMap<V>`, or a map's entry `m[k]` or `s^[k]` whose key `k` is
    /// a `&[u8]` range. A place whose last step is an index over anything
    /// but a map, or its state, is an ordinary place.
    fn check_atomic_target(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        place_node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<(TypedExpression, AtomicTarget), CheckStop> {
        let suffixes = self
            .types
            .declarations
            .tree
            .children_with(place_node, Production::Psuffix)?;
        if let Some(last) = suffixes.last().copied()
            && let Some(offset_node) = self.types.declarations.tree.subscript_offset(last)?
        {
            let prefix = self
                .check_place_borrow_prefix(context, node, node, place_node, bindings, loop_depth)?;
            let shape = match (prefix.mode, prefix.expression.ty()) {
                (CheckedMode::Reference, CheckedType::Nominal(nominal)) => {
                    match &self.types.nominal(nominal)?.kind {
                        CheckedNominalKind::Shared { shape, .. } => Some(*shape),
                        _ => None,
                    }
                }
                _ => None,
            };
            let entry = match shape {
                Some(CheckedShared::Map { entry }) => Some((entry, None)),
                Some(CheckedShared::State { entry }) => {
                    // The state is named only by the binder of the statement
                    // that holds it, so the path the prefix names is rooted
                    // at that binder.
                    let holder = prefix.reference.as_ref().and_then(|reference| {
                        match reference.paths.as_slice() {
                            [place] => match place.root {
                                PlaceRoot::Binding(binding) => Some(binding),
                                PlaceRoot::Constant(_) => None,
                            },
                            _ => None,
                        }
                    });
                    // [SHARE-2] a reference that may name more than one
                    // state names none an enclosing statement holds.
                    let Some(holder) = holder else {
                        return self.types.declarations.issue_node(
                            SemanticRule::Share2,
                            node,
                            SemanticIssueKind::AtomicTargetNotShared {
                                found:
                                    "an entry of a map state no enclosing atomic statement holds"
                                        .to_owned(),
                                mechanical_fix: SHARE2_HOLD_THE_STATE,
                            },
                        );
                    };
                    Some((entry, Some(holder)))
                }
                Some(CheckedShared::Object) | None => None,
            };
            if let Some((entry, holder)) = entry {
                let mut probe = bindings.clone();
                let key = self.check_atom(context, offset_node, &mut probe, loop_depth)?;
                let bytes = key.mode == CheckedMode::Range
                    && key.expression.ty() == CheckedType::Integer(IntegerType::U8);
                if !bytes {
                    return self.types.declarations.issue_node(
                        SemanticRule::Share2,
                        offset_node,
                        SemanticIssueKind::AtomicKeyNotBytes {
                            found: self
                                .types
                                .checked_value_name(key.mode, key.expression.ty())?,
                            mechanical_fix: SHARE2_KEY_A_BYTE_RANGE,
                        },
                    );
                }
                return Ok((
                    prefix,
                    AtomicTarget::Entry {
                        entry,
                        key: Box::new(key),
                        holder,
                    },
                ));
            }
        }
        let target =
            self.check_place_borrow(context, node, node, place_node, bindings, loop_depth)?;
        let form = match (target.mode, target.expression.ty()) {
            (CheckedMode::Reference, CheckedType::Nominal(nominal)) => {
                match self.types.nominal(nominal)?.kind.clone() {
                    CheckedNominalKind::Shared {
                        state,
                        shape: CheckedShared::Object,
                    } => Some(AtomicTarget::Object { state }),
                    CheckedNominalKind::Shared {
                        state,
                        shape: CheckedShared::Map { .. },
                    } => Some(AtomicTarget::Map {
                        state: CheckedType::Nominal(self.keyed_state(context, state)?),
                    }),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(form) = form else {
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
        Ok((target, form))
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
        allow_guard: bool,
    ) -> Result<(Option<(CheckedExpression, EffectSet)>, super::BlockResult), CheckStop> {
        let guard = match self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Expr)?
        {
            Some(expression_node) if !allow_guard => {
                return self.types.declarations.issue_node(
                    SemanticRule::Share2,
                    expression_node,
                    SemanticIssueKind::AtomicGuardOnMap {
                        mechanical_fix: SHARE2_NO_GUARD_ON_A_MAP,
                    },
                );
            }
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
