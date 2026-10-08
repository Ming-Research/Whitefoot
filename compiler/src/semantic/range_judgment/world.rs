//! The symbolic world of one function's range judgment: atoms, storage
//! containers and their versions, joins, and the state one path carries.
//!
//! A *container* is one run of elements an index tuple addresses: the content
//! of a `Box<Array<T>>`, the run a range parameter names, a constant-capacity
//! `Array`, or a `Segments` addressed by segment and element. Containers are
//! identified by the storage location that holds them, so two spellings that
//! reach one storage, through a move into a struct or out of a payload,
//! reach one container. Every write makes a new *version* of its container
//! defined by the old one, which is what lets a fact stated about an old
//! version answer a question about a newer one: a read of the newer version
//! is the written value's projection at the written index, and the old
//! version's projection at other indices or at a disjoint sibling field.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::super::model::{BindingId, CheckedMeasure, IntegerType};
use super::super::range_facts::CheckedRangeProjection;
use super::solver::{AtomId, Linear, Literal, Relation};

/// What owns a location's storage.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) enum Origin {
    /// A binding at one execution of the statement that binds it.
    Binding(BindingId, u32),
    /// The referent of a reference parameter, or a by-value parameter.
    Parameter(BindingId),
    /// One call's result at one execution of the call.
    CallResult(u32),
    /// One construction at one execution.
    Constructed(u32),
}

/// One step below a location's origin.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) enum Step {
    Field(u32),
    BoxContent,
    Payload {
        variant: u32,
        field: u32,
        variants: u32,
    },
    Tag(u32),
}

/// One storage location: an origin and the steps below it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct Location {
    pub(super) origin: Origin,
    pub(super) steps: Vec<Step>,
}

impl Location {
    pub(super) fn root(origin: Origin) -> Self {
        Self {
            origin,
            steps: Vec::new(),
        }
    }

    pub(super) fn child(&self, step: Step) -> Self {
        let mut steps = self.steps.clone();
        steps.push(step);
        Self {
            origin: self.origin.clone(),
            steps,
        }
    }

    pub(super) fn starts_with(&self, prefix: &Self) -> bool {
        self.origin == prefix.origin && self.steps.starts_with(&prefix.steps)
    }
}

pub(super) type ContainerId = u32;
pub(super) type VersionId = u32;
pub(super) type JoinId = u32;
pub(super) type FactId = u32;

/// One container: its location and how many indices select an element.
#[derive(Clone, Debug)]
pub(super) struct Container {
    pub(super) location: Location,
    pub(super) arity: usize,
}

/// How one version of a container was made.
#[derive(Clone, Debug)]
pub(super) enum VersionDef {
    /// The contents the container held when the judgment first saw it.
    Initial,
    /// Contents nothing here describes.
    Fresh,
    /// A located write, with the scalar projections of its value that the
    /// walk can name. Missing projections are unknown at the written tuple.
    Write {
        previous: VersionId,
        indices: Vec<Linear>,
        projection: Vec<CheckedRangeProjection>,
        values: BTreeMap<Vec<CheckedRangeProjection>, Stored>,
    },
    /// Unknown writes at any tuple, retaining disjoint projections.
    Forget {
        previous: VersionId,
        projections: BTreeSet<Vec<CheckedRangeProjection>>,
    },
    /// One of the versions of a join's arms, by the arm taken.
    Join {
        join: JoinId,
        versions: Vec<VersionId>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Version {
    pub(super) def: VersionDef,
}

/// What one atom stands for.
#[derive(Clone, Debug)]
pub(super) enum AtomDef {
    /// An integer nothing here defines.
    Opaque,
    /// One element of one version.
    Read {
        version: VersionId,
        indices: Vec<Linear>,
        projection: Vec<CheckedRangeProjection>,
    },
    /// A measure of a container in one generation of its descriptor; the
    /// world interns one atom per container, generation and measure.
    Measure,
    /// The length of one segment of a `Segments` container.
    SegmentLength {
        container: ContainerId,
        generation: u32,
        row: Linear,
    },
    /// One of a join's arm values, by the arm taken.
    Joined { join: JoinId, values: Vec<Linear> },
}

#[derive(Clone, Debug)]
pub(super) struct Atom {
    pub(super) def: AtomDef,
    /// The atom's integer type, whose range bounds it; `None` for a
    /// mathematical integer such as a bound variable.
    pub(super) ty: Option<IntegerType>,
}

/// Every atom, container, version and join of one function's judgment.
#[derive(Debug, Default)]
pub(super) struct World {
    pub(super) atoms: Vec<Atom>,
    reads: HashMap<(VersionId, Vec<Linear>, Vec<CheckedRangeProjection>), AtomId>,
    measures: HashMap<(ContainerId, u32, CheckedMeasure), AtomId>,
    segment_lengths: HashMap<(ContainerId, u32, Linear), AtomId>,
    pub(super) containers: Vec<Container>,
    container_index: HashMap<Location, ContainerId>,
    pub(super) versions: Vec<Version>,
    initial: HashMap<ContainerId, VersionId>,
    /// Each join's arms: the literals each arm added since the fork.
    pub(super) joins: Vec<Vec<Vec<Literal>>>,
    next_origin: u32,
    next_generation: u32,
    /// What a dry walk records as written.
    pub(super) log: Option<Modified>,
}

/// What a walk wrote: the summary a loop header havocs.
#[derive(Clone, Debug, Default)]
pub(super) struct Modified {
    pub(super) containers: BTreeSet<ContainerId>,
    pub(super) descriptors: BTreeSet<ContainerId>,
    pub(super) projections: BTreeMap<ContainerId, BTreeSet<Vec<CheckedRangeProjection>>>,
    pub(super) bindings: BTreeSet<BindingId>,
    pub(super) slots: BTreeSet<Location>,
    /// A write whose target the judgment cannot place.
    pub(super) everything: bool,
}

impl World {
    pub(super) fn opaque(&mut self, ty: Option<IntegerType>) -> Linear {
        Linear::atom(self.push(AtomDef::Opaque, ty))
    }

    fn push(&mut self, def: AtomDef, ty: Option<IntegerType>) -> AtomId {
        let id = AtomId::try_from(self.atoms.len()).unwrap_or(AtomId::MAX);
        self.atoms.push(Atom { def, ty });
        id
    }

    pub(super) fn read(
        &mut self,
        version: VersionId,
        indices: Vec<Linear>,
        projection: Vec<CheckedRangeProjection>,
        ty: Option<IntegerType>,
    ) -> Linear {
        let key = (version, indices.clone(), projection.clone());
        if let Some(atom) = self.reads.get(&key) {
            return Linear::atom(*atom);
        }
        let atom = self.push(
            AtomDef::Read {
                version,
                indices,
                projection,
            },
            ty,
        );
        self.reads.insert(key, atom);
        Linear::atom(atom)
    }

    pub(super) fn measure(
        &mut self,
        container: ContainerId,
        generation: u32,
        measure: CheckedMeasure,
    ) -> Linear {
        let key = (container, generation, measure);
        if let Some(atom) = self.measures.get(&key) {
            return Linear::atom(*atom);
        }
        let atom = self.push(AtomDef::Measure, Some(IntegerType::U64));
        self.measures.insert(key, atom);
        Linear::atom(atom)
    }

    pub(super) fn segment_length(
        &mut self,
        container: ContainerId,
        generation: u32,
        row: Linear,
    ) -> Linear {
        let key = (container, generation, row.clone());
        if let Some(atom) = self.segment_lengths.get(&key) {
            return Linear::atom(*atom);
        }
        let atom = self.push(
            AtomDef::SegmentLength {
                container,
                generation,
                row,
            },
            Some(IntegerType::U64),
        );
        self.segment_lengths.insert(key, atom);
        Linear::atom(atom)
    }

    pub(super) fn joined(
        &mut self,
        join: JoinId,
        values: Vec<Linear>,
        ty: Option<IntegerType>,
    ) -> Linear {
        Linear::atom(self.push(AtomDef::Joined { join, values }, ty))
    }

    pub(super) fn container(&mut self, location: Location, arity: usize) -> Option<ContainerId> {
        if let Some(id) = self.container_index.get(&location) {
            return (self.containers[*id as usize].arity == arity).then_some(*id);
        }
        let id = ContainerId::try_from(self.containers.len()).ok()?;
        self.containers.push(Container {
            location: location.clone(),
            arity,
        });
        self.container_index.insert(location, id);
        Some(id)
    }

    pub(super) fn containers_under(&self, location: &Location) -> Vec<ContainerId> {
        self.containers
            .iter()
            .enumerate()
            .filter(|(_, container)| container.location.starts_with(location))
            .map(|(id, _)| id as ContainerId)
            .collect()
    }

    pub(super) fn initial_version(&mut self, container: ContainerId) -> VersionId {
        if let Some(version) = self.initial.get(&container) {
            return *version;
        }
        let version = self.new_version(VersionDef::Initial);
        self.initial.insert(container, version);
        version
    }

    pub(super) fn new_version(&mut self, def: VersionDef) -> VersionId {
        let id = VersionId::try_from(self.versions.len()).unwrap_or(VersionId::MAX);
        self.versions.push(Version { def });
        id
    }

    /// A join of arms with the given condition deltas. Each arm's literals
    /// begin with `selector == arm`, one fresh selector per join, so every
    /// choice the join defines takes the same arm.
    pub(super) fn new_join(&mut self, arms: Vec<Vec<Literal>>) -> JoinId {
        let id = JoinId::try_from(self.joins.len()).unwrap_or(JoinId::MAX);
        let selector = self.opaque(None);
        let arms = arms
            .into_iter()
            .enumerate()
            .map(|(arm, delta)| {
                let mut literals = vec![Literal::new(
                    selector.clone(),
                    Relation::Equal,
                    Linear::constant(arm as i128),
                )];
                literals.extend(delta);
                literals
            })
            .collect();
        self.joins.push(arms);
        id
    }

    pub(super) fn new_origin(&mut self) -> u32 {
        self.next_origin += 1;
        self.next_origin
    }

    pub(super) fn origin_mark(&self) -> u32 {
        self.next_origin
    }

    pub(super) fn new_generation(&mut self) -> u32 {
        self.next_generation += 1;
        self.next_generation
    }
}

/// A boolean condition over literals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Cond {
    Literal(Literal),
    And(Vec<Cond>),
    Or(Vec<Cond>),
    Not(Box<Cond>),
    Constant(bool),
    Unknown,
}

impl Cond {
    /// The literals that hold when the condition has `truth`, as far as one
    /// conjunction of literals says.
    pub(super) fn literals(&self, truth: bool, out: &mut Vec<Literal>) {
        match (self, truth) {
            (Self::Literal(literal), true) => out.push(literal.clone()),
            (Self::Literal(literal), false) => out.push(negated(literal)),
            (Self::And(parts), true) | (Self::Or(parts), false) => {
                for part in parts {
                    part.literals(truth, out);
                }
            }
            (Self::Not(inner), _) => inner.literals(!truth, out),
            (Self::And(_) | Self::Or(_), _) | (Self::Constant(_) | Self::Unknown, _) => {}
        }
    }

    /// Whether the condition is constantly `!truth`, so the branch taking
    /// `truth` is not reached.
    pub(super) fn excludes(&self, truth: bool) -> bool {
        match self {
            Self::Constant(value) => *value != truth,
            Self::Not(inner) => inner.excludes(!truth),
            _ => false,
        }
    }
}

/// The one-literal negation.
pub(super) fn negated(literal: &Literal) -> Literal {
    let relation = match literal.relation {
        Relation::Equal => Relation::NotEqual,
        Relation::NotEqual => Relation::Equal,
        Relation::Less => Relation::GreaterEqual,
        Relation::LessEqual => Relation::Greater,
        Relation::Greater => Relation::LessEqual,
        Relation::GreaterEqual => Relation::Less,
    };
    Literal::new(literal.left.clone(), relation, literal.right.clone())
}

/// What a reference names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum View {
    /// A run: element `k` is the container's element at `prefix ++ [offset + k]`.
    Run {
        container: ContainerId,
        prefix: Vec<Linear>,
        offset: Linear,
        length: Linear,
    },
    /// One element of a container.
    Element {
        container: ContainerId,
        indices: Vec<Linear>,
        /// None for a descendant outside the range projection vocabulary.
        projection: Option<Vec<CheckedRangeProjection>>,
    },
    /// A whole place.
    Place(Location),
    Unknown,
}

/// One value a binding holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Value {
    Int(Linear),
    Bool(Cond),
    Ref(View),
    /// An owned aggregate stored at this location.
    Owned(Location),
    /// A struct being constructed, field by field.
    Struct(Vec<Value>),
    /// An enum value being constructed.
    Variant {
        variant: u32,
        variants: u32,
        fields: Vec<Value>,
    },
    Unknown,
}

/// Immutable contents captured by an aggregate read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReadSource {
    pub(super) version: VersionId,
    pub(super) indices: Vec<Linear>,
    pub(super) projection: Vec<CheckedRangeProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Stored {
    Int(Linear),
    Read(ReadSource),
    Unknown,
}

/// What one location below an owned value holds, where the judgment knows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Slot {
    /// The storage moved here from another location.
    Alias(Location),
    Read(ReadSource),
    Unknown,
    Int(Linear),
    Ref(View),
}

/// The state one path carries.
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub(super) values: BTreeMap<BindingId, Value>,
    pub(super) slots: BTreeMap<Location, Slot>,
    /// Each container's current version; an absent one is at its initial.
    pub(super) versions: BTreeMap<ContainerId, VersionId>,
    /// Each container's descriptor generation; an absent one is 0.
    pub(super) generations: BTreeMap<ContainerId, u32>,
    pub(super) conds: Vec<Literal>,
    pub(super) facts: Vec<FactId>,
    /// Joins one of whose arms was taken on the way here.
    pub(super) disjunctions: Vec<JoinId>,
    /// Facts that hold once a match finds `location` holding `variant`.
    pub(super) routed: Vec<(Location, u32, FactId)>,
    pub(super) variants: BTreeMap<Location, u32>,
}

impl State {
    /// Nearest stored definition wins, including a write that shadows a
    /// source definition at an ancestor. An alias is resolved before lookup.
    pub(super) fn read_source(&self, location: &Location) -> Option<ReadSource> {
        for length in (0..=location.steps.len()).rev() {
            let prefix = Location {
                origin: location.origin.clone(),
                steps: location.steps[..length].to_vec(),
            };
            if let Some(slot) = self.slots.get(&prefix) {
                return match slot {
                    Slot::Read(source) => {
                        let mut source = source.clone();
                        source
                            .projection
                            .extend(owned_projection(&location.steps[length..])?);
                        Some(source)
                    }
                    _ => None,
                };
            }
        }
        None
    }

    /// The literals and choices every query in this state starts from.
    pub(super) fn premises(&self, world: &World) -> (Vec<Literal>, Vec<Vec<Vec<Literal>>>) {
        let choices = self
            .disjunctions
            .iter()
            .map(|join| world.joins[*join as usize].clone())
            .collect();
        (self.conds.clone(), choices)
    }

    pub(super) fn version(&self, world: &mut World, container: ContainerId) -> VersionId {
        match self.versions.get(&container) {
            Some(version) => *version,
            None => world.initial_version(container),
        }
    }

    pub(super) fn generation(&self, container: ContainerId) -> u32 {
        self.generations.get(&container).copied().unwrap_or(0)
    }

    /// Follows aliases from the longest aliased prefix down.
    pub(super) fn resolve(&self, location: &Location) -> Location {
        let mut current = location.clone();
        // Aliases point at storage that existed first, so this terminates;
        // the bound only guards against a malformed alias table.
        for _ in 0..64 {
            let mut rewritten = None;
            for length in (0..=current.steps.len()).rev() {
                let prefix = Location {
                    origin: current.origin.clone(),
                    steps: current.steps[..length].to_vec(),
                };
                if let Some(Slot::Alias(target)) = self.slots.get(&prefix) {
                    let mut steps = target.steps.clone();
                    steps.extend_from_slice(&current.steps[length..]);
                    rewritten = Some(Location {
                        origin: target.origin.clone(),
                        steps,
                    });
                    break;
                }
            }
            match rewritten {
                Some(next) => current = next,
                None => return current,
            }
        }
        current
    }

    /// Records one written element.
    pub(super) fn write_element(
        &mut self,
        world: &mut World,
        container: ContainerId,
        indices: Vec<Linear>,
        projection: Vec<CheckedRangeProjection>,
        values: BTreeMap<Vec<CheckedRangeProjection>, Stored>,
    ) {
        let previous = self.version(world, container);
        let version = world.new_version(VersionDef::Write {
            previous,
            indices,
            projection: projection.clone(),
            values,
        });
        self.versions.insert(container, version);
        if let Some(log) = &mut world.log {
            log.projections
                .entry(container)
                .or_default()
                .insert(projection);
        }
    }

    /// Forgets the contents of one container, and its descriptor when
    /// `descriptor` says the write can change it.
    pub(super) fn havoc_container(
        &mut self,
        world: &mut World,
        container: ContainerId,
        descriptor: bool,
    ) {
        let version = world.new_version(VersionDef::Fresh);
        self.versions.insert(container, version);
        if descriptor {
            let generation = world.new_generation();
            self.generations.insert(container, generation);
        }
        if let Some(log) = &mut world.log {
            log.containers.insert(container);
            if descriptor {
                log.descriptors.insert(container);
            }
        }
    }

    /// Forgets everything stored at or below `location`.
    pub(super) fn havoc_location(&mut self, world: &mut World, location: &Location) {
        let location = self.resolve(location);
        for container in world.containers_under(&location) {
            self.havoc_container(world, container, true);
        }
        let below: Vec<Location> = self
            .slots
            .keys()
            .filter(|slot| slot.starts_with(&location))
            .cloned()
            .collect();
        for slot in below {
            self.slots.remove(&slot);
            if let Some(log) = &mut world.log {
                log.slots.insert(slot);
            }
        }
        self.slots.insert(location.clone(), Slot::Unknown);
        self.forget_variants(&location);
        if let Some(log) = &mut world.log {
            log.slots.insert(location);
        }
    }

    /// Forgets which variant every enum at or below `location` holds, and
    /// the facts a match on one of them would enable.
    pub(super) fn forget_variants(&mut self, location: &Location) {
        self.variants
            .retain(|known, _| !known.starts_with(location));
        self.routed.retain(|(at, _, _)| !at.starts_with(location));
    }

    /// Forgets every container's contents: a write the judgment cannot place.
    pub(super) fn havoc_everything(&mut self, world: &mut World) {
        for container in 0..world.containers.len() as ContainerId {
            self.havoc_container(world, container, true);
        }
        self.slots.retain(|_, slot| matches!(slot, Slot::Alias(_)));
        self.variants.clear();
        self.routed.clear();
        if let Some(log) = &mut world.log {
            log.everything = true;
        }
    }

    pub(super) fn set_value(&mut self, world: &mut World, binding: BindingId, value: Value) {
        self.values.insert(binding, value);
        if let Some(log) = &mut world.log {
            log.bindings.insert(binding);
        }
    }

    pub(super) fn set_slot(&mut self, world: &mut World, location: Location, slot: Slot) {
        let location = self.resolve(&location);
        if let Some(log) = &mut world.log {
            log.slots.insert(location.clone());
        }
        self.slots.insert(location, slot);
    }
}

/// Joins the live states of a fork's arms. The fork's conditions are the
/// common prefix every arm extended.
pub(super) fn join(world: &mut World, fork_conds: usize, arms: Vec<State>) -> Option<State> {
    join_states(world, fork_conds, arms).map(|(state, _)| state)
}

/// [`join`], with the join that numbers the arms in the order given, when
/// there is more than one.
pub(super) fn join_states(
    world: &mut World,
    fork_conds: usize,
    arms: Vec<State>,
) -> Option<(State, Option<JoinId>)> {
    let mut arms = arms;
    if arms.len() <= 1 {
        return arms.pop().map(|state| (state, None));
    }
    let deltas: Vec<Vec<Literal>> = arms
        .iter()
        .map(|arm| arm.conds.get(fork_conds..).unwrap_or_default().to_vec())
        .collect();
    let join = world.new_join(deltas);
    let first = &arms[0];
    let mut out = State {
        conds: first.conds[..fork_conds.min(first.conds.len())].to_vec(),
        ..State::default()
    };
    // Values every arm holds.
    for binding in first.values.keys() {
        let all: Option<Vec<&Value>> = arms.iter().map(|arm| arm.values.get(binding)).collect();
        let Some(all) = all else {
            continue;
        };
        let value = join_values(world, join, &all, &arms, &mut out);
        out.values.insert(*binding, value);
    }
    let locations: BTreeSet<Location> = arms
        .iter()
        .flat_map(|arm| arm.slots.keys().cloned())
        .collect();
    for location in locations {
        let all: Vec<_> = arms.iter().map(|arm| arm.slots.get(&location)).collect();
        if let Some(slot) = all[0]
            && all.iter().all(|other| *other == Some(slot)) {
                out.slots.insert(location, slot.clone());
                continue;
            }
        // An absent override in one arm means that arm's ancestor source,
        // never permission to discard another arm's write. Unsupported
        // definitions remain unknown and still shadow an ancestor source.
        let versions = arms
            .iter()
            .map(|arm| {
                let mut stored = BTreeMap::new();
                stored_projections(
                    world,
                    arm,
                    &Value::Owned(location.clone()),
                    &mut Vec::new(),
                    &mut stored,
                );
                snapshot_version(world, stored)
            })
            .collect();
        let version = world.new_version(VersionDef::Join { join, versions });
        out.slots.insert(
            location,
            Slot::Read(ReadSource {
                version,
                indices: Vec::new(),
                projection: Vec::new(),
            }),
        );
    }
    let containers: BTreeSet<ContainerId> = arms
        .iter()
        .flat_map(|arm| arm.versions.keys().copied())
        .collect();
    for container in containers {
        let versions: Vec<VersionId> = arms
            .iter()
            .map(|arm| arm.version(world, container))
            .collect();
        let version = if versions.iter().all(|version| *version == versions[0]) {
            versions[0]
        } else {
            world.new_version(VersionDef::Join { join, versions })
        };
        out.versions.insert(container, version);
    }
    let described: BTreeSet<ContainerId> = arms
        .iter()
        .flat_map(|arm| arm.generations.keys().copied())
        .collect();
    for container in described {
        let generations: Vec<u32> = arms.iter().map(|arm| arm.generation(container)).collect();
        let generation = if generations.iter().all(|value| *value == generations[0]) {
            generations[0]
        } else {
            world.new_generation()
        };
        out.generations.insert(container, generation);
    }
    out.facts = first
        .facts
        .iter()
        .copied()
        .filter(|fact| arms.iter().all(|arm| arm.facts.contains(fact)))
        .collect();
    out.disjunctions = first
        .disjunctions
        .iter()
        .copied()
        .filter(|earlier| arms.iter().all(|arm| arm.disjunctions.contains(earlier)))
        .collect();
    out.disjunctions.push(join);
    out.routed = first
        .routed
        .iter()
        .filter(|entry| arms.iter().all(|arm| arm.routed.contains(entry)))
        .cloned()
        .collect();
    for (location, variant) in &first.variants {
        if arms
            .iter()
            .all(|arm| arm.variants.get(location) == Some(variant))
        {
            out.variants.insert(location.clone(), *variant);
        }
    }
    Some((out, Some(join)))
}

/// One value every arm holds, joined.
pub(super) fn join_values(
    world: &mut World,
    join: JoinId,
    all: &[&Value],
    arms: &[State],
    out: &mut State,
) -> Value {
    if all.iter().all(|value| *value == all[0]) {
        return all[0].clone();
    }
    if let Some(values) = all
        .iter()
        .map(|value| match value {
            Value::Int(value) => Some(value.clone()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
    {
        return Value::Int(world.joined(join, values, None));
    }
    if all.iter().all(|value| {
        matches!(
            value,
            Value::Owned(_) | Value::Struct(_) | Value::Variant { .. }
        )
    }) {
        let versions = all
            .iter()
            .zip(arms)
            .map(|(value, state)| {
                let mut stored = BTreeMap::new();
                stored_projections(world, state, value, &mut Vec::new(), &mut stored);
                snapshot_version(world, stored)
            })
            .collect();
        let version = world.new_version(VersionDef::Join { join, versions });
        let location = Location::root(Origin::Constructed(world.new_origin()));
        out.slots.insert(
            location.clone(),
            Slot::Read(ReadSource {
                version,
                indices: Vec::new(),
                projection: Vec::new(),
            }),
        );
        return Value::Owned(location);
    }
    Value::Unknown
}

/// Sibling struct fields are disjoint; payload storage of different variants
/// overlaps even when their field ordinals differ.
pub(super) fn projections_overlap(
    left: &[CheckedRangeProjection],
    right: &[CheckedRangeProjection],
) -> bool {
    for (left, right) in left.iter().zip(right) {
        if left == right {
            continue;
        }
        return matches!((left, right),
            (CheckedRangeProjection::Payload { variant: a, .. }, CheckedRangeProjection::Payload { variant: b, .. }) if a != b);
    }
    true
}

/// Owned paths name stored scalar projections, including enum tags.
pub(super) fn owned_projection(steps: &[Step]) -> Option<Vec<CheckedRangeProjection>> {
    steps
        .iter()
        .map(|step| match step {
            Step::Field(field) => Some(CheckedRangeProjection::Field(*field)),
            Step::BoxContent => Some(CheckedRangeProjection::BoxContent),
            Step::Payload {
                variant,
                field,
                variants,
            } => Some(CheckedRangeProjection::Payload {
                variant: *variant,
                field: *field,
                variants: *variants,
            }),
            Step::Tag(variants) => Some(CheckedRangeProjection::Tag(*variants)),
        })
        .collect()
}

pub(super) fn stored_projections(
    world: &mut World,
    state: &State,
    value: &Value,
    path: &mut Vec<CheckedRangeProjection>,
    values: &mut BTreeMap<Vec<CheckedRangeProjection>, Stored>,
) {
    match value {
        Value::Int(value) => {
            values.insert(path.clone(), Stored::Int(value.clone()));
        }
        Value::Struct(fields) => {
            for (ordinal, field) in fields.iter().enumerate() {
                path.push(CheckedRangeProjection::Field(ordinal as u32));
                stored_projections(world, state, field, path, values);
                path.pop();
            }
        }
        Value::Variant {
            variant,
            variants,
            fields,
        } => {
            path.push(CheckedRangeProjection::Tag(*variants));
            values.insert(
                path.clone(),
                Stored::Int(Linear::constant(*variant as i128)),
            );
            path.pop();
            for (ordinal, field) in fields.iter().enumerate() {
                path.push(CheckedRangeProjection::Payload {
                    variant: *variant,
                    field: ordinal as u32,
                    variants: *variants,
                });
                stored_projections(world, state, field, path, values);
                path.pop();
            }
        }
        Value::Owned(location) => {
            let location = state.resolve(location);
            if let Some(source) = state.read_source(&location) {
                values.insert(path.clone(), Stored::Read(source));
            }
            // Snapshot named scalar contents before the destination write.
            // Sources are immutable versions; later writes cannot change them.
            for (slot, value) in &state.slots {
                if !slot.starts_with(&location) {
                    continue;
                }
                let Some(suffix) = owned_projection(&slot.steps[location.steps.len()..]) else {
                    continue;
                };
                let length = path.len();
                path.extend(suffix);
                match value {
                    Slot::Int(value) => {
                        values.insert(path.clone(), Stored::Int(value.clone()));
                    }
                    Slot::Alias(source) => stored_projections(
                        world,
                        state,
                        &Value::Owned(source.clone()),
                        path,
                        values,
                    ),
                    Slot::Read(source) => {
                        values.insert(path.clone(), Stored::Read(source.clone()));
                    }
                    Slot::Unknown => {
                        values.insert(path.clone(), Stored::Unknown);
                    }
                    Slot::Ref(_) => {}
                }
                path.truncate(length);
            }
            for container in world.containers_under(&location) {
                let at = &world.containers[container as usize].location;
                let Some(suffix) = owned_projection(&at.steps[location.steps.len()..]) else {
                    continue;
                };
                let length = path.len();
                path.extend(suffix);
                for measure in [CheckedMeasure::Length, CheckedMeasure::Capacity] {
                    path.push(CheckedRangeProjection::Measure(measure));
                    let value = world.measure(container, state.generation(container), measure);
                    values.insert(path.clone(), Stored::Int(value));
                    path.pop();
                }
                path.truncate(length);
            }
        }
        _ => {}
    }
}

/// Capture aggregate definitions in a zero-index version for lazy projections.
fn snapshot_version(
    world: &mut World,
    values: BTreeMap<Vec<CheckedRangeProjection>, Stored>,
) -> VersionId {
    let previous = world.new_version(VersionDef::Fresh);
    world.new_version(VersionDef::Write {
        previous,
        indices: Vec::new(),
        projection: Vec::new(),
        values,
    })
}
