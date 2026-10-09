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
