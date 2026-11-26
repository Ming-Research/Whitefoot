# Code placement and function alignment

A change that leaves a compute kernel's generated code byte-identical can
still make the kernel measurably slower, because an unrelated change ahead of
it in the image moves its instructions. This investigation asks whether
starting every emitted and runtime function on a 64-byte boundary makes the
five compute kernels' times independent of where they land, what it costs,
and whether loop-header alignment adds anything. Its decisions are in
[design/decisions/compiler/lowering/code-alignment.md](../../../design/decisions/compiler/lowering/code-alignment.md).
The cost of the aligned layout to `records` on some processors, accepted
provisionally when the alignment was adopted, was
[measured again below](#the-aligned-layouts-cost-to-records-reopened): it is
established on the EPYC 9V74 alone, no candidate layout rule met the
criterion, and the owner accepted the present layout
([the outcome](#the-outcome)).

## The observation

The `records` kernel's hot function, `wf__par_seq_summarize_records`, ran
about 21 ms at one worker when it started at image offset 0x3200 and about
29 ms at 0x3210, with identical instructions: cachegrind counted 2,512,523,716
and 2,512,524,120. One more imported libc function adds a PLT entry before
`.text`, which is enough to move it; the stackful waiting-context floor's
`mprotect`, since removed, did that, and so did an unrelated `getpagesize`
import linked beside the base runtime. The times were 20.7 ms for the base,
29.5 ms with the extra import and 28.5 ms for the candidate floor, medians of
eleven runs on a 2.1 GHz Xeon. These readings were recorded in `docs/todo.md`
until this investigation replaced that entry.

The [result-register placement controls](../result-registers/DESIGN.md#hosted-compute-regression)
had found the same on a local Intel host: a never-called function that
shifted `records`' loop copies by 16 to 48 bytes, with no executed instruction
changed, moved either of two lowerings by up to 15 percent at one worker and
18 percent at two and four, and reversed their order, while moving only the
runtime did not. One further control compiled both arms with
`-falign-loops=64`; none aligned a function.

PR #251 (`std::fs::truncate_file`) then failed `compute-regression` on
`records` in three of its four measured runs, each time against a merge base
whose kernel objects were byte-identical to the candidate's:

- run 37423869839 at `0f4d7458`: FAIL, `records` adverse at two widths;
- run 37424374444 at `1b202dc4`, which differs from `0f4d7458` only in
  `docs/todo.md`: PASS, with `records W=4 wall=0.930878 lower=5/5` reported
  as a suspect and `W=2 wall=0.997473 lower=4/5`;
- run 37426533473 at `a98d8ce1`: FAIL, `records W=1 wall=0.993087 lower=5/5`,
  `W=2 wall=0.956197 lower=5/5`, `W=4 wall=0.915599 lower=5/5`;
- run 37427969647 at `d6acc01a`: FAIL, `records` adverse at two widths.

The slowdown was present in every run. Whether the verdict failed depended
only on whether width 2 also crossed the 0.97 line. In run 37426533473's
artifact, `records.o`, `records_oracle.o`, `runner.o` and every scheduler and
completion-runtime object are byte-identical between the arms; only
`native/ordinary_values.o` and `native/completion/bridge.o` differ, by the new
truncation functions. Their one new import, `ftruncate`, grows `.rela.plt` by
24 bytes and `.plt` by 16, so every function of `.text` moves by 16 bytes.

### Local reproduction

The same images, built locally with the merge base's runtime (`c2e5a180`) and
the PR's (`364f86c2`) and one compiler, since the kernel's LLVM is identical,
have the CI images' addresses (`llvm-nm -n`):

| function | base | PR | base mod 64 | PR mod 64 |
|---|---|---|---|---|
| `wf__par_split_summarize_records.0` | 0x2cf0 | 0x2d00 | 48 | 0 |
| `wf__par_chunk_summarize_records.1` | 0x30d0 | 0x30e0 | 16 | 32 |
| `wf__par_seq_summarize_records` | 0x3280 | 0x3290 | 0 | 16 |

Data did not move in any way that could matter: `.bss` starts at 0x1c300 in
both images with every symbol at the same address, and the one `.data` symbol
that moved, the 8-byte `split_work`, moved from 0x1c2b0 to 0x1c2b8 inside the
same 64-byte line. The difference between the images is code placement.

On the local host (Intel Xeon, family 6 model 207, 4 vCPUs, Linux 6.18,
clang 18.1.3), ten interleaved rounds of the two images read `records` at
width 1 unchanged (paired ratio 1.025, 1 of 10 rounds slower), at width 2
0.897 and at width 4 0.905, both 10 of 10 slower: the CI pattern. At widths 2
and 4 the hot function is the chunk function, whose start moved from 16 to 32
modulo 64.

## Clang does not align LLVM input with `-falign-functions`

Clang implements `-falign-functions=N` in its C front end, which writes the
alignment onto each function it generates. A module given to clang as LLVM
input passes no front end, so the option has no effect on it. With clang
18.1.3 and a module of two functions, `f` and `g`, neither carrying an
`align`:

| arguments | `f` | loop in `f` | `g` |
|---|---|---|---|
| `-O2` | 2^4 | 2^4 | 2^4 |
| `-O2 -falign-functions=64` | 2^4 | 2^4 | 2^4 |
| `-O2 -mllvm -align-all-functions=6` | 2^6 | 2^4 | 2^6 |
| `-O2 -falign-loops=64` | 2^4 | 2^6 | 2^4 |
| C source, `-O2 -falign-functions=64` | | | 2^6 |

An `align 64` on the definition in the module, and loop metadata
`!{!"llvm.loop.align", i32 64}`, both take effect. Both the driver and
`tests/performance/Makefile` compile the emitted module as `-x ir`.

This changes what the [2026-09-11 alignment
sections](../compute-runtime/RESULTS.md) measured. The compute bundle's
`WF_ALIGN` passed `-falign-functions=64 -falign-loops=32` to the emitted
module's `.ll` as well as to the runtime's C, so the module's loops were
aligned to 32 bytes but none of its functions was aligned. That run's shifted
arm moved the scheduler and not the kernel, and its alignment arm left every
kernel function where it was; neither measured what aligning the kernel's
functions does to a kernel that moves.

## Alternatives

- **Function alignment written into the emitted module**, `align 64` on every
  definition the emitter produces and on every definition of the hand-written
  `ordinary_values.ll`. It reaches every consumer of the module, the driver's
  link, `--emit-llvm` with the regression Makefile, and link-time
  optimization, from one place.
- **The same alignment through host-compiler arguments**,
  `-mllvm -align-all-functions=6`: an internal LLVM option the regression
  Makefile and a link-time-optimized link would each have to repeat.
- **Aligning each module's code section without padding each function**: one
  `align 64` definition raises the object's `.text` alignment to 64 bytes, so
  the module's functions keep their offsets within a line whatever precedes
  the module, without padding between them. A change in one function's size,
  which most compiler changes make, still moves every later function of the
  module within its line, and the C runtime has no such option short of
  aligning its functions.
- **Aligning only the functions that are hot**: the readings below show
  that a kernel's time depends on where several of its functions fall, and
  which are hot is not known when the module is emitted.
- **`-mbranches-within-32B-boundaries`**: pads branches away from 32-byte
  boundaries for the Skylake-family jump erratum; it leaves the function's
  start where it falls, and the hosts that show the effect are not that
  family.
- **Placement-robust measurement without a compiler change**: the gate would
  stop blaming changes, but a program would still change speed by tens of
  percent when an unrelated import moves it.
- **Runtime alignment** as well, `-falign-functions=64` on the C runtime: a
  change to one runtime unit moves every runtime function after it, which
  the module's alignment alone does not prevent.
- **Loop-header alignment** as well, `-falign-loops=32` on the module: once
  a function starts on a 64-byte boundary its loops' offsets within their
  lines are fixed, so this is a separate performance choice whose padding
  runs on every loop entry.

## Exploratory local readings

These come from the 4-vCPU local host, which is noisy; they selected the
direction the owner approved and do not decide anything. Eight interleaved
rounds of `records`, with a pad of 0, 16, 32 or 48 bytes linked ahead of the
module and the PR's runtime (`un`, current compiler; `al`, every module
function aligned with `-mllvm -align-all-functions=6`); per-placement medians
in milliseconds:

| width | `un` base, p0, m16, m32, m48 | `al` base, p0, m16, m32, m48 |
|---|---|---|
| 1 | 24.75, 23.46, 30.26, 23.00, 23.86 | 24.41, 24.54, 24.50, 24.71, 24.29 |
| 2 | 12.40, 12.36, 12.78, 15.80, 11.66 | 12.38, 12.28, 12.73, 12.46, 12.77 |
| 4 | 6.12, 6.53, 6.28, 8.07, 5.68 | 6.33, 6.60, 6.48, 6.63, 6.48 |

The unaligned images span 32, 36 and 42 percent; the aligned ones 1.7, 4.0
and 4.7 percent, inside this host's noise. The time follows the joint
placement of several functions rather than the chunk function's start alone:
`un m32` puts the chunk function at 0 modulo 64, as alignment does, and is the
slowest unaligned image at widths 2 and 4. Six rounds of the other four
kernels showed spreads of a few percent under either build and no cost of
alignment beyond that noise. Alignment adds never-executed padding to each
kernel module: 576 to 1,280 bytes of `.text` across the five modules.

## Method

The runs below used `placement.sh` and `summarize.py` as they stood at
`8097c03a`, and `main` kept them, the reducer gaining one paired line, until
both were rebuilt for [the next runs](#the-next-runs): one compiler builds
every arm, and the reducer reads the arms from the data. The October harness
worked as follows.
It built two trees with their own compilers and
their own `tests/performance/Makefile`: the unaligned tree (`main` before this
change) and the aligned tree (this change). From their objects it links four
arms of every kernel:

- `U`: the unaligned tree's images, as `main` ships them;
- `F`: the aligned tree's emitted module with the unaligned tree's runtime
  (Q1 alone);
- `FR`: the aligned tree's images, module and runtime aligned (Q1 and Q2);
- `FRL`: `FR` with the module also compiled with `-falign-loops=32` (Q3);

each at six placements: `p0`; `m16`, `m32` and `m48`, that many bytes linked
ahead of the module, which moves the module and everything after it as a new
import does; and `r16` and `r48`, that many bytes linked ahead of the runtime
only, as a grown runtime unit does. A byte-identical copy of `U p0`, `null`,
is timed as its own image. Under alignment a pad cannot move code within a
line: in a local build with the hosted runners' toolchain, `FR`'s `m16` and
`r16` reproduce `p0`'s addresses, and its `m32`, `m48` and `r48` move code by
exactly 64 bytes, so `FR`'s invariance is invariance to whole-line shifts,
and its `m16` and `r16` act as further nulls. Every image is verified at widths 1, 2 and 4, then
each round runs every image once per kernel and width, one process each, with
the regression runner's one warmup and five recorded calls, in an order that
rotates and alternates direction from round to round. Every process is
pinned to vCPUs 0–3. [`summarize.py`](summarize.py) reduces the rounds.

The runs use the `placement` experiment of
[`compute-bench.yml`](../../../.github/workflows/compute-bench.yml) on the
owner's i9-14900K, a Hyper-V machine with 32 vCPUs whose mapping onto
performance and efficiency cores the guest cannot see, and on a hosted
`ubuntu-24.04` runner. A first run of three rounds on each host sizes the
decisive run; the decisive run's number of rounds is chosen from its spread and
stated with the results.

## Criterion

Fixed before the decisive runs. A process's time is the median of its five
recorded calls. Two series of the same kernel and width are compared by round:
the ratio is the median of the per-round ratios, and a difference is
established when that ratio is beyond 3 percent with at least 80 percent of
the rounds leaning the same way, the regression gate's own threshold.

- **C0, host control.** A kernel and width where `null` differs from `U p0`
  is inconclusive on that host and counts for nothing below.
- **C1, discrimination.** On a host where no placement of `U` differs from
  `U`'s `p0` in any conclusive cell, the host cannot discriminate and its
  readings select nothing. C1 first required a spread of 10 percent across
  `U`'s placements. The three-round sizing run on the 14900K (run
  37432962314) read `U`'s `records` spreads at 7.7, 5.7 and 8.8 percent, with
  width 1 at 9.48 ms under `m16` against 8.80 to 9.00 ms elsewhere, so the
  10 percent line would have excluded a host that shows the effect. C1 was
  restated as the paired rule above before any decisive run. Admitting more
  hosts makes Q1 and Q2 harder to meet, not easier.
- **C2, invariance.** An arm is invariant at a kernel and width when no
  placement differs from that arm's `p0`, in either direction.
- **C3, cost.** An arm costs time at a kernel and width when the per-round
  median over its placements is slower than the same median of the arm it is
  measured against.
- **Q1** holds when `F` meets C2 over `p0`, `m16`, `m32` and `m48` and does
  not cost against `U`, in every conclusive cell of every discriminating host.
- **Q2** holds when `FR` meets C2 over all six placements and does not cost
  against `F`.
- **Q3**, loop alignment, stays refused unless `FRL` meets C2, costs nothing
  against `FR` in any cell, and is faster than `FR` by the same rule at two or
  more widths of some kernel.
- If neither `F` nor `FR` meets C2, no alignment is adopted and the TODO
  records the result.

Finally the event itself: the PR #251 pair, the merge base's runtime against
the PR's under the aligned compiler, must pass `tests/performance/compare.sh`.
The workflow's replay step built the merge base's runtime (`c2e5a180`, its
`ordinary_values.ll` given the alignment its successors carry) with the
revision's compiler and flags, and compared it with the `FR` images. The step
was [removed later](#the-sizing-runs), once that runtime no longer linked with
the compiler's modules.

## Results

The decisive runs are at `d4f67df6` or `8097c03a`, which differ only in
records and workflow text, the unaligned tree being their merge base with
`main`, `364f86c2`. Cells are milliseconds per call, the median over the
rounds of each process's median call.

The criterion compares `FR` with `F`, not with `U`. On the cell medians,
`U`'s median placement over `FR`'s, for `records` (below 1 means the aligned
images are slower):

| run | processor | width 1 | width 2 | width 4 |
|---|---|---|---|---|
| 37434117176 | i9-14900K | 0.994 | 0.995 | 0.997 |
| 37434120227 | EPYC 7763 | 1.001 | 1.008 | 0.988 |
| 37436514442 | EPYC 7763 | 1.001 | 1.010 | 0.990 |
| 37436517830 | EPYC 7763 | 1.000 | 1.012 | 0.985 |
| 37439142869 | EPYC 7763 | 0.999 | 0.982 | 0.986 |
| 37439146282 | EPYC 7763 | 0.999 | 1.021 | 0.968 |
| 37439150433 | EPYC 7763 | 1.003 | 1.051 | 0.975 |
| 37436510214 | EPYC 9V45 | 0.953 | 1.012 | 1.107 |
| 37439138936 | Xeon 8370C | 0.970 | 1.021 | 1.001 |
| 37432965442, three rounds | EPYC 9V74 | 0.954 | 0.877 | 0.910 |

The reducer now prints this comparison, paired by round, for later runs.

### i9-14900K, 20 rounds (run 37434117176)

The null moved no cell (C0). `U` met C1 in three cells, all `records`:
`m16` against `p0` 0.947 at width 1, and `m32` against `p0` 0.947 at width 2 and
0.952 at width 4. Q1 and Q2 hold: `F` and `FR` differ from their `p0` in no
cell at any placement, and neither costs in any cell, the cost ratios lying
between 0.994 and 1.011. Q3 does not select loop alignment: `FRL` is invariant
and slower in no cell, but faster than `FR` by the gate's rule in none either,
its ratios lying between 0.993 and 1.013. The other four kernels move by at
most 3.8 percent across `U`'s placements and in no cell by the paired rule.
`records`:

| width | arm | p0 | m16 | m32 | m48 | r16 | r48 |
|---|---|---|---|---|---|---|---|
| 1 | U | 8.851 | 9.318 | 8.842 | 8.875 | 8.833 | 8.877 |
| 1 | FR | 8.836 | 8.924 | 8.911 | 8.945 | 8.935 | 8.860 |
| 2 | U | 4.688 | 4.555 | 4.913 | 4.601 | 4.534 | 4.659 |
| 2 | FR | 4.662 | 4.569 | 4.652 | 4.650 | 4.672 | 4.643 |
| 4 | U | 2.441 | 2.405 | 2.548 | 2.399 | 2.391 | 2.418 |
| 4 | FR | 2.400 | 2.421 | 2.427 | 2.418 | 2.409 | 2.446 |

The replay of PR #251's runtime change under the aligned compiler passed
`compare.sh`: `records` 1.003421, 1.034001 and 1.021749 at widths 1, 2 and 4,
with one single-width suspect, `stencil` at width 1, 0.947191 with four of five
pairs lower.

### Hosted ubuntu-24.04, AMD EPYC 7763, six runs of 12 rounds

The hosted pool assigns an AMD EPYC 7763 (Zen 3), 9V74 (Zen 4) or 9V45, or an
Intel Xeon Platinum 8370C, machine of four vCPUs, two cores of two threads,
and a dispatch cannot choose one. Of eight decisive hosted runs, six landed on
a 7763: run 37434120227 at `d4f67df6`, and runs 37436514442, 37436517830,
37439142869, 37439146282 and 37439150433 at `8097c03a`, which differs from
`d4f67df6` only in records and workflow text. In all six the null moved no
cell, and `U` met C1 in two or three `records` cells, always at width 1 under
`m16` (0.926 to 0.938) and at width 4 under `m32` or `m48` (0.883 to 0.920, and
1.082 to 1.109).

No placement moved `FR` in any cell of any of the six, and `FR` cost nothing
against `F` in any cell (Q2). `F`, the module aligned over the unaligned
runtime, failed C2 or C3 in one cell in two of the six. In run 37436514442 it
read 10.261 ms at `p0` against 9.622 to 10.031 ms at the other placements of
`records` at width 4, a `m32` against `p0` ratio of 1.044 and a cost of 0.969
against `U` with 92 percent of the rounds slower. In run 37439142869 it cost
0.948 against `U` at `records` width 2, again with 92 percent of the rounds
slower. Q1 as written therefore fails on this host. The module pads move `F`'s
emitted functions by whole 64-byte lines and its unaligned runtime by 0 or 64
bytes, and `FR` in the same runs shows neither effect; on the cell medians of
those two runs `FR` lies within 2 percent of `U`'s median placement. Q3 never
selects loop alignment: `FRL` was faster than `FR` only at `records` width 2
in the first run, 1.040. The other four kernels spread by at most 1.8 percent
under `U` in the first run. Every replay of PR #251's runtime change passed,
two of them with a `records` width-2 suspect (0.946963 and 0.906576).
`records` in the first run:

| width | arm | p0 | m16 | m32 | m48 | r16 | r48 |
|---|---|---|---|---|---|---|---|
| 1 | U | 17.134 | 18.378 | 17.140 | 17.148 | 17.136 | 17.121 |
| 1 | FR | 17.100 | 17.144 | 17.105 | 17.093 | 17.185 | 17.145 |
| 2 | U | 8.825 | 9.051 | 9.280 | 8.652 | 9.132 | 9.141 |
| 2 | FR | 9.041 | 8.835 | 9.570 | 9.003 | 8.924 | 9.031 |
| 4 | U | 9.643 | 9.606 | 10.526 | 8.520 | 9.576 | 9.183 |
| 4 | FR | 9.778 | 9.590 | 9.389 | 9.639 | 9.862 | 10.155 |

### Hosted ubuntu-24.04, Intel Xeon Platinum 8370C, 12 rounds (run 37439138936)

The null moved no cell; `U` met C1 at `records` widths 1 and 2, spreading by
12.4 and 13.9 percent. Q2 holds: `FR`'s `records` spreads are 1.4, 0.3 and 0.2
percent. `F` failed C2 in one cell, `stencil` at width 2, `m48` against `p0`
1.042. The aligned `records` at width 1 reads 19.24 to 19.50 ms against
`U`'s 18.73 to 21.04, whose median placement is 18.77, about 3 percent
faster. `FRL` was faster than `FR` at `records` width 1, 1.036, and slower in
one cell, so Q3 does not select it. The replay passed.

### Hosted ubuntu-24.04, AMD EPYC 9V45, 12 rounds (run 37436510214)

The null moved no cell; `U` met C1 in all three `records` cells, spreading by
9.7, 15.1 and 32.8 percent. Q1 and Q2 hold in every cell; aligned `records` is
faster than `U` at width 4, a cost ratio of 1.107. `FRL` was faster than `FR`
at every width of `records`, 1.069, 1.087 and 1.042, but moved with placement
in some cell (the reducer at that revision did not name which), so Q3 does not
select it. The replay passed. `records`:

| width | arm | p0 | m16 | m32 | m48 | r16 | r48 |
|---|---|---|---|---|---|---|---|
| 1 | U | 10.974 | 11.862 | 10.845 | 11.694 | 10.816 | 10.815 |
| 1 | FR | 11.482 | 11.394 | 11.508 | 11.470 | 11.424 | 11.402 |
| 2 | U | 6.132 | 5.834 | 6.354 | 5.520 | 6.101 | 6.137 |
| 2 | FR | 5.996 | 5.937 | 6.060 | 6.152 | 6.081 | 6.024 |
| 4 | U | 5.836 | 6.166 | 6.774 | 5.099 | 5.958 | 5.964 |
| 4 | FR | 5.388 | 5.406 | 5.410 | 5.385 | 5.362 | 5.366 |

### Hosted ubuntu-24.04, AMD EPYC 9V74

No decisive run reached a 9V74. The three-round sizing run (run 37432965442)
did, and read the aligned `records` slower than every unaligned placement at
width 2: `U` 7.533 to 8.151 ms across its six placements against `FR` 8.637
to 8.833 and `F` 8.664 to 8.735. On the medians `FR` took 4.8, 14.0 and 9.9
percent longer than `U` at widths 1, 2 and 4. Three rounds select nothing.

`compute-regression` on this change compares the aligned candidate with
images of the merge base's compiler on whichever processor the hosted pool
assigns, which its log did not name until this change added it. That baseline
is not the experiment's `U`: `tests/performance/Makefile` includes the
candidate's `compiler/runtime.mk`, so the baseline's C runtime compiles with
`-falign-functions=64` while its module and `ordinary_values.ll` stay
unaligned, a layout no arm of the experiment built. The five runs below had
byte-identical compiler and `tests/performance` inputs. Two passed, at
`d4f67df6` and `8097c03a`, and three failed on `records`, five of five or four
of five pairs lower each time:

| run | commit | width 2 | width 4 |
|---|---|---|---|
| 37432958961 | `1bfe9449` | 0.875429 | 0.850267 |
| 37439315247 | `3ce59221` | 0.879152 | 0.862255 |
| 37441911232 | `210e06ae` | 0.914782 | 0.926627 |

The later runs name their processor; their compiler and `tests/performance`
inputs differ from those five only in a compiler test and the README:

| run | commit | processor | width 2 | width 4 | verdict |
|---|---|---|---|---|---|
| 37445053860 | `d1a13082` | EPYC 7763 | 0.992566 | 0.917337 | pass, width 4 a suspect |
| 37446065946 | `02aa1489` | EPYC 9V45 | 0.905475 | 0.927351 | fail |
| 37447392813 | `e7f10360` | EPYC 7763 | 0.971279 | 0.845258 | pass, width 4 a suspect |

The failures are therefore not the 9V74's alone, though their size matches
what the 9V74 sizing run reads for `main`'s placement, `U` at `p0` over `FR`
at `p0`: 0.884 at width 2 and 0.917 at width 4. The 9V45 and 7763 readings do
not match those processors' decisive runs, where `FR` was faster than `U` at
`p0` or within 2 percent of it: on the 9V45 5.996 against 6.132 ms at width 2 and 5.388
against 5.836 at width 4, and on the first 7763 run 9.778 against 9.643 ms at
width 4. Either hosts reporting the same model differ, or the baseline's aligned
runtime under its unaligned module is a faster layout of `records` on these
processors; no run here separates the two. Every one of these runs passed its
placement control, the three with a 96-byte pad after moving each kernel's
module by 128 bytes; the one at `210e06ae` reported a single-width suspect
there, `records` at width 1, 0.962886 with four of five pairs lower.

## Conclusion

These are the conclusions of the October runs, whose shifts moved code by at
most 96 bytes. The later runs establish a cost of the aligned layout to
`records` on the EPYC 9V74 and a dependence of the aligned `records` on a
2048-byte move on the 9V74 and the 9V45 ([the outcome](#the-outcome)).

- **Invariance.** With the module and the runtime aligned (`FR`), no shift
  changed any kernel's time by the gate's rule in any cell of the nine
  decisive runs, on the 14900K, six 7763s, a 9V45 and an 8370C, while the
  unaligned images moved `records` on every one of them (Q2).
- **The module alone.** `F` failed Q1 as written in one cell in three of the
  eight hosted runs (`records` on two 7763s, `stencil` on the 8370C) and held
  on the 14900K and the 9V45. The adopted configuration is `FR`.
- **Cost.** `FR` cost nothing against `F` anywhere. Against `U`'s median
  placement, the criterion did not judge it; on the cell medians the aligned
  `records` took between 4.9 percent longer (the 9V45 at width 1) and 9.7
  percent less time (the 9V45 at width 4) in every decisive run, and 4.8 to
  14.0 percent longer on the 9V74 in a three-round run. Against the
  regression gate's baseline, the merge base's module over an aligned
  runtime, four of eight `compute-regression` runs of this change failed on
  `records` at ratios of 0.850 to 0.927, 7.8 to 17.6 percent longer; the only
  one of them that names its processor ran on a 9V45, and both passing runs
  that name theirs ran on 7763s and read width 4 as a suspect, 0.917 and
  0.845.
- **Loop alignment** was selected nowhere, though it made `records` faster at
  some widths on the 9V45 and the 8370C.
- **The event.** Every replay of PR #251's runtime change under the aligned
  compiler passed `compare.sh`. `compute-regression`'s placement control
  passed on the aligned compiler in all five hosted runs, with a 32-byte pad,
  which moves aligned code by 0 or 64 bytes depending on where the code ahead
  of the module ends (64 in a local build). It now pads by 96 bytes, which
  moves aligned code by one or two whole lines and unaligned code by 32 bytes
  within a line, and checks that the module moved; locally it fails on the
  unaligned compiler (`records` 0.790, 0.905 and 0.863) and passes on the
  aligned one.

## Limitations

- A 64-byte boundary fixes where instructions fall within their line. Effects
  keyed on higher address bits, such as branch-predictor aliasing between
  functions or 4 KiB aliasing, can remain: the aligned arms' `m` and `r`
  placements shift by 0 or 64 bytes, and `F`, the module aligned over the
  unaligned runtime, moved under such a shift in one cell of one 7763 run.
  The later runs measured one such effect: a 2048-byte move changes the
  aligned `records` at widths 2 and 4 on the EPYC 9V74 and the 9V45
  ([the outcome](#the-outcome)).
- Apple's arm64 cores use 128-byte lines. The macOS runner resolves only
  about 20 percent ([compute-runtime results](../compute-runtime/RESULTS.md)),
  so nothing here is measured on arm64.
- The host C objects of the kernel's oracle and the timing runner are not
  aligned; they lie outside the timed interval except for the calls into the
  kernel.

## The aligned layout's cost to `records`, reopened

The decision accepted the aligned layout's cost to `records` provisionally,
on a three-round run. This part asks why the aligned `records` is slower on
the EPYC 9V74 and whether a deterministic layout exists without that cost.
The subsections before [the next runs](#the-next-runs) were read on
2026-11-25 from the runs above and from one probe; that subsection states the
new runs' question, arms and criterion, fixed before any of them ran, the
subsections after it report those runs, and [the outcome](#the-outcome)
closes the section.

### The earlier runs under a 95 percent interval

The reducer now gives the shipped arm's line an interval: per round, the
median of `U` over its six placements divided by the same median of `FR`,
and the order statistics that bound the median of those ratios at 95 percent
(the 3rd and 10th of 12 rounds, the 6th and 15th of 20). Below 1 the aligned
images are slower. `records`, from the saved `raw.tsv` of each run:

| run | processor | width 1 | width 2 | width 4 |
|---|---|---|---|---|
| 37434117176 | i9-14900K | 0.996 (0.992 to 1.002) | 1.005 (0.995 to 1.012) | 1.005 (0.992 to 1.017) |
| 37434120227 | EPYC 7763 | 1.003 (0.999 to 1.005) | 0.971 (0.955 to 1.029) | 0.988 (0.947 to 1.026) |
| 37436514442 | EPYC 7763 | 1.002 (0.998 to 1.006) | 1.003 (0.985 to 1.026) | 0.999 (0.981 to 1.023) |
| 37436517830 | EPYC 7763 | 1.001 (0.998 to 1.003) | 1.015 (0.953 to 1.025) | 0.982 (0.958 to 0.999) |
| 37439142869 | EPYC 7763 | 1.000 (0.996 to 1.002) | 0.988 (0.884 to 1.027) | 0.974 (0.952 to 1.071) |
| 37439146282 | EPYC 7763 | 1.002 (0.999 to 1.003) | 1.009 (0.948 to 1.051) | 0.968 (0.954 to 0.988) |
| 37439150433 | EPYC 7763 | 1.003 (0.997 to 1.004) | 1.011 (1.006 to 1.039) | 0.984 (0.954 to 0.995) |
| 37436510214 | EPYC 9V45 | 0.959 (0.948 to 0.978) | 1.008 (0.984 to 1.022) | 1.102 (1.087 to 1.108) |
| 37439138936 | Xeon 8370C | 0.970 (0.968 to 0.976) | 1.018 (1.015 to 1.024) | 1.001 (1.000 to 1.002) |
| 37432965442, three rounds | EPYC 9V74 | 0.956 | 0.880 | 0.909 |

An interval wholly below 1 appears at width 1 on the 9V45 and the 8370C and
at width 4 on three of the six 7763 runs; three rounds give no interval, so
the 9V74's cost is still unmeasured by this test. Its three per-round ratios
lie between 0.949 and 0.970, 0.872 and 0.886, and 0.906 and 0.930.

### Where the inner loops of `records` fall

Read from the candidate image of `compute-regression` run 37446065946
(`02aa1489`, clang 18.1.3), whose module is the aligned arms'. Width 1 runs
`wf__par_seq_summarize_records`; widths 2 and 4 run
`wf__par_split_summarize_records.0`, which holds the chunk function's body
inline. Each has the same two inner loops: 54 bytes over the continuation
bytes of a multi-byte sequence, at function offset 0x110 and 0x2d0, and 21
bytes over ASCII bytes, at 0x170 and 0x330. Those offsets are 16 and 48
modulo 64 in both functions, so a function's start within its line places
both loops:

| start modulo 64 | multi-byte loop, bytes of its line | ASCII loop |
|---|---|---|
| 0, every aligned arm | 16 to 69, crosses into the next line | 48 to 68, crosses |
| 16 | 32 to 85, crosses | 0 to 20 |
| 32 | 48 to 101, crosses | 16 to 36 |
| 48 | 0 to 53 | 32 to 52 |

`U`'s module placements start the sequential function at 16, 32, 48 and 0
(`p0`, `m16`, `m32`, `m48`) and the split function at 0, 16, 32 and 48. At
widths 2 and 4 `m48`, the split function at 48, is the fastest placement of
`U` on all four hosted models and within 1.5 percent of the fastest on the
14900K, and `m32`, at 32, is the slowest at width 4 on every host. At width 1
`m32`, the sequential function at 48, is the fastest or within 1 percent of
it on every host, and `m16`, at 32, the slowest on every host but the 9V74.
On the 9V74's three rounds the sequential function at 48 read 14.956 ms
against 17.119 to 17.369 for `FR`. So on every processor measured the
unaligned placement that keeps both inner loops of the hot function inside
one line each is the fastest or within 1.5 percent of it, and the aligned
layout fixes both across a line boundary. This reading was found in these
runs, not predicted before them: it is the hypothesis the new runs test.

It does not account for all of the 9V74's reading. At widths 2 and 4 `U` at
`p0` starts the split function at 0, as `FR` does, and read 7.636 ms against
`FR`'s 8.637 to 8.833 at width 2 and 7.826 against 8.530 to 8.690 at width
4. What else differs between those images is where the module's
other functions fall, the address bits above a line, and the image's data,
which an aligned image moves because its code is larger. The
[stop-signals record](../stop-signals/README.md#startup-cost-on-amd-hosts)
measured `stencil` 9 to 30 percent slower at width 1 on the 7763 and the
9V74 when a thread created before the entry changed where memory was mapped.
No arm here holds the data's addresses fixed while the code moves.

### The regression comparison's baseline now

Since `45608d629` `tests/performance/Makefile` includes the selected
revision's `compiler/runtime.mk`, so each side of `compute-regression` builds
its runtime by its own revision's rules. With alignment on `main` both sides
are aligned, and the layout's fixed cost is the same on both. The baseline
described [above](#hosted-ubuntu-2404-amd-epyc-9v74), an unaligned module
over an aligned runtime, was the baseline only of the runs of the change
that introduced alignment; the arm `UR` below rebuilds it to separate the two
explanations those runs left open.

### Hardware counters on the hosts

A probe (run 45622862240 at `0ee3d56b2`, 24 hosted `ubuntu-24.04` jobs and
one on the 14900K) asked each host for its counter flags, its `perf` event
sources, the kernel's own line and a `perf stat` of user-mode cycles,
branches and branch misses:

| processor | jobs | counters |
|---|---|---|
| EPYC 7763, 9V74, 9V45 | 8, 2, 2 | none: no counter flag, no `cpu` event source, the kernel reports "PMU not available due to virtualization, using software events only", and `cycles` and the raw AMD events are not supported |
| Xeon 6975P-C | 12 | cycles, instructions, branches, branch misses, `idq.dsb_uops`, `idq.mite_uops`, `baclears.any`, `br_misp_retired.all_branches` |
| i9-14900K | 1 | the same on `cpu_core` and `cpu_atom`, and `frontend_retired.dsb_miss` |

Branch-prediction and op-cache counts therefore cannot be read on the 9V74
or the 9V45 as the hosted pool provides them. The software clock still
samples there, which gives time per instruction address. The pool has
changed since the runs above: half of this probe's hosted jobs, and 15 of 24
in the run before it (45622347072), landed on the Xeon 6975P-C, which no
earlier run met, and none on the Xeon 8370C.

### The next runs

Question: on the 9V74 and the 9V45, is the aligned `records` slower than the
unaligned placements' median, is the cause the position of its inner loops in
their lines, the image's data or neither, and does a layout rule that applies
to every function alike remove the cost while keeping the times independent
of placement?

One compiler builds every arm, so the arms differ in layout alone: the
unaligned module is the emitted module with ` align 64` removed from each
definition, and the unaligned runtime is the same sources compiled without
`-falign-functions=64`. Every host builds with the LLVM major the gate pins,
which the job checks and refuses to run without, not the hosted image's
clang 18.1.3. The loops may therefore fall elsewhere than in the table above:
the table is recomputed from the new module objects and committed here before
the decisive runs, with the arms the loop reading names. The manifest holds
each module object's digest; hosts whose objects are identical share one
table, and objects that differ get a table each.

- `U`, `F`, `FR` as above, and `UR`, the unaligned module and
  `ordinary_values.ll` over the aligned C runtime.
- `O16`, `O32`, `O48`: `FR` with every function of the module starting that
  many bytes past a 64-byte boundary. They are diagnostic: one offset chosen
  for one kernel's loops is not a rule.
- A candidate rule is the whole layout its form in the compiler would give,
  since the driver and `compiler/runtime.mk` pass one list of host arguments
  to the module and to every runtime unit:
  - `L32`, `L64`: `FR` with the module, `ordinary_values.ll` and the C
    runtime all compiled with `-falign-loops=32` or `-falign-loops=64` as
    well.
  - `A32`: the module's and `ordinary_values.ll`'s functions aligned to 32
    bytes and the C runtime compiled with `-falign-functions=32`.
  - `P`: `FR` with each function of the emitted module started at the one of
    0, 16, 32 and 48 bytes past a 64-byte boundary that leaves the fewest of
    its innermost loops of at most 64 bytes across a line boundary, the
    smallest such offset on a tie. The offset comes from the function's own
    code, so nothing ahead of the function changes it, and no padding is
    executed. `ordinary_values.ll` and the C runtime keep the 64-byte start.
    The run refuses a `P` image in which a function's crossing loops are not
    the number its chosen offset predicted.
- Code placements `p0`, `m16`, `m32`, `m48`, `r16`, `r48`, and for the
  aligned arms `m1024` and `m2048`, which move aligned code by 16 and 32
  lines and so change its offset within a page.
- Data placements: `FR` at `p0` with 128, 1024 and 2048 bytes linked at the
  front of `.data`, which moves the writable static data (`.data` and
  `.bss`) and leaves the code and the read-only data where they are
  (`d128`, `d1024`, `d2048`), and `U` at `p0` with the filler that gives its
  `.data` and `.bss` `FR`'s addresses (`dFR`). The smallest pad was 64 bytes
  until the first sizing run's layout check found the writable data aligned
  to 128 bytes, which moved part of it by 128 under a 64-byte pad; every
  pad is now a multiple of that alignment.
- Every decisive run also samples `FR` at `p0` and the fastest placement of
  `U` by the software clock, reduced to time per 64-byte line.

Hosts: two EPYC 9V74 jobs, and one job each on an EPYC 9V45, an EPYC 7763, a
Xeon 6975P-C and the i9-14900K, at least 12 rounds each. Every one of them
runs `records` with every arm and placement and the other four kernels with
`U`, `FR` and the four candidate rules at the code placements, so each host
is caught once. "Every host" below means these six jobs. A dispatch that
lands more decisive jobs on a model than this asks for keeps them all: each
is reported, "every host" covers it, and a condition stated for both 9V74
jobs must hold on every 9V74 job that measured.

Criterion, with the per-round ratio and interval defined above:

- The cost is established at a width when `FR`'s interval against `U` lies
  wholly below 1 on both 9V74 jobs. Where it does not, no cost is established
  and nothing below is selected for that width.
- Where the cost is established, it is attributed to the module's alignment
  if `F`'s interval against `U` also lies wholly below 1 on both jobs and
  `UR`'s does not, to the runtime's if `UR`'s does and `F`'s does not, and is
  not separated otherwise.
- The loop reading compares, among `FR`, `O16`, `O32` and `O48`, the arm the
  recomputed table gives the fewest crossing loops in the width's hot
  function with the arm it gives the most. The width's hot function is the
  module function with the most software-clock samples of `FR` at `p0` at
  that width in the sizing run. Of arms that tie, the one whose functions
  start nearest the line's start is taken (`FR`, then `O16`, `O32`, `O48`),
  as `P` breaks its ties; where all four give the same count, the table
  predicts no difference and the reading is not made at that width. It
  holds at a width if the first is faster than the second by the gate's rule
  on both 9V74 jobs, and is rejected if that interval spans 1 or lies below
  it. It accounts for the cost if, besides, `FR` is not the first arm and
  the first arm's interval against `U` does not lie wholly below 1. It is
  read whether or not the cost is established: a present layout that shows
  no cost while the reading holds stands at a good offset under this
  toolchain and nothing more.
- The data reading holds on a host if a data placement moves `FR` against
  its `p0` by the gate's rule, or `U` at `p0` under `dFR` differs from `U` at
  `p0` by it, and is rejected otherwise. If it holds on a 9V74, the arms are
  measured again with every image of a kernel given the same data addresses
  before any difference between arms is read as an effect of code.
- The baseline reading is for the two explanations the regression runs
  [above](#hosted-ubuntu-2404-amd-epyc-9v74) left open. On the 9V45 and the
  7763, at widths 2 and 4 and at `p0`: it holds if `FR` is slower than `UR`
  by the gate's rule and not slower than `U` by it, which makes the aligned
  runtime under an unaligned module a faster layout than either, as those
  runs met it; it is rejected if `FR` is not slower than `UR` by the gate's
  rule; and if `FR` is slower than both, those failures were the aligned
  layout's own cost on that host. A rejection does not show that hosts of
  one model differ: that stays undetermined unless the two 9V74 jobs, the
  only pair of one model here, disagree by the gate's rule on one
  comparison. Nor does it reach October's images: their baseline was the
  merge base's module built by clang 18.1.3, and this arm is the head's
  module without its alignment, built by the pinned LLVM.
- A candidate rule passes when, on both 9V74 jobs and at every width, its
  interval against `U` does not lie wholly below 1; no placement moves it
  against its `p0` by the gate's rule in any conclusive cell of any kernel on
  any host (C2); and on no host is any kernel slower than under `FR` by the
  gate's rule. `FR` is read by the first two conditions as well.
- A rule that passes is then compared with the head as a compiler change on
  programs larger than the kernels, by
  [`compiler-compare.yml`](../../../.github/workflows/compiler-compare.yml)
  on the 14900K (the hash-map cells, Halo's interpreter and the wasm
  interpreter, under that instrument's own rule), and the change in the
  kernels' code size is reported. A workload it judges slower is reported
  with the rule. `P` has no form in the compiler until it is chosen, so its
  comparison follows the choice and precedes the decision's record. Such a
  form is a pass over generated code: it can reach the driver's plain build
  and its cached build, and cannot reach `--fragments` or `--full-lto`,
  where the linker generates the code, or a consumer of `--emit-llvm`. Firn
  is built with `--full-lto`.
- Every job on a host that counts branch misses and decoded micro-ops, the
  Xeon 6975P-C and the 14900K, also runs `records` under `perf stat` after
  its timed rounds: `FR` at `p0`, `U` at each code placement and each rule at
  `p0`, so that no second job is needed to reach the layouts a 9V74 names.
  Where the cost is established, the record reports the counts of `FR` at
  `p0`, of the placement of `U` fastest on the 9V74 and of each rule that
  passes.
- If no rule passes, the record states the cause as far as these arms locate
  it and the cost each rule left.

### The rebuilt harness

[`placement.sh`](placement.sh) builds every arm with one compiler,
[`layout.py`](layout.py) writes each arm's module and refuses a run whose
images do not have the layout their arm and placement claim, and
[`summarize.py`](summarize.py) applies the criterion above. Both tools carry a
self-test that the placement job runs before it builds anything. Each was
seen failing on one deliberately wrong expectation, the crossing count of a
loop table and the cost verdict of an interval wholly below 1, at
`bcb297c0e` (temporary-workflow runs 45632117676 and 45633343603: both
self-test steps failed), and passing once that one line was restored at
`fd08f4eaf` (run 45632117012).

### The sizing runs

The first sizing runs, at `f6143fb8e` on the 14900K (run 45681914464) and on
a hosted runner (run 45681910389), stopped at the layout check before any
timing, with the same 307 problems on both. None was a wrong layout. The
emitted module defines the scheduler's entry points (`wf__par_publish`,
`wf__floor_run` and the like) as weak functions that the C runtime's strong
ones replace, and `bridge.c` defines the coroutine helpers as weak functions
that `ordinary_values.ll` replaces; the check judged the copy in the image by
the claim of the object whose copy the linker dropped. It now attributes a
name to its one strong definition, or its only one, and leaves out a name it
cannot resolve. The one other problem was real: a 64-byte data pad moved
part of the writable data by 128 bytes, since some of it is aligned to 128,
and the smallest data pad is now 128 bytes. Both changes are at `7828cbaa7`.

What the two runs established before the check:

- The kernels' `FR` module code is the same on both hosts: the disassembly
  of each module object differs only in the object's path, so one loop
  table serves every host.
- `-falign-loops` takes effect on LLVM input under the pinned LLVM: `L32`
  starts the hot loops of `records` at 0 and 32 within their lines, and
  `L64` at 0.
- `O16`, `O32` and `O48` start every function of the module, the hot ones
  included, 16, 32 and 48 bytes past a line.

The loop table of `records` under the pinned LLVM, read from the `FR` module
object: each innermost loop of at most 64 bytes, as its offset in the
function, its length and its place in its line under `FR`, and how many of
the function's loops cross a line when the function starts 0, 16, 32 or 48
bytes past one, which is where `FR`, `O16`, `O32` and `O48` start it.

| function | loops | crossing at 0, 16, 32, 48 |
|---|---|---|
| `wf__par_seq_summarize_records` | 0xb0, 64 bytes, at 48; 0x180, 21, at 0 | 1, 0, 1, 2 |
| `wf__par_chunk_summarize_records.1` | 0x30, 58, at 48; 0x100, 21, at 0 | 1, 0, 1, 2 |
| `wf__par_seq__par_chunk_summarize_records.1` | as the line above | 1, 0, 1, 2 |
| `wf__par_split_summarize_records.0` | 0x340, 21, at 0 | 0, 0, 0, 1 |
| `wf_record_summary` | 0xb0, 21, at 48; 0x124, 31, at 36 | 2, 1, 0, 0 |
| `wf_validate_record` | 0xc0, 21, at 0; 0x135, 32, at 53 | 1, 0, 0, 2 |
| `wf_box_array_filled$instance$e194368921a477a7` | 0x70, 21, at 48; 0x8c, 13, at 12 | 1, 0, 0, 1 |
| `wf__floor_record_latch` | 0x60, 4, at 32; 0x80, 31, at 0; 0x8a, 36, at 10 | 0, 0, 1, 2 |

This differs from the October table: the multi-byte loop is 64 bytes long,
not 54, the chunk's body is also a function of its own, the split
function's copy of that loop is 69 bytes long and so outside the table, and
`FR` leaves one of the two loops across a line, not both. The hot function
at each width is read from the rerun's software-clock samples, by the rule
above. By the table, a width whose hot function is the sequential or the
chunk function compares `O16`, with no crossing loop, against `O48`, with
two; a width whose hot function is the split function compares `FR` against
`O48`.

The rerun at `4fbed9570`, which holds `7828cbaa7` and main's later runtime
change, on a hosted runner (run 45731244532) landed on a Xeon 6975P-C and
passed the layout check with all 277 images. Its build, check and
verification took 80 seconds and each round about 130, so the decisive runs
take 20 rounds, the 6th and 15th ratios bounding the interval, inside the
stage's limit of 6,000 seconds on a host half again as slow. Its loop table
of `records` is the table above, line for line. It counted cycles,
instructions, branches, branch misses, `idq.dsb_uops`, `idq.mite_uops` and
`baclears.any`. It gave no software-clock sample: the harness asked `perf
script` for the symbol and DSO fields without `ip`, discarded its messages,
and kept no line that named the image. The harness now asks for
`ip,sym,symoff,dso`, keeps `perf script`'s messages, and stops the job when a
profile holds no sample of its image; the hot functions are read from a
rerun with that fix. The run's job on the 14900K (45731248572)
stopped before building, as another work's measurement held the host's check
lock. Exploratory, from three rounds and without an interval: on this Xeon
`O16`, which starts the sequential and chunk functions 16 bytes past a line,
where the table leaves them no crossing loop, was slower than `U` in every
round at every width, by 4 to 13 percent, and `P`, which starts those two
functions at 16 as well, by about 5 percent at width 1 only.

The rerun with the fix, at `83aed4d2f` on a hosted Xeon 6975P-C (run
45734506357), sampled 11,000 to 23,000 times per image. The module function
with the most samples of `FR` at `p0` is `wf__par_seq_summarize_records` at
width 1 (72 percent of all samples) and `wf__par_split_summarize_records.0`
at widths 2 and 4 (72 and 75 percent); `wf__par_chunk_summarize_records.1`
drew none, and the split function holds the same loops as the sequential
function, at other offsets. The rerun's job on the 14900K (run
45734510057) gives the same hot functions, 70 to 73 percent of its samples,
and the same loop table; its rounds took 140 to 170 seconds. By the table
and the rule above the loop reading therefore compares `O16` with `O48` at
width 1 and `FR` with `O48` at widths 2 and 4, where `FR` is the arm of
fewest crossing loops and so cannot account for a cost. The decisive runs are dispatched with
`placement_loop_arms` set to `1=O16:O48 2=FR:O48 4=FR:O48`.

The samples also bound what the loop reading can say. In both hot
functions, in `FR` at `p0` and in the placement of `U` the run found
fastest, 95 to 98 percent of the function's samples fall in the lines
spanned by one loop of about 220 bytes (offsets 0x120 to 0x1fe of the
sequential function, 0x2e0 to 0x3ba of the split function), which the
table, counting innermost loops of at most 64 bytes, leaves out; the
21-byte loop lies inside it. The lines of the function's other short loop,
64 bytes at 0xb0 of the sequential function and, too long for the table, 69
bytes at 0x270 of the split function, drew at most 2.1 percent. The
reading is made as fixed above; a rejection would not show that the
position of this larger loop is without effect.

The replay of PR #251's runtime change could no longer run: `c2e5a1808`'s
runtime does not define `wf__heap_give`, `wf__heap_take` or
`wf__empty_window`, which this compiler's modules call, so its images did
not link. An empty `placement_event` given to a dispatch was replaced by the
input's default in run 45731244532, so every placement job that measured from
then on ended failed at that step, after every reading and before the
artifact's upload, which ran regardless. The step and its input were removed
from the placement job after the decisive runs.

### The decisive runs

A temporary workflow dispatched them on the engine's push of `f2a1a495f`,
which adds only that workflow to `542cd620a`, the commit recording the loop
arms: compute-bench run 45737803637, 48 hosted jobs of which those on an
EPYC 9V74 or 9V45 measure, run 45737806327, four hosted jobs for an EPYC
7763 or a Xeon 6975P-C, and run 45737811705 on the 14900K, each with 20
rounds and `placement_loop_arms` as above. Fifteen jobs measured: seven on
a 9V74 (jobs 2, 8, 9, 30, 40, 44 and 48 of the first run), three on a 9V45
(4, 21, 43), one on a 7763, three on a Xeon 6975P-C and the 14900K. Every
one passed the layout check with 277 images and ended failed only at the
replay step. The null moved no cell on a hosted job and one on the 14900K.

`records`, `FR` against `U` (per round, `U`'s median over its code
placements divided by `FR`'s; below 1 the aligned images are slower):

| run, job | processor | width 1 | width 2 | width 4 |
|---|---|---|---|---|
| 45737803637, 2 | EPYC 9V74 | 0.900 (0.892 to 0.916) | 0.981 (0.970 to 0.997) | 0.959 (0.952 to 0.964) |
| 45737803637, 8 | EPYC 9V74 | 0.905 (0.877 to 0.913) | 0.983 (0.970 to 0.994) | 0.963 (0.947 to 0.973) |
| 45737803637, 9 | EPYC 9V74 | 0.887 (0.861 to 0.910) | 0.988 (0.973 to 0.992) | 0.964 (0.959 to 0.971) |
| 45737803637, 30 | EPYC 9V74 | 0.891 (0.859 to 0.904) | 0.980 (0.967 to 0.994) | 0.968 (0.960 to 0.973) |
| 45737803637, 40 | EPYC 9V74 | 0.898 (0.882 to 0.906) | 0.988 (0.978 to 0.995) | 0.967 (0.960 to 0.973) |
| 45737803637, 44 | EPYC 9V74 | 0.896 (0.867 to 0.910) | 0.985 (0.972 to 0.996) | 0.957 (0.951 to 0.966) |
| 45737803637, 48 | EPYC 9V74 | 0.892 (0.884 to 0.911) | 0.986 (0.973 to 0.993) | 0.955 (0.944 to 0.964) |
| 45737803637, 4 | EPYC 9V45 | 1.016 (1.004 to 1.026) | 1.017 (1.006 to 1.031) | 0.997 (0.990 to 1.012) |
| 45737803637, 21 | EPYC 9V45 | 1.013 (0.993 to 1.028) | 1.010 (0.998 to 1.026) | 1.015 (1.007 to 1.039) |
| 45737803637, 43 | EPYC 9V45 | 1.007 (1.000 to 1.014) | 1.020 (0.998 to 1.040) | 1.020 (1.010 to 1.027) |
| 45737806327, 2 | EPYC 7763 | 0.999 (0.997 to 1.002) | 1.002 (0.998 to 1.008) | 1.000 (0.998 to 1.002) |
| 45737806327, 1 | Xeon 6975P-C | 0.993 (0.990 to 0.994) | 1.001 (0.996 to 1.003) | 1.008 (1.003 to 1.009) |
| 45737806327, 3 | Xeon 6975P-C | 0.995 (0.992 to 0.995) | 1.001 (0.998 to 1.008) | 1.005 (0.999 to 1.012) |
| 45737806327, 4 | Xeon 6975P-C | 0.993 (0.993 to 0.996) | 1.002 (0.999 to 1.006) | 1.006 (0.996 to 1.016) |
| 45737811705, 1 | i9-14900K | 1.001 (1.001 to 1.002) | 1.000 (0.998 to 1.000) | 0.995 (0.976 to 1.003) |

Each arm against `U` the same way, the median over a model's jobs:

| processor (jobs) | width | `F` | `UR` | `O16` | `O32` | `O48` | `L32` | `L64` | `A32` | `P` |
|---|---|---|---|---|---|---|---|---|---|---|
| EPYC 9V74 (7) | 1 | 0.898 | 0.924 | 0.969 | 1.043 | 0.907 | 1.040 | 0.972 | 1.039 | 0.968 |
| EPYC 9V74 (7) | 2 | 0.981 | 1.080 | 1.053 | 1.128 | 0.997 | 1.132 | 1.075 | 1.122 | 0.990 |
| EPYC 9V74 (7) | 4 | 0.962 | 1.102 | 1.059 | 1.170 | 1.032 | 1.174 | 1.110 | 1.162 | 0.959 |
| EPYC 9V45 (3) | 1 | 1.015 | 1.000 | 0.927 | 1.015 | 0.965 | 1.008 | 1.011 | 1.012 | 0.927 |
| EPYC 9V45 (3) | 2 | 1.018 | 1.013 | 0.935 | 1.041 | 0.968 | 1.036 | 1.030 | 1.033 | 1.014 |
| EPYC 9V45 (3) | 4 | 1.015 | 1.023 | 0.907 | 1.049 | 0.996 | 1.045 | 1.041 | 1.042 | 1.013 |
| EPYC 7763 (1) | 1 | 1.000 | 0.999 | 0.934 | 1.003 | 1.001 | 1.002 | 1.001 | 1.002 | 0.935 |
| EPYC 7763 (1) | 2 | 1.001 | 0.998 | 0.937 | 1.004 | 1.005 | 1.001 | 1.001 | 1.002 | 1.001 |
| EPYC 7763 (1) | 4 | 0.999 | 1.061 | 0.915 | 1.087 | 1.047 | 1.094 | 1.063 | 1.085 | 1.000 |
| Xeon 6975P-C (3) | 1 | 0.994 | 0.993 | 0.948 | 1.008 | 0.989 | 1.008 | 1.003 | 1.005 | 0.949 |
| Xeon 6975P-C (3) | 2 | 1.002 | 1.003 | 0.951 | 1.009 | 1.004 | 1.011 | 1.003 | 1.007 | 1.001 |
| Xeon 6975P-C (3) | 4 | 1.006 | 1.016 | 0.884 | 1.033 | 0.973 | 1.030 | 1.019 | 1.029 | 1.008 |
| i9-14900K (1) | 1 | 1.001 | 1.000 | 0.947 | 1.002 | 1.000 | 1.001 | 1.000 | 1.001 | 0.949 |
| i9-14900K (1) | 2 | 1.000 | 1.001 | 0.999 | 1.002 | 1.002 | 1.002 | 1.002 | 1.002 | 1.000 |
| i9-14900K (1) | 4 | 0.999 | 1.002 | 1.000 | 1.001 | 1.002 | 1.005 | 1.001 | 1.001 | 1.002 |

The readings, by the criterion above:

- **Cost: established** at widths 1, 2 and 4: `FR`'s interval lies wholly
  below 1 on all seven 9V74 jobs.
- **Attribution:** at width 1 **not separated**, since `F` and `UR` both lie
  wholly below 1 on all seven; at widths 2 and 4 **the module's
  alignment**, `F` wholly below 1 and `UR` wholly above it on all seven.
- **Loop reading:** at width 1 it **holds**, `O16` faster than `O48` by the
  gate's rule on all seven (ratio 1.067 to 1.068), and **does not account
  for the cost**, since `O16` itself lies wholly below `U` (0.967 to 0.971).
  At widths 2 and 4 it is **rejected**: `FR`, the arm of fewest crossing
  loops, is slower than `O48` (0.984 to 0.991 at width 2, 0.929 to 0.937 at
  width 4, by the gate's rule at width 4). In the table `FR` leaves one of
  the sequential function's two loops across a line and none of the split
  function's.
- **Data reading: rejected** on every job: no data placement moved `FR`,
  and `dFR` did not move `U`.
- **Baseline reading.** On the 9V45, **rejected** at width 2 on all three
  jobs, and at width 4 rejected on jobs 21 and 43 and **holds** on job 4; on
  the 7763, rejected at width 2 and holds at width 4. The seven 9V74 jobs
  agree on each comparison of the reading (`FR` slower than `UR` and not
  than `U` at both widths, on every one); the three 9V45 jobs disagree at
  width 4, where `FR` against `UR` is 0.966 on job 4 and 0.981 and 0.988 on
  the other two, about the gate's 0.97. `UR` is not `U`'s layout with an
  aligned runtime alone: its module lies 0x120 bytes later in the image than
  `U`'s (at `p0`, `wf__par_seq_summarize_records` at 0x4740 against 0x4620),
  which starts the split function 32 bytes past a line.
- **Rules: none passes.**
  - `FR`: the cost, and C2 at `m2048`: at width 4 on every 9V74 job,
    faster than at `p0` by 12 to 13 percent, and at width 2 on six of them,
    by 3.6 to 7.7 percent; at width 4 on every 9V45 job, slower by 9 to 10
    percent, and at width 2 on two of them, by 4 to 5 percent; and `fir` at
    width 2 on 9V45 job 21.
  - `L32`: no cost on any 9V74 job at any width, slower than `FR` nowhere,
    and C2 fails at `m2048` on the three 9V45 jobs at width 4 (0.899 to
    0.932); it moved nothing else.
  - `L64`: the cost at width 1 on all seven 9V74 jobs (0.968 to 0.974); C2
    holds and it is slower than `FR` nowhere.
  - `A32`: no cost, and C2 fails on every 9V74 job, where `m32` and `m48`
    make `records` 11 to 18 percent slower, and at `m32` or `m48` on every
    other hosted job.
  - `P`: the cost, C2 as `FR`'s and at `m1024` at width 1 on the 9V45, and
    slower than `FR` at width 1 on every model but the 9V74 (by 4 to 9
    percent), where it starts the sequential function 16 bytes past a line.

What the arms locate. With the function's code unchanged, `records`' time
on the 9V74 follows where its hot function starts within a 64-byte line,
in one order at every width: median cells, in ms, of the seven jobs at
widths 1, 2 and 4:

| hot function's start past a line | arm at that start | width 1 | width 2 | width 4 |
|---|---|---|---|---|
| 0 | `FR` | 17.56 | 8.80 | 9.00 |
| 16 | `O16` | 16.21 | 8.20 | 8.16 |
| 32 | `O32` | 15.05 | 7.64 | 7.36 |
| 48 | `O48` | 17.28 | 8.68 | 8.34 |

Every image whose hot functions have `FR`'s code, which is every arm but
`L32` and `L64`, lies within 0.12 ms of the row of its hot function's start
at every width, by `nm`, except `FR` at `m1024` at width 1 and `FR` and `P`
at `m2048` at widths 2 and 4. At `p0` `U` starts the sequential function at
32 and the split function at 0, at `m16` at 48 and 16, at `m32` at 0 and
32, at `m48` at 16 and 48, so `U`'s median over its placements mixes these
rows, while `F` and `FR` take the slowest. The crossing count of the table does
not order them: 16 leaves no loop of the hot functions across a line and is
second, 32 leaves one of the sequential function's and is first. Every
layout measured that puts the 21-byte loop 32 bytes past its line, `O32`,
`L32` (where `-falign-loops=32` puts it there with the function at 0),
`A32` at `p0` and the fastest placements of `U`, is in the fastest row;
`L64`, which puts it at 0 with every loop aligned to 64, lies between. The
data's place has no part in it, as the data reading shows. With that code
and the sequential function at 0, the 9V74's time at width 1 also varies
from process to process of one image: the interquartile range of the 20
process times is 7.5 to 12.8 percent of their median for every such image
(`F`, `FR`, `UR` and `A32` at their placements that start it there, and
`U` at `m32`), the processes falling on two or three levels (16.4, 17.5 and
18.9 ms for `FR` at `p0` on job 2); it is at most 1.4 percent for every
image that starts the function elsewhere, 1.0 to 1.2 percent for `L32`, and
1.3 to 6.1 percent for `L64`. What differs between processes
of one image is not measured here; the address-space layout's
randomization is one candidate.

A second dependence appears under a move of 2048 bytes, not of 1024. At
widths 2 and 4 it changes `FR` and `P` on the 9V74, faster (`FR` 8.80 to
8.23 and 9.00 to 7.96 ms, `P` 8.81 to 8.24 and 8.97 to 7.94), and `FR`,
`P`, `L32` and `O16` on the 9V45, slower (`FR` 5.50 to 5.80 and 5.14 to
5.71 ms, `L32` 5.45 to 5.58 and 4.97 to 5.51). It leaves `F`, the same
module as `FR` over the unaligned runtime, unchanged on both models, as it
leaves `O32`, `O48`, `A32` and `L64` there and every rule on the 7763, the
Xeon and the 14900K. Such a move changes no distance within the code it
moves, only that code's addresses and its distance from what the linker
puts before the module, so the arms leave this dependence unlocated. The
shifts of the earlier runs and of the regression check's placement control
were at most 96 bytes, so neither could show it: on these two models the
aligned layout's time does not hold under a move of 2048 bytes.

Software-clock samples of the hot function on the 9V74 (job 2 of run
45737803637), 20 processes per image, per 64-byte line as its offset from
the function's start; `FR` at `p0` starts the function on a line, the
fastest placement of `U` 32 bytes past one:

| width | image | samples of the image, of the function | lines with the most samples |
|---|---|---|---|
| 1 | `FR` `p0` | 14,465, 10,598 | +0x100 4,388; +0x140 2,955; +0x1c0 1,771; +0x180 1,039 |
| 1 | `U` `r16` | 13,019, 9,161 | +0x120 5,562; +0x1a0 1,423; +0x160 1,063; +0x1e0 885 |
| 2 | `FR` `p0` | 16,257, 11,866 | +0x2c0 5,025; +0x300 2,865; +0x380 2,265; +0x340 1,345 |
| 2 | `U` `m32` | 14,064, 9,793 | +0x2e0 5,979; +0x360 1,558; +0x320 1,090; +0x3a0 881 |
| 4 | `FR` `p0` | 27,581, 21,694 | +0x2c0 11,588; +0x380 4,666; +0x340 2,780; +0x300 2,087 |
| 4 | `U` `m32` | 23,386, 17,704 | +0x2e0 11,538; +0x360 2,803; +0x3a0 1,806; +0x320 1,231 |

In both layouts the time lies in the lines of the 220-byte loop, and the
aligned image draws more samples there at each width. The hosted AMD
machines expose no counter ([above](#hardware-counters-on-the-hosts)), so
the mechanism is not read on them. The same two layouts on the hosts that
count, over ten processes per image: branch misses per branch, the share
of micro-ops from the decoded micro-op cache, and micro-ops from the legacy
decoders (the three Xeon jobs as a range):

| host | width | `FR` `p0` | `U` at the 9V74's fastest (`r16` at width 1, `m32` at 2 and 4) |
|---|---|---|---|
| Xeon 6975P-C | 1 | 0.265 to 0.266 %; 95.1 to 97.0 %; 187 to 275 M | 0.268 %; 98.1 to 98.5 %; 76 to 103 M |
| Xeon 6975P-C | 2 | 0.266 to 0.268 %; 96.8 to 97.4 %; 196 to 286 M | 0.270 to 0.271 %; 97.7 to 98.5 %; 118 to 153 M |
| Xeon 6975P-C | 4 | 0.272 to 0.273 %; 98.6 to 98.8 %; 209 to 265 M | 0.280 to 0.281 %; 98.9 to 99.2 %; 134 to 173 M |
| i9-14900K | 1 | 0.268 %; 98.9 %; 63 M | 0.295 %; 96.2 %; 219 M |
| i9-14900K | 2 | 0.269 %; 96.6 %; 179 M | 0.273 %; 96.4 %; 202 M |
| i9-14900K | 4 | 0.272 %; 96.8 %; 182 M | 0.274 %; 97.2 %; 178 M |

On the Xeon the aligned image takes 1.2 to 3.6 times the micro-ops from
the legacy decoders that `U` takes, without being slower (`FR` against `U`
0.993 to 1.008),
and on neither model does the branch-miss rate differ by more than 0.03
points between the two layouts; these counts do not name what makes the
9V74 slower.

Limits. Seven 9V74 jobs of one dispatch, which may share physical
machines; 20 rounds each. The images are this compiler's under LLVM 22, not
October's. Only `records` was analyzed by line; the four other kernels
enter through C2 and the comparison with `FR`. The 14900K runs other works'
checks beside a placement job, since they take their own locks.

### The outcome

- **The cost.** By the criterion, which is read on the 9V74 jobs, the
  aligned layout's cost to `records` is established on the EPYC 9V74 alone,
  at widths 1, 2 and 4: `FR` against `U` is 0.887 to 0.905 at width 1, 0.980
  to 0.988 at width 2 and 0.955 to 0.968 at width 4 on the seven jobs, every
  interval wholly below 1, so the aligned `records` takes 10.5 to 12.7, 1.2
  to 2.0 and 3.3 to 4.7 percent longer. On the other models no interval lies
  wholly below 1 on the 9V45, the 7763 and the 14900K; on the three Xeon
  6975P-C jobs the interval at width 1 lies wholly below 1, by 0.5 to 0.7
  percent, and none at widths 2 and 4.
- **The cause, as far as the arms locate it.** With the hot function's code
  unchanged, the 9V74's time follows where that function starts within its
  64-byte line, in one order at every width: fastest at 32, then 16 and 48,
  slowest at 0, where the present layout fixes it. The loop reading holds at
  width 1 and does not account for the cost there, since `O16` is itself
  wholly slower than `U`. It is rejected at widths 2 and 4: there the hot
  function is the split function, the table leaves none of its short loops
  across a line under `FR`, which makes `FR` the arm of fewest crossing loops,
  and `FR` is still slower than `O48`. Under `FR` the table leaves one of the
  sequential function's two short loops across a line. The data reading is
  rejected on every job. The hosted AMD machines expose no counter, and the
  counts on the Xeon 6975P-C and the 14900K do not name a mechanism.
- **What the arms leave unlocated.** The present layout's time on `records`
  changes under a 2048-byte move at widths 2 and 4: on the 9V74 it is faster
  than at `p0`, by 12 to 13 percent at width 4 on every job and 3.6 to 7.7
  percent at width 2 on six, and on the 9V45 slower, by 9 to 10 percent at
  width 4 on every job and 4 to 5 percent at width 2 on two. At width 1 on
  the 9V74, with the sequential function at 0, the time also varies between
  processes of one image, the interquartile range of the process times being
  7.5 to 12.8 percent of their median.
- **The rules.** None passed. `L32` failed C2 alone, at `m2048` at width 4 on
  the three 9V45 jobs; `L64` failed the cost alone, at width 1 on the seven
  9V74 jobs; `A32` and `P` failed more, as listed above. Neither `L32` nor
  `L64` was compared on a program larger than the kernels, a step the
  criterion reserves for a rule that passes. Either would have been a host
  argument, `-falign-loops=32` or `-falign-loops=64`, at each build entry
  that compiles the module or the runtime; by clang 22's driver sources that
  argument reaches the compile step only, so a link that generates code would
  also need the code generator's own `-align-loops` passed to the linker.
  That was read in the sources and not confirmed in a disassembly.
- **The choice.** The owner was asked to choose among adopting `L32`,
  adopting `L64`, accepting the present layout and measuring once more before
  choosing, and accepted the present layout. The compiler is unchanged, and
  the two rules not taken are recorded under the decisions the introduction
  links. The regression comparison builds both of its sides with this layout
  ([above](#the-regression-comparisons-baseline-now)), so the 9V74's cost is
  the same on both; the two dependences above are not removed by that.
