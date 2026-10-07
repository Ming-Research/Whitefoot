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

### Handler table address as a parameter (`wfbase`)

Passing the handler table's address along the chain, instead of forming it
with `adrp` and `add` in every arm, removes 10% of the instructions per
dispatch (`loop` 20.2 to 18.2) and changes no cycle count beyond the layout
noise: against `wfhoist` in ten interleaved launches (`run-wf-base.tsv`)
`loop` +0.3%, `fib` -1.3%, `sieve` +1.3%, `mandel` -0.6%. On this core the
two instructions issued in the slack of a dispatch; E1's C form had shown
the same for its instruction count. It is kept because it costs one register
only where one is free, but it is not a speed result here.

Single launches of one binary also moved between two levels during this
work (`wfhoist` on `fib`: 607 million cycles in the interleaved run, 1151
million in three later launches), the placement effect Silverfir-nano's
record describes; the tables report medians of interleaved launches.

### Values kept in the frame past the registers

A measurement of compiler/match-dispatch-lowering's spill (the interpreter's
loop-invariant values moved to frame slots when the parts need more argument
registers than the convention has), run by GPT-6.1 sol per the owner's
chore policy and checked here. Criterion, recorded before measuring: the
split loop that spills is justified when it takes at least 5% fewer cycles
than the same loop emitted whole, on the median of at least 7 alternating
launches per kernel. Variants, both from `wf/vm.wf` at main d6d6456cf on
the M1 Pro: S gives `run` 16 extra `u64` parameters passed through
unchanged and read by `Movi` (`--dispatch-ledger`: split into 23 arms
taking all 24 integer registers, one value kept in the frame); W adds
`if pc == 18446744073709551615_u64 { return ...; }` before the `match`,
which never fires, so the loop is emitted whole. Both pass every kernel's
checksum. Cycles, seven alternating launches per variant and kernel
([run-spill.tsv](run-spill.tsv); fib was repeated once more because of one
outlier per variant):

| Kernel | S median | W median | Fewer cycles, S |
|---|---:|---:|---:|
| loop | 3,979,329,968 | 6,024,855,078 | 34.0% |
| fib (14 launches) | 636,933,902 | 992,618,892 | 35.8% |
| sieve | 2,359,838,391 | 3,111,945,191 | 24.2% |
| mandel | 364,613,536 | 506,358,032 | 28.0% |

The spill meets the criterion on every kernel. This covers one spilled value
under `preserve_none` on arm64; more spilled values, the C convention and
x86-64 are not measured.

## Stage 3: a wasm interpreter running CoreMark

The design and criteria are in
[the investigation](../../investigations/match-dispatch/DESIGN.md#stage-3-a-wasm-interpreter-running-coremark).
`wasm/gen.py` writes the interpreter; `wasm/coremark.py` alternates
launches and takes medians. Module: Silverfir-nano's
`benchmarks/wasi/coremark/coremark.wasm`, arguments `0x0 0x0 0x66 2000`
(the 2K performance run's seeds, 2000 iterations, about 1.6 s under the
Whitefoot interpreter and 0.35 s under Silverfir-nano's). M1 Pro;
Silverfir-nano built from main as `sf-nano-cli --interp`; the Whitefoot
interpreter compiled with `preserve_none` by `whitefootc` built from the
branch each step names: v1-v2c from `claude/wasm-interp`, which changes no
compiler source from main; v2d from `claude/pin-through-callees`, which
adds stack-box pinning; and v2e-v2h from a local merge, not kept, of that
branch with `claude/checker-closure-scaling`, whose active-term closures
change the checker and were not merged into main. The compiler of the
branch that carries these results has the pinning but not the active-term
closures, and no step was measured again with it.

### v1, the direct stack machine

Every launch reported CoreMark's list, matrix and state CRCs (0xe714,
0x1fd7, 0x8e3a) and one final CRC (0x4983) on both interpreters. Seven
alternating launches ([run-wasm-v1.tsv](run-wasm-v1.tsv)):

| Interpreter | Median score | Spread | Ratio |
|---|---:|---:|---:|
| Whitefoot v1 | 1261.0 | 5.8% | 0.223 |
| Silverfir-nano | 5665.7 | 6.9% | 1 |

The ratio is below the 0.3-0.5x the criterion predicted. One launch of the
dispatch-counting build (`gen.py --count`) and one `/usr/bin/time -l` launch
of the measured build at 2000 iterations:

| Quantity | Value |
|---|---:|
| Dispatches | 1,271,009,318 |
| Instructions retired | 23,117,633,381 |
| Cycles | 5,014,438,171 |
| Instructions per dispatch | 18.2 |
| Cycles per dispatch | 3.94 |

The dispatch count moves by tens between launches, with the digits CoreMark
prints for its timing. The interpreter function splits into 178 per-arm functions taking 15 of
the 24 integer argument registers (`--dispatch-ledger`). At 3.94 cycles a
dispatch costs about what E0's tail-call forms cost with frame round trips
(4.18-4.25 cycles), and at an IPC of 4.6 the core is not waiting on loads:
the loss is the number of dispatches and the instructions each executes.
v1 dispatches once per wasm operation, including every `local.get`,
`local.set` and constant, and each handler checks the operand stack's depth
and the fetch index and moves values through the frame. Silverfir-nano's
interpreter folds locals and constants into its operations' operands
(its static fallthrough statistics are dominated by `MovSlot`, `MovConst`
and folded arithmetic), so it executes fewer dispatches for the same work.
Silverfir-nano's own dispatch count is not reported: `/usr/bin/time -l`
counts only its startup (about 1.1 million cycles), so its execution is
not measured on these counters.

### v2a, register form

Operations read and write frame slots named by u16 operands (`I32Add(d, a,
b)`); the translator tracks each operand's provider (its temporary, a local
or a constant), so `local.get` and constants emit nothing, a `local.set`
after an operation retargets that operation's destination, and operands
reach their temporaries only at control flow. The stack pointer is gone:
the interpreter function requires `fp + 65536 <= stack.len`, checked once
per call and return, which covers every slot access. Predicted before
measuring: dispatches 40-50% of v1's, score 0.45-0.55x.

### v2b, constants in frame slots

Each function's distinct constants, collected before its body is
translated, are copied into frame slots after its locals when it is
entered, and a constant operand names its slot. Predicted: about 23% fewer
dispatches than v2a, score 0.42-0.45x.

### v2 results

Seven alternating launches each ([run-wasm-v2a.tsv](run-wasm-v2a.tsv) with
v1 in the same run, [run-wasm-v2b.tsv](run-wasm-v2b.tsv) with v2a), every
launch with correct CRCs:

| Build | Median score | Ratio to Silverfir-nano | Dispatches | Instructions per dispatch | Cycles per dispatch |
|---|---:|---:|---:|---:|---:|
| v1 | 1260.2 | 0.221 | 1,271,009,318 | 18.2 | 3.94 |
| v2a | 1970.4 | 0.346 | 822,373,679 | 18.8 | 3.91 |
| v2b | 2214.8 | 0.390 | 652,267,155 | 20.9 | 4.35 |

Both scores fall short of their predictions. v2a removed 35% of v1's
dispatches, not 50-60%; v2b removed the 21% its prediction named, but each
remaining dispatch cost more, the constant copy on every call adding to the
calls' cost and the removed `Const` dispatches having been cheap ones.
v2b's remaining dispatches by kind (`gen.py --profile`): `Copy` 107 million,
`BrIf` 103 million, `I32Add` 102 million, `I32Load` 53 million, `I32And` 43
million; the compares that feed a `BrIf` (`I32Ne`, `I32Eqz`, `I32Eq` and the
ordered compares) total about 70 million.

The machine code of v2b's `I32Add` handler is 18 instructions to its
indirect branch. Beyond the operation itself, it reloads the stack box's
pointer from its reference (two instructions and a dependent load), because
the arm hands the reference to its helper function and the hoisting rule
pins only a reference used for box projections; it recomputes the next
cell's address from the cell index; and it adds the frame base to each slot
index. The first was a compiler limitation, which v2d removes (below); the
other two are the derived-address item in `docs/todo.md`. Writing the handlers'
bodies back into the arms, which would avoid the reload, does not check: the
interpreter function then grows past what the checker handles in minutes.

### v2c, compare and branch fused

A `br_if` whose condition is the i32 comparison emitted just before it, and
that moves no result, becomes one compare-and-branch operation
(`BrI32Ne(a, b, t)` and the other nine); an `if` on such a comparison
branches on its negation, and `eqz` maps onto the existing `BrUnless` and
`BrIf`. The condition no longer reaches a temporary, so a condition held in
a local is tested in place. Predicted: about 11% fewer dispatches than v2b,
score about 0.42x. Seven alternating launches
([run-wasm-v2c.tsv](run-wasm-v2c.tsv)), every launch with correct CRCs:

| Build | Median score | Ratio to Silverfir-nano | Dispatches | Instructions per dispatch | Cycles per dispatch |
|---|---:|---:|---:|---:|---:|
| v2b | 2239.6 | 0.390 | 652,267,155 | 20.9 | 4.35 |
| v2c | 2534.9 | 0.441 | 551,583,965 | 21.6 | 4.53 |

The dispatches fell 15.4%, more than predicted, because `Copy` also fell
from 107 to 77 million: conditions held in locals no longer needed one. The
largest remaining kinds are `I32Add` (102 million), `Copy` (77 million),
`I32Load` (53 million) and `I32And` (43 million).

### v2d, the stack box kept across helper calls

The handlers' helper functions declare `writes(stack.inner)` and
`writes(mem.inner)`, writes below the boxes' content, in place of
`writes(stack)` and `writes(mem)`, and the interpreter is compiled by the
compiler of branch `claude/pin-through-callees`, which keeps such a
reference pinned across those calls and hands each callee a part-local slot
holding the hoisted box pointer. The `I32Add` handler's machine code loses
its reload of the stack box's pointer (`ldr x11, [x22]`): seventeen
instructions to its indirect branch instead of eighteen, and no dependent
load. Seven alternating launches ([run-wasm-v2d.tsv](run-wasm-v2d.tsv)), every
launch with correct CRCs; Silverfir-nano's spread includes one slow launch:

| Build | Median score | Ratio to Silverfir-nano | Instructions | Cycles |
|---|---:|---:|---:|---:|
| v2c | 2522.1 | 0.441 | 11,918,394,281 | 2,498,674,814 |
| v2d | 2617.8 | 0.458 | 11,230,519,951 | 2,431,061,674 |

The score rises 3.8%, above the 2% criterion.

### v2e, handler bodies written into their arms

`gen.py --inline` writes each helper's body into its arm, binding the
helper's parameters with `let`, except a body that delivers a value from a
`match` (`give`), which stays a call: the checker's handling of such
deliveries grows faster than linearly with the function, and the fully
inlined interpreter took 313 s to check where this form takes 3.5 s. Built
by the compiler with stack-box pinning and active-term closures merged, the
`I32Add` arm's machine code is the same seventeen instructions as v2d's: by
v2d the helpers were already inlined by LLVM, so writing them into the
source changes the checker's work, not the dispatch. Its median in the v2f
run below is 2635.0.

### v2f, fewer copies

`gen.py --profile` with each copy site given its own operation kind
attributed v2e's 77 million `Copy` dispatches: 64 million from a
`local.get` followed by a `local.set` (a copy between locals, 20.6 million
of them directly after another such copy), 12.5 million from operands put
in their temporaries before control flow, 9.2 million of those before a
`br_table` and 2.1 million before a `return`. v2f reads the operand a
`br_table` or `return` consumes, and the result at a function's end, from
its local in place (a `return` and a function's end no longer put any
other operand in its temporary), and merges a copy between locals emitted
directly after another into one `Copy2(d, s, e, t)`, which moves `s` to
`d` and then `t` to `e`; a loop's start, a branch target, ends the merging.
Criterion, set before measuring: adopt if the median score rises at least
2%, as for v2d. Dispatches fell from 551,583,984 to 523,297,303 (5.1%),
`Copy` and `Copy2` together to 49 million. Seven alternating launches
([run-wasm-v2f.tsv](run-wasm-v2f.tsv)), every launch with correct CRCs,
and one `/usr/bin/time -l` launch each:

| Build | Median score | Spread | Instructions | Cycles |
|---|---:|---:|---:|---:|
| v2e | 2635.0 | 3.6% | 11,229,455,758 | 2,420,843,786 |
| v2f | 2706.4 | 2.4% | 10,906,830,435 | 2,354,421,800 |

The score rises 2.7%, meeting the criterion, with cycles down 2.7%.

### v2g, address additions folded into loads and stores

An i32 load or store whose address is the temporary an `i32.add` emitted
just before it wrote takes that addition's two operand slots instead
(`I32LoadIx(d, a, b, o)` loads from `a + b + o`, the sum wrapping to 32
bits as the `i32.add` did, and likewise the other i32 loads and stores), and
the addition is not emitted; a store qualifies when its value comes from a
local, so that no copy is emitted between the addition and the store.
Silverfir-nano's translator folds such additions the same way. Predicted
before measuring: 25-35 million fewer dispatches, from the 37.7 million
`I32Add` dispatches directly followed by a load or store in v2e's
operation-pair profile; criterion: adopt if the median score rises at
least 2%. Dispatches fell from 523,297,303 to 506,088,437 (3.3%), below the
prediction: 17.2 million additions folded. A load's only operand is its
address, so an addition directly before an eligible load that was not
folded wrote a local (`local.set` or `local.tee` took over its
destination), which the fold does not reach, as for all but 586 of the 8.7
million additions before an `I32Load8U`; before a store, the addition may
also be the stored value, or the value a constant. Seven
alternating launches ([run-wasm-v2g.tsv](run-wasm-v2g.tsv)), every launch
with correct CRCs, and one `/usr/bin/time -l` launch each:

| Build | Median score | Spread | Instructions | Cycles |
|---|---:|---:|---:|---:|
| v2f | 2739.7 | 3.0% | 10,906,796,639 | 2,348,767,399 |
| v2g | 2832.9 | 3.2% | 10,648,499,411 | 2,284,244,384 |

The score rises 3.4%, meeting the criterion.

### Not adopted: pairs of additions

v2g's operation-pair profile shows 33.3 million `I32Add` dispatches
directly followed by another. Folding the second into the first's sum
(`(a + b) + c`, when the second adds the temporary the first wrote) folded
425: the additions are independent, each written to a local, as in a loop
that advances two indices. Merging an `I32Add` emitted directly after
another, with no branch target between them, into one `I32Add2(e, x, y, d,
a, b)` that performs both in order (and splitting it again when a load or
store folds the second), was predicted to remove 20-33 million dispatches;
criterion: adopt if the median score rises at least 2%. It removed 18.4
million (506,088,437 to 487,719,418, 3.6%); seven alternating launches
([run-wasm-add-pairs.tsv](run-wasm-add-pairs.tsv)), every launch with
correct CRCs:

| Build | Median score | Spread | Instructions | Cycles |
|---|---:|---:|---:|---:|
| v2g | 2762.4 | 1.7% | 10,648,948,853 | 2,285,196,012 |
| add-pairs | 2809.0 | 0.6% | 10,483,486,410 | 2,247,321,457 |

The score rises 1.7%, short of the criterion with spreads that decide it,
so the interpreter keeps v2g's form. A merged dispatch saves the indirect
branch and the fetch but not the second addition's three slot accesses.

### Frame slots addressed from a derived pointer

The interpreter's handlers address frame slots as `stack^.inner[fp + k]`,
so each split part formed `fp + k` for every slot it touched and the stack
block's first element again. Branch `claude/derived-addresses` addresses an
element whose offset is a checked sum, computed in the part, of a value the
part has from its entry (here `fp`) and another value, from a pointer to
element `fp` that the part's prelude derives once
(compiler/match-dispatch-lowering). The `I32Add` arm's machine code goes
from seventeen instructions to fifteen: its three `fp + k` additions are
gone, and the block's first element and `fp`'s scaling fold into two
additions at the top of the arm. Criterion, set before the v2g
measurement: adopt if the median score rises at least 2%. Each build
compiled by the compiler with stack-box pinning and active-term closures,
without and with the change; every launch with correct CRCs:

| Interpreter | Launches | Median score, without | with | Instructions, without | with | Cycles, without | with |
|---|---:|---:|---:|---:|---:|---:|---:|
| v2e ([run](run-derived-v2e.tsv)) | 7 | 2567.4 | 2594.0 | 11,230,506,405 | 10,654,763,608 | 2,423,045,295 | 2,414,653,203 |
| v2g ([run](run-derived-v2g.tsv)) | 15 | 2762.4 | 2820.9 | 10,649,927,045 | 10,072,938,212 | 2,291,486,095 | 2,235,379,256 |
| v2h ([run](run-wasm-v2h-nano.tsv)) | 7 | 2971.8 | 2998.5 | 9,716,197,938 | 9,397,630,140 | 2,125,190,385 | 2,110,365,739 |

The instructions fall 5.1% and 5.4%. On v2e the cycles and score do not
move beyond the spread (score +1.0%, cycles -0.3%); on v2g, whose folded
loads and stores each address two or three slots, the cycles fall 2.4% and
the score rises 2.1%, meeting the criterion. A seven-launch v2g run
before this one gave +1.9%, within its 2% spread, which is why the
fifteen-launch run decides; two `/usr/bin/time -l` launches of each v2g
build gave cycles within 0.1% of each other. On v2h, whose accumulator
forms already drop many slot accesses, the instructions fall 3.3% and the
score rises 0.9% with cycles 0.7% lower, below the criterion: the gain
shrinks as fewer slots are addressed per dispatch.

### v2h, an accumulator register

The interpreter function takes a `u64` parameter `acc`, carried by every
tail call; the split lowering keeps it in a register (318 arms taking 16 of
the 24 integer argument registers, `--dispatch-ledger`). Operations gain
forms that leave their result in `acc` instead of slot `d` (`I32AddD`), take
an operand from it (`I32AddA`, `I32StoreV`, `BrIfC`), or both
(`I32AddAD`): 125 forms of the i32 arithmetic, comparisons, loads and
stores, the compare-and-branch operations, `BrIf`, `BrUnless` and `Select`.
When the translator emits an operation that pops the temporary the
operation emitted just before it wrote, with no label between (the test
the compare and address fusions use), and both have such forms, the
earlier one leaves its result in `acc` and the later one reads it there.
The operand stack's discipline gives that temporary no other reader; a
result that `local.set` or `local.tee` retargeted to a local keeps its
store. A form keeps its operation's fields, a field read from `acc`
holding 65535, which is never a slot, so the compare and address fusions
and the patching of forward branches carry it. Silverfir-nano's
interpreter keeps such values in an accumulator register the same way. In
v2g's profile 202 million of the 506 million dispatches read the slot the
dispatch before them wrote, locals included. Predicted before measuring:
the same dispatch count, 4-6% fewer instructions, 3-6% fewer cycles and a
score 3-6% higher; criterion: adopt if the median score rises at least 2%.
Dispatches: 506,088,440, of which 134.8 million leave their result in
`acc`. Both builds compiled by the compiler with stack-box pinning and
active-term closures; seven alternating launches
([run-wasm-v2h.tsv](run-wasm-v2h.tsv)), every launch with correct CRCs,
and one `/usr/bin/time -l` launch each:

| Build | Median score | Spread | Instructions | Cycles |
|---|---:|---:|---:|---:|
| v2g | 2805.0 | 0.8% | 10,649,237,559 | 2,287,685,521 |
| v2h | 3016.6 | 4.1% | 9,715,057,852 | 2,126,003,548 |

The score rises 7.5%, above the prediction and the criterion, with every
v2h launch faster than every v2g launch; instructions fall 8.8% and cycles
7.1%. In the same binary the `I32Add` arm is 19 instructions to its
indirect branch, `I32AddA` 16 and `I32AddAD` 14: a form reading `acc`
drops its operand's index load and slot load, and a form writing it drops
its destination's index load, address and store.

Against Silverfir-nano in one run of seven alternating launches
([run-wasm-v2h-nano.tsv](run-wasm-v2h-nano.tsv)), every launch with correct
CRCs, v2h's median is 2971.8 (spread 0.3%) and Silverfir-nano's 5730.7
(0.9%), a ratio of 0.519; v2h compiled by the compiler with derived
frame-slot addresses as well, below, scores 2998.5 (0.3%), 0.523.

## Argument registers

How many arguments each calling convention passes in registers, which
compiler/match-dispatch-lowering uses as its register budget. `regprobe.py`
reports two numbers for each target:
- **Direct:** the largest n for which a function of n `i64` (or n `double`)
  parameters reads no argument from the stack.
- **Table-loaded:** the largest n for which a dispatch part's own transfer
  compiles with no argument on the stack. Here n integer parameters are each
  recomputed, and a guaranteed tail call goes through an address loaded from
  an external table.

The parts transfer the second way, so the budget is the table-loaded
column. With clang 19.1.1 and 20.1.2 on a hosted `ubuntu-24.04` runner
([run 37521521191](https://github.com/Ming-Research/Whitefoot/actions/runs/37521521191)):

| convention | target | direct integer | direct floating | table-loaded integer |
|---|---|---:|---:|---:|
| `preserve_none` | aarch64 (Apple, Linux) | 24 | 8 | 24 |
| `preserve_none` | x86-64 Linux | 12 | 8 | 11 |
| `preserve_none` | x86-64 Windows | 12 | 8 | 12 |
| C | aarch64 (Apple, Linux) | 8 | 8 | 8 |
| C | x86-64 Linux | 6 | 8 | 6 |
| C | x86-64 Windows | 4 | 4 | 4 |

The Windows C convention's four positions are shared between the two kinds.

On x86-64 Linux the transfer's loaded address takes one of the twelve
registers `preserve_none` passes arguments in. A split that used all twelve
failed in clang 19's register allocation ("ran out of registers during
register allocation"); see "Stage 3 on x86-64" below. Clang 18.1.3 has no
`preserve_none` (every count 0), and its C counts match those above. The
earlier direct-only measurement, with clang 21.0.0, gave the same direct
counts. Before this run, the probe also counted a module the compiler
refused as one with no stack argument; it now counts it as one that does
not fit.

## Stage 3 on x86-64

The design, prediction and decision rule are in
[the investigation](../../investigations/match-dispatch/DESIGN.md#stage-3-on-x86-64).
v2h was written by `wasm/gen.py` and compiled by the branch's `whitefootc`
after the budget correction. It was compared with Silverfir-nano `5f248e44`
`sf-nano-cli --interp` on the CoreMark 2K performance run, 7 alternating
launches, every launch with correct CRCs
([run 37522471290](https://github.com/Ming-Research/Whitefoot/actions/runs/37522471290)):

| host | clang, convention | the dispatch loop | Whitefoot | Silverfir-nano | ratio |
|---|---|---|---:|---:|---:|
| 14900K | 18.1.3, C | split into 318 arms, 6 integer registers, 9 values in the frame | 4376.4 (1.1%) | 7812.5 (1.2%) | 0.560 |
| hosted EPYC 7763 | 19.1.1, `preserve_none` | split into 318 arms, 11 integer registers, 4 values in the frame | 1984.1 (3.1%) | 2574.0 (1.9%) | 0.771 |

Medians, with each engine's spread across its launches. The hosted
runner's processor is not chosen, so its row indicates only. Before the
budget correction, the `preserve_none` build failed in register allocation
([run 37519332120](https://github.com/Ming-Research/Whitefoot/actions/runs/37519332120)).

## Stage 3: the code cursor

The design and the decision rule are in
[the investigation](../../investigations/match-dispatch/DESIGN.md#stage-3-the-code-cursor).
v2h was written by `wasm/gen.py` and compiled by `whitefootc` at main
`c1034407f`. A temporary job on the branch `claude/stage3-cursor`, at
`0eca64937`, disassembled the hot arms
([run 37536017866](https://github.com/Ming-Research/Whitefoot/actions/runs/37536017866)).
The Linux job then failed in its last command, a header listing whose pipe
closed early; every arm had been printed by then.

| host | clang, convention | the dispatch loop |
|---|---|---|
| ubuntu-24.04, x86-64 | 22.1.8, `preserve_none` | split into 318 arms, 11 integer registers, 4 values in the frame |
| macos-15, AArch64 | Apple 17.0.0, `preserve_none` | split into 318 arms, 16 of 24 integer registers |

`I32Add`'s path from entry to its transfer, by role:

| role | x86-64 | count | AArch64 | count |
|---|---|---:|---|---:|
| operation, operand and result slots | `movzwl` ×3, `movl`, `addl`, `movq` | 6 | `ldrh` ×3, `add` ×5, `ldr` ×2, `str` | 11 |
| frame base | `leaq (,%r15,8)`, `addq 0xc8(%r11)` | 2 | (in the slot additions) | 0 |
| next index and bounds test | `leaq 0x1(%r14)`, `cmpq %rsi`, `jae` | 3 | `add x8, x22, #1`, `cmp x8, x25`, `b.hs` | 3 |
| index to address | `movq`, `shlq $0x4`, `leaq (%rdx,%r10)`, `addq $0x10` | 4 | `add x4, x26, x8, lsl #4` | 1 |
| tag, handler and transfer | `movl 0x10(%rdx,%r10)`, `leaq table(%rip)`, `movq (%rbx,%r10,8)`, `jmpq` | 4 | `ldr w9, [x4, #0x10]!`, `ldr x7, [x5, x9, lsl #3]`, `br x7` | 3 |
| moves | `movq %rax, %r14`, `movq %r10, %rax` | 2 | `mov x22, x8` | 1 |
| total | | 21 | | 19 |

The other arms the job printed end in the same steps, in an order and with
registers of their own: `I32AddA`, `I32AddAD`, `BrIf`, `BrI32LtS`, `Copy`,
`Copy2`, `I32Load`, `I32Store`, `Call` and `Return`. Each has exactly one
shift by 4 and one 16-byte address adjustment, and each leaves the next
index in its carried register. Most move it there after forming the
address. `I32AddAD` increments that register in place and first copies the
old index for the trap report. On x86-64, `Call` moves it there before
the shift. A
branch arm loads its target from the operation in place of
`leaq 0x1(%r14)`. Forming and testing the next address therefore takes
about 7 instructions per dispatch on x86-64 and 4 on AArch64. v2h has no
`BrI32LtSC` or `LocalTee` arm.

The arms receive the matched element's address, the header value they read
the operation's fields through (`%r9` on x86-64, `x4` on AArch64). The
dispatch forms the next element's address from the next index, not from
it.

### A, the address beside the index

The construction and the adoption rule are in
[the investigation](../../investigations/match-dispatch/DESIGN.md#stage-3-the-code-cursor).
The branch `claude/stage3-cursor` at `a8afbbe88`, whose compiler change is
`7f86697b1`, was measured against its merge base `e1708490c` by the
temporary `stage3-cursor` job: CoreMark 2K, 7 alternating launches, every
launch with correct CRCs
([run 37542365068](https://github.com/Ming-Research/Whitefoot/actions/runs/37542365068)).
Both compilers split the loop the same way. The branch's ledger adds
"carries the matched Op's address between the parts".

`I32Add`'s path from entry to its transfer:
- **x86-64:** 18 instructions, down from 21. The move, the shift and the
  two additions became one addition (`add $0x10, %r9`), and the tag is
  loaded through the received address (`mov 0x10(%r9), %r10d`).
- **AArch64:** 18 instructions, down from 19. The shifted addition went,
  and the pre-indexed tag load (`ldr w9, [x4, #0x10]!`) now advances the
  received address.

Both match the prediction. A branch arm forms its target's address from
the received one: `sub`, `shl` and `lea` on x86-64.

| host | clang, convention | base | A | ratio | A above base, in pairs |
|---|---|---:|---:|---:|---|
| hosted EPYC 7763 | 22.1.8, `preserve_none` | 1892.1 (8.1%) | 1974.3 (4.3%) | 1.043 | 6 of 7, 0.995-1.090 |
| macos-15, M1 (virtual) | Apple 17.0.0, `preserve_none` | 2074.7 (15.5%) | 2252.3 (12.3%) | 1.086 | 7 of 7, 1.039-1.158 |

Medians, with each side's spread across its launches. Under the rule these
hosts are indications only: neither processor is chosen, and the
virtualized M1 spreads by more than the effect. The decision waits for the
14900K with the pinned LLVM.

### B, the address in place of the index

B is the branch at `c96bc1701`. Its compiler change, `7d2a5e7a8`, is kept on
the branch `claude/stage3-cursor-instead`. The ledger adds "carries the
matched Op's address between the parts in place of its index", and the
loop takes the same 11 integer registers, the run's end address in place
of the index. On x86-64:
- **`I32Add`** runs 17 instructions, one fewer than A, where two were
  predicted. `lea 0x10(%r8), %rax`, `cmp %r9, %rax` and `jae` test the
  moved address against the end, and the index's move is gone. The moved
  address still passes through a move into its carried register
  (`mov %rax, %r8`).
- **`BrIf`** recovers `pc` on entry, in four instructions (`lea`, `mov`,
  `sub`, `shr $0x4`), and saves and restores `%rbp`.
- **The handler table's address** is still formed in every arm: the end
  address took the index's register.

### A and B on the 14900K

The job compared the merge base `e1708490c`, A (`a8afbbe88`) and B
(`c96bc1701`) in one run: CoreMark 2K, 7 launches of each, interleaved,
every launch with correct CRCs. The 14900K's `/usr/bin/clang` had been
moved to 22.1.8 before the run
([run 37544282861](https://github.com/Ming-Research/Whitefoot/actions/runs/37544282861)).
The hosted rows come from the same run's other jobs and from
[run 37543903255](https://github.com/Ming-Research/Whitefoot/actions/runs/37543903255).

| host | base | A | B | A / base | B / A |
|---|---:|---:|---:|---|---|
| 14900K, 22.1.8, `preserve_none` | 4705.9 (1.2%) | 5319.1 (2.1%) | 5235.6 (0.8%) | 1.130; 7 of 7 pairs above, 1.105-1.142 | 0.984; 0 of 7 above, 0.979-1.000 |
| hosted EPYC 7763, 22.1.8 | 1906.6 (11.2%) | 1939.9 (7.5%) | 1988.1 (12.0%) | 1.017; 5 of 7 | 1.025; 5 of 7 |
| hosted EPYC 9V45, 22.1.8 | 3418.8 (2.4%) | 3552.4 (3.6%) | 3571.4 (6.5%) | 1.039; 7 of 7 | 1.005; 3 of 7 |
| macos-15, M1 (virtual) | 2702.7 (14.9%) | 2747.3 (20.3%) | 2617.8 (30.7%) | 1.016; 5 of 7 | 0.953; 1 of 7 |
| macos-15, M1 (virtual) | 1901.1 (18.3%) | 1982.2 (11.5%) | 1846.7 (21.7%) | 1.043; 6 of 7 | 0.932; 0 of 7 |

Medians, with each side's spread across its launches. The first EPYC row
and the first M1 row are from run 37544282861, the others from run
37543903255. The 14900K's spreads are below 2.2% for every side. A is 13.0%
above the base, B 1.6% below A, each in every pair. The hosted runners
agree that A gains and disagree on B, within their spreads.

A second run on the 14900K repeated A against the base with a copy of the
base's interpreter, the twin, as the noise control
([run 37545234428](https://github.com/Ming-Research/Whitefoot/actions/runs/37545234428);
A's compiler at `82cba945e`, equal to `7f86697b1`'s):

| host | base | twin | A | twin / base | A / base |
|---|---:|---:|---:|---|---|
| 14900K, 22.1.8, `preserve_none` | 4683.8 (1.4%) | 4728.1 (0.7%) | 5305.0 (1.3%) | 1.009; 4 of 7, 0.998-1.014 | 1.133; 7 of 7, 1.122-1.135 |
| hosted EPYC 7763, 22.1.8 | 1906.6 (6.2%) | 1902.9 (7.4%) | 1970.4 (2.7%) | 0.998; 4 of 7 | 1.033; 7 of 7 |

The twin sits within 1% of the base and A 13.3% above it. The macos-15
job of the same run spread by 30-51% and is not shown.

## Stage 3: x86-64 register pressure

The question and the rule are in
[the investigation](../../investigations/match-dispatch/DESIGN.md#stage-3-x86-64-register-pressure).
The branch `claude/stage3-x86-regs` at `2eccb0f94`, whose compiler equals
the code cursor's `f1db34716`, emitted v2h's x86-64 module with clang
22.1.8
([run 37548771908](https://github.com/Ming-Research/Whitefoot/actions/runs/37548771908),
artifact `stage3-x86-ir`). The loop splits into 318 arms and takes all 11
integer registers. The arms counted below use the value in an emitted
instruction other than loading it from the frame, storing it into a
part's pin slot, passing it on unchanged, or joining it.

| part parameter | the `run` parameter it comes from | arms using it | where the parts keep it |
|---|---|---:|---|
| `fp`, the frame base | `fp` | 304 | register |
| the code's length | `code` | 254 | register |
| the stack's elements | `stack` | 129 | **frame** |
| `acc` | `acc` | 102 | register |
| the memory's elements | `mem` | 40 | register |
| the constants' reference | `consts` | 2 | register |
| the globals' elements | `globals` | 2 | register |
| the function table's elements | `funcs` | 2 | frame |
| the branch table's elements | `brtab` | 2 | frame |
| the indirect-call table's elements | `table` | 1 | frame |
| the code's elements | `code` | 0 | register |

The parts also take `pc`, the matched element's address, the result's
destination and the frame. Every arm that reads a frame slot reloads the
stack's element address (`add 0xc8(%r11), %r10`). The split keeps that
value in the frame although 129 arms read it, while three values that at
most two arms read hold registers, one of them read by none. Two causes:
- **The spill order misses reads through replaced projections.** It counts
  the arms whose instructions name a value. An arm reads the stack's box
  through its own projection, which emission replaces by the one computed
  before the loop, so the count misses those reads.
- **The cursor leaves the code's element address a parameter.** Since the
  code cursor, the header receives the element's address instead of
  forming it, so no part reads the run's address. It still counts as read
  by every arm, because the header's indexing instruction names it.

### The register-pressure candidate, and the code cursor on Halo

The register-pressure candidate counts reads through replaced projections
and pins, and drops parameters no part reads. Two runs measured it on the
14900K with clang 22.1.8, at the branch's `5c1436474`, whose compiler
equals `89dfccb01`'s. Each compared three compilers, with a twin of the
base and 7 interleaved launches:
- **step:** main before the code cursor, `e1708490c`;
- **base:** main with the code cursor, `3a260446c`;
- **head:** the candidate.

On the wasm interpreter the candidate's split keeps 3 values in the frame,
not 4: the code's element address is no longer a parameter.
CoreMark 2K scores, higher is better
([run 37551028726](https://github.com/Ming-Research/Whitefoot/actions/runs/37551028726)):

| host | before the cursor | cursor | twin | candidate | candidate / cursor | cursor / before |
|---|---:|---:|---:|---:|---|---|
| 14900K | 4705.9 (0.5%) | 5305.0 (0.8%) | 5291.0 (1.8%) | 5376.3 (4.2%) | 1.013; 5 of 7, 0.977-1.022 | 1.127; 7 of 7 |
| hosted EPYC 7763 | 1888.6 (8.2%) | 1938.0 (3.9%) | 1924.9 (6.3%) | 2036.7 (6.7%) | 1.051; 7 of 7, 1.020-1.101 | 1.026; 6 of 7 |

Halo's Lua interpreter, Halo-wf at `acb39ad7f`, built with full LTO.
`halo.vm.run` splits into 73 arms on 11 integer registers under all three
compilers, keeps no value in the frame, and with the cursor its ledger
adds "carries the matched Cell's address between the parts". Wall time
per launch in seconds, lower is better; `fib` at N = 35 instead of the
kernel's 30
([run 37551026003](https://github.com/Ming-Research/Whitefoot/actions/runs/37551026003)):

| kernel | before the cursor | cursor | twin | candidate | candidate / cursor | cursor / before |
|---|---:|---:|---:|---:|---|---|
| `fib` | 1.1108 (1.1%) | 1.1206 (2.7%) | 1.1228 (0.8%) | 1.1224 (2.2%) | 1.002 | 1.009; slower in 6 of 7 |
| `loop` | 0.4373 (0.5%) | 0.4620 (0.5%) | 0.4626 (0.7%) | 0.4622 (1.2%) | 1.000 | **1.057; slower in 7 of 7, 1.054-1.061** |

The twin sits within 0.2% of the cursor's times. The code cursor makes
Halo's `loop` 5.7% slower, and its `fib` about 1% slower. Halo's arms take
the next index from a helper's result, which the lowering cannot see as
`pc` plus a constant. Every such edge therefore moves the received address
by `next - pc` elements: a subtraction more than forming the address from
the run, as the header did before, with nothing saved.

### Edges by their step

The branch at `b7db6b95f` moves the received address only where an edge's
index is the received one plus a constant, seen through joins whose every
incoming value is the index. Every other edge forms the address from the
run, and the parts keep the run's address where an arm has such an edge.
It also carries the register-pressure candidate above. The same three
compilers were compared, with a twin of the cursor's
([runs 37555992672](https://github.com/Ming-Research/Whitefoot/actions/runs/37555992672)
for CoreMark and
[37555989802](https://github.com/Ming-Research/Whitefoot/actions/runs/37555989802)
for Halo). The wasm interpreter keeps 4 values in the frame again: its
branch arms read the run's address.

CoreMark 2K scores, higher is better:

| host | before the cursor | cursor | twin | branch | branch / cursor | branch / before |
|---|---:|---:|---:|---:|---|---|
| 14900K | 4683.8 (1.2%) | 5263.2 (1.3%) | 5263.2 (1.6%) | 5390.8 (1.1%) | 1.024; 7 of 7, 1.016-1.035 | 1.151; 7 of 7 |
| hosted EPYC 7763 | 1879.7 (6.1%) | 1930.5 (3.6%) | 1926.8 (5.9%) | 2059.7 (6.2%) | 1.067; 7 of 7 | 1.096; 7 of 7 |

Halo, wall time per launch in seconds, lower is better:

| kernel | before the cursor | cursor | twin | branch | branch / before | branch / cursor |
|---|---:|---:|---:|---:|---|---|
| `fib` | 1.1084 (0.5%) | 1.1223 (2.6%) | 1.1199 (0.4%) | 1.1137 (1.0%) | 1.005; slower in 7 of 7, 1.000-1.014 | 0.992 |
| `loop` | 0.4367 (0.6%) | 0.4617 (1.2%) | 0.4614 (0.4%) | 0.4367 (0.5%) | 1.000; 0.994-1.004 | 0.946; faster in 7 of 7 |

On the 14900K, `I32Add` runs 17 instructions. The reload of the stack's
element address from the frame is gone: `lea (%rcx,%r15,8), %rbx` takes it
from a register. A branch arm forms its target's address from the run
(`shl`, `lea`, `add`), without the subtraction.

An intermediate build without the joins (`885a0af31`; runs 37555134002
and 37555131716) measured CoreMark 1.019 against the cursor and Halo's
`loop` 1.000 and `fib` 1.000 against before the cursor. Its unit tests
failed where an arm's `pc + 1` follows the join of an `if`.

## Stage 3: the gap to Silverfir-nano

The question and the use of the result are in
[the investigation](../../investigations/match-dispatch/DESIGN.md#stage-3-the-gap-to-silverfir-nano).
v2h was compiled by main at `0b7f5c5b9`. Silverfir-nano is `5f248e44`,
`sf-nano-cli --interp`, whose handlers are generated assembly. wasmi is
`wasmi_cli` 2.0.0, a Rust interpreter that dispatches by tail calls when
built optimized for x86-64 or AArch64. All runs use CoreMark 2K with
correct CRCs.

**The ratio on the 14900K**, clang 22.1.8, 7 interleaved launches, medians
([run 37562405938](https://github.com/Ming-Research/Whitefoot/actions/runs/37562405938)):

| engine | score | against nano |
|---|---:|---|
| v2h | 5434.8 (1.1%) | 0.701, 0.691-0.707 |
| twin of v2h | 5449.6 (1.6%) | 0.703 |
| Silverfir-nano | 7751.9 (1.9%) | 1.000 |

**On the owner's MacBook Air M5**, Apple clang 21. The owner authorized
these local runs. The release compiler's `llvm.coro.end` form fails on
Apple clang 21, so v2h was compiled by a compiler built on the Mac from
`b60436d33`, main with its probe fixed (PR #264). Scores are from 7
interleaved launches; the rest from 3 single launches each, whose spread
is below 1%.

| engine | score | against nano | dispatches | instructions | cycles | instructions per dispatch | cycles per dispatch | IPC |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| v2h | 2378.1 | 0.496 | 506.1M | 9.345G | 1.896G | 18.47 | 3.75 | 4.93 |
| wasmi 2.0.0 | 3338.9 | 0.696 | 576.9M | 4.502G | 1.354G | 7.80 | 2.35 | 3.33 |
| Silverfir-nano | 4796.2 | 1.000 | 521.6M | 4.051G | 0.957G | 7.77 | 1.84 | 4.23 |

How each count was taken:
- **v2h:** `gen.py --count`, which counts in the interpreter function's
  header. That build is not split, but its count does not depend on the
  lowering.
- **Silverfir-nano:** its `interp-count` feature, read with
  `--interp-stats`.
- **wasmi:** a copy of its source counting at its tail-call `dispatch!`
  macro and at the first dispatch.
- **Instructions and cycles:** `/usr/bin/time -l`.

A second 14900K run added wasmi and Silverfir-nano's x86-64 dispatch count
([run 37567804610](https://github.com/Ming-Research/Whitefoot/actions/runs/37567804610)):

| engine | score | against nano |
|---|---:|---|
| v2h | 5405.4 (2.4%) | 0.692, 0.689-0.704 |
| twin of v2h | 5420.1 (1.1%) | 0.694 |
| wasmi 2.0.0 | 6622.5 (2.0%) | 0.848, 0.842-0.863 |
| Silverfir-nano | 7812.5 (2.3%) | 1.000 |

Silverfir-nano's x86-64 build makes 521,583,097 dispatches, 23 fewer than
its AArch64 build, so the counts compare across the two.

v2h makes the fewest dispatches of the three, 3% fewer than Silverfir-nano
and 12% fewer than wasmi. The gap is in each dispatch: on the M5 v2h
executes 18.5 instructions per dispatch where both others execute 7.8, and
its higher IPC recovers part of that. Relative costs per dispatch, from the
scores and the dispatch counts:

| | 14900K | M5 |
|---|---:|---:|
| v2h | 1.49 | 2.08 |
| wasmi 2.0.0 | 1.07 | 1.30 |
| Silverfir-nano | 1.00 | 1.00 |

At wasmi's cost per dispatch, v2h's fewer dispatches would put it at about
0.97 of Silverfir-nano on the 14900K.

**Where v2h's instructions go.** The comparison pairs v2h's `I32Add` on
x86-64 (main, 17 instructions; see "Edges by their step") with wasmi's
`i32_add_rss` on AArch64. That handler adds two stack slots into wasmi's
integer register, and its listing is from the M5 binary.

| role | v2h `I32Add`, x86-64 | wasmi `i32_add_rss`, AArch64 |
|---|---:|---:|
| operand fields | 3 (`movzwl` each) | 1 (`ldp` of two byte offsets) |
| frame base | 1 (`lea (%rcx,%r15,8)`) | 0 (the frame is a pointer) |
| operation and slots | 3 | 3 (two loads, one add into the register) |
| next index and bounds test | 3 (`lea`, `cmp`, `jae`) | 0 |
| next operation and its handler | 4 (tag load, cursor add, table address, handler load) | 1 (`ldr x7, [x1, #0x10]!`: the next cell holds its handler's address) |
| moves | 2 | 0 |
| transfer | 1 | 1 |
| total | 17 | 6 |

Most of the difference is in the dispatch: 9 instructions against 1.
- **Bounds test:** wasmi tests no bound, since its validated code cannot
  fall off its end.
- **Handler address:** its cells hold their handler's address, so the next
  handler is one load that also advances the pointer.
- **Operands:** its operand fields are byte offsets read in pairs.

Silverfir-nano's cells likewise hold their handler's address. Its x86-64
handlers keep two locals and the accumulator in registers and preload the
next handler word.

## Stage 3: the handler's address in the element

Candidate 1 of
[the investigation](../../investigations/match-dispatch/DESIGN.md#stage-3-binding-what-wasmi-and-silverfir-nano-bind),
measured as an upper bound with the prototype at `e840fb423`. The base is
main at `8b647edbb`. In the prototype, a union-laid-out enum that exactly
one split loop matches gains a pointer word after its largest variant.
Every construction writes the address of the arm its tag selects, and the
dispatch loads the next arm from the element it moves to. The ledger
confirms that v2h's loop dispatches this way: "dispatches through the
handler word in each Op".

**CoreMark on the 14900K**, clang 22.1.8, 7 interleaved launches
([run 37572316181](https://github.com/Ming-Research/Whitefoot/actions/runs/37572316181)):

| engine | median score | spread | against base |
|---|---:|---:|---:|
| head | 5970.1 | 4.6% | 1.101 |
| twin of the base | 5376.3 | 6.5% | 0.992 |
| base | 5420.1 | 3.2% | 1.000 |

- **Head against base, launch by launch:** ahead in all 7 pairs, at
  1.055 to 1.114, median 1.093.
- **Twin against base:** 0.967 to 1.011.
- **Against Silverfir-nano:** that run's 7812.5 is not in this one. Taking
  it anyway, head would be about 0.76 of nano and base 0.69.

The hosted runners agree in direction. Ubuntu 24.04 measures 1.090, with
the twin at 0.992. macOS 15 measures 1.008, but its spreads of 10% to 25%
cannot separate the sides.

**`I32Add` on x86-64** shrinks from 17 to 15 instructions on its path to
the next arm:
- **Removed:** the handler table's address (`lea`) and the load from the
  table.
- **Changed:** the tag load becomes a load of the next element's handler
  word (`mov 0x28(%r9),%r10`).
- **Element size:** 16 to 24 bytes, so the cursor step becomes
  `add $0x18`.
- **Unchanged:** the frame base, the bounds test and the two moves.

**Halo on the 14900K**
([run 37572318684](https://github.com/Ming-Research/Whitefoot/actions/runs/37572318684)),
time ratio to base:

| kernel | head | twin |
|---|---:|---:|
| `fib` | 1.0000 | 1.0040 |
| `loop` | 1.0045 | 0.9988 |

Neither kernel is more than 2% slower, but Halo's interpreter does not
receive the mechanism, so this shows only that nothing else changed. Its
loop over `Cell` has no handler-word line in the ledger.

The prototype keeps an enum only where the word leaves its size and
alignment within its product layout's. `Cell` (Halo-wf `acb39ad7f`,
`lib/halo/value/module.wfm`) has 187 `u8` and 23 `u32` fields, so its
product layout is 4-byte aligned, and an 8-byte word would raise that.
v2h's `Op` has a `u64` field, so its product layout is already 8-byte
aligned.

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
