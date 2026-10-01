//! [RANGE-3] the fixed quantifier-free judgment of range proofs.
//!
//! A query is a finite problem over integer atoms: unit literals that hold,
//! choices of which exactly one alternative holds, and rules whose
//! conclusions hold once their guards do. The judgment asks whether the
//! problem has no model, and it runs to completion:
//!
//! 1. **Theory.** A set of literals is contradictory when, after every
//!    equality with a unit coefficient is solved for its greatest atom and
//!    every element read is identified with every other read of the same
//!    place at the same solved indices (read congruence), a solved
//!    disequality is `0 != 0`, a solved constant comparison is false, or
//!    the remaining inequalities, each tightened over the integers as it is
//!    formed, have no rational solution, which Fourier-Motzkin elimination
//!    decides in any order since it tightens nothing it derives.
//! 2. **Saturation.** A choice none of whose other alternatives is
//!    consistent with the units asserts its one consistent alternative, and a
//!    rule whose every guard the units entail asserts its conclusions. A
//!    guard is entailed when the units with its negation are contradictory.
//! 3. **Split.** When saturation leaves a choice undecided, each consistent
//!    alternative is tried in written order; then each disequality the units
//!    hold is tried as `<` and as `>`. The problem has no model when every
//!    branch is contradictory.
//!
//! Nothing here searches for an instance or a lemma: every rule and choice
//! comes with the problem, and the only branching is over the problem's own
//! finite alternatives, run to completion. The two structural ceilings are
//! the caller's, on the problem's atoms and instances; the only stop here is
//! this checker's `i128` arithmetic, a capability limit of the checker.

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
        let units = self.units.clone();
        let decided = vec![false; self.choices.len()];
        let fired = vec![false; self.rules.len()];
        self.search(units, decided, fired)
    }

    fn search(
        &self,
        mut units: Vec<Literal>,
        mut decided: Vec<bool>,
        mut fired: Vec<bool>,
    ) -> Result<Verdict, Capacity> {
        // Saturation: decide what the units force, fire what they entail.
        loop {
            if self.contradictory(&units)? {
                return Ok(Verdict::Refuted);
            }
            let mut changed = false;
            for (index, choice) in self.choices.iter().enumerate() {
                if decided[index] {
                    continue;
                }
                let mut open = Vec::new();
                for alternative in choice {
                    let mut with = units.clone();
                    with.extend(alternative.iter().cloned());
                    if !self.contradictory(&with)? {
                        open.push(alternative);
                    }
                }
                match open.as_slice() {
                    [] => return Ok(Verdict::Refuted),
                    [only] => {
                        units.extend(only.iter().cloned());
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
                let mut entailed = true;
                for guard in &rule.guards {
                    if !self.entails(&units, guard)? {
                        entailed = false;
                        break;
                    }
                }
                if entailed {
                    units.extend(rule.conclusions.iter().cloned());
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
            for alternative in &self.choices[index] {
                let mut with = units.clone();
                with.extend(alternative.iter().cloned());
                if self.contradictory(&with)? {
                    continue;
                }
                match self.search(with, decided.clone(), fired.clone())? {
                    Verdict::Refuted => {}
                    Verdict::Open => return Ok(Verdict::Open),
                }
            }
            return Ok(Verdict::Refuted);
        }
        // Then on the first two reads of one place whose index tuples the
        // units neither identify nor separate: read congruence asks exactly
        // whether they are one element.
        let solved = self.solve(&units)?;
        match self.open_read_pair(&units, &solved)? {
            ReadPair::Derived(derived) => {
                let mut with = units.clone();
                with.extend(derived);
                return self.search(with, decided, fired);
            }
            ReadPair::Split { equal, apart } => {
                let mut with = units.clone();
                with.extend(equal);
                match self.search(with, decided.clone(), fired.clone())? {
                    Verdict::Refuted => {}
                    Verdict::Open => return Ok(Verdict::Open),
                }
                for literal in apart {
                    let mut with = units.clone();
                    with.push(literal);
                    match self.search(with, decided.clone(), fired.clone())? {
                        Verdict::Refuted => {}
                        Verdict::Open => return Ok(Verdict::Open),
                    }
                }
                return Ok(Verdict::Refuted);
            }
            ReadPair::Settled => {}
        }
        // Then on the first disequality the theory left open.
        for (position, literal) in units.iter().enumerate() {
            if literal.relation != Relation::NotEqual {
                continue;
            }
            let Some(difference) = literal
                .left
                .minus(&literal.right)
                .and_then(|difference| difference.substituted(&solved.solution))
            else {
                return Err(Capacity::Arithmetic);
            };
            if difference.is_constant() {
                continue;
            }
            for relation in [Relation::Less, Relation::Greater] {
                let mut with = units.clone();
                with[position] =
                    Literal::new(literal.left.clone(), relation, literal.right.clone());
                match self.search(with, decided.clone(), fired.clone())? {
                    Verdict::Refuted => {}
                    Verdict::Open => return Ok(Verdict::Open),
                }
            }
            return Ok(Verdict::Refuted);
        }
        Ok(Verdict::Open)
    }

    /// Read congruence's question about the reads of one place: the
    /// disequalities the units already force between two index tuples that
    /// differ in one position, or else the first pair, in atom order, that
    /// may be one element and is not identical.
    fn open_read_pair(&self, units: &[Literal], solved: &Solved) -> Result<ReadPair, Capacity> {
        let mut by_place: BTreeMap<u32, Vec<Vec<Linear>>> = BTreeMap::new();
        for kind in &self.atoms {
            let AtomKind::Read { place, indices } = kind else {
                continue;
            };
            let mut reduced = Vec::with_capacity(indices.len());
            for index in indices {
                reduced.push(
                    index
                        .substituted(&solved.solution)
                        .ok_or(Capacity::Arithmetic)?,
                );
            }
            let tuples = by_place.entry(*place).or_default();
            if !tuples.contains(&reduced) {
                tuples.push(reduced);
            }
        }
        let mut derived = Vec::new();
        let mut split = None;
        for tuples in by_place.values() {
            for (position, first) in tuples.iter().enumerate() {
                for second in &tuples[position + 1..] {
                    if first.len() != second.len() {
                        continue;
                    }
                    let differing: Vec<(&Linear, &Linear)> = first
                        .iter()
                        .zip(second)
                        .filter(|(left, right)| left != right)
                        .collect();
                    let equal: Vec<Literal> = differing
                        .iter()
                        .map(|(left, right)| {
                            Literal::new((*left).clone(), Relation::Equal, (*right).clone())
                        })
                        .collect();
                    let apart: Vec<Literal> = differing
                        .iter()
                        .map(|(left, right)| {
                            Literal::new((*left).clone(), Relation::NotEqual, (*right).clone())
                        })
                        .collect();
                    let mut with = units.to_vec();
                    with.extend(equal.iter().cloned());
                    if self.contradictory(&with)? {
                        // Forced apart: one differing position is a unit.
                        if let [single] = apart.as_slice()
                            && !already_apart(units, single)
                        {
                            derived.push(single.clone());
                        }
                        continue;
                    }
                    if split.is_none() {
                        split = Some(ReadPair::Split { equal, apart });
                    }
                }
            }
        }
        if !derived.is_empty() {
            return Ok(ReadPair::Derived(derived));
        }
        Ok(split.unwrap_or(ReadPair::Settled))
    }

    /// Whether the units entail `literal`.
    pub(crate) fn entails(&self, units: &[Literal], literal: &Literal) -> Result<bool, Capacity> {
        for negation in literal.negation() {
            let mut with = units.to_vec();
            with.push(negation);
            if !self.contradictory(&with)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Solves unit-coefficient equalities and closes read congruence.
    fn solve(&self, literals: &[Literal]) -> Result<Solved, Capacity> {
        let mut solved = Solved::default();
        let mut pending: Vec<Linear> = Vec::new();
        for literal in literals {
            if literal.relation == Relation::Equal {
                pending.push(
                    literal
                        .left
                        .minus(&literal.right)
                        .ok_or(Capacity::Arithmetic)?,
                );
            }
        }
        loop {
            let mut progressed = false;
            let mut kept = Vec::new();
            for difference in pending.drain(..) {
                let difference = difference
                    .substituted(&solved.solution)
                    .ok_or(Capacity::Arithmetic)?;
                if difference.is_constant() {
                    if difference.constant != 0 {
                        solved.contradiction = true;
                    }
                    continue;
                }
                // Solve for the greatest atom with a unit coefficient.
                let Some((atom, coefficient)) = difference
                    .terms
                    .iter()
                    .rev()
                    .find(|(_, coefficient)| coefficient.abs() == 1)
                    .copied()
                else {
                    kept.push(difference);
                    continue;
                };
                // atom * c + rest = 0, so atom = -rest / c with c = +-1.
                let mut rest = difference.clone();
                rest.terms.retain(|(candidate, _)| *candidate != atom);
                let solution = rest.scaled(-coefficient).ok_or(Capacity::Arithmetic)?;
                solved.assign(atom, solution)?;
                progressed = true;
            }
            pending = kept;
            // Read congruence: one place read at one solved index tuple is
            // one value.
            let mut seen: BTreeMap<(u8, u32, Vec<Linear>), AtomId> = BTreeMap::new();
            for (index, kind) in self.atoms.iter().enumerate() {
                let atom = index as AtomId;
                let key = match kind {
                    AtomKind::Plain => continue,
                    AtomKind::Read { place, indices } => {
                        let mut reduced = Vec::with_capacity(indices.len());
                        for index in indices {
                            reduced.push(
                                index
                                    .substituted(&solved.solution)
                                    .ok_or(Capacity::Arithmetic)?,
                            );
                        }
                        (0_u8, *place, reduced)
                    }
                    AtomKind::SegmentLength { place, segment } => (
                        1_u8,
                        *place,
                        vec![
                            segment
                                .substituted(&solved.solution)
                                .ok_or(Capacity::Arithmetic)?,
                        ],
                    ),
                };
                match seen.get(&key) {
                    Some(other) => {
                        let difference = Linear::atom(atom)
                            .minus(&Linear::atom(*other))
                            .and_then(|difference| difference.substituted(&solved.solution))
                            .ok_or(Capacity::Arithmetic)?;
                        if !(difference.is_constant() && difference.constant == 0) {
                            pending.push(difference);
                            progressed = true;
                        }
                    }
                    None => {
                        seen.insert(key, atom);
                    }
                }
            }
            if !progressed || solved.contradiction {
                solved.unsolved = pending;
                return Ok(solved);
            }
        }
    }

    /// Whether the literals are contradictory in the theory.
    pub(crate) fn contradictory(&self, literals: &[Literal]) -> Result<bool, Capacity> {
        let solved = self.solve(literals)?;
        if solved.contradiction {
            return Ok(true);
        }
        let mut inequalities = Vec::new();
        for difference in &solved.unsolved {
            inequalities.push(Inequality(difference.clone()).normalized());
            inequalities
                .push(Inequality(difference.scaled(-1).ok_or(Capacity::Arithmetic)?).normalized());
        }
        for literal in literals {
            let left = literal
                .left
                .substituted(&solved.solution)
                .ok_or(Capacity::Arithmetic)?;
            let right = literal
                .right
                .substituted(&solved.solution)
                .ok_or(Capacity::Arithmetic)?;
            match literal.relation {
                Relation::Equal => {}
                Relation::NotEqual => {
                    let difference = left.minus(&right).ok_or(Capacity::Arithmetic)?;
                    if difference.is_constant() && difference.constant == 0 {
                        return Ok(true);
                    }
                }
                Relation::LessEqual => {
                    inequalities
                        .push(Inequality::at_most(&left, &right).ok_or(Capacity::Arithmetic)?);
                }
                Relation::GreaterEqual => {
                    inequalities
                        .push(Inequality::at_most(&right, &left).ok_or(Capacity::Arithmetic)?);
                }
                Relation::Less => {
                    inequalities
                        .push(Inequality::below(&left, &right).ok_or(Capacity::Arithmetic)?);
                }
                Relation::Greater => {
                    inequalities
                        .push(Inequality::below(&right, &left).ok_or(Capacity::Arithmetic)?);
                }
            }
        }
        eliminate(inequalities)
    }
}

/// Whether a unit already states `literal`'s two sides apart: a
/// disequality or a strict order between the same two sides, either way.
fn already_apart(units: &[Literal], literal: &Literal) -> bool {
    let Some(difference) = literal.left.minus(&literal.right) else {
        return false;
    };
    let Some(negated) = difference.scaled(-1) else {
        return false;
    };
    units.iter().any(|unit| {
        matches!(
            unit.relation,
            Relation::NotEqual | Relation::Less | Relation::Greater
        ) && unit
            .left
            .minus(&unit.right)
            .is_some_and(|other| other == difference || other == negated)
    })
}

/// What read congruence asks of one branch.
enum ReadPair {
    /// Disequalities the units force, not yet among them.
    Derived(Vec<Literal>),
    /// Two reads that may be one element: equal, or apart at one position.
    Split {
        equal: Vec<Literal>,
        apart: Vec<Literal>,
    },
    /// Every pair is identical or forced apart.
    Settled,
}

/// Solved equalities: each solved atom's value over unsolved atoms.
#[derive(Default)]
struct Solved {
    solution: BTreeMap<AtomId, Linear>,
    /// Equalities without a unit coefficient, as differences equal to zero.
    unsolved: Vec<Linear>,
    contradiction: bool,
}

impl Solved {
    fn assign(&mut self, atom: AtomId, value: Linear) -> Result<(), Capacity> {
        let single = BTreeMap::from([(atom, value.clone())]);
        for solution in self.solution.values_mut() {
            *solution = solution.substituted(&single).ok_or(Capacity::Arithmetic)?;
        }
        self.solution.insert(atom, value);
        Ok(())
    }
}

/// Fourier-Motzkin elimination of every atom: whether the inequalities,
/// each already tightened over the integers, have no rational solution.
fn eliminate(inequalities: Vec<Inequality>) -> Result<bool, Capacity> {
    let mut current: BTreeSet<Inequality> = BTreeSet::new();
    for inequality in inequalities {
        if inequality.contradictory() {
            return Ok(true);
        }
        if !inequality.trivial() {
            current.insert(inequality);
        }
    }
    loop {
        // The atom with the fewest products, least identity first.
        let mut counts: BTreeMap<AtomId, (usize, usize)> = BTreeMap::new();
        for inequality in &current {
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
            return Ok(false);
        };
        let mut upper = Vec::new();
        let mut lower = Vec::new();
        let mut next = BTreeSet::new();
        for inequality in current {
            let weight = inequality.0.coefficient(atom);
            if weight > 0 {
                upper.push((weight, inequality));
            } else if weight < 0 {
                lower.push((-weight, inequality));
            } else {
                next.insert(inequality);
            }
        }
        for (up_weight, up) in &upper {
            for (low_weight, low) in &lower {
                let left = up.0.scaled(*low_weight).ok_or(Capacity::Arithmetic)?;
                let right = low.0.scaled(*up_weight).ok_or(Capacity::Arithmetic)?;
                let combined = Inequality(left.plus(&right).ok_or(Capacity::Arithmetic)?).reduced();
                if combined.contradictory() {
                    return Ok(true);
                }
                if !combined.trivial() {
                    next.insert(combined);
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
}
