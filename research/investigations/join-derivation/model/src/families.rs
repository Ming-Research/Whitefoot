//! The input families of `DESIGN.md`, each a program of the model's
//! language built at a given size. With `tiny`, every unknown has a small
//! finite range, so that the exhaustive evaluator can run the program.

use crate::theory::{Lin, TL, V};
use crate::{PhiSrc, Stmt};

pub struct Size {
    pub k: usize,
    pub n: usize,
}

impl Size {
    pub fn label(&self) -> String {
        if self.n == 0 {
            format!("k{}", self.k)
        } else {
            format!("n{}k{}", self.n, self.k)
        }
    }

    /// Sizes the exponential candidates are not run at.
    pub fn large(&self) -> bool {
        self.k > 16
    }
}

pub struct Family {
    pub name: &'static str,
    pub sizes: Vec<Size>,
    pub tiny: Vec<Size>,
    pub make: fn(&Size, bool) -> Vec<Stmt>,
}

#[derive(Default)]
struct B {
    next: V,
}

impl B {
    fn id(&mut self) -> V {
        self.next += 1;
        self.next - 1
    }
}

fn v(value: V) -> Lin {
    Lin::v(value)
}

fn k(constant: i128) -> Lin {
    Lin::k(constant)
}

/// `a <= b` as a form that is at most zero.
fn le(a: &Lin, b: &Lin) -> Lin {
    a.sub(b)
}

fn lt(a: &Lin, b: &Lin) -> Lin {
    a.sub(b).addk(1)
}

fn not(cond: &Lin) -> Lin {
    cond.scale(-1).addk(1)
}

fn iff(cond: Lin, then: Vec<Stmt>, otherwise: Vec<Stmt>, phis: Vec<PhiSrc>) -> Stmt {
    let negated = not(&cond);
    Stmt::Branch(vec![(vec![cond], then), (vec![negated], otherwise)], phis)
}

fn fresh(b: &mut B, out: &mut Vec<Stmt>, low: i128, high: Option<i128>) -> V {
    let value = b.id();
    out.push(Stmt::Fresh(value, Some(low), high));
    value
}

/// A condition the derivation knows nothing about.
fn unknown(b: &mut B, out: &mut Vec<Stmt>) -> Lin {
    let flag = fresh(b, out, 0, Some(1));
    k(1).sub(&v(flag))
}

/// `if c { x = x + amount }` on an unknown `c`; returns the join value.
fn bump(b: &mut B, out: &mut Vec<Stmt>, x: V, amount: &Lin) -> V {
    let cond = unknown(b, out);
    let raised = b.id();
    let joined = b.id();
    out.push(iff(cond, vec![Stmt::Let(raised, v(x).add(amount))], vec![], vec![(joined, vec![raised, x], Some(x))]));
    joined
}

fn chain(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let high = if tiny { 2 } else { 100 };
    let mut x = fresh(&mut b, &mut out, 0, Some(high));
    for _ in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(1));
    }
    out.push(Stmt::Assert("final".into(), v(x).addk(-(high + size.k as i128))));
    out
}

fn chainsym(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(3);
    let n = fresh(&mut b, &mut out, 0, range);
    let mut x = fresh(&mut b, &mut out, 0, range);
    out.push(Stmt::Assume(le(&v(x), &v(n)), false));
    for _ in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(1));
    }
    out.push(Stmt::Assert("final".into(), v(x).sub(&v(n)).addk(-(size.k as i128))));
    out
}

fn addends(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let start = fresh(&mut b, &mut out, 0, tiny.then_some(2));
    let mut x = start;
    let mut sum = v(start);
    for _ in 0..size.k {
        let y = fresh(&mut b, &mut out, 0, Some(if tiny { 1 } else { 1000 }));
        x = bump(&mut b, &mut out, x, &v(y));
        sum = sum.add(&v(y));
    }
    out.push(Stmt::Assert("final".into(), v(x).sub(&sum)));
    out
}

fn guarded_with(size: &Size, tiny: bool, questions: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(4);
    let n = fresh(&mut b, &mut out, 0, range);
    let mut i = fresh(&mut b, &mut out, 0, range);
    out.push(Stmt::Assume(lt(&v(i), &v(n)), false));
    for step in 0..size.k {
        let cond = v(i).addk(2).sub(&v(n));
        if questions {
            let mark = if step + 1 == size.k { "-last".to_owned() } else { step.to_string() };
            out.push(Stmt::Query(format!("true{mark}"), cond.clone()));
            out.push(Stmt::Query(format!("false{mark}"), not(&cond)));
        }
        let raised = b.id();
        let joined = b.id();
        out.push(iff(cond, vec![Stmt::Let(raised, v(i).addk(1))], vec![], vec![(joined, vec![raised, i], Some(i))]));
        i = joined;
    }
    out.push(Stmt::Assert("final".into(), lt(&v(i), &v(n))));
    out
}

fn guarded(size: &Size, tiny: bool) -> Vec<Stmt> {
    guarded_with(size, tiny, false)
}

fn op5(size: &Size, tiny: bool) -> Vec<Stmt> {
    guarded_with(size, tiny, true)
}

fn minsel(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(2);
    let first = fresh(&mut b, &mut out, 0, range);
    let mut least = first;
    let mut offered = vec![first];
    for _ in 0..size.k {
        let a = fresh(&mut b, &mut out, 0, range);
        let joined = b.id();
        out.push(iff(lt(&v(a), &v(least)), vec![], vec![], vec![(joined, vec![a, least], Some(least))]));
        least = joined;
        offered.push(a);
    }
    let chosen = offered[(size.k + 1) / 2];
    out.push(Stmt::Assert("final".into(), le(&v(least), &v(chosen))));
    out
}

fn binsearch(_: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(3);
    let len = fresh(&mut b, &mut out, 0, range);
    let low = fresh(&mut b, &mut out, 0, range);
    let high = fresh(&mut b, &mut out, 0, range);
    out.push(Stmt::Assume(le(&v(low), &v(high)), false));
    out.push(Stmt::Assume(le(&v(high), &v(len)), false));
    out.push(Stmt::Assume(lt(&v(low), &v(high)), false));
    let mid = fresh(&mut b, &mut out, 0, range);
    out.push(Stmt::Assume(le(&v(low), &v(mid)), false));
    out.push(Stmt::Assume(lt(&v(mid), &v(high)), false));
    let cond = unknown(&mut b, &mut out);
    let above = b.id();
    let new_low = b.id();
    let new_high = b.id();
    out.push(iff(
        cond,
        vec![Stmt::Let(above, v(mid).addk(1))],
        vec![],
        vec![(new_low, vec![above, low], Some(low)), (new_high, vec![high, mid], Some(high))],
    ));
    out.push(Stmt::Assert("within".into(), le(&v(new_high), &v(len))));
    out.push(Stmt::Assert("ordered".into(), le(&v(new_low), &v(new_high))));
    out
}

fn matched(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let arms = size.n as i128;
    let high = if tiny { 2 } else { 100 };
    let mut x = fresh(&mut b, &mut out, 0, Some(high));
    for _ in 0..size.k {
        let selector = fresh(&mut b, &mut out, 0, Some(arms - 1));
        let mut bodies = Vec::new();
        let mut raws = Vec::new();
        for arm in 0..arms {
            let raised = b.id();
            raws.push(raised);
            bodies.push((
                vec![v(selector).addk(-arm), k(arm).sub(&v(selector))],
                vec![Stmt::Let(raised, v(x).addk(arm))],
            ));
        }
        let joined = b.id();
        out.push(Stmt::Branch(bodies, vec![(joined, raws, Some(x))]));
        x = joined;
    }
    out.push(Stmt::Assert("final".into(), v(x).addk(-(high + size.k as i128 * (arms - 1)))));
    out
}

fn nest_level(b: &mut B, out: &mut Vec<Stmt>, x: V, depth: usize) -> V {
    if depth == 0 {
        return x;
    }
    let cond = unknown(b, out);
    let raised = b.id();
    let mut body = vec![Stmt::Let(raised, v(x).addk(1))];
    let inner = nest_level(b, &mut body, raised, depth - 1);
    let joined = b.id();
    out.push(iff(cond, body, vec![], vec![(joined, vec![inner, x], Some(x))]));
    joined
}

fn nest(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let high = if tiny { 2 } else { 100 };
    let start = fresh(&mut b, &mut out, 0, Some(high));
    let x = nest_level(&mut b, &mut out, start, size.k);
    out.push(Stmt::Assert("final".into(), v(x).addk(-(high + size.k as i128))));
    out
}

fn widerel(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let high = if tiny { 1 } else { 100 };
    let mut places: Vec<V> = (0..size.n).map(|_| fresh(&mut b, &mut out, 0, Some(high))).collect();
    for _ in 0..size.k {
        let cond = unknown(&mut b, &mut out);
        let raised: Vec<V> = places.iter().map(|_| b.id()).collect();
        let body = places.iter().zip(&raised).map(|(x, t)| Stmt::Let(*t, v(*x).addk(1))).collect();
        let joined: Vec<V> = places.iter().map(|_| b.id()).collect();
        let phis = (0..size.n).map(|p| (joined[p], vec![raised[p], places[p]], Some(places[p]))).collect();
        out.push(iff(cond, body, vec![], phis));
        places = joined;
    }
    out.push(Stmt::Assert("final".into(), v(places[0]).addk(-(high + size.k as i128))));
    out
}

fn wideirr(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let high = if tiny { 1 } else { 100 };
    let mut places: Vec<V> = (0..size.n).map(|_| fresh(&mut b, &mut out, 0, Some(high))).collect();
    for _ in 0..size.k {
        for place in &mut places {
            *place = bump(&mut b, &mut out, *place, &k(1));
        }
    }
    out.push(Stmt::Assert("final".into(), v(places[0]).addk(-(high + size.k as i128))));
    out
}

/// `ok = if cond { 1 } else { 0 }`; returns `ok`.
fn flag(b: &mut B, out: &mut Vec<Stmt>, cond: Lin) -> V {
    let one = b.id();
    let zero = b.id();
    let ok = b.id();
    out.push(iff(cond, vec![Stmt::Let(one, k(1))], vec![Stmt::Let(zero, k(0))], vec![(ok, vec![one, zero], None)]));
    ok
}

/// `if ok >= 1 { body }`.
fn guard(ok: V, body: Vec<Stmt>) -> Stmt {
    iff(k(1).sub(&v(ok)), body, vec![], vec![])
}

fn flag_range(tiny: bool) -> (i128, Option<i128>) {
    if tiny {
        (4, Some(5))
    } else {
        (0, Some(10))
    }
}

fn flags(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let mut oks = Vec::new();
    let mut goal = k(5 * size.k as i128);
    for _ in 0..size.k {
        let y = fresh(&mut b, &mut out, low, high);
        oks.push(flag(&mut b, &mut out, k(5).sub(&v(y))));
        goal = goal.sub(&v(y));
    }
    let mut body = vec![Stmt::Assert("final".into(), goal)];
    for ok in oks.into_iter().rev() {
        body = vec![guard(ok, body)];
    }
    out.extend(body);
    out
}

fn flagchain(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let y = fresh(&mut b, &mut out, low, high);
    let mut ok = flag(&mut b, &mut out, k(5).sub(&v(y)));
    for _ in 1..size.k {
        ok = flag(&mut b, &mut out, k(1).sub(&v(ok)));
    }
    out.push(guard(ok, vec![Stmt::Assert("final".into(), k(5).sub(&v(y)))]));
    out
}

/// `if c { y = 0; z = 1 } else { y = 1; z = 0 }`; returns `(y, z)`.
fn swap(b: &mut B, out: &mut Vec<Stmt>) -> (V, V) {
    let cond = unknown(b, out);
    let ids: Vec<V> = (0..6).map(|_| b.id()).collect();
    out.push(iff(
        cond,
        vec![Stmt::Let(ids[0], k(0)), Stmt::Let(ids[1], k(1))],
        vec![Stmt::Let(ids[2], k(1)), Stmt::Let(ids[3], k(0))],
        vec![(ids[4], vec![ids[0], ids[2]], None), (ids[5], vec![ids[1], ids[3]], None)],
    ));
    (ids[4], ids[5])
}

fn swaps_with(size: &Size, invariants: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let mut total = k(-(size.k as i128));
    for step in 0..size.k {
        let (y, z) = swap(&mut b, &mut out);
        let pair = v(y).add(&v(z));
        if invariants {
            let stated = pair.addk(-1);
            out.push(Stmt::Invariant(format!("inv{step}"), stated));
        }
        total = total.add(&pair);
    }
    out.push(Stmt::Assert("final".into(), total));
    out
}

fn swaps(size: &Size, _: bool) -> Vec<Stmt> {
    swaps_with(size, false)
}

fn swapsinv(size: &Size, _: bool) -> Vec<Stmt> {
    swaps_with(size, true)
}

fn mix(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let y = fresh(&mut b, &mut out, low, high);
    let ok = flag(&mut b, &mut out, k(5).sub(&v(y)));
    let top = if tiny { 1 } else { 100 };
    let mut x = fresh(&mut b, &mut out, 0, Some(top));
    for _ in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(1));
    }
    let goal = v(x).sub(&v(y)).addk(-(top - 5 + size.k as i128));
    out.push(guard(ok, vec![Stmt::Assert("final".into(), goal)]));
    out
}

fn rel1(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (y, z) = swap(&mut b, &mut out);
    let top = if tiny { 1 } else { 100 };
    let mut x = fresh(&mut b, &mut out, 0, Some(top));
    for _ in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(1));
    }
    out.push(Stmt::Assert("final".into(), v(y).add(&v(z)).add(&v(x)).addk(-(1 + top + size.k as i128))));
    out
}

/// k joins without a join value: both arms state `i < len` of a pair of
/// their own. With `uneven`, the two arms state different bounds.
fn armfact_with(size: &Size, tiny: bool, uneven: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(3);
    let mut goal = k(size.k as i128);
    for _ in 0..size.k {
        let i = fresh(&mut b, &mut out, 0, range);
        let len = fresh(&mut b, &mut out, 0, range);
        let cond = unknown(&mut b, &mut out);
        let stated = lt(&v(i), &v(len));
        let other = if uneven { stated.addk(2) } else { stated.clone() };
        out.push(iff(cond, vec![Stmt::Assume(stated, false)], vec![Stmt::Assume(other, false)], vec![]));
        goal = goal.add(&v(i)).sub(&v(len));
    }
    out.push(Stmt::Assert("final".into(), goal));
    out
}

fn armfact(size: &Size, tiny: bool) -> Vec<Stmt> {
    armfact_with(size, tiny, false)
}

fn multiexit(size: &Size, tiny: bool) -> Vec<Stmt> {
    armfact_with(size, tiny, true)
}

/// k joins without a join value whose arms state different facts:
/// `y >= 5` in one, `z >= 5` in the other.
fn armcorr(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = if tiny { (4, Some(5)) } else { (0, Some(10)) };
    let mut goal = k(5 * size.k as i128);
    for _ in 0..size.k {
        let y = fresh(&mut b, &mut out, low, high);
        let z = fresh(&mut b, &mut out, low, high);
        let cond = unknown(&mut b, &mut out);
        out.push(iff(cond, vec![Stmt::Assume(k(5).sub(&v(y)), false)], vec![Stmt::Assume(k(5).sub(&v(z)), false)], vec![]));
        goal = goal.sub(&v(y)).sub(&v(z));
    }
    out.push(Stmt::Assert("final".into(), goal));
    out
}

/// ADDENDS with the invariant `x <= x_0 + y_1 + .. + y_i` after each join.
fn addendsinv(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let start = fresh(&mut b, &mut out, 0, tiny.then_some(2));
    let mut x = start;
    let mut sum = v(start);
    for step in 0..size.k {
        let y = fresh(&mut b, &mut out, 0, Some(if tiny { 1 } else { 1000 }));
        x = bump(&mut b, &mut out, x, &v(y));
        sum = sum.add(&v(y));
        out.push(Stmt::Invariant(format!("inv{step}"), v(x).sub(&sum)));
    }
    out.push(Stmt::Assert("final".into(), v(x).sub(&sum)));
    out
}

/// CHAINSYM with an invariant of a new shape after each join:
/// `x <= n + w_j + j` for a fresh `w_j >= 0`.
fn chainsyminv(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(2);
    let n = fresh(&mut b, &mut out, 0, range);
    let mut x = fresh(&mut b, &mut out, 0, range);
    out.push(Stmt::Assume(le(&v(x), &v(n)), false));
    for step in 0..size.k {
        let w = fresh(&mut b, &mut out, 0, Some(if tiny { 1 } else { 3 }));
        x = bump(&mut b, &mut out, x, &k(1));
        out.push(Stmt::Invariant(format!("inv{step}"), v(x).sub(&v(n)).sub(&v(w)).addk(-(step as i128 + 1))));
    }
    out.push(Stmt::Assert("final".into(), v(x).sub(&v(n)).addk(-(size.k as i128))));
    out
}

/// CHAIN with an obligation after every join, as an overflow check would
/// be.
fn chainobl(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let high = if tiny { 2 } else { 100 };
    let mut x = fresh(&mut b, &mut out, 0, Some(high));
    for step in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(1));
        let name = if step + 1 == size.k { "final".to_owned() } else { format!("step{step}") };
        out.push(Stmt::Assert(name, v(x).addk(-(high + step as i128 + 1))));
    }
    out
}

/// One `match` of m arms, each giving `x` a value of its own that is at
/// most 100.
fn matchshapes(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let arms = size.k as i128;
    let top = if tiny { 2 } else { 100 };
    let given: Vec<V> = (0..size.k).map(|_| fresh(&mut b, &mut out, 0, Some(top))).collect();
    let selector = fresh(&mut b, &mut out, 0, Some(arms - 1));
    let bodies = (0..arms).map(|arm| (vec![v(selector).addk(-arm), k(arm).sub(&v(selector))], vec![])).collect();
    let joined = b.id();
    out.push(Stmt::Branch(bodies, vec![(joined, given, None)]));
    out.push(Stmt::Assert("final".into(), v(joined).addk(-top)));
    out
}

/// One `match` of m arms: arm j sets `tag = j % 3` and a `len`, which is
/// below `n` in the arms with tag 0. Then `if tag == 0 { len < n }`.
fn bigtag(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let arms = size.k as i128;
    let range = Some(if tiny { 2 } else { 50 });
    let n = fresh(&mut b, &mut out, 0, range);
    let selector = fresh(&mut b, &mut out, 0, Some(arms - 1));
    let (mut bodies, mut tags, mut lens) = (Vec::new(), Vec::new(), Vec::new());
    for arm in 0..arms {
        let mut body = Vec::new();
        let tag = b.id();
        body.push(Stmt::Let(tag, k(arm % 3)));
        let len = fresh(&mut b, &mut body, 0, range);
        if arm % 3 == 0 {
            body.push(Stmt::Assume(lt(&v(len), &v(n)), false));
        }
        tags.push(tag);
        lens.push(len);
        bodies.push((vec![v(selector).addk(-arm), k(arm).sub(&v(selector))], body));
    }
    let tag = b.id();
    let len = b.id();
    out.push(Stmt::Branch(bodies, vec![(tag, tags, None), (len, lens, None)]));
    out.push(iff(v(tag), vec![Stmt::Assert("final".into(), lt(&v(len), &v(n)))], vec![], vec![]));
    out
}

/// k times the pair `if c { a = a + 1 }`, `if c { b = b + 1 }` on one
/// condition `c`; then `a <= b`.
fn samecond(size: &Size, _: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let flag = fresh(&mut b, &mut out, 0, Some(1));
    let cond = k(1).sub(&v(flag));
    let mut pair = [b.id(), b.id()];
    out.push(Stmt::Let(pair[0], k(0)));
    out.push(Stmt::Let(pair[1], k(0)));
    for _ in 0..size.k {
        for place in &mut pair {
            let raised = b.id();
            let joined = b.id();
            out.push(iff(cond.clone(), vec![Stmt::Let(raised, v(*place).addk(1))], vec![], vec![(joined, vec![raised, *place], Some(*place))]));
            *place = joined;
        }
    }
    out.push(Stmt::Assert("final".into(), v(pair[0]).sub(&v(pair[1]))));
    out
}

/// k pairs of flags over equal values `y_i == w_i`: `okx_i` from
/// `y_i >= 5` and `oky_i` from `w_i >= 5`. Inside the conditions on every
/// `okx_i`, every `oky_i` is 1. With `x_first` the `okx` joins stand before
/// the `oky` joins, otherwise after them.
fn order_with(size: &Size, tiny: bool, x_first: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let mut pairs = Vec::new();
    for _ in 0..size.k {
        let y = fresh(&mut b, &mut out, low, high);
        let w = fresh(&mut b, &mut out, low, high);
        out.push(Stmt::Assume(v(y).sub(&v(w)), true));
        pairs.push((y, w));
    }
    let (mut okx, mut oky) = (Vec::new(), Vec::new());
    for first in [true, false] {
        for (y, w) in &pairs {
            if first == x_first {
                okx.push(flag(&mut b, &mut out, k(5).sub(&v(*y))));
            } else {
                oky.push(flag(&mut b, &mut out, k(5).sub(&v(*w))));
            }
        }
    }
    let mut goal = k(size.k as i128);
    for ok in &oky {
        goal = goal.sub(&v(*ok));
    }
    let mut body = vec![Stmt::Assert("final".into(), goal)];
    for ok in okx.into_iter().rev() {
        body = vec![guard(ok, body)];
    }
    out.extend(body);
    out
}

fn orderxy(size: &Size, tiny: bool) -> Vec<Stmt> {
    order_with(size, tiny, true)
}

fn orderyx(size: &Size, tiny: bool) -> Vec<Stmt> {
    order_with(size, tiny, false)
}

/// `lo <= hi`; `if c { lo = hi }`; k times `if d { lo += 1; hi += 1 }`;
/// then `lo <= hi`.
fn tie(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(2);
    let mut lo = fresh(&mut b, &mut out, 0, range);
    let mut hi = fresh(&mut b, &mut out, 0, range);
    out.push(Stmt::Assume(le(&v(lo), &v(hi)), false));
    let cond = unknown(&mut b, &mut out);
    let joined = b.id();
    out.push(iff(cond, vec![], vec![], vec![(joined, vec![hi, lo], Some(lo))]));
    lo = joined;
    for _ in 0..size.k {
        let cond = unknown(&mut b, &mut out);
        let ids: Vec<V> = (0..4).map(|_| b.id()).collect();
        out.push(iff(
            cond,
            vec![Stmt::Let(ids[0], v(lo).addk(1)), Stmt::Let(ids[1], v(hi).addk(1))],
            vec![],
            vec![(ids[2], vec![ids[0], lo], Some(lo)), (ids[3], vec![ids[1], hi], Some(hi))],
        ));
        lo = ids[2];
        hi = ids[3];
    }
    out.push(Stmt::Assert("final".into(), le(&v(lo), &v(hi))));
    out
}

/// SWAPS(k), then the question whether the sum is already decided.
fn pmono(size: &Size, _: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let mut total = k(-(size.k as i128));
    for _ in 0..size.k {
        let (y, z) = swap(&mut b, &mut out);
        total = total.add(&v(y)).add(&v(z));
    }
    out.push(Stmt::Query("decided-last".into(), total));
    out
}

/// FLAGCHAIN(k), and inside its condition k questions about unknowns.
fn op5casc(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let y = fresh(&mut b, &mut out, low, high);
    let mut ok = flag(&mut b, &mut out, k(5).sub(&v(y)));
    for _ in 1..size.k {
        ok = flag(&mut b, &mut out, k(1).sub(&v(ok)));
    }
    let mut body = Vec::new();
    for step in 0..size.k {
        let unknown = fresh(&mut b, &mut body, 0, Some(1));
        let mark = if step + 1 == size.k { "-last".to_owned() } else { step.to_string() };
        body.push(Stmt::Query(format!("asked{mark}"), k(1).sub(&v(unknown))));
    }
    body.push(Stmt::Assert("final".into(), k(5).sub(&v(y))));
    out.push(guard(ok, body));
    out
}

/// MIX with the invariant `5 * ok <= y` after the flag's join.
fn mixinv(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let y = fresh(&mut b, &mut out, low, high);
    let ok = flag(&mut b, &mut out, k(5).sub(&v(y)));
    out.push(Stmt::Invariant("inv".into(), v(ok).scale(5).sub(&v(y))));
    let top = if tiny { 1 } else { 100 };
    let mut x = fresh(&mut b, &mut out, 0, Some(top));
    for _ in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(1));
    }
    let goal = v(x).sub(&v(y)).addk(-(top - 5 + size.k as i128));
    out.push(guard(ok, vec![Stmt::Assert("final".into(), goal)]));
    out
}

/// REL1 with the invariant `y + z <= 1` after the swap.
fn rel1inv(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (y, z) = swap(&mut b, &mut out);
    out.push(Stmt::Invariant("inv".into(), v(y).add(&v(z)).addk(-1)));
    let top = if tiny { 1 } else { 100 };
    let mut x = fresh(&mut b, &mut out, 0, Some(top));
    for _ in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(1));
    }
    out.push(Stmt::Assert("final".into(), v(y).add(&v(z)).add(&v(x)).addk(-(1 + top + size.k as i128))));
    out
}

/// FLAGS with an invariant about something else between the flags and the
/// conditions that test them.
fn flagscut(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let mut oks = Vec::new();
    let mut goal = k(5 * size.k as i128);
    for _ in 0..size.k {
        let y = fresh(&mut b, &mut out, low, high);
        oks.push(flag(&mut b, &mut out, k(5).sub(&v(y))));
        goal = goal.sub(&v(y));
    }
    let other = fresh(&mut b, &mut out, 0, Some(3));
    out.push(Stmt::Invariant("inv".into(), v(other).scale(-1)));
    let mut body = vec![Stmt::Assert("final".into(), goal)];
    for ok in oks.into_iter().rev() {
        body = vec![guard(ok, body)];
    }
    out.extend(body);
    out
}

/// GUARDED with a limit that grows as it goes: `n_j = n_(j-1) + d_j`.
fn limitchain(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(3);
    let mut n = fresh(&mut b, &mut out, 0, range);
    let mut i = fresh(&mut b, &mut out, 0, range);
    out.push(Stmt::Assume(lt(&v(i), &v(n)), false));
    for _ in 0..size.k {
        let d = fresh(&mut b, &mut out, 0, Some(1));
        let grown = b.id();
        out.push(Stmt::Let(grown, v(n).add(&v(d))));
        n = grown;
        let raised = b.id();
        let joined = b.id();
        out.push(iff(v(i).addk(2).sub(&v(n)), vec![Stmt::Let(raised, v(i).addk(1))], vec![], vec![(joined, vec![raised, i], Some(i))]));
        i = joined;
    }
    out.push(Stmt::Assert("final".into(), lt(&v(i), &v(n))));
    out
}

/// No join: `s_j = s_(j-1) + a_j`, each `a_j` in 0..255, and after each
/// step the obligation that the sum is within its bound.
fn sumobl(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let top = if tiny { 2 } else { 255 };
    let mut sum = fresh(&mut b, &mut out, 0, Some(top));
    for step in 0..size.k {
        let a = fresh(&mut b, &mut out, 0, Some(top));
        let next = b.id();
        out.push(Stmt::Let(next, v(sum).add(&v(a))));
        sum = next;
        let name = if step + 1 == size.k { "final".to_owned() } else { format!("step{step}") };
        out.push(Stmt::Assert(name, v(sum).addk(-(top * (step as i128 + 2)))));
    }
    out
}

/// A branch `depth` deep whose every innermost arm states `i < len`.
fn nested_check(b: &mut B, out: &mut Vec<Stmt>, stated: &Lin, depth: usize) {
    if depth == 0 {
        out.push(Stmt::Assume(stated.clone(), false));
        return;
    }
    let cond = unknown(b, out);
    let (mut then, mut otherwise) = (Vec::new(), Vec::new());
    nested_check(b, &mut then, stated, depth - 1);
    nested_check(b, &mut otherwise, stated, depth - 1);
    out.push(iff(cond, then, otherwise, vec![]));
}

/// k pairs, each checked inside a branch two deep; then the sum.
fn nestfact(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(3);
    let mut goal = k(size.k as i128);
    for _ in 0..size.k {
        let i = fresh(&mut b, &mut out, 0, range);
        let len = fresh(&mut b, &mut out, 0, range);
        nested_check(&mut b, &mut out, &lt(&v(i), &v(len)), 2);
        goal = goal.add(&v(i)).sub(&v(len));
    }
    out.push(Stmt::Assert("final".into(), goal));
    out
}

/// One pair checked inside a branch k deep.
fn nestdeep(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = tiny.then_some(3);
    let i = fresh(&mut b, &mut out, 0, range);
    let len = fresh(&mut b, &mut out, 0, range);
    nested_check(&mut b, &mut out, &lt(&v(i), &v(len)), size.k);
    out.push(Stmt::Assert("final".into(), lt(&v(i), &v(len))));
    out
}

/// A `match` of eight arms that sets `t` to the arm's number.
fn eight(b: &mut B, out: &mut Vec<Stmt>) -> V {
    let selector = fresh(b, out, 0, Some(7));
    let (mut bodies, mut raws) = (Vec::new(), Vec::new());
    for arm in 0..8 {
        let value = b.id();
        raws.push(value);
        bodies.push((vec![v(selector).addk(-arm), k(arm).sub(&v(selector))], vec![Stmt::Let(value, k(arm))]));
    }
    let joined = b.id();
    out.push(Stmt::Branch(bodies, vec![(joined, raws, None)]));
    joined
}

/// k swaps and one `match` of eight arms that needs no split; the `match`
/// stands after the swaps or before them.
fn split_with(size: &Size, after: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let mut total = k(-(size.k as i128) - 7);
    let mut t = None;
    if !after {
        t = Some(eight(&mut b, &mut out));
    }
    for _ in 0..size.k {
        let (y, z) = swap(&mut b, &mut out);
        total = total.add(&v(y)).add(&v(z));
    }
    let t = t.unwrap_or_else(|| eight(&mut b, &mut out));
    out.push(Stmt::Assert("final".into(), total.add(&v(t))));
    out
}

fn splitafter(size: &Size, _: bool) -> Vec<Stmt> {
    split_with(size, true)
}

fn splitbefore(size: &Size, _: bool) -> Vec<Stmt> {
    split_with(size, false)
}

/// One BIGTAG `match`; returns its `tag` and `len`.
fn tagged(b: &mut B, out: &mut Vec<Stmt>, n: V, arms: i128, range: Option<i128>) -> (V, V) {
    let selector = fresh(b, out, 0, Some(arms - 1));
    let (mut bodies, mut tags, mut lens) = (Vec::new(), Vec::new(), Vec::new());
    for arm in 0..arms {
        let mut body = Vec::new();
        let tag = b.id();
        body.push(Stmt::Let(tag, k(arm % 3)));
        let len = fresh(b, &mut body, 0, range);
        if arm % 3 == 0 {
            body.push(Stmt::Assume(lt(&v(len), &v(n)), false));
        }
        tags.push(tag);
        lens.push(len);
        bodies.push((vec![v(selector).addk(-arm), k(arm).sub(&v(selector))], body));
    }
    let tag = b.id();
    let len = b.id();
    out.push(Stmt::Branch(bodies, vec![(tag, tags, None), (len, lens, None)]));
    (tag, len)
}

/// Two BIGTAG matches; inside both tests, the sum of the two lengths.
fn bigtag2(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let range = Some(if tiny { 2 } else { 50 });
    let n = fresh(&mut b, &mut out, 0, range);
    let (tag1, len1) = tagged(&mut b, &mut out, n, size.k as i128, range);
    let (tag2, len2) = tagged(&mut b, &mut out, n, size.k as i128, range);
    let goal = v(len1).add(&v(len2)).sub(&v(n).scale(2)).addk(2);
    let inner = iff(v(tag2), vec![Stmt::Assert("final".into(), goal)], vec![], vec![]);
    out.push(iff(v(tag1), vec![inner], vec![], vec![]));
    out
}

/// FLAGCHAIN(k) and k joins that need no decision; inside the last flag's
/// test, `y` plus the k values is at least 5. The k joins stand after the
/// flags, or before them.
fn flagnoise_with(size: &Size, tiny: bool, noise_first: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let (low, high) = flag_range(tiny);
    let y = fresh(&mut b, &mut out, low, high);
    let mut goal = k(5).sub(&v(y));
    let mut ok = 0;
    for first in [true, false] {
        if first == noise_first {
            for _ in 0..size.k {
                let cond = unknown(&mut b, &mut out);
                let t = flag(&mut b, &mut out, cond);
                goal = goal.sub(&v(t));
            }
        } else {
            ok = flag(&mut b, &mut out, k(5).sub(&v(y)));
            for _ in 1..size.k {
                ok = flag(&mut b, &mut out, k(1).sub(&v(ok)));
            }
        }
    }
    out.push(guard(ok, vec![Stmt::Assert("final".into(), goal)]));
    out
}

fn flagnoise(size: &Size, tiny: bool) -> Vec<Stmt> {
    flagnoise_with(size, tiny, false)
}

fn flagnoise2(size: &Size, tiny: bool) -> Vec<Stmt> {
    flagnoise_with(size, tiny, true)
}

/// k pairs: `z = if c { n - 5 } else { n - 3 }` with `n >= 10`; `x <= z`;
/// `if d { x = 5 }`. Then the sum of every `x - z` is at most 0. The bound
/// of the second join needs a fact about `n`, which only the first join's
/// bound names.
fn psbridge(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let mut goal = k(0);
    for _ in 0..size.k {
        let n = fresh(&mut b, &mut out, 10, tiny.then_some(11));
        let cond = unknown(&mut b, &mut out);
        let (low, high, z) = (b.id(), b.id(), b.id());
        out.push(iff(cond, vec![Stmt::Let(low, v(n).addk(-5))], vec![Stmt::Let(high, v(n).addk(-3))], vec![(z, vec![low, high], None)]));
        let x = fresh(&mut b, &mut out, 0, tiny.then_some(8));
        out.push(Stmt::Assume(le(&v(x), &v(z)), false));
        let cond = unknown(&mut b, &mut out);
        let (five, joined) = (b.id(), b.id());
        out.push(iff(cond, vec![Stmt::Let(five, k(5))], vec![], vec![(joined, vec![five, x], Some(x))]));
        goal = goal.add(&v(joined)).sub(&v(z));
    }
    out.push(Stmt::Assert("final".into(), goal));
    out
}

/// `3 * x == 2 * y` and `x <= 5`, so `x` is even and at most 4; then k
/// times `if c { x = x + 6 }`; then `x <= 4 + 6k`. The equality has no unit
/// coefficient.
fn stride(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let y = fresh(&mut b, &mut out, 0, Some(if tiny { 7 } else { 1000 }));
    let mut x = fresh(&mut b, &mut out, 0, Some(5));
    out.push(Stmt::Assume(v(x).scale(3).sub(&v(y).scale(2)), true));
    for _ in 0..size.k {
        x = bump(&mut b, &mut out, x, &k(6));
    }
    out.push(Stmt::Assert("final".into(), v(x).addk(-(4 + 6 * size.k as i128))));
    out
}

/// SUMOBL where after every 16 steps the sum is stated to be within its
/// bound and then replaced by a new value that has only that bound: what a
/// cut that forgets definitions would leave.
fn sumoblcut(size: &Size, tiny: bool) -> Vec<Stmt> {
    let (mut b, mut out) = (B::default(), Vec::new());
    let top = if tiny { 2 } else { 255 };
    let mut sum = fresh(&mut b, &mut out, 0, Some(top));
    for step in 0..size.k {
        let a = fresh(&mut b, &mut out, 0, Some(top));
        let next = b.id();
        out.push(Stmt::Let(next, v(sum).add(&v(a))));
        sum = next;
        let bound = top * (step as i128 + 2);
        let name = if step + 1 == size.k { "final".to_owned() } else { format!("step{step}") };
        out.push(Stmt::Assert(name, v(sum).addk(-bound)));
        if step % 16 == 15 {
            out.push(Stmt::Invariant(format!("inv{step}"), v(sum).addk(-bound)));
            sum = fresh(&mut b, &mut out, 0, Some(bound));
        }
    }
    out
}

fn ks(list: &[usize]) -> Vec<Size> {
    list.iter().map(|k| Size { k: *k, n: 0 }).collect()
}

fn nks(list: &[(usize, usize)]) -> Vec<Size> {
    list.iter().map(|(n, k)| Size { k: *k, n: *n }).collect()
}

pub fn all() -> Vec<Family> {
    let grown = [1, 2, 3, 4, 6, 8, 10, 12, 14, 16, 24, 32, 48, 64, 96, 128];
    let wide = [(4, 4), (4, 8), (4, 16), (4, 32), (16, 4), (16, 8), (16, 16), (16, 32)];
    let family = |name, make: fn(&Size, bool) -> Vec<Stmt>| Family { name, sizes: ks(&grown), tiny: ks(&[1, 2, 3]), make };
    vec![
        family("CHAIN", chain),
        family("CHAINSYM", chainsym),
        family("ADDENDS", addends),
        family("GUARDED", guarded),
        family("MINSEL", minsel),
        Family { name: "BINSEARCH", sizes: ks(&[1]), tiny: ks(&[1]), make: binsearch },
        Family { name: "MATCH", sizes: nks(&[(4, 1), (4, 2), (4, 3), (4, 4), (4, 6), (4, 8), (4, 16), (4, 32), (4, 64)]), tiny: nks(&[(3, 1), (3, 2)]), make: matched },
        family("NEST", nest),
        Family { name: "WIDEREL", sizes: nks(&wide), tiny: nks(&[(2, 2)]), make: widerel },
        Family { name: "WIDEIRR", sizes: nks(&wide), tiny: nks(&[(2, 2)]), make: wideirr },
        family("FLAGS", flags),
        family("FLAGCHAIN", flagchain),
        family("SWAPS", swaps),
        family("SWAPSINV", swapsinv),
        family("MIX", mix),
        family("REL1", rel1),
        Family { name: "OP5", sizes: ks(&grown), tiny: ks(&[1, 2]), make: op5 },
        family("ARMFACT", armfact),
        family("MULTIEXIT", multiexit),
        family("ARMCORR", armcorr),
        family("ADDENDSINV", addendsinv),
        family("CHAINSYMINV", chainsyminv),
        family("CHAINOBL", chainobl),
        Family { name: "MATCHSHAPES", sizes: ks(&[2, 4, 8, 16, 32, 64, 128, 256]), tiny: ks(&[2, 3]), make: matchshapes },
        Family { name: "BIGTAG", sizes: ks(&[3, 6, 12, 16, 24, 48, 64, 96, 128, 192, 256]), tiny: ks(&[3, 6]), make: bigtag },
        family("SAMECOND", samecond),
        family("ORDERXY", orderxy),
        family("ORDERYX", orderyx),
        family("TIE", tie),
        Family { name: "PMONO", sizes: ks(&[1, 2, 3, 4, 5, 6, 7, 8]), tiny: ks(&[1, 2, 3]), make: pmono },
        Family { name: "OP5CASC", sizes: ks(&grown), tiny: ks(&[1, 2]), make: op5casc },
        family("MIXINV", mixinv),
        family("REL1INV", rel1inv),
        family("FLAGSCUT", flagscut),
        family("FLAGNOISE", flagnoise),
        family("FLAGNOISE2", flagnoise2),
        Family { name: "PSBRIDGE", sizes: ks(&grown), tiny: ks(&[1, 2]), make: psbridge },
        family("STRIDE", stride),
        Family { name: "SUMOBLCUT", sizes: ks(&[16, 32, 48, 64, 96, 128]), tiny: ks(&[]), make: sumoblcut },
        family("LIMITCHAIN", limitchain),
        family("SUMOBL", sumobl),
        Family { name: "NESTFACT", sizes: ks(&[1, 2, 3, 4, 6, 8, 16, 32, 64]), tiny: ks(&[1, 2]), make: nestfact },
        Family { name: "NESTDEEP", sizes: ks(&[1, 2, 3, 4, 5, 6, 7, 8]), tiny: ks(&[1, 2, 3]), make: nestdeep },
        Family { name: "SPLITAFTER", sizes: ks(&[1, 2, 3, 4]), tiny: ks(&[1, 2]), make: splitafter },
        Family { name: "SPLITBEFORE", sizes: ks(&[1, 2, 3, 4]), tiny: ks(&[1, 2]), make: splitbefore },
        Family { name: "BIGTAG2", sizes: ks(&[3, 6, 9, 12, 15, 24, 48]), tiny: ks(&[3]), make: bigtag2 },
    ]
}

/// The theory-only family: `n` variables and `2n` inequalities with
/// coefficients in -3..=3 and a constant that keeps the origin feasible.
pub fn dense(n: u32, seed: u64) -> Vec<TL> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(u64::from(n));
    let mut next = move |modulus: u64| {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) % modulus
    };
    let mut lits = Vec::new();
    for _ in 0..2 * n {
        let mut lin = Lin::k(-(next(11) as i128));
        for value in 0..n {
            let coefficient = next(7) as i128 - 3;
            lin = lin.add(&Lin::v(value).scale(coefficient));
        }
        lits.push(TL { lin, eq: false, tag: 0 });
    }
    lits
}
