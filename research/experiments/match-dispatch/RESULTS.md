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

## Limitations

- One core type. Silverfir-nano's figures were taken on an M4, and its
  1.09-cycle floor on a synthetic four-instruction handler chain is not this
  experiment's `floor`; the two are not compared here. The comparison with
  Silverfir-nano is the later wasm interpreter running CoreMark.
- The interpreter has no accumulator and no register-resident locals, so
  five of the six kernels are bound by values passing through frame memory
  (about four cycles per dispatch whatever the shape), which compresses the
  differences between shapes; an interpreter that removes those round trips
  moves toward the `floor` kernel, where the differences are larger.
- Each launch draws a different address-space layout; the median over
  launches and the null comparison bound that effect at about 1% here, well
  below the effects reported, but a variant's layout draw is not separated
  from its code.
