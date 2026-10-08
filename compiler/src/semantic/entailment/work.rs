//! Temporary checker work attribution for the many-arm join cost, outside
//! the acceptance and derivation state. Remove it after the join-cost fix
//! unless it is kept as a maintained diagnostic.
//!
//! `WHITEFOOT_CHECK_WORK=<scratch TSV>` appends these counters through the
//! ordinary semantic path, including `whitefootc --check`. The variable is
//! read once per process; unset means no file. Output errors are reported on
//! stderr and do not change the verdict. Use a separate path per process when
//! measuring.
//!
//! The headerless TSV columns are process ID, analysis-run ID, function name,
//! kind, ordinal, metric, unsigned value. Each function analysis invocation
//! (including candidate reanalysis and synthetic contract implication) has a
//! distinct run ID. Cached functions that skip analysis emit nothing. Rows are
//! buffered until the invocation finishes; interrupted analyses have no rows.
//! Counters saturate at `u64::MAX`; that value makes a measurement unusable.
//!
//! - `function`: `joins` counts `join_at` calls; `join_passes` counts their
//!   full, ordinary-fallback and expanded-row passes; `intern_calls` counts
//!   ledger interning attempts before deduplication; `closure_cache_hits`
//!   counts remembered closed-view reuse.
//! - `join`: ordinal identifies a pass; `join` identifies its enclosing
//!   `join_at` call. `inputs` includes contradictory predecessors;
//!   `union_rows` counts contributing closure rows plus requested extra rows
//!   (zero for an all-contradictory join); `pairs_evaluated` counts ordered L0
//!   pairs visited, including diagonals; `pairs_retained` counts stored L0
//!   cells, including reused proofs. `parent_references` counts constructed
//!   JoinParent entries submitted to interning, including deduplicated nodes;
//!   `intern_calls` includes the pass's input closures and proof preparation.
//!   Fallback merging outside a pass is included in the function total.
//! - `snapshot`, `closure`, `probe`: `count`, `rows_sum`, `rows_max`,
//!   `cells_sum`, `cells_max` describe materialization inputs, computed closed
//!   views (excluding cache hits), and proof-free contradiction probes,
//!   respectively. Rows are dense matrix dimensions; cells are occupied L0
//!   cells for snapshots/closures and allocated matrix cells for probes.
//!   Snapshots count each materialization call, not numeric snapshot copies.
//!
//! These counters observe work; they never select acceptance or a derivation.
//! Measure elapsed time separately with counters unset, and compare paired
//! instrumented runs to quantify observation overhead.

use std::cell::RefCell;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static OUTPUT: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
static NEXT_RUN: AtomicU64 = AtomicU64::new(0);
/// Set once the output opens, so that a hook costs one load when disabled.
static ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static ACTIVE: RefCell<Option<FunctionWork>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Sizes {
    count: u64,
    rows: u64,
    max_rows: u64,
    cells: u64,
    max_cells: u64,
}

impl Sizes {
    fn add(&mut self, rows: usize, cells: usize) {
        self.count = self.count.saturating_add(1);
        self.rows = self.rows.saturating_add(rows as u64);
        self.max_rows = self.max_rows.max(rows as u64);
        self.cells = self.cells.saturating_add(cells as u64);
        self.max_cells = self.max_cells.max(cells as u64);
    }
}

#[derive(Default)]
struct JoinWork {
    join: u64,
    inputs: usize,
    union_rows: usize,
    pairs_evaluated: usize,
    pairs_retained: usize,
    parent_references: u64,
    intern_calls: u64,
}

struct FunctionWork {
    run: u64,
    name: String,
    joins: u64,
    passes: Vec<JoinWork>,
    active_pass: Option<usize>,
    snapshots: Sizes,
    closures: Sizes,
    probes: Sizes,
    closure_cache_hits: u64,
    intern_calls: u64,
}

fn update(f: impl FnOnce(&mut FunctionWork)) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    ACTIVE.with(|active| {
        if let Some(work) = active.borrow_mut().as_mut() {
            f(work);
        }
    });
}

/// One entailment analysis invocation, including speculative candidate runs.
pub(super) struct FunctionScope(Option<FunctionWork>, bool);

pub(super) fn function(name: &str) -> FunctionScope {
    let output = OUTPUT.get_or_init(|| {
        std::env::var_os("WHITEFOOT_CHECK_WORK").and_then(|path| {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                Ok(file) => Some(Mutex::new(file)),
                Err(error) => {
                    eprintln!("checker work output unavailable: {error}");
                    None
                }
            }
        })
    });
    if output.is_none() {
        return FunctionScope(None, false);
    }
    ENABLED.store(true, Ordering::Relaxed);
    let previous = ACTIVE.with(|active| {
        active.replace(Some(FunctionWork {
            run: NEXT_RUN.fetch_add(1, Ordering::Relaxed),
            name: name.replace(['\t', '\r', '\n'], " "),
            joins: 0,
            passes: Vec::new(),
            active_pass: None,
            snapshots: Sizes::default(),
            closures: Sizes::default(),
            probes: Sizes::default(),
            closure_cache_hits: 0,
            intern_calls: 0,
        }))
    });
    FunctionScope(previous, true)
}

impl Drop for FunctionScope {
    fn drop(&mut self) {
        if !self.1 {
            return;
        }
        let work = ACTIVE.with(|active| active.replace(self.0.take()));
        if let Some(work) = work
            && !std::thread::panicking()
        {
            work.write();
        }
    }
}

pub(super) fn join() {
    update(|work| work.joins = work.joins.saturating_add(1));
}

/// Each full, ordinary-fallback or expanded-row pass has its own row width.
pub(super) struct JoinPass(Option<usize>);

pub(super) fn join_pass(inputs: usize) -> JoinPass {
    let mut previous = None;
    update(|work| {
        previous = work.active_pass.replace(work.passes.len());
        work.passes.push(JoinWork {
            join: work.joins,
            inputs,
            ..JoinWork::default()
        });
    });
    JoinPass(previous)
}

impl Drop for JoinPass {
    fn drop(&mut self) {
        update(|work| work.active_pass = self.0);
    }
}

pub(super) fn join_pairs(rows: usize, evaluated: usize, retained: usize) {
    update(|work| {
        if let Some(index) = work.active_pass {
            let pass = &mut work.passes[index];
            pass.union_rows = rows;
            pass.pairs_evaluated = evaluated;
            pass.pairs_retained = retained;
        }
    });
}

/// Count calls before deduplication; count JoinParent entries without walking
/// parent vectors again. Closure/proof preparation calls belong to the pass.
pub(super) fn intern(node: &super::state::DerivationNode) {
    update(|work| {
        work.intern_calls = work.intern_calls.saturating_add(1);
        if let Some(index) = work.active_pass {
            let pass = &mut work.passes[index];
            pass.intern_calls = pass.intern_calls.saturating_add(1);
            use super::state::DerivationNode;
            let parents = match node {
                DerivationNode::JoinBound { parents, .. }
                | DerivationNode::JoinDistinct { parents, .. }
                | DerivationNode::JoinGoal { parents, .. }
                | DerivationNode::JoinContradiction { parents, .. } => parents.len(),
                _ => 0,
            };
            pass.parent_references = pass.parent_references.saturating_add(parents as u64);
        }
    });
}

pub(super) fn snapshot(rows: usize, cells: usize) {
    update(|work| work.snapshots.add(rows, cells));
}

pub(super) fn closure(rows: usize, cells: usize) {
    update(|work| work.closures.add(rows, cells));
}

pub(super) fn probe(rows: usize, cells: usize) {
    update(|work| work.probes.add(rows, cells));
}

pub(super) fn closure_cache_hit() {
    update(|work| work.closure_cache_hits = work.closure_cache_hits.saturating_add(1));
}

impl FunctionWork {
    fn write(&self) {
        use std::fmt::Write as _;
        let mut text = String::new();
        let mut row = |kind: &str, ordinal: usize, metric: &str, value: u64| {
            let _ = writeln!(
                text,
                "{}\t{}\t{}\t{kind}\t{ordinal}\t{metric}\t{value}",
                std::process::id(),
                self.run,
                self.name,
            );
        };
        row("function", 0, "joins", self.joins);
        row("function", 0, "join_passes", self.passes.len() as u64);
        row("function", 0, "intern_calls", self.intern_calls);
        row("function", 0, "closure_cache_hits", self.closure_cache_hits);
        for (kind, sizes) in [
            ("snapshot", &self.snapshots),
            ("closure", &self.closures),
            ("probe", &self.probes),
        ] {
            for (metric, value) in [
                ("count", sizes.count),
                ("rows_sum", sizes.rows),
                ("rows_max", sizes.max_rows),
                ("cells_sum", sizes.cells),
                ("cells_max", sizes.max_cells),
            ] {
                row(kind, 0, metric, value);
            }
        }
        for (index, pass) in self.passes.iter().enumerate() {
            for (metric, value) in [
                ("join", pass.join),
                ("inputs", pass.inputs as u64),
                ("union_rows", pass.union_rows as u64),
                ("pairs_evaluated", pass.pairs_evaluated as u64),
                ("pairs_retained", pass.pairs_retained as u64),
                ("parent_references", pass.parent_references),
                ("intern_calls", pass.intern_calls),
            ] {
                row("join", index, metric, value);
            }
        }
        if let Some(Some(output)) = OUTPUT.get()
            && let Ok(mut file) = output.lock()
            && let Err(error) = file.write_all(text.as_bytes())
        {
            eprintln!("checker work output failed: {error}");
        }
    }
}
