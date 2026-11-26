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
`stable_scatter_probe_times_whole_calls`, first committed in `94696cfad`).
They fail on purpose so that their report reaches the test result, and they
leave the branch before it merges. The source is the maintained program,
unchanged, built with `--par`.

They run on two commits. On the commit before the emitter change, the
structure probe reports what main's ordinary emission holds. On the commit
with [the selected lowering](#the-lowering-the-owner-selected), the structure
probe reports what the changed compiler's ordinary emission holds, and the
timing probe measures that emission, `after`, against `before`: the same
compiler emitting under the present construction rule, through a switch that
exists only in the compiler's own tests and leaves the branch with the probes.
`before` counts as main's emission if its module has the length and the hash
that the structure probe reports on the earlier commit for the `--par` module
it emits there through the same entry. The timing probe is asked for alone:
the tests named in one request run as threads of one process.

Besides those emissions, each probe builds variants by editing an emitted
module's text before the host optimizer runs. A variant shows what the
optimized code and the time of this program would be under a lowering; it
does not show that the lowering is correct in general. A variant whose module
the host refuses, or whose result the independent oracle refuses, is named in
the report and left out.

| Image | How it is made |
| --- | --- |
| `before` | the present construction rule |
| `twin` | `before` again, for the run's spread |
| `after` | the selected lowering, as the changed compiler emits it |
| `no-clear` | `before` with the whole-value clear before each empty `Option<Chunk>` removed; the tag store stays |
| `headers-only` | `before` with that clear cut down to the two run lengths |
| `tag-only` | `after`, and each whole `Option<Chunk>` transfer in `radix_scatter` moves only the tag byte |
| `in-place` | another source: the block writer stores an empty chunk and partitions into it through a reference |
| `in-place-tag-only` | `in-place` with the `tag-only` edit |
| `sequential` | `before`'s source built without `--par`, timed only |

`no-clear` stores the same value as `after` and is not the same text: on the
product path the changed compiler still zeroes the tag as a field before it
stores the tag, through an address of its own. It stays in the campaign to
show whether the two time alike. `in-place` is a control for what the block
writer's value transfers cost. It is not a proposal to change the program: a
redundant transfer of a by-value form is a compiler gap, and a changed
spelling would hide it.

Structure: every constant clear or copy of at least 2,048 bytes (one run's
payload) in the host-optimized module, by function, for the ordinary emission
of the commit the probe runs on and for the variants made from it.

Time: one call scatters 1,048,593 mixed keys. A process makes one unreported
call and 20 timed ones, checked against the independent oracle outside the
timed interval, and contributes the median wall and process CPU time. Nine
passes run every image once at 1, 8 and 32 workers, rotating the image order.
Each pass also times `before` on the 8,209 keys of the program's own entry,
200 calls a process, to tell a slow call apart from the process startup that
the [par-suite baseline](../par-suite/DESIGN.md#baseline-2026-10-09) includes
in its 4.86 at 32 workers.
An image's per-pass ratio is its median over `before`'s in the same pass and
width. The interval is the distribution-free 95% interval of the median of
those ratios, as `median_interval` of
`research/experiments/hash-map-compare/compare.py` defines it: with nine
ratios, the second smallest and the second largest. A width counts only if
the twin's wall interval there contains 1. A width that fails this is run once
more; if it fails again the result is reported as unresolved, not as passed.
32 workers is reported and decides nothing.

Fixed before measuring:

- **The empty slots matter** if `tag-only`'s wall interval at eight workers
  lies wholly below 1 and its wall and CPU intervals at one worker do not lie
  wholly above 1. If its interval at eight workers contains 1, removing the
  append's whole-block work has no effect this instrument resolves, and the
  item's first outcome cannot rest on it.
- **Leaving the inactive payload unwritten is enough for the append** if the
  optimized `after` module holds no constant clear or copy of at least 2,048
  bytes that the optimized `tag-only` module lacks, and `tag-only`'s note
  counts at least one shortened transfer. `tag-only` has no whole transfer of
  an appended slot by construction, so what it keeps is the block writer's or
  the entry's, in whichever function the optimizer put it; a difference that
  comes from the two modules being inlined differently is read in the probe's
  context lines and reported as such. Otherwise the copy needs a placement
  change of its own, which `tag-only` stands in for.
- **The block writer's transfers matter** if `in-place-tag-only`'s wall
  interval at eight workers lies wholly below `tag-only`'s.
- **The selected lowering is kept**, and the item's first outcome holds, if
  `after`'s wall interval at eight workers lies wholly below 1, its wall and
  CPU intervals at one worker do not lie wholly above 1, and it is enough for
  the append by the criterion above. This one campaign is the acceptance
  measurement. If the three intervals hold and the append keeps a whole-block
  clear or copy, the lowering does not give the first outcome alone, and that
  is reported before anything else is done. If a width stays unresolved, that
  is reported and nothing is decided. In every other case (the interval at
  eight workers contains 1 or lies above it, or a wall or CPU interval at one
  worker lies wholly above 1) the emitter change is taken back and the
  remaining cost is recorded here, which is the item's second outcome.

Limits known in advance: the i9-14900K runner is a hybrid processor and the
probe pins nothing, so eight workers may land on either kind of core; other
runners of the same machine may be busy, which the twin measures and does not
remove; process CPU time includes the runtime's idle lanes; the native chain
is serial and allocates its chunks with `calloc`, so it is context for one
worker only.

What the lowering cannot remove, stated before the first result: the append
still stores a tag byte into every 4,120-byte slot. Where the chunk
allocation's pages are fresh at a call, which the allocator decides and no
probe here observes, the serial append loop is still the first to touch
nearly every one of them; only the stores of the clear and of the copy go.
The native control of 2026-09-21 took its chunks from `calloc`
(`research/experiments/compute-bench/radix_scatter_bench.c` includes the
oracle's `allocate`), which may hand out pages it never touched, so its
0.186 ms against 0.596 ms is not a bound on what this lowering recovers.
Every image keeps the tag store, so the campaign measures what the lowering
gives and does not separate the first touch from the rest.

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
Of the libraries under `lib/std/collections`, the ordered map and the slab
name constant-capacity windows in their structs (the ordered map's one-slot
`Slots<Box<OrderedNode<K, V>>, 1>` links and its node's
`Slots<OrderedEntry<..>, 15>`; `SlabCell<T>`'s `Slots<T, 1>`), and the hash
map names none. The lowering changes a module only where such a struct, or a
caller's payload that holds a window, sits inline in a variant of an enum:
the ordered map's `Option<OrderedEntry<..>>` does, the node and the slab's
cell do not, and the library enums over a caller's key and value
(`HashMapSlot<K, V>`, `HashMapPut<K, V>`, `OrderedPut<K, V>` and the `Option`
of their pairs) do when the key or the value holds a window. That is its
falsifier for the two consumers of the 2026-09-23 comparison: if one of their
modules changes under the lowering, its regression has to be measured again
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
| `lib/std/collections`, as firn instantiates them | `HashMapSlot`, `HashMapPut` and `Option<HashMapPair>` over `Key` with `Bytes`, `unit` or `f64`; `OrderedPut` and `Option<OrderedPair>` over `Rank` | `Slots<u8, 24>`, through `Key` and `Bytes` |

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
| firn `HashMapSlot<Key, Bytes>`, `HashMapPut<Key, Bytes>`, `Option<HashMapPair<Key, Bytes>>` | 96 | no |
| firn `Option<Entry>` | 80 | no |
| firn `Value`, `Option<Rank>` | 64 | no |
| firn `Option<Key>` | 56 | no |
| firn's hash map enums over `Key` with `unit` or `f64`, its `OrderedPut<Rank, unit>` and `Option<OrderedPair<Rank, unit>>` | 56 to 64 | no |
| firn `Bytes` | 40 | no |
| ordered map `Option<OrderedEntry<..>>`, firn's instance | 80 | no |
| ordered map `Option<OrderedEntry<..>>`, the test program's instances | 40 | no |

An instance of the ordered map whose key and value pass 104 bytes together
would put its `Option<OrderedEntry<..>>` over the limit; no maintained source
has one. The falsifier for the "no" rows and for the libraries without a
window is a census: a probe emits the modules of firn, of the hash map, slab
and ordered map programs and of the three programs that build a large value's
window-less variant (the scatter with and without `--par`, the dynamic inflate
and the block pool) at the commit before the compiler change and at the commit
after it, and reports each module's length and a hash of its text. The first
group must be the same text at both commits, the second must differ.

## Results

### Structure and census, 2026-11-25

Two runs of the fast workflow on the 14900K runner, read from the probes'
reports and the named tests' results:

- main's rule: commit `5d836f636`, whose emitter is main's at `ca8fe55bc`,
  [run 45694027022](https://github.com/Ming-Research/Whitefoot/actions/runs/45694027022);
- the selected rule: commit `ea1bf64df`,
  [run 45694027617](https://github.com/Ming-Research/Whitefoot/actions/runs/45694027617).

The structure probe of these runs ended its report with a cut tail, which
lost the ordinary module's list of whole-block transfers; what follows is
read from its context lines and from the lists of the other images and of
the build without `--par`. Commit `9282c7d81` ends the report with every
list and each function's call sites.

1. The `--par` module the probe emits is 145,203 bytes with FNV-1a
   `7a67fe1ca703ed4d` under main's rule, and 145,325 bytes with
   `b707dcd037d72654` under the selected rule.
2. Under main's rule the optimizer already builds each appended empty slot in
   place: the append loop of `wf_radix_scatter`, unrolled by four, stores the
   tag byte into the slot and then clears its 4,112 bytes from offset 8 with
   one `memset`; no copy of a temporary into the slot is left on that path.
   Without `--par`, `wf_radix_scatter` holds five such `memset`s. Of the clear
   and the copy seen on 2026-09-21, the copy is gone on main and the clear
   remains.
3. Under the selected rule, `wf_radix_scatter` holds no constant clear or copy
   of 2,048 bytes or more in either build besides the block writer's copies
   it inlines. The emitted module holds no zero store of the whole
   `Option<Chunk>` in either world: the assertion added to
   `stable_scatter_matches_an_independent_oracle_and_hands_out_output_work`
   fails under main's rule (in the world without `--par`, the first it
   checks) and passes under the selected rule, whose run also passed the
   oracle in both worlds.
4. In both modules the out-of-line definition
   `place_back$instance$bf6eebe3abbd4940` keeps a 4,120-byte `memcpy`, since
   it copies whatever value it is given. These runs do not say whether
   anything calls it.
5. The block writer is the same under both rules: `partition_block` returns
   its two runs through two 2,056-byte copies, and `write_chunk` copies them
   into a `Chunk` (two of 2,056 bytes), the `Chunk` into the `Some` (4,112)
   and the `Some` into the slot (4,119 after the tag byte). That is about
   16 KB copied for each block of 2 KB of keys.
6. The in-place control was rejected by the checker in both runs (OWN-1: a
   by-value match of a window element whose payload is not a copy type);
   `9282c7d81` matches it through a reference.

The census probe (`programs::memory::stable_scatter_probe_reports_module_census`)
reported each module's byte length and FNV-1a:

| Module | Main's rule | Selected rule |
| --- | --- | --- |
| firn | 7,035,808 `7b2f92a657c8456c` | 7,035,808 `7b2f92a657c8456c` |
| hash map, without `--par` | 939,997 `82e1e9587b226859` | 939,997 `82e1e9587b226859` |
| hash map, `--par` defaults | 996,377 `e709c9bedfff4eb2` | 996,377 `e709c9bedfff4eb2` |
| slab, without `--par` | 497,926 `440d5676b086b484` | 497,926 `440d5676b086b484` |
| slab, `--par` defaults | 497,926 `440d5676b086b484` | 497,926 `440d5676b086b484` |
| ordered map, without `--par` | 556,766 `f6a8427ede4ba216` | 556,766 `f6a8427ede4ba216` |
| ordered map, `--par` defaults | 627,011 `f7024073ceb4cbe1` | 627,011 `f7024073ceb4cbe1` |
| scatter, without `--par` | 73,824 `6ff694a8b766aae6` | 73,887 `ebd1454e91829d25` |
| scatter, `--par` | 145,203 `7a67fe1ca703ed4d` | 145,325 `b707dcd037d72654` |
| dynamic inflate | 341,013 `a6b306de6e5cbc77` | 341,295 `a628c0069f63afe3` |
| block pool | 60,751 `58e0dffed0280889` | 60,810 `fc0f204434889055` |

The first group is the same text under both rules and the second differs, as
[the selected lowering](#the-lowering-the-owner-selected) predicted. Under the
selected rule the four raw deflate tests, the block pool's test and the hash
map, slab and ordered map tests in both lowering modes passed.

### Structure with every list and call site, 2026-11-25

Two more runs of the structure probe as `9282c7d81` reports it, on the
14900K runner, after main had been merged into the branch with no change to
the emitter, the target layout or storage selection:

- main's rule: commit `74de62e27`, the branch with the emitter change taken
  back,
  [run 45757882131](https://github.com/Ming-Research/Whitefoot/actions/runs/45757882131);
- the selected rule: commit `13a9f4847`,
  [run 45757884913](https://github.com/Ming-Research/Whitefoot/actions/runs/45757884913).

Their `--par` modules have the lengths and hashes of the first two runs
(145,203 bytes, `7a67fe1ca703ed4d`; 145,325 bytes, `b707dcd037d72654`), so
those merges left the scatter's emission unchanged. A call-site count of 0
below means that no function of the optimized module calls the function that
holds the transfer.

1. Under main's rule, what the optimized `--par` module holds and its
   `tag-only` edit lacks is ten 4,112-byte `memset`s, five in
   `wf_radix_scatter` and five in `wf__par_seq_radix_scatter` (each called
   once), and the 4,120-byte `memcpy` of
   `place_back$instance$bf6eebe3abbd4940` (call sites: 0). The `memset`s are
   the append's clear of item 2 above; `no-clear` removes two emitted clears,
   one in each of those two functions.
2. Under the selected rule, the only such difference is the 4,120-byte
   `memcpy` of that `place_back` instance, whose transfer is the one the
   `tag-only` edit shortens (`tag-only: 1 transfers shortened`). Nothing in
   the optimized module calls it: the append inlined into the scatter's root
   keeps no clear and no copy of 2,048 bytes or more. By the criterion
   above, leaving the inactive payload unwritten is enough for the append;
   the difference is an out-of-line definition, not code that runs.
3. Under both rules, every other constant clear or copy of 2,048 bytes or
   more belongs to the block writer or to the entry: the two 2,056-byte
   copies of its result in `wf_partition_block` (3 call sites), and
   `write_chunk`'s copies of
   2,056, 2,056, 4,112 and 4,119 bytes inlined twice into
   `wf__par_chunk_radix_scatter.1` and `wf__par_seq__par_chunk_radix_scatter.1`
   (each called once), and the entry's 65,672-byte clear of the input in
   `wf_main` and `wf__par_seq_main`. The in-place control removes the block
   writer's copies and keeps the two 2,056-byte copies of the out-of-line
   `partition_block` definitions (call sites: 0).

### Timing

Not run yet. GitHub refused the requests for an exhausted API budget at
18:35, 19:16, 20:35 and 20:36 UTC on 2026-11-25, and once refused a push for
an error of its own storage.
