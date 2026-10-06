# Code placement and function alignment

A change that leaves a compute kernel's generated code byte-identical can
still make the kernel measurably slower, because an unrelated change ahead of
it in the image moves its instructions. This investigation asks whether
starting every emitted and runtime function on a 64-byte boundary makes the
five compute kernels' times independent of where they land, what it costs,
and whether loop-header alignment adds anything. Its decision is in
[design/compiler/code-alignment.md](../../../design/compiler/code-alignment.md).

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

[`placement.sh`](placement.sh) builds two trees with their own compilers and
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
The workflow's replay step builds the merge base's runtime (`c2e5a180`, its
`ordinary_values.ll` given the alignment its successors carry) with this
revision's compiler and flags, and compares it with the `FR` images.

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
unaligned, a layout no arm of the experiment built
([TODO](../../../docs/todo.md#verification-tooling)). The five runs below had
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
- Apple's arm64 cores use 128-byte lines. The macOS runner resolves only
  about 20 percent ([compute-runtime results](../compute-runtime/RESULTS.md)),
  so nothing here is measured on arm64.
- The host C objects of the kernel's oracle and the timing runner are not
  aligned; they lie outside the timed interval except for the calls into the
  kernel.
