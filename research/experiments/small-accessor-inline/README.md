# Small accessor inline-cost witnesses

This compile-only experiment belongs to board item `lm-small-accessor-inline`.
The question is whether a 48-byte by-value accessor result can keep narrow
field accesses while remaining profitable at an LLVM 22 statically cold call
site. These witnesses are synthetic LLVM IR, not emitted Whitefoot programs
and not a downstream performance comparison.

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
The driver executes only `opt` and `llc`; it records their exact command
arrays, versions, inline remarks, optimized IR, assembly, and per-function
static instruction counts. It never compiles or runs Whitefoot or a native
executable. LLVM-mca is optional follow-up rather than a timing source.

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
