//! Acceptance of one analyzed function (`design/compiler/acceptance-records.md`):
//! every obligation record answered and discharged.
//!
//! The rule of an undischarged record was fixed where the checker formed it,
//! so the report is a table from that rule to its issue, filled in from the
//! record and the judgment that answered it. [DIAG-1] admits one rule and one
//! location, selected by the order in which obligations are decided.

use crate::semantic::check::DeclarationInventory;
use crate::semantic::check::TypeContext;
use crate::syntax::NodeId;
use crate::{
    NodePath, Production, SemanticCompilerFailure, SemanticIssue, SemanticIssueKind,
    SemanticLocation, SemanticRule, StaticObligationDisposition,
};

use super::super::entailment::{
    CallGoalDisposition, FunctionEntailment, ObligationFamily, PostconditionDisposition,
    RangeEndpointReading, SourceProofCertificateFailure, TermRead,
};
use super::super::goal::{GoalExpression, GoalOperation};
use super::super::model::{CheckedFunction, CheckedNumericType, FunctionId};
use super::super::obligations::{ObligationRecord, RecordAnswer};
use super::{CheckStop, references, repairs};

/// One position in the causal order in which obligations are decided.
///
/// `Child` is a syntax child ordinal, so a plain node path orders a failure
/// exactly where the walk reaches it. `AfterSubtree` is the position
/// immediately after everything one node encloses: it is greater than every
/// child ordinal under that node and still less than the node's following
/// siblings.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ProofPosition {
    Child(u32),
    AfterSubtree,
}

/// The payload disposition and the repair's disposition of one goal no step
/// discharged [ENT-4, MSR-4].
fn dispositions(refuted: bool) -> (StaticObligationDisposition, repairs::Disposition) {
    if refuted {
        (
            StaticObligationDisposition::Refuted,
            repairs::Disposition::Refuted,
        )
    } else {
        (
            StaticObligationDisposition::Unproved,
            repairs::Disposition::Unproved,
        )
    }
}

impl<'unit> DeclarationInventory<'unit> {
    /// Where one undischarged body record is decided.
    ///
    /// Every judgment but one is decided where it stands. INV-1's backedge
    /// judgment is the exception: it is proved only after the whole loop body
    /// has been walked, and a body failure that demotes a value to a fresh
    /// full-range atom is exactly what breaks it. Positioning the backedge
    /// after the body it consumes therefore reports the cause rather than the
    /// effect, while INV-1's base judgment stays at the header where it is
    /// decided. A local invariant is decided at the written `use` that owns
    /// its failure.
    fn proof_position(
        &self,
        record: &ObligationRecord,
        answer: RecordAnswer,
        entailment: &FunctionEntailment,
    ) -> Result<Vec<ProofPosition>, CheckStop> {
        let path = match answer {
            RecordAnswer::LoopInvariant(index) => {
                let outcome = entailment
                    .loop_invariants
                    .get(index)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if outcome.proof.base && outcome.proof.step == Some(false) {
                    let node = self
                        .tree
                        .node_with_path(&outcome.node_path)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    if let Some(loop_node) = self.enclosing_loop_node(node)? {
                        let mut components = self
                            .tree
                            .path(loop_node)?
                            .components()
                            .iter()
                            .copied()
                            .map(ProofPosition::Child)
                            .collect::<Vec<_>>();
                        components.push(ProofPosition::AfterSubtree);
                        return Ok(components);
                    }
                }
                &outcome.node_path
            }
            RecordAnswer::SourceProof(index) => entailment
                .source_proofs
                .get(index)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .rejection_node_path(),
            RecordAnswer::Obligation(_)
            | RecordAnswer::CallGoal(_)
            | RecordAnswer::Postcondition(_)
            | RecordAnswer::Uninhabited => &record.site,
        };
        Ok(path
            .components()
            .iter()
            .copied()
            .map(ProofPosition::Child)
            .collect())
    }
    /// The enclosing `loop_stmt` or `for_stmt` of one loop-header invariant.
    ///
    /// A header invariant is always written inside its loop statement, so the
    /// ancestor exists for every well-formed tree. The absent case keeps the
    /// caller total and simply leaves the invariant at its own position.
    fn enclosing_loop_node(&self, node: NodeId) -> Result<Option<NodeId>, SemanticCompilerFailure> {
        let mut current = node;
        loop {
            let production = self.tree.production(current)?;
            if production == Production::LoopStmt || production == Production::ForStmt {
                return Ok(Some(current));
            }
            match self.tree.parent(current)? {
                Some(parent) => current = parent,
                None => return Ok(None),
            }
        }
    }
    fn source_location(&self, path: &NodePath) -> Result<SemanticLocation, CheckStop> {
        let node = self
            .tree
            .node_with_path(path)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(SemanticLocation::SourceNode(
            path.clone(),
            self.tree.coordinate(node)?,
        ))
    }
    fn undischarged_loop_invariant(
        &self,
        entailment: &FunctionEntailment,
        index: usize,
    ) -> Result<SemanticIssue, CheckStop> {
        let outcome = entailment
            .loop_invariants
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let obligation = if !outcome.proof.base {
            crate::LoopInvariantProofObligation::Base
        } else if outcome.proof.step == Some(false) {
            crate::LoopInvariantProofObligation::Backedge
        } else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let refuted = match obligation {
            crate::LoopInvariantProofObligation::Base => outcome.proof.base_refuted,
            crate::LoopInvariantProofObligation::Backedge => outcome.proof.step_refuted,
        };
        let (disposition, repair) = dispositions(refuted);
        let (mechanical_fix, required_relation) = match obligation {
            crate::LoopInvariantProofObligation::Base => (
                repairs::loop_invariant_base(repair, &outcome.name),
                outcome.base_target.clone(),
            ),
            crate::LoopInvariantProofObligation::Backedge => (
                repairs::loop_invariant_backedge(repair, &outcome.name),
                outcome.backedge_target.clone(),
            ),
        };
        Ok(SemanticIssue {
            rule: SemanticRule::Inv1,
            location: self.source_location(&outcome.node_path)?,
            kind: SemanticIssueKind::UndischargedLoopInvariant {
                name: outcome.name.clone(),
                obligation,
                required_relation,
                disposition,
                mechanical_fix,
            },
            request: None,
        })
    }
    fn undischarged_source_proof(
        &self,
        entailment: &FunctionEntailment,
        index: usize,
    ) -> Result<SemanticIssue, CheckStop> {
        let outcome = entailment
            .source_proofs
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let location = self.source_location(outcome.rejection_node_path())?;
        if let Some(failure) = outcome.check.target_failure {
            let (reason, mechanical_fix) = match failure {
                SourceProofCertificateFailure::ArithmeticOverflow => (
                    "the invariant target exceeds the i128 proof domain after current value images are substituted",
                    "split or rescale the invariant so its normalized current-value coefficients and constant fit i128",
                ),
                SourceProofCertificateFailure::FormationCapacity => (
                    "the invariant target exceeds a fixed affine formation capacity after current value images are substituted",
                    "split the invariant into smaller local invariants whose normalized current-value shapes fit the fixed capacities",
                ),
                SourceProofCertificateFailure::RepeatedUse { .. }
                | SourceProofCertificateFailure::UseCapacity { .. }
                | SourceProofCertificateFailure::NonlinearResidual
                | SourceProofCertificateFailure::InvalidFactor { .. } => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
            };
            return Ok(SemanticIssue {
                rule: SemanticRule::Inv1,
                location,
                kind: SemanticIssueKind::InvalidInvariant {
                    reason,
                    mechanical_fix,
                },
                request: None,
            });
        }
        if !outcome.certificate_written {
            if !outcome.check.premises.is_empty()
                || outcome.check.source_failure.is_some()
                || outcome.check.certificate_failure.is_some()
                || outcome.check.residual_failure.is_some()
                || outcome.check.redundant
                || outcome.check.combination
            {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            let (disposition, repair) = dispositions(outcome.check.target_refuted);
            return Ok(SemanticIssue {
                rule: SemanticRule::Inv1,
                location,
                kind: SemanticIssueKind::UndischargedLocalInvariant {
                    name: outcome.name.clone(),
                    disposition,
                    mechanical_fix: repairs::local_invariant(repair, &outcome.name),
                },
                request: None,
            });
        }
        let failure_obligation = |failure| match failure {
            SourceProofCertificateFailure::RepeatedUse { first, repeated } => {
                crate::SourceProofObligation::RepeatedUse { first, repeated }
            }
            SourceProofCertificateFailure::UseCapacity { maximum, actual } => {
                crate::SourceProofObligation::UseCapacity { maximum, actual }
            }
            SourceProofCertificateFailure::ArithmeticOverflow => {
                crate::SourceProofObligation::CertificateArithmeticOverflow
            }
            SourceProofCertificateFailure::FormationCapacity => {
                crate::SourceProofObligation::CertificateFormationCapacity
            }
            SourceProofCertificateFailure::InvalidFactor { use_index } => {
                crate::SourceProofObligation::InvalidUseFactor { use_index }
            }
            SourceProofCertificateFailure::NonlinearResidual => {
                crate::SourceProofObligation::NonlinearCertificateSum
            }
        };
        let obligation = if let Some(failure) = outcome.check.source_failure {
            failure_obligation(failure)
        } else if outcome.check.redundant {
            crate::SourceProofObligation::RedundantUseBlock
        } else if let Some(failure) = outcome.check.certificate_failure {
            failure_obligation(failure)
        } else if let Some(index) = outcome.check.first_unproved_premise {
            crate::SourceProofObligation::Premise(index)
        } else if let Some(failure) = outcome.check.residual_failure {
            failure_obligation(failure)
        } else if !outcome.check.combination {
            crate::SourceProofObligation::Combination
        } else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let mechanical_fix = match obligation {
            crate::SourceProofObligation::Premise(_) => {
                "establish this use relation from facts already available before the invariant statement, or replace it with a relation AUTO can prove in that same entering context"
            }
            crate::SourceProofObligation::Combination => {
                "rewrite the invariant target, use relations, or explicit positive factors so their source-order weighted sum leaves a residual proved by the fixed direct L0 or interval rule"
            }
            crate::SourceProofObligation::RedundantUseBlock => {
                "remove the use block; AUTO already proves this invariant target from the same entering context in this specification version"
            }
            crate::SourceProofObligation::RepeatedUse { .. } => {
                "replace repeated normalized use relations with one use carrying their combined explicit positive factor"
            }
            crate::SourceProofObligation::UseCapacity { .. } => {
                "split this local certificate into named intermediate invariants so every written use list is within the fixed structural capacity"
            }
            crate::SourceProofObligation::CertificateArithmeticOverflow => {
                "split or rescale this certificate so every source-order proof-domain coefficient and constant operation fits i128"
            }
            crate::SourceProofObligation::CertificateFormationCapacity => {
                "split this certificate into smaller named intermediate invariants whose canonical affine shapes fit the fixed formation capacities"
            }
            crate::SourceProofObligation::InvalidUseFactor { .. } => {
                "write a canonical positive bare-decimal factor, or omit the factor when it is one"
            }
            crate::SourceProofObligation::NonlinearCertificateSum => {
                "the multiplied operand must be one the checker holds as a single value — a parameter or a call result — because a locally derived one is expanded into its own operands and no admitted product then matches the sum; take it as a parameter, or scale the premise by a bare decimal instead"
            }
        };
        Ok(SemanticIssue {
            rule: SemanticRule::Prf1,
            location,
            kind: SemanticIssueKind::UndischargedSourceProof {
                name: outcome.name.clone(),
                obligation,
                mechanical_fix,
            },
            request: None,
        })
    }
    /// Whether a node lies in a `requires_clause` or a `contract_define`,
    /// whose places a requirement forms at body entry and which evaluate
    /// nothing [FN-8].
    fn in_requirement(&self, path: &NodePath) -> Result<bool, CheckStop> {
        let mut node = self.tree.node_with_path(path);
        while let Some(current) = node {
            if matches!(
                self.tree.production(current)?,
                Production::RequiresClause | Production::ContractDefine
            ) {
                return Ok(true);
            }
            node = self.tree.parent(current)?;
        }
        Ok(false)
    }
}

impl<'unit> TypeContext<'unit> {
    /// Accepts `function` exactly when every record is answered and
    /// discharged, and otherwise reports the undischarged record decided
    /// first.
    ///
    /// Body obligations are ordered by where they are decided and then by
    /// their rule's rank; the relations of FN-9, decided at the exits, follow
    /// in source order. A record left unanswered while another is answered
    /// undischarged is that failure's consequence: the engine stops judging
    /// what a failed obligation guards. Unanswered records alone, or a
    /// judgment answering none, are the checker and the engine disagreeing,
    /// and the function is not accepted.
    pub(super) fn entailment_rejection(&self, function: &CheckedFunction) -> Result<(), CheckStop> {
        let entailment = &function.entailment;
        if entailment.answers.len() != function.obligations.len()
            || !entailment.unrecorded.is_empty()
        {
            return Err(SemanticCompilerFailure::ObligationContract.into());
        }
        let mut body = Vec::new();
        let mut relations = Vec::new();
        for (record, answer) in function.obligations.iter().zip(&entailment.answers) {
            let Some(answer) = *answer else {
                continue;
            };
            if answer.discharged(entailment) {
                continue;
            }
            if record.rule == SemanticRule::Fn9 {
                relations.push((record, answer));
            } else {
                body.push((
                    self.declarations
                        .proof_position(record, answer, entailment)?,
                    record,
                    answer,
                ));
            }
        }
        // `min_by` keeps the first of several equal minima, so the selection
        // depends only on this order and on record order, never on a hash.
        let selected = body
            .into_iter()
            .min_by(|left, right| {
                left.0.cmp(&right.0).then_with(|| {
                    left.1
                        .rule
                        .definition_rank()
                        .cmp(&right.1.rule.definition_rank())
                })
            })
            .map(|(_, record, answer)| (record, answer))
            .or_else(|| relations.into_iter().next());
        if let Some((record, answer)) = selected {
            return Err(CheckStop::source_issue(
                self.undischarged_issue(function, record, answer)?,
            ));
        }
        if entailment.answers.iter().any(Option::is_none) {
            return Err(SemanticCompilerFailure::ObligationContract.into());
        }
        Ok(())
    }
    /// The issue one undischarged record reports: its rule selects the kind,
    /// and the record with the judgment that answered it fill it in.
    fn undischarged_issue(
        &self,
        function: &CheckedFunction,
        record: &ObligationRecord,
        answer: RecordAnswer,
    ) -> Result<SemanticIssue, CheckStop> {
        match (record.rule, answer) {
            (SemanticRule::Inv1, RecordAnswer::LoopInvariant(index)) => self
                .declarations
                .undischarged_loop_invariant(&function.entailment, index),
            (SemanticRule::Inv1 | SemanticRule::Prf1, RecordAnswer::SourceProof(index)) => self
                .declarations
                .undischarged_source_proof(&function.entailment, index),
            (
                SemanticRule::Op4
                | SemanticRule::Op2
                | SemanticRule::Op6
                | SemanticRule::Eff5
                | SemanticRule::Op11
                | SemanticRule::Ref2
                | SemanticRule::Ref4,
                RecordAnswer::Obligation(index),
            ) => self.undischarged_obligation(function, record.rule, index),
            (
                SemanticRule::Fn8 | SemanticRule::Op14 | SemanticRule::Type11,
                RecordAnswer::CallGoal(index),
            ) => self.undischarged_call_requirement(function, record.rule, index),
            (SemanticRule::Fn9, RecordAnswer::Postcondition(index)) => {
                self.undischarged_postcondition(function, index)
            }
            _ => Err(SemanticCompilerFailure::InvalidResolution.into()),
        }
    }
    /// [DIAG-1] the calls among the writes that removed a failed bound's
    /// facts, each with its callee, line and written place; none when any of
    /// the writes is not a call, whose length change no row can deny.
    fn killing_calls(
        &self,
        notes: &[crate::semantic::entailment::MeasureKillNote],
    ) -> Result<Vec<repairs::KillingCall>, CheckStop> {
        let mut calls = Vec::new();
        for note in notes {
            let node = self
                .declarations
                .tree
                .node_with_path(&note.source)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            if self.declarations.tree.production(node)? != Production::Call {
                return Ok(Vec::new());
            }
            let spelling = self.declarations.tree.source_spelling(node)?;
            let callee = spelling
                .split('(')
                .next()
                .unwrap_or_default()
                .split("::<")
                .next()
                .unwrap_or_default()
                .to_owned();
            let (_, line) = self.declarations.tree.source_line(&note.source)?;
            calls.push(repairs::KillingCall {
                callee,
                line,
                written: note.written.clone(),
            });
        }
        Ok(calls)
    }

    /// [ENT-6] one undischarged source obligation, reported under its
    /// record's rule with the repair what its goal reads selects.
    fn undischarged_obligation(
        &self,
        function: &CheckedFunction,
        rule: SemanticRule,
        index: usize,
    ) -> Result<SemanticIssue, CheckStop> {
        let outcome = function
            .entailment
            .obligations
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let residual = outcome
            .residual
            .clone()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let location = self.declarations.source_location(&outcome.node_path)?;
        let (disposition, repair) = dispositions(outcome.refuted);
        // [DIAG-1] what the goal reads selects its routes: a canonical goal
        // names its data, a bounds relation its terms. A bounds residual is
        // written from the source atoms it relates, so it is itself a
        // condition.
        let editable = self.editable_functions()?;
        let reads = match &outcome.canonical_goal {
            Some(goal) => {
                repairs::GoalTerms::of_goal(goal, function, &outcome.written_before, &editable)
            }
            None => repairs::GoalTerms::of_terms(
                &function.entailment.obligation_term_reads(outcome),
                function,
                &outcome.written_before,
                &editable,
            ),
        };
        let condition = match outcome.family {
            ObligationFamily::Bounds => true,
            _ => outcome
                .canonical_goal
                .as_ref()
                .is_some_and(repairs::is_source_relation),
        };
        let case = repairs::GoalCase {
            disposition: repair,
            terms: reads.terms,
            referenced: reads.referenced,
            called: reads.called,
            text: &residual,
            condition,
            function: &function.name,
        };
        let kind = match (rule, outcome.family) {
            (SemanticRule::Op4, ObligationFamily::Bounds) => {
                // A subscript a contract clause forms is established by an
                // earlier requirement and skipped by no guard [FN-8].
                let constant_offset = matches!(
                    function.entailment.obligation_term_reads(outcome).first(),
                    Some(TermRead::Constant)
                );
                let calls = self.killing_calls(&outcome.killed_by)?;
                let mechanical_fix = if self.declarations.in_requirement(&outcome.node_path)? {
                    repairs::clause_bounds(&case, constant_offset)
                } else if let (Some(note), false) = (outcome.killed_by.first(), calls.is_empty()) {
                    repairs::bounds_after_kill(&case, &note.measure, &calls)
                } else {
                    repairs::bounds(&case, constant_offset)
                };
                SemanticIssueKind::UndischargedBoundsObligation {
                    mechanical_fix,
                    residual,
                    disposition,
                }
            }
            (SemanticRule::Op2, ObligationFamily::IntegerDomain) => {
                SemanticIssueKind::UndischargedIntegerDomainObligation {
                    mechanical_fix: repairs::integer_domain(
                        &case,
                        outcome
                            .canonical_goal
                            .as_ref()
                            .and_then(repairs::total_forms),
                    ),
                    residual,
                    disposition,
                }
            }
            (SemanticRule::Op6, ObligationFamily::ConversionDomain) => {
                let Some(GoalExpression::Operation {
                    row:
                        GoalOperation::NumericConversion {
                            source,
                            destination,
                            ..
                        },
                    ..
                }) = &outcome.canonical_goal
                else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let source_name = self.checked_type_name(source.checked_type())?;
                let destination_name = self.checked_type_name(destination.checked_type())?;
                let checked = format!("cvt.checked::<{source_name}, {destination_name}>");
                // An affine invariant bounds an integer operand only.
                let integer_source = matches!(
                    source,
                    CheckedNumericType::Integer(_) | CheckedNumericType::GenericInteger(_)
                );
                SemanticIssueKind::UndischargedConversionDomainObligation {
                    mechanical_fix: repairs::conversion_domain(
                        &case,
                        &checked,
                        &destination_name,
                        integer_source,
                    ),
                    residual,
                    disposition,
                }
            }
            (SemanticRule::Eff5, ObligationFamily::CallSeparation(query)) => {
                let separation = function
                    .call_separations
                    .get(query as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let mechanical_fix = repairs::call_separation(
                    separation.positions.first().copied(),
                    separation.one_argument,
                );
                SemanticIssueKind::UndischargedCallSeparation {
                    residual,
                    mechanical_fix,
                }
            }
            (SemanticRule::Op11, ObligationFamily::ExchangeSeparation(_)) => {
                SemanticIssueKind::UndischargedCallSeparation {
                    residual,
                    mechanical_fix: "prove before the `swap` that the two positions are distinct, or exchange places that are equal or disjoint without an ancestor relation",
                }
            }
            (SemanticRule::Ref2, ObligationFamily::ReferencePreservation(query)) => {
                let use_site = function
                    .call_separations
                    .get(query as usize)
                    .and_then(|query| query.reference_use.as_ref())
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                SemanticIssueKind::InvalidReferenceUse {
                    binder: use_site.binder.clone(),
                    event: use_site.event,
                    mechanical_fix: references::REF2_FORM_AGAIN,
                }
            }
            (SemanticRule::Ref4, ObligationFamily::RangeFormation) => {
                SemanticIssueKind::UndischargedRangeFormationObligation {
                    mechanical_fix: repairs::range_formation(&case),
                    residual,
                    disposition,
                }
            }
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        Ok(SemanticIssue {
            rule,
            location,
            kind,
            request: None,
        })
    }
    fn undischarged_call_requirement(
        &self,
        function: &CheckedFunction,
        rule: SemanticRule,
        index: usize,
    ) -> Result<SemanticIssue, CheckStop> {
        let outcome = function
            .entailment
            .call_goals
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let location = self.declarations.source_location(&outcome.node_path)?;
        let disposition = match outcome.disposition {
            CallGoalDisposition::Discharged => {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            CallGoalDisposition::Refuted => crate::CallRequirementDisposition::Refuted,
            CallGoalDisposition::Unproved => crate::CallRequirementDisposition::Unproved,
        };
        let (static_disposition, repair) =
            dispositions(outcome.disposition == CallGoalDisposition::Refuted);
        let reads = repairs::GoalTerms::of_goal(
            &outcome.goal.root,
            function,
            &outcome.written_before,
            &self.editable_functions()?,
        );
        let case = repairs::GoalCase {
            disposition: repair,
            terms: reads.terms,
            referenced: reads.referenced,
            called: reads.called,
            text: &outcome.rendered_goal,
            condition: repairs::is_source_relation(&outcome.goal.root),
            function: &function.name,
        };
        // [OP-14] `free_empty` has its own site: "An undischarged obligation
        // is a hard error citing OP-14 at the complete `call`, rendering the
        // residual". The record carries the rule the checker's operand-row
        // table selected for the callee.
        if rule == SemanticRule::Op14 {
            return Ok(SemanticIssue {
                rule: SemanticRule::Op14,
                location,
                kind: SemanticIssueKind::UndischargedEmptyRunRelease {
                    mechanical_fix: repairs::empty_run_release(&case),
                    residual: outcome.rendered_goal.clone(),
                    disposition: static_disposition,
                },
                request: None,
            });
        }
        let requires_clause = self.declarations.node_location(&outcome.requires_clause)?;
        // [TYPE-11] a construction's type invariant is judged as a
        // requirement the constructing body owes itself, and reported as
        // that invariant.
        if rule == SemanticRule::Type11 {
            return Ok(SemanticIssue {
                rule,
                location,
                kind: SemanticIssueKind::UndischargedTypeInvariant {
                    type_invariant: requires_clause,
                    instantiated_goal: outcome.rendered_goal.clone(),
                    disposition,
                    // A construction is a `call`; every other site is an
                    // edge leaving an atomic block.
                    mechanical_fix: if self
                        .declarations
                        .tree
                        .node_with_path(&outcome.node_path)
                        .map(|node| self.declarations.tree.production(node))
                        .transpose()?
                        == Some(Production::Call)
                    {
                        repairs::construction_invariant(&case)
                    } else {
                        repairs::atomic_exit_invariant(&case)
                    },
                },
                request: None,
            });
        }
        let signature = self
            .signatures
            .get(outcome.callee.0 as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let ranges = outcome
            .range_lengths
            .iter()
            .map(|reading| {
                let start = self.range_endpoint_spelling(&reading.start)?;
                let end = self.range_endpoint_spelling(&reading.end)?;
                Ok(repairs::RangeLength {
                    range: format!("{}[{}..{}]", reading.base, start.0, end.0),
                    start,
                    end,
                    difference_goal: reading.difference_goal.clone(),
                })
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        let mechanical_fix = repairs::call_requirement(
            &case,
            &ranges,
            repairs::range_goal_shape(&outcome.goal.root),
        );
        Ok(SemanticIssue {
            rule: SemanticRule::Fn8,
            location,
            kind: SemanticIssueKind::UndischargedCallRequirement(Box::new(
                crate::UndischargedCallRequirementDetail {
                    concrete_callee: self.render_function_instance(signature)?,
                    requires_clause,
                    instantiated_goal: outcome.rendered_goal.clone(),
                    disposition,
                    mechanical_fix,
                },
            )),
            request: None,
        })
    }
    /// [REF-4, DIAG-1] one range endpoint as a repair spells it, and whether
    /// a relation can name it. An endpoint no relation names is spelled from
    /// the source occurrence that evaluated it, a field or element read that
    /// the writer copies into a binding.
    fn range_endpoint_spelling(
        &self,
        endpoint: &RangeEndpointReading,
    ) -> Result<(String, bool), CheckStop> {
        match endpoint {
            RangeEndpointReading::Spelled(spelling) => Ok((spelling.clone(), true)),
            RangeEndpointReading::Unspelled(Some(occurrence)) => {
                let node = usize::try_from(*occurrence)
                    .ok()
                    .and_then(NodeId::from_index)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                Ok((self.declarations.tree.source_spelling(node)?, false))
            }
            RangeEndpointReading::Unspelled(None) => Ok(("?".to_owned(), false)),
        }
    }
    /// [FN-9] the first failed exit of one relation, or its missing exit.
    fn undischarged_postcondition(
        &self,
        function: &CheckedFunction,
        index: usize,
    ) -> Result<SemanticIssue, CheckStop> {
        let proof = function
            .entailment
            .postconditions
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let Some(exit) = proof
            .exits
            .iter()
            .find(|exit| exit.disposition != PostconditionDisposition::Discharged)
        else {
            if !proof.exits.is_empty() {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            return Ok(SemanticIssue {
                rule: SemanticRule::Fn9,
                location: self.declarations.source_location(&proof.selector)?,
                kind: SemanticIssueKind::NoSelectedNormalExit {
                    residual: "no selected normal exit",
                    mechanical_fix: repairs::NO_SELECTED_EXIT,
                },
                request: None,
            });
        };
        let (disposition, repair) = match exit.disposition {
            PostconditionDisposition::Discharged => {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            PostconditionDisposition::Refuted => (
                crate::PostconditionProofDisposition::Refuted,
                repairs::Disposition::Refuted,
            ),
            PostconditionDisposition::Unproved => (
                crate::PostconditionProofDisposition::Unproved,
                repairs::Disposition::Unproved,
            ),
        };
        Ok(SemanticIssue {
            rule: SemanticRule::Fn9,
            location: self.declarations.source_location(&exit.statement)?,
            kind: SemanticIssueKind::UndischargedPostcondition(Box::new(
                crate::UndischargedPostconditionDetail {
                    concrete_function: match self
                        .signatures
                        .get(function.id.0 as usize)
                        .filter(|signature| signature.declaration == function.declaration)
                    {
                        Some(signature) => self.render_function_instance(signature)?,
                        None => function.name.clone(),
                    },
                    postcondition: self.declarations.node_location(&proof.block)?,
                    conjunct: proof.relation_ordinal,
                    selector: self.declarations.node_location(&proof.selector)?,
                    relation: exit.residual.clone(),
                    disposition,
                    mechanical_fix: if self
                        .declarations
                        .tree
                        .node_with_path(&exit.statement)
                        .map(|node| self.declarations.tree.production(node))
                        .transpose()?
                        == Some(Production::ReturnStmt)
                    {
                        repairs::postcondition(
                            repair,
                            repairs::returns_call_result(
                                function,
                                &exit.statement,
                                &self.editable_functions()?,
                            ),
                        )
                    } else {
                        repairs::propagated_postcondition(repair)
                    },
                },
            )),
            request: None,
        })
    }
    /// [FN-9] the functions whose `ensures` a writer can add to: every one
    /// but the prelude's.
    fn editable_functions(&self) -> Result<std::collections::HashSet<FunctionId>, CheckStop> {
        let mut editable = std::collections::HashSet::new();
        for signature in &self.signatures {
            if !self.declarations.tree.is_prelude_node(signature.node)? {
                editable.insert(signature.id);
            }
        }
        Ok(editable)
    }
}
