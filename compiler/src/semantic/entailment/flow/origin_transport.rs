//! ENT-4's finite query view of live ordinary-let definitions.
//!
//! Expansion keys are shared typed DAG nodes, not expanded expression trees.
//! The view never becomes flow state, including at a kill or join.

use super::super::state::{DerivationId, OriginEquality};
use super::*;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum OriginNode {
    Datum(GoalDatum),
    Operation {
        row: GoalOperation,
        types: Vec<CheckedType>,
        constants: Vec<CheckedConst>,
        result: CheckedType,
        arguments: Vec<usize>,
    },
}

#[derive(Clone)]
struct Expansion {
    key: usize,
    origins: Vec<DerivationId>,
}

#[derive(Default)]
struct OriginKeys {
    definitions: HashMap<BindingId, (GoalExpression, DerivationId)>,
    nodes: Vec<OriginNode>,
    ids: HashMap<OriginNode, usize>,
    memo: HashMap<GoalExpression, Expansion>,
    following: HashSet<BindingId>,
}

impl OriginKeys {
    fn intern(&mut self, node: OriginNode) -> usize {
        if let Some(key) = self.ids.get(&node) {
            return *key;
        }
        let key = self.nodes.len();
        self.nodes.push(node.clone());
        self.ids.insert(node, key);
        key
    }

    fn expansion(&mut self, expression: &GoalExpression) -> Expansion {
        if let Some(expansion) = self.memo.get(expression) {
            return expansion.clone();
        }
        let mut origins = Vec::new();
        let key = match expression {
            GoalExpression::Datum(
                datum @ GoalDatum::Place {
                    root,
                    projections,
                    ty,
                },
            ) => {
                if let Some((definition, proof)) = self.definitions.get(root).cloned() {
                    assert!(
                        self.following.insert(*root),
                        "ordinary-let definitions are acyclic"
                    );
                    let expanded = self.expansion(&definition);
                    self.following.remove(root);
                    let projected = if projections.is_empty() {
                        Some(expanded.key)
                    } else if let OriginNode::Datum(datum) = self.nodes[expanded.key].clone() {
                        let mut projected = Some(GoalExpression::Datum(datum));
                        for projection in projections {
                            projected =
                                projected.and_then(|value| value.with_projection(*projection, *ty));
                        }
                        projected.map(|value| {
                            let GoalExpression::Datum(datum) = value else {
                                unreachable!()
                            };
                            self.intern(OriginNode::Datum(datum))
                        })
                    } else {
                        None
                    };
                    if let Some(key) = projected {
                        origins = expanded.origins;
                        origins.push(proof);
                        key
                    } else {
                        self.intern(OriginNode::Datum(datum.clone()))
                    }
                } else {
                    self.intern(OriginNode::Datum(datum.clone()))
                }
            }
            GoalExpression::Datum(datum) => self.intern(OriginNode::Datum(datum.clone())),
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        let expanded = self.expansion(argument);
                        origins.extend(expanded.origins);
                        expanded.key
                    })
                    .collect();
                self.intern(OriginNode::Operation {
                    row: *row,
                    types: type_arguments.clone(),
                    constants: const_arguments.clone(),
                    result: *result,
                    arguments,
                })
            }
        };
        origins.sort_unstable();
        origins.dedup();
        let expansion = Expansion { key, origins };
        self.memo.insert(expression.clone(), expansion.clone());
        expansion
    }

    fn term_view(
        &mut self,
        expression: &GoalExpression,
        representatives: &HashMap<usize, GoalExpression>,
    ) -> GoalExpression {
        if fragment_type(expression.ty()).is_some()
            && let Some(representative) = representatives.get(&self.expansion(expression).key)
        {
            return representative.clone();
        }
        match expression {
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
                    .map(|argument| self.term_view(argument, representatives))
                    .collect(),
            },
            GoalExpression::Datum(_) => expression.clone(),
        }
    }
}

fn collect(
    expression: GoalExpression,
    expressions: &mut Vec<GoalExpression>,
    seen: &mut HashSet<GoalExpression>,
) {
    if !seen.insert(expression.clone()) {
        return;
    }
    expressions.push(expression.clone());
    if let GoalExpression::Operation { arguments, .. } = expression {
        for argument in arguments {
            collect(argument, expressions, seen);
        }
    }
}

fn definition_parents(left: &Expansion, right: &Expansion) -> Box<[DerivationId]> {
    assert_eq!(
        left.key, right.key,
        "transport preserves the complete typed origin"
    );
    let mut origins = left.origins.clone();
    origins.extend_from_slice(&right.origins);
    origins.sort_unstable();
    origins.dedup();
    origins.into_boxed_slice()
}

impl Reasoning<'_, '_, '_> {
    fn transport_origin_sign(
        &mut self,
        from: GoalId,
        goal: GoalId,
        sign: GoalSign,
        parent: DerivationId,
        expansions: &HashMap<GoalId, Expansion>,
    ) -> DerivationId {
        if from == goal {
            return parent;
        }
        let origins = definition_parents(&expansions[&from], &expansions[&goal]);
        self.vocabulary
            .derivations
            .intern(DerivationNode::OriginTransport {
                from,
                goal,
                sign,
                parent,
                origins,
            })
    }

    pub(super) fn origin_query_view(
        &mut self,
        context: ProofContext<'_>,
        submitted: Option<&GoalExpression>,
    ) -> FactState {
        let mut keys = OriginKeys::default();
        let mut expressions = Vec::new();
        let mut seen = HashSet::new();
        let mut definitions = context.facts.goal_origins.iter().collect::<Vec<_>>();
        definitions.sort_by_key(|(binding, _)| binding.0);
        for (binding, origin) in definitions {
            let definition = self.vocabulary.goals.expression(origin.goal).clone();
            keys.definitions
                .insert(*binding, (definition.clone(), origin.proof));
            collect(
                goal_binding_place(*binding, [], definition.ty()),
                &mut expressions,
                &mut seen,
            );
            collect(definition, &mut expressions, &mut seen);
        }
        let mut entering = context.facts.opaque.iter().copied().collect::<Vec<_>>();
        entering.sort_by_key(|(goal, sign)| (goal.0, *sign == GoalSign::Negative));
        for (goal, _) in &entering {
            collect(
                self.vocabulary.goals.expression(*goal).clone(),
                &mut expressions,
                &mut seen,
            );
        }
        if let Some(submitted) = submitted {
            collect(submitted.clone(), &mut expressions, &mut seen);
        }

        // One representative and a star of equalities per integer class.
        // All identities come from the fixed collection above, before adding
        // any query fact; no inferred relation selects another candidate.
        let mut representatives = HashMap::new();
        let mut integer_classes: BTreeMap<usize, Vec<(GoalId, TermId, i128)>> = BTreeMap::new();
        let mut expansions = HashMap::new();
        for expression in &expressions {
            let expansion = keys.expansion(expression);
            let goal = self.intern_goal_expression(expression.clone());
            if fragment_type(expression.ty()).is_some()
                && let Some((term, offset)) = self.goal_side(expression)
            {
                representatives
                    .entry(expansion.key)
                    .or_insert_with(|| expression.clone());
                integer_classes
                    .entry(expansion.key)
                    .or_default()
                    .push((goal, term, offset));
            }
            expansions.insert(goal, expansion);
        }
        let boolean_trees = expressions
            .iter()
            .filter(|expression| expression.ty() == CheckedType::Bool)
            .cloned()
            .collect::<Vec<_>>();
        for expression in boolean_trees {
            let view = keys.term_view(&expression, &representatives);
            collect(view, &mut expressions, &mut seen);
        }
        let mut classes: BTreeMap<usize, Vec<GoalId>> = BTreeMap::new();
        for expression in &expressions {
            if expression.ty() != CheckedType::Bool {
                continue;
            }
            let goal = self.intern_goal_expression(expression.clone());
            let expansion = keys.expansion(expression);
            classes.entry(expansion.key).or_default().push(goal);
            expansions.insert(goal, expansion);
        }
        let mut view = context.facts.clone();
        for members in integer_classes.values() {
            let (left, left_term, left_offset) = members[0];
            for &(right, right_term, right_offset) in &members[1..] {
                if left_term == right_term && left_offset == right_offset {
                    continue;
                }
                let Some(difference) = right_offset.checked_sub(left_offset) else {
                    continue;
                };
                let relation = Relation::Equal {
                    left: left_term,
                    right: right_term,
                    difference,
                };
                let left_expression = self.vocabulary.goals.expression(left).clone();
                let right_expression = self.vocabulary.goals.expression(right).clone();
                let goal = self.intern_goal_expression(GoalExpression::Operation {
                    row: GoalOperation::Integer {
                        operation: CheckedIntegerOperation::Equal,
                        operand_type: left_expression.ty(),
                    },
                    type_arguments: Vec::new(),
                    const_arguments: Vec::new(),
                    result: CheckedType::Bool,
                    arguments: vec![left_expression, right_expression],
                });
                let proof = self
                    .vocabulary
                    .derivations
                    .intern(DerivationNode::OriginEquality {
                        detail: Box::new(OriginEquality {
                            left,
                            right,
                            goal,
                            relation: relation.clone(),
                            origins: definition_parents(&expansions[&left], &expansions[&right]),
                        }),
                    });
                view.establish_from_proof(&relation, proof, &self.vocabulary.derivations);
            }
        }

        // Only entering sources supply projected relations. A derived sign
        // below never becomes another numeric/AUTO premise.
        for (from, sign) in entering {
            let parent = context.facts.opaque_proofs[&(from, sign)];
            for &goal in &classes[&expansions[&from].key] {
                let proof = self.transport_origin_sign(from, goal, sign, parent, &expansions);
                view.establish_derived_goal(goal, sign, proof);
                if from != goal
                    && let Some(relation) = self.vocabulary.goals.projection(goal).cloned()
                {
                    let relation = if sign == GoalSign::Positive {
                        relation
                    } else {
                        relation.negated()
                    };
                    let projection =
                        self.vocabulary
                            .derivations
                            .intern(DerivationNode::OriginProjection {
                                goal,
                                sign,
                                relation: relation.clone(),
                                parent: proof,
                            });
                    view.establish_from_proof(&relation, projection, &self.vocabulary.derivations);
                }
            }
        }

        // Monotone finite signed closure. Every changing pass installs at
        // least one of exactly two signs per collected Boolean member.
        loop {
            let closed = close(
                &view,
                &self.vocabulary.terms,
                &self.vocabulary.goals,
                &mut self.vocabulary.derivations,
            );
            if closed.contradictory() {
                return view;
            }
            let mut additions = Vec::new();
            for members in classes.values() {
                for &from in members {
                    for sign in [GoalSign::Positive, GoalSign::Negative] {
                        if members
                            .iter()
                            .all(|goal| view.opaque.contains(&(*goal, sign)))
                        {
                            continue;
                        }
                        let mut parent = closed.goal_proof(
                            from,
                            sign,
                            &self.vocabulary.goals,
                            &mut self.vocabulary.derivations,
                        );
                        if parent.is_none() && sign == GoalSign::Positive {
                            let expression = self.vocabulary.goals.expression(from).clone();
                            let affine =
                                self.affine_goal_ordering_target(&expression, context.affine);
                            let prepared = ProofContext {
                                facts: &view,
                                affine: context.affine,
                                closed: None,
                                origin_view: OriginView::Prepared,
                            };
                            let result = self.prove_signed(prepared, &expression, affine.as_ref());
                            if result.route == Some(ProofRoute::Contradiction) {
                                view.promote_to_contradiction(result.derivation);
                                return view;
                            }
                            if result.disposition == ProofDisposition::Proved {
                                parent = result.derivation;
                            }
                        }
                        if let Some(parent) = parent {
                            for &goal in members {
                                if !view.opaque.contains(&(goal, sign)) {
                                    let proof = self.transport_origin_sign(
                                        from,
                                        goal,
                                        sign,
                                        parent,
                                        &expansions,
                                    );
                                    additions.push((goal, sign, proof));
                                }
                            }
                        }
                    }
                }
            }
            if additions.is_empty() {
                return view;
            }
            for (goal, sign, proof) in additions {
                if !view.opaque.contains(&(goal, sign)) {
                    view.establish_derived_goal(goal, sign, proof);
                }
            }
        }
    }
}
