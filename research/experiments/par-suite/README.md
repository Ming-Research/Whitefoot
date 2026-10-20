# Automatic parallel lowering baseline suite

This manual experiment compares the same source built without `--par`
(`T_seq`) and with ordinary `--par` (`T_W`). It implements stage one of the
[baseline investigation](../../investigations/par-suite/DESIGN.md), without
changing compiler policy, the specification, or the maintained gate. Keep
these files while that investigation consumes the suite; retire them together
when its successor replaces the protocol.

## Generated coverage

`generate.py OUTPUT` uses Python 3's standard library and writes 216 standalone
Whitefoot sources plus `manifest.tsv` into a **fresh scratch directory**.
No generated file belongs in the repository. The manifest records cell ID,
split, family, shape, intended bound, steps, tasks, calls, spread, word count,
seed, source path and SHA-256. Sizes and seeds are source constants; there are
no workload command-line inputs. Each source returns a full `u64` checksum
from `suite_entry`; the measurement host prints it through the ordinary host
adapter. Its own `main` also consumes the answer, so the source is a valid
standalone sequential program.

| Family | Cells | Axes |
| --- | ---: | --- |
| Work | 144 | 6 shapes × 8 work sizes × 3 spreads |
| Hot sequential callers | 18 | 6 shapes × 10, 10,000, 10,000,000 calls; four site units and one mix step |
| Intended bottleneck | 36 | 6 shapes × 3 spreads × memory/allocation |
| Size edges | 18 | Shape-specific input sizes, including 1 and 100,000,000 |

The six shapes are a flat counted wrapping-sum reduction (PAR-2), balanced
binary recursion, binary recursion split about 90/10 and 99/1, a deep spine
with one side leaf per level, and chained five-call diamonds (`a,b -> c,d -> e`).
The recursive siblings and diamond pairs use PAR-1 ordinary adjacent calls.
The DAG is a small bounded call DAG, not a general runtime graph executor.
For it, `tasks` counts diamonds: there are five leaf calls per unit. For the
other shapes, it counts leaf calls. Binary splits use integer division with a
minimum of one on the small side; at tiny sizes the skewed shapes coincide.

Arithmetic work is a dependent xor/shift/multiply/add recurrence, with
1, 10, 100, 1,000, 10,000, 100,000, 1,000,000 or 10,000,000 steps per largest
task. This is a **target** range of roughly a nanosecond to ten milliseconds,
not a measured conversion: CI must establish the actual time range. Work
cells use 16 leaves (flat/balanced), 128 (skewed), 64 (spine), or 16 diamonds.
Hot callers feed each checksum into the next call, so the outer loop remains
sequential and identical calls cannot be hoisted out of it.

Cost tiers are selected from each task's index mixed with the fixed seed
`0x574650415231`, modulo three. Divisors are `(1,1,1)`, `(1,10,100)`, or
`(1,100,10000)`. Each task executes `max(1, steps/divisor)` steps. Thus the
declared spread is a maximum: small work sizes clip it, and a small site may
not contain both extremes. Costs are fixed data, independent of worker width
and observed timing. No timing is used to regenerate or select points.

Memory cells construct a 64 MiB boxed array and stream prefixes up to its
8,388,608 words, with four site units. Allocation cells allocate one
`Box<Array<u64>>` per leaf, fill and fold 1..1,024 elements, with 128 site
units. Setup, checksum folding and release are charged to the process.
These are intended bottlenecks: cache size, optimizer elimination and emitted
loops must be inspected before claiming memory or allocation dominance.
Sources, LLVM, objects, build ledgers and executable images are retained.

The size family uses flat counts `(1,1000,1000000,100000000)`, balanced counts
`(1,1024,65536)`, skewed and spine counts `(1,128,1024)`, and DAG counts
`(1,128)`. The upper flat count is a reduction, so it needs no 800 MiB output
array. This sparse grid is not a complete Cartesian product; it prioritizes
shape/work coverage, repeated small sites and explicit size edges.

## Held-out rule

A seeded shuffle assigns exactly 108 points to `visible` and 108 to
`held-out`, before measurement. The seed and point order are fixed in the
generator. Do not tune a direction, constants, sizes or thresholds on held-out
measurements. `--phase visible` excludes them; `--phase verdict` is the only
entry that measures them. Parsing/type-checking held-out sources is allowed.
The real-program list also reserves `percent_decode` and `utf8parse` for
verdicts. Publishing a verdict is not permission to reuse its held-out points
to tune a later direction; a newly frozen split would then be needed.

## Running on CI

Use Linux, Python 3, Clang at `/usr/bin/clang`, Make, awk, sed, coreutils and
an already built compiler from the same checkout/runtime revision. Compiler
construction is separate from program timing. Run the first useful small
sample before choosing campaign scale; `pilot` uses six visible one-step
cells. No command below belongs on the owner's development laptop.

```sh
python3 -B research/experiments/par-suite/run.py \
  --compiler "$PWD/compiler/target/gate/whitefootc" \
  --build-dir /tmp/par-pilot-build --results /tmp/par-pilot-results \
  --phase pilot --rounds 3 --workers 1 2 \
  --startup-ns 0 --allowance-source uncalibrated-zero

python3 -B research/experiments/par-suite/run.py \
  --compiler "$PWD/compiler/target/gate/whitefootc" \
  --build-dir /tmp/par-visible-build --results /tmp/par-visible-results \
  --phase visible --rounds 5 \
  --startup-ns 0 --allowance-source uncalibrated-zero
```

For a frozen verdict use fresh paths and `--phase verdict`. The default widths
are `1 2 4 8 16 default`; `default` unsets `WF_WORKERS`, it is not an alias for
an explicit width. Above-core counts remain included. On the 14900K add `32`.
Rounds must be at least three. `--process-timeout` defaults to 60 seconds;
a timeout is missing measurement evidence and stops the campaign, never a
source-language rejection or a passing cell. Existing directories are refused.

Dispatch `compute-bench.yml` with `experiment: par-suite`,
`placement_runner: github|14900k` and `placement_rounds` (minimum 3). The job
builds the compiler, runs instrument controls, runs the small visible pilot,
then measures the frozen baseline grid including held-out verdict cells and
real programs. It uploads data on failure as well as success. The 60-minute
job limit is a campaign guard, not evidence that this initial grid fits an
hour: no native campaign has yet been run. Coordinate 14900K use through its
CI queue before dispatch; the job does not access it through the home network.

## Measurement and verdict

Generated images reuse `compiler/runtime.mk` and
`tests/programs/compute/host-adapter.awk`, as the existing compute harnesses
do. Each cell is compiled once in each mode; the twin is a byte-for-byte copy
of the `--par` executable, verified by SHA-256. Compilation order alternates
by cell. Measurement rotates cell, image and width order and reverses image/
width order in alternate rounds. It does not compile inside timed intervals.
All three images run at each requested width. Scheduler overrides are cleared;
no experimental policy flags or framework scheduler are installed.

`measure.c` encloses fork/exec through exit with `CLOCK_MONOTONIC`, and uses
`wait4`'s user plus system CPU time across all child threads. Bootstrap, pool
startup, stdout and shutdown are included. There is no unreported warmup for
these whole-process intervals. Every invocation writes one `raw.tsv` row:

```text
cell build workers round wall_ns cpu_ns exit_status checksum
```

The actual separator is a tab. Output and stderr remain in separate per-run
logs. A nonzero expected-status deviation, invalid generated checksum, or
checksum disagreement across any build, width or round stops the run after
writing the failed row. The runner records host topology, affinity, compiler,
source and instrument identities, allowances, and image hashes before/after.

`summarize.py RESULTS` requires the entire declared matrix with no duplicate
or extra samples. For each cell and width it reports the median of paired
`T_W/T_seq` ratios, each build's median CPU/wall, and the maximum paired
`abs(T_twin-T_W)/T_seq` as twin spread. The proportional allowance defaults to
`epsilon=0.02`. Supply a **previously measured and frozen per-host** startup
allowance as `--startup-ns D --allowance-source CALIBRATION_ID` for a qualified
H1 campaign. It cannot be fitted to these cell results.

The first manual baseline explicitly uses `d=0`, recorded as
`uncalibrated-zero`; its H1 labels are exploratory observations under that
stricter bound, not the criteria's final calibrated verdict. No startup
measurement is invented. With `A = median(epsilon + d/T_seq)` and
`E = median((T_W-(1+epsilon)*T_seq-d)/T_seq)`:

- Twin spread greater than `A` gives **inconclusive**.
- Otherwise `E <= 0` gives **pass**, and `E > twin spread` gives **fail**.
- The remaining overlap gives **inconclusive**.

W1 always reports a diagnostic/inconclusive H1 label because H1's claim
concerns two or more workers. A fail requires one separately retained rerun
before the criteria call it a refutation. The script does not automatically
retry or change thresholds. Ratios do not change its exit status; malformed,
missing or contradictory evidence does. `summary.tsv` contains every cell;
`summary.txt` prints the worst 20 by wall ratio.

## Maintained programs and next coverage

`real-programs.tsv` is the explicit inventory. Twelve self-contained entry
programs run through the same process protocol, including six further formal
compute entry points. Their existing answer checks supply expected exit
status; stdout SHA-256 and status are the recorded output identity. They are
sub-second **candidates**, not locally measured claims: a process reaching one
second stops the run for investigation rather than silently dropping it.

The five formal performance kernels (Mandelbrot, UTF-8 records, FIR, adaptive
quadrature, stencil) use the unchanged `tests/performance/Makefile`, runner,
complete-result oracles, `compare.sh`, reducer and verdict. The compiler shim
removes only `--par` for the sequential arm. First the identical parallel
images occupy both arms of the maintained null campaign; only a passing null
allows the sequential/parallel campaign. The inventory is checked against the
maintained Makefile. These results live under `kernels-null/` and
`kernels-seq-par/`, with their own checksums/comparison extents and manifests.

That established instrument measures five calls after warmup, five rounds,
and eligible widths 1/2/4 only. Its paired performance verdict is distinct
from this suite's H1 process verdict; the suite does not relabel its checked
output extent as a checksum or its call time as process time. These kernel
rounds are fixed even when `--rounds` changes. `--skip-kernels` is an explicit
partial campaign, recorded in metadata. Daily CI continues to consume only
the maintained instrument, with no dependency back into this research tree.

Next steps, intentionally unimplemented in this baseline:

- OpenMP C references for flat and balanced shapes, then skew/spine/DAG
  references and H2 work/critical-path accounting. Reuse compute-bench's
  `map`/`fork2` backend contracts; it currently supplies serial, static,
  oneTBB, Rayon and Parlay backends, not an OpenMP backend. Do not substitute
  another algorithm's speedup or count a missing reference as H2 success.
- Freeze a measured per-host startup allowance before a qualified H1 verdict;
  establish the actual per-task time range and full-run duration on CI.
- Snowghost setup/style/layout/edits, Halo's sequential interpreter, and
  firn compute beside I/O: hooks pending pinned checkouts and workload commands
  from their owning repositories. No stub result is emitted.
- `wfgrep` requires explicit input fixtures; broader maintained program
  coverage must name inputs and output oracles rather than merely run main.
- Apple Silicon, CPU-affinity restrictions, a busy host, and composition with
  waiting/I/O contexts remain qualification work. A clean Linux run cannot
  establish those claims, or H1's universal argument about fork overhead.
