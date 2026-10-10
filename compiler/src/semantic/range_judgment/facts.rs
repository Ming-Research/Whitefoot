//! Active range facts, their instances, and the problems that judge one
//! obligation [RANGE-3, RANGE-4].
//!
//! A fact is a checked clause with a *frame*: what each place and value it
//! names denoted when it became active. A frame binds each place to one
//! version of one container, so a fact never changes meaning: a later write
//! makes a newer version, and a question about that version reaches the
//! fact's version only through the write's definition.
//!
//! One problem judges one obligation. Its instances are fixed by the
//! problem itself: every active fact is instantiated at each tuple whose
//! every bound variable takes a value some element read of the problem
//! already selects through one of the fact's own reads (its triggers), then
//! once more at the reads those first instances add. Second-round instances
//! form none. Written certificate instances join the completed two rounds.

use std::collections::{BTreeMap, BTreeSet};

use super::super::model::CheckedMeasure;
use super::super::range_facts::{
    CheckedRangeClause, CheckedRangePlace, CheckedRangeProjection, CheckedRangeRelation,
    CheckedRangeRoot, CheckedRangeShape, CheckedRangeTerm, RangeComparison,
};
use super::solver::{
    AtomId, AtomKind, Capacity, Linear, Literal, Problem, Relation, Rule, Verdict,
};
use super::world::{
    AtomDef, ContainerId, FactId, Stored, VersionDef, VersionId, World, projections_overlap,
};

/// What one place of a clause denotes in a frame.
#[derive(Clone, Debug)]
pub(super) enum PlaceView {
    /// A run of elements: element `k` is the container's element at
    /// `prefix ++ [offset + k]` in `version`.
    Run {
        container: ContainerId,
        version: VersionId,
        generation: u32,
        prefix: Vec<Linear>,
        offset: Linear,
        length: Linear,
    },
    /// A `Segments`: element `[d, k]` in `version`, `rows` segments.
    Segments {
        container: ContainerId,
        version: VersionId,
        generation: u32,
        rows: Linear,
    },
    /// An indexable value below an outer container element.
    Element {
        version: VersionId,
        indices: Vec<Linear>,
        projection: Vec<CheckedRangeProjection>,
    },
    /// A place the judgment cannot view.
    Unknown,
}

/// What a clause's places and values denote.
#[derive(Clone, Debug, Default)]
pub(super) struct Frame {
    pub(super) places: BTreeMap<CheckedRangePlace, PlaceView>,
    pub(super) values: BTreeMap<CheckedRangeRoot, Linear>,
    pub(super) aggregates: BTreeMap<CheckedRangeRoot, VersionId>,
}

impl Frame {
    /// [MSR-1] fixes the descriptor length wherever a formed term substituted
    /// its type's constant. A copied descriptor may read the source's length,
    /// so this equality connects the copy's bounds to the source's facts.
    fn type_facts(
        &self,
        world: &mut World,
        fixed_lengths: &BTreeSet<CheckedRangePlace>,
    ) -> BTreeSet<Literal> {
        fixed_lengths
            .iter()
            .filter_map(|place| {
                let fixed = Linear::constant(i128::from(place.fixed_length?));
                let length = match self.places.get(place)? {
                    PlaceView::Run { length, .. } => length.clone(),
                    PlaceView::Element {
                        version,
                        indices,
                        projection,
                    } => {
                        let mut path = projection.clone();
                        path.push(CheckedRangeProjection::Measure(CheckedMeasure::Length));
                        world.read(
                            *version,
                            indices.clone(),
                            path,
                            Some(super::super::model::IntegerType::U64),
                        )
                    }
                    PlaceView::Segments { .. } | PlaceView::Unknown => return None,
                };
                (length != fixed).then(|| Literal::new(length, Relation::Equal, fixed))
            })
            .collect()
    }
}

/// One active fact.
#[derive(Clone, Debug)]
pub(super) struct Fact {
    pub(super) clause: CheckedRangeClause,
    pub(super) frame: Frame,
}

/// One clause formed at one tuple.
pub(super) struct Formed {
    /// Descriptor equalities needed by fixed-length substitutions in this instance.
    pub(super) type_facts: BTreeSet<Literal>,
    /// Each bound variable's range, the clause's guards, and every read's
    /// selection of an existing element.
    pub(super) premises: Vec<Literal>,
    pub(super) conditions: Vec<Rule>,
    pub(super) conclusions: Vec<Rule>,
}

/// Forms clause terms in one frame at one tuple.
struct Former<'world> {
    world: &'world mut World,
    frame: &'world Frame,
    binders: &'world [Linear],
    iterations: &'world [Linear],
    bounds: Vec<Literal>,
    guards: Vec<Literal>,
    fixed_lengths: BTreeSet<CheckedRangePlace>,
}

impl Former<'_> {
    /// Record only lengths actually substituted while forming terms. Delay
    /// descriptor reads until formation succeeds; the vacuity check must not
    /// introduce reads for a clause with no instances.
    fn fixed_length(&mut self, place: &CheckedRangePlace) -> Option<Linear> {
        let fixed = Linear::constant(i128::from(place.fixed_length?));
        self.fixed_lengths.insert(place.clone());
        Some(fixed)
    }

    fn term(&mut self, term: &CheckedRangeTerm) -> Option<Linear> {
        match term {
            CheckedRangeTerm::Constant(value) => Some(Linear::constant(*value)),
            CheckedRangeTerm::ConstGeneric { declaration, ty } => {
                Some(self.world.const_generic(*declaration, *ty))
            }
            CheckedRangeTerm::Bound(position) => self.binders.get(*position as usize).cloned(),
            CheckedRangeTerm::Iteration(position) => {
                self.iterations.get(*position as usize).cloned()
            }
            CheckedRangeTerm::Value(root) => self.frame.values.get(root).cloned(),
            CheckedRangeTerm::ValueProjection {
                root,
                indices,
                projection,
                element,
            } => {
                let values = indices
                    .iter()
                    .map(|index| self.term(index))
                    .collect::<Option<Vec<_>>>()?;
                let version = *self.frame.aggregates.get(root)?;
                let implicit: Vec<_> = (0..values.len() as u32).collect();
                Some(self.projected_read(version, values, projection, *element, &implicit, Some(0)))
            }
            CheckedRangeTerm::Measure { place, measure, .. } => {
                if *measure == CheckedMeasure::Length
                    && let Some(length) = self.fixed_length(place)
                {
                    return Some(length);
                }
                match (self.frame.places.get(place)?, measure) {
                    (PlaceView::Run { length, .. }, CheckedMeasure::Length) => Some(length.clone()),
                    (
                        PlaceView::Run {
                            container,
                            generation,
                            ..
                        },
                        measure,
                    ) => Some(self.world.measure(*container, *generation, *measure)),
                    (PlaceView::Segments { rows, .. }, CheckedMeasure::Length) => {
                        Some(rows.clone())
                    }
                    (
                        PlaceView::Element {
                            version,
                            indices,
                            projection,
                        },
                        measure,
                    ) => {
                        let mut path = projection.clone();
                        path.push(CheckedRangeProjection::Measure(*measure));
                        Some(self.world.read(
                            *version,
                            indices.clone(),
                            path,
                            Some(super::super::model::IntegerType::U64),
                        ))
                    }
                    _ => None,
                }
            }
            CheckedRangeTerm::SegmentLength { place, segment } => {
                let row = self.term(segment)?;
                match self.frame.places.get(place)?.clone() {
                    PlaceView::Segments {
                        container,
                        generation,
                        rows,
                        ..
                    } => {
                        self.within(&row, &rows);
                        Some(self.world.segment_length(container, generation, row))
                    }
                    PlaceView::Element {
                        version,
                        mut indices,
                        mut projection,
                    } => {
                        projection.push(CheckedRangeProjection::Index(indices.len() as u32));
                        indices.push(row);
                        projection.push(CheckedRangeProjection::Measure(CheckedMeasure::Length));
                        Some(self.projected_read(
                            version,
                            indices,
                            &projection,
                            super::super::model::IntegerType::U64,
                            &[],
                            None,
                        ))
                    }
                    _ => None,
                }
            }
            CheckedRangeTerm::Read {
                place,
                shape,
                indices,
                projection,
                element,
                implicit_indices,
                guarded_from,
            } => {
                let mut values = Vec::with_capacity(indices.len());
                for index in indices {
                    values.push(self.term(index)?);
                }
                match (self.frame.places.get(place)?.clone(), shape) {
                    (
                        PlaceView::Run {
                            version,
                            prefix,
                            offset,
                            length,
                            ..
                        },
                        CheckedRangeShape::Run,
                    ) => {
                        let index = values.first()?;
                        let length = self.fixed_length(place).unwrap_or(length);
                        self.within(index, &length);
                        let projection =
                            super::world::shift_projection(projection, 0, prefix.len());
                        let implicit: Vec<_> = implicit_indices
                            .iter()
                            .map(|index| *index + prefix.len() as u32)
                            .collect();
                        let mut selected = prefix;
                        selected.push(offset.plus(index)?);
                        selected.extend_from_slice(&values[1..]);
                        Some(self.projected_read(
                            version,
                            selected,
                            &projection,
                            *element,
                            &implicit,
                            *guarded_from,
                        ))
                    }
                    (
                        PlaceView::Segments {
                            container,
                            version,
                            generation,
                            rows,
                        },
                        CheckedRangeShape::Segments,
                    ) => {
                        let row = values.first()?;
                        let index = values.get(1)?;
                        self.within(row, &rows);
                        let length = self
                            .world
                            .segment_length(container, generation, row.clone());
                        self.within(index, &length);
                        Some(self.projected_read(
                            version,
                            values,
                            projection,
                            *element,
                            implicit_indices,
                            *guarded_from,
                        ))
                    }
                    (
                        PlaceView::Element {
                            version,
                            indices: prefix,
                            projection: mut path,
                        },
                        _,
                    ) => {
                        let base = prefix.len();
                        let count = match shape {
                            CheckedRangeShape::Run => 1,
                            CheckedRangeShape::Segments => 2,
                        };
                        for position in 0..count {
                            path.push(CheckedRangeProjection::Index((base + position) as u32));
                        }
                        let guarded = guarded_from.map(|at| at + path.len());
                        path.extend(super::world::shift_projection(projection, 0, base));
                        let mut implicit: Vec<_> = implicit_indices
                            .iter()
                            .map(|index| *index + base as u32)
                            .collect();
                        if let Some(length) = self.fixed_length(place) {
                            self.within(values.first()?, &length);
                            implicit.push(base as u32);
                        }
                        let mut indices = prefix;
                        indices.extend(values);
                        Some(
                            self.projected_read(
                                version, indices, &path, *element, &implicit, guarded,
                            ),
                        )
                    }
                    _ => None,
                }
            }
            CheckedRangeTerm::Sum { terms, constant } => {
                let mut sum = Linear::constant(*constant);
                for (weight, part) in terms {
                    sum = sum.plus(&self.term(part)?.scaled(*weight)?)?;
                }
                Some(sum)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn projected_read(
        &mut self,
        version: VersionId,
        indices: Vec<Linear>,
        projection: &[CheckedRangeProjection],
        element: super::super::model::IntegerType,
        implicit_indices: &[u32],
        guarded_from: Option<usize>,
    ) -> Linear {
        let mut depth = projection
            .iter()
            .find_map(|step| match step {
                CheckedRangeProjection::Index(position) => Some(*position as usize),
                _ => None,
            })
            .unwrap_or(indices.len());
        for (at, step) in projection.iter().enumerate() {
            if let CheckedRangeProjection::Index(position) = step {
                let position = *position as usize;
                depth = position + 1;
                if implicit_indices.contains(&(position as u32)) {
                    continue;
                }
                let mut length_path = projection[..at].to_vec();
                length_path.push(CheckedRangeProjection::Measure(CheckedMeasure::Length));
                let length = self.world.read(
                    version,
                    indices[..position].to_vec(),
                    length_path,
                    Some(super::super::model::IntegerType::U64),
                );
                self.within(&indices[position], &length);
                depth = position + 1;
            }
            if let CheckedRangeProjection::Payload {
                variant, variants, ..
            } = step
            {
                let mut tag_path = projection[..at].to_vec();
                tag_path.push(CheckedRangeProjection::Tag(*variants));
                let tag = self
                    .world
                    .read(version, indices[..depth].to_vec(), tag_path, None);
                let guards = if guarded_from.is_some_and(|start| at >= start) {
                    &mut self.guards
                } else {
                    &mut self.bounds
                };
                guards.push(Literal::new(
                    tag,
                    Relation::Equal,
                    Linear::constant(*variant as i128),
                ));
            }
        }
        self.world
            .read(version, indices, projection.to_vec(), Some(element))
    }

    fn within(&mut self, index: &Linear, length: &Linear) {
        self.bounds.push(Literal::new(
            index.clone(),
            Relation::GreaterEqual,
            Linear::constant(0),
        ));
        self.bounds
            .push(Literal::new(index.clone(), Relation::Less, length.clone()));
    }

    fn relation(&mut self, relation: &CheckedRangeRelation) -> Option<Literal> {
        let left = self.term(&relation.left)?;
        let right = self.term(&relation.right)?;
        Some(Literal::new(left, comparison(relation.comparison), right))
    }
}

pub(super) const fn comparison(comparison: RangeComparison) -> Relation {
    match comparison {
        RangeComparison::Equal => Relation::Equal,
        RangeComparison::NotEqual => Relation::NotEqual,
        RangeComparison::Less => Relation::Less,
        RangeComparison::LessEqual => Relation::LessEqual,
        RangeComparison::Greater => Relation::Greater,
        RangeComparison::GreaterEqual => Relation::GreaterEqual,
    }
}

/// An empty range contributes no tuple and no projected reads to a problem.
/// Compare affine endpoints before forming the clause's guards/conclusions;
/// fresh binders preserve dependencies between two written ranges.
pub(super) fn vacuous(world: &mut World, clause: &CheckedRangeClause, frame: &Frame) -> bool {
    let binders: Vec<_> = clause.binders.iter().map(|_| world.opaque(None)).collect();
    let mut former = Former {
        world,
        frame,
        binders: &binders,
        iterations: &[],
        bounds: Vec::new(),
        guards: Vec::new(),
        fixed_lengths: BTreeSet::new(),
    };
    clause.binders.iter().any(|binder| {
        let Some(start) = former.term(&binder.start) else {
            return false;
        };
        let Some(end) = former.term(&binder.end) else {
            return false;
        };
        end.minus(&start)
            .is_some_and(|width| width.is_constant() && width.constant <= 0)
    })
}

/// Forms `clause` in `frame` at the tuple `binders`, or `None` where the
/// frame cannot view one of its places.
pub(super) fn form(
    world: &mut World,
    clause: &CheckedRangeClause,
    frame: &Frame,
    binders: &[Linear],
    iterations: &[Linear],
) -> Option<Formed> {
    let mut former = Former {
        world,
        frame,
        binders,
        iterations,
        bounds: Vec::new(),
        guards: Vec::new(),
        fixed_lengths: BTreeSet::new(),
    };
    let mut premises = Vec::new();
    for (position, binder) in clause.binders.iter().enumerate() {
        let value = binders.get(position)?.clone();
        let start = former.term(&binder.start)?;
        let end = former.term(&binder.end)?;
        premises.push(Literal::new(value.clone(), Relation::GreaterEqual, start));
        premises.push(Literal::new(value, Relation::Less, end));
    }
    let mut conditions = Vec::new();
    for guard in &clause.guards {
        let literal = former.relation(guard)?;
        if former.guards.is_empty() {
            premises.push(literal);
        } else {
            conditions.push(Rule {
                guards: std::mem::take(&mut former.guards),
                conclusions: vec![literal],
            });
        }
    }
    let mut conclusions = Vec::new();
    for conclusion in &clause.conclusions {
        let literal = former.relation(conclusion)?;
        conclusions.push(Rule {
            guards: std::mem::take(&mut former.guards),
            conclusions: vec![literal],
        });
    }
    premises.append(&mut former.bounds);
    // An expanded equality repeats the outer read's bounds in every leaf.
    // Each conclusion needs the conjunction once; repeated literals otherwise
    // cause a full theory probe per leaf per conclusion during saturation.
    let mut seen = BTreeSet::new();
    premises.retain(|literal| seen.insert(literal.clone()));
    Some(Formed {
        type_facts: frame.type_facts(former.world, &former.fixed_lengths),
        premises,
        conditions,
        conclusions,
    })
}

/// One read of a clause whose indices name bound variables directly.
struct Trigger {
    place: CheckedRangePlace,
    shape: CheckedRangeShape,
    /// Per index position: the bound variable it names, or `None`.
    positions: Vec<Option<u32>>,
    projection: Vec<CheckedRangeProjection>,
    /// Only an overflowing template uses the original aggregate prefix. It
    /// must still report capacity when a later, unmaterialized field is read.
    descendants: bool,
}

fn collect_triggers(term: &CheckedRangeTerm, out: &mut Vec<Trigger>, overflowing: bool) {
    match term {
        CheckedRangeTerm::Read {
            place,
            shape,
            indices,
            projection,
            implicit_indices,
            guarded_from,
            ..
        } => {
            let descendants = overflowing && guarded_from.is_some();
            let count = if descendants {
                implicit_indices
                    .first()
                    .map_or(indices.len(), |at| *at as usize)
            } else {
                indices.len()
            };
            let positions: Vec<Option<u32>> = indices[..count]
                .iter()
                .map(|index| match index {
                    CheckedRangeTerm::Bound(position) => Some(*position),
                    _ => None,
                })
                .collect();
            if positions.iter().any(Option::is_some) {
                out.push(Trigger {
                    place: place.clone(),
                    shape: *shape,
                    positions,
                    projection: if descendants {
                        projection[..guarded_from.unwrap()].to_vec()
                    } else {
                        projection.clone()
                    },
                    descendants,
                });
            }
            for index in indices {
                collect_triggers(index, out, overflowing);
            }
        }
        CheckedRangeTerm::SegmentLength { segment, .. } => {
            collect_triggers(segment, out, overflowing)
        }
        CheckedRangeTerm::Sum { terms, .. } => {
            for (_, part) in terms {
                collect_triggers(part, out, overflowing);
            }
        }
        CheckedRangeTerm::Constant(_)
        | CheckedRangeTerm::ConstGeneric { .. }
        | CheckedRangeTerm::Bound(_)
        | CheckedRangeTerm::Iteration(_)
        | CheckedRangeTerm::Value(_)
        | CheckedRangeTerm::ValueProjection { .. }
        | CheckedRangeTerm::Measure { .. } => {}
    }
}

/// The largest number of atoms one problem may reach.
pub(super) use super::super::range_facts::MAX_RANGE_ATOMS as MAX_ATOMS;
/// The largest number of instances one fact contributes to one problem.
pub(super) use super::super::range_facts::MAX_RANGE_INSTANCES as MAX_INSTANCES;

/// One obligation's problem under construction, over world atoms.
#[derive(Default)]
pub(super) struct Query {
    /// Each substituted descriptor equality belongs to the query once, across
    /// its owed clause, automatic instances and written instances.
    pub(super) type_facts: BTreeSet<Literal>,
    /// Terms of the whole owed conjunction count toward the problem and
    /// seed instantiation, without assuming its other conclusions.
    pub(super) support: Vec<Literal>,
    pub(super) units: Vec<Literal>,
    pub(super) choices: Vec<Vec<Vec<Literal>>>,
    pub(super) rules: Vec<Rule>,
}

impl Query {
    fn add_instance(
        &mut self,
        world: &mut World,
        fact: &Fact,
        binders: &[Linear],
        iterations: &[Linear],
        atoms: &mut BTreeSet<AtomId>,
    ) {
        let Some(formed) = form(world, &fact.clause, &fact.frame, binders, iterations) else {
            return;
        };
        for literal in formed.type_facts {
            if self.type_facts.insert(literal.clone()) {
                collect_literal(&literal, atoms);
            }
        }
        for mut conclusion in formed.conclusions {
            conclusion.guards.extend(formed.premises.iter().cloned());
            for literal in
                conclusion
                    .guards
                    .iter()
                    .chain(&conclusion.conclusions)
                    .chain(formed.conditions.iter().flat_map(|condition| {
                        condition.guards.iter().chain(&condition.conclusions)
                    }))
            {
                collect_literal(literal, atoms);
            }
            if formed.conditions.is_empty() {
                self.rules.push(conclusion);
            } else {
                // (B and every (G => C)) => Q is the disjunction of
                // !B, each (G and !C), and Q. Keep it in the existing
                // choice representation so a violated conditional premise
                // exposes the tag that lets another instance supply C.
                self.choices
                    .push(conditional_instance(&conclusion, &formed.conditions));
            }
        }
    }
}

/// An implication with conditional premises, in the solver's existing
/// disjunction-of-conjunctions representation. Each formed conclusion is one
/// literal, and each condition may assert a conjunction.
fn conditional_instance(conclusion: &Rule, conditions: &[Rule]) -> Vec<Vec<Literal>> {
    let mut alternatives: Vec<_> = conclusion
        .guards
        .iter()
        .flat_map(Literal::negation)
        .map(|literal| vec![literal])
        .collect();
    for condition in conditions {
        for negation in condition.conclusions.iter().flat_map(Literal::negation) {
            let mut violated = condition.guards.clone();
            violated.push(negation);
            alternatives.push(violated);
        }
    }
    alternatives.push(conclusion.conclusions.clone());
    alternatives
}

/// Judges one query with the given facts active and the given written
/// instances, by the fixed procedure of [`super::solver`].
pub(super) fn judge(
    world: &mut World,
    facts: &[Fact],
    active: &[FactId],
    written: &[(FactId, Vec<Linear>, Vec<Linear>)],
    mut query: Query,
) -> Result<Verdict, Capacity> {
    let mut atoms: BTreeSet<AtomId> = BTreeSet::new();
    let mut expanded: BTreeSet<AtomId> = BTreeSet::new();
    for literal in query
        .units
        .iter()
        .chain(&query.type_facts)
        .chain(&query.support)
        .chain(query.choices.iter().flatten().flatten())
        .chain(
            query
                .rules
                .iter()
                .flat_map(|rule| rule.guards.iter().chain(&rule.conclusions)),
        )
    {
        collect_literal(literal, &mut atoms);
    }
    expand(world, &mut atoms, &mut expanded, &mut query)?;
    // Freeze the read set before each round. All facts see the same set,
    // regardless of their order; reads formed in round two never trigger a
    // third round. Keep each fact/tuple once across both rounds [RANGE-3].
    let mut seen = BTreeSet::new();
    let mut counts = BTreeMap::new();
    for _ in 0..2 {
        let ground: Vec<(VersionId, Vec<Linear>, Vec<CheckedRangeProjection>)> = atoms
            .iter()
            .filter_map(|atom| match &world.atoms[*atom as usize].def {
                AtomDef::Read {
                    version,
                    indices,
                    projection,
                } => Some((*version, indices.clone(), projection.clone())),
                _ => None,
            })
            .collect();
        let mut instances = Vec::new();
        for fact_id in active {
            let fact = &facts[*fact_id as usize];
            let mut triggers = Vec::new();
            if vacuous(world, &fact.clause, &fact.frame) {
                continue;
            }
            let mut projections = BTreeMap::new();
            for relation in fact
                .clause
                .relations()
                .filter(|relation| relation.projected)
            {
                *projections.entry(&relation.node).or_insert(0_usize) += 1;
            }
            for binder in &fact.clause.binders {
                collect_triggers(&binder.start, &mut triggers, false);
                collect_triggers(&binder.end, &mut triggers, false);
            }
            for relation in fact.clause.relations() {
                let overflowing = projections
                    .get(&relation.node)
                    .is_some_and(|count| *count > MAX_ATOMS);
                collect_triggers(&relation.left, &mut triggers, overflowing);
                collect_triggers(&relation.right, &mut triggers, overflowing);
            }
            let mut candidates: Vec<BTreeSet<Linear>> =
                vec![BTreeSet::new(); fact.clause.binders.len()];
            for trigger in &triggers {
                let Some(view) = fact.frame.places.get(&trigger.place) else {
                    continue;
                };
                for (version, indices, projection) in &ground {
                    let expected = match view {
                        PlaceView::Run { prefix, .. } => {
                            super::world::shift_projection(&trigger.projection, 0, prefix.len())
                        }
                        PlaceView::Element {
                            indices: prefix,
                            projection,
                            ..
                        } => {
                            let mut path = projection.clone();
                            let count = match trigger.shape {
                                CheckedRangeShape::Run => 1,
                                CheckedRangeShape::Segments => 2,
                            };
                            for position in 0..count {
                                path.push(CheckedRangeProjection::Index(
                                    (prefix.len() + position) as u32,
                                ));
                            }
                            path.extend(super::world::shift_projection(
                                &trigger.projection,
                                0,
                                prefix.len(),
                            ));
                            path
                        }
                        _ => trigger.projection.clone(),
                    };
                    let matches = if trigger.descendants {
                        projection.len() > expected.len()
                            && projection.starts_with(&expected)
                            && !matches!(
                                projection.last(),
                                Some(CheckedRangeProjection::Measure(_))
                            )
                    } else {
                        expected == *projection
                    };
                    if matches {
                        match_trigger(view, trigger, *version, indices, &mut candidates);
                    }
                }
            }
            // [RANGE-3] the ceiling counts the instances the fact forms: the
            // product over its binders, which is zero when one has no value.
            // Reads accumulate across rounds, so this also counts the union
            // of both rounds, not a fresh allowance of 256 for each.
            let formed = candidates
                .iter()
                .try_fold(1_usize, |product, values| product.checked_mul(values.len()));
            if formed.is_none_or(|formed| formed > MAX_INSTANCES) {
                return Err(Capacity::Instances);
            }
            let mut tuples: Vec<Vec<Linear>> = vec![Vec::new()];
            for values in &candidates {
                let mut next = Vec::new();
                for tuple in &tuples {
                    for value in values {
                        let mut extended = tuple.clone();
                        extended.push(value.clone());
                        next.push(extended);
                    }
                }
                tuples = next;
            }
            for tuple in tuples {
                if seen.insert((*fact_id, tuple.clone(), Vec::new())) {
                    count_instance(&mut counts, *fact_id)?;
                    instances.push((*fact_id, tuple));
                }
            }
        }
        for (fact_id, binders) in instances {
            let fact = &facts[fact_id as usize];
            query.add_instance(world, fact, &binders, &[], &mut atoms);
        }
        expand(world, &mut atoms, &mut expanded, &mut query)?;
    }
    // [RANGE-4] Written instances join the automatic instances; their reads
    // do not seed additional automatic rounds.
    for (fact_id, binders, iterations) in written {
        let fact = &facts[*fact_id as usize];
        if !vacuous(world, &fact.clause, &fact.frame)
            && seen.insert((*fact_id, binders.clone(), iterations.clone()))
        {
            count_instance(&mut counts, *fact_id)?;
            query.add_instance(world, fact, binders, iterations, &mut atoms);
        }
    }
    expand(world, &mut atoms, &mut expanded, &mut query)?;
    // Each typed atom's range.
    for atom in &atoms {
        if let Some(ty) = world.atoms[*atom as usize].ty {
            let (low, high) = super::walk::integer_range(ty);
            let value = Linear::atom(*atom);
            query.units.push(Literal::new(
                value.clone(),
                Relation::GreaterEqual,
                Linear::constant(low),
            ));
            query.units.push(Literal::new(
                value,
                Relation::LessEqual,
                Linear::constant(high),
            ));
        }
    }
    localize(world, &atoms, query).judge()
}

/// Charge one tuple, irrespective of the number of its conclusions.
fn count_instance(counts: &mut BTreeMap<FactId, usize>, fact: FactId) -> Result<(), Capacity> {
    let count = counts.entry(fact).or_default();
    *count += 1;
    if *count > MAX_INSTANCES {
        return Err(Capacity::Instances);
    }
    Ok(())
}

fn match_trigger(
    view: &PlaceView,
    trigger: &Trigger,
    version: VersionId,
    indices: &[Linear],
    candidates: &mut [BTreeSet<Linear>],
) {
    let (view_version, prefix, offset) = match view {
        PlaceView::Run {
            version,
            prefix,
            offset,
            ..
        } => (*version, prefix.as_slice(), Some(offset)),
        PlaceView::Segments { version, .. } => (*version, &[][..], None),
        PlaceView::Element {
            version, indices, ..
        } => (*version, indices.as_slice(), None),
        PlaceView::Unknown => return,
    };
    let expected = prefix.len() + trigger.positions.len();
    if view_version != version
        || indices.len() < expected
        || (!trigger.descendants && indices.len() != expected)
    {
        return;
    }
    if indices[..prefix.len()] != *prefix {
        return;
    }
    for (position, binder) in trigger.positions.iter().enumerate() {
        let Some(binder) = binder else {
            continue;
        };
        let index = &indices[prefix.len() + position];
        let value = match offset {
            // The last position of a run view is relative to its offset.
            Some(offset) if position == 0 => index.minus(offset),
            _ => Some(index.clone()),
        };
        if let (Some(value), Some(set)) = (value, candidates.get_mut(*binder as usize)) {
            set.insert(value);
        }
    }
}

fn collect_literal(literal: &Literal, atoms: &mut BTreeSet<AtomId>) {
    for (atom, _) in literal.left.terms.iter().chain(literal.right.terms.iter()) {
        atoms.insert(*atom);
    }
}

fn collect_linear(linear: &Linear, atoms: &mut BTreeSet<AtomId>) {
    for (atom, _) in &linear.terms {
        atoms.insert(*atom);
    }
}

/// Adds every definition the problem's atoms carry: each read of a written
/// or joined version, and each joined value, as a choice over its cases,
/// until no atom is left undefined.
fn expand(
    world: &mut World,
    atoms: &mut BTreeSet<AtomId>,
    expanded: &mut BTreeSet<AtomId>,
    query: &mut Query,
) -> Result<(), Capacity> {
    loop {
        let Some(atom) = atoms.iter().copied().find(|atom| !expanded.contains(atom)) else {
            return Ok(());
        };
        expanded.insert(atom);
        if atoms.len() > MAX_ATOMS {
            return Err(Capacity::Atoms);
        }
        let def = world.atoms[atom as usize].def.clone();
        let ty = world.atoms[atom as usize].ty;
        let value = Linear::atom(atom);
        let mut alternatives: Vec<Vec<Literal>> = Vec::new();
        match def {
            AtomDef::Opaque | AtomDef::Measure => {}
            AtomDef::SegmentLength { row, .. } => collect_linear(&row, atoms),
            AtomDef::Read {
                version,
                indices,
                projection,
            } => {
                for index in &indices {
                    collect_linear(index, atoms);
                }
                if let Some(CheckedRangeProjection::Tag(variants)) = projection.last() {
                    query.units.push(Literal::new(
                        value.clone(),
                        Relation::GreaterEqual,
                        Linear::constant(0),
                    ));
                    query.units.push(Literal::new(
                        value.clone(),
                        Relation::Less,
                        Linear::constant(*variants as i128),
                    ));
                }
                match world.versions[version as usize].def.clone() {
                    VersionDef::Initial | VersionDef::Fresh => {}
                    VersionDef::Copied { source, arity } => {
                        let mut path = source.projection;
                        path.extend((0..arity).map(|position| {
                            CheckedRangeProjection::Index((source.indices.len() + position) as u32)
                        }));
                        path.extend(super::world::shift_projection(
                            &projection,
                            0,
                            source.indices.len(),
                        ));
                        let selected = world.read(
                            source.version,
                            source.indices.into_iter().chain(indices).collect(),
                            path,
                            ty,
                        );
                        alternatives.push(vec![Literal::new(
                            value.clone(),
                            Relation::Equal,
                            selected,
                        )]);
                    }
                    VersionDef::Forget {
                        previous,
                        projections,
                    } => {
                        if !projections
                            .iter()
                            .any(|path| projections_overlap(path, &projection))
                        {
                            let old = world.read(previous, indices.clone(), projection.clone(), ty);
                            alternatives.push(vec![Literal::new(
                                value.clone(),
                                Relation::Equal,
                                old,
                            )]);
                        }
                    }
                    VersionDef::Write {
                        previous,
                        indices: written,
                        projection: footprint,
                        values: stored,
                    } => {
                        let old = world.read(previous, indices.clone(), projection.clone(), ty);
                        let mut hit: Vec<Literal> = indices
                            .iter()
                            .zip(&written)
                            .map(|(index, at)| {
                                Literal::new(index.clone(), Relation::Equal, at.clone())
                            })
                            .collect();
                        // Different declared fields are disjoint. An ancestor
                        // replacement overlaps every descendant, measures included.
                        if !projections_overlap(&projection, &footprint) {
                            hit.push(Literal::new(value.clone(), Relation::Equal, old.clone()));
                        } else if let Some(relative) = projection.strip_prefix(footprint.as_slice())
                        {
                            for length in (0..=relative.len()).rev() {
                                if let Some(stored) = stored.get(&relative[..length]) {
                                    let defined = match stored {
                                        Stored::Int(value) if length == relative.len() => {
                                            Some(value.clone())
                                        }
                                        Stored::Collection {
                                            container,
                                            version,
                                            generation,
                                            arity,
                                        } => {
                                            let suffix = &relative[length..];
                                            match suffix {
                                                [CheckedRangeProjection::Measure(measure)] => {
                                                    Some(world.measure(
                                                        *container,
                                                        *generation,
                                                        *measure,
                                                    ))
                                                }
                                                [
                                                    CheckedRangeProjection::Index(at),
                                                    CheckedRangeProjection::Measure(
                                                        CheckedMeasure::Length,
                                                    ),
                                                ] if *arity == 2 => Some(world.segment_length(
                                                    *container,
                                                    *generation,
                                                    indices[*at as usize].clone(),
                                                )),
                                                [CheckedRangeProjection::Index(at), ..]
                                                    if suffix.len() >= *arity =>
                                                {
                                                    let at = *at as usize;
                                                    Some(world.read(
                                                        *version,
                                                        indices[at..].to_vec(),
                                                        super::world::shift_projection(
                                                            &suffix[*arity..],
                                                            at,
                                                            0,
                                                        ),
                                                        ty,
                                                    ))
                                                }
                                                _ => None,
                                            }
                                        }
                                        Stored::Read(source) => {
                                            let mut path = source.projection.clone();
                                            path.extend(super::world::shift_projection(
                                                &relative[length..],
                                                written.len(),
                                                source.indices.len(),
                                            ));
                                            Some(
                                                world.read(
                                                    source.version,
                                                    source
                                                        .indices
                                                        .iter()
                                                        .cloned()
                                                        .chain(
                                                            indices[written.len()..]
                                                                .iter()
                                                                .cloned(),
                                                        )
                                                        .collect(),
                                                    path,
                                                    ty,
                                                ),
                                            )
                                        }
                                        _ => None,
                                    };
                                    if let Some(defined) = defined {
                                        hit.push(Literal::new(
                                            value.clone(),
                                            Relation::Equal,
                                            defined,
                                        ));
                                    }
                                    break;
                                }
                            }
                        }
                        alternatives.push(hit);
                        for (index, at) in indices.iter().zip(&written) {
                            alternatives.push(vec![
                                Literal::new(index.clone(), Relation::NotEqual, at.clone()),
                                Literal::new(value.clone(), Relation::Equal, old.clone()),
                            ]);
                        }
                    }
                    VersionDef::Join { join, versions } => {
                        let arms = world.joins[join as usize].clone();
                        for (arm, version) in arms.iter().zip(versions) {
                            let selected =
                                world.read(version, indices.clone(), projection.clone(), ty);
                            let mut alternative = arm.clone();
                            alternative.push(Literal::new(
                                value.clone(),
                                Relation::Equal,
                                selected,
                            ));
                            alternatives.push(alternative);
                        }
                    }
                }
            }
            AtomDef::Joined { join, values } => {
                let arms = world.joins[join as usize].clone();
                for (arm, selected) in arms.iter().zip(values) {
                    let mut alternative = arm.clone();
                    alternative.push(Literal::new(value.clone(), Relation::Equal, selected));
                    alternatives.push(alternative);
                }
            }
        }
        if !alternatives.is_empty() {
            for literal in alternatives.iter().flatten() {
                collect_literal(literal, atoms);
            }
            query.choices.push(alternatives);
        }
    }
}

/// Renumbers the problem's atoms densely and gives the solver each read's
/// identity for congruence. `atoms` holds every atom the query names: each
/// literal's atoms were collected before [`expand`] closed the set.
fn localize(world: &World, atoms: &BTreeSet<AtomId>, query: Query) -> Problem {
    let local: BTreeMap<AtomId, AtomId> = atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| (*atom, index as AtomId))
        .collect();
    let map_linear = |linear: &Linear| -> Linear {
        let mut terms: Vec<(AtomId, i128)> = linear
            .terms
            .iter()
            .map(|(atom, coefficient)| (local[atom], *coefficient))
            .collect();
        terms.sort_unstable();
        Linear {
            terms,
            constant: linear.constant,
        }
    };
    let map_literal = |literal: &Literal| -> Literal {
        Literal::new(
            map_linear(&literal.left),
            literal.relation,
            map_linear(&literal.right),
        )
    };
    let mut reads: BTreeMap<(VersionId, Vec<CheckedRangeProjection>), u32> = BTreeMap::new();
    let mut descriptors: BTreeMap<(ContainerId, u32), u32> = BTreeMap::new();
    let mut kinds = Vec::with_capacity(atoms.len());
    for atom in atoms {
        let kind = match &world.atoms[*atom as usize].def {
            AtomDef::Read {
                version,
                indices,
                projection,
            } => {
                let next = reads.len() as u32;
                let place = *reads.entry((*version, projection.clone())).or_insert(next);
                AtomKind::Read {
                    place,
                    indices: indices.iter().map(&map_linear).collect(),
                }
            }
            AtomDef::SegmentLength {
                container,
                generation,
                row,
            } => {
                let next = descriptors.len() as u32;
                let place = *descriptors.entry((*container, *generation)).or_insert(next);
                AtomKind::SegmentLength {
                    place,
                    segment: map_linear(row),
                }
            }
            _ => AtomKind::Plain,
        };
        kinds.push(kind);
    }
    let mut problem = Problem {
        atoms: kinds,
        ..Problem::default()
    };
    for literal in query.units.iter().chain(&query.type_facts) {
        problem.units.push(map_literal(literal));
    }
    for choice in &query.choices {
        let mut alternatives = Vec::with_capacity(choice.len());
        for alternative in choice {
            alternatives.push(alternative.iter().map(&map_literal).collect());
        }
        problem.choices.push(alternatives);
    }
    for rule in &query.rules {
        problem.rules.push(Rule {
            guards: rule.guards.iter().map(&map_literal).collect(),
            conclusions: rule.conclusions.iter().map(&map_literal).collect(),
        });
    }
    problem
}

#[cfg(test)]
mod aggregate_tests {
    use super::*;
    use crate::semantic::model::IntegerType;

    #[test]
    fn symbolic_const_clause_and_expression_share_one_typed_value() {
        use super::super::world::{State, Value};
        use crate::semantic::model::CheckedValue;
        let declaration = crate::DeclarationId::from_index(0).unwrap();
        let ty = IntegerType::U64;
        let mut world = World::default();
        let mut state = State::default();
        let Value::Int(read) = super::super::constants::value(
            &mut world,
            &mut state,
            &CheckedValue::ConstGeneric { declaration, ty },
        ) else {
            panic!("a const generic is an integer value")
        };
        let frame = Frame::default();
        let mut former = Former {
            world: &mut world,
            frame: &frame,
            binders: &[],
            iterations: &[],
            bounds: Vec::new(),
            guards: Vec::new(),
            fixed_lengths: BTreeSet::new(),
        };
        assert_eq!(
            former.term(&CheckedRangeTerm::ConstGeneric { declaration, ty }),
            Some(read.clone())
        );
        assert_ne!(
            former.term(&CheckedRangeTerm::ConstGeneric {
                declaration: crate::DeclarationId::from_index(1).unwrap(),
                ty,
            }),
            Some(read)
        );
        assert_eq!(world.atoms.len(), 2);
        assert!(world.atoms.iter().all(|atom| atom.ty == Some(ty)));
    }

    fn projected_fact(world: &mut World, count: usize) -> Fact {
        let root = CheckedRangeRoot::Result(0);
        let mut frame = Frame::default();
        frame
            .aggregates
            .insert(root, world.new_version(VersionDef::Initial));
        let node = crate::NodePath {
            components: Vec::new(),
        };
        Fact {
            frame,
            clause: CheckedRangeClause {
                declaration: crate::DeclarationId::from_index(0).unwrap(),
                name: "projected".to_owned(),
                node: node.clone(),
                binders: Vec::new(),
                guards: Vec::new(),
                conclusions: (0..count)
                    .map(|field| CheckedRangeRelation {
                        node: node.clone(),
                        left: CheckedRangeTerm::ValueProjection {
                            root,
                            indices: Vec::new(),
                            projection: vec![CheckedRangeProjection::Field(field as u32)],
                            element: IntegerType::U64,
                        },
                        comparison: RangeComparison::Equal,
                        right: CheckedRangeTerm::Constant(0),
                        projected: true,
                    })
                    .collect(),
            },
        }
    }

    #[test]
    fn projected_conclusions_belong_to_one_instance() {
        let mut world = World::default();
        let fact = projected_fact(&mut world, MAX_INSTANCES + 1);
        assert_eq!(
            judge(&mut world, &[fact], &[0], &[], Query::default()),
            Ok(Verdict::Open)
        );
    }

    #[test]
    fn expanded_array_bounds_and_type_facts_are_not_repeated_per_projection() {
        use crate::semantic::range_facts::CheckedRangeBinder;
        let mut world = World::default();
        let mut fact = projected_fact(&mut world, MAX_INSTANCES + 1);
        let place = CheckedRangePlace {
            root: CheckedRangeRoot::Result(1),
            path: Vec::new(),
            fixed_length: Some(1),
        };
        let length = world.opaque(Some(IntegerType::U64));
        fact.frame.places.insert(
            place.clone(),
            PlaceView::Run {
                container: 0,
                version: world.new_version(VersionDef::Initial),
                generation: 0,
                prefix: Vec::new(),
                offset: Linear::constant(0),
                length: length.clone(),
            },
        );
        fact.clause.binders.push(CheckedRangeBinder {
            start: CheckedRangeTerm::Constant(0),
            end: CheckedRangeTerm::Measure {
                place: place.clone(),
                measure: CheckedMeasure::Length,
                shape: CheckedRangeShape::Run,
            },
        });
        for (index, relation) in fact.clause.conclusions.iter_mut().enumerate() {
            let index = CheckedRangeTerm::Constant(index as i128);
            relation.left = CheckedRangeTerm::Read {
                place: place.clone(),
                shape: CheckedRangeShape::Run,
                indices: vec![CheckedRangeTerm::Bound(0), index.clone()],
                projection: vec![CheckedRangeProjection::Index(1)],
                element: IntegerType::U8,
                implicit_indices: vec![1],
                guarded_from: Some(0),
            };
            relation.right = CheckedRangeTerm::ValueProjection {
                root: CheckedRangeRoot::Result(0),
                indices: vec![index],
                projection: vec![CheckedRangeProjection::Index(0)],
                element: IntegerType::U8,
            };
        }
        let k = world.opaque(None);
        let formed = form(
            &mut world,
            &fact.clause,
            &fact.frame,
            std::slice::from_ref(&k),
            &[],
        )
        .unwrap();
        let expected = BTreeSet::from([Literal::new(
            length,
            Relation::Equal,
            Linear::constant(1),
        )]);
        assert_eq!(formed.type_facts, expected);
        assert_eq!(
            formed.premises,
            vec![
                Literal::new(k.clone(), Relation::GreaterEqual, Linear::constant(0)),
                Literal::new(k.clone(), Relation::Less, Linear::constant(1)),
            ],
            "every projection repeats the same two outer bounds"
        );
        let mut query = Query {
            type_facts: formed.type_facts,
            ..Query::default()
        };
        let mut atoms = BTreeSet::new();
        for literal in &query.type_facts {
            collect_literal(literal, &mut atoms);
        }
        // The owed clause and distinct instances share the descriptor fact.
        // Every instance still contributes every projection and its atoms.
        for binder in [k, world.opaque(None)] {
            query.add_instance(&mut world, &fact, &[binder], &[], &mut atoms);
        }
        assert_eq!(query.type_facts, expected);
        assert_eq!(query.rules.len(), 2 * fact.clause.conclusions.len());
        assert!(query.rules.iter().all(|rule| rule.guards.len() == 2));
        let problem = localize(&world, &atoms, query);
        assert_eq!(problem.units.len(), 1);
    }

    #[test]
    fn type_facts_follow_substituted_lengths_and_preserve_source_reads() {
        let mut world = World::default();
        let mut fact = projected_fact(&mut world, 1);
        let place = CheckedRangePlace {
            root: CheckedRangeRoot::Result(1),
            path: Vec::new(),
            fixed_length: Some(1),
        };
        let version = world.new_version(VersionDef::Initial);
        let indices = vec![Linear::constant(0)];
        let projection = vec![CheckedRangeProjection::Field(0)];
        fact.frame.places.insert(
            place.clone(),
            PlaceView::Element {
                version,
                indices: indices.clone(),
                projection: projection.clone(),
            },
        );
        let formed = form(&mut world, &fact.clause, &fact.frame, &[], &[]).unwrap();
        assert!(
            formed.type_facts.is_empty(),
            "an unused place needs no length read"
        );
        assert!(!world.atoms.iter().any(|atom| {
            matches!(&atom.def, AtomDef::Read { projection, .. }
                if projection.last() == Some(&CheckedRangeProjection::Measure(CheckedMeasure::Length)))
        }));

        fact.clause.conclusions[0].left = CheckedRangeTerm::Measure {
            place: place.clone(),
            measure: CheckedMeasure::Length,
            shape: CheckedRangeShape::Run,
        };
        let formed = form(&mut world, &fact.clause, &fact.frame, &[], &[]).unwrap();
        let mut length_path = projection;
        length_path.push(CheckedRangeProjection::Measure(CheckedMeasure::Length));
        let source_length = world.read(version, indices, length_path, Some(IntegerType::U64));
        let expected = BTreeSet::from([Literal::new(
            source_length.clone(),
            Relation::Equal,
            Linear::constant(1),
        )]);
        assert_eq!(formed.type_facts, expected);
        assert_eq!(formed.conclusions[0].conclusions[0].left, Linear::constant(1));

        fact.clause.conclusions[0].left = CheckedRangeTerm::Read {
            place: place.clone(),
            shape: CheckedRangeShape::Run,
            indices: vec![CheckedRangeTerm::Constant(0)],
            projection: Vec::new(),
            element: IntegerType::U64,
            implicit_indices: Vec::new(),
            guarded_from: None,
        };
        let formed = form(&mut world, &fact.clause, &fact.frame, &[], &[]).unwrap();
        assert_eq!(
            formed.type_facts, expected,
            "an element view substitutes its bound"
        );

        // A copied collection's Run length is the same source descriptor read.
        fact.frame.places.insert(
            place.clone(),
            PlaceView::Run {
                container: 0,
                version: world.new_version(VersionDef::Initial),
                generation: 0,
                prefix: Vec::new(),
                offset: Linear::constant(0),
                length: source_length,
            },
        );
        let formed = form(&mut world, &fact.clause, &fact.frame, &[], &[]).unwrap();
        assert_eq!(
            formed.type_facts, expected,
            "a copied run substitutes its bound without losing the source's length"
        );
        let PlaceView::Run { length, .. } = fact.frame.places.get_mut(&place).unwrap() else {
            unreachable!()
        };
        *length = Linear::constant(1);
        let formed = form(&mut world, &fact.clause, &fact.frame, &[], &[]).unwrap();
        assert!(
            formed.type_facts.is_empty(),
            "a constant descriptor needs no bridge"
        );
    }

    #[test]
    fn a_vacuous_fixed_length_adds_no_descriptor_read() {
        use crate::semantic::range_facts::CheckedRangeBinder;
        let mut world = World::default();
        let mut fact = projected_fact(&mut world, 1);
        let place = CheckedRangePlace {
            root: CheckedRangeRoot::Result(1),
            path: Vec::new(),
            fixed_length: Some(0),
        };
        fact.frame.places.insert(
            place.clone(),
            PlaceView::Element {
                version: world.new_version(VersionDef::Initial),
                indices: vec![Linear::constant(0)],
                projection: Vec::new(),
            },
        );
        fact.clause.binders.push(CheckedRangeBinder {
            start: CheckedRangeTerm::Constant(0),
            end: CheckedRangeTerm::Measure {
                place,
                measure: CheckedMeasure::Length,
                shape: CheckedRangeShape::Run,
            },
        });
        assert!(vacuous(&mut world, &fact.clause, &fact.frame));
        assert!(
            world
                .atoms
                .iter()
                .all(|atom| !matches!(atom.def, AtomDef::Read { .. }))
        );
    }

    #[test]
    fn written_instance_ceiling_counts_tuples_not_conclusions() {
        use crate::semantic::range_facts::CheckedRangeBinder;
        for count in [MAX_INSTANCES, MAX_INSTANCES + 1] {
            let mut world = World::default();
            let mut fact = projected_fact(&mut world, 2);
            fact.clause.binders.push(CheckedRangeBinder {
                start: CheckedRangeTerm::Constant(0),
                end: CheckedRangeTerm::Constant(1000),
            });
            let written: Vec<_> = (0..count)
                .map(|index| (0, vec![Linear::constant(index as i128)], Vec::new()))
                .collect();
            assert_eq!(
                judge(&mut world, &[fact], &[], &written, Query::default()),
                if count == MAX_INSTANCES {
                    Ok(Verdict::Open)
                } else {
                    Err(Capacity::Instances)
                }
            );
        }
    }

    #[test]
    fn a_vacuous_expansion_adds_neither_instances_nor_atoms() {
        use crate::semantic::range_facts::CheckedRangeBinder;
        let mut world = World::default();
        let mut fact = projected_fact(&mut world, MAX_ATOMS + 1);
        fact.clause.binders.push(CheckedRangeBinder {
            start: CheckedRangeTerm::Constant(0),
            end: CheckedRangeTerm::Constant(0),
        });
        let written: Vec<_> = (0..=MAX_INSTANCES)
            .map(|index| (0, vec![Linear::constant(index as i128)], Vec::new()))
            .collect();
        assert_eq!(
            judge(&mut world, &[fact], &[0], &written, Query::default()),
            Ok(Verdict::Open)
        );
        assert!(
            world
                .atoms
                .iter()
                .all(|atom| !matches!(atom.def, AtomDef::Read { .. }))
        );
    }

    #[test]
    fn every_projected_conclusion_counts_toward_the_atom_ceiling() {
        // The base context leaves exactly one atom for the fact's projections.
        // Support terms seed the problem without assuming a contradiction.
        for count in [1, 2] {
            let mut world = World::default();
            let mut query = Query::default();
            for _ in 0..MAX_ATOMS - 1 {
                query.support.push(Literal::new(
                    world.opaque(None),
                    Relation::Equal,
                    Linear::constant(0),
                ));
            }
            let fact = projected_fact(&mut world, count);
            assert_eq!(
                judge(&mut world, &[fact], &[0], &[], query),
                if count == 1 {
                    Ok(Verdict::Open)
                } else {
                    Err(Capacity::Atoms)
                }
            );
        }
    }

    #[test]
    fn a_read_of_an_unmaterialized_field_still_reports_capacity() {
        use crate::semantic::range_facts::CheckedRangeBinder;
        let mut world = World::default();
        let mut fact = projected_fact(&mut world, MAX_ATOMS + 1);
        let place = CheckedRangePlace {
            root: CheckedRangeRoot::Result(0),
            path: Vec::new(),
            fixed_length: None,
        };
        let version = world.new_version(VersionDef::Initial);
        fact.frame.places.insert(
            place.clone(),
            PlaceView::Run {
                container: 0,
                version,
                generation: 0,
                prefix: Vec::new(),
                offset: Linear::constant(0),
                length: Linear::constant(1),
            },
        );
        fact.clause.binders.push(CheckedRangeBinder {
            start: CheckedRangeTerm::Constant(0),
            end: CheckedRangeTerm::Constant(1),
        });
        for relation in &mut fact.clause.conclusions {
            let CheckedRangeTerm::ValueProjection {
                projection,
                element,
                ..
            } = &relation.left
            else {
                unreachable!()
            };
            relation.left = CheckedRangeTerm::Read {
                place: place.clone(),
                shape: CheckedRangeShape::Run,
                indices: vec![CheckedRangeTerm::Bound(0)],
                projection: projection.clone(),
                element: *element,
                implicit_indices: Vec::new(),
                guarded_from: Some(0),
            };
        }
        for (projection, expected) in [
            (
                vec![CheckedRangeProjection::Field(9999)],
                Err(Capacity::Atoms),
            ),
            (
                vec![
                    CheckedRangeProjection::Field(9999),
                    CheckedRangeProjection::Measure(CheckedMeasure::Length),
                ],
                Ok(Verdict::Open),
            ),
            (Vec::new(), Ok(Verdict::Open)),
        ] {
            let read = world.read(
                version,
                vec![Linear::constant(0)],
                projection,
                Some(IntegerType::U64),
            );
            let query = Query {
                support: vec![Literal::new(read, Relation::Equal, Linear::constant(0))],
                ..Query::default()
            };
            assert_eq!(
                judge(&mut world, &[fact.clone()], &[0], &[], query),
                expected
            );
        }
    }

    #[test]
    fn bool_values_keep_distinct_stable_declaration_order_tags() {
        use super::super::world::{Cond, Value};
        let mut world = World::default();
        for (truth, expected) in [(true, 0), (false, 1)] {
            let Value::Bool(_, tag) = world.boolean(Cond::Constant(truth)) else {
                unreachable!()
            };
            assert_eq!(tag, Linear::constant(expected));
        }
        let first = world.boolean(Cond::Unknown);
        assert_eq!(first, first.clone(), "copying preserves the value identity");
        assert_ne!(
            first,
            world.boolean(Cond::Unknown),
            "unrelated Bool values are distinct"
        );
    }

    #[test]
    fn enum_payload_domains_do_not_enter_the_clause_premises() {
        let mut world = World::default();
        let mut fact = projected_fact(&mut world, 1);
        let root = CheckedRangeRoot::Result(0);
        fact.clause.conclusions = (0..2)
            .map(|variant| CheckedRangeRelation {
                node: fact.clause.node.clone(),
                left: CheckedRangeTerm::ValueProjection {
                    root,
                    indices: Vec::new(),
                    projection: vec![CheckedRangeProjection::Payload {
                        variant,
                        field: 0,
                        variants: 2,
                    }],
                    element: IntegerType::U64,
                },
                comparison: RangeComparison::Equal,
                right: CheckedRangeTerm::Constant(3),
                projected: true,
            })
            .collect();
        let formed = form(&mut world, &fact.clause, &fact.frame, &[], &[]).unwrap();
        assert!(
            formed.premises.is_empty(),
            "mutually exclusive tags must not make the whole clause vacuous"
        );
        assert_eq!(formed.conclusions.len(), 2);
        for (variant, rule) in formed.conclusions.iter().enumerate() {
            assert_eq!(rule.guards.len(), 1);
            assert_eq!(rule.guards[0].right, Linear::constant(variant as i128));
        }
    }
}
