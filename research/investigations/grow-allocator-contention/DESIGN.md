# Window growth under parallel workers

## Question

Lowering `grow` of a `Box<Slots<T>>` cell to `realloc`
([storage-representation](../../../design/compiler/storage-representation.md))
made Halo's single-threaded integer-table kernel 14.0% faster on the
i9-14900K. With four workers, Snowghost's text preparation lost most of its
parallel speedup at the same change. Can growth keep the single-threaded gain
without the parallel loss?

## Observation that opened it

Snowghost-wf measured text preparation of the ecma262 page on a GitHub-hosted
4-core AMD EPYC runner (three rounds, two twins; runs 38010107365 and
38013642734):

- the 4-thread to 1-thread time ratio rose from about 0.48 (v0.94) to about
  0.79 (843c85091e58); html5 from about 0.50 to about 0.645;
- `futex` calls in three text runs at `WF_WORKERS=4` rose from about 1,076 at
  631d3ff to about 160,000 at b2209fd, whose only runtime change is the
  `realloc` growth (#280); fixing glibc's mmap threshold left both unchanged;
- at b2209fd, 64% of `futex` calls are wakes in `__lll_lock_wake_private` on
  one lock address, 54.5 points of them under `realloc` from Snowghost's
  capacity-doubling buffers (`push_planned`, `push_item`, `push_segment`),
  and the 36% waits are under the same callers.

glibc serves small `malloc` and `free` from a per-thread cache without a lock;
`realloc` always takes the lock of the arena that owns the chunk.

## Proposal under test

Growth of a block smaller than a constant `C` copies into a fresh block
(`wf__heap_take`, `memcpy`, `wf__heap_give`); larger blocks keep `realloc`.
Two experiment builds, identical except for `C`:

- `C` = 1 KiB, about glibc's per-thread cache limit for one request;
- `C` = 128 KiB, glibc's initial mmap threshold.

## Comparison and rejection criteria

Each build is compared with its own base (main 87fa2524f) on the same host in
interleaved runs with a twin:

- Snowghost text preparation, ecma262 and html5, 4-thread to 1-thread ratio
  and `futex` count at `WF_WORKERS=4` (Snowghost-wf's measurement). The
  proposal is rejected for a `C` whose ratio stays above 0.55 on ecma262 or
  whose `futex` count stays above 10,000.
- Halo's integer-table and sort kernels on the i9-14900K (Halo-wf's
  measurement). The proposal is rejected for a `C` that makes either kernel
  slower than the base beyond the twin's spread.

If both values of `C` pass, the smaller one is preferred, because it keeps
`realloc` for more of the blocks that can grow in place.

## Results

Pending.
