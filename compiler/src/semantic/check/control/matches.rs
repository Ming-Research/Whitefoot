use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationId, DeclarationRole, DeferredUseRole, FixedTerminal, Production,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::super::model::{
    CheckedEnumType, CheckedExpression, CheckedField, CheckedMatchArm, CheckedMatchBinder,
    CheckedMode, CheckedNominalKind, CheckedProjectedDrop, CheckedStatement, CheckedType,
};
use super::super::super::places::PlaceStep;
use super::super::super::tree::ConditionalAlternative;
use super::super::references::{ReferenceInfo, RequiredReferent};
use super::super::{
    CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, RefinementWitness,
};
use super::{BlockResult, BreakState, ControlCounters, ControlScope, GiveContext};

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

pub(super) struct MatchResult {
    pub(super) scrutinee: CheckedExpression,
    pub(super) enum_type: CheckedEnumType,
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
    pub(super) break_states: Vec<BreakState>,
}

/// How a `match` scrutinee is written, which is what [OWN-13] and [REF-1]
/// read to decide whether the match goes through a reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrutineeSpelling {
    /// `&x`: a `borrow_expr` naming the enum's own path [REF-1]. No `deref`
    /// step stands between the expression and the enum it names.
    Borrowed,
    /// `deref(p)` and anything selected below it: the storage a reference
    /// names, reached under the step [REF-1] requires.
    Dereferenced,
    /// Every other written form: an owned place, a call result, a literal.
    Other,
}

impl<'unit> Checker<'unit> {
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
        let Some(pbase) = self.tree.first_child_with(place, Production::Pbase)? else {
            return Ok(ScrutineeSpelling::Other);
        };
        Ok(if self.has_fixed(pbase, crate::FixedTerminal::Deref)? {
            ScrutineeSpelling::Dereferenced
        } else {
            ScrutineeSpelling::Other
        })
    }

    pub(super) fn check_match(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
        value_delivery: bool,
    ) -> Result<MatchResult, CheckStop> {
        let expression_node = self
            .tree
            .first_child_with(node, Production::Expr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let mut scrutinee =
            self.check_match_expression(function, expression_node, bindings, scope.loops.len())?;
        let spelling = self.scrutinee_spelling(expression_node)?;
        // [REF-1] a reference variable denotes the reference, and the storage
        // it names is reached only through `deref`, so a bare holder written
        // where the enum itself is required is that missing step. A `Box` is
        // not one of these at v0.60: its content is the ordinary field
        // `inner` [TYPE-9], so a `Box` scrutinee is the ordinary wrong-type
        // judgment [TYPE-5] the descriptor below makes.
        if spelling == ScrutineeSpelling::Other
            && scrutinee.reference_value
            && self
                .satisfies_referent_requirement(scrutinee.expression.ty(), RequiredReferent::Enum)?
        {
            return self.issue_node(
                SemanticRule::Type7,
                expression_node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(holder)`",
                },
            );
        }
        // [OWN-13] matching through a reference leaves the scrutinee live and
        // binds each payload as a reference naming the scrutinee path
        // extended by that payload step [REF-1]. `deref(p)` reads the value
        // at the path `p` names everywhere else, so the reading is made here,
        // where the rule distinguishes the two, and not at the place walk.
        if spelling == ScrutineeSpelling::Dereferenced && scrutinee.reference.is_none() {
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
            scrutinee.mode = CheckedMode::Reference;
            scrutinee.reference = Some(reference);
        }
        let scrutinee = scrutinee;
        let descriptor = self.match_descriptor(scrutinee.expression.ty(), expression_node)?;
        let base_bindings = bindings.clone();
        let base_keys = base_bindings.keys().copied().collect::<Vec<_>>();
        let base_key_set = base_keys.iter().copied().collect::<HashSet<_>>();
        let value_match = self.tree.production(node)? == Production::ValueMatch;
        if value_match != value_delivery {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let local_give_context = value_delivery.then(|| GiveContext::empty(&base_key_set, scope));
        let arm_nodes = self.tree.children_with(node, Production::Arm)?;
        let mut seen = HashSet::new();
        let mut duplicate_arm = None;
        let mut resolved_variants = Vec::with_capacity(arm_nodes.len());
        for arm_node in &arm_nodes {
            let variant = self.match_variant(&descriptor, *arm_node)?.clone();
            if let CheckedEnumType::Nominal(owner) = descriptor.enum_type {
                self.reject_inaccessible_variant(owner, &variant.name, *arm_node)?;
            }
            if !seen.insert(variant.tag) {
                duplicate_arm.get_or_insert(*arm_node);
            }
            resolved_variants.push(variant);
        }
        let missing_variants = descriptor
            .variants
            .iter()
            .filter(|variant| !seen.contains(&variant.tag))
            .map(|variant| variant.name.clone())
            .collect::<Vec<_>>();
        if !missing_variants.is_empty() {
            return self.issue_node(
                SemanticRule::Err2,
                node,
                SemanticIssueKind::NonExhaustiveMatch { missing_variants },
            );
        }
        if let Some(arm) = duplicate_arm {
            return self.unsupported(UnsupportedSemanticFeature::DuplicateMatchArm, arm);
        }

        let mut arms = Vec::with_capacity(arm_nodes.len());
        let mut normal_states = Vec::new();
        // [LIV-1] one label per predecessor state, in the order the states are
        // collected, so a liveness disagreement can name the two edges the
        // writer wrote rather than two indices.
        let mut normal_labels: Vec<String> = Vec::new();
        let mut give_states = Vec::new();
        let mut give_labels: Vec<String> = Vec::new();
        let mut break_states = Vec::new();
        let mut effects = scrutinee.effects.clone();
        let mut all_paths_deliver = true;
        for (arm_node, variant) in arm_nodes.into_iter().zip(&resolved_variants) {
            let arm_scope = ControlScope {
                loops: scope.loops,
                give_context: local_give_context.as_ref().or(scope.give_context),
            };
            let mut arm_bindings = base_bindings.clone();
            let variant_ordinal = descriptor
                .variants
                .iter()
                .position(|candidate| candidate.name == variant.name);
            let (binders, covered) = self.check_match_binders(
                variant,
                (descriptor.enum_type, variant_ordinal),
                arm_node,
                &mut arm_bindings,
                counters,
                scope.loops.len(),
                &scrutinee,
            )?;
            let statements = self.tree.children_with(arm_node, Production::Stmt)?;
            let mut checked = self.check_block(
                function,
                &statements,
                &mut arm_bindings,
                counters,
                arm_scope,
            )?;
            let leaving = Self::bindings_leaving_scope(&arm_bindings, &base_keys);
            Self::invalidate_control_exits(
                &mut arm_bindings,
                &mut checked.give_states,
                &mut checked.break_states,
                arm_scope.give_context,
                &leaving,
            );
            let fallthrough_drops = if checked.can_continue {
                self.live_affine_drops(&arm_bindings, &base_key_set, arm_node)?
            } else {
                Vec::new()
            };
            let label = format!("the `{}` arm", variant.name);
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
            break_states.extend(checked.break_states);
            arms.push(CheckedMatchArm {
                tag: variant.tag,
                binders,
                covered,
                body: checked.statements,
                fallthrough_drops,
            });
        }
        if value_match {
            if !all_paths_deliver {
                return self.issue_node(SemanticRule::Give1, node, SemanticIssueKind::InvalidGive);
            }
            self.join_states(&base_keys, &give_states, &give_labels, node, bindings)?;
        } else {
            self.join_states(&base_keys, &normal_states, &normal_labels, node, bindings)?;
        }
        Ok(MatchResult {
            scrutinee: scrutinee.expression,
            enum_type: descriptor.enum_type,
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
            break_states,
        })
    }

    /// [GRAM-6] the Bool conditional.
    ///
    /// It produces exactly the checked shape the Bool `match` produced before
    /// the spelling changed: a two-armed match over [`CheckedEnumType::Bool`]
    /// with `True` tagged 1 and `False` tagged 0. Lowering, entailment,
    /// cleanup, and drops therefore need no `if` of their own. The arms cannot
    /// come from [`Self::check_match`], which reads `arm` nodes and resolves
    /// each one's variant by constructor name; an `if` owns no arm at all, so
    /// its two are built here from the same descriptor.
    pub(super) fn check_if(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
        value_delivery: bool,
    ) -> Result<MatchResult, CheckStop> {
        if (self.tree.production(node)? == Production::ValueIf) != value_delivery {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        self.check_conditional(function, node, bindings, counters, scope, value_delivery)
    }

    /// The conditional body shared by both forms.
    ///
    /// `opens_delivery` is not "this is a `value_if`": [GIVE-1] gives an
    /// else-if chain one delivery set belonging to the chain's binding, so
    /// only the outermost `value_if` opens the context and every chained one
    /// contributes to it, exactly as a statement `match` propagates `give`s.
    fn check_conditional(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
        opens_delivery: bool,
    ) -> Result<MatchResult, CheckStop> {
        let value_if = self.tree.production(node)? == Production::ValueIf;
        let expression_node = self
            .tree
            .first_child_with(node, Production::Expr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let condition =
            self.check_match_expression(function, expression_node, bindings, scope.loops.len())?;
        // [TYPE-7] exclusivity, which [GRAM-6] keeps: a condition reached
        // through a holder is the implicit read, and its own `own Bool`
        // judgment forms no rejection. `RequiredReferent::Enum` already
        // admits `Bool`, the prelude enum this condition must be.
        if self.reads_implicitly_through_holder(
            condition.reference_value,
            condition.expression.ty(),
            RequiredReferent::Enum,
        )? {
            return self.issue_node(
                SemanticRule::Type7,
                expression_node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(holder)`",
                },
            );
        }
        // [GRAM-6] the condition takes [OP-5]'s judgment exactly; every
        // failure that is not TYPE-7's implicit read cites GRAM-6 here.
        if condition.expression.ty() != CheckedType::Bool || condition.mode != CheckedMode::Own {
            return self.issue_node(
                SemanticRule::Gram6,
                expression_node,
                SemanticIssueKind::InvalidConditionalForm {
                    mechanical_fix: "give the condition exact value mode and type `own Bool`",
                },
            );
        }
        // The exact owned Bool judgment gives the same non-escaping header
        // boundary as an owned enum match [OWN-6, GRAM-6].
        let blocks = self.tree.conditional_blocks(node)?;
        self.reject_unspellable_else(node, &blocks.alternative, value_if)?;

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
            function,
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
                self.check_block(function, &[], &mut else_bindings, counters, arm_scope)?
            }
            ConditionalAlternative::Block(statements) => self.check_block(
                function,
                statements,
                &mut else_bindings,
                counters,
                arm_scope,
            )?,
            // An `else if` chain: the nested conditional is the whole
            // alternative and is not wrapped in a `stmt` node. It never opens
            // a delivery context of its own — [GIVE-1] gives the whole chain
            // one delivery set, belonging to the chain's binding.
            ConditionalAlternative::Chain(nested) => {
                let chained = self.check_conditional(
                    function,
                    *nested,
                    &mut else_bindings,
                    counters,
                    arm_scope,
                    false,
                )?;
                BlockResult {
                    statements: vec![CheckedStatement::Match {
                        scrutinee: chained.scrutinee,
                        enum_type: chained.enum_type,
                        arms: chained.arms,
                        continues: chained.can_continue,
                    }],
                    can_continue: chained.can_continue,
                    effects: chained.effects,
                    all_paths_deliver: chained.all_paths_deliver,
                    give_states: chained.give_states,
                    break_states: chained.break_states,
                }
            }
        };

        let mut arms = Vec::with_capacity(2);
        let mut normal_states = Vec::new();
        let mut normal_labels: Vec<String> = Vec::new();
        let mut give_states = Vec::new();
        let mut give_labels: Vec<String> = Vec::new();
        let mut break_states = Vec::new();
        let mut effects = condition.effects.clone();
        let mut all_paths_deliver = true;
        // The then-block is the `True` arm and the alternative is the `False`
        // arm, tagged from the one Bool descriptor the `match` spelling used
        // so the two spellings cannot drift apart. `bool_descriptor` lists the
        // variants in that order.
        let descriptor = Self::bool_descriptor();
        for (variant, (mut checked, mut branch_bindings)) in descriptor
            .variants
            .iter()
            .zip([(then_checked, then_bindings), (else_checked, else_bindings)])
        {
            let leaving = Self::bindings_leaving_scope(&branch_bindings, &base_keys);
            Self::invalidate_control_exits(
                &mut branch_bindings,
                &mut checked.give_states,
                &mut checked.break_states,
                arm_scope.give_context,
                &leaving,
            );
            let fallthrough_drops = if checked.can_continue {
                self.live_affine_drops(&branch_bindings, &base_key_set, node)?
            } else {
                Vec::new()
            };
            let label = if variant.tag == 1 {
                "the `if` branch".to_owned()
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
            break_states.extend(checked.break_states);
            arms.push(CheckedMatchArm {
                tag: variant.tag,
                binders: Vec::new(),
                covered: Vec::new(),
                body: checked.statements,
                fallthrough_drops,
            });
        }
        if opens_delivery {
            if !all_paths_deliver {
                return self.issue_node(SemanticRule::Give1, node, SemanticIssueKind::InvalidGive);
            }
            self.join_states(&base_keys, &give_states, &give_labels, node, bindings)?;
        } else {
            self.join_states(&base_keys, &normal_states, &normal_labels, node, bindings)?;
        }
        Ok(MatchResult {
            scrutinee: condition.expression,
            enum_type: CheckedEnumType::Bool,
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
            break_states,
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
            CheckedType::Bool => self.issue_node(
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
                    return self.issue_node(
                        SemanticRule::Type5,
                        node,
                        SemanticIssueKind::type_mismatch(
                            "an enum scrutinee, whose variants the arms match",
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
            _ => self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "an enum scrutinee, whose variants the arms match",
                    self.checked_type_name(ty)?,
                ),
            ),
        }
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

    /// [GRAM-10] binds the payload fields an arm writes, in declared order,
    /// and returns with its binders the releases of the fields a final `..`
    /// covers in an own-place match [WIN-3, STOR-3].
    #[allow(clippy::too_many_arguments)]
    fn check_match_binders(
        &self,
        variant: &VariantDescriptor,
        (enum_type, variant_ordinal): (CheckedEnumType, Option<usize>),
        arm: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        loop_depth: usize,
        scrutinee: &super::super::TypedExpression,
    ) -> Result<(Vec<CheckedMatchBinder>, Vec<CheckedProjectedDrop>), CheckStop> {
        let mode = scrutinee.mode;
        let written =
            if let Some(list) = self.tree.first_child_with(arm, Production::FieldbindList)? {
                self.tree.children_with(list, Production::Fieldbind)?
            } else {
                Vec::new()
            };
        // [GRAM-10] the rest marker is the arm's own `..` token; a range in
        // the arm body spells its `..` inside a statement, not on the arm.
        let rest = self.has_fixed(arm, FixedTerminal::DotDot)?;
        let mut binders = Vec::with_capacity(written.len());
        let mut covered = Vec::new();
        let mut cursor = 0_usize;
        for written in written {
            let spelling = self
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
                return self.invalid_match_fields(variant, written);
            };
            if offset > 0 && !rest {
                return self.invalid_match_fields(variant, written);
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
                self.reject_inaccessible_field(owner, Some(ordinal), index, &field.name, written)?;
            }
            let declaration = self.declaration_at(written, DeclarationRole::MatchBinder)?;
            let binding = Self::allocate_binding(counters.next_binding)?;
            counters
                .binding_names
                .push(declaration.spelling().to_owned());
            let field_ordinal =
                u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            // [OWN-13] matching through a reference leaves the scrutinee live
            // and binds each payload as a reference naming the scrutinee path
            // extended by that payload step [REF-1], valid exactly while the
            // arm's refinement fact holds [REF-2, ENT-3.S15]. Matching an own
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
            let refinement_witnesses = if mode.is_reference() {
                let origin = self.tree.path(arm)?.clone();
                scrutinee
                    .reference
                    .as_ref()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .paths
                    .iter()
                    .cloned()
                    .map(|place| RefinementWitness {
                        origin: origin.clone(),
                        place,
                        variant: variant.tag,
                        valid: true,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            if let Some(reference) = &reference {
                self.record_reference_origins(binding, &reference.paths);
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
                        reference,
                        refinement_witnesses,
                    },
                )
                .is_some()
            {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            binders.push(CheckedMatchBinder {
                node_path: self.tree.path(written)?.clone(),
                binding,
                field: field_ordinal,
                mode,
                ty: field.ty,
            });
        }
        if cursor < variant.fields.len() {
            if !rest {
                return self.invalid_match_fields(variant, arm);
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
                if self.linear_release_obligation(field.ty)?.is_some() {
                    return self.issue_node(
                        SemanticRule::Win3,
                        arm,
                        SemanticIssueKind::InvalidElementMove {
                            mechanical_fix: "bind the linear payload in the arm and consume it: V(f: a, ..) => { ... }",
                        },
                    );
                }
                let ordinal =
                    u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                if !self.drop_paths(field.ty, vec![ordinal])?.is_empty() {
                    releases.push(CheckedProjectedDrop {
                        fields: vec![ordinal],
                        ty: field.ty,
                    });
                }
            }
        }
        Ok((binders, releases))
    }

    /// Applies [REF-2]'s two arm/branch-exit events to every state that can
    /// cross that boundary. A normal edge, `give`, and `break` carry separate
    /// ownership maps, while a delivered reference is accumulated separately
    /// in its [`GiveContext`]; all four must observe the same exit.
    fn invalidate_control_exits(
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        give_states: &mut [HashMap<DeclarationId, LocalBinding>],
        break_states: &mut [BreakState],
        give_context: Option<&GiveContext>,
        leaving: &[super::super::super::model::BindingId],
    ) {
        Self::invalidate_references_leaving_scope(bindings, leaving);
        for state in give_states.iter_mut() {
            Self::invalidate_references_leaving_scope(state, leaving);
        }
        for state in break_states.iter_mut() {
            state.invalidate_references_leaving_scope(leaving);
        }
        if let Some(context) = give_context
            && !give_states.is_empty()
        {
            context.invalidate_reference_roots_leaving_scope(leaving);
        }
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
                            mechanical_fix: "every predecessor of a join agrees on a binding's \
                                             live-or-dead status: consume it on every predecessor, \
                                             on none, or commit a value back into it before the \
                                             predecessor that consumed it reaches the join",
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
            *bindings
                .get_mut(key)
                .ok_or(SemanticCompilerFailure::InvalidResolution)? = joined;
        }
        Ok(())
    }
}
