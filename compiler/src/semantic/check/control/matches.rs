use crate::semantic::check::CheckContext;
use crate::semantic::check::DeclarationInventory;
use crate::semantic::check::FunctionContext;
use crate::semantic::check::TypeContext;
use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, DeferredUseRole, FixedTerminal,
    LexicalUseRole, Production, ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind,
    SemanticLocation, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::super::model::{
    CheckedEnumType, CheckedExpression, CheckedField, CheckedMatchArm, CheckedMatchBinder,
    CheckedMatchSelection, CheckedMatchType, CheckedMode, CheckedNominalKind, CheckedProjectedDrop,
    CheckedStatement, CheckedType, CheckedValue,
};
use super::super::super::places::{PlaceRoot, PlaceStep};
use super::super::super::tree::ConditionalAlternative;
use super::super::references::{ReferenceInfo, RequiredReferent};
use super::super::{CheckStop, Checker, EffectSet, LocalBinding, RefinementWitness};
use super::{BlockResult, ControlCounters, ControlScope, GiveContext, LoopTransferState};
use crate::syntax::views::MatchArmHeader;

#[derive(Clone)]
struct VariantDescriptor {
    name: String,
    tag: u32,
    fields: Vec<CheckedField>,
}

struct MatchDescriptor {
    enum_type: CheckedEnumType,
    variants: Vec<VariantDescriptor>,
}

struct ArmDescriptor {
    name: String,
    selection: CheckedMatchSelection,
    variant: Option<VariantDescriptor>,
}

pub(super) struct MatchResult {
    /// [OP-5] the complete condition `expr` of an `if` or `value_if`, whose
    /// source redundancy the entailment walk judges; `None` for a source match.
    pub(super) condition: Option<crate::NodePath>,
    pub(super) scrutinee: CheckedExpression,
    pub(super) match_type: CheckedMatchType,
    pub(super) arms: Vec<CheckedMatchArm>,
    /// [GIVE-1] the mode and type the delivery set derived, or `None` for a
    /// statement `match` and for a value initializer that delivers nothing.
    pub(super) delivered: Option<(CheckedMode, CheckedType)>,
    /// [REF-1] the union of the path sets the delivering arms name, where the
    /// delivery set delivers references.
    pub(super) delivered_reference: Option<ReferenceInfo>,
    pub(super) can_continue: bool,
    pub(super) all_paths_deliver: bool,
    pub(super) effects: EffectSet,
    pub(super) give_states: Vec<HashMap<DeclarationId, LocalBinding>>,
    pub(super) loop_transfers: Vec<LoopTransferState>,
}

/// How a `match` scrutinee is written, which is what [OWN-13] and [REF-1]
/// read to decide whether the match goes through a reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrutineeSpelling {
    /// `&x`: a `borrow_expr` naming the enum's own path [REF-1]. No `^`
    /// step stands between the expression and the enum it names.
    Borrowed,
    /// `p^` and anything selected below it: the storage a reference
    /// names, reached under the step [REF-1] requires.
    Dereferenced,
    /// An owned place, whose stored enum may receive a refinement.
    OwnedPlace,
    /// A computed expression or literal, whose operands receive no refinement.
    Other,
}

impl<'unit> Checker<'_, 'unit> {
    pub(super) fn check_match(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
        value_delivery: bool,
    ) -> Result<MatchResult, CheckStop> {
        let FunctionContext { check_context, .. } = context;
        let expression_node = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Expr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let mut scrutinee =
            self.check_match_expression(context, expression_node, bindings, scope.loops.len())?;
        let spelling = self
            .types
            .declarations
            .scrutinee_spelling(expression_node)?;
        // [REF-1] a reference variable denotes the reference, and the storage
        // it names is reached only through `^`, so a bare holder written
        // where the enum itself is required is that missing step. A `Box` is
        // not one of these at v0.60: its content is the ordinary field
        // `inner` [TYPE-9], so a `Box` scrutinee is the ordinary wrong-type
        // judgment [TYPE-5] the descriptor below makes.
        if (matches!(
            spelling,
            ScrutineeSpelling::Other | ScrutineeSpelling::OwnedPlace
        ) || matches!(
            scrutinee.expression.ty(),
            CheckedType::Integer(_) | CheckedType::GenericInt(_)
        ))
            && scrutinee.reference_value
            && (matches!(
                scrutinee.expression.ty(),
                CheckedType::Integer(_) | CheckedType::GenericInt(_)
            ) || self.types.satisfies_referent_requirement(
                scrutinee.expression.ty(),
                RequiredReferent::Enum,
            )?)
        {
            return self.types.declarations.issue_node(
                SemanticRule::Type7,
                expression_node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `holder^`".to_owned(),
                },
            );
        }
        // [OWN-13] matching through a reference leaves the scrutinee live and
        // binds each payload as a reference naming the scrutinee path
        // extended by that payload step [REF-1]. `p^` reads the value
        // at the path `p` names everywhere else, so the reading is made here,
        // where the rule distinguishes the two, and not at the place walk.
        if spelling == ScrutineeSpelling::Dereferenced
            && scrutinee.reference.is_none()
            && !matches!(
                scrutinee.expression.ty(),
                CheckedType::Integer(_) | CheckedType::GenericInt(_)
            )
        {
            // Place checking retains both the selected members and every
            // place read to evaluate their operands. Only the selected set is
            // the borrowed-match referent; an index loaded from another place
            // must not become another enum root [REF-1, OWN-13].
            let mut places = scrutinee
                .accesses
                .iter()
                .filter(|access| access.selected)
                .map(|access| access.place.clone());
            let first = places
                .next()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let mut reference =
                ReferenceInfo::formed(super::super::references::ReferenceKind::Single, first);
            for place in places {
                reference.join(&ReferenceInfo::formed(
                    super::super::references::ReferenceKind::Single,
                    place,
                ));
            }
            reference.selection = Some(self.payload_selection_identity(
                &scrutinee.expression,
                &reference.paths,
                expression_node,
                bindings,
            )?);
            scrutinee.mode = CheckedMode::Reference;
            scrutinee.reference = Some(reference);
        }
        let scrutinee = scrutinee;
        let descriptor = if matches!(
            scrutinee.expression.ty(),
            CheckedType::Integer(_) | CheckedType::GenericInt(_)
        ) {
            None
        } else {
            Some(
                self.types
                    .match_descriptor(scrutinee.expression.ty(), expression_node)?,
            )
        };
        let match_type = descriptor.as_ref().map_or_else(
            || CheckedMatchType::Integer(scrutinee.expression.ty()),
            |descriptor| CheckedMatchType::Enum(descriptor.enum_type),
        );
        let base_bindings = bindings.clone();
        let base_keys = base_bindings.keys().copied().collect::<Vec<_>>();
        let base_key_set = base_keys.iter().copied().collect::<HashSet<_>>();
        let value_match = self.types.declarations.tree.production(node)? == Production::ValueMatch;
        if value_match != value_delivery {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let local_give_context = value_delivery.then(|| GiveContext::empty(&base_key_set, scope));
        let arm_nodes = self
            .types
            .declarations
            .tree
            .children_with(node, Production::Arm)?;
        let resolved_arms = if let Some(descriptor) = &descriptor {
            self.resolve_enum_match_arms(check_context, node, descriptor, &arm_nodes)?
        } else {
            self.resolve_integer_match_arms(context, node, scrutinee.expression.ty(), &arm_nodes)?
        };

        let mut arms = Vec::with_capacity(arm_nodes.len());
        let mut normal_states = Vec::new();
        // [LIV-1] one label per predecessor state, in the order the states are
        // collected, so a liveness disagreement can name the two edges the
        // writer wrote rather than two indices.
        let mut normal_labels: Vec<String> = Vec::new();
        let mut give_states = Vec::new();
        let mut give_labels: Vec<String> = Vec::new();
        let mut loop_transfers = Vec::new();
        let mut effects = scrutinee.effects.clone();
        // [EFF-2, OWN-13] dispatch observes the enum tag even when forming
        // the scrutinee reference read no value. Attribute that observation
        // to every resolved referent, at the most precise EFF-1 path; EFF-1
        // has no separate tag step. Operand reads remain in scrutinee.effects.
        if let Some(reference) = &scrutinee.reference {
            for place in &reference.paths {
                for path in self.effect_paths_for_place(expression_node, place, &base_bindings)? {
                    effects.add_read(path);
                }
            }
        }
        let mut all_paths_deliver = true;
        for (arm_node, arm) in arm_nodes.into_iter().zip(&resolved_arms) {
            let arm_scope = ControlScope {
                loops: scope.loops,
                give_context: local_give_context.as_ref().or(scope.give_context),
            };
            let mut arm_bindings = base_bindings.clone();
            let origin = self.types.declarations.tree.path(arm_node)?.clone();
            let (binders, covered) =
                if let (Some(descriptor), Some(variant)) = (&descriptor, &arm.variant) {
                    // [ENT-3.S15] the arm witnesses its tracked scrutinee, even
                    // when `..` binds no payload. Keep that occurrence on its root's
                    // ordinary flow state so writes, aliases and joins share the
                    // existing witness invalidation machinery.
                    let places = if let Some(reference) = &scrutinee.reference {
                        reference.paths.clone()
                    } else if spelling == ScrutineeSpelling::OwnedPlace {
                        scrutinee
                            .accesses
                            .iter()
                            .filter(|access| access.selected)
                            .map(|access| access.place.clone())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let selection = if places.is_empty() {
                        None
                    } else {
                        Some(self.payload_selection_identity(
                            &scrutinee.expression,
                            &places,
                            expression_node,
                            bindings,
                        )?)
                    };
                    for place in places {
                        if let PlaceRoot::Binding(root) = place.root
                            && let Some(local) = arm_bindings
                                .values_mut()
                                .find(|local| local.binding == root)
                        {
                            local.refinement_witnesses.push(RefinementWitness {
                                origin: origin.clone(),
                                selection: selection
                                    .clone()
                                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                                place,
                                variant: variant.tag,
                                valid: true,
                            });
                        }
                    }
                    let variant_ordinal = descriptor
                        .variants
                        .iter()
                        .position(|candidate| candidate.name == variant.name);
                    self.check_match_binders(
                        check_context,
                        variant,
                        (descriptor.enum_type, variant_ordinal),
                        arm_node,
                        &mut arm_bindings,
                        counters,
                        scope.loops.len(),
                        &scrutinee,
                    )?
                } else {
                    (Vec::new(), Vec::new())
                };
            let statements = self
                .types
                .declarations
                .tree
                .children_with(arm_node, Production::Stmt)?;
            let mut checked =
                self.check_block(context, &statements, &mut arm_bindings, counters, arm_scope)?;
            // Arm-local evidence cannot cross any normal, give or loop
            // transfer edge. A captured payload reference is independent of
            // the witness and retains its ordinary REF-2 validity.
            Checker::forget_refinement_occurrence(&mut arm_bindings, &origin);
            for state in &mut checked.give_states {
                Checker::forget_refinement_occurrence(state, &origin);
            }
            for state in &mut checked.loop_transfers {
                state.forget_refinement_occurrence(&origin);
            }
            let leaving = Checker::bindings_leaving_scope(&arm_bindings, &base_keys);
            Checker::invalidate_control_exits(
                &mut arm_bindings,
                &mut checked.give_states,
                &mut checked.loop_transfers,
                arm_scope.give_context,
                &leaving,
            );
            let fallthrough_drops = if checked.can_continue {
                self.types.live_affine_drops(
                    check_context,
                    &arm_bindings,
                    &base_key_set,
                    arm_node,
                )?
            } else {
                Vec::new()
            };
            let label = format!("the `{}` arm", arm.name);
            if checked.can_continue {
                normal_states.push(arm_bindings);
                normal_labels.push(label.clone());
            }
            all_paths_deliver &= !checked.can_continue && checked.all_paths_deliver;
            effects = effects.union(checked.effects);
            give_labels.extend(std::iter::repeat_n(
                format!("a delivering edge of {label}"),
                checked.give_states.len(),
            ));
            give_states.extend(checked.give_states);
            loop_transfers.extend(checked.loop_transfers);
            arms.push(CheckedMatchArm {
                node_path: self.types.declarations.tree.path(arm_node)?.clone(),
                label,
                selection: arm.selection,
                binders,
                covered,
                body: checked.statements,
                fallthrough_drops,
            });
        }
        if value_match {
            if !all_paths_deliver {
                return self.types.declarations.issue_node(
                    SemanticRule::Give1,
                    node,
                    SemanticIssueKind::InvalidGive,
                );
            }
            self.types.declarations.join_states(
                &base_keys,
                &give_states,
                &give_labels,
                node,
                bindings,
            )?;
        } else {
            self.types.declarations.join_states(
                &base_keys,
                &normal_states,
                &normal_labels,
                node,
                bindings,
            )?;
        }
        Ok(MatchResult {
            condition: None,
            scrutinee: scrutinee.expression,
            match_type,
            arms,
            delivered: local_give_context.as_ref().and_then(GiveContext::delivered),
            delivered_reference: local_give_context
                .as_ref()
                .and_then(GiveContext::delivered_reference),
            can_continue: if value_match {
                !give_states.is_empty()
            } else {
                !normal_states.is_empty()
            },
            all_paths_deliver,
            effects,
            give_states: if value_match { Vec::new() } else { give_states },
            loop_transfers,
        })
    }

    /// [GRAM-6] the Bool conditional.
    ///
    /// It produces exactly the checked shape the Bool `match` produced before
    /// the spelling changed: a two-armed match over [`CheckedEnumType::Bool`]
    /// with `True` tagged 1 and `False` tagged 0. Lowering, entailment,
    /// cleanup, and drops therefore need no `if` of their own. The arms cannot
    /// come from [`Checker::check_match`], which reads `arm` nodes and resolves
    /// each one's variant by constructor name; an `if` owns no arm at all, so
    /// its two are built here from the same descriptor.
    pub(super) fn check_if(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
        value_delivery: bool,
    ) -> Result<MatchResult, CheckStop> {
        if (self.types.declarations.tree.production(node)? == Production::ValueIf) != value_delivery
        {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        self.check_conditional(context, node, bindings, counters, scope, value_delivery)
    }

    /// [GRAM-6] the condition judgment of an `if`, which an atomic
    /// statement's guard takes as well [SHARE-2].
    pub(super) fn check_condition(
        &mut self,
        context: FunctionContext<'_, '_>,
        expression_node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<super::super::TypedExpression, CheckStop> {
        let condition =
            self.check_match_expression(context, expression_node, bindings, loop_depth)?;
        // [TYPE-7] exclusivity, which [GRAM-6] keeps: a condition reached
        // through a holder is the implicit read, and its own `own Bool`
        // judgment forms no rejection. `RequiredReferent::Enum` already
        // admits `Bool`, the prelude enum this condition must be.
        if self.types.reads_implicitly_through_holder(
            condition.reference_value,
            condition.expression.ty(),
            RequiredReferent::Enum,
        )? {
            return self.types.declarations.issue_node(
                SemanticRule::Type7,
                expression_node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `holder^`".to_owned(),
                },
            );
        }
        // [GRAM-6] the condition takes [OP-5]'s judgment exactly; every
        // failure that is not TYPE-7's implicit read cites GRAM-6 here.
        if condition.expression.ty() != CheckedType::Bool || condition.mode != CheckedMode::Own {
            return self.types.declarations.issue_node(
                SemanticRule::Gram6,
                expression_node,
                SemanticIssueKind::InvalidConditionalForm {
                    mechanical_fix: "give the condition exact value mode and type `own Bool`",
                },
            );
        }
        Ok(condition)
    }

    /// The conditional body shared by both forms.
    ///
    /// `opens_delivery` is not "this is a `value_if`": [GIVE-1] gives an
    /// else-if chain one delivery set belonging to the chain's binding, so
    /// only the outermost `value_if` opens the context and every chained one
    /// contributes to it, exactly as a statement `match` propagates `give`s.
    fn check_conditional(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
        opens_delivery: bool,
    ) -> Result<MatchResult, CheckStop> {
        let FunctionContext { check_context, .. } = context;
        let value_if = self.types.declarations.tree.production(node)? == Production::ValueIf;
        let expression_node = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Expr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let condition =
            self.check_condition(context, expression_node, bindings, scope.loops.len())?;
        // The exact owned Bool judgment gives the same non-escaping header
        // boundary as an owned enum match [OWN-6, GRAM-6].
        let blocks = self.types.declarations.tree.conditional_blocks(node)?;
        self.types
            .declarations
            .reject_unspellable_else(node, &blocks.alternative, value_if)?;

        let base_bindings = bindings.clone();
        let base_keys = base_bindings.keys().copied().collect::<Vec<_>>();
        let base_key_set = base_keys.iter().copied().collect::<HashSet<_>>();
        let local_give_context = opens_delivery.then(|| GiveContext::empty(&base_key_set, scope));
        let arm_scope = ControlScope {
            loops: scope.loops,
            give_context: local_give_context.as_ref().or(scope.give_context),
        };

        let mut then_bindings = base_bindings.clone();
        let then_checked = self.check_block(
            context,
            &blocks.then_statements,
            &mut then_bindings,
            counters,
            arm_scope,
        )?;
        let mut else_bindings = base_bindings.clone();
        let else_checked = match &blocks.alternative {
            // [ERR-2] the else-free `if` is the empty-alternative form, so its
            // False arm is the empty block rather than a missing one.
            ConditionalAlternative::Absent => {
                self.check_block(context, &[], &mut else_bindings, counters, arm_scope)?
            }
            ConditionalAlternative::Block(statements) => {
                self.check_block(context, statements, &mut else_bindings, counters, arm_scope)?
            }
            // An `else if` chain: the nested conditional is the whole
            // alternative and is not wrapped in a `stmt` node. It never opens
            // a delivery context of its own — [GIVE-1] gives the whole chain
            // one delivery set, belonging to the chain's binding.
            ConditionalAlternative::Chain(nested) => {
                let chained = self.check_conditional(
                    context,
                    *nested,
                    &mut else_bindings,
                    counters,
                    arm_scope,
                    false,
                )?;
                BlockResult {
                    statements: vec![CheckedStatement::Match {
                        condition: chained.condition,
                        node_path: self.types.declarations.tree.path(*nested)?.clone(),
                        scrutinee: chained.scrutinee,
                        match_type: chained.match_type,
                        arms: chained.arms,
                        continues: chained.can_continue,
                    }],
                    can_continue: chained.can_continue,
                    effects: chained.effects,
                    all_paths_deliver: chained.all_paths_deliver,
                    give_states: chained.give_states,
                    loop_transfers: chained.loop_transfers,
                }
            }
        };

        let mut arms = Vec::with_capacity(2);
        let mut normal_states = Vec::new();
        let mut normal_labels: Vec<String> = Vec::new();
        let mut give_states = Vec::new();
        let mut give_labels: Vec<String> = Vec::new();
        let mut loop_transfers = Vec::new();
        let mut effects = condition.effects.clone();
        let mut all_paths_deliver = true;
        // The then-block is the `True` arm and the alternative is the `False`
        // arm, tagged from the one Bool descriptor the `match` spelling used
        // so the two spellings cannot drift apart. `bool_descriptor` lists the
        // variants in that order.
        let descriptor = Checker::bool_descriptor();
        for (variant, (mut checked, mut branch_bindings)) in descriptor
            .variants
            .iter()
            .zip([(then_checked, then_bindings), (else_checked, else_bindings)])
        {
            let leaving = Checker::bindings_leaving_scope(&branch_bindings, &base_keys);
            Checker::invalidate_control_exits(
                &mut branch_bindings,
                &mut checked.give_states,
                &mut checked.loop_transfers,
                arm_scope.give_context,
                &leaving,
            );
            let fallthrough_drops = if checked.can_continue {
                self.types.live_affine_drops(
                    check_context,
                    &branch_bindings,
                    &base_key_set,
                    node,
                )?
            } else {
                Vec::new()
            };
            let label = if variant.tag == 1 {
                "the `if` branch".to_owned()
            } else if matches!(blocks.alternative, ConditionalAlternative::Absent) {
                let mut label =
                    "the implicit false edge of this `if` (no `else` is written)".to_owned();
                if matches!(
                    condition.expression,
                    CheckedExpression::Constant(super::super::super::model::CheckedValue::Bool(_))
                ) {
                    label.push_str("; constant conditions do not remove this structural edge [FN-1]");
                }
                label
            } else {
                "the `else` branch".to_owned()
            };
            if checked.can_continue {
                normal_states.push(branch_bindings);
                normal_labels.push(label.clone());
            }
            all_paths_deliver &= !checked.can_continue && checked.all_paths_deliver;
            effects = effects.union(checked.effects);
            give_labels.extend(std::iter::repeat_n(
                format!("a delivering edge of {label}"),
                checked.give_states.len(),
            ));
            give_states.extend(checked.give_states);
            loop_transfers.extend(checked.loop_transfers);
            arms.push(CheckedMatchArm {
                node_path: self.types.declarations.tree.path(node)?.clone(),
                label,
                selection: CheckedMatchSelection::Variant(variant.tag),
                binders: Vec::new(),
                covered: Vec::new(),
                body: checked.statements,
                fallthrough_drops,
            });
        }
        if opens_delivery {
            if !all_paths_deliver {
                return self.types.declarations.issue_node(
                    SemanticRule::Give1,
                    node,
                    SemanticIssueKind::InvalidGive,
                );
            }
            self.types
                .declarations
                .join_states(&base_keys, &give_states, &give_labels, node, bindings)
                .map_err(|stop| self.conditional_join_repair(check_context, &base_bindings, stop))?;
        } else {
            self.types
                .declarations
                .join_states(&base_keys, &normal_states, &normal_labels, node, bindings)
                .map_err(|stop| self.conditional_join_repair(check_context, &base_bindings, stop))?;
        }
        Ok(MatchResult {
            condition: Some(self.types.declarations.tree.path(expression_node)?.clone()),
            scrutinee: condition.expression,
            match_type: CheckedMatchType::Enum(CheckedEnumType::Bool),
            arms,
            delivered: local_give_context.as_ref().and_then(GiveContext::delivered),
            delivered_reference: local_give_context
                .as_ref()
                .and_then(GiveContext::delivered_reference),
            can_continue: if opens_delivery {
                !give_states.is_empty()
            } else {
                !normal_states.is_empty()
            },
            all_paths_deliver,
            effects,
            give_states: if opens_delivery {
                Vec::new()
            } else {
                give_states
            },
            loop_transfers,
        })
    }

    fn conditional_join_repair(
        &self,
        context: &CheckContext<'_>,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        mut stop: CheckStop,
    ) -> CheckStop {
        if let CheckStop::Issue(issue) = &mut stop
            && let SemanticIssueKind::LivenessJoinDisagreement {
                binding,
                mechanical_fix,
                ..
            } = &mut issue.kind
            && let Some(local) = bindings.iter().find_map(|(declaration, local)| {
                (self
                    .types
                    .declarations
                    .declaration_spelling(*declaration)
                    .ok()
                    .as_ref()
                    == Some(binding))
                .then_some(local)
            })
            && local.mode == CheckedMode::Own
            && matches!(self.types.is_copy_type(context, local.ty), Ok(false))
            && matches!(self.types.linear_release_obligation(context, local.ty), Ok(None))
            && let Ok(ty) = self.types.checked_type_name(local.ty)
        {
            *mechanical_fix = super::super::repairs::writer_ownership::join(Some((binding, &ty)));
        }
        stop
    }

    fn bool_descriptor() -> MatchDescriptor {
        MatchDescriptor {
            enum_type: CheckedEnumType::Bool,
            variants: vec![
                VariantDescriptor {
                    name: "True".to_owned(),
                    tag: 1,
                    fields: Vec::new(),
                },
                VariantDescriptor {
                    name: "False".to_owned(),
                    tag: 0,
                    fields: Vec::new(),
                },
            ],
        }
    }

    /// [GRAM-12, TYPE-6, ERR-2] enum header and complete-variant judgments.
    fn resolve_enum_match_arms(
        &self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        descriptor: &MatchDescriptor,
        arm_nodes: &[NodeId],
    ) -> Result<Vec<ArmDescriptor>, CheckStop> {
        let mut seen = HashSet::new();
        let mut duplicate = None;
        let mut arms = Vec::with_capacity(arm_nodes.len());
        for arm_node in arm_nodes {
            if !matches!(
                self.types.declarations.tree.match_arm_header(*arm_node)?,
                MatchArmHeader::Variant(_)
            ) {
                return self.types.declarations.issue_node(
                    SemanticRule::Gram12,
                    *arm_node,
                    SemanticIssueKind::InvalidIntegerMatchArm {
                        reason: "an enum scrutinee requires a variant TYPEID and payload binders"
                            .to_owned(),
                        mechanical_fix: "write a variant header belonging to the scrutinee's enum"
                            .to_owned(),
                    },
                );
            }
            let variant = self
                .types
                .declarations
                .match_variant(descriptor, *arm_node)?
                .clone();
            if let CheckedEnumType::Nominal(owner) = descriptor.enum_type {
                self.types.reject_inaccessible_variant(
                    check_context,
                    owner,
                    &variant.name,
                    *arm_node,
                )?;
            }
            if !seen.insert(variant.tag) {
                duplicate.get_or_insert(variant.name.clone());
            }
            arms.push(ArmDescriptor {
                name: variant.name.clone(),
                selection: CheckedMatchSelection::Variant(variant.tag),
                variant: Some(variant),
            });
        }
        let missing_variants = descriptor
            .variants
            .iter()
            .filter(|variant| !seen.contains(&variant.tag))
            .map(|variant| variant.name.clone())
            .collect::<Vec<_>>();
        if !missing_variants.is_empty() {
            return self.types.declarations.issue_node(
                SemanticRule::Err2,
                node,
                SemanticIssueKind::NonExhaustiveMatch { missing_variants },
            );
        }
        if let Some(variant) = duplicate {
            return self.types.declarations.issue_node(
                SemanticRule::Err2,
                node,
                SemanticIssueKind::DuplicateMatchVariant {
                    variant,
                    mechanical_fix: "remove the repeated variant arm".to_owned(),
                },
            );
        }
        Ok(arms)
    }

    fn resolve_integer_match_arms(
        &self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        ty: CheckedType,
        arm_nodes: &[NodeId],
    ) -> Result<Vec<ArmDescriptor>, CheckStop> {
        let mut seen = HashMap::new();
        let mut fallback = None;
        let mut arms = Vec::with_capacity(arm_nodes.len());
        for (ordinal, arm_node) in arm_nodes.iter().copied().enumerate() {
            let header = self.types.declarations.tree.match_arm_header(arm_node)?;
            if header == MatchArmHeader::Else {
                if fallback.is_some() || ordinal + 1 != arm_nodes.len() {
                    return self.types.declarations.issue_node(
                        SemanticRule::Gram12,
                        arm_node,
                        SemanticIssueKind::InvalidIntegerMatchArm {
                            reason: "an integer match has at most one `else`, in its final arm"
                                .to_owned(),
                            mechanical_fix: "keep one `else` arm and move it after every value arm"
                                .to_owned(),
                        },
                    );
                }
                fallback = Some(arm_node);
                arms.push(ArmDescriptor {
                    name: "else".to_owned(),
                    selection: CheckedMatchSelection::Else,
                    variant: None,
                });
                continue;
            }
            let (value, name) = match header {
                MatchArmHeader::Literal(token) => {
                    let bytes = self.types.declarations.tree.token_bytes(token)?;
                    let value = if crate::syntax::terminal::is_generic_numeric_literal(bytes) {
                        let identity = self.types.check_generic_numeric_identity(context, arm_node, bytes.first() == Some(&b'1'))?;
                        let CheckedExpression::Constant(value) = identity.expression else {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        };
                        value
                    } else {
                        self.types.declarations.parse_literal(arm_node, token)?
                    };
                    (value, String::from_utf8_lossy(bytes).into_owned())
                }
                MatchArmHeader::Constant(token) => {
                    let usage = self.types.declarations.use_at(context.check_context, arm_node, LexicalUseRole::IntegerMatchValue)?;
                    let ResolvedTarget::Source { declaration, class: DeclarationClass::NamedConst } = usage.target() else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    let constant = self.types.constants.get(&declaration).copied().ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    (self.types.constant(constant)?.value.clone(), String::from_utf8_lossy(self.types.declarations.tree.token_bytes(token)?).into_owned())
                }
                MatchArmHeader::Variant(_) => return self.types.declarations.issue_node(SemanticRule::Gram12, arm_node,
                    SemanticIssueKind::InvalidIntegerMatchArm {
                        reason: "an integer scrutinee requires one integer literal, named const, or `else`".to_owned(),
                        mechanical_fix: "replace the variant header with one constant of the scrutinee's integer type".to_owned(),
                    }),
                MatchArmHeader::Else => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
            };
            let integer = match value {
                CheckedValue::Integer {
                    ty: integer_type,
                    bits,
                } if CheckedType::Integer(integer_type) == ty => {
                    let value = i128::from(bits);
                    if integer_type.signed() && bits & (1_u64 << (integer_type.width() - 1)) != 0 {
                        value - (1_i128 << integer_type.width())
                    } else {
                        value
                    }
                }
                CheckedValue::NumericIdentity {
                    ty: value_type,
                    one,
                } if value_type == ty && matches!(ty, CheckedType::GenericInt(_)) => {
                    i128::from(one)
                }
                _ => {
                    return self.types.declarations.issue_node(
                        SemanticRule::Gram12,
                        arm_node,
                        SemanticIssueKind::InvalidIntegerMatchArm {
                            reason: format!(
                                "the arm value must have exact integer type {}",
                                self.types.checked_type_name(ty)?
                            ),
                            mechanical_fix: format!(
                                "write an integer literal or named const of type {}",
                                self.types.checked_type_name(ty)?
                            ),
                        },
                    );
                }
            };
            if let Some(earlier) = seen.insert(integer, arm_node) {
                return self.types.declarations.issue_node(
                    SemanticRule::Gram12,
                    arm_node,
                    SemanticIssueKind::DuplicateIntegerMatchValue {
                        value: integer,
                        earlier: SemanticLocation::SourceNode(
                            self.types.declarations.tree.path(earlier)?.clone(),
                            self.types.declarations.tree.coordinate(earlier)?,
                        ),
                        mechanical_fix: "remove the later arm or give it a distinct integer value"
                            .to_owned(),
                    },
                );
            }
            arms.push(ArmDescriptor {
                name,
                selection: CheckedMatchSelection::Integer(integer),
                variant: None,
            });
        }
        if seen.is_empty() {
            return self.types.declarations.issue_node(
                SemanticRule::Gram12,
                node,
                SemanticIssueKind::InvalidIntegerMatchArm {
                    reason: "an integer match has at least one value arm".to_owned(),
                    mechanical_fix: "add a literal or named-const value arm".to_owned(),
                },
            );
        }
        // Distinct well-typed labels all lie in the domain, so cardinality
        // decides completeness without enumerating the type's values.
        let (complete, domain) = match ty {
            CheckedType::Integer(integer_type) => {
                let count = 1_u128 << integer_type.width();
                (
                    seen.len() as u128 == count,
                    format!("all {count} mathematical values"),
                )
            }
            CheckedType::GenericInt(_) => (
                false,
                "the complete domain of every admitted Int type".to_owned(),
            ),
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        if complete == fallback.is_some() {
            return self.types.declarations.issue_node(
                SemanticRule::Err2,
                node,
                SemanticIssueKind::InvalidIntegerMatchCoverage {
                    integer_type: self.types.checked_type_name(ty)?,
                    domain,
                    mechanical_fix: if complete {
                        "remove the unnecessary `else` arm"
                    } else {
                        "add one final `else => { ... }` arm to cover every other value"
                    }
                    .to_owned(),
                },
            );
        }
        Ok(arms)
    }

    /// [GRAM-10] bind the payload fields in declared order and derive covered releases.
    #[allow(clippy::too_many_arguments)]
    fn check_match_binders(
        &mut self,
        check_context: &CheckContext<'_>,
        variant: &VariantDescriptor,
        (enum_type, variant_ordinal): (CheckedEnumType, Option<usize>),
        arm: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        loop_depth: usize,
        scrutinee: &super::super::TypedExpression,
    ) -> Result<(Vec<CheckedMatchBinder>, Vec<CheckedProjectedDrop>), CheckStop> {
        let mode = scrutinee.mode;
        let written = if let Some(list) = self
            .types
            .declarations
            .tree
            .first_child_with(arm, Production::FieldbindList)?
        {
            self.types
                .declarations
                .tree
                .children_with(list, Production::Fieldbind)?
        } else {
            Vec::new()
        };
        // [GRAM-10] the rest marker is the arm's own `..` token; a range in
        // the arm body spells its `..` inside a statement, not on the arm.
        let rest = self
            .types
            .declarations
            .tree
            .has_fixed(arm, FixedTerminal::DotDot)?;
        let mut binders = Vec::with_capacity(written.len());
        let mut covered = Vec::new();
        let mut cursor = 0_usize;
        for written in written {
            let spelling = self
                .types
                .declarations
                .deferred_use_at(written, DeferredUseRole::MatchField)?
                .spelling()
                .to_owned();
            // Every written field name appears once, in declared order, so
            // the next one is found at or after the field the previous binder
            // took; only a final `..` lets the arm skip fields.
            let Some(offset) = variant.fields[cursor..]
                .iter()
                .position(|field| field.name == spelling)
            else {
                return self
                    .types
                    .declarations
                    .invalid_match_fields(variant, written);
            };
            if offset > 0 && !rest {
                return self
                    .types
                    .declarations
                    .invalid_match_fields(variant, written);
            }
            covered.extend(cursor..cursor.saturating_add(offset));
            let index = cursor + offset;
            cursor = index + 1;
            let field = &variant.fields[index];
            // [MOD-5] outside the enum's declaring module an arm binds only
            // published payload fields; `..` covers the rest.
            if let CheckedEnumType::Nominal(owner) = enum_type
                && let Some(ordinal) = variant_ordinal
            {
                self.types.reject_inaccessible_field(
                    check_context,
                    owner,
                    Some(ordinal),
                    index,
                    &field.name,
                    written,
                )?;
            }
            let declaration = self
                .types
                .declarations
                .declaration_at(written, DeclarationRole::MatchBinder)?;
            let binding = Checker::allocate_binding(counters.next_binding)?;
            counters
                .binding_names
                .push(declaration.spelling().to_owned());
            let field_ordinal =
                u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            // [OWN-13] matching through a reference leaves the scrutinee live
            // and binds each payload as a reference naming the scrutinee path
            // extended by that payload step [REF-1]. The arm's refinement fact
            // admits the selection [ENT-3.S15]; the selected place outlives
            // the arm until its enum is replaced [REF-2]. Matching an own
            // place moves it instead, and its binders receive own payloads.
            let reference = if mode.is_reference() {
                let parent = scrutinee
                    .reference
                    .as_ref()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let step = PlaceStep::Payload {
                    variant: variant.tag,
                    field: field_ordinal,
                };
                let mut payload = parent.clone();
                payload.extend(step);
                Some(payload)
            } else {
                None
            };
            if let Some(reference) = &reference {
                self.body
                    .record_reference_origins(binding, &reference.paths);
            }
            if bindings
                .insert(
                    declaration.id(),
                    LocalBinding {
                        binding,
                        declaration: declaration.id(),
                        mode,
                        ty: field.ty,
                        live: true,
                        loop_depth,
                        compiler_updated: false,
                        read_only_state: false,
                        reference,
                        commit_target: None,
                        refinement_witnesses: Vec::new(),
                        handle_watch: None,
                        call_value: false,
                    },
                )
                .is_some()
            {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            binders.push(CheckedMatchBinder {
                node_path: self.types.declarations.tree.path(written)?.clone(),
                binding,
                field: field_ordinal,
                mode,
                ty: field.ty,
            });
        }
        if cursor < variant.fields.len() {
            if !rest {
                return self.types.declarations.invalid_match_fields(variant, arm);
            }
            covered.extend(cursor..variant.fields.len());
        }
        // [WIN-3, STOR-3] an own-place match consumes the scrutinee, so each
        // covered payload takes its release on entry to the arm and a covered
        // linear payload cannot be released at all; a reference match leaves
        // the scrutinee live and covers without releasing.
        let mut releases = Vec::new();
        if !mode.is_reference() {
            for index in covered {
                let field = &variant.fields[index];
                if self
                    .types
                    .linear_release_obligation(check_context, field.ty)?
                    .is_some()
                {
                    return self.types.declarations.issue_node(
                        SemanticRule::Win3,
                        arm,
                        SemanticIssueKind::InvalidElementMove {
                            mechanical_fix: "bind the linear payload in the arm and consume it: V(f: a, ..) => { ... }",
                        },
                    );
                }
                let ordinal =
                    u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                if !self
                    .types
                    .drop_paths(check_context, field.ty, vec![ordinal])?
                    .is_empty()
                {
                    releases.push(CheckedProjectedDrop {
                        fields: vec![ordinal],
                        ty: field.ty,
                    });
                }
            }
        }
        Ok((binders, releases))
    }

    pub(super) fn forget_refinement_occurrence(
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        origin: &crate::NodePath,
    ) {
        for local in bindings.values_mut() {
            local
                .refinement_witnesses
                .retain(|witness| &witness.origin != origin);
        }
    }

    /// Applies [REF-2]'s two arm/branch-exit events to every state that can
    /// cross that boundary. A normal edge, `give`, and `break` carry separate
    /// ownership maps, while a delivered reference is accumulated separately
    /// in its [`GiveContext`]; all four must observe the same exit.
    pub(super) fn invalidate_control_exits(
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        give_states: &mut [HashMap<DeclarationId, LocalBinding>],
        loop_transfers: &mut [LoopTransferState],
        give_context: Option<&GiveContext>,
        leaving: &[super::super::super::model::BindingId],
    ) {
        Checker::invalidate_references_leaving_scope(bindings, leaving);
        for state in give_states.iter_mut() {
            Checker::invalidate_references_leaving_scope(state, leaving);
        }
        for state in loop_transfers.iter_mut() {
            state.invalidate_references_leaving_scope(leaving);
        }
        if let Some(context) = give_context
            && !give_states.is_empty()
        {
            context.invalidate_reference_roots_leaving_scope(leaving);
        }
    }
}

impl<'unit> DeclarationInventory<'unit> {
    fn scrutinee_spelling(&self, expression: NodeId) -> Result<ScrutineeSpelling, CheckStop> {
        // [GRAM-5] `expr := atom infix_tail? | call`, and [OWN-13] asks its
        // question of a *place* scrutinee: an `infix_tail` makes the
        // expression an operation over two atoms, whose value is "a non-place
        // expression scrutinee", an owned temporary moved into the match,
        // whatever the first atom happens to spell.
        if self
            .tree
            .first_child_with(expression, Production::InfixTail)?
            .is_some()
        {
            return Ok(ScrutineeSpelling::Other);
        }
        let Some(atom) = self.tree.first_child_with(expression, Production::Atom)? else {
            return Ok(ScrutineeSpelling::Other);
        };
        if self
            .tree
            .first_child_with(atom, Production::BorrowExpr)?
            .is_some()
        {
            return Ok(ScrutineeSpelling::Borrowed);
        }
        let Some(place) = self.tree.first_child_with(atom, Production::Place)? else {
            return Ok(ScrutineeSpelling::Other);
        };
        Ok(if self.tree.place_has_dereference(place)? {
            ScrutineeSpelling::Dereferenced
        } else {
            ScrutineeSpelling::OwnedPlace
        })
    }
    /// [GRAM-6] the two `else` spellings the rule refuses, each reported at
    /// the node the rule names.
    fn reject_unspellable_else(
        &self,
        node: NodeId,
        alternative: &ConditionalAlternative,
        value_if: bool,
    ) -> Result<(), CheckStop> {
        let ConditionalAlternative::Block(statements) = alternative else {
            return Ok(());
        };
        if statements.is_empty() {
            // A `value_if`'s empty `else` delivers nothing, and that is
            // [GIVE-1]'s empty delivery set rather than this rejection.
            if value_if {
                return Ok(());
            }
            return self.issue_node(
                SemanticRule::Gram6,
                node,
                SemanticIssueKind::InvalidConditionalForm {
                    mechanical_fix: "delete the empty `else` and spell the else-free `if`",
                },
            );
        }
        let [only] = statements.as_slice() else {
            return Ok(());
        };
        let nested = self.tree.only_child(*only)?;
        if self.tree.production(nested)? != Production::IfStmt {
            return Ok(());
        }
        // In a `value_if` whose else block is exactly one else-free `if`, the
        // branch cannot deliver, [GIVE-1] owns that rejection, and the chain
        // form could not be spelled anyway — so GRAM-6 forms no candidate.
        if value_if
            && matches!(
                self.tree.conditional_blocks(nested)?.alternative,
                ConditionalAlternative::Absent
            )
        {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Gram6,
            nested,
            SemanticIssueKind::InvalidConditionalForm {
                mechanical_fix: "flatten the nested `if` to `else if`",
            },
        )
    }
    /// [TYPE-6] an arm label resolves against the scrutinee's already known
    /// enum type: it names one of that enum's variants, whatever other enum
    /// declares a variant of the same spelling.
    fn match_variant<'descriptor>(
        &self,
        descriptor: &'descriptor MatchDescriptor,
        arm: NodeId,
    ) -> Result<&'descriptor VariantDescriptor, CheckStop> {
        let label = self.deferred_use_at(arm, DeferredUseRole::ArmVariant)?;
        descriptor
            .variants
            .iter()
            .find(|variant| variant.name == label.spelling())
            .ok_or_else(|| {
                self.issue_value(
                    SemanticRule::Type6,
                    arm,
                    SemanticIssueKind::ForeignMatchVariant,
                )
            })
    }
    fn invalid_match_fields<ResultValue>(
        &self,
        variant: &VariantDescriptor,
        node: NodeId,
    ) -> Result<ResultValue, CheckStop> {
        self.issue_node(
            SemanticRule::Gram10,
            node,
            SemanticIssueKind::InvalidMatchFields {
                variant: variant.name.clone(),
                declared_fields: variant
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect(),
            },
        )
    }
    /// [LIV-1] the join of every predecessor's ownership state.
    ///
    /// Liveness is judged first and is a source rejection: every predecessor
    /// must agree on the live-or-dead status of every binding in scope, and a
    /// disagreement names the binding and the two predecessors. Only then does
    /// the checker's own capability limit on joining the remaining state
    /// apply, so a disagreeing predecessor pair can never be answered with a
    /// stop instead of a rejection.
    pub(super) fn join_states(
        &self,
        base_keys: &[DeclarationId],
        states: &[HashMap<DeclarationId, LocalBinding>],
        labels: &[String],
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        if states.len() != labels.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let Some(first) = states.first() else {
            return Ok(());
        };
        for key in base_keys {
            let mut joined = first
                .get(key)
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let mut live_predecessor = labels
                .first()
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for (state, label) in states.iter().zip(labels).skip(1) {
                let candidate = state
                    .get(key)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if candidate.live != joined.live {
                    let (live, dead) = if joined.live {
                        (live_predecessor.clone(), label.clone())
                    } else {
                        (label.clone(), live_predecessor.clone())
                    };
                    return self.issue_node(
                        SemanticRule::Liv1,
                        node,
                        SemanticIssueKind::LivenessJoinDisagreement {
                            binding: self.declaration_spelling(*key)?,
                            live_predecessor: live,
                            dead_predecessor: dead,
                            mechanical_fix: super::super::repairs::writer_ownership::join(None),
                        },
                    );
                }
                if !joined.agrees_with(candidate) {
                    return self.unsupported(UnsupportedSemanticFeature::OwnershipJoin, node);
                }
                if joined.live {
                    live_predecessor.clone_from(label);
                }
                joined.join_from(candidate);
            }
            if let Some(reference) = &mut joined.reference {
                reference.anchor_selection(self.tree.path(node)?, joined.binding);
            }
            *bindings
                .get_mut(key)
                .ok_or(SemanticCompilerFailure::InvalidResolution)? = joined;
        }
        Ok(())
    }
}

impl<'unit> TypeContext<'unit> {
    fn match_descriptor(
        &self,
        ty: CheckedType,
        node: NodeId,
    ) -> Result<MatchDescriptor, CheckStop> {
        match ty {
            // [GRAM-6] conditional control is type-driven and each form is the
            // sole legal one for its class, so a Bool scrutinee is rejected
            // here whatever its arms spell. Its descriptor survives below for
            // `if`, which is the spelling this class does take.
            CheckedType::Bool => self.declarations.issue_node(
                SemanticRule::Gram6,
                node,
                SemanticIssueKind::InvalidConditionalForm {
                    mechanical_fix: "spell the Bool conditional `if`",
                },
            ),
            CheckedType::Nominal(id) => {
                // [TYPE-7]'s implicit read was already excluded by the caller,
                // so a non-enum nominal here is the scrutinee's own mismatch.
                let CheckedNominalKind::Enum { variants } = &self.nominal(id)?.kind else {
                    return self.declarations.issue_node(
                        SemanticRule::Type5,
                        node,
                        SemanticIssueKind::type_mismatch(
                            "an integer or enum scrutinee, whose values or variants the arms match",
                            self.checked_type_name(ty)?,
                        ),
                    );
                };
                let variants = variants
                    .iter()
                    .map(|variant| VariantDescriptor {
                        name: variant.name.clone(),
                        tag: variant.tag,
                        fields: variant.fields.clone(),
                    })
                    .collect();
                Ok(MatchDescriptor {
                    enum_type: CheckedEnumType::Nominal(id),
                    variants,
                })
            }
            _ => self.declarations.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "an integer or enum scrutinee, whose values or variants the arms match",
                    self.checked_type_name(ty)?,
                ),
            ),
        }
    }
}
