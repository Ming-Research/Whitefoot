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
    datum: Option<GoalDatum>,
}

#[derive(Default)]
struct OriginKeys {
    definitions: HashMap<BindingId, (GoalExpression, DerivationId)>,
    nodes: Vec<OriginNode>,
    ids: HashMap<OriginNode, usize>,
    memo: HashMap<GoalExpression, Vec<(GoalExpression, Expansion)>>,
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
        if let Some((_, expansion)) = self.memo.get(expression).and_then(|views| {
            views
                .iter()
                .find(|(view, _)| view.same_admission_view(expression))
        }) {
            return expansion.clone();
        }
        let mut origins = Vec::new();
        let mut expanded_datum = None;
        let key = match expression {
            GoalExpression::Datum(
                datum @ GoalDatum::Place {
                    root,
                    projections,
                    ty,
                    ..
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
                        Some((expanded.key, expanded.datum.clone()))
                    } else if let Some(datum) = expanded.datum.clone() {
                        let mut projected = Some(GoalExpression::Datum(datum));
                        for projection in projections {
                            projected =
                                projected.and_then(|value| value.with_projection(*projection, *ty));
                        }
                        projected.map(|value| {
                            let GoalExpression::Datum(datum) = value else {
                                unreachable!()
                            };
                            (self.intern(OriginNode::Datum(datum.clone())), Some(datum))
                        })
                    } else {
                        None
                    };
                    if let Some((key, datum)) = projected {
                        origins = expanded.origins;
                        origins.push(proof);
                        expanded_datum = datum;
                        key
                    } else {
                        expanded_datum = Some(datum.clone());
                        self.intern(OriginNode::Datum(datum.clone()))
                    }
                } else {
                    expanded_datum = Some(datum.clone());
                    self.intern(OriginNode::Datum(datum.clone()))
                }
            }
            GoalExpression::Datum(datum) => {
                expanded_datum = Some(datum.clone());
                self.intern(OriginNode::Datum(datum.clone()))
            }
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
        let expansion = Expansion {
            key,
            origins,
            datum: expanded_datum,
        };
        self.memo
            .entry(expression.clone())
            .or_default()
            .push((expression.clone(), expansion.clone()));
        expansion
    }

    fn term_view(
        &mut self,
        expression: &GoalExpression,
        representatives: &HashMap<usize, GoalExpression>,
    ) -> GoalExpression {
        // [ENT-4.OT] exact duplicate trees do not supply a second spelling's
        // admission; a genuine ordinary-let expansion still replaces a tree.
        if fragment_type(expression.ty()).is_some()
            && let Some(representative) = representatives.get(&self.expansion(expression).key)
            && (expression != representative || expression.same_admission_view(representative))
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
    seen: &mut HashMap<GoalExpression, Vec<GoalExpression>>,
) {
    let views = seen.entry(expression.clone()).or_default();
    if views
        .iter()
        .any(|view| view.same_admission_view(&expression))
    {
        return;
    }
    views.push(expression.clone());
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
        if self.vocabulary.goals.canonical(from) == self.vocabulary.goals.canonical(goal) {
            return parent;
        }
        let origins = definition_parents(&expansions[&from], &expansions[&goal]);
        assert!(
            !origins.is_empty(),
            "different origin trees need a live ordinary-let introduction"
        );
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
        let mut seen = HashMap::new();
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
        let mut numeric_views = seen.clone();
        let mut entering = context
            .facts
            .opaque_sources
            .iter()
            .flat_map(|((_, sign), views)| {
                views
                    .iter()
                    .map(|(goal, proof)| (*goal, *sign, *proof, true))
            })
            .collect::<Vec<_>>();
        for (goal, sign) in &context.facts.opaque {
            if context
                .facts
                .opaque_sources
                .get(&(*goal, *sign))
                .is_none_or(|views| views.is_empty())
            {
                entering.push((
                    *goal,
                    *sign,
                    context.facts.opaque_proofs[&(*goal, *sign)],
                    false,
                ));
            }
        }
        entering.sort_by_key(|(goal, sign, _, _)| (goal.0, *sign == GoalSign::Negative));
        for (goal, _, _, numeric) in &entering {
            let expression = self.vocabulary.goals.expression(*goal).clone();
            if *numeric {
                collect(expression.clone(), &mut Vec::new(), &mut numeric_views);
            }
            collect(expression, &mut expressions, &mut seen);
        }
        for fact in &context.facts.exclusions {
            if fact.views.is_empty() {
                // Common canonical truth does not recover an admission view
                // that was absent on a reaching path.
                collect(
                    self.vocabulary.goals.expression(fact.expression).clone(),
                    &mut expressions,
                    &mut seen,
                );
            } else {
                for view in &fact.views {
                    let expression = self.vocabulary.goals.expression(view.expression).clone();
                    collect(expression.clone(), &mut Vec::new(), &mut numeric_views);
                    collect(expression, &mut expressions, &mut seen);
                }
            }
        }
        if let Some(submitted) = submitted {
            collect(submitted.clone(), &mut Vec::new(), &mut numeric_views);
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
                && numeric_views.get(expression).is_some_and(|views| {
                    views
                        .iter()
                        .any(|view| view.same_admission_view(expression))
                })
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
        // Query-local representatives share the complete set and retain the
        // ordinary-let links that justify replacing its expression.
        for fact in &context.facts.exclusions {
            let source = self.vocabulary.goals.expression(fact.expression).clone();
            let source_expansion = keys.expansion(&source);
            for expression in &expressions {
                let expansion = keys.expansion(expression);
                if expansion.key != source_expansion.key || expression == &source {
                    continue;
                }
                let goal = self.intern_goal_expression(expression.clone());
                let numeric = !fact.views.is_empty()
                    && numeric_views.get(expression).is_some_and(|views| {
                        views
                            .iter()
                            .any(|view| view.same_admission_view(expression))
                    });
                let projection = numeric
                    .then(|| self.exclusion_projection(expression))
                    .flatten();
                let mut parents = vec![fact.proof];
                parents.extend_from_slice(&definition_parents(&source_expansion, &expansion));
                let proof = self
                    .vocabulary
                    .derivations
                    .intern(DerivationNode::IntegerExclusion {
                        detail: Box::new(ExclusionProof {
                            projection,
                            expression: goal,
                            values: std::rc::Rc::clone(&fact.values),
                            conclusion: ExclusionConclusion::Set,
                            event: None,
                            parents,
                        }),
                    });
                view.establish_exclusion(Exclusion::from_view(
                    goal,
                    std::rc::Rc::clone(&fact.values),
                    projection,
                    proof,
                    &self.vocabulary.goals,
                ));
            }
        }
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
        for (from, sign, parent, _) in entering {
            for &goal in &classes[&expansions[&from].key] {
                let proof = self.transport_origin_sign(from, goal, sign, parent, &expansions);
                view.establish_derived_goal(goal, sign, proof, &self.vocabulary.goals);
                if self.vocabulary.goals.canonical(from) != self.vocabulary.goals.canonical(goal)
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
                        if members.iter().all(|goal| {
                            view.opaque
                                .contains(&(self.vocabulary.goals.canonical(*goal), sign))
                        }) {
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
                                if !view
                                    .opaque
                                    .contains(&(self.vocabulary.goals.canonical(goal), sign))
                                {
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
                if !view
                    .opaque
                    .contains(&(self.vocabulary.goals.canonical(goal), sign))
                {
                    view.establish_derived_goal(goal, sign, proof, &self.vocabulary.goals);
                }
            }
        }
    }
}
