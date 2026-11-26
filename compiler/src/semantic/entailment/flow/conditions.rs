//! [OP-5] source redundancy of an `if` or `value_if` condition: the judgment
//! reads the same condition origins [ENT-3.S1] establishes at branch entry,
//! in the entering state, and publishes nothing. Its outcome answers the
//! condition's record; a decided condition retains rendered text only.

use super::*;

impl Judging<'_, '_, '_> {
    /// Judges one condition after its own evaluation and before either
    /// branch's entry facts. A truth decides the test exactly when every fact
    /// the branch it selects would establish at entry is already derivable:
    /// each goal origin under that sign through the Signed Goal disposition
    /// [ENT-4, ENT-6, MSR-4], and the comparison origin, when present, under
    /// that sign in the closed L0 state. Deleting the test and its dead branch
    /// then removes no fact a later goal could use. True is tried first; a
    /// contradictory entering state and an undecided condition are admitted.
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
        if self.input.context.judge_conditions
            && (!origins.goals.is_empty() || origins.comparison.is_some())
        {
            let context = ProofContext::new(&state.facts, &state.affine);
            let closed = context.close(
                &self.vocabulary.terms,
                &self.vocabulary.goals,
                &mut self.vocabulary.derivations,
            );
            if !closed.contradictory() {
                super::super::work::condition();
                let mut views = HashMap::new();
                for truth in [true, false] {
                    if let Some((residual, proofs)) =
                        self.decided_condition(origins, context, truth, &mut views)
                    {
                        outcome.decided = Some(truth);
                        outcome.residual = residual;
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

    /// The rendered condition and the proofs of every origin under `truth`,
    /// or `None` when one origin is not derivable under it.
    fn decided_condition(
        &mut self,
        origins: &ArmFacts,
        context: ProofContext<'_>,
        truth: bool,
        views: &mut HashMap<GoalId, FactState>,
    ) -> Option<(String, Vec<DerivationId>)> {
        let mut proofs = Vec::new();
        let mut residual = None;
        for &origin in &origins.goals {
            let expression = self.vocabulary.goals.expression(origin).clone();
            if residual.is_none() {
                residual = Some(self.input.render_concrete_goal(&expression));
            }
            super::super::work::condition_query();
            let result = self
                .reasoning()
                .prove_condition_origin(context, origin, truth, views);
            match (result.disposition, result.derivation) {
                (ProofDisposition::Proved, Some(proof)) => proofs.push(proof),
                _ => return None,
            }
        }
        if let Some(relation) = &origins.comparison {
            if residual.is_none() {
                residual = Some(self.reasoning().render_relation_of_terms(relation));
            }
            let negated = relation.negated();
            let relation = if truth { relation } else { &negated };
            super::super::work::condition_query();
            // The comparison is an L0 fact at branch entry, so only the
            // ordinary closed state may decide it; a pending view would fall
            // back to origin transport and delete a test whose L0 fact later
            // L0 readers still need.
            let result = self.reasoning().prove(
                ProofContext {
                    origin_view: OriginView::Ordinary,
                    ..context
                },
                ProofGoal::Ordering {
                    relation,
                    affine: None,
                },
            );
            match (result.disposition, result.derivation) {
                (ProofDisposition::Proved, Some(proof)) => proofs.push(proof),
                _ => return None,
            }
        }
        Some((residual.unwrap_or_default(), proofs))
    }

    /// Renders the source premises of one deciding proof before the ledger
    /// is pruned. The strings grant no fact and name no arena identity.
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
                    Some(self.reasoning().render_relation_of_terms(relation))
                }
                DerivationNode::ImplicitBound {
                    left,
                    right,
                    bound,
                    kind,
                } => Some(format!(
                    "{kind:?}: {}",
                    self.reasoning().render_relation_of_terms(&Relation::Bound {
                        left: *left,
                        right: *right,
                        bound: *bound,
                    })
                )),
                DerivationNode::SourceDistinct {
                    left,
                    right,
                    difference,
                    ..
                } => Some(self.reasoning().render_relation_of_terms(&Relation::Distinct {
                    left: *left,
                    right: *right,
                    difference: *difference,
                })),
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
