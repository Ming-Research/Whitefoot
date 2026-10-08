# Indexed reductions in counted loops

Status: question and proposal, 2026-10-08. Nothing here is a rule yet; the
owner approved investigating it as the next PAR-2 extension (ledger Q148 of
the Paged work session). The open choices are listed at the end.

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

PAR-2 denies this loop today: the write to `counts.inner[bucket]` is neither
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

## Open choices for the owner

1. Lowering strategy: A, B or C (recommended A).
2. Whether constant idempotent marks (`set flags[e] = True()`) are admitted
   by normalizing them to `bor` with a proved-`True` contribution, or left
   to a later change (recommended later; they need a rule about stores that
   are not written as an operation).
3. Whether the loop may also carry a scalar accumulator in the same body
   (recommended yes: the two recombine independently).
