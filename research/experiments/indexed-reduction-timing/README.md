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

Rounds are interleaved: each round runs, for 256 cells and then 4096 cells,
`seq`, `par8`, `twin`, `par1` once each, in that fixed order. An unrecorded
pass first compares every build's checksum with `seq`'s; any mismatch, in that
pass or later, makes `run.sh` exit nonzero. It also runs `par8` once with
`WF_SCHED_REPORT=2` and keeps the scheduler's report (`threads`,
`workers_started`, `steals`) in `logs/`, to show that workers ran.

The worker count and the parallel lowering are the compute-bench conventions:
`--par` selects parallel lowering at build time and `WF_WORKERS` the pool size
at run time (`WF_WORKERS=1` is the sequential compute world;
[compute-bench README](../compute-bench/README.md#running-it),
`compiler/src/bin/whitefootc.rs`). `WF_SPLIT_WORK` is unset, so the default
split grain applies.

## Timed region

Each process generates its keys, then reads the monotonic clock, runs
`histogram` (the 256- or 4096-cell allocation, the counting loop and the
joined combine of any private copies), and reads the clock again. That
interval is `hist_ns`. Key generation, the checksum, output and process
start-up are outside it. `wall_ns` is the whole process as the shell saw it
and includes the key generation (a sequential loop identical in every build);
it is reported as a second view and dilutes any ratio. The criterion is judged
on `hist_ns`.

## Rejecting result

Pre-registered in the design document before any measurement
([Criterion](../../investigations/indexed-reductions/DESIGN.md#criterion)):

> On the i9-14900K through CI, the histogram above over 10^7 keys and 256
> cells runs at least 2 times faster with 8 workers than sequentially under
> lowering A, measured with interleaved runs and a twin of the sequential
> build. A smaller speedup, or a slowdown at 4096 cells, rejects A in favor of
> re-examining B or C.

Reading rules for this harness, fixed before measuring: "faster" is the median
over rounds of the per-round ratio `seq / par8` of `hist_ns`; "slowdown" is
that median below 1 at 4096 cells; a ratio inside the range of the per-round
`seq / twin` ratios is not a measured effect. The `seq / par1` ratio is
reported to show where a deficit comes from and decides nothing.

## Running

CI only, through the temporary workflow, whose inputs are `rounds` (default 3,
the size of a probe: choose the scale for the real run from the spreads it
shows) and `runner` (`github` or `14900k`). On the 14900K the workflow pins
`0-7` and runs `par8` at 8 workers and refuses a host with fewer than 8
processors; a hosted runner pins all its processors and uses that many
workers, which tests the harness and decides nothing about the criterion.
The workflow does what these commands do:

```sh
make -C compiler build
bash research/experiments/indexed-reduction-timing/run.sh measure \
  compiler/target/gate/whitefootc "$RUNNER_TEMP/timing" 3
```

`run.sh probe` is the same with 3 rounds and no round argument. `CPUS`
(default `0-7`) and `WORKERS` (default 8) are environment. The work directory
receives `raw.tsv` (`cells round build workers wall_ns hist_ns checksum`),
`summary.txt` (medians, spreads and paired ratios from `summarize.py`),
`manifest.txt` (revision, host, binary hashes) and `logs/`.

## Results

None yet.
