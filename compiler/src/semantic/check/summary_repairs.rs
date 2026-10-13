//! Explanation of unavailable callee summaries, read only after rejection.

use super::{CheckStop, TypeContext, repairs};
use crate::semantic::entailment::PostconditionDisposition;
use crate::semantic::model::{CheckedExpression, CheckedFunction, CheckedStatement, FunctionId};
use crate::{SemanticCompilerFailure, SemanticIssueKind, SemanticLocation};

impl TypeContext<'_> {
    /// The selected rejection and every proof result stay unchanged. This
    /// inspection borrows the completed inventory only to explain its missing
    /// summaries; no retained analysis or proof receipt contains this text.
    pub(super) fn explain_unavailable_summary(
        &self,
        mut stop: CheckStop,
        caller: &CheckedFunction,
        functions: &[&CheckedFunction],
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
        stop
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
