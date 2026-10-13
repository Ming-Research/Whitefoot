//! Canonical merge-only frontiers [ENT-5]. Only the walker can seal a
//! frontier; snapshots and prover cache demands cannot create a boundary.

use super::super::LoopInductionInput;
use super::super::state::{HeaderRelationInput, TransportedHeaderRelation};
use super::*;

pub(super) struct InductionBatch {
    pub(super) input: LoopInductionInput,
    pub(super) members: Vec<RelationBatch>,
    pub(super) hidden_update: Option<DerivationId>,
}

impl Analyzer<'_, '_> {
    pub(super) fn exit_frontier_scopes(
        &mut self,
        state: &mut ProofFlowState,
        frontier: &mut [FlowEdge],
        depth: usize,
    ) {
        if frontier.is_empty() {
            self.exit_scopes_to(state, depth);
        } else {
            for edge in frontier {
                self.exit_scopes_to(&mut edge.state, depth);
            }
        }
    }

    pub(super) fn seal_frontier(
        &mut self,
        state: &mut ProofFlowState,
        frontier: &mut Vec<FlowEdge>,
    ) {
        if frontier.is_empty() {
            return;
        }
        let mut edges = std::mem::take(frontier);
        edges.sort_by(|a, b| a.site.components().cmp(b.site.components()));
        let sites = edges
            .iter()
            .map(|edge| edge.site.clone())
            .collect::<Vec<_>>();
        let inputs = edges.into_iter().map(|edge| edge.state).collect::<Vec<_>>();
        *state = self.join_with_transport(&inputs, &sites);
    }

    pub(super) fn join_with_transport(
        &mut self,
        inputs: &[ProofFlowState],
        sites: &[crate::NodePath],
    ) -> ProofFlowState {
        let mut output = self.judging().join_flows(inputs);
        self.vocabulary.promote_flow_contradiction(&mut output);
        if output.facts.all_derivable {
            return output;
        }
        let templates = self
            .frames
            .loops
            .iter()
            .flat_map(|frame| frame.templates.iter().cloned())
            .collect::<Vec<_>>();
        let mut publication = Vec::new();
        for template in templates {
            // Output formability reads the ordinary join, before any member
            // is published. Source views share identities, never new facts.
            let Ok((instance, relation_view)) =
                self.reasoning()
                    .relation_instance(&template.relation, &output, None)
            else {
                continue;
            };
            let mut evidence = Vec::new();
            let mut proved = true;
            let source_ordinal = self
                .frames
                .loops
                .iter()
                .find(|frame| frame.id == template.loop_id)
                .and_then(|frame| {
                    frame
                        .templates
                        .iter()
                        .position(|candidate| candidate.declaration == template.declaration)
                })
                .expect("active template has its written ordinal");
            let mut first_loss = None;
            for (ordinal, input) in inputs.iter().enumerate() {
                let batch =
                    self.reasoning()
                        .prove_relation_instance(&template.relation, input, None);
                proved &= batch.disposition == TargetDisposition::Proved;
                if batch.disposition != TargetDisposition::Proved
                    && first_loss.is_none()
                    && let Some(component) = batch.evidence.failing_component
                {
                    let leaves = template
                        .relation
                        .left
                        .postorder()
                        .chain(template.relation.right.postorder())
                        .filter(|leaf| {
                            matches!(
                                leaf.kind,
                                CheckedAffineExpressionKind::Local { .. }
                                    | CheckedAffineExpressionKind::Measure(_)
                                    | CheckedAffineExpressionKind::Field(_)
                                    | CheckedAffineExpressionKind::Constant { .. }
                                    | CheckedAffineExpressionKind::ConstGeneric { .. }
                            )
                        });
                    let atoms = relation_view.affine.measure_atoms.borrow();
                    let mut support_terms = Vec::new();
                    let mut measures = Vec::new();
                    for (leaf, image) in leaves.zip(&instance.operands) {
                        if !matches!(
                            leaf.kind,
                            CheckedAffineExpressionKind::Measure(_)
                                | CheckedAffineExpressionKind::Field(_)
                        ) {
                            continue;
                        }
                        let mut terms = atoms
                            .iter()
                            .filter_map(|(term, value)| (value == image).then_some(*term))
                            .collect::<Vec<_>>();
                        terms.sort_unstable();
                        if let [term] = terms.as_slice() {
                            measures.push(*term);
                        }
                        support_terms.extend(terms);
                    }
                    support_terms.sort_unstable();
                    support_terms.dedup();
                    measures.sort_unstable();
                    measures.dedup();
                    let mut conjunct = template.relation.clone();
                    conjunct.equality = false;
                    if component == 1 {
                        std::mem::swap(&mut conjunct.left, &mut conjunct.right);
                    }
                    first_loss = Some(LostHeaderRelation {
                        template: SourceLoopInvariantRef {
                            loop_id: template.loop_id,
                            source_ordinal: source_ordinal as u32,
                        },
                        name: template.name.clone(),
                        relation: self
                            .input
                            .render_checked_invariant_relation(&template.relation, None),
                        conjunct: self.input.render_checked_invariant_relation(&conjunct, None),
                        incoming: sites
                            .get(ordinal)
                            .cloned()
                            .unwrap_or_else(|| template.relation.node_path.clone()),
                        components: instance.components.clone(),
                        bindings: template
                            .relation
                            .left
                            .postorder()
                            .chain(template.relation.right.postorder())
                            .filter_map(|expression| match &expression.kind {
                                CheckedAffineExpressionKind::Local { binding, .. } => Some(*binding),
                                _ => None,
                            })
                            .collect(),
                        support: support_terms
                            .into_iter()
                            .map(|term| self.vocabulary.terms.kind(term).clone())
                            .collect(),
                        kills: input
                            .measure_kills
                            .iter()
                            .filter(|kill| {
                                measures.contains(&kill.term)
                                    && sites.get(ordinal).is_some_and(|site| {
                                        kill.source.components().starts_with(site.components())
                                    })
                            })
                            .cloned()
                            .collect(),
                    });
                }
                evidence.push(HeaderRelationInput {
                    site: sites
                        .get(ordinal)
                        .cloned()
                        .unwrap_or_else(|| template.relation.node_path.clone()),
                    instance: batch.evidence.instance,
                    components: batch.evidence.components,
                    contradiction: batch.evidence.contradiction,
                });
            }
            if !proved {
                if let Some(loss) = first_loss {
                    output.lost_headers.push(loss);
                }
                continue;
            }
            for (component, inequality) in instance.components.iter().enumerate() {
                let proof =
                    self.vocabulary
                        .derivations
                        .intern(DerivationNode::TransportedHeaderRelation {
                            detail: Box::new(TransportedHeaderRelation {
                                template: SourceLoopInvariantRef {
                                    loop_id: template.loop_id,
                                    source_ordinal: source_ordinal as u32,
                                },
                                inputs: evidence.clone(),
                                output: instance.clone(),
                                component: component as u8,
                            }),
                        });
                let occurrence = self.vocabulary.header_relation_roots;
                self.vocabulary.header_relation_roots = occurrence
                    .checked_add(1)
                    .expect("transported relation roots exceed the u32 identity space");
                self.vocabulary
                    .derivations
                    .add_root(DerivationRootKind::HeaderRelation { occurrence }, proof);
                publication.push(ActiveAffineFact {
                    inequality: inequality.clone(),
                    evidence: AffineFactEvidence::Derivation(proof),
                });
            }
        }
        for fact in publication {
            if !output
                .affine
                .facts
                .iter()
                .any(|old| old.inequality == fact.inequality)
            {
                output.affine.facts.push(fact);
            }
        }
        output
    }

    pub(super) fn prove_induction_frontier(
        &mut self,
        loop_id: CheckedLoopId,
        invariants: &[CheckedLoopInvariant],
        edges: &[FlowEdge],
        binder: Option<BindingId>,
        kills: &LoopKills,
    ) -> Vec<InductionBatch> {
        let mut batches = Vec::new();
        for edge in edges {
            debug_assert_summarized(&edge.state, kills);
            let mut hidden_update = None;
            let mut next = None;
            if let Some(binder) = binder
                && let Some(value) = edge.state.affine.values.get(&binder).and_then(|value| {
                    value
                        .add(&AffineForm::constant(1), &mut AffineCheckState::new())
                        .ok()
                })
            {
                let limit = self
                    .vocabulary
                    .terms
                    .intern(TermKind::Constant(u64::MAX as i128));
                if let Ok(inequality) = AffineInequality::from_forms(
                    &value,
                    &AffineForm::constant(u64::MAX as i128),
                    &mut AffineCheckState::new(),
                ) {
                    let result = self.reasoning().prove(
                        ProofContext::new(&edge.state.facts, &edge.state.affine),
                        ProofGoal::Affine {
                            inequality: &inequality,
                            right: Some(limit),
                        },
                    );
                    if result.disposition == ProofDisposition::Proved {
                        hidden_update = result.derivation;
                    }
                }
                next = Some(NextHeader {
                    loop_id,
                    edge: &edge.site,
                    binder,
                    value,
                });
            }
            let mut members = Vec::new();
            for invariant in invariants {
                let mut result = self.reasoning().prove_relation_instance(
                    &invariant.relation,
                    &edge.state,
                    next.as_ref(),
                );
                if binder.is_some() && hidden_update.is_none() {
                    result.disposition = TargetDisposition::Unproved;
                }
                self.vocabulary.retain_loop_relation(&result.evidence);
                members.push(result);
            }
            if let Some(proof) = hidden_update {
                self.vocabulary.retain_induction_root(proof);
            }
            batches.push(InductionBatch {
                input: LoopInductionInput {
                    site: edge.site.clone(),
                    branch: edge.branch.clone(),
                    route: edge.route.clone(),
                },
                members,
                hidden_update,
            });
        }
        batches
    }
}

impl Vocabulary {
    pub(super) fn retain_induction_root(&mut self, proof: DerivationId) {
        let occurrence = self.loop_induction_roots;
        self.loop_induction_roots = occurrence
            .checked_add(1)
            .expect("loop induction roots exceed the u32 identity space");
        self.derivations
            .add_root(DerivationRootKind::LoopInduction { occurrence }, proof);
    }

    pub(super) fn retain_loop_relation(&mut self, evidence: &super::super::LoopRelationEvidence) {
        for proof in evidence
            .components
            .iter()
            .chain(evidence.contradiction.iter())
            .chain(
                evidence
                    .instance
                    .iter()
                    .flat_map(|instance| instance.formation.iter()),
            )
        {
            self.retain_induction_root(*proof);
        }
    }
}
