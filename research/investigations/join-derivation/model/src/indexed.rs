//! The theory step again, with the same verdict and another way of working:
//! every row is found through the variables it holds, so that substituting
//! an equality or eliminating a variable touches only the rows that hold
//! that variable. `theory.rs` rereads every row at each step.

use std::collections::{BTreeSet, HashMap};
use std::time::Instant;

use crate::lp::{self, Lp};
use crate::theory::{norm_eq, norm_le, Lin, Norm, Out, Stats, TL, V};

const CAPACITY: usize = 300_000;

struct Rows {
    rows: Vec<Option<(Lin, u128, bool)>>,
    holds: HashMap<V, Vec<usize>>,
    live: usize,
    /// The next parameter the integer solution of an equality may name.
    fresh: V,
}

impl Rows {
    fn push(&mut self, lin: Lin, tag: u128, eq: bool) -> usize {
        let id = self.rows.len();
        for value in lin.vars() {
            self.holds.entry(value).or_default().push(id);
        }
        self.rows.push(Some((lin, tag, eq)));
        self.live += 1;
        id
    }

    fn take(&mut self, id: usize) -> Option<(Lin, u128, bool)> {
        let row = self.rows[id].take();
        if row.is_some() {
            self.live -= 1;
        }
        row
    }

    /// The rows that hold `value` now, each once, in order.
    fn holding(&mut self, value: V, st: &mut Stats) -> Vec<usize> {
        let Some(list) = self.holds.get_mut(&value) else { return Vec::new() };
        st.work += list.len() as u64;
        list.sort_unstable();
        list.dedup();
        let rows = &self.rows;
        list.retain(|id| rows[*id].as_ref().is_some_and(|(lin, _, _)| lin.coeff(value) != 0));
        list.clone()
    }
}

pub fn theory(input: &[TL], keep: Option<V>, st: &mut Stats) -> Out {
    let mut rows = Rows { rows: Vec::with_capacity(input.len() * 2), holds: HashMap::new(), live: 0, fresh: 1 << 30 };
    let mut pending: Vec<usize> = Vec::new();
    for lit in input {
        if lit.eq {
            match norm_eq(lit.lin.clone()) {
                Norm::Keep(lin) => pending.push(rows.push(lin, lit.tag, true)),
                Norm::True => {}
                Norm::False => return Out::Contra(lit.tag),
            }
        } else {
            match norm_le(lit.lin.clone(), true) {
                Norm::Keep(lin) => {
                    rows.push(lin, lit.tag, false);
                }
                Norm::True => {}
                Norm::False => return Out::Contra(lit.tag),
            }
        }
    }
    // Solve the equalities over the integers. An equality with a unit
    // coefficient is substituted, on the unit variable that the fewest rows
    // hold. One without is first reduced by a step of Euclid's algorithm:
    // its variable of least coefficient `a` is replaced by a new parameter
    // minus the others times their quotients by `a`, which maps the integer
    // solutions one to one and leaves every other coefficient below `a`.
    let mut next = 0;
    while next < pending.len() {
        let id = pending[next];
        next += 1;
        let Some((lin, _, true)) = rows.rows[id].as_ref() else { continue };
        let unit = lin
            .t
            .iter()
            .filter(|(value, coefficient)| coefficient.abs() == 1 && Some(*value) != keep)
            .min_by_key(|(value, _)| (rows.holds.get(value).map_or(0, Vec::len), *value))
            .copied();
        let (value, replacement, replaced_tag) = match unit {
            Some((value, unit)) => {
                let (solved, solved_tag, _) = rows.take(id).expect("the row is there");
                // unit * value + rest == 0, so value == -unit * rest.
                let rest = Lin { t: solved.t.iter().copied().filter(|(other, _)| *other != value).collect(), c: solved.c };
                (value, rest.scale(-unit), solved_tag)
            }
            None => {
                let Some((least, a)) = lin.t.iter().filter(|(value, _)| Some(*value) != keep).min_by_key(|(value, coefficient)| (coefficient.abs(), *value)).copied() else {
                    continue;
                };
                let parameter = rows.fresh;
                rows.fresh += 1;
                let mut replacement = Lin::v(parameter);
                for (other, coefficient) in &lin.t {
                    if *other != least {
                        replacement = replacement.comb(1, &Lin::v(*other), -coefficient.div_euclid(a));
                    }
                }
                st.euclid += 1;
                (least, replacement, 0)
            }
        };
        let change = replacement.sub(&Lin::v(value));
        for other in rows.holding(value, st) {
            let (lin, tag, eq) = rows.take(other).expect("a holding row is there");
            let coefficient = lin.coeff(value);
            let Some(lin) = lin.checked_comb(1, &change, coefficient) else {
                st.capacity = true;
                return Out::Capacity;
            };
            let normal = if eq { norm_eq(lin) } else { norm_le(lin, true) };
            match normal {
                Norm::Keep(lin) => {
                    let id = rows.push(lin, tag | replaced_tag, eq);
                    if eq {
                        pending.push(id);
                    }
                }
                Norm::True => {}
                Norm::False => return Out::Contra(tag | replaced_tag),
            }
        }
    }
    // What is left as an equality names only the kept variable.
    for id in 0..rows.rows.len() {
        if let Some((lin, tag, true)) = rows.rows[id].clone() {
            rows.take(id);
            rows.push(lin.scale(-1), tag, false);
            rows.push(lin, tag, false);
        }
    }
    if st.use_lp {
        let les: Vec<Lin> = rows.rows.iter().flatten().map(|(lin, _, _)| lin.clone()).collect();
        return match lp::solve(&les, keep, st) {
            Lp::Infeasible => Out::Contra(0),
            Lp::Feasible | Lp::Unbounded => Out::Open(Vec::new()),
            Lp::Supremum(numerator, denominator) => {
                let kept = keep.expect("a supremum has a kept variable");
                Out::Open(vec![(Lin { t: vec![(kept, denominator)], c: -numerator }, 0)])
            }
            Lp::Capacity => {
                st.capacity = true;
                Out::Capacity
            }
        };
    }
    // One row for each form: the tightest.
    let mut forms: HashMap<Vec<(V, i128)>, usize> = HashMap::new();
    for id in 0..rows.rows.len() {
        let Some((lin, _, _)) = rows.rows[id].as_ref() else { continue };
        let (form, constant) = (lin.t.clone(), lin.c);
        match forms.get(&form).copied() {
            None => {
                forms.insert(form, id);
            }
            Some(first) => {
                let kept = rows.rows[first].as_ref().map_or(i128::MIN, |(lin, _, _)| lin.c);
                if constant > kept {
                    rows.take(first);
                    forms.insert(form, id);
                } else {
                    rows.take(id);
                }
            }
        }
    }
    // Fourier-Motzkin elimination, the cheapest variable first.
    let cost_of = |rows: &mut Rows, value: V, st: &mut Stats| -> u64 {
        let (mut up, mut down) = (0u64, 0u64);
        for id in rows.holding(value, st) {
            if rows.rows[id].as_ref().is_some_and(|(lin, _, _)| lin.coeff(value) > 0) {
                up += 1;
            } else {
                down += 1;
            }
        }
        up * down
    };
    let mut values: Vec<V> = rows.holds.keys().copied().filter(|value| Some(*value) != keep).collect();
    values.sort_unstable();
    let mut order: BTreeSet<(u64, V)> = BTreeSet::new();
    let mut costs: HashMap<V, u64> = HashMap::new();
    for value in values {
        let cost = cost_of(&mut rows, value, st);
        order.insert((cost, value));
        costs.insert(value, cost);
    }
    let mut steps = 0u64;
    while let Some((cost, value)) = order.pop_first() {
        let actual = cost_of(&mut rows, value, st);
        if actual != cost {
            order.insert((actual, value));
            costs.insert(value, actual);
            continue;
        }
        st.fm_elims += 1;
        steps += 1;
        if steps % 64 == 0 {
            if let Some(deadline) = st.deadline {
                if Instant::now() > deadline {
                    st.timeout = true;
                    return Out::Capacity;
                }
            }
        }
        let (mut uppers, mut lowers) = (Vec::new(), Vec::new());
        for id in rows.holding(value, st) {
            let (lin, tag, _) = rows.take(id).expect("a holding row is there");
            forms.remove(&lin.t);
            let coefficient = lin.coeff(value);
            if coefficient > 0 {
                uppers.push((lin, tag, coefficient));
            } else {
                lowers.push((lin, tag, coefficient));
            }
        }
        if uppers.len() * lowers.len() + rows.live > CAPACITY {
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
                let tag = up_tag | down_tag;
                match norm_le(lin, false) {
                    Norm::Keep(lin) => match forms.get(&lin.t).copied() {
                        Some(first) => {
                            if let Some((kept, kept_tag, _)) = rows.rows[first].as_mut() {
                                if lin.c > kept.c {
                                    kept.c = lin.c;
                                    *kept_tag = tag;
                                }
                            }
                        }
                        None => {
                            let form = lin.t.clone();
                            let id = rows.push(lin, tag, false);
                            forms.insert(form, id);
                        }
                    },
                    Norm::True => {}
                    Norm::False => return Out::Contra(tag),
                }
            }
        }
        st.peak = st.peak.max(rows.live);
    }
    let rest = rows.rows.into_iter().flatten().map(|(lin, tag, _)| (lin, tag)).collect();
    Out::Open(rest)
}
