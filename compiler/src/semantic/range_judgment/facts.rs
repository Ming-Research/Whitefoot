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
}

/// One active fact.
#[derive(Clone, Debug)]
pub(super) struct Fact {
    pub(super) clause: CheckedRangeClause,
    pub(super) frame: Frame,
}

/// One clause formed at one tuple.
pub(super) struct Formed {
    /// Each bound variable's range, the clause's guards, and every read's
    /// selection of an existing element.
    pub(super) premises: Vec<Literal>,
    pub(super) conclusions: Vec<Literal>,
}

/// Forms clause terms in one frame at one tuple.
struct Former<'world> {
    world: &'world mut World,
    frame: &'world Frame,
    binders: &'world [Linear],
    iterations: &'world [Linear],
    bounds: Vec<Literal>,
}

impl Former<'_> {
    fn term(&mut self, term: &CheckedRangeTerm) -> Option<Linear> {
        match term {
            CheckedRangeTerm::Constant(value) => Some(Linear::constant(*value)),
            CheckedRangeTerm::Bound(position) => self.binders.get(*position as usize).cloned(),
            CheckedRangeTerm::Iteration(position) => {
                self.iterations.get(*position as usize).cloned()
            }
            CheckedRangeTerm::Value(root) => self.frame.values.get(root).cloned(),
            CheckedRangeTerm::Measure { place, measure, .. } => {
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
                        self.within(index, &length);
                        let projection =
                            super::world::shift_projection(projection, 0, prefix.len());
                        let mut selected = prefix;
                        selected.push(offset.plus(index)?);
                        selected.extend_from_slice(&values[1..]);
                        Some(self.projected_read(version, selected, &projection, *element))
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
                        Some(self.projected_read(version, values, projection, *element))
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
                        path.extend(super::world::shift_projection(projection, 0, base));
                        let mut indices = prefix;
                        indices.extend(values);
                        Some(self.projected_read(version, indices, &path, *element))
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

    fn projected_read(
        &mut self,
        version: VersionId,
        indices: Vec<Linear>,
        projection: &[CheckedRangeProjection],
        element: super::super::model::IntegerType,
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
                self.bounds.push(Literal::new(
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
    };
    let mut premises = Vec::new();
    for (position, binder) in clause.binders.iter().enumerate() {
        let value = binders.get(position)?.clone();
        let start = former.term(&binder.start)?;
        let end = former.term(&binder.end)?;
        premises.push(Literal::new(value.clone(), Relation::GreaterEqual, start));
        premises.push(Literal::new(value, Relation::Less, end));
    }
    for guard in &clause.guards {
        premises.push(former.relation(guard)?);
    }
    let mut conclusions = Vec::new();
    for conclusion in &clause.conclusions {
        conclusions.push(former.relation(conclusion)?);
    }
    premises.append(&mut former.bounds);
    Some(Formed {
        premises,
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
}

fn collect_triggers(term: &CheckedRangeTerm, out: &mut Vec<Trigger>) {
    match term {
        CheckedRangeTerm::Read {
            place,
            shape,
            indices,
            projection,
            ..
        } => {
            let positions: Vec<Option<u32>> = indices
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
                    projection: projection.clone(),
                });
            }
            for index in indices {
                collect_triggers(index, out);
            }
        }
        CheckedRangeTerm::SegmentLength { segment, .. } => collect_triggers(segment, out),
        CheckedRangeTerm::Sum { terms, .. } => {
            for (_, part) in terms {
                collect_triggers(part, out);
            }
        }
        CheckedRangeTerm::Constant(_)
        | CheckedRangeTerm::Bound(_)
        | CheckedRangeTerm::Iteration(_)
        | CheckedRangeTerm::Value(_)
        | CheckedRangeTerm::Measure { .. } => {}
    }
}

/// The largest number of instances one fact contributes to one problem.
pub(super) const MAX_INSTANCES: usize = 256;
/// The largest number of atoms one problem may reach.
pub(super) const MAX_ATOMS: usize = 4096;

/// One obligation's problem under construction, over world atoms.
#[derive(Default)]
pub(super) struct Query {
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
        for literal in formed.premises.iter().chain(&formed.conclusions) {
            collect_literal(literal, atoms);
        }
        self.rules.push(Rule {
            guards: formed.premises,
            conclusions: formed.conclusions,
        });
    }
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
            for binder in &fact.clause.binders {
                collect_triggers(&binder.start, &mut triggers);
                collect_triggers(&binder.end, &mut triggers);
            }
            for relation in fact.clause.relations() {
                collect_triggers(&relation.left, &mut triggers);
                collect_triggers(&relation.right, &mut triggers);
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
                    if expected == *projection {
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
                if seen.insert((*fact_id, tuple.clone())) {
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
        query.add_instance(world, fact, binders, iterations, &mut atoms);
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
    if view_version != version || indices.len() != prefix.len() + trigger.positions.len() {
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
    for literal in &query.units {
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
