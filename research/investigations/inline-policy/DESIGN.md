# How the compiler decides inlining

Status board item wf-inline-policy. This record holds the question, the
approaches compared, the criteria written before measuring, and the results
as they arrive, and the design they select.

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
ones) cost between the threshold and 1.44 times it (almost all of them the C
runtime's, as round 3 corrects), and Halo's
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

[Run 40099834955](https://github.com/Ming-Research/Whitefoot/actions/runs/40099834955)
on commit `a964e5e88` measured the hint: the wrapper rewrote the bench's
piped module and Halo's full-LTO module, and the hinted callees meet 325
(the bench's census in the hint arm: 154 sites at 325, 982 at 225, the rest
the runtime's C and the compiler-owned `ordinary_values` unit, which keep
LLVM's defaults). Against today's settings with shrinking:

| Arm (runner) | bench text | Halo text | v2h text | corpus text | bench / Halo / v2h compile | remove-insert 1,000 / 10,000 / 1,000,000 | fib | cells judged slower |
| --- | ---: | ---: | ---: | ---: | --- | --- | --- | --- |
| hint (Xeon 8573C) | 1.0075 | 1.0526 | 1.0156 | 1.0003 | 1.003 / 1.036 / 1.021 | 0.960 [0.947, 0.971] / 0.970 [0.960, 0.983] / 0.964 ? | 0.897 [0.893, 0.908] | none |
| hint with full LTO (EPYC 9V74) | 0.410 | 1.0526 | 1.0156 | 0.400 | 0.903 / 1.032 / 1.002 | 0.936 / 0.944 / 0.879 | 0.933 | iterate 1.061, 1.046, 1.029; lookup-miss 1,000 1.035 |

The hint arm also made iterate faster at all three sizes (0.754, 0.787,
0.944) and remove at all three (0.944, 0.982, 0.983), and changed no memory
cell. Against today's settings without shrinking (EPYC 9V45): remove-insert
0.986 [0.975, 0.996], 0.991 [0.975, 0.997] and 0.980 [0.963, 0.994], fib
0.907 (unresolved), and draining 1.85, 1.59 and 1.28 times slower, shrinking's
own rebuilding as in round 2.

**Correction to round 2's census.** The census counts a call site as one
between Whitefoot definitions when both names start with `wf`, which also
admits the compiler-owned C runtime (`wf_pool_take`, `wf_timer_remove`) that
every program compiles. Split by origin (remarks from the C units carry a
`.c` location; in Halo's full-LTO records, Whitefoot source definitions are
the qualified or instantiated names), the sites kept at 225 that cost at most
325 are:

| Program | Whitefoot definitions | compiler-owned C runtime |
| --- | ---: | ---: |
| bench (shrinking) | 29 | 133 |
| Halo | 356 | 95 |
| v2h | 45 | 133 |
| corpus (97 programs) | 49 | 12,901 |

So the corpus's 12,975 sites are almost all the runtime's C, counted once per
program, and threshold 325's 7.3% corpus growth is mostly the runtime's. The
programs that are mostly Whitefoot carry the pattern: Halo has 356 of its
2,719 kept Whitefoot sites within 1.44 times the threshold.

Reading of round 3:

- The hint meets every criterion written before measuring: both witness
  intervals lie wholly below 1, fib improves, no cell is judged slower, and
  against the library before shrinking no remove-insert interval lies above
  1. It recovers less of the witness on its runner than 325 did on another
  model (0.96–0.97 against 0.92) and grows the code far less (bench 0.75%
  against 7.4%, corpus 0.03% against 7.3%, Halo 5.3% against 6.4%), because it
  leaves the runtime's C at LLVM's defaults; that this explains the
  difference in the witness is deduced, not measured.
- The hint composes with full LTO as 325 did. Full LTO's iterate slowdown
  repeats a third time on the EPYC 9V74 and never on the other models; it
  belongs to the linkage question, not to the hint.

## Design

The compiler gives every Whitefoot definition it emits LLVM's `inlinehint`
attribute, in the attribute group all of them carry, and nothing else
changes: the compiler-owned C runtime and `ordinary_values` keep LLVM's
defaults, a statically cold call site keeps its threshold of 45, and LLVM
still prices every call.

Grounds:

- Whitefoot has no inline annotation, and should not need one: a writer
  cannot know where LLVM's cost model places a cliff, and the witness shows
  the source-level forms (branch weights, a hint or always-inline on one
  function) only move the boundary one caller out. C++ and Rust writers
  mark their small hot functions `inline` and `#[inline]`, and LLVM's hint
  level is calibrated for that use; a Whitefoot definition is the
  equivalent of such a function, a whole-program unit whose body is always
  visible to its callers.
- The hint is an IR attribute of the definition, so the cached ThinLTO build,
  the fragment build and full LTO all see it without an option on each
  compile and link command, and the runtime's C, which is ordinary C, is
  priced as C.
- It is LLVM's own level, not a number chosen from the witness: rounds 1
  and 2 showed that a threshold picked to recover the witness is a position
  on a cliff (400 recovered less than 325).
- It is not the withdrawn always-inline policy (storage-placement node): the
  hint keeps LLVM's pricing after each callee's own simplification and
  marks no subset by a size budget.

Not chosen:

- A global `-inline-threshold` of 325: the same gain for Whitefoot code, but
  it also raises the runtime's C (corpus text +7.3% instead of +0.03%) and
  must reach every compile and LTO link as an `-mllvm` option.
- Thresholds above 325, and `-O3`: Halo's text grows by 28.8% at 500 and by
  109% and 318% at 1,000 and 2,000; 400 recovers less of the witness than
  325; `-O3` does not recover it.
- Call-site selection by the compiler (approach B2), such as always-inline
  for a small callee called in a loop: it needs a size bound with no
  structural ground, which is why the always-inline policy was withdrawn,
  and LLVM already reads loop frequency at every site. Not measured.
- Smaller emitted code (approach C): the witness's priced instructions are
  hashing, probing and comparing, nothing Whitefoot's lowering adds
  (round 1), so there is nothing to remove for it.
- Branch weights on rarely taken directions (Whitefoot#420): measured 1.050–1.056
  on the witness, because a cold site's threshold of 45 stops the lookup
  from being inlined. Weights and the hint are separate decisions; the hint
  does not lift the cold threshold, so their interaction is that item's
  measurement, not this one's.

Separate from this design, recorded for its own board item
(wf-internal-linkage): internal linkage for the default build (approach B1), which full LTO approximates, makes the
bench's and the corpus's text 56%–60% smaller and the witness faster, but
slows iterate by 3%–6% on one runner model in three runs out of three; it
needs that slowdown attributed before a decision.

Acceptance numbers, against the same compiler without the hint and the same
standard library: Halo's text grows by at most 6% and the corpus's total by
at most 1%; Halo's and v2h's compile times grow by at most 5% in interleaved
pairs on one hosted runner. Ground: the hint arm measured 5.3%, 0.03%, 3.6%
and 2.1%, and threshold 325, the cheaper global alternative, grows Halo's
text by 6.4%; a larger growth means the implementation is not the design
measured here. The timing acceptance is the board item's, on the
i9-14900K.

## Round 4: the implemented hint (hosted runners)

Written before measuring. The compiler now emits the hint itself
(`emit_module` in `compiler/src/backend/emitter.rs`). The temporary workflow
compares this branch's compiler with its merge base's, both with the
shrinking library, and builds the bench a third time with the merge base's
compiler and round 3's hint arm.

- The implementation is the measured design when the module it emits for
  the bench is the merge base's with `inlinehint` added to attribute group
  #0, which is the rewrite the hint arm made, and the bench's code differs
  from the merge base's. A first version of this check compared the linked
  code with the hint arm's and found it different
  ([run 40114031344](https://github.com/Ming-Research/Whitefoot/actions/runs/40114031344));
  the arm hands clang a named file where whitefootc pipes the module, so the
  modules are compared instead, and the code's difference is reported.
- The acceptance bounds of the design apply: Halo's text grows by at most 6%
  and the corpus's total by at most 1%, and Halo's and v2h's median compile
  times over five interleaved pairs by at most 5%.
- The HashMap, fib and CoreMark cells repeat round 3's hint arm as a hosted
  screen; the timing acceptance is the i9-14900K's.


### Results

Two runs on AMD EPYC 7763 hosted runners measured the same compiler change:
[run 40115068629](https://github.com/Ming-Research/Whitefoot/actions/runs/40115068629)
on commit `9a5ba9455` and
[run 40833295931](https://github.com/Ming-Research/Whitefoot/actions/runs/40833295931)
on commit `f356a7c59`, which changed only `design/log.md`. Both compare with
the merge base `05e4c58eb`.

- The emitted bench module is the merge base's with `inlinehint` added to
  group #0, byte for byte, so the implementation is the design round 3
  measured. The linked code differs from the merge base's, and is as large as
  the hint arm's (197,474 bytes of text) with different bytes.
- Text: bench 1.0075, Halo 1.0526, v2h 1.0156, corpus 1.0003 (97 programs),
  the same as round 3's hint arm. Median compile time over five interleaved
  pairs: Halo 1.027 and 1.014, v2h 1.007 and 1.018, bench 1.011 and 0.998.
  Every acceptance bound holds.
- Cells, both runs: remove-insert 0.925, 0.922, 0.864 and 0.923, 0.924, 0.844,
  every interval wholly below 1; fib 0.880 and 0.880; CoreMark unresolved and
  then 0.964. One cell is judged slower in both runs: iterate at 1,000,000,
  1.0038 [1.0003, 1.0073] and 1.0046 [1.0014, 1.0087], with twin intervals
  holding 1, where round 3's hint arm made it 0.944 on a Xeon 8573C. No floor
  was given, so the verdicts decide nothing; the i9-14900K run compares that
  cell with its floor, and a regression there needs the owner's card.

## Round 5: the timing acceptance (i9-14900K)

Written before measuring. Every side is built from the commit to merge (or
one whose compiler sources equal it), against its merge base with `main`.

- **Hash-map cells.** `compiler-compare.yml` on the 14900K, group
  `hash_map` only, CPU 2, `floor` set to the hash-map floor recorded for the
  14900K (21 cells, twin spread 0.5%–10.6%). Two pairs:
  1. inline: candidate = the commit with the shrinking library of
     wf-need-firn-hash-shrink, base = its merge base with the same library.
     Accepted when remove-insert's interval lies wholly below 1 at 1,000,
     10,000 and 1,000,000. Any cell wholly above 1 with its noise at or under
     the floor is a regression of the design (iterate at 1,000,000 is the
     cell round 4's hosted runs flagged at +0.4%), and needs the owner's
     card stating its size before the change merges.
  2. against the library before shrinking: candidate as in 1, base = the
     merge base with `main`'s library. Accepted when no remove-insert
     interval lies wholly above 1; other cells are reported, since shrinking
     itself changes them.
  Each run takes about five minutes on the 14900K (the earlier full-default
  run of this workflow there took five). The interpreters group is left out:
  its wall timer resolves only 0.1 s (compare-perf-wall-time), so its floor
  does not count until compare-timer-resolution fixes it.
- **Firn.** `redis-bench.yml` compare mode on the 14900K, Firn built by the
  candidate's compiler and by the merge base's, everything else the same:
  redis-benchmark's default tests at depths 1 and 16, 16 passes each, the
  Firn floor recorded for that mode. Accepted when no test's interval lies
  wholly above 1 with the twin's interval holding 1. It runs once Firn
  compiles with a compiler of `main` (firn-pin-wf-main).
