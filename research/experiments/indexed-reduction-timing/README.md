# Indexed-reduction timing

Serves the performance criterion of
[indexed reductions](../../investigations/indexed-reductions/DESIGN.md#criterion).
Temporary: the workflow `.github/workflows/indexed-reduction-timing.yml` that
runs it is removed before the branch is ready; this directory stays as the
measurement's record.

## Question

Does lowering A (private copies combined in leaf order) make the design
document's histogram over 10^7 keys faster with 8 workers than sequentially,
and is it still no slower at 4096 cells?

## Comparison

The same source, built twice and run under four conditions, on the owner's
i9-14900K through CI (`runner: 14900k`), every process pinned with `taskset`:

| Build | Compiler flags | `WF_WORKERS` | Role |
| --- | --- | --- | --- |
| `seq` | none | 1 | the sequential baseline |
| `par8` | `--par` | 8 | the measured arm |
| `twin` | none, a byte copy of `seq` | 1 | noise control: identical to `seq` |
| `par1` | `--par` | 1 | the parallel build's sequential world: separates the cost of carrying the parallel lowering from the benefit of workers |

Two programs: `histogram.wf` (256 cells, `u8` keys, the design document's
loop verbatim) and `histogram_4096.wf` (4096 cells, `u16` keys masked to
twelve bits with `iand(wide, 4095_u64)`, otherwise identical). Keys are the
high half of successive xorshift64 states from one fixed seed, so every build
sees the same 10^7 keys.

Each executable takes one decimal command-line argument, K: for example,
`seq-256 0` runs the baseline and `seq-256 20` runs twenty histograms. Missing,
extra, empty, nondecimal, overlong (more than twenty digits) or overflowing
arguments exit with status 2. Every invocation generates the same keys once;
the generator's final state seeds a running checksum, including at K=0.
Each of the K repetitions allocates a fresh histogram over those keys, hashes
all its counters and folds that digest into the running checksum with XOR
followed by wrapping multiplication. The program writes that checksum as one
64-bit little-endian word, making every repetition's result contribute to the
output. K=0 performs key generation and the final output without a histogram.

Rounds are interleaved: each round runs, for 256 cells and then 4096 cells,
`seq`, `par8`, `twin`, `par1` in that fixed order, each at K=0 followed by
K=REPS. REPS defaults to 20 and is configurable. An unrecorded pass first
compares every build's checksum with `seq`'s for the same cell count and K;
checksums at different K are not compared. Any mismatch, in that pass or
later, makes `run.sh` exit nonzero. It also runs `par8` at K=REPS once with
`WF_SCHED_REPORT=2` and keeps the scheduler's report (`threads`,
`workers_started`, `steals`) in `logs/`, to show that workers ran; its checksum
is checked too.

The worker count and the parallel lowering are the compute-bench conventions:
`--par` selects parallel lowering at build time and `WF_WORKERS` the pool size
at run time (`WF_WORKERS=1` is the sequential compute world;
[compute-bench README](../compute-bench/README.md#running-it),
`compiler/src/bin/whitefootc.rs`). `WF_SPLIT_WORK` is unset, so the default
split grain applies.

## Timed quantity

The criterion uses process wall time. For each cell count, build and round,
the harness records `wall_ns` at K=0 and at K=REPS, then computes:

```text
per_histogram_ns = (wall_ns(K=REPS) - wall_ns(K=0)) / REPS
```

`wall_ns` is the elapsed time between the shell's two `date +%s%N` readings
around the pinned, timeout-wrapped process. The difference estimates the
additional cost of the repetitions: histogram allocation and initialization,
counting, joined combine of private copies, digest, checksum fold, loop
overhead and histogram cleanup. Both processes include argument parsing,
startup, key generation and one final output. Subtracting K=0 estimates that
common cost; differences in scheduling, cache state and startup remain noise,
which the interleaved rounds and sequential twin expose. This is an amortized
process estimate including the digest, rather than an isolated counting-loop
interval.

In-program timing was dropped after hosted probe run `37775470319` reported
about 2.1 ms for 10^7 keys into 256 cells, about 0.2 ns per key, implausible for
the sequential counting loop. The local counter array does not escape, so
LLVM can move its computation across the opaque clock calls; those reads do
not reliably bracket the histogram. The programs now contain no clock reads
and report no in-program interval. That probe is invalid timing evidence.

`summarize.py` forms each baseline-subtracted pair before computing medians,
minima, maxima, spreads and the paired per-round ratios. Missing or duplicate
runs and inconsistent REPS values are invalid data. A nonpositive paired
difference is retained in the timing statistics and reported as unresolved;
any ratio involving that build and round is reported unavailable, without
dropping noisy rounds or using them to judge the criterion.

## Rejecting result

Pre-registered in the design document before any measurement
([Criterion](../../investigations/indexed-reductions/DESIGN.md#criterion)):

> On the i9-14900K through CI, the histogram above over 10^7 keys and 256
> cells runs at least 2 times faster with 8 workers than sequentially under
> lowering A, measured with interleaved runs and a twin of the sequential
> build. A smaller speedup, or a slowdown at 4096 cells, rejects A in favor of
> re-examining B or C.

Reading rules for this harness, fixed before measuring: "faster" is the median
over rounds of the per-round ratio `seq / par8` of `per_histogram_ns`;
"slowdown" is that median below 1 at 4096 cells; a ratio inside the range of the per-round
`seq / twin` ratios is not a measured effect. The `seq / par1` ratio is
reported to show where a deficit comes from and decides nothing.

## Running

Measured through a temporary workflow on the research branch, removed once
the results below were recorded (its last revision is `cd2481dc4` in the
branch history): it built the compiler on the runner and ran the commands
below on the i9-14900K, pinned to `0-7` with 8 workers, or on a hosted runner
with all its processors, which tests the harness and decides nothing about
the criterion. To repeat it, restore that workflow on a work branch.

```sh
make -C compiler build
REPS=20 bash research/experiments/indexed-reduction-timing/run.sh measure \
  compiler/target/gate/whitefootc "$RUNNER_TEMP/timing" 3
```

`run.sh probe` is the same with 3 rounds and no round argument. `CPUS`
(default `0-7`), `WORKERS` (default 8) and `REPS` (default 20, a positive
decimal u64 of at most twenty digits) are environment. The work directory
receives `raw.tsv` (`cells round build workers k wall_ns checksum`), with
both K rows for each build and round,
`summary.txt` (medians, spreads and paired ratios from `summarize.py`),
`manifest.txt` (revision, host, REPS, binary hashes) and `logs/`.

## Results

The first hosted probe timed the histogram call in-program and measured about
0.2 ns per key, which a sequential counting loop cannot reach: the optimizer
moved the local, unescaped counting loop across the opaque clock reads. The
in-program timing was dropped for the paired K=0 / K=20 process timing above.

All runs at main `691ea8106` (the #274 merge), through the temporary workflow
on this branch, since removed.

| Run | Host | Rounds | 256 cells seq/par8 median (min..max) | 4096 cells seq/par8 median (min..max) | seq/twin 256 / 4096 |
|---|---|---|---|---|---|
| 37832422819 | GitHub ubuntu-24.04, 4 vCPU, 4 workers | 3 | 1.722 (1.592..1.878) | 1.695 (1.671..1.732) | 0.989 / 1.005 |
| 37834742412 | i9-14900K VM, cpus 0-7, 8 workers | 3 | 2.328 (1.895..3.212) | 2.000 (1.789..2.310) | 0.976 / 1.004 |
| 37835484141 | i9-14900K VM, cpus 0-7, 8 workers | 10 | 2.377 (2.152..3.342) | 1.878 (1.582..2.169) | 0.955 / 0.999 |

Sequential time per histogram on the i9-14900K: 1.98 ms (256 cells) and
2.19 ms (4096 cells), median of 10 rounds. The scheduler report showed 8
threads, 7 workers started and about 420 steals per run.

Reading against the rejecting result: the 3-round probe's spread straddled
2.0, so the run was extended to 10 rounds; there every paired round at 256
cells is at least 2.15, and 4096 cells is faster, not slower (1.88). The
performance criterion holds for lowering A on this machine and workload; it
says nothing about other hosts or reduction shapes.
