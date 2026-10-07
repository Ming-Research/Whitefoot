# Address-stable paged storage

Status: direction selected by the owner on 2026-10-06 (Q122: build the
built-in `Paged<T>` with Snowghost's requirements R1 to R6 in one change);
implementation in progress on this branch. The design-tree and
specification changes await the owner's approval. The criterion below was
written before any measurement.

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

What storage gives stable element addresses, O(1) indexing, and the same
proof model as `Slots` (exact places, index separation, `apart`
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
- **R3** Growth never moves an element; only directory words move. REF-2 is
  unchanged: growth still invalidates references. No compaction, no middle
  insertion; deletion, tombstones and generations stay library code.
- **R4** `&p^[a..b]` has its own range kind, which may cross pages and is
  not a `&[T]`.
- **R5** A bridge to contiguity: a page count, and page k as an ordinary
  contiguous `&[T]`.
- **R6** The page size is fixed per element type at compile time and is a
  power of two.

R7 (adopting a private `Paged`'s pages into a retained one, for splices) and
a smaller first page for tiny owners are recorded under "Not in this change".

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
slots are stored in fixed-size pages:

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
  reallocate the directory; no element moves (R3). The row writes the whole
  cell, so growth invalidates every reference into the storage and kills its
  facts exactly as `grow` does for `Slots`; it also overlaps every element
  access, which keeps an address formation (which reads the directory) from
  overlapping a directory reallocation under [PAR-1].
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
  with `ensures result >= 1_u64` as its only fact; B is fixed per element
  type and target at compile time, so acceptance never depends on it.

### Parallel permission and range facts

- A `Paged` subscript joins [PAR-2]'s proved single-binder affine elements,
  its certified elements and [RANGE-1]'s integer-storage terms exactly as a
  `Slots` subscript does (R2).
- A page subscript `&p.pages[a*k + b]` is a proved single-binder affine
  element, as a `Segments` segment subscript is.
- A run reference `&p[s*i + b..s*i + b + s]` passed as an argument is a
  proved range reference, as a range over a `Slots` is.

### Lowering

- The descriptor in the `Box` content holds `len`, `cap`, the directory
  pointer and the directory's capacity in pages.
- B is the largest power of two with `B * stride <= 4096`, and at least 1.
  Pages hold `B` elements each; `grow_paged` allocates pages up to
  `ceil(cap / B)` and doubles the directory when it is full, copying page
  pointers only.
- An element address is `directory[i >> s] + (i & (B - 1)) * stride`: two
  dependent loads from the descriptor.
- A `&Run<T>` is passed as the directory pointer, `lo` and `len`; growth
  invalidates it, so its directory pointer cannot go stale.
- `&p.pages[k]` lowers to an ordinary `&[T]` (page pointer and its
  initialized length).
- Release drops the initialized elements in index order, frees each page and
  the directory, then the cell. Allocation sizes are checked as [STOR-6]
  states and exhaustion is a resource failure ([STOR-8]).

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

The lowering formula needs a finite choice when the actual element stride is
zero: use B = 4096, equivalent to using one byte as the page-sizing divisor,
while retaining the actual zero displacement for addresses. For positive
strides above 4096 bytes use B = 1. Directory growth starts at one entry and
doubles when full; a zero-capacity owner needs no element page. These choices
preserve logical indices, element counts and the general resource-failure
rules independently of target padding.

`Run` shares the nominal-type collision domain but admits only the direct
`&Run<T>` parameter kind, including a function-kind signature. This uses the
existing TYPEID application grammar and prevents a source nominal from
claiming the same spelling. Its ordinary derived local reference kind has no
stored value or generic-argument form. A page selector has no such declaration
entry.

## Not in this change

Each is recorded in `docs/todo.md` with its reopening condition:

- **R7 page adoption** for splices (Snowghost's P9).
- **A smaller first page** for owners that hold a few elements.
- **References that survive growth.** Elements do not move, but growth
  rewrites the directory, so a refinement needs an effect part that every
  address formation reads (as Astra's round-four proposal sketched).
- **Facts relating a slot to its page**, so a page step can be separated
  from element subscripts outside it.
- **Field-reading range terms** (`c4`), **exported structural uniqueness**,
  and **reading an unwritten scalar inside a certified loop**: separate gaps
  Snowghost's probes found.
- **FN-6 closed-term cycles**, which nested visitors need.

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
