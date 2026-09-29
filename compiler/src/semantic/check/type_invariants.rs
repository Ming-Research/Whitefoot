//! Struct type invariants [TYPE-11].
//!
//! A struct's type invariant is formed once, as the requirement clause of a
//! function whose one parameter is the invariant's binder, into a predicate
//! over a parameter at ordinal zero [FN-8] and a difference-bound relation
//! over that parameter's exit state [FN-9]. Every use substitutes its own
//! subject for ordinal zero: a callable's parameter or referent, its exit
//! state, a result ordinal, or a construction's field operands.

use std::collections::HashMap;

use super::requires::{ClauseKind, ExpandedClauseDatum, ExpandedClauseExpression};
use super::*;
use crate::semantic::goal::{
    CheckedCallRequirement, ConcreteGoal, GoalDatum, GoalExpression, GoalProjection, GoalTemplate,
};
use crate::semantic::postcondition::{
    CheckedPostcondition, CheckedPostconditionSelector, ParameterDenotation, PostconditionPlace,
    PostconditionPlaceRoot, RelationDatum, RelationTemplate, RelationTerm,
};
use crate::{DeclarationClass, FixedTerminal, LexicalUseRole, ResolvedTarget};

const TYPE11_DROP_THE_GENERICS: &str = "declare the invariant on a struct without generics, or state the relation as a `requires` and `ensures` pair on each function that takes the value";
const TYPE11_DROP_OPAQUE: &str = "declare the invariant on a struct that is not `opaque`";
const TYPE11_MAKE_THE_FIELD_READONLY: &str = "write the field `public readonly`, so that code outside the declaring module reads it and only that module writes it, or make it private";
const TYPE11_RENAME_THE_INVARIANT: &str = "give each type invariant of the struct its own name";
const TYPE11_STATE_ONE_COMPARISON: &str = "state the invariant as one comparison between two sides, such as `table.next < table.slots.len`; write two invariants for a conjunction";
const TYPE11_ONE_DATUM_A_SIDE: &str = "keep one field or measure of the binder on each side, displaced by a constant, such as `table.next + 1_u64 <= table.slots.len`";
const TYPE11_NAME_THE_BINDER: &str =
    "relate at least one field or measure of the binder, such as `table.next`";

/// One formed type invariant, over a parameter at ordinal zero of the
/// struct's type.
#[derive(Clone, Debug)]
pub(super) struct TypeInvariantTemplate {
    /// The `type_invariant` occurrence, the clause identity of every
    /// requirement and postcondition it becomes [TYPE-11].
    pub(super) clause: NodePath,
    /// The predicate, as a requirement of a function whose parameter zero is
    /// a value of the struct [FN-8].
    pub(super) goal: GoalTemplate,
    /// The same relation, as a postcondition over parameter zero's exit
    /// state [FN-9, MSR-3].
    pub(super) relation: RelationTemplate,
    /// The binder's origin, the source candidate of every selector the
    /// relation's postconditions carry.
    pub(super) binder: crate::SourceOrigin,
}

impl<'unit> Checker<'_, 'unit> {
    /// [TYPE-11] forms every struct's type invariants once, before any
    /// function is checked, so that each callable taking or returning the
    /// struct receives them as implicit clauses.
    pub(super) fn collect_type_invariants(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        for template_index in 0..self.types.nominal_templates.len() {
            let template = self.types.nominal_templates[template_index].clone();
            if template.role != DeclarationRole::Struct {
                continue;
            }
            let tree = &self.types.declarations.tree;
            let invariants = tree.children_with(template.node, Production::TypeInvariant)?;
            let Some(first) = invariants.first().copied() else {
                continue;
            };
            if tree
                .first_child_with(template.node, Production::Generics)?
                .is_some()
            {
                return self.types.declarations.issue_node(
                    SemanticRule::Type11,
                    first,
                    SemanticIssueKind::InvalidTypeInvariant {
                        reason: "its struct declares generics",
                        mechanical_fix: TYPE11_DROP_THE_GENERICS,
                    },
                );
            }
            if tree.has_fixed(template.node, FixedTerminal::Opaque)? {
                return self.types.declarations.issue_node(
                    SemanticRule::Type11,
                    first,
                    SemanticIssueKind::InvalidTypeInvariant {
                        reason: "its struct is opaque",
                        mechanical_fix: TYPE11_DROP_OPAQUE,
                    },
                );
            }
            for field in tree.children_with(template.node, Production::Field)? {
                if tree.has_fixed(field, FixedTerminal::Public)?
                    && !tree.has_fixed(field, FixedTerminal::Readonly)?
                {
                    let name = self
                        .types
                        .declarations
                        .tree
                        .direct_token_with(field, crate::TerminalPredicate::Identifier)?
                        .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                    return self.types.declarations.issue_node(
                        SemanticRule::Type11,
                        field,
                        SemanticIssueKind::TypeInvariantWritableField {
                            field: String::from_utf8_lossy(
                                self.types.declarations.tree.token_bytes(name)?,
                            )
                            .into_owned(),
                            mechanical_fix: TYPE11_MAKE_THE_FIELD_READONLY,
                        },
                    );
                }
            }
            let nominal = self
                .types
                .source_nominal_instance(template.declaration, &GenericSubstitution::default())
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let mut names = Vec::with_capacity(invariants.len());
            let mut formed = Vec::with_capacity(invariants.len());
            for invariant in invariants {
                let name = self
                    .types
                    .declarations
                    .declaration_at(invariant, DeclarationRole::TypeInvariantName)?
                    .spelling()
                    .to_owned();
                if names.contains(&name) {
                    return self.types.declarations.issue_node(
                        SemanticRule::Type11,
                        invariant,
                        SemanticIssueKind::InvalidTypeInvariant {
                            reason: "another type invariant of its struct has its name",
                            mechanical_fix: TYPE11_RENAME_THE_INVARIANT,
                        },
                    );
                }
                names.push(name.clone());
                formed.push(self.form_type_invariant(
                    check_context,
                    template.declaration,
                    invariant,
                    name,
                    nominal,
                )?);
            }
            self.types.type_invariants.insert(nominal, formed);
        }
        Ok(())
    }

    /// One invariant, judged as the requirement clause of a function whose
    /// one parameter is its binder [TYPE-11, FN-8].
    fn form_type_invariant(
        &mut self,
        check_context: &CheckContext<'_>,
        declaration: DeclarationId,
        invariant: NodeId,
        name: String,
        nominal: NominalId,
    ) -> Result<TypeInvariantTemplate, CheckStop> {
        let binder = self
            .types
            .declarations
            .declaration_at(invariant, DeclarationRole::InvariantBinder)?;
        let ty = CheckedType::Nominal(nominal);
        let clause = self.types.declarations.tree.path(invariant)?.clone();
        let parameter = ParameterSignature {
            declaration: binder.id(),
            node_path: clause.clone(),
            name: binder.spelling().to_owned(),
            mode: CheckedMode::Own,
            ty,
        };
        let signature = FunctionSignature {
            id: FunctionId(u32::MAX),
            declaration: binder.id(),
            node: invariant,
            name,
            symbol: String::new(),
            region_parameters: Vec::new(),
            parameters: vec![parameter.clone()],
            result_mode: CheckedMode::Own,
            result: CheckedType::Unit,
            results: Vec::new(),
            result_list: None,
            effects_node: invariant,
            declared_effects: EffectSet::default(),
            waits: false,
            formal_parameter: None,
            substitution: GenericSubstitution::default(),
        };
        let check_context = &CheckContext {
            writing_module: self
                .types
                .declarations
                .resolved
                .declaration(declaration)
                .and_then(crate::DeclarationRecord::module),
            ..*check_context
        };
        let expression = self
            .types
            .declarations
            .tree
            .first_child_with(invariant, Production::ClauseExpr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        self.types.declarations.validate_clause_condition(
            ClauseKind::Requires,
            invariant,
            expression,
        )?;
        let binding = BindingId(0);
        let mut bindings =
            HashMap::from([(binder.id(), Checker::parameter_local(&parameter, binding)?)]);
        let mut body = BodyChecker::default();
        let condition = {
            let mut attempt = Checker::new(
                self.types,
                &mut body,
                self.analysis,
                self.reject_entailment,
                self.receipts,
            );
            attempt
                .check_expression(
                    FunctionContext {
                        check_context,
                        function: &signature,
                    },
                    expression,
                    &mut bindings,
                    0,
                )
                .map_err(Checker::clause_conditional_repair)?
        };
        self.types.validate_clause_checked_forms(
            ClauseKind::Requires,
            invariant,
            &condition.expression,
        )?;
        if condition.mode != CheckedMode::Own || condition.expression.ty() != CheckedType::Bool {
            return self.types.declarations.issue_node(
                SemanticRule::Op5,
                expression,
                SemanticIssueKind::InvalidPredicateCondition,
            );
        }
        // The binder denotes its value's exit state, so the relation below
        // is an output of the function whose parameter it is [FN-9]; the
        // predicate ignores the denotation [FN-8].
        let expanded_bindings = HashMap::from([(
            binding,
            ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
                ordinal: 0,
                projections: Vec::new(),
                ty,
                denotation: ParameterDenotation::ExitState,
            }),
        )]);
        let expanded = self.types.build_clause_expression(
            check_context,
            expression,
            &condition.expression,
            &bindings,
            &expanded_bindings,
        )?;
        let goal = GoalTemplate::new(
            expanded
                .clone()
                .into_goal_expression()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?,
        );
        let relation = self.types.type_invariant_relation(invariant, expanded)?;
        Ok(TypeInvariantTemplate {
            clause,
            goal,
            relation,
            binder: binder.origin().clone(),
        })
    }

    /// [TYPE-11] whether a parameter's written type is exactly a struct with
    /// type invariants, by value or behind `&`, and which one. A parameter
    /// whose written type is a type parameter takes none, whatever its
    /// instance.
    pub(super) fn declared_invariant_struct(
        &self,
        check_context: &CheckContext<'_>,
        parameter: &ParameterSignature,
    ) -> Result<Option<NominalId>, CheckStop> {
        if !matches!(parameter.mode, CheckedMode::Own | CheckedMode::Reference) {
            return Ok(None);
        }
        let CheckedType::Nominal(nominal) = parameter.ty else {
            return Ok(None);
        };
        if !self.types.type_invariants.contains_key(&nominal) {
            return Ok(None);
        }
        let node = self
            .types
            .declarations
            .tree
            .node_with_path(&parameter.node_path)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let Some(ty) = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Type)?
        else {
            return Ok(None);
        };
        self.types
            .written_invariant_struct(check_context, ty, nominal)
    }

    /// [TYPE-11] the requirements a callable's type invariants add after its
    /// written ones: each invariant of each parameter's declared struct, in
    /// parameter order, and those of `shared_new`'s state over its argument.
    pub(super) fn implicit_type_invariant_requirements(
        &self,
        check_context: &CheckContext<'_>,
        signature: &FunctionSignature,
    ) -> Result<Vec<CheckedRequirement>, CheckStop> {
        let mut requirements = Vec::new();
        for (ordinal, parameter) in signature.parameters.iter().enumerate() {
            if let Some(nominal) = self.declared_invariant_struct(check_context, parameter)? {
                let ordinal =
                    u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                requirements.extend(self.type_invariant_requirements(
                    nominal,
                    ordinal,
                    parameter.mode == CheckedMode::Reference,
                ));
            }
        }
        // [SHARE-1] the prelude's `shared_new` moves its argument into the
        // object's state, where every atomic statement assumes the state's
        // invariants [TYPE-11].
        if signature.name == "shared_new"
            && self.types.declarations.tree.is_body_less(signature.node)?
            && let Some(CheckedType::Nominal(nominal)) =
                signature.parameters.first().map(|parameter| parameter.ty)
        {
            requirements.extend(self.type_invariant_requirements(nominal, 0, false));
        }
        Ok(requirements)
    }

    /// [TYPE-11] each type invariant over the parameter at `ordinal`, or over
    /// its referent, as that callable's requirement [FN-8].
    pub(super) fn type_invariant_requirements(
        &self,
        nominal: NominalId,
        ordinal: u32,
        reference: bool,
    ) -> Vec<CheckedRequirement> {
        self.types
            .type_invariants
            .get(&nominal)
            .into_iter()
            .flatten()
            .map(|invariant| CheckedRequirement {
                template: GoalTemplate::new(substitute_goal(
                    &invariant.goal.root,
                    ordinal,
                    reference,
                )),
                clause: invariant.clause.clone(),
                subject: Some(ordinal),
            })
            .collect()
    }

    /// [TYPE-11] each type invariant over the exit state of a reference
    /// parameter whose row writes it, and over each result ordinal whose
    /// declared type is the struct, as that callable's postcondition after
    /// its written ones [FN-9], selecting every explicit return.
    pub(super) fn type_invariant_postconditions(
        &self,
        context: FunctionContext<'_, '_>,
        parameters: &[CheckedParameter],
        body: &[CheckedStatement],
    ) -> Result<Vec<CheckedPostcondition>, CheckStop> {
        let FunctionContext {
            check_context,
            function,
        } = context;
        let mut subjects = Vec::new();
        for (ordinal, parameter) in function.parameters.iter().enumerate() {
            if parameter.mode != CheckedMode::Reference
                || !super::ensures::parameter_has_exit_state(function, parameter)
            {
                continue;
            }
            if let Some(nominal) = self.declared_invariant_struct(check_context, parameter)? {
                let ordinal =
                    u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                subjects.push((nominal, InvariantSubject::ExitParameter(ordinal), 0));
            }
        }
        for (ordinal, result) in function.results.iter().enumerate() {
            let CheckedType::Nominal(nominal) = result.ty else {
                continue;
            };
            if result.mode != CheckedMode::Own || !self.types.type_invariants.contains_key(&nominal)
            {
                continue;
            }
            let Some(ty) = self
                .types
                .declarations
                .tree
                .first_child_with(result.rtype, Production::Type)?
            else {
                continue;
            };
            if self
                .types
                .written_invariant_struct(check_context, ty, nominal)?
                .is_some()
            {
                let ordinal =
                    u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                subjects.push((nominal, InvariantSubject::Result(ordinal), ordinal));
            }
        }
        let mut postconditions = Vec::new();
        for (nominal, subject, result_ordinal) in subjects {
            for invariant in self
                .types
                .type_invariants
                .get(&nominal)
                .into_iter()
                .flatten()
            {
                let selector = CheckedPostconditionSelector {
                    function: function.id,
                    block: invariant.clause.clone(),
                    selector: invariant.clause.clone(),
                    candidate: invariant.binder.clone(),
                    ordinal: result_ordinal,
                    variant: None,
                    field: None,
                    result_type: function
                        .results
                        .get(result_ordinal as usize)
                        .map_or(function.result, |result| result.ty),
                };
                postconditions.push(self.types.build_checked_postcondition(
                    context,
                    parameters,
                    selector,
                    substitute_relation(&invariant.relation, subject),
                    body,
                )?);
            }
        }
        Ok(postconditions)
    }

    /// [TYPE-11] each type invariant a construction of `nominal` owes, over
    /// its field operands' pre-construction images, and those images.
    pub(super) fn construction_invariants(
        &self,
        context: FunctionContext<'_, '_>,
        carrier: &NodePath,
        nominal: NominalId,
        operands: &[(NodeId, CheckedType, TypedExpression)],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(Vec<CheckedCallRequirement>, Vec<GoalExpression>), CheckStop> {
        let Some(templates) = self.types.type_invariants.get(&nominal) else {
            return Ok((Vec::new(), Vec::new()));
        };
        let FunctionContext {
            check_context,
            function,
        } = context;
        let mut arguments = Vec::with_capacity(operands.len());
        for (ordinal, (atom, ty, value)) in operands.iter().enumerate() {
            let ordinal =
                u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            arguments.push(self.types.call_goal_argument(
                check_context,
                function.id,
                carrier,
                ordinal,
                *atom,
                CheckedMode::Own,
                *ty,
                value,
                None,
                bindings,
            )?);
        }
        let invariants = templates
            .iter()
            .map(|template| {
                Ok(CheckedCallRequirement {
                    requires_clause: template.clause.clone(),
                    subject: None,
                    goal: ConcreteGoal::new(
                        construct_goal(&template.goal.root, &arguments)
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        Ok((invariants, arguments))
    }

    /// [TYPE-11] each type invariant of an atomic statement's state over the
    /// binder's referent, which the binder names as a reference parameter
    /// does: at itself, with no leading dereference.
    pub(super) fn atomic_invariants(
        &self,
        state: CheckedType,
        binding: BindingId,
    ) -> Vec<CheckedCallRequirement> {
        let CheckedType::Nominal(nominal) = state else {
            return Vec::new();
        };
        self.types
            .type_invariants
            .get(&nominal)
            .into_iter()
            .flatten()
            .filter_map(|invariant| {
                Some(CheckedCallRequirement {
                    requires_clause: invariant.clause.clone(),
                    subject: None,
                    goal: ConcreteGoal::new(binder_goal(&invariant.goal.root, binding)?),
                })
            })
            .collect()
    }
}

impl TypeContext<'_> {
    /// [TYPE-11] whether a written `type` names exactly `nominal`'s source
    /// struct, not a type parameter an instance substituted it for.
    pub(super) fn written_invariant_struct(
        &self,
        check_context: &CheckContext<'_>,
        ty: NodeId,
        nominal: NominalId,
    ) -> Result<Option<NominalId>, CheckStop> {
        if !self.declarations.tree.names_nominal(ty)? {
            return Ok(None);
        }
        let usage = self
            .declarations
            .use_at(check_context, ty, LexicalUseRole::Type)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::NominalType,
        } = usage.target()
        else {
            return Ok(None);
        };
        let written = self.source_nominal_instance(declaration, &GenericSubstitution::default());
        Ok((written == Some(nominal)).then_some(nominal))
    }

    /// [TYPE-11] one type invariant's relation: one comparison whose sides
    /// are each one relation term, at least one naming the binder.
    fn type_invariant_relation(
        &self,
        invariant: NodeId,
        expanded: ExpandedClauseExpression,
    ) -> Result<RelationTemplate, CheckStop> {
        let invalid = |reason, mechanical_fix| {
            self.declarations.issue_node(
                SemanticRule::Type11,
                invariant,
                SemanticIssueKind::InvalidTypeInvariant {
                    reason,
                    mechanical_fix,
                },
            )
        };
        let ExpandedClauseExpression::Operation {
            row:
                crate::semantic::goal::GoalOperation::Integer {
                    operation,
                    operand_type,
                },
            arguments,
            ..
        } = expanded
        else {
            return invalid(
                "its relation is not one comparison between two sides",
                TYPE11_STATE_ONE_COMPARISON,
            );
        };
        let Some(normalized) = super::ensures::normalized_relation(operation) else {
            return invalid(
                "its relation is not one comparison between two sides",
                TYPE11_STATE_ONE_COMPARISON,
            );
        };
        let [left, right] = arguments.as_slice() else {
            return invalid(
                "its relation is not one comparison between two sides",
                TYPE11_STATE_ONE_COMPARISON,
            );
        };
        let (Some(left), Some(right)) = (
            Checker::postcondition_relation_term(left, operand_type),
            Checker::postcondition_relation_term(right, operand_type),
        ) else {
            return invalid(
                "a side is not one datum displaced by a constant",
                TYPE11_ONE_DATUM_A_SIDE,
            );
        };
        if !left.datum.is_exit_state() && !right.datum.is_exit_state() {
            return invalid(
                "no side names a field or measure of its binder",
                TYPE11_NAME_THE_BINDER,
            );
        }
        Ok(RelationTemplate {
            operation,
            operands: [left, right],
            normalized,
        })
    }
}

/// [TYPE-11] the predicate over the parameter at `ordinal`: its value, or
/// its referent when `reference`, in place of parameter zero.
fn substitute_goal(expression: &GoalExpression, ordinal: u32, reference: bool) -> GoalExpression {
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: 0,
            projections,
            ty,
        }) => GoalExpression::Datum(GoalDatum::Parameter {
            ordinal,
            projections: reference
                .then_some(GoalProjection::Deref)
                .into_iter()
                .chain(projections.iter().copied())
                .collect(),
            ty: *ty,
        }),
        GoalExpression::Datum(datum) => GoalExpression::Datum(datum.clone()),
        GoalExpression::Operation {
            row,
            type_arguments,
            const_arguments,
            result,
            arguments,
        } => GoalExpression::Operation {
            row: *row,
            type_arguments: type_arguments.clone(),
            const_arguments: const_arguments.clone(),
            result: *result,
            arguments: arguments
                .iter()
                .map(|argument| substitute_goal(argument, ordinal, reference))
                .collect(),
        },
    }
}

/// [TYPE-11] the predicate over a construction: each field datum of
/// parameter zero read from that field's operand image instead.
fn construct_goal(
    expression: &GoalExpression,
    arguments: &[GoalExpression],
) -> Option<GoalExpression> {
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: 0,
            projections,
            ty,
        }) => {
            let (GoalProjection::Field(field), rest) = projections.split_first()? else {
                return None;
            };
            let mut image = arguments.get(*field as usize)?.clone();
            for projection in rest {
                image = image.with_projection(*projection, *ty)?;
            }
            Some(image)
        }
        GoalExpression::Datum(datum) => Some(GoalExpression::Datum(datum.clone())),
        GoalExpression::Operation {
            row,
            type_arguments,
            const_arguments,
            result,
            arguments: operands,
        } => Some(GoalExpression::Operation {
            row: *row,
            type_arguments: type_arguments.clone(),
            const_arguments: const_arguments.clone(),
            result: *result,
            arguments: operands
                .iter()
                .map(|operand| construct_goal(operand, arguments))
                .collect::<Option<Vec<_>>>()?,
        }),
    }
}

/// [TYPE-11] the predicate over an atomic statement's binder: parameter
/// zero's place is the place the binder names.
fn binder_goal(expression: &GoalExpression, binding: BindingId) -> Option<GoalExpression> {
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: 0,
            projections,
            ty,
        }) => Some(GoalExpression::Datum(GoalDatum::Place {
            root: binding,
            projections: projections.clone(),
            ty: *ty,
        })),
        GoalExpression::Datum(GoalDatum::Parameter { .. }) => None,
        GoalExpression::Datum(datum) => Some(GoalExpression::Datum(datum.clone())),
        GoalExpression::Operation {
            row,
            type_arguments,
            const_arguments,
            result,
            arguments,
        } => Some(GoalExpression::Operation {
            row: *row,
            type_arguments: type_arguments.clone(),
            const_arguments: const_arguments.clone(),
            result: *result,
            arguments: arguments
                .iter()
                .map(|argument| binder_goal(argument, binding))
                .collect::<Option<Vec<_>>>()?,
        }),
    }
}

/// Where a type invariant's relation is a postcondition [TYPE-11, FN-9].
#[derive(Clone, Copy)]
pub(super) enum InvariantSubject {
    /// The exit state of the reference parameter at this ordinal.
    ExitParameter(u32),
    /// The declared result at this ordinal.
    Result(u32),
}

/// [TYPE-11] the relation over `subject` in place of parameter zero's exit
/// state.
pub(super) fn substitute_relation(
    relation: &RelationTemplate,
    subject: InvariantSubject,
) -> RelationTemplate {
    let term = |term: &RelationTerm| RelationTerm {
        datum: substitute_relation_datum(&term.datum, subject),
        displacement: term.displacement,
    };
    RelationTemplate {
        operation: relation.operation,
        operands: [term(&relation.operands[0]), term(&relation.operands[1])],
        normalized: relation.normalized,
    }
}

fn substitute_relation_datum(datum: &RelationDatum, subject: InvariantSubject) -> RelationDatum {
    match (datum, subject) {
        (
            RelationDatum::Parameter {
                ordinal: 0,
                projections,
                ty,
                denotation: ParameterDenotation::ExitState,
            },
            InvariantSubject::ExitParameter(ordinal),
        ) => RelationDatum::Parameter {
            ordinal,
            projections: std::iter::once(GoalProjection::Deref)
                .chain(projections.iter().copied())
                .collect(),
            ty: *ty,
            denotation: ParameterDenotation::ExitState,
        },
        (
            RelationDatum::Parameter {
                ordinal: 0,
                projections,
                ty,
                denotation: ParameterDenotation::ExitState,
            },
            InvariantSubject::Result(ordinal),
        ) => RelationDatum::Result {
            ordinal,
            projections: projections.clone(),
            ty: *ty,
        },
        (
            RelationDatum::Measure(
                measure,
                PostconditionPlace {
                    root: PostconditionPlaceRoot::ExitParameter { ordinal: 0 },
                    projections,
                    ty,
                },
            ),
            subject,
        ) => RelationDatum::Measure(
            *measure,
            match subject {
                InvariantSubject::ExitParameter(ordinal) => PostconditionPlace {
                    root: PostconditionPlaceRoot::ExitParameter { ordinal },
                    projections: std::iter::once(GoalProjection::Deref)
                        .chain(projections.iter().copied())
                        .collect(),
                    ty: *ty,
                },
                InvariantSubject::Result(ordinal) => PostconditionPlace {
                    root: PostconditionPlaceRoot::Result { ordinal },
                    projections: projections.clone(),
                    ty: *ty,
                },
            },
        ),
        (datum, _) => datum.clone(),
    }
}
