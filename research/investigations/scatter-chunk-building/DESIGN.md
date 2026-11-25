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
  `no-clear` module's optimized `radix_scatter` holds no constant clear or copy
  of at least 2,048 bytes. Otherwise the copy needs a placement change of its
  own, which `tag-only` stands in for.
- **The block writer's transfers matter** if `in-place-tag-only`'s wall
  interval at eight workers lies wholly below `tag-only`'s.
- **The proposal is rejected** if neither `tag-only` nor `in-place-tag-only`
  resolves an improvement at eight workers. The remaining cost is then
  recorded here, which is the item's second outcome.

Limits known in advance: the i9-14900K runner is a hybrid processor and the
probe pins nothing, so eight workers may land on either kind of core; other
runners of the same machine may be busy, which the twin measures and does not
remove; process CPU time includes the runtime's idle lanes; the native chain
is serial and allocates its chunks with `calloc`, so it is context for one
worker only.

## Results

None yet.
