//! Experiment E1 (`research/investigations/derivation-ceiling`): the shadow
//! derivation's configuration and its counters. Instrument code of an
//! unmerged experiment branch.
//!
//! `WHITEFOOT_E1=<tsv>` (or [`open`]) makes the range walk run on every
//! function in shadow, once per configuration, posing every obligation; the
//! shadow's answers and counts go only to the file. `WHITEFOOT_E1_TIMING=1`
//! runs one shadow walk per function, in the configuration
//! `WHITEFOOT_E1_JOIN` names (`selector` or `weakest`), with no counters, and
//! prints the phase totals at exit through [`timing_totals`]. With the file
//! open and [`set_off`] on, no shadow walk runs and the extra interval
//! capture stays off: the rows then carry today's ordinary answers alone, for
//! comparison with the rows of a run with the shadow on.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use super::solver::{AtomId, E1Measure, Literal};

/// The configurations of the shadow derivation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Config {
    /// Today's walk: operand intervals as ordinary S7 recorded them.
    Baseline,
    /// Operand intervals by the derivation's projection, and each
    /// nonlinear row's result range on its result.
    Remedy,
    /// `Remedy` and, at each join, the weakest common bound.
    RemedyJoin,
    /// No shadow walk: the ordinary answers alone.
    Off,
}

impl Config {
    const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Remedy => "remedy",
            Self::RemedyJoin => "remedy+join",
            Self::Off => "off",
        }
    }
}

static OUTPUT: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();
static OPENED: AtomicBool = AtomicBool::new(false);
static LABEL: Mutex<String> = Mutex::new(String::new());
static OFF: AtomicBool = AtomicBool::new(false);

/// Nanoseconds spent, for the timing mode's totals.
static ORDINARY_NANOS: AtomicU64 = AtomicU64::new(0);
static WALK_NANOS: AtomicU64 = AtomicU64::new(0);
static SHADOW_NANOS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static CONFIG: Cell<Option<Config>> = const { Cell::new(None) };
    static CURRENT: RefCell<Option<(Key, BTreeSet<AtomId>)>> = const { RefCell::new(None) };
    static ACC: RefCell<BTreeMap<Key, Acc>> = const { RefCell::new(BTreeMap::new()) };
}

fn output() -> &'static Mutex<Option<std::fs::File>> {
    OUTPUT.get_or_init(|| {
        let file = std::env::var_os("WHITEFOOT_E1").and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        });
        if file.is_some() {
            OPENED.store(true, Ordering::Relaxed);
        }
        Mutex::new(file)
    })
}

/// Directs the counters to `path`, as `WHITEFOOT_E1` does.
pub fn open(path: &std::path::Path) {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("E1 counter file opens");
    let mut slot = output().lock().expect("E1 output lock");
    *slot = Some(file);
    OPENED.store(true, Ordering::Relaxed);
}

/// The case or program the following rows belong to.
pub fn label(case: &str) {
    case.clone_into(&mut LABEL.lock().expect("E1 label lock"));
}

/// Whether the counters are written.
pub(crate) fn data() -> bool {
    let _ = output();
    OPENED.load(Ordering::Relaxed)
}

/// Turns the shadow off while the counters are written: rows then carry the
/// ordinary answers alone.
pub fn set_off(off: bool) {
    OFF.store(off, Ordering::Relaxed);
}

/// Whether ordinary entailment captures S7 operand intervals in every
/// function, for the shadow walk, rather than in range participants alone.
pub(crate) fn captures() -> bool {
    data() && !OFF.load(Ordering::Relaxed)
}

/// Whether the timing mode runs one uncounted shadow walk per function.
pub(crate) fn timing() -> bool {
    static TIMING: OnceLock<bool> = OnceLock::new();
    *TIMING.get_or_init(|| std::env::var_os("WHITEFOOT_E1_TIMING").is_some_and(|v| v == "1"))
}

/// The configurations each function's shadow walk runs in.
pub(crate) fn configs() -> Vec<Config> {
    if data() && OFF.load(Ordering::Relaxed) {
        vec![Config::Off]
    } else if data() {
        vec![Config::Baseline, Config::Remedy, Config::RemedyJoin]
    } else if timing() {
        match std::env::var("WHITEFOOT_E1_JOIN").as_deref() {
            Ok("weakest") => vec![Config::RemedyJoin],
            _ => vec![Config::Remedy],
        }
    } else {
        Vec::new()
    }
}

/// The configuration of the shadow walk running on this thread, if any.
pub(crate) fn config() -> Option<Config> {
    CONFIG.with(Cell::get)
}

/// Whether derivation intervals and result ranges are used.
pub(crate) fn remedy() -> bool {
    matches!(config(), Some(Config::Remedy | Config::RemedyJoin))
}

/// Whether the weakest common bound is added at joins.
pub(crate) fn weakest_join() -> bool {
    config() == Some(Config::RemedyJoin)
}

/// Whether the next judgment on this thread is counted.
pub(crate) fn measuring() -> bool {
    data() && config().is_some() && CURRENT.with(|current| current.borrow().is_some())
}

/// What a counted query belongs to.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Key {
    /// Obligation records, by index, posed at one site.
    Records(Vec<usize>),
    /// A range clause owed at a site.
    Clause(String),
    /// An [OP-5] condition record, asked whether it is decided `truth`.
    Condition(usize, bool),
}

/// One obligation's accumulated counts.
#[derive(Clone, Debug, Default)]
pub(crate) struct Acc {
    pub(crate) queries: u64,
    pub(crate) refuted: u64,
    pub(crate) atoms: usize,
    pub(crate) instances: usize,
    pub(crate) form_nanos: u128,
    pub(crate) judge_nanos: u128,
    pub(crate) n1: usize,
    pub(crate) n2: usize,
    pub(crate) n3: usize,
    pub(crate) max_cases: usize,
    pub(crate) product: u128,
    pub(crate) root_refuted: u64,
    pub(crate) backjump_nodes: usize,
    pub(crate) max_depth: usize,
    pub(crate) full_nodes: usize,
    pub(crate) full_capped: bool,
    pub(crate) full_nanos: u128,
    pub(crate) notes: BTreeSet<&'static str>,
}

impl Acc {
    /// Both queries of one record, as one row's counts.
    pub(crate) fn merged(&self, other: &Self) -> Self {
        let mut notes = self.notes.clone();
        notes.extend(other.notes.iter().copied());
        Self {
            queries: self.queries + other.queries,
            refuted: self.refuted + other.refuted,
            atoms: self.atoms.max(other.atoms),
            instances: self.instances.max(other.instances),
            form_nanos: self.form_nanos + other.form_nanos,
            judge_nanos: self.judge_nanos + other.judge_nanos,
            n1: self.n1.max(other.n1),
            n2: self.n2.max(other.n2),
            n3: self.n3.max(other.n3),
            max_cases: self.max_cases.max(other.max_cases),
            product: self.product.max(other.product),
            root_refuted: self.root_refuted + other.root_refuted,
            backjump_nodes: self.backjump_nodes + other.backjump_nodes,
            max_depth: self.max_depth.max(other.max_depth),
            full_nodes: self.full_nodes + other.full_nodes,
            full_capped: self.full_capped || other.full_capped,
            full_nanos: self.full_nanos + other.full_nanos,
            notes,
        }
    }
}

/// The obligation the next judgment counts toward, until the guard drops.
pub(crate) struct Current(bool);

pub(crate) fn current(key: Key, goal: &[Literal]) -> Current {
    if !data() || config().is_none() {
        return Current(false);
    }
    let mut atoms = BTreeSet::new();
    for literal in goal {
        atoms.extend(literal.left.terms.iter().map(|(atom, _)| *atom));
        atoms.extend(literal.right.terms.iter().map(|(atom, _)| *atom));
    }
    CURRENT.with(|current| *current.borrow_mut() = Some((key, atoms)));
    Current(true)
}

impl Drop for Current {
    fn drop(&mut self) {
        if self.0 {
            CURRENT.with(|current| *current.borrow_mut() = None);
        }
    }
}

/// The goal's atoms of the counted judgment, as world atoms.
pub(crate) fn goal_atoms() -> BTreeSet<AtomId> {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .map(|(_, atoms)| atoms.clone())
            .unwrap_or_default()
    })
}

/// Records one counted judgment of the current obligation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn judged(
    atoms: usize,
    instances: usize,
    form_nanos: u128,
    judge_nanos: u128,
    refuted: bool,
    measure: &E1Measure,
) {
    let Some(key) = CURRENT.with(|current| current.borrow().as_ref().map(|(key, _)| key.clone()))
    else {
        return;
    };
    ACC.with(|acc| {
        let mut acc = acc.borrow_mut();
        let entry = acc.entry(key).or_default();
        entry.queries += 1;
        entry.refuted += u64::from(refuted);
        entry.atoms = entry.atoms.max(atoms);
        entry.instances = entry.instances.max(instances);
        entry.form_nanos += form_nanos;
        entry.judge_nanos += judge_nanos;
        entry.n1 = entry.n1.max(measure.n1);
        entry.n2 = entry.n2.max(measure.n2);
        entry.n3 = entry.n3.max(measure.n3);
        entry.max_cases = entry.max_cases.max(measure.max_cases);
        entry.product = entry.product.max(measure.product);
        entry.root_refuted += u64::from(measure.refuted_at_root);
        entry.backjump_nodes += measure.backjump_nodes;
        entry.max_depth = entry.max_depth.max(measure.max_depth);
        entry.full_nodes += measure.full_nodes.unwrap_or_default();
        entry.full_capped |= measure.full_capped;
        entry.full_nanos += measure.full_nanos;
    });
}

/// Records a reason an obligation's shadow answer is not a plain verdict.
pub(crate) fn note(key: Key, reason: &'static str) {
    if !data() || config().is_none() {
        return;
    }
    ACC.with(|acc| {
        acc.borrow_mut().entry(key).or_default().notes.insert(reason);
    });
}

/// Runs `walk` as the shadow walk of `config`, returning what it counted.
pub(crate) fn shadow<T>(config: Config, walk: impl FnOnce() -> T) -> (T, BTreeMap<Key, Acc>, u128) {
    CONFIG.with(|cell| cell.set(Some(config)));
    ACC.with(|acc| acc.borrow_mut().clear());
    let start = Instant::now();
    let value = walk();
    let nanos = start.elapsed().as_nanos();
    CONFIG.with(|cell| cell.set(None));
    let counted = ACC.with(|acc| std::mem::take(&mut *acc.borrow_mut()));
    SHADOW_NANOS.fetch_add(u64::try_from(nanos).unwrap_or(u64::MAX), Ordering::Relaxed);
    (value, counted, nanos)
}

/// Adds the time of one ordinary entailment analysis as it drops.
/// Only the outermost analysis on a thread counts, so a nested analysis is
/// not counted twice.
pub(crate) struct OrdinaryTimer(Option<Instant>);

thread_local! {
    static ORDINARY_DEPTH: Cell<usize> = const { Cell::new(0) };
}

impl OrdinaryTimer {
    pub(crate) fn start() -> Self {
        let depth = ORDINARY_DEPTH.with(|depth| {
            let value = depth.get();
            depth.set(value + 1);
            value
        });
        Self((depth == 0).then(Instant::now))
    }
}

impl Drop for OrdinaryTimer {
    fn drop(&mut self) {
        ORDINARY_DEPTH.with(|depth| depth.set(depth.get() - 1));
        if let Some(start) = self.0 {
            ordinary_time(start.elapsed().as_nanos());
        }
    }
}

/// Adds the time one function's ordinary entailment took.
pub(crate) fn ordinary_time(nanos: u128) {
    ORDINARY_NANOS.fetch_add(u64::try_from(nanos).unwrap_or(u64::MAX), Ordering::Relaxed);
}

/// Adds the time one function's ordinary range walk took.
pub(crate) fn walk_time(nanos: u128) {
    WALK_NANOS.fetch_add(u64::try_from(nanos).unwrap_or(u64::MAX), Ordering::Relaxed);
}

/// The totals of the ordinary entailment, the ordinary range walk and the
/// shadow walks, in nanoseconds.
pub fn timing_totals() -> (u64, u64, u64) {
    (
        ORDINARY_NANOS.load(Ordering::Relaxed),
        WALK_NANOS.load(Ordering::Relaxed),
        SHADOW_NANOS.load(Ordering::Relaxed),
    )
}

/// One row of the counter file.
pub(crate) struct Row<'a> {
    pub(crate) config: Config,
    pub(crate) scope: &'static str,
    pub(crate) prelude: bool,
    pub(crate) function: &'a str,
    pub(crate) record: String,
    pub(crate) rule: String,
    pub(crate) kind: String,
    pub(crate) site: String,
    pub(crate) ordinary: &'static str,
    pub(crate) route: String,
    pub(crate) shadow: String,
    pub(crate) acc: Option<&'a Acc>,
    pub(crate) walk_nanos: u128,
}

/// The header of the counter file's rows.
pub const HEADER: &str = "label\tconfig\tscope\tprelude\tfunction\trecord\trule\tkind\tsite\tordinary\troute\tshadow\tqueries\trefuted\troot_refuted\tatoms\tinstances\tn1\tn2\tn3\tmax_cases\tproduct\tbackjump_nodes\tmax_depth\tfull_nodes\tfull_capped\tform_ns\tjudge_ns\tfull_ns\tnotes\twalk_ns\tbatch";

pub(crate) fn write(rows: &[Row<'_>]) {
    if rows.is_empty() || !data() {
        return;
    }
    let label = LABEL.lock().expect("E1 label lock").clone();
    static BATCH: AtomicU64 = AtomicU64::new(0);
    let batch = BATCH.fetch_add(1, Ordering::Relaxed);
    let mut text = String::new();
    for row in rows {
        let clean = |value: &str| value.replace(['\t', '\n'], " ");
        let default = Acc::default();
        let acc = row.acc.unwrap_or(&default);
        let notes = acc.notes.iter().copied().collect::<Vec<_>>().join(",");
        text.push_str(&format!(
            "{label}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{batch}\n",
            row.config.name(),
            row.scope,
            u8::from(row.prelude),
            clean(row.function),
            row.record,
            clean(&row.rule),
            clean(&row.kind),
            clean(&row.site),
            row.ordinary,
            row.route,
            row.shadow,
            acc.queries,
            acc.refuted,
            acc.root_refuted,
            acc.atoms,
            acc.instances,
            acc.n1,
            acc.n2,
            acc.n3,
            acc.max_cases,
            acc.product,
            acc.backjump_nodes,
            acc.max_depth,
            acc.full_nodes,
            u8::from(acc.full_capped),
            acc.form_nanos,
            acc.judge_nanos,
            acc.full_nanos,
            notes,
            row.walk_nanos,
        ));
    }
    let mut slot = output().lock().expect("E1 output lock");
    if let Some(file) = slot.as_mut() {
        let _ = file.write_all(text.as_bytes());
    }
}
