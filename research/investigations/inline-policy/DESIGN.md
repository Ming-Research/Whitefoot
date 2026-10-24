# How the compiler decides inlining

Status board item wf-inline-policy. This record holds the question, the
approaches compared, the criteria written before measuring, and the results
as they arrive. It is a study; no compiler behavior has changed yet.

## Question

Whitefoot leaves every inlining decision to LLVM's cost model at `-O2`
(`HOST_OPTIMIZATION_ARGUMENTS` in `compiler/src/driver.rs`). A small function
on a hot path that grows by a few instructions crosses LLVM's threshold, is
no longer inlined, and its callers' loops pay a real call each iteration.

The witness is the standard library's `hash_map_remove` with shrinking
(wf-need-firn-hash-shrink, Whitefoot#436): the shrink test and its call grow
the optimized removal from 75 to 81 instructions, it stops being inlined into
the bench's loop, and remove-insert at a constant size, which never shrinks,
became 4.0%–4.5% slower on a hosted runner and 1.6%–8.5% slower on the
i9-14900K sample (measured in [Whitefoot#436](https://github.com/Ming-Research/Whitefoot/pull/436),
whose record is `research/investigations/hash-shrink/DESIGN.md` on that branch).
Source-level annotations did not recover it there: a 1:2000 branch weight on
the shrink branch, an inline hint or always-inline on the removal, LLVM's
hot/cold splitting before the inliner, and the partial inliner. Only
`-inline-threshold=2000` did (0.995–0.998 against the base).

The questions, from the board item:

1. Root: is the witness an accident of where the threshold falls, or does
   Whitefoot's emitted code put many hot functions near it?
2. Which of at least three approaches is best: global inliner parameters;
   the compiler giving LLVM what it knows (call-site or linkage facts);
   smaller emitted code?
3. What is shared with Whitefoot#420 (branch weights on rarely taken
   directions, wf-req-halo-dispatch-arm-spills-own)?
4. The acceptance numbers: how much code size and compile time may grow.

## Earlier evidence that bounds the design

- A Whitefoot `alwaysinline` policy bounded by pre-expansion IR units was
  measured and withdrawn: it marked 44% of Halo's definitions, grew its text
  by 25.7% and fused unoptimized bodies before their own simplification
  ([in-place-parameters](../in-place-parameters/DESIGN.md#bounded-inlining-measured-outcome-and-withdrawal);
  the storage-placement node rejects it).
- A per-function `function-inline-threshold` attribute cannot raise a
  statically cold call site's threshold of 45: LLVM stops pricing once the
  running cost passes that threshold (same record,
  [withdrawal outcome](../in-place-parameters/DESIGN.md#withdrawal-outcome-downstream-timing-and-the-accessor-attribution)).
- Raising the ThinLTO import limit for fragment builds was rejected on link
  time (board item lm-small-accessor-inline: limit 200 made Halo's fragment
  link 68% slower).
- LLVM 22 at `-O2` uses threshold 225, 325 for an `inlinehint` callee, 45
  for a call site whose static frequency is below 2% of its caller's entry,
  and no locally-hot boost; `-O3` raises the default to 250 and gives a call
  site at least 60 times its caller's entry frequency 525
  (`llvm/lib/Analysis/InlineCost.cpp`, read from source, not measured here).

Read from the compiler source: every ordinary Whitefoot definition is emitted
with external linkage (`Linkage::External` in `compiler/src/backend/emission.rs`).
The default build hands the one module and the runtime's C units to one
`clang -O2` command without LTO, so LLVM must keep every definition and never
grants the bonus for a function's last call site; `--full-lto` and the cached
ThinLTO build internalize at link time instead. Which build a program uses
therefore changes its inline decisions, and the hash-map bench uses the
default build. Whether this matters for the witness is measured below.

## Approaches

- **A. Global inliner parameters.** `-inline-threshold` at 325, 500, 1,000
  and 2,000, and `-O3` in place of `-O2` (threshold 250 and the locally-hot
  call-site threshold). Couples every program to one LLVM release's cost
  constants.
- **B. Facts the compiler knows.** B1: internal linkage for every definition
  nothing outside the program calls, in every build mode, which LLVM rewards
  by itself (last-call bonus, deleting inlined bodies, interprocedural
  passes). The first round approximates it by linking the default build with
  full LTO, which internalizes. B2: call-site attributes from the compiler's
  own knowledge, such as a call in a loop to a small callee, studied after
  the census shows which sites matter.
- **C. Smaller emitted code.** Per-instruction inline cost of the witness's
  remaining call, to find what Whitefoot's lowering adds that the source does
  not need.

## Round 1: census and sweep (hosted runners)

Instrument: `research/experiments/inline-policy/` (README there). The temporary
workflow builds this branch's compiler with the shrink library of
`claude/hash-shrink`; every arm is a copy of that one binary with a clang
wrapper beside it, so only the clang arguments differ.

Census, today's settings: every inline decision LLVM reports for the hash-map
bench (with and without shrinking), Halo's bench (full LTO), the wasm
interpreter v2h (cached ThinLTO), Firn (cached ThinLTO, if it compiles with
this compiler) and the standalone programs under `tests/programs/`. Reported
per program: decisions by reason, thresholds met, priced sites by cost over
threshold, and the sites within 25% above or 20% below the threshold. The
witness's remaining call is priced instruction by instruction with
`print<inline-cost>` on the optimized bench module.

Sweep, each arm against today's settings: text size and compile time
(two interleaved repeats) of the bench, Halo, v2h, Firn and the corpus total;
the HashMap cells at 1,000, 10,000 and 1,000,000 pairs and Halo's fib and
CoreMark, through `research/experiments/compiler-compare/run.sh`, with a twin
of the base image as the noise control.

Written before measuring:

- An arm recovers the witness when remove-insert's candidate/base interval
  lies wholly below 1 at 1,000 and 10,000 pairs. One that does not is not a
  fix for this item, whatever else it does.
- An arm is rejected as a global setting when any HashMap, fib or CoreMark
  cell is judged slower.
- Hosted timing is exploratory: no floor is given, the runners differ, and a
  cell's verdict here selects what the i9-14900K measures, not the
  acceptance.
- Code size and compile time have no bound yet. The design sets one from
  this round with a stated ground; this round only reports them.
- If the census finds few Whitefoot sites near the threshold outside the
  witness, the root is the threshold's position, and the answer leans to
  B or C for the witness; if many hot sites sit near it, a global or
  linkage change is preferred over per-site marking.

### Results

Pending the first run.
