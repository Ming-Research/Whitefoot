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
//! already selects through one of the fact's own reads (its triggers), once,
//! together with the instances a certificate writes. No instance is formed
//! from an instance's reads.

use std::collections::{BTreeMap, BTreeSet};

use super::super::model::CheckedMeasure;
use super::super::range_facts::{
    CheckedRangeClause, CheckedRangePlace, CheckedRangeRelation, CheckedRangeRoot,
    CheckedRangeShape, CheckedRangeTerm, RangeComparison,
};
use super::solver::{
    AtomId, AtomKind, Capacity, Linear, Literal, Problem, Relation, Rule, Verdict,
};
use super::world::{AtomDef, ContainerId, FactId, VersionDef, VersionId, World};

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
            CheckedRangeTerm::Measure { place, measure } => {
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
                    _ => None,
                }
            }
            CheckedRangeTerm::SegmentLength { place, segment } => {
                let row = self.term(segment)?;
                let PlaceView::Segments {
                    container,
                    generation,
                    rows,
                    ..
                } = self.frame.places.get(place)?.clone()
                else {
                    return None;
                };
                self.within(&row, &rows);
                Some(self.world.segment_length(container, generation, row))
            }
            CheckedRangeTerm::Read {
                place,
                shape,
                indices,
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
                        let [index] = values.as_slice() else {
                            return None;
                        };
                        self.within(index, &length);
                        let mut selected = prefix;
                        selected.push(offset.plus(index)?);
                        Some(self.world.read(version, selected, Some(*element)))
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
                        let [row, index] = values.as_slice() else {
                            return None;
                        };
                        self.within(row, &rows);
                        let length = self
                            .world
                            .segment_length(container, generation, row.clone());
                        self.within(index, &length);
                        Some(self.world.read(version, values, Some(*element)))
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
    /// Per index position: the bound variable it names, or `None`.
    positions: Vec<Option<u32>>,
}

fn collect_triggers(term: &CheckedRangeTerm, out: &mut Vec<Trigger>) {
    match term {
        CheckedRangeTerm::Read { place, indices, .. } => {
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
                    positions,
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
    // Triggered instances, from the reads the problem holds now.
    let ground: Vec<(VersionId, Vec<Linear>)> = atoms
        .iter()
        .filter_map(|atom| match &world.atoms[*atom as usize].def {
            AtomDef::Read { version, indices } => Some((*version, indices.clone())),
            _ => None,
        })
        .collect();
    let mut instances: Vec<(FactId, Vec<Linear>, Vec<Linear>)> = written.to_vec();
    for fact_id in active {
        let fact = &facts[*fact_id as usize];
        let mut triggers = Vec::new();
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
            for (version, indices) in &ground {
                match_trigger(view, trigger, *version, indices, &mut candidates);
            }
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
            if tuples.len() > MAX_INSTANCES {
                return Err(Capacity::Instances);
            }
        }
        for tuple in tuples {
            if tuple.len() == fact.clause.binders.len() {
                instances.push((*fact_id, tuple, Vec::new()));
            }
        }
    }
    for (fact_id, binders, iterations) in instances {
        let fact = &facts[fact_id as usize];
        let Some(formed) = form(world, &fact.clause, &fact.frame, &binders, &iterations) else {
            continue;
        };
        for literal in formed.premises.iter().chain(formed.conclusions.iter()) {
            collect_literal(literal, &mut atoms);
        }
        query.rules.push(Rule {
            guards: formed.premises,
            conclusions: formed.conclusions,
        });
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
            Some(offset) if position + 1 == trigger.positions.len() => index.minus(offset),
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
            AtomDef::Read { version, indices } => {
                for index in &indices {
                    collect_linear(index, atoms);
                }
                match world.versions[version as usize].def.clone() {
                    VersionDef::Initial | VersionDef::Fresh => {}
                    VersionDef::Write {
                        previous,
                        indices: written,
                        value: stored,
                    } => {
                        let old = world.read(previous, indices.clone(), ty);
                        let mut hit: Vec<Literal> = indices
                            .iter()
                            .zip(&written)
                            .map(|(index, at)| {
                                Literal::new(index.clone(), Relation::Equal, at.clone())
                            })
                            .collect();
                        if let Some(stored) = stored {
                            hit.push(Literal::new(value.clone(), Relation::Equal, stored));
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
                            let selected = world.read(version, indices.clone(), ty);
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
    let mut descriptors: BTreeMap<(ContainerId, u32), u32> = BTreeMap::new();
    let mut kinds = Vec::with_capacity(atoms.len());
    for atom in atoms {
        let kind = match &world.atoms[*atom as usize].def {
            AtomDef::Read { version, indices } => AtomKind::Read {
                place: *version,
                indices: indices.iter().map(&map_linear).collect(),
            },
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
