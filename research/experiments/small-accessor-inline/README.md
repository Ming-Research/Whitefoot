# Small accessor inline-cost witnesses

This experiment belongs to board item `lm-small-accessor-inline`. Apart from
the [import-limit timing](#fragment-link-import-limit-timing-on-the-14900k)
at the end, it is compile-only.
The question is whether a 48-byte by-value accessor result can keep narrow
field accesses while remaining profitable at an LLVM 22 statically cold call
site. The top-level witnesses are synthetic LLVM IR, not emitted Whitefoot
programs; `snowghost/` adds IR extracted from Snowghost's emitted program
([below](#snowghost-extracted-witnesses)). None of them is a downstream
performance comparison.

## Protocol recorded before measurements

Compare the same twelve-i32, 48-byte record at source offset 16 using:

- **granules:** Whitefoot's `copy_granules` shape in
  `compiler/src/backend/emitter/places.rs`: inbounds byte GEPs, align-1 scalar
  loads, every source load before every destination store, no transfer TBAA
  or invented alias scopes;
- **read-through:** a pointer-returning private accessor representation and
  immediate caller field reads, valid only while source storage remains
  live and unchanged; compare one observed field and the complete record;
- **memcpy-tbaa:** an ordinary 48-byte memcpy with twelve `!tbaa.struct`
  field triples, with disjoint source/destination required by the intrinsic;
  the witness callers supply fresh result allocas;
- **masked:** a twelve-lane i32 masked gather followed by masked scatter,
  all lanes enabled and all source lanes captured before destination writes.

For each, run full `opt -O2` with inline remarks using the IR's generic
x86-64 target/TTI, then `llc -O2 -mcpu=alderlake`, retaining the optimized
IR and assembly. Two cold callers
observe one field (`partial`) or read the complete result (`full`). A
complete-copy consumer is essential: eliminating eleven fields after
inlining does not show that a retained copy respects narrow boundaries.
The public accessor remains in the module so its standalone copy lowering
is also visible. Branch weights 1:10000, rather than `cold`/`alwaysinline`
callee attributes or custom inlining thresholds, establish the cold call site.

Reject granules as an inline repair if the cold call remains. Reject
read-through as a general snapshot lowering if it lacks a proof that the
source remains unchanged/live, and as a useful experiment if immediate
read-only consumers retain copying or do not inline. Reject memcpy-tbaa if
any retained transfer reads across i32 boundaries; metadata describes fields
but is not a machine-access-width constraint. Reject masked transfers if
they fail either structural property or retain substantially more machine
instructions than the scalar/granule copy. No native execution or timings
are required to reject a structural candidate. Static instruction counts
describe emitted regions, not dynamic counts or measured cycles.

The existing granule decision explicitly promises no final machine width.
The twelve homogeneous i32 leaves deliberately expose backend merging;
results do not establish widths for Snowghost's actual mixed field layout.
The minimal accessor has no ownership/tag/bounds dispatch, and thus gives
an optimistic inline cost relative to a real accessor containing those paths.
The IR uses no `optnone`, volatile accesses, forced inlining, or
optimization-disabling attributes to preserve an artificial copy shape.

## Reproduction

```sh
python3 research/experiments/small-accessor-inline/run.py --llvm-bin /opt/homebrew/opt/llvm@22/bin --output /tmp/small-accessor-inline
```

CI can supply its LLVM 22 bin directory and a scratch output directory.
The driver executes only LLVM 22 tools from `--llvm-bin`: `opt` and `llc`
for every witness, and `llvm-extract`, `llvm-dis`, `llvm-nm` and `llvm-lto2`
for the [Snowghost witnesses](#snowghost-extracted-witnesses). It records
their exact command arrays, versions, inline remarks, optimized IR, assembly,
and per-function static instruction counts. It never compiles or runs
Whitefoot or a native executable. LLVM-mca is optional follow-up rather than
a timing source.

## Witnesses and first control observation

The first direct-copy granule run **inlined**, cost 20 against 45. Its
pre-inline IR still contained all 24 scalar memory instructions; the
inliner can account for caller alloca promotion. Therefore a bare copy
does not reproduce the downstream failure, and the experiment retains it
as a favorable control rather than adding optimization barriers.

The four primary witnesses add one identical external source-resolution
call, `resolve_source(ptr)`, before the source-offset GEP. This models an
accessor that calls an unresolved lookup helper. It costs 35 additional
inline units in the granule comparison and reproduces the observed
granule-versus-intrinsic decision. The helper's body and executed cost are
unknown and excluded from instruction counts; no claim is made that
Snowghost's `computed_of` has this exact helper, control flow, or layout.
This is an IR feasibility witness, not an extraction of that function.

- `granules.ll`: the current transfer spelling.
- `read-through.ll`: pointer result with immediate stable partial/full reads.
- `memcpy-tbaa.ll`: memcpy metadata does not prevent wide lowering.
- `masked.ll`: gather/scatter cost and retained per-lane widths.
- `direct-*.ll`: the same four forms without the lookup helper; the
  granule control already inlines and its partial copy disappears.
- `mixed-granules.ll` and `mixed-direct-granules.ll`: alternate six i32
  and six float leaves at the same four-byte offsets. These retain
  original scalar types as Whitefoot does; they test whether homogeneous
  leaf merging accounts for the final width result.
- `placed-granules.ll`: direct copy with a caller-supplied result place
  instead of a fresh alloca. This is a SROA/placement control, **not** a
  semantics-equivalent pointer-result comparison: its result writes are
  observable to the caller that supplied the destination.

## LLVM 22.1.8 results (2026-10-11)

The snapshots under `results/` retain optimized IR (`*.opt.ll`), assembly
(`*.asm.txt`) and inline
remarks for every witness, plus a pre-inline dump for the primary granule
witness. All results used Homebrew LLVM 22.1.8 on this Mac, but the emitted
target is x86-64: generic x86-64 optimization followed by alderlake
lowering. They are compile-only observations, without execution, clocks,
llvm-mca cycle estimates or Whitefoot builds/tests.

Both partial and full callers had threshold 45 and the same cost/verdict
within each row. Static counts cover each entire emitted function,
including branches, calls, returns and prologue/epilogue. The taken-path
columns count entry plus the straight-line rare path and any out-of-line
accessor body, excluding the untaken return and the unknown
`resolve_source` body. A gather instruction counts as one machine
instruction, not one micro-op or one lane access.

| Primary transfer form | Cold inline cost / verdict | Accessor static | Partial caller static | Full caller static | Partial taken path | Full taken path |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| granules | 55 / missed | 11 | 12 | 18 | 22 | 28 |
| read-through | 0 / inline | 5 | 9 | 14 | 8 | 13 |
| memcpy-tbaa | 0 / inline | 11 | 9 | 14 | 8 | 13 |
| masked | 5 / inline | 48 | 52 | 60 | 51 | 59 |
| mixed granules | 55 / missed | 30 | 12 | 36 | 41 | 65 |

| Control | Cold inline cost / verdict | Accessor static | Partial caller static | Full caller static | Partial taken path | Full taken path |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| direct granules | 20 / inline | 6 | 6 | 9 | 5 | 8 |
| direct read-through | -35 / inline | 2 | 6 | 9 | 5 | 8 |
| direct memcpy-tbaa | -35 / inline | 6 | 6 | 9 | 5 | 8 |
| direct masked | -30 / inline | 43 | 49 | 53 | 48 | 52 |
| direct mixed granules | 20 / inline | 25 | 6 | 28 | 5 | 27 |
| placed granules | 80 / missed | 6 | 10 | 20 | 15 | 25 |

`results/summary.tsv` and `results/summary.json` give the mechanically
counted regions and the exact instruction lists. The driver emits the
same summaries to its scratch output directory. The next table records
manual inspection of final payload accesses; stack saves, metadata
constant loads and return-address traffic are not record payload accesses.

| Form | Final source payload accesses | Final result/output payload accesses | Field-width conclusion |
| --- | --- | --- | --- |
| granules, lookup or direct | Standalone accessor: 32 + 16-byte loads | 32 + 16-byte stores; lookup full caller then copies its temporary with 16-byte loads/stores | No width preservation for homogeneous copy; direct partial reduces to one 4-byte load |
| read-through | Partial: one 4-byte load; full: 32 + 16-byte loads after inlining | Partial: no result allocation/store; full: 32 + 16-byte output stores | Useful for a stable partial consumer; whole-copy consumer still merges |
| memcpy-tbaa | Partial: one 4-byte load; inlined full: 32 + 16-byte loads; standalone: two overlapping 32-byte loads at source offsets 16 and 32 | Inlined full: 32 + 16-byte stores; standalone: overlapping 32-byte stores at result offsets 0 and 16 | Metadata does not preserve leaf widths; full copy fails the criterion |
| masked | Two `vpgatherdd` instructions read twelve enabled 4-byte lanes | Twelve 4-byte `vmovd`/`vpextrd` scatter stores; full caller then copies its temporary with three 16-byte loads/stores | Transfer itself retains 4-byte lanes, but full consumption introduces narrow-store/wide-load traffic locally |
| mixed granules | Twelve scalar 4-byte loads (`movl` / `vmovss`) | Twelve scalar 4-byte stores; mixed full consumer also uses 4-byte accesses | Preserved in these snapshots, including direct full inlining; this is not a width contract |

The lookup masked partial path emits 51 instructions against granules'
22 and read-through's 8. Its full path emits 59 against granules' 28 and
read-through's 13. Even the direct no-helper control retains a full
gather/scatter for a one-field read: 48 versus 5 taken-path instructions.
These counts are sufficient to reject it under the preregistered
instruction-growth criterion. They do not quantify cycles or the effect
on Snowghost's timings, and the full mixed-granule path (65 instructions)
illustrates why a count alone cannot compare different field layouts.

The pre-inline primary dump contains twelve align-1 i32 loads followed by
twelve align-1 stores; the full optimized homogeneous accessor contains
three `<4 x i32>` loads/stores, and alderlake lowering merges two into
32-byte accesses. Conversely the alternate i32/float granules remain
narrow. This isolates a layout-dependent final-width limitation already
allowed by the storage-placement decision.

## Feasibility conclusion and limits

The structural lead is a compiler-private read-through result for stable
read-only consumers, retaining the existing by-value source semantics and
materializing a snapshot before any invalidation. Immediate partial use
removes the copy and keeps one original-width load in this witness. A
later source mutation, source destruction, escaping/exposed result, or
unproved lifetime makes a raw pointer-result substitution invalid; full
consumption alone is not sufficient to preserve field boundaries. No
compiler implementation or source-language reference return is proposed
by these files.

Memcpy-tbaa restores inlining here but fails retained-copy widths, so
limiting it to candidate functions would reintroduce the producer-policy
problem wherever a retained result copy consumes separately stored fields.
Masked transfers pass the cold-inlining and copy-lane checks but fail the
cost criterion and retain the otherwise removable one-field copy. Keep
them as a rejected witness rather than a backend proposal.

An LLVM inline-cost correction that recognizes scalar transfer groups
before early stopping could retain today's granule emission; it requires
an LLVM implementation/validation and downstream comparison outside this
experiment. Reaching memcpy-like cost would not itself strengthen the
final-width guarantee: the homogeneous control still merges. The main
investigation section specifies that LLVM candidate and the design-tree
gap check.

The actual Snowghost accessor layouts, pre-inline IR, result placement,
lookup behavior, inliner stage and execution counts remain unverified.
In particular, the favorable direct-copy control prevents attributing
every 48-byte record return's failure to granules alone.

## Snowghost extracted witnesses

`snowghost/` holds Snowghost's real accessor bodies with real callers, for
the [result-origin follow-up](../../investigations/in-place-parameters/DESIGN.md#snowghosts-accessors-real-shapes-and-the-result-origin-contract).
Provenance: Snowghost-wf run 38198957372, compiler release
`wf-26355924f942`, Snowghost source `c8f5e17` (`15e8d4a` plus a workflow
and pin), command `whitefootc --emit-llvm --graph modules.wfg --entry
layout_oracle -o layout_oracle.ll`. Each `*-emitted.ll` copies the listed
definitions verbatim from that whole-program IR, with the named types,
storage-role TBAA and alias-scope metadata and attribute group they use;
every other callee becomes a declaration with its emitted signature. The
`*-origin.ll` files are hand-written edits of those copies that apply the
proposed result-origin contract and nothing else:

- `styles-emitted.ll`: `element_position`, `computed_of`, `font_index`,
  `font_of`, `same_shaping`, `column_gap_of`, `box_of`, `initial_box`,
  `positioned`, `text_of`, `valign_edge` from `renderer/layout`.
- `styles-origin.ll`: `computed_of`, `font_of`, `box_of` and `text_of`
  return `ptr`, the source element's address or the destination where they
  build a value; their callers read through the returned address.
- `shape-buffer-emitted.ll`: `blank_item`, `read_item`, `write_item`,
  `fail`, `put_item`, `put_out`, `next_glyphs`, `next_glyph`, `item_at`,
  `current`, `out_at`, `make_room`, `shift_forward`, `apply_forward`, `sync`
  from `renderer/font`.
- `shape-buffer-origin.ll`: `read_item` returns the element's address, the
  `item_at`/`current`/`out_at` wrappers forward it, `apply_forward` and
  `make_room` read and pass it through, and `next_glyphs` and
  `shift_forward` capture it into their slot before the writing call.

For each witness the driver runs the whole module through `opt -O2` (a
non-fragment build) and emulates `whitefootc --cache --fragments function`:
`llvm-extract --func` per defined function, `opt
-passes=thinlto-pre-link<O2> --thinlto-bc`, then `llvm-lto2 run -O2
--thinlto-threads=1 --save-temps` with every defined symbol prevailing.
It records each fragment's summary instruction count, which ThinLTO
compares with its import limit of 100. It also records what each
post-import module imports, both pipelines' inline remarks, and
`llc -O2 -mcpu=alderlake` static counts. The control
`styles-emitted-import-200` repeats the fragment link of
`styles-emitted.ll` with `--import-instr-limit=200`. Callees outside a
witness are declarations, so each remark is the outcome at that real call
edge, not a replay of the whole program.

`results/snowghost/` retains `summary.tsv` and `summary.json` (remarks,
imports, summary and static counts) and the fragment-build assembly of the
two styles witnesses, which the path counts in the investigation use.

| Function | Summary instructions, emitted → origin | Fragment build, emitted | Fragment build, origin | Import limit 200, emitted |
| --- | --- | --- | --- | --- |
| `computed_of` | 119 → 28 | not imported | inlined, 70/225 | inlined, 190/225 |
| `font_of` | 104 → 29 | not imported | inlined, 125/225 | inlined, 220/225 |
| `box_of` | 107 → 68 | not imported | inlined, 145/225 | inlined, 195/225 |
| `text_of` | 109 → 134 | not imported | not imported | imported, not inlined, 465/225 |
| `read_item` | 62 → 13 | inlined, 50–115/225 | inlined, −15/225 | not run |
| `write_item` | 79 → 79 | not inlined, 290/225 | not inlined, 290/225 | not run |
| `next_glyphs` | 29 → 80 | not inlined, 330/225 | not inlined, 330/225 | not run |

The whole-module pipeline inlines `computed_of`, `font_of` and `box_of` with
the emitted bodies too, at 190, 220 and 195. These witnesses therefore show
no cold-site threshold at the real edges: the fragment build's import limit
excludes the three styles accessors, and the ordinary threshold 225
excludes `write_item`, `next_glyphs` and `text_of`. The origin variants
compile and optimize; they are not executed, so their equivalence to the
emitted functions rests on inspection of the edits.

## Fragment-link import limit: timing on the 14900K

Written before the run. The mechanism check, build costs and the adoption
criterion are in the
[investigation](../../investigations/in-place-parameters/DESIGN.md#fragment-link-import-limit-question-comparison-and-rejection-criterion);
the hosted probes found that limits 200 and 150 remove the direct calls to the
small accessors but make Halo's fragment link 48% slower at 150 (7.5 s to
11.0 s). This is the timing the owner chose to take before weighing that cost.

**Question.** How much of the slowdown of Snowghost's six word-edit cells
(ecma262, html5 and apollo11, each in the sequential build and at four
workers) and of apollo11's sequential font-size change does a ThinLTO import
limit of 150 in the fragment link recover, and are Halo's seven kernels left
no slower?

**Comparison.** `.github/workflows/import-limit-timing.yml` (temporary) builds
three compilers from this branch: base (the limit argument removed), twin (the
same source built separately) and candidate (limit 150). Snowghost-wf `c8f5e17`
(renderer and harness identical to its main) is built by each in fragment
mode, sequential and `--par`, and its incremental-layout harness (the one
behind Snowghost-wf run 38197250440) times the edit cells on the i9-14900K
pinned to CPUs 2,4,6,10,12,14. Every cell runs the three builds back to back
for 7 rounds, the order rotating each round. A cell's value in a round is the
median microseconds over its edits; a ratio is candidate (or twin) over
base within the same round, and the twin's noise in a cell is the minimum to
maximum of its 7 twin/base ratios. Halo-wf `641eb29` is timed with its own
pair instrument (`run.py --before-binary`, CPU 2, 7 pairs per kernel), base
against twin and against candidate, in the fragment build, where the limit
applies, and in the `--full-lto` build Halo's own timing uses, where it is not
passed and the candidate's binary should be byte-identical to base (the run
prints the hashes).

**Falsifier.** If on every word-edit cell and on apollo11's sequential
font-size cell the median candidate/base ratio lies inside the twin's range,
the limit recovers none of the slowdown beyond noise, and the import gate is
not what costs these cells time. A cell counts as faster only where that
median is below the twin range's minimum. Halo is slower beyond noise for a
kernel whose candidate/base ratio exceeds the twin's for that kernel.

**Not decided here.** Whether the recovered share is at least half of the
slowdown against `wf-78223721f77d` (the investigation's adoption bound) needs
that release's cells, which this run does not build; compare ratios with run
38197250440, not microseconds, since this run uses compilers built from this
branch. The all-cells veto (no cell slower than base by more than the twin's
range plus 1%) is reported for the cells timed: the word, sentence, colour and
font-size edits on the three pages in both builds. The root-font and block
edits (an input away), html5 style matching and the four-thread full layout
stages are not timed. One machine, one Snowghost and one Halo revision, one
compiler build.
