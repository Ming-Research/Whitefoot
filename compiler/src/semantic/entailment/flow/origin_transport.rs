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
    allocation_order: Vec<usize>,
}

#[derive(Default)]
struct OriginKeys {
    definitions: HashMap<BindingId, (GoalExpression, DerivationId)>,
    nodes: Vec<OriginNode>,
    ids: HashMap<OriginNode, usize>,
    memo: HashMap<GoalExpression, Vec<(GoalExpression, Expansion)>>,
    following: HashSet<BindingId>,
    allocation_order: Vec<usize>,
    collection_order: HashMap<usize, usize>,
}

impl OriginKeys {
    fn visit_key(&mut self, key: usize) {
        let next = self.collection_order.len();
        self.collection_order.entry(key).or_insert(next);
        self.allocation_order.push(key);
    }

    fn intern(&mut self, node: OriginNode) -> usize {
        let key = if let Some(key) = self.ids.get(&node) {
            *key
        } else {
            let key = self.nodes.len();
            self.nodes.push(node.clone());
            self.ids.insert(node, key);
            key
        };
        self.visit_key(key);
        key
    }

    fn start_collection(&mut self) {
        self.allocation_order.clear();
        self.collection_order.clear();
    }

    fn expansion(&mut self, expression: &GoalExpression) -> Expansion {
        if let Some((_, expansion)) = self.memo.get(expression).and_then(|views| {
            views
                .iter()
                .find(|(view, _)| view.same_admission_view(expression))
        }) {
            let expansion = expansion.clone();
            for &key in &expansion.allocation_order {
                self.visit_key(key);
            }
            return expansion;
        }
        let allocation_start = self.allocation_order.len();
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
        let mut allocation_order = self.allocation_order[allocation_start..].to_vec();
        let mut seen = HashSet::new();
        allocation_order.retain(|key| seen.insert(*key));
        let expansion = Expansion {
            key,
            origins,
            datum: expanded_datum,
            allocation_order,
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

type EnteringOrigin = (GoalId, GoalSign, DerivationId, bool);

#[derive(Clone)]
struct OriginCollection {
    entering: Vec<EnteringOrigin>,
    exclusions: Vec<(GoalId, bool)>,
    expressions: Vec<GoalExpression>,
    seen: HashMap<GoalExpression, Vec<GoalExpression>>,
    numeric_views: HashMap<GoalExpression, Vec<GoalExpression>>,
}

/// The expansion DAG survives exactly an unchanged live-definition map;
/// the source collection additionally requires unchanged ordered signed
/// sources (including proof and numeric-admission identity) and ordered
/// exclusion expression views. Thus a hit reproduces the same preorder
/// trees and transport parents. Replaying each
/// expansion's allocation trace restores fresh-build class order, so cached
/// node identities cannot change which retained proof is selected.
#[derive(Default)]
pub(super) struct OriginQueryCache {
    origins: Vec<(BindingId, GoalId, DerivationId)>,
    keys: OriginKeys,
    collection: Option<OriginCollection>,
}

impl OriginQueryCache {
    fn set_origins(&mut self, origins: Vec<(BindingId, GoalId, DerivationId)>) {
        if self.origins != origins {
            self.origins = origins;
            self.keys = OriginKeys::default();
            self.collection = None;
        }
        self.keys.start_collection();
    }

    fn source_collection(
        &mut self,
        goals: &GoalTable,
        entering: &[EnteringOrigin],
        exclusions: &[(GoalId, bool)],
    ) -> OriginCollection {
        if self.collection.as_ref().is_none_or(|collection| {
            collection.entering != entering || collection.exclusions != exclusions
        }) {
            let mut expressions = Vec::new();
            let mut seen = HashMap::new();
            for &(binding, goal, proof) in &self.origins {
                let definition = goals.expression(goal).clone();
                self.keys
                    .definitions
                    .insert(binding, (definition.clone(), proof));
                collect(
                    goal_binding_place(binding, [], definition.ty()),
                    &mut expressions,
                    &mut seen,
                );
                collect(definition, &mut expressions, &mut seen);
            }
            let mut numeric_views = seen.clone();
            for (goal, _, _, numeric) in entering {
                let expression = goals.expression(*goal).clone();
                if *numeric {
                    collect(expression.clone(), &mut Vec::new(), &mut numeric_views);
                }
                collect(expression, &mut expressions, &mut seen);
            }
            for &(goal, numeric) in exclusions {
                let expression = goals.expression(goal).clone();
                if numeric {
                    collect(expression.clone(), &mut Vec::new(), &mut numeric_views);
                }
                collect(expression, &mut expressions, &mut seen);
            }
            self.collection = Some(OriginCollection {
                entering: entering.to_vec(),
                exclusions: exclusions.to_vec(),
                expressions,
                seen,
                numeric_views,
            });
        }
        self.collection.as_ref().unwrap().clone()
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
        self.build_origin_query_view(context, submitted, false)
            .expect("ordinary origin views always build")
    }

    /// Share the two OP-5 truths only when the newly submitted negation has
    /// no distinct spelling to receive its inferred sign. Its sole extra
    /// member and term view cannot transport a fact into an entering class;
    /// a nontrivial class requires the two specification-defined inventories.
    pub(super) fn try_condition_origin_view(
        &mut self,
        context: ProofContext<'_>,
        negated: &GoalExpression,
    ) -> Option<FactState> {
        self.build_origin_query_view(context, Some(negated), true)
    }

    fn build_origin_query_view(
        &mut self,
        context: ProofContext<'_>,
        submitted: Option<&GoalExpression>,
        require_new_class: bool,
    ) -> Option<FactState> {
        let mut cache = std::mem::take(&mut self.vocabulary.origin_queries);
        let mut origins = context
            .facts
            .goal_origins
            .iter()
            .map(|(binding, origin)| (*binding, origin.goal, origin.proof))
            .collect::<Vec<_>>();
        origins.sort_by_key(|(binding, _, _)| binding.0);
        cache.set_origins(origins);
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
        let exclusions = entering_exclusion_expressions(&context.facts.exclusions);
        let OriginCollection {
            mut expressions,
            mut seen,
            mut numeric_views,
            ..
        } = cache.source_collection(&self.vocabulary.goals, &entering, &exclusions);
        let keys = &mut cache.keys;
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
                    .entry(keys.collection_order[&expansion.key])
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
            classes
                .entry(keys.collection_order[&expansion.key])
                .or_default()
                .push(goal);
            expansions.insert(goal, expansion);
        }
        if require_new_class {
            let submitted = submitted.expect("sharing requires the submitted negation");
            let expansion = keys.expansion(submitted);
            let term_view = keys.term_view(submitted, &representatives);
            let allowed = [
                self.vocabulary.goals.id(submitted),
                self.vocabulary.goals.id(&term_view),
            ];
            let class_was_present = cache
                .collection
                .as_ref()
                .unwrap()
                .expressions
                .iter()
                .filter(|expression| expression.ty() == CheckedType::Bool)
                .any(|expression| keys.expansion(expression).key == expansion.key);
            if class_was_present
                || classes[&keys.collection_order[&expansion.key]]
                    .iter()
                    .any(|goal| !allowed.contains(&Some(*goal)))
            {
                self.vocabulary.origin_queries = cache;
                return None;
            }
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
                            values: std::sync::Arc::clone(&fact.values),
                            conclusion: ExclusionConclusion::Set,
                            event: None,
                            parents,
                        }),
                    });
                view.establish_exclusion(Exclusion::from_view(
                    goal,
                    std::sync::Arc::clone(&fact.values),
                    projection,
                    proof,
                    &self.vocabulary.goals,
                ));
            }
        }
        let class_keys = keys.collection_order.clone();
        self.vocabulary.origin_queries = cache;
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
            for &goal in &classes[&class_keys[&expansions[&from].key]] {
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
                return Some(view);
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
                                return Some(view);
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
                return Some(view);
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

/// Goal identities are allocated in source order. A join's first input
/// chooses storage order, which must not choose an OT representative.
fn entering_exclusion_expressions(exclusions: &[Exclusion]) -> Vec<(GoalId, bool)> {
    let mut expressions = Vec::new();
    for fact in exclusions {
        if fact.views.is_empty() {
            // Common canonical truth supplies no lost numeric admission.
            expressions.push((fact.expression, false));
        } else {
            expressions.extend(fact.views.iter().map(|view| (view.expression, true)));
        }
    }
    expressions.sort_by_key(|(goal, _)| goal.0);
    expressions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boolean(binding: u32) -> GoalExpression {
        goal_binding_place(BindingId(binding), [], CheckedType::Bool)
    }

    fn raw_goal(goals: &mut GoalTable, expression: GoalExpression) -> GoalId {
        goals.intern(expression, None, None, Vec::new())
    }

    #[test]
    fn origin_memo_invalidates_changed_definition_and_transport_parent() {
        let mut goals = GoalTable::default();
        let old = raw_goal(&mut goals, boolean(1));
        let new = raw_goal(&mut goals, boolean(2));
        let mut cache = OriginQueryCache::default();
        cache.set_origins(vec![(BindingId(0), old, DerivationId(7))]);
        cache.source_collection(&goals, &[], &[]);
        assert_eq!(cache.keys.expansion(&boolean(0)).origins, [DerivationId(7)]);

        cache.set_origins(vec![(BindingId(0), new, DerivationId(8))]);
        let collection = cache.source_collection(&goals, &[], &[]);
        assert_eq!(collection.expressions, [boolean(0), boolean(2)]);
        let expanded = cache.keys.expansion(&boolean(0));
        assert_eq!(expanded.key, cache.keys.expansion(&boolean(2)).key);
        assert_ne!(expanded.key, cache.keys.expansion(&boolean(1)).key);
        assert_eq!(expanded.origins, [DerivationId(8)]);

        // Even an identical RHS needs the current introduction as its parent.
        cache.set_origins(vec![(BindingId(0), new, DerivationId(9))]);
        cache.source_collection(&goals, &[], &[]);
        assert_eq!(cache.keys.expansion(&boolean(0)).origins, [DerivationId(9)]);

        cache.set_origins(Vec::new());
        assert!(cache.source_collection(&goals, &[], &[]).expressions.is_empty());
        assert!(cache.keys.expansion(&boolean(0)).origins.is_empty());
    }

    #[test]
    fn origin_collection_invalidates_changed_entering_source_and_numeric_view() {
        let mut goals = GoalTable::default();
        let first = raw_goal(&mut goals, boolean(1));
        let second = raw_goal(&mut goals, boolean(2));
        let mut cache = OriginQueryCache::default();
        cache.set_origins(Vec::new());
        let first_sources = [(first, GoalSign::Positive, DerivationId(3), true)];
        let collection = cache.source_collection(&goals, &first_sources, &[]);
        assert_eq!(collection.expressions, [boolean(1)]);
        assert!(collection.numeric_views.contains_key(&boolean(1)));

        let second_sources = [(second, GoalSign::Negative, DerivationId(4), false)];
        let collection = cache.source_collection(&goals, &second_sources, &[]);
        assert_eq!(collection.expressions, [boolean(2)]);
        assert!(!collection.numeric_views.contains_key(&boolean(2)));
        assert_eq!(collection.entering, second_sources);

        let numeric_sources = [(second, GoalSign::Negative, DerivationId(5), true)];
        let collection = cache.source_collection(&goals, &numeric_sources, &[]);
        assert!(collection.numeric_views.contains_key(&boolean(2)));
        assert_eq!(collection.entering, numeric_sources);
    }

    #[test]
    fn origin_collection_invalidates_changed_exclusion_expression_and_numeric_view() {
        let integer = |binding| {
            goal_binding_place(BindingId(binding), [], CheckedType::Integer(IntegerType::U64))
        };
        let mut goals = GoalTable::default();
        let first = raw_goal(&mut goals, integer(1));
        let second = raw_goal(&mut goals, integer(2));
        let mut cache = OriginQueryCache::default();
        cache.set_origins(Vec::new());
        let first_exclusions = [(first, true)];
        let collection = cache.source_collection(&goals, &[], &first_exclusions);
        assert_eq!(collection.expressions, [integer(1)]);
        assert!(collection.numeric_views.contains_key(&integer(1)));

        let second_exclusions = [(second, true)];
        let collection = cache.source_collection(&goals, &[], &second_exclusions);
        assert_eq!(collection.expressions, [integer(2)]);
        assert!(collection.numeric_views.contains_key(&integer(2)));

        let opaque_exclusions = [(second, false)];
        let collection = cache.source_collection(&goals, &[], &opaque_exclusions);
        assert_eq!(collection.expressions, [integer(2)]);
        assert!(!collection.numeric_views.contains_key(&integer(2)));
        assert_eq!(collection.exclusions, opaque_exclusions);

        assert!(
            cache
                .source_collection(&goals, &[], &[])
                .expressions
                .is_empty()
        );
    }

    #[test]
    fn cached_expansions_keep_fresh_class_order_after_query_order_changes() {
        let negated = GoalExpression::Operation {
            row: GoalOperation::Boolean(CheckedBooleanOperation::Not),
            type_arguments: Vec::new(),
            const_arguments: Vec::new(),
            result: CheckedType::Bool,
            arguments: vec![boolean(1)],
        };
        let expressions = [boolean(2), negated, boolean(1)];
        let mut cached = OriginKeys::default();
        for expression in expressions.iter().rev() {
            cached.expansion(expression);
        }
        cached.start_collection();
        let mut fresh = OriginKeys::default();
        for expression in &expressions {
            let cached_expansion = cached.expansion(expression);
            let fresh_expansion = fresh.expansion(expression);
            assert_eq!(
                cached.collection_order[&cached_expansion.key],
                fresh.collection_order[&fresh_expansion.key],
                "persistent node ids must not reorder the fixed-point classes"
            );
        }
    }
}

#[cfg(test)]
mod exclusion_order_tests {
    use super::*;

    #[test]
    fn entering_exclusions_follow_source_allocation_not_first_predecessor_order() {
        let mut goals = GoalTable::default();
        for bits in 0..10 {
            goals.intern(
                GoalExpression::Datum(GoalDatum::Literal(CheckedValue::Integer {
                    ty: IntegerType::U8,
                    bits,
                })),
                None,
                None,
                Vec::new(),
            );
        }
        let fact = |expression, views: &[u32]| {
            let make_view = |id| {
                Exclusion::from_view(
                    GoalId(id),
                    std::sync::Arc::from([0_i128]),
                    None,
                    DerivationId(0),
                    &goals,
                )
            };
            let mut fact = make_view(expression);
            fact.views = views.iter().flat_map(|id| make_view(*id).views).collect();
            fact
        };
        let facts = [fact(8, &[9, 8]), fact(4, &[]), fact(1, &[2, 1])];
        assert_eq!(
            entering_exclusion_expressions(&facts),
            vec![
                (GoalId(1), true),
                (GoalId(2), true),
                (GoalId(4), false),
                (GoalId(8), true),
                (GoalId(9), true),
            ]
        );
        let reversed = facts.iter().cloned().rev().collect::<Vec<_>>();
        assert_eq!(
            entering_exclusion_expressions(&reversed),
            entering_exclusion_expressions(&facts)
        );
    }
}
