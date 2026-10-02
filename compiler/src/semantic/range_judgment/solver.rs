//! [RANGE-3] the fixed quantifier-free judgment of range proofs.
//!
//! A query is a finite problem over integer atoms: unit literals that hold,
//! choices of which exactly one alternative holds, and rules whose
//! conclusions hold once their guards do. The judgment asks whether the
//! problem has no model, and it runs to completion:
//!
//! 1. **Theory.** A set of literals is contradictory when, after its
//!    equalities are solved over the integers and every element read is
//!    identified with every other read of the same place at the same solved
//!    indices (read congruence), the equalities have no integer solution, a
//!    solved disequality is `0 != 0`, a solved constant comparison is false,
//!    or the inequalities, each tightened over the integers as it is formed,
//!    have no rational solution, which Fourier-Motzkin elimination decides
//!    in any order since it tightens nothing it derives. Solving renames
//!    atoms by integer changes of variables until an equality has a unit
//!    coefficient, so the free atoms parametrize the integer solutions one
//!    to one; tightening is then the same in every parametrization, and the
//!    verdict depends only on the set of literals and grows with it.
//! 2. **Saturation.** A choice none of whose other alternatives is
//!    consistent with the units asserts its one consistent alternative, and a
//!    rule whose every guard the units entail asserts its conclusions. A
//!    guard is entailed when the units with its negation are contradictory.
//! 3. **Split.** When saturation leaves a choice undecided, each consistent
//!    alternative is tried in written order; then each unsettled pair of
//!    reads is tried equal and apart at each differing position; then each
//!    disequality the units hold is tried as `<` and as `>`. The problem has
//!    no model when every branch is contradictory. Because the theory only
//!    grows with the literals and every split covers all of its item's
//!    cases, this verdict does not depend on the order of the splits.
//! 4. **Backjumping.** Each unit records the splits it rests on, and each
//!    contradiction the splits its literals rest on. A branch refuted
//!    without its own split's case refutes the split's other branches too,
//!    which are not tried; by the order independence above, the verdict is
//!    the full case analysis's.
//!
//! Nothing here searches for an instance or a lemma: every rule and choice
//! comes with the problem, and the only branching is over the problem's own
//! finite alternatives, run to completion. The two structural ceilings are
//! the caller's, on the problem's atoms and instances; the only stop here is
//! this checker's `i128` arithmetic, a capability limit of the checker.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

/// One atom of a problem, chosen by the caller.
pub(crate) type AtomId = u32;

/// A linear integer term over atoms.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Linear {
    /// Sorted by atom, no zero coefficient.
    pub(crate) terms: Vec<(AtomId, i128)>,
    pub(crate) constant: i128,
}

impl Linear {
    pub(crate) fn constant(value: i128) -> Self {
        Self {
            terms: Vec::new(),
            constant: value,
        }
    }

    pub(crate) fn atom(atom: AtomId) -> Self {
        Self {
            terms: vec![(atom, 1)],
            constant: 0,
        }
    }

    pub(crate) fn is_constant(&self) -> bool {
        self.terms.is_empty()
    }

    pub(crate) fn scaled(&self, factor: i128) -> Option<Self> {
        if factor == 0 {
            return Some(Self::constant(0));
        }
        let mut terms = Vec::with_capacity(self.terms.len());
        for (atom, coefficient) in &self.terms {
            terms.push((*atom, coefficient.checked_mul(factor)?));
        }
        Some(Self {
            terms,
            constant: self.constant.checked_mul(factor)?,
        })
    }

    pub(crate) fn plus(&self, other: &Self) -> Option<Self> {
        let mut merged: BTreeMap<AtomId, i128> = BTreeMap::new();
        for (atom, coefficient) in self.terms.iter().chain(other.terms.iter()) {
            let entry = merged.entry(*atom).or_insert(0);
            *entry = entry.checked_add(*coefficient)?;
        }
        Some(Self {
            terms: merged
                .into_iter()
                .filter(|(_, value)| *value != 0)
                .collect(),
            constant: self.constant.checked_add(other.constant)?,
        })
    }

    pub(crate) fn minus(&self, other: &Self) -> Option<Self> {
        self.plus(&other.scaled(-1)?)
    }

    pub(crate) fn plus_constant(&self, value: i128) -> Option<Self> {
        let mut out = self.clone();
        out.constant = out.constant.checked_add(value)?;
        Some(out)
    }

    fn coefficient(&self, atom: AtomId) -> i128 {
        self.terms
            .iter()
            .find(|(candidate, _)| *candidate == atom)
            .map_or(0, |(_, value)| *value)
    }

    /// Replaces every atom the substitution solves by its solution.
    fn substituted(&self, solved: &BTreeMap<AtomId, Linear>) -> Option<Self> {
        if !self.terms.iter().any(|(atom, _)| solved.contains_key(atom)) {
            return Some(self.clone());
        }
        let mut out = Self::constant(self.constant);
        for (atom, coefficient) in &self.terms {
            let term = match solved.get(atom) {
                Some(solution) => solution.scaled(*coefficient)?,
                None => Self {
                    terms: vec![(*atom, *coefficient)],
                    constant: 0,
                },
            };
            out = out.plus(&term)?;
        }
        Some(out)
    }
}

/// One comparison.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum Relation {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

/// `left relation right`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Literal {
    pub(crate) left: Linear,
    pub(crate) relation: Relation,
    pub(crate) right: Linear,
}

impl Literal {
    pub(crate) fn new(left: Linear, relation: Relation, right: Linear) -> Self {
        Self {
            left,
            relation,
            right,
        }
    }

    /// The negation, as alternatives one of which holds when this does not.
    pub(crate) fn negation(&self) -> Vec<Literal> {
        let flip = |relation| Literal::new(self.left.clone(), relation, self.right.clone());
        match self.relation {
            Relation::Equal => vec![flip(Relation::Less), flip(Relation::Greater)],
            Relation::NotEqual => vec![flip(Relation::Equal)],
            Relation::Less => vec![flip(Relation::GreaterEqual)],
            Relation::LessEqual => vec![flip(Relation::Greater)],
            Relation::Greater => vec![flip(Relation::LessEqual)],
            Relation::GreaterEqual => vec![flip(Relation::Less)],
        }
    }
}

/// `term <= 0` over the integers.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Inequality(Linear);

impl Inequality {
    /// `left <= right`.
    fn at_most(left: &Linear, right: &Linear) -> Option<Self> {
        Some(Self(left.minus(right)?).normalized())
    }

    /// `left < right`, which over the integers is `left - right <= -1`.
    fn below(left: &Linear, right: &Linear) -> Option<Self> {
        Some(Self(left.minus(right)?.plus_constant(1)?).normalized())
    }

    /// Divides by the coefficients' common divisor and floors the bound,
    /// which is exact over the integers: the tightening [RANGE-3] applies to
    /// each inequality a literal forms.
    fn normalized(self) -> Self {
        let divisor = self
            .0
            .terms
            .iter()
            .fold(0_i128, |gcd, (_, value)| greatest_divisor(gcd, value.abs()));
        if divisor <= 1 {
            return self;
        }
        // `sum <= -constant`: floor the bound `-constant / divisor`.
        let Some(bound) = self.0.constant.checked_neg() else {
            return self;
        };
        let floored = bound.div_euclid(divisor);
        Self(Linear {
            terms: self
                .0
                .terms
                .iter()
                .map(|(atom, value)| (*atom, value / divisor))
                .collect(),
            constant: -floored,
        })
    }

    /// Divides by the common divisor of the coefficients and the constant,
    /// which keeps the rational solutions: elimination combines without
    /// tightening, so its verdict is the same in every elimination order.
    fn reduced(self) -> Self {
        let divisor = self
            .0
            .terms
            .iter()
            .fold(self.0.constant.abs(), |gcd, (_, value)| {
                greatest_divisor(gcd, value.abs())
            });
        if divisor <= 1 {
            return self;
        }
        Self(Linear {
            terms: self
                .0
                .terms
                .iter()
                .map(|(atom, value)| (*atom, value / divisor))
                .collect(),
            constant: self.0.constant / divisor,
        })
    }

    fn contradictory(&self) -> bool {
        self.0.terms.is_empty() && self.0.constant > 0
    }

    fn trivial(&self) -> bool {
        self.0.terms.is_empty() && self.0.constant <= 0
    }
}

fn greatest_divisor(left: i128, right: i128) -> i128 {
    let (mut left, mut right) = (left, right);
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

/// What stops a problem's judgment short of an answer about the problem.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Capacity {
    /// More atoms than one problem holds, a structural ceiling [RANGE-3].
    Atoms,
    /// More instances of one fact than one problem holds, a structural
    /// ceiling [RANGE-3].
    Instances,
    /// A coefficient or constant outside this checker's `i128` arithmetic.
    /// The derivation's arithmetic is exact [RANGE-3], so this is a compiler
    /// capability and never a source verdict.
    Arithmetic,
}

impl Capacity {
    /// The ceiling's description in a diagnostic.
    pub(crate) const fn describe(self) -> &'static str {
        match self {
            Self::Atoms => "the 4096 atoms one problem holds",
            Self::Instances => "the 256 instances of one fact one problem holds",
            Self::Arithmetic => "the checker's i128 arithmetic",
        }
    }
}

/// What an atom stands for, as far as congruence is concerned.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum AtomKind {
    /// An opaque integer.
    Plain,
    /// An element read: the read storage version and its index tuple.
    Read { place: u32, indices: Vec<Linear> },
    /// One segment's length: the storage version and the segment index.
    SegmentLength { place: u32, segment: Linear },
}

/// A rule `guards => conclusions`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Rule {
    pub(crate) guards: Vec<Literal>,
    pub(crate) conclusions: Vec<Literal>,
}

/// A finite problem.
#[derive(Clone, Debug, Default)]
pub(crate) struct Problem {
    pub(crate) atoms: Vec<AtomKind>,
    pub(crate) units: Vec<Literal>,
    /// Each choice: exactly one alternative, a conjunction, holds.
    pub(crate) choices: Vec<Vec<Vec<Literal>>>,
    pub(crate) rules: Vec<Rule>,
}

/// The judgment's answer for one problem.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verdict {
    /// The problem has no model.
    Refuted,
    /// Some branch stayed consistent.
    Open,
}

impl Problem {
    /// Whether the problem has no model.
    pub(crate) fn judge(&self) -> Result<Verdict, Capacity> {
        self.judge_in(&Run::backjumping())
    }

    /// [`Self::judge`] within `run`.
    fn judge_in(&self, run: &Run) -> Result<Verdict, Capacity> {
        let units = self
            .units
            .iter()
            .map(|literal| Unit::given(literal.clone()))
            .collect();
        let decided = vec![false; self.choices.len()];
        let fired = vec![false; self.rules.len()];
        Ok(match self.search(units, decided, fired, 0, run)? {
            Outcome::Refuted(_) => Verdict::Refuted,
            Outcome::Open => Verdict::Open,
        })
    }

    /// One branch at `depth`, the identity its own split takes. A probe
    /// that tests a literal against the units takes the same identity: the
    /// probe's literal never stays among the units, and no unit rests on a
    /// split this deep yet.
    fn search(
        &self,
        mut units: Vec<Unit>,
        mut decided: Vec<bool>,
        mut fired: Vec<bool>,
        depth: usize,
        run: &Run,
    ) -> Result<Outcome, Capacity> {
        run.nodes.set(run.nodes.get() + 1);
        // Saturation: decide what the units force, fire what they entail.
        loop {
            if let Some(core) = self.contradiction(&units)? {
                return Ok(Outcome::Refuted(core));
            }
            let mut changed = false;
            for (index, choice) in self.choices.iter().enumerate() {
                if decided[index] {
                    continue;
                }
                let mut open = Vec::new();
                // Why the alternatives found contradictory are excluded.
                let mut excluded = Tags::default();
                for alternative in choice {
                    let mut with = units.clone();
                    with.extend(Unit::resting_on(alternative, &Tags::single(depth)));
                    match self.contradiction(&with)? {
                        Some(core) => excluded.union(&core.without(depth)),
                        None => open.push(alternative),
                    }
                }
                match open.as_slice() {
                    [] => return Ok(Outcome::Refuted(excluded)),
                    [only] => {
                        units.extend(Unit::resting_on(only, &excluded));
                        decided[index] = true;
                        changed = true;
                    }
                    _ => {}
                }
            }
            for (index, rule) in self.rules.iter().enumerate() {
                if fired[index] {
                    continue;
                }
                if let Some(because) = self.entailment(&units, &rule.guards, depth)? {
                    units.extend(Unit::resting_on(&rule.conclusions, &because));
                    fired[index] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Split on the first undecided choice.
        if let Some(index) = decided.iter().position(|done| !done) {
            decided[index] = true;
            let branches: Vec<Vec<Unit>> = self.choices[index]
                .iter()
                .map(|alternative| {
                    let mut with = units.clone();
                    with.extend(Unit::resting_on(alternative, &Tags::single(depth)));
                    with
                })
                .collect();
            return self.split(branches, &decided, &fired, depth, run);
        }
        // Then on the first two reads of one place whose index tuples the
        // units neither identify nor separate: read congruence asks exactly
        // whether they are one element.
        let solved = self.solve(&units)?;
        match self.open_read_pair(&units, &solved, depth)? {
            ReadPair::Derived(derived) => {
                units.extend(derived);
                return self.search(units, decided, fired, depth, run);
            }
            ReadPair::Split { equal, apart } => {
                let mut branches = Vec::with_capacity(apart.len() + 1);
                let mut with = units.clone();
                with.extend(Unit::resting_on(&equal, &Tags::single(depth)));
                branches.push(with);
                for literal in apart {
                    let mut with = units.clone();
                    with.extend(Unit::resting_on(&[literal], &Tags::single(depth)));
                    branches.push(with);
                }
                return self.split(branches, &decided, &fired, depth, run);
            }
            ReadPair::Settled => {}
        }
        // Then on the first disequality the theory left open.
        for (position, unit) in units.iter().enumerate() {
            let literal = &unit.literal;
            if literal.relation != Relation::NotEqual {
                continue;
            }
            let Some((difference, _)) = literal
                .left
                .minus(&literal.right)
                .and_then(|difference| substituted(&difference, &solved.solution))
            else {
                return Err(Capacity::Arithmetic);
            };
            if difference.is_constant() {
                continue;
            }
            let mut tags = unit.tags.clone();
            tags.insert(depth);
            let branches = [Relation::Less, Relation::Greater]
                .into_iter()
                .map(|relation| {
                    let mut with = units.clone();
                    with[position] = Unit {
                        literal: Literal::new(
                            literal.left.clone(),
                            relation,
                            literal.right.clone(),
                        ),
                        tags: tags.clone(),
                    };
                    with
                })
                .collect();
            return self.split(branches, &decided, &fired, depth, run);
        }
        Ok(Outcome::Open)
    }

    /// Tries each branch of the split at `depth` in order. A branch refuted
    /// by units none of which rests on this split refutes its siblings too:
    /// each holds those units, the theory's contradictions only grow with
    /// the units, and the verdict does not depend on the order of the
    /// splits below. This conflict-directed backjumping skips only branches
    /// already refuted, so the verdict is the full case analysis's.
    fn split(
        &self,
        branches: Vec<Vec<Unit>>,
        decided: &[bool],
        fired: &[bool],
        depth: usize,
        run: &Run,
    ) -> Result<Outcome, Capacity> {
        let mut core = Tags::default();
        for with in branches {
            let refuted = match self.contradiction(&with)? {
                Some(why) => why,
                None => {
                    match self.search(with, decided.to_vec(), fired.to_vec(), depth + 1, run)? {
                        Outcome::Refuted(why) => why,
                        Outcome::Open => return Ok(Outcome::Open),
                    }
                }
            };
            if run.backjump && !refuted.contains(depth) {
                return Ok(Outcome::Refuted(refuted));
            }
            core.union(&refuted.without(depth));
        }
        Ok(Outcome::Refuted(core))
    }

    /// The splits the units' entailment of every guard rests on, or `None`
    /// when one guard is not entailed: a guard is entailed when the units
    /// with each of its negations are contradictory.
    fn entailment(
        &self,
        units: &[Unit],
        guards: &[Literal],
        depth: usize,
    ) -> Result<Option<Tags>, Capacity> {
        let mut because = Tags::default();
        for guard in guards {
            for negation in guard.negation() {
                let mut with = units.to_vec();
                with.push(Unit {
                    literal: negation,
                    tags: Tags::single(depth),
                });
                match self.contradiction(&with)? {
                    Some(core) => because.union(&core.without(depth)),
                    None => return Ok(None),
                }
            }
        }
        Ok(Some(because))
    }

    /// Read congruence's question about the reads of one place, asked of
    /// classes of reads at one solved index tuple. A pair of classes is
    /// settled when, at a position where the tuples differ, the difference
    /// is constant or a unit's disequality or strict comparison states it
    /// apart. Of the unsettled pairs, each the units force apart at its only
    /// differing position yields that disequality as a unit; failing those,
    /// the first pair in atom order is split into its cases: equal, and
    /// apart at each differing position. Literals are
    /// written with the index expressions of each class's first read, so
    /// they name only the problem's atoms.
    fn open_read_pair(
        &self,
        units: &[Unit],
        solved: &Solved,
        depth: usize,
    ) -> Result<ReadPair, Capacity> {
        let mut by_place: BTreeMap<u32, Vec<ReadClass>> = BTreeMap::new();
        for kind in &self.atoms {
            let AtomKind::Read { place, indices } = kind else {
                continue;
            };
            let mut reduced = Vec::with_capacity(indices.len());
            for index in indices {
                reduced.push(
                    substituted(index, &solved.solution)
                        .ok_or(Capacity::Arithmetic)?
                        .0,
                );
            }
            let classes = by_place.entry(*place).or_default();
            if !classes.iter().any(|(tuple, _)| *tuple == reduced) {
                classes.push((reduced, indices));
            }
        }
        // The differences the units state apart, solved, up to sign.
        let mut separated = BTreeSet::new();
        for unit in units {
            let literal = &unit.literal;
            if matches!(
                literal.relation,
                Relation::NotEqual | Relation::Less | Relation::Greater
            ) {
                let (difference, _) = literal
                    .left
                    .minus(&literal.right)
                    .and_then(|difference| substituted(&difference, &solved.solution))
                    .ok_or(Capacity::Arithmetic)?;
                separated.insert(up_to_sign(difference).ok_or(Capacity::Arithmetic)?);
            }
        }
        let mut derived = Vec::new();
        let mut split = None;
        for classes in by_place.values() {
            for (position, (first, first_indices)) in classes.iter().enumerate() {
                for (second, second_indices) in &classes[position + 1..] {
                    if first.len() != second.len() {
                        continue;
                    }
                    let mut differing = Vec::new();
                    let mut settled = false;
                    for (index, (left, right)) in first.iter().zip(second).enumerate() {
                        if left == right {
                            continue;
                        }
                        let difference = left.minus(right).ok_or(Capacity::Arithmetic)?;
                        if difference.is_constant()
                            || separated
                                .contains(&up_to_sign(difference).ok_or(Capacity::Arithmetic)?)
                        {
                            settled = true;
                            break;
                        }
                        differing.push(index);
                    }
                    if settled {
                        continue;
                    }
                    let sides = |relation| -> Vec<Literal> {
                        differing
                            .iter()
                            .map(|index| {
                                Literal::new(
                                    first_indices[*index].clone(),
                                    relation,
                                    second_indices[*index].clone(),
                                )
                            })
                            .collect()
                    };
                    let (equal, apart) = (sides(Relation::Equal), sides(Relation::NotEqual));
                    let mut with = units.to_vec();
                    with.extend(Unit::resting_on(&equal, &Tags::single(depth)));
                    match (self.contradiction(&with)?, apart.as_slice()) {
                        // Forced apart at its one differing position.
                        (Some(core), [single]) => derived.push(Unit {
                            literal: single.clone(),
                            tags: core.without(depth),
                        }),
                        // The equal case stays a branch even when the units
                        // exclude it, so the splits its exclusion rests on
                        // join the split's refutation.
                        _ => {
                            if split.is_none() {
                                split = Some(ReadPair::Split { equal, apart });
                            }
                        }
                    }
                }
            }
        }
        if !derived.is_empty() {
            return Ok(ReadPair::Derived(derived));
        }
        Ok(split.unwrap_or(ReadPair::Settled))
    }

    /// Solves the equalities over the integers and closes read congruence;
    /// each solution keeps the splits it rests on.
    fn solve(&self, units: &[Unit]) -> Result<Solved, Capacity> {
        let mut solved = Solved {
            fresh: AtomId::try_from(self.atoms.len()).map_err(|_| Capacity::Atoms)?,
            ..Solved::default()
        };
        let mut pending: Vec<(Linear, Tags)> = Vec::new();
        for unit in units {
            if unit.literal.relation == Relation::Equal {
                pending.push((
                    unit.literal
                        .left
                        .minus(&unit.literal.right)
                        .ok_or(Capacity::Arithmetic)?,
                    unit.tags.clone(),
                ));
            }
        }
        loop {
            let mut progressed = false;
            for (difference, mut why) in pending.drain(..) {
                let (difference, used) =
                    substituted(&difference, &solved.solution).ok_or(Capacity::Arithmetic)?;
                why.union(&used);
                progressed |= solved.equate(difference, why)?;
            }
            // Read congruence: one place read at one solved index tuple is
            // one value.
            let mut seen: BTreeMap<(u8, u32, Vec<Linear>), (AtomId, Tags)> = BTreeMap::new();
            for (index, kind) in self.atoms.iter().enumerate() {
                let atom = index as AtomId;
                let mut why = Tags::default();
                let key = match kind {
                    AtomKind::Plain => continue,
                    AtomKind::Read { place, indices } => {
                        let mut reduced = Vec::with_capacity(indices.len());
                        for index in indices {
                            let (index, used) =
                                substituted(index, &solved.solution).ok_or(Capacity::Arithmetic)?;
                            why.union(&used);
                            reduced.push(index);
                        }
                        (0_u8, *place, reduced)
                    }
                    AtomKind::SegmentLength { place, segment } => {
                        let (segment, used) =
                            substituted(segment, &solved.solution).ok_or(Capacity::Arithmetic)?;
                        why.union(&used);
                        (1_u8, *place, vec![segment])
                    }
                };
                match seen.get(&key) {
                    Some((other, other_why)) => {
                        let (difference, used) = Linear::atom(atom)
                            .minus(&Linear::atom(*other))
                            .and_then(|difference| substituted(&difference, &solved.solution))
                            .ok_or(Capacity::Arithmetic)?;
                        if !(difference.is_constant() && difference.constant == 0) {
                            why.union(other_why);
                            why.union(&used);
                            pending.push((difference, why));
                            progressed = true;
                        }
                    }
                    None => {
                        seen.insert(key, (atom, why));
                    }
                }
            }
            if !progressed || solved.contradiction.is_some() {
                return Ok(solved);
            }
        }
    }

    /// The splits a contradiction among the units rests on, or `None` when
    /// the units are consistent in the theory.
    fn contradiction(&self, units: &[Unit]) -> Result<Option<Tags>, Capacity> {
        let solved = self.solve(units)?;
        if let Some(why) = solved.contradiction {
            return Ok(Some(why));
        }
        let mut inequalities = Vec::new();
        for unit in units {
            let literal = &unit.literal;
            let (left, left_used) =
                substituted(&literal.left, &solved.solution).ok_or(Capacity::Arithmetic)?;
            let (right, right_used) =
                substituted(&literal.right, &solved.solution).ok_or(Capacity::Arithmetic)?;
            let mut why = unit.tags.clone();
            why.union(&left_used);
            why.union(&right_used);
            let inequality = match literal.relation {
                Relation::Equal => continue,
                Relation::NotEqual => {
                    let difference = left.minus(&right).ok_or(Capacity::Arithmetic)?;
                    if difference.is_constant() && difference.constant == 0 {
                        return Ok(Some(why));
                    }
                    continue;
                }
                Relation::LessEqual => Inequality::at_most(&left, &right),
                Relation::GreaterEqual => Inequality::at_most(&right, &left),
                Relation::Less => Inequality::below(&left, &right),
                Relation::Greater => Inequality::below(&right, &left),
            };
            inequalities.push((inequality.ok_or(Capacity::Arithmetic)?, why));
        }
        eliminate(inequalities)
    }
}

/// The splits one unit, solution or contradiction rests on: the depths of
/// splits along the current branch, as a bit set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Tags(Vec<u64>);

impl Tags {
    fn single(depth: usize) -> Self {
        let mut tags = Self::default();
        tags.insert(depth);
        tags
    }

    fn insert(&mut self, depth: usize) {
        let word = depth / 64;
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        self.0[word] |= 1 << (depth % 64);
    }

    fn contains(&self, depth: usize) -> bool {
        self.0
            .get(depth / 64)
            .is_some_and(|word| word & (1 << (depth % 64)) != 0)
    }

    fn union(&mut self, other: &Self) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        for (word, other) in self.0.iter_mut().zip(&other.0) {
            *word |= other;
        }
    }

    fn without(&self, depth: usize) -> Self {
        let mut tags = self.clone();
        if let Some(word) = tags.0.get_mut(depth / 64) {
            *word &= !(1 << (depth % 64));
        }
        tags
    }
}

/// A literal a branch holds, with the splits it rests on.
#[derive(Clone, Debug)]
struct Unit {
    literal: Literal,
    tags: Tags,
}

impl Unit {
    /// A literal the problem gives, resting on no split.
    fn given(literal: Literal) -> Self {
        Self {
            literal,
            tags: Tags::default(),
        }
    }

    /// Each of `literals`, resting on `tags`.
    fn resting_on<'a>(literals: &'a [Literal], tags: &'a Tags) -> impl Iterator<Item = Self> + 'a {
        literals.iter().map(move |literal| Self {
            literal: literal.clone(),
            tags: tags.clone(),
        })
    }
}

/// One judgment's search: how many branches it has saturated, and whether
/// a branch refuted without its split's case closes the split's other
/// branches. Without backjumping the search is the full case analysis,
/// against which tests compare it.
struct Run {
    nodes: Cell<usize>,
    backjump: bool,
}

impl Run {
    fn backjumping() -> Self {
        Self {
            nodes: Cell::new(0),
            backjump: true,
        }
    }
}

/// A branch's answer: refuted, with the splits its refutation rests on, or
/// open.
enum Outcome {
    Refuted(Tags),
    Open,
}

/// `linear` or its negation, whichever has a positive first coefficient.
fn up_to_sign(linear: Linear) -> Option<Linear> {
    match linear.terms.first() {
        Some((_, coefficient)) if *coefficient < 0 => linear.scaled(-1),
        _ => Some(linear),
    }
}

/// The reads of one place at one solved index tuple: that tuple, and the
/// index expressions of the class's first read.
type ReadClass<'a> = (Vec<Linear>, &'a [Linear]);

/// What read congruence asks of one branch.
enum ReadPair {
    /// Disequalities the units force, not yet among them.
    Derived(Vec<Unit>),
    /// Two reads that may be one element: equal, or apart at one differing
    /// position.
    Split {
        equal: Vec<Literal>,
        apart: Vec<Literal>,
    },
    /// Every pair is identical or settled apart.
    Settled,
}

/// Solved equalities: each solved atom's value over the free atoms, with
/// the splits it rests on. The free atoms range over every integer, so they
/// parametrize the equalities' integer solutions one to one.
#[derive(Default)]
struct Solved {
    solution: BTreeMap<AtomId, (Linear, Tags)>,
    /// The next atom a renaming introduces, past the problem's own.
    fresh: AtomId,
    /// The splits the first contradiction found rests on.
    contradiction: Option<Tags>,
}

impl Solved {
    /// Takes `difference == 0`, over free atoms, among the equalities:
    /// records a contradiction when it has no integer solution, and
    /// otherwise solves it for one atom, renaming atoms first until one has
    /// a unit coefficient. Whether it solved an atom.
    fn equate(&mut self, difference: Linear, why: Tags) -> Result<bool, Capacity> {
        let divisor = difference
            .terms
            .iter()
            .fold(0_i128, |gcd, (_, value)| greatest_divisor(gcd, value.abs()));
        if divisor == 0 || difference.constant % divisor != 0 {
            if difference.constant != 0 && self.contradiction.is_none() {
                self.contradiction = Some(why);
            }
            return Ok(false);
        }
        let mut difference = Linear {
            terms: difference
                .terms
                .iter()
                .map(|(atom, value)| (*atom, value / divisor))
                .collect(),
            constant: difference.constant / divisor,
        };
        loop {
            // Solve for the greatest atom with a unit coefficient.
            if let Some((atom, coefficient)) = difference
                .terms
                .iter()
                .rev()
                .find(|(_, coefficient)| coefficient.abs() == 1)
                .copied()
            {
                // atom * c + rest = 0, so atom = -rest / c with c = +-1.
                let mut rest = difference;
                rest.terms.retain(|(candidate, _)| *candidate != atom);
                let solution = rest.scaled(-coefficient).ok_or(Capacity::Arithmetic)?;
                self.assign(atom, solution, why)?;
                return Ok(true);
            }
            // No unit coefficient, and the coefficients are coprime: with w
            // the least coefficient in magnitude, on the greatest such atom
            // x, rename x = r - sum (a div w) y over the other atoms y. Then
            // w r + sum (a mod w) y + c = 0 has a smaller least coefficient,
            // and the renaming, a change of integer variables both ways,
            // rests on nothing.
            let (atom, weight) = difference
                .terms
                .iter()
                .rev()
                .min_by_key(|(_, coefficient)| coefficient.abs())
                .copied()
                .ok_or(Capacity::Arithmetic)?;
            let renamed = self.fresh;
            self.fresh = self.fresh.checked_add(1).ok_or(Capacity::Arithmetic)?;
            let mut value = Linear::atom(renamed);
            for (other, coefficient) in &difference.terms {
                if *other != atom {
                    let step = Linear::atom(*other)
                        .scaled(coefficient.div_euclid(weight))
                        .ok_or(Capacity::Arithmetic)?;
                    value = value.minus(&step).ok_or(Capacity::Arithmetic)?;
                }
            }
            difference = difference
                .substituted(&BTreeMap::from([(atom, value.clone())]))
                .ok_or(Capacity::Arithmetic)?;
            self.assign(atom, value, Tags::default())?;
        }
    }

    fn assign(&mut self, atom: AtomId, value: Linear, tags: Tags) -> Result<(), Capacity> {
        let single = BTreeMap::from([(atom, value.clone())]);
        for (solution, why) in self.solution.values_mut() {
            if solution.coefficient(atom) != 0 {
                *solution = solution.substituted(&single).ok_or(Capacity::Arithmetic)?;
                why.union(&tags);
            }
        }
        self.solution.insert(atom, (value, tags));
        Ok(())
    }
}

/// `linear` with every atom the solution solves replaced, and the splits
/// the replaced solutions rest on.
fn substituted(
    linear: &Linear,
    solution: &BTreeMap<AtomId, (Linear, Tags)>,
) -> Option<(Linear, Tags)> {
    let mut used = Tags::default();
    if !linear
        .terms
        .iter()
        .any(|(atom, _)| solution.contains_key(atom))
    {
        return Some((linear.clone(), used));
    }
    let mut out = Linear::constant(linear.constant);
    for (atom, coefficient) in &linear.terms {
        let term = match solution.get(atom) {
            Some((value, why)) => {
                used.union(why);
                value.scaled(*coefficient)?
            }
            None => Linear {
                terms: vec![(*atom, *coefficient)],
                constant: 0,
            },
        };
        out = out.plus(&term)?;
    }
    Some((out, used))
}

/// Fourier-Motzkin elimination of every atom: the splits a refutation of
/// the inequalities, each already tightened over the integers, rests on, or
/// `None` when they have a rational solution.
fn eliminate(inequalities: Vec<(Inequality, Tags)>) -> Result<Option<Tags>, Capacity> {
    let mut current: BTreeMap<Inequality, Tags> = BTreeMap::new();
    for (inequality, why) in inequalities {
        if inequality.contradictory() {
            return Ok(Some(why));
        }
        if !inequality.trivial() {
            current.entry(inequality).or_insert(why);
        }
    }
    loop {
        // The atom with the fewest products, least identity first.
        let mut counts: BTreeMap<AtomId, (usize, usize)> = BTreeMap::new();
        for inequality in current.keys() {
            for (atom, coefficient) in &inequality.0.terms {
                let entry = counts.entry(*atom).or_default();
                if *coefficient > 0 {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                }
            }
        }
        let Some(atom) = counts
            .iter()
            .min_by_key(|(atom, (upper, lower))| (upper * lower, **atom))
            .map(|(atom, _)| *atom)
        else {
            return Ok(None);
        };
        let mut upper = Vec::new();
        let mut lower = Vec::new();
        let mut next = BTreeMap::new();
        for (inequality, why) in current {
            let weight = inequality.0.coefficient(atom);
            if weight > 0 {
                upper.push((weight, inequality, why));
            } else if weight < 0 {
                lower.push((-weight, inequality, why));
            } else {
                next.entry(inequality).or_insert(why);
            }
        }
        for (up_weight, up, up_why) in &upper {
            for (low_weight, low, low_why) in &lower {
                let left = up.0.scaled(*low_weight).ok_or(Capacity::Arithmetic)?;
                let right = low.0.scaled(*up_weight).ok_or(Capacity::Arithmetic)?;
                let combined = Inequality(left.plus(&right).ok_or(Capacity::Arithmetic)?).reduced();
                let mut why = up_why.clone();
                why.union(low_why);
                if combined.contradictory() {
                    return Ok(Some(why));
                }
                if !combined.trivial() {
                    next.entry(combined).or_insert(why);
                }
            }
        }
        current = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atoms(problem: &mut Problem, kind: AtomKind) -> Linear {
        let id = problem.atoms.len() as AtomId;
        problem.atoms.push(kind);
        Linear::atom(id)
    }

    fn plain(problem: &mut Problem) -> Linear {
        atoms(problem, AtomKind::Plain)
    }

    fn read(problem: &mut Problem, place: u32, index: &Linear) -> Linear {
        atoms(
            problem,
            AtomKind::Read {
                place,
                indices: vec![index.clone()],
            },
        )
    }

    fn unit(problem: &mut Problem, left: &Linear, relation: Relation, right: &Linear) {
        problem
            .units
            .push(Literal::new(left.clone(), relation, right.clone()));
    }

    #[test]
    fn elimination_refutes_a_cycle() {
        let mut problem = Problem::default();
        let (x, y, z) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        unit(&mut problem, &x, Relation::Less, &y);
        unit(&mut problem, &y, Relation::Less, &z);
        unit(&mut problem, &z, Relation::Less, &x);
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn a_consistent_set_stays_open() {
        let mut problem = Problem::default();
        let (x, y) = (plain(&mut problem), plain(&mut problem));
        unit(&mut problem, &x, Relation::Less, &y);
        assert_eq!(problem.judge(), Ok(Verdict::Open));
    }

    #[test]
    fn a_left_inverse_separates_two_iterations() {
        // pos[order[i]] == i, pos[order[j]] == j, i != j, order[i] == order[j].
        let mut problem = Problem::default();
        let (i, j) = (plain(&mut problem), plain(&mut problem));
        let order_i = read(&mut problem, 0, &i);
        let order_j = read(&mut problem, 0, &j);
        let pos_i = read(&mut problem, 1, &order_i);
        let pos_j = read(&mut problem, 1, &order_j);
        unit(&mut problem, &pos_i, Relation::Equal, &i);
        unit(&mut problem, &pos_j, Relation::Equal, &j);
        unit(&mut problem, &i, Relation::NotEqual, &j);
        unit(&mut problem, &order_i, Relation::Equal, &order_j);
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn without_the_inverse_two_reads_stay_unseparated() {
        let mut problem = Problem::default();
        let (i, j) = (plain(&mut problem), plain(&mut problem));
        let order_i = read(&mut problem, 0, &i);
        let order_j = read(&mut problem, 0, &j);
        unit(&mut problem, &i, Relation::NotEqual, &j);
        unit(&mut problem, &order_i, Relation::Equal, &order_j);
        assert_eq!(problem.judge(), Ok(Verdict::Open));
    }

    #[test]
    fn congruence_follows_solved_indices() {
        // s == at + 1, pos[at + 1] == 5, pos[s] != 5 is contradictory.
        let mut problem = Problem::default();
        let (at, s) = (plain(&mut problem), plain(&mut problem));
        let next = at.plus_constant(1).unwrap();
        let pos_next = read(&mut problem, 0, &next);
        let pos_s = read(&mut problem, 0, &s);
        unit(&mut problem, &s, Relation::Equal, &next);
        unit(
            &mut problem,
            &pos_next,
            Relation::Equal,
            &Linear::constant(5),
        );
        unit(
            &mut problem,
            &pos_s,
            Relation::NotEqual,
            &Linear::constant(5),
        );
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn a_rule_fires_only_on_entailed_guards() {
        // fresh: c[b] != absent => c[b] < at. With c[b] == at < absent the
        // guard is entailed and the conclusion contradicts.
        let absent = Linear::constant(u64::MAX as i128);
        let mut problem = Problem::default();
        let (at, b) = (plain(&mut problem), plain(&mut problem));
        let c_b = read(&mut problem, 0, &b);
        unit(&mut problem, &at, Relation::Less, &absent);
        unit(&mut problem, &c_b, Relation::Equal, &at);
        problem.rules.push(Rule {
            guards: vec![Literal::new(
                c_b.clone(),
                Relation::NotEqual,
                absent.clone(),
            )],
            conclusions: vec![Literal::new(c_b.clone(), Relation::Less, at.clone())],
        });
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
        // Without `at < absent` the guard is not entailed and nothing fires.
        problem.units.remove(0);
        assert_eq!(problem.judge(), Ok(Verdict::Open));
    }

    #[test]
    fn a_write_choice_is_decided_or_split() {
        // c1 = c0 with c1[w] = v. Reading c1[q] with q < w is the old value.
        let mut problem = Problem::default();
        let (q, w, v) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        let new = read(&mut problem, 1, &q);
        let old = read(&mut problem, 0, &q);
        problem.choices.push(vec![
            vec![
                Literal::new(q.clone(), Relation::Equal, w.clone()),
                Literal::new(new.clone(), Relation::Equal, v.clone()),
            ],
            vec![
                Literal::new(q.clone(), Relation::NotEqual, w.clone()),
                Literal::new(new.clone(), Relation::Equal, old.clone()),
            ],
        ]);
        unit(&mut problem, &q, Relation::Less, &w);
        unit(&mut problem, &new, Relation::NotEqual, &old);
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
        // Undecided q: both branches must refute; the hit branch does not.
        problem.units.remove(0);
        assert_eq!(problem.judge(), Ok(Verdict::Open));
        // With v == old as well, both branches refute.
        unit(&mut problem, &v, Relation::Equal, &old);
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn a_disequality_splits_when_strictness_matters() {
        // x != 0, x >= 0, x < 1 is contradictory only by splitting x != 0.
        let mut problem = Problem::default();
        let x = plain(&mut problem);
        let zero = Linear::constant(0);
        unit(&mut problem, &x, Relation::NotEqual, &zero);
        unit(&mut problem, &x, Relation::GreaterEqual, &zero);
        unit(&mut problem, &x, Relation::Less, &Linear::constant(1));
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn a_frontier_element_splits_on_its_read() {
        // The back edge of `for (at ...)` with `invariant forall up(e in
        // 0..at) when p[e] < n: p[e] < e`: the new element e == at is
        // covered by the body's path condition on p[at], the old ones by the
        // old fact.
        let mut problem = Problem::default();
        let (n, at, e, selector) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        let p_at = read(&mut problem, 0, &at);
        let p_e = read(&mut problem, 0, &e);
        let top = Linear::constant(u64::MAX as i128);
        unit(
            &mut problem,
            &n,
            Relation::LessEqual,
            &Linear::constant(4_294_967_294),
        );
        unit(
            &mut problem,
            &e,
            Relation::GreaterEqual,
            &Linear::constant(0),
        );
        unit(
            &mut problem,
            &e,
            Relation::Less,
            &at.plus_constant(1).unwrap(),
        );
        unit(&mut problem, &p_e, Relation::Less, &n);
        unit(&mut problem, &p_e, Relation::GreaterEqual, &e);
        problem.choices.push(vec![
            vec![
                Literal::new(selector.clone(), Relation::Equal, Linear::constant(0)),
                Literal::new(p_at.clone(), Relation::Equal, top.clone()),
            ],
            vec![
                Literal::new(selector.clone(), Relation::Equal, Linear::constant(1)),
                Literal::new(p_at.clone(), Relation::NotEqual, top.clone()),
                Literal::new(p_at.clone(), Relation::Less, at.clone()),
            ],
        ]);
        problem.rules.push(Rule {
            guards: vec![
                Literal::new(e.clone(), Relation::GreaterEqual, Linear::constant(0)),
                Literal::new(e.clone(), Relation::Less, at.clone()),
                Literal::new(p_e.clone(), Relation::Less, n.clone()),
            ],
            conclusions: vec![Literal::new(p_e.clone(), Relation::Less, e.clone())],
        });
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn a_derivation_runs_every_branch_to_completion() {
        // Thirteen independent two-way choices and a contradiction that only
        // the leaves' disequality splits find: all 2^13 leaves are judged,
        // with no branch budget to stop short of the answer.
        let mut problem = Problem::default();
        let x = plain(&mut problem);
        unit(
            &mut problem,
            &x,
            Relation::GreaterEqual,
            &Linear::constant(0),
        );
        unit(&mut problem, &x, Relation::LessEqual, &Linear::constant(1));
        unit(&mut problem, &x, Relation::NotEqual, &Linear::constant(0));
        unit(&mut problem, &x, Relation::NotEqual, &Linear::constant(1));
        for _ in 0..13 {
            let y = plain(&mut problem);
            problem.choices.push(vec![
                vec![Literal::new(
                    y.clone(),
                    Relation::Equal,
                    Linear::constant(0),
                )],
                vec![Literal::new(y, Relation::Equal, Linear::constant(1))],
            ]);
        }
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn elimination_does_not_tighten_what_it_derives() {
        // a + b <= 1, a - b <= 0, 2a >= 1 + e and e >= 0 have no integer
        // solution: the first two give 2a <= 1. Tightening that derived
        // inequality to a <= 0 refutes the set when b is eliminated first,
        // but eliminating a first derives 2e <= 0 and nothing false, so
        // tightening what elimination derives makes the verdict depend on
        // the order. Only the literals are tightened, each of which here
        // already has coprime coefficients, and the rational solution
        // a = b = 1/2, e = 0 leaves the set open in every order.
        let mut problem = Problem::default();
        let (a, b, e) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        let zero = Linear::constant(0);
        unit(
            &mut problem,
            &a.plus(&b).unwrap(),
            Relation::LessEqual,
            &Linear::constant(1),
        );
        unit(&mut problem, &a, Relation::LessEqual, &b);
        unit(
            &mut problem,
            &a.scaled(2).unwrap(),
            Relation::GreaterEqual,
            &e.plus_constant(1).unwrap(),
        );
        unit(&mut problem, &e, Relation::GreaterEqual, &zero);
        assert_eq!(problem.judge(), Ok(Verdict::Open));
        // A literal is tightened: 2a - 2b <= 1 is a - b <= 0.
        let mut tightened = Problem::default();
        let (a, b) = (plain(&mut tightened), plain(&mut tightened));
        let twice = a.scaled(2).unwrap().minus(&b.scaled(2).unwrap()).unwrap();
        unit(
            &mut tightened,
            &twice,
            Relation::LessEqual,
            &Linear::constant(1),
        );
        unit(&mut tightened, &a, Relation::Greater, &b);
        assert_eq!(tightened.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn an_elimination_past_i128_is_the_arithmetic_capacity() {
        // p*a >= q*b and q*a < p*b with p = 2^120 and q = p + 1, which are
        // coprime: eliminating either atom multiplies p by q, about 2^240.
        let mut problem = Problem::default();
        let (a, b) = (plain(&mut problem), plain(&mut problem));
        let p = 1_i128 << 120;
        let q = p + 1;
        unit(
            &mut problem,
            &a.scaled(p).unwrap(),
            Relation::GreaterEqual,
            &b.scaled(q).unwrap(),
        );
        unit(
            &mut problem,
            &a.scaled(q).unwrap(),
            Relation::Less,
            &b.scaled(p).unwrap(),
        );
        assert_eq!(problem.judge(), Err(Capacity::Arithmetic));
    }

    #[test]
    fn normalization_tightens_over_the_integers() {
        // 2x <= 1 means x <= 0 over the integers.
        let x = Linear::atom(0);
        let inequality = Inequality::at_most(&x.scaled(2).unwrap(), &Linear::constant(1)).unwrap();
        assert_eq!(inequality.0.terms, vec![(0, 1)]);
        assert_eq!(inequality.0.constant, 0);
    }

    /// x != 0, x >= 0, x < 1 under `choices` two-way choices of independent
    /// atoms, each alternative consistent.
    fn independent_choices(choices: usize) -> Problem {
        let mut problem = Problem::default();
        for _ in 0..choices {
            let y = plain(&mut problem);
            problem.choices.push(vec![
                vec![Literal::new(
                    y.clone(),
                    Relation::Equal,
                    Linear::constant(0),
                )],
                vec![Literal::new(y, Relation::Equal, Linear::constant(1))],
            ]);
        }
        let x = plain(&mut problem);
        let zero = Linear::constant(0);
        unit(&mut problem, &x, Relation::NotEqual, &zero);
        unit(&mut problem, &x, Relation::GreaterEqual, &zero);
        unit(&mut problem, &x, Relation::Less, &Linear::constant(1));
        problem
    }

    #[test]
    fn backjumping_skips_the_splits_a_refutation_does_not_use() {
        // The refutation below the last choice rests on the disequality's
        // split alone, so no other alternative of any choice is tried: one
        // branch per depth, not one per combination of the choices.
        let problem = independent_choices(12);
        let run = Run::backjumping();
        assert_eq!(problem.judge_in(&run), Ok(Verdict::Refuted));
        assert_eq!(run.nodes.get(), 13);
    }

    /// One two-way choice whose first alternative is refuted only below a
    /// further split, by a contradiction that rests on that alternative, and
    /// whose second stays open: the second must still be tried, so a
    /// contradiction that lost the alternative it rests on would backjump
    /// past it and refute the problem.
    fn assert_rests_on_the_choice(problem: &Problem) {
        assert_eq!(problem.judge(), Ok(Verdict::Open));
    }

    #[test]
    fn a_refutation_through_a_solved_equality_rests_on_its_split() {
        // z == 0 or z == 5; x != z, x >= 0, x < 1. z == 0 refutes only after
        // the disequality splits, through the solution of z.
        let mut problem = Problem::default();
        let (z, x) = (plain(&mut problem), plain(&mut problem));
        problem.choices.push(vec![
            vec![Literal::new(
                z.clone(),
                Relation::Equal,
                Linear::constant(0),
            )],
            vec![Literal::new(
                z.clone(),
                Relation::Equal,
                Linear::constant(5),
            )],
        ]);
        unit(&mut problem, &x, Relation::NotEqual, &z);
        unit(
            &mut problem,
            &x,
            Relation::GreaterEqual,
            &Linear::constant(0),
        );
        unit(&mut problem, &x, Relation::Less, &Linear::constant(1));
        assert_rests_on_the_choice(&problem);
    }

    #[test]
    fn a_refutation_through_elimination_rests_on_its_split() {
        // z >= 1 or z <= -1; y != 1, z <= y <= 1. z >= 1 refutes only after
        // the disequality splits, by eliminating z.
        let mut problem = Problem::default();
        let (z, y) = (plain(&mut problem), plain(&mut problem));
        problem.choices.push(vec![
            vec![Literal::new(
                z.clone(),
                Relation::GreaterEqual,
                Linear::constant(1),
            )],
            vec![Literal::new(
                z.clone(),
                Relation::LessEqual,
                Linear::constant(-1),
            )],
        ]);
        unit(&mut problem, &y, Relation::NotEqual, &Linear::constant(1));
        unit(&mut problem, &y, Relation::GreaterEqual, &z);
        unit(&mut problem, &y, Relation::LessEqual, &Linear::constant(1));
        assert_rests_on_the_choice(&problem);
    }

    #[test]
    fn a_refutation_through_read_congruence_rests_on_its_split() {
        // i == 0 or i == 1; y == p[i], y != 5, 5 <= p[0] < 6. i == 0 refutes
        // only after the disequality splits, through p[i] being p[0].
        let mut problem = Problem::default();
        let (i, y) = (plain(&mut problem), plain(&mut problem));
        let zero = Linear::constant(0);
        let at_i = read(&mut problem, 0, &i);
        let at_zero = read(&mut problem, 0, &zero);
        problem.choices.push(vec![
            vec![Literal::new(i.clone(), Relation::Equal, zero.clone())],
            vec![Literal::new(
                i.clone(),
                Relation::Equal,
                Linear::constant(1),
            )],
        ]);
        unit(&mut problem, &y, Relation::Equal, &at_i);
        unit(&mut problem, &y, Relation::NotEqual, &Linear::constant(5));
        unit(
            &mut problem,
            &at_zero,
            Relation::GreaterEqual,
            &Linear::constant(5),
        );
        unit(&mut problem, &at_zero, Relation::Less, &Linear::constant(6));
        assert_rests_on_the_choice(&problem);
    }

    /// x == 3z and x == -2y, in either order, with x == 3: the integer
    /// solutions need x even and a multiple of three.
    fn equalities_in_order(three_first: bool) -> Problem {
        let mut problem = Problem::default();
        let (x, y, z) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        let (three_z, minus_two_y) = (z.scaled(3).unwrap(), y.scaled(-2).unwrap());
        let (first, second) = if three_first {
            (three_z, minus_two_y)
        } else {
            (minus_two_y, three_z)
        };
        unit(&mut problem, &x, Relation::Equal, &first);
        unit(&mut problem, &x, Relation::Equal, &second);
        unit(&mut problem, &x, Relation::Equal, &Linear::constant(3));
        problem
    }

    #[test]
    fn the_theory_does_not_depend_on_the_order_of_its_equalities() {
        // Solving x == 3z first leaves 3z + 2y == 0 without a unit
        // coefficient; it is solved over the integers all the same.
        assert_eq!(equalities_in_order(true).judge(), Ok(Verdict::Refuted));
        assert_eq!(equalities_in_order(false).judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn an_equality_without_a_unit_coefficient_is_solved_over_the_integers() {
        // 2x + 3y == 1 with x and y in [0, 1] has the rational solution
        // (1/2, 0) and no integer one.
        let mut problem = Problem::default();
        let (x, y) = (plain(&mut problem), plain(&mut problem));
        let sum = x.scaled(2).unwrap().plus(&y.scaled(3).unwrap()).unwrap();
        unit(&mut problem, &sum, Relation::Equal, &Linear::constant(1));
        for atom in [&x, &y] {
            unit(
                &mut problem,
                atom,
                Relation::GreaterEqual,
                &Linear::constant(0),
            );
            unit(
                &mut problem,
                atom,
                Relation::LessEqual,
                &Linear::constant(1),
            );
        }
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
        // With x in [0, 2] and y in [-1, 1], (2, -1) is an integer
        // solution.
        let mut open = Problem::default();
        let (x, y) = (plain(&mut open), plain(&mut open));
        let sum = x.scaled(2).unwrap().plus(&y.scaled(3).unwrap()).unwrap();
        unit(&mut open, &sum, Relation::Equal, &Linear::constant(1));
        unit(&mut open, &x, Relation::GreaterEqual, &Linear::constant(0));
        unit(&mut open, &x, Relation::LessEqual, &Linear::constant(2));
        unit(&mut open, &y, Relation::GreaterEqual, &Linear::constant(-1));
        unit(&mut open, &y, Relation::LessEqual, &Linear::constant(1));
        assert_eq!(open.judge(), Ok(Verdict::Open));
    }

    #[test]
    fn reads_at_strided_indices_settle() {
        // p[2i] and p[3j]: tried equal, 2i == 3j is solved, so the pair is
        // then one element; tried apart, it is separated. Before equalities
        // were solved over the integers, the equal branch left the pair as
        // open as before and split it again without end.
        let mut problem = Problem::default();
        let (i, j) = (plain(&mut problem), plain(&mut problem));
        let first = read(&mut problem, 0, &i.scaled(2).unwrap());
        let second = read(&mut problem, 0, &j.scaled(3).unwrap());
        unit(&mut problem, &first, Relation::NotEqual, &second);
        unit(&mut problem, &i, Relation::Equal, &Linear::constant(3));
        assert_eq!(problem.judge(), Ok(Verdict::Open));
        unit(&mut problem, &j, Relation::Equal, &Linear::constant(2));
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    #[test]
    fn a_pair_forced_apart_at_two_positions_is_split_over_them() {
        // p[a, b] != p[c, d], so the pair is not one element; a <= c <= a
        // and b <= d <= b leave no position where it can differ, which only
        // orienting each position's disequality shows.
        let mut problem = Problem::default();
        let (a, b, c, d) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        let two = |problem: &mut Problem, first: &Linear, second: &Linear| {
            atoms(
                problem,
                AtomKind::Read {
                    place: 0,
                    indices: vec![first.clone(), second.clone()],
                },
            )
        };
        let left = two(&mut problem, &a, &b);
        let right = two(&mut problem, &c, &d);
        unit(&mut problem, &left, Relation::NotEqual, &right);
        for (one, other) in [(&a, &c), (&b, &d)] {
            unit(&mut problem, one, Relation::LessEqual, other);
            unit(&mut problem, one, Relation::GreaterEqual, other);
        }
        assert_eq!(problem.judge(), Ok(Verdict::Refuted));
    }

    /// The splits a contradiction among `units`, each resting on the given
    /// split depths, rests on.
    fn core(problem: &Problem, units: &[(Literal, &[usize])]) -> Option<Tags> {
        let units: Vec<Unit> = units
            .iter()
            .map(|(literal, depths)| {
                let mut tags = Tags::default();
                for depth in *depths {
                    tags.insert(*depth);
                }
                Unit {
                    literal: literal.clone(),
                    tags,
                }
            })
            .collect();
        problem.contradiction(&units).unwrap()
    }

    #[test]
    fn an_equality_without_an_integer_solution_rests_on_its_literals() {
        // x == 2y + w, x == 2z and w == 1: 2z == 2y + 1 has no integer
        // solution, through w == 1 alone of the split-resting literals.
        let mut problem = Problem::default();
        let (x, y, z, w) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        let core = core(
            &problem,
            &[
                (
                    Literal::new(
                        x.clone(),
                        Relation::Equal,
                        y.scaled(2).unwrap().plus(&w).unwrap(),
                    ),
                    &[],
                ),
                (
                    Literal::new(x.clone(), Relation::Equal, z.scaled(2).unwrap()),
                    &[],
                ),
                (Literal::new(w, Relation::Equal, Linear::constant(1)), &[4]),
                (Literal::new(x, Relation::GreaterEqual, z), &[7]),
            ],
        )
        .expect("contradictory");
        assert!(core.contains(4));
        assert!(!core.contains(7));
    }

    #[test]
    fn an_equality_solved_after_renaming_rests_on_its_literals() {
        // 2x + 3y == 1, resting on split 2, with x and y in [0, 1].
        let mut problem = Problem::default();
        let (x, y) = (plain(&mut problem), plain(&mut problem));
        let sum = x.scaled(2).unwrap().plus(&y.scaled(3).unwrap()).unwrap();
        let mut units = vec![(
            Literal::new(sum, Relation::Equal, Linear::constant(1)),
            &[2][..],
        )];
        for atom in [&x, &y] {
            units.push((
                Literal::new(atom.clone(), Relation::GreaterEqual, Linear::constant(0)),
                &[],
            ));
            units.push((
                Literal::new(atom.clone(), Relation::LessEqual, Linear::constant(1)),
                &[],
            ));
        }
        assert!(core(&problem, &units).expect("contradictory").contains(2));
    }

    #[test]
    fn a_split_without_the_equal_case_keeps_why_it_was_excluded() {
        // p[a, b] and p[c, d] with a <= c <= a and b <= d <= b. Where the
        // choice takes p[a, b] != p[c, d], the pair cannot be one element
        // and is split over its positions, both refuted; the refutation
        // rests on that choice through the excluded equal case, so the
        // choice's other alternative, where a == c, b == d is a model, must
        // still be tried.
        let mut problem = Problem::default();
        let (a, b, c, d) = (
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
            plain(&mut problem),
        );
        let left = two_index_read(&mut problem, &a, &b);
        let right = two_index_read(&mut problem, &c, &d);
        for (one, other) in [(&a, &c), (&b, &d)] {
            unit(&mut problem, one, Relation::LessEqual, other);
            unit(&mut problem, one, Relation::GreaterEqual, other);
        }
        problem.choices.push(vec![
            vec![Literal::new(
                left.clone(),
                Relation::NotEqual,
                right.clone(),
            )],
            vec![Literal::new(left, Relation::Equal, right)],
        ]);
        assert_eq!(problem.judge(), Ok(Verdict::Open));
    }

    fn two_index_read(problem: &mut Problem, first: &Linear, second: &Linear) -> Linear {
        atoms(
            problem,
            AtomKind::Read {
                place: 0,
                indices: vec![first.clone(), second.clone()],
            },
        )
    }

    /// A deterministic stream of small numbers.
    struct Draw(u64);

    impl Draw {
        fn below(&mut self, bound: usize) -> usize {
            // xorshift64
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % bound as u64) as usize
        }

        fn literal(&mut self, terms: &[Linear]) -> Literal {
            let relations = [
                Relation::Equal,
                Relation::NotEqual,
                Relation::Less,
                Relation::LessEqual,
                Relation::Greater,
                Relation::GreaterEqual,
            ];
            let left = terms[self.below(terms.len())].clone();
            let right = if self.below(3) == 0 {
                Linear::constant(self.below(4) as i128 - 1)
            } else {
                terms[self.below(terms.len())]
                    .plus_constant(self.below(3) as i128 - 1)
                    .unwrap()
            };
            Literal::new(left, relations[self.below(6)], right)
        }
    }

    /// A small problem: four plain atoms, reads of a two-index place and of
    /// a one-index place at them, units with some index positions pinned by
    /// `<=` both ways, two-way choices and a rule.
    fn drawn_problem(draw: &mut Draw) -> Problem {
        let mut problem = Problem::default();
        let plains: Vec<Linear> = (0..4).map(|_| plain(&mut problem)).collect();
        let mut terms = plains.clone();
        for _ in 0..2 + draw.below(2) {
            let (first, second) = (plains[draw.below(4)].clone(), plains[draw.below(4)].clone());
            terms.push(two_index_read(&mut problem, &first, &second));
        }
        for _ in 0..draw.below(2) {
            let index = plains[draw.below(4)]
                .plus_constant(draw.below(2) as i128)
                .unwrap();
            terms.push(read(&mut problem, 1, &index));
        }
        for _ in 0..draw.below(3) {
            let (one, other) = (draw.below(4), draw.below(4));
            unit(
                &mut problem,
                &plains[one],
                Relation::LessEqual,
                &plains[other],
            );
            unit(
                &mut problem,
                &plains[one],
                Relation::GreaterEqual,
                &plains[other],
            );
        }
        for _ in 0..1 + draw.below(4) {
            let literal = draw.literal(&terms);
            problem.units.push(literal);
        }
        for _ in 0..draw.below(3) {
            let alternatives = vec![vec![draw.literal(&terms)], vec![draw.literal(&terms)]];
            problem.choices.push(alternatives);
        }
        if draw.below(2) == 0 {
            problem.rules.push(Rule {
                guards: vec![draw.literal(&terms)],
                conclusions: vec![draw.literal(&terms)],
            });
        }
        problem
    }

    #[test]
    fn backjumping_keeps_the_full_case_analysis_verdict() {
        let mut draw = Draw(0x9e37_79b9_7f4a_7c15);
        for index in 0..4000 {
            let problem = drawn_problem(&mut draw);
            let full = Run {
                nodes: Cell::new(0),
                backjump: false,
            };
            assert_eq!(
                problem.judge(),
                problem.judge_in(&full),
                "problem {index}: {problem:?}"
            );
        }
    }
}
