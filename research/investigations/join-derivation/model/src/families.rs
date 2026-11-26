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
            out.push(Stmt::Assert(format!("inv{step}"), stated.clone()));
            out.push(Stmt::Assume(stated, false));
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
