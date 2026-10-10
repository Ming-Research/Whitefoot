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

### Halo, single thread (i9-14900K)

Halo-wf run 38018817431 (2026-10-10, branch `claude/grow-copy-bench` at
Halo-wf 5bb82f9 plus a temporary workflow): one Halo source built by main's
`wf-87fa2524f4da`, its twin, `wf-exp-fe2f48fbd8cc` (1 KiB) and
`wf-exp-4d073028396d` (128 KiB), full LTO, compared with Halo's
`research/experiments/halo-bench/run.py` at 1, 3 and 6 interleaved pairs.
Ratios are experiment time over control time at 6 pairs:

| kernel | twin | 1 KiB | 128 KiB |
|---|---:|---:|---:|
| integer-table | 0.997 | 1.004 | 1.006 |
| sort | 0.992 | 1.001 | 0.995 |
| fib | 1.002 | 0.993 | 0.983 |
| loop | 1.002 | 1.001 | 1.002 |
| string-key | 1.000 | 1.002 | 0.990 |
| concat | 1.002 | 1.003 | 1.001 |
| binary-trees | 0.996 | 0.993 | 0.998 |

The twin ranged 0.997 to 1.013 on integer-table and 0.992 to 0.998 on sort
across the three pair counts. Neither value of `C` makes either kernel slower
beyond that spread, so the Halo criterion passes for both.

### Snowghost, four workers

Snowghost-wf run 38018868437 (2026-10-10, branch `research/grow-threshold`) on
a GitHub-hosted AMD EPYC 7763 with 4 vCPUs: each release built twice from
Snowghost-wf bb98d43 (v0.94 from 9d720c4 as the target reference), five
alternating rounds with reversed order, because the twins differed by more
than 0.15 in the trial. Text preparation, median 4-thread to 1-thread ratio,
twin a / b, and `futex` calls in three ecma262 text runs at `WF_WORKERS=4`:

| build | ecma262 | html5 | ecma262 1 thread (s) | futex |
|---|---|---|---|---:|
| v0.94 (0b7f5c5) | 0.495 / 0.508 | 0.507 / 0.518 | 0.637 / 0.617 | 205 / 272 |
| main, always `realloc` | 0.781 / 0.790 | 0.653 / 0.642 | 0.597 / 0.587 | 158,627 / 159,641 |
| copy below 1 KiB | 0.518 / 0.519 | 0.524 / 0.534 | 0.543 / 0.550 | 904 / 1,028 |
| copy below 128 KiB | 0.530 / 0.512 | 0.540 / 0.534 | 0.543 / 0.540 | 1,157 / 1,105 |

The later layout passes recovered too (ecma262 ratio 0.83 on main, 0.55 / 0.62
at 1 KiB, 0.58 / 0.59 at 128 KiB, 0.62 / 0.60 at v0.94). `mremap` stayed at
29 to 30 calls in every build. Neither limit reaches the rejection line, and
the single-thread time also fell by about 8% against main.

## Conclusion

Both limits pass both criteria: neither reaches the Snowghost rejection line,
and neither makes Halo's integer-table or sort slower beyond the twin's
spread. Their Snowghost twins overlap (0.518 / 0.519 against 0.530 / 0.512),
so the rule fixed before measuring selects 1 KiB. Contention on blocks of
1 KiB or more, which still take an arena lock through `realloc`, was not
measured: no allocation-size distribution was recorded for either workload.
