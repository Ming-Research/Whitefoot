//! Struct type invariants [TYPE-11].
//!
//! A struct's type invariant is formed once, as the requirement clause of a
//! function whose one parameter is the invariant's binder, into a predicate
//! over a parameter at ordinal zero [FN-8] and a difference-bound relation
//! over that parameter's exit state [FN-9]. Every use substitutes its own
//! subject for ordinal zero: a parameter or referent the contract-read
//! footprint reads, a construction's field operands, an atomic target's
//! referent, or a sealed anchor [TYPE-11.S1].

use std::collections::HashMap;

use super::requires::{ClauseKind, ExpandedClauseDatum, ExpandedClauseExpression};
use super::*;
use crate::semantic::goal::{
    CheckedCallRequirement, ConcreteGoal, GoalDatum, GoalExpression, GoalProjection, GoalTemplate,
};
use crate::semantic::model::CheckedAffineInvariant;
use crate::semantic::postcondition::{
    NormalizedRelation, ParameterDenotation, PostconditionPlace, PostconditionPlaceRoot,
    RelationDatum, RelationTemplate,
};
use crate::FixedTerminal;

const TYPE11_DROP_THE_GENERICS: &str = "declare the invariant on a struct without generics, or state the relation as a `requires` and `ensures` pair on each function that takes the value";
const TYPE11_DROP_OPAQUE: &str = "declare the invariant on a struct that is not `opaque`";
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
    /// The same relation over parameter zero's exit state [FN-9, MSR-3],
    /// which a constant construction evaluates.
    pub(super) relation: RelationTemplate,
}

impl<'unit> Checker<'_, 'unit> {
    /// [TYPE-11] forms every struct's type invariants once, before any
    /// function is checked, so that every construction, atomic target and
    /// sealed anchor of the struct reads them.
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
                            mechanical_fix: super::repairs::INVARIANT_FIELD_READONLY,
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
                let range = self
                    .types
                    .declarations
                    .tree
                    .first_child_with(invariant, Production::RangeClause)?;
                let name = self
                    .types
                    .declarations
                    .declaration_at(
                        range.unwrap_or(invariant),
                        if range.is_some() {
                            DeclarationRole::RangeFact
                        } else {
                            DeclarationRole::TypeInvariantName
                        },
                    )?
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
                if let Some(range) = range {
                    let clause = self.form_range_type_invariant(
                        check_context,
                        template.declaration,
                        invariant,
                        range,
                        nominal,
                    )?;
                    self.types
                        .range_type_invariants
                        .entry(nominal)
                        .or_default()
                        .push(clause);
                    continue;
                }
                formed.push(self.form_type_invariant(
                    check_context,
                    template.declaration,
                    invariant,
                    name,
                    nominal,
                )?);
            }
            if !formed.is_empty()
                && let Some(record) = self.types.nominals.get_mut(nominal.0 as usize)
            {
                record.type_invariant = true;
                record.affine_invariants = formed.iter().map(affine_invariant).collect();
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
            contract_reads: None,
            waits: false,
            held_parameters: Vec::new(),
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
        })
    }

    /// [TYPE-11] whether an own or reference parameter's declared type,
    /// after the function instance's substitution [FN-2], is a struct with
    /// type invariants, by value or behind `&`, and which one. A parameter
    /// written as a type parameter takes the invariants of the struct its
    /// instance binds, exactly as one written as that struct does.
    pub(super) fn declared_invariant_struct(
        &self,
        parameter: &ParameterSignature,
    ) -> Option<NominalId> {
        if !matches!(parameter.mode, CheckedMode::Own | CheckedMode::Reference) {
            return None;
        }
        let CheckedType::Nominal(nominal) = parameter.ty else {
            return None;
        };
        self.types
            .type_invariants
            .contains_key(&nominal)
            .then_some(nominal)
    }

    /// [TYPE-11, FN-3] each invariant of each parameter's declared struct,
    /// over that parameter or its referent, in parameter order: the places
    /// they read belong to the callable's contract-read footprint. A call
    /// owes them only as the debts its hand-offs require [TYPE-11.S4].
    pub(super) fn implicit_type_invariant_requirements(
        &self,
        signature: &FunctionSignature,
    ) -> Result<Vec<CheckedRequirement>, CheckStop> {
        let mut requirements = Vec::new();
        for (ordinal, parameter) in signature.parameters.iter().enumerate() {
            if let Some(nominal) = self.declared_invariant_struct(parameter) {
                let ordinal =
                    u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                requirements.extend(self.type_invariant_requirements(
                    nominal,
                    ordinal,
                    parameter.mode == CheckedMode::Reference,
                ));
            }
        }
        Ok(requirements)
    }

    /// [FN-8] whether the build supplies a callable's definition, as it does
    /// for a [PRE-1] declaration and a [PRE-2] host function, so that no
    /// Whitefoot body proves a postcondition against it [SCOPE-3].
    pub(super) fn supplied_definition(
        &self,
        signature: &FunctionSignature,
    ) -> Result<bool, CheckStop> {
        let host = self
            .types
            .declarations
            .declaration_home(signature.declaration)
            .is_some_and(|(package, path)| {
                *package == crate::PackageKey::Standard && crate::library::is_host_module(path)
            });
        let prelude = self
            .types
            .declarations
            .tree
            .is_prelude_node(signature.node)?;
        Ok(host || prelude)
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

    /// [TYPE-11] each type invariant a construction of `nominal` owes, over
    /// its field operands' pre-construction images, and those images.
    pub(super) fn construction_invariants(
        &mut self,
        context: FunctionContext<'_, '_>,
        carrier: &NodePath,
        nominal: NominalId,
        operands: &[(NodeId, CheckedType, TypedExpression)],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(Vec<CheckedCallRequirement>, Vec<GoalExpression>), CheckStop> {
        if let Some(clauses) = self.types.range_type_invariants.get(&nominal) {
            self.body
                .range_facts
                .constructions
                .insert(carrier.clone(), clauses.clone());
        }
        let Some(templates) = self.types.type_invariants.get(&nominal) else {
            return Ok((Vec::new(), Vec::new()));
        };
        if templates.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
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
                self.body.exactness(),
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

impl Checker<'_, '_> {
    /// [TYPE-11] each struct construction a `const` initializer writes owes
    /// its struct's type invariants over the written field values, which
    /// decide it exactly. Judged once the invariants are formed, since an
    /// invariant may name a constant.
    pub(super) fn judge_constant_invariants(&self, items: &[NodeId]) -> Result<(), CheckStop> {
        if self.types.type_invariants.is_empty() {
            return Ok(());
        }
        for node in self.types.declarations.constant_order(items)? {
            let declaration = self
                .types
                .declarations
                .declaration_at(node, DeclarationRole::NamedConst)?
                .id();
            let Some(constant) = self
                .types
                .constants
                .get(&declaration)
                .and_then(|id| self.types.checked_constants.get(id.0 as usize))
            else {
                continue;
            };
            let value_node = self
                .types
                .declarations
                .tree
                .first_child_with(node, Production::Cvalue)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.judge_constant_value(value_node, &constant.value)?;
        }
        Ok(())
    }

    /// One written `cvalue` and its value: nested constructions first, then
    /// this one's own invariants. A `cvalue` naming another constant writes
    /// no construction here; that constant's declaration judged its own.
    fn judge_constant_value(&self, node: NodeId, value: &CheckedValue) -> Result<(), CheckStop> {
        let (
            nested,
            CheckedValue::Struct {
                fields: nested_values,
                ..
            }
            | CheckedValue::Array {
                elements: nested_values,
                ..
            },
        ) = (
            self.types
                .declarations
                .tree
                .children_with(node, Production::Cvalue)?,
            value,
        )
        else {
            return Ok(());
        };
        if nested.len() != nested_values.len() {
            return Ok(());
        }
        for (child, child_value) in nested.iter().zip(nested_values) {
            self.judge_constant_value(*child, child_value)?;
        }
        let CheckedValue::Struct {
            ty: CheckedType::Nominal(nominal),
            ..
        } = value
        else {
            return Ok(());
        };
        for clause in self
            .types
            .range_type_invariants
            .get(nominal)
            .into_iter()
            .flatten()
        {
            if let Some(issue) = crate::semantic::range_judgment::judge_constant_invariant(
                clause,
                value,
                self.types.declarations.tree.path(node)?,
            ) {
                return Err(self.range_issue(&issue));
            }
        }
        for invariant in self
            .types
            .type_invariants
            .get(nominal)
            .into_iter()
            .flatten()
        {
            let terms = invariant
                .relation
                .operands
                .iter()
                .map(|term| {
                    self.constant_datum(&term.datum, value)
                        .map(|datum| datum + term.displacement)
                })
                .collect::<Option<Vec<_>>>();
            let holds = terms
                .as_deref()
                .is_some_and(|terms| match invariant.relation.normalized {
                    NormalizedRelation::Equal => terms[0] == terms[1],
                    NormalizedRelation::NotEqual => terms[0] != terms[1],
                    NormalizedRelation::UpperBound {
                        left,
                        right,
                        strict,
                    } => {
                        let (left, right) = (terms[usize::from(left)], terms[usize::from(right)]);
                        if strict { left < right } else { left <= right }
                    }
                });
            if holds {
                continue;
            }
            let text = match terms.as_deref() {
                Some([left, right]) => format!(
                    "{left} {} {right}",
                    comparison_spelling(invariant.relation.operation)
                ),
                _ => {
                    let tree = &self.types.declarations.tree;
                    let clause = tree
                        .node_with_path(&invariant.clause)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let expression = tree
                        .first_child_with(clause, Production::ClauseExpr)?
                        .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                    tree.source_spelling(expression)?
                }
            };
            return self.types.declarations.issue_node(
                SemanticRule::Type11,
                node,
                SemanticIssueKind::UndischargedTypeInvariant {
                    type_invariant: self.types.declarations.node_location(&invariant.clause)?,
                    mechanical_fix: super::repairs::constant_construction_invariant(&text),
                    instantiated_goal: text,
                    disposition: if terms.is_some() {
                        crate::CallRequirementDisposition::Refuted
                    } else {
                        crate::CallRequirementDisposition::Unproved
                    },
                },
            );
        }
        Ok(())
    }

    /// One relation datum's mathematical value over a constant struct value
    /// in place of parameter zero.
    fn constant_datum(&self, datum: &RelationDatum, value: &CheckedValue) -> Option<i128> {
        let read = match datum {
            RelationDatum::Parameter {
                ordinal: 0,
                projections,
                ..
            } => constant_projection(value, projections)?,
            RelationDatum::NamedConst {
                declaration,
                projections,
                ..
            } => {
                let id = self.types.constants.get(declaration)?;
                let constant = self.types.checked_constants.get(id.0 as usize)?;
                constant_projection(&constant.value, projections)?
            }
            RelationDatum::Literal { value, .. } => value,
            RelationDatum::Measure(
                crate::semantic::model::CheckedMeasure::Length,
                PostconditionPlace {
                    root: PostconditionPlaceRoot::ExitParameter { ordinal: 0 },
                    projections,
                    ..
                },
            ) => {
                let CheckedValue::Array { elements, .. } = constant_projection(value, projections)?
                else {
                    return None;
                };
                return i128::try_from(elements.len()).ok();
            }
            _ => return None,
        };
        let CheckedValue::Integer { ty, bits } = read else {
            return None;
        };
        Some(crate::semantic::entailment::integer_value(*ty, *bits))
    }
}

/// The field a projection of struct-field steps reaches in a constant value.
fn constant_projection<'value>(
    value: &'value CheckedValue,
    projections: &[GoalProjection],
) -> Option<&'value CheckedValue> {
    projections
        .iter()
        .try_fold(value, |value, projection| match (projection, value) {
            (GoalProjection::Field(field), CheckedValue::Struct { fields, .. }) => {
                fields.get(*field as usize)
            }
            _ => None,
        })
}

/// The source spelling of a relation's comparison.
const fn comparison_spelling(
    operation: crate::semantic::model::CheckedIntegerOperation,
) -> &'static str {
    match operation {
        crate::semantic::model::CheckedIntegerOperation::Equal => "==",
        crate::semantic::model::CheckedIntegerOperation::NotEqual => "!=",
        crate::semantic::model::CheckedIntegerOperation::Less => "<",
        crate::semantic::model::CheckedIntegerOperation::LessEqual => "<=",
        crate::semantic::model::CheckedIntegerOperation::Greater => ">",
        _ => ">=",
    }
}

impl TypeContext<'_> {
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
        if left.parameter.is_some() || right.parameter.is_some() {
            return invalid(
                "a side is not one datum displaced by a constant",
                TYPE11_ONE_DATUM_A_SIDE,
            );
        }
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
/// [TYPE-11.S1] one formed invariant as the sealing rules read it: its
/// predicate over parameter zero, and the projection of each binder datum.
fn affine_invariant(template: &TypeInvariantTemplate) -> CheckedAffineInvariant {
    let mut data = Vec::new();
    binder_data(&template.goal.root, &mut data);
    CheckedAffineInvariant {
        clause: template.clause.clone(),
        goal: template.goal.root.clone(),
        data,
    }
}

/// Each distinct projection from parameter zero that `expression` reads.
fn binder_data(expression: &GoalExpression, data: &mut Vec<Vec<GoalProjection>>) {
    match expression {
        GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: 0,
            projections,
            ..
        }) => {
            if !data.contains(projections) {
                data.push(projections.clone());
            }
        }
        GoalExpression::Datum(_) => {}
        GoalExpression::Operation { arguments, .. } => {
            for argument in arguments {
                binder_data(argument, data);
            }
        }
    }
}

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
            admission: Default::default(),
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
