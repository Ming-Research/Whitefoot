# Address-stable paged storage

Status: direction selected by the owner on 2026-10-06 (Q122: build the
built-in `Paged<T>` with Snowghost's requirements R1 to R6 in one change);
the built-in form is implemented. The owner selected ruling B on card
`paged-small-owners` on 2026-10-11; the small-owner amendment below is the
current implementation direction. The original C1 to C3 criterion below
was written before its measurements.

## Question

Snowghost's layout keeps per-owner payloads (blocks, paragraphs, child
contexts) whose identities are stable slot numbers, and its owner decision
Q114 A requires that an insertion copy no earlier payload. Whitefoot offers
two kinds of indexed storage, and neither fits:

- `Slots<T>` is contiguous, so growing it copies every payload and
  invalidates every reference into it.
- A recursive page tree written in source (Snowghost's `SlotPages`, an enum
  of `Box` pages and `Fork` nodes) never copies a payload, but every access
  is an O(log n) descent, the descent cannot be abstracted without a
  returned reference or a callback, and whole-node copies through generic
  helpers cost 29.7% of html5 full-layout samples on the i9-14900K before
  Snowghost split its nodes into separately paged field groups.

What storage gives stable element addresses after bounded initial growth,
O(1) indexing, and the same proof model as `Slots` (exact places, index separation, `apart`
certificates, parallel permission), without returned references?

## How the question was reached

Q122 began as "functions returning references" (Snowghost-wf `docs/todo.md`
on `research/m2-layout`, first item of "Whitefoot requirements"). Four rounds
of design, each with independent agents and adversarial review, rejected:

1. **Returned references with a declared source** (`-> b: &Block in pages`).
   Two review rounds found thirteen classes of corner cases: readonly
   laundering through a module's inside view and through function-kind
   refinement, another argument's write or `move` destroying the target,
   whole-extent versus anchor validity, reassigned index parameters, range
   exactness, multi-operand return order, multi-source alternatives,
   termination of chained containment, the backend's `nocapture`
   justification (`compiler/src/backend/emitter.rs`, which rests on REF-3),
   exact-versus-descendant sources, and transfer of facts and rebinding.
   They have one cause: a place that flows outward reaches a frame that did
   not form it, so every rule that reads places needs a transfer clause. A
   returned reference also has no answer for "not found".
2. **Visitors through function-kind parameters**, which keep REF-3 and are
   expressible today (`hash_map_edit` in
   `lib/std/collections/hash_map/hash-map.wf`). They leave one view at a
   time, coarse rows, re-descents for two-element operations, and FN-6
   refuses nested composition with a changed callback.
3. **Caller-expanded projections with a checked key-partition proof**: seven
   new rules, a new proof judgment and a new path step, and the storage still
   costs O(log n) per access.

The owner's conclusion: the root problem is storage, not references. With
storage whose index is one ordinary step, the caller forms every place
itself and none of the thirteen classes can arise.

## Requirements (Snowghost, 2026-10-06)

Stated by Snowghost-wf after the owner reviewed its mock API:

- **R1** A separate type `Paged<T>`. `Slots<T>` stays unchanged, the
  contiguous, fastest, relocating vector: a function that wants contiguity
  and one that wants stable positions state different intent.
- **R2** One-level index semantics: `p^[i]`, `p^[i].f` and `p^.len` are
  ordinary sequence places; RANGE, `apart`, PAR-2 and REF-4 treat them
  exactly as `Slots` elements; page arithmetic exists only in lowering.
- **R3** Under the owner's 2026-10-11 small-owner ruling, growth may move
  only page 0 before its allocated length reaches B, copying at most B - 1
  initialized elements in total across its replacements. Full-sized pages
  retain every element address. REF-2 is unchanged: growth still invalidates
  references. No compaction, no middle insertion; deletion, tombstones and
  generations stay library code.
- **R4** `&p^[a..b]` has its own range kind, which may cross pages and is
  not a `&[T]`.
- **R5** A bridge to contiguity: a page count, and page k as an ordinary
  contiguous `&[T]`.
- **R6** The page size is fixed per element type at compile time and is a
  power of two.

R7 (adopting a private `Paged`'s pages into a retained one, for splices)
remains under "Not in this change"; any future adoption admits only full
pages. The smaller first page is selected below.

### Evidence from Snowghost's probes

Snowghost-wf branch `research/storage-mocks` at `44f616b`,
`research/investigations/storage-layout/probes`, CI run 37563423924 with the
released compiler `wf-f949e676acfa`:

- `c1-native-pages.wf`: hand-written two-level pages
  `Box<Slots<Box<Slots<T>>>>` grow by copying only page pointers (checked in
  the emitted LLVM), but a dense loop over a flat slot writing
  `&pages^.inner[s / B].inner[s % B].field` is denied ("condition 2: the body
  writes storage that is neither introduced by the iteration nor the
  accumulator"); nested page and offset loops are permitted. A source library
  keeps the performance property and loses R2.
- `c4-field-inverse-negative.wf`: a range term reading a field below an
  element is refused (`RANGE-1: InvalidRangeClause`), a separate gap.
- `c5-changing-callback-negative.wf`: FN-6 refuses re-entry with a changed
  callback (`PolymorphicRecursion`).

## Design

### Language surface

`Paged<T>` is a fifth storage shape, a window in the sense of [WIN-1] whose
slots use uniform logical page coordinates with the backing lengths below:

```whitefoot
opaque nocopy struct Paged<T> {
  readonly len: u64;
  readonly cap: u64;
}
```

- **Placement.** Runtime-capacity only: its home is the content of a `Box`,
  as for the runtime-capacity forms and `Segments` ([TYPE-9]).
- **Construction.** `box_paged_new<T>(capacity: u64) -> result: Box<Paged<T>>`
  starts empty with `cap == capacity`, like `box_slots_new`.
- **Growth.** `grow_paged<T>(cell: &Box<Paged<T>>, capacity: u64)` with the
  row `writes(cell)` and the contract of `grow`: `capacity >= cap` before,
  `cap == capacity` and `len` unchanged after. It allocates pages and may
  reallocate the cell that holds the directory, and may replace the small
  first page under R3. The row writes the whole cell, so growth invalidates
  every reference into the storage and overlaps every element access. Fact
  preservation across relocation is the separate Whitefoot PR #352 dependency
  described below; the base specification still kills overlapping facts.
  Lowering additionally cuts an overlap group before forming a Paged address through the owner or directory, because reference formation
  itself has no source content read under [PAR-1].
- **Window operations.** `place_back` and `take_back` admit `Paged<T>` as
  their window argument with their existing records, rows and contracts
  (`place_back` requires `len < cap`). `insert_at`, `remove_at`, `append`,
  `split_off`, `place_front` and `take_front` do not (R3; R7 is deferred).
  `free_empty` admits `Box<Paged<T>>`.
- **Window parts.** `next`, `last`, `filled` and `free` mean what [WIN-2]
  states, so a `place_back` preserves references to filled slots and the
  facts about them, as for `Slots`.
- **Subscript.** `p[i]` is an element subscript with the obligation
  `i < p.len` ([OP-4]); the logical offset i selects page `i >> s`, offset
  `i & (B - 1)`, an injective map ([MSR-1]).
- **Measures.** `len` and `cap`, both exact; no `head`.
- **Run references (R4).** `&p[lo..hi]` over a `Paged` forms a run
  reference of kind `&Run<T>` under [REF-4]'s obligation
  `lo <= hi <= p.len`. Its one measure is `len == hi - lo`; it admits element
  subscripts and re-slicing into another `&Run<T>`; as a resolved place it is
  `p` extended by a range step, separated and overlapped exactly as a range
  over a `Slots`. `&Run<T>` is a reference kind ([TYPE-8]), written only as a
  parameter kind; `Run` is a prelude name, not a type. A `&Run<T>` is never a
  `&[T]` and a `&[T]` is never a `&Run<T>`.
- **Pages (R5).** `p.pages` is not a field and occupies no declaration
  domain, as `Segments`' `all`. `p.pages.len` is a measure term, the number of
  pages holding initialized elements, supported by the descriptor word `len`.
  `&p.pages[k]`, under the obligation `k < p.pages.len`, forms an ordinary
  `&[T]` over page k's initialized elements. As a resolved place it is `p`
  extended by a page step capturing k: two page steps are separated when
  their offsets are proved distinct, a page step is contained in `p.filled`,
  and it overlaps every element subscript and range of `p`, because no term
  relates a slot to its page.
- **Page size (R6).** `paged_page_len<T>() -> result: u64 pure` returns B,
  with `ensures result >= 1_u64` as its only fact. B is fixed per element
  type by the language (see Lowering), the same on every target, so neither
  acceptance nor observable behavior depends on the target.

### Parallel permission and range facts

- A `Paged` subscript joins [PAR-2]'s proved single-binder affine elements,
  its certified elements and [RANGE-1]'s integer-storage terms exactly as a
  `Slots` subscript does (R2).
- A page subscript `&p.pages[a*k + b]` is a proved single-binder affine
  element, as a `Segments` segment subscript is.
- A run reference `&p[s*i + b..s*i + b + s]` passed as an argument is a
  proved range reference, as a range over a `Slots` is.

### Lowering

- A capacity-0 `Box<Paged<T>>` owner points at the shared immutable empty
  header and allocates nothing. A positive-capacity owner is one pointer to
  one header-first allocation:
  `{ len: i64, cap: i64, dircap: i64, pages: [dircap x ptr] }`. The directory
  starts at `cell + 24`; it has no separate pointer or allocation.
- B is the largest power of two with `B * stride_ceiling(T) <= 4096`, and
  at least 1, using [OP-9]'s language stride ceiling so that B, which a
  program can observe, is the same on every target (owner ruling Q135);
  the actual stride, at or below the ceiling, addresses the elements.
  Page 0 is allocated for the least power of two covering positive capacity,
  capped at B; later pages hold B elements each. Positive construction doubles
  `dircap` from one until it covers `ceil(cap / B)`, then allocates the cell
  and those pages.
- Positive `grow_paged` from the shared header allocates its first cell and
  pages. Growth retains an allocated cell while its directory has room. When
  it needs more entries, it doubles `dircap` until it fits, allocates a new cell,
  copies the header and existing page pointers only, frees the old cell and
  stores the replacement through its `&Box<Paged<T>>` parameter. It then
  replaces page 0 when the needed power-of-two length exceeds its old length
  below B, copying only initialized elements, and allocates the missing full
  pages. It records the exact requested capacity. Once page 0 reaches B, every
  existing page is retained. `place_back` and `take_back` never reallocate the
  cell or a page.
- An element address loads the page pointer at `cell + 24 + 8 * (i >> s)`,
  then adds `(i & (B - 1)) * stride`: one dependent load from the cell,
  matching the hand-written header-first page table.
- A `&Run<T>` is passed as the directory pointer `cell + 24`, `lo` and
  `len`; growth invalidates it before replacing the cell, so its directory
  pointer cannot go stale. The directory pointer retains `nonnull`,
  `readonly` and the ordinary no-capture boundary, with no `noalias` or
  `dereferenceable` promise; readonly concerns accesses through this pointer,
  not a separate allocation.
- `&p.pages[k]` lowers to an ordinary `&[T]` (page pointer and its
  initialized length). Parallel captures retain the same owner and run
  forms, and overlap lowering joins before a later argument forms a Paged
  address through an owner pointer or directory that growth may replace. A
  call taking a Paged cell reference ends its overlap group: even a pure
  callee that ignores it must enter while the cell still exists for its
  ordinary reference attributes. This covers later growth inside a wrapper
  and a refused hand-out executed at the join.
- Release drops the initialized elements in index order and frees each page
  at its actual allocated extent, deriving page 0's length from capacity;
  the Box then frees the cell, which includes the directory. Release of the
  shared header frees nothing. The cell size
  `24 + 8 * dircap` and each page size use checked arithmetic against the
  target allocation maximum [STOR-6]; exhaustion is a resource failure
  [STOR-8]. A positive-capacity zero-stride page uses one allocation byte and
  zero element displacement.

## Boundary choices for specification and lowering

The Slots-equivalence direction also fixes the boundary cases: zero capacity
constructs an empty owner, growth to the same capacity is admitted, the public
capacity is exactly the requested value rather than the rounded page storage,
and a decrease fails the growth requirement. Growth's whole-cell write still
invalidates references at equal capacity. Back removal keeps allocated pages
for later placement and follows Slots' conservative invalidation of selected
window references.

A page reference captures only the initialized part of its page at formation.
Appending preserves that extent without extending its captured length, as for
a Slots range. The page count is supported by the owner's length word; a
read through a parameter therefore needs `reads(p.len)`, as does page
formation when it captures the initialized extent, and a length write
kills a fact about that count. This does not introduce facts relating page
indices to element indices. The page loop's mapped storage is its pages;
page formation's read of the separate length word remains an ordinary read,
which conflicts with an append but not with writes of page elements.

B takes the language stride ceiling, which is at least one and is fixed by
OP-9's table independently of the actual representation, so a zero actual
stride needs no special case: its addresses keep zero displacement whatever B
is, and a stride ceiling of one, as for `u8` or `unit`, gives B = 4096. A
stride ceiling above 4096 gives B = 1. Directory growth starts at one entry and
doubles when full; a zero-capacity owner needs no allocation. These choices
preserve logical indices, element counts and the general resource-failure
rules independently of target padding.

`Run` shares the nominal-type collision domain but admits only the direct
`&Run<T>` parameter kind, including a function-kind signature. This uses the
existing TYPEID application grammar and prevents a source nominal from
claiming the same spelling. Its ordinary derived local reference kind has no
stored value or generic-argument form. A page selector has no such declaration
entry.

## Not in this change

These capabilities are outside the selected small-owner amendment:

- **R7 page adoption** for splices (Snowghost's P9). Any future operation
  adopts only pages containing B initialized elements, whose backing therefore
  holds B elements; page 0 is filled to B before adoption.
- **References that survive growth.** Growth can move the small first page
  and rewrites the directory, so a refinement needs an effect part that every
  address formation reads (as Astra's round-four proposal sketched).
- **Facts relating a slot to its page**, so a page step can be separated
  from element subscripts outside it.
- **Field-reading range terms** (`c4`), **exported structural uniqueness**,
  and **reading an unwritten scalar inside a certified loop**: separate gaps
  Snowghost's probes found.
- **FN-6 closed-term cycles**, which nested visitors need.

## Small owners: shared empty header and a growing first page

**Question.** Can per-owner Paged storage avoid empty-header allocations and
full first pages while retaining uniform page addressing and bounded growth
copying? The owner selected option B on card `paged-small-owners` on
2026-10-11. The prior R3 prohibition on moving any element is replaced by
bounded relocation of page 0; stable slot identity and the existing REF-2
invalidation remain.

**Measured motivation.** Snowghost reported one ecma262 page with 34,302
blocks and 10,217 contexts, each owning two initially empty Paged instances:
89,038 extra header allocations and 100,466 extra full pages under the
4096-byte page budget. Its layout-node stride ceiling is 528 bytes, so B = 4.
The page budget does not imply that each actual page occupies 4096 bytes:
its allocation is B times the selected target stride. Reported peak memory
rose from 860 MB to 903 MB and full layout was 3.3% slower. These are
Snowghost's observations, supplied with the ruling; Whitefoot has not
remeasured them. The source is Snowghost-wf branch
[`research/m2-paged-mechanism`, `MECHANISM.md`](https://github.com/Ming-Research/Snowghost-wf/blob/research/m2-paged-mechanism/research/investigations/m2-paged/MECHANISM.md).

**Mechanism.** Capacity 0 points at the shared immutable empty header and
allocates nothing; first positive growth allocates. At positive capacity
below B, page 0 reserves the least power of two covering that capacity.
Growth beyond its allocated length replaces it and copies its initialized
elements, until its allocated length reaches B. The increasing lengths are
powers of two, so the sum of possible copied elements is at most
`1 + 2 + ... + B/2 = B - 1`; B = 1 needs no page replacement. Subsequent
growth appends full pages and may replace the directory cell, retaining
every existing element address. Logical capacity remains the requested
count, page 0 starts at logical index 0, and addressing remains page
`i >> s`, slot `i & (B - 1)`, with no additional per-index test. Release
skips the shared header and frees a small page 0 at its actual size.
Snowghost confirms that it keeps neither a page reference nor an element
address across growth; its block identity is a slot number.

**Fact-preservation dependency.** Whitefoot [PR #352, growth retains
unchanged facts](https://github.com/Ming-Research/Whitefoot/pull/352), on
`claude/grow-keeps-facts`, is approved but not in this amendment's base.
Its ENT-5 relocation classification is by the canonical `grow_paged`
operation and preserves support in the unchanged length and filled element
storage, including owned descendants, while reference validity still
follows REF-2. A page-0 replacement changes storage addresses without
changing those values, so the same classification applies after #352 lands;
no separate element-equality premise is needed. OP-10 identifies the move
as a relocation and leaves fact survival to ENT-5. The dependent range-fact
witness must be enabled and pass after #352 is integrated; this amendment
does not import that branch's broad proof-system changes or claim that its
base preserves those facts.

**Remaining comparison and veto.** Snowghost must retest
[Snowghost-wf PR #61, the Paged mechanism](https://github.com/Ming-Research/Snowghost-wf/pull/61)
on hosted timing `m2-paged-time`, comparing the same full-layout workload
with its prior storage mechanism and the revised Paged mechanism under the
same conditions. The small-owner proposal is vetoed when full layout is
more than 3% slower. The prior 3.3% loss does not establish the revised
mechanism's result; allocation savings, peak memory and full-layout timing
remain to measure.

## Alternatives rejected

- **A library two-level page table**: keeps stable addresses but loses R2;
  the flat-slot loop is denied (`c1`). Restoring it would need quotient and
  remainder facts (distinct s gives distinct `(s / B, s % B)`) in the
  permission and range rules, a second, multi-step separation argument
  instead of the one index step every rule already reads.
- **Stable growth inside `Slots`**: one type for two intents (R1); every
  `Slots` access would pay the directory load and lose contiguous `&[T]`
  ranges.
- **`grow` preserving references through `Box` content**
  (`Slots<Box<T>>`): one allocation per element, scattered payloads, and
  pointer copies on growth.
- **A radix directory with references surviving growth**: more rules
  (a directory effect part read by every address formation) for a property
  R3 does not require.

## Criterion

Recorded before any measurement. Snowghost ports its `research/m2-pages`
experiment, which uses hand-written two-level pages, to `Paged<T>` from an
experiment release of this branch, and runs it on the i9-14900K.

- **C1, permission.** The dense flat-slot writing loop of `c1-native-pages`,
  rewritten over `Paged`, is permitted by [PAR-2], and so are Snowghost's
  dense payload passes after the port.
- **C2, no payload movement.** In emitted LLVM, `grow_paged` copies
  directory words only, never an element.
- **C3, access cost.** html5 full-layout time with `Paged` is within the run
  noise of the hand-written two-level pages, measured with interleaved runs
  and a twin of the base as a noise control.

The design is rejected if C1 fails for any reason other than an
implementation defect, or if C3 shows `Paged` access materially slower than
the hand-written pages. Either would send the question back to a library
page table plus quotient and remainder facts.

### C3 result and header-first response

2026-10-07: Snowghost reported i9-14900K run `37610930210` with a
built-in/hand-written geometric mean of **1.061** and a base-twin ratio of
**0.991**; layout was up to 12% slower and box construction up to 42% slower.
This fails the original C3 criterion for the separate-directory lowering.
Code inspection identifies one structural difference: the built-in access
loaded `dir_ptr` from its descriptor before loading a page pointer, while
`Box<Slots<Box<Slots<T>>>>` stored its page pointers after the header. The
header-first cell above removes that extra dependent load and is the owner's
selected response to be measured against the same criterion. The C3 result
does not yet measure this response or isolate the whole loss to that load.

### C3 header-first measurement on the M5 Air

2026-10-07, with the i9-14900K out of service and the owner's approval to
time on the M5 Air under the host lock. Snowghost-wf commit `ddba8ab`,
[`research/investigations/storage-layout/c3-m5/`](https://github.com/Ming-Research/Snowghost-wf/tree/ddba8ab/research/investigations/storage-layout/c3-m5)
holds the harness, every measurement and the build pins: macOS arm64
drivers from CI run `37710808694`, every build warmed once, then three
rounds running every build once in turn; per-run time is
`(T(REPS) - T(0)) / REPS` (boxes 30, layout 10) and each cell takes the best
round. `pages` is the hand-written two-level page table and `twin` the same
driver timed again; `paged` is the port on the separate-directory lowering
(`wf-exp-496186df5346`), `hf` the same source on the header-first cell
(`wf-exp-f1971c00269a`).

| layout, ratio to `pages` | twin | paged | hf |
|---|---|---|---|
| html5 sequential | 0.998 | 1.015 | 0.980 |
| html5, four workers | 1.041 | 1.054 | 1.049 |
| ecma262 sequential | 1.006 | 1.002 | 1.003 |

Box construction: hf 0.994, 0.937 and 1.005 against twin 1.001, 1.010
and 1.013. The C3 criterion holds for the header-first cell on this
machine: every html5 layout cell is within the twin's spread. The run does
not show the separate-directory lowering failing C3 here, since its
four-worker and ecma262 cells are also within the spread, and so it does not
attribute a recovery to the cell layout; the `hf` build also carries the
more conservative overlap cut at calls taking `&Paged<T>`, which the
four-worker cell confounds with the layout. The evidence is moderate: the
third round ran 10 to 40 percent slower throughout, consistent with thermal
throttling, which the best-of-rounds summary discards, and Air times are not
comparable with the i9-14900K's. Snowghost repeats the run on the
i9-14900K with performance counters and page-fault counts. A like-for-like
port with one `Paged` per existing store stays 1 to 9 percent slower with
the header-first cell; that its 82,907 per-owner stores with full first
pages cause this was a hypothesis for the
[small-owner investigation](#small-owners-shared-empty-header-and-a-growing-first-page),
which the page-fault counts can reject.

### C3 on the i9-14900K: both lowerings fail

2026-10-08, run `37725144848` on the i9-14900K, the same builds, three
interleaved rounds, best round per cell; Snowghost-wf commit `5b517e7`,
[`research/investigations/storage-layout/c3-14900k/`](https://github.com/Ming-Research/Snowghost-wf/tree/5b517e7/research/investigations/storage-layout/c3-14900k)
holds every round, the page-fault counts and box-build profiles. Ratios to
`pages`, recomputed from the committed rounds:

| layout | twin | paged | hf |
|---|---|---|---|
| html5 sequential | 1.000 | 1.093 | 1.098 |
| html5, four workers | 1.009 | 1.114 | 1.076 |
| ecma262 sequential | 0.990 | 1.031 | 1.021 |
| ecma262, four workers | 1.010 | 1.079 | 1.049 |

The html5 sequential rounds are tight (pages 0.683 to 0.687 s, both
built-in builds 0.747 to 0.757 s), so C3 fails on the precise machine for
the separate directory and for the header-first cell alike, and this run
supersedes the M5 Air reading above. Box construction is noisier (twin 0.91
to 1.10) and slower still for both (1.06 to 1.58). Its profiles are
dominated by the allocator in every build, more so for the built-in ports,
and the html5 box build takes 123,490 page faults with `pages`, 186,919
with `paged` and 292,328 with `hf`; the header-first cell adds a cell
reallocation on every directory doubling. The layout loss is not yet
attributed: no layout-phase profile was taken, and the port also moved the
payload pools onto `Paged`. Under the criterion this rejects the built-in
form unless an attributed, fixable cause is found; the owner decides.
