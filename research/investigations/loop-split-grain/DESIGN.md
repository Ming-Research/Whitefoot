# Small split loops at four workers

## Question

A counted loop that PAR-2 permits is lowered as a range split (the
[parallel-lowering](../../../design/compiler/parallel-lowering.md) decisions on
range splits and runtime-query entry). In the overlapped world, which a run
with two or more workers executes, each execution of the loop computes its
span and weight, calls the runtime's `wf__par_split_budget`, and calls the
splitter with the returned allowance. When the span's priced work is below the
150,000-unit work unit, the allowance is zero and the splitter calls the chunk,
which runs the loop. The sequential world, which a one-worker run executes,
calls the chunk directly, and LLVM inlines it into its caller
(`compiler/src/backend/emitter/parallel.rs`, `emit_loop_split`).

Snowghost measured that such loops cost real time when a sequential traversal
calls them many times with a few iterations each:

- The text-layout mode L1, which offers no other work, was 5 to 24 percent
  slower at two or four workers than at one on every measured page, and making
  `write_run_span`'s loop unsplittable in a local build brought the flat page's
  four-worker L1 from 1.80 s to 1.52 s against 1.53 s and 1.57 s at one worker
  ([Snowghost layout measurement](https://github.com/mbbill/Snowghost/blob/7f7542f/research/investigations/concurrency/DESIGN.md#layout-measurement)).
- The html5 text edit restacks a flow of 104,321 entries. The per-edit median
  was 55.2 ms at four workers and 22.7 ms at one; with three small split loops
  in `renderer/layout/flow.wf` made sequential in the same source, 28.1 ms and
  22.8 ms (Snowghost-wf `research/investigations/incremental-layout/runs/step3b.txt`).

Both were measured with compilers older than main at this investigation's
start (`3b1e5de75`). The backlog item "small loops that cannot be split still
pay the query at every call" proposes calling the chunk directly in the caller
when the static price is below the work unit, as the call grain already does
for calls. This investigation asks, before choosing that or another change:

1. Does a small split loop called many times still cost time at four workers
   against one, on main?
2. If it does, how much of that cost is the split site itself (the query, the
   splitter entry, and the chunk not being inlined into its caller), and how
   much is elsewhere?

## Prior objection

The [general dispatch reassessment](../compute-model/DESIGN.md#general-dispatch-reassessment)
withdrew an all-site change that kept the query and called the chunk when the
allowance was zero: on an Intel host the records kernel at one worker became
30.7 percent slower, although one-worker runs never execute the overlapped
world. The cause found was changed inlining and placement in the shared
caller, not the new branch. The design tree therefore retains runtime-query
entry through the splitter. Any lowering this investigation proposes must
therefore show that it leaves the formal kernels' one-worker and four-worker
times unchanged in the maintained compute-regression comparison, not only that
it helps the small loop. This step measures attribution only and changes no
compiler code.

## Step 1: attribution on a minimal program

The program [`small_split.wf`](../../experiments/loop-split-grain/small_split.wf)
calls `mark`, whose loop writes three cells of one boxed array, from a
sequential loop 20 million times (the build's `CALLS`). The outer loop is not
split, because every call writes the array. The arms differ only in the
overlapped world's split site
([`arms.py`](../../experiments/loop-split-grain/arms.py)); the sequential world,
splitter, chunk, runtime and link are the same in each:

| Arm | Overlapped split site |
| --- | --- |
| emitted | as the compiler emits it: query, then splitter |
| twin | the emitted image copied under another name, the noise control |
| direct | calls the chunk, with no query and no splitter, as the sequential world does |
| zero | keeps the query; calls the chunk when the allowance is zero, the splitter otherwise |
| plain | the program compiled without `--par` |

Each arm runs at one and four workers (`WF_WORKERS`), with the arm order
rotated each round and the worker order alternated
([`measure.sh`](../../experiments/loop-split-grain/measure.sh)). Wall time
includes process start, which is the same in every arm. The machine is the
i9-14900K, through the `14900k` runner; a hosted run first checks that the
build works and sizes `CALLS` so one run takes about 0.2 to 1 s.

Readings, with `gap` the emitted arm's median four-worker time minus its
one-worker time, and `noise` the largest per-round deviation of the twin's
four-worker time from the emitted arm's:

- **No cost on main**: `gap` is no more than `noise`. The backlog item is
  then not reproduced by a small loop alone. The next step is the Snowghost
  edit itself, measured on main, before any lowering change.
- **The site is the cost**: the direct arm removes at least half of `gap`,
  with its four-worker time below the emitted arm's in every round. A
  lowering that skips the site when the work is small is then the candidate.
  The zero arm separates the query from the rest: if it removes nearly as much
  as direct, the query is cheap and the splitter entry or the lost inlining is
  the cost; if it removes little, the query itself is.
- **The cost is elsewhere**: `gap` exceeds `noise` and the direct arm
  removes less than half of it. The proposal is rejected as the main fix, and
  the rest of the overlapped world is examined instead: the caller's code, or
  the runtime's lanes running while the main thread works.

Step 1 is exploratory for the zero arm. Its readings above are fixed before
the measurement, but no threshold is set for it.

## Step 1 results

Run [37904899953](https://github.com/Ming-Research/Whitefoot/actions/runs/37904899953)
on the i9-14900K (16 cores and 32 threads as the runner reports them), at
`aa794e6d0` (main `3b1e5de75` plus this bundle), with `CALLS` 200 million,
11 rounds. Milliseconds are medians:

| Arm | One worker | Four workers | Four over one |
| --- | ---: | ---: | ---: |
| emitted | 108.1 | 1074.7 | 9.95 |
| twin | 108.0 | 1069.2 | 9.90 |
| direct | 108.0 | 108.0 | 1.00 |
| zero | 108.0 | 541.1 | 5.01 |
| plain | 108.0 | 107.9 | 1.00 |

Per round at four workers, over the emitted arm: twin 0.980 to 1.037, direct
0.100 to 0.105, zero 0.501 to 0.520, plain 0.100 to 0.105.

So `gap` is 966.6 ms, about 4.8 ns for each of the 200 million calls, and
`noise` is 3.7 percent of the emitted time. The direct arm removes all of
`gap` and is faster than the emitted arm in every round. By the readings fixed
above, **the site is the cost**. The zero arm removes half of it: keeping the
query costs about 2.2 ns a call, and the splitter entry together with the
chunk not being inlined costs about the other 2.7 ns. A change that keeps
either half recovers only half. At one worker every arm runs the same
sequential world and takes the same time.

A hosted sizing run before it
([37904042475](https://github.com/Ming-Research/Whitefoot/actions/runs/37904042475),
two cores with two threads each, 20 million calls, three rounds) agreed: four
workers took 5.7 times one worker's time, direct 0.97 times, and zero 4.2 times.

These numbers are for one program shape: a three-iteration loop of weight
7, called back to back. How much a real program loses depends on how often
it reaches such a site. Snowghost's measurements above are the motivating
cases; they were taken with older compilers and are re-measured in step 2.
