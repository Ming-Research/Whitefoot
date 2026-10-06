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
alignment beyond that noise. Alignment adds 48 bytes of never-executed padding
per function on average: 576 to 1,280 bytes per kernel module.

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
is timed as its own image. Every image is verified at widths 1, 2 and 4, then
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
- **C1, discrimination.** On a host where no kernel and width of `U` spreads
  by 10 percent or more across its six placements, the host cannot
  discriminate and its readings select nothing.
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

## Results

Pending the runs.

## Limitations

- A 64-byte boundary fixes where instructions fall within their line. Effects
  keyed on higher address bits, such as branch-predictor aliasing between
  functions or 4 KiB aliasing, can remain; the `m` and `r` placements measure
  what remains only for shifts below 64 bytes.
- Apple's arm64 cores use 128-byte lines. The macOS runner resolves only
  about 20 percent ([compute-runtime results](../compute-runtime/RESULTS.md)),
  so nothing here is measured on arm64.
- The host C objects of the kernel's oracle and the timing runner are not
  aligned; they lie outside the timed interval except for the calls into the
  kernel.
