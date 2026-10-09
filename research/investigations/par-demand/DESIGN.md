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

This describes the measured prototype. The [proposed third change](#caller-local-slices-proposed-third-change-before-measurement) below describes the current unmeasured draft.

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
