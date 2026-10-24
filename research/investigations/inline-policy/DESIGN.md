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

[Run 40091914632](https://github.com/Ming-Research/Whitefoot/actions/runs/40091914632)
on commit `b8fb5a166`, GitHub-hosted runners (AMD EPYC 7763, 9V45 and 9V74;
each arm ran on one runner, named below), clang 22. Every arm's remarks
show its setting reached the decisions: the thresholds met moved to the
arm's value in the default build and in Halo's full-LTO link. Firn's main
line (`7cf12df4c`) does not compile with this compiler (its Halo dependency
names a binding `held`, now a reserved word), so it is left out; 97 of the
103 standalone programs under `tests/programs/` compiled.

**The witness.** In the default build of the bench with today's settings, the
unshrunk removal's entry is inlined into the timed loop at cost 205 against
225. With shrinking, LLVM first inlines `hash_map_shrink` (cost 90, the
wrapper that tests the load and calls the rebuild) into the removal's body,
and the removal then prices at 310 against 225 at all three of its call
sites and stays out of line; `hash_map_rebuild` stays out of line in both.
The removal was already at 91% of the threshold before shrinking. The
instructions LLVM charges in the optimized removal are its own hash, probe
and compare work (loads, compares, branches, shifts and multiplies of the
hash); `print<inline-cost>` shows no checks or wrappers that the source does
not ask for. So approach C has nothing to remove in the witness.

**Halo.** `native_binding`, the guard on the Lua call path that
wf-req-halo-dispatch-arm-spills-own names, is not inlined into `prepare` (two
call sites), `collect_if_due` or `cjson_reclaim`: cost 310 against 225 in the
full-LTO link. This confirms from LLVM's own remarks the attribution that
board item read from IR.

**Census.** The census of today's settings counts call sites between
Whitefoot definitions; its counts are given with round 2's, which priced
every site in full.

**Sweep.** Text size and compile time (median of two interleaved compiles)
of each arm against today's settings:

| Arm (runner) | bench text | Halo text | v2h text | corpus text | bench compile | Halo compile | v2h compile |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| threshold 325 (7763) | 1.074 | 1.064 | 1.055 | 1.073 | 1.065 | 1.035 | 1.029 |
| threshold 500 (9V45) | 1.159 | 1.288 | 1.115 | 1.159 | 1.120 | 1.124 | 1.016 |
| threshold 1000 (7763) | 1.546 | 2.091 | 1.419 | 1.493 | 1.473 | 1.388 | 1.127 |
| threshold 2000 (9V74) | 1.833 | 4.178 | 1.644 | 1.762 | 1.667 | 2.236 | 1.152 |
| `-O3` (7763) | 1.104 | 1.092 | 1.066 | 1.090 | 1.062 | 1.033 | 1.014 |
| full LTO for the default build (9V74) | 0.412 | 1.000 | 1.000 | 0.400 | 0.885 | 0.981 | 1.005 |

Timing, candidate/base ratio with its 95% interval (no floor: verdicts are
exploratory; `none` is no detected change, `?` unresolved after 20 rounds):

| Arm | remove-insert 1,000 | remove-insert 10,000 | remove-insert 1,000,000 | fib | CoreMark | other cells judged slower |
| --- | --- | --- | --- | --- | --- | --- |
| 325 | 0.927 [0.923, 0.930] | 0.922 [0.912, 0.935] | 0.844 ? | 0.882 [0.875, 0.886] | ? | iterate 1,000,000 1.003 |
| 500 | 1.012 none | 1.010 [1.000, 1.021] | 0.971 [0.966, 0.990] | 0.885 ? | ? | insert-presized 1,000,000, insert-growing 1,000, remove 1,000 and 10,000 |
| 1000 | 0.970 [0.957, 0.974] | 0.959 [0.948, 0.969] | 0.869 [0.849, 0.883] | 0.881 [0.879, 0.909] | ? | none |
| 2000 | 0.959 [0.949, 0.965] | 0.961 [0.958, 0.966] | 0.900 [0.888, 0.923] | 0.995 [0.995, 0.999] | none | iterate 1,000,000 1.023 |
| `-O3` | none | none | ? | 0.966 [0.959, 0.970] | ? | iterate 1,000 and 1,000,000 |
| full LTO | 0.984 [0.977, 0.988] | 0.979 [0.976, 0.992] | 0.980 [0.968, 0.997] | none | none | iterate at all three sizes, 1.03–1.06 |

Halo and v2h are built the same way in the full-LTO arm (Halo already with
full LTO, v2h with cached ThinLTO), and both their images and their timing
are unchanged there, as they must be.

Reading of round 1, against the criteria written before it:

- Thresholds 325, 1,000 and 2,000 recover the witness (both intervals wholly
  below 1); 500 does not, on a different runner model, which the hosted
  noise does not let this round separate from a real difference. `-O3` does
  not, although 36 of the bench's sites met its locally-hot threshold of 525;
  which sites those are this round did not report. Full LTO
  alone moves remove-insert about 2%, not the 4%–4.5% that shrinking cost.
- Code size grows steeply above 500: Halo's text doubles at 1,000 and
  quadruples at 2,000, so those are rejected as a global setting whatever
  their timing.
- Threshold 325 gives the largest single gain seen, Halo fib 12% faster,
  consistent with `native_binding` (cost 310) being inlined at 325 and not
  at 225; that attribution is deduced, and round 2 checks it in the remarks.
- Linking the default build with full LTO, which internalizes every symbol
  but the entry points, cuts the default build's text by 59% and its compile
  time by 12% here: every definition being external keeps the whole
  standard library and runtime in the image. It is independent of the
  threshold question and a candidate on its own, but the iterate cells it
  slows need attribution first.

## Round 2: full-cost census and the threshold's neighbourhood (hosted runners)

Written before measuring. The census repeats with `-inline-cost-full`, which
prices every site completely without changing a decision, and reports
Halo's `native_binding` sites. The sweep compares, each on its own runner:
today's settings against themselves (how often this hosted comparison judges
a cell when nothing differs); thresholds 275, 325 and 400; full LTO with
threshold 325 against today's settings; and threshold 325 with shrinking
against today's settings without shrinking, which the acceptance's second
comparison needs.

- Threshold 325 is confirmed when it again recovers the witness and fib
  improves; the same-settings arm sets how many judged cells an arm may show
  and still count as no regression on hosted runners.
- If 275 already recovers the witness and fib with less growth, the lowest
  threshold that does is preferred; the choice still needs a structural
  ground, not only the witness, so the full-cost census's distribution near
  225 decides how much a threshold moves across the corpus.
- The comparison with the library before shrinking passes when no
  remove-insert interval lies wholly above 1.

### Results

[Run 40094784937](https://github.com/Ming-Research/Whitefoot/actions/runs/40094784937)
on commit `68e63dfa8`. The same-settings arm (Intel Xeon Platinum 8573C) judged
none of its 23 cells slower or faster and produced byte-identical images, so
on a hosted runner a judged cell is not a frequent false alarm; the arms
below ran on AMD EPYC 7763 except full LTO with threshold 325 (9V45).

**Full-cost census.** `-inline-cost-full` changed no count of the default
build's or the cached build's census: with remarks requested, LLVM already
prices those sites in full. It changed Halo's full-LTO census, whose round 1
costs were truncated. Kept call sites between Whitefoot definitions, with
ordinary thresholds (cold sites, at 45, apart), and how many a higher base
threshold would admit at their first pricing (the bonuses LLVM adds scale
with the base):

| Program | decisions | kept, priced | admitted at 275 | at 325 | at 400 | kept cold sites |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| bench (shrinking) | 2,136 | 819 | 66 | 162 | 189 | 58 |
| Halo | 7,683 | 6,572 | 350 | 452 | 706 | 1,419 |
| v2h | 3,636 | 1,209 | 94 | 178 | 222 | 226 |
| corpus (97 programs) | 187,255 | 71,673 | 4,979 | 12,975 | 15,477 | 6,314 |

So the witness is not alone: in the corpus 12,975 kept sites (18% of the kept
ones) cost between the threshold and 1.44 times it, and Halo's
`native_binding` is again 310 against 225 at its four sites. These are
static counts, the first pricing of each site; admitting one changes the
pricing of its callers.

**Sweep.** Against today's settings with shrinking:

| Arm | bench text | Halo text | corpus text | Halo compile | remove-insert 1,000 / 10,000 / 1,000,000 | fib | cells judged slower |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| threshold 275 | 1.036 | 1.044 | 1.031 | 1.023 | none / none / ? | 0.968 ? | lookup-miss 1,000 |
| threshold 325 | 1.074 | 1.064 | 1.073 | 1.033 | 0.923 / 0.923 / 0.862 | 0.884 [0.881, 0.886] | iterate 1,000,000 1.003 |
| threshold 400 | 1.115 | 1.189 | 1.120 | 1.081 | 0.989 / 0.995 / ? | 0.866 [0.857, 0.869] | remove 1,000 and 10,000; iterate 1,000,000 |
| full LTO, threshold 325 | 0.442 | 1.064 | 0.437 | 1.072 | 0.969 / 0.959 / 0.914 | 0.897 ? | lookup-hit 1,000 |

Threshold 325 with shrinking against today's settings without shrinking:
remove-insert 0.972 [0.966, 0.985], 0.974 [0.968, 0.979] and 0.964
[0.946, 0.985], fib 0.885, and draining a map to empty 1.96, 1.54 and 1.32
times slower at the three sizes, which is shrinking's own rebuilding work,
accepted on card hash-shrink-cold-path and not an inlining effect.

Reading of round 2:

- Threshold 325 repeats round 1 on the same runner model: the witness
  recovers past the library before shrinking, and fib is 12% faster. 275
  does not recover the witness; 400 recovers less of it than 325 and slows
  draining, while fib gains slightly more. The response is not monotonic
  in the threshold: admitting more sites moves the cliff to the callers'
  callers. A threshold chosen by the witness is a position on that cliff,
  which the next library change can move again; the number needs a ground
  of its own.
- Full LTO for the default build keeps its 56%–58% smaller text with
  threshold 325 and the witness recovers there too; its round 1 iterate
  slowdown does not repeat on this runner model.

## Round 3: the hint level for Whitefoot's own definitions (hosted runners)

Written before measuring. 325 is LLVM's own `inlinehint` threshold, the
level clang gives a C++ function declared `inline` and rustc a Rust
`#[inline]` function. Whitefoot has no inline annotation, so the compiler
could state the hint itself for every source definition, with the IR
attribute `inlinehint` instead of an LLVM command-line option: that keeps
the compiler-owned C runtime at LLVM's defaults, keeps the cold-site cap of
45 (a hint does not lift it), and travels through every build mode,
fragments included, without `-mllvm` plumbing into each link. The arm adds
the attribute to the attribute group every emitted definition carries.

- The hint arm is preferred over a global threshold when, against today's
  settings, it recovers the witness (both remove-insert intervals wholly
  below 1) and fib improves with no cell judged slower beyond the 0.3%
  iterate cell 325 already shows; it is compared with 325 by text size and
  compile time.
- Against today's settings without shrinking, no remove-insert interval
  lies wholly above 1.
- Hint with full LTO is measured to see whether the two compose as 325 and
  full LTO did.

### Results

[Run 40097435146](https://github.com/Ming-Research/Whitefoot/actions/runs/40097435146)
on commit `66a13639e` measured nothing about the hint: whitefootc pipes the
emitted module to clang as `-x ir -`, and the wrapper rewrote only named
module files, so both hint arms built byte-identical images to their bases
(the thresholds met did not move). The wrapper now rewrites the piped module
and the fragments, logs each rewrite, and the workflow fails a hint arm that
rewrote nothing. One observation stands: full LTO for the default build
again slowed iterate by 3.3%–4.5% at all three sizes on an AMD EPYC 9V74,
as in round 1 on the same model, while on a 9V45 (round 2) it did not.
