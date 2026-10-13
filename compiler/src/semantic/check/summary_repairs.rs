//! Explanation of unavailable callee summaries, read only after rejection.

use super::{CheckStop, TypeContext, repairs};
use crate::semantic::entailment::{
    CallGoalDisposition, ComponentPostconditions, EntailmentCallee, EntailmentContext,
    PostconditionDisposition, VerifiedPostconditionSummary, postcondition_schedule,
    unavailable_summary_discharges_call,
};
use crate::semantic::model::{
    CheckedContractQuery, CheckedExpression, CheckedFunction, CheckedStatement, FunctionId,
};
use crate::{SemanticCompilerFailure, SemanticIssueKind, SemanticLocation};

pub(super) struct SummaryRepairInputs<'a> {
    pub(super) callees: &'a [EntailmentCallee],
    pub(super) contract_queries: &'a [CheckedContractQuery],
    pub(super) binding_names: &'a [String],
}

impl TypeContext<'_> {
    /// The selected rejection and every proof result stay unchanged. This
    /// inspection borrows the completed inventory only to explain its missing
    /// summaries; no retained analysis or proof receipt contains this text.
    pub(super) fn explain_unavailable_summary(
        &self,
        mut stop: CheckStop,
        caller: &CheckedFunction,
        functions: &[&CheckedFunction],
        inputs: &SummaryRepairInputs<'_>,
    ) -> CheckStop {
        if let CheckStop::Issue(issue) = &mut stop
            && let SemanticIssueKind::UndischargedPostcondition(detail) = &mut issue.kind
            && detail.disposition == crate::PostconditionProofDisposition::Unproved
            && let SemanticLocation::SourceNode(statement, _) = &issue.location
            && let Ok(Some(repair)) =
                self.unavailable_return_summary(caller, statement, functions)
        {
            detail.mechanical_fix = repair;
        }
        if let CheckStop::Issue(issue) = &mut stop
            && let SemanticIssueKind::UndischargedCallRequirement(detail) = &mut issue.kind
            && detail.disposition == crate::CallRequirementDisposition::Unproved
            && let SemanticLocation::SourceNode(call, _) = &issue.location
            && let SemanticLocation::SourceNode(clause, _) = &detail.requires_clause
            && let Ok(Some(repair)) =
                self.unavailable_call_summary(caller, call, clause, functions, inputs)
        {
            detail.mechanical_fix = repair;
        }
        stop
    }

    fn unavailable_call_summary(
        &self,
        caller: &CheckedFunction,
        call: &crate::NodePath,
        clause: &crate::NodePath,
        functions: &[&CheckedFunction],
        inputs: &SummaryRepairInputs<'_>,
    ) -> Result<Option<String>, CheckStop> {
        let Some(target) = caller.entailment.call_goals.iter().find(|outcome| {
            outcome.node_path == *call
                && outcome.requires_clause == *clause
                && outcome.disposition == CallGoalDisposition::Unproved
                && outcome.subject.is_none()
                && outcome.lost_header.is_none()
        }) else {
            return Ok(None);
        };
        let Some(schedule) = postcondition_schedule(functions.iter().copied()) else {
            return Ok(None);
        };
        let Some(&caller_component) = schedule.function_components.get(caller.id.0 as usize)
        else {
            return Ok(None);
        };
        let verified_postconditions = functions
            .iter()
            .map(|function| {
                function
                    .entailment
                    .postconditions
                    .iter()
                    .filter(|proof| proof.summary.is_some())
                    .filter_map(|proof| {
                        function.postconditions.get(proof.relation_ordinal as usize)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let verified_postcondition_proofs = functions
            .iter()
            .map(|function| {
                function
                    .entailment
                    .postconditions
                    .iter()
                    .filter(|proof| proof.summary.is_some())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let component_postconditions = functions
            .iter()
            .map(|function| {
                let component = schedule.function_components[function.id.0 as usize];
                let hypotheses = function
                    .postconditions
                    .iter()
                    .enumerate()
                    .filter(|_| component == caller_component)
                    .filter_map(|(ordinal, clause)| {
                        Some((
                            clause,
                            VerifiedPostconditionSummary {
                                function: function.id,
                                block: clause.selector.block.clone(),
                                relation_ordinal: u32::try_from(ordinal).ok()?,
                                component,
                            },
                        ))
                    })
                    .collect();
                ComponentPostconditions {
                    component,
                    hypotheses,
                }
            })
            .collect::<Vec<_>>();
        let const_parameter_types = self.const_generic_types().collect();
        let copy_type_parameters = self.copy_type_parameters();
        let context = EntailmentContext {
            declarations: self.declarations.resolved.declarations(),
            callees: inputs.callees,
            constants: &self.checked_constants,
            constant_ids: &self.constants,
            const_parameter_types: &const_parameter_types,
            copy_type_parameters: &copy_type_parameters,
            nominals: &self.nominals,
            elements: &self.elements,
            contract_queries: inputs.contract_queries,
            verified_postconditions: &verified_postconditions,
            verified_postcondition_proofs: &verified_postcondition_proofs,
            component_postconditions: &component_postconditions,
            binding_names: inputs.binding_names,
        };
        for occurrence in schedule
            .calls
            .iter()
            .filter(|call| call.caller == caller.id)
        {
            let Some(callee) = functions.get(occurrence.callee.0 as usize) else {
                continue;
            };
            let component = schedule.function_components[callee.id.0 as usize];
            if component == caller_component {
                continue;
            }
            for (ordinal, (postcondition, proof)) in callee
                .postconditions
                .iter()
                .zip(&callee.entailment.postconditions)
                .enumerate()
            {
                if proof.summary.is_some()
                    || postcondition.selector.generated
                    || postcondition.selector.is_routed()
                {
                    continue;
                }
                let Some(exit) = proof
                    .exits
                    .iter()
                    .find(|exit| exit.disposition != PostconditionDisposition::Discharged)
                else {
                    continue;
                };
                let identity = VerifiedPostconditionSummary {
                    function: callee.id,
                    block: postcondition.selector.block.clone(),
                    relation_ordinal: u32::try_from(ordinal)
                        .map_err(|_| SemanticCompilerFailure::InvalidResolution)?,
                    component,
                };
                if !unavailable_summary_discharges_call(
                    caller, &context, occurrence, postcondition, &identity, target,
                ) {
                    continue;
                }
                let node = self
                    .declarations
                    .tree
                    .node_with_path(&proof.block)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let summary = self.declarations.tree.source_spelling(node)?;
                let signature = self
                    .signatures
                    .get(callee.id.0 as usize)
                    .filter(|signature| signature.declaration == callee.declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let instance = self.render_function_instance(signature)?;
                let (_, call_line) = self.declarations.tree.source_line(&occurrence.node_path)?;
                let (_, exit_line) = self.declarations.tree.source_line(&exit.statement)?;
                return Ok(Some(repairs::unavailable_call_summary(
                    &instance, &summary, call_line, &exit.residual, exit_line,
                )));
            }
        }
        Ok(None)
    }

    fn unavailable_return_summary(
        &self,
        caller: &CheckedFunction,
        statement: &crate::NodePath,
        functions: &[&CheckedFunction],
    ) -> Result<Option<String>, CheckStop> {
        if caller.entailment.postconditions.iter().any(|proof| {
            proof
                .exits
                .iter()
                .any(|exit| &exit.statement == statement && exit.lost_header.is_some())
        }) {
            return Ok(None);
        }
        let Some(id) = caller
            .body
            .as_deref()
            .and_then(|body| returned_call(body, statement))
        else {
            return Ok(None);
        };
        let Some(callee) = functions.iter().find(|function| function.id == id) else {
            return Ok(None);
        };
        let Some(unavailable) = callee.entailment.postconditions.iter().find(|proof| {
            proof.summary.is_none()
                && callee.postconditions.iter().any(|postcondition| {
                    postcondition.selector.block == proof.block
                        && postcondition.selector.selector == proof.selector
                        && !postcondition.selector.generated
                        && !postcondition.selector.is_routed()
                        && postcondition
                            .relation
                            .operands
                            .iter()
                            .any(|operand| operand.contains_result())
                })
        }) else {
            return Ok(None);
        };
        let failed_ensure = callee.entailment.postconditions.iter().find_map(|proof| {
            proof
                .exits
                .iter()
                .find(|exit| exit.disposition != PostconditionDisposition::Discharged)
        });
        let (rule, failed, site) = if let Some(exit) = failed_ensure {
            ("FN-9", exit.residual.clone(), &exit.statement)
        } else if let Some(invariant) = callee
            .entailment
            .loop_invariants
            .iter()
            .find(|invariant| !invariant.proof.discharged())
        {
            let relation = if invariant.proof.base {
                &invariant.backedge_target
            } else {
                &invariant.base_target
            };
            ("INV-1", relation.clone(), &invariant.node_path)
        } else {
            return Ok(None);
        };
        let clause = self
            .declarations
            .tree
            .node_with_path(&unavailable.block)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let summary = self.declarations.tree.source_spelling(clause)?;
        let signature = self
            .signatures
            .get(callee.id.0 as usize)
            .filter(|signature| signature.declaration == callee.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let instance = self.render_function_instance(signature)?;
        let (_, line) = self.declarations.tree.source_line(site)?;
        Ok(Some(repairs::unavailable_return_summary(
            &instance, &summary, rule, &failed, line,
        )))
    }
}

/// A direct return, or its immediately preceding direct-call binding, has
/// no intervening write which could replace the result. Other dataflow keeps
/// the ordinary repair rather than guessing that an old call still matters.
fn returned_call(statements: &[CheckedStatement], path: &crate::NodePath) -> Option<FunctionId> {
    statements
        .iter()
        .enumerate()
        .find_map(|(index, statement)| match statement {
            CheckedStatement::Return {
                node_path, value, ..
            } if node_path == path => match value {
                CheckedExpression::UserCall {
                    function,
                    formal_contract: None,
                    ..
                } => Some(*function),
                CheckedExpression::Binding { binding, .. } => {
                    match index.checked_sub(1).and_then(|index| statements.get(index)) {
                        Some(CheckedStatement::Let {
                            binding: initialized,
                            value: CheckedExpression::UserCall {
                                function,
                                formal_contract: None,
                                ..
                            },
                            ..
                        }) if initialized == binding => Some(*function),
                        _ => None,
                    }
                }
                _ => None,
            },
            CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
                arms.iter().find_map(|arm| returned_call(&arm.body, path))
            }
            CheckedStatement::Loop { body, .. }
            | CheckedStatement::CountedRange { body, .. }
            | CheckedStatement::Atomic { body, .. } => returned_call(body, path),
            _ => None,
        })
}
