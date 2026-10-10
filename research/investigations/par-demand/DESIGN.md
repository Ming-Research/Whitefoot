# Demand-driven hand-out: experiment 1, the cost of waiting for demand

The owner chose demand-driven hand-out for `--par` (the plan in
`research/investigations/par-grain-plan`, on its branch until merged): work is
handed to another worker only when an idle worker has asked for it, and only
when the work is worth moving; static prices are advice. The owner also chose,
for its three open questions: run two experiments, this one first; learn
"worth" at run time with a clock read at hand-outs and samples, falling back to
the static price on targets without a clock; keep the recursion cut and its
sequential clone, giving a stolen task a fresh budget and refreshing the cut
on demand at a bounded rate.

This investigation is the first experiment. It changes no default behaviour:
everything it adds is behind a compiler option and an environment setting.

## Question

Can the bookkeeping that demand-driven hand-out needs be paid on the path that
hands nothing out, without making a `--par` program slower than the same
program built without `--par`?

Demand-driven hand-out keeps every permitted fork and split as a point where
work could be handed out, and asks at each such point whether anyone wants
it. When nobody does (the common case: no idle worker, or a one-core host, or
work too small), the program must run as if it were sequential. If the
checks, the slice driver that replaces recursive range bisection, and the
state they need make that path measurably slower, the direction fails its
first criterion (never slower than one thread) before any hand-out happens,
and the design must change before anything else is built.

## The prototype

This describes the first measured prototype. The [fourth change](#the-fourth-change-fixed-before-it-measures) below describes the current unmeasured candidate; the sections between record each measured change and its result.

Behind `--par-demand` (compiler) and with no worker ever asking (runtime
setting `WF_PAR_DEMAND=off-never-request`, so every check answers "nobody
wants it"):

1. **The request word.** Each lane has one word that an idle worker would set
   to ask that lane for work (written only if zero). A point that could hand
   work out reads its own lane's word and branches; this experiment never sets
   it.
2. **Group calls.** A permitted statement-group member is offered only when
   the request word is set; otherwise it runs as the ordinary call its refused
   edge already makes. The recursion budget and its cut stay as they are.
3. **Loop splits: the slice driver.** A permitted counted loop runs its chunk
   on successive slices of the remaining range instead of being bisected by a
   precomputed allowance. Each slice is sized from the static price to about
   5 microseconds of work, and the driver reads the request word between
   slices. With nobody asking it runs all slices in order.
4. **Static pruning.** A site whose span times weight is known at compile time
   to be below the work unit (a constant extent, a loop-free and call-free
   body) gets no check and no driver at all: the chunk is called directly, as
   in the sequential world. This is the small-loop case of the paused
   backlog item on small split loops.

Nothing is timed by a clock in this experiment; learning "worth" belongs to
stage 3.

## Workloads

Each one built three ways from the same source and compiler: without `--par`
(the baseline `T_seq`), with `--par-demand` (the candidate), and today's
`--par` (for reference), plus an identical-image twin of the candidate. The
candidate runs at one worker (it then runs the sequential world, a control
that should equal `T_seq`) and at four and eight workers with no requests.

| Workload | What it stresses |
| --- | --- |
| a three-iteration split loop called 200 million times | a check at a tiny site; static pruning must remove it |
| the same loop with a runtime extent of 3 | a check that cannot be pruned |
| fib-like balanced recursion with nanosecond nodes | checks at group calls above the cut |
| a deep spine with tiny side leaves | checks at a long chain of group calls |
| a hot sequential loop calling a helper with data-dependent work | checks inside an otherwise sequential hot path |
| the formal kernels (mandelbrot, records, fir, stencil, prefix, histogram) | the slice driver against today's leaf loop: vectorization, alias facts, loop shape |
| a helper whose internal loop is large and splittable | a site that must keep its check (not prunable) |

## Measurement

On the i9-14900K through CI, interleaved rounds with each arm's twin, after a
small hosted sizing run. Wall and CPU time per run.

## Pass and fail, fixed before measuring

Let `T_seq` be the median sequential time and `noise` the larger of the
candidate twin's spread and 1 percent.

- **Pass**: on every workload, the candidate at four and eight workers with no
  requests takes at most `1.02 * T_seq` plus `noise`; and at one worker it is
  within `noise` of `T_seq`.
- **Fail**: any workload above that bound after one rerun. A failure on the
  formal kernels names the slice driver; a failure on the recursion or
  spine workloads names the group-call check; a failure on the tiny loop
  names pruning.
- **Inconclusive**: when the twin's spread exceeds 2 percent for a workload,
  that workload decides nothing and is reported as such.
- The optimized code of each hot site is inspected and the report says what
  the check compiled to; a pass whose site was optimized away entirely (so the
  check was never measured) does not count for that site.

Overhead is measured at constant work (the owner, 2026-10-10): every arm of
a cell runs the same input, repetitions and extent, and each result is checked
against the independent oracle. For a cell at `W` workers the overhead is the
process's total CPU time (all threads, user and system, over the timed call)
divided by the sequential build's, minus one; the wall ratio says only
whether the run got faster. Every summary reports both, for the candidate and
for today's `--par`.

A pass sends the work to the second experiment (does demand-driven hand-out
keep today's speedups?). A fail goes back to the owner with the attribution
before any further building.

## Results of the first run

Run: [compute-bench 37986473421](https://github.com/Ming-Research/Whitefoot/actions/runs/37986473421),
branch `claude/par-demand` at f548108a5, i9-14900K self-hosted runner (32
logical CPUs), 10 interleaved rounds of every arm, width and workload, two
samples each, 2026-10-09. A hosted sizing run first
([37937492878](https://github.com/Ming-Research/Whitefoot/actions/runs/37937492878),
4-vCPU EPYC, sizing repetitions) found the four arms' outputs equal and every
maintained `--par` module byte-identical to the pre-prototype compiler.

Median wall time of the candidate (`demand`) and of today's `--par` (`par`)
over the sequential build, at 1 / 4 / 8 workers; `twin` is a copy of the
candidate image:

| workload | demand | par | twin |
|---|---|---|---|
| small_constant | 1.000 / 1.001 / 1.024 | 1.000 / 1.021 / 1.012 | 1.001 / 0.999 / 1.001 |
| small_split | 1.001 / 3.923 / 3.955 | 0.992 / 5.318 / 5.317 | 0.993 / 3.929 / 3.953 |
| recursion | 1.007 / 1.005 / 1.005 | 1.007 / 0.273 / 0.143 | 1.005 / 1.007 / 1.008 |
| spine | 0.997 / 1.024 / 1.035 | 0.994 / 3.652 / 4.371 | 0.994 / 1.025 / 1.017 |
| hot_helper | 0.985 / 0.992 / 0.993 | 1.001 / 0.999 / 0.999 | 0.986 / 0.992 / 0.993 |
| large_helper | 0.997 / 1.055 / 1.055 | 0.996 / 0.525 / 0.525 | 0.997 / 1.056 / 1.054 |
| mandelbrot | 0.998 / 0.998 / 1.000 | 1.001 / 0.372 / 0.207 | 0.998 / 0.999 / 1.000 |
| records | 1.005 / 1.077 / 1.072 | 1.003 / 0.415 / 0.183 | 1.002 / 1.079 / 1.073 |
| fir | 0.942 / 1.082 / 1.111 | 0.945 / 0.459 / 0.333 | 0.943 / 1.085 / 1.125 |
| stencil | 0.999 / 0.954 / 1.014 | 1.002 / 0.538 / 0.470 | 1.007 / 0.945 / 1.011 |
| prefix | 0.985 / 1.001 / 1.055 | 1.005 / 0.891 / 1.433 | 0.995 / 1.041 / 1.047 |
| histogram | 0.997 / 0.988 / 0.982 | 0.961 / 0.897 / 0.901 | 0.971 / 1.016 / 0.980 |

Verdict under the rule above as `summarize.py` applies it: every workload is
inconclusive. Its spread is the range of all forty candidate and twin samples
over their median, which exceeded 2 percent in at least one width of every
workload; the 14900K's spreads ran from 0.5 to 71 percent.

Read per cell, one workload fails beyond that question. `small_split` at four
and eight workers exceeded its bound in the first attempt and in the rerun
(3.92 to 3.96 times sequential, spread 1.0 to 1.7 percent), while its
one-worker cell ran at 1.00. The rule names pruning for it. The candidate's IR
for its three-iteration `mark` loop is a slice driver whose loop calls
`wf__par_demand_requested()` on every slice before it tests whether the
remaining range is worth handing out (149,999 / 7 + 1 iterations): the literal
pruning does not cover a runtime extent, and every call reads the request
word, which costs nothing at one worker and about 2.8 ns per call once idle
workers exist. `records`, `fir` and `large_helper` ran 5 to 11 percent slower
at four and eight workers, within the noise bound in most cells; `spine` 2 to
4 percent; the group-call check on `recursion`, `hot_helper` and `mandelbrot`
stayed within 1 percent. The optimized-site inspection the rule requires was
not done, so no cell could pass.

## The rerun's change and rule, fixed before it measures

The owner chose to treat `small_split` as a failure, change the candidate and
rerun experiment 1 on the i9-14900K before experiment 2 (status board,
2026-10-09).

Change: the slice driver tests whether the remaining range is worth handing
out (at least the minimum span and more than one iteration) before it reads
the request word, and reads the word only when it is. A range below the
minimum span never touches the word. Reading the word once per call instead
of once per slice was considered and not taken: after the reordering only a
range worth handing out reads it, once per slice of about 5,000 work units,
and a once-per-call read would keep a long loop from ever handing work to a
worker that becomes idle during it, which experiment 2 measures. The
statement groups' check is unchanged.

Rule: as in "Pass and fail" above, with one change. The twin's spread decides
each workload and width on its own: a cell whose spread exceeds 2 percent
decides nothing, and the other widths of that workload keep their verdicts.
The spread stays the range of all candidate and twin samples of that cell over
their median. The optimized code of each hot site is inspected after the run,
from the images' disassembly, before any cell is counted as a pass.

Expected if the change works: `small_split` at four and eight workers within
its bound, as at one worker. If it stays above its bound after a rerun, the
cost is not the request word's read and the attribution goes back to the
owner.

## Results of the rerun

Run: [compute-bench 37990848736](https://github.com/Ming-Research/Whitefoot/actions/runs/37990848736),
`claude/par-demand` at 8a404f827, i9-14900K, 10 interleaved rounds,
2026-10-09, under the per-width rule above.

`small_split` still fails: 3.239 at four workers (rerun 3.240, spreads 1.4
and 1.2 percent), 3.243 at eight (spread 2.1 percent, so that width decides
nothing), against 1.000 at one. Moving the request-word read behind the span
test removed about 0.7 of the first run's 3.9; the rest is not the read.

Inspection of the candidate image's optimized code
(`demand/small_split.o.s`): at one worker the adapter selects the sequential
world, so the one-worker cell runs the sequential clone and says nothing
about the driver. At four and eight workers each three-iteration `mark` call
is an out-of-line call of `wf__par_slice_mark.0`, which saves six registers
and divides twice by the site's weight (7), passed as an argument, before the
span test skips the request word and the chunk's three stores run inline. The
remaining cost is that per-call driver setup on a range far below the minimum
span: the attribution the rule names pruning, since the literal pruning does
not reach a runtime extent.

The other cells decide nothing under the 2 percent rule, but their medians
repeat the first run's: `fir` 1.088 and 1.098, `large_helper` 1.064 and
1.065, at four and eight workers; `recursion`, `hot_helper`, `spine` and
`mandelbrot` within 2.5 percent; `records` 1.016 and 1.007 where the first
run measured 1.077 and 1.072.

## The second change, fixed before it measures

The owner chose to change the candidate again and rerun experiment 1 under
the same rule (status board, 2026-10-09); a failure goes, with everything
above, to a stronger model for a plan.

Change: the call site of a slice driver compares the whole range with the
site's minimum span, a constant there because the site's weight is static,
and calls the chunk directly when the range is smaller. Only a range worth
handing out enters the driver, so a tiny range pays one comparison and no
driver entry, saved registers or division.

Inspection of `fir` before the rerun, from the first rerun's images
(`fir.o.s`): one worker runs the sequential clone, whose inner multiply-add
loop is compact and whose outer loop is unrolled by two. Four and eight
workers run the chunk `wf__par_chunk_filter.1`, which receives the loop's
bounds and captures as parameters; LLVM unrolls its inner loop by four into
one serial chain of dependent adds with more index arithmetic. The chunk is
the same in today's `--par` image, where the parallel speedup hides it. So the
6 to 10 percent on `fir`, and plausibly on `large_helper`, is the chunk's code
generation, not demand bookkeeping; this change does not address it. Expected
from the change: `small_split` within its bound at four and eight workers;
`fir` and `large_helper` unchanged.

## Results of the second rerun

Run: [compute-bench 37995973324](https://github.com/Ming-Research/Whitefoot/actions/runs/37995973324),
`claude/par-demand` at 122afdc1d, i9-14900K, 10 interleaved rounds,
2026-10-09, same rule.

`small_split` fell from 3.24 to 1.126 at four workers and 1.123 at eight,
still above the 1.02 bound, but its twin spreads (8.0 and 13.6 percent) make
both cells decide nothing. `large_helper` fails at four workers: 1.065, rerun
1.061, spreads 1.1 and 0.8 percent; at eight 1.063, rerun 1.064 with a 5.6
percent rerun spread, so that width decides nothing. `fir` 1.079 and 1.091
(spreads 5.9 percent), `spine` 1.034 and 1.035, `mandelbrot` 1.014 and 1.013,
`records` 1.006 and 1.021; the group-call checks on `recursion` and
`hot_helper` stay within 1 percent.

So experiment 1 fails on `large_helper`, the slice-driver attribution, as
the pre-rerun inspection of `fir` predicted; the call-site comparison removed
most of `small_split`'s cost, whose remaining 12 percent is noise-bound here.
Following the owner's instruction for a failure of this round, the whole
record above goes to a stronger model for a plan.


## Caller-local slices: proposed third change, before measurement

This is an unmeasured experimental implementation after the second rerun,
not an adopted design or a claim that the owner's performance bar is met.
The design tree and specification are unchanged. The question is whether
preserving the caller's optimization context and amortizing polls over a
whole work unit remove the failures together. The same per-width pass/fail
rule applies; no noisy cell becomes a pass by prediction.

### What the third run's code actually shows

The evidence below refers to the `run3/par-demand/{seq,demand}` images from
[the second rerun](https://github.com/Ming-Research/Whitefoot/actions/runs/37995973324),
whose recorded revision is `122afdc1d03c611b249e5bdbc218e6d0183814e9`.
Line numbers are in the saved `.o.s` files, not newly generated assembly.

- **Runtime tiny range (`small_split`).** Both `mark` and its chunk have
  already inlined into `wf_workload`. The remaining cost is not another
  chunk call. Sequential lines 233–247 check the input range and enter the
  loop. Demand lines 423–435 additionally reload the Box slot and seed,
  materialize `upper - lower`, saturate it with `cmovaeq`, compare against
  21,428, and branch to the driver. Lines 478–482 are the three scalar stores'
  loop; it makes no request call. The cold driver still makes captures live
  across a possible call, with a larger frame (lines 339–351) and spills.
  The saturated expression survives even though the caller just checked
  that addition did not wrap. Thus the 12 percent has a concrete fast-path
  mechanism; the noisy measurements do not isolate each instruction's cost.
- **Large scalar helper (`large_helper`).** It is not the same clear
  inner-loop code-quality defect as FIR. Sequential lines 105–126 and
  demand lines 659–677 use the same four-way rotate/multiply/add structure.
  Demand lines 570–588 do two divisions at driver entry; lines 614–638 do
  span arithmetic, request call, reloads, slice-bound choice and a remainder
  setup *on every slice*. The static weight is 8 (`large_helper.ll.raw`,
  caller), so 5,000 units gives 625 iterations. The measured source has
  two million-iteration helper calls per repetition. Each helper starts
  1,600 slices and polls 1,571 times (while at least 18,750 remain), plus
  the caller's group poll. A 625-iteration batch repeatedly needs the
  modulo-four cleanup. The optimized arithmetic is much cheaper than the
  prototype's assumed nanoseconds per IR unit. This is strong evidence for
  polling/strip-mining overhead, not proof of what fraction of the 6 percent
  it accounts for; a same-source ablation remains necessary for attribution.
- **FIR (`fir`).** Sequential lines 144–188 have two outer iterations and
  a compact inner `movsd; mulsd; addsd; incq; addq; cmpq; jbe` loop. Demand
  lines 805–825 have four multiplies and four dependent scalar adds, with
  negative indexed addresses and an inner cleanup at lines 826–850. The
  original chunk exposes endpoints and eight captures as unrelated formals
  (`split.rs`, `build_chunk`); its `last_tap + 1` relation and caller-local
  storage facts become available too late when LLVM has already transformed
  the standalone loop. In `fir.ll.raw`, line 229 has `noalias nonnull nocapture`
  on the source range pointers, while line 840's chunk signature has bare
  pointers; lines 240–241 form `last_tap + 1`, which the chunk receives as an
  independent parameter. Outlining also cuts across the original outer loop.
  This changes optimization opportunities and choices; assembly alone does
  not identify which LLVM pass or individual missing fact selected the
  slower schedule. The driver adds its own polls: weight 182 gives 27 outer
  iterations per old slice, and lines 741–771 show call/reloads/setup.
  Both code shape and bookkeeping are implicated.
- **Spine (`spine`).** Lines 188–217 save five registers, test/decrement the
  recursion budget and call the request accessor; lines 250–260 retain a
  recursive call with work after it. The sequential tail-shaped path is
  reached only at the cut. This mechanism is unchanged in this proposal.
- **World selection.** `small_split.o.s` demand lines 1332–1354 query
  `wf__par_pool_active` once per benchmark entry and choose the demand body
  or sequential clone. It is not paid per one of the 200 million `mark`
  invocations. The one-worker control therefore does not test waiting for
  demand. The linked `large_helper.disassembly` at `0x17000` shows the real
  accessor's attachment test, TLS access, pointer test and request load;
  the weak `ret 0` fallback in the `.o.s` is not its linked implementation.

### Alternatives and recommendation

1. **Only enlarge slices or inline the existing chunk.** Cheap to implement,
   and a larger slice reduces repeated remainder/poll costs, but neither
   alone addresses both the caller-context loss and the tiny dynamic guard.
   Enlarging a nominal number of nanoseconds does not calibrate actual work.
2. **Test once, run the complete sequential clone when unrequested.** This
   best preserves sequential code, but cannot answer demand arriving during
   a long loop. It trades away experiment 2 and is not recommended.
3. **Caller-local slices, early chunk expansion, cold recursive hand-out
   (recommended experiment).** This draft implements that combination.
   In whole-module emission LLVM receives the no-request loop in its original
   caller, with a literal step and `alwaysinline` synthesized chunks. The request check leads to
   the existing driver only when true; it otherwise runs a local slice and
   checks again. A short remainder runs as one chunk without polling.
4. **Asynchronous promotion of sequential machine code.** Patchable safe
   points or interruption could avoid many polling instructions, but need a
   target-specific continuation/register-map and synchronization design.
   They offer no portable zero-cost guarantee, and are a separate design
   investigation if cooperative checkpoints still fail.

The adopted permission, lane/frame ABI, structured join, reduction identity,
recursion cut and two worlds remain. No workload name selects a path. A
nonempty entry test dominates subtraction; advancing by `step` only when
`upper - cursor >= step` proves both generated `nuw` operations, including
an endpoint at `u64::MAX`. An empty/inverted range returns its seed. A
handoff passes the *current* accumulator and cursor with the original upper
bound and captures, so it neither repeats the prefix nor loses its result.

The candidate polls once per `max(2, ceil(150000 / weight))` iterations,
at entry if worthwhile and then after each such slice. The recursive driver
uses the same interval. This replaces the 5,000-unit interval: helper slices
become 18,750 iterations and FIR slices 825. It is a work-unit interval, not
150 microseconds. A helper with one million iterations now makes 53 polls,
and executes its final short remainder once; the group poll stays. The
existing driver can re-read a request that disappeared between the caller
and entry and correctly continue without publication.

**Owner decision required before adoption:** choose caller-local expansion
and work-unit polling instead of the outlined 5,000-unit driver. The latter
is an explicit responsiveness tradeoff, not a performance-neutral refactor.
The current task authorizes an uncommitted experimental implementation and
report; it does not record approval in the design tree. Specification delta:
none. Ordinary non-demand `--par` output is required to remain byte-identical;
all new emission branches and chunk attributes are gated by demand mode.

### Costs, limitations and experiment 2

Compiler emission adds constant-size control flow per split and two static
chunk call sites (full slice and final tail). Forced expansion can duplicate
large/nested bodies, increasing optimizer work and object size, and can make
source helpers too large for further inlining. Neither cost has been measured.
Source functions and recursive drivers are not forced inline. The existing
fragment splitter retains external chunks in separate compilation units under
function granularity, and can separate them from named-module callers under
module granularity (`backend/fragments.rs`, `split_module` and `fragment_module`).
ThinLTO can then import a body already optimized in isolation. Thus this patch
does not establish early expansion for fragmented builds. Qualifying or changing
fragment ownership is deferred until this whole-module experiment earns the
approach; it needs its own code-quality and compile-cost comparison before a
general compiler claim. Even the
one-worker demand clone can compile differently because its generated chunks
now expand earlier. The ordinary compiler modes do not use this attribute.

For N iterations and interval G, the no-request path pays at most floor(N/G)
request calls plus constant entry/tail work. A request call includes TLS and
an ordinary C call; group-call overhead is unchanged. This is a *count*
bound. A static IR weight supplies no lower bound on optimized wall time,
so it is not a 2-percent time guarantee. Tiny dynamic sites still may retain
a size branch, and cold paths may still induce spills. This draft is a
falsifiable candidate, not a solution proved for nearly every program.

For experiment 2, demand already present at a worthwhile entry can still
seed recursive far-half hand-outs immediately. Demand arriving later can
wait one larger slice, and an expensive/data-dependent iteration has no
wall-time bound. This could lose speedup on irregular loops. The next stage
must qualify both constants under the original bar and implement the chosen
runtime calibration before static units can be treated only as advice.
Clock calibration alone cannot retroactively bound a first observation or
an arbitrary input phase change. Indexed reductions still use the legacy
splitter, so neither their timing nor their speedup validates demand-only
hand-out for those sites. These unresolved limits, and the unchanged spine
cost, prevent claiming the full owner's goal complete.

### Prediction fixed before the next run

Ratios below are predictions for the candidate divided by sequential at both
four and eight workers with requests disabled, not acceptance thresholds.
One worker is predicted near 1.00 for every workload; the early-inlining
change still requires inspecting that control. The existing twin-noise rule
and rerun rule decide all cells without alteration.

| Workload | Predicted ratio at 4 / 8 | Falsifiable expectation |
| --- | --- | --- |
| small_constant | 0.99–1.01 / 0.99–1.01 | pruning still emits no polling |
| small_split | 1.00–1.04 / 1.00–1.04 | saturated span and repeated driver setup disappear; low confidence in eliminating every size check/spill |
| recursion | 0.99–1.01 / 0.99–1.01 | group/cut paths unchanged |
| spine | 1.02–1.04 / 1.02–1.04 | no improvement predicted; this can still fail in quiet cells |
| hot_helper | 0.99–1.01 / 0.99–1.01 | group paths unchanged |
| large_helper | 1.00–1.02 / 1.00–1.02 | request calls fall about thirtyfold and caller loop retains constant slice extent |
| mandelbrot | 0.99–1.02 / 0.99–1.02 | expensive iterations amortize checks |
| records | 0.99–1.02 / 0.99–1.02 | caller-local captures and less frequent polling |
| fir | 1.00–1.03 / 1.00–1.03 | early expansion must recover caller-dependent loop simplification; medium/low confidence |
| stencil | 0.99–1.02 / 0.99–1.02 | preserve inner-loop optimization, fewer checkpoints |
| prefix | 0.99–1.03 / 0.99–1.03 | inspect any demand sites separately from legacy reduction work |
| histogram | 0.99–1.03 / 0.99–1.03 | indexed fallback remains; no demand-only claim |

Reject this candidate if any qualified cell exceeds the unchanged bound after
its rerun. Inspect optimized code before counting a pass, especially helper
inlining, FIR's inner loop, tiny-range spills and surviving polls. To separate
causes if needed, compare context-only and interval-only variants in the same
CI panel; no effect here has yet been causally measured.

### Validation boundary for this draft

`cargo check --offline --lib --tests` and `cargo clippy --offline --lib --tests`,
each with `CARGO_BUILD_JOBS=4`, passed locally as explicitly permitted by the
caller. They type-check tests but do not execute their assertions. No build,
test binary, generated program, benchmark or timing was run in this task.

Backend shape coverage replaces the earlier size-only caller test with a
request-dominated handoff and recurring local-slice check, and adds carried
seed/cursor/empty-range checks and early-inline isolation on nested chunks.
The old implementation has neither the local-slice blocks nor the attributes,
so those assertions reject it by inspection. **Red/green execution has not
been performed** under this task's prohibition. CI must run the focused
backend cases, LLVM validation and existing native on/off-demand cases,
then the complete legacy-emission comparison and experiment 1. In particular,
the same-process ordinary-emission test detects leaked option state; it does
not replace a comparison with the pre-change compiler revision.


Formatting: `make -C compiler format` failed on existing formatting drift in
untouched files (including `compiler/src/bin/whitefootc.rs` and
`compiler/tests/support/mod.rs`). Those files were not reformatted. A focused
`rustfmt --check` with edition 2024 and `skip_children=true` passed for all
four changed Rust files. `git diff --check` passed. The harness README's stale
whole-workload noise description was corrected to the already-selected
per-width rule; no reducer or verdict changed.


Completion review: a separate read-only GPT-6 agent reviewed the complete
six-file uncommitted diff against
`23d60293c48989b8eca7a1445b23e44438c41361`, the affected consumers and relevant
A/D/C/T/V and G/DC checklist items. It ran only `git diff --check`. Finding
F1, an overbroad early-inlining claim for fragmented builds, was fixed by
qualifying comments and guidance and recording the deferred qualification;
the reviewer rechecked that wording. No unresolved concrete correctness defect
was found by inspection. C4, T5, DC1 and DC4 remain unverified as applicable:
emitted LLVM/execution, red/green assertions, the adoption decision, ordinary
byte identity and the performance promises are not established by this review.
The implementer also repaired a test's overly broad select matcher before
review completion; literal `select i1 true` must not be mistaken for dynamic
span saturation. Final permitted cargo check and clippy both exited zero
after that repair. Work stops for the caller's review/push and CI evidence;
there is no commit, design approval or claimed experiment pass.

## Results of the third rerun (the stronger model's candidate)

Run: [compute-bench 37999258600](https://github.com/Ming-Research/Whitefoot/actions/runs/37999258600),
`claude/par-demand` at 7c5c99301, i9-14900K, 10 interleaved rounds,
2026-10-09, same rule.

The candidate is worse than the second rerun's and falls outside every
prediction recorded above for the workloads that decided the round:

| workload | predicted, 4 and 8 workers | measured, 4 / 8 workers | second rerun |
|---|---|---|---|
| small_split | 1.00 to 1.04 | 1.261 / 1.257 (8 workers fails: rerun 1.258, spreads 1.6 and 1.0 percent) | 1.126 / 1.123 |
| large_helper | 1.00 to 1.02 | 1.305 / 1.305 (rerun 1.306) | 1.065 / 1.063 |
| fir | 1.00 to 1.03 | 1.132 / 1.163 | 1.079 / 1.091 |
| records | 0.99 to 1.02 | 1.051 / 1.068 | 1.006 / 1.021 |
| spine | 1.02 to 1.04 | 1.005 / 1.037 | 1.034 / 1.035 |
| recursion, hot_helper, small_constant | about 1.00 | within 1 percent | within 1 percent |

At one worker every workload stayed near 1.00, as before. The gate on
7c5c99301 also failed two of the candidate's new backend tests before any
assertion (their sources did not compile). So caller-local slicing with
inlined chunks and per-offer-unit polling, as implemented, is rejected by
its own predictions; following the owner's instruction, the record goes to
the strongest model.

## Diagnosis of the third rerun, and the fourth change, fixed before it measures

This section reads the saved images of the second rerun (`run3`, revision
122afdc1d) and the third (`run4`, revision 7c5c99301) against the sequential
build, says where the no-request time goes, and fixes the next candidate and
its predictions before the i9-14900K measures it. Line numbers are in the
saved `.o.s` files of each run's `demand` and `seq` arms. Cycle estimates
assume about one cycle per sequential `large_helper` iteration (200 million
iterations in 40.1 ms), so they are rough and serve attribution, not
accounting.

### The idle workers are parked; the cost is the main thread's code

In every demand cell at four and eight workers, in both runs, the process CPU
time of the candidate equals its wall time: `large_helper` 42.65 ms wall and
42.63 ms CPU in the second rerun, 52.29 and 52.29 in the third; `fir` 6.88
and 6.88; `small_split` 211.1 and 211.1. Today's `par` arm, whose helpers
work and spin, shows their CPU plainly (84 ms CPU for 21 ms wall on
`large_helper` at four workers). So during the measured sample the helpers
are parked and execute nothing: they do not share a core with the main
thread, do not read its deque lines, and nothing writes the request word.
`hot_helper` and `recursion`, which run with the same parked helpers and
poll at every group call, sit at 0.99 to 1.01. The whole difference between
a one-worker cell and a four-worker cell is therefore which code the main
thread runs: the sequential clone at one worker, the demand body at four.

A cell that runs the demand body with no helper threads at all would close
the question beyond doubt: a one-lane pool (the runtime starts no worker and
world selection still enters the demand body). It needs a runtime setting the
scheduler does not have (`wf__par_start` refuses fewer than two lanes) and is
not implemented here; its prediction is that it equals the four- and
eight-worker demand cells within the twin's noise. Add it only if a later run
shows a demand cell whose CPU exceeds its wall.

### What each workload paid, run by run

**`large_helper`, second rerun, 1.065.** The hot loop is the out-of-line
slice driver's. Its slice loop (`run3/demand/large_helper.o.s`, `.LBB11_8` at
line 610 to `.LBB11_15` at 606) runs per slice: the span test, a
`callq wf__par_demand_requested@PLT` (line 622) with the register shuffling
around it, the minimum and end computation (`.LBB11_10`, 627), the
runtime-unroll prologue (`.LBB11_12`, 640) and then the same four-way body as
the sequential build (`.LBB11_14`, 656, against the sequential `wf_helper`
`.LBB0_4` at line 24). With weight 8 a slice is 625 iterations, so each
million-iteration helper call runs 1,600 slices: 320,000 slices per
repetition at about 40 cycles each over 200 million one-cycle iterations is
6.4 percent, which is the measured 6.5. The accessor itself is eleven
instructions in the linked image (`large_helper.disassembly` at `0x171c0`: a
thread-local flag test, a thread-local pointer load, a null test and the
word load, around a push and pop), and the call forces the caller to move
its live values into callee-saved registers and reload them after. The
static price put a slice at "about 5 microseconds"; it was 125 nanoseconds.
The IR weight overestimates this body's cost per iteration about eightfold.

**`large_helper`, third rerun, 1.305.** The caller-local slice loop is
rolled. `wf_workload` in `run4/demand/large_helper.o.s` computes the slice
end as a literal (`leaq 18750(%rbp), %rax`, line 525) and its inner loop
`.LBB10_17` (527 to 534) is one iteration per trip: `rolq; imulq; addq;
incq; cmpq; jb`, where the sequential `wf_helper` loop (`.LBB0_4`, 24 to 44)
and the caller's inlined copy (`.LBB1_5`, 104 to 125) run four iterations
per trip. The second slice loop (`.LBB10_32`, 639) is the same, and only the
final remainder loop (`.LBB10_23`, 573), which runs once per helper call, is
unrolled. A slice of 18,750 iterations fifty-three times per call puts 99.4
percent of the iterations in the rolled loop, and the measured 30 percent is
the rolled loop's cost per iteration. The cause is the shape the candidate
chose: a constant trip count. LLVM's runtime unroller serves loops whose trip
count it does not know, which is what the sequential loop and the second
rerun's driver loop had; a loop that LLVM knows runs exactly 18,750 times is
neither runtime-unrolled nor partially unrolled at this optimization level,
and 18,750 is not a multiple of four. The polls themselves had become cheap
(53 per million iterations) and were not the cost.

**`small_split`, second rerun, 1.126.** `mark` and its chunk inline into the
walker. Against the sequential walker (`run3/seq/small_split.o.s`, the
vectorized `.LBB3_21` at 265 and scalar `.LBB3_23` at 282), the demand walker
(`run3/demand/small_split.o.s`, 423 to 435) reloads the Box slot from the
stack on every call (`movq 8(%rsp), %rax` before the length compare), forms
the span, saturates it (`cmovaeq`, line 433), compares it with 21,428 and
branches, then runs the same three stores. The reload is the cost of the cold
hand-out path: the driver receives the address of the `cells` slot, so the
slot escapes and LLVM cannot keep the Box pointer in a register as the
sequential build does. A sequential `mark` call is about five cycles
(187 ms over 200 million calls), so a few instructions and one dependent
load are 12 percent.

**`small_split`, third rerun, 1.26.** Worse because the caller-local slice
machinery raised register pressure in the walker itself:
`run4/demand/small_split.o.s` spills before the span test on every call
(`movq %r8, 40(%rsp)` at 589 and the stores around it) and reloads three
registers after the one-slice path (`.LBB12_25`, 595 to 598), whether or not
the range is tiny. The compare itself is not the problem; the spills around
it are.

**`fir`.** In the second rerun the chunk is out of line
(`run3/demand/fir.o.s`, `wf__par_chunk_filter.1` at 1002): its inner loop
(`.LBB19_6`, 1059) is unrolled four ways into one chain of dependent
`addsd`, and the caller's `noalias` facts on the two range pointers
(`fir.ll.raw` line 229) are not on the chunk's bare pointer parameters (line
840). The sequential `wf_filter` (`run3/seq/fir.o.s`) keeps the compact
inner loop (`.LBB2_14`, 152) and unrolls the outer loop by two (`.LBB2_13`,
144, `.LBB2_16`, 171). In the third rerun the chunk inlined into `wf_filter`
and the inner loop (`run4/demand/fir.o.s`, `.LBB11_10`, 302) is instruction
for instruction the sequential one; the outer loop (`.LBB11_9`, 294) differs
by two instructions per output of sixty-four taps, which cannot explain 13
to 16 percent. `fir`'s one-worker cell, which runs the sequential clone in
both runs, moved from 0.952 to 1.000 between them, so about five percent of
any `fir` cell is code placement, and both runs' `fir` cells were
inconclusive (twin spreads 5.9 to 7.1 percent). The third rerun's `fir`
loss is unattributed; the polls are not it (one per 825 outputs).

**`spine`, 1.035 in both runs.** The recursion budget at four and eight
workers is eight levels; below the cut the sequential clone runs. The eight
budgeted levels pay the accessor call and five saved registers each, about
twenty cycles against about one for a plain level, and 8 x 20 over 4,000
levels is the measured 3.5 percent.

**`records`, 1.05 to 1.07 in the third rerun from 1.01 to 1.02.** Not
inspected to the instruction; the same constant-trip slice shape applies to
its record loop, and its cells were inconclusive (spreads 7 to 8 percent).

### The three mechanisms

1. **The poll was a call.** Eleven instructions plus call, return and the
   caller's register traffic, about 40 cycles, paid once per slice and once
   per budgeted group call. At 625-iteration slices it was all of
   `large_helper`'s loss; at 18,750 it was negligible, but `spine` and every
   group site still pay it.
2. **The slice loop did not have the sequential loop's shape.** Outlined
   with bounds and captures as parameters (runs 1 to 3), it lost the caller's
   alias and bounds facts; inlined with a literal step (run 4), it gained a
   constant trip count and lost runtime unrolling. In both cases LLVM
   compiled a different loop from the one the sequential build has.
3. **A tiny runtime extent in a hot caller pays the cold path.** Captures
   passed by address escape, and a hand-out call in the loop body raises
   register pressure in the hot caller. These are fractions of a cycle per
   call, which is 10 to 25 percent of a five-cycle call.

### Options considered

- **A. Keep the accessor call and only widen the interval.** Removes most of
  the slice polls (the third rerun already did) but leaves the loop-shape
  loss that caused that run's failure, and the group sites' cost on `spine`.
  Not taken.
- **B. One slice loop with a data-dependent length, and a poll that is one
  thread-local load.** Taken, below. It makes the no-request path the
  sequential loop over successive sub-ranges plus a compare, a minimum, an
  addition and a load per slice.
- **C. Test once at loop entry and run the sequential clone when nobody
  asks.** Exactly the sequential code, but a request arriving during a long
  loop is never answered; it gives up experiment 2 and is not taken, as
  before.
- **D. Asynchronous promotion (patchable safe points).** Would remove every
  poll instruction; needs a target-specific continuation and register-map
  design. Out of this experiment's scope, as the third change recorded.
- **E. Capture by loaded value.** When a captured reference is only loaded
  in the loop body, pass the loaded pointer instead of the slot's address, so
  the slot does not escape and the sequential register allocation survives.
  This is the general fix for mechanism 3's reload and would also improve
  today's `--par` chunks; it is a lowering change in `split.rs`'s capture
  construction with its own correctness argument (the body must not store to
  the slot, which PAR-2's independence gives), not taken in this round and
  recorded as the next step if `small_split` is to improve further.

### The fourth change, fixed before it measures

1. **The poll is a load.** The demand module defines a thread-local word
   `@wf__par_demand_word` (a weak zero definition the scheduler's strong
   `_Thread_local` replaces at link; Windows takes the external declaration),
   and every check, at a group call, in the caller's slice loop and in the
   out-of-line driver, is `load atomic i64 ... monotonic`, a compare and a
   branch: on ELF one `mov %fs:...` instruction, no call, no saved registers.
   The lane keeps only the address of its owner's word, registered when the
   owner attaches; an idle thief writes through it (write-if-zero), and
   publication clears the owner's own word. World selection
   (`wf__par_pool_active`) starts the pool and attaches the selecting thread
   in demand mode, since no poll starts anything any more. The C accessor
   `wf__par_demand_requested` remains for the native probe and returns the
   same word. The lane layout stays private; the module names one word.
2. **One slice loop, variable length.** `emit_demand_split` emits a head
   that computes `remaining = upper - cursor`, polls only when `remaining >=
   step`, and runs the chunk from one call site over `[cursor, cursor +
   min(remaining, step))`; the backedge is taken while `end < upper`. There
   is no second chunk call for a tail, so the inlined loop (the chunk is
   still `alwaysinline` in demand mode) has one copy with an unknown trip
   count, as the sequential loop has. A range below the step is one slice and
   never reads the word. `nuw` on the subtraction and addition follow from
   `cursor < upper` at the head and `count <= remaining`.
3. **The interval is unchanged**: `max(2, ceil(150,000 / weight))`
   iterations, as the third change set it; adopting it is still the owner's
   decision. At this interval the second rerun's loop-shape evidence says the
   per-slice work beyond the body is a few instructions plus the loop's
   runtime-unroll remainder, and the third rerun's says 53 polls per million
   iterations are immaterial once each is a load.

Not addressed: mechanism 3. The tiny-extent site still pays the span
compare, the minimum, the addition and the escaped capture's reload per call.
Option E would remove the reload; nothing removes the compare short of
hoisting the extent test out of the enclosing source loop, which LLVM does
not do here (the extent is loop-invariant in `small_split`'s walker, but the
range endpoints are not). **On the owner's bar:** a site whose whole
sequential cost is five cycles cannot absorb any runtime decision within two
percent. All three studies said so (the plan's "H1 as written cannot be met
literally" and its proposed per-decision-point allowance for sites with a
runtime extent); this round makes that concrete: `small_split` will not pass
the 1.02 bound under direction B or any other direction that decides at run
time, and its verdict should be read against an allowance the owner sets, or
the site must be priced statically (the extent is a runtime value here by
construction of the workload). Every other workload is predicted to pass.

### Validation boundary

`cargo check` and `cargo clippy` (`--offline --lib --tests`,
`CARGO_BUILD_JOBS=4`) pass; the changed Rust files are `rustfmt`-clean; the
embedded Whitefoot sources of the backend tests were checked with a released
compiler and compile. Nothing was built or run. CI must run the backend
tests (`par_demand.rs`: the single-call-site slice loop, the thread-local
poll and no accessor call, the driver's word read behind its span test, the
group check, ordinary-emission byte identity), the scheduler probe
`sched-demand-test` (posting through the registered address, write-if-zero,
clearing on publication, the disabled setting), the maintained program tests
at widths 1 and 4 with both settings, the legacy-identity comparison, and
then experiment 1 on the i9-14900K.

### Prediction for the fourth run

Candidate over sequential at four and eight workers with requests disabled;
one worker runs the sequential clone and is predicted near 1.00 as before.
The pass/fail rule is unchanged.

| Workload | Predicted ratio at 4 / 8 | What would falsify the diagnosis |
| --- | --- | --- |
| small_constant | 0.99–1.01 | pruning still emits no poll |
| small_split | 1.06–1.14 | expected to fail the 1.02 bound: mechanism 3 is not addressed; below 1.06 would mean the reload was not the cost, above 1.14 that the one-slice path spills |
| recursion | 0.99–1.01 | group paths unchanged, poll now a load |
| spine | 1.00–1.03 | the budgeted levels' cost falls from about 20 cycles to the budget machinery alone; above 1.03 means the call was not the cost |
| hot_helper | 0.99–1.01 | group poll a load |
| large_helper | 1.00–1.02 | the inlined slice loop must be four-way unrolled as the sequential one (inspect `wf_workload`); above 1.02 with that shape means strip-mining itself costs |
| mandelbrot | 0.99–1.02 | expensive iterations amortize everything |
| records | 0.98–1.04 | noise-bound; the slice loop shape must match the sequential record loop |
| fir | 0.98–1.06 | inner loop identical to sequential (it already was in the third rerun); the width is placement, which the one-worker cell's own swing bounds |
| stencil, prefix, histogram | inconclusive by twin spread, as in every run | an indexed fallback on prefix and histogram says nothing about slices |

### Consequence for experiment 2

A request is answered at the next slice boundary or group call, so hand-out
latency is bounded by one step of work: about 18,750 iterations (a few
microseconds) on `large_helper`, 825 outputs of 64 taps (tens of
microseconds) on `fir`. Static units overestimate these bodies' time about
eightfold, so the nominal 150,000-unit slice is far shorter than 150
microseconds here; a body whose optimized cost per unit is much lower still
(a vectorized map) would make slices shorter, which only shortens the latency
and costs a few cycles per slice. The driver, once entered, halves on demand
as before, so the speedup side of experiment 2 is unchanged by this round
except for the lower cost of each poll inside it.

## Results of the fourth rerun (the strongest model's candidate)

Run: [compute-bench 38006694877](https://github.com/Ming-Research/Whitefoot/actions/runs/38006694877),
`claude/par-demand` at 31b4a1588, i9-14900K, 10 interleaved rounds,
2026-10-10 00:00 to 00:06 UTC, same rule.

| workload | predicted, 4 and 8 workers | measured, 4 / 8 workers | best earlier (122afdc1d) |
|---|---|---|---|
| small_split | 1.06 to 1.14 | 1.799 / 1.795 (both fail: reruns 1.798, 1.800, spreads 1.1 to 1.9 percent) | 1.126 / 1.123 |
| large_helper | 1.00 to 1.02 | 1.294 / 1.294 | 1.065 / 1.063 |
| fir | 0.98 to 1.06 | 1.249 / 1.386 (spreads 40 percent) | 1.079 / 1.091 |
| spine | 1.00 to 1.03 | 1.026 / 1.037 | 1.034 / 1.035 |
| recursion, hot_helper, mandelbrot, records, small_constant | about 1.00 | within 2.3 percent | within 2.1 percent |

The candidate's process CPU equalled its wall time in every cell, which
confirms its diagnosis that parked helpers cost nothing here. Its stated
falsifier for `large_helper` holds: in `demand/large_helper.o.s` the slice
loop inlined into `wf_workload` (`bb2.i.i`) runs one `rolq`/`imulq` per trip,
not the sequential loop's four, so the variable-length slice loop did not
recover the sequential shape. Neither restructuring of the slice loop (the
third and fourth reruns) beat the second rerun's out-of-line chunk, whose
unrolled body was intact and whose loss the strongest model attributed to one
runtime call per slice.

Both escalations the owner named are spent; the next step goes back to the
owner.

## The fifth change, fixed before it measures

The owner chose (status board, 2026-10-10) to return to the second change
(122afdc1d), the best measured, and change only how a poll reads the request
word, then rerun under the same rule.

Change: the compiler-side files of the slice driver, the call-site
comparison and their tests are 122afdc1d's; the runtime keeps the request
word as the owner thread's `_Thread_local wf__par_demand_word` (from
31b4a1588: registered at attach, written by a thief through that address,
cleared by a publish), and every poll is `load atomic i64, ptr
@wf__par_demand_word monotonic` instead of a call of
`wf__par_demand_requested`. The slice loop's shape, its 5,000-unit interval
and the call-site fast path are unchanged.

Prediction at four and eight workers: `large_helper` 1.00 to 1.02, since
the second rerun's 6.5 percent was one runtime call per 625 iterations around
an unchanged four-way body; `fir` below its 1.08 to 1.09, likely inconclusive
by spread; `spine` within 2 percent; the group-call workloads unchanged near
1.00; `small_split` about 1.1, unaffected (its range never reaches a poll),
which the owner's new allowance below decides.

## The allowance for tiny decision points

The owner ruled (status board, 2026-10-10) that the bound is the wider of
two: each decision point may cost at most 1 ns or 2 percent more per
execution. For a cell whose sequential time per decision-point execution is
`t` nanoseconds, the bound at four and eight workers is `max(1.02, 1 + 1/t)`
plus `noise`; at one worker it stays within `noise` of 1. `small_split`
executes its decision point once per `mark` call, 200,000,000 times in
`T_seq` of about 188 ms, so `t` is about 0.94 ns and its bound about 2.06;
every other workload's decision points are far rarer, so their bound stays
1.02. This rule is fixed before the fifth change is measured.

## Results of the fifth rerun

Run: [compute-bench 38008068559](https://github.com/Ming-Research/Whitefoot/actions/runs/38008068559),
`claude/par-demand` at 3cc812a5a, i9-14900K, 10 interleaved rounds,
2026-10-10 00:18 to 00:23 UTC, the rule with the decision-point allowance.

No cell fails. At four and eight workers: `large_helper` 1.038 and 1.039
(above its 1.03 bound in the first attempt at a 0.7 percent spread; its rerun,
1.040, had a 6.3 percent spread and decides nothing), against 1.065 before the
poll became a load and the predicted 1.00 to 1.02; `fir` 0.957 and 0.984;
`small_split` 1.107 and 1.117, within its 2.2 allowance but undecided by its
9 to 13 percent spread; `spine` 0.985 and 1.031; `recursion` and
`hot_helper` within 1 percent; `records` 1.068 and 1.077 and `mandelbrot` up
to 1.050, both undecided by spreads of 6 to 8 percent. Process CPU equalled
wall in every demand cell.

What remains for `large_helper` is per-slice cost: at the 5,000-unit interval
its weight-8 loop slices every 625 iterations of about one cycle, so about 24
cycles of slice setup per slice give its 3.8 percent. Widening the interval
to one offer unit (150,000 units) would divide that by 30 without changing
the loop's shape, but it lengthens how long an idle worker can wait for work,
which experiment 2 measures; that choice is the owner's. No cell can pass
until the optimized-site inspection is written, and most cells stay
undecided while the twin spread on this host exceeds 2 percent: the timed
processes are not pinned to CPUs on a 32-vCPU Hyper-V guest.

## The sixth change, fixed before it measures

The owner chose (status board, 2026-10-10) to widen the slice interval from
5,000 to 150,000 work units, one offer unit, and to pin the measured
processes before rerunning under the same rule with the decision-point
allowance.

Change: `SLICE_NANOS` is 150,000; nothing else in the compiler or runtime
changes from 3cc812a5a. Measurement: `measure.py` runs each timed process
under `taskset` on the first `max(W, 1)` logical CPUs taken one per physical
core (from Linux's thread-sibling lists), the same set for every arm of that
width, and records the sets in `identity.json`; where the lists or `taskset`
are missing it records that the run was not pinned. Each summary reports the
candidate's process CPU time (all threads, user and system) beside its wall
time, as the owner asked.

Prediction at four and eight workers: `large_helper` 1.00 to 1.01; the other
workloads as in the fifth rerun; demand CPU equal to wall in every cell, since
nothing is handed out. Spreads lower than the fifth rerun's if pinning removes
migration; if they do not fall below 2 percent for most cells, the noise
measure itself goes back to the owner.

## Results of the sixth rerun

Run: [compute-bench 38015737639](https://github.com/Ming-Research/Whitefoot/actions/runs/38015737639),
`claude/par-demand` at a111b11b1, i9-14900K, 10 interleaved rounds,
2026-10-10 02:11 to 02:15 UTC, processes pinned to one logical CPU per core
(`identity.json`: {1: [0], 4: [0, 2, 4, 6], 8: [0, 2, ..., 14]}).

No cell fails. `large_helper` 0.999 at four and eight workers, as predicted,
with CPU equal to wall; `small_split` 1.105 and 1.106, within its 2.2
allowance; `recursion`, `hot_helper`, `spine`, `mandelbrot` within 1.2
percent; `fir` 0.95 and 0.98; `records` 1.065 and 1.078, as in the fifth
rerun (1.068, 1.077) and above the second rerun's 1.006 and 1.021, which
points at the poll change and is examined next. Today's `--par` at the same
work: `large_helper` 0.525 wall at 2.10 and 4.19 times the sequential CPU,
`spine` 3.48 and 4.25 wall at 13.9 and 34.0 times the CPU.

Pinning did not bring the twin spreads under 2 percent: most cells still range
from 3 to 36 percent, so most cells decide nothing and the noise measure
goes back to the owner, as stated before the run.

## The poll was still a call; the seventh change, fixed before it measures

Inspection of the sixth rerun's `demand/records.o.s` shows that the poll the
fifth change meant as one thread-local load compiled to the general-dynamic
TLS sequence, `leaq wf__par_demand_word@TLSGD(%rip)` and `callq
__tls_get_addr@PLT`: the image is position-independent and the word a weak
external, so LLVM assumed it might live in a shared library. So the fifth and
sixth reruns still polled through a call, and `large_helper`'s recovery in the
sixth came from the wider interval, not from the cheaper poll. `records`'s
chunk is instruction for instruction the same in the second and sixth
reruns' images and its driver differs only in the poll, while its 1.07
held at both intervals; the cause is not settled, and `records` is the kernel
recorded as sensitive to code placement on some hosts.

Change: the module declares the word `thread_local(initialexec)` and the
runtime defines and declares it with `tls_model("initial-exec")` (not on
Windows), valid because every image is a statically linked executable. A
clang check of the same IR at `-O2 -fPIC` gives `movq
wf__par_demand_word@GOTTPOFF(%rip), %rax; movq %fs:(%rax), %rax` with no call,
against the `__tls_get_addr` call without the model. That check compiled a
five-line IR file to assembly on the owner's MacBook (clang targeting
x86_64 Linux, no linking or running), because it decided whether the change
was worth a 14900K slot and a CI round would have taken longer than the
change; the next run's own `demand/*.o.s` images, which CI keeps, are the
evidence of record for what the poll compiled to.

Prediction at four and eight workers: `large_helper`, `small_split`,
`recursion`, `hot_helper`, `spine` as in the sixth rerun; `records` lower than
1.07 if the poll's call contributed to it, unchanged if its cost is placement.

## Results of the seventh rerun

Run: [compute-bench 38016588285](https://github.com/Ming-Research/Whitefoot/actions/runs/38016588285),
`claude/par-demand` at c893fda30, i9-14900K, 10 interleaved rounds, pinned,
2026-10-10 02:24 to 02:28 UTC. Its images confirm the poll: `records.o.s`,
`large_helper.o.s` and `spine.o.s` contain no `__tls_get_addr` and no call of
the accessor, and each poll is `movq wf__par_demand_word@GOTTPOFF(%rip)`
followed by an `%fs`-relative load.

No cell fails. Medians at four and eight workers, the candidate's CPU ratio
equal to its wall ratio within 0.4 percent in every cell: `records` 0.994 and
1.006, down from 1.065 and 1.078, so the hidden call caused its loss;
`large_helper` 0.999 and 0.999; `recursion` 1.007 and 1.009; `hot_helper`
0.983 and 0.985; `spine` 1.019 and 1.017; `mandelbrot` 1.012 and 1.008;
`fir` 0.951 and 0.957; `histogram` 0.969 and 0.950; `small_split` 1.110 and
1.170 within its 2.2 allowance; `stencil` 1.046 and 0.999 and `prefix` 1.038
and 1.043, whose spreads (17 to 20 percent) cover those medians. Most cells'
spreads still exceed 2 percent, so under the current noise measure they
decide nothing; that measure awaits the owner. The optimized-site inspection
the rule requires is the remaining step before any cell can count as a pass.

## Inspection of the seventh rerun's images

`research/experiments/par-demand/inspection-38016588285.json` records, per
workload, from the seventh rerun's optimized images, that the timed work
survives optimization in all twelve workloads and what each decision point on
its path compiled to; every surviving poll is a `GOTTPOFF` load and an
`%fs`-relative load. The inspection was written by a read-only model from the
images and checked by a second read. Two findings: `small_constant`'s site
was denied parallel admission, so the workload never exercises literal
pruning of an admitted site, and pruning is untested by this set; `prefix` and
`histogram` run demand slice drivers in these images, not the legacy splitter
an earlier note assumed for indexed reductions.

## `small_constant` corrected to exercise pruning

The inspection found that `small_constant`'s loop, which wrote
`cells^.inner[lo + j]`, was denied by PAR-2 (its computed subscript reads as
an indexed accumulator), so every run so far measured a sequential loop and
said nothing about pruning. The workload now writes `cells^.inner[i]` for
`i` in the literal range `0..3`, which the `--par` ledger of the
wf-exp-b7054cbb15dc compiler reports as "loop permitted, eligible; no
accumulator" (checked on the owner's MacBook with `whitefootc --par
--par-ledger --emit-llvm`, no build or run, to avoid a CI round for a
one-line ledger); `micro_oracle.c` has its own branch for it, matching a
direct C transcription at five sizes. The next run's ledger and
`demand/small_constant.o.s` are the evidence of record that the site is
admitted and pruned. Earlier runs' `small_constant` cells stand as
measurements of a denied loop.

## Where today's `--par` spends its extra CPU (fixed before it measures)

This is a side measurement for experiment 2, not part of experiment 1's
verdict. In the seventh rerun, ordinary `--par` (the `par` arm) used far more
process CPU than the sequential build for several workloads, without a
matching wall-time gain. Dividing the CPU ratio by the wall ratio gives the
average number of threads on a CPU: `large_helper` 4.00 and 8.00 at four and
eight workers with a 1.9x speedup at both, `spine` 3.99 and 8.01 with wall
3.5x and 4.4x the sequential build, `stencil` 2.75 and 4.57 with speedups 1.71
and 2.16, `small_split` 1.00 with wall 5.4x. The seventh rerun's samples also
show that each process's first timed call, and only the first, pays CPU above
its wall time in the candidate: about 3.4 ms at four workers and 8.5 ms at
eight in most workloads, 23 ms (`hot_helper`) and 16 ms (`stencil`) at eight.

**Question.** Of the extra CPU, how much is (a) idle lanes spinning through
the 1,000 us idle window (`WF_PAR_IDLE_WINDOW_US` in
`compiler/src/backend/sched/core.c`), (b) extra instructions spent handing
work out, and (c) the same instructions running slower on more cores, through
memory traffic or contention?

**Comparison.** Three arms at four and eight workers, pinned one CPU per core
as in the experiment: `seq`, `par`, and `nospin`, which links `par`'s object
against `core.c` compiled with `-DWF_PAR_IDLE_WINDOW_US=0`, so an idle lane
parks after 1,024 spin rounds (about 12 us by the file's own probe) instead of
spinning for up to a millisecond. Nothing else differs. Ten interleaved rounds
time both calls of each process with the experiment's runner; the second
call is the steady state, the first carries the pool start. A separate batch
of three rounds runs each process under `perf stat` with whatever counters
the host grants without changing its settings (`perf_event_paranoid` is
recorded, not changed); counts are whole-process, so they are read only as
differences from `seq`, whose preparation and checking are identical. One
round runs `par` and `nospin` with `WF_SCHED_REPORT=2`, which prints the
process's total steal count at exit.

Reading, per workload and width, from second-call medians: (a) is
`par` CPU minus `nospin` CPU, over `seq` CPU; (b) is `nospin` user
instructions minus `seq` user instructions, over `seq`; (c) is `nospin`
cycles per user instruction over `seq`'s. Where the host grants no hardware
counters, (b) and (c) stay unseparated and the result says so.

**Predictions that would reject the reading given to the owner.**
- `large_helper`: the extra CPU is spinning. Rejected if `nospin`'s CPU
  ratio exceeds 1.5 at eight workers or 1.3 at four (`par` reads 4.19 and
  2.10), or if its wall ratio is more than 10 percent above `par`'s.
- `spine`: the extra CPU is hand-out work with every lane busy, not
  spinning. Rejected if `nospin` removes more than 20 percent of `par`'s CPU.
- `small_split`: helpers sleep and the cost is the main thread's per-call
  entry. Rejected if `nospin` differs from `par` by more than 5 percent in
  wall or CPU, or if the process steals more than once per thousand calls.
- `recursion`: healthy stealing. Rejected if `nospin` moves its CPU by more
  than 5 percent.
- Pool start: the first call's CPU above wall is the idle window. Rejected
  if `nospin` still shows more than 1 ms of it at eight workers.
- `stencil`, `fir`, `prefix`, `histogram`, `mandelbrot` and `records` carry
  no prediction; the split is what this measures.

The arm, driver and job are temporary: `cpu_split.py`, the `cpu-split`
targets in the experiment's Makefile and the `par-cpu-split` job of
`compute-bench.yml` are removed in the commit that records the result, which
names the revision that held them.

## Results of the CPU breakdown

Run: [compute-bench 38031476878](https://github.com/Ming-Research/Whitefoot/actions/runs/38031476878),
`claude/par-demand` at 81330087b (which held the temporary arm, driver and
job), i9-14900K, built and measured on that host, 10 interleaved rounds,
pinned one CPU per core, 2026-10-10 06:35 to 06:41 UTC. Medians of the second
call; ratios against the same width's `seq`. The host's `perf_event_paranoid`
reads -1, yet every hardware event (`cycles`, `instructions`, user and
kernel) reads `<not supported>`: the Hyper-V guest exposes no performance
counters. Only software counters were recorded, so (b) extra hand-out
instructions and (c) slower instructions on more cores stay unseparated.

| Workload | W | `par` wall | `par` CPU | `nospin` wall | `nospin` CPU | Spin share (`par` − `nospin` CPU, over `seq` CPU) | Steals per process (`par`) |
|---|---|---|---|---|---|---|---|
| `large_helper` | 4 | 0.525 | 2.098 | 0.544 | 1.475 | 0.62 | 1,380 |
| `large_helper` | 8 | 0.525 | 4.193 | 0.544 | 2.348 | 1.85 | 2,921 |
| `spine` | 4 | 3.464 | 13.947 | 3.049 | 12.056 | 1.89 | 136,698 |
| `spine` | 8 | 4.277 | 33.664 | 3.838 | 30.062 | 3.60 | 156,560 |
| `small_split` | 4 | 5.717 | 5.717 | 5.713 | 5.713 | 0.00 | 0 (no worker started) |
| `small_split` | 8 | 5.648 | 5.648 | 5.638 | 5.637 | 0.01 | 0 (no worker started) |
| `recursion` | 4 | 0.272 | 1.105 | 0.279 | 1.067 | 0.04 | 30 |
| `recursion` | 8 | 0.140 | 1.109 | 0.148 | 1.099 | 0.01 | 80 |
| `stencil` | 4 | 0.559 | 1.548 | 0.562 | 1.528 | 0.02 | 259 |
| `stencil` | 8 | 0.464 | 2.112 | 0.482 | 2.150 | -0.04 | 983 |
| `mandelbrot` | 8 | 0.270 | 1.676 | 0.262 | 1.250 | 0.43 | 42 |
| `records` | 8 | 0.313 | 1.888 | 0.357 | 1.289 | 0.60 | 72 |
| `prefix` | 8 | 0.974 | 3.304 | 0.886 | 0.886 | 2.42 | 57 |
| `histogram` | 8 | 0.929 | 1.433 | 0.962 | 1.156 | 0.28 | 45 |

Against the predictions:

- `large_helper`: **rejected.** `nospin` reads 1.475 at four workers and
  2.348 at eight, above the 1.3 and 1.5 bounds; its wall is 3.6 percent above
  `par`'s. Spinning in the window is 57 to 58 percent of the extra CPU, not
  nearly all of it. The steal counts also contradict the reading that at most
  about two lanes work: each of the 400 helper calls a process makes is
  stolen from about 3.5 times at four workers and 7.3 at eight, so the loop
  is handed out, and the 1.9x ceiling at both widths has another cause, not
  yet identified. `nospin`'s context switches (527 and 1,422 per process
  against 9 and 21) show its lanes park and wake around every call.
- `spine`: holds. `nospin` removes 14 and 11 percent of the CPU; the rest is
  hand-out work with every lane busy, about 5 to 8 steals per call.
- `small_split`: holds, and more sharply than predicted: the pool never
  starts (`workers_started=0`), so the 5.7x cost is entirely the code the
  `--par` build runs on the main thread.
- `recursion`: holds; `nospin` moves its CPU by 3.4 and 0.9 percent.
- Pool start: the design could not test it cleanly, because the `par` and
  `nospin` arms hand work out during the first call too. As the first call's
  CPU above wall minus the second call's, the five kernels read 5 to 22 ms in
  `par` and 0.55 to 0.94 ms in `nospin` at eight workers, except `histogram`
  at 1.58 ms, which exceeds the 1 ms bound. The candidate's own start-up
  cost needs the candidate linked against the zero-window core.
- No prediction: spinning is most of the extra CPU at eight workers for
  `prefix` (all), `records`, `mandelbrot` and `histogram` (63 to 67
  percent), and none of it for `stencil`, whose extra CPU is hand-out or
  memory cost the counters could not separate. Removing the window costs
  wall on several of them (`records` W=8 0.313 to 0.357, `mandelbrot` W=4
  0.274 to 0.309), the trade the window was chosen for.

Found along the way: the corrected `small_constant` folds completely in the
sequential build (1.3 us per call) and runs 886 ms in the `--par` build with
no worker started, so ordinary `--par` currently blocks an optimization the
sequential build performs. Experiment 1's next run measures whether the
demand build does the same.

The temporary arm, driver and job are removed in the commit that records this
section; 81330087b holds them.

### Two causes read from the IR afterwards

Both readings come from `whitefootc --par --emit-llvm` output of the
wf-exp-b7054cbb15dc compiler and the runtime source; nothing was built or
run for them. They are deductions that match the measurements, not yet
separately tested.

**`large_helper`'s 1.9x ceiling.** The workload's pair publishes the second
`helper` call and then runs the first on the publishing lane
(`par.offer` → `wf__par_publish`, then `call @wf_helper`). The first call's
loop asks `wf__par_split_budget`, which returns 0 whenever the asking lane's
own deque is not empty (`sched/core.c`, the `bottom - top > 0` test), and it
is not: the sibling was just pushed. So the first helper always runs its
whole million-iteration loop unsplit, while a thief takes the sibling and
splits it across the remaining lanes. Each repetition then lasts about one
helper's sequential time, half the sequential build's, at any width: wall
0.525 at four and eight workers. The rule declines to split when the lane
already has queued work, which assumes that work can occupy the other lanes;
a single coarse sibling occupies one. Demand-driven hand-out offers the first
helper's loop whenever a lane asks, whatever its deque holds, so experiment 2
should show `large_helper` pass 0.5 under the candidate; a chunk trace
(`WF_PAR_TRACE`) of ordinary `--par` would test the reading directly.

**`small_constant` and `small_split` under ordinary `--par`.** Every call of
`mark` calls `wf__par_split_budget`, defined in the separately compiled
runtime and opaque to the optimizer, then the recursive splitter, which can
publish a frame holding the cells pointer to the runtime. The recursive
splitter cannot be inlined and the pointer escapes into it, so the three
stores per call stay behind calls and the repetition loop cannot fold, even
though the pool never starts. That is the 886 ms against 1.3 us, and the
same per-call path is `small_split`'s 5.7x. The candidate's call site
compares the span with a compile-time minimum (`emit_demand_split`) and calls
the non-recursive chunk directly below it, so for the literal range `0..3`
the comparison folds and the chunk can inline; the next experiment-1 run's
`demand/small_constant.o.s` and its cell test that.

## The paired noise rule, fixed before re-judging

The owner chose (status board, card on how noise is measured, 2026-10-10) to
replace the max/min spread with a paired confidence interval, and to re-judge
the earlier runs with it. This section fixes the rule before any run is
re-judged; it supersedes the "Inconclusive" and `noise` clauses of
[Pass and fail](#pass-and-fail-fixed-before-measuring) and of the per-width
revision, and keeps everything else, including the decision-point allowance
and the inspection requirement.

For one cell (workload, width, attempt), each round `r` gives the paired ratio
`q_r = candidate wall_r / sequential wall_r`, from the second call of each
process. The statistic is the median of the `q_r`; its 95 percent interval is
the 2.5th and 97.5th percentiles of 10,000 bootstrap medians, each drawn by
resampling the rounds with replacement, with a fixed seed so the verdict is
reproducible. The bound is `max(1.02, 1 + decisions × 1 ns / T_seq)` at four and
eight workers; at one worker, where the adapter runs the sequential clone, the
band `[0.98, 1.02]` keeps the earlier rule's two sides. Noise no longer widens
either, because the interval carries it.

- Pass: the interval's upper end is at or below the bound (at one worker, the
  whole interval lies in the band).
- Exceeds: its lower end is above the bound (at one worker, the whole interval
  lies outside the band); one rerun follows, and a second exceeding interval
  fails the cell.
- Otherwise the cell is inconclusive.
- Machine control: the same interval over `twin wall_r / candidate wall_r`
  must contain 1; otherwise the cell is void, because two byte-identical images
  disagreed. The card's text named the twin against the sequential build;
  since the twin is a copy of the candidate, only twin against candidate
  tests the machine, and the board records this reading for the owner.

The rule is applied unchanged to the fifth, sixth and seventh reruns'
measurements as recorded; nothing is re-measured.

### Re-judged under the paired rule

The seventh rerun ([compute-bench 38016588285](https://github.com/Ming-Research/Whitefoot/actions/runs/38016588285),
c893fda30, the current code, with its own inspection
`inspection-38016588285.json`) at four and eight workers: **15 pass, 9
inconclusive, none exceeds, none void.**

- Pass: `small_constant` (the old, denied loop), `small_split` (1.106 and
  1.159, within its 2.07 allowance), `recursion` W=4, `hot_helper`,
  `large_helper` (1.000 [0.999, 1.002] and 0.999 [0.997, 1.001]),
  `mandelbrot`, `records` W=4, `fir` (0.950 and 0.954), `histogram` W=8.
- Inconclusive, interval straddling 1.02: `spine` (W=8 1.032 [1.011, 1.035]),
  `recursion` W=8 (1.009 [1.007, 1.023]), `records` W=8, `stencil`, `prefix`,
  `histogram` W=4.
- One worker: five pass; `fir` exceeds the band on the fast side (0.946
  [0.941, 0.961]: the candidate image runs its sequential clone faster than the
  sequential build, a code-placement difference rather than a cost); `records`
  and `histogram` are void (their twins disagree with the candidate); the rest
  straddle.

For context only, since their images are not the inspected ones: the sixth
rerun (a111b11b1) reads 17 pass, 5 inconclusive and `records` exceeding at
both widths (1.064 [1.054, 1.071], 1.076), the hidden `__tls_get_addr` call
the seventh change removed; the fifth (3cc812a5a, the 5,000-unit interval)
reads `large_helper` W=4 failing (1.037 [1.035, 1.041] in both attempts),
which the sixth change's interval removed.

So no cell of the current code exceeds its bound, and the cells still
undecided need narrower intervals: more rounds on the next run, which also
measures the corrected `small_constant`.

## Results of the eighth rerun (30 rounds)

Run: [compute-bench 38034318254](https://github.com/Ming-Research/Whitefoot/actions/runs/38034318254),
`claude/par-demand` at 665b64ff7, i9-14900K, 30 interleaved rounds, pinned,
2026-10-10 07:34 to 07:47 UTC, judged by the paired rule with this run's own
inspection `inspection-38034318254.json` (written by a read-only model from
the run's images). At four and eight workers: **19 pass, 4 inconclusive, 1
fail.**

- Fail: `spine` W=8, 1.034 [1.029, 1.038], rerun [1.028, 1.038]; W=4 is
  inconclusive at 1.028 with a rerun interval [1.019, 1.035]. Cause, from
  `demand/spine.o.s` against `seq/spine.o.s`: the sequential clone turns the
  accumulating recursion into a loop, while `wf__par_budget_spine`, which the
  candidate enters at W>1 with the runtime budget (9 levels at eight workers,
  `log2(8 × 64)`), keeps each budgeted level a real call that saves five
  registers, reads the request word and decrements the budget. At about 0.9
  us per `spine` call, 32 ns more is the 3.4 percent. One worker runs the
  sequential clone and reads 0.997. The prototype kept the static recursion
  budget by design ([The prototype](#the-prototype), item 2), so this
  attributes the failure to that budget clone's shape, not to the request
  check; recursion on demand is stage 4. The owner decides how to proceed
  (status board card on the spine result).
- `small_constant`, corrected: its literal loop is now admitted, and the
  candidate's ledger records the site as pruned. Both the sequential build
  and the candidate fold the whole repetition loop (about 1.2 us per call);
  today's `--par` takes 738 ms. So the call-site pruning removes the lost
  optimization found in the CPU breakdown. Its ratios are timer noise around
  a microsecond, so the cell is inconclusive and, with its hot work
  optimized away in both builds, would not count as a pass in any case.
- Pass at both widths: `small_split` (1.110, within 2.07), `recursion`,
  `hot_helper`, `large_helper`, `mandelbrot`, `records`, `fir`, `stencil`,
  `histogram`; `prefix` passes at W=8 and is inconclusive at W=4.
- One worker: seven pass; `fir` again runs its sequential clone 5 percent
  faster than the sequential build (0.947), outside the band on the fast
  side; four straddle.

## Experiment 2, fixed before it measures

The owner chose (status board, 2026-10-10) to continue to experiment 2 with
`spine`'s recursion handed to stage 4, to add H3, and to compare the chosen
idle-wait policy against today's in this experiment.

**Question.** When idle workers do ask, does demand-driven hand-out keep
today's `--par` speedups while staying within H1 and H3, and does the
idle-wait policy the owner chose (at most one idle worker spins, the others
park at once, the spin length adapts to recent waits) cut wasted CPU without
losing wall time?

**Arms.** `seq`; `par` (today's `--par` with the shipped runtime); `demand`
(the candidate with `WF_PAR_DEMAND=on`, today's 1 ms idle window); `idle1`
(the same candidate object linked against the runtime built with the new
idle policy, so the comparison is same-source); `twin` (a byte copy of
`demand`). Workloads: the twelve of experiment 1. Widths 1, 4 and 8, each
process pinned to one logical CPU per performance core (on the native
14900K, CPUs 0, 2, ..., 14; the harness refuses to run if a chosen CPU is not
a performance core). Thirty interleaved rounds; second call of each process,
as before; first-call CPU above wall is reported separately as start-up.

**Round count, fixed before the sample.** The 14900K is shared, so a
six-round sample on it runs first and judges nothing. From its spread, the
decisive run's round count `n` (at most thirty) is chosen as the smallest
count at which, by the sample's per-cell spread of `q_r`, every cell's
interval would be narrower than its distance to its bound or no wider than
0.02. Only the decisive run's rounds are judged; the sample's are reported
beside them and never pooled, so stopping early cannot select a verdict.

**Rules, per cell at four and eight workers, by the paired bootstrap interval
of experiment 1** (`q_r` per round, median, 95 percent interval, fixed seed;
a disagreeing twin voids the cell):

- E2-H1: `demand / seq` wall upper end at most the experiment-1 bound
  (`max(1.02, 1 + decisions × 1 ns / T_seq)`); the same for `idle1`.
- E2-keep: where today's `par` is faster than `seq` (the `par / seq` wall
  interval lies below 1), `demand / par` wall upper end at most 1.05; the
  same for `idle1`.
- E2-H3: per round, `m_r = (cpu_arm − 1.1 × cpu_seq − 0.1 × max(0, wall_seq −
  wall_arm) × W) / cpu_seq`; the cell passes when the upper end of the
  interval of the median `m_r` is at most 0, for `demand` and for `idle1`; the
  same quantity is reported for `par` for comparison.
- E2-idle: `idle1 / demand` wall upper end at most 1.02, with the CPU change
  reported beside it.
- An interval wholly above its bound exceeds; one rerun follows; a second
  exceeding interval fails the cell. Otherwise the cell is inconclusive.
- `spine` is measured and reported but decides nothing here (stage 4 owns
  recursion); `small_constant` and any cell whose timed work is optimized
  away in both builds decide nothing.
- The optimized-code inspection of experiment 1 is repeated for the images
  this run builds.

**What would reject the direction as built.** A failing E2-keep cell on a
workload where today's `par` speeds up means demand-driven hand-out loses
speedup there (hand-out latency or granularity); a failing E2-H1 cell means
the asking path costs more than its bound; a failing E2-H3 cell for `idle1`
means waiting workers still burn CPU that buys no time. Each goes back to
the owner with its attribution before any further building.

**Implementation note, recorded before the run.** The `idle1` arm's runtime
differs from `demand`'s in two places, both compiled only under
`WF_PAR_IDLE_SINGLE_SPINNER`: the idle spin policy above, and the join
wait's park, which in `idle1` announces the joining lane as idle before its
final scan (as a worker does) so that a publish can wake it to help; a lane
that loses the spinner slot parks at once, and without that announcement a
parked joiner could not be given other work. So E2-idle compares the policy
as built, both changes together, not the spin rule alone.

### A crash at exit in the demand prototype

The hosted build of experiment 2 ([compute-bench run 38051814229](https://github.com/Ming-Research/Whitefoot/actions/runs/38051814229))
stopped when `twin/records verify` died with SIGSEGV at eight workers on a
four-CPU runner; `demand/records`, the same bytes, had passed just before. A
repetition job on the same runner type (temporary workflow, run
38052790854) passed `seq` and `par` 360 times each at widths 4, 8 and 16 and
caught the crash in `demand` at width 8: a worker faulted in `wf.par_find`
while the main thread was already in `exit()`. The cause is the prototype's
request word. A thief writes the lane owner's thread-local word through the
address the owner registered; lane 0 belongs to whichever thread attached
first, here the program's own thread that `wf_floor` starts and joins before
`exit()`, so a worker still scanning wrote into that thread's freed storage.
Production `--par` has no such address and is not affected.

The fix withdraws the address at the owner's thread exit (a POSIX
thread-specific destructor) and waits until no thief that read it before the
withdrawal is still inside `wf__par_request`; thieves count themselves in and
out around the read and store. That adds two atomic operations to every failed
scan of the `demand` and `idle1` arms, on the idle path only, so experiment 2
measures the demand arm with this cost and experiment 1's demand arm without
it. The scheduler probe now checks that an exited owner's word is withdrawn.

With the fix, the same repetition on the same runner type
(run 38053140359, revision 39d2ec9a5) ran `records verify` 150 times for each
of `demand`, `idle1` and `twin`, at 8 and 16 workers, with requests on and
off: 1,800 runs, no failure. Before it, `demand` crashed within its first 60
runs at 8 workers, and the compute-bench verify within its first few.

## Experiment 2's sample: hot_helper slows down eleven to fifteen times

The six-round sample ([compute-bench run 38054220812](https://github.com/Ming-Research/Whitefoot/actions/runs/38054220812),
revision ddebcb4be, native 14900K, one logical CPU per performance core) judges
nothing by the round-count rule; it is reported because one cell is far
outside every other. `hot_helper` (median second-call wall, ms, six rounds):

| Width | seq | par | demand | idle1 | twin |
|---:|---:|---:|---:|---:|---:|
| 4 | 151.9 | 151.8 | 2,340 | 2,573 | 2,356 |
| 8 | 151.9 | 151.8 | 1,768 | 1,969 | 1,843 |

Process CPU at width 4 is 9.36 s for `demand` against 0.152 s for `seq`, so
every lane is busy. In experiment 1's thirty rounds (run 38034318254, the
14900K before its move from a Hyper-V guest to native Ubuntu) the same cell
passed at about 1.00. `hot_helper` runs a statement group of two independent
calls of a helper whose work is at most 31 rotations (about 15 ns), ten
million times; `par` never splits it.

**Attribution, fixed before it runs.** Question: is the slowdown hand-out of
the group's call on nearly every iteration because idle lanes re-request at
once, or the exit fix's counted request (two atomic operations per failed
scan, on the victim's request line, which the owner's publish also reads)?
Comparison, `hot_helper` at width 4 on the 14900K, five processes each, the
scheduler report on (`WF_SCHED_REPORT=2`, steals per lane):

- A: `demand` as built here, four performance cores;
- B: `demand` linked with experiment 1's scheduler (75f91b6ba, before the
  exit fix and the idle policy), four performance cores;
- C: A with requests off (`WF_PAR_DEMAND=off-never-request`);
- D: `par`, four performance cores;
- E: A on two performance and two efficiency cores, where the idle window is
  withheld (more than one performance level).

Reading: if B is as slow as A, the exit fix is not the cause; if C is near
`seq`, requests are; steals near ten million in A mean a hand-out per
iteration; E near `seq` would tie the slowdown to idle lanes that spin. If A
is slow and B is not, the exit fix's counting is the cause and is replaced.

**Attribution result** (temporary workflow, run 38055303611, 14900K,
13:22–13:24 UTC; second-call wall and process CPU, steals over the process):

| Arm | Wall, ms (5 processes) | CPU, ms | Steals |
|---|---|---|---|
| A `demand`, 4 P-cores | 1,707 / 2,579 / 2,468 / 2,736 / 2,418 | 6,699–10,940 | 9.1–12.5 million |
| B experiment 1's scheduler | 149 (one process); four died with SIGSEGV at exit | 149 | 4.7 million |
| C requests off | 149.3–149.5 | equal to wall | 0 |
| D `par` | 151.8–151.9 | equal to wall | 0 |
| E `demand`, 2 P- and 2 E-cores (no idle window) | 149 / 149 / 2,509 / 295 / 2,622 | 149–10,491 | 4.2–11.6 million |
| F `idle1`, 4 P-cores (three shown) | 2,497 / 2,339 / 2,751 | 9,355–11,004 | 10.2–12.1 million |

Reading, by the rule above: C equals `seq`, so the requests cause the
slowdown; A's steals are about one per iteration of the ten-million-iteration
loop, so a request is honoured by handing out a call of about 15 ns at
every poll; E is bimodal, so the slow state is self-sustaining once entered
and does not need the idle window, only idle lanes that ask again as soon as
a steal finishes. B's four crashes are the exit fault above on native
hardware (it reproduced here in four of five processes), so B leaves the
exit fix's share unmeasured; A against E's fast processes, the same image,
shows the fix does not force the slow state.

**What this rejects.** As built, demand-driven hand-out honours every
request whatever the call's cost, so a group of calls cheaper than one
hand-out turns into one hand-out per iteration whenever lanes are idle:
E2-H1 fails by eleven to fifteen times, where today's `par` never splits.
This is the failure the plan's rejection paragraph sends back to the
direction card; the hand-out needs a grain floor, a cost below which a
request is not honoured.

### The grain floor: demand takes `--par`'s call grain

The prototype lowered `--par-demand` with every permitted call offer kept
(`CallGrain::Every`), so a request could hand out any call; `--par` keeps a
call offer only when its callee reaches recursion that offers its own calls
or its static work reaches the work unit that prices a range split
(`CallGrain::WorkUnit`). `--par-demand` now takes that same grain by default,
and `--par-call-grain off` restores experiment 1's every-offer lowering. A
request then decides when a statically eligible call is handed out, never
whether a call below the work unit is: `hot_helper`'s group is pruned and its
helper no longer polls (`demand_takes_the_par_call_grain_so_a_cheap_group_never_polls`).

This is the smallest floor that closes the witness, and it rests on the same
static estimates `--par` uses, so it cannot be worse than `--par` at choosing
call offers; a per-site floor learned from measured task durations (bounded
re-probing, suppression after short samples) is the candidate if a workload
shows the static estimate handing out cheap calls or withholding costly ones.
Loop slices keep their own floor (a range below the minimum span never polls);
that floor is checked on the whole remaining range, so a far half can be as
small as half the minimum span, about 75,000 estimated units, well above a
hand-out, and is left as it is.

**Experiment 2 is rerun with this lowering**, under the rules and round-count
rule above, unchanged: the six-round sample's arms were built with every offer
kept, so its rounds are not reused. Prediction, fixed now: `hot_helper`'s
`demand / seq` wall interval lies within 1.02 at four and eight workers, and
no other cell's `demand` or `idle1` result moves outside its sample interval
except through fewer hand-outs of calls below the work unit.

### Experiment 2 with the call grain: the six-round sample

[Compute-bench run 38058755664](https://github.com/Ming-Research/Whitefoot/actions/runs/38058755664)
(revision 037c2add2, native 14900K, 14:16–14:20 UTC, one logical CPU per
performance core) judges nothing by the round-count rule. The prediction
fixed before it holds for `hot_helper`: `demand / seq` and `idle1 / seq` wall
are 1.000 at four and eight workers, intervals within 1.001. Cells whose
six-round interval already lies wholly beyond a bound, to be decided by the
decisive run:

- E2-keep (`demand / par` where `par` speeds up, bound 1.05): `mandelbrot`
  at eight workers 1.176 [1.124, 1.244], `stencil` at eight 1.123
  [1.083, 1.144], `recursion` at eight 1.093 [1.059, 1.153]; `idle1` is
  further out on `mandelbrot` (1.122 at four, 1.396 at eight) and `stencil`
  at eight (1.192).
- E2-H3 (CPU beyond 1.1 × sequential plus a tenth of the saved wall per
  worker, bound 0): `stencil` at four and eight for both arms.
- E2-idle (`idle1 / demand`, bound 1.02): `mandelbrot` at eight 1.191,
  `fir` at eight 1.041.

`histogram` at eight is void: its twin disagreed. By the rule, every cell's
interval is narrower than its distance to its bound only well beyond thirty
rounds for several cells, so the decisive run takes thirty.

### Experiment 2's decisive run (thirty rounds)

[Compute-bench run 38059295009](https://github.com/Ming-Research/Whitefoot/actions/runs/38059295009)
(revision 21c56e36f, the lowering of 037c2add2; native 14900K, 14:30–14:47
UTC; widths 1, 4 and 8, one logical CPU per performance core; thirty
interleaved rounds, then one rerun of every exceeding cell, which the
harness ran: 1,500 second-attempt rows). Columns are each rule's median and
95 percent paired-bootstrap interval: wall ratios (E2-H1 against `seq`,
E2-keep against `par` where `par` speeds up, E2-idle `idle1 / demand`) and
the CPU margin `m` of E2-H3 (at most 0 passes). "Inconclusive" here includes
cells whose intervals pass: by the rule, a pass also needs this run's
optimized-code inspection, not yet done. Fails need none.

| Workload | W | demand / seq | demand / par | demand CPU m | idle1 / par | idle1 CPU m | idle1 / demand | Verdict |
|---|---:|---|---|---|---|---|---|---|
| small_split | 4 | 1.113 [1.095, 1.134] | – | 0.013 [-0.005, 0.034] | – | -0.003 [-0.005, 0.021] | 0.999 [0.976, 1.004] | inconclusive |
| small_split | 8 | 1.104 [1.096, 1.167] | – | 0.005 [-0.004, 0.067] | – | -0.004 [-0.006, 0.015] | 1.001 [0.991, 1.003] | inconclusive |
| recursion | 4 | 0.272 [0.256, 0.280] | 1.050 [1.031, 1.079] | -0.311 [-0.366, -0.273] | 1.057 [1.028, 1.084] | -0.340 [-0.399, -0.326] | 1.006 [0.987, 1.032] | inconclusive |
| recursion | 8 | 0.137 [0.134, 0.143] | 1.080 [1.052, 1.099] | -0.706 [-0.738, -0.641] | 1.093 [1.057, 1.128] | -0.716 [-0.743, -0.671] | 1.017 [0.976, 1.042] | fail (keep-idle1) |
| hot_helper | 4 | 1.000 [1.000, 1.000] | – | -0.100 [-0.100, -0.100] | – | -0.100 [-0.100, -0.100] | 1.000 [1.000, 1.000] | inconclusive |
| hot_helper | 8 | 1.000 [1.000, 1.000] | – | -0.100 [-0.100, -0.100] | – | -0.100 [-0.100, -0.100] | 1.000 [1.000, 1.000] | inconclusive |
| large_helper | 4 | 0.256 [0.256, 0.257] | 0.516 [0.515, 0.516] | -0.392 [-0.393, -0.335] | 0.515 [0.514, 0.515] | -0.392 [-0.394, -0.379] | 0.998 [0.997, 1.001] | inconclusive |
| large_helper | 8 | 0.138 [0.138, 0.138] | 0.277 [0.277, 0.277] | -0.635 [-0.707, -0.633] | 0.277 [0.277, 0.278] | -0.645 [-0.776, -0.634] | 1.001 [0.998, 1.004] | inconclusive |
| mandelbrot | 4 | 0.280 [0.275, 0.288] | 1.081 [1.050, 1.112] | -0.204 [-0.319, -0.194] | 1.128 [1.065, 1.149] | -0.298 [-0.327, -0.270] | 1.026 [0.993, 1.071] | fail (keep-idle1) |
| mandelbrot | 8 | 0.167 [0.163, 0.168] | 1.215 [1.198, 1.243] | -0.527 [-0.537, -0.515] | 1.273 [1.248, 1.309] | -0.654 [-0.698, -0.624] | 1.058 [1.010, 1.092] | fail (keep-demand, keep-idle1) |
| records | 4 | 0.269 [0.268, 0.272] | 0.987 [0.981, 0.994] | -0.436 [-0.559, -0.360] | 0.982 [0.977, 0.985] | -0.562 [-0.588, -0.517] | 0.993 [0.990, 0.999] | inconclusive |
| records | 8 | 0.153 [0.150, 0.157] | 1.014 [0.986, 1.048] | -0.592 [-1.053, -0.482] | 1.017 [0.989, 1.027] | -0.959 [-1.114, -0.907] | 0.990 [0.971, 1.004] | inconclusive |
| fir | 4 | 0.249 [0.248, 0.250] | 0.656 [0.652, 0.658] | -0.348 [-0.350, -0.346] | 0.662 [0.656, 0.665] | -0.535 [-0.610, -0.509] | 1.006 [1.005, 1.008] | inconclusive |
| fir | 8 | 0.131 [0.130, 0.131] | 0.502 [0.497, 0.509] | -0.727 [-0.730, -0.725] | 0.524 [0.516, 0.535] | -1.241 [-1.347, -1.015] | 1.040 [1.014, 1.054] | inconclusive |
| stencil | 4 | 0.571 [0.566, 0.577] | 1.035 [1.029, 1.055] | 0.257 [0.245, 0.290] | 1.067 [1.055, 1.077] | 0.196 [0.164, 0.223] | 1.025 [1.014, 1.038] | fail (H3-demand, keep-idle1, H3-idle1) |
| stencil | 8 | 0.539 [0.530, 0.548] | 1.119 [1.105, 1.132] | 0.991 [0.919, 1.074] | 1.170 [1.155, 1.193] | 0.377 [0.365, 0.411] | 1.044 [1.031, 1.063] | void |
| prefix | 4 | 0.862 [0.844, 0.873] | 1.020 [0.987, 1.024] | -0.250 [-0.283, -0.215] | 1.004 [0.985, 1.038] | -0.300 [-0.312, -0.252] | 0.999 [0.988, 1.020] | inconclusive |
| prefix | 8 | 0.840 [0.816, 0.854] | 1.023 [1.017, 1.040] | -0.323 [-0.411, 0.105] | 1.030 [1.005, 1.049] | -0.382 [-0.441, -0.349] | 0.997 [0.980, 1.012] | inconclusive |
| histogram | 4 | 0.889 [0.874, 0.921] | 0.999 [0.985, 1.017] | 0.005 [-0.025, 0.052] | 1.012 [0.989, 1.029] | -0.090 [-0.100, -0.045] | 1.013 [0.996, 1.035] | inconclusive |
| histogram | 8 | 0.865 [0.838, 0.908] | 1.021 [1.002, 1.029] | 0.152 [0.085, 0.230] | 1.006 [0.982, 1.027] | -0.138 [-0.179, -0.098] | 0.993 [0.971, 1.012] | fail (H3-demand) |


**What this rejects**, by the rules fixed before measuring:

- Demand-driven hand-out as built loses speedup where today's `par` has it:
  `mandelbrot` at eight workers is 21.5 percent slower than `par` after the
  rerun (E2-keep fails); `recursion` and `stencil` lie above 1.05 at eight
  workers too but only `idle1` fails there, and `stencil` at eight is void
  (its twin disagreed).
- The single-spinner idle policy (`idle1`) is not an improvement: it fails
  E2-keep on `recursion` at eight, `mandelbrot` at four and eight and
  `stencil` at four, and nowhere passes E2-idle by a margin; today's idle
  policy stays.
- Waiting workers still burn CPU that buys no time on `stencil` at four
  (both arms) and `histogram` at eight (`demand`), so E2-H3 fails there.

The rejection paragraph sends each of these back to the direction card. The
fixed points the run does give: `hot_helper` is now 1.000 against `seq` at
both widths, `large_helper`, `records`, `fir` and `prefix` keep or improve
on `par`, and every cell passes E2-H1 against its bound. That bound is
not the never-slower bar: `small_split`, a loop of many cheap decisions, is
10 to 11 percent slower than `seq` at four and eight workers (bound about
1.90 from its decision count), so a loop whose slices are all cheap still
pays for its polls.

## Experiment 3: attribution of experiment 2's failures, fixed before it measures

The direction card after experiment 2 is open; this batch serves its
recommended option (the [hybrid plan](hybrid-plan.md)) and its alternative
of a second independent plan alike, and judges no direction. Each arm
changes one thing against `demand` as built at 9324e6551, with `demand`'s
twin as the noise control, under the paired rule of experiment 1 (thirty
rounds after a six-round sizing sample, widths 4 and 8, one logical CPU per
performance core on the native 14900K):

| Arm | The one change | Question | Rejects the cause when |
|---|---|---|---|
| `order` | a request publishes the near half and the owner runs the far half, as `par`'s splitter does | does publication order explain `mandelbrot`'s loss (its costly points are in the last quarter)? | `order / demand` on `mandelbrot` at 8 has an interval containing 1 |
| `seed` | a counted loop that passes the minimum span publishes a bounded first frontier (the existing 16 chunks per lane ceiling, of at least the minimum span each) before its first slice, then refines on request as now | is delayed exposure of work the cause on `mandelbrot`, `recursion` and `stencil`? | `seed / demand` intervals contain 1 on those cells |
| `extent` | demand's slice drivers price a slice with the runtime extent estimate `par` uses (`split.work`) instead of the static weight | does coarse static pricing cause `stencil`'s loss and `histogram`'s sequential merge? | `extent / demand` intervals contain 1 on `stencil` and `histogram`, wall and CPU |
| `dedup` | a failed scan counts itself into a victim's request only when the victim's word is zero, read before the count | does the exit fix's counted request cost the idle CPU of E2-H3? | `dedup`'s CPU margin interval on `stencil` at 4 and `histogram` at 8 overlaps `demand`'s |

Each arm also runs once with per-lane counters (requests attempted and
posted, publications, steals, parks, spin and park time), in a separate
instrumented build that decides nothing; timed arms carry no instrument.
`small_split`'s remaining 10 to 11 percent is attributed by inspecting the
decisive image's machine code, not by an arm. The batch ends with a table
per cause: supported, rejected or undecided, and the hybrid plan's changes
whose cause it rejects are dropped from the plan before any is built.

### Experiment 3 implementation boundary

The arms are selected by the research-only `--par-demand-ablation
order|seed|extent` option (omitting it keeps the demand lowering unchanged,
which the CLI and shape tests check) and, for `dedup` and the diagnostic
counters, by the experiment Makefile's runtime macros
`WF_PAR_DEMAND_DEDUP` and `WF_PAR_DEMAND_COUNTERS`, never compiled into an
ordinary or `demand` object (the identity check compares both against
fadbc866b). `seed` affects counted loops only, so loop-free recursion is a
negative control here, not a test of eager recursive offers.

**`dedup`, amended before any measurement.** The registered form read the
victim's word before counting itself in, but that read is not protected:
the owner can withdraw the address, see no counted thief and exit between
the thief's load of the address and its read of the word, which is then
freed thread-local storage. The arm instead keeps a `request_posted` flag in
the lane, which outlives every owner: a thief reads it uncounted and skips
the counted request when it is set; the thief that posts sets it; the
owner's publication clears it after clearing its word. The word is touched
only inside the counted section, as before. A stale flag only skips or
repeats a request, which is advice. The question and the rejection rule are
unchanged.

The reducer reports, per workload and width, each arm's paired ratio to
`demand` and its E2-H3 margin, the twin's ratio (void if it excludes 1), and
the per-cause table. For `extent`'s joint wall and CPU criterion, both
intervals containing 1 reject the cause, both wholly below 1 support it, and
mixed results stay undecided; `dedup` is supported only by disjoint, lower
CPU-margin intervals. The six-round sizing projection scales interval width
by `sqrt(6/n)`, and the decisive reducer requires the saved sample and its
selected round count.

## Experiment 3's results

[Compute-bench run 38063795788](https://github.com/Ming-Research/Whitefoot/actions/runs/38063795788)
(revision 5a6f57277; native 14900K, 15:32–15:51 UTC; one logical CPU per
performance core). The harness ran the six-round sample, which by the rule
selected thirty rounds, then the thirty decisive rounds, not pooled with
the sample; counter builds ran once per arm and decide nothing. Every twin
interval contains 1, so every cell is valid. Wall ratios are each
arm ÷ `demand` (below 1 is faster), median and 95 percent interval;
`par` is shown as `par ÷ demand` for reference.

| Workload | W | `par` | `order` | `seed` | `extent` | `dedup` |
|---|---:|---|---|---|---|---|
| mandelbrot | 4 | 0.944 | 1.061 [1.037, 1.075] | 0.961 [0.937, 0.984] | 0.952 [0.921, 0.955] | 1.001 [0.994, 1.032] |
| mandelbrot | 8 | 0.836 | 1.081 [1.063, 1.101] | 0.912 [0.903, 0.918] | 0.838 [0.831, 0.848] | 1.011 [1.000, 1.035] |
| stencil | 4 | 0.960 | 1.000 [0.990, 1.016] | 0.973 [0.962, 0.993] | 0.947 [0.935, 0.961] | 1.010 [0.990, 1.019] |
| stencil | 8 | 0.892 | 0.994 [0.983, 1.001] | 0.957 [0.948, 0.973] | 0.888 [0.883, 0.896] | 0.996 [0.986, 1.013] |
| recursion | 4 | 0.943 | 1.001 [0.978, 1.027] | 0.982 [0.969, 1.019] | 0.992 [0.969, 1.026] | 0.991 [0.960, 1.012] |
| recursion | 8 | 0.938 | 1.037 [0.978, 1.075] | 1.018 [0.990, 1.049] | 1.017 [0.980, 1.041] | 1.005 [0.979, 1.042] |
| histogram | 8 | 0.996 | 1.007 [0.965, 1.036] | 0.987 [0.949, 1.014] | 0.990 [0.961, 1.049] | 0.992 [0.966, 1.025] |
| records | 8 | 0.980 | 0.993 [0.977, 1.021] | 3.579 [3.488, 3.693] | 1.002 [0.990, 1.020] | 1.004 [0.967, 1.057] |
| fir | 8 | 1.995 | 1.004 [0.999, 1.006] | 3.706 [3.699, 3.708] | 0.996 [0.993, 0.997] | 1.000 [0.998, 1.003] |

E2-H3 CPU margin (at most 0 passes), `demand` against `extent`: `stencil`
at four +0.284 against +0.186, at eight +1.064 against +0.605; `histogram`
at eight +0.240 against +0.176.

**Per cause, by the registered rule:**

- `order` on `mandelbrot` at eight: undecided by the rule (its interval
  excludes 1, but in the worsening direction): publishing the near half and
  running the far half is 6 to 8 percent slower. Publication order is not
  the cause; the change is dropped.
- `seed` on `mandelbrot` and `stencil`: supported (4 to 9 percent faster
  than `demand`), on `recursion` rejected. It is not adoptable as built: it
  makes `records` and `fir` at eight 3.6 to 3.7 times slower, its counter
  build parking workers on `fir` (14 parks, 16 ms) where `demand` parks
  none; the cause of that is not yet attributed.
- `extent` on `stencil`: supported, reaching `par`'s wall at eight (0.888
  against 0.892) and passing it at four; on `histogram`: rejected.
  Exploratory, beyond the registration: it also closes `mandelbrot`'s whole
  loss to `par` (0.838 against 0.836 at eight, 0.952 against 0.944 at four)
  and changes no other cell outside its noise. Static per-iteration weights
  under-price a slice whose iterations do variable work, so demand's slices
  were too coarse to balance.
- `dedup` on `stencil` and `histogram`: rejected; the counted request does
  not cost the excess CPU.

**What stays unexplained.** `recursion` at eight is 6 percent slower than
`par` under every arm. `stencil` still fails E2-H3 with `extent`; its
counters (one process, eight workers) show demand spinning about 120 ms in
total and `extent` 22 ms, while process CPU stays about twice sequential, so
most of the excess is in the slices' own execution, consistent with memory
bandwidth shared by eight workers rather than with waiting.

## Experiment 4: the call grain and runtime-extent pricing, fixed before it measures

The owner chose option A on the direction card: keep the hybrid direction
with only changes whose attribution supports them. Demand retains `par`'s
call grain and now prices slices with the runtime-extent estimate `split.work`
by default, the `extent` arm of experiment 3. Missing estimates keep the
static weight. Publication order, eager seeding and the idle policy stay as
in experiment 2 with the call grain; dedup and counters remain research-only.
Cheap-region versioning from the hybrid plan is not yet built.

**Arms and workloads.** The twelve workloads of experiment 2, at widths 1,
4 and 8: `seq`, `par`, `demand` (the new default, requests on), `static`
(the old demand pricing, `--par-demand --par-demand-ablation static`, requests
on), and `twin` (a byte copy of `demand`). Every arm runs the same input,
repetitions and extent, checked against the independent oracle. Processes
are pinned to one logical CPU per performance core on the native 14900K.
The second call judges steady execution; first-call CPU above wall is
reported separately.

**Round count.** In the same job, first collect six full-work sizing rounds,
which judge nothing. Freeze `n` as the smallest count from 6 through 30 for
which every applicable rule's projected interval width, scaled by
`sqrt(6/n)`, is at most 0.02 or its median's distance to the bound; use 30
if none qualifies. Then collect `n` separate decisive rounds, never pooled
with sizing. Each exceeding cell gets exactly one rerun of `n` rounds.
The saved six-round measurements and frozen count are required evidence.

**Rules, per cell.** Keep experiment 1's paired rule: per-round ratios,
their median, 10,000 bootstrap draws with seed `20261010`, and the 95 percent
interval. A `twin / demand` interval excluding 1 voids the cell.

- E4-seq, literal never-slower: `demand / seq` wall upper end at most 1.00
  passes; lower end above 1.00 exceeds; otherwise inconclusive. There is
  no decision-point allowance or two-sided one-worker band.
- E4-par: where `par / seq` wall's interval lies wholly below 1,
  `demand / par` wall upper end at most 1.05 passes.
- E4-gain: where `static / seq` wall's interval lies wholly below 1,
  `demand / static` wall upper end at most 1.05 passes, preserving the
  old demand lowering's gains.
- E4-H3 keeps experiment 2's margin unchanged:
  `m_r = (cpu_demand - 1.1 * cpu_seq - 0.1 * max(0, wall_seq - wall_demand) * W) / cpu_seq`;
  the median margin's interval upper end at most 0 passes. Report the same
  margin for `par` and `static` for comparison.
- An interval's lower end above its bound exceeds. A second exceeding
  interval after the one rerun fails; disagreement between attempts or an
  interval straddling its bound stays inconclusive.
- `small_constant` and cells whose timed work is optimized away in both
  builds decide nothing. `spine` is reported and decides nothing here.
- A pass also requires optimized-code inspection of this run's images,
  identifying surviving timed work and what the decision compiled to,
  as in experiment 2. Missing inspection never becomes a pass.

**What would reject the direction as built.** A failing E4-par or E4-gain
cell rejects the claim that this lowering preserves available speedups.
A failing E4-seq cell on a workload with parallel work rejects the
never-slower claim. Each returns to the owner with its attribution before
further building; E4-H3 still reports whether extra CPU buys saved wall time.

**Predictions, fixed before measurement.** `mandelbrot` and `stencil` pass
E4-par. `small_split` fails E4-seq: its remaining 10 to 11 percent is the
per-call decision cost, which runtime-extent pricing does not remove; the
hybrid plan's cheap-region versioning that addresses it is not yet built.


## Experiment 4's results

[Compute-bench run 38091958903](https://github.com/Ming-Research/Whitefoot/actions/runs/38091958903),
`claude/par-demand` at `ff90cbdd4b398eb3a3a6b0f71c6a87757b282447`, native
i9-14900K (`MBSDESKTOP`, Linux 7.0.0-38-generic, x86-64; Clang 18.1.3),
2026-10-10 22:39–23:03 UTC. The native measurement log ends with exit 0;
both its job and the hosted build/verification job succeeded.
The saved `par-demand-results-14900k/sizing-e4/{measurements.tsv,summary.json,identity.json}`
contains six full-work rounds. Recalculation with this revision's
`summarize.py` freezes **n = 30**: no count through thirty qualifies; for
example, sizing `histogram` W=4 E4-par has interval width 0.421261, still
0.188394 after projection to thirty, against target 0.02. These rounds judge
nothing and are not pooled. The decisive TSV has 13,200 rows: thirty rounds
of all 36 cells, both calls and all five arms, plus exactly one thirty-round
rerun of each of eight exceeding cells. The frozen count, pairing, call
coverage and constant comparison counts agree with the identities and source
harness. Recomputed numerical summaries match the downloaded summaries;
four independently calculated medians and bootstrap intervals (small_split
W=4 E4-seq, recursion W=8 rerun E4-par, mandelbrot W=8 E4-par, stencil W=8
rerun E4-H3) also match exactly. All downloaded executable hashes match
`identity.json`; each twin is byte-identical to demand. All 48 inspected
workload objects byte-match `par-demand-images/par-demand-images.tar.gz`.
No downloaded image was executed.

Placement: `measurement-host.txt` identifies the 14900K; the saved
`identity.json` records the pin sets and Linux P-core topology, and this
revision's `measure.py` passes those sets to `taskset -c` for every process.
The native log confirms execution of that harness; it does not print
per-process affinity. The recorded sets are: W=1 uses CPU
`0`, W=4 uses `0,2,4,6`, W=8 uses `0,2,4,6,8,10,12,14`, one logical CPU
per P-core. CPU 0 was retained for comparability with experiments 1–3. As the
owner notes, CPU 0 handles interrupts and has larger noise than the other
P-cores; this is a placement caveat, not a reason to widen a bound or discard
an observation. No per-CPU interrupt measurement is in the artifacts.

All ratios below are dimensionless paired **second-call** wall ratios
(10,000 bootstrap draws, seed `20261010`);
H3 is the dimensionless margin defined above, normalized by same-round seq
CPU. Values are initial decisive medians [95% intervals], except the
reference H3 column, which contains only par/static medians (their full
intervals remain in `summary.json`). Verdicts use full precision, both
attempts where required, twin voiding and the optimized-code inspection
below. P = pass, F = fail, I = inconclusive, V = void, – = not applicable
or prospectively excluded. The verdict order is **seq / par / gain / H3**.
E4-par and E4-gain apply only when their respective reference/seq interval
is wholly below 1; this includes W=1 large_helper, fir and histogram.
Small_constant and spine decide nothing at every width. All numerical
evidence is in this run's `par-demand-results-14900k/{summary.json,measurements.tsv}`.

| Workload | W | demand / seq wall, median [95% interval], ideal ≤ 1.00 | demand / par wall, median [95% interval], ideal ≤ 1.05 | demand / static wall, median [95% interval], ideal ≤ 1.05 | demand H3 m, normalized to seq CPU, median [95% interval], ideal ≤ 0 | par / static H3 m, normalized to seq CPU, medians, ideal ≤ 0 | Verdict seq / par / gain / H3 |
|---|---:|---|---|---|---|---|---|
| small_constant | 1 | 0.9887 [0.9370, 1.0313] | 0.9805 [0.9657, 1.0071] | 1.0023 [0.9571, 1.0203] | -0.1144 [-0.1564, -0.0656] | -0.1012 / -0.0943 | – / – / – / – |
| small_constant | 4 | 0.9736 [0.9291, 1.0508] | 1.32e-06 [1.27e-06, 1.51e-06] | 0.9756 [0.9493, 1.0050] | -0.1311 [-0.1870, -0.0461] | 610665.7059 / -0.1079 | – / – / – / – |
| small_constant | 8 | 0.9711 [0.9104, 1.1003] | 1.37e-06 [1.31e-06, 1.5e-06] | 0.9789 [0.8632, 1.0494] | -0.1292 [-0.2533, -0.0241] | 598671.4273 / -0.0937 | – / – / – / – |
| small_split | 1 | 1.0004 [0.9999, 1.0013] | 0.9999 [0.9990, 1.0008] | 1.0000 [0.9994, 1.0004] | -0.0997 [-0.1004, -0.0990] | -0.0999 / -0.0998 | I / – / – / P |
| small_split | 4 | 1.0992 [1.0937, 1.1222] | 0.2523 [0.2503, 0.2576] | 1.0018 [0.9948, 1.0178] | -0.0037 [-0.0063, 0.0087] | 3.2574 / -0.0056 | F / – / – / I |
| small_split | 8 | 1.0962 [1.0940, 1.1020] | 0.2519 [0.2488, 0.2570] | 1.0011 [0.9975, 1.0037] | -0.0038 [-0.0056, 0.0020] | 3.2969 / -0.0052 | F / – / – / I |
| recursion | 1 | 1.0004 [0.9995, 1.0011] | 1.0001 [0.9998, 1.0007] | 0.9998 [0.9995, 1.0003] | -0.0997 [-0.1009, -0.0990] | -0.0998 / -0.0998 | I / – / – / P |
| recursion | 4 | 0.2737 [0.2682, 0.2771] | 1.0417 [1.0256, 1.0686] | 0.9875 [0.9626, 1.0099] | -0.2956 [-0.3153, -0.2814] | -0.3496 / -0.3177 | P / I / P / P |
| recursion | 8 | 0.1425 [0.1392, 0.1449] | 1.0941 [1.0743, 1.1194] | 1.0169 [1.0042, 1.0315] | -0.6354 [-0.6616, -0.6272] | -0.7518 / -0.6443 | P / F / P / P |
| spine | 1 | 0.9897 [0.9875, 0.9912] | 1.0004 [0.9995, 1.0009] | 0.9999 [0.9992, 1.0009] | -0.1107 [-0.1131, -0.1096] | -0.1110 / -0.1099 | – / – / – / – |
| spine | 4 | 2.9113 [2.8859, 2.9306] | 0.8247 [0.8137, 0.8329] | 1.0051 [0.9808, 1.0214] | 10.6322 [10.3605, 10.6695] | 13.0115 / 10.4138 | – / – / – / – |
| spine | 8 | 3.4936 [3.4424, 3.5112] | 0.8672 [0.8591, 0.8801] | 0.9939 [0.9799, 1.0134] | 26.8951 [26.3962, 27.0988] | 30.8165 / 26.9584 | – / – / – / – |
| hot_helper | 1 | 1.000052 [0.999933, 1.000106] | 1.000002 [0.999891, 1.000057] | 1.000091 [0.999974, 1.000150] | -0.099944 [-0.100087, -0.099911] | -0.1000 / -0.1001 | I / – / – / P |
| hot_helper | 4 | 0.999902 [0.999760, 1.000112] | 1.000071 [0.999943, 1.000339] | 1.000059 [0.999897, 1.000158] | -0.100093 [-0.100206, -0.099888] | -0.1002 / -0.1001 | I / – / – / P |
| hot_helper | 8 | 1.000200 [0.999999, 1.000350] | 1.000092 [0.999934, 1.000226] | 0.999940 [0.999800, 1.000071] | -0.099795 [-0.100036, -0.099654] | -0.0999 / -0.0999 | I / – / – / P |
| large_helper | 1 | 0.9952 [0.9950, 0.9954] | 0.9999 [0.9997, 1.0001] | 0.9999 [0.9997, 1.0001] | -0.1053 [-0.1055, -0.1051] | -0.1054 / -0.1054 | P / P / P / P |
| large_helper | 4 | 0.2568 [0.2566, 0.2571] | 0.5158 [0.5155, 0.5163] | 1.0004 [0.9992, 1.0014] | -0.3902 [-0.3915, -0.3318] | 0.6928 / -0.3901 | P / P / P / P |
| large_helper | 8 | 0.1381 [0.1378, 0.1382] | 0.2771 [0.2770, 0.2777] | 1.0010 [0.9988, 1.0023] | -0.7765 [-0.7781, -0.6430] | 2.4872 / -0.7779 | P / P / P / P |
| mandelbrot | 1 | 1.0025 [1.0020, 1.0033] | 1.0005 [0.9999, 1.0009] | 0.9994 [0.9987, 1.0003] | -0.0975 [-0.0982, -0.0967] | -0.0976 / -0.0968 | F / – / – / P |
| mandelbrot | 4 | 0.2609 [0.2600, 0.2617] | 1.0044 [1.0003, 1.0083] | 0.9495 [0.8976, 0.9556] | -0.4465 [-0.4479, -0.4391] | -0.4466 / -0.2022 | P / P / P / P |
| mandelbrot | 8 | 0.1378 [0.1373, 0.1383] | 1.0084 [1.0031, 1.0122] | 0.8370 [0.8150, 0.8543] | -0.5822 [-0.5844, -0.5782] | -0.5851 / -0.5366 | P / P / P / P |
| records | 1 | 1.0015 [1.0010, 1.0027] | 1.0010 [1.0002, 1.0018] | 0.9995 [0.9992, 1.0006] | -0.0983 [-0.0991, -0.0973] | -0.0994 / -0.0980 | F / – / – / P |
| records | 4 | 0.2697 [0.2672, 0.2715] | 0.9884 [0.9826, 0.9943] | 0.9988 [0.9892, 1.0064] | -0.3699 [-0.4568, -0.3561] | -0.4412 / -0.3880 | P / P / P / P |
| records | 8 | 0.1554 [0.1527, 0.1576] | 1.0241 [1.0032, 1.0397] | 1.0143 [0.9875, 1.0307] | -0.7302 [-1.0752, -0.4938] | -1.0385 / -0.8945 | P / P / P / P |
| fir | 1 | 0.9484 [0.9478, 0.9491] | 1.0004 [0.9998, 1.0011] | 0.9995 [0.9989, 1.0008] | -0.1570 [-0.1576, -0.1564] | -0.1571 / -0.1575 | P / P / P / P |
| fir | 4 | 0.2484 [0.2481, 0.2487] | 0.6540 [0.6501, 0.6578] | 0.9971 [0.9961, 0.9978] | -0.3486 [-0.3531, -0.3471] | 0.0844 / -0.3463 | P / P / P / P |
| fir | 8 | 0.1300 [0.1297, 0.1304] | 0.4964 [0.4927, 0.5054] | 0.9949 [0.9934, 0.9983] | -0.7288 [-0.7323, -0.7255] | 0.1192 / -0.7266 | P / P / P / P |
| stencil | 1 | 1.0020 [0.9969, 1.0047] | 0.9994 [0.9976, 1.0040] | 1.0000 [0.9976, 1.0024] | -0.0982 [-0.1034, -0.0956] | -0.0991 / -0.0989 | I / – / – / P |
| stencil | 4 | 0.5541 [0.5451, 0.5625] | 0.9889 [0.9799, 1.0019] | 0.9592 [0.9506, 0.9643] | 0.1849 [0.1582, 0.2252] | 0.1932 / 0.2982 | V / V / V / V |
| stencil | 8 | 0.4865 [0.4796, 0.5306] | 0.9994 [0.9960, 1.0080] | 0.8896 [0.8813, 0.8999] | 0.6169 [0.5502, 0.7987] | 0.6094 / 1.0504 | P / P / P / F |
| prefix | 1 | 1.0023 [0.9952, 1.0111] | 1.0011 [0.9957, 1.0061] | 1.0012 [0.9942, 1.0068] | -0.0977 [-0.1049, -0.0927] | -0.0999 / -0.0983 | I / – / – / P |
| prefix | 4 | 0.8573 [0.8430, 0.8743] | 1.0048 [0.9898, 1.0203] | 0.9896 [0.9750, 1.0032] | -0.2549 [-0.2810, 0.3972] | -0.2858 / -0.1975 | P / P / P / I |
| prefix | 8 | 0.8399 [0.8302, 0.8514] | 1.0176 [1.0067, 1.0296] | 0.9825 [0.9780, 0.9969] | -0.3451 [-0.3689, 0.7606] | -0.3966 / -0.2652 | P / P / P / I |
| histogram | 1 | 0.9781 [0.9710, 0.9864] | 1.0003 [0.9919, 1.0066] | 0.9983 [0.9945, 1.0042] | -0.1244 [-0.1318, -0.1150] | -0.1196 / -0.1248 | P / P / P / P |
| histogram | 4 | 0.8763 [0.8625, 0.8913] | 0.9970 [0.9809, 1.0133] | 0.9956 [0.9729, 1.0214] | -0.0127 [-0.0368, 0.0129] | -0.0190 / -0.0172 | V / V / V / V |
| histogram | 8 | 0.8920 [0.8542, 0.9159] | 1.0121 [0.9960, 1.0390] | 0.9746 [0.9496, 1.0183] | 0.2136 [0.1220, 0.2722] | 0.1755 / 0.2265 | P / P / P / I |

**Rule totals (36 cells each):** E4-seq 17 P / 4 F / 7 I / 2 V / 6 –;
E4-par 15 P / 1 F / 1 I / 2 V / 17 –; E4-gain 17 P / 0 F / 0 I / 2 V /
17 –; E4-H3 22 P / 1 F / 5 I / 2 V / 6 –. There are no missing reruns
and no inspection-dependent pass left unverified.

**The one rerun.** Initial quantities and straddles are in the table above;
these are the second attempt's median [95% interval] for each initially
exceeding rule. All other rules retain their initial verdicts unless a twin
voids the cell. A straddle does not trigger another attempt or become a pass
because a rerun for another rule happened to pass.

| Workload | W | Rerun E4-seq wall, demand/seq, ideal ≤ 1.00 | Rerun E4-par wall, demand/par, ideal ≤ 1.05 | Rerun E4-H3 m, normalized to seq CPU, ideal ≤ 0 | Disposition |
|---|---:|---|---|---|---|
| small_split | 4 | 1.0966 [1.0940, 1.1128] | – | – | seq fails; H3 still straddles 0 |
| small_split | 8 | 1.0944 [1.0899, 1.0985] | – | – | seq fails; initial H3 straddle stays inconclusive despite rerun -0.0056 [-0.0101, -0.0015] |
| recursion | 8 | – | 1.0814 [1.0641, 1.1107] | – | par fails |
| mandelbrot | 1 | 1.0029 [1.0024, 1.0034] | – | – | seq fails; no one-worker tolerance |
| records | 1 | 1.0017 [1.0006, 1.0022] | – | – | seq fails; no one-worker tolerance |
| stencil | 4 | – | – | +0.1845 [0.1528, 0.2290] | every rule void: rerun twin/demand 0.991756 [0.983806, 0.998745] |
| stencil | 8 | – | – | +0.6512 [0.5525, 0.7947] | H3 fails |
| histogram | 8 | – | – | +0.0982 [-0.0081, 0.2187] | H3 inconclusive: rerun straddles 0 |

Histogram W=4 is also void, initially: twin/demand 1.016731
[1.000368, 1.028459]. All other initial and rerun twin intervals contain 1.
Recursion W=4 E4-par and the seven E4-seq straddles receive no rerun for
those rules. Hot_helper W=8 E4-seq's lower end is 0.999999; rounding it to
1.000 must not manufacture a verdict.

**Optimized-code inspection.** The evidence here is this run's ELF objects,
read with `xcrun llvm-objdump -dr --no-show-raw-insn` for
`{seq,demand,par,static}/<workload>.o`, not earlier runs' assembly. Addresses
below identify instructions in `demand/<workload>.o`; the matched seq
objects retain the corresponding computation. The timed `wf_bench_*`
entries were followed into the selected bodies. For workloads with a retained offer, W=1 pool-active dispatch selects the
sequential clone; W=4/8 select the demand body. The grain-pruned hot_helper
and literal-pruned small_constant have no dispatch. Nonzero TLS requests lead to lane acquisition and publication,
with join/release after local work. No compiler, test or workload was built or run locally.

| Workload | Surviving timed work and compiled decision in this run |
|---|---|
| small_constant | `wf_bench_micro` at 0x410 reduces the repetition loop to final-salt arithmetic and three stores at 0x450–0x46a; seq does likewise. Allocation, zeroing and checksum remain, but the 200 million mark calls do not. Literal pruning emits no request test. Excluded by registration and optimized-away inspection. |
| small_split | `wf_workload` retains the repetition loop (exit test at 0x47d, backward jump at 0x551) and runtime-extent stores. Its saturated span calculation and `cmp $21428` at 0x4b2–0x4c8 bypass the slice driver for extent 3. The cold driver retains a guarded TLS poll; the timed tiny path pays the span decision and its surrounding live state, not that poll. |
| recursion | `wf__par_budget_fib` at 0x280 retains both recursive children and addition; the sequential clone retains recursion/accumulation. Budget exhaustion transfers to the seq clone. Above the cut, decrement precedes `GOTTPOFF`, `%fs` load, test and branch at 0x2a6–0x2b4, then hand-out. Demand and static executable hashes are identical; extent pricing does not change this call-only workload. The loss's detailed attribution remains open. |
| spine | Budgeted recursion and leaf arithmetic survive; the sequential clone is an unrolled accumulation loop. Budget/TLS request tests remain. Prospectively excluded because stage 4 owns recursion. |
| hot_helper | `wf_bench_micro` at 0x550 retains data-dependent rotate/add loops at 0x5f0 and 0x6c0 and the repetition loop (exit test at 0x57f, backward jump at 0x76c). The call grain removes the cheap sibling offer: this object has no demand-word relocation or pool-active call. This tests the grain-pruned path, not a surviving group poll. |
| large_helper | The repeated pair of helper calls remains; `wf__par_slice_helper.0` at 0x680 retains rotate-by-13/multiply/add work. The group TLS test at 0x41f, call-site span guards, slice-entry divisions and guarded `%fs` load/test/branch at 0x707–0x70e survive. |
| mandelbrot | Point stores and the floating-point escape loop at 0x960–0x998 survive. Runtime limit-derived saturated pricing at 0x40f–0x459 feeds the span/driver decision; static uses its constant price. Remaining-span guard precedes TLS load/test/branch at 0x8f7–0x905. |
| records | `wf__par_slice_summarize_records.0` at 0xac0 retains offset/UTF-8 scans and output stores. Span guards and slice divisions lead to guarded TLS load/test/branch at 0xb84–0xb92. Demand and static executable hashes are identical. |
| fir | Slice and sequential clone retain tap/input loads, ordered `mulsd`/`addsd` loops and result stores. Runtime tap-count pricing feeds the span decision; slice-entry divisions and guarded TLS load/test/branch at 0x8ae–0x8bc remain. |
| stencil | Initialization and both row-step branches retain grid stores and neighbor `addpd`/`mulpd` loops. Runtime width-derived pricing feeds outer slices; remaining-span guards precede TLS polls, e.g. initialization at 0x1aca–0x1ad8. Nested row-width guards bypass the driver for width 1024 while retaining inlined FP work. The fixture measures outer polls, not the nested row poll. |
| prefix | Vector block sums and dependent middle/output/tail scans survive. Runtime block-size pricing feeds outer slices and guarded TLS load/test/branch at 0x11a6–0x11b4; nested `sum_block`'s `cmp $29999` bypasses its driver for block size 64. |
| histogram | Key loads/division/indexed increments survive in the block body, with strided bucket sums in merging. Runtime extents feed outer block slicing and guarded TLS tests. Bucket/merge span guards bypass their drivers for the fixture's sizes; indexed scatter remains sequential. |

**First-call CPU above wall, reported separately.** These are initial-round
medians of `cpu_ns − wall_ns` in ms; signed values are retained (the clocks
have separate read boundaries), and none enters a steady-call verdict.

| Workload | demand W=1 / 4 / 8 CPU−wall, median ms, ideal 0 | par W=4 / 8 CPU−wall, median ms, ideal 0 | static W=4 / 8 CPU−wall, median ms, ideal 0 |
|---|---|---|---|
| small_constant | 0.0016 / 0.0024 / 0.0024 | -0.0542 / -0.0788 | 0.0024 / 0.0024 |
| small_split | -0.0139 / 3.3803 / 8.2187 | -0.1068 / -0.1031 | 3.4013 / 8.1885 |
| recursion | -0.0032 / 58.6776 / 72.3896 | 53.4516 / 60.7479 | 58.9268 / 71.1980 |
| spine | 0.0016 / 90.6879 / 253.5804 | 110.2206 / 292.3816 | 90.1117 / 253.6083 |
| hot_helper | -0.0077 / -0.0027 / -0.0031 | -0.0034 / -0.0027 | -0.0034 / -0.0027 |
| large_helper | -0.0027 / 35.9732 / 45.2909 | 70.0066 / 159.0723 | 35.9995 / 46.3221 |
| mandelbrot | 0.0018 / 9.4164 / 10.6356 | 8.8321 / 8.4725 | 9.9036 / 12.8402 |
| records | 0.0051 / 7.4890 / 8.3003 | 6.7880 / 7.5329 | 7.2183 / 7.7723 |
| fir | 0.0029 / 6.2211 / 9.6228 | 3.8076 / 3.8623 | 6.8987 / 10.1412 |
| stencil | 0.0050 / 68.1400 / 124.2821 | 65.6379 / 118.1590 | 72.9925 / 148.0322 |
| prefix | 0.0041 / 3.4380 / 8.1107 | 0.0643 / 0.1648 | 3.4354 / 8.0822 |
| histogram | -0.0032 / 9.9364 / 20.8326 | 6.6548 / 13.8529 | 9.8177 / 20.5029 |

**Predictions and direction.** Mandelbrot passes E4-par at W=4 and 8,
1.0044 [1.0003, 1.0083] and 1.0084 [1.0031, 1.0122]. Stencil passes at
W=8, 0.9994 [0.9960, 1.0080]; its W=4 initial ratio 0.9889
[0.9799, 1.0019] is numerically within the bound but void after the twin
check, so the prediction is only partly verified. Small_split fails E4-seq
at W=4/8 as predicted; W=1 remains inconclusive. Runtime-extent pricing
supports the speedup correction for mandelbrot and the valid stencil W=8
cell: demand/static is 0.9495 and 0.8370 on mandelbrot, 0.8896 on stencil
W=8. Every valid applicable E4-gain cell passes. Nevertheless recursion W=8
rejects preservation of all available par speedups; small_split and the
mandelbrot/records one-worker controls reject the literal never-slower claim
as built. Stencil W=8 still fails the CPU-for-saved-wall criterion. The
registration returns these failures to the owner before further building;
this result selects no additional lowering change.

**Data cautions and remaining evidence.** No missing/duplicate paired row,
changed comparison count, hash mismatch or oracle mismatch was found. The
hosted job log records thirty oracle PASS lines for each workload; the native
measurement log records no oracle error, and the source runner emits each
TSV row only after its oracle check. Logs were read through the GitHub
connector after `gh run view` could not connect. This is not a local oracle
rerun. All outliers remain in the paired bootstrap: for example, initial
prefix demand second-call CPU at W=4 ranges 1.7147–5.2940 ms (median
1.8234), at W=8 1.6844–8.7350 ms (median 1.8402), consistent with its broad,
undecided H3 intervals despite negative medians. Histogram W=8 demand
wall ranges 19.1498–29.8708 ms (median 20.9053); small_split W=4 demand
wall ranges 243.1115–269.5096 ms (median 244.8592). The excluded
small_constant's roughly microsecond demand/seq timings and enormous par
H3 margins compare folded work with surviving ordinary-par repetitions and
cannot establish pruning cost. Detailed causes of the recursion loss,
one-worker losses and CPU tails remain unverified; stencil W=4 and
histogram W=4 require new machine-qualified evidence, not reinterpretation
of these void cells. No specification, compiler, test or design-tree
commitment changes in this results-only edit.
