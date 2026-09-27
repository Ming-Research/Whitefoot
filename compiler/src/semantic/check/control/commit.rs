//! [SET-1] the one `set` commit.
//!
//! `set_stmt := "set" place "=" expr ";"` [GRAM-4] writes exactly one place,
//! so this rule has one written form. The order is the rule's order — the
//! target is resolved and evaluated without reading or consuming the value
//! stored there, then the right-hand side, then the premises [SET-1] rechecks
//! under [LIV-1], then the one commit.
//!
//! v0.59's multi-target commit is retired with [LIV-2]: the target list, the
//! ordinal types a call's result list supplied, the pairwise overlap
//! condition over two targets, the index-separation conflicts it deferred to
//! entailment, and the declaring `set` target that minted its own binding all
//! had more than one written place as their subject, and [GRAM-4] now writes
//! one. What survives is the read-out, which is a fact about *this* target
//! and its own right-hand side.

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{DeclarationId, Production, SemanticCompilerFailure, SemanticIssueKind, SemanticRule};

use super::super::super::model::{CheckedMode, CheckedSetTarget, CheckedStatement};
use super::super::super::places::{PlaceRoot, ResolvedPlace};
use super::super::expressions::{MutationTarget, ResolvedPlaceSet, WIN3_LINEAR_TARGET};
use super::super::references::{InvalidationEvent, RequiredReferent};
use super::super::{CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding};
use super::{ControlScope, StatementResult};

/// [OP-12] the target of a commit whose direct-call right-hand side may read
/// its first actual out atomically.
pub(in crate::semantic::check) struct CommitReadOut {
    place: ResolvedPlaceSet,
    /// Whether the read-out owes the element-position judgment [MSR-2]. A
    /// measured place can itself carry a subscript [MSR-1], so `grid[0][1]`
    /// and `grid[1][1]` differ despite agreeing in their last offset; the
    /// flag never substitutes for that complete path.
    element: bool,
    /// The target's selected type, which [OP-12]'s result condition compares
    /// against the called row's declared result.
    ty: crate::semantic::model::CheckedType,
    /// The direct call that is the complete right-hand side. [OP-12] grants
    /// its read-out only to that call's first actual; an occurrence in any
    /// later actual follows ordinary [OWN-1].
    call: Option<crate::NodePath>,
    atomic_call: bool,
    argument_zero: bool,
    read_out_allowed: bool,
    failed_atomic_read_out: bool,
    read_out: bool,
}

impl<'unit> Checker<'unit> {
    /// [OP-12] a read-out spends the target for the rest of the right-hand
    /// side, including later scalar reads or references of its descendants. A
    /// measure reads only its descriptor, so reading an enclosing run's
    /// descriptor does not read a spent element [MSR-2].
    pub(in crate::semantic::check) fn check_commit_place_live(
        &self,
        place: &ResolvedPlace,
        node: NodeId,
        descriptor: bool,
    ) -> Result<(), CheckStop> {
        let targets = self.commit_read_outs.borrow();
        let spent = targets.iter().any(|target| {
            target.read_out
                && target.place.members.iter().any(|member| {
                    member.contains(place) && (!descriptor || member.path.len() <= place.path.len())
                })
        });
        if spent {
            return self.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::UseAfterMove {
                    mechanical_fix: "introduce a new `let` binding before reuse",
                },
            );
        }
        Ok(())
    }

    /// Whether `place` is the [OP-12] read-out of the target of the commit now
    /// being checked, recording that read-out when it is.
    ///
    /// The moved place is the target place, or a place reached through it. A
    /// `move` of a strict prefix of the target is an ordinary consuming use
    /// and is not answered here. The target is read out at most once, because
    /// after its read-out it is dead for the remainder of the evaluation.
    pub(in crate::semantic::check) fn take_commit_read_out(&self, place: &ResolvedPlace) -> bool {
        self.take_commit_storage_read_out(place, false)
    }

    /// Whether `place[offset]` is the read-out of an element target of the
    /// commit now being checked [SET-1, MSR-2].
    ///
    /// An element target is matched only at an offset provably the same as
    /// its own: reading one element out and reinitializing another would
    /// leave the second holding a value that never left and the first holding
    /// none, so an offset this rule cannot decide keeps the refusal.
    pub(in crate::semantic::check) fn take_commit_element_read_out(
        &self,
        place: &ResolvedPlace,
    ) -> bool {
        self.take_commit_storage_read_out(place, true)
    }

    fn take_commit_storage_read_out(&self, place: &ResolvedPlace, element: bool) -> bool {
        let mut targets = self.commit_read_outs.borrow_mut();
        for target in targets.iter_mut() {
            if target.read_out || target.element != element || target.place.identity != *place {
                continue;
            }
            if !target.read_out_allowed {
                // [OP-12, DIAG-1] an affine target in actual zero of the
                // direct RHS call has attempted the atomic spelling. If the
                // call failed OP-12's result condition, ordinary ownership
                // consumes the root and the required refusal is OWN-1 at
                // this argument atom, recorded after its normal check.
                target.failed_atomic_read_out |= target.argument_zero;
                continue;
            }
            target.read_out = true;
            return true;
        }
        false
    }

    /// Selects the sole direct RHS call that already satisfies [OP-12]'s
    /// complete result condition. This runs before its actuals so only actual
    /// zero can receive the read-out spelling judgment.
    pub(in crate::semantic::check) fn prepare_atomic_update_call(
        &self,
        call: &crate::NodePath,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        let target_ty = {
            let targets = self.commit_read_outs.borrow();
            let [target] = &targets[..] else {
                return Ok(());
            };
            if target.call.as_ref() != Some(call) {
                return Ok(());
            }
            target.ty
        };
        let routed = self
            .postcondition_selectors_for_signature(signature)?
            .iter()
            .any(|selector| selector.variant.is_some());
        let mut targets = self.commit_read_outs.borrow_mut();
        let [target] = &mut targets[..] else {
            return Ok(());
        };
        target.atomic_call = signature.result_mode == CheckedMode::Own
            && signature.result == target_ty
            && signature
                .results
                .iter()
                .all(|result| result.mode == CheckedMode::Own && result.ty == target_ty)
            && !routed;
        Ok(())
    }

    /// Makes the read-out available only while the direct call's first actual
    /// is checked. Later actuals are ordinary ordered ownership uses.
    pub(in crate::semantic::check) fn enter_atomic_update_argument(
        &self,
        call: &crate::NodePath,
        ordinal: usize,
    ) {
        for target in self.commit_read_outs.borrow_mut().iter_mut() {
            target.argument_zero = ordinal == 0
                && target
                    .call
                    .as_ref()
                    .is_some_and(|candidate| candidate == call);
            target.read_out_allowed = target.atomic_call && target.argument_zero;
        }
    }

    /// Completes [OP-12]'s failed-result diagnostic after ordinary ownership
    /// has checked the attempted first actual. This is false for later
    /// actuals, so an ordinary complete-binding move there remains [SET-1]'s
    /// normal post-RHS reinitialization.
    pub(in crate::semantic::check) fn reject_failed_atomic_update_argument(
        &self,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        if !self
            .commit_read_outs
            .borrow()
            .iter()
            .any(|target| target.failed_atomic_read_out)
        {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Own1,
            node,
            SemanticIssueKind::UseAfterMove {
                mechanical_fix: "return the target's type on every normal result without a routed failure, or move the old value in an ordinary statement before `set`",
            },
        )
    }

    /// [OP-12] the target place of the `set` whose right-hand side is being
    /// checked, when a call whose declared result is `own T` for that
    /// target's own type T stands in that right-hand side.
    ///
    /// "`set p = f(move p, args...);` for an affine place and
    /// `set p = f(p, args...);` for a copy one, where the first argument of
    /// the call is the target place itself, is the atomic in-place update."
    /// The first-argument condition is the caller's, because only the caller
    /// holds the actual's resolved path; what this answers is the other half
    /// — which target this call stands over, and whether the call's result
    /// condition holds. "A call that fails the result condition is not an
    /// atomic update and is judged as an ordinary `set`", so a row returning
    /// anything else yields `None` here and reaches [SET-1]'s ordinary
    /// judgment untouched.
    pub(in crate::semantic::check) fn atomic_update_target(
        &self,
        call: &crate::NodePath,
    ) -> Option<ResolvedPlace> {
        let targets = self.commit_read_outs.borrow();
        let [target] = &targets[..] else {
            return None;
        };
        if !target.atomic_call || target.call.as_ref() != Some(call) {
            return None;
        }
        Some(target.place.identity.clone())
    }

    /// [GRAM-4, SET-1] one `set` statement.
    pub(super) fn check_commit(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        _counters: &mut super::ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let [target_node] = self.tree.children_with(node, Production::Place)?[..] else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let [value_node] = self.tree.children_with(node, Production::Expr)?[..] else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };

        // [REF-1] a `set` whose target is a reference variable and whose
        // right-hand side is a `borrow_expr` rebinds that name and writes no
        // storage, so [SET-1]'s value-target judgment does not apply to it.
        if let Some(result) = self.check_reference_rebinding(
            function,
            node,
            target_node,
            value_node,
            bindings,
            scope,
        )? {
            return Ok(result);
        }

        // [SET-1] the target is resolved and evaluated first, without reading
        // or consuming the value stored there.
        let revives = self.commit_revives_binding(target_node, bindings)?;
        let mut mutation =
            self.check_set_target(function, target_node, bindings, scope.loops.len())?;
        if revives {
            // An entry-dead complete binding has no current owner whose state
            // this initialization could write [SET-1, EFF-2]. Its bare target
            // evaluates no offsets and no other expressions.
            mutation.effects = EffectSet::NONE;
        }
        let mut effects = mutation.effects.clone();

        // [SET-1] evaluates the right-hand side under ordinary ownership.
        // [OP-12] alone recognizes its direct call's first target actual as
        // an atomic read-out rather than a root-killing consume.
        let (value, atomic_read_out) =
            self.check_commit_value(function, &mutation, value_node, bindings, scope.loops.len())?;
        effects = effects.union(value.effects.clone());

        // [TYPE-5] "the right-hand side of `set p = e;` must produce exactly
        // `own T` ... After the TYPE-7 implicit-read exclusivity below, a
        // different right-hand-side mode or type is a hard error citing
        // TYPE-5 at the complete `expr` child of the `set_stmt`, carrying
        // expected `own T` and the actual mode and type." The mode is part of
        // the judgment and part of the rendering: a holder written where the
        // target's value is required is refused here, not silently accepted
        // because its carried type agrees.
        let target_type = mutation.target.ty();
        if value.mode != CheckedMode::Own || target_type != value.expression.ty() {
            if self.reads_implicitly_through_holder(
                value.reference_value,
                value.expression.ty(),
                RequiredReferent::Exact(target_type),
            )? {
                return self.issue_node(
                    SemanticRule::Type7,
                    value_node,
                    SemanticIssueKind::MissingDereference {
                        mechanical_fix: "write `deref(.)`",
                    },
                );
            }
            return self.issue_node(
                SemanticRule::Type5,
                value_node,
                SemanticIssueKind::type_mismatch(
                    self.checked_value_name(CheckedMode::Own, target_type)?,
                    self.checked_value_name(value.mode, value.expression.ty())?,
                ),
            );
        }
        let root_live_after_rhs = bindings
            .get(&mutation.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .live;
        self.judge_commit_admission(&mutation, target_node, atomic_read_out, bindings)?;
        // [WIN-3, STOR-3] "Assigning over any owned place releases the old
        // value when it is affine." The commit may revive an
        // entry-dead binding, and a right-hand side that reads any target out
        // takes the value the write would otherwise displace. Both were
        // admitted just above, and neither is readable from the target's type
        // or path, so the judgment that settled them records its answer here
        // for lowering [SET-1, LIV-1, DIAG-2].
        let displaces_live_value = root_live_after_rhs && !atomic_read_out;
        // Every source rejection of this statement is judged above; a target
        // this compiler cannot lower stops here and nowhere earlier [DIAG-1].
        if let Some(feature) = mutation.unsupported {
            return self.unsupported(feature, target_node);
        }
        // [REF-2] the write invalidates every live reference whose path this
        // target is a proper prefix of. Writing the storage at the target's
        // own path, or below it, is a content write and invalidates nothing.
        for place in &mutation.place.members {
            self.invalidate_references_with_separation(
                bindings,
                place,
                &InvalidationEvent::PrefixWritten,
                Some(self.tree.path(node)?),
            )?;
        }
        if self.commit_reinitializes_binding(&mutation) {
            bindings
                .get_mut(&mutation.declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .live = true;
        }

        Ok(Self::continuing_statement(
            CheckedStatement::Set {
                node_path: self.tree.path(node)?.clone(),
                target: mutation.target,
                value: value.expression,
                displaces_live_value,
            },
            effects,
        ))
    }

    /// The right-hand side, checked under the possible atomic read-out context
    /// this commit installs for [OP-12].
    ///
    /// The context is removed before any rejection leaves this function, so
    /// no later statement of any function can read a stale target.
    fn check_commit_value(
        &self,
        function: &FunctionSignature,
        mutation: &MutationTarget,
        value_node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<(super::super::TypedExpression, bool), CheckStop> {
        let call = match self.tree.children(value_node)? {
            [child] if self.tree.production(*child)? == Production::Call => {
                Some(self.tree.path(*child)?.clone())
            }
            _ => None,
        };
        self.commit_read_outs.replace(vec![CommitReadOut {
            // [REF-1, OP-12] exact read-out matching uses the unique member
            // where one exists and the holder identity for a true union.
            // After a read-out, liveness ranges over every storage member.
            place: mutation.place.clone(),
            element: mutation.element,
            ty: mutation.target.ty(),
            call,
            atomic_call: false,
            argument_zero: false,
            read_out_allowed: false,
            failed_atomic_read_out: false,
            read_out: false,
        }]);
        let outcome = self.check_expression(function, value_node, bindings, loop_depth);
        let read_out = self
            .commit_read_outs
            .take()
            .into_iter()
            .any(|target| target.read_out);
        Ok((outcome?, read_out))
    }

    /// [SET-1] the premises rechecked after the right-hand side, under
    /// [LIV-1].
    fn judge_commit_admission(
        &self,
        mutation: &MutationTarget,
        target_node: NodeId,
        atomic_read_out: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        // The root's liveness is re-established after the right-hand side
        // [SET-1, LIV-1]. A complete binding this commit reinitializes is the
        // one dead root a commit revives — because it was already dead when
        // the statement resolved it, or because this statement's own read-out
        // took its value. Every projected, dereferenced or subscripted target
        // still demands a live root.
        let reinitializes = self.commit_reinitializes_binding(mutation);
        let root_live = bindings
            .get(&mutation.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .live;
        if !reinitializes && !root_live {
            return self.issue_node(
                SemanticRule::Own1,
                target_node,
                SemanticIssueKind::UseAfterMove {
                    mechanical_fix: "introduce a new `let` binding before reuse",
                },
            );
        }
        self.revalidate_mutation_access(mutation.through_reference, bindings, target_node)?;
        let ty = mutation.target.ty();
        if self.is_copy_type(ty)? || atomic_read_out || !root_live {
            return Ok(());
        }
        // [WIN-3] assigning over any owned place releases the old value when
        // it is affine. A linear one has no release, which the target-class
        // judgment already refused at formation, so what remains here is the
        // affine case, which this commit admits: the old value takes its
        // compiler-derived release [STOR-3].
        if !matches!(
            self.linearity_class(ty)?,
            super::super::linearity::LinearityClass::Linear
        ) {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Win3,
            target_node,
            SemanticIssueKind::LinearAssignmentTarget {
                target_type: self.checked_type_name(ty)?,
                mechanical_fix: WIN3_LINEAR_TARGET,
            },
        )
    }

    /// [REF-1] a `set` whose target is a reference variable and whose
    /// right-hand side is a `borrow_expr` rebinds that name.
    ///
    /// The rebinding writes no storage, so it exhibits no effect and takes no
    /// commit. A continuing loop rebinding contributes to [REF-1]'s finite
    /// header summaries before the settled structural walk is published.
    fn check_reference_rebinding(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        target_node: NodeId,
        value_node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        scope: ControlScope<'_>,
    ) -> Result<Option<StatementResult>, CheckStop> {
        if !self
            .tree
            .children_with(target_node, Production::Psuffix)?
            .is_empty()
        {
            return Ok(None);
        }
        let Some(declaration) = self.complete_binding_target(target_node)? else {
            return Ok(None);
        };
        let Some(local) = bindings.get(&declaration) else {
            return Ok(None);
        };
        let Some(previous_kind) = local.reference.as_ref().map(|reference| reference.kind) else {
            return Ok(None);
        };
        let expected_mode = local.mode;
        let expected_type = local.ty;
        // [REF-1] only a binding that crosses the current loop's backedge is
        // loop-carried. A local declared inside that loop, and every binding
        // outside a loop, may be rebound to a different path shape.
        let value = self.check_expression(function, value_node, bindings, scope.loops.len())?;
        let Some(reference) = value.reference.clone() else {
            // [TYPE-7] a `set` whose target is a reference variable and whose
            // right-hand side is a value is not a rebinding.
            return self.issue_node(
                SemanticRule::Type7,
                target_node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(.)`",
                },
            );
        };
        // A rebinding changes only the path a reference variable names. Its
        // reference kind and referent type remain the type of that binding
        // [TYPE-5, REF-1]. This check is explicit because the reference path
        // bypasses SET-1's ordinary owned-value commit judgment.
        if value.mode != expected_mode
            || value.expression.ty() != expected_type
            || reference.kind != previous_kind
        {
            return self.issue_node(
                SemanticRule::Type5,
                value_node,
                SemanticIssueKind::type_mismatch(
                    self.checked_value_name(expected_mode, expected_type)?,
                    self.checked_value_name(value.mode, value.expression.ty())?,
                ),
            );
        }
        let binding = bindings
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .binding;
        for context in scope.loops {
            if !context.reference_rebindings.contains(&node) {
                continue;
            }
            let token = super::super::references::LoopReferenceToken {
                loop_id: context.id,
                owner: binding,
            };
            let summarized = self.loop_reference_summaries.borrow().contains_key(&token);
            if summarized
                && self.join_loop_reference_summary(
                    token,
                    expected_type,
                    previous_kind,
                    &reference.paths,
                    bindings,
                )?
            {
                return Err(CheckStop::ReferenceSummaryChanged);
            }
        }
        self.record_reference_origins(binding, &reference.paths);
        let local = bindings
            .get_mut(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        local.reference = Some(reference);
        local.live = true;
        Ok(Some(Self::continuing_statement(
            CheckedStatement::Set {
                node_path: self.tree.path(node)?.clone(),
                target: CheckedSetTarget::Place(super::super::super::model::CheckedWritablePlace {
                    binding: local.binding,
                    fields: Vec::new(),
                    mode: expected_mode,
                    ty: local.ty,
                    declares: false,
                }),
                value: value.expression,
                // [REF-1] a reference rebinding displaces no owner.
                displaces_live_value: false,
            },
            value.effects,
        )))
    }

    /// Whether this target is the complete binding its own declaration names,
    /// which is the one target shape a commit reinitializes [SET-1].
    ///
    /// A `deref` target writes a place the reference does not own, and a
    /// projected or subscripted target writes one component of a value, so
    /// neither is that shape.
    fn commit_reinitializes_binding(&self, mutation: &MutationTarget) -> bool {
        matches!(&mutation.target, CheckedSetTarget::Place(place) if place.fields.is_empty())
            && mutation.through_reference.is_none()
            && mutation.place.identity.path.is_empty()
            && matches!(mutation.place.identity.root, PlaceRoot::Binding(_))
    }

    /// Whether this written target is a complete binding that is already
    /// dead, which is the one shape [SET-1] reinitializes from dead.
    fn commit_revives_binding(
        &self,
        target_node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<bool, CheckStop> {
        if !self
            .tree
            .children_with(target_node, Production::Psuffix)?
            .is_empty()
        {
            return Ok(false);
        }
        let Some(declaration) = self.complete_binding_target(target_node)? else {
            return Ok(false);
        };
        Ok(bindings.get(&declaration).is_some_and(|local| !local.live))
    }
}
