# Indexed reductions in counted loops

Status: the owner selected the Proposal, lowering A (private copies combined
in leaf order) and choice 3 (scalar and indexed accumulators in one body); the
rule and lowering merged to main as v0.102 (#274), with the Criterion's
results under Results. Because the merged rule permitted none of Snowghost's
32 counted sites, the owner selected the further forms in
[Extensions after the count](#extensions-after-the-count) (PR #290), which
also admit the constant marks first deferred.

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
2. The original choice deferred constant idempotent marks
   (`set flags[e] = True()`). The owner reopened and selected them in
   [Extensions after the count](#extensions-after-the-count).
3. A loop may also carry a scalar accumulator in the same body; the two
   recombine independently.

## Lowering contract

The primary agent authorized Q1, extending the scalar-only lowering boundary.
The permission payload retains each checked root place, cell projection and
family kind alongside the existing optional scalar. Lowering captures a
nonowning range, its source/private mode and its initialized cell count at
loop entry; the IR carries the projection and the reduction operation/identity
or mark constant. Their lifetime is structural: the site owns
private storage until every splitter call joins, and the outlined body only
borrows its assigned cells. Nested outlined loops transport the enclosing
private root mappings through their captures.

For roots without call-updated families, each family uses one allocation
containing one dense cell range per leaf,
identity-filled for reductions or false-filled for marks. The recursive
splitter halves those ranges with its iteration range. After the join,
reductions visit flat cell offsets in leaf order followed by cell order;
marks OR each cell's masks in leaf order and conditionally store its constant.
Both combine into the current destination projection before freeing the slab. The scalar seed/result uses
its existing independent recombination. A sequential world or zero-budget
split passes the original root ranges and allocates no private storage.

Pricing charges three traversals of each family's leaves times initialized cells for copy,
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
Pruning retains the indexed ranges, storage modes and initialized counts consumed by the
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
checks pass in the gate of #274's merged head; the criterion results are under
Results above. Paged remains deferred until PR #263 lands on main.

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

The architecture owner authorized this interface extension in the continuation
request: family kind `Reduce { op, identity }` or `Mark { constant }`, dense
private values, and a destination projection with the root's element stride
and the field's byte offset and type. This supersedes the earlier interface
stop; no other interface gap has been routed around.

The checker retains the checked root, field ordinals, scalar type and kind.
An operation remains direct or one immutable single-use temporary. A mark is
an integer/Bool literal or named constant; equal typed values agree even when
names differ. Root occurrence counting aggregates the permitted cell operands
across sibling field families and requires zero cell reads for marks. The
root-wide policy still denies an extra read of another record field, a
whole-element write, a root-derived index, differing mark values, and a mark
mixed with an operation on the same family. An affine sibling field is brought
under the indexed policy when any family needs reduction, preventing a shared
write beside private storage.

`IrIndexedReduction` carries the family kind and `IrIndexedProjection`,
whose root type and field ordinals resolve to byte stride and offset through
the backend's target layout (LLVM constant GEP expressions). A whole-element
projection has offset zero. The capture record transports its pointer/count
and a private-storage Bool; sibling families share the captured initialized
root length. The dedicated indexed address and range operations prevent an
ordinary source-element stride from addressing a dense field or mask slab.
Operand remapping, capture pruning and refused-frame splicing retain that
mode as well as the range and count. Before forming an inbounds address,
emission selects the correct stride and offset, avoiding an invalid unused
source-layout pointer into a smaller private slab.

Each mark leaf writes true to a false-filled mask. After joining, the site
ORs one cell's masks in leaf order and writes only if any mask was set. An
outermost store writes the typed constant, including zero; a nested join
sets the enclosing mask. Reduction fields use identity-filled dense values
and combine into `root + i * element_stride + field_offset`, or the enclosing
dense field slab. Source execution and zero budget retain direct source
writes. Checked allocation sizes, failure release of previously acquired
slabs and STOR-8 termination remain in the same acquisition path. Pricing
charges every family's private cells even when their length capture is shared.

The PAR-2 amendment changes only permission and sequential-equivalence
reasoning, with no grammar or token changes. Before, indexed cells had to be
whole integer/Bool elements with an operation; after, a fixed field projection
can be a family and a fixed typed constant can mark it. The owner's explicit
extension/interface selection supplies the ground; no additional direction
choice was made. The former deferred Bool-mark conformance witness is renamed
positive and now checks its result. Five negative permission fixtures retain
ordinary accepted-source verdicts. Positive fixtures cover equal named and
literal marks, zero marks, Boolean marks, and two record-field operations.

`tests/programs/parallel/indexed_marks_fields.wf` owns independent literal
oracles: 262144 contributions over five cells give counts 52436 in cells zero
through three and 52435 in cell four from a seed of seven; untouched cells
retain seven. Marks start at nine or false and check every touched/untouched
cell; a u64 mark changes 987654321 to 123456789 to distinguish destination
stride and store width from the one-byte mask. Record tags stay 93. The nested
case expects 262151 in one counter, zero in one marked byte, one marked Bool
field, an affine-only sibling mark in each outer iteration, and unchanged
siblings. A separate conformance function gives sibling fields two different
affine maps. Backend observers force four
leaves, require allocations and releases, check zero budget, and fail each of
the two sibling-field slab acquisitions. Lowering assertions pin one shared
length capture and separate scalar projections. The ordinary program test
also runs the fixture's sequential and unmodified-runtime parallel builds.
These are authored execution tests, not executed results in this worktree.

### Found along the way

- Fixed: selecting families per field must also select an affine sibling
  family when their maps differ or any family needs a private reduction;
  otherwise a sibling write could escape the indexed root's policy.
- Fixed: the permission ledger now names constant marks when there are no
  binary operations, instead of reporting no accumulator.
- Pending qualification: the existing TODO entry is updated from an interface
  stop to CI qualification. Paged storage and the downstream recount remain
  deferred on their previous grounds.

### Verification status

All five extensions are on PR #290's branch with the specification at v0.106
and the approval logged in spec/log.md and design/log.md. Source acceptance,
intended denials, IR validity and native values are established by that PR's
gate run on its merged head; nothing was built or run locally. Snowghost's
recount of its 32 candidate loops with these forms is recorded under
[Copied cells and helper calls](#copied-cells-and-helper-calls).

The two paragraphs below record the implementation-time review of the
uncommitted extensions 3 and 4, before they were committed.

Independent read-only completion review covered the full working-tree diff
against `64f48f06cdd8d332d05865d75bf4c912fe80429f`, its untracked fixtures,
and affected consumers using the inherited Codex model (exact runtime model
identifier unavailable). It considered review groups A, D, C, T and V plus
G1–G3/DC1–DC4, with no group skipped wholesale, using only git state/diff and
source inspection. No suites or design-lint statistics were collected.

The review found one missing regression observation: the new selection of an
affine sibling family and of two different affine field maps had no fixture.
The native outer loop now marks `rows[outer].affine` beside colliding inner
updates; its observer requires 22 allocations and 36 leaves, which would
change if the affine family were omitted. The positive field fixture adds
`different_maps` with `count[i]` and `enabled[i+1]`, and semantic/lowering
assertions require both families and their shared count. The reviewer inspected
these fixes and found no remaining issue within scope. These expected counts
are test assertions, not observed results. Typechecking, ordinary WF checking,
LLVM validation, native observations and full safety qualification remain
unverified. No additional architecture decision was required.

## Copied cells and helper calls

### Recount after the extensions

Snowghost-wf recounted the same 32 loops with release `wf-64c0f956df63`
(main `64c0f956d`, v0.107): 11 permitted as written and 11 with local helper
narrowing, against 0 and 0 before the extensions. The permission criterion
(at least 16) still fails, five short. The per-site record is Snowghost-wf
revision `5b8e728`,
`research/investigations/storage-layout/par-classification/g-indexed-290.md`
(report run 37887019421, gate run 37888369049). By first denial, the 21
remaining sites are:

- four copied-cell chains, `let old = R[e]; let next = old op x; set R[e] =
  next;`, two in `set_coverage_bits`, one in `mark_lookups` and one in
  `sort_run`, while the one-step `let next = R[e] op x;` is admitted;
- two loops whose updates happen inside a helper reached through a whole-root
  reference argument, `build_filters` calling `set_coverage_bits` and
  `collect_stage` calling `mark_lookups`; the helpers' own updates are the
  copied-cell chains above;
- fifteen sites that are not indexed reductions under the fixed operation
  set: whole-record get/put (5), a hidden-dominates-maximum selection (3), a
  key-and-payload winner (3), a check of the existing cell before writing
  (1), a scalar constant mark with an error exit (1), an exact `+` with a
  guard and error exits (1) and a `NodeId` record overwrite (1).

### Selected direction

The owner selected both remaining forms on the status board (card
`paged-274-recount`, option A, 2026-10-09), following the earlier selection
to design helper-call updates if the extensions stayed below 16:

1. **Copied cell.** The accumulator operand of an operation update may be a
   fresh immutable single-use copy of the same cell, `let c = R[e]`, made
   earlier in the same block. The copy observes the cell exactly where the
   direct operand would, and no statement between them may touch the root,
   so the update computes the same value as the admitted direct form.
2. **Helper calls.** A call is an update of an indexed family when its
   reference argument reaches the root and the callee's whole body treats
   that parameter as PAR-2 treats an indexed accumulator: every occurrence is
   a measure read or an update of a family below it, with one fixed kind per
   family. The callee's net effect on the root is then a multiset of cell
   contributions under one associative, commutative operation (or a set of
   cells receiving one constant), independent of the root's contents, so
   iterations may still be recombined in any order. A callee summary may rely
   on its own call-form updates only through an acyclic chain of callees.

The fifteen other sites need user-defined associative operations, selection
with payload, or reads of partial results; they stay outside this rule.

### Lowering

A call passes a reference, so its callee indexes a private copy by the
root's own logical indices and may read its measures. If any family of a root
has a call update, every family of that root shares one block per leaf
with the root's storage shape and element stride, carrying the root's
measures, with identity in the family's cells; the leaf passes a reference to
its own block. A constant mark fills its cells with a sentinel that differs
from the constant (the constant with its lowest bit inverted), since the
callee writes only the constant there; the join stores the constant where a
leaf's cell holds it. Roots without call-updated families keep dense
per-family private slabs.

### Criterion

Recorded before implementation: with both forms, Snowghost's recount of the
same 32 loops admits at least 16 as written or with local helper narrowing.
The first-denial classification predicts 17 (the four copied-cell loops and
the two helper-call loops); fewer than 16 rejects this extension as
insufficient for the criterion, and a site denied for a condition hidden
behind its first denial is reported with that condition. Performance of the
call form is not claimed; the indexed timing comparison above measured the
direct form only.

### Result

**Permission: passes.** Snowghost-wf recounted the same 32 loops with
experiment release `wf-exp-68fe93be539c` (this branch's checker at
`68fe93be5`, v0.109 draft; lowering still sequential for call-form families
at that revision): 17 permitted as written and 17 with local helper
narrowing, none needing narrowing, against 11 at v0.107 and 0 at v0.102.
The six new admissions are exactly the predicted ones: the four copied-cell
loops (`set_coverage_bits` twice, `mark_lookups`, `sort_run`) and the two
helper-call loops (`build_filters`, `collect_stage`). Five loops changed
their first denial and stay denied (`grid_place`, `measure_rows` and the
three `collapse_borders` loops); there is no new denial. One observed
boundary: a helper that replaces a cell with an arbitrary value
(`put_i32`'s `set list^.inner[at] = value;`) carries no summary, as the rule
intends. Record: Snowghost-wf revision `4a9258b`,
`research/investigations/storage-layout/par-classification/g-indexed-exp68.md`
(check run 37904741220, report run 37904741267, both on `28f6ae7`). The
count measures permission only, not runtime overlap or speed.
