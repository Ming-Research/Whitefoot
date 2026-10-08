//! OP-5 consumes the same condition origins S1 establishes at branch entry.
//! Queries never establish a fact; their results answer structural records.

use super::*;

impl Judging<'_, '_, '_> {
    pub(super) fn judge_condition(
        &mut self,
        site: &crate::NodePath,
        origins: &ArmFacts,
        state: &ProofFlowState,
    ) {
        let mut outcome = super::super::ConditionOutcome {
            node_path: site.clone(),
            decided: None,
            residual: String::new(),
            facts: Vec::new(),
        };
        if self.input.context.judge_conditions {
            let context = ProofContext::new(&state.facts, &state.affine);
            let closed = context.close(
                &self.vocabulary.terms,
                &self.vocabulary.goals,
                &mut self.vocabulary.derivations,
            );
            if !closed.contradictory() {
                super::super::work::condition();
                // [OP-5] A sign decides the test exactly when every fact the
                // branch it selects would establish at entry [ENT-3.S1] is
                // already derivable: each goal origin under that sign and the
                // comparison origin under it. Deleting the test and the dead
                // branch then loses no fact a later goal could use.
                for truth in [true, false] {
                    if origins.goals.is_empty() && origins.comparison.is_none() {
                        break;
                    }
                    let mut proofs = Vec::new();
                    let mut residual = None;
                    let mut decided = true;
                    for &origin in &origins.goals {
                        let expression = self.vocabulary.goals.expression(origin).clone();
                        if residual.is_none() {
                            residual = Some(self.input.render_concrete_goal(&expression));
                        }
                        let goal = if truth {
                            expression
                        } else {
                            GoalExpression::Operation {
                                row: GoalOperation::Boolean(CheckedBooleanOperation::Not),
                                type_arguments: Vec::new(),
                                const_arguments: Vec::new(),
                                result: CheckedType::Bool,
                                arguments: vec![expression],
                            }
                        };
                        let result = self.condition_query(&goal, context);
                        match (result.disposition, result.derivation) {
                            (ProofDisposition::Proved, Some(proof)) => proofs.push(proof),
                            _ => {
                                decided = false;
                                break;
                            }
                        }
                    }
                    if decided && let Some(relation) = &origins.comparison {
                        if residual.is_none() {
                            residual = Some(self.reasoning().render_relation(relation));
                        }
                        let negated = relation.negated();
                        let relation = if truth { relation } else { &negated };
                        super::super::work::condition_query();
                        let result = self.reasoning().prove(
                            context,
                            ProofGoal::Ordering {
                                relation,
                                affine: None,
                            },
                        );
                        match (result.disposition, result.derivation) {
                            (ProofDisposition::Proved, Some(proof)) => proofs.push(proof),
                            _ => decided = false,
                        }
                    }
                    if decided {
                        outcome.decided = Some(truth);
                        outcome.residual = residual.unwrap_or_default();
                        for proof in proofs {
                            for fact in self.condition_facts(proof) {
                                if !outcome.facts.contains(&fact) {
                                    outcome.facts.push(fact);
                                }
                            }
                        }
                        super::super::work::redundant_condition();
                        break;
                    }
                }
            }
        }
        self.output.conditions.push(outcome);
    }

    fn condition_query(
        &mut self,
        expression: &GoalExpression,
        context: ProofContext<'_>,
    ) -> ProofResult {
        super::super::work::condition_query();
        let affine = self
            .reasoning()
            .affine_goal_ordering_target(expression, context.affine);
        self.reasoning().prove(
            context,
            ProofGoal::Signed {
                expression,
                affine: affine.as_ref(),
            },
        )
    }

    /// Render the selected proof's premises before ledger pruning. Diagnostic
    /// strings grant no facts and contain no IDs from the temporary arena.
    fn condition_facts(&mut self, root: DerivationId) -> Vec<String> {
        let mut facts = Vec::new();
        let mut pending = vec![root];
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let node = self.vocabulary.derivations.nodes[id.0 as usize].clone();
            let text = match &node {
                DerivationNode::SourceBound { relation, .. }
                | DerivationNode::OperationFact { relation, .. } => {
                    Some(self.reasoning().render_relation(relation))
                }
                DerivationNode::ImplicitBound {
                    left,
                    right,
                    bound,
                    kind,
                } => Some(format!(
                    "{kind:?}: {}",
                    self.reasoning().render_relation(&Relation::Bound {
                        left: *left,
                        right: *right,
                        bound: *bound
                    })
                )),
                DerivationNode::SourceDistinct { left, right, .. } => {
                    Some(self.reasoning().render_relation(&Relation::Distinct {
                        left: *left,
                        right: *right,
                        difference: 0,
                    }))
                }
                DerivationNode::SourceGoal { goal, sign, .. }
                | DerivationNode::BooleanLiteral { goal, sign } => Some(format!(
                    "{sign:?}: {}",
                    self.input
                        .render_concrete_goal(self.vocabulary.goals.expression(*goal))
                )),
                DerivationNode::AffineConsequence { premises, .. } => {
                    for premise in premises {
                        let name = match premise.source {
                            SourceAffineFactRef::LoopInvariant(source) => self
                                .output
                                .loop_invariants
                                .iter()
                                .find(|outcome| {
                                    outcome.loop_id == source.loop_id
                                        && outcome.source_ordinal == source.source_ordinal
                                })
                                .map(|outcome| outcome.name.clone()),
                            SourceAffineFactRef::SourceProof { source_ordinal } => self
                                .output
                                .source_proofs
                                .get(source_ordinal as usize)
                                .map(|outcome| outcome.name.clone()),
                            SourceAffineFactRef::JoinedSourceProof { .. } => Some(
                                "invariant relation retained on every incoming edge".to_owned(),
                            ),
                        };
                        if let Some(name) = name {
                            facts.push(format!("invariant {name}"));
                        }
                    }
                    None
                }
                _ => None,
            };
            if let Some(text) = text {
                facts.push(text);
            }
            pending.extend(node.parent_ids().into_iter().rev());
        }
        let mut unique = HashSet::new();
        facts.retain(|fact| unique.insert(fact.clone()));
        facts
    }
}
