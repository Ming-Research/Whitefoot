# A plan for how `--par` decides what to hand out

This investigation answers the owner's comment that `--par` task splitting
has had no overall plan: small fixes kept arriving one problem at a time,
each well measured, while no policy has been qualified across the problems
together. It collects the known problems, what each earlier change did and
measured, and the possible overall directions with the criteria that choose
between them. It proposes a direction and a first stage; it changes no code.

## What decides parallel execution today

Four separate decisions stand between a source program and a task running on
another worker. Each lives in a different place and sees different
information.

1. **Permission**: may these statements or iterations overlap? PAR-1 groups
   adjacent statements whose footprints are independent; PAR-2 permits
   counted-loop iterations with independent writes or admitted accumulators
   (`spec/kernel-spec.md`, PAR-1 and PAR-2; `compiler/src/semantic/permission.rs`,
   `loop_permission.rs`). Worker count never selects acceptance.
2. **Exposure**: does lowering turn permitted work into offers? Statement
   groups become offered calls; counted loops become recursive range splits
   with a loop at each leaf (`compiler/src/lowering/builder/split.rs`,
   `compiler/src/backend/emitter/parallel.rs`). Non-call statements, frame
   size and result handling end groups.
3. **Profitability**: is the work big enough to offer? Three mechanisms
   answer this separately:
   - calls: a compile-time filter keeps an offer only if its callee's static
     weight reaches 150,000 or it reaches recursion that offers its own calls
     (`compiler/src/lowering/builder/call_grain.rs`);
   - loops: at run time `wf__par_split_budget` turns span and weight into a
     power-of-two number of chunks, at most 16 per worker, each worth at least
     the 150,000-unit work unit (`compiler/src/backend/sched/core.c`);
   - recursion: a depth budget of floor(log2(64 x workers)), capped at 24,
     spent at each group call into the component; at zero the call enters the
     sequential clone (`compiler/src/backend/emitter/frontier.rs`).
4. **Execution**: does the host run the offers efficiently? Per-worker
   Chase-Lev deques with stealing, joins that help, a 1 ms idle spin window on
   uniform hosts, a worker per logical CPU by default
   ([parallel runtime](../../../design/compiler/parallel-lowering/parallel-runtime.md)).

Pricing everywhere is a static estimate of IR instructions, with runtime loop
extents substituted where a captured length is available; nothing measures
actual task cost.

## What has been tried

The full timeline, with sources, is in the
[history appendix](#appendix-history-of-interventions). In outline:

| Period | Change | Effect | Status |
| --- | --- | --- | --- |
| Aug 21 | Lane scans replaced by owner deques and stealing | per-fork excess 48.8 to 4.8 ns | adopted |
| Aug 21 | Per-task demand bookkeeping in a shared bitmask | layout oracle 0.49 to 0.93 s | rejected |
| Aug 22 | Two worlds, chosen once at start | fib(38) pool-off tax removed | adopted |
| Aug 23 | Range splits with loops at leaves; work unit 1,200,000 | 3.1x for loop leaves vs 3.6-7.6x penalty for per-iteration leaves | adopted |
| Sep 8 | Scalar-leaf call filter | quadrature publications 8,219 to 1,643 | replaced Sep 29 |
| Sep 11 | Recursion budget from worker count | quadrature W4 8.2 to 4.6 ms | adopted |
| Sep 11 | Idle window 1 ms, spin bound 1,024 | fixed SMT co-location losses; up to 7% more CPU | adopted; 1 ms never swept |
| Sep 12 | Work unit 1,200,000 to 150,000 | mandelbrot W8 0.692x time | adopted |
| Sep 13 | Global work unit 10,000 | prefix and histogram faster; chain-pull W4 2x slower | rejected |
| Sep 13 | Runtime extents in loop prices | prefix W8 3.6 to 1.2 ms; chain-pull W2 51% slower | provisional |
| Sep 22-23 | Query, then call the chunk at zero allowance, at every site | stencil W4 22% faster; records W1 31% slower from placement | withdrawn |
| Sep 29 | Static call grain with a recursion exemption | Snowghost setup ecma262 W4 10.4 to 0.28 s | adopted, provisional |
| Sep 29 | Recursion budget spent only at group calls | style ecma262 W4 1.04x to 3.10x; apollo11 still 1.14x | adopted |
| Oct 6 | 64-byte function alignment | placement invariance on tested hosts; records up to 14% slower on one EPYC | adopted |
| Oct 8 | Exemption only for recursion that offers its own calls | Snowghost edit W4 3,128 to 274 us on the 14900K | adopted |
| Oct 9 | Conditional calls in groups | no edit speedup alone | adopted |
| Oct 9 | Small loops skip query and splitter | micro W4 1,075 to 108 ms; unqualified | paused |

## The problem as a whole

Every repair in that table fixed a real problem, and then the next decision
in the chain became the limit. The same symptom recurs through different
mechanisms: **permission finds safe work, and a separate, later decision has
to find out whether the work is worth handing out, with information it often
does not have.**

The known open problems, from the board backlog (item keys in brackets),
fall into five groups:

1. **Prices that are wrong in both directions.** Static weights miss work
   that depends on data: a 1-byte and a 65,536-byte record get the same
   price, and loaded per-task costs never reach the split point
   [coord-wfbl-03-27, coord-wfbl-03-29]. Runtime extents fix some loops and
   break others [coord-wfbl-03-27]. Call grain can drop an expensive helper
   whose cost comes from its arguments and keep a cheap call that reaches
   offering recursion [coord-wfbl-03-59]. Lengths are lost through local
   owners and writable references [coord-wfbl-03-28].
2. **A depth budget where the work is not where the depth is.** Unbalanced
   trees spend the budget above their heavy subtrees: apollo11's style runs
   at 1.14x on four workers, against about 3.7x with the budget off, while
   turning it off makes stable scatter 30-37% slower [coord-wfbl-03-58].
   Some recursion gets no budget at all [coord-wfbl-03-61].
3. **Machinery paid where nothing is handed out.** Small split loops pay
   about 4.8 ns per call at four workers for a query and a splitter entry,
   ten times their work in the minimal case [coord-wfbl-03-54]; the earlier
   all-site fix regressed one-worker code through placement [coord-wfbl-03-30].
4. **Execution costs the price does not see.** Idle spinning on SMT siblings
   [coord-wfbl-03-60], small allocations that get slower with more workers
   [coord-wfbl-03-55], serial setup and packing around parallel phases
   [coord-wfbl-03-22, coord-wfbl-03-25, coord-wfbl-03-31], worker chunks
   without alias facts [coord-wfbl-03-34].
5. **No single place and no common yardstick.** The decisions are spread
   across translation, a grain pass and the emitter [coord-wfbl-03-39], and
   measurements have repeatedly been confounded by code placement and short
   CPU intervals [coord-wfbl-03-32, coord-wfbl-03-48]. No experiment has
   compared one policy across kernels, recursion, real programs and hosts at
   once.

Permission gaps (PAR-1 and PAR-2 forms the checker refuses, eleven open
items) limit how much work exists to hand out, and are a separate line: the
policy chosen here decides how work is handed out, whatever permission
admits.

## Possible overall directions

**A. One static cost model, in one place.** Move every profitability
decision into one pass after ordinary lowering [coord-wfbl-03-39], with one
cost model for calls, loops and recursion; improve the model where it is
wrong (argument-dependent helper prices, per-task loaded costs, recursion
priced by work rather than depth); let sites whose work is certainly small
skip the runtime entirely. Runtime cost stays minimal and decisions stay
deterministic and visible in `--par-ledger`. Its weakness is the one the
history shows most: costs that depend on data cannot be priced before the
data exists, and every improvement to the model so far has helped some
programs and hurt others.

**B. Hand out on demand, with static prices only as a floor.** The
compiler exposes every permitted fork and split cheaply, and the runtime
hands work out only when another worker could take it: a split continues
halving, and a call is offered, only while the current worker's deque is
empty, which is a load of the worker's own deque ends, not a shared signal.
This is lazy task creation and lazy binary splitting (Mohr, Kranz and
Halstead 1991; Tzannes, Caragea, Barua and Vishkin 2010); heartbeat
scheduling (Acar, Charguéraud, Guatto, Rainey and Sieczkowski 2018) is a
timed variant with a proven overhead bound. Today's loop query already
returns zero when the worker's deque holds work, but only after a call into
the runtime and only for loops. Static prices remain only to
skip the machinery where the work is certainly too small to matter. The
recursion depth budget becomes unnecessary for most recursion, because a
busy worker stops offering at every depth, and unbalanced trees offer where
the work actually is. Its costs: a larger change to lowering and the
runtime; a check on every potential fork, which must cost about a
nanosecond to win; and two earlier refusals to answer. The August
demand-bookkeeping trial doubled the layout time, but it used a shared
bitmask read by every task; the "full deque refuses" decision rejected a
heartbeat gate because rate-limited promotion cannot remove coarse-grain
overhead. Neither tested a worker-local emptiness check.

**C. Prices from profiles.** A training run records actual task costs and
feeds them to the compiler. This answers data-dependent costs for inputs
like the training input, but needs representative inputs, makes the build
depend on a run, and helps nothing that A or B could not do first. It is a
later refinement of A, not a direction of its own.

**D. Continue problem by problem.** Take the backlog in priority order as
before. Each step is small and measured, but this is the approach the owner
has judged to make no overall progress, and the history agrees: each repair
moved the limit rather than removing the class of problem.

## Criteria

A direction is chosen on these, measured on one suite:

1. **Never slower**: on every workload, more workers are never slower than
   one by more than the twin's noise; today the small-loop case is 10x
   slower and the hosted SMT edit 1.44x slower.
2. **Speedup where work exists**: the formal kernels, the recursion
   kernels (quadrature, merge sort, quick sort) and Snowghost's setup,
   style and layout at four and eight workers, against today.
3. **Robust to shape**: unbalanced trees (apollo11 style shape B) and
   data-dependent costs (the phased DAG, first-index records) without
   per-workload tuning.
4. **CPU spent**: wall-time gains reported with CPU time, as earlier
   trials repeatedly traded one for the other.
5. **Weak targets**: nothing that needs timers, signals or more than
   word-sized atomics on the hot path, since embedded targets with weak
   cores are a goal.
6. **Understandable**: one place decides, and the ledger can say why a
   site did or did not hand out work.

The suite is the first thing the plan builds, because no earlier
experiment had one: the five formal kernels, the three recursion kernels,
the small-loop microbenchmark, the DAG and scatter controls, Snowghost's
setup, style, layout and edit on html5, ecma262 and apollo11, at one, two,
four, eight and sixteen workers on the i9-14900K, and at one, two and four
on a hosted two-core SMT runner, each with an identical-image twin.

## Recommendation and stages

Recommended: **B, with A's single decision pass as its first stage.**
B addresses the two problem groups that static pricing has failed on
repeatedly (prices and depth), and it makes the small-site fix part of a
general rule instead of a special case. A's restructuring is needed by
either direction, so it comes first.

1. **The suite and its baseline** on today's main. Done when every
   workload above runs in one dispatchable job and the baseline table is
   recorded. Expected about two days of work plus 14900K time.
2. **One decision pass** [coord-wfbl-03-39]: move call grain, split
   selection and offer filtering into one pass after ordinary lowering,
   with byte-identical `--par` output for every maintained program. A
   refactor with no behavior change.
3. **A demand-driven prototype** for loops and calls only: halve and offer
   only while the worker's own deque is empty; keep the static floor.
   Compare against today and against the paused small-loop fix on the
   suite, with criteria fixed before the run. This stage decides between A
   and B with evidence instead of argument.
4. **Recursion**: if stage 3 succeeds, replace the depth budget for
   ordinary recursion by the same rule, and re-measure apollo11 and
   scatter.
5. **Execution costs** (SMT spinning, allocator, setup), measured on the
   same suite once the hand-out policy is settled.

The paused small-loop work [coord-wfbl-03-54] waits for stage 3: if B is
chosen, the static floor of stage 3 is that fix; if A, the paused branch is
its first piece.

What would overturn the recommendation: a worker-local emptiness check
that costs as much as today's query (stage 3 measures it first, on the
microbenchmark), or a suite result where demand-driven splitting loses to
static pricing on regular kernels by more than noise.

## Appendix: history of interventions

The history below was assembled from the investigations and design nodes
it cites; each row's source has the full measurement.

- **Aug 21, owner deques** (`408dd34eb`): per-fork excess 48.8 to 4.80 ns;
  `bal_d8_w16` W8 4.32 to 0.49 s; Apple M4 `q4` W64 25.55 to 0.25 s
  ([gap hunt](../proof-derived-parallelism/gap-hunt-findings.md)).
- **Aug 21, demand bookkeeping**: shared seeking bitmask took the layout
  oracle from 0.49 to 0.93 s; rejected.
- **Aug 22, two worlds** (`629fad88c`): fib(38) 0.235 to 0.079 s, equal to
  the sequential build ([results](../proof-derived-parallelism/RESULTS.md)).
- **Aug 23, range splits** (`f7127c03`): loop leaves 3.1x; per-iteration
  leaves 3.6-7.6x penalty; work unit 1,200,000 from a weight-150 width sweep
  ([loop design](../proof-derived-parallelism/loop/DESIGN.md)).
- **Aug 23, placement**: 196 bytes of never-called runtime shifted hot
  code; two of sixteen 4-byte shifts cost 1.18-1.28x ([baseline](../proof-derived-parallelism/bench/baseline-20260823/README.md)).
- **Sep 8, scalar-leaf filter** (`cb6f999b1`): quadrature publications
  8,219 to 1,643; replaced by call grain.
- **Sep 8-11, static recursion frontiers**: quadrature 31-37% faster at depth
  8, text 33 to 37 KB; replaced by the runtime budget
  ([prior bundle](../compute-runtime/PRIOR-BUNDLE.md)).
- **Sep 11, runtime recursion budget** (PR #37): quadrature W4 8,216 to
  4,583 us ([results](../compute-runtime/RESULTS.md)).
- **Sep 11, idle policy** (PR #43): window fixed SMT co-location (FIR W2
  1.290 to 0.994), up to 7% more CPU elsewhere; 1 ms reasoned, not swept;
  disabled on asymmetric cores (PR #49).
- **Sep 12, work unit 150,000** (`fe4487deb`): mandelbrot W8 0.692x.
- **Sep 13, work unit 10,000 and runtime extents**: the former rejected
  (chain-pull W4 15.4 to 30.4 ms), the latter provisional (chain-pull W2
  +51%) ([compute model](../compute-model/DESIGN.md)).
- **Sep 14-21, frontier on scatter** (PR #78): off made W8 packing 0.57 to
  1.27 ms; no change.
- **Sep 21-23, captures and frame fitting**: rescue-only pruning; exact
  layout fitting.
- **Sep 22, read-only Box-array lengths in prices**: hash helper W4 10.5
  to 2.8 ms, 5% more CPU.
- **Sep 22-23, zero-allowance dispatch** (PR #89): stencil W4 0.78x
  single-site; general version records W1 +31% on Xeon; withdrawn
  ([reassessment](../compute-model/DESIGN.md#general-dispatch-reassessment)).
- **Sep 29, call grain** (PR #177): Snowghost setup ecma262 W4 10.4 to
  0.28 s, html5 5.8 to 0.22 s; formal kernels byte-identical
  ([call-offer grain](../call-offer-grain/DESIGN.md)).
- **Sep 29, budget spent at group calls**: style ecma262 1.04x to 3.10x,
  apollo11 1.04x to 1.14x.
- **Sep 29, segmented storage**: parallel small allocations W2 1.15 s
  against 0.71 s sequential.
- **Oct 6, function alignment** (`e7f103600`): placement-invariant on tested
  hosts; records 5-14% slower on EPYC 9V74
  ([code placement](../code-placement/DESIGN.md)).
- **Oct 8, indexed reductions** (PR #274, #290): histogram W8 2.4x on the
  14900K; 0 of Snowghost's 32 candidate loops permitted as written.
- **Oct 8, recursion exemption narrowed** (PR #278): Snowghost edit W4
  3,128 to 274 us on the 14900K
  ([recursive offers](../recursive-offer-grain/DESIGN.md)).
- **Oct 9, conditional calls** (PR #289): capability only
  ([edit parallelism](../edit-parallelism/DESIGN.md)).
- **Oct 9, small split loops**: query plus splitter cost 4.8 ns per call;
  fix paused for this plan (the loop-split-grain investigation, on branch
  `claude/loop-split-grain`).
