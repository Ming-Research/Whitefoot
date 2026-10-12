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
per-process affinity. The recorded sets are: W=1 uses CPU 0 alone,
W=4 uses `0,2,4,6`, W=8 uses `0,2,4,6,8,10,12,14`, one logical CPU
per P-core. Every W=1 cell, including both attempts of the mandelbrot and
records E4-seq failures, was therefore measured on CPU 0. CPU 0 was retained
for comparability with experiments 1–3. As the owner notes, CPU 0 handles
interrupts and has larger noise than the other P-cores; this caveat applies
at every measured width and is not a reason to widen a bound or discard an
observation. No per-CPU interrupt measurement is in the artifacts.

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


## Attribution after experiment 4

**Established observations.** Fable's independent normalized-disassembly
review found that the W=1 sequential computation in the `--par` images is
instruction-identical to seq: records has zero normalized difference;
mandelbrot differs only in register naming. Inspection of the saved ELF
objects with `xcrun llvm-objdump -dr --no-show-raw-insn` corroborates the
matching sequential kernels. The timed entries contain one pool-active
call/test/branch before selecting their sequential computation, outside the
point/record loops; dispatch does not recur per point or record. Fable's
tens-of-nanoseconds estimate for that short dispatch is an instruction-cost
estimate, not a measured dispatch latency.

Recalculated from this run's `measurements.tsv`, column 6 equal to 1 (steady
call), the initial/rerun paired median demand/seq losses are **0.2544% /
0.2913% for mandelbrot**, and **0.1498% / 0.1744% for records**. The same-round
wall differences have medians **33.30 / 38.15 µs** and **15.86 / 18.51 µs**,
respectively. These remain E4-seq failures. An image-layout effect between
separate executables is the leading hypothesis, not an established cause:
the byte-identical demand twin cannot detect a placement difference between
seq and demand/par. Experiment 5a below tests that hypothesis without a
compiler change.

Recursion W=8 demand uses **80.00 ms CPU / 10.04 ms wall** initially and
**79.85 / 9.91 ms** on rerun; par uses **72.21 / 9.21 ms** and
**72.19 / 9.20 ms**. Descriptive medians across the two attempts are
**79.95 / 10.01 ms** for demand and **72.21 / 9.21 ms** for par; attempts
remain separate for every verdict. The demand object tests the request only
at the victim's next budgeted node (`wf__par_budget_fib`, decrement then TLS
load/test/branch). Its exhausted-budget edge enters `wf__par_seq_fib`, which
has no request test. Waiting for work exposure while workers spin is thus a
plausible explanation of the extra CPU and wall time; these totals and code
shape do not measure spin time or isolate it from request traffic and
scheduling cost.

Small_split's runtime-extent decision is invariant across the repeated calls
but still inside their loop; the small path bypasses the poll. The paired
excess divided by the recorded 200,000,000 repetitions is **0.1105 / 0.1077
ns per call** at W=4 (initial/rerun), **0.1072 / 0.1052 ns** at W=8. That is
roughly **0.6 cycles per call** when converted using the host's recorded
6 GHz maximum, not a hardware-cycle measurement or a claim that it ran at
that frequency. It is consistent with paying the surviving invariant decision
and its live state on every call, not request polling.

Stencil W=8 has positive H3 margins for both arms: initial medians
**+0.6169 demand / +0.6094 par**, rerun **+0.6512 / +0.7338**. Its initial
wall medians are **34.36 ms demand / 34.28 ms par**, against **70.72 ms seq**;
CPU medians are **147.76 / 148.34 ms**, against **70.71 ms seq**. The H3
excess is therefore shared with ordinary par. Memory bandwidth limiting
scaling is the working interpretation of this stencil; these wall/CPU totals
alone do not establish bandwidth saturation.

The owner has three open cards on the
[status board](https://claude.ai/artifact/7tocXS3iUdthCLCQCMd3ip): how
never-slower is stated and checked; whether budgeted recursion keeps
request-gated offers; whether H3 gates memory-bound programs. This attribution
settles none of them.

## Experiment 5a: the layout-floor control, fixed before it measures

**Question and falsifier.** Can layout alone produce the size of the W=1
losses when exactly the same sequential WF object runs in different linked
images? Reject the image-layout explanation under the quiet-control reading
below. No compiler or runtime change is part of this experiment.

**Construction.** Use all six existing formal kernels: mandelbrot, records,
fir, stencil, prefix and histogram. They already have harnesses and same-work
oracles, without the long microbenchmark repetition loops. Arms are `seq`,
`par`, `demand`, its byte-identical `twin`, and three `seq-shifted` controls:
`seq-shift64`, `seq-shift4160`, `seq-shift65664`. The controls link an uncalled
padding object with **64, 4096+64, or 65536+128 bytes of `.text`**, respectively,
before seq's WF object; unshifted seq is the zero-padding baseline. Every
control reuses seq's exact WF, oracle, runner and native-runtime objects and
link flags; it performs no second LLVM optimization. Padding starts at
64-byte alignment and has no timed execution. The hosted build records
linked disassembly; before verification and before timing, the harness checks
that every text symbol owned by the WF object moved by exactly the specified
size. The shared object hash, image hashes, sizes and linked symbol offsets
are retained in `identity.json`. A missing/discarded/reordered padding control
fails construction, rather than producing a false quiet result.

**Placement and settings.** Only W=1 is measured. Every arm is pinned with
`taskset -c 2` to one allowed first logical sibling of a P-core, excluding
CPU 0. Linux topology, the P-core CPU mask, allowed CPUs and `{1: [2]}` are
recorded; missing P-core evidence or unavailable CPU 2 stops the run. The
14900K is required. Existing W>1 placement remains unchanged for other
experiments. Demand and twin use `WF_PAR_DEMAND=on`; seq, par and shifted seq
use `off-never-request`. Verify every selected image against the independent
oracle with requests both on and off before measurement.

**Protocol and sizing.** Interleave all arms in rotating/reversed round
order, rotate workload order, and retain both calls. Only column-6 call 1
judges the control; report first-call CPU above wall separately. Each ratio is
the median of same-round wall ratios, with **10,000 bootstrap draws, seed
20261010 and a 95% interval**. Six full-work sizing rounds on the 14900K judge
nothing and are saved under `sizing-e5a/`. Apply E4's sizing rule to each
arm/seq wall ratio: project its interval width by `sqrt(6/n)` and select the
smallest `n` from 6 through 30 with every projected width at most
`max(0.02, |median-1|)`; otherwise use 30. Freeze that count before collecting
`n` separate decisive rounds; never pool sizing observations. There is no
exceeding-bound rerun in this attribution control. The reducer requires the
saved sample and its exact frozen count. It reports wall and CPU ratios for
par and demand beside the controls, so their W=1 losses are remeasured in the
same job. A disagreeing twin voids its cell and leaves the overall reading
inconclusive; sizing itself cannot select a reading.

**Reading registered before measurement.** For each shifted-seq/seq control,
let `h = (upper-lower)/2`, `d = |median-1|`, and define its spread as
`max(h, d)` (either uncertainty or a layout displacement). Across all selected
workloads and padding sizes:

- If every control has both `h < 0.001` and `d < 0.001`, reject image layout
  as the W=1 cause; the E4 W=1 failures stand as real defects.
- If any control has spread **>= 0.0015**, read the W=1 cells as being at the
  instrument floor. This does not prove layout caused either E4 loss.
- Otherwise the attribution is inconclusive.

These readings add a layout control; they change no E4 threshold or verdict
and select no lowering fix. A global floor reading records which workloads
and placements established it, without claiming every workload has the same
layout sensitivity.

**Dispatch.** On `claude/par-demand`, dispatch `compute-bench.yml` with
`experiment=par-demand`, `par_demand_experiment=5a`,
`placement_runner=14900k`, and `placement_rounds` empty; leave other inputs
at their defaults. The hosted job builds, checks harness controls and verifies
images, then the native job sizes and measures them. No local build, test,
harness execution or timing is authorized. Construction, harness cases and
measurements remain unverified until that CI run. Keep this harness with the
investigation while its layout attribution needs reproduction; retire it
when that question no longer needs measurement.


## Experiment 5a's results

[Compute-bench run 38096360826](https://github.com/Ming-Research/Whitefoot/actions/runs/38096360826),
`claude/par-demand-5a` at `81eb205186d42495fbd6e1646523df5431147e5c`, native
i9-14900K (`MBSDESKTOP`, Linux 7.0.0-38-generic, x86-64; Clang 18.1.3).
The 14900K job ran 2026-10-10 23:53:11–23:55:30 UTC (run metadata supplied
with the downloaded results). Both sizing and decisive identities record
`{1: [2]}`: every W=1 arm used **CPU 2**. Experiment 4 used **CPU 0 alone**
at W=1, including both attempts of the mandelbrot and records E4-seq failures.
The experiments therefore differ in both the core and the run; this comparison
does not isolate either effect. The identities give matching seq, par, demand
and twin image hashes across the two experiments for all selected workloads.

**Frozen sample and verification.** Recomputing `sizing-e5a/measurements.tsv`
with the maintained `summarize.py` freezes **n = 30** under the registered
six-round sizing projection. The 504 sizing rows judge nothing and are not
pooled with the 2,520 decisive rows: thirty separate rounds, all six workloads,
all seven arms and both calls, without a rerun. Recomputed decisive quantities
and the overall reading match the downloaded `summary.json` exactly.
Every downloaded image hash matches `identity.json`; each twin is byte-identical
to demand. Sequential WF object hashes match the layout identities, whose
recorded WF text-symbol offsets differ from seq by each specified padding size.
This checks the saved records, without executing downloaded images or
independently repeating CI construction or oracle verification.

**Registered reading: inconclusive.** Records' twin/demand interval is
**0.998128 [0.995764, 0.999452]**, which excludes 1 and voids its W=1 cell.
The registered rule, “A disagreeing twin voids its cell and leaves the overall
reading inconclusive,” applies even though some controls reach the spread
criterion. There is no overall instrument-floor or quiet-control verdict.
No E4 threshold or verdict changes, and no lowering fix is selected.

All table ratios are dimensionless medians of same-round **second-call wall**
ratios, with 95% bootstrap intervals (10,000 draws, seed 20261010). Seq is the
baseline except for twin/demand, whose baseline is demand. The ideal is 1 for
shifted controls and the identical twin, and ≤ 1 for demand/seq and par/seq;
the latter are observations here. `h`, `d` and spread apply only to controls;
quiet requires both `h < 0.001` and `d < 0.001`, while spread ≥ 0.0015 is the
registered floor criterion subject to twin validity. E4 columns give initial
W=1 ratios on CPU 0, independently recomputed from its decisive TSV; they
are compared with E5a on CPU 2, without pooling. All numerical evidence is in
`{summary.json,measurements.tsv,identity.json}` and `sizing-e5a/` in this run's
downloaded results; E4 comparisons use its `par-demand-results-14900k/`.

CPU/seq ratios below are dimensionless second-call paired ratios (baseline seq,
ideal ≤ 1). First-call CPU−wall is reported separately in µs (ideal 0); its
medians follow arm order seq / par / demand / twin / shift64 / shift4160 /
shift65664 and do not judge the control.

**mandelbrot, W=1.**

| Wall ratio (named denominator is baseline) | E5a CPU 2 median [95% interval], dimensionless; ideal controls/twin = 1, demand/seq and par/seq ≤ 1 | h = (upper−lower)/2, dimensionless; ideal < 0.001 | d = abs(median−1), dimensionless; ideal < 0.001 | Spread = max(h,d), dimensionless; quiet ideal < 0.001, floor ≥ 0.0015 | E4 CPU 0 same wall ratio, median [95% interval], dimensionless; ideal ≤ 1 |
|---|---|---:|---:|---:|---|
| seq-shift64/seq | 0.998446 [0.992160, 1.001382] | 0.004611 | 0.001554 | 0.004611 | – |
| seq-shift4160/seq | 0.997448 [0.994153, 1.002178] | 0.004012 | 0.002552 | 0.004012 | – |
| seq-shift65664/seq | 0.999106 [0.994774, 1.000582] | 0.002904 | 0.000894 | 0.002904 | – |
| demand/seq | 0.997604 [0.991936, 1.001920] | – | – | – | 1.002544 [1.001952, 1.003329] |
| par/seq | 1.000095 [0.993876, 1.001786] | – | – | – | 1.002616 [1.001364, 1.003380] |
| twin/demand | 1.000884 [0.998058, 1.003137] | – | – | – | – |

CPU/seq: demand 0.996067 [0.991936, 1.000725];
par 1.000094 [0.993686, 1.001783]. First-call CPU−wall medians (µs):
1.8135 / 1.8235 / 1.7970 / 1.7345 / 1.8250 / 1.8075 / 1.8220.

**records, W=1 — void cell; observations only.**

| Wall ratio (named denominator is baseline) | E5a CPU 2 median [95% interval], dimensionless; ideal controls/twin = 1, demand/seq and par/seq ≤ 1 | h = (upper−lower)/2, dimensionless; ideal < 0.001 | d = abs(median−1), dimensionless; ideal < 0.001 | Spread = max(h,d), dimensionless; quiet ideal < 0.001, floor ≥ 0.0015 | E4 CPU 0 same wall ratio, median [95% interval], dimensionless; ideal ≤ 1 |
|---|---|---:|---:|---:|---|
| seq-shift64/seq | 0.997979 [0.997156, 0.999484] | 0.001164 | 0.002021 | 0.002021 | – |
| seq-shift4160/seq | 0.998919 [0.997623, 1.000800] | 0.001588 | 0.001081 | 0.001588 | – |
| seq-shift65664/seq | 1.000695 [0.998557, 1.002577] | 0.002010 | 0.000695 | 0.002010 | – |
| demand/seq | 1.000950 [0.999745, 1.005268] | – | – | – | 1.001498 [1.000961, 1.002727] |
| par/seq | 1.000102 [0.998107, 1.000788] | – | – | – | 1.000626 [1.000270, 1.001401] |
| twin/demand | 0.998128 [0.995764, 0.999452] | – | – | – | – |

CPU/seq: demand 1.000954 [0.999710, 1.005266];
par 1.000268 [0.999509, 1.001171]. First-call CPU−wall medians (µs):
4.7495 / 4.9210 / 4.9430 / 4.6815 / 4.5635 / 4.9815 / 4.9190.

**fir, W=1.**

| Wall ratio (named denominator is baseline) | E5a CPU 2 median [95% interval], dimensionless; ideal controls/twin = 1, demand/seq and par/seq ≤ 1 | h = (upper−lower)/2, dimensionless; ideal < 0.001 | d = abs(median−1), dimensionless; ideal < 0.001 | Spread = max(h,d), dimensionless; quiet ideal < 0.001, floor ≥ 0.0015 | E4 CPU 0 same wall ratio, median [95% interval], dimensionless; ideal ≤ 1 |
|---|---|---:|---:|---:|---|
| seq-shift64/seq | 1.000707 [0.999952, 1.001742] | 0.000895 | 0.000707 | 0.000895 | – |
| seq-shift4160/seq | 0.999869 [0.999061, 1.002862] | 0.001901 | 0.000131 | 0.001901 | – |
| seq-shift65664/seq | 1.000192 [0.999317, 1.001247] | 0.000965 | 0.000192 | 0.000965 | – |
| demand/seq | 0.956309 [0.954103, 0.957170] | – | – | – | 0.948415 [0.947836, 0.949131] |
| par/seq | 0.956249 [0.955298, 0.957548] | – | – | – | 0.948067 [0.947504, 0.949149] |
| twin/demand | 1.000139 [0.999163, 1.000902] | – | – | – | – |

CPU/seq: demand 0.956268 [0.954101, 0.957146];
par 0.956096 [0.955122, 0.957532]. First-call CPU−wall medians (µs):
2.6000 / 2.5495 / 2.5215 / 2.4955 / 2.5585 / 2.6260 / 2.5615.

**stencil, W=1.**

| Wall ratio (named denominator is baseline) | E5a CPU 2 median [95% interval], dimensionless; ideal controls/twin = 1, demand/seq and par/seq ≤ 1 | h = (upper−lower)/2, dimensionless; ideal < 0.001 | d = abs(median−1), dimensionless; ideal < 0.001 | Spread = max(h,d), dimensionless; quiet ideal < 0.001, floor ≥ 0.0015 | E4 CPU 0 same wall ratio, median [95% interval], dimensionless; ideal ≤ 1 |
|---|---|---:|---:|---:|---|
| seq-shift64/seq | 1.000281 [0.998950, 1.001390] | 0.001220 | 0.000281 | 0.001220 | – |
| seq-shift4160/seq | 1.000559 [0.998618, 1.002394] | 0.001888 | 0.000559 | 0.001888 | – |
| seq-shift65664/seq | 1.001295 [0.999962, 1.002412] | 0.001225 | 0.001295 | 0.001295 | – |
| demand/seq | 1.000972 [0.999808, 1.002083] | – | – | – | 1.002030 [0.996893, 1.004693] |
| par/seq | 1.000279 [0.999707, 1.002809] | – | – | – | 1.000956 [0.996803, 1.003859] |
| twin/demand | 0.999714 [0.998726, 1.000660] | – | – | – | – |

CPU/seq: demand 1.000548 [0.999903, 1.001607];
par 1.000310 [0.999714, 1.002875]. First-call CPU−wall medians (µs):
4.4250 / 4.4280 / 4.8175 / 2.2625 / 4.4015 / 4.6230 / 3.3025.

**prefix, W=1.**

| Wall ratio (named denominator is baseline) | E5a CPU 2 median [95% interval], dimensionless; ideal controls/twin = 1, demand/seq and par/seq ≤ 1 | h = (upper−lower)/2, dimensionless; ideal < 0.001 | d = abs(median−1), dimensionless; ideal < 0.001 | Spread = max(h,d), dimensionless; quiet ideal < 0.001, floor ≥ 0.0015 | E4 CPU 0 same wall ratio, median [95% interval], dimensionless; ideal ≤ 1 |
|---|---|---:|---:|---:|---|
| seq-shift64/seq | 0.999723 [0.995603, 1.004419] | 0.004408 | 0.000277 | 0.004408 | – |
| seq-shift4160/seq | 0.999263 [0.993312, 1.004139] | 0.005413 | 0.000737 | 0.005413 | – |
| seq-shift65664/seq | 1.002369 [0.994525, 1.006287] | 0.005881 | 0.002369 | 0.005881 | – |
| demand/seq | 1.001887 [0.994488, 1.007151] | – | – | – | 1.002314 [0.995216, 1.011101] |
| par/seq | 0.997610 [0.991612, 1.004748] | – | – | – | 1.000053 [0.991769, 1.009214] |
| twin/demand | 0.997908 [0.993007, 1.000253] | – | – | – | – |

CPU/seq: demand 1.002419 [0.994915, 1.005408];
par 0.996760 [0.992882, 1.004215]. First-call CPU−wall medians (µs):
3.6520 / 3.7820 / 3.7505 / 3.7050 / 3.8335 / 3.6845 / 3.7240.

**histogram, W=1.**

| Wall ratio (named denominator is baseline) | E5a CPU 2 median [95% interval], dimensionless; ideal controls/twin = 1, demand/seq and par/seq ≤ 1 | h = (upper−lower)/2, dimensionless; ideal < 0.001 | d = abs(median−1), dimensionless; ideal < 0.001 | Spread = max(h,d), dimensionless; quiet ideal < 0.001, floor ≥ 0.0015 | E4 CPU 0 same wall ratio, median [95% interval], dimensionless; ideal ≤ 1 |
|---|---|---:|---:|---:|---|
| seq-shift64/seq | 0.996883 [0.990956, 1.004459] | 0.006751 | 0.003117 | 0.006751 | – |
| seq-shift4160/seq | 1.000617 [0.997401, 1.005238] | 0.003918 | 0.000617 | 0.003918 | – |
| seq-shift65664/seq | 1.002081 [0.999074, 1.003415] | 0.002170 | 0.002081 | 0.002170 | – |
| demand/seq | 0.982690 [0.979628, 0.986413] | – | – | – | 0.978058 [0.970988, 0.986388] |
| par/seq | 0.985039 [0.977496, 0.988885] | – | – | – | 0.981913 [0.973347, 0.985246] |
| twin/demand | 0.994230 [0.991198, 1.001736] | – | – | – | – |

CPU/seq: demand 0.982691 [0.979626, 0.986834];
par 0.985038 [0.977695, 0.988928]. First-call CPU−wall medians (µs):
-2.6255 / -3.0365 / -2.9225 / -3.1375 / -3.2150 / -3.3165 / -3.5060.

**Control observations, separate from the overall reading.** Fourteen of
eighteen controls have spread ≥ 0.0015: all three shifts for mandelbrot,
records, prefix and histogram, plus shift4160 for fir and stencil. The records
controls belong to the void cell. Only records shift64 excludes 1:
0.997979 [0.997156, 0.999484]. All other control intervals contain 1.
The largest observed spread is histogram shift64, 0.006751, from its
half-width; it is uncertainty, not evidence of a layout displacement.

**W=1 comparison observations.** Mandelbrot demand/seq is now 0.997604
[0.991936, 1.001920], against E4's initial 1.002544 [1.001952, 1.003329];
par/seq is 1.000095 [0.993876, 1.001786], against 1.002616
[1.001364, 1.003380]. Records demand/seq is 1.000950 [0.999745, 1.005268],
against 1.001498 [1.000961, 1.002727]; par/seq is 1.000102
[0.998107, 1.000788], against 1.000626 [1.000270, 1.001401].
These CPU 2 intervals contain 1; records remains void. Fir and histogram
demand/seq and par/seq intervals remain wholly below 1; stencil and prefix
contain 1 in both runs. The tables give every comparison. The changed core
and run prevent attributing these differences to removal of CPU 0 noise or
to layout. The W=1 cause remains unresolved; the E4 failures retain their
registered verdicts.


## Experiment 5c: cheap-region versioning, fixed before it measures

**Question.** Does selecting an invariant cheap region's sequential clone once
at entry remove small_split's repeated decision cost, while preserving every
protected workload's wall time? The attribution after experiment 4 identified
an invariant extent decision inside its repeated walker, with a small path
that already bypassed polling. Versioning is now implemented under default
`--par-demand`; `--par-demand-ablation unversioned` disables only that
function-body region selection. Pricing, slice drivers, call grain, recursion
budget, publication order, request protocol and native runtime remain default
demand behavior. This is a same-source ablation, not a language change.

**Arms and workloads.** Use all twelve experiment-4 workloads at W=1, 4 and 8:
small_constant, small_split, recursion, spine, hot_helper, large_helper,
mandelbrot, records, fir, stencil, prefix and histogram. Arms are `seq` (no
parallel flag), `par` (`--par`), `demand` (default versioned `--par-demand`),
`unversioned` (`--par-demand --par-demand-ablation unversioned`), and `twin`
(a byte copy of demand). Demand, unversioned and twin use requests on; seq
and par use off-never-request. Hosted CI builds and verifies every arm at
every width against the existing independent oracle, with requests both on
and off. Every arm uses the same recorded full input, repetitions and extent;
twin equality is required before measurement.

**Placement.** Require the 14900K, with one logical CPU per P-core and no
process pinned to CPU 0: W1 `{2}`, W4 `{2,4,6,8}`, W8
`{2,4,6,8,10,12,14,1}`. CPU 1 is CPU 0's sibling: W8 shares CPU 0's physical
core, so excluding logical CPU 0 does not isolate that physical core from its
activity. Before sizing, validate the exact sets against the P-core mask,
online CPUs, allowed affinity and reciprocal sibling topology; reject missing
evidence, SMT duplication or a different topology rather than substitute a
CPU. Record the source topology, selected sets and that W8 sharing in
`identity.json`, in both sizing and decisive evidence.

**Protocol and round count.** Use E4's protocol: rotate/reverse workload,
width and arm order, retain both calls and judge the second call only, reporting
first-call CPU above wall separately. In the same 14900K job, collect six
full-work sizing rounds that judge nothing. Freeze `n` as the smallest count
from 6 through 30 for which every applicable rule's projected interval width,
scaled by `sqrt(6/n)`, is at most `max(0.02, |median-bound|)`; use 30 if none
qualifies. Apply that rule to E4-seq, E4-par, E4-H3 and E5c-keep, including
keep on reported controls. Save the sample under `sizing-e5c/` and collect
`n` separate decisive rounds, never pooled with sizing. Every exceeding cell
gets exactly one rerun of `n` rounds, with all five arms interleaved. Saved
six-round measurements and the exact frozen count are required evidence.

**Rules, per cell.** Keep E4's paired per-round ratios, their medians,
10,000 bootstrap draws with seed `20261010`, and 95% intervals. A
`twin / demand` interval excluding 1 voids the cell. Retain the following
E4 rules exactly:

- E4-seq, literal never-slower: `demand / seq` wall upper end at most 1.00
  passes; lower end above 1.00 exceeds; otherwise inconclusive. There is
  no decision-point allowance or two-sided one-worker band.
- E4-par: where `par / seq` wall's interval lies wholly below 1,
  `demand / par` wall upper end at most 1.05 passes.
- E4-H3 keeps experiment 2's margin unchanged:
  `m_r = (cpu_demand - 1.1 * cpu_seq - 0.1 * max(0, wall_seq - wall_demand) * W) / cpu_seq`;
  the median margin's interval upper end at most 0 passes. Report the same
  margin for par and unversioned for comparison.
- E5c-keep: `demand / unversioned` wall upper end at most 1.05 in every cell
  passes; versioning must not cost a protected workload. This rule has no
  reference-speedup condition and includes small_constant and spine.
- An interval's lower end above its bound exceeds. A second exceeding
  interval after the one rerun fails; disagreement between attempts or an
  interval straddling its bound stays inconclusive. Either attempt's
  disagreeing twin voids the cell.
- As in E4, small_constant, spine and cells whose timed work is optimized away
  in both builds decide nothing for the E4 rules. E5c-keep still reports its
  literal comparison on those cells. Sizing never supplies a passing verdict.
- A pass requires this run's optimized-image inspection, identifying surviving
  timed work and what the decision compiled to. Compare normalized disassembly
  of **each versioned sequential clone against its corresponding seq body**,
  retaining the comparison even when it is not identical. Inventory every
  region in the demand LLVM image so a comparison cannot silently omit a
  versioned clone. For small_split, inspect the walker for absence of any
  per-iteration scheduling decision, and confirm that unversioned retains it.
  Missing inspection never becomes a pass.

**Attribution falsifier.** If small_split W4/W8 demand/seq remains above
1.00 (lower end > 1.00), although image inspection shows its walker contains
no per-iteration decision, the decision was not the whole cause. Use the
one-rerun rule to report a persistent failing cell as falsifying that
attribution; retain both attempts' intervals and the inspection, including
an initial loss that the rerun does not confirm. A missing inspection or
void twin leaves attribution undecided.

**Predictions, fixed before measurement.** Small_split W4/W8 pass E4-seq
or land within the layout spread 5a observed. That is a prediction beside
the literal verdict, not an allowance: 5a measured kernels at W1 and did not
measure a small_split layout control. Records and fir are unchanged by
versioning in the protected sense: E5c-keep passes at every width.

The owner's open card on how never-slower is stated and checked may change
how E4-seq cells inside the layout floor are read. This experiment reports
both the **literal verdict** and the **5a observed layout spread** beside
each W1/E4-seq cell, without choosing between them or changing a threshold.
Report the workload's three shifted-seq spreads where measured, and the
panel's largest observed spread (0.006751); mark micros as unmeasured and
records' 5a cell as void. The overall 5a reading remains inconclusive. These
historical W1 controls establish neither a per-workload allowance for micros
nor a W4/W8 layout floor.

**Dispatch and pending evidence.** After the owner commits these edits on
`claude/par-demand`, select `experiment=par-demand`,
`par_demand_experiment=5c`, `placement_runner=14900k`, and empty
`placement_rounds` in compute-bench; leave other inputs at defaults. The
hosted job constructs/verifies and runs harness controls, then the 14900K
job sizes and measures. Using the supplied approximately 35-minute E4 run
at four-arm scale, five arms imply about **44 minutes** at equal round and
rerun scale (`35 * 5 / 4`), excluding hosted construction and queue time.
Sizing and the number of exceeding cells can change that estimate. No 5c
build, test, harness execution or measurement has run locally; new compiler
shape checks, construction, topology, image comparisons and results await CI
and image review. Retain the harness while this attribution needs reproduction;
retire it with the investigation when that question ends.


## Experiment 5c's results

[Compute-bench run 38100091671](https://github.com/Ming-Research/Whitefoot/actions/runs/38100091671),
`claude/par-demand` at `19733d3551e5e8aedf907a05363b235178e2f89e`, native
i9-14900K (`MBSDESKTOP`, Linux 7.0.0-38-generic, x86-64; Clang 18.1.3,
native runtime `-O2 -falign-functions=64`). The 14900K job ran
2026-10-11 01:00:25–01:21:27 UTC (run metadata supplied with the artifacts).
This analysis reads downloaded data and disassembles downloaded objects; it
neither executes those images nor independently repeats CI's construction,
oracle verification or compiler checks.

**Placement and sample.** Both decisive and sizing `identity.json` record
W1 `{2}`, W4 `{2,4,6,8}`, W8 `{2,4,6,8,10,12,14,1}`. The saved P-core mask is
`0-15`; all selected CPUs are allowed, online and on distinct physical cores.
The saved reciprocal sibling lists are `2-3`, `4-5`, `6-7`, `8-9`, `10-11`,
`12-13`, `14-15`, and **`0-1`**. W8 therefore uses **CPU 1, CPU 0's sibling**:
no process is pinned to logical CPU 0, but its physical core is shared with
CPU 0 activity. The pinned sets match the registered placement and this
revision's `measure.py` applies them through `taskset -c`; the identity is
saved configuration evidence, not a per-process affinity trace.

Recomputing `sizing-e5c/measurements.tsv` with this revision's `summarize.py`
freezes **n = 30** under the registered six-round projection. The **2,160**
sizing rows judge nothing and are not pooled with the **12,300** decisive
rows: thirty rounds of all 36 cells, five arms and both calls, plus exactly
one thirty-round rerun of recursion W8, hot_helper W1, stencil W4/W8 and
histogram W8. There is no missing required rerun. Pairing, both-call coverage
and constant comparison counts pass the reducer's data checks. Every initial
and rerun quantity exactly matches downloaded `summary.json`; an independent
TSV reducer also reproduces every median and interval for small_split W1/4/8
E4-seq, recursion W4/W8 E4-par, hot_helper W1 E4-seq (both attempts),
mandelbrot/records W1 E4-seq, and stencil/histogram W1/4/8 H3 (both attempts
where present). All 60 executable hashes match the identity; all twelve twins
are byte-identical to demand. All 48 workload objects match the hosted
`par-demand-images/par-demand-images.tar.gz` bundle, as do all 96 workload
LLVM and linked-disassembly files used for inventory and path inspection.

Evidence is this run's `par-demand-results-14900k/{measurements.tsv,summary.json,identity.json}`,
`sizing-e5c/`, and `par-demand-images`; the normalized image comparisons were
made from those objects with `llvm-objdump -dr`, normalizing addresses,
relocation addends and clone callee names. The downloaded summary
has no image-inspection evidence, so it withholds passes. The verdicts below
add the inspection recorded here and preserve every numerical bound, twin
void and rerun disagreement.

**Literal verdicts.** Each ratio is a dimensionless median of same-round
second-call wall ratios, with a 95% median bootstrap interval (10,000 draws,
seed `20261010`). H3 is the registered dimensionless margin normalized to
same-round seq CPU. P = pass, F = persistent fail, I = inconclusive,
V = either attempt's twin interval excludes 1, – = not applicable. Voiding
has precedence, including the reported controls. E4-par applies only where
that attempt's par/seq interval is wholly below 1; E5c-keep has no such
condition. Small_constant and spine decide nothing for E4; their literal
keep comparison still applies. A rerun disagreement stays I, including
recursion W8 keep's initial pass and rerun straddle.

| Rule (quantity, baseline and ideal bound) | P, cells | F, cells | I, cells | V, cells | Not applicable, cells | Total, cells |
|---|---:|---:|---:|---:|---:|---:|
| E4-seq: demand/seq wall, dimensionless, ≤ 1.00 | 18 | 1 | 10 | 2 | 5 | 36 |
| E4-par: demand/par wall, dimensionless, ≤ 1.05 | 16 | 1 | 1 | 2 | 16 | 36 |
| E4-H3: demand CPU margin/seq CPU, dimensionless, ≤ 0 | 24 | 3 | 2 | 2 | 5 | 36 |
| E5c-keep: demand/unversioned wall, dimensionless, ≤ 1.05 | 32 | 0 | 2 | 2 | 0 | 36 |

| Workload | Workers W, count | E4-seq verdict, demand/seq wall ≤ 1.00 | E4-par verdict, demand/par wall ≤ 1.05 | E4-H3 verdict, CPU margin/seq CPU ≤ 0 | E5c-keep verdict, demand/unversioned wall ≤ 1.05 |
|---|---:|---|---|---|---|
| small_constant | 1 | – | – | – | P |
| small_constant | 4 | V | V | V | V |
| small_constant | 8 | – | – | – | I |
| small_split | 1 | I | – | P | P |
| small_split | 4 | I | – | P | P |
| small_split | 8 | I | – | P | P |
| recursion | 1 | I | – | P | P |
| recursion | 4 | P | I | P | P |
| recursion | 8 | P | F | P | I |
| spine | 1 | – | – | – | P |
| spine | 4 | – | – | – | P |
| spine | 8 | – | – | – | P |
| hot_helper | 1 | F | – | P | P |
| hot_helper | 4 | I | – | P | P |
| hot_helper | 8 | I | – | P | P |
| large_helper | 1 | P | P | P | P |
| large_helper | 4 | P | P | P | P |
| large_helper | 8 | P | P | P | P |
| mandelbrot | 1 | I | – | P | P |
| mandelbrot | 4 | P | P | P | P |
| mandelbrot | 8 | P | P | P | P |
| records | 1 | I | – | P | P |
| records | 4 | P | P | P | P |
| records | 8 | P | P | P | P |
| fir | 1 | P | P | P | P |
| fir | 4 | V | V | V | V |
| fir | 8 | P | P | P | P |
| stencil | 1 | I | – | P | P |
| stencil | 4 | P | P | F | P |
| stencil | 8 | P | P | F | P |
| prefix | 1 | I | – | P | P |
| prefix | 4 | P | P | P | P |
| prefix | 8 | P | P | I | P |
| histogram | 1 | P | P | P | P |
| histogram | 4 | P | P | I | P |
| histogram | 8 | P | P | F | P |

**Cells that fail, remain inconclusive or are void.** Every F/I rule is
shown with both attempts when the cell was rerun. V rows show the twin
quantity that invalidates all four rules, rather than assigning a numerical
loss to an invalid cell. No straddle receives an extra rerun. Values are
rounded to six decimal places here; full precision selects the verdict.

| Workload | Workers W, count | Rule and quantity definition, dimensionless; denominator is baseline; ideal seq ≤ 1.00, par/keep ≤ 1.05, H3 ≤ 0, twin = 1 | Initial median [95% interval], dimensionless | Rerun median [95% interval], dimensionless | Literal verdict |
|---|---:|---|---|---|---|
| small_constant | 4 | twin/demand wall | 0.910316 [0.875278, 0.983942] | – | V, all rules |
| small_constant | 8 | E5c-keep: demand/unversioned wall | 0.962202 [0.903262, 1.050378] | – | I |
| small_split | 1 | E4-seq: demand/seq wall | 0.999431 [0.991852, 1.000053] | – | I |
| small_split | 4 | E4-seq: demand/seq wall | 1.000403 [0.999764, 1.000865] | – | I |
| small_split | 8 | E4-seq: demand/seq wall | 0.999503 [0.999080, 1.000103] | – | I |
| recursion | 1 | E4-seq: demand/seq wall | 1.006915 [0.975683, 1.035369] | – | I |
| recursion | 4 | E4-par: demand/par wall | 1.065696 [1.036252, 1.090506] | – | I |
| recursion | 8 | E4-par: demand/par wall | 1.085946 [1.078074, 1.099753] | 1.113828 [1.092974, 1.138903] | F |
| recursion | 8 | E5c-keep: demand/unversioned wall | 0.993155 [0.979064, 1.035969] | 1.036260 [1.014796, 1.070927] | I |
| hot_helper | 1 | E4-seq: demand/seq wall | 1.000225 [1.000053, 1.000307] | 1.000080 [1.000021, 1.000158] | F |
| hot_helper | 4 | E4-seq: demand/seq wall | 1.000169 [0.999996, 1.000320] | – | I |
| hot_helper | 8 | E4-seq: demand/seq wall | 1.000068 [0.999988, 1.000251] | – | I |
| mandelbrot | 1 | E4-seq: demand/seq wall | 0.999256 [0.995571, 1.003663] | – | I |
| records | 1 | E4-seq: demand/seq wall | 1.000481 [0.998778, 1.003119] | – | I |
| fir | 4 | twin/demand wall | 0.999276 [0.998775, 0.999767] | – | V, all rules |
| stencil | 1 | E4-seq: demand/seq wall | 1.001200 [0.997680, 1.005501] | – | I |
| stencil | 4 | E4-H3: demand CPU margin/seq CPU | 0.157522 [0.130168, 0.192113] | 0.190670 [0.169718, 0.238023] | F |
| stencil | 8 | E4-H3: demand CPU margin/seq CPU | 0.566776 [0.508151, 0.789405] | 0.573548 [0.524100, 0.791695] | F |
| prefix | 1 | E4-seq: demand/seq wall | 0.999175 [0.987787, 1.002858] | – | I |
| prefix | 8 | E4-H3: demand CPU margin/seq CPU | -0.323137 [-0.386081, 0.170838] | – | I |
| histogram | 4 | E4-H3: demand CPU margin/seq CPU | 0.009517 [-0.011076, 0.028027] | – | I |
| histogram | 8 | E4-H3: demand CPU margin/seq CPU | 0.209959 [0.123325, 0.274229] | 0.188200 [0.129365, 0.201424] | F |

For comparison, the same H3 definition gives these reference margins;
they are observations, not additional verdict rules.

| Workload | Workers W, count | Attempt, count | par H3 margin/seq CPU, median [95% interval], dimensionless; ideal ≤ 0 | unversioned H3 margin/seq CPU, median [95% interval], dimensionless; ideal ≤ 0 |
|---|---:|---:|---|---|
| stencil | 4 | 1 | 0.197336 [0.175415, 0.215047] | 0.162281 [0.135427, 0.197381] |
| stencil | 4 | 2 | 0.191643 [0.165263, 0.238878] | 0.191301 [0.169985, 0.228424] |
| stencil | 8 | 1 | 0.591898 [0.538024, 0.766110] | 0.544900 [0.510954, 0.759245] |
| stencil | 8 | 2 | 0.586261 [0.544869, 0.792086] | 0.597755 [0.554273, 0.794594] |
| histogram | 4 | 1 | -0.004463 [-0.028549, 0.016034] | -0.001831 [-0.023480, 0.023598] |
| histogram | 8 | 1 | 0.177984 [0.098098, 0.210363] | 0.160518 [0.131763, 0.220703] |
| histogram | 8 | 2 | 0.153223 [0.112222, 0.194956] | 0.180469 [0.126233, 0.222413] |

**Images and clone inventory.** Only **small_split's `wf_workload`** has a
`par.region.entry` cheap-region versioning block. Its entry selects sequential
work for extent < 21,429 (`cmp $0x53b4` / `ja`), then tail-jumps to the clone
for extent 3. The sequential `wf_workload` clone is identical to seq after
normalization, retains the repetition walker and runtime-extent stores, and
has no per-iteration scheduling decision. Unversioned retains the span
calculation and `cmp $0x53b4` inside the walker on every repetition, plus the
cold edge to `wf__par_slice_mark.0`. Thus the intended decision removal is
visible in this run's optimized code.

The existing entry selection between parallel and sequential worlds also
survives in small_split, recursion, spine, large_helper, mandelbrot, records,
fir, stencil, prefix and histogram. At W1 those entries select sequential
work; at W4/W8 they select the parallel body, with small_split then selecting
its cheap sequential region. **Small_constant and hot_helper have neither
cheap-region versioning nor entry clone selection.** All other workloads
have no new cheap region. Demand and unversioned executable hashes are equal
for every workload except small_split, so this ablation changes only that
workload's image.

The comparison uses `xcrun llvm-objdump -dr` and the objects' symbol sizes
(`-t`) to bound each function, excluding inter-function padding and byte
continuation lines. Instruction addresses and branch targets become local
instruction labels; resolved private text addresses and relocation addends
are normalized, clone callees are mapped to their seq names, and constant
pool relocations retain their referenced constant bytes (`-s`). Internal
alignment instructions remain. This avoids mistaking different addresses,
unnamed resource-abort targets or adjacent weak functions for different
code. All 42 sequential-clone definitions in the ten LLVM images are
inventoried in the retained diff: 25 original-function clones and 17 generated
chunk clones. No clone is omitted. Generated chunks have no standalone
counterpart in seq; their retained comparisons use the original owning seq
function containing that loop, explicitly a different function boundary/ABI.
Their nonidentity does not establish different work.

In this table clone names omit `wf__par_seq_`; corresponding seq names use
`wf_`. Iden = identical after address/relocation normalization; Diff =
different beyond register renaming. **No comparison is register-renaming
only.** “Selected” names the timed sequential body, with its reachable
original-function helpers; untimed main/smoke clones remain inventoried too.

| Workload | Selected original-function clones vs corresponding seq functions | Other original-function clones | Generated chunk clones vs owning seq function |
|---|---|---|---|
| small_split | workload, mark: Iden | – | mark.1: Diff |
| recursion | workload, fib: Iden | – | none |
| spine | workload, spine: Iden | – | none |
| large_helper | workload, helper: Iden | – | helper.1: Diff |
| mandelbrot | render_points: Iden | command_smoke, main: Iden | render_points.1: Diff |
| records | summarize_records: Iden | main: Iden | summarize_records.1: Diff |
| fir | filter: Diff | command_smoke, main: Iden | filter.1: Diff |
| stencil | stencil: Diff; stencil_row: Iden | main: Iden | stencil_row.1, stencil.1/.3/.5: Diff |
| prefix | prefix: Diff; sum_block: Iden | main: Iden | sum_block.1, prefix.1/.3, main.1: Diff |
| histogram | histogram, merge_bucket: Diff | main: Iden | merge_bucket.1, histogram.1/.3, main.1: Diff |

The differing selected bodies are not merely renamed registers: fir's seq
filter unrolls output iterations by two, while demand's clone unrolls the
inner tap loop by four; prefix changes its stack frame (40 to 56 bytes),
loop preparation and branch conditions; histogram's merge introduces a
four-way strided reduction and its enclosing histogram differs; stencil's
enclosing body has changed loop organization while stencil_row is identical.
The retained diff records these differences without assigning their timing
cause to versioning, whose unversioned ablation has the same images here.

Timed work survives: small_split retains stores and accumulation; recursion
retains Fibonacci recursion/addition and budgeted request-gated offers;
spine retains its deep recurrence, whose leaf is folded; large_helper retains
its data-dependent recurrence over the slice; mandelbrot retains escape
iterations; records retains validation and reduction; fir retains tap multiply/
adds; stencil retains row updates; prefix retains block sums and scan;
histogram retains bucket counting and merging. Small_constant is the
registered folded control: its repeated tiny stores collapse to final values
(the reported keep comparison still applies).

The surviving decision in each demand slice driver is an extent/grain
comparison followed, only above its bound, by a TLS `%fs` request-word load,
test and conditional branch to hand-out. This appears in helper, render_points,
summarize_records, filter, stencil_row/stencil, sum_block/prefix and
merge_bucket/histogram drivers. The recursion and spine budgeted bodies
test the budget, transfer to the sequential clone when exhausted, and otherwise
decrement it before the TLS load/test/branch. These are inspected scheduling
paths; the W1 entry selection bypasses them, and small_split's cheap-region
selection bypasses its driver at the measured extent.

**Hot_helper W1 executes the same work as seq.** Its grain-pruned image has
no `par.region.entry`, no sequential clone, no pool-active selection and no
request poll in the timed body. `wf_bench_micro` contains the inlined helper's
data-dependent rotate/add loops and the repetition loop. Both this actual
timed wrapper and `wf_workload` have identical normalized disassembly to seq;
the linked images have different addresses. The persistent E4-seq loss is
therefore **not evidence of an executed scheduling decision**. Its cause is
unattributed; equal normalized code does not waive the literal bound.

**E4-seq beside 5a's observed layout spreads.** Every W1 cell is shown,
including controls, plus all near-1 W4/W8 E4-seq cells. The historical spread
is `max((upper−lower)/2, abs(median−1))` of shifted-seq/seq wall ratios on
CPU 2 at W1, in shift64 / shift4160 / shift65664 order. The panel maximum
is **0.006751**. Micros were unmeasured; records' 5a cell was void; 5a's
**overall reading remains inconclusive**. These observations select neither
a layout allowance nor a changed literal verdict, and establish no W4/W8
floor. Full historical intervals remain in [5a's results](#experiment-5as-results).

| Workload | Workers W, count | E5c demand/seq second-call wall median [95% interval], dimensionless; baseline seq, ideal ≤ 1.00 | Literal E4-seq verdict | E5a W1 shifted-seq/seq spread, dimensionless; baseline unshifted seq, quiet ideal < 0.001, floor criterion ≥ 0.0015 | E5a panel largest spread, dimensionless; same definition |
|---|---:|---|---|---|---:|
| small_constant | 1 | 0.987234 [0.967257, 1.010961] | – | unmeasured micro | 0.006751 |
| small_split | 1 | 0.999431 [0.991852, 1.000053] | I | unmeasured micro | 0.006751 |
| small_split | 4 | 1.000403 [0.999764, 1.000865] | I | unmeasured micro | 0.006751 |
| small_split | 8 | 0.999503 [0.999080, 1.000103] | I | unmeasured micro | 0.006751 |
| recursion | 1 | 1.006915 [0.975683, 1.035369] | I | unmeasured micro | 0.006751 |
| spine | 1 | 0.995890 [0.994655, 0.997714] | – | unmeasured micro | 0.006751 |
| hot_helper | 1 | 1.000225 [1.000053, 1.000307]; rerun 1.000080 [1.000021, 1.000158] | F | unmeasured micro | 0.006751 |
| hot_helper | 4 | 1.000169 [0.999996, 1.000320] | I | unmeasured micro | 0.006751 |
| hot_helper | 8 | 1.000068 [0.999988, 1.000251] | I | unmeasured micro | 0.006751 |
| large_helper | 1 | 0.993082 [0.992103, 0.993744] | P | unmeasured micro | 0.006751 |
| mandelbrot | 1 | 0.999256 [0.995571, 1.003663] | I | 0.004611 / 0.004012 / 0.002904 | 0.006751 |
| records | 1 | 1.000481 [0.998778, 1.003119] | I | 0.002021 / 0.001588 / 0.002010 (5a void) | 0.006751 |
| fir | 1 | 0.955989 [0.954594, 0.956985] | P | 0.000895 / 0.001901 / 0.000965 | 0.006751 |
| stencil | 1 | 1.001200 [0.997680, 1.005501] | I | 0.001220 / 0.001888 / 0.001295 | 0.006751 |
| prefix | 1 | 0.999175 [0.987787, 1.002858] | I | 0.004408 / 0.005413 / 0.005881 | 0.006751 |
| histogram | 1 | 0.982851 [0.977106, 0.990422] | P | 0.006751 / 0.003918 / 0.002170 | 0.006751 |

**First-call startup, separate from the judging second call.** The table
reports ranges of each cell/attempt's median first-call CPU−wall across
W1/W4/W8 and available reruns, in µs (ideal 0; negative values are retained).
Individual intervals remain in the downloaded summary. No startup quantity
changes a verdict above.

| Workload | seq CPU−wall median range, µs, ideal 0 | par CPU−wall median range, µs, ideal 0 | demand CPU−wall median range, µs, ideal 0 | unversioned CPU−wall median range, µs, ideal 0 | twin CPU−wall median range, µs, ideal 0 |
|---|---:|---:|---:|---:|---:|
| small_constant | 1.855 to 2.415 | -212.464 to 1.866 | 1.847 to 2.473 | 1.893 to 2.498 | 1.859 to 2.470 |
| small_split | -27.055 to -11.411 | -215.887 to -11.167 | -12.303 to 8180.533 | -10.255 to 8208.817 | -10.479 to 8226.217 |
| recursion | -7.760 to -2.940 | -3.938 to 61843.848 | -3.740 to 73559.643 | -3.322 to 73499.147 | -3.358 to 72649.314 |
| spine | 1.658 to 2.200 | 1.502 to 291503.086 | 0.547 to 253852.193 | 1.508 to 249853.918 | 1.572 to 256137.410 |
| hot_helper | -9.242 to -3.852 | -6.774 to -5.285 | -6.048 to -4.609 | -8.465 to -3.123 | -6.261 to -4.615 |
| large_helper | -4.741 to -2.167 | -2.607 to 163311.834 | -2.462 to 45422.296 | -2.252 to 46043.231 | -2.558 to 44430.779 |
| mandelbrot | 1.840 to 2.035 | 1.812 to 9170.317 | 1.847 to 9614.306 | 1.841 to 9487.752 | 1.775 to 9384.718 |
| records | 4.967 to 5.410 | 4.975 to 7123.237 | 4.962 to 10093.975 | 4.981 to 8694.914 | 4.889 to 8390.300 |
| fir | 2.679 to 4.229 | 2.670 to 4762.956 | 2.792 to 10328.636 | 2.533 to 8535.094 | 2.638 to 9670.317 |
| stencil | 3.181 to 5.098 | 5.069 to 117536.027 | 4.159 to 123370.539 | -1.721 to 124896.064 | 2.826 to 123713.621 |
| prefix | 3.925 to 4.473 | 3.834 to 109.725 | 3.951 to 8076.534 | 3.886 to 8051.243 | 3.857 to 8079.452 |
| histogram | -2.498 to -0.164 | -1.845 to 13720.567 | -1.885 to 20944.281 | -1.988 to 20859.787 | -3.114 to 20958.394 |

**Falsifier and predictions.** Small_split W4/W8 have no persistent interval
wholly above 1: both straddle 1, so the registered whole-cause falsifier is
**not triggered**, despite confirmed removal of the per-iteration decision.
This does not prove that decision was the whole cause. Their median
absolute displacements, 0.000403 and 0.000497, and interval half-widths,
0.000551 and 0.000512, are below the historical panel maximum 0.006751.
Thus the prediction's “within the observed spread” alternative is numerically
met against that panel observation; its literal-pass alternative is not met.
There is no small_split layout control and no measured W4/W8 layout floor,
so this comparison is descriptive, not an attribution or allowance.
E5c-keep improves small_split W4/W8 to 0.911491 [0.857601, 0.914235] and
0.913083 [0.910791, 0.914479] against unversioned, consistent with removing
the repeated decision. Records passes keep at all widths. Fir passes keep
at W1/W8, but W4's twin void prevents confirming the prediction at every
width. Protected workload preservation is also incomplete because recursion
W8 keep is inconclusive; small_constant W8 keep is a reported-control
straddle, not a failure. There are no persistent E5c-keep failures.

**What remains and why this analysis stops.** Recursion W8 still fails
E4-par (8.59% initial and 11.38% rerun median loss against par); W4 remains
inconclusive. This rejects preservation of every available par speedup and
returns to the owner's open card **whether budgeted recursion keeps
request-gated offers**. Stencil W4/W8 and histogram W8 fail H3; histogram W4
and prefix W8 remain inconclusive. Their positive CPU margins return to the
open card **whether H3 gates memory-bound programs**, without inferring
bandwidth saturation from these measurements. Hot_helper W1 fails the
literal never-slower bound in both attempts, despite identical normalized
timed code; mandelbrot/records W1 and the other listed straddles remain
inconclusive. These return to the open card **how never-slower is stated
and checked**, beside the unchanged 5a observations. All three cards are on
the [owner's status board](https://claude.ai/artifact/7tocXS3iUdthCLCQCMd3ip);
this result settles none of them and selects no further lowering change.

Suspicious controls are small_constant W4 (twin/demand 0.910316
[0.875278, 0.983942], tiny folded work) and fir W4 (0.999276
[0.998775, 0.999767], identical twin bytes): both are void, not discarded
or relabeled as passes. W8's CPU 0 physical-core sharing remains a placement
caveat, not a retrospective exclusion. No specification, threshold, test
expectation or design decision changed. No local build, compiler execution,
image execution or test was performed; no commit or push was made.

**Analysis review.** A separate read-only Codex/GPT-6 agent reviewed
`19733d3551e5e8aedf907a05363b235178e2f89e` to this working-tree results
addition, including the complete untracked normalized-comparison artifact,
registration, 5a evidence, relevant design commitments and downloaded data.
It reproduced the sizing and quantities, strict verdicts, all nonpassing-cell
values and startup ranges, regenerated all 42 clone comparisons, and audited
address/relocation normalization against raw disassembly. Findings: none
within scope. Checklist A4/D2 passed; D1/V3 and specification delivery were
not applicable; code/test/gate groups were outside this prose-only change.
CI construction, oracle execution and semantic correctness were not
independently rerun. Patch whitespace passed `git diff --check`.

## Experiment 6: eager budgeted recursion and sequential identity, fixed before it measures

**Question and selection ground.** The owner's 2026-10-11 rulings select
immediate offers within the existing recursion budget, mechanical sequential
identity instead of inferring identity from timing, a same-job layout floor
beside the literal never-slower diagnostic, and a wall/CPU width sweep beside
H3. Does removing the request test above the recursion cut recover recursion
W8's lost speedup while preserving experiment 5c's protected cells? Can the
timed one-worker and versioned-cheap computations meet instruction identity
and execute their entry decisions outside every loop?

The competing explanations are delayed recursive work exposure versus other
scheduler or optimized-code costs. Compare default demand with an otherwise
identical request-gated budget arm and ordinary par. Experiment 5c measured
recursion W8 demand/par at 1.08–1.11; the request is observed only at a
budgeted node, with none below the cut. The owner's approximately 0.14 ms
leaf wait is a motivating estimate, not a latency measurement in this trial.
No new measurement preceded this registration.

**Change and controls.** A budget-variant function offers immediately instead
of first testing its request word. Budget initialization, spending at groups,
carried callback budgets, exhausted-budget sequential transfer, lane refusal,
call grain, loop-slice request gating and the runtime are unchanged. The
existing budget remains the bound on exposed recursive work; it is not reset
on steals or increased. This implements the eager recursive part of the
[hybrid proposal](hybrid-plan.md), without adopting its other mechanisms.

Use the same twelve workloads, inputs, repetitions and extents as experiment
5c. At W1/4/8 interleave `seq`, `par`, `demand`, `previous`, `unversioned`
and the byte-identical demand `twin`. `previous` uses
`--par-demand --par-demand-ablation requested`, reconstructing the prior
request-gated budget behavior with region versioning retained. `unversioned`
uses its existing ablation with the new default budget policy, retaining the
5c versioning comparison. These two controls separate preservation of the
previous candidate from preservation against the unversioned control; neither
is silently substituted for the other. Requests are on for demand, previous,
unversioned and twin; seq and par use `off-never-request`.

At W1 add `seq-shift64`, `seq-shift4160`, `seq-shift65664` for **all twelve**
workloads, reusing the exact seq WF, oracle, runner and runtime objects and
5a's uncalled padding construction. Validate and retain every WF text-symbol
shift before timing. Do not import 5a's historical floor or use a different
workload's layout sensitivity. Add W2 for stencil and histogram, with the
same six ordinary arms, to complete their 1/2/4/8 wall and CPU sweep.

**Construction identity, before timing.** Every W1 workload is covered,
including small_constant and spine. Check the timed `wf_bench_micro` entry
for the six micros and each kernel's `wf_bench_<workload>` entry, its selected
sequential computation and reachable original-function helpers. Also cover
small_split's cheap-region selection at W4/W8 (extent 3), and inventory every
`par.region.entry` in the demand IR so a newly versioned region cannot escape
coverage. Zero dispatches are expected where pruning removed selection;
otherwise establish that the pool-active decision and any cheap-region
decision execute once per timed call, before the computation's loops, not
once per iteration. Count the two decisions separately when both survive.

Under [option D](#identity-construction-option-d), compare normalized
disassembly of corresponding canonical sequential computations,
preserving operations, data constants, offsets, register relationships and
control-flow destinations while removing placement addresses and consistent
register renaming. Follow the selected path through the actual timed entry;
merely comparing an unused clone does not qualify. The explicit cold
resource-record writer boundary is listed separately, and the evidenced
pool-query entry adapter is counted and timed separately. Neither boundary
waives a computation mismatch. A mismatch, missing body,
uncovered region or unproved dispatch placement fails hosted construction,
naming workload, function and the first differing instruction or missing
proof. Retain the report and a deliberate changed-body negative control.
Correct oracle output cannot waive this identity requirement.

Experiment 5c already found nonidentical selected bodies in fir, stencil,
prefix and histogram, beyond register renaming. This gate may therefore stop
the experiment before timing: that is a compiler optimization gap to expose,
not permission to omit those cells, normalize away changed loop organization,
or relax the requirement. The separate seq module and the demand module's
clones can receive different host-optimizer transformations even when their
source computations agree. Fixing that general preservation boundary is not
silently included in the request-gating change.

**Placement and protocol.** Require native 14900K CI and experiment 5c's
validated P-core placement: W1 `{2}`, W2 `{2,4}`, W4 `{2,4,6,8}`, W8
`{2,4,6,8,10,12,14,1}`. Exclude logical CPU 0, retain the caveat that CPU 1
shares its physical core, and reject unavailable CPUs or duplicate cores.
Verify all selected arms against the independent oracles with requests both
on and off before timing. Rotate/reverse the paired arms, workload order and
widths; retain first-call wall and CPU/startup separately and judge call 1
(the second call). Use paired per-round ratios, their median, **10,000
bootstrap draws, seed 20261010, and 95% intervals**.

Collect six full-work sizing rounds in the same job; they judge nothing.
Freeze the smallest `n` from 6 through 30 whose projected interval widths,
scaled by `sqrt(6/n)`, are all at most `max(0.02, |median-bound|)`; otherwise
use 30. Include the literal seq diagnostic, par and both preservation
comparisons, plus W1 shifted-seq ratios about 1; H3 is reported and does not
select `n`. Save `sizing-e6/` and require its exact frozen count. Collect
separate decisive rounds and never pool sizing with them. An exceeding
gated cell receives exactly one further `n`-round attempt with all its arms.
A twin/demand interval excluding 1 in either attempt voids the cell and its
floor reading. A persistent exceedance fails; attempt disagreement or a
straddle stays inconclusive, with no additional reruns.

**Rules and rejection, fixed before measurement.**

- E6-recursion: recursion W8 demand/par wall upper bound **<= 1.05**, without
  requiring a separate par-speedup condition. Persistent exceedance rejects
  recovery of the observed loss and returns the attribution for investigation.
  Other applicable par-speedup cells retain E4-par's 1.05 bound.
- E6-keep: **every cell protected by 5c** keeps demand/unversioned **and**
  demand/previous wall upper bounds **<= 1.05**, including its reported
  controls. Apply these bounds to the added W2 cells too. A persistent
  exceedance rejects preservation; missing image inspection cannot pass.
- E6-identity: the construction check must pass for every covered cell.
  Failed or missing construction evidence prevents timing qualification;
  timing never repairs an identity mismatch.
- E6-seq diagnostic: retain the literal demand/seq wall upper bound
  **<= 1.00** and its interval classification. For each W1 workload and
  attempt, let `[L_s,U_s]` be each shifted-seq/seq wall interval and set
  `F = max_s(max(abs(L_s-1), abs(U_s-1)))`. If the **entire** demand/seq
  interval is contained in `[1-F,1+F]`, record **at the floor**, alongside
  the literal result, rather than a failure. This is an empirical envelope,
  not proof of layout causality or a universal performance guarantee. An
  interval outside it retains its literal diagnostic; absent controls or a
  void twin supply no floor. No W4/W8 allowance is inferred from W1 controls.
  The seq diagnostic alone does not trigger an E6 rerun or reject construction.
- H3: report the unchanged margin
  `(cpu_demand - 1.1*cpu_seq - 0.1*max(0,wall_seq-wall_demand)*W)/cpu_seq`
  and its interval, without an H3 failure or H3-triggered rerun in this
  experiment. For stencil/histogram report absolute wall and CPU at every
  width and paired adjacent-width wall/CPU ratios (2/1, 4/2, 8/4).
  An adjacent wall-ratio upper bound below 1 demonstrates further speedup;
  a straddle is inconclusive and a lower bound above 1 shows slowing.
  A positive H3 margin while wall still shortens is a reported CPU tradeoff,
  not a rejection. Without demonstrated scaling, report that the owner's
  speedup condition is unestablished; no unconditional CPU-cost exemption
  follows from making H3 diagnostic in this trial. Width comparisons use
  the initial complete sweep, not a mixture of selectively rerun widths.

The owner's earlier per-call rule remains for a decision whose condition
cannot be hoisted: at most **1 ns or 2% per execution, whichever is wider**.
It is not an allowance for W1 identity or small_split's invariant region.
Applying it requires an inspected surviving decision and its execution count;
do not invent a count for a loop-slice poll or label a layout displacement as
decision cost. This experiment adds no nonhoistable per-call decision.

**Dispatch and pending evidence.** Dispatch `compute-bench.yml` with
`experiment=par-demand`, `par_demand_experiment=6`,
`placement_runner=14900k`, and empty `placement_rounds`. Hosted CI constructs
the images, runs negative harness controls, checks identity and verifies the
oracles before the dependent timing job. The session that owns this line commits,
pushes and runs CI. No local build, test suite, compiler or harness execution, or measurement
is authorized for this edit. Syntax compilation of edited Python is the only
permitted local code check. There is no specification change or approval-log
entry. Retain these controls while the investigation needs reproduction.

**Construction implementation boundary.** The checker targets the hosted
Linux x86-64 ELF images. It bounds functions by ELF symbol sizes, preserves
ABI registers and normalizes consistent nonvolatile/scratch-register renaming,
checks referenced constant bytes, and rejects address-valued data references
whose object identity it cannot preserve. Its supported entry-shell proof
follows concrete timed scalar inputs and requires restored arguments, saved
registers and stack before a tail transfer. It rejects unknown instructions,
branches and calling forms. The formal kernel adapters also contain nontail
calls followed by Box-result unpacking. A supported W1 CFG projection diagnoses
these wrappers by retaining every nondecision instruction, following the
inactive pool edge, and checking the dynamic pool count against the LLVM proof.
A differing save, restore, result store or computation is reported as a
demonstrated difference. Identical projected text alone does not prove that
live caller-clobbered values or flags survive the removed pool call; a wrapper
without the entry-state preservation proof may qualify only as the
explicitly evidenced entry-adapter boundary selected under option D; it
remains unproved as literal whole-wrapper machine identity.
Distinguish that checker limitation from a demonstrated differing sequential
body. Option D admits only its evidenced boundaries; a differing computation
or unsupported attribution still blocks timing. Hosted execution must
establish which supported shapes the actual optimized images use.

Only edited-Python syntax compilation and patch-whitespace checks ran locally.
The authored compiler regression, Python cases, compiled changed-body negative
control, oracle checks and measurements await CI. Scoped independent
cross-reviews covered the compiler and the experiment machinery separately;
they fixed sizing-rerun admission and identity-proof issues involving flags,
register aliases, readonly address identity and private stack saves. A fresh
whole-change reviewer could not start because the agent tool reached its
thread limit and the separate CLI review failed initialization with
`Operation not permitted`; these scoped reviews do not replace that missing
fresh review. No approval or measured success is claimed.

### Hosted construction attribution at 968ed23c8

Run 38142512574 reached comparison for every workload and stopped before
timing. The saved `.ll.raw` is compiler output before LLVM optimization;
the saved `.ll` also contains the timed host adapter. The paired `.o.s`
and final disassembly distinguish private labels from real instruction
differences. This is construction evidence, not a performance result.

In the table, **a** means a checker gap, **b** means different machine code
for the same pre-optimization sequential computation, and **c** means
different pre-optimization IR. An entry adapter needs an explicit
qualification: its literal IR differs because it contains the pool query;
its inactive computation can nevertheless match after selecting the
inactive edge and collapsing the join phi. Calling that projection equal
does not prove that the query preserves entry state or costs nothing.

| Workload / reported function | Reported first difference | Attribution |
|---|---|---|
| small_constant / wf__heap_change | address-valued RIP reference | a: bounded readonly string address was unsupported |
| small_split / wf_bench_micro | call frame_dummy+0xa0 / +0xe0 | a: private abort callee lost its symbol; root IR matches, but helper closure is c |
| small_split / wf__heap_change | address-valued RIP reference | a |
| small_split / wf__par_seq_workload | call frame_dummy+0xa0 / +0xe0 | a: misleading nearest symbol; root IR matches, helper closure is c |
| recursion / wf_bench_micro | push rax / saved2 | c for literal dispatcher IR; inactive projected computation IR and fib closure match, so the remaining projected entry machine difference is b |
| spine / wf_bench_micro | mov rdx,rax / push saved0 | c for literal dispatcher IR; inactive projected computation IR and spine closure match, so the remaining projected entry machine difference is b |
| large_helper / wf_bench_micro | mov rdx,rax / push saved0 | c for literal adapter; inactive projected adapter matches, but helper closure is c |
| mandelbrot / wf_bench_mandelbrot | stack allocation 0x18 / 0x28 | c for literal adapter and computation closure |
| mandelbrot / wf__heap_change | address-valued RIP reference | a |
| mandelbrot / wf__par_seq_render_points | call frame_dummy+0xa0 / +0xe0 | a for call naming; computation IR is independently c |
| records / wf_bench_records | push saved0 / rbp | a for register naming; remaining adapter saves are real c |
| records / wf__par_seq_summarize_records | call frame_dummy+0xa0 / +0xe0 | a for call naming; computation IR is independently c |
| records / wf__heap_change | address-valued RIP reference | a |
| fir / wf_bench_fir | sub 0x18,rsp / push rax | c for literal adapter and computation closure |
| fir / wf__heap_change | address-valued RIP reference | a |
| fir / wf__par_seq_filter | sub 0x18,rsp / push rax | c: outlined sequential chunk already differs in IR |
| stencil / wf_bench_stencil | push saved0 / rbp | a for register naming; remaining adapter saves are real c |
| stencil / wf__par_seq_stencil | mov rsi,saved4 / rbp | c: not a consistent whole-function register permutation; IR also differs |
| stencil / wf__heap_change | address-valued RIP reference | a |
| prefix / wf_bench_prefix | push saved0 / rbp | a for register naming; remaining adapter saves are real c |
| prefix / wf__par_seq_prefix | stack allocation 0x28 / 0x38 | c: outlined sequential chunks already differ in IR |
| prefix / wf__heap_change | address-valued RIP reference | a |
| histogram / wf_bench_histogram | push saved0 / rbp | a for register naming; remaining adapter saves are real c |
| histogram / wf__par_seq_histogram | stack offset 0x10 / 0x18 | c: outlined sequential chunks already differ in IR |
| histogram / wf__heap_change | address-valued RIP reference | a |

The private call is LLVM's `.Lwf_resource_abort`, visible in `.o.s` but
absent from the original final ELF symbol table. `frame_dummy+offset` is
objdump's nearest-symbol description, not the callee's identity. The
checker now requires exact retained ELF function bounds and resolves the
destination address first; it does not erase arbitrary call offsets.
Experiment 6 retains assembler temporaries and linker locals in every arm.
Function reconstruction uses symbol types and exact ranges, including
repeated basic-block labels from different objects; reachable support
includes private targets of conditional transfers as well as calls.
The readonly address is the bridge's allocation-thread-inventory error
string. Its normalization retains translation-unit-qualified object names,
complete bytes, size, interior offset and overlapping object identities;
equal bytes alone do not establish pointer identity. RBP can be an ordinary
nonvolatile temporary. It is normalized consistently with the other saved
registers unless explicit stack/frame instructions give it a frame role.
Push order, save count, stack offsets and frame allocation remain evidence.
Synthetic controls require relocated identical private callees and readonly
addresses to match, and mutated bodies, changed pointer alias relationships
and inconsistent register permutations to differ. The hosted native control
requires retained symbols to leave executable section bytes unchanged and
to expose a deliberately changed private callee. These new controls await CI;
only Python syntax compilation and patch whitespace checks ran locally.

Correct callee naming reveals another genuine difference: demand's
`wf_resource_record_abort` acquires a private latch with `cmpxchg` and parks
losing writers, whereas seq starts the write loop immediately. The abort
thunk itself has matching IR; its reachable support does not. This is not
repaired by renaming its call.

The clone IR differences arise before Clang. [Loop synthesis](../../../compiler/src/lowering/builder/loops.rs)
replaces original loops with chunk functions before the [backend emits
sequential clones](../../../compiler/src/backend/emitter.rs); its
[sequential split emitter](../../../compiler/src/backend/emitter/parallel.rs)
calls the chunk. In the saved paired raw IR, small_split's `workload` bodies match,
but seq's `mark` contains its loop and demand's sequential `mark` calls a
chunk. The same distinction applies to large_helper's `helper`. The roots
`render_points`, `summarize_records`, `filter`, `stencil`, `prefix` and
`histogram` themselves contain new sequential chunk calls. Their signatures,
alignment, target settings and function attributes match; their CFGs and
function boundaries do not. Recursion's `workload`/`fib` and spine's
`workload`/`spine`/`sharedleaf` closures match before optimization.
The [timed adapters](../../../tests/programs/compute/host-adapter.awk)
still preserve inputs across a new pool call, and Clang can
inline the inactive computation in the mixed module. That explains real
entry save differences without attributing them to a changed clone IR.
For stencil, identical six-register push order fixes the normalization
mapping; changing a later use from RBX to RBP cannot be absorbed by a
consistent permutation.

### What would establish machine identity

These are alternatives for the owner, not an implemented compiler change
or a relaxation of E6-identity. The current failures remain construction
failures after the checker repairs.

First, the compiler would need a canonical sequential lowering made before
parallel loop synthesis, covering the entire reachable computation and its
support, attributes and globals. Cloning the already transformed graph
cannot meet the promised byte-identical lowering. The resource-record
writer's differing concurrency policy also needs an explicit boundary:
sharing that support unchanged may conflict with parallel error handling;
excluding it from identity would change the check's scope.

Emitting matching original IR and putting an identical `noinline` boundary
in both images would prevent the timed adapter from absorbing the
computation. It costs a call or tail transfer and the optimizer's freedom
across that boundary. It does not by itself prevent inlining or other
interprocedural decisions *inside* the computation from depending on the
rest of each module. Separate LLVM modules optimized independently have no
general contract guaranteeing identical machine code from equal function
text; matching attributes and disabling selected passes reduces variation
without proving the owner's machine-code criterion.

A stronger option is to compile the canonical sequential closure **once**
to an object and link that exact object into both images, keeping demand's
dispatcher and parallel bodies in separate objects and disabling LTO across
the boundary. Both W1 dispatch and the cheap-region edge call or tail-jump
to that same computation. This preserves internal sequential optimization
and fixes register allocation and frame layout by construction. Relocations,
linker relaxation and bindings still need the final disassembly check.
Its costs are the outer call or tail-transfer boundary, input preservation
across the once-per-entry decision, and compiler/linker management of a closed
common computation and its support. If the boundary separates the existing
seq adapter from its computation, it removes their current inlining
opportunity, and that cost must be measured against the existing seq image.
Alternatively the shared object can include the original sequential adapter
and preserve that inlining, placing demand's dispatcher outside it. The
candidate still pays the additional entry save/restore and transfer, which
remain timed and require the entry-state proof. Separate modules can meet the criterion
with a reused common object; independently regenerated code is not that
guarantee.

Comparing canonical sequential IR instead would mechanically establish
equal input to LLVM for the full closure, under LLVM's correctness contract.
It would permit different spills, frames and instruction counts and hence
would not prove equal machine cost. This requires the owner to change the
disassembly rule, and still requires correcting today's outlined clone IR.

The remaining question is where identity starts: the computation behind
an explicit entry boundary, with the one allowed entry decision and its
save/restore cost measured separately, or the entire timed entry. A timed
entry with a new pool call cannot literally equal an entry with no call.
Making both baselines pay the same dispatcher would conceal its cost
relative to the existing seq baseline. The common-object option can satisfy
normalized disassembly identity for the computation; it cannot make that
extra entry work disappear. Preserve the present failing criterion until
the owner chooses that boundary and the acceptable dispatch cost.

### Canonical sequential lowering before loop synthesis

The common first step for demand's construction alternatives is to preserve the
non-parallel lowering before any parallel synthesis. The question is whether
each sequential computation reached by W1 dispatch, cheap-region selection,
budget exhaustion or sequential refusal has the same pre-optimization LLVM
as the corresponding non-parallel build, including its ordinary helpers,
attributes and constants. Compare the complete recorded computation closure
after restoring only variant symbol names, and independently compare the
parallel computation before and after canonical emission. A surviving
synthesized chunk call in an original sequential function, a changed parallel
body, a lost dependency or a changed ordinary callable ABI rejects this step.
The compiler cases run in CI; no local build or execution qualifies it.

The proposed demand implementation retains a separately lowered `Off` program made
from the same checked program, selected entry roots and target before the
existing parallel builder runs. It emits that program with the ordinary
sequential emitter, then imports the computation's recorded definition,
declaration, type and attribute dependencies under canonical variant names.
Using the ordinary emitter matters: the old sequential-clone emission flag
also refused an ordinary dispatch optimization, so restoring the graph alone
would not restore the non-parallel LLVM input. Constant identity, references
and link-selected weak runtime contracts must survive the import. Original
source helpers reached below a selected root belong to this closure even if
their offers were pruned. Synthesized variants required only for captured-ABI
worker fallback remain separate from it.

For demand only, this replaces two former assumptions in the proposed design tree: that
upward hand-out reachability alone found every body requiring a sequential
variant, and that an outlined sequential chunk would inline back to the
non-parallel input. The parallel leaf still loops over a subrange; this step
does not select a common native object or an LLVM inlining boundary.

The registered `legacy-identity` control remains unchanged: `--par` emission
must match the pre-prototype compiler at `ac5f1498a` byte for byte on its nine
sources. Canonical construction is therefore limited to demand, with no
retained `Off` graph in eager modes. `--par` keeps the transformed-graph clones,
upward clone set and whole-function clone emission. Extending canonical
construction to `--par` requires a separate qualified comparison across Halo,
Firn and Snowghost builds; experiment 6 does not move that baseline.

The hosted control at `bb29a0dfc` reported `parallel/tree.wf` first and stopped
there. The broader On effects below are code deductions from the canonical
importer and the legacy LLVM definitions, not nine newly measured failures.
The earlier hosted artifacts at `968ed23c8` record all nine legacy pairs as
byte-identical to `ac5f1498a`. Every source has sequential clone roots: the
broader construction moved those original-function definitions from the
ordinary clone emission into the canonical import after module attributes,
so it changed module bytes even where a body's instructions could agree.

| Registered source | Additional effect of the reverted canonical On construction |
| --- | --- |
| `parallel/tree.wf` | Expanded the recursive closure to ordinary helpers such as `mix`, `leaf` and `branch`, redirecting formerly shared calls to new sequential variants. |
| `parallel/range_fold.wf` | Replaced `folded`'s captured chunk call with its in-place loop and added canonical `mix` and output helpers. |
| `parallel/indexed_reductions.wf` | Restored the histogram, extrema and product loops inside their source variants rather than calling synthesized reduction chunks; retained aggregate destination bodies and added construction helpers. |
| `compute/mandelbrot.wf` | Restored the rendering map inside `render_points` and added the previously shared `escape_count` to the canonical closure. |
| `compute/records.wf` | Restored the output map inside `summarize_records` and added `record_summary` and `validate_record`, including destination-form bodies. |
| `compute/fir.wf` | Restored the output and tap loops inside `filter` rather than entering a captured outer chunk, and added allocation/fill helpers. |
| `compute/stencil.wf` | Restored source row and grid loops and the two row phases inside their source variants; the time-step dependency remained sequential. |
| `compute/prefix.wf` | Restored source sum/block phases and added previously shared `scan_block`, preserving its sequential scan recurrence. |
| `compute/histogram.wf` | Restored source counting/merging phases and added previously shared `count_block`, keeping colliding scatter sequential inside that helper. |

This inspection predicts changes for all nine sources under the reverted
broad scope; it establishes neither optimized machine-code differences nor
performance. The unchanged [registered control](../../experiments/par-demand/legacy_identity.py)
must establish current On byte identity in CI. Internal compiler cases guard
the absence of a retained canonical graph and the original chunk-based
construction; they do not replace the independent old-revision comparison.

**Resource-support boundary.** The resource-record latch exists because
parallel callers can fail concurrently. A canonical sequential function can
run on a worker while other tasks remain live: budget exhaustion, refused
offers and cheap-region selection all transfer on the current thread. For
example, worker A can enter a sequential clone and exhaust the heap while
worker B is already writing the heap or stack record. Letting A use the
direct single-thread writer would permit a second/interleaved record or
abort before B completes. The existing process-wide latch is shared with
the stack-fault handler, and losing writers park. Preserve that support and
its native override binding; no per-world latch or unlatched canonical
failure path is safe.

Consequently canonical *computation* does not by itself make the entire
resource-support closure match seq's current direct writer. Full support
identity would need one canonical latched writer in the non-parallel baseline
too, or an explicitly shared cold support boundary. The former adds only
cold failure code/state and layout changes, but changes the baseline; the
latter needs an interface/scope decision. This step preserves the latch and
reports that remaining identity gap rather than removing concurrency safety.

Whole-program handler-word planning exposes another boundary: sequential and
parallel emission can choose different layouts for an enum, and even equal
word counts can store different worlds' handler addresses. For example, an
enum `Cell` used by an ordinary dispatch loop can carry one private handler
word in `Off`, while handing `Cell` to parallel calls makes it runtime-visible
and removes that word. A canonical entrance taking `&Cell` still has a thin
pointer signature, but reads incompatible storage. With one word in both
worlds, an incoming parallel handler pointer can instead enter a parallel
handler from canonical dispatch.

Preserve the parallel plan. Check actual canonical entrances, including
worker-fallback calls, transitively through parameters, results and owner
payloads; reject differing representations and every nonzero handler-word
crossover. Canonical-only private types and handler families can retain their
ordinary plan under isolated names. This is an unsupported backend boundary,
not a source-language rejection. A common handler binding or representation
conversion needs a separate decision; returning to the synthesized sequential
body would hide the gap.

### Hosted identity after canonical demand lowering at 39401dd11

The hosted check at `39401dd11` (compute-bench run `38150667865`) no longer
reports the computation functions or their sequential helper closures as
different. Its remaining operand-resolution failures are the heap TLS slot
and the external-object GOT slot used by `wf_bridge_fail`. The checker now
resolves local-exec TLS through the ELF TLS segment and symbol bounds, and
mutable RIP references through bounded objects or exact dynamic object
relocations. It compares object identity and the byte offset within that
object, retaining external symbol versions and relocation addends. Different
objects or bytes still differ; unresolved identities still fail construction.

The table below records the expected remaining failures after that checker
repair, derived from the complete hosted differences rather than a new run.
CI must confirm the repaired operand comparisons. A `latched-writer` requires
the shared atomic acquisition and loser parking, with the remaining write and
abort CFG unchanged. An `entry-adapter` requires a proved pool/region dispatch
and a completely compared selected sequential closure; it retains the first
differing machine instruction. Neither category admits timing.

| Workload | Remaining function(s) | Category and evidence | Depends on |
| --- | --- | --- | --- |
| small_constant | None expected | Only the two object-resolution failures were reported. | CI verification of named operands. |
| small_split | `wf_resource_record_abort` | latched-writer: direct writer versus `wf__floor_record_latch` acquisition. | Owner's identity-construction card: resource-support boundary. |
| recursion | `wf_bench_micro` | entry-adapter: `push %rax` versus `push %saved2.0` at instruction 3. | Owner's identity-construction card: timed entry boundary. |
| spine | `wf_bench_micro` | entry-adapter: `mov %rdx,%rax` versus `push %saved0.0` at instruction 1. | Owner's identity-construction card: timed entry boundary. |
| large_helper | `wf_bench_micro` | entry-adapter: the same instruction-1 difference as spine. | Owner's identity-construction card: timed entry boundary. |
| mandelbrot | `wf_bench_mandelbrot`; `wf_resource_record_abort` | entry-adapter: `push %saved0.0` versus `push %rbp` at instruction 1; latched-writer. | Owner's identity-construction card: timed entry and resource support. |
| records | `wf_bench_records`; `wf_resource_record_abort` | entry-adapter: `mov 0x30(%rsp),%saved2.0` versus `push %saved3.0` at instruction 4; latched-writer. | Owner's identity-construction card: timed entry and resource support. |
| fir | `wf_bench_fir`; `wf_resource_record_abort` | entry-adapter: `push %rbp` versus `push %saved0.0` at instruction 1; latched-writer. | Owner's identity-construction card: timed entry and resource support. |
| stencil | `wf_bench_stencil`; `wf_resource_record_abort` | entry-adapter: `mov %r9,%saved2.0` versus `push %saved3.0` at instruction 4; latched-writer. | Owner's identity-construction card: timed entry and resource support. |
| prefix | `wf_bench_prefix`; `wf_resource_record_abort` | entry-adapter: `push %saved0.0` versus `push %rbp` at instruction 1; latched-writer. | Owner's identity-construction card: timed entry and resource support. |
| histogram | `wf_bench_histogram`; `wf_resource_record_abort` | entry-adapter: `mov %r9,%saved2.0` versus `push %saved3.0` at instruction 4; latched-writer. | Owner's identity-construction card: timed entry and resource support. |

For every reported writer difference, the first difference is instruction 4:
seq prepares `0x14` while demand calls the latch accessor. The latch remains
necessary on worker-reachable failure paths. For the wrappers, the pool query
and its consequences for register/frame allocation remain actual construction
differences, qualified by the projection boundary described in
[Hosted construction attribution at 968ed23c8](#hosted-construction-attribution-at-968ed23c8).
The additive computation status covers the canonical body/helper closure,
excluding those explicitly evidenced support and entry differences; it does
not prove the entire timed wrapper identical or its entry state preserved.
The overall identity status and process exit remain failing while any
construction difference remains, pending the owner's identity-construction
choice.

The next hosted check at `75e1c249e` (run `38152065332`) resolved the external
GOT reference and the direct TLS load. It exposed a second TLS spelling:
`wf__heap_change` loads the thread pointer with `mov %fs:0,%rcx`, then forms
the address with `lea -0x58(%rcx),%rcx` in seq and `lea -0x70(%rcx),%rcx` in
demand. Resolving this address requires register provenance through the CFG,
not just direct segment operands. Only a proved thread-pointer-derived base
may supply the TLS object identity; clobbers or disagreeing incoming paths
must retain the raw operand.

The attribution also required a surviving optimized canonical call. Spine,
large_helper, mandelbrot and fir inline that entry into the inactive wrapper
path, so this requirement left them ordinary failures even though the raw
adapter selects the canonical entry. Records, stencil, prefix and histogram
retain that call, but their complete-closure evidence was blocked by the TLS
address difference. Recursion retains the call and has no heap-helper failure,
so it already qualified. Attribution for an inlined entry requires the raw
inactive adapter to match seq's adapter after canonical callee naming,
including its call inputs and result handling, plus the optimized pool CFG
projection. This is provenance evidence for the entry-adapter category, not
proof of equal inlined machine instructions or preserved state across the
removed query; construction remains failing.

Rechecking the saved `38152065332` images with these repairs reports
`Covered canonical sequential body/helper closure: pass; full construction:
fail` and exits 1. No heap-helper or ordinary failures remain. The remaining
16 differences are the nine timed wrappers classified as entry-adapter and
the seven resource writers classified as latched-writer, as listed above;
small_constant and hot_helper pass construction. No artifact executable was
run. This establishes operand identity and the stated attribution on those
saved images, without settling the owner's construction boundary or allowing
timing.

### Identity construction: option D

The owner chose **D on the identity-construction card on 2026-10-11**:
retain canonical sequential lowering and the per-build normalized-disassembly
gate for every covered computation closure. The resource-failure record
writer is an explicit cold-path boundary; the timed wrapper's pool query is
entry dispatch, counted and timed separately under the original
count-entry-dispatch ruling. This supersedes the earlier requirement that
these two known construction differences themselves stop timing. It does
not replace computation identity with IR identity. Option A, compiling a
single canonical object without cross-boundary LTO, remains the fallback
for a workload whose computation closure differs.

Admission requires complete evidence, not a category label. The cold
`wf_resource_record_abort` boundary must preserve the complete write/abort
IR and differ only by the shared latch acquisition and losing-writer park,
with the corresponding actual-image latch/atomic evidence. A changed writer
operation or an unknown support difference still fails. Entry attribution
requires the exact inactive adapter projection, canonical call inputs and
result handling, corresponding computation closure comparisons and the
optimized pool CFG proof. Count zero queries where selection was pruned,
otherwise exactly one query per timed entry, outside every computation loop;
count any cheap-region decision separately and retain its covered active
paths. Inlined entries require canonical IR provenance as well as the actual
pool projection. Unknown differences, incomplete evidence, missing regions
or differing computation still fail before timing. Reports retain the first
differing instructions and list both admitted categories and their counts.

**Separate entry-dispatch measurement, fixed before it measures.** A generic
native witness links the real demand runtime and compares a noinline direct
entry with a noinline pool-query/conditional-transfer entry. The active and
inactive destinations are separately compiled opaque leaves with identical
computation, without LTO. Hosted disassembly must establish exactly one query
outside cycles, both conditional destinations and equal leaf computation.
This measures the query and conditional transfer; it does not attribute the
real workload wrapper's register allocation, saves or spills to dispatch.
Both remain present in the full-workload measurements.

Use the same W1/2/4/8 placement and demand settings. Record first-query wall
and CPU separately, warm the witness, then begin a paired pilot at 1,024
entries and double up to 1,048,576 until both batches last at least
`max(1 ms, 1000 * CLOCK_MONOTONIC resolution)`. Freeze the entry count at each
width; reaching the cap without this precision is reported as inconclusive.
Alternate direct/dispatch batch order. Retain six sizing pairs separately
from the full-work sizing's frozen `n` decisive pairs. Report paired wall and
CPU differences per entry, raw samples, counts and the existing 10,000-draw,
seed-20261010, 95% bootstrap. These are diagnostic entry costs, with no
1 ns/2% allowance or additional timing rejection; that earlier allowance
continues to apply only to nonhoistable per-call decisions.

Dispatch `compute-bench.yml` on this branch with `experiment=par-demand`,
`par_demand_experiment=6`, `placement_runner=14900k` and empty
`placement_rounds`; leave other inputs at defaults. Six full-work sizing
rounds select `n` in 6..30 by the unchanged interval projection. Each full
round has 264 workload image invocations and 528 timed workload calls, plus
four paired entry-witness processes. If the six measured rounds, including
paired entry diagnostics, take `S` seconds, initial sizing plus decisive
measurement projects to
`S * (1 + n/6)`; rerunning every cell once would project to
`S * (1 + 2*n/6)`. These estimates exclude witness calibration, reduction,
hosted construction and queue time, and assume the sample's throughput
persists. No experiment-6 sizing duration exists yet; sizing chooses rounds,
not a fixed duration. The timing command retains its 90-minute timeout and
the runner job its 120-minute timeout; neither is a performance prediction.


### Experiment 6 results at d8adf2c2a

[Compute-bench run 38179220701](https://github.com/Ming-Research/Whitefoot/actions/runs/38179220701) measured revision `d8adf2c2a3b9519b94a920cc7e58397d1d511077` on 2026-10-11. The hosted build ran 22:08-22:15 UTC; native i9-14900K timing ran 22:15:50-22:42:56 UTC (27 minutes 6 seconds). These timestamps are the run record supplied with the artifact, not timings inferred from workload samples. Both jobs succeeded; execution success does not select an experimental verdict. Evidence is the run's `par-demand-results-14900k` artifact: `summary.json`, `measurements.tsv`, `sizing-e6/`, `identity.json`, `sequential-identity.json`, `entry-dispatch-measurements.json`, image files and host records. Field paths below identify that frozen artifact; no local measurement or native execution was performed for this reading.

`identity.json` records a clean revision, Linux on MBSDESKTOP, six separate sizing rounds and 30 frozen decisive rounds, with one further 30-round attempt for spine W4, spine W8 and fir W1. Pins are W1 {2}, W2 {2,4}, W4 {2,4,6,8}, W8 {2,4,6,8,10,12,14,1}, retaining the CPU-1 sharing caveat. Seq, par and shifted seq use off-never-request; demand, previous, unversioned and twin use on. Inputs and repetitions are the registered manifest, retained in `identity.json.manifest`; startup call 0 is separate and call 1 decides. Paired intervals retain 10,000 bootstrap draws, seed 20261010 and 95% coverage. Build-host records identify Ubuntu clang 18.1.3; measurement-host identifies Intel Core i9-14900K. The measurement-host shell's `rounds=10` is an unused default environment value; the authoritative frozen count is 30 in `identity.json.rounds` and `summary.json.decisive_rounds`.

**Registered verdict: rejected, not qualified for adoption.** E6-keep rejects spine W4/W8, and the retained par criterion rejects fir W1. E6-identity is supported under D. Recursion recovery remains undecided despite meeting its numerical target: every cell lacks the registered optimized hot-site inspection. The saved reducer consequently reports 3 failed, 7 void and 28 inconclusive cells, with no qualified timing passes. There is no `inspection.json` in the artifact; neither constructing new inspection after seeing the result nor borrowing identity's different proof supplies that missing attribution for this reading.

The seven initial twin exclusions are large_helper W1/W8, records W8, fir W4, stencil W4, prefix W4 and histogram W1 (`summary.json.cells[workload,width].initial.twin_wall_ratio.interval`). Their cells and floor readings are void. All six twin intervals in the three rerun cells include 1. The registered one-rerun limit stands; no extra attempt is introduced. Numerical success without inspection is UNDECIDED, twin exclusion is VOID (undecided), and persistent registered rejection is **FAIL**. Entries below are median [95% interval], except counts, widths and envelope endpoints. Source fields remain authoritative when rounding is near a bound.

**Recursion recovery and retention of existing par speedups.** E6-recursion is undecided: the W8 interval meets 1.05, but the retained hot-site inspection obligation is missing. The retained E6-par bound is rejected by fir W1 in both attempts. Other shown numerical passes remain undecided for missing inspection; VOID rows cannot support a verdict. Controls with no applicable par rule are omitted here and remain in the artifact.

| Workload / width (identifier / threads; baseline = n/a; ideal = n/a) | Demand / par wall (ratio; baseline = par image at same width; ideal upper CI <= 1.05) | Verdict (category; baseline = registered E6-par/recursion rule; ideal = supported) |
| --- | --- | --- |
| recursion W1 | 1.004893 [0.990815, 1.043269] | UNDECIDED |
| recursion W4 | 1.023093 [0.994274, 1.029245] | UNDECIDED |
| recursion W8 | 0.996933 [0.984322, 1.002980] | UNDECIDED |
| large_helper W1 | 1.000235 [1.000120, 1.000336] | VOID |
| large_helper W4 | 0.515925 [0.515363, 0.516335] | UNDECIDED |
| large_helper W8 | 0.277544 [0.277244, 0.278039] | VOID |
| mandelbrot W4 | 1.006865 [1.003100, 1.009530] | UNDECIDED |
| mandelbrot W8 | 1.008187 [1.003523, 1.010954] | UNDECIDED |
| records W4 | 0.993205 [0.989393, 0.997003] | UNDECIDED |
| records W8 | 0.994315 [0.977797, 1.017640] | VOID |
| fir W1 | **FAIL 1.052447 [1.050722, 1.054529]; rerun 1.052762 [1.050590, 1.054853]** | FAIL |
| fir W4 | 0.658110 [0.652399, 0.674690] | VOID |
| fir W8 | 0.495685 [0.489460, 0.498396] | UNDECIDED |
| stencil W2 | 1.004510 [0.987279, 1.017654] | UNDECIDED |
| stencil W4 | 1.002198 [0.988890, 1.015943] | VOID |
| stencil W8 | 1.002483 [0.997954, 1.008585] | UNDECIDED |
| prefix W4 | 1.002956 [0.992833, 1.013570] | VOID |
| prefix W8 | 1.017278 [1.008572, 1.022347] | UNDECIDED |
| histogram W1 | 1.019498 [1.017429, 1.025198] | VOID |
| histogram W2 | 0.992506 [0.986508, 0.996873] | UNDECIDED |
| histogram W4 | 0.990270 [0.962115, 1.013545] | UNDECIDED |
| histogram W8 | 1.011934 [0.983660, 1.034089] | UNDECIDED |

Source: `summary.json.cells[workload,width].initial.rules["E6-par"].{median,interval,status}`, `.rerun.rules["E6-par"]` where present, and `.verdicts["E6-par"]`. The recursion W8 row is the unconditional E6-recursion comparison.


**Preservation against both controls.** E6-keep is rejected by spine W4 and W8 against previous demand; both attempts lie wholly above 1.05 and both twins include 1. All other preservation verdicts are undecided, including numerical passes, because inspection is missing or the twin voids the cell. Small_constant W4/W8 additionally have intervals straddling 1.05. No missing inspection is supplied retrospectively by the identity gate.

| Workload / width (identifier / threads; baseline = n/a; ideal = n/a) | Demand / unversioned wall (ratio; baseline = unversioned image, same width; ideal upper CI <= 1.05) | Demand / previous wall (ratio; baseline = request-gated previous image, same width; ideal upper CI <= 1.05) | Verdicts, unversioned / previous (categories; baseline = E6-keep; ideal = supported / supported) |
| --- | --- | --- | --- |
| small_constant W1 | 1.005863 [0.988528, 1.035520] | 1.001803 [0.988563, 1.020226] | UNDECIDED / UNDECIDED |
| small_constant W4 | 1.014327 [0.976611, 1.075592] | 1.004902 [0.927074, 1.108640] | UNDECIDED / UNDECIDED |
| small_constant W8 | 0.980231 [0.928369, 1.095703] | 0.967384 [0.880205, 1.023857] | UNDECIDED / UNDECIDED |
| small_split W1 | 1.000042 [0.999644, 1.000601] | 0.999743 [0.999409, 1.000385] | UNDECIDED / UNDECIDED |
| small_split W4 | 0.914259 [0.911753, 0.916225] | 1.000050 [0.999592, 1.000559] | UNDECIDED / UNDECIDED |
| small_split W8 | 0.912307 [0.899808, 0.914623] | 0.999833 [0.999234, 1.000712] | UNDECIDED / UNDECIDED |
| recursion W1 | 1.009996 [0.978460, 1.039036] | 1.008445 [0.981249, 1.037289] | UNDECIDED / UNDECIDED |
| recursion W4 | 1.007964 [0.995674, 1.033425] | 0.982662 [0.948137, 1.002928] | UNDECIDED / UNDECIDED |
| recursion W8 | 1.001239 [0.996377, 1.011697] | 0.928796 [0.905733, 0.956744] | UNDECIDED / UNDECIDED |
| spine W1 | 1.000694 [0.999313, 1.002177] | 0.999901 [0.997518, 1.001261] | UNDECIDED / UNDECIDED |
| spine W4 | 0.999996 [0.990567, 1.007289]; rerun 0.999606 [0.988919, 1.006974] | **FAIL 1.289710 [1.268600, 1.301329]; rerun 1.292912 [1.282670, 1.301292]** | UNDECIDED / FAIL |
| spine W8 | 1.002082 [0.993890, 1.006549]; rerun 1.003421 [0.998602, 1.008466] | **FAIL 1.279648 [1.267138, 1.285284]; rerun 1.297710 [1.283602, 1.307440]** | UNDECIDED / FAIL |
| hot_helper W1 | 0.999914 [0.999783, 1.000051] | 0.999942 [0.999835, 1.000044] | UNDECIDED / UNDECIDED |
| hot_helper W4 | 1.000124 [0.999925, 1.000254] | 0.999906 [0.999731, 1.000101] | UNDECIDED / UNDECIDED |
| hot_helper W8 | 1.000028 [0.999920, 1.000207] | 0.999916 [0.999661, 1.000037] | UNDECIDED / UNDECIDED |
| large_helper W1 | 1.000167 [0.999904, 1.000287] | 1.000028 [0.999963, 1.000208] | VOID / VOID |
| large_helper W4 | 1.000228 [0.998421, 1.002393] | 1.000533 [0.998756, 1.001253] | UNDECIDED / UNDECIDED |
| large_helper W8 | 1.000351 [0.998363, 1.002020] | 0.999573 [0.998802, 1.001845] | VOID / VOID |
| mandelbrot W1 | 1.001813 [0.998209, 1.002898] | 0.999234 [0.998121, 1.002221] | UNDECIDED / UNDECIDED |
| mandelbrot W4 | 0.999111 [0.995064, 1.000604] | 0.997668 [0.993924, 1.002002] | UNDECIDED / UNDECIDED |
| mandelbrot W8 | 1.000191 [0.998424, 1.002330] | 0.999688 [0.994891, 1.004761] | UNDECIDED / UNDECIDED |
| records W1 | 1.000958 [0.999952, 1.003349] | 1.002178 [0.999600, 1.003844] | UNDECIDED / UNDECIDED |
| records W4 | 1.000300 [0.995224, 1.003100] | 0.998479 [0.994253, 1.003195] | UNDECIDED / UNDECIDED |
| records W8 | 1.003859 [0.978639, 1.018964] | 0.992838 [0.982610, 1.006171] | VOID / VOID |
| fir W1 | 0.998703 [0.996273, 1.001888]; rerun 1.001118 [0.999706, 1.002371] | 1.000709 [0.997612, 1.001955]; rerun 1.000455 [0.998758, 1.001997] | UNDECIDED / UNDECIDED |
| fir W4 | 0.999906 [0.999158, 1.001790] | 1.000382 [0.998507, 1.001363] | VOID / VOID |
| fir W8 | 1.000043 [0.998986, 1.002325] | 1.000590 [0.998784, 1.002403] | UNDECIDED / UNDECIDED |
| stencil W1 | 0.999834 [0.998381, 1.001361] | 0.998164 [0.996846, 1.002506] | UNDECIDED / UNDECIDED |
| stencil W2 | 1.005057 [0.987921, 1.018349] | 0.998937 [0.991778, 1.016999] | UNDECIDED / UNDECIDED |
| stencil W4 | 1.008793 [0.994862, 1.022593] | 1.005357 [0.993255, 1.013013] | VOID / VOID |
| stencil W8 | 1.001216 [0.993516, 1.009861] | 1.001132 [0.994934, 1.004471] | UNDECIDED / UNDECIDED |
| prefix W1 | 1.005280 [0.997872, 1.008594] | 1.000332 [0.994032, 1.009704] | UNDECIDED / UNDECIDED |
| prefix W4 | 1.010273 [0.994435, 1.017529] | 1.003239 [0.992495, 1.024004] | VOID / VOID |
| prefix W8 | 0.995392 [0.983817, 1.002991] | 0.993157 [0.985617, 1.002179] | UNDECIDED / UNDECIDED |
| histogram W1 | 0.999706 [0.996755, 1.004071] | 0.999218 [0.996683, 1.003779] | VOID / VOID |
| histogram W2 | 1.002140 [0.994642, 1.011326] | 0.991464 [0.979659, 1.014097] | UNDECIDED / UNDECIDED |
| histogram W4 | 1.003632 [0.984133, 1.010912] | 1.011209 [0.988206, 1.026785] | UNDECIDED / UNDECIDED |
| histogram W8 | 1.017903 [0.977059, 1.043411] | 1.031259 [1.002781, 1.039246] | UNDECIDED / UNDECIDED |

Source: `summary.json.cells[workload,width].initial.rules["E6-keep-unversioned"/"E6-keep-previous"].{median,interval,status}`, corresponding `.rerun.rules` where shown, and `.verdicts`.


**Construction identity and entry counts.** E6-identity is supported for all twelve covered W1 computations and small_split's active cheap paths. The D exclusions remain explicit: seven cold latched writers and nine entry adapters, not literal whole-image identity. All decisions are outside computation loops.

| Workload (identifier; baseline = n/a; ideal = n/a) | Covered widths (threads; baseline = registered cells; ideal = all required) | Canonical closure verdict (category; baseline = seq normalized computation; ideal = pass) | Pool queries per timed entry (count; baseline = registered entry selection; ideal = 1, or 0 when pruned) | Cheap-region decisions per selected entry (count; baseline = invariant region selection; ideal = 1 if present, otherwise 0) |
| --- | --- | --- | --- | --- |
| small_constant | 1 | pass | 0 | 0 |
| small_split | 1,2,4,8 | pass | 1 | 1 |
| recursion | 1 | pass | 1 | 0 |
| spine | 1 | pass | 1 | 0 |
| hot_helper | 1 | pass | 0 | 0 |
| large_helper | 1 | pass | 1 | 0 |
| mandelbrot | 1 | pass | 1 | 0 |
| records | 1 | pass | 1 | 0 |
| fir | 1 | pass | 1 | 0 |
| stencil | 1 | pass | 1 | 0 |
| prefix | 1 | pass | 1 | 0 |
| histogram | 1 | pass | 1 | 0 |

Source: `sequential-identity.json.workloads[workload].{covered_widths,computation_status}`; `.entry_decisions[workload].{pool_decisions_per_timed_call,outside_loops,regions[].decisions_per_active_timed_call}`. Exclusion counts: top-level `.category_counts`.


**Never-slower diagnostic and same-job floor.** This is diagnostic, not an additional rejection gate. Values are initial attempt intervals; fir W1 includes its mandated rerun. FAIL marks a literal interval wholly above 1 outside an available floor, or at a wider width where no floor is allowed; it is not a new experiment-rejection criterion. AT FLOOR suppresses a floor-relative failure while retaining the literal classification. A straddle is undecided. All ratios for void cells are displayed only as observations, not qualified evidence.

| Workload / width (identifier / threads; baseline = n/a; ideal = n/a) | Demand / seq wall (ratio; baseline = single-thread seq image; ideal upper CI <= 1.00) | Literal diagnostic (category; baseline = 1.00 plus registered floor rule; ideal = supported or at floor) | Layout envelope (ratio bounds; baseline = shifted-seq / seq at W1 only; ideal = entire demand interval contained) |
| --- | --- | --- | --- |
| small_constant W1 initial | 1.005966 [0.993200, 1.015815] | UNDECIDED: straddle | [0.954632, 1.045368] at the floor |
| small_constant W4 initial | 0.986150 [0.886090, 1.058886] | UNDECIDED: straddle | n/a at this width |
| small_constant W8 initial | 0.990968 [0.898619, 1.051597] | UNDECIDED: straddle | n/a at this width |
| small_split W1 initial | 1.000134 [0.999850, 1.000586] | UNDECIDED: straddle | [0.999035, 1.000965] at the floor |
| small_split W4 initial | 1.000279 [1.000036, 1.001489] | **FAIL diagnostic only** | n/a at this width |
| small_split W8 initial | 1.000394 [0.999077, 1.001069] | UNDECIDED: straddle | n/a at this width |
| recursion W1 initial | 0.992952 [0.978638, 1.029725] | UNDECIDED: straddle | [0.951105, 1.048895] at the floor |
| recursion W4 initial | 0.259064 [0.252534, 0.265631] | supported literal | n/a at this width |
| recursion W8 initial | 0.128849 [0.127445, 0.132137] | supported literal | n/a at this width |
| spine W1 initial | 0.996526 [0.994969, 0.998274] | supported literal | [0.994934, 1.005066] at the floor |
| spine W4 initial | 3.694303 [3.685246, 3.720779] | **FAIL diagnostic only** | n/a at this width |
| spine W4 rerun | 3.687732 [3.671385, 3.705936] | **FAIL diagnostic only** | n/a at this width |
| spine W8 initial | 4.491429 [4.471548, 4.503199] | **FAIL diagnostic only** | n/a at this width |
| spine W8 rerun | 4.490251 [4.457230, 4.517177] | **FAIL diagnostic only** | n/a at this width |
| hot_helper W1 initial | 1.000184 [1.000077, 1.000322] | literal exceeds; AT FLOOR | [0.999583, 1.000417] at the floor |
| hot_helper W4 initial | 1.000080 [0.999973, 1.000435] | UNDECIDED: straddle | n/a at this width |
| hot_helper W8 initial | 1.000078 [0.999881, 1.000385] | UNDECIDED: straddle | n/a at this width |
| large_helper W1 initial | 0.993129 [0.992437, 0.993996] | VOID (raw pass) | VOID |
| large_helper W4 initial | 0.256536 [0.256212, 0.256909] | supported literal | n/a at this width |
| large_helper W8 initial | 0.138079 [0.137889, 0.138299] | VOID (raw pass) | VOID |
| mandelbrot W1 initial | 0.998149 [0.994556, 1.002658] | UNDECIDED: straddle | [0.993608, 1.006392] at the floor |
| mandelbrot W4 initial | 0.261169 [0.260387, 0.261854] | supported literal | n/a at this width |
| mandelbrot W8 initial | 0.137419 [0.136628, 0.138586] | supported literal | n/a at this width |
| records W1 initial | 1.000790 [0.996790, 1.001860] | UNDECIDED: straddle | [0.996972, 1.003028] outside the floor |
| records W4 initial | 0.272107 [0.269867, 0.272609] | supported literal | n/a at this width |
| records W8 initial | 0.148684 [0.144757, 0.151549] | VOID (raw pass) | VOID |
| fir W1 initial | 1.003976 [1.002556, 1.006617] | **FAIL diagnostic only** | [0.998135, 1.001865] outside the floor |
| fir W1 rerun | 1.005104 [1.003721, 1.007085] | **FAIL diagnostic only** | [0.998296, 1.001704] outside the floor |
| fir W4 initial | 0.248124 [0.247381, 0.248469] | VOID (raw pass) | VOID |
| fir W8 initial | 0.129878 [0.129516, 0.130090] | supported literal | n/a at this width |
| stencil W1 initial | 0.999125 [0.997058, 1.000734] | UNDECIDED: straddle | [0.995357, 1.004643] at the floor |
| stencil W2 initial | 0.669962 [0.663935, 0.680522] | supported literal | n/a at this width |
| stencil W4 initial | 0.586899 [0.552826, 0.607105] | VOID (raw pass) | VOID |
| stencil W8 initial | 0.507113 [0.475940, 0.531152] | supported literal | n/a at this width |
| prefix W1 initial | 1.002187 [0.994474, 1.008434] | UNDECIDED: straddle | [0.989548, 1.010452] at the floor |
| prefix W4 initial | 0.868507 [0.851031, 0.883347] | VOID (raw pass) | VOID |
| prefix W8 initial | 0.836449 [0.824380, 0.846769] | supported literal | n/a at this width |
| histogram W1 initial | 1.002820 [0.997397, 1.007920] | VOID (raw inconclusive) | VOID |
| histogram W2 initial | 0.911546 [0.898136, 0.919567] | supported literal | n/a at this width |
| histogram W4 initial | 0.876601 [0.854238, 0.892962] | supported literal | n/a at this width |
| histogram W8 initial | 0.858484 [0.850899, 0.879130] | supported literal | n/a at this width |

Source: `summary.json.cells[workload,width].initial.literal_seq_diagnostic.{median,interval,status}` and `.initial.layout_floor.{band,status,controls}`; corresponding `.rerun` fields where labeled. Envelope follows the preregistered max-endpoint radius, not another workload's floor.


Eight W1 readings are at the floor; records and fir are outside it; large_helper and histogram have no usable floor because their twins exclude 1. Records straddles 1, so its literal result is undecided. Fir is wholly above 1 in both attempts, outside both floors. Hot_helper's tiny literal exceedance is recorded at the floor, not a failure.

**H3 and continued scaling.** Reporting H3 is supported: every cell has the registered CPU margin and interval. Its sign is not a gate. The table shows the complete initial demand sweep only; no selectively rerun widths are mixed in. Positive stencil W8 H3 does not establish continuing speedup because comparisons touching W4 are void. Histogram 8/4 straddles 1 while CPU rises, so further speedup is unestablished. The two nonvoid adjacent numerical speedups still lack surviving-work inspection for CPU attribution.

| Workload / width (identifier / threads; baseline = n/a; ideal = n/a) | Demand wall (ms; baseline = same-width seq in artifact; ideal <= seq wall) | Demand process CPU (ms; baseline = same-width seq CPU; diagnostic ideal = lower for equal wall benefit, no gate) | Demand H3 margin (dimensionless; baseline = registered seq CPU/wall allowance; diagnostic ideal <= 0, no gate) | Higher / preceding demand wall (ratio; baseline = preceding sweep width; ideal upper CI < 1) | Higher / preceding demand CPU (ratio; baseline = preceding sweep width; ideal <= 1 at equal wall benefit, no gate) | Scaling qualification (category; baseline = registered sweep/twin/inspection rules; ideal = demonstrated faster, valid) |
| --- | --- | --- | --- | --- | --- | --- |
| stencil W1 | 72.245 [72.129, 72.340] | 72.239 [72.127, 72.335] | -0.1006 [-0.1032, -0.0992] | n/a | n/a | inconclusive; inspection missing |
| stencil W2 | 48.388 [48.107, 49.088] | 78.634 [78.048, 80.073] | -0.0763 [-0.0876, -0.0472] | 0.6725 [0.6670, 0.6855] | 1.0948 [1.0838, 1.1207] | demonstrated faster; inspection missing |
| stencil W4 | 39.538 [39.238, 40.083] | 103.768 [102.622, 105.791] | 0.2555 [0.1908, 0.3488] | 0.8181 [0.8098, 0.8215] | 1.3106 [1.3002, 1.3264] | void; inspection missing |
| stencil W8 | 34.459 [34.327, 34.851] | 148.149 [146.788, 151.678] | 0.6970 [0.5352, 0.8067] | 0.8755 [0.8664, 0.8826] | 1.4344 [1.4213, 1.4632] | void; inspection missing |
| histogram W1 | 23.112 [22.931, 23.282] | 23.112 [22.931, 23.282] | -0.0972 [-0.1029, -0.0924] | n/a | n/a | void; inspection missing |
| histogram W2 | 21.103 [20.779, 21.418] | 23.908 [23.596, 24.246] | -0.0917 [-0.1006, -0.0739] | 0.9138 [0.9071, 0.9362] | 1.0366 [1.0283, 1.0561] | void; inspection missing |
| histogram W4 | 20.322 [19.968, 20.502] | 26.361 [26.032, 26.528] | -0.0132 [-0.0507, 0.0087] | 0.9591 [0.9457, 0.9708] | 1.0972 [1.0841, 1.1083] | demonstrated faster; inspection missing |
| histogram W8 | 20.622 [20.228, 20.992] | 32.624 [31.816, 32.773] | 0.1458 [0.1222, 0.1777] | 1.0182 [0.9874, 1.0471] | 1.2341 [1.2027, 1.2571] | uncertain; inspection missing |

Source: `summary.json.sweep[workload].widths[width].wall_ns.demand`, `.cpu_ns.demand`, `.h3.demand`, `.validity`, `.inspection`; `.adjacent_widths[arm="demand",higher_width=width].{higher_over_lower_wall,higher_over_lower_cpu,scaling}`. ns converted to ms by division by 1,000,000. All 38 demand H3 observations remain in `.cells[].initial.h3.demand`; other arms are also retained.


**Separate entry-dispatch cost.** All four widths reach the registered 1,048,576-entry cap without both batches meeting the precision rule. These observed distributions are therefore precision-undecided, not qualified evidence of a 1 ns bound; the 1 ns/2% nonhoistable-call rule does not apply here. First-query values include pool initialization at active widths. Process CPU includes concurrent runtime CPU; it is not an isolated instruction cost. The generic witness does not measure workload-wrapper spills/saves.

| Workload / width (identifier / threads; baseline = n/a; ideal = n/a) | Dispatch minus direct wall / entry (ns/entry; baseline = direct no-query witness; diagnostic ideal = 0, no gate) | Dispatch minus direct process CPU / entry (ns/entry; baseline = direct witness; diagnostic ideal = 0, no gate) | First query wall (us; baseline = no query, 0 us; diagnostic ideal = 0, no gate) | First query process CPU (us; baseline = no query, 0 us; diagnostic ideal = 0, no gate) | Precision verdict (category; baseline = registered pilot stopping rule; ideal = sufficient) |
| --- | --- | --- | --- | --- | --- |
| generic witness W1 | 0.9992 [0.9955, 1.0018] | 0.9991 [0.9954, 1.0018] | 0.167 [0.164, 0.172] | 0.474 [0.470, 0.508] | UNDECIDED: calibration cap |
| generic witness W2 | 1.0012 [1.0000, 1.0037] | 2.0613 [2.0449, 2.0658] | 62.379 [53.072, 101.112] | 62.886 [53.648, 101.527] | UNDECIDED: calibration cap |
| generic witness W4 | 1.0007 [0.9971, 1.0062] | 4.2061 [4.1266, 4.2539] | 133.609 [127.263, 137.304] | 142.174 [137.328, 149.901] | UNDECIDED: calibration cap |
| generic witness W8 | 1.0037 [1.0025, 1.0063] | 8.5838 [8.5335, 8.6174] | 224.190 [198.167, 243.975] | 401.589 [354.176, 488.923] | UNDECIDED: calibration cap |

Source: `summary.json.entry_dispatch.widths[width].decisive.{wall_delta_ns_per_entry,cpu_delta_ns_per_entry,first_query_wall_ns,first_query_cpu_ns}` and `.precision`, `.calls_per_batch`; raw pairs in `entry-dispatch-measurements.json.decisive[width,round]`. First-query ns divided by 1,000 for us.


The computation evidence establishes the D-qualified sequential computation boundary on these twelve sources, not literal identity of whole wrappers or concurrent cold failure support. The eager budget change numerically recovers recursion W8, but does not preserve the request-gated candidate on spine; this run cannot attribute that regression beyond the paired control and cannot claim an unconditional never-slower program. W1 fir's diagnostic lies outside its own layout envelope in both attempts, while wider spine is substantially slower than seq. Histogram's eighth worker step has no demonstrated wall benefit, and void stencil comparisons cannot establish its continued scaling condition.

The next decision is how to revise the general eager-recursion policy after the spine preservation rejection and address fir's retained par failure; the current candidate has not met its adoption criteria. Missing hot-site evidence, twin validity and capped entry precision also need to be resolved for a newly registered follow-up. This result establishes no bound for other programs, machines, placements, sizes, widths beyond 1/2/4/8, or nonhoistable per-call decisions. It supplies no universal layout-causality explanation and no unconditional CPU-cost exemption.


### After experiment 6

**Proposed analysis and plan; no compiler policy or follow-up measurement is selected here.** The rejected run is evidence for investigation, not a reason to repeat the same round. The observations below use only revision d8adf2c2a's `par-demand-results-14900k` artifact and the source at that revision. They leave experiment 6's registration and verdict unchanged. New predictions below would be registered for a distinct, owner-selected follow-up before any measurement.

**Spine: removal of request gating is established; its dynamic cost mechanism is not yet counted.** In `demand/spine.ll` and `previous/spine.ll`, `wf_spine`, `wf_workload`, `wf__par_seq_spine`, `wf__par_seq_workload` and `wf_bench_micro` have identical IR bodies. The actual timed wrapper also compares identically after address/register normalization. Both link the same `native-demand` objects, requests are on in both, and the previous arm differs only by `--par-demand-ablation requested` (`Makefile`'s flags/native-previous definitions). This is not a change of canonical sequential regions, loop synthesis, entry dispatch, scheduler implementation or idle policy between those two arms.

The budgeted recursive body is the discriminating code. In `demand/spine.disassembly`, `wf__par_budget_spine` at 0x2a00 checks its budget/base case at 0x2a0c-0x2a20 and unconditionally attempts lane acquisition at 0x2a2f, then publishes at 0x2a51, joins at 0x2a5d and releases at 0x2a69. In `previous/spine.disassembly`, the same function loads the request TLS word and tests it at 0x2a23-0x2a31; a zero request skips the acquire at 0x2a3e and the publish/join/release path. The acquire still may refuse in either arm; static offers are not successful publications.

The offered work is the long recursive child, not the cheap side leaf. `spine.wf` has one recursive child followed by `leaf`; `wf_leaf` at 0x2940-0x2948 is rotate, multiply-by-three via lea, return. On the offered path the caller does only the rotate at 0x2a56 before joining, then the lea/add at 0x2a9f-0x2aa3 after the result. This gives almost no useful complementary work while the child runs. The grain rule in `compiler/src/lowering/builder/call_grain.rs` retains a call that reaches an offered cyclic component, while the source-last member remains the join site regardless of its cost. A recursion budget bounds exposure but does not prove worthwhile parallel span. At configured W4/W8 the runtime budget formula is 8/9; this chain can attempt at most 8/9 recursive handouts per invocation (80,000/90,000 across 10,000 repetitions), not 256/512 independent branches. Those are static configured upper bounds, not observed offer or steal counts. Before the cut, at most 8/9 of the 4,000 side-leaf contributions can overlap the long child; the remaining 3,992/3,991 are inside the sequential descent. The source therefore supplies little parallel span even if every permitted handout succeeds. The W1 seq median is about 1.05 us per spine invocation (10.512 ms divided by 10,000), so the offered child is long relative to its sibling but still small in absolute measured batch cost; this is not the roughly 0.14 ms recursion leaf that motivated eager Fibonacci offers.

The initial median wall/CPU milliseconds are seq 10.505/10.505, previous 30.312/120.487, demand 38.905/155.901 at W4; at W8 they are seq 10.539/10.537, previous 37.019/296.010, demand 47.333/376.465. W1 demand/previous is 0.999901, with inactive entry selection. These values come from `summary.json.cells[spine,width].initial.{wall_ns,cpu_ns}` and `.rules["E6-keep-previous"].median`. They show CPU use without useful scaling, but cannot divide CPU into useful computation, handoff/join overhead and idle spinning. A shared idle policy can amplify a different offer pattern without having changed itself.

No scheduling-counter run was saved: `identity.json.instrumented` is false, the TSV contains wall/CPU/result counts only, and there is no counter directory. `measure.py` clears `WF_SCHED_REPORT`; only experiment 3 runs separate counter images. The existing counter target does not cover previous demand and the instrumented CLI rejects experiment 6. Entry-witness pool-query counts do not count spine publications, steals or spin time. Thus extra acquisition opportunities are proved; extra successful publications and their contribution to the slowdown are still hypotheses.

**FIR: the main par advantage is a differing optimized computation, not one extra query.** `demand/fir` and `previous/fir` are byte-identical executables and their LLVM is identical. The recursion ablation therefore does not change this FIR path. Demand and par each execute one pool query per timed call (`wf_bench_fir`: demand 0x3af7-0x3afe, par 0x38ad-0x38b4). Inactive W1 enters demand 0x3b22 and par 0x38d8; no request polls occur in the hot tap loops.

| Image / function | Timed tap-loop instructions | Construction visible before LLVM |
| --- | --- | --- |
| `seq/fir`, `wf_bench_fir` at 0x2d50 | [0x2e40,0x2e59) and [0x2e80,0x2e9a): seven instructions per tap, two outputs per outer iteration | Canonical source loop |
| `demand/fir` (also `previous/fir`), `wf_bench_fir` at 0x3ad0 | [0x3be0,0x3bfa) and [0x3c20,0x3c3b): same scalar structure, different registers/encoding | Canonical source loop inlined into a pool adapter |
| `par/fir`, `wf_bench_fir` at 0x3890 | [0x39d0,0x3a1f): four ordered scalar multiply/add taps in 16 instructions, one output per outer iteration, remainder at 0x3a40 | `par/fir.ll`'s `wf__par_seq_filter` calls synthesized `wf__par_seq__par_chunk_filter.1` at line 972; chunk definition at line 1234 |

Both scalar starts in seq/demand are 32-byte aligned. Their encodings differ: seq's two seven-instruction loops occupy 25/26 bytes, demand's 26/27, including additional REX prefixes for high registers. Demand's adapter at 0x3ade-0x3af2 preserves active-branch arguments and has different saved registers/stack traffic from seq. Once-per-entry saves cannot alone price the repeated loop's register allocation and encoding. Inlining couples dispatch context to optimized computation.

Standalone `seq/fir: wf_filter` and `demand/fir: wf__par_seq_filter` normalize to identical 111-instruction bodies. That D evidence is real, but does not prove that the inlined copies within differently optimized wrappers have identical instruction bytes or cost. The adapter exception includes the inlined region in this construction. This is a preservation/proof-boundary gap to make explicit, not a reason to label all wrapper differences as a 1 ns decision.

From `summary.json.cells[fir,1].initial.wall_over_seq.par`, par/seq is about 0.95461 in both attempts: par is about 4.54% faster than seq. Demand/seq is 1.003976 initially and 1.005104 on rerun, and demand/par is 1.052447/1.052762. This decomposes the ratio into a larger par-versus-canonical computation advantage and a smaller demand-versus-seq residue. The latter is 31.719 us using the initial wall medians (7.492931 versus 7.461212 ms), whereas the isolated query witness observes about 1 ns per entry and remains precision-inconclusive. One query in a roughly 7.5 ms call cannot be used as an explanation for a roughly 0.37 ms demand/par difference; both images have it anyway.

The differently unrolled computation is a concrete explanation candidate, not a quantified attribution of its 4.54% benefit. The 0.40-0.51% residue could involve register allocation/encoding, wrapper-context optimization, allocation/cache state or layout. Seq-shift controls do not isolate those variables, and being outside their envelope does not prove that every possible layout effect is absent. Minimal causal controls must keep wrapper, runtime and layout treatment matched while varying computation construction, then keep the computation fixed while varying the query adapter.

**Inspection: a manual step was not delivered, rather than a producer failing to write.** `README.md` lines 127-147 explicitly require real per-build optimized-code observations in `inspection.json` and say that inspecting saved code and reducing unchanged measurements completes the report without retiming. No active par-demand pipeline step writes that file. `measure.py` lines 363-364 reads it if present, otherwise supplies an empty mapping; `summarize.py` requires `hot_work_survives`, `evidence` and `check_compiles_to`. The E6 workflow runs measurement/reduction but supplies no manual inspection artifact or production step. Its success is execution success, not a qualified positive verdict.

The smallest completion is genuine inspection of this run's saved images/IR/ledger, retained as revision/image-bound evidence, followed by reduction of the same TSV. That would resolve missing-inspection statuses only, never the persistent spine/FIR rejections or the void twins. The prospective harness repair is an explicit build-bound inspection delivery/validation step before claiming qualification; identity pass must not manufacture hot-site evidence. This does not require another 14900K run.

**Twins: code/configuration equality is established; the runtime cause is not.** All twelve demand/twin executable pairs are byte-identical, independently checked against their saved SHA-256 hashes. Both use the same inputs, workers, pins and demand-on setting. All paired result counts agree. The runner uses the arm label only for output after timing (`runner.c` lines 31-32); code does not choose a workload/arm-specific computation. The distinct executable paths and process startup remain different. The images are PIE and no runtime mappings, monotonic timestamps, core frequencies, worker counts or scheduler event deltas were saved, so static identity does not eliminate process-layout or machine-state effects.

There is a real order-control defect: `measure.py`'s rotation by `(round_id + width_offset) % arm_count` is coupled to reversal on round parity. For six-arm W4, demand occupies slots 0/2/4 and twin 1/3/5; at W8 the parity reverses. Their relative before/after counts are 20/10 or 10/20 rather than balanced. Nine-arm W1 gives 16/14. The following exploratory splits diagnose that confound; they do not alter the frozen paired intervals or void verdicts.

| Voided cell | Registered twin/demand wall 95% interval | Paired-ratio interquartile range | Twin after/before count | Conditional median after/before |
| --- | --- | --- | --- | --- |
| large_helper W1 | [0.999578, 0.999842] | [0.999546, 0.999970] | 16/14 | 0.999678/0.999715 |
| large_helper W8 | [0.996557, 0.999462] | [0.996269, 1.002101] | 10/20 | 0.997780/0.998528 |
| records W8 | [1.007850, 1.036853] | [0.999468, 1.047827] | 10/20 | 1.012894/1.036853 |
| fir W4 | [0.997997, 0.999823] | [0.997587, 1.000127] | 20/10 | 0.999067/0.999219 |
| stencil W4 | [0.983126, 0.997108] | [0.977081, 0.997793] | 20/10 | 0.989863/0.994283 |
| prefix W4 | [0.977671, 0.999743] | [0.970396, 1.004100] | 20/10 | 0.978554/1.008689 |
| histogram W1 | [0.991332, 0.999652] | [0.984818, 1.000651] | 16/14 | 0.993158/0.999514 |

Source: registered intervals are `summary.json.cells[workload,width].initial.twin_wall_ratio.interval`; spreads and conditional medians are calculated from `measurements.tsv`, attempt 1, sample 1, pairing the same workload/width/round. IQR uses inclusive sample quartiles; after/before follows actual TSV row order. These are exploratory statistics, not additional acceptance rules.

Prefix W4 changes sign with order (0.978554 after versus 1.008689 before), but large_helper W1 has nearly identical conditional medians, and records W8 remains slower in both order groups. No common order-only cause is established. Large_helper W8 round 10 is stronger than ordinary tiny jitter: demand takes 6,636,276 ns wall / 55,632,651 ns CPU, twin 47,962,631 / 47,959,093. The paired wall ratio is 7.227341 and aggregate CPU/wall drops from about 8.38 to 1.00. It is consistent with lost aggregate concurrency, but wall/CPU cannot distinguish task exposure, stealing, parking, OS scheduling or failure to start workers. Other wide observed ratio ranges include prefix W4 0.761870-1.093468 and histogram W1 0.919785-1.149746. Calling all seven ordinary 14900K noise is unsupported; calling all seven an arm-order bug is also unsupported. All remain void. A concrete request-startup hypothesis deserves measurement for large_helper, independently of the arm-order defect. Its twin first call in round 10 was already slow (49,282,471 ns wall / 56,662,729 ns CPU); its second call's 47,962,631 ns wall nearly equals seq in the same round (48,035,567 ns). In the identical demand/twin image, `wf_workload` tests the request word at 0x2b63-0x2b6a before acquiring, and `wf__par_slice_helper.0` tests it at 0x2e47-0x2e4e before further splitting. A zero request can therefore leave useful work serial. Source `core.c` starts/helpers-readies before `wf__par_attach` registers the owner's word (lines 1596-1620); a null word cannot receive a request (lines 424-441), and `wf__par_should_scan` skips scans after an unchanged epoch (lines 1219-1234). Registering the word does not itself bump that epoch or wake an idle worker. This admits a performance scenario in which early helper scans predate owner registration, then scanning is suppressed/parked and request-gated root work remains unexposed. The artifact cannot show that this schedule occurred, distinguish it from partial worker startup, or establish it as the cause of the seven voids. The first-call data specifically prevent attributing this outlier solely to the gap between calls.

**Proposed sequence and falsifiers, fixed here before any follow-up.** These are separate attribution controls, not an extra E6 attempt. Use the registered placements, inputs, independent oracles and paired second calls; retain raw first calls. All performance intervals below use 10,000 bootstrap draws, seed 20261010, 95%. No counter-image time enters a performance comparison. Start each new control with six small diagnostic/pilot rounds, inspect precision and duration, then freeze its named decisive count before measuring. Report unsuccessful or incomplete controls instead of restarting until green.

1. **Deliver inspection without timing.** Inspect all twelve saved workloads, including actual selected hot loops and retained/pruned offer sites, and supply genuine evidence plus image/IR hashes. Add a general inspection-input validation stage so missing/stale evidence is visible before qualification. Prediction: genuine surviving-work cases cease to be undecided solely for missing inspection; optimized-away work remains unqualified, and the three persistent rejections and seven voids remain. Reject the completeness claim if any workload lacks surviving-work/decision evidence or hashes do not identify this build. This is source/artifact review and Python reduction only: zero 14900K timing cost. No historical inspection may be copied as a substitute.

2. **Separate spine acquisitions/publications from idle contention.** Reuse the exact eager/requested WF objects in separate diagnostic images linked against one common counter runtime. Extend research-only counter wiring to those actual arms; collect per-second-call deltas for acquire attempts/refusals, publications, steals, requests, parks and spin/park time, plus actual lane count. Existing publication/steal counters suffice for the first question; acquire/refusal and per-call snapshots need explicit diagnostic support. Prediction for excess-handoff attribution: eager has a positive paired acquire-attempt difference per completed spine invocation, and, if extra successful publications are claimed, their difference also has a positive lower interval endpoint. An upper endpoint <= 0 rejects the respective claim; a straddle stays undecided. Counts identify a mechanism, not a wall-time benefit.

   Then compare uninstrumented eager/requested x shipped/zero-idle-window, reusing the WF objects and one runtime implementation apart from that controlled idle-window setting, with a twin for each candidate and seq. Record the actual selected idle window: x86 CPU-level detection may return unknown/uniform, so P-core affinity alone does not prove it is zero. Define the interaction as `(eager/requested wall under shipped idle) / (eager/requested wall under zero idle)`. A lower endpoint > 1 supports an idle-window contribution; an upper endpoint <= 1 rejects that directional explanation, and a straddle is undecided. If the eager/requested interval remains wholly above 1.05 with zero idle, idle-window removal alone is rejected as the repair. Do not change the shipped scheduler on the basis of high CPU alone.

   Only after this evidence, propose a general demand-only grain refinement: an offered recursive child needs grain-worthy complementary independent work in the same group, including the caller's join-site work, rather than qualifying merely because the child reaches an offered SCC. Balanced recursion remains eager within the unchanged budget; a single costly chain plus a tiny join leaf loses the offer. No workload name or runtime special case selects it, and --par's legacy baseline remains unchanged. Prediction: spine loses its useless offer family and no longer pays repeated transfers, while recursion W8 retains demand/par upper <= 1.05 and every protected demand/unversioned and demand/previous upper <= 1.05. Reject the candidate on a persistent protected exceedance using one registered rerun; require canonical identity/dispatch proof as before. Retain demand/seq's 1.00 diagnostic and each eligible cell's own registered layout floor, with no inferred W4/W8 allowance. Exact seq-like timing is a prediction to check, not a result already proved by removing offers.

3. **Isolate FIR's optimized computation from its adapter.** First use matched noinline entrances in both seq and demand as a diagnostic construction, so the actually executed computation is the compared callee rather than an inlined copy excluded inside the adapter. Keep target/options/attributes, argument/result ABI, runtime and cold support treatment matched; retain the old E6 seq/par/demand images as before controls. Do not change ordinary --par lowering. Under the same wrapper, compare canonical source and legacy synthesized-chunk computation, both with/without one pool query and the same three layout treatments. Predicate: restoring the legacy ordered-tap construction retains its four-tap hot loop and has a canonical/legacy wall-ratio lower endpoint > 1 under matched adapter treatment. Upper <= 1 rejects the claimed directional kernel advantage; a straddle or mismatched machine shapes is undecided. This prices the loop difference without attributing it to query count.

   For the smaller residue, hold the kernel object fixed and compare direct/query adapters with validated arguments/results and one query outside loops. Prediction: their ratio is at the same-job floor and their measured per-call delta is too small to reproduce the roughly 32 us initial demand/seq median difference. If the matched adapter's lower delta endpoint reaches 32 us, reject the claim that adapter execution is negligible at this workload size; otherwise report its interval rather than converting a cap-limited micro witness into proof. Retain all observations in absolute ns and ratios; no new adapter allowance is added to the E6 verdict.

   The compiler/checker change proposed for qualification is a general opaque canonical-computation boundary on both sides of the comparison, with the entry exception confined to query, argument transfer and result handling. Matching IR/noinline is not guaranteed to produce identical machine code across separate modules: if the actual callee comparison fails, the owner's option A fallback is to compile the canonical closure once and link that object with no cross-boundary LTO. Its globals, attributes and the worker-safe cold-writer external boundary need an explicit build contract before implementation (for example, a canonical allocator can reach `wf_resource_record_abort`, so linking the seq private direct writer into a worker-reachable closure is not safe); a shared object must not introduce an unlatched worker-reachable writer. This is an interface/representation gap to decide, not a special FIR workaround. Register candidate/frozen-E6-par W1 upper <= 1.05, identity on the actual executed closure, and candidate/frozen-E6-seq's 1.00 diagnostic plus its same-job floor. Reject a persistent par exceedance or any computation mismatch; slowing a newly compiled seq denominator cannot hide a regression against the frozen baseline. No tap reassociation or contraction is proposed.

4. **Qualify the twin schedule before a full performance decision.** Pair each rotation in both directions before advancing: rotation `(round_id // 2 + width_offset) % arm_count`, reversal on parity. Complete pairs balance relative order; complete `2*arm_count` blocks balance positions. For an all-arm follow-up, propose 36 fixed decisive rounds (a complete multiple of both 12- and 18-round blocks), not a post-hoc extension of E6's 30. Verify schedule counts mechanically, and test path/label crossover plus a same-executable-path twin on the seven voided cells; never add a computation selected by the twin label. Save monotonic start/end times, actual lane counts, and runtime code/data mappings; retain separate scheduler snapshots for large_helper W8. For large_helper also retain owner request-word registration time, helper ready/scan/park times, the selected idle window and request/publication/steal deltas around each call. Prediction for the unexposed-root hypothesis: the near-seq slow cases have a functioning wider pool but no initial owner request and negligible useful publication/steal deltas; a slow case with sustained requests/publications/steals rejects that specific explanation. If confirmed, propose correcting the general owner-registration/initial-scan ordering or triggering a requester rescan/wake on registration, with safety/lifetime tests, while retaining demand-driven loop slices; do not seed arbitrary work or change policy from this static possibility alone. Prediction for an order-only explanation: the formerly biased order groups converge with balanced role crossover. If a balanced/swapped/same-path cell's twin interval still excludes 1, reject order-only attribution for that cell and stop qualification for further state/mapping analysis. Every twin must still include 1 before its cell can qualify; no outlier removal, wider twin band or replacement of E6's voids is authorized.

**Proposed 14900K cost and stopping points.** None of these jobs is queued or run. Cost estimates use this artifact's second-call medians and its six-round elapsed sizing value, not a new measurement. Pilot and preparation overhead are uncertain; provisioned windows are budgets, not predictions.

| Proposed stage | Observed-data projection | Proposed 14900K reservation / stop |
| --- | --- | --- |
| Complete inspection and replay reduction | No native work | 0 timing minutes |
| Spine counters plus 30-round nine-arm 2x2 control at W1/4/8 | About 44 s summed timed calls for the 2x2, using 4 copies of each policy plus seq and two calls/process; about 53 s with six pilot rounds. Counter/setup/reduction costs are not included. | Up to 5 min for the focused diagnostic, estimate revised only after its six-round pilot; no full rerun if mechanism evidence is missing. |
| FIR W1 matched-construction/adapter controls | Existing calls are about 7.1-7.5 ms; one eight-arm, 30-round/two-call wave sums about 3.6 s, before per-process oracle preparation and extra layout waves. | Up to 5 min for a focused control; freeze exact arms after its hosted construction and six-round pilot, before decisive timing. |
| Seven-cell twin/path/order diagnosis | Short paired control; existing distribution includes a 48 ms outlier and no process-overhead timestamps | Up to 5 min initially, with logged state; no repeated passes until the interval includes 1. |
| Full qualification, only after the preceding decisions/repairs | `identity.json.duration_projection.sizing_elapsed_seconds` = 243.806 s: six sizing plus 36 decisive rounds projects to 1706.642 s (28.44 min), including each process's startup/oracle preparation but excluding calibration/reduction/separate verification/reruns. Adding six rounds to the observed 27:06 job at that throughput suggests roughly 31 min if other costs persist. | Reserve 35-40 min; retain the 90-minute command bound. New sizing on changed code must confirm scale. A failure returns to analysis, not a further retry. |

The full follow-up would retain every E6 comparison, oracle, identity obligation, floor formula, CPU report and one-rerun/twin rule, with its separately proposed balanced schedule/count and named new attribution controls. It does not reinterpret E6 or relax its failed bounds. Owner selection of compiler policy and the canonical boundary comes before implementation and before committing the final follow-up protocol.

#### Stage 1: inspection and bounded diagnostics, fixed before measurement

The owner selected follow-up option A, diagnose first and then change strategy,
on 2026-10-11. This stage implements evidence delivery and causal controls only.
No compiler grain policy, canonical production boundary or startup repair is
adopted. The predictions and falsifiers in steps 1-4 above remain unchanged;
the following fixes the diagnostic construction, counts and stopping rules
before any diagnostic measurement. Six pilot rounds precede each diagnostic's
fixed decisive count. Pilots never enter decisive intervals. Use the existing
paired median, 10,000 bootstrap draws, seed 20261010 and 95% intervals. A
straddling directional interval remains undecided; no additional rounds or
outlier removal can rescue it.

**Inspection replay, not a new experiment.** Fresh inspection of all twelve
workloads in run 38179220701 is retained in
[inspection-38179220701.json](../../experiments/par-demand/inspection-38179220701.json).
Its records bind revision d8adf2c2a3b9519b94a920cc7e58397d1d511077, all selected
linked images, emitted/raw IR, lowering ledgers and inspected disassembly by
SHA-256. The evidence names selected timed loops, retained/pruned offers and
the difference between a separately retained function and an inlined timed
path. It is not copied from a historical inspection.
The twelve records contain 384 artifact hash bindings; the retained inspection
SHA-256 is `f0cfa0949b38b79b5f6971b65df6ee3cbf1c4325c2abbd105047f353a2f47dd7`.

Small_constant's intended 200-million-repeat hot store loop does **not**
survive in the selected timed path: demand `wf_bench_micro` at
0x2b10-0x2b2a has only the final three stores, while its 0x2b40-0x2b99 backedge
is the 4096-cell checksum. A repeat loop in the retained `wf_workload` does
not establish execution of that loop inside the timed wrapper. Its three
cells therefore remain inconclusive with explicit optimized-away evidence.
The other eleven workloads retain their inspected hot work and intended
non-pruned sites. Static inspection does not prove that a request, acquisition
or steal happened at runtime.

The untouched TSV was copied into a scratch reduction directory and reduced
with `summarize.py --experiment 6 --inspection <fresh record>`. The original
artifact remains read-only. `measurements.tsv` SHA-256 is
`fc2a3bf36a27c818eb7b80dd9d5db6490af70584aa88d0e3e185e52b08135712`;
the original `summary.json` SHA-256 is
`8dbd6467dcbd5f1bdaba8b0ee201ec0e98991dc3d7cca05db0919cb901acf613`.
The replayed summary SHA-256 is
`8d26380a10d7dfcb6964c101b573bb62b20eb0e1553b01eedca0a1273b03a783`.
No timing, interval, threshold, twin rule or construction admission changed.

| Workload | Widths newly qualified as pass | Widths still rejected | Widths still void | Widths still inconclusive |
| --- | --- | --- | --- | --- |
| small_constant | none | none | none | 1, 4, 8: optimized-away hot work |
| small_split | 1, 4, 8 | none | none | none |
| recursion | 1, 4, 8 | none | none | none |
| spine | 1 | 4, 8 | none | none |
| hot_helper | 1, 4, 8 | none | none | none |
| large_helper | 4 | none | 1, 8 | none |
| mandelbrot | 1, 4, 8 | none | none | none |
| records | 1, 4 | none | 8 | none |
| fir | 8 | 1 | 4 | none |
| stencil | 1, 2, 8 | none | 4 | none |
| prefix | 1, 8 | none | 4 | none |
| histogram | 2, 4, 8 | none | 1 | none |

Source: old and replayed `summary.json.cells[].{workload,width,status}`.
Of 38 cells, 25 change from inconclusive to pass; three remain inconclusive,
three fail and seven are void. The candidate remains rejected. In particular,
the newly qualified recursion W8 result does not erase spine's preservation
failures or FIR W1's par-preservation failure. The new reduction output stays
outside the repository; the evidence and this result are the retained record.

**Inspection delivery and ordering contracts.** `inspection.py` validates
complete workload coverage, the measured revision and every supplied artifact
hash, including all selected images/IR and cited machine/ledger observations.
Missing, stale, malformed or mismatched evidence fails before qualification;
`measure.py` no longer supplies an empty mapping, and the reducer revalidates
the input independently. This would have stopped E6's missing-inspection
qualification path visibly. `hot_work_survives: false` is valid evidence that
blocks a positive verdict, not a missing input.

The new order uses rotation `(round_id // 2 + width_offset) % arm_count` and
reverses the second round of each pair. Thus every arm pair is before/after
balanced at each width, even without a complete positional rotation block.
Authored six-/nine-arm tests count every pair in 30/36 rounds and every slot
in a complete 12/18-round block; the old coupled rotation fails these counts.
An odd decisive count from an existing sizing rule now fails explicitly,
without changing that rule or rounding its result. The new diagnostic counts
are fixed even counts.

There is an inspection-delivery interface gap for fresh qualified builds:
the workflow currently accepts a repository-relative manual record, but
committing a record for HEAD H creates H', which must not accept revision H
as its own evidence. An external post-build delivery mechanism is needed
before the next full qualified run. This stage does not conceal that gap
with historical evidence or a relaxed binding. Its diagnostics are causal
controls with their own construction proofs, not qualified performance cells.

**Spine diagnostic.** Question: do eager budgeted calls cause more acquisition
attempts/publications than requested calls, and does the shipped idle window
amplify their cost? Reuse the eager/requested WF objects; link both counter
arms against the **same** research counter runtime. At W1/4/8 record second-call
delta counts for attempts, refusals, publications, steals, attempted/posted
requests and parks, plus spin/park nanoseconds, completed invocation count,
actual lanes/ready helpers and the selected idle window. Normalize counts by
completed invocation count. Counter timings are never performance evidence.
Acquisition/publication claims use their respective positive paired difference
and the rejection/undecided rules in step 2; actual partial startup cannot
establish a full-width causal claim.

Separately run uninstrumented eager/requested x shipped/zero-idle-window,
with byte-identical twins for each of the four combinations plus seq: nine
arms, W1/4/8, six pilots then 30 decisive rounds. Runtime observations remain
outside the timed call; the hot runtime has no counter hooks. The shipped
and zero builds differ only in the idle-window override. Report the step-2
interaction and its registered rejection rule, every twin interval and the
eager/requested zero-window interval. A twin excluding 1 voids its comparison;
counter and uninstrumented observations are separate phases.

**FIR diagnostic.** Question: does the ordered four-tap legacy computation
explain par's advantage under matched adapters, and can adapter execution
explain the roughly 32 us residue? A research-only IR merger imports the
canonical closure and retains the legacy closure in **one** computation
module/object, with noinline roots and no cross-boundary LTO. All images link
that exact object, one common C wrapper/adapter object, one runtime and
equal-size selector bridges. Globals/private dependencies are namespaced;
native weak boundaries remain shared, and worker-reachable cold resource
failure support remains latched. This diagnostic does not implement the
owner's option-A production build structure or change ordinary --par.

Compare canonical/legacy computation x direct/query adapter, with a copied
twin for each of the four combinations: eight arms at W1, each under three
padding treatments (64, 4160, 65664 bytes), six pilots then 30 decisive rounds
per treatment. Inspect and hash the actual compared closure and its placement;
require the legacy ordered four-tap loop to survive and one query outside
loops. If matched construction fails or LLVM removes that loop shape, stop
without a causal result; do not force optimization to obtain the prediction.
Use the kernel ratio and adapter absolute-ns/ratio comparisons and rejection
rules in step 3. Same-treatment twins provide the noise check; the three
layouts are reported separately rather than choosing a favorable one.
For the adapter's same-job layout floor, the 64-byte treatment is the fixed
reference; pair each fixed direct computation at 4160 and 65664 bytes with
its 64-byte counterpart by round. The radius is the largest absolute
distance from 1 of either control's 95% interval endpoints. A query/direct
interval contained in `[1-radius, 1+radius]` is at the floor. A void control
or comparison cannot establish this floor. This new diagnostic floor does
not replace any frozen E6 floor or failed criterion.

**Twin/startup diagnostic.** Question: does balanced order/path-role crossover
remove the seven voids, and do slow large_helper calls begin without exposed
owner work? At the seven workload/width cells in the preceding void table,
compare original demand/twin paths, swapped path roles and one identical
executable path used for both labels: six arms, six pilots then 36 decisive
rounds. All twin images are byte-identical and each label pair is ordered in
both directions equally often. Save monotonic process/call start/end times,
actual lanes/ready counts and `/proc/self/maps`; do not aggregate path
treatments into a favorable replacement interval. The step-4 rejection rule
applies to each balanced/swapped/same-path twin comparison.

Run separate counter/trace large_helper W8 controls alongside this diagnostic,
two path roles per round, not mixed into the uninstrumented wall intervals.
Record helper-ready and owner-word-registration events, initial scan epochs,
park/wake events, initial request observations and second-call request,
publication and steal deltas. Retain the first 16 scan events per lane plus
scan count and last timestamp/epoch, rather than logging every spin scan;
the trace declares that sampling limit. Missing critical startup events or
event-buffer drops block the specific startup interpretation. Trace overhead
can perturb startup and is not evidence of its frequency in uninstrumented
E6. A full wider pool with near-seq calls and no owner request/useful exposure
supports the registered hypothesis; slow calls with sustained requests,
publications and steals reject it. Partial startup is reported separately.

**Dispatch, smoke and cost.** Use `compute-bench.yml`, `experiment=par-demand`,
`par_demand_experiment=diagnostic-spine`, `diagnostic-fir` or
`diagnostic-twin`, first with `placement_runner=github`, then `14900k`.
Hosted runs perform construction/oracle checks and six pilot rounds only,
with available CPUs and W1/W2 (FIR W1); they make no timing claim. Each native
run first repeats/validates its own hosted smoke before artifact transfer,
and verifies the transferred proof against its images. Native placement is
E6's P-core placement without CPU 0: W1 {2}, W4 {2,4,6,8}, W8
{2,4,6,8,10,12,14,1}. Native counts and intervals cannot use hosted pilots.

| Diagnostic | Hosted expected total / smoke reservation | 14900K expected execution / hard stop |
| --- | --- | --- |
| Spine counters and idle interaction | About 8-12 min including both compiler builds; smoke at most 5 min | About 1-3 min including preparation; at most 5 min |
| FIR computation/adapter | About 8-12 min including builds/IR construction; smoke at most 5 min | About 1-3 min including three layouts/preparation; at most 5 min |
| Twin order/path/startup | About 8-12 min including builds; smoke at most 5 min | About 1-3 min including separate traces; at most 5 min |

These are budgets and projections from E6, not new observations. Every mode
has a 300-second wall deadline, including construction replay, pilots,
preparation and reduction. After six pilots, a projection that exceeds the
remaining reservation stops before decisive rounds. An incomplete mode
retains its data/error and supplies no causal verdict. No full follow-up is
authorized by passing smoke or by any one diagnostic.

**Startup code finding: a general exposure defect, not an E6 attribution.**
The saved runtime starts helpers and waits for their ready count before
`wf__par_attach` registers lane 0's request word (`core.c` at the measured
revision, 1596-1620). A request scan cannot post to a NULL word (424-441),
and an unchanged scan epoch suppresses later scans (1219-1234). Registration
neither advances that epoch nor wakes parked helpers.

An allowed schedule is: all helpers announce ready, scan while lane 0's word
is NULL, exhaust the finite idle window and park; the owner is delayed until
after those final scans, then attaches and registers its word. If the root
and its loop slices require a request before publishing, the owner sees zero
requests and computes the entire root sequentially. No publication advances
the epoch/wakes a helper, and no guaranteed event makes a helper request
this newly registered owner. Consequently parallel work can stay unexposed
for the rest of an arbitrarily long call. This is loss of progress of work
exposure, **not program deadlock**: the owner still computes and terminates.
Spurious wakes do not supply a guarantee, and the schedule does not require
the 14900K's idle window to be zero.

Proposed general fix, not implemented here: make owner registration part of
the startup/exposure contract, either register a safely owned request word
before helpers' first scan, or notify idle helpers and ensure the newly
registered owner receives an initial targeted request/acknowledged rescan.
Simply bumping an epoch or waking one random victim scan is insufficient to
prove that this owner is requested. Preserve word withdrawal, request-user
lifetime protection and partial-startup behavior; add a controlled late-owner
schedule test before adopting a repair. The trace diagnostic establishes
whether this static defect actually explains the saved twin behavior, not
whether the schedule is possible. The defect goes to the owner independently
of the experiment; no strategy change or retry is made in this stage.

#### Spine diagnostic results

[Compute-bench run 38200095633](https://github.com/Ming-Research/Whitefoot/actions/runs/38200095633)
ran `par_demand_experiment=diagnostic-spine` at revision
`2f5d5be091528bea00d39a5770c88d26f57da737`; its i9-14900K job ran
03:37:12-03:38:47 UTC (run record supplied with the artifact). Evidence is that
run's `par-demand-results-14900k` artifact: `diagnostic-spine.json` (cited as
`ds.`), `identity.json`, `measurement-host.txt` and the images and disassembly
under `diagnostics/spine/`. `diagnostic-spine-smoke.json` is the hosted,
unqualified smoke; it is used only for the binding check below. Every interval
was recomputed from the raw rows with `summarize.median_interval` (paired
second calls, median, 10,000 bootstrap draws, seed 20261010, 95%) and agrees
with the saved `ds.reports[]` to every printed digit. No build, native
execution or retiming was performed for this reading. Each statement is marked
**registered reading** or **exploratory**; exploratory figures add no criterion
and establish no cause.

**Validity: complete and valid as a diagnostic; not an adoption result.**

- `ds.status` is `complete`, `ds.smoke` is false and `ds.qualification` is
  "diagnostic only; no E6 adoption verdict"; every `ds.reports[].qualified` is
  false. The native mode refuses to run without `14900K` in `/proc/cpuinfo` and
  without a validated hosted smoke (`diagnostics.py` `run_diagnostic`), and
  `measurement-host.txt` names an Intel Core i9-14900K. This is the registered
  native diagnostic, not an E6 performance cell.
- Binding: `ds.hosted_smoke_sha256` equals the SHA-256 of the transferred
  `diagnostic-spine-smoke.json`, whose `construction` equals `ds.construction`;
  `ds.construction.runtime_source_sha256` (`e17236ba...`) equals
  `compiler/src/backend/sched/core.c` at 2f5d5be09; `identity.json.revision` is
  2f5d5be09 with empty `dirty`. The branch's next commit, 5f9c84978, changes only
  the FIR part of `diagnostics.py` and its tests, so this reading uses the run's
  spine reduction unchanged.
- Construction: each twin is byte-identical to its image
  (`ds.construction.images[].sha256`). A policy's shipped and zero images differ
  only in `diagnostic-shipped-core.o` versus `diagnostic-zero-core.o`; eager and
  requested images, uninstrumented and counter alike, differ only in
  `demand/spine.o` versus `previous/spine.o`; both counter images link the same
  `diagnostic-counter-core.o` (`ds.construction.link_inputs`). In
  `diagnostics/spine/eager-shipped.disassembly`, `wf__par_budget_spine` (0x3a80)
  acquires unconditionally at 0x3aaf and publishes the recursive child
  (`wf__par_thunk_spine.0`, frame n-1, v+1, budget-1) at 0x3ad1, doing only the
  `rol` at 0x3ad6 before its join at 0x3add; `requested-shipped.disassembly`
  tests the request word at 0x3aa3-0x3ab1 before its acquire at 0x3abe. The
  discriminating code described under "After experiment 6" is unchanged in
  these objects.
- Matrix, order and inputs: 162 pilot and 810 decisive uninstrumented rows (nine
  arms, W1/4/8, six and 30 rounds) and 36/180 counter rows, without duplicates.
  In the decisive block every compared pair (eager/requested at each window,
  each image/twin) runs first in exactly 15 of 30 rounds at every width, and so
  does the counter pair. All rows passed `validate_sample` (oracle pass, one
  comparison extent, snapshots bracketing the timed call). `ds.placement` is the
  registered W1 {2}, W4 {2,4,6,8}, W8 {2,4,6,8,10,12,14,1}, with the CPU-1
  sharing caveat; `ds.settings` has `WF_PAR_DEMAND=on`, `WF_SPLIT_WORK` unset
  and `WF_SCHED_REPORT=0`; every counter row used 10,000 repetitions of extent
  4,000 (`ds.counter_decisive[].inputs`).
- Deadline: six pilot rounds took 13.06 s (`ds.pilot_elapsed_seconds`), the
  projection was 78.37 s (`ds.duration_projection_seconds`) and the mode
  finished in 78.75 s (`ds.elapsed_seconds`), within the 300-second bound.
- Void and exclusion conditions: all twelve twin intervals include 1 (table
  below), so no comparison is void. Every W4/W8 counter row has a complete pool
  before and after its second call (4 lanes/3 ready or 8/7, owner word
  registered; `ds.reports[].counter_pool_complete` is true), so the
  partial-startup exclusion does not apply. The idle-window treatment is real:
  `samples[].after.idle_window_us` is 1000 in every shipped and counter W4/W8
  row and 0 in every zero row. This removes the earlier caveat that x86 level
  detection could leave the shipped window at zero on this placement. At W1 no
  pool starts (lane count 0 in every observation), so W1 is a no-pool control.

**Q1, acquisitions and publications (registered reading: supported at W4 and
W8).** Step 2 predicted a positive paired eager-minus-requested acquire-attempt
difference per completed invocation, and the same for successful publications
if they are claimed.

| Width (threads; baseline = n/a; ideal = n/a) | Eager minus requested acquire attempts per completed invocation (count; baseline = requested counter image, same width and round; supporting ideal = lower CI > 0) | Eager minus requested publications per completed invocation (count; baseline = requested counter image, same width and round; supporting ideal = lower CI > 0) | Complete pool in every counter row (yes/no; baseline = width lanes, width-1 ready, owner word registered; ideal = yes) | Registered reading (category; baseline = step-2 rule; ideal = supported) |
| --- | --- | --- | --- | --- |
| W1 | 0 [0, 0] | 0 [0, 0] | yes (no pool) | rejected by the literal rule; neither arm reaches an acquire without a pool |
| W4 | 2.7182 [2.62185, 2.75265] | 2.7182 [2.62185, 2.75265] | yes | supported, both claims |
| W8 | 1.7201 [1.68365, 1.73430] | 1.7201 [1.68365, 1.73430] | yes | supported, both claims |

Source: `ds.reports[width].counter_differences_per_completed_invocation.{acquire_attempts,publications}.{median,interval,status}` and `.counter_pool_complete`; underlying second-call deltas of `ds.counter_decisive[].samples[1].snapshot_{before,after}.counters`, divided by the 10,000 completed `spine(n: 4000, ...)` invocations of each call.

Eager attempts and publishes more than requested at W4 and W8. Neither arm
refused an acquire, so each extra attempt became a publication. As registered,
these counts identify the handoff mechanism, not its wall-time cost. The
reducer applies the same directional rule to steals, requests, parks and spin
time; those statuses are not registered claims and are read below as
exploratory.

**Q2, idle-window interaction (registered reading: idle-window contribution
rejected at W4 and W8; idle-window removal alone rejected as the repair).**

| Width (threads; baseline = n/a; ideal = n/a) | Interaction: (eager/requested wall, shipped window) / (eager/requested wall, zero window) (ratio; baseline = zero-window pair; supporting ideal = lower CI > 1) | Eager / requested wall, zero window (ratio; baseline = requested-zero image; repair ideal = upper CI <= 1.05) | Eager / requested wall, shipped window (ratio; baseline = requested-shipped image; ideal = upper CI <= 1.05; context, no rule) | Registered reading (category; baseline = step-2 rules; ideal = n/a) |
| --- | --- | --- | --- | --- |
| W1 | 1.001127 [0.997822, 1.005194] | 0.998920 [0.996293, 1.001268] | 1.000584 [0.998566, 1.002357] | interaction undecided (straddle); repair not rejected; no pool |
| W4 | 0.903564 [0.888411, 0.914298] | 1.412615 [1.399840, 1.429482] | 1.267530 [1.259611, 1.277800] | contribution rejected (upper <= 1); removal alone rejected (lower > 1.05) |
| W8 | 0.945064 [0.931991, 0.951734] | 1.358977 [1.349093, 1.377072] | 1.287400 [1.276887, 1.300072] | contribution rejected (upper <= 1); removal alone rejected (lower > 1.05) |

Source: `ds.reports[width].{idle_interaction,zero_idle_eager_over_requested,zero_idle_repair_rejected}`; the shipped ratio is recomputed from `ds.decisive[]` second-call `wall_ns` with the same estimator.

The shipped 1,000 us window does not amplify eager's relative cost: the
eager/requested ratio is larger without it. Removing the window lowers both
arms' wall (exploratory, below) but leaves eager 36-41% slower than requested.

**Twin validity (registered void rule: no comparison void).**

| Width (threads; baseline = n/a; ideal = n/a) | Eager-shipped twin / image wall (ratio; baseline = same image, same round; ideal = CI includes 1) | Eager-zero twin / image wall (ratio; same baseline; ideal = CI includes 1) | Requested-shipped twin / image wall (ratio; same baseline; ideal = CI includes 1) | Requested-zero twin / image wall (ratio; same baseline; ideal = CI includes 1) |
| --- | --- | --- | --- | --- |
| W1 | 1.000698 [0.996547, 1.002477] | 1.000143 [0.999053, 1.001884] | 1.001174 [0.999767, 1.003177] | 0.998578 [0.997314, 1.000581] |
| W4 | 1.002928 [0.992289, 1.011531] | 1.002592 [0.994118, 1.008820] | 0.999524 [0.984604, 1.006578] | 0.995590 [0.969107, 1.038155] |
| W8 | 0.997513 [0.994260, 1.005000] | 0.997103 [0.991382, 1.002085] | 0.994210 [0.988629, 1.008392] | 1.005597 [0.992486, 1.012933] |

Source: `ds.reports[width].twins[arm].{median,interval}`, `.twin_valid` (true at every width).

**Exploratory: what the counters show about spine's grain at W4 and W8.**

| Arm / width (counter image / threads; baseline = n/a; ideal = n/a) | Acquire attempts per invocation (count; baseline = configured budget, 8 at W4 and 9 at W8; ideal = n/a) | Publications per invocation (count; baseline = budget; ideal = only grain-worthy offers) | Steals per invocation (count; baseline = publications; ideal = n/a) | Requests posted per invocation (count; baseline = 0; ideal = n/a) | Parks per invocation (count; baseline = 0; ideal = n/a) | Scan and spin time per invocation, all lanes (ns; baseline = 0; ideal = 0) | Scan and spin share of lane time (fraction; baseline = 0; ideal = 0) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| eager W4 | 8.0000 [8, 8] | 8.0000 [8, 8] | 6.2865 [6.21405, 6.36335] | 7.1401 [7.11445, 7.17310] | 0 [0, 0] | 9159.3 [9140.8, 9189.8] | 0.564 |
| requested W4 | 5.2818 [5.24735, 5.37815] | 5.2818 [5.24735, 5.37815] | 4.3418 [4.28360, 4.43780] | 5.3678 [5.32745, 5.46895] | 0 [0, 0] | 6977.6 [6946.8, 7004.7] | 0.568 |
| eager W8 | 9.0000 [9, 9] | 9.0000 [9, 9] | 8.5304 [8.50850, 8.55905] | 8.7652 [8.74840, 8.77325] | 0 [0, 0] | 28138.7 [28044.6, 28191.6] | 0.665 |
| requested W8 | 7.2799 [7.26570, 7.31635] | 7.2799 [7.26570, 7.31635] | 6.9467 [6.92270, 6.98185] | 7.4342 [7.41265, 7.46285] | 0 [0, 0] | 23386.9 [23360.7, 23500.2] | 0.666 |

Source: second-call deltas of `ds.counter_decisive[label, width].samples[1].snapshot_{before,after}.counters.{acquire_attempts,publications,steals,requests_posted,parks,spin_ns}` divided by 10,000, median [95% bootstrap] of the 30 rounds. Share = median `spin_ns` / (median counter-image `samples[1].wall_ns` x width); `spin_ns` covers scans and spin/yield policy calls of idle and joining lanes, excluding callbacks and sleeps (`core.c` 1143-1158, 1384-1392). Refusals are 0 in every row.

- Eager acquires and publishes exactly its configured budget, 8 at W4 and 9 at
  W8, in all 30 rounds (minimum equals maximum). By the disassembly above, each
  published frame is the rest of the recursive chain, at least 3,991 of 4,000
  levels, and the publisher's only work before its join is one `rol`.
  Publications are mostly stolen (ratio of medians 0.786 at W4, 0.948 at W8), so
  the chain moves from lane to lane up to the budget per invocation while the
  publisher waits; no publication creates overlapping work.
- Requested is not a no-offer baseline here. Idle helpers keep the owner's
  word requested (`owner_request` is 1 before the second call in 479 of the 480
  W4/W8 policy rows, `ds.decisive[].samples[1].before`), and requested publishes 5.28 of 8
  (W4) and 7.28 of 9 (W8) budget slots per invocation, 82% and 95% of them
  stolen. Requested itself is 2.917767 [2.887273, 2.934173] (W4) and 3.482447
  [3.465364, 3.516830] (W8) times seq's wall (`ds.decisive[]`, second calls,
  same estimator). Most of spine's loss against seq is therefore common to both
  policies; eager's extra 2.72/1.72 publications per invocation add the
  remaining 1.27-1.29 factor.
- Work per split against the split's cost, combining counts from the counter
  images with walls from the uninstrumented images (cross-phase arithmetic, not
  a performance comparison): seq takes 1.0487 us per invocation at W4 and
  1.0501 us at W8 (median second-call `wall_ns` / 10,000), which is the whole
  chain a publication can carry. (arm wall - seq wall) / publications is 0.356
  us (eager W4), 0.381 us (requested W4), 0.408 us (eager W8) and 0.361 us
  (requested W8) per publication: each handoff costs roughly a third of the
  invocation's entire sequential work and buys no overlap. This run has no
  per-split size or latency counter, so the figure is an estimate.
- Helper idle behaviour: no lane parked during any eager counter call, and only
  one requested counter call parked at all (4 parks, W8 round 3). Lanes spend
  56-57% (W4) and 66-67% (W8) of their time scanning or spinning in both
  arms. Uninstrumented CPU/wall is 3.99-4.03 at W4 and 7.97-8.06 at W8 in all
  four policy and window images (ratio of median `cpu_ns` to median `wall_ns`),
  so lanes stay hot through every call under either window. The zero window's
  lower wall (requested shipped/zero 1.242302 [1.212359, 1.258647] at W4,
  1.147034 [1.138163, 1.159778] at W8; eager 1.114008 [1.107521, 1.129656] and
  1.084949 [1.076907, 1.091969]) is not explained by parking. A candidate is
  that a zero window scans every round (`core.c` 1285-1287) while the shipped
  window scans only after an epoch change; the registration put counters only
  on the shipped window, so this run cannot test it.
- Replication of E6: eager-shipped/seq is 3.706134 [3.680189, 3.728036] (W4)
  and 4.493759 [4.470422, 4.537311] (W8), against E6's demand/seq 3.694303 and
  4.491429; eager/requested under the shipped window is 1.267530 and 1.287400,
  against E6's demand/previous 1.289710 and 1.279648 (E6 values from
  `summary.json` of run 38179220701 as tabulated above). The diagnostic images
  reproduce E6's spine regression.

**Startup exposure defect: not tested by this run (registered); one process
shows its whole-call signature (exploratory).** The spine registration
contains no startup test. The registered test is the twin/startup
diagnostic's large_helper W8 trace, which this run does not include: spine
counter rows request no trace (`ds.counter_decisive[].startup_trace` is null),
so helper-ready, word-registration, scan and park order and the time to the
first steal are unobserved.

| Process (arm, width, decisive round; baseline = n/a; ideal = n/a) | First call wall / CPU (ms; baseline = same-round seq 10.480 / 10.480; ideal for a request-gated arm = parallel work exposed) | Second call wall / CPU (ms; baseline = same-round seq 10.478 / 10.478; ideal = parallel work exposed) | Before and after second call: lanes / ready / owner word registered / owner request (values; baseline = 8 / 7 / true / 1 in the other 119 shipped-window W8 policy rows; ideal = same) |
| --- | --- | --- | --- |
| requested-shipped-twin, W8, round 5 | 12.184 / 19.646 | 10.751 / 10.758 | 8 / 7 / true / 0, both times |
| requested-shipped, W8, round 5 (same image bytes) | 36.416 / 290.241 | 35.753 / 280.744 | 8 / 7 / true / 1, both times |

Source: `ds.decisive[width=8, round=5, label].samples[0,1].{wall_ns,cpu_ns,before,after}`; ns divided by 1,000,000. The other 119 rows are the W8 eager-shipped, eager-shipped-twin, requested-shipped and requested-shipped-twin decisive rows.

The pool was complete and the owner's word registered, yet no request was
pending at either observation, second-call CPU equals wall (no helper ran) and
the wall is seq's: the request-gated owner computed the whole call alone. The
first call's CPU exceeds its wall by 7.462 ms, about 1.07 ms for each of seven
helpers, consistent with every helper spinning through its 1,000 us window
without work and then parking. The pattern occurred in 1 of 60 decisive
requested-shipped processes at W8 (image and twin), 0 of 60 at W4, 0 of 120
requested-zero processes, 0 of 60 requested counter processes, and in no
eager process, which publishes without a request. It does not void the cell:
its round ratio is 0.300714, but the requested-shipped W8 twin interval still
includes 1, and the registered interaction uses only the non-twin images.

Reading the measured runtime (`core.c` at 2f5d5be09) gives a schedule that
produces this pattern without requiring helpers to park before registration.
Once a helper's first 1,024 rounds pass, a nonzero window lets it scan only
after an epoch change (`wf__par_should_scan` 1283-1300; `entered` set at 1366).
Each failed scan posts one request to one seeded random victim (`wf__par_find`
1105-1139), and the scan before parking (1533) posts one more. The epoch moves
only on a push, a steal or a signal (1093, 1035, 1001). Lane 0 registers its
word only in `wf__par_attach`, after `wf__par_start`'s ready wait (1662,
1686). An owner that registers after the helpers' scanning has stopped, with
no later request landing on lane 0, therefore never publishes, and nothing
bumps the epoch or wakes a parked helper. This is the registered defect
family, widened: helpers need only stop scanning before registration, not
park. A zero window scans every round (1285-1287), consistent with no
zero-window occurrence, though 120 processes do not establish a rate. The
observation shows that the static defect is reachable on the 14900K with a
complete eight-lane pool and can hold for an entire 10.75 ms call. It does not
show the event order, does not attribute E6's seven voids and is no
performance claim; in spine the unexposed call was faster than the parallel
ones.

**Ambiguities in the registration.** (1) At W1 the step-2 rule reads
"rejected" because neither arm can reach an acquire without a pool; read
literally it rejects the claim, read by its purpose (concurrent widths) it is
not applicable. Neither reading affects W4/W8. (2) "Per completed spine
invocation" is implemented as per completed outer `spine(4000)` call (10,000
per timed call), not per recursive call; the sign and the readings are the
same under either denominator. (3) The reducer gives every counter a
directional status, but only acquire attempts and publications have a
registered claim; the other statuses are exploratory. (4) The shipped-window
eager/requested ratio has no registered rule and is shown only as context.

**Proposed consequence, not selected here.** Step 2's registered order now
allows the demand-only grain refinement to be proposed: the counters show
spine's offers are budget-length chain handoffs whose complementary work is a
single `rol`, and the idle window is rejected as the explanation and as the
repair. Because requested also publishes most of its budget and is itself
2.9-3.5 times slower than seq, a candidate that only restores request gating
cannot meet the never-slower aim on spine; the refinement must remove offers
without grain-worthy complementary work under either policy. Separately, the
startup observation supports bringing the owner-registration exposure repair
to the owner and running the registered twin/startup trace; this run alone
selects neither.

#### Twin and startup diagnostic results

[Compute-bench run 38200599238](https://github.com/Ming-Research/Whitefoot/actions/runs/38200599238)
ran `par_demand_experiment=diagnostic-twin` at revision
`5f9c8497843a8254b2f40dd2aae80b20eb3462e0`; its i9-14900K job ran
03:49:46-03:51:59 UTC (run record supplied with the artifact). Evidence is that
run's `par-demand-results-14900k` artifact: `diagnostic-twin.json` (cited as
`dt.`), `identity.json` and `measurement-host.txt`;
`diagnostic-twin-smoke.json` is the hosted, unqualified smoke and is used only
for binding. Intervals were recomputed from the raw rows with
`summarize.median_interval` (paired second calls, median, 10,000 draws, seed
20261010, 95%) and agree with `dt.reports[]` to every printed digit. No build,
native execution or retiming was performed for this reading. Statements are
marked **registered reading** or **exploratory** as in the spine section.

**Validity: complete and valid as a diagnostic, with one void cell.**

- `dt.status` is `complete`, `dt.smoke` is false, `dt.qualification` is
  "diagnostic only; no E6 adoption verdict", and `measurement-host.txt` names
  an Intel Core i9-14900K. `dt.hosted_smoke_sha256` equals the transferred
  smoke's SHA-256 and the smoke's `construction` equals `dt.construction`.
- Revision: `identity.json.revision` is 5f9c84978 with empty `dirty`;
  `dt.construction.runtime_source_sha256` is `e17236ba...`, the same
  `core.c` bytes as the spine run. The only difference from 2f5d5be09 is two
  `diagnostics.py` hunks in the FIR loop-shape helpers (`strongly_connected`,
  `innermost_loops`, `operands`, `ordered_tap_chain`, `selected_call_shapes`)
  and their tests; the twin mode's preparation, image plan, path resolution,
  sample validation, run loop and reduction are unchanged.
- Construction: each workload's twin is byte-identical to its demand image
  (`dt.construction.images[].sha256`). The `same` pair runs the demand image
  under both labels, `distinct` runs demand as D and twin as T, `swapped` the
  reverse (`dt.decisive[].physical_image`). The trace image links the same WF
  object as large_helper's demand image and differs only in the counter core and
  counter primitive objects (`dt.construction.link_inputs`).
- Matrix and order: 252 pilot and 1,512 decisive rows (seven cells, six arms,
  six and 36 rounds) and 12/72 trace rows, without duplicates. In every cell
  each pair's T runs first in exactly 18 of 36 decisive rounds, and trace-T
  precedes trace-D in 18 of 36. Placement and settings are as registered
  (`dt.placement`, `dt.settings`); six pilot rounds took 16.31 s, the
  projection was 114.16 s and the mode finished in 113.88 s.
- Trace conditions: `startup_trace.dropped` is 0 in all 84 trace rows, and
  every row carries lane 0's `word-registered` and `idle-window-selected` and
  each helper's `helper-ready` and `scan` events (`validate_sample`). Every
  trace row's pool is complete, 8 lanes and 7 ready, before and after both
  calls, so there is no partial startup to report separately. The selected
  idle window is 1000 us in every W4/W8 row; W1 rows start no pool.
- Void condition: one of 21 twin intervals excludes 1 (histogram W1, `same`),
  which voids that cell (table below).

**Q1, order and path-role crossover (registered reading: six of the seven E6
voids do not recur under balanced order; histogram W1 rejects order-only
attribution).**

| Cell (workload / threads; baseline = n/a; ideal = n/a) | Distinct paths, twin / demand wall (ratio; baseline = demand image path, same round; ideal = CI includes 1) | Swapped roles, demand-as-T / twin-as-D wall (ratio; baseline = twin image path as D; ideal = CI includes 1) | Same path, T / D wall (ratio; baseline = the same image under label D; ideal = CI includes 1) | Registered reading (category; baseline = step-4 rule; ideal = valid) |
| --- | --- | --- | --- | --- |
| large_helper W1 | 0.999968 [0.999728, 1.000233] | 1.000003 [0.999937, 1.000081] | 0.999846 [0.999763, 1.000062] | valid; order-only not rejected |
| large_helper W8 | 0.999717 [0.998434, 1.002281] | 0.998589 [0.994344, 1.000391] | 1.000053 [0.998522, 1.002389] | valid; order-only not rejected |
| records W8 | 1.003893 [0.988931, 1.022863] | 1.010745 [0.983709, 1.025016] | 0.992022 [0.977978, 1.011117] | valid; order-only not rejected |
| fir W4 | 1.003297 [0.989283, 1.017177] | 0.997803 [0.993128, 1.003117] | 1.001061 [0.990352, 1.004863] | valid; order-only not rejected |
| stencil W4 | 1.000716 [0.983812, 1.011405] | 0.998271 [0.984360, 1.006544] | 0.992439 [0.982974, 1.000798] | valid; order-only not rejected |
| prefix W4 | 0.996024 [0.986760, 1.010855] | 1.003511 [0.986744, 1.011076] | 0.996935 [0.990928, 1.013158] | valid; order-only not rejected |
| histogram W1 | 1.002244 [0.995716, 1.007409] | 0.995950 [0.992241, 1.000339] | **1.003887 [1.000876, 1.007783]** | **void; order-only rejected; qualification stops for state/mapping analysis** |

Source: `dt.reports[workload,width].twins.{distinct,swapped,same}.{median,interval}`, `.order_only_rejected`, `.twin_valid`, `.causal_status`.

"Not rejected" is the registered outcome, not proof that order caused the E6
voids: the rule can reject order-only attribution but cannot confirm it.
Histogram W1 has no pool, so its same-path exclusion cannot involve the
scheduler; its lower endpoint is 0.000876 above 1, and one exclusion among 21
intervals at 95% coverage is about what chance gives (exploratory). The
registered rule has no multiplicity allowance, and the reading stands.

**Q2, startup exposure (registered reading: the large_helper W8 trace
supports the unexposed-root hypothesis).** The registered rule: a full wider
pool with near-seq calls and no owner request or useful exposure supports it;
slow calls with sustained requests, publications and steals reject it.

| Trace process (phase, round, role; baseline = n/a; ideal = n/a) | Owner registration after last helper ready (us; baseline = 0.185-32.889 in the 66 exposed decisive rows; ideal = 0) | Second call wall / CPU (ms; baseline = large_helper W1 in this run, 47.744-47.748 ms per arm, and E6 W8 seq 48.087 ms; ideal = about 6.7 ms) | Publications / steals, first and second call (count; baseline = about 3,000 / 2,400 per call in exposed rows; ideal = n/a) | Owner request before and after second call (word value; baseline = 1; ideal = 1) | Helpers parked before registration (of 7; baseline = n/a; ideal = n/a) | Helpers proven to have stopped scanning before registration (of 7; baseline = n/a; ideal = n/a) |
| --- | --- | --- | --- | --- | --- | --- |
| decisive 10, trace-T | 1103.4 | 47.859 / 47.852 | 0 / 0, 0 / 0 | 0, 0 | 0 | 6 |
| decisive 14, trace-T | 1098.2 | 47.827 / 47.826 | 0 / 0, 0 / 0 | 0, 0 | 0 | 6 |
| decisive 15, trace-T | 1157.4 | 47.819 / 47.818 | 0 / 0, 0 / 0 | 0, 0 | 5 | 6 |
| decisive 19, trace-D | 1105.8 | 47.838 / 47.837 | 0 / 0, 0 / 0 | 0, 0 | 1 | 6 |
| decisive 27, trace-D | 1105.4 | 47.838 / 47.837 | 0 / 0, 0 / 0 | 0, 0 | 1 | 7 |
| decisive 32, trace-D | 1112.4 | 47.835 / 47.834 | 0 / 0, 0 / 0 | 0, 0 | 1 | 7 |
| pilot 2, trace-T | 1109.9 | 47.863 / 47.862 | 0 / 0, 0 / 0 | 0, 0 | 0 | not computed |

Source: `dt.counter_decisive[]` and `dt.counter_pilots[]`: `startup_trace.events[]` (`helper-ready`, lane 0 `word-registered`, `park`), `samples[0,1].{wall_ns,cpu_ns,before.owner_request,after.owner_request}` and counter deltas of `samples[].snapshot_{before,after}.counters`; E6 seq from run 38179220701 `summary.json.cells[large_helper,8].initial.wall_ns.seq.median`; W1 from `dt.decisive[large_helper,1]`. "Stopped scanning" counts helpers whose park lies less than 1,000 us after registration: a helper's last every-round scan precedes its window, which precedes its park by at least the 1,000 us window (`core.c` 1352-1376), so such a helper ended its every-round scanning before registration.

Six of 72 decisive trace processes (8.3%) and one of 12 pilots meet the
supporting condition: complete pools, the owner never requested, no
publication or steal in either call, second-call CPU equal to wall, and a
near-seq call. No near-seq call had sustained requests, publications or
steals. The registered reading is **supported**, with the registered caveat
that trace overhead can perturb startup; its rate here is not E6's rate.

Ambiguity: the registration does not define "slow". Under "slow = near seq",
the reading above holds. Under any wider reading, decisive round 16 trace-D
(second call 9.952 ms against an exposed median of 6.719 ms, 2,605
publications, 2,076 steals and 4 parks) is a slow, exposed call; it would
reject the hypothesis as the explanation of that call, not of the six near-seq
calls, whose cause the hypothesis names.

**Q2 detail: what happened before registration (exploratory, from the
trace).**

- In all six unexposed processes the owner registered 1.098-1.157 ms after
  the last helper became ready; in all 66 exposed decisive processes it
  registered 0.185-32.889 us after it. Every helper in an unexposed process
  made exactly 1,026 scans in the first call, all at epoch 0
  (`samples[0].snapshot_after.scans[].{count,last_scan_epoch}`), and the first
  call attempted exactly 7,182 = 7 x 1,026 requests and posted 7 (one to each
  helper's own word). That is 1,025 every-round scans in each helper's first
  spin bound, then no scan through the 1,000 us window (the epoch never
  moved), then one scan before parking (`core.c` 1515-1533).
- **The wider mechanism is supported.** In rounds 10 and 14 no helper had
  parked when the owner registered, yet none requested it afterwards: six of
  seven helpers had already stopped scanning and were spinning without
  scanning, and their pre-park scans after registration did not reach lane 0.
  Parking before registration occurred in only four of the six processes (8 of
  42 helper instances). Stopping scanning, not parking, is what the six
  processes share.
- Why the pre-park scans miss the owner: lane seeds are fixed
  (`0x9e3779b97f4a7c15 * (index + 1)`, `core.c` 1580) and each failed scan
  advances the seed once and requests one victim (1112-1113, 1134-1137).
  Replaying that sequence for eight lanes, the 1,026th scan of lanes 1-7
  targets lanes 5, 1, 5, 1, 6, 2, 6; for four lanes, lanes 1-3 target 2, 1,
  1. On this startup path no helper's pre-park request can reach lane 0 at W4
  or W8: once registration follows every helper's 1,025th scan, the owner is
  never requested. The replay script is `scratchpad/twin-analysis/seeds.py`.
  Each helper's first 1,026 scans target lane 0 112-249 times, first within
  scans 2-9, which is why prompt registration always exposed work here.
- Time to first steal is not recorded: the trace has no steal or publication
  event, and helper scans are sampled to the first 16 per lane. In the six
  unexposed processes no steal occurred in either call, more than 97 ms in
  total. In exposed processes, the earliest helper scan seeing a nonzero epoch
  came 0.346-59.032 us after registration (median 0.579 us, 25 rows where a
  helper's sampled scans outlasted registration), and lane 0's first sampled
  scan, which follows its first publication, came 29.3-144.0 us after
  registration (median 41.0 us, 66 rows) with the epoch already at 25-159.
  Once the owner is registered while a helper still scans every round, work is
  exposed within microseconds.
- Why the owner was about 1.1 ms late is not established. The delay equals
  roughly one helper's 1,025 scans (sampled spacing median 196 ns, about
  0.2 ms) plus its 1,000 us window, which fits an owner that cannot run until
  a spinning helper sharing its CPU starts yielding. In each unexposed process
  one helper parked 0.9-30 ms after registration, also consistent with sharing
  the owner's CPU. Per-thread CPU placement was not recorded, so this is a
  candidate only. In two exposed processes (rounds 16 and 35, trace-D), a
  helper's start was delayed by a similar 1.2-1.5 ms instead; registration
  followed that late helper, which was still scanning every round, and work was
  exposed.

**Exploratory: the same signature in the uninstrumented images.** Without
tracing, the signature appears in every request-gated cell with a pool except
prefix W4.

| Cell (workload / threads; baseline = n/a; ideal = n/a) | Decisive processes whose second call had no owner request before or after and CPU/wall <= 1.006 (count of 216; baseline = 0; ideal = 0) | Their second-call wall (ms; baseline = cell median of 216; ideal = cell median) | Pilot processes with the same signature (count of 36; baseline = 0; ideal = 0) |
| --- | --- | --- | --- |
| large_helper W8 | 5 | 47.806-47.945 (median 6.640) | 2 |
| records W8 | 7 | 10.529-10.661 (median 1.587) | 0 |
| fir W4 | 3 | 7.991-8.093 (median 2.826) | 1 |
| stencil W4 | 1 | 64.694 (median 39.368) | 0 |
| prefix W4 | 0 | n/a (median 1.758) | 0 |

Source: `dt.decisive[]` and `dt.pilots[]` second calls: `before.owner_request`, `after.owner_request`, `after.lane_count`, `cpu_ns`, `wall_ns`. W1 cells start no pool and are omitted.

Four of the seven E6 voids (large_helper W8, records W8, fir W4, stencil W4)
are cells where whole calls ran unexposed in this run, and E6's large_helper W8
round-10 outlier had this signature. That makes the startup defect a plausible
contributor to those voids, but no interval here establishes it: balanced
order kept all four cells' twin intervals around 1 despite the outliers, and
the rate in E6's processes is unknown.

**Repair evidence (exploratory, from the trace and the seed replay).** The
registration's proposed repair is to make owner registration part of the
startup contract. The six unexposed schedules discriminate the candidates:

| Candidate repair (mechanism; baseline = current runtime; ideal = exposure guaranteed) | Rounds 10 and 14: nothing parked at registration, six or more helpers no longer scanning (outcome; baseline = unexposed; ideal = exposed) | Rounds 15, 19, 27 and 32: one to five helpers parked at registration (outcome; baseline = unexposed; ideal = exposed) | Guarantee (category; baseline = none; ideal = by construction) |
| --- | --- | --- | --- |
| Registration bumps the scan epoch | each non-scanning helper scans once more; per the replay, lane 2's 1,027th victim is lane 0 (W8 and W4), so this schedule is rescued only by the fixed seed sequence | parked helpers are not woken; rescue again depends on which helpers remain in their window | none; depends on seed sequence and timing |
| Registration wakes one idle worker | nothing is parked, so `wf__par_wake_one` finds no idle bit; no change | the woken helper restarts 1,025 every-round scans and reaches lane 0 within a few | none; fails when no helper is parked |
| Owner posts its own initial request when it registers its word | the owner's first poll publishes; the publication bumps the epoch, so non-scanning helpers rescan and steal | the publication also wakes one parked helper (`wf__par_publish`, 1736-1760) | exposure of the root's first offer by construction |
| Register lane 0's word before helpers start | helpers' first scans post to lane 0, and the request persists until the owner publishes | same | by construction, but requires choosing lane 0's owner before `wf__par_start` (today the CAS in `wf__par_attach` decides) |

Source: trace rows as in the Q2 table; `core.c` 1001 (signal bump), 1009-1021 (`wf__par_wake_one`), 1035 (push bump), 1093 (steal bump), 1283-1300 (scan gating), 1662-1686 (ready wait, attach, registration); victims from `scratchpad/twin-analysis/seeds.py`.

**Proposed consequence, not selected here.** The trace turns the static
startup finding into an observed, attributable failure. It occurs with a
complete pool, needs only that helpers stop scanning before registration, and
on this startup path is not rescued by later requests, because the epoch never
moves and every pre-park victim misses lane 0. A repair should guarantee that a
newly registered owner is requested, independent of helper state and seeds:
the owner's initial self-request at registration does so most locally, and
registering lane 0's word before helpers start does so at the cost of fixing
lane 0's owner earlier. An epoch bump or a wake alone is not sufficient.

The test should fail before the repair and pass after, deterministically, by
controlling the schedule rather than timing it. A `WF_SCHED_TEST` hook holds
the owner between the ready wait and `wf__par_register_word` until every helper
has made its pre-park scan (window nonzero; one variant releases the owner
while helpers are still spinning in the window, matching rounds 10 and 14, and
one after they park, matching round 15). The owner then runs a request-gated
root that polls its word a bounded number of times and publishes only when
requested. The test asserts at least one publication and one steal. At W4 and
W8 the current runtime fails both variants, because the seed replay shows no
pre-park request reaches lane 0. A self-request repair passes both; a wake-only
repair fails the first variant. A zero-window control passes before and after.
Word withdrawal, request-user lifetime and partial-startup tests stay
unchanged. Owner approval of the repair comes before implementation; this run
changes no policy.

#### FIR diagnostic results

[Compute-bench run 38201260238](https://github.com/Ming-Research/Whitefoot/actions/runs/38201260238)
ran `par_demand_experiment=diagnostic-fir` at revision
`5f9c8497843a8254b2f40dd2aae80b20eb3462e0`, whose construction check finds
the legacy loop as the innermost loop's dataflow (`diagnostics.py`
`innermost_loops`, `ordered_tap_chain`). Its i9-14900K job ran 03:58:03-03:59:03
UTC (run record supplied with the artifact). Evidence is that run's
`par-demand-results-14900k` artifact: `diagnostic-fir.json` (cited as `df.`),
`identity.json`, `measurement-host.txt` and `diagnostics/fir/`;
`diagnostic-fir-smoke.json` only binds. Intervals were recomputed from the raw
rows with `summarize.median_interval` (paired second calls, median, 10,000
draws, seed 20261010, 95%) and agree with `df.reports[]`. No build, native
execution or retiming was performed for this reading.

**Validity: complete, construction admitted, two of three padding treatments
void under the implemented rule.**

- `df.status` is `complete`, `df.smoke` is false and `df.qualification` is
  "diagnostic only; no E6 adoption verdict"; `measurement-host.txt` names an
  Intel Core i9-14900K. `identity.json.revision` is 5f9c84978 with empty
  `dirty`; the smoke's SHA-256 matches `df.hosted_smoke_sha256` and its
  construction equals `df.construction`; the runtime hash is the same
  `e17236ba...` as in the other two diagnostics.
- Construction (`df.construction.fir`):
  - All 24 timed images link one computation object (`compute_object`
    `51814ae8...`), and each twin is byte-identical to its image.
  - Within a padding treatment, the four combinations differ only in their
    selector object (`df.construction.link_inputs`), and the exposed and actual
    roots keep the same absolute start and size (`matched_placement`).
  - The legacy ordered four-tap loop survives as the innermost loop of
    `wf__par_seq_filter`. In `diagnostics/fir/64/legacy-direct.disassembly` it
    is 0x45c0-0x460d: four `mulsd`/`addsd` taps accumulated one after another
    into `%xmm0` (`legacy_four_tap_loops`).
  - The canonical closure is instruction-identical to seq's noinline
    `wf_filter` after normalization, apart from the explicitly shared, latched
    `wf_resource_record_abort` (`actual_canonical_closure`).
  - No FMA appears, and each query adapter calls `wf__par_pool_active` exactly
    once, outside every loop (`validate_fir_images`).
- Matrix and order: 144 pilot and 720 decisive rows (eight arms, three
  treatments, six and 30 rounds), without duplicates, 524,288 outputs per call;
  every compared pair runs first in exactly 15 of 30 rounds in every
  treatment. W1 placement is CPU 2, and the settings are as registered. Pilots
  took 6.97 s, the projection was 41.82 s and the mode finished in 42.35 s.
- Void condition: two of twelve twin intervals exclude 1, on byte-identical
  images: legacy-query at 64 bytes, 0.997403 [0.996977, 0.999825], and
  canonical-query at 4160 bytes, 0.998478 [0.997446, 0.999459]. The reducer
  frozen before the run voids every comparison of a treatment with an excluded
  twin, and then the layout floor, which needs every treatment's twins
  (`df.reports[].causal_status`, `.same_job_layout_floor[].status`).

**Q1, kernel construction under matched adapters (registered reading:
supported at 65664 bytes; void at 64 and 4160).** Step 3 predicted a
canonical/legacy wall ratio with lower endpoint > 1 under the same adapter
while the legacy four-tap loop survives; upper <= 1 rejects.

| Treatment / adapter (padding bytes / adapter; baseline = n/a; ideal = n/a) | Canonical / legacy wall (ratio; baseline = legacy computation, same adapter and treatment; supporting ideal = lower CI > 1) | Canonical minus legacy wall (us per call; baseline = legacy, same round; ideal = n/a) | Registered reading (category; baseline = step-3 rule and reducer void rule; ideal = valid) | Alternative reading: void only comparisons whose own arms' twins exclude 1 (category; baseline = same rule; ideal = valid) |
| --- | --- | --- | --- | --- |
| 64 / direct | 1.051271 [1.050186, 1.053614] | 404.0 [396.2, 422.3] | void (raw: supported) | supported |
| 64 / query | 1.051526 [1.049443, 1.052369] | 406.2 [390.6, 412.8] | void (raw: supported) | void (legacy-query twin) |
| 4160 / direct | 1.052783 [1.051610, 1.054690] | 416.0 [407.0, 430.3] | void (raw: supported) | supported |
| 4160 / query | 1.053122 [1.051446, 1.054450] | 418.6 [406.0, 429.4] | void (raw: supported) | void (canonical-query twin) |
| 65664 / direct | 1.053730 [1.052741, 1.055593] | 423.5 [415.6, 438.2] | **supported** | supported |
| 65664 / query | 1.053478 [1.050583, 1.055129] | 421.5 [399.9, 434.7] | **supported** | supported |

Source: `df.reports[padding].canonical_over_legacy[adapter].{median,interval,status,raw_status}` and `.twins`; us deltas recomputed from `df.decisive[]` second-call `wall_ns`.

Under matched adapters, the canonical source loop is 5.1-5.4% slower than the
legacy ordered four-tap loop in all six comparisons, void or not, and no
interval approaches 1. That matches E6's FIR W1 demand/par of 1.052447 and
par/seq of about 0.95461. The legacy construction's advantage therefore
belongs to the computation, not to the query or wrapper. As registered, this
prices the loop difference without attributing it to one machine-code feature.

**Q2, adapter residue (registered reading: the adapter-negligible claim is
not rejected; the floor is void).** Step 3 rejects negligible adapter
execution if the query-minus-direct lower endpoint reaches 32 us; otherwise
the interval is reported.

| Treatment / computation (padding bytes / construction; baseline = n/a; ideal = n/a) | Query minus direct wall (ns per call; baseline = direct adapter, same computation and round; rejection = lower CI >= 32,000) | Query / direct wall (ratio; baseline = direct adapter; ideal = within the same-job floor) | Same-job floor band (ratio bounds; baseline = 64-byte direct reference; ideal = contains the query/direct interval) | Registered reading (category; baseline = step-3 rule and reducer; ideal = valid) |
| --- | --- | --- | --- | --- |
| 64 / canonical | 1830.5 [-7066.0, 10253.0] | 1.000220 [0.999150, 1.001238] | [0.997649, 1.002351]; raw: at floor | void; raw: not rejected |
| 64 / legacy | 7674.0 [-1434.5, 19479.0] | 1.000973 [0.999818, 1.002476] | [0.997407, 1.002593]; raw: at floor | void; raw: not rejected |
| 4160 / canonical | 2653.0 [-19637.0, 15375.5] | 1.000319 [0.997644, 1.001853] | [0.997649, 1.002351]; raw: outside by 0.000005 | void; raw: not rejected |
| 4160 / legacy | 5555.0 [-3897.0, 11392.5] | 1.000704 [0.999505, 1.001445] | [0.997407, 1.002593]; raw: at floor | void; raw: not rejected |
| 65664 / canonical | -917.0 [-23318.5, 14689.0] | 0.999889 [0.997191, 1.001772] | [0.997649, 1.002351]; raw: outside (lower) | not rejected; floor void |
| 65664 / legacy | 1286.5 [-16466.5, 9721.5] | 1.000163 [0.997917, 1.001233] | [0.997407, 1.002593]; raw: at floor | not rejected; floor void |

Source: `df.reports[padding].{query_minus_direct_ns,query_over_direct,query_residue_claim_rejected,raw_query_residue_claim_rejected,same_job_layout_floor}`; raw floor membership recomputed from the reported band and interval. Bands come from the direct computations at 4160 and 65664 bytes paired with 64 bytes: canonical 1.000041 [0.998902, 1.001537] and 1.000204 [0.998857, 1.002351]; legacy 0.999272 [0.998154, 1.000833] and 0.999015 [0.997407, 1.001119].

Every lower endpoint is negative and every upper endpoint is at most 19.5 us,
so one query outside loops cannot reproduce E6's roughly 32 us demand/seq
residue. The registered statements are only "not rejected" and the
intervals. Whether the adapter ratio is at the floor is undecided under the
frozen rule. Under the alternative reading, whose direct floor controls all
have valid twins, the legacy adapter is at the floor in all three treatments
and the canonical adapter at 64 bytes only; at 4160 and 65664 bytes the
canonical interval's lower end lies just below the band.

**Exploratory observations.**

- The two excluding twins are small (0.26% and 0.15%), on byte-identical
  images run first equally often. Their order-conditional medians,
  0.997323/0.998258 and 0.999078/0.997763 with the twin after or before its
  image, show no consistent order sign. Two of twelve exclusions at 95%
  coverage is more than chance usually gives (about 0.6 expected), so a small
  path- or state-dependent effect remains possible, as in histogram W1's
  same-path exclusion. The frozen rule's coupling of a whole treatment to one
  twin is what turns these small exclusions into void kernel comparisons.
- Absolute scale differs from E6: the canonical computation takes about
  8.30 ms and the legacy about 7.88 ms per call (`df.decisive[]` medians in
  every treatment), against E6's fir W1 seq 7.461 ms and par about 7.12 ms
  (run 38179220701). Both constructions are about 11% slower in this
  noinline, single-module build with the diagnostic runner. The ratio
  reproduces E6 while the absolute times do not, so these numbers price the
  loop difference within this construction, not E6's absolute walls. Which
  part of the build costs the 11% was not measured.

**Proposed consequence, not selected here.** The FIR question is answered in
direction: par's W1 advantage is the legacy computation's ordered four-tap
loop, about 5% at this size, and not the query adapter, whose cost is not
distinguishable from zero at microsecond resolution. This supports option A's
direction: an opaque canonical-computation boundary compiled once, so demand
and seq run the same machine code. It also makes the remaining fir W1
par-preservation failure a choice between keeping canonical identity, which
costs about 5% against legacy par, and an owner-approved general change to the
canonical loop's lowering. Reassociation and contraction are excluded as
before. Any further timing should either use the narrower void rule above or
pre-register why one excluded twin voids every comparison in its treatment.

#### Startup exposure repair

**Implemented on the work branch; not yet run.** This applies the
self-request candidate from the repair table in the twin and startup results
above. It changes `--par-demand`'s runtime only; ordinary `--par` does not
compile the demand section of `compiler/src/backend/sched/core.c`.

- **Change.** `wf__par_attach`, after the owner registers lane 0's request
  word, calls the new `wf__par_request_self`, which stores 1 into the owner's
  own thread-local word when the pool's requests are on
  (`WF_PAR_DEMAND=off-never-request` posts nothing). The owner's first
  request-gated poll therefore publishes. Every publication pushes, and the
  push bumps the publish epoch, so a helper spinning in its window rescans;
  when any lane has announced itself idle, the publication also wakes one
  through `wf__par_wake_one` and `wf__par_signal`, which bumps the epoch again
  under the station lock. A helper between its idle announcement and its
  sleep either sees the item in its pre-park scan or is woken, by the existing
  announce-before-final-scan protocol. Workers' own registrations are
  unchanged. Only the owner thread writes its word here, so request-user
  counting, word withdrawal at thread exit and partial startup are untouched;
  registration happens once per process, right after the pool starts.
- **Test.** `compiler/src/backend/sched/startup_probe.c`, built as
  `sched-startup` and run by `make -C compiler sched-startup-test` inside
  `completion-test` at four and eight lanes. It compiles the core with
  `WF_SCHED_STARTUP_TEST`, whose hooks choose the window, hold the owner just
  before registration, hold helpers before their first scan, and record each
  helper's window entry and park; none exists in a shipped build. The root
  polls its word until asked, or until every helper has parked, after which
  no scan can happen before a publication; it then publishes one offer and
  waits for a helper to run it before joining.
  - `late-spinning`: a ten-second window, owner held until every helper has
    entered it; the case checks that no helper had parked at registration.
    Before the repair each helper's one remaining scan is its 1,026th, whose
    victim the seed replay gives as lanes 2, 1, 1 at four lanes and 5, 1, 5,
    1, 6, 2, 6 at eight, never lane 0, so the owner is never asked and the
    case fails when the windows close. A wake-only repair also fails here,
    since nothing is parked.
  - `late-parked`: a 1,000 us window, owner held until every helper has
    parked. Before the repair no lane scans again, so the case fails at its
    first poll. A wake-only repair passes this case.
  - `registered-first`, the control: a zero window, helpers held before their
    first scan until the owner has registered. Each helper asks lane 0 within
    its first nine scans by the same replay, so the case passes before and
    after the repair. The control holds the helpers rather than the owner
    because a zero window does not keep helpers scanning: each parks after
    1,042 scans, and an owner held until then is never asked either.
  - `requests-off`: the `late-parked` schedule under
    `WF_PAR_DEMAND=off-never-request`; the owner must never be asked and no
    steal may occur.
- **Limits.** The before-repair failures are argued from the code and the
  seed replay, not observed: this branch carries the repair. A spurious
  return from a helper's condition wait would add scans the replay does not
  count, which could let the old runtime pass by chance; it cannot make the
  repaired runtime fail. Whether the repair removes the near-seq calls in
  timed runs is a separate measurement.
