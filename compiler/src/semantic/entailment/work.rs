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
//! - `function` also aggregates affine work: `affine_attempts` counts query
//!   preparations (ordinary targets and certificate residuals);
//!   `affine_memo_hits` and `affine_memo_misses` partition those attempts by
//!   closed-state identity and complete ordered candidate equality.
//!   `affine_cold_demands` counts distinct vector requests before promotion,
//!   including absent vectors and the request that triggers promotion.
//!   `affine_left_candidates_scanned` counts left candidates visited by lazy
//!   requests, including failed inverse keys; the promotion-triggering request
//!   scans none. It excludes the complete builder's pair traversal.
//!   `affine_promotions` counts complete builds, at most one per residency.
//!   `affine_final_family_starts` counts entry into AUTO's final L0 family;
//!   `affine_final_family_exhaustions` counts those traversals returning no
//!   proof after all entries. Repeated traversals count separately even when
//!   the complete index is reused. `affine_failed_reuse_hits` counts targets
//!   whose complete AUTO failure is reused within one numeric disposition,
//!   after revalidating the closed facts and ordered candidate images under
//!   that session's fixed binding images and automatic premises. These hits
//!   execute no final-family traversal. Cold scans are aggregated per request;
//!   all affine counters are aggregated per function before emission.
//! - Additional `function` affine observations (all emitted even when zero):
//!   `affine_auto_calls_initial` counts initial numeric/bounded-relation AUTO
//!   submissions; `affine_auto_calls_bridge` counts Step-6 residual submissions
//!   in both numeric and bounded-relation routes; `affine_auto_calls_other`
//!   counts other standalone AUTO (including quotient targets). Submissions
//!   include failed-query reuse hits, which perform no DIRECT search.
//!   `affine_direct_initial`, `affine_direct_single`, `affine_direct_pair`,
//!   `affine_direct_final_l0` count actual DIRECT calls in each AUTO family;
//!   `affine_direct_other` counts DIRECT calls outside an AUTO family. Initial
//!   here means DIRECT(T) of every AUTO, including a bridge-origin AUTO.
//!   Tightened variants count in the same family; unformable residuals do not.
//!   `affine_tightenings_formed` counts representable integer tightenings in
//!   the shared candidate scratch path, even if an earlier variant succeeds.
//!   `affine_overflow_pair` counts discarded pair sums; `_tightening` counts
//!   overflowing factor computations or tightening formations; `_residual`
//!   counts discarded candidate residuals; `_direct` counts DIRECT returning
//!   arithmetic overflow; `_bridge` counts discarded bridge image/target
//!   formations; `_index` counts discarded L0 image formations in lazy or
//!   complete index preparation. These are operations, not unique queries.
//!   `affine_overflow_residual_bound_up` and `_bound_down` are subsets of
//!   `_residual`: final bound subtraction overflow toward MAX or MIN. They
//!   exclude coefficient overflow before bound subtraction. Non-overflow
//!   arithmetic errors and failed inverse lookup keys are not counted.
//!   `affine_interval_construction_probes` and `_warm_probes` partition the
//!   endpoint-map membership probes in DIRECT: absent (construct) or present
//!   (reuse). Dot-product and successful-parent endpoint reads are excluded.
//! - The `affine_shadow_*` counters observe numeric proofs entering Step 6
//!   after complete initial AUTO failure. `groups` partitions into
//!   `would_certify`, `rejected` (all fixed assignments fail validation or do
//!   not violate the target), and `unknown` (stale/unavailable inputs,
//!   arithmetic uncertainty without a surviving model, or a work cutoff).
//!   `assignments` counts attempted zero-clamped, all-minimum and all-maximum
//!   vectors, in that order. They must satisfy every canonical automatic
//!   premise, strongest indexed L0 image and strongest atom interval and
//!   violate the target. The view is rechecked after ordinary bridge setup
//!   and every actual AUTO, and every submitted bridge/residual is validated.
//!   `bridge_calls` and `final_exhaustions` count actual residual submissions
//!   (including reuse hits) and actual final-family exhaustions only inside
//!   completely certified groups. They exclude the initial AUTO. Other
//!   bounded-relation Step-6 routes contribute no shadow group.
//!   `operations` counts charged observer units: visited/copied atom records,
//!   images/coefficients, endpoint queries, vector entries, inequality checks,
//!   scalar lookup/multiply/add and a conservative key-sorting allowance. It
//!   is not an instruction count; closure lookup internals are not expanded.
//!   Storage/scans are charged before allocation or traversal. At most
//!   100,000 units per group and 2,000,000 per function invocation are allowed;
//!   `group_cutoffs` and `function_cutoffs` count the first refused charge per
//!   affected group. A cutoff only invalidates this observer, never AUTO.
//!   With cutoffs/unknowns, measured certified coverage is a lower bound.
//!   No shadow snapshot, allocation or probe executes without an active
//!   counter scope. The probe owns detached data and publishes no proof.
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

#[derive(Default)]
struct AffineWork {
    attempts: u64,
    memo_hits: u64,
    memo_misses: u64,
    cold_demands: u64,
    left_candidates_scanned: u64,
    promotions: u64,
    final_family_starts: u64,
    final_family_exhaustions: u64,
    failed_reuse_hits: u64,
    extra: [u64; AffineMetric::Count as usize],
}

/// Observer-only affine metrics. Names are shared by TSV emission and tests.
#[derive(Clone, Copy)]
pub(super) enum AffineMetric {
    AutoCallsInitial,
    AutoCallsBridge,
    AutoCallsOther,
    DirectInitial,
    DirectSingle,
    DirectPair,
    DirectFinalL0,
    DirectOther,
    TighteningsFormed,
    OverflowPair,
    OverflowTightening,
    OverflowResidual,
    OverflowResidualBoundUp,
    OverflowResidualBoundDown,
    OverflowDirect,
    OverflowBridge,
    OverflowIndex,
    IntervalConstructionProbes,
    IntervalWarmProbes,
    ShadowGroups,
    ShadowWouldCertify,
    ShadowRejected,
    ShadowUnknown,
    ShadowGroupCutoffs,
    ShadowFunctionCutoffs,
    ShadowAssignments,
    ShadowOperations,
    ShadowBridgeCalls,
    ShadowFinalExhaustions,
    Count,
}

impl AffineMetric {
    const NAMES: [&'static str; Self::Count as usize] = [
        "affine_auto_calls_initial",
        "affine_auto_calls_bridge",
        "affine_auto_calls_other",
        "affine_direct_initial",
        "affine_direct_single",
        "affine_direct_pair",
        "affine_direct_final_l0",
        "affine_direct_other",
        "affine_tightenings_formed",
        "affine_overflow_pair",
        "affine_overflow_tightening",
        "affine_overflow_residual",
        "affine_overflow_residual_bound_up",
        "affine_overflow_residual_bound_down",
        "affine_overflow_direct",
        "affine_overflow_bridge",
        "affine_overflow_index",
        "affine_interval_construction_probes",
        "affine_interval_warm_probes",
        "affine_shadow_groups",
        "affine_shadow_would_certify",
        "affine_shadow_rejected",
        "affine_shadow_unknown",
        "affine_shadow_group_cutoffs",
        "affine_shadow_function_cutoffs",
        "affine_shadow_assignments",
        "affine_shadow_operations",
        "affine_shadow_bridge_calls",
        "affine_shadow_final_exhaustions",
    ];
}

pub(super) fn affine_result<T>(
    result: Result<T, super::affine::AffineCheckError>,
    metric: AffineMetric,
) -> Result<T, super::affine::AffineCheckError> {
    if matches!(
        result,
        Err(super::affine::AffineCheckError::ArithmeticOverflow)
    ) {
        affine_metric(metric);
    }
    result
}

pub(super) fn affine_metric(metric: AffineMetric) {
    affine_add(metric, 1);
}

pub(super) fn affine_add(metric: AffineMetric, amount: u64) {
    update(|work| {
        let value = &mut work.affine.extra[metric as usize];
        *value = value.saturating_add(amount);
    });
}

/// No allocation, snapshot or probe runs without an active counter scope.
pub(super) fn affine_observing() -> bool {
    if !enabled() {
        return false;
    }
    ACTIVE.with(|active| active.borrow().is_some())
}

pub(super) fn affine_exhaustions() -> u64 {
    if !affine_observing() {
        return 0;
    }
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .unwrap()
            .affine
            .final_family_exhaustions
    })
}

pub(super) const SHADOW_FUNCTION_OPERATIONS: u64 = 2_000_000;

/// Charges only observer work, never the affine checker's arithmetic state.
pub(super) fn shadow_charge(amount: u64) -> bool {
    let mut charged = false;
    update(|work| {
        let count = &mut work.affine.extra[AffineMetric::ShadowOperations as usize];
        if amount <= SHADOW_FUNCTION_OPERATIONS.saturating_sub(*count) {
            *count += amount;
            charged = true;
        }
    });
    charged
}

fn enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = TEST_ENABLED.with(std::cell::Cell::get) {
        return enabled;
    }
    ENABLED.load(Ordering::Relaxed)
}

#[cfg(test)]
thread_local! {
    static TEST_ENABLED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Installs a test-only thread-local sink without touching the process output
/// OnceLock, environment, or another test's active invocation.
#[cfg(test)]
pub(super) struct AffineTestScope(Option<FunctionWork>, Option<bool>);

#[cfg(test)]
impl AffineTestScope {
    pub(super) fn new() -> Self {
        let previous = ACTIVE.with(|active| active.replace(Some(FunctionWork::new("test"))));
        Self(
            previous,
            TEST_ENABLED.with(|enabled| enabled.replace(Some(true))),
        )
    }

    pub(super) fn disabled() -> Self {
        let previous = ACTIVE.with(|active| active.replace(None));
        Self(
            previous,
            TEST_ENABLED.with(|enabled| enabled.replace(Some(false))),
        )
    }

    pub(super) fn rows(&self) -> String {
        ACTIVE.with(|active| active.borrow().as_ref().unwrap().text())
    }
}

#[cfg(test)]
impl Drop for AffineTestScope {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.replace(self.0.take()));
        TEST_ENABLED.with(|enabled| enabled.set(self.1));
    }
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
    affine: AffineWork,
}

fn update(f: impl FnOnce(&mut FunctionWork)) {
    if !enabled() {
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
    let previous = ACTIVE.with(|active| active.replace(Some(FunctionWork::new(name))));
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

pub(super) fn affine_query(hit: bool) {
    update(|work| {
        let affine = &mut work.affine;
        affine.attempts = affine.attempts.saturating_add(1);
        if hit {
            affine.memo_hits = affine.memo_hits.saturating_add(1);
        } else {
            affine.memo_misses = affine.memo_misses.saturating_add(1);
        }
    });
}

pub(super) fn affine_cold_demand(left_candidates: usize) {
    update(|work| {
        let affine = &mut work.affine;
        affine.cold_demands = affine.cold_demands.saturating_add(1);
        affine.left_candidates_scanned = affine
            .left_candidates_scanned
            .saturating_add(left_candidates as u64);
    });
}

pub(super) fn affine_promotion() {
    update(|work| work.affine.promotions = work.affine.promotions.saturating_add(1));
}

// Thread-local observations of the production traversal hooks, independent of
// the optional TSV sink. Tests compare deltas, so parallel tests do not share
// counts and no environment variable or output file is needed.
#[cfg(test)]
thread_local! {
    static AFFINE_TRAVERSALS: std::cell::Cell<[u64; 3]> = const { std::cell::Cell::new([0; 3]) };
}

#[cfg(test)]
pub(super) fn affine_traversals() -> [u64; 3] {
    AFFINE_TRAVERSALS.with(std::cell::Cell::get)
}

#[cfg(test)]
fn affine_traversal(index: usize) {
    AFFINE_TRAVERSALS.with(|counts| {
        let mut value = counts.get();
        value[index] += 1;
        counts.set(value);
    });
}

pub(super) fn affine_failed_reuse_hit() {
    #[cfg(test)]
    affine_traversal(2);
    update(|work| {
        work.affine.failed_reuse_hits = work.affine.failed_reuse_hits.saturating_add(1);
    });
}

pub(super) fn affine_final_family_start() {
    #[cfg(test)]
    affine_traversal(0);
    update(|work| {
        work.affine.final_family_starts = work.affine.final_family_starts.saturating_add(1);
    });
}

pub(super) fn affine_final_family_exhausted() {
    #[cfg(test)]
    affine_traversal(1);
    update(|work| {
        work.affine.final_family_exhaustions =
            work.affine.final_family_exhaustions.saturating_add(1);
    });
}

impl FunctionWork {
    fn new(name: &str) -> Self {
        Self {
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
            affine: AffineWork::default(),
        }
    }

    fn text(&self) -> String {
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
        for (metric, value) in [
            ("affine_attempts", self.affine.attempts),
            ("affine_memo_hits", self.affine.memo_hits),
            ("affine_memo_misses", self.affine.memo_misses),
            ("affine_cold_demands", self.affine.cold_demands),
            (
                "affine_left_candidates_scanned",
                self.affine.left_candidates_scanned,
            ),
            ("affine_promotions", self.affine.promotions),
            ("affine_failed_reuse_hits", self.affine.failed_reuse_hits),
            (
                "affine_final_family_starts",
                self.affine.final_family_starts,
            ),
            (
                "affine_final_family_exhaustions",
                self.affine.final_family_exhaustions,
            ),
        ] {
            row("function", 0, metric, value);
        }
        for (metric, value) in AffineMetric::NAMES.iter().zip(self.affine.extra) {
            row("function", 0, metric, value);
        }
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
        text
    }

    fn write(&self) {
        let text = self.text();
        if let Some(Some(output)) = OUTPUT.get()
            && let Ok(mut file) = output.lock()
            && let Err(error) = file.write_all(text.as_bytes())
        {
            eprintln!("checker work output failed: {error}");
        }
    }
}
