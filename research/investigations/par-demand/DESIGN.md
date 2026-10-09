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
