# E0: interpreter dispatch shapes compiled by LLVM

The question, the candidate lowerings and the criteria, fixed before these
measurements, are in the
[match-dispatch investigation](../../investigations/match-dispatch/DESIGN.md#e0-lowering-shapes-in-c).
This bundle holds one register-machine interpreter in C (`vm.c`) whose
handler semantics are written once and compiled under each dispatch shape and
operand-access form (`build.sh`), and the interleaved runner (`run.py`).

```sh
research/experiments/match-dispatch/build.sh <scratch-root>/e0
python3 research/experiments/match-dispatch/run.py <scratch-root>/e0 --rounds 10 --tsv raw.tsv
python3 research/experiments/match-dispatch/run.py <scratch-root>/e0 --summarize raw.tsv
```

## Conditions

- Apple M1 Pro (8 cores), macOS 26.6.2, Apple clang 21.0.0
  (clang-2100.3.34.2), `-O2`.
- Run 1 (`run1.tsv`): `vm.c` of commit `e63346cdb`, five kernels (loop, fib,
  sieve, mandel, poly), access forms checked, u8 and raw, ten launches of
  every binary and kernel, interleaved in a shuffled order per round.
- Run 2 (`run2.tsv`): `vm.c` of the commit adding this file, which adds the
  `floor` kernel and the `u8v` access form; five launches each.
- Cycles and instructions retired per process from `/usr/bin/time -l`,
  divided by the kernel's dispatch count from the counting build. All
  variants of a kernel return the same checksum; the runner stops on a
  mismatch.
- Names: `switch` a switch in a loop; `goto` computed goto; `tail` one
  function per opcode with `musttail` through a handler table under the
  default C convention; `tailpn` the same under `preserve_none`; `cell` and
  `cellpn` the handler's offset from a base handler stored in each cell
  instead of the table (option B). Access forms: `checked` compares the frame
  index with the frame length and the fetch index with the code length; `u8`
  reads 8-bit operands with 256 slots of frame headroom and still compares
  the fetch index; `u8v` is `u8` without the fetch comparison, as if verified
  bytecode facts reached the loop; `raw` uses cell and frame pointers with no
  check, which Whitefoot cannot express.

## Results

Geometric mean over kernels of median cycles per dispatch:

| shape  | checked (1) | u8 (1) | raw (1) | checked (2) | u8 (2) | u8v (2) | raw (2) |
|--------|------------:|-------:|--------:|------------:|-------:|--------:|--------:|
| switch | 4.995 | 5.314 | 4.851 | 4.765 | 5.274 | 5.534 | 4.706 |
| goto   | 5.027 | 4.864 | 3.697 | 4.660 | 4.540 | 3.946 | 3.249 |
| tail   | 6.089 | 4.504 | 3.783 | 6.040 | 4.367 | 3.597 | 3.375 |
| tailpn | 4.254 | 4.180 | 3.781 | 3.875 | 3.727 | 3.644 | 3.342 |
| cell   | 6.106 | 4.371 | 3.683 | 6.033 | 4.273 | 3.663 | 3.176 |
| cellpn | 4.189 | 4.114 | 3.687 | 3.855 | 3.717 | 3.518 | 3.153 |

(1) run 1, five kernels; (2) run 2, six kernels including `floor`.

The dispatch floor (`floor`, run 2), where no value passes between
dispatches through the frame: `cellpn-raw` 1.480, `tailpn-raw` 1.838,
`goto-raw` 1.829, `switch-raw` 3.894; `tailpn-checked` 2.428 against
`switch-checked` 3.977.

Null comparison: `tailpn-checked` with 64 or 2,048 bytes of unreachable code
before its handlers measured 4.212 and 4.214 against 4.254 in run 1 (1.0%),
and 3.840 and 3.823 against 3.875 in run 2 (0.9 to 1.3%). The selection rule's
threshold is therefore its floor of 2%. Per-cell spreads are in the summaries
`run.py --summarize` prints; most are under 2%, with occasional single-launch
outliers that the median absorbs.

## Verdicts under the recorded criteria

- **Shape selection.** In the forms Whitefoot can express (checked, u8),
  `tailpn` and `cellpn` are the lowest, 15% (checked) and 21% (u8) below
  `switch` in run 1, and `goto` is no better than `switch` there. The
  falsifier did not fire: per-arm functions with `musttail` beat the switch
  well beyond the threshold.
- **Tag to handler (option B).** Run 1, the recorded five-kernel test: `cellpn`
  beats `tailpn` by 1.5% (checked), 1.6% (u8) and 2.5% (raw), short of the
  3% the criterion requires, so B does not go to a decision card on this
  evidence. Run 2's `floor` kernel, added after the criterion, shows the
  mechanism the criterion could not see: with no frame round trip to hide
  behind, B removes 0.36 cycles of a 1.84-cycle raw dispatch (20%), and the
  six-kernel raw geomean then differs by 5.7%.
- **Calling convention.** The default convention matches `preserve_none` when
  the handler's parameters fit the eight arm64 argument registers (raw, seven
  parameters: 3.783 against 3.781), costs 8% with nine (u8) and 43% with ten
  (checked: 6.089 against 4.254), where it is worse than `switch`. The cost is
  the parameters passed on the stack.
- **Operand checks**, for the Halo investigation's G4 (run 2, `tailpn`):
  checking every frame index against the frame length costs 4.0% over `u8`
  (3.875 against 3.727); the fetch comparison costs 2.3% over `u8v` (3.727
  against 3.644); and `u8v` remains 9.0% above `raw` (3.644 against 3.342),
  which is the index representation (`code[pc]`, `regs[base + a]`) rather
  than any check.

## What the results say about the lowering

- The win of per-arm functions is register allocation, not the indirect
  branch: `goto`, which replicates the dispatch inside one function, matches
  `tail` only in the raw form, where little state is live; with the u8
  form's extra live values it recovers less than half of `tailpn`'s gain,
  and with the checked form's it falls back to `switch`. `switch` itself varies
  between forms in ways the work does not explain (`switch-u8v` is slower
  than `switch-u8`), which is the one-function form's sensitivity to the
  allocator over a large body.
- The register budget is the owner's spill-block direction made
  quantitative: a lowering that passes more state than the convention's
  argument registers pays for it on every dispatch, so the spill block must
  take the excess rather than the stack argument area.
- The index-to-pointer gap (9%) is a compiler opportunity rather than a
  language gap: when every use of a loop-carried index is an address into
  one array and offsets from itself, the handler chain could carry the
  derived address instead. It needs its own design and evidence.
- LLVM rematerializes the handler table's address with two instructions in
  every handler; option B computes the target from a base the same way. A
  base held as a parameter would remove both.

## The same work on Silverfir-nano's interpreter

`wasm/kernels.c` restates four of the kernels in C, step for step with
`vm.c`'s bytecode; built with wasi-sdk 34.0 at `-O2` it prints the same
checksums on Silverfir-nano's interpreter as every `vm.c` variant, and the
runner stops on a mismatch. `poly` is left out because its body, compiled
from C, folds to constants (1,560 dispatches instead of 154 million), and
`floor` because compiled C removes its stores. `wasm/compare.py` interleaves
Silverfir-nano (commit `5f248e44` of its main branch, release build, `--interp`)
and six `vm.c` variants of run 2 on the same M1 Pro, ten launches each
(`run-nano.tsv`); Silverfir-nano's start-up, measured as a run that does
nothing (10.9 million cycles), is subtracted from its kernels. Dispatch counts
come from a separate Silverfir-nano build with its `interp-count` feature.

| | loop | fib | sieve | mandel |
|---|---:|---:|---:|---:|
| Silverfir-nano dispatches | 840.0M | 158.5M | 541.1M | 92.0M |
| `vm.c` dispatches | 1000.0M | 149.3M | 470.1M | 87.6M |
| Silverfir-nano, cycles per dispatch | 2.044 | 2.251 | 2.169 | 2.056 |
| `tailpn-u8`, cycles per dispatch | 4.002 | 3.887 | 4.483 | 3.728 |
| `tailpn-u8`, cycles against Silverfir-nano | 2.33x | 1.63x | 1.80x | 1.73x |
| `cellpn-u8` against Silverfir-nano | 2.32x | 1.52x | 1.78x | 1.76x |
| `tailpn-u8v` against Silverfir-nano | 2.33x | 1.59x | 1.78x | 1.70x |
| `cellpn-raw` against Silverfir-nano | 2.30x | 1.25x | 1.59x | 1.74x |
| `switch-u8` against Silverfir-nano | 2.95x | 2.15x | 1.96x | 2.66x |

The dispatch counts are within 20% of each other, so the gap is the cost of
a dispatch: about 2.0 to 2.25 cycles in Silverfir-nano on this core, close
to the 2.02 it records for CoreMark on an M4, against 3.6 to 4.5 here. Even
the unchecked pointer form stays 1.25 to 2.3 times slower. The E0
interpreter routes every operand through a frame slot, while Silverfir-nano
keeps a just-produced value in an accumulator register and its hottest
locals in registers; its own record attributes 29% and 15% to those two
mechanisms. The dispatch shape is therefore the smaller lever: the larger is
keeping interpreter values out of frame memory, which in Whitefoot is the
writer's choice of loop-carried state that the per-arm lowering then passes
in registers.

## E1: Silverfir-nano's register residency in the same interpreter

`vm1.c` is `vm.c` with Silverfir-nano's two residency mechanisms, chosen by
a link pass and selectable at run time: an accumulator (`acc`, and `facc`
for floats) that every producer sets and that the next instruction in the
same region and domain reads instead of the frame slot; two pinned locals
per function (`accpin`), the most-referenced slots of one domain, held in
registers with write-through and reloaded at calls and returns; and `full`,
which also leaves a slot unstored where liveness over slot reads shows it
dead, as Silverfir-nano leaves a stack temporary unstored. Handlers are
specialised by operand residency class, 64 variants per opcode. Every
variant and mode returns `vm.c`'s checksums. `-DHANDLER_BASE_PARAM` passes
the handler table's or base handler's address along the chain instead of
letting each handler rematerialise it. Same M1 Pro, same Silverfir-nano
build, interleaved: `run-e1.tsv` (five launches; its `full` rows predate a
liveness fix and repeat `accpin`, so they are not reported) and
`run-e1-base.tsv` (six launches).

Cycles against Silverfir-nano on the same work (loop, fib, sieve, mandel):

| variant | loop | fib | sieve | mandel |
|---|---:|---:|---:|---:|
| `tailpn-u8`, none | 2.33x | 1.91x | 1.89x | 1.67x |
| `tailpn-u8`, acc | 1.85x | 1.83x | 1.80x | 1.33x |
| `tailpn-u8`, accpin | 1.79x | 1.77x | 1.67x | 1.26x |
| `tailpn-u8`, full, base parameter | 1.68x | 1.75x | 1.60x | 1.28x |
| `tailpn-u8v`, full, base parameter | 1.58x | 1.73x | 1.54x | 1.25x |
| `cellpn-raw`, accpin | 1.38x | 1.38x | 1.36x | 1.13x |
| `cellpn-raw`, full, base parameter | 1.37x | 1.35x | 1.34x | 1.16x |

- The accumulator and the pinned locals take the WF-expressible `u8` form
  from 1.67-2.33x to 1.26-1.79x of Silverfir-nano; the accumulator gives
  most of it on `loop`, `fib` and `mandel`, the pinned locals most on `sieve`.
- Leaving dead slots unstored removes 8% of `loop`'s instructions and no
  cycles, consistent with Silverfir-nano's measured 0.023 cycles per store.
- Passing the handler base as a parameter removes the two instructions that
  rematerialise it in every handler (`adrp`, `add`): the raw form's `loop`
  handler falls to 6.8 instructions per dispatch against Silverfir-nano's
  6.2, yet its cycles stay at 2.34 per dispatch against 2.03. What remains
  is not instruction count; the visible differences are LLVM's pre-indexed
  writeback load of the next cell, which Silverfir-nano measured as
  serialising, and Silverfir-nano's load of the next handler at handler
  entry. `fib` also pays for a call protocol that saves and reloads pin
  words.
- The `u8` form stays 1.1-1.3x above the raw form with every mechanism on,
  of which the fetch comparison is 1-6% (`u8v`) and the rest is computing
  `code[pc]` and `regs[base + a]` from indices in every handler.

## Stage 2: the Whitefoot interpreter under the compiler's lowering

`wf/vm.wf` is `vm.c`'s interpreter in the `u8` form, written as guaranteed
self-tail transfers, one binary per kernel, each exiting 0 only when its
checksum equals `vm.c`'s; `WHITEFOOTC=<compiler> WF_NAME=<name> build.sh
<dir>` builds them. It is compiled by one compiler source twice: with
compiler/match-dispatch-lowering (`wfsplit`) and with
`FunctionEmitter::plan_dispatch` in
`compiler/src/backend/emitter/dispatch.rs` returning `Ok(None)` before it
looks for a loop (`wfwhole`). Same M1 Pro, eight interleaved launches
against Silverfir-nano and the C forms (`run-wf.tsv`); dispatch counts are
`vm.c`'s, since the bytecode is the same.

| | loop | fib | sieve | mandel |
|---|---:|---:|---:|---:|
| `wfwhole`, cycles | 5217M | 933M | 2848M | 432M |
| `wfsplit`, cycles | 4417M | 776M | 2564M | 408M |
| split against whole | -15.3% | -16.8% | -10.0% | -5.7% |
| `wfsplit` against C `tailpn-u8` | 1.11x | 1.35x | 1.23x | 1.26x |
| `wfsplit` against Silverfir-nano | 2.59x | 2.19x | 2.20x | 2.16x |
| `wfsplit`, instructions per dispatch | 23.2 | 25.1 | 24.1 | 23.6 |
| C `tailpn-u8`, instructions per dispatch | 17.8 | 18.3 | 18.1 | 17.9 |

The first build of the lowering, which left the `match` scrutinee's copy
in the shared frame, measured 12% slower than `wfwhole` on `loop` in a
single probe launch; the measurement above is the build with that slot
part-local, and no interleaved comparison isolates the slot alone. The
remaining distance to the C form is visible in the arm code: each dispatch
reloads `code`'s box pointer and length and each arm `regs`'s box pointer,
which the whole-function loop had hoisted; the cell is 24 bytes against
16; and the call and return arms push and pop frame records through the
library's window operations.

### Invariant loads hoisted (`wfhoist`)

The lowering then computes in the enclosing function what the loop cannot
change: `code`'s box referent and length (a read-only reference passed
through unchanged) and the referents of `regs`, `mem` and `frames`, whose
boxes the loop only projects, so no arm reloads a box pointer; the
references themselves leave the parts' parameters (nine remain). Same core,
eight interleaved launches (`run-wf-hoist.tsv`), with the whole-function
and first split builds remeasured beside it:

| | loop | fib | sieve | mandel |
|---|---:|---:|---:|---:|
| `wfwhole`, cycles | 5234M | 934M | 2855M | 434M |
| `wfsplit`, cycles | 4434M | 773M | 2498M | 409M |
| `wfhoist`, cycles | 4004M | 607M | 2378M | 341M |
| `wfhoist` against `wfwhole` | -23.5% | -35.0% | -16.7% | -21.4% |
| `wfhoist` against C `tailpn-u8` | 1.00x | 1.05x | 1.13x | 1.05x |
| `wfhoist` against Silverfir-nano | 2.34x | 1.71x | 2.03x | 1.81x |
| `wfhoist`, instructions per dispatch | 20.2 | 21.5 | 20.8 | 20.6 |

The WF interpreter now runs at the C tail-call form's speed on `loop` and
within 13% elsewhere. Its distance to Silverfir-nano is that of the C form
with the same design: E1 measured the accumulator and pinned locals, which
this interpreter does not have, as the larger lever.

## Argument registers

How many arguments each calling convention passes in registers, which
compiler/match-dispatch-lowering uses as its register budget: `regprobe.py`
compiles a function of n `i64` (or n `double`) parameters with clang
21.0.0 for each target and reports the largest n whose assembly reads no
argument from the stack.

| convention | target | integer | floating |
|---|---|---:|---:|
| `preserve_none` | aarch64 (Apple, Linux) | 24 | 8 |
| `preserve_none` | x86-64 (Linux, Windows) | 12 | 8 |
| C | aarch64 (Apple, Linux) | 8 | 8 |
| C | x86-64 Linux | 6 | 8 |
| C | x86-64 Windows | 4 | 4 |

The Windows C convention's four positions are shared between the two kinds.

## Limitations

- One core type. Silverfir-nano's recorded 1.09-cycle floor, on a synthetic
  four-instruction handler chain on an M4, is not this experiment's `floor`
  and is not compared. The comparison above runs the same work on the same
  core, but two different instruction sets: wasm compiled by LLVM against
  hand-assembled `vm.c` bytecode.
- The interpreter has no accumulator and no register-resident locals, so
  five of the six kernels are bound by values passing through frame memory
  (about four cycles per dispatch whatever the shape), which compresses the
  differences between shapes; an interpreter that removes those round trips
  moves toward the `floor` kernel, where the differences are larger.
- Each launch draws a different address-space layout; the median over
  launches and the null comparison bound that effect at about 1% here, well
  below the effects reported, but a variant's layout draw is not separated
  from its code.
