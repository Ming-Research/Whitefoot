# Call results read at their source

This investigation is written before any implementation or measurement of
the mechanism it proposes. It supersedes the first draft of the same
mechanism, the "result-origin contract" in
[in-place-parameters](../in-place-parameters/DESIGN.md#snowghosts-accessors-real-shapes-and-the-result-origin-contract),
whose evidence (the extracted accessors, their callers and the hand-edited
witnesses) stays there and is cited here. What changed from that draft, and
why, is listed under [Changes from the first draft](#changes-from-the-first-draft).

## Question

Can a call whose by-value record result is a copy of storage the caller can
already reach be lowered so that the caller reads the fields it uses from
that storage, with no source syntax, no change to the specification, no
change to the host toolchain, and no dependence of a caller's emitted
fragment on its callee's body; and does that recover, on the i9-14900K, a
material share of the Snowghost edit-cell slowdown that followed field-wise
record transfers, without slowing Halo?

## What the regression is

On 2026-10-11 Whitefoot replaced whole-record copies of stored structs up to
16 granules with per-field loads and stores
([storage-placement](../../../design/compiler/storage-placement.md), the
bounded-transfer decision), because wide copies of separately stored fields
caused store-forwarding stalls. Each granule now counts as one load and one
store in LLVM's summaries. Snowghost's small accessors that index an element
and return it by value grew past ThinLTO's import limit of 100 summary
instructions: `computed_of` 119, `font_of` 104, `box_of` 107, `text_of` 109
([extracted witnesses](../../experiments/small-accessor-inline/README.md#snowghost-extracted-witnesses)).
Under `--cache --fragments function`, where each function is its own ThinLTO
fragment, a callee can be inlined only after the link imports it, so these
four stopped being inlined across fragments; their direct call sites in
`layout_oracle` rose from 4 to 160.

The cost, against `wf-78223721f77d` (the release before field-wise
transfers), on the 14900K (Snowghost-wf run 38197250440, two rounds; the
newer side is `wf-d837678c6905`), as the ratio S of median edit times:

| Cell | Reference µs | Newer µs | S | 1 − 1/S |
| --- | ---: | ---: | ---: | ---: |
| sequential ecma262 word | 52.75 | 55.5 | 1.052 | 0.049 |
| sequential html5 word | 37.75 | 40.5 | 1.073 | 0.068 |
| sequential apollo11 word | 109.5 | 119.5 | 1.091 | 0.083 |
| four-worker ecma262 word | 54.75 | 62.5 | 1.142 | 0.124 |
| four-worker html5 word | 40.25 | 45.0 | 1.118 | 0.105 |
| four-worker apollo11 word | 118.5 | 130.0 | 1.097 | 0.088 |
| sequential apollo11 font-size | 109.5 | 125.0 | 1.142 | 0.124 |

Reference values are the mean of the four reference medians (base and its
control build, two rounds), newer values the mean of two. The two sides
built different Snowghost sources (`903dafc` and `ad94bfc`), because the
older release does not build the newer source; S therefore also contains
any effect of that source difference, which is not separated.

Callgrind on a hosted runner (Snowghost-wf run 38183553516, sequential
apollo11 word edit, first four edits, collection toggled on
`wf_layout.text_changed` and `wf_layout.update`) counts 12,257,362 against
13,518,149 instructions, +1,260,787. The growth sits in the accessors'
self instructions: `computed_of` +622,811, `font_of` +271,096, `read_item`
+163,507, `write_item` +154,761, `text_of` +135,162, `next_glyphs` +125,109,
`box_of` +54,624, with their former callers shrinking.

## The rejected first arm: a larger import limit

The owner first chose to raise the fragment link's ThinLTO import limit.
At 200 it restored the inlining but grew Halo's fragment link by 68%; at 150
it grew Halo's link by 48% on a hosted runner
([probe results](../in-place-parameters/DESIGN.md#step-1-at-150-halos-link-still-fails-the-bound))
and by 45% on the 14900K (720 ms to 1,044 ms, Whitefoot run 38322491672),
against a bound of 25%. In that 14900K run (7 rounds, Snowghost-wf
`c8f5e17`, base, twin and limit-150 compilers interleaved), the candidate
over base median ratio r of the seven focus cells was:

| Cell | r (range) | Twin/base (range) | Beyond twin range | Recovered share (1 − r)/(1 − 1/S) |
| --- | --- | --- | --- | ---: |
| sequential ecma262 word | 0.982 (0.965–1.000) | 1.000 (0.982–1.018) | no | 36% |
| sequential html5 word | 0.976 (0.952–1.000) | 1.000 (0.952–1.025) | no | 35% |
| sequential apollo11 word | 0.983 (0.967–1.000) | 1.000 (0.967–1.017) | no | 20% |
| four-worker ecma262 word | 0.967 (0.935–0.983) | 0.983 (0.952–1.017) | no | 27% |
| four-worker html5 word | 1.000 (0.977–1.048) | 1.023 (1.000–1.047) | no | 0% |
| four-worker apollo11 word | 0.969 (0.953–1.008) | 0.984 (0.977–1.016) | yes | 35% |
| sequential apollo11 font-size | 0.913 (0.898–1.455) | 1.000 (0.984–1.024) | yes | 70% |

The word cells moved about 2%, only one of six beyond the twin's range;
their median recovered share is 31%, but a twin ratio 2% from 1 moves one
cell's share by 20 to 40 points, so single-cell shares are noise-limited.
At 150 the link imported `font_of`, nearly all `box_of`, `text_of` and
`read_item`, but not `computed_of`, whose direct call sites rose from 88 to
135 because its callers were inlined into more fragments while it, at 119
instructions, presumably exceeds the decayed second-level limit (150 × 0.7 =
105) there; the per-edge imports were not recorded.
Both limits were rejected; the owner then chose this direction.

## Proposal: the result-source contract

### What the language already fixes

`let v = f(p)` binds a value: the value of the result when `f` returns. The
language observes no address [STOR-7], returns no reference [REF-3], and
permits the compiler to use proved facts for optimization forms [DIAG-2].
Where that value is a copy of a place that stays unchanged until every use
of `v`, reading the place is indistinguishable from reading a copy. The
contract below is storage placement under these rules; it adds no source
form and changes no rule, verdict or conformance case.

### Two entries for every destination-form definition

Today a definition whose result does not fit return registers
([result-registers](../../../design/compiler/result-registers.md)) takes a
caller destination `%wf.result` and writes the result there. Under the
contract every such definition, WF-defined or linked, also gets an internal
entry `<symbol>.source` with the same parameters that returns a `ptr`: the
address that holds the result. (The first draft called it `.origin`; this
record says "source" throughout.)

- A WF body's source entry returns, on a path whose result is a by-value
  Load snapshot of a place under one of the function's reference parameters,
  that place's address, provided the existing read-through stability
  judgment of the snapshot analysis
  (`select_read_through`/`source_survives` in
  `compiler/src/backend/storage/snapshots.rs`), applied with the function's
  exit as the use, proves the place unchanged and valid from the read to
  the exit, the exit's drops and cleanup included. Every other path builds
  the result in `%wf.result` and returns `%wf.result`. Paths join as a `phi`
  of addresses.
- A path that returns another call's result returns that call's source
  address only when the inner call received this function's own
  `%wf.result` as its destination and the inner callee's source set (below),
  substituted at that call, lies under this function's reference
  parameters and is stable to exit. This covers `item_at` forwarding
  `read_item`.
- Never callee-owned storage: locals, by-value parameters, frames and
  anything released at return are excluded by construction, since only
  places under reference parameters qualify.
- Waiting definitions [WAIT-1] and definitions whose source path would
  cross a split dispatch part or an overlapping execution group return
  `%wf.result` on every path. The entry still exists, so its existence
  depends on the signature alone.
- A linked definition's source entry calls its C body into `%wf.result`
  and returns `%wf.result`. This keeps one callable ABI for WF and linked
  bodies with the same signature (the compiler root decision), as the
  register-result entries already do for linked bodies.
- The public symbol keeps today's destination ABI. Its body calls
  `<symbol>.source` and, unless the returned address equals `%wf.result`,
  transfers from it into `%wf.result` with the ordinary granule transfer,
  every source granule captured before any store, which the permitted
  result/input overlap already requires.

### Caller side: signature facts only

At a direct call of a destination-form callee, the **source set** is every
place the callee's row names, read or written, after EFF-5 substitution of
the actual arguments, plus the call's destination slot. It is computed from
the callee's signature; the caller never consults the callee's body.

The caller calls `<symbol>.source` and reads through the returned address
when both hold:

1. every use of the result is a field read, or an existing eligible
   read-through consumer (an immutable by-value argument the snapshot rules
   already admit, such as `glyph_allowed(face, item)`);
2. checked effects and IR place facts prove every place of the source set
   unchanged and valid from the call through each use, with the same
   invalidating events Load snapshots use: possibly aliasing stores,
   possibly aliasing drops and releases [STOR-3], owner moves, window
   operations [OP-10] such as `grow`, and calls that may write an aliasing
   argument, as REF-2 lists them.

Otherwise the caller calls the public symbol, which behaves as today. A
caller never captures a copy of a source-entry result: when the callee
read from a source, a capture is the same one copy the callee makes today,
and when the callee built into the destination, it is an extra copy, so a
capture never saves anything and moves code into callers (the shape-buffer
witness grew `next_glyphs` from 29 to 80 summary instructions and
`shift_forward` from 49 to 100 for exactly this).

Indirect calls through function values, parallel-lane thunks, calls from
waiting functions and calls from overlapping groups in the parallel world
always use the public symbol. A caller whose row only reads every root of
the source set (the case of all 12 `computed_of` callers and all 5
`font_of` callers) gets condition 2 for its whole activation from OWN-9,
with no per-use analysis.

### Why it is sound

Let the callee read place P under reference parameter q and return P's
address. By EFF-2 the callee's row covers that read, so P lies under the
path the row names for q, and after EFF-5 substitution P lies under a place
of the caller's source set. The callee-side judgment makes P hold the
returned value at the callee's exit. The caller-side judgment makes every
place of the source set, P included, unchanged and valid from the call to
each use. Each read through the address therefore yields the field of the
value at return, which is what the by-value result denotes. A path that
built into the destination returns the caller's own slot, which the caller
does not reuse before the last use. Because the caller reads through only
when every use qualifies, no use ever sees a mix of a caller copy and the
source.

Concurrency adds one condition. During the callee, OWN-9 and PAR-1 make
every place the callee reads through a reference read-only to other code.
After the return, the caller holds the reference arguments it passed, so
storage under them can be written only by the caller's own statements and
calls, which condition 2 examines. The end of an atomic statement's hold
must invalidate every source under the held state. The snapshot analysis
treats a call without a source signature as invalidating every root; whether
the hold's end is lowered as such a call is unverified, and a compiler test
must show that reading through stops there (below). Overlapping groups in
the parallel world are excluded on both sides, as for Load snapshots.

### Fragment builds

This is the requirement the import-limit arm could not meet without a
global cost: the caller cannot see the callee's body, and its fragment must
not depend on it.

- The caller's fragment declares `<symbol>.source` from the callee's
  signature. Whether a callee has a source entry depends only on its result
  using a destination, and the source set only on its row. The
  [fragment splitter](../../../compiler/src/backend/fragments.rs) already
  requires a fragment's text to be a function of its own definitions and of
  the signatures they name; the contract keeps that, so a callee body edit
  leaves its callers' fragments, and their cached objects, unchanged
  ([incremental-compilation](../../../design/compiler/incremental-compilation.md)).
- The callee's fragment contains both entries. The source entry carries no
  result transfer on its source paths, so its ThinLTO summary is small:
  `computed_of` 28, `font_of` 29, `box_of` 68 in the hand-edited witnesses,
  under the first-level limit of 100 and, for the first two, the
  second-level 70 and third-level 49. The link then imports and inlines it
  as it does any small function; no import or inline setting changes.
- Where the source entry is not imported (a larger body, or a deeper import
  level), the out-of-line call still returns an address and the caller reads
  one field through it: the 48-byte store and reload disappear even without
  inlining. In the fragment assembly, `font_index`'s element path falls from
  34 to 24 instructions.
- Whole-program and full-LTO builds emit the same entries and calls. There
  is one lowering path; the host's ordinary inlining then removes the calls
  as it did before field-wise transfers.

### Backend facts

The source entry's returned address is `nonnull` and aligned to the result
type, and never `noalias`, since it aliases the source. It is not marked
`dereferenceable`: a caller may release the source after its last use, and
an extent that the host may treat as holding beyond the point of return
could license a speculative load after that release. A reference parameter
keeps `captures(none)` when every returned source lies behind an owner
pointer loaded from its referent, as for all seven accessors (Box payloads
and window elements), since the returned provenance is then the loaded
pointer's; a parameter whose own referent may be returned gets
`captures(ret: address, provenance)`, and no capture attribute where the
probed host spelling has no return-only form
([backend-facts](../../../design/compiler/backend-facts.md), the
no-capture probe). Loads through a source address carry the field's
ordinary type metadata and no readonly-owner access scope.

### Design-tree changes, proposed only

`design/` is not edited by this study. If the comparison below does not
reject the contract, these go to the owner on an approval card:

- **compiler/storage-placement**, new decision: "Read a destination-form
  call result through the address its callee's source entry returns when
  every use is a field read or an eligible read-through consumer and checked
  effects and IR place facts prove every place the callee's substituted row
  names, with the call's destination, unchanged and valid from the call
  through each use, calling the public entry otherwise and never capturing
  in the caller; a source entry returns the address of a by-value Load
  snapshot of a place under one of its reference parameters when the
  read-through stability judgment holds from that read to its exit, cleanup
  included, and builds every other result in the destination, because
  Snowghost's `computed_of`, `font_of` and `box_of` miss ThinLTO import
  through their result copies alone while every caller reads a few fields of
  storage its row only reads, instead of a toolchain-wide import limit, an
  LLVM cost change that needs a patched host, captures that move the copy
  into callers, or borrows in the language." The Load-snapshot decision's
  exclusions (waiting, overlap, split parts, exposed storage) apply
  unchanged.
- **compiler/result-registers**, qualifying the destination rule: "Every
  WF-defined or linked definition whose result uses a destination also has
  an internal `<symbol>.source` entry with the same parameters returning the
  result's address under compiler/storage-placement, a linked one calling
  its C body into the destination and returning it; the public symbol keeps
  the destination ABI by calling that entry and transferring from the
  returned address unless it is the destination, so a caller's choice
  depends on the callee's signature alone, instead of changing every
  destination-form signature or deciding the entry's existence from the
  body."
- **compiler/backend-facts**: "Keep `captures(none)` on a source entry's
  reference parameter whose returned sources all lie behind owner pointers
  loaded from its referent, give `captures(ret: address, provenance)` to one
  whose own referent may be returned, omitting it where the host spelling
  lacks a return-only form, and mark the returned address `nonnull` and
  aligned but neither `noalias` nor `dereferenceable`, because the address
  outlives the call with the source's provenance and the caller may release
  the source after its last use, instead of the ordinary noncapture claim, a
  fresh-allocation promise, or an extent the host may assume past that
  release."
- **compiler/incremental-compilation** needs no change: caller fragments
  still depend on callee signatures only, and its ThinLTO decision already
  requires measured correction of modularity-induced runtime losses.

No specification change is needed (see
[What the language already fixes](#what-the-language-already-fixes)).

## Alternatives

| Alternative | Decided by |
| --- | --- |
| Raise the fragment link's import limit to 200 or 150 | Measured: Halo's fragment link +68% / +48% (hosted) and +45% (14900K) against a 25% bound; on the 14900K one of six word cells moved beyond the twin range. A global knob that changes every import decision to repair seven functions. Rejected. |
| LLVM inline-cost patch (candidate (e): count a recognized granule group as one intrinsic in `InlineCost`) | It changes the inliner, not the ThinLTO summary count, so it cannot pass the import gate that excludes `computed_of`, `font_of` and `box_of`. It also needs a patched LLVM, while Whitefoot hands its modules to the host clang and probes its spellings instead of pinning LLVM (backend-facts). Rejected for this regression. |
| LLVM summary-and-cost patch (candidate (e′): count the group as one unit in both `InlineCost` and the summary) | Would reach the import gate and the remaining `write_item`/`next_glyphs` misses, but needs the same patched host LLVM. Not pursued here; whether it stays a candidate under the host-clang policy is the owner's question. |
| Inline hints (`inlinehint`, `alwaysinline`, a per-function threshold) | ThinLTO decides import before any inline decision, so an inline-stage setting does not reach a callee that is never imported. The per-function threshold attribute already failed, because LLVM stops at the call site's threshold before applying it ([withdrawal record](../in-place-parameters/DESIGN.md#withdrawal-outcome-downstream-timing-and-the-accessor-attribution)), and could not reach the import gate either. A Whitefoot always-inline policy is already rejected in storage-placement (44% of Halo's definitions marked, text +25.7%). Whether LLVM 22's importer honours any callee attribute is not checked here; it is not needed for the decision. |
| Memcpy (with or without `!tbaa.struct`) for result transfers | Restores inlining but reintroduces wide loads across separately stored fields; the producer-sensitive policy failed Halo (concat 1.174, string-key 1.300). Rejected in storage-placement. |
| One entry whose every caller reads through and captures where needed | The copy moves into every uncovered caller: `next_glyphs` 29 → 80 and `shift_forward` 49 → 100 summary instructions in the shape-buffer witness. Rejected in favour of two entries. |
| Caller uses facts from the callee's body (which paths return a source) | Makes the caller's fragment depend on the callee's body, so every callee edit invalidates its callers' cached objects, against the fragment text rule. The row-derived source set is enough for every Snowghost caller. Rejected. |
| Demand specialization: a helper returning only the scalar a caller needs | Needs a body-dependent symbol per demand and demanded-result summaries; after the contract and inlining the covered sites get the same field reads. Rejected. |
| Writer-spelled form: return a reference, or a `from` clause naming the source | REF-3 forbids returned references and storage-placement rejects making snapshots borrows; a clause would restate what the row already bounds and make writers spell a placement choice the compiler can prove. No language gap is involved, so no new syntax. |
| Build Snowghost with module-granularity fragments | Moves the regression rather than removing it: cross-module callers still miss import, and it changes the downstream build's incremental grain. The incremental-compilation decision requires correcting such losses in the compiler. Rejected. |

## Coverage estimate, re-checked against the real accessor IR

The first draft estimated that the contract removes about 35% (range
25–45%) of the +1,260,787-instruction cell regression. Re-checked:

- **Per-call self instructions.** The fragment-build assembly of
  `computed_of` (`results/snowghost/styles-emitted.thin.asm.txt`) has a
  25-instruction element path ending in a 32-byte and a 16-byte store to the
  destination; with the call it is about 26, as assumed. `font_of` and
  `box_of` are taken as 29 from the same file. Dividing the attributed self
  growth gives about 24,000 `computed_of`, 9,300 `font_of` and 1,900
  `box_of` calls in the four edits.
- **Saved instructions per call**, from the fragment assembly of the
  hand-edited witnesses: 10 per `computed_of` call (`font_index` 34 → 24),
  17 more per `font_of` call (`column_gap_of`'s path 60 → 33, 27 in all,
  10 of them its `computed_of` call), 24 more per `box_of` call
  (`positioned` 68 → 34, 34 in all).
- **Total**: 24,000 × 10 + 9,300 × 17 + 1,900 × 24 ≈ 240,000 + 158,000 +
  46,000 ≈ 444,000, 35% of the regression: `computed_of` about 19 points,
  `font_of` 12.5, `box_of` 3.6. The arithmetic stands.

What it rests on, and what could move it:

- Call counts inferred from self instructions divided by one static path
  length; the pseudo-element path is shorter, so the `computed_of` count is
  a lower bound if pseudo paths occur.
- Savings from three static path pairs in hand-edited witnesses whose
  equivalence to a compiler implementation is by inspection; callers that
  read several fields (`same_shaping` reads `font_of`'s result twice) are
  not modelled.
- One cell: the callgrind attribution is the sequential apollo11 word edit
  only. The other five word cells and the font-size cell are assumed to
  have the same composition.
- The before-and-after builds of the attribution, not the current main.
  The pre-check below measures the current base directly.
- Instructions, not cycles. The emitted path stores 48 bytes with 256-bit
  and 128-bit stores and the caller reloads one 4-byte field from inside
  them; whether that reload forwards without a stall on the 14900K is
  unverified. A stall would make the time share larger than the
  instruction share.
- What it leaves: `text_of` (its size comes from rebuilding the value, and
  its source variant grows to 134), `write_item` and `next_glyphs` (they
  miss the ordinary inline threshold of 225 with copies that must stay),
  and the `read_item` sites whose result outlives a write.

The limit-150 arm gives one calibration in time. It removed calls to
`font_of`, most of `box_of`, `text_of` and `read_item`, but none of
`computed_of`'s copies, and recovered a median 31% of the word-cell
slowdown, within noise in five of six cells. The contract removes
`computed_of`'s copies (the largest single share, about 19 points of
instructions) and loses `text_of`'s inlining. The revised expectation is
therefore a median recovered share of **25–45% of the word-cell slowdown**
(the arm's 31% lies inside that range), with the candidate at or below the
arm in most word cells, and a smaller share of the font-size cell than the
arm's 70%, because `text_of` stays out of line. About two thirds
of the slowdown is expected to remain; the contract alone is not expected to
meet Snowghost's acceptance.

## Comparison fixed before measurement

Four compilers built from one source, the implementation branch's base:

1. **base**: main at the branch's base;
2. **twin**: the same source built separately, the noise control;
3. **candidate**: base plus the result-source contract;
4. **control**: base with the fragment link's import limit at 150, built
   from the same base (the limit argument from PR #374), as a mechanism
   comparison only, never proposed.

Programs: Snowghost-wf (the revision current when the run starts, renderer
identical to its main) `layout_oracle`, sequential and `--par`, in fragment
mode with an empty cache per build; Halo-wf `bench` in fragment mode and
with `--full-lto` (Halo's own timing build; the contract applies there too,
so unlike the import limit its binaries are expected to differ).

**Stage 0, correctness (gate, hosted).** The candidate passes `make check`
and adds compiler tests, each shown to fail when the condition it guards is
removed from the implementation: a source written between the call and a
field read (`v = f(&s); s.x = 1; read v.x` returns the old value); a source
released or moved between them; a window `grow` of the source's owner
between them; a call whose substituted row writes an overlapping path
between them; the end of an atomic statement's hold between them; a callee
that writes its source after reading it (returns its destination); a
callee path that returns a local or by-value parameter (returns its
destination); a forwarded inner call whose destination is a local slot (not
forwarded); a join of a source path and a built path in the callee; and an
indirect call and a waiting caller (public symbol). Every arm's
`layout_oracle` output on the three pages and Halo's checked outputs are
identical, since the contract changes no value.

**Stage 1, instruction counts and build cost (hosted, temporary
workflow).** For each compiler: direct call sites of `computed_of`,
`font_of` and `box_of` in the sequential fragment build of `layout_oracle`,
counting both the public and the `.source` symbols with `objdump`; text
bytes, fragment link time and total build time of Snowghost (sequential and
`--par`) and Halo (fragment and full-LTO); and callgrind 3.22 total and
per-function self instructions of the sequential apollo11 word cell with
the attribution's settings (first four edits, collection toggled on
`wf_layout.text_changed` and `wf_layout.update`). Base and twin must agree
within 0.5% of instructions and 1% of times, or the stage is re-run once.

**Stage 2, timing (14900K, temporary workflow derived from
`import-limit-timing.yml`).** Snowghost's incremental-layout harness on CPUs
2,4,6,10,12,14, cells: word, sentence, colour and font-size edits on
ecma262, html5 and apollo11, sequential and four workers; 7 rounds, the four
compilers back to back in each cell, order rotated each round; a cell's
value is its median microseconds per edit, a ratio is paired within a round,
and the twin's noise in a cell is the minimum to maximum of its seven
twin/base ratios. Halo: `run.py --before-binary` on CPU 2, 7 pairs per
kernel, base against twin, candidate and control, fragment and full-LTO.
The recovered share of a cell is (1 − r)/(1 − 1/S), with r the candidate's
median ratio and S the cell's slowdown from the table above.

### Predictions

- Stage 1: the candidate leaves at most 6 direct call sites of the three
  styles accessors, both symbols counted (5% of base's 136 at
  `c8f5e17`: 88, 7 and 41); their combined self instructions fall by at
  least 80%; the cell's total falls by at least 250,000 instructions (20%
  of the regression), centrally about 440,000; text grows by at most 2%.
- Stage 2: the median recovered share over the six word cells is 25–45%;
  the candidate's ratio is at or below the control's in at least four of the
  six word cells; no Halo kernel moves beyond its twin.

### Rejection criteria

Stage 1 runs first; a failure there stops the experiment before timing.

1. **Mechanism.** More than 6 direct call sites of the three styles
   accessors remain: the source entries are not imported as predicted, and
   the premise fails. The sites are then compared one by one with the
   control's before anything else.
2. **Size of effect.** The cell's total falls by less than 125,000
   instructions (10% of the regression): the contract is too small to be
   the next step, and the direction returns to the owner. A fall between
   125,000 and 250,000 fails the prediction without rejecting; the
   coverage estimate is then revised from the measured per-function counts
   before any timing.
3. **Build cost.** For Snowghost or Halo in any build mode, text grows by
   more than 5%, or the fragment link by more than 10%, or total build time
   by more than 5%, against base. The contract adds one small entry per
   destination-form definition and changes no import setting, so growth
   beyond these bounds means the duplication is not small; the import-limit
   arm was rejected at +45% link.
4. **Timing effect.** Fewer than four of the seven focus cells (six word
   cells and sequential apollo11 font-size) have a candidate/base median
   below their twin's minimum, or the median recovered share over the six
   word cells is below 15%. The limit-150 control reached two of seven and
   31%; the twin itself in that run had a median share of 0%.
5. **Veto.** Any timed Snowghost cell whose candidate/base median exceeds
   the twin's maximum by more than 0.01, or any Halo kernel, in either
   build, whose candidate/base ratio exceeds both 1.01 and its twin/base
   ratio plus 0.01. A vetoed cell or kernel is re-run once; a repeat rejects
   the contract in its present form.

A candidate whose instructions fall while timing stays within the twin
range is reported as no timing effect, not as a pass. If the contract passes
and, as predicted, about two thirds of the slowdown remains, the remainder
(`text_of`'s rebuild, `write_item`, `next_glyphs`, the uncovered
`read_item` sites) goes to the owner with its candidates: the `text_of`
placement repair (its anonymous-box rebuild copies five nested fields twice
because the found value is placed in the result) and (e′). The contract is
not widened to cover copies that must stay.

## Changes from the first draft

Made before any measurement of the contract, after the import-limit results
above:

- **Callers never capture.** The draft let a caller capture before the first
  invalidating event. A capture saves nothing over the callee's copy and adds
  one when the callee built into the destination, so a caller that cannot
  read through every use now calls the public symbol.
- **Linked definitions get a source entry** that builds into the destination,
  so whether a caller can use `.source` depends on the callee's signature and
  not on whether its implementation is WF or linked, keeping one callable
  ABI.
- **No `dereferenceable` on the returned address**, for the reason under
  [Backend facts](#backend-facts); the draft had it.
- **Forwarding requires the inner call to use the outer destination**, so a
  forwarded address can never be a callee-local slot.
- **The comparison adds** the limit-150 control in the same interleaving,
  build-cost bounds, the 14900K timing criteria and the correctness stage;
  the Stage 1 predictions and the 125,000 and 250,000 thresholds are the
  draft's, unchanged.
- **Name**: `.origin` became `.source`.

## Limits

The study is compile-only evidence plus earlier measurements: no
implementation exists, and the witnesses' source variants are hand edits.
S comes from a different run that built two Snowghost sources; the
recovered share inherits that and the per-cell noise shown above. One
machine, one Snowghost and one Halo revision per run. Where the importer
treats the new entry differently from the witnesses (for example a deeper
import chain), Stage 1's call-site count shows it before any timing.
