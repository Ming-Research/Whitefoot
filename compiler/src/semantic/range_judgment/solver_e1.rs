//! Experiment E1 (`research/investigations/derivation-ceiling`): what one
//! problem's root holds open, and the value interval the theory step gives
//! without a split. Instrument code of an unmerged experiment branch.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AtomId, AtomKind, Capacity, Inequality, Linear, Literal, Problem, Relation, Run,
    Tags, Unit, Verdict, combine_bounds, exclusion_cases, substituted, up_to_sign,
};

/// One problem's structural counts and search work.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct E1Measure {
    pub(crate) refuted_at_root: bool,
    /// Open items at the saturated root.
    pub(crate) n1: usize,
    /// The same item kinds, counted syntactically before the theory.
    pub(crate) n2: usize,
    /// Open items at the root connected to the goal's atoms.
    pub(crate) n3: usize,
    pub(crate) max_cases: usize,
    /// Product of the root open items' case counts, saturating.
    pub(crate) product: u128,
    pub(crate) backjump_nodes: usize,
    pub(crate) max_depth: usize,
    /// Nodes of the full case analysis, or `None` when not run.
    pub(crate) full_nodes: Option<usize>,
    pub(crate) full_capped: bool,
    pub(crate) full_nanos: u128,
}

const FULL_LIMIT: usize = 1 << 20;

struct Item {
    cases: usize,
    atoms: Vec<AtomId>,
}

fn literal_atoms(literal: &Literal, out: &mut Vec<AtomId>) {
    out.extend(literal.left.terms.iter().map(|(atom, _)| *atom));
    out.extend(literal.right.terms.iter().map(|(atom, _)| *atom));
}

struct Components(Vec<usize>);

impl Components {
    fn new(size: usize) -> Self {
        Self((0..size).collect())
    }
    fn find(&mut self, mut atom: usize) -> usize {
        while self.0[atom] != atom {
            self.0[atom] = self.0[self.0[atom]];
            atom = self.0[atom];
        }
        atom
    }
    fn union_all(&mut self, atoms: &[AtomId]) {
        let Some(first) = atoms.first() else {
            return;
        };
        let root = self.find(*first as usize);
        for atom in &atoms[1..] {
            let other = self.find(*atom as usize);
            if other != root {
                self.0[other] = root;
            }
        }
    }
}

impl Problem {
    /// The verdict with the compiler's search, and, apart from it, the
    /// counts the experiment records. `full` also runs the full case
    /// analysis, without backjumping, up to a node cap.
    pub(crate) fn e1_judge(
        &self,
        goal: &BTreeSet<AtomId>,
        full: bool,
    ) -> Result<(Verdict, E1Measure), Capacity> {
        let run = Run::backjumping();
        let verdict = self.judge_in(&run)?;
        let mut measure = E1Measure {
            backjump_nodes: run.nodes.get(),
            max_depth: run.max_depth.get(),
            n2: self.e1_syntactic(),
            ..E1Measure::default()
        };
        match self.e1_root()? {
            None => measure.refuted_at_root = true,
            Some(units) => {
                let items = self.e1_open_items(&units)?;
                let mut components = Components::new(self.atoms.len());
                self.e1_connect(&mut components);
                let goal_roots: BTreeSet<usize> = goal
                    .iter()
                    .filter(|atom| (**atom as usize) < self.atoms.len())
                    .map(|atom| components.find(*atom as usize))
                    .collect();
                measure.n1 = items.len();
                measure.product = 1;
                for item in &items {
                    measure.max_cases = measure.max_cases.max(item.cases);
                    measure.product = measure.product.saturating_mul(item.cases as u128);
                    if item
                        .atoms
                        .iter()
                        .any(|atom| goal_roots.contains(&components.find(*atom as usize)))
                    {
                        measure.n3 += 1;
                    }
                }
            }
        }
        if full {
            let start = std::time::Instant::now();
            let full_run = Run {
                backjump: false,
                limit: FULL_LIMIT,
                ..Run::backjumping()
            };
            let _ = self.judge_in(&full_run)?;
            measure.full_nodes = Some(full_run.nodes.get());
            measure.full_capped = full_run.exhausted.get();
            measure.full_nanos = start.elapsed().as_nanos();
        }
        Ok((verdict, measure))
    }

    /// Choices, exclusions, inclusions and disequalities anywhere in the
    /// problem, and pairs of reads of one place with differently written
    /// index tuples.
    fn e1_syntactic(&self) -> usize {
        let mut count = self.choices.len();
        let literals = self
            .units
            .iter()
            .chain(self.choices.iter().flatten().flatten())
            .chain(
                self.rules
                    .iter()
                    .flat_map(|rule| rule.guards.iter().chain(&rule.conclusions)),
            );
        let mut seen = BTreeSet::new();
        for literal in literals {
            if matches!(
                literal.relation,
                Relation::NotEqual | Relation::Excluded | Relation::Included
            ) && seen.insert(literal.clone())
            {
                count += 1;
            }
        }
        let mut by_place: BTreeMap<u32, BTreeSet<&Vec<Linear>>> = BTreeMap::new();
        for kind in &self.atoms {
            if let AtomKind::Read { place, indices } = kind {
                by_place.entry(*place).or_default().insert(indices);
            }
        }
        for tuples in by_place.values() {
            let n = tuples.len();
            count += n * n.saturating_sub(1) / 2;
        }
        count
    }

    /// The root after the theory's saturation, as `search` saturates it at
    /// depth 0, or `None` when the theory refutes it there.
    fn e1_root(&self) -> Result<Option<Vec<Unit>>, Capacity> {
        let run = Run::backjumping();
        let depth = 0;
        let mut units: Vec<Unit> = self
            .units
            .iter()
            .map(|literal| Unit::given(literal.clone()))
            .collect();
        let mut decided = vec![false; self.choices.len()];
        let mut fired = vec![false; self.rules.len()];
        loop {
            if self.contradiction(&units)?.is_some() {
                return Ok(None);
            }
            let mut changed = false;
            for (index, choice) in self.choices.iter().enumerate() {
                if decided[index] {
                    continue;
                }
                let mut open = Vec::new();
                for alternative in choice {
                    let mut with = units.clone();
                    with.extend(Unit::resting_on(alternative, &Tags::single(depth)));
                    if self.contradiction(&with)?.is_none() {
                        open.push(alternative);
                    }
                }
                match open.as_slice() {
                    [] => return Ok(None),
                    [only] => {
                        units.extend(Unit::resting_on(only, &Tags::default()));
                        decided[index] = true;
                        changed = true;
                    }
                    _ => {}
                }
            }
            let mut index = 0;
            while index < self.rules.len() {
                if fired[index] {
                    index += 1;
                    continue;
                }
                let guards = &self.rules[index].guards;
                let mut end = index + 1;
                while end < self.rules.len() && self.rules[end].guards == *guards {
                    end += 1;
                }
                if self.entailment(&units, guards, depth, &run)?.is_some() {
                    for position in index..end {
                        if !fired[position] {
                            units.extend(Unit::resting_on(
                                &self.rules[position].conclusions,
                                &Tags::default(),
                            ));
                            fired[position] = true;
                            changed = true;
                        }
                    }
                }
                index = end;
            }
            if !changed {
                break;
            }
        }
        let _ = decided;
        let root = units;
        Ok(Some(root))
    }

    /// The root's open items: undecided choices, unsettled read pairs,
    /// unfixed exclusions and inclusions, unfixed disequalities.
    fn e1_open_items(&self, units: &[Unit]) -> Result<Vec<Item>, Capacity> {
        let mut items = Vec::new();
        // A choice is decided at the root when exactly one alternative is
        // consistent with the units; an open one has two or more.
        for choice in &self.choices {
            let mut open = 0;
            let mut atoms = Vec::new();
            for alternative in choice {
                for literal in alternative {
                    literal_atoms(literal, &mut atoms);
                }
                let mut with = units.to_vec();
                with.extend(Unit::resting_on(alternative, &Tags::default()));
                if self.contradiction(&with)?.is_none() {
                    open += 1;
                }
            }
            if open >= 2 {
                items.push(Item { cases: open, atoms });
            }
        }
        let solved = self.solve(units)?;
        // Read pairs.
        let mut by_place: BTreeMap<u32, Vec<(Vec<Linear>, AtomId)>> = BTreeMap::new();
        for (index, kind) in self.atoms.iter().enumerate() {
            let AtomKind::Read { place, indices } = kind else {
                continue;
            };
            let mut reduced = Vec::with_capacity(indices.len());
            for value in indices {
                reduced.push(
                    substituted(value, &solved.solution)
                        .ok_or(Capacity::Arithmetic)?
                        .0,
                );
            }
            let classes = by_place.entry(*place).or_default();
            if !classes.iter().any(|(tuple, _)| *tuple == reduced) {
                classes.push((reduced, index as AtomId));
            }
        }
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
        for classes in by_place.values() {
            for (position, (first, first_atom)) in classes.iter().enumerate() {
                for (second, second_atom) in &classes[position + 1..] {
                    if first.len() != second.len() {
                        continue;
                    }
                    let mut differing = 0;
                    let mut settled = false;
                    for (left, right) in first.iter().zip(second) {
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
                        differing += 1;
                    }
                    if !settled && differing > 0 {
                        items.push(Item {
                            cases: differing + 1,
                            atoms: vec![*first_atom, *second_atom],
                        });
                    }
                }
            }
        }
        for unit in units {
            let literal = &unit.literal;
            match literal.relation {
                Relation::Excluded | Relation::Included => {
                    let (value, _) = substituted(&literal.left, &solved.solution)
                        .ok_or(Capacity::Arithmetic)?;
                    if value.is_constant() {
                        continue;
                    }
                    let cases = if literal.relation == Relation::Excluded {
                        exclusion_cases(literal.left.clone(), literal.members()).len()
                    } else {
                        literal.members().len()
                    };
                    let mut atoms = Vec::new();
                    literal_atoms(literal, &mut atoms);
                    items.push(Item { cases, atoms });
                }
                Relation::NotEqual => {
                    let (difference, _) = literal
                        .left
                        .minus(&literal.right)
                        .and_then(|difference| substituted(&difference, &solved.solution))
                        .ok_or(Capacity::Arithmetic)?;
                    if difference.is_constant() {
                        continue;
                    }
                    let mut atoms = Vec::new();
                    literal_atoms(literal, &mut atoms);
                    items.push(Item { cases: 2, atoms });
                }
                _ => {}
            }
        }
        Ok(items)
    }

    /// Atoms that share a literal, a choice, a rule or an index tuple are
    /// connected.
    fn e1_connect(&self, components: &mut Components) {
        let mut atoms = Vec::new();
        for literal in &self.units {
            atoms.clear();
            literal_atoms(literal, &mut atoms);
            components.union_all(&atoms);
        }
        for choice in &self.choices {
            atoms.clear();
            for literal in choice.iter().flatten() {
                literal_atoms(literal, &mut atoms);
            }
            components.union_all(&atoms);
        }
        for rule in &self.rules {
            atoms.clear();
            for literal in rule.guards.iter().chain(&rule.conclusions) {
                literal_atoms(literal, &mut atoms);
            }
            components.union_all(&atoms);
        }
        for (index, kind) in self.atoms.iter().enumerate() {
            atoms.clear();
            atoms.push(index as AtomId);
            match kind {
                AtomKind::Read { indices, .. } => {
                    for value in indices {
                        atoms.extend(value.terms.iter().map(|(atom, _)| *atom));
                    }
                }
                AtomKind::SegmentLength { segment, .. } => {
                    atoms.extend(segment.terms.iter().map(|(atom, _)| *atom));
                }
                AtomKind::Plain => continue,
            }
            components.union_all(&atoms);
        }
    }
}

/// The tightest constant bounds the theory step gives `target` over the
/// problem's units, with no split: equalities solved over the integers,
/// each inequality tightened, and Fourier-Motzkin projection onto the
/// target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Projected {
    /// The units are contradictory.
    Contradictory,
    Bounds(Option<i128>, Option<i128>),
}

pub(crate) fn project(problem: &Problem, target: &Linear) -> Result<Projected, Capacity> {
    let units: Vec<Unit> = problem
        .units
        .iter()
        .map(|literal| Unit::given(literal.clone()))
        .collect();
    let solved = problem.solve(&units)?;
    if solved.contradiction.is_some() {
        return Ok(Projected::Contradictory);
    }
    let (target, _) = substituted(target, &solved.solution).ok_or(Capacity::Arithmetic)?;
    if target.is_constant() {
        return Ok(Projected::Bounds(
            Some(target.constant),
            Some(target.constant),
        ));
    }
    let keep = solved.fresh;
    let mut current: BTreeMap<Inequality, Tags> = BTreeMap::new();
    for unit in &units {
        let literal = &unit.literal;
        let (left, _) = substituted(&literal.left, &solved.solution).ok_or(Capacity::Arithmetic)?;
        let (right, _) =
            substituted(&literal.right, &solved.solution).ok_or(Capacity::Arithmetic)?;
        let inequality = match literal.relation {
            Relation::LessEqual => Inequality::at_most(&left, &right),
            Relation::GreaterEqual => Inequality::at_most(&right, &left),
            Relation::Less => Inequality::below(&left, &right),
            Relation::Greater => Inequality::below(&right, &left),
            Relation::NotEqual => {
                let difference = left.minus(&right).ok_or(Capacity::Arithmetic)?;
                if difference.is_constant() && difference.constant == 0 {
                    return Ok(Projected::Contradictory);
                }
                continue;
            }
            Relation::Excluded | Relation::Included => {
                if left.is_constant()
                    && (literal.members().binary_search(&left.constant).is_ok()
                        == (literal.relation == Relation::Excluded))
                {
                    return Ok(Projected::Contradictory);
                }
                continue;
            }
            Relation::Equal => continue,
        }
        .ok_or(Capacity::Arithmetic)?;
        if inequality.contradictory() {
            return Ok(Projected::Contradictory);
        }
        if !inequality.trivial() {
            current.entry(inequality).or_default();
        }
    }
    let tee = Linear::atom(keep);
    for inequality in [
        Inequality::at_most(&tee, &target),
        Inequality::at_most(&target, &tee),
    ] {
        current
            .entry(inequality.ok_or(Capacity::Arithmetic)?)
            .or_default();
    }
    loop {
        let mut counts: BTreeMap<AtomId, (usize, usize)> = BTreeMap::new();
        for inequality in current.keys() {
            for (atom, coefficient) in &inequality.0.terms {
                if *atom == keep {
                    continue;
                }
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
            break;
        };
        let mut upper = Vec::new();
        let mut lower = Vec::new();
        let mut next = BTreeMap::new();
        for (inequality, why) in current {
            let weight = inequality.0.coefficient(atom);
            if weight > 0 {
                upper.push((weight, inequality, why));
            } else if weight < 0 {
                lower.push((
                    weight.checked_neg().ok_or(Capacity::Arithmetic)?,
                    inequality,
                    why,
                ));
            } else {
                next.entry(inequality).or_insert(why);
            }
        }
        if combine_bounds(&upper, &lower, &mut next)?.is_some() {
            return Ok(Projected::Contradictory);
        }
        current = next;
    }
    let mut low: Option<i128> = None;
    let mut high: Option<i128> = None;
    for inequality in current.keys() {
        // `weight * t + constant <= 0`.
        let [(atom, weight)] = inequality.0.terms.as_slice() else {
            continue;
        };
        if *atom != keep {
            continue;
        }
        let constant = inequality.0.constant;
        let Some(bound) = constant.checked_neg() else {
            continue;
        };
        if *weight > 0 {
            let value = bound.div_euclid(*weight);
            high = Some(high.map_or(value, |old| old.min(value)));
        } else {
            let magnitude = weight.checked_neg().ok_or(Capacity::Arithmetic)?;
            // `-m t <= -constant`, so `t >= constant / m`, rounded up.
            let value = -((-constant).div_euclid(magnitude));
            low = Some(low.map_or(value, |old| old.max(value)));
        }
    }
    if let (Some(low), Some(high)) = (low, high)
        && low > high
    {
        return Ok(Projected::Contradictory);
    }
    Ok(Projected::Bounds(low, high))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(atoms: usize, units: Vec<Literal>) -> Problem {
        Problem {
            atoms: vec![AtomKind::Plain; atoms],
            units,
            ..Problem::default()
        }
    }

    fn unit(left: Linear, relation: Relation, right: Linear) -> Literal {
        Literal::new(left, relation, right)
    }

    #[test]
    fn e1_projection_gives_the_tightest_unsplit_bounds() {
        // x >= 3, x <= 2y, y <= 5: x in [3, 10].
        let x = Linear::atom(0);
        let y = Linear::atom(1);
        let units = vec![
            unit(x.clone(), Relation::GreaterEqual, Linear::constant(3)),
            unit(
                x.clone(),
                Relation::LessEqual,
                y.scaled(2).expect("small coefficient"),
            ),
            unit(y.clone(), Relation::LessEqual, Linear::constant(5)),
        ];
        let projected = project(&problem(2, units.clone()), &x).expect("projects");
        assert_eq!(projected, Projected::Bounds(Some(3), Some(10)));
        // 2x == y + 1 with y in [0, 6]: x in [1, 3], over the integers.
        let units = vec![
            unit(
                x.scaled(2).expect("small"),
                Relation::Equal,
                y.plus_constant(1).expect("small"),
            ),
            unit(y.clone(), Relation::GreaterEqual, Linear::constant(0)),
            unit(y.clone(), Relation::LessEqual, Linear::constant(6)),
        ];
        let projected = project(&problem(2, units), &x).expect("projects");
        assert_eq!(projected, Projected::Bounds(Some(1), Some(3)));
        // x < 1 and x > 1 together are contradictory.
        let units = vec![
            unit(x.clone(), Relation::Less, Linear::constant(1)),
            unit(x.clone(), Relation::Greater, Linear::constant(1)),
        ];
        assert_eq!(
            project(&problem(1, units), &x).expect("projects"),
            Projected::Contradictory
        );
    }

    #[test]
    fn e1_root_counts_open_items_and_zero_for_a_refuted_root() {
        // x != y with both free: one open disequality, two cases.
        let x = Linear::atom(0);
        let y = Linear::atom(1);
        let open = problem(2, vec![unit(x.clone(), Relation::NotEqual, y.clone())]);
        let goal: BTreeSet<AtomId> = [0].into_iter().collect();
        let (_, measure) = open.e1_judge(&goal, true).expect("judges");
        assert!(!measure.refuted_at_root);
        assert_eq!((measure.n1, measure.n3, measure.max_cases), (1, 1, 2));
        // x != 2 and 2x == 4: the theory refutes the root; no item counts.
        let refuted = problem(
            1,
            vec![
                unit(x.clone(), Relation::NotEqual, Linear::constant(2)),
                unit(x.scaled(2).expect("small"), Relation::Equal, Linear::constant(4)),
            ],
        );
        let (verdict, measure) = refuted.e1_judge(&goal, true).expect("judges");
        assert_eq!(verdict, Verdict::Refuted);
        assert!(measure.refuted_at_root);
        assert_eq!(measure.n1, 0);
        // An item not connected to the goal counts in N1 but not N3.
        let z = Linear::atom(2);
        let w = Linear::atom(3);
        let apart = problem(
            4,
            vec![
                unit(x.clone(), Relation::NotEqual, y.clone()),
                unit(z, Relation::NotEqual, w),
            ],
        );
        let (_, measure) = apart.e1_judge(&goal, true).expect("judges");
        assert_eq!((measure.n1, measure.n3), (2, 1));
    }
}
