# Indexed reductions in counted loops

Status: lowering A and scalar/indexed composition remain selected. The owner
reported that the merged rule permits none of Snowghost's 32 counted sites
and selected the extensions below. The original Proposal and Criterion record
that earlier experiment; the extension implementation and open interface
boundary are recorded in [Extensions after the count](#extensions-after-the-count).

## Question

Can [PAR-2](../../../spec/kernel-spec.md) permit a counted loop whose
iterations combine into an indexed family of accumulator cells, a histogram
or a coverage bitmap, where several iterations intentionally address the
same cell, without proving the addresses distinct?

```whitefoot
fn histogram(keys: &[u8]) -> made: Box<Array<u64>> reads(keys) {
  let counts = box_array_filled::<u64>(count: 256_u64, value: 0_u64);
  let count = keys^.len;
  for (i in 0_u64..count) {
    let bucket = cvt::<u8, u64>(keys^[i]);
    set counts.inner[bucket] = counts.inner[bucket] +wrap 1_u64;
  }
  return move counts;
}
```

Before this extension, PAR-2 denied this loop: the write to `counts.inner[bucket]` is neither
iteration-own storage, the scalar accumulator, a proved affine element, a
proved range reference nor a certified element, and two iterations may
write the same cell.

## Evidence

Snowghost-wf's classification of the 726 loops PAR-2 denied in its browser
engine at `wf-exp-0eee42d9be0f`
([classification](https://github.com/Ming-Research/Snowghost-wf/blob/research/storage-mocks/research/investigations/storage-layout/par-classification/classification.md),
section "G-indexed"), from source reading only:

- 32 loops are shared indexed commutative reductions or idempotent marks
  (coverage-word OR, flags, row maxima, histograms, cascade and border
  winners), 25 of them estimated hot.
- 16 to 22 whole loops are plausible with integer and Boolean carriers and
  local helper narrowing; cascade and border winners need a record carrier
  with a total key and are out of scope here.
- Across all 726, further PAR rules are worth roughly 30 to 45 loops; three
  quarters of the denials are the algorithms' real order. Indexed reductions
  are the largest single family.

## Proposal

Extend PAR-2's accumulator condition from one whole binding to an indexed
family of cells of one root declared outside the loop:

- every occurrence of the root in the body is one `set R[e] = R[e] op x`
  whose target and first operand are the same subscripted place, with `op`
  one fixed operation for the root across the body and one of the operations
  PAR-2 already admits for a scalar accumulator (`+wrap`, `*wrap`, `iand`,
  `ior`, `ixor`, `imin`, `imax`, `band`, `bor`, `bxor`);
- the subscript `e` and the contribution `x` read nothing of the root, so
  no iteration observes a partial result (no prefix reads, no
  check-before-update);
- the subscript's ordinary [OP-4] bound is proved as for any write; no
  injectivity is asked.

Every admitted operation is associative and commutative at its fixed width,
so any order of combining the iterations' contributions gives the
sequential result exactly; this is the same ground PAR-2 already uses for
the scalar accumulator.

## Lowering choices

The permission is semantic; how the implementation recombines is not. Each
choice below keeps the sequential result.

- **A. Private copies, combined in leaf order.** Each split leaf reduces
  into its own copy of the root initialized to the operation's identity, and
  the copies fold into the root in leaf order after the loop, as the scalar
  accumulator does today. Cost: one copy and one elementwise combine per
  leaf, so it pays only when the loop's work dominates leaves times cells;
  the existing grain pricing can refuse to split. No synchronization.
- **B. Atomic read-modify-write.** Every update is one atomic operation on
  the shared cell. Cost: an atomic per update, contention on hot cells, and
  compare-and-swap loops for `imin`, `imax` and `*wrap`; it also adds a
  synchronization primitive to the generated code, where the project's aim
  is that proofs authorize parallel independence without adding locks or
  dependencies.
- **C. Cell-range partition.** Each worker owns a range of cells and scans
  every iteration, applying only contributions to its cells. Cost: every
  worker evaluates every subscript, so the iteration work is multiplied by
  the worker count; it suits only loops whose subscript is cheap and whose
  contribution is expensive.

Recommended: A, because it reuses the scalar accumulator's split, identity
and combine, adds no synchronization, and lets the grain decision decline
small loops.

## Criterion

Recorded before any implementation or measurement:

- **Permission.** With the rule as stated, at least 16 of Snowghost's 32
  G-indexed loops are permitted without source rewrites beyond local helper
  narrowing. Fewer rejects the rule as too narrow for its cost.
- **Performance.** On the i9-14900K through CI, the histogram above over
  10^7 keys and 256 cells runs at least 2 times faster with 8 workers than
  sequentially under lowering A, measured with interleaved runs and a twin
  of the sequential build. A smaller speedup, or a slowdown at 4096 cells,
  rejects A in favor of re-examining B or C.
- **Soundness.** Conformance cases cover each admitted operation, a prefix
  read and a check-before-update (both denied), a subscript or contribution
  that reads the root (denied), two different operations on one root
  (denied), and a mixed affine element and indexed reduction on one root
  (denied).

## Owner selections

1. Lowering strategy: A selected. B and C remain comparison alternatives.
2. The original choice deferred constant idempotent marks
   (`set flags[e] = True()`). The owner reopened and selected them in
   [Extensions after the count](#extensions-after-the-count).
3. A loop may also carry a scalar accumulator in the same body; the two
   recombine independently.

## Lowering contract

The primary agent authorized Q1, extending the scalar-only lowering boundary.
The permission payload retains each checked root place and its fixed operation
alongside the existing optional scalar. Lowering captures a nonowning range
and its initialized cell count at loop entry; the IR also carries the element
type, operation and identity. Their lifetime is structural: the site owns
private storage until every splitter call joins, and the outlined body only
borrows its assigned cells. Nested outlined loops transport the enclosing
private root mappings through their captures.

Each root uses one allocation containing one identity-filled cell range per
leaf. The recursive splitter halves those ranges with its iteration range.
After the join, the site visits flat cell offsets in ascending order, which
is leaf order followed by cell order, combining each private contribution into
the original cell before freeing the allocation. The scalar seed/result uses
its existing independent recombination. A sequential world or zero-budget
split passes the original root ranges and allocates no private storage.

Pricing charges three traversals of leaves times initialized cells for copy,
fill and combine, conservatively retaining the copy charge although identities
do not require copying source seeds. Saturating scheduling arithmetic cannot
turn overflowing overhead into a cheap offer. A split is refused when its
estimated work does not exceed that charge. Allocation counts and byte sizes
use the ordinary target-bounded checked allocation helper. Failure while
acquiring temporary slabs releases earlier slabs before STOR-8 termination.
Heap exhaustion during leaf execution, in a leaf's own allocation or a
nested split's, terminates the program from the trusted base [STOR-8], so no
private storage needs release on that path, as for every other live owner.

Indexed frames use the existing needed-capture pruning and selected-target
layout check. A conservative estimate charges any inline aggregate a whole
lane slot, so refusing before those checks incorrectly leaves the Slots
fixture sequential merely because its scope retains the initializer array.
Pruning retains the indexed ranges and initialized counts consumed by the
splitter and allocation site, and remaps their capture positions. A genuinely
oversized candidate reuses its completed body with indexed ranges rebound to
the original storage, or the enclosing split's private storage. This restores
the existing frame-fitting and refusal behavior without changing permission
or the private-slab lowering choice.

The maintained compiler tests inspect private fill, leaf-order combine, frees,
all operations and storage shapes; native observers force a split budget,
count private allocations and leaves, force zero budget, refuse a tiny loop
over many cells, fail each temporary allocation in a two-root loop, and check
a live aggregate capture that fits after pruning against one whose frame is
truly too wide. The
whole-program oracle checks colliding histogram updates, positive minima,
negative maxima, odd wrapping products, untouched cells and an independent
scalar count against fixed expected values and a sequential build. These
checks have not been run in this worktree: the owner prohibits local builds,
compilation, tests and lint, and prohibits committing or pushing this round.
CI must establish the emitted IR's validity and the native observations before
this implementation is qualified. The performance and downstream permission
criteria above remain unmeasured. Paged remains deferred until PR #263 lands
on main; it is not a type or storage path in this checkout.

## Extensions after the count

### Evidence and selected direction

The owner reports 0 permitted sites out of Snowghost's 32 G-indexed sites
after the original rule merged in Whitefoot's indexed-reduction change. That
fails the pre-registered permission criterion of at least 16. The cited
per-site record is Snowghost-wf branch `research/par-count-274`, revision
`80345cb`, file
`research/investigations/storage-layout/par-classification/g-indexed-274.md`.
That record was not available in this checkout or the inspected local source
roots; no remote was contacted. The count is owner-supplied evidence, not a
new compiler run.

The owner selected five extensions, keeping the per-cell order-independence
argument: root len/cap reads, a one-step immutable single-use temporary,
constant integer/Bool marks, integer/Bool fields below record elements, and
unsigned saturating addition for both scalar and indexed accumulators. The
question for a recount is whether those forms recover at least 16 of the
same 32 sites without source rewrites beyond local helper narrowing. A lower
count still rejects the permission criterion. No new count or performance
claim is made here; the original performance comparison remains required.

### Implemented changes awaiting execution

PAR-2 now admits a measure read of the exact indexed root. Its descriptor is
unchanged by the permitted writes. The checker separates those occurrences
from cell reads using the existing measure/place representation. A bound
check that reads `starts.inner.len` may precede a colliding cell update.

An indexed update can use `let t = R[e] op x; set R[e] = t;`, or the commuted
initializer, in one block. The checker retains candidate initializers while
walking ordinary read/write footprints. Any intervening root access or write
to a place the initializer reads invalidates the candidate. Every commit is
recorded, including a repeated write to a place already in the retained write
set. At completion, t must have one runtime use and no writes after initialization;
forming a reference to t is a use even when it reads no contents.
The checker admits exactly one step, not a chain. Lowering preserves the
written let and set; its existing root-to-private-range mapping redirects the
initializer's read and the set's write together. The diagnostic names an
invalid update/temporary instead of describing a computed value as a constant
mark.

Unsigned `+sat` now belongs to the scalar/indexed operation set with identity
zero. Type checking distinguishes it from signed saturation. The scalar join
uses the ordinary saturation operation; the indexed join emits the unsigned
saturating-add intrinsic, rather than wrapping addition. The algebra is
`min(sum, MAX)` over nonnegative inputs. At i8, `(127 +sat 1) +sat -1` differs
from `127 +sat (1 +sat -1)`, so signed saturation remains outside the set.

The formal fixtures cover len/cap, direct and commuted temporaries, Bool
temporaries, temporary duplication/borrowing/rebinding/chaining, intervening root reads
and repeated writes, index and contribution mutation, and signed versus
unsigned saturation. The existing indexed wrong-operation fixture now uses
signed saturation; its unsigned form is a positive under the amended rule.
Compiler permission assertions consume these fixtures. Backend tests add
private-range IR assertions and a forced-four-leaf native observer with
literal expected cell counts, untouched cells, and scalar/indexed u32 sums
that remain below the maximum in each leaf and saturate only at the join.
The identity table covers every unsigned width independently of emission.

### Interface boundary: constant marks and record fields

These two parts are stopped under the owner's instruction to report an
insufficient interface before extending it. Their spec/checker/lowering
changes and fixtures have not been applied. Minimal fragments are:

```whitefoot
set cells[0_u64] = 1_u8;
set rows[0_u64].count = rows[0_u64].count +wrap 1_u64;
```

`IndexedReduction` retains a container root and `LoopCombine` only;
`IrIndexedReduction` retains a range capture, count, one element type,
identity and binary operation. `indexed_capture` and the backend select the
original range when no private split runs. Thus a Bool mask for a u8 mark
cannot inhabit that same typed capture. The backend also assumes identical
private and destination strides. `indexed_slice` maps a container prefix,
and storage redirection recognizes a terminal subscript; it cannot select a
field-specific private range or combine a scalar range into strided record
fields.

Recommended extension for owner review: make a family retain its checked
container plus field path and either a combine operation or a typed mark
constant. Give lowering distinct private-storage and destination descriptions,
so a mark can write a Bool mask while its unsplit execution still stores the
source constant, and a field can use packed private scalars while its join
projects the original record field. Nested splits must preserve that mapping;
a mark joining into an enclosing mask combines by OR, and the outermost join
stores the constant. Zero-budget and declined splits must retain source
semantics. The user has been asked whether to extend this interface; elapsed
time supplies no approval.

Required qualification after that extension: colliding and untouched integer
and Bool marks, named constants, different constants and operation/mark mixes,
root-dependent mark indices, independent record field families, unchanged
sibling fields, and whole-element writes mixed with field families. Force
splits, zero budget, nested splits and allocation failure. The masks must be
released after structured joins and introduce no atomics.

### Verification status

All additions in this round are uncommitted and unexecuted. The owner
prohibits local builds, compilation, tests, lint, whitefootc, CI, external
services, commits and pushes. Only edited Rust-file formatting is allowed.
The specification title and archives, design/log.md and spec/log.md are left
for the owner; the edited design decisions are proposed records. A future CI
run must check acceptance, permissions, IR validity and native observations
before these changes are qualified, followed by the downstream recount.

Independent read-only completion review covered the complete uncommitted diff
and new fixtures against `db72af4347a44de7d389d97ca28efd661a7c59b3`, using an
inherited Codex model whose exact model identifier was unavailable. It read
the repository review groups A, D, C, T and V and design checks G1–G3/DC1–DC4,
plus the changed regions and their direct consumers; it ran no commands that
build, compile, execute, test or lint. The review found one correspondence
issue: “operand-read occurrence” did not clearly exclude an extra reference
formation. The rule now requires exactly one runtime use, and a borrowed-
temporary negative pins that distinction. The reviewer inspected that fix and
the separate fixture repair adding explicit returned-array length guards
before the native oracle's subscript reads. No findings remain within the
implemented scope. Actual source acceptance, diagnostics, IR validity, native
results, mechanical design/spec checks and overall safety qualification
remain unverified. No design-lint statistics were collected.
