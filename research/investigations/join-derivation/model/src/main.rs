//! The model of the join-derivation study
//! (`research/investigations/join-derivation/DESIGN.md`).
//!
//! It holds a small value-form language, the theory step of [RANGE-3] by
//! plain Fourier-Motzkin elimination, the candidate join rules and decision
//! steps, the input families, and an exhaustive evaluator that decides each
//! obligation's truth on small instances. It prints one CSV row per
//! measurement. It is an instrument of an unmerged study branch.

mod families;
mod theory;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use theory::{theory, Lin, Out, Stats, TL, TOP, V};

/// Where a literal of the walk came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The condition of an enclosing arm.
    Path,
    /// A fact from a source: a requirement, an invariant, a type range.
    Fact,
    /// The definition of a value.
    Def,
    /// A join value's equality with its arm value.
    Phi,
    /// A state bound a join recorded.
    State,
    /// A transition bound a join recorded.
    Trans,
}

/// `lin <= 0`, or `lin == 0` when `eq`.
#[derive(Clone, Debug)]
pub struct Lit {
    pub lin: Lin,
    pub eq: bool,
    pub kind: Kind,
}

/// One arm of a branch: its conditions and its body.
pub type ArmSrc = (Vec<Lin>, Vec<Stmt>);
/// A join value, its value in each arm, and the place's value at entry.
pub type PhiSrc = (V, Vec<V>, Option<V>);

/// The model's statements.
pub enum Stmt {
    /// An unknown value with an optional range.
    Fresh(V, Option<i128>, Option<i128>),
    /// A value defined as a linear expression.
    Let(V, Lin),
    /// A fact: `lin <= 0`, or `lin == 0` when the flag is set.
    Assume(Lin, bool),
    /// An `if` or a `match`.
    Branch(Vec<ArmSrc>, Vec<PhiSrc>),
    /// An obligation `lin <= 0` that is expected to hold.
    Assert(String, Lin),
    /// A question `lin <= 0` whose expected answer is "not proved".
    Query(String, Lin),
}

struct Arm {
    lits: Vec<Lit>,
    inner: Vec<usize>,
}

struct Phi {
    v: V,
    raw: Vec<V>,
    entry: Option<V>,
}

struct Join {
    arms: Vec<Arm>,
    phis: Vec<Phi>,
    entry_lits: Vec<Lit>,
    entry_joins: Vec<usize>,
    watermark: V,
    values: Vec<V>,
}

struct Oblig {
    name: String,
    expect: bool,
    gamma: Vec<Lit>,
    joins: Vec<usize>,
    goal: Lin,
}

#[derive(Default)]
struct Model {
    joins: Vec<Join>,
    obligs: Vec<Oblig>,
    defs: HashMap<V, Lin>,
    phi_owner: HashMap<V, usize>,
    value_joins: HashMap<V, Vec<usize>>,
    seen: V,
}

impl Model {
    fn build(program: &[Stmt]) -> Model {
        let mut model = Model::default();
        let mut ctx = Vec::new();
        let mut joins = Vec::new();
        model.walk(program, &mut ctx, &mut joins);
        for (index, join) in model.joins.iter().enumerate() {
            for value in &join.values {
                model.value_joins.entry(*value).or_default().push(index);
            }
        }
        model
    }

    fn see(&mut self, value: V) {
        if value >= self.seen {
            self.seen = value + 1;
        }
    }

    fn walk(&mut self, stmts: &[Stmt], ctx: &mut Vec<Lit>, open: &mut Vec<usize>) {
        for stmt in stmts {
            match stmt {
                Stmt::Fresh(value, low, high) => {
                    self.see(*value);
                    if let Some(low) = low {
                        ctx.push(Lit { lin: Lin::k(*low).sub(&Lin::v(*value)), eq: false, kind: Kind::Fact });
                    }
                    if let Some(high) = high {
                        ctx.push(Lit { lin: Lin::v(*value).sub(&Lin::k(*high)), eq: false, kind: Kind::Fact });
                    }
                }
                Stmt::Let(value, expr) => {
                    self.see(*value);
                    self.defs.insert(*value, expr.clone());
                    ctx.push(Lit { lin: Lin::v(*value).sub(expr), eq: true, kind: Kind::Def });
                }
                Stmt::Assume(lin, eq) => ctx.push(Lit { lin: lin.clone(), eq: *eq, kind: Kind::Fact }),
                Stmt::Assert(name, goal) | Stmt::Query(name, goal) => self.obligs.push(Oblig {
                    name: name.clone(),
                    expect: matches!(stmt, Stmt::Assert(..)),
                    gamma: ctx.clone(),
                    joins: open.clone(),
                    goal: goal.clone(),
                }),
                Stmt::Branch(arms, phis) => {
                    let watermark = self.seen;
                    let entry_lits = ctx.clone();
                    let entry_joins = open.clone();
                    let (base_lits, base_joins) = (ctx.len(), open.len());
                    let id = self.joins.len();
                    self.joins.push(Join {
                        arms: Vec::new(),
                        phis: Vec::new(),
                        entry_lits: Vec::new(),
                        entry_joins: Vec::new(),
                        watermark,
                        values: Vec::new(),
                    });
                    let mut done = Vec::new();
                    for (index, (conds, body)) in arms.iter().enumerate() {
                        for cond in conds {
                            ctx.push(Lit { lin: cond.clone(), eq: false, kind: Kind::Path });
                        }
                        self.walk(body, ctx, open);
                        let mut lits = ctx[base_lits..].to_vec();
                        for (value, raw, _) in phis {
                            lits.push(Lit { lin: Lin::v(*value).sub(&Lin::v(raw[index])), eq: true, kind: Kind::Phi });
                        }
                        let inner = open[base_joins..].to_vec();
                        ctx.truncate(base_lits);
                        open.truncate(base_joins);
                        done.push(Arm { lits, inner });
                    }
                    let mut values = HashSet::new();
                    for arm in &done {
                        for lit in &arm.lits {
                            values.extend(lit.lin.vars());
                        }
                    }
                    for (value, _, _) in phis {
                        self.see(*value);
                        self.phi_owner.insert(*value, id);
                        values.insert(*value);
                    }
                    let mut values: Vec<V> = values.into_iter().collect();
                    values.sort_unstable();
                    self.joins[id] = Join {
                        arms: done,
                        phis: phis.iter().map(|(v, raw, entry)| Phi { v: *v, raw: raw.clone(), entry: *entry }).collect(),
                        entry_lits,
                        entry_joins,
                        watermark,
                        values,
                    };
                    open.push(id);
                }
            }
        }
    }

    /// `value` as a linear expression of values that exist below `watermark`.
    fn expand(&self, value: V, watermark: V) -> Option<Lin> {
        if value < watermark {
            return Some(Lin::v(value));
        }
        let def = self.defs.get(&value)?;
        let mut result = Lin::k(def.c);
        for (inner, coefficient) in &def.t {
            let expanded = self.expand(*inner, watermark)?;
            result = result.comb(1, &expanded, *coefficient);
        }
        Some(result)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bnd {
    None,
    Interval,
    Rel,
    RelAll,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Split {
    None,
    All,
    Window(u64),
}

#[derive(Clone, Copy, Debug)]
struct Cfg {
    name: &'static str,
    bnd: Bnd,
    sat: bool,
    split: Split,
    latest_first: bool,
    backjump: bool,
    eager: bool,
    /// Whether the work can grow exponentially with the size.
    exponential: bool,
}

const fn cfg(name: &'static str, bnd: Bnd, sat: bool, split: Split) -> Cfg {
    Cfg { name, bnd, sat, split, latest_first: true, backjump: false, eager: false, exponential: matches!(split, Split::All) }
}

fn configs() -> Vec<Cfg> {
    vec![
        cfg("sel-late", Bnd::None, false, Split::All),
        Cfg { latest_first: false, ..cfg("sel-early", Bnd::None, false, Split::All) },
        Cfg { backjump: true, ..cfg("sel-backjump", Bnd::None, false, Split::All) },
        cfg("bnd-int", Bnd::Interval, false, Split::None),
        cfg("bnd-rel", Bnd::Rel, false, Split::None),
        Cfg { eager: true, ..cfg("bnd-rel-eager", Bnd::Rel, false, Split::None) },
        cfg("bnd-relall", Bnd::RelAll, false, Split::None),
        cfg("both-late", Bnd::Rel, false, Split::All),
        Cfg { latest_first: false, ..cfg("both-early", Bnd::Rel, false, Split::All) },
        cfg("sat", Bnd::Rel, true, Split::None),
        cfg("win2", Bnd::Rel, true, Split::Window(2)),
        cfg("win4", Bnd::Rel, true, Split::Window(4)),
        cfg("win16", Bnd::Rel, true, Split::Window(16)),
        cfg("win64", Bnd::Rel, true, Split::Window(64)),
        cfg("win16-nosat", Bnd::Rel, false, Split::Window(16)),
        cfg("win16-nobnd", Bnd::None, true, Split::Window(16)),
    ]
}

enum Res {
    Proved(u128),
    Unproved,
    Abort,
}

struct Engine<'a> {
    m: &'a Model,
    cfg: Cfg,
    st: Stats,
    bounds: Vec<Option<Rc<Vec<Lit>>>>,
    node_cap: u64,
}

impl<'a> Engine<'a> {
    fn new(m: &'a Model, cfg: Cfg, deadline: Instant, node_cap: u64) -> Self {
        let st = Stats { deadline: Some(deadline), ..Stats::default() };
        Engine { m, cfg, st, bounds: vec![None; m.joins.len()], node_cap }
    }

    /// The literals connected to `roots`, with the bounds of each open join
    /// whose join value is reached.
    fn assemble(&mut self, lits: &[TL], open: &[usize], roots: &[V]) -> Vec<TL> {
        let mut pool: Vec<TL> = lits.to_vec();
        let mut index: HashMap<V, Vec<usize>> = HashMap::new();
        for (position, lit) in pool.iter().enumerate() {
            for value in lit.lin.vars() {
                index.entry(value).or_default().push(position);
            }
        }
        let mut open_phi: HashMap<V, usize> = HashMap::new();
        if self.cfg.bnd != Bnd::None {
            for join in open {
                for phi in &self.m.joins[*join].phis {
                    open_phi.insert(phi.v, *join);
                }
            }
        }
        let mut reached: HashSet<V> = roots.iter().copied().collect();
        let mut work: Vec<V> = reached.iter().copied().collect();
        work.sort_unstable();
        let mut expanded: HashSet<usize> = HashSet::new();
        let mut included: Vec<bool> = vec![false; pool.len()];
        while let Some(value) = work.pop() {
            if let Some(join) = open_phi.get(&value).copied() {
                if expanded.insert(join) {
                    let bounds = self.bounds_of(join);
                    for bound in bounds.iter() {
                        let position = pool.len();
                        for inner in bound.lin.vars() {
                            index.entry(inner).or_default().push(position);
                        }
                        pool.push(TL { lin: bound.lin.clone(), eq: false, tag: 0 });
                        included.push(false);
                    }
                }
            }
            let positions = index.get(&value).cloned().unwrap_or_default();
            for position in positions {
                if !included[position] {
                    included[position] = true;
                    for inner in pool[position].lin.vars() {
                        if reached.insert(inner) {
                            work.push(inner);
                        }
                    }
                }
            }
        }
        pool.into_iter().zip(included).filter(|(_, keep)| *keep).map(|(lit, _)| lit).collect()
    }

    /// The bounds a join records, by the configuration's rule.
    fn bounds_of(&mut self, join: usize) -> Rc<Vec<Lit>> {
        if let Some(bounds) = &self.bounds[join] {
            return Rc::clone(bounds);
        }
        let m = self.m;
        let j = &m.joins[join];
        let mut candidates: Vec<(Lin, Kind)> = Vec::new();
        let mut known: HashSet<Vec<(V, i128)>> = HashSet::new();
        let mut offer = |form: Lin, kind: Kind, candidates: &mut Vec<(Lin, Kind)>| {
            let form = Lin { t: form.t, c: 0 };
            if !form.t.is_empty() && known.insert(form.t.clone()) {
                candidates.push((form, kind));
            }
        };
        match self.cfg.bnd {
            Bnd::None => {}
            Bnd::Interval => {
                for phi in &j.phis {
                    offer(Lin::v(phi.v), Kind::State, &mut candidates);
                    offer(Lin::v(phi.v).scale(-1), Kind::State, &mut candidates);
                }
            }
            Bnd::Rel | Bnd::RelAll => {
                let all = self.cfg.bnd == Bnd::RelAll;
                for phi in &j.phis {
                    for raw in &phi.raw {
                        if let Some(expr) = m.expand(*raw, j.watermark) {
                            let form = Lin::v(phi.v).sub(&expr);
                            offer(form.scale(-1), Kind::Trans, &mut candidates);
                            offer(form, Kind::Trans, &mut candidates);
                        }
                    }
                }
                let entries: HashMap<V, V> = j.phis.iter().filter_map(|phi| phi.entry.map(|entry| (entry, phi.v))).collect();
                let carried = |kind: Kind| matches!(kind, Kind::Path | Kind::Fact | Kind::State) || (all && kind == Kind::Trans);
                let mut pool: Vec<Lit> = j.entry_lits.iter().filter(|lit| carried(lit.kind)).cloned().collect();
                let mut owners: Vec<usize> = entries.keys().filter_map(|entry| m.phi_owner.get(entry).copied()).collect();
                owners.sort_unstable();
                owners.dedup();
                for owner in owners {
                    if j.entry_joins.contains(&owner) {
                        let bounds = self.bounds_of(owner);
                        pool.extend(bounds.iter().filter(|lit| carried(lit.kind)).cloned());
                    }
                }
                for lit in &pool {
                    if !lit.lin.vars().any(|value| entries.contains_key(&value)) {
                        continue;
                    }
                    let mut form = lit.lin.clone();
                    for (entry, value) in &entries {
                        form = form.replace(*entry, &Lin::v(*value));
                    }
                    if lit.eq {
                        offer(form.scale(-1), Kind::State, &mut candidates);
                    }
                    offer(form, Kind::State, &mut candidates);
                }
            }
        }
        let mut recorded = Vec::new();
        for (form, kind) in candidates {
            let mut highest: Option<i128> = None;
            let mut unbounded = false;
            for (index, arm) in j.arms.iter().enumerate() {
                let mut at_arm = form.clone();
                for phi in &j.phis {
                    at_arm = at_arm.replace(phi.v, &Lin::v(phi.raw[index]));
                }
                let mut lits: Vec<TL> = j
                    .entry_lits
                    .iter()
                    .chain(arm.lits.iter().filter(|lit| lit.kind != Kind::Phi))
                    .map(|lit| TL { lin: lit.lin.clone(), eq: lit.eq, tag: 0 })
                    .collect();
                let roots: Vec<V> = at_arm.vars().collect();
                let open: Vec<usize> = j.entry_joins.iter().chain(arm.inner.iter()).copied().collect();
                lits = self.assemble(&lits, &open, &roots);
                lits.push(TL { lin: Lin::v(TOP).sub(&at_arm), eq: false, tag: 0 });
                self.st.projections += 1;
                match theory(&lits, Some(TOP), &mut self.st) {
                    Out::Contra(_) => continue,
                    Out::Capacity => unbounded = true,
                    Out::Open(rest) => {
                        let mut best: Option<i128> = None;
                        for (lin, _) in rest {
                            let coefficient = lin.coeff(TOP);
                            if coefficient > 0 {
                                let bound = (-lin.c).div_euclid(coefficient);
                                best = Some(best.map_or(bound, |current: i128| current.min(bound)));
                            }
                        }
                        match best {
                            Some(bound) => highest = Some(highest.map_or(bound, |current| current.max(bound))),
                            None => unbounded = true,
                        }
                    }
                }
                if unbounded {
                    break;
                }
            }
            if let (Some(bound), false) = (highest, unbounded) {
                recorded.push(Lit { lin: form.addk(-bound), eq: false, kind });
            }
        }
        self.st.bound_facts += recorded.len() as u64;
        let recorded = Rc::new(recorded);
        self.bounds[join] = Some(Rc::clone(&recorded));
        recorded
    }

    /// The open joins that share a value with what `roots` reach through
    /// `lits`. With `through`, a join found this way makes its own values
    /// reached too, which gives every join the obligation can depend on.
    fn near(&self, lits: &[TL], open: &[usize], roots: &[V], through: bool) -> Vec<usize> {
        let mut index: HashMap<V, Vec<usize>> = HashMap::new();
        for (position, lit) in lits.iter().enumerate() {
            for value in lit.lin.vars() {
                index.entry(value).or_default().push(position);
            }
        }
        let open_set: HashSet<usize> = open.iter().copied().collect();
        let mut reached: HashSet<V> = roots.iter().copied().collect();
        let mut work: Vec<V> = reached.iter().copied().collect();
        let mut seen_lit = vec![false; lits.len()];
        let mut found: HashSet<usize> = HashSet::new();
        while let Some(value) = work.pop() {
            if let Some(positions) = index.get(&value) {
                for position in positions {
                    if !seen_lit[*position] {
                        seen_lit[*position] = true;
                        for inner in lits[*position].lin.vars() {
                            if reached.insert(inner) {
                                work.push(inner);
                            }
                        }
                    }
                }
            }
            if let Some(joins) = self.m.value_joins.get(&value) {
                for join in joins {
                    if open_set.contains(join) && found.insert(*join) && through {
                        for inner in &self.m.joins[*join].values {
                            if reached.insert(*inner) {
                                work.push(*inner);
                            }
                        }
                    }
                }
            }
        }
        let mut found: Vec<usize> = found.into_iter().collect();
        found.sort_unstable();
        if self.cfg.latest_first {
            found.reverse();
        }
        found
    }

    fn decide(&self, lits: &[TL], open: &[usize], roots: &[V], join: usize, arm: usize, tag: u128) -> (Vec<TL>, Vec<usize>, Vec<V>) {
        let chosen = &self.m.joins[join].arms[arm];
        let mut lits = lits.to_vec();
        let mut roots = roots.to_vec();
        for lit in &chosen.lits {
            roots.extend(lit.lin.vars());
            lits.push(TL { lin: lit.lin.clone(), eq: lit.eq, tag });
        }
        let mut open: Vec<usize> = open.iter().copied().filter(|other| *other != join).collect();
        open.extend(chosen.inner.iter().copied());
        (lits, open, roots)
    }

    fn contradictory(&mut self, lits: &[TL], open: &[usize], roots: &[V]) -> Option<Result<u128, ()>> {
        let problem = self.assemble(lits, open, roots);
        match theory(&problem, None, &mut self.st) {
            Out::Contra(tags) => Some(Ok(tags)),
            Out::Open(_) => None,
            Out::Capacity => Some(Err(())),
        }
    }

    fn search(&mut self, mut lits: Vec<TL>, mut open: Vec<usize>, mut roots: Vec<V>, budget: u64, depth: u32) -> Res {
        self.st.nodes += 1;
        if self.st.nodes > self.node_cap {
            self.st.node_cap_hit = true;
            return Res::Abort;
        }
        match self.contradictory(&lits, &open, &roots) {
            Some(Ok(tags)) => {
                self.st.leaves += 1;
                return Res::Proved(tags);
            }
            Some(Err(())) => return Res::Abort,
            None => {}
        }
        if self.cfg.sat {
            loop {
                let near = self.near(&lits, &open, &roots, false);
                let mut changed = false;
                for join in near {
                    let arms = self.m.joins[join].arms.len();
                    let mut alive = Vec::new();
                    for arm in 0..arms {
                        let (lits2, open2, roots2) = self.decide(&lits, &open, &roots, join, arm, 0);
                        self.st.probes += 1;
                        match self.contradictory(&lits2, &open2, &roots2) {
                            Some(Ok(_)) => {}
                            Some(Err(())) => return Res::Abort,
                            None => alive.push((lits2, open2, roots2)),
                        }
                    }
                    if alive.is_empty() {
                        self.st.leaves += 1;
                        return Res::Proved(0);
                    }
                    if alive.len() == 1 {
                        let (lits2, open2, roots2) = alive.pop().expect("one arm");
                        lits = lits2;
                        open = open2;
                        roots = roots2;
                        self.st.forced += 1;
                        changed = true;
                        break;
                    }
                }
                if !changed {
                    break;
                }
            }
        }
        let pick = match self.cfg.split {
            Split::None => None,
            Split::All => self.near(&lits, &open, &roots, true).into_iter().next(),
            Split::Window(_) => self
                .near(&lits, &open, &roots, false)
                .into_iter()
                .find(|join| self.m.joins[*join].arms.len() as u64 <= budget),
        };
        let Some(join) = pick else {
            self.st.leaves += 1;
            return Res::Unproved;
        };
        let arms = self.m.joins[join].arms.len();
        let bit = if depth < 128 { 1u128 << depth } else { 0 };
        let mut rests = 0u128;
        for arm in 0..arms {
            let (lits2, open2, roots2) = self.decide(&lits, &open, &roots, join, arm, bit);
            match self.search(lits2, open2, roots2, budget / arms as u64, depth + 1) {
                Res::Proved(tags) => {
                    if self.cfg.backjump && bit != 0 && tags & bit == 0 {
                        return Res::Proved(tags);
                    }
                    rests |= tags & !bit;
                }
                other => return other,
            }
        }
        Res::Proved(rests)
    }

    /// Returns the verdict and the number of joins the obligation can depend
    /// on at the root.
    fn prove(&mut self, oblig: &Oblig) -> (&'static str, usize) {
        let mut lits: Vec<TL> = oblig.gamma.iter().map(|lit| TL { lin: lit.lin.clone(), eq: lit.eq, tag: 0 }).collect();
        lits.push(TL { lin: oblig.goal.scale(-1).addk(1), eq: false, tag: 0 });
        let mut roots: Vec<V> = oblig.goal.vars().collect();
        for lit in &oblig.gamma {
            if lit.kind == Kind::Path {
                roots.extend(lit.lin.vars());
            }
        }
        let cone = self.near(&lits, &oblig.joins, &roots, true).len();
        let budget = match self.cfg.split {
            Split::Window(product) => product,
            _ => u64::MAX,
        };
        let verdict = match self.search(lits, oblig.joins.clone(), roots, budget, 0) {
            Res::Proved(_) => "proved",
            Res::Unproved => "unproved",
            Res::Abort => "abort",
        };
        (verdict, cone)
    }
}

struct Outcome {
    verdicts: Vec<(String, bool, &'static str, usize, Stats)>,
    total: Stats,
    facts_last: usize,
    facts_max: usize,
    nanos: u128,
}

fn run(model: &Model, cfg: Cfg, limit: Duration, node_cap: u64) -> Outcome {
    let start = Instant::now();
    let mut engine = Engine::new(model, cfg, start + limit, node_cap);
    if cfg.eager {
        for join in 0..model.joins.len() {
            engine.bounds_of(join);
        }
    }
    let mut verdicts = Vec::new();
    for oblig in &model.obligs {
        let before = engine.st.clone();
        let (verdict, cone) = engine.prove(oblig);
        verdicts.push((oblig.name.clone(), oblig.expect, verdict, cone, engine.st.minus(&before)));
    }
    let nanos = start.elapsed().as_nanos();
    let sizes: Vec<usize> = engine.bounds.iter().map(|bounds| bounds.as_ref().map_or(0, |b| b.len())).collect();
    Outcome {
        verdicts,
        total: engine.st.clone(),
        facts_last: sizes.last().copied().unwrap_or(0),
        facts_max: sizes.iter().copied().max().unwrap_or(0),
        nanos,
    }
}

/// Decides each obligation by running the program on every value of every
/// unknown. `None` when an unknown has no finite range or the runs are too
/// many.
fn truth(program: &[Stmt]) -> Option<HashMap<String, bool>> {
    fn collect(stmts: &[Stmt], fresh: &mut Vec<(V, i128, i128)>, ok: &mut bool) {
        for stmt in stmts {
            match stmt {
                Stmt::Fresh(value, Some(low), Some(high)) => fresh.push((*value, *low, *high)),
                Stmt::Fresh(..) => *ok = false,
                Stmt::Branch(arms, _) => {
                    for (_, body) in arms {
                        collect(body, fresh, ok);
                    }
                }
                _ => {}
            }
        }
    }
    fn eval(lin: &Lin, env: &HashMap<V, i128>) -> i128 {
        lin.t.iter().map(|(value, coefficient)| coefficient * env[value]).sum::<i128>() + lin.c
    }
    fn exec(stmts: &[Stmt], env: &mut HashMap<V, i128>, result: &mut HashMap<String, bool>) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Fresh(..) => {}
                Stmt::Let(value, expr) => {
                    let got = eval(expr, env);
                    env.insert(*value, got);
                }
                Stmt::Assume(lin, eq) => {
                    let got = eval(lin, env);
                    if (*eq && got != 0) || (!*eq && got > 0) {
                        return false;
                    }
                }
                Stmt::Assert(name, goal) | Stmt::Query(name, goal) => {
                    let holds = eval(goal, env) <= 0;
                    let slot = result.entry(name.clone()).or_insert(true);
                    *slot = *slot && holds;
                }
                Stmt::Branch(arms, phis) => {
                    let taken = arms.iter().position(|(conds, _)| conds.iter().all(|cond| eval(cond, env) <= 0));
                    let Some(taken) = taken else { return false };
                    if !exec(&arms[taken].1, env, result) {
                        return false;
                    }
                    for (value, raw, _) in phis {
                        let got = env[&raw[taken]];
                        env.insert(*value, got);
                    }
                }
            }
        }
        true
    }
    let mut fresh = Vec::new();
    let mut ok = true;
    collect(program, &mut fresh, &mut ok);
    if !ok {
        return None;
    }
    let mut runs: u128 = 1;
    for (_, low, high) in &fresh {
        runs = runs.saturating_mul((high - low + 1) as u128);
    }
    if runs > 4_000_000 {
        return None;
    }
    let mut result: HashMap<String, bool> = HashMap::new();
    let mut current: Vec<i128> = fresh.iter().map(|(_, low, _)| *low).collect();
    loop {
        let mut env: HashMap<V, i128> = fresh.iter().zip(&current).map(|((value, _, _), got)| (*value, *got)).collect();
        exec(program, &mut env, &mut result);
        let mut position = 0;
        loop {
            if position == fresh.len() {
                return Some(result);
            }
            if current[position] < fresh[position].2 {
                current[position] += 1;
                break;
            }
            current[position] = fresh[position].1;
            position += 1;
        }
    }
}

const HEADER: &str = "row,family,size,config,obligation,expect,verdict,cone_joins,theory_calls,fm_pairs,fm_elims,probes,forced,nodes,leaves,projections,bound_facts,facts_last_join,facts_max_join,time_ns,note";

fn emit(row: &str, family: &str, size: &str, config: &str, name: &str, expect: &str, verdict: &str, cone: usize, st: &Stats, facts: (usize, usize), nanos: u128) {
    let mut note = String::new();
    if st.capacity {
        note.push_str("capacity ");
    }
    if st.timeout {
        note.push_str("time-guard ");
    }
    if st.node_cap_hit {
        note.push_str("node-cap ");
    }
    println!(
        "{row},{family},{size},{config},{name},{expect},{verdict},{cone},{},{},{},{},{},{},{},{},{},{},{},{nanos},{}",
        st.theory_calls,
        st.fm_pairs,
        st.fm_elims,
        st.probes,
        st.forced,
        st.nodes,
        st.leaves,
        st.projections,
        st.bound_facts,
        facts.0,
        facts.1,
        note.trim()
    );
}

fn measure() {
    println!("{HEADER}");
    let limit = Duration::from_secs(40);
    let node_cap = 1 << 18;
    for family in families::all() {
        let mut stopped: HashSet<&'static str> = HashSet::new();
        for size in &family.sizes {
            let program = (family.make)(size, false);
            let model = Model::build(&program);
            let label = size.label();
            for cfg in configs() {
                if stopped.contains(cfg.name) {
                    continue;
                }
                if cfg.exponential && size.large() {
                    continue;
                }
                if cfg.bnd == Bnd::RelAll && size.k > 32 {
                    continue;
                }
                let first = run(&model, cfg, limit, node_cap);
                let mut nanos = first.nanos;
                if first.nanos < 300_000_000 {
                    for _ in 0..4 {
                        nanos = nanos.min(run(&model, cfg, limit, node_cap).nanos);
                    }
                }
                let facts = (first.facts_last, first.facts_max);
                for (name, expect, verdict, cone, st) in &first.verdicts {
                    let keep = name == "final" || name.ends_with("last") || family.name == "BINSEARCH";
                    if keep {
                        emit("oblig", family.name, &label, cfg.name, name, if *expect { "holds" } else { "open" }, verdict, *cone, st, facts, 0);
                    }
                }
                let asserts = first.verdicts.iter().filter(|(_, expect, ..)| *expect);
                let all = if first.verdicts.iter().any(|(.., verdict, _, _)| *verdict == "abort") {
                    "abort"
                } else if asserts.clone().all(|(.., verdict, _, _)| *verdict == "proved") {
                    "proved"
                } else {
                    "unproved"
                };
                let queries_open = first.verdicts.iter().filter(|(_, expect, ..)| !*expect).all(|(.., verdict, _, _)| *verdict == "unproved");
                let expect = if queries_open { "queries-open" } else { "QUERY-DECIDED" };
                emit("total", family.name, &label, cfg.name, "*", expect, all, 0, &first.total, facts, nanos);
                if first.total.node_cap_hit || first.total.timeout {
                    stopped.insert(cfg.name);
                }
            }
        }
    }
}

fn check() {
    println!("row,family,size,config,obligation,truth,verdict,finding");
    let mut bad = 0;
    let mut compared = 0;
    for family in families::all() {
        for size in &family.tiny {
            let program = (family.make)(size, true);
            let Some(truth) = truth(&program) else {
                println!("check,{},{},-,-,-,-,NOT-ENUMERATED", family.name, size.label());
                bad += 1;
                continue;
            };
            let model = Model::build(&program);
            for cfg in configs() {
                let outcome = run(&model, cfg, Duration::from_secs(60), 1 << 22);
                for (name, _, verdict, _, _) in &outcome.verdicts {
                    let holds = truth[name];
                    let finding = if *verdict == "proved" && !holds {
                        bad += 1;
                        "UNSOUND"
                    } else if cfg.name.starts_with("sel-") && (*verdict == "proved") != holds {
                        bad += 1;
                        "SEL-INEXACT"
                    } else {
                        ""
                    };
                    compared += 1;
                    println!("check,{},{},{},{name},{holds},{verdict},{finding}", family.name, size.label(), cfg.name);
                }
            }
        }
    }
    println!("summary,compared,{compared},findings,{bad}");
    if bad != 0 {
        std::process::exit(1);
    }
}

fn dense() {
    println!("row,n,inequalities,seed,result,fm_pairs,fm_elims,peak_inequalities,time_ns");
    for n in 4..=14u32 {
        for seed in 1..=3u64 {
            let lits = families::dense(n, seed);
            let mut st = Stats { deadline: Some(Instant::now() + Duration::from_secs(60)), ..Stats::default() };
            let start = Instant::now();
            let result = match theory(&lits, None, &mut st) {
                Out::Contra(_) => "contradictory",
                Out::Open(_) => "feasible",
                Out::Capacity => {
                    if st.timeout {
                        "time-guard"
                    } else {
                        "capacity"
                    }
                }
            };
            println!("dense,{n},{},{seed},{result},{},{},{},{}", lits.len(), st.fm_pairs, st.fm_elims, st.peak, start.elapsed().as_nanos());
        }
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let body = move || match mode.as_str() {
        "measure" => measure(),
        "check" => check(),
        "dense" => dense(),
        _ => {
            eprintln!("usage: join-derivation-model measure|check|dense");
            std::process::exit(2);
        }
    };
    let worker = std::thread::Builder::new().stack_size(1 << 30).spawn(body).expect("worker thread");
    if worker.join().is_err() {
        std::process::exit(101);
    }
}
