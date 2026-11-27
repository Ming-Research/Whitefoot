//! Linear expressions and the theory step: equalities with a unit
//! coefficient are substituted, each inequality is tightened over the
//! integers once, and plain Fourier-Motzkin elimination decides rational
//! infeasibility. What elimination derives is not tightened again.

use std::collections::HashMap;
use std::time::Instant;

pub type V = u32;

/// The variable a projection keeps.
pub const TOP: V = u32::MAX;

/// The most inequalities one elimination may hold before the instrument
/// gives up.
const CAPACITY: usize = 300_000;

#[derive(Clone, Default, Debug)]
pub struct Stats {
    pub theory_calls: u64,
    pub fm_pairs: u64,
    pub fm_elims: u64,
    pub probes: u64,
    pub forced: u64,
    pub nodes: u64,
    pub leaves: u64,
    pub projections: u64,
    pub bound_facts: u64,
    pub lp_pivots: u64,
    /// Literals and tableau entries visited: the unit that is meant to
    /// predict the time.
    pub work: u64,
    /// Literals and terms handed to theory calls: what the rule asks to
    /// be read, whatever the theory step then does with it.
    pub rule_lits: u64,
    pub rule_terms: u64,
    /// The most distinct values in one theory call.
    pub max_atoms: usize,
    /// Use the theory step of `indexed.rs`.
    pub indexed: bool,
    pub use_lp: bool,
    pub peak: usize,
    pub capacity: bool,
    pub timeout: bool,
    pub node_cap_hit: bool,
    pub deadline: Option<Instant>,
}

impl Stats {
    pub fn minus(&self, before: &Stats) -> Stats {
        Stats {
            theory_calls: self.theory_calls - before.theory_calls,
            fm_pairs: self.fm_pairs - before.fm_pairs,
            fm_elims: self.fm_elims - before.fm_elims,
            probes: self.probes - before.probes,
            forced: self.forced - before.forced,
            nodes: self.nodes - before.nodes,
            leaves: self.leaves - before.leaves,
            projections: self.projections - before.projections,
            bound_facts: self.bound_facts - before.bound_facts,
            lp_pivots: self.lp_pivots - before.lp_pivots,
            work: self.work - before.work,
            rule_lits: self.rule_lits - before.rule_lits,
            rule_terms: self.rule_terms - before.rule_terms,
            max_atoms: self.max_atoms,
            indexed: self.indexed,
            use_lp: self.use_lp,
            peak: self.peak,
            capacity: self.capacity,
            timeout: self.timeout,
            node_cap_hit: self.node_cap_hit,
            deadline: self.deadline,
        }
    }
}

/// A linear expression: the sum of `coefficient * value` over `t`, plus `c`.
/// `t` is sorted by value and holds no zero coefficient.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Lin {
    pub t: Vec<(V, i128)>,
    pub c: i128,
}

impl Lin {
    pub fn k(c: i128) -> Lin {
        Lin { t: Vec::new(), c }
    }

    pub fn v(value: V) -> Lin {
        Lin { t: vec![(value, 1)], c: 0 }
    }

    pub fn coeff(&self, value: V) -> i128 {
        match self.t.binary_search_by_key(&value, |(other, _)| *other) {
            Ok(position) => self.t[position].1,
            Err(_) => 0,
        }
    }

    pub fn vars(&self) -> impl Iterator<Item = V> + '_ {
        self.t.iter().map(|(value, _)| *value)
    }

    /// `a * self + b * other`, or `None` when a number leaves `i128`.
    pub fn checked_comb(&self, a: i128, other: &Lin, b: i128) -> Option<Lin> {
        let mut t = Vec::with_capacity(self.t.len() + other.t.len());
        let (mut i, mut j) = (0, 0);
        while i < self.t.len() || j < other.t.len() {
            let left = self.t.get(i);
            let right = other.t.get(j);
            let (value, coefficient) = match (left, right) {
                (Some((lv, lc)), Some((rv, rc))) if lv == rv => {
                    i += 1;
                    j += 1;
                    (*lv, a.checked_mul(*lc)?.checked_add(b.checked_mul(*rc)?)?)
                }
                (Some((lv, lc)), Some((rv, _))) if lv < rv => {
                    i += 1;
                    (*lv, a.checked_mul(*lc)?)
                }
                (Some((lv, lc)), None) => {
                    i += 1;
                    (*lv, a.checked_mul(*lc)?)
                }
                (_, Some((rv, rc))) => {
                    j += 1;
                    (*rv, b.checked_mul(*rc)?)
                }
                (None, None) => unreachable!(),
            };
            if coefficient != 0 {
                t.push((value, coefficient));
            }
        }
        let c = a.checked_mul(self.c)?.checked_add(b.checked_mul(other.c)?)?;
        Some(Lin { t, c })
    }

    pub fn comb(&self, a: i128, other: &Lin, b: i128) -> Lin {
        self.checked_comb(a, other, b).expect("a builder expression stays within i128")
    }

    pub fn add(&self, other: &Lin) -> Lin {
        self.comb(1, other, 1)
    }

    pub fn sub(&self, other: &Lin) -> Lin {
        self.comb(1, other, -1)
    }

    pub fn scale(&self, factor: i128) -> Lin {
        self.comb(factor, &Lin::k(0), 0)
    }

    pub fn addk(&self, constant: i128) -> Lin {
        Lin { t: self.t.clone(), c: self.c + constant }
    }

    /// `self` with `from` replaced by `to`.
    pub fn replace(&self, from: V, to: &Lin) -> Lin {
        let coefficient = self.coeff(from);
        if coefficient == 0 {
            return self.clone();
        }
        self.comb(1, &to.sub(&Lin::v(from)), coefficient)
    }
}

/// A literal handed to the theory: `lin <= 0`, or `lin == 0` when `eq`.
/// `tag` names the case splits the literal rests on.
#[derive(Clone, Debug)]
pub struct TL {
    pub lin: Lin,
    pub eq: bool,
    pub tag: u128,
}

pub enum Out {
    /// The literals are contradictory; the tags of the literals used.
    Contra(u128),
    /// Not contradictory. With a kept variable, the inequalities over it.
    Open(Vec<(Lin, u128)>),
    /// The instrument's capacity, its arithmetic or its time guard was hit.
    Capacity,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn div_ceil(a: i128, b: i128) -> i128 {
    let quotient = a.div_euclid(b);
    if a.rem_euclid(b) == 0 {
        quotient
    } else {
        quotient + 1
    }
}

pub enum Norm {
    Keep(Lin),
    True,
    False,
}

/// `lin <= 0` in lowest terms. With `tighten`, the bound is rounded over
/// the integers; without, the constant is divided only when it divides.
pub fn norm_le(mut lin: Lin, tighten: bool) -> Norm {
    if lin.t.is_empty() {
        return if lin.c > 0 { Norm::False } else { Norm::True };
    }
    let mut divisor = lin.t.iter().fold(0, |acc, (_, coefficient)| gcd(acc, *coefficient));
    if !tighten {
        divisor = gcd(divisor, lin.c);
    }
    if divisor > 1 {
        for (_, coefficient) in &mut lin.t {
            *coefficient /= divisor;
        }
        lin.c = div_ceil(lin.c, divisor);
    }
    Norm::Keep(lin)
}

pub fn norm_eq(mut lin: Lin) -> Norm {
    if lin.t.is_empty() {
        return if lin.c == 0 { Norm::True } else { Norm::False };
    }
    let divisor = lin.t.iter().fold(0, |acc, (_, coefficient)| gcd(acc, *coefficient));
    if lin.c % divisor != 0 {
        return Norm::False;
    }
    if divisor > 1 {
        for (_, coefficient) in &mut lin.t {
            *coefficient /= divisor;
        }
        lin.c /= divisor;
    }
    Norm::Keep(lin)
}

/// Decides whether `input` is contradictory. With `keep`, that variable is
/// neither substituted nor eliminated, and the inequalities left over it
/// are returned.
pub fn theory(input: &[TL], keep: Option<V>, st: &mut Stats) -> Out {
    st.theory_calls += 1;
    st.work += input.len() as u64;
    st.rule_lits += input.len() as u64;
    let mut atoms: Vec<V> = Vec::new();
    for lit in input {
        st.rule_terms += lit.lin.t.len() as u64;
        atoms.extend(lit.lin.vars());
    }
    atoms.sort_unstable();
    atoms.dedup();
    st.max_atoms = st.max_atoms.max(atoms.len());
    if st.indexed && !st.use_lp {
        return crate::indexed::theory(input, keep, st);
    }
    if let Some(deadline) = st.deadline {
        if Instant::now() > deadline {
            st.timeout = true;
            return Out::Capacity;
        }
    }
    let mut eqs: Vec<(Lin, u128)> = Vec::new();
    let mut les: Vec<(Lin, u128)> = Vec::new();
    for lit in input {
        if lit.eq {
            match norm_eq(lit.lin.clone()) {
                Norm::Keep(lin) => eqs.push((lin, lit.tag)),
                Norm::True => {}
                Norm::False => return Out::Contra(lit.tag),
            }
        } else {
            match norm_le(lit.lin.clone(), true) {
                Norm::Keep(lin) => les.push((lin, lit.tag)),
                Norm::True => {}
                Norm::False => return Out::Contra(lit.tag),
            }
        }
    }
    // Substitute each equality that has a variable with a unit coefficient.
    loop {
        let found = eqs.iter().enumerate().find_map(|(position, (lin, _))| {
            lin.t
                .iter()
                .find(|(value, coefficient)| coefficient.abs() == 1 && Some(*value) != keep)
                .map(|(value, coefficient)| (position, *value, *coefficient))
        });
        let Some((position, value, unit)) = found else { break };
        let (solved, solved_tag) = eqs.swap_remove(position);
        st.work += (eqs.len() + les.len()) as u64;
        let mut next_eqs = Vec::with_capacity(eqs.len());
        for (lin, tag) in eqs {
            let coefficient = lin.coeff(value);
            if coefficient == 0 {
                next_eqs.push((lin, tag));
                continue;
            }
            let Some(lin) = lin.checked_comb(1, &solved, -coefficient * unit) else {
                st.capacity = true;
                return Out::Capacity;
            };
            match norm_eq(lin) {
                Norm::Keep(lin) => next_eqs.push((lin, tag | solved_tag)),
                Norm::True => {}
                Norm::False => return Out::Contra(tag | solved_tag),
            }
        }
        eqs = next_eqs;
        let mut next_les = Vec::with_capacity(les.len());
        for (lin, tag) in les {
            let coefficient = lin.coeff(value);
            if coefficient == 0 {
                next_les.push((lin, tag));
                continue;
            }
            let Some(lin) = lin.checked_comb(1, &solved, -coefficient * unit) else {
                st.capacity = true;
                return Out::Capacity;
            };
            match norm_le(lin, true) {
                Norm::Keep(lin) => next_les.push((lin, tag | solved_tag)),
                Norm::True => {}
                Norm::False => return Out::Contra(tag | solved_tag),
            }
        }
        les = next_les;
    }
    for (lin, tag) in eqs {
        les.push((lin.scale(-1), tag));
        les.push((lin, tag));
    }
    if st.use_lp {
        let mut rows = Vec::with_capacity(les.len());
        for (lin, tag) in &les {
            if lin.t.is_empty() {
                if lin.c > 0 {
                    return Out::Contra(*tag);
                }
            } else {
                rows.push(lin.clone());
            }
        }
        return match crate::lp::solve(&rows, keep, st) {
            crate::lp::Lp::Infeasible => Out::Contra(0),
            crate::lp::Lp::Feasible | crate::lp::Lp::Unbounded => Out::Open(Vec::new()),
            crate::lp::Lp::Supremum(numerator, denominator) => {
                let kept = keep.expect("a supremum has a kept variable");
                Out::Open(vec![(Lin { t: vec![(kept, denominator)], c: -numerator }, 0)])
            }
            crate::lp::Lp::Capacity => {
                st.capacity = true;
                Out::Capacity
            }
        };
    }
    // Fourier-Motzkin elimination.
    loop {
        let mut unique: HashMap<Vec<(V, i128)>, (i128, u128)> = HashMap::with_capacity(les.len());
        for (lin, tag) in les {
            if lin.t.is_empty() {
                if lin.c > 0 {
                    return Out::Contra(tag);
                }
                continue;
            }
            match unique.get_mut(&lin.t) {
                Some(slot) => {
                    if lin.c > slot.0 {
                        *slot = (lin.c, tag);
                    }
                }
                None => {
                    unique.insert(lin.t, (lin.c, tag));
                }
            }
        }
        les = unique.into_iter().map(|(t, (c, tag))| (Lin { t, c }, tag)).collect();
        les.sort_by(|left, right| left.0.t.cmp(&right.0.t));
        st.peak = st.peak.max(les.len());
        st.work += les.len() as u64;
        let mut counts: HashMap<V, (u64, u64)> = HashMap::new();
        for (lin, _) in &les {
            for (value, coefficient) in &lin.t {
                if Some(*value) == keep {
                    continue;
                }
                let slot = counts.entry(*value).or_default();
                if *coefficient > 0 {
                    slot.0 += 1;
                } else {
                    slot.1 += 1;
                }
            }
        }
        let Some((value, _)) = counts.iter().map(|(value, (up, down))| (*value, up * down)).min_by_key(|(value, cost)| (*cost, *value)) else {
            return Out::Open(les);
        };
        st.fm_elims += 1;
        let mut uppers = Vec::new();
        let mut lowers = Vec::new();
        let mut rest = Vec::new();
        for (lin, tag) in les {
            let coefficient = lin.coeff(value);
            if coefficient > 0 {
                uppers.push((lin, tag, coefficient));
            } else if coefficient < 0 {
                lowers.push((lin, tag, coefficient));
            } else {
                rest.push((lin, tag));
            }
        }
        if (uppers.len() as u128) * (lowers.len() as u128) + rest.len() as u128 > CAPACITY as u128 {
            st.capacity = true;
            return Out::Capacity;
        }
        for (up, up_tag, up_coefficient) in &uppers {
            for (down, down_tag, down_coefficient) in &lowers {
                st.fm_pairs += 1;
                st.work += 1;
                let Some(lin) = up.checked_comb(-down_coefficient, down, *up_coefficient) else {
                    st.capacity = true;
                    return Out::Capacity;
                };
                match norm_le(lin, false) {
                    Norm::Keep(lin) => rest.push((lin, up_tag | down_tag)),
                    Norm::True => {}
                    Norm::False => return Out::Contra(up_tag | down_tag),
                }
            }
        }
        les = rest;
        if let Some(deadline) = st.deadline {
            if Instant::now() > deadline {
                st.timeout = true;
                return Out::Capacity;
            }
        }
    }
}
