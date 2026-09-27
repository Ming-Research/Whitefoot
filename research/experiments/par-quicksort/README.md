# Parallel quicksort under `--par`

Dated 2026-09-25. This record supports the timing in the root README's
[second example](../../../README.md#sequential-code-parallel-results).

## Question

How long does a run of the README's quicksort take on 2,000,000 `u64` values
when the program is built without `--par`, and when it is built with `--par`
and run on four workers, on one four-processor host?

## Program

[`quicksort.wf`](quicksort.wf) starts with the README's `quicksort`, so the
ledger's line numbers match the README excerpt. It continues with a Lomuto
`partition` that takes the last element as the pivot, a xorshift `fill` with a
fixed seed, `sorted`, and `main`, which fills a heap array of 2,000,000 values,
sorts it and returns exit status 0 only when the result is sorted. Every run
sorts the same input.

`whitefootc --par --par-ledger quicksort.wf -o quicksort` prints, among
its lines:

```text
PAR permitted   quicksort.wf:10  pair(quicksort, quicksort)  eligible
PAR frontier    recursion budget runtime-derived  family emitted for 1 of 1 cyclic components
PAR frontier    component(quicksort)  budget-carrying clone family, entered with recursion budget runtime-derived
```

The runtime derives that budget from the worker count as
`floor(log2(64 * workers))`, 8 at four workers (`wf__par_recursion_budget` in
`compiler/src/backend/sched/core.c`); calls below it run the sequential clone.
The loops in `partition`, `fill` and `sorted` are denied in the same ledger,
so they run on one worker.

On 2026-09-26 `main`'s result type and its two `exit_status` calls were
qualified with `std::process`, where the standard library now declares them,
so the program compiles with the current compiler. Every other line and every
line number is unchanged, and the compiler at `3cd7e8139` prints the same
three ledger lines. The times below were measured before that edit, with the
compiler named under Environment.

## Environment

- Host: 4 processors (`getconf _NPROCESSORS_ONLN`), `Intel(R) Xeon(R)
  Processor @ 2.80GHz` (`model name` in `/proc/cpuinfo`), x86-64 Linux 6.18,
  shared with other agent sessions.
- `whitefootc` built from the compiler sources of `main` at `da368080f` with
  the `gate` Cargo profile, which changes how the compiler itself is compiled
  and not the program it emits; Ubuntu clang 18.1.3 at `-O2`, the level
  `whitefootc` links with.

## Method

[`measure.sh`](measure.sh) builds the program twice, without and with
`--par`, and runs each configuration RUNS times (7 here): the sequential
build, the `--par` build with `WF_WORKERS=1`, and the `--par` build with
`WF_WORKERS=4`. It prints every whole-process wall time in seconds (bash
`time`, to the millisecond) and the best per configuration, and stops if a run
does not exit 0. Each batch ran entirely under the host verification lock, so
no other guarded build, test or benchmark ran during it; unguarded processes
of other sessions were not excluded.

```sh
WHITEFOOTC=<repository-root>/compiler/target/gate/whitefootc \
  perl .github/run-check.pl par-quicksort \
  research/experiments/par-quicksort/measure.sh <scratch-root>/par-quicksort 7
```

## Results

Wall seconds, two batches of seven runs each, taken at 08:58 and 09:03 UTC:

| Configuration | Batch 1 best | Batch 1 range | Batch 2 best | Batch 2 range |
|---|---|---|---|---|
| sequential build | 0.178 | 0.178–0.197 | 0.181 | 0.181–0.195 |
| `--par` build, `WF_WORKERS=1` | 0.177 | 0.177–0.180 | 0.181 | 0.181–0.186 |
| `--par` build, `WF_WORKERS=4` | 0.069 | 0.069–0.087 | 0.065 | 0.065–0.086 |

The README quotes batch 1's best times to two decimal places, 0.18 s
sequentially and 0.07 s on four workers; batch 2 rounds to the same values.
The `--par` build on one worker runs as fast as the sequential build.

## Limitations

- One input size, one fixed input, one host, one worker count besides one.
- Whole-process time: it includes process start, the 16 MB allocation, `fill`
  and `sorted`, which are sequential, as is each call's `partition`; the top
  call alone scans all 2,000,000 values on one worker.
- Best of seven on a shared host, not a statistical comparison; no claim is
  made for other hosts, sizes or worker counts.
