//! A second decision of the theory step's last condition: rational
//! infeasibility by an exact simplex method with Bland's rule, in place of
//! Fourier-Motzkin elimination. It takes the inequalities the common first
//! part of the theory step leaves (equalities substituted, each inequality
//! tightened once), so its verdict is the same relation.

use std::collections::BTreeMap;

use crate::theory::{Lin, Stats, V};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct R {
    n: i128,
    d: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl R {
    const ZERO: R = R { n: 0, d: 1 };

    fn int(n: i128) -> R {
        R { n, d: 1 }
    }

    fn make(n: i128, d: i128) -> Option<R> {
        if d == 0 {
            return None;
        }
        let g = gcd(n, d).max(1);
        let (n, d) = (n / g, d / g);
        Some(if d < 0 { R { n: n.checked_neg()?, d: d.checked_neg()? } } else { R { n, d } })
    }

    fn add(self, other: R) -> Option<R> {
        let g = gcd(self.d, other.d).max(1);
        let n = self.n.checked_mul(other.d / g)?.checked_add(other.n.checked_mul(self.d / g)?)?;
        R::make(n, self.d.checked_mul(other.d / g)?)
    }

    fn mul(self, other: R) -> Option<R> {
        let g1 = gcd(self.n, other.d).max(1);
        let g2 = gcd(other.n, self.d).max(1);
        R::make((self.n / g1).checked_mul(other.n / g2)?, (self.d / g2).checked_mul(other.d / g1)?)
    }

    fn neg(self) -> R {
        R { n: -self.n, d: self.d }
    }

    fn sub(self, other: R) -> Option<R> {
        self.add(other.neg())
    }

    fn div(self, other: R) -> Option<R> {
        self.mul(R::make(other.d, other.n)?)
    }

    fn less(self, other: R) -> Option<bool> {
        Some(self.n.checked_mul(other.d)? < other.n.checked_mul(self.d)?)
    }

    fn positive(self) -> bool {
        self.n > 0
    }

    fn negative(self) -> bool {
        self.n < 0
    }
}

pub enum Lp {
    Infeasible,
    Feasible,
    /// The supremum of the kept variable, as numerator and denominator.
    Supremum(i128, i128),
    Unbounded,
    Capacity,
}

struct Tableau {
    /// For each basic variable, its row over the nonbasic variables.
    rows: BTreeMap<usize, BTreeMap<usize, R>>,
    value: Vec<R>,
    upper: Vec<Option<R>>,
    pivots: u64,
}

impl Tableau {
    fn can_rise(&self, var: usize) -> Option<bool> {
        match self.upper[var] {
            None => Some(true),
            Some(bound) => self.value[var].less(bound),
        }
    }

    /// Moves the nonbasic `var` by `step` and every basic variable with it.
    fn shift(&mut self, var: usize, step: R) -> Option<()> {
        self.value[var] = self.value[var].add(step)?;
        for (basic, row) in &self.rows {
            if let Some(coefficient) = row.get(&var) {
                self.value[*basic] = self.value[*basic].add(coefficient.mul(step)?)?;
            }
        }
        Some(())
    }

    /// Makes the nonbasic `enter` basic in place of the basic `leave`.
    fn pivot(&mut self, leave: usize, enter: usize) -> Option<()> {
        self.pivots += 1;
        let row = self.rows.remove(&leave)?;
        let pivot = *row.get(&enter)?;
        // leave = pivot * enter + rest  =>  enter = leave / pivot - rest / pivot
        let mut solved: BTreeMap<usize, R> = BTreeMap::new();
        solved.insert(leave, R::int(1).div(pivot)?);
        for (var, coefficient) in &row {
            if *var != enter {
                solved.insert(*var, coefficient.div(pivot)?.neg());
            }
        }
        for other in self.rows.values_mut() {
            if let Some(factor) = other.remove(&enter) {
                for (var, coefficient) in &solved {
                    let sum = other.get(var).copied().unwrap_or(R::ZERO).add(factor.mul(*coefficient)?)?;
                    if sum.n == 0 {
                        other.remove(var);
                    } else {
                        other.insert(*var, sum);
                    }
                }
            }
        }
        self.rows.insert(enter, solved);
        Some(())
    }

    /// `Some(true)` when every bound holds, `Some(false)` when none can.
    fn feasible(&mut self, limit: u64) -> Option<bool> {
        loop {
            if self.pivots > limit {
                return None;
            }
            let mut broken = None;
            for basic in self.rows.keys() {
                if let Some(bound) = self.upper[*basic] {
                    if bound.less(self.value[*basic])? {
                        broken = Some((*basic, bound));
                        break;
                    }
                }
            }
            let Some((basic, bound)) = broken else { return Some(true) };
            let mut enter = None;
            for (var, coefficient) in &self.rows[&basic] {
                if coefficient.positive() || (coefficient.negative() && self.can_rise(*var)?) {
                    enter = Some((*var, *coefficient));
                    break;
                }
            }
            let Some((enter, coefficient)) = enter else { return Some(false) };
            let step = bound.sub(self.value[basic])?.div(coefficient)?;
            self.shift(enter, step)?;
            self.pivot(basic, enter)?;
        }
    }

    /// The supremum of `goal` once the bounds hold; `Ok(None)` when it has
    /// none.
    fn maximize(&mut self, goal: usize, limit: u64) -> Option<Option<R>> {
        loop {
            if self.pivots > limit {
                return None;
            }
            // The goal over the nonbasic variables.
            let objective: Vec<(usize, R)> = match self.rows.get(&goal) {
                Some(row) => row.iter().map(|(var, coefficient)| (*var, *coefficient)).collect(),
                None => vec![(goal, R::int(1))],
            };
            let mut enter = None;
            for (var, coefficient) in objective {
                if coefficient.positive() && self.can_rise(var)? {
                    enter = Some((var, true));
                    break;
                }
                if coefficient.negative() {
                    enter = Some((var, false));
                    break;
                }
            }
            let Some((enter, rise)) = enter else { return Some(Some(self.value[goal])) };
            // The largest move of `enter` that keeps every bound.
            let mut best: Option<(R, Option<usize>)> = None;
            if rise {
                if let Some(bound) = self.upper[enter] {
                    best = Some((bound.sub(self.value[enter])?, None));
                }
            }
            for (basic, row) in &self.rows {
                let (Some(bound), Some(coefficient)) = (self.upper[*basic], row.get(&enter)) else { continue };
                let rate = if rise { *coefficient } else { coefficient.neg() };
                if !rate.positive() {
                    continue;
                }
                let room = bound.sub(self.value[*basic])?.div(rate)?;
                let closer = match &best {
                    None => true,
                    Some((current, _)) => room.less(*current)?,
                };
                if closer {
                    best = Some((room, Some(*basic)));
                }
            }
            let Some((room, leave)) = best else { return Some(None) };
            let step = if rise { room } else { room.neg() };
            self.shift(enter, step)?;
            if let Some(leave) = leave {
                self.pivot(leave, enter)?;
            }
        }
    }
}

/// Decides `les` (each `lin <= 0`), and with `keep` the supremum of that
/// variable.
pub fn solve(les: &[Lin], keep: Option<V>, st: &mut Stats) -> Lp {
    let mut columns: BTreeMap<V, usize> = BTreeMap::new();
    for lin in les {
        for value in lin.vars() {
            let next = columns.len();
            columns.entry(value).or_insert(next);
        }
    }
    let free = columns.len();
    let mut tableau = Tableau {
        rows: BTreeMap::new(),
        value: vec![R::ZERO; free + les.len()],
        upper: vec![None; free + les.len()],
        pivots: 0,
    };
    for (index, lin) in les.iter().enumerate() {
        let row: BTreeMap<usize, R> = lin.t.iter().map(|(value, coefficient)| (columns[value], R::int(*coefficient))).collect();
        tableau.rows.insert(free + index, row);
        let Some(bound) = lin.c.checked_neg() else { return Lp::Capacity };
        tableau.upper[free + index] = Some(R::int(bound));
    }
    let limit = 200_000;
    let result = match tableau.feasible(limit) {
        None => Lp::Capacity,
        Some(false) => Lp::Infeasible,
        Some(true) => match keep.and_then(|value| columns.get(&value).copied()) {
            None if keep.is_some() => Lp::Unbounded,
            None => Lp::Feasible,
            Some(goal) => match tableau.maximize(goal, limit) {
                None => Lp::Capacity,
                Some(None) => Lp::Unbounded,
                Some(Some(best)) => Lp::Supremum(best.n, best.d),
            },
        },
    };
    st.lp_pivots += tableau.pivots;
    result
}
