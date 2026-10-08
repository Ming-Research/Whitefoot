# Indexed reductions in counted loops

Status: the owner selected the Proposal, lowering A (private copies combined
in leaf order), and choice 3 (scalar and indexed accumulators in one body).
Choice 2 (constant idempotent marks) is deferred. Permission and the private-range
lowering path are implemented on the work branch. Compilation, execution and
CI verification remain pending. The Criterion below is unchanged.

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

## Results

Measured after the rule and lowering A merged (main `691ea8106`, v0.102):

- **Permission: fails.** Snowghost-wf counted its 32 G-indexed loops with
  release `wf-691ea8106920` (branch `research/par-count-274` at `80345cb`,
  write-up `research/investigations/storage-layout/par-classification/g-indexed-274.md`,
  runs 37833206599 and 37830969997): 0 permitted as written and 0 with local
  helper narrowing. The blockers are spellings, not the rule's safety
  conditions: constant marks (about 7), an update computed into a
  single-assignment temporary before the `set` (about 5), a `len` read of the
  indexed root (1), integer or Bool fields of record cells (2 together with
  constant marks), and helper calls whose bodies use the temporary form (3);
  about 14 sites are not order-independent reductions at all. Snowghost's
  estimate is 13 to 15 sites with the first four extensions, still short of
  16. The owner decides the follow-up on the status board.
- **Performance: holds.** On the i9-14900K, 8 workers against sequential:
  2.38 times at 256 cells (every one of 10 paired rounds at least 2.15) and
  1.88 times at 4096 cells ([measurement](../../experiments/indexed-reduction-timing/README.md#results)).
- **Soundness:** the conformance cases listed above are on main.

## Owner selections

1. Lowering strategy: A selected. B and C remain comparison alternatives.
2. Constant idempotent marks (`set flags[e] = True()`) are deferred;
   normalizing them to `bor` with a proved-`True` contribution needs a rule
   about stores that are not written as an operation.
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
