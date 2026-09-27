//! [FN-4] compatibility of the existing FN-8/FN-9 clause trees after
//! binding. Ordinary clause checking owns formation and define expansion;
//! the shared entailment engine owns each finite implication query.

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{DeclarationId, NodePath, Production, SemanticCompilerFailure, SemanticRule};

use super::super::super::entailment::{
    CallGoalDisposition, EntailmentContext, FunctionEntailment, analyze_function, contract_implies,
    finalize_function_entailment,
};
use super::super::super::goal::{CheckedRequirement, GoalDatum, GoalExpression, GoalTemplate};
use super::super::super::model::{
    BindingId, CheckedBoundPostcondition, CheckedCallContract, CheckedContractQuery,
    CheckedExpression, CheckedFunction, CheckedParameter, CheckedType, ContractQueryId,
};
use super::super::super::postcondition::PostconditionConstantOrigin;
use super::super::requires::{ExpandedClauseDatum, ExpandedClauseExpression};
use super::super::{CheckStop, Checker, ControlCounters, FunctionSignature};

#[derive(Eq, PartialEq)]
struct InterfaceContracts {
    requires: Vec<InterfaceRequirement>,
    ensures: Vec<InterfaceEnsures>,
}

#[derive(Eq, PartialEq)]
struct InterfaceRequirement {
    clause: NodePath,
    template: GoalTemplate,
    /// The [ENT-2] clause (b) places the requirement forms.
    places: Vec<CheckedExpression>,
}

#[derive(Eq, PartialEq)]
struct InterfaceEnsures {
    clause: NodePath,
    ordinal: u32,
    variant: Option<crate::BuiltinPreludeId>,
    field: Option<crate::BuiltinPreludeId>,
    result_type: CheckedType,
    expression: ExpandedClauseExpression,
    relation_ordinal: u32,
    selector: super::super::super::postcondition::CheckedPostconditionSelector,
    relation: super::super::super::postcondition::RelationTemplate,
}

#[derive(Clone, Copy)]
struct ContractVariable {
    mode: super::super::super::model::CheckedMode,
    ty: CheckedType,
}

impl<'unit> Checker<'unit> {
    pub(super) fn check_formal_contract_formation(
        &self,
        signature: &FunctionSignature,
    ) -> Result<(), CheckStop> {
        self.check_entry_formers(signature)?;
        let contracts = self.behavior_contracts(signature)?;
        self.judge_formal_requirement_places(signature, &contracts.requires)
    }

    /// [ENT-2, FN-8] a formal's block has an ordinary block's formation rules,
    /// so each requirement forms its places at the body entry of whatever
    /// function is bound to it, in the state holding the formal's requirements
    /// written before its clause, and their subscripts owe [OP-4] there. That
    /// state is the hypothetical premise set every [FN-4] query over these
    /// requirements establishes, so no query premise names a place whose
    /// subscripts nothing judged.
    fn judge_formal_requirement_places(
        &self,
        signature: &FunctionSignature,
        requires: &[InterfaceRequirement],
    ) -> Result<(), CheckStop> {
        if requires
            .iter()
            .all(|requirement| requirement.places.is_empty())
        {
            return Ok(());
        }
        let variables = signature
            .parameters
            .iter()
            .map(|parameter| ContractVariable {
                mode: parameter.mode,
                ty: parameter.ty,
            })
            .collect::<Vec<_>>();
        let premises = requires
            .iter()
            .map(|requirement| {
                (
                    requirement.clause.clone(),
                    requirement.template.clone(),
                    requirement.places.clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut function = self.contract_hypothesis(signature, &variables, premises)?;
        // A residual is read at the formal's own clause, so it names the
        // formal's own parameters rather than the alpha-renamed datums.
        for (parameter, formal) in function.parameters.iter_mut().zip(&signature.parameters) {
            parameter.name.clone_from(&formal.name);
        }
        self.form_function_obligation_records(&mut function)?;
        let entailment =
            self.with_contract_context(&function, |context| analyze_function(&function, context));
        function.entailment = entailment;
        self.entailment_rejection(&function)
    }

    pub(super) fn check_behavior_contracts(
        &self,
        node: NodeId,
        instance: Option<super::super::super::model::FunctionId>,
        formal: &FunctionSignature,
        actual: &FunctionSignature,
    ) -> Result<CheckedCallContract, CheckStop> {
        // Retain formal source identities for resolution, with the exact
        // implementation types.
        let mut normalized = formal.clone();
        for (left, right) in normalized.parameters.iter_mut().zip(&actual.parameters) {
            left.mode = right.mode;
            left.ty = right.ty;
        }
        for (left, right) in normalized.results.iter_mut().zip(&actual.results) {
            left.mode = right.mode;
            left.ty = right.ty;
        }
        normalized.result = actual.result;
        normalized.result_mode = actual.result_mode;
        normalized.result_list = actual.result_list;
        let formal_contracts = self.behavior_contracts(&normalized)?;
        let actual_contracts = self.behavior_contracts(actual)?;
        let site = self.tree.path(node)?.clone();
        let mut accepted_queries = Vec::new();
        let requirement_variables = normalized
            .parameters
            .iter()
            .map(|parameter| ContractVariable {
                mode: parameter.mode,
                ty: parameter.ty,
            })
            .collect::<Vec<_>>();
        // Weaker precondition: every requirement the actual states must
        // follow from the formal set its callers establish.
        let formal_requirements = formal_contracts
            .requires
            .iter()
            .map(|requirement| requirement.template.clone())
            .collect::<Vec<_>>();
        let formal_requirement_paths = formal_contracts
            .requires
            .iter()
            .map(|requirement| requirement.clause.clone())
            .collect::<Vec<_>>();
        for required in &actual_contracts.requires {
            let mut proof = self.contract_implication(
                &normalized,
                &requirement_variables,
                &formal_requirement_paths,
                &formal_requirements,
                &required.template.root,
            )?;
            if !contract_query_discharged(&proof) {
                return self.behavior_mismatch(
                    SemanticRule::Fn4,
                    node,
                    "a requirement no caller of the formal interface establishes",
                );
            }
            finalize_function_entailment(&mut proof);
            accepted_queries.push(CheckedContractQuery {
                instance,
                site: site.clone(),
                premises: formal_requirement_paths.clone(),
                goal: required.clause.clone(),
                proof,
            });
        }
        // Stronger postcondition: at each result route, the actual set alone
        // must discharge every relation the formal promises there. Entry
        // parameter, exit parameter and result identities are distinct even
        // where their selected types and written projections agree.
        let parameter_count = normalized.parameters.len();
        let mut postcondition_premises = Vec::with_capacity(formal_contracts.ensures.len());
        for promised in &formal_contracts.ensures {
            let available = actual_contracts
                .ensures
                .iter()
                .filter(|published| ensures_available_on_route(published, promised))
                .collect::<Vec<_>>();
            let premises = available
                .iter()
                .map(|published| {
                    expanded_contract_goal(&published.expression, parameter_count)
                        .map(GoalTemplate::new)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let premise_paths = available
                .iter()
                .map(|published| published.clause.clone())
                .collect::<Vec<_>>();
            let goal = expanded_contract_goal(&promised.expression, parameter_count)?;
            let postcondition_variables = contract_postcondition_variables(&normalized, promised);
            let mut proof = self.contract_implication(
                &normalized,
                &postcondition_variables,
                &premise_paths,
                &premises,
                &goal,
            )?;
            if !contract_query_discharged(&proof) {
                return self.behavior_mismatch(
                    SemanticRule::Fn4,
                    node,
                    "a promised relation the supplied function does not publish",
                );
            }
            finalize_function_entailment(&mut proof);
            accepted_queries.push(CheckedContractQuery {
                instance,
                site: site.clone(),
                premises: premise_paths,
                goal: promised.clause.clone(),
                proof,
            });
            postcondition_premises.push(
                available
                    .iter()
                    .map(|published| published.relation_ordinal)
                    .collect::<Vec<_>>(),
            );
        }
        let mut retained = self.contract_queries.borrow_mut();
        let mut query_ids = Vec::with_capacity(accepted_queries.len());
        for query in accepted_queries {
            let index = if let Some(index) = retained.iter().position(|existing| *existing == query)
            {
                index
            } else {
                retained.push(query);
                retained.len() - 1
            };
            query_ids.push(ContractQueryId(
                u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            ));
        }
        let requirement_count = actual_contracts.requires.len();
        let postcondition_queries = query_ids.split_off(requirement_count);
        let postconditions = formal_contracts
            .ensures
            .into_iter()
            .zip(postcondition_queries)
            .zip(postcondition_premises)
            .map(
                |((promised, query), actual_premises)| CheckedBoundPostcondition {
                    selector: promised.selector,
                    relation: promised.relation,
                    query,
                    actual_premises,
                },
            )
            .collect();
        Ok(CheckedCallContract {
            requirements: formal_contracts
                .requires
                .into_iter()
                .map(|requirement| CheckedRequirement {
                    template: requirement.template,
                    clause: requirement.clause,
                })
                .collect(),
            requirement_queries: query_ids,
            postconditions,
        })
    }

    /// One finite [FN-4] query over alpha-renamed declaration datums. The
    /// temporary checked function has no body and publishes no summary; its
    /// requirements are only the hypothetical premise set submitted to the
    /// ordinary S4/AUTO entry. A formal requirement premise's places were
    /// judged in this same premise state when the formal was formed
    /// [`Self::judge_formal_requirement_places`], so the query forms none.
    fn contract_implication(
        &self,
        signature: &FunctionSignature,
        variables: &[ContractVariable],
        premise_paths: &[NodePath],
        premises: &[GoalTemplate],
        goal: &GoalExpression,
    ) -> Result<FunctionEntailment, CheckStop> {
        if premise_paths.len() != premises.len() {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let premises = premise_paths
            .iter()
            .zip(premises)
            .map(|(clause, template)| (clause.clone(), template.clone(), Vec::new()))
            .collect();
        let function = self.contract_hypothesis(signature, variables, premises)?;
        Ok(self.with_contract_context(&function, |context| {
            contract_implies(&function, context, goal)
        }))
    }

    /// The body-less function one contract query or formation judgment
    /// runs in: its parameters are alpha-renamed contract datums, and its
    /// requirements are the hypothetical premise set, each with the clause
    /// places it forms.
    fn contract_hypothesis(
        &self,
        signature: &FunctionSignature,
        variables: &[ContractVariable],
        premises: Vec<(NodePath, GoalTemplate, Vec<CheckedExpression>)>,
    ) -> Result<CheckedFunction, CheckStop> {
        let parameters = variables
            .iter()
            .enumerate()
            .map(|(ordinal, variable)| {
                let ordinal =
                    u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                Ok(CheckedParameter {
                    name: format!("contract_{ordinal}"),
                    declaration: DeclarationId::from_index(ordinal as usize)
                        .ok_or(SemanticCompilerFailure::CounterOverflow)?,
                    node_path: NodePath {
                        components: vec![ordinal],
                    },
                    binding: BindingId(ordinal),
                    mode: variable.mode,
                    ty: variable.ty,
                    range_element: None,
                })
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        let mut requirements = Vec::with_capacity(premises.len());
        let mut requirement_places = Vec::with_capacity(premises.len());
        for (clause, template, places) in premises {
            requirements.push(CheckedRequirement { template, clause });
            requirement_places.push(places);
        }
        Ok(CheckedFunction {
            formal_hypothesis: true,
            id: signature.id,
            declaration: signature.declaration,
            module: self
                .resolved
                .declaration(signature.declaration)
                .and_then(crate::DeclarationRecord::module)
                .unwrap_or(crate::ModuleId::BUNDLE_ROOT),
            name: signature.name.clone(),
            symbol: signature.symbol.clone(),
            function_actuals: Vec::new(),
            region_parameters: Vec::new(),
            parameters,
            result_mode: signature.result_mode,
            result: signature.result,
            declared_state_writes: Vec::new(),
            requirements,
            requirement_places,
            postconditions: Vec::new(),
            body: None,
            reference_origins: Vec::new(),
            body_disposition: Default::default(),
            allocates: false,
            call_separations: Vec::new(),
            permission_separation_queries: Vec::new(),
            obligations: Vec::new(),
            entailment: FunctionEntailment::default(),
        })
    }

    /// Runs `query` in the declaration-only proof context of one contract
    /// hypothesis: no callee, contract query or verified summary is visible.
    fn with_contract_context<ResultValue>(
        &self,
        function: &CheckedFunction,
        query: impl FnOnce(&EntailmentContext<'_>) -> ResultValue,
    ) -> ResultValue {
        let binding_names = function
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        let elements = self.elements.borrow();
        let const_parameter_types = self.const_generic_types().collect();
        let context = EntailmentContext {
            declarations: self.resolved.declarations(),
            callees: &[],
            constants: &self.checked_constants,
            constant_ids: &self.constants,
            const_parameter_types: &const_parameter_types,
            nominals: &self.nominals,
            elements: &elements,
            contract_queries: &[],
            verified_postconditions: &[],
            verified_postcondition_proofs: &[],
            binding_names: &binding_names,
        };
        query(&context)
    }

    fn behavior_contracts(
        &self,
        signature: &FunctionSignature,
    ) -> Result<InterfaceContracts, CheckStop> {
        let Some(block) = self
            .tree
            .first_child_with(signature.node, Production::ContractBlock)?
        else {
            return Ok(InterfaceContracts {
                requires: Vec::new(),
                ensures: Vec::new(),
            });
        };
        let mut bindings = HashMap::new();
        for (ordinal, parameter) in signature.parameters.iter().enumerate() {
            let binding = BindingId(
                u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            );
            bindings.insert(
                parameter.declaration,
                self.parameter_local(parameter, binding)?,
            );
        }
        let mut next_binding =
            u32::try_from(bindings.len()).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
        let mut next_loop = 0;
        let mut names = signature
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect();
        let mut counters = ControlCounters {
            next_binding: &mut next_binding,
            next_loop: &mut next_loop,
            binding_names: &mut names,
        };
        let checked =
            self.check_requires(signature, block, &mut bindings.clone(), &mut counters)?;
        let requires = checked
            .requirements
            .into_iter()
            .zip(checked.places)
            .map(|(requirement, places)| InterfaceRequirement {
                clause: requirement.clause,
                template: requirement.template,
                places,
            })
            .collect();
        let mut ensures = Vec::new();
        let selectors = self.postcondition_selectors_from_source(signature)?;
        let mut relations = Vec::with_capacity(selectors.len());
        for (relation_ordinal, selector) in selectors.iter().enumerate() {
            let relation = self.check_postcondition_clause(
                signature,
                selector,
                &mut bindings.clone(),
                &mut counters,
            )?;
            relations.push(relation.clone());
            let mut expression = self.expand_postcondition_clause(
                signature,
                selector,
                &mut bindings.clone(),
                &mut counters,
            )?;
            normalize_substituted_constants(&mut expression);
            ensures.push(InterfaceEnsures {
                clause: selector.block.clone(),
                ordinal: selector.ordinal,
                variant: selector.variant,
                field: selector.field.as_ref().map(|field| field.declaration),
                result_type: selector.result_type,
                expression,
                relation_ordinal: u32::try_from(relation_ordinal)
                    .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                selector: selector.clone(),
                relation,
            });
        }
        self.check_published_relation_consistency(signature, &selectors, &relations)?;
        Ok(InterfaceContracts { requires, ensures })
    }
}

fn contract_query_discharged(proof: &FunctionEntailment) -> bool {
    matches!(
        proof.contract_goals.as_slice(),
        [outcome] if outcome.disposition == CallGoalDisposition::Discharged
            && outcome.derivation.is_some()
    )
}

/// The clauses available everywhere a promised clause is selected. An
/// unrouted actual clause holds on every normal return, including an Ok route;
/// a routed clause holds only on that exact result ordinal and route.
fn ensures_available_on_route(published: &InterfaceEnsures, promised: &InterfaceEnsures) -> bool {
    match (published.variant, promised.variant) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(published_variant), Some(promised_variant)) => {
            published.ordinal == promised.ordinal
                && published_variant == promised_variant
                && published.field == promised.field
        }
    }
}

fn contract_postcondition_variables(
    signature: &FunctionSignature,
    selector: &InterfaceEnsures,
) -> Vec<ContractVariable> {
    signature
        .parameters
        .iter()
        .chain(signature.parameters.iter())
        .map(|parameter| ContractVariable {
            mode: parameter.mode,
            ty: parameter.ty,
        })
        .chain(
            signature
                .results
                .iter()
                .enumerate()
                .map(|(ordinal, result)| ContractVariable {
                    mode: result.mode,
                    // A routed ordinal's datum is the Ok payload, not the
                    // whole Result value declared at that ordinal [FN-9].
                    ty: if selector.variant.is_some()
                        && usize::try_from(selector.ordinal).ok() == Some(ordinal)
                    {
                        selector.result_type
                    } else {
                        result.ty
                    },
                }),
        )
        .collect()
}

fn expanded_contract_goal(
    expression: &ExpandedClauseExpression,
    parameter_count: usize,
) -> Result<GoalExpression, CheckStop> {
    let parameter_count =
        u32::try_from(parameter_count).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
    Ok(match expression {
        ExpandedClauseExpression::Datum(ExpandedClauseDatum::Parameter {
            ordinal,
            projections,
            ty,
            exit_state,
        }) => GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: if *exit_state {
                parameter_count
                    .checked_add(*ordinal)
                    .ok_or(SemanticCompilerFailure::CounterOverflow)?
            } else {
                *ordinal
            },
            projections: projections.clone(),
            ty: *ty,
        }),
        ExpandedClauseExpression::Datum(ExpandedClauseDatum::Result {
            ordinal,
            projections,
            ty,
        }) => GoalExpression::Datum(GoalDatum::Parameter {
            ordinal: parameter_count
                .checked_mul(2)
                .and_then(|base| base.checked_add(*ordinal))
                .ok_or(SemanticCompilerFailure::CounterOverflow)?,
            projections: projections.clone(),
            ty: *ty,
        }),
        ExpandedClauseExpression::Datum(ExpandedClauseDatum::NamedConst {
            declaration,
            projections,
            ty,
        }) => GoalExpression::Datum(GoalDatum::NamedConst {
            declaration: *declaration,
            projections: projections.clone(),
            ty: *ty,
        }),
        ExpandedClauseExpression::Datum(ExpandedClauseDatum::Literal { value, .. }) => {
            GoalExpression::Datum(GoalDatum::Literal(value.clone()))
        }
        ExpandedClauseExpression::Operation {
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
                .map(|argument| expanded_contract_goal(argument, parameter_count as usize))
                .collect::<Result<Vec<_>, _>>()?,
        },
        ExpandedClauseExpression::InvalidSelectorUse { .. } => {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
    })
}

fn normalize_substituted_constants(expression: &mut ExpandedClauseExpression) {
    match expression {
        ExpandedClauseExpression::Datum(ExpandedClauseDatum::Literal { value, origin }) => {
            if matches!(value.ty(), CheckedType::Integer(_))
                && matches!(
                    origin,
                    PostconditionConstantOrigin::ConstGeneric { .. }
                        | PostconditionConstantOrigin::GenericNumericIdentity { .. }
                )
            {
                *origin = PostconditionConstantOrigin::Literal;
            }
        }
        ExpandedClauseExpression::Operation { arguments, .. } => {
            for argument in arguments {
                normalize_substituted_constants(argument);
            }
        }
        ExpandedClauseExpression::Datum(_)
        | ExpandedClauseExpression::InvalidSelectorUse { .. } => {}
    }
}
