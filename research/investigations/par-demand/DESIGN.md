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
