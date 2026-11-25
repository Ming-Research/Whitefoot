# Chunk building in the stable scatter

## Question

The stable scatter
([`radix_scatter.wf`](../../../tests/programs/compute/radix_scatter.wf)) keeps
each block's two runs in an `Option<Chunk>`, a tag and two `Slots<u64, 256>`,
4,120 bytes. It appends one empty slot per block and then writes each block's
runs. On 2026-09-21, on an Apple M1 Pro, this chunk building was the largest
gap to a native chain of the same decomposition (0.596 ms against 0.186 ms of
a 1.930 ms call at eight workers), and the optimized append loop cleared 4,112
bytes and copied 4,120 for every empty slot
([joined-phase result](../compute-model/DESIGN.md#joined-phase-result-2026-09-21)).
The compiler has changed since: frame roots have their own allocations, and a
constant-capacity window is built header-only. Three things are open on the
present main line:

1. Which whole-block clears and copies does the optimized code still hold, in
   which functions?
2. How much of the whole call do they cost at one and at eight workers?
3. Which lowering removes them without changing the source?

One candidate was refused before. On 2026-09-23 the owner declined leaving an
enum's inactive payload unwritten at construction, after the owning hash map's
wide replacement and the slab's retained scalar lookup ran slower with it
([same-source comparison](../../experiments/container-representation/map-library/RESULTS.md#completed-comparison-gains-with-unresolved-regressions)).
That comparison ran on the M1 Pro, before separate frame allocations and
layout-bounded transfers, and it also left empty windows unwritten, which main
has done since 2026-10-09. Its two regressions are the consumers a new
proposal of that lowering has to measure again.

## Probes and the criteria fixed before the first result

The probes are two ignored tests in `compiler/src/backend/tests/ranges.rs`
(`stable_scatter_probe_reports_clears_and_copies` and
`stable_scatter_probe_times_whole_calls`, commit `94696cfad`). They fail on
purpose so that their report reaches the test result, and they leave the
branch before it merges. The source is the maintained program, unchanged,
built with `--par`.

Besides the ordinary emission (`main`), each probe builds variants by editing
the emitted module's text before the host optimizer runs. A variant shows what
the optimized code and the time of this program would be under a lowering; it
does not show that the lowering is correct in general. A variant whose module
the host refuses, or whose result the independent oracle refuses, is named in
the report and left out.

| Variant | Edit |
| --- | --- |
| `twin` | none: the same module again, for the run's spread |
| `no-clear` | the whole-value clear before each empty `Option<Chunk>` is removed; the tag store stays |
| `headers-only` | that clear is cut down to the two run lengths |
| `tag-only` | `no-clear`, and each whole `Option<Chunk>` transfer in `radix_scatter` moves only the tag byte |
| `in-place` | another source: the block writer stores an empty chunk and partitions into it through a reference |
| `in-place-tag-only` | `in-place` with the `tag-only` edits |
| `sequential` | none: the same source built without `--par`, timed only |

`in-place` is a control for what the block writer's value transfers cost. It
is not a proposal to change the program: a redundant transfer of a by-value
form is a compiler gap, and a changed spelling would hide it.

Structure: every constant clear or copy of at least 2,048 bytes (one run's
payload) in the host-optimized module, by function, for each variant.

Time: one call scatters 1,048,593 mixed keys. A process makes one unreported
call and 20 timed ones, checked against the independent oracle outside the
timed interval, and contributes the median wall and process CPU time. Nine
passes run every image once at 1, 8 and 32 workers, rotating the image order.
Each pass also times `main` on the 8,209 keys of the program's own entry, 200
calls a process, to tell a slow call apart from the process startup that the
[par-suite baseline](../par-suite/DESIGN.md#baseline-2026-10-09) includes in
its 4.86 at 32 workers.
A variant's per-pass ratio is its median over `main`'s in the same pass and
width. The interval is the distribution-free 95% interval of the median of
those ratios, as `median_interval` of
`research/experiments/hash-map-compare/compare.py` defines it: with nine
ratios, the second smallest and the second largest. A width is usable when the
twin's interval contains 1.

Fixed before measuring:

- **The empty slots matter** if `tag-only`'s wall interval at eight workers
  lies wholly below 1 and its wall and CPU intervals at one worker do not lie
  wholly above 1. If its interval at eight workers contains 1, removing the
  append's whole-block work has no effect this instrument resolves, and the
  item's first outcome cannot rest on it.
- **Leaving the inactive payload unwritten is enough for the append** if the
  optimized `no-clear` module holds no constant clear or copy of at least
  2,048 bytes that the optimized `tag-only` module lacks, and `tag-only`'s
  note counts at least one shortened transfer. `tag-only` has no whole
  transfer of an appended slot by construction, so what it keeps is the block
  writer's or the entry's, in whichever function the optimizer put it; a
  difference that comes from the two modules being inlined differently is
  read in the probe's context lines and reported as such. Otherwise the copy
  needs a placement change of its own, which `tag-only` stands in for.
- **The block writer's transfers matter** if `in-place-tag-only`'s wall
  interval at eight workers lies wholly below `tag-only`'s.
- **The proposal is rejected** if neither `tag-only` nor `in-place-tag-only`
  resolves an improvement at eight workers. The remaining cost is then
  recorded here, which is the item's second outcome.
- **The selected lowering** ([below](#the-lowering-the-owner-selected)) builds
  this program's empty slots as `no-clear` does, so `no-clear` decides it
  before any compiler change: the change is made if `no-clear`'s wall
  interval at eight workers lies wholly below 1, its wall and CPU intervals at
  one worker do not lie wholly above 1, and it is enough for the append by
  the criterion above. If the interval at eight workers contains 1, no
  compiler change is made and the cost is recorded. If the time improves and
  the append keeps a whole-block clear or copy, the lowering does not give
  the item's first outcome alone, and that is reported before any compiler
  change.

Limits known in advance: the i9-14900K runner is a hybrid processor and the
probe pins nothing, so eight workers may land on either kind of core; other
runners of the same machine may be busy, which the twin measures and does not
remove; process CPU time includes the runtime's idle lanes; the native chain
is serial and allocates its chunks with `calloc`, so it is context for one
worker only.

## What the source and the emitter show, read on 2026-11-25

These are read from the code at `ca8fe55bc`, not measured.

**Where the clear comes from.** `initialize_construction` in
`compiler/src/backend/emitter/places.rs` stores a zero of the whole value
unless a field of the *active* variant holds constant-capacity window storage;
only such active fields are left out, because the field copy that follows
writes them. `None<Chunk>()` has no active field, so its construction stores
4,120 zero bytes and then the tag. `Some<Chunk>(value: ...)` zeroes only its
tag and copies the 4,112-byte chunk in, and `Chunk(low: ..., high: ...)`
zeroes nothing. The test
`backend::tests::windows::aggregate_construction_omits_only_copied_window_fields`
pins this: its `empty_product` and `other` build a variant beside an inactive
window and expect one zero store of the whole value.

**Where the copies come from.** `copy_storage` in the same file moves a value
that has no bounded transfer (one of more than 128 bytes, or one that holds a
window) with one `memmove` of its whole size, the unoccupied slots of a
window included. `place_back` is a function instance of its own
(`row_place` in `compiler/src/lowering/builder/prelude.rs`) that receives the
value by address and copies it into the slot, and the block writer's
`set output^[0] = Some<Chunk>(...)` is an `IrInstruction::Store` of a value
built in the frame. A construction is redirected into the place that consumes
it only for a fresh binding whose address is taken
(`select_destinations` in `compiler/src/backend/storage.rs`), not for a store
or a placement.

**The lowering `no-clear` stands for.** Leave every field that holds
constant-capacity window storage out of the zero store, the fields of an
inactive variant included, and keep the zero store for every other byte. It
extends the header-only construction of 2026-10-09 from the active variant to
the whole value, and it differs from the lowering declined on 2026-09-23 in
what it leaves alone: a type with no inline window is built exactly as today.
Of the libraries under `lib/std/collections`, only the ordered map names a
constant-capacity window (its one-slot `Slots<Box<OrderedNode<K, V>>, 1>`
links); the hash map and the slab name none, so their emitted modules are the
same text under it unless a caller gives them a window payload. That is its
falsifier for those two consumers: if a module of the 2026-09-23 comparison
changes under the lowering, its two regressions have to be measured again
before the lowering is proposed.

**Which values a lowering keyed on window storage alone would change.** A
search of the maintained sources for an enum with an inline constant-capacity
window in a variant's payload finds, besides the scatter's `Option<Chunk>`:

| Where | Enum | Inline window |
| --- | --- | --- |
| `tests/programs/block_pool.wf` | `Option<Lease>` | `Slots<u8, 256>` |
| `tests/programs/raw_deflate_dynamic.wf` | `Result<HuffmanTable, InflateError>` | `Slots<u64, 16>`, `Slots<u64, 286>` |
| `tests/programs/short_lived_windows.wf` | `Held` | `Slots<Box<u64>, 64>`, `Ring<Box<u64>, 4>` |
| `apps/firn/bytes/module.wfm` | `Bytes`, and through it `Value`, `Option<Entry>`, `Option<Key>`, `Option<Rank>` | `Slots<u8, 24>` |
| `lib/std/collections/ordered_map/ordered-map.wf` | `Option<OrderedEntry<K, V, OrderedNode<K, V>>>` | `Slots<Box<OrderedNode<K, V>>, 1>` |

and a number of conformance cases with windows of 1 to 8 elements. Firn's
byte strings and the ordered map's links are small values, and both
regressions of 2026-09-23 were of small or medium values whose optimized code
changed shape. No measurement of firn or of the ordered map under such a
lowering exists. A value that holds an inline window has no bounded transfer
at any size (`transfer_leaves` in `compiler/src/target.rs` gives a window no
granule), so every row above already moves by one `memmove` of its whole
size, and no existing class separates the small rows from the large ones: a
lowering that left the small ones as today would need a size of its own.
Which maintained modules a candidate changes is to be confirmed from their
emitted text.

**What the par-suite ratio is.** The 4.86 of the
[par-suite baseline](../par-suite/DESIGN.md#baseline-2026-10-09) divides the
process wall time of the `--par` image by that of the sequential image of the
same program, on its own 8,209 keys, at 32 workers. Both images build the same
33 chunks, so chunk building is in the numerator and the denominator alike:
removing it cannot lower that ratio and, by shortening both, raises it a
little. The ratio also predates the demand-driven hand-out of work. It is
therefore not the measure of this investigation; the whole call on 1,048,593
keys is.

## The lowering the owner selected

On 2026-11-25 the owner chose, among leaving the inactive window fields
unwritten in large values only, leaving them unwritten at every size, leaving
every inactive payload unwritten, and changing nothing: **in a value of more
than 128 bytes, a field of an inactive variant that holds constant-capacity
window storage is not zero-filled; a value of at most 128 bytes is built as
today.** The choice holds if the probes resolve a cost of the empty slots at
eight workers; 128 is the bounded transfer's limit. The other two lowerings
lost on what they would change without a measurement: the second firn's byte
strings and the ordered map's links, the third also the two consumers of the
2026-09-23 comparison.

Sizes of the values above on x86-64, worked out from their declarations and
not yet from the compiler:

| Value | Bytes | Changes |
| --- | ---: | --- |
| scatter `Option<Chunk>` | 4,120 | yes |
| `Result<HuffmanTable, InflateError>` | 2,456 | yes |
| `Held` of `short_lived_windows.wf` | 608 | yes, where its window-less variant is built, which the program does not do today |
| `Option<Lease>` | 272 | yes |
| firn `Option<Entry>` | 80 | no |
| firn `Value`, `Option<Rank>` | 64 | no |
| firn `Option<Key>` | 56 | no |
| firn `Bytes` | 40 | no |
| ordered map `Option<OrderedEntry<..>>`, firn's instance | 80 | no |
| ordered map `Option<OrderedEntry<..>>`, the test program's instances | 40 | no |

An instance of the ordered map whose key and value pass 104 bytes together
would put its `Option<OrderedEntry<..>>` over the limit; no maintained source
has one. The falsifier for the "no" rows and for the libraries without a
window: a probe emits the modules of firn, of the hash map, slab and ordered
map programs and of the three programs that build a large value's window-less
variant (the scatter, the dynamic inflate and the block pool) at the commit
before the compiler change and at the commit after it, and reports each
module's length and a hash of its text. The first group must be the same text
at both commits, the second must differ.

## Acceptance measurement of the selected lowering

Fixed before the first result. One build of the changed compiler makes three
images of the unchanged scatter with `--par`: `before`, built under the
present construction rule through a switch that exists only in the compiler's
own tests and leaves the branch with the probes; an identical twin of
`before`; and `after`. The protocol is the timing probe's: 1,048,593 mixed
keys, one unreported and 20 timed calls a process, nine passes rotating the
image order at 1, 8 and 32 workers, each image checked against the
independent oracle first, the per-pass ratio of `after` to `before`, and the
distribution-free 95% interval of the median of the nine ratios.

- The item's first outcome holds if the wall interval at eight workers has
  its upper end below 1.00, and the wall and the CPU interval at one worker
  do not lie wholly above 1.00.
- A width counts only if the twin's wall interval there contains 1. A width
  that fails this is run once more; if it fails again the result is reported
  as unresolved, not as passed.
- 32 workers is reported and decides nothing.

## Results

None yet. No probe has run: GitHub refused each request to start them for an
exhausted API budget, on 2026-11-25 at 18:35 UTC and again at 19:16 UTC, less
than ten minutes after the budget's hourly reset at 19:06:39 UTC.
