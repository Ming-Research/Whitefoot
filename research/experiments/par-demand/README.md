# Demand hand-out experiments

This is the opt-in prototype for [the prospective experiment](../../investigations/par-demand/DESIGN.md).
It changes no language rule or adopted production policy. Remove or replace this
harness when that investigation ends. No gate consumes this directory.

`--par-demand` implies `--par`, takes `--par`'s call grain (`--par-call-grain
off` keeps every permitted group, as experiment 1 did), and keeps the existing recursion budget,
cut and sequential clone. The current prototype uses an outlined slice driver
and the original chunk. The caller bypasses the driver below
`ceil(150000 / static_weight)` iterations. The driver runs slices of
`max(1, floor(150000 / static_weight))` iterations, polling only when the
remaining span reaches the minimum and exceeds one. A request publishes the
far half and runs the near half locally. Reductions seed the near half with
their live value and combine near then far after joining. Caller-local slices
and forced early expansion were rejected in the investigation.
Indexed accumulators deliberately retain the existing prepare/split/finish path;
the ledger says `legacy splitter for indexed`. This exception can still publish
work with requests off. Inspect each formal kernel's ledger before attributing
its result to slices; an indexed fallback is not evidence about a slice driver.

Static pruning is deliberately limited to literal endpoint values and a chunk
with no calls or nested loops. It reads no new checked facts and does not price
an unknown helper loop as bounded work. A zero scheduling weight marks a pruned
site only after the demand-mode pass. Its caller goes straight to the chunk,
and its unused driver is omitted. The existing non-demand lowering, weight
assignment and emission branches retain their output text.

The C lane layout remains private. Both checks emit one load of the module's
thread-local request word:

```llvm
%word = load atomic i64, ptr @wf__par_demand_word monotonic, align 8
%requested = icmp ne i64 %word, 0
```

At a group this branches around acquisition and merges a null frame into the
existing refused edge. At a non-indexed loop, the remaining-work comparison
precedes the poll inside the driver, and a false poll executes a chunk slice
then returns to the comparison. Its recursive workers use
the same interval and still check before offering a half. The word is this
thread's `wf__par_demand_word`, a `_Thread_local` the demand scheduler unit
defines; the module carries a weak zero definition so that it still links and
runs sequentially without the runtime (Windows takes the external
declaration). On ELF the load is one `mov %fs:...` instruction; on Mach-O a
thread-local access is a `_tlv_get_addr` call, which only correctness relies
on. There is no exported lane offset, no runtime call on the no-request path
and no clock read in the scheduling decision. The emitted strong demand-mode
marker lets the runtime enable posting before a pool starts even through an
indexed splitter. Ordinary modules lack the marker. Their C units preprocess
out the demand field, posting, clearing, accessor and startup setting
entirely; only demand modules link the two opt-in scheduler objects. This
preserves the ordinary runtime's lane layout and scheduler path as well as
legacy LLVM emission.

An idle worker writes one selected victim's word only when zero after a failed
scan, through the address the victim's lane registered when its owner thread
attached (null, and so unaskable, before). Publication clears the owner's own
word before making its frame stealable. The setting
`WF_PAR_DEMAND=off-never-request` is read once on demand startup, prevents
every posting and leaves the checks at zero. `on` (or unset) enables posting.
Hints carry no task or result synchronization; deque and join ordering remain
the existing protocol. World selection (`wf__par_pool_active`) starts the
pool and attaches the selecting thread in demand mode, so asking for demand
cannot wait on the first offer and the first offer cannot wait on a request.
`wf__par_demand_requested()` returns the same word for native probes; the
emitted code never calls it.

## Reproduction in CI

Dispatch `.github/workflows/compute-bench.yml` with `experiment=par-demand`,
`placement_runner=github` for sizing or `14900k` for the real panel, and
`placement_rounds` for the round count. Every dispatch first builds on a hosted
runner, verifies results with requests on and off at widths 1 and 4, compares
legacy emission, and runs one small sizing round. The selected runner then
uses those exact images; the hosted selection remains sizing-only. The native
runner shares the formal performance instrument's prepare/call/check boundary:
only the WF call is timed, with wall and process CPU clocks. Sample zero warms
the same workload and is retained but excluded from the verdict. No local
performance measurement is required.

```sh
make -C research/experiments/par-demand build verify BUILD=/tmp/par-demand
make -C research/experiments/par-demand measure BUILD=/tmp/par-demand ROUNDS=10
make -C research/experiments/par-demand summarize BUILD=/tmp/par-demand
```

Run these commands in CI. `WFC` names the built compiler. `seq`, `demand`, and
`par` compile the same source and compiler with no flag, `--par-demand`, and
`--par` respectively. `twin` copies the complete candidate executable; both
construction and measurement check equality. Oracle and non-scheduler runtime objects and native optimization settings are
shared; only demand enables the two scheduler units' experiment macro. `manifest.json` owns workloads and batch
sizes. The static and runtime tiny loops run 200 million calls in the full
panel. `small_split.wf` reuses the requested branch's `mark` and shifted walker,
with a host-supplied repeat count/extent and an observable complete-array sum;
`small_constant.wf` replaces only the leaf span with `0..3` and supplies its
bounds. The six formal kernels remain owned by `tests/programs/compute`; the
Makefile compiles those `.wf` sources directly. Prefix and histogram reuse
`blocked_oracle.c` through the small performance adapter. Other kernels reuse
their formal performance APIs unchanged.

Rounds rotate and reverse workload, width and build order. In experiments 1
and 2, exceeded bounds get
one extra interleaved batch at the same workload and width. No timing or source
result is replaced on a rerun. The reducer reports wall/CPU medians, candidate
ratio intervals and the initial/rerun verdicts. A failed rerun is `fail`; a first
exceedance followed by a pass is `inconclusive` (the batches disagree). The
statistic is the median of paired per-round ratios with a 95% interval from
10,000 bootstrap medians, seed `20261010`. A twin/demand interval excluding 1
voids the cell. Experiment 1 uses the decision-point allowance at widths 4/8
and the `[0.98, 1.02]` band at width 1; experiment 2 adds the preregistered
keep-speedup, CPU-margin and idle-policy rules.
The script reports an experiment failure as data, and exits unsuccessfully
only for malformed/missing evidence or a build/execution/oracle error.

No result may pass until optimized code is inspected. The build saves emitted
LLVM, ledgers, optimized assembly and linked disassembly (including the C
accessor). Put `inspection.json` beside the measurements with one entry per
workload, for example:

```json
{
  "small_constant": {
    "hot_work_survives": true,
    "check_compiles_to": "no request load at mark; the three stores survive",
    "evidence": "demand/small_constant.disassembly: inspected mark and its callers"
  }
}
```

Use actual observations, never this example as evidence. `hot_work_survives`
means the hot work and every intended non-pruned demand site survived; name the
machine instructions in `check_compiles_to`. Missing inspection or optimized-away
work blocks a pass. The initial report therefore remains inconclusive wherever
timing alone meets the bounds. Inspecting code and applying the same reducer
completes the report without taking another measurement. A formal-kernel failure
is attributed to the slice driver, recursion/spine/hot-helper to group checks,
and tiny loops to pruning, as fixed in the investigation; inspect the ledger to
identify an indexed fallback before interpreting that attribution.

## Validation boundary

Compiler tests cover option isolation, checks before group acquisition, the
existing recursion cut, runtime-extent drivers, literal small-site pruning and
indexed fallback. Maintained program tests compare sequential results with
both request settings at widths 1/4. The native scheduler probe deterministically
exercises failed-scan posting, write-if-zero, publication clearing, reposting
and the disabled setting. Its Make target belongs to the runtime gate.
The manual reducer controls exercise wrong ratios, noisy twins, absent
inspection, duplicate rows and missing measurements.

`legacy_identity.py` compares complete `--par` LLVM for nine maintained programs
against the pre-prototype compiler at
`ac5f1498a431df3043fe42a5ce38f4eaf750e4c0`; the workflow builds that revision in
an isolated checkout. The compiler unit test also compares legacy bytes before
and after a demand compilation in one process, detecting leaked option state.
The baseline comparison, emitted-IR validity, native program tests, reducer
controls and performance results must run in CI; cargo check alone proves none
of those. No measurement has been taken by this implementation task.

## Experiment 3: isolated attribution

Select `par_demand_experiment=3`, `placement_runner=14900k`, with
`placement_rounds` empty. The hosted job builds and verifies the images; the
14900K takes six full-work sizing rounds, archives them in `sizing-e3/`, freezes
the chosen count (6 through 30), and collects a separate decisive matrix at
widths 4 and 8. There is no exceeding-cell rerun. Sizing projects each paired
interval's width by `sqrt(6/n)` and selects the first count whose projected
width is at most 0.02 or its median's distance to the comparison point (1 for
ratios, 0 for H3). The sample is never pooled with decisive observations.

The compiler accepts `--par-demand-ablation none|order|seed|extent` only with
`--par-demand`; omission is `none`. Each arm changes one mechanism:

- `order` publishes near and runs far, keeping the original seeds, bounds,
  joins and near-then-far reduction combine.
- `seed` passes `min(span / minimum_span, lanes * 16)` at entry. A recursive
  publication chain hands out one far share at a time, then the owner runs the
  remaining near share. Every share is at least the minimum span; all shares
  use the existing request-driven refinement without reseeding. Frame refusal
  retains ordinary-call fallback. Statement groups and recursion cuts do not
  change, so a loop-free recursion workload is a negative control for this arm.
- `extent` evaluates the existing `split.work` estimate and uses it for both
  admission and slice pricing, clamped to one as in the eager runtime. Missing
  estimates retain the static price. No work-estimator rule changes.
- `dedup` (`WF_PAR_DEMAND_DEDUP`) skips the counted request when the victim
  lane's `request_posted` flag is set; the posting thief sets it and the
  owner's publication clears it. The flag lives in the lane, which outlives
  every owner, so reading it uncounted is safe, while the owner's word is still
  touched only inside the counted section (the investigation records why the
  registered read-before-count form was amended).

`summary.json` reports every arm/demand paired wall ratio, CPU ratio and H3
margin, plus a cause table for the preregistered cells. Neutral intervals
reject the specified cause; intervals wholly below 1 support it; regression
or mixed wall/CPU evidence stays undecided. For extent, both wall and CPU must
support improvement or both contain 1 for rejection. Dedup uses overlap of
the two H3 intervals for rejection, and disjoint improving intervals plus a
negative paired margin-change interval for support. Voided cells and sizing
data decide nothing. These are attribution verdicts, not acceptance of the
hybrid plan; dropping any proposed change remains an owner decision.

The separate `counter-build` target reuses timed WF objects and replaces only
the scheduler/clock objects with `WF_PAR_DEMAND_COUNTERS`. `--instrumented`
runs one process per arm/workload/width with `WF_SCHED_REPORT=2`, retaining
stdout and stderr in `counter/counters/`; the runner's two calls remain within
that one process. Counters are per-lane atomic exit snapshots: attempted and
posted requests, publications, steals, parks, scan/spin/yield elapsed ns, and
completed park elapsed ns. Concurrent writes that observed zero each count as
a post. Outstanding sleeps at exit are not included; these diagnostics do not
claim exclusive CPU attribution. Counter images carry a `counter-build`
marker, and the measurement driver refuses to time them or summarize them.

`ablation-identity` compares both ordinary and macro-off demand `core.o`
against `fadbc866b7423ab50606177f962724df309bf8e6` with the same flags. CLI
tests compare explicit `none` with an omitted ablation; backend tests check
each arm's structure and default emission after compiling the arms. The
existing pre-prototype `--par` comparison remains wired. All these checks are
CI obligations, not verified results of this uncommitted implementation.
