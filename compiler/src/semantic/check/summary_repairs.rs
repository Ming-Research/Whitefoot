//! Explanation of unavailable callee summaries, read only after rejection.

use super::{CheckStop, Checker, TypeContext, repairs};
use crate::semantic::entailment::{
    ComponentPostconditions, EntailmentCallee, EntailmentContext, VerifiedPostconditionSummary,
    postcondition_schedule, unavailable_summary_discharges_record,
};
use crate::semantic::model::{CheckedContractQuery, CheckedFunction};
use crate::{SemanticCompilerFailure, SemanticIssue, SemanticIssueKind, SemanticLocation};

pub(super) struct SummaryRepairInputs<'a> {
    pub(super) callees: &'a [EntailmentCallee],
    pub(super) contract_queries: &'a [CheckedContractQuery],
    pub(super) binding_names: &'a [String],
    /// Whether the original analysis of the caller judged its source
    /// conditions [OP-5]; the replay judges the same ones.
    pub(super) judge_conditions: bool,
}

impl Checker<'_, '_> {
    /// Explains a rejection whose proof needed a withheld callee summary.
    pub(super) fn explain_summary_stop(
        &self,
        stop: CheckStop,
        caller: &CheckedFunction,
        functions: &[&CheckedFunction],
        callees: &[EntailmentCallee],
        binding_names: &[String],
    ) -> CheckStop {
        let inputs = SummaryRepairInputs {
            callees,
            contract_queries: &self.analysis.contract_queries,
            binding_names,
            judge_conditions: !self.types.is_generic_instance(caller),
        };
        self.types
            .explain_unavailable_summary(stop, caller, functions, &inputs)
    }
}

impl TypeContext<'_> {
    /// The existing obligation record selects the counterfactual target.
    /// Neither source shape nor diagnostic family decides causality, and
    /// the original rejection selection and every accepted proof stay intact.
    pub(super) fn explain_unavailable_summary(
        &self,
        mut stop: CheckStop,
        caller: &CheckedFunction,
        functions: &[&CheckedFunction],
        inputs: &SummaryRepairInputs<'_>,
    ) -> CheckStop {
        if let CheckStop::Issue(issue) = &mut stop
            && let Ok(Some(target)) = self.rejected_record(caller, issue)
            && let Ok(Some(repair)) =
                self.unavailable_summary(caller, target, functions, inputs)
            && let Some(mechanical_fix) = proof_repair(&mut issue.kind)
        {
            *mechanical_fix = repair;
        }
        stop
    }

    /// Recover the already selected record using the ordinary renderer.
    /// This does not select another rejection or change its source order.
    fn rejected_record(
        &self,
        function: &CheckedFunction,
        issue: &SemanticIssue,
    ) -> Result<Option<usize>, CheckStop> {
        for (index, (record, answer)) in function
            .obligations
            .iter()
            .zip(&function.entailment.answers)
            .enumerate()
        {
            if let Some(answer) = *answer
                && !answer.discharged(&function.entailment)
                && self.undischarged_issue(function, record, answer)? == *issue
            {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    fn unavailable_summary(
        &self,
        caller: &CheckedFunction,
        target: usize,
        functions: &[&CheckedFunction],
        inputs: &SummaryRepairInputs<'_>,
    ) -> Result<Option<String>, CheckStop> {
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
        let type_spellings = self.diagnostic_constant_type_spellings();
        let context = EntailmentContext {
            source_spellings: &self.source_spellings,
            postcondition_clauses: &self.postcondition_clauses,
            postcondition_sources: &self.postcondition_sources,
            type_spellings: &type_spellings,
            captured_places: &self.captured_places,
            judge_conditions: inputs.judge_conditions,
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
            let Err(CheckStop::Issue(failure)) = self.entailment_rejection(callee) else {
                continue;
            };
            let SemanticLocation::SourceNode(failed_site, _) = &failure.location;
            let postconditions = callee
                .postconditions
                .iter()
                .zip(&callee.entailment.postconditions)
                .enumerate()
                .filter(|(_, (_, proof))| proof.summary.is_none())
                .map(|(ordinal, (clause, _))| {
                    Ok((
                        clause.clone(),
                        VerifiedPostconditionSummary {
                            function: callee.id,
                            block: clause.selector.block.clone(),
                            relation_ordinal: u32::try_from(ordinal)
                                .map_err(|_| SemanticCompilerFailure::InvalidResolution)?,
                            component,
                        },
                    ))
                })
                .collect::<Result<Vec<_>, CheckStop>>()?;
            if postconditions.is_empty()
                || !unavailable_summary_discharges_record(
                    caller, &context, occurrence, &postconditions, target,
                )
            {
                continue;
            }
            let summary = postconditions
                .iter()
                .map(|(clause, _)| self.declarations.tree.path_spelling(&clause.selector.block))
                .collect::<Result<Vec<_>, _>>()?
                .join(" ");
            let signature = self
                .signatures
                .get(callee.id.0 as usize)
                .filter(|signature| signature.declaration == callee.declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let instance = self.render_function_instance(signature)?;
            let (call_file, call_line) =
                self.declarations.tree.source_line(&occurrence.node_path)?;
            let (failed_file, failed_line) = self.declarations.tree.source_line(failed_site)?;
            let declared_site = match &failure.kind {
                SemanticIssueKind::UndischargedPostcondition(detail) => {
                    let SemanticLocation::SourceNode(path, _) = &detail.postcondition;
                    let (file, line) = self.declarations.tree.source_line(path)?;
                    Some(format!("{file}:{line}"))
                }
                _ => None,
            };
            let failed = match &failure.kind {
                SemanticIssueKind::UndischargedLoopInvariant {
                    required_relation, ..
                } => required_relation.clone(),
                SemanticIssueKind::UndischargedPostcondition(detail) => detail.relation.clone(),
                SemanticIssueKind::UndischargedCallRequirement(detail) => {
                    detail.instantiated_goal.clone()
                }
                SemanticIssueKind::UndischargedBoundsObligation { residual, .. }
                | SemanticIssueKind::UndischargedIntegerDomainObligation { residual, .. }
                | SemanticIssueKind::UndischargedConversionDomainObligation { residual, .. }
                | SemanticIssueKind::UndischargedRangeFormationObligation { residual, .. }
                | SemanticIssueKind::UndischargedCallSeparation { residual, .. } => {
                    residual.clone()
                }
                _ => self.declarations.tree.path_spelling(failed_site)?,
            };
            return Ok(Some(repairs::unavailable_summary(
                &instance,
                &summary,
                &format!("{call_file}:{call_line}"),
                failure.rule_id(),
                &failed,
                declared_site.as_deref(),
                &format!("{failed_file}:{failed_line}"),
            )));
        }
        Ok(None)
    }
}

/// Rendering destinations only: the obligation record and private proof
/// decide whether the new explanation applies.
fn proof_repair(kind: &mut SemanticIssueKind) -> Option<&mut String> {
    match kind {
        SemanticIssueKind::UndischargedCallRequirement(detail) => {
            Some(&mut detail.mechanical_fix)
        }
        SemanticIssueKind::UndischargedPostcondition(detail) => Some(&mut detail.mechanical_fix),
        SemanticIssueKind::UndischargedBoundsObligation { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedIntegerDomainObligation { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedConversionDomainObligation { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedRangeFormationObligation { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedEmptyRunRelease { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedTypeInvariant { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedLoopInvariant { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedLocalInvariant { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedSourceProof { mechanical_fix, .. }
        | SemanticIssueKind::UndischargedCallSeparation { mechanical_fix, .. }
        | SemanticIssueKind::InvalidHeldContract { mechanical_fix, .. }
        | SemanticIssueKind::InvalidReferenceUse { mechanical_fix, .. } => Some(mechanical_fix),
        _ => None,
    }
}
