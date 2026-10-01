# concurrent-map-bench results

The measurement of [the concurrent hash index investigation](../../investigations/concurrent-map/DESIGN.md#the-measurement),
whose rules and criteria are stated there. Host: a 4-CPU cloud virtual
machine, "Intel(R) Xeon(R) Processor @ 2.10GHz", 15 GB, Linux 6.18; native
code built by Clang 18 at `-O3 -march=x86-64-v3`; Java 21, Go 1.24.7,
.NET 8.0.131, Rust 1.94.1; comparators at the pins in `deps.sh`.

## Checks

`make verify` passes for every implementation. Each check was made to fail
once by a broken map: in C a racing table, an update that adds nothing, a
remove that removes nothing, dropped inserts, a wrong get and an unlocked
update; in Java, Go and .NET an update that adds nothing and a remove that
removes nothing. The runtime's own test of the index,
`make -C compiler concurrent-map-test`, fails on a copy whose update does
not run its edit and on a copy whose reads skip the version check.

## The comparators' baseline

The full profile of 2026-10-01, stopped after the 2^10 and 2^20 sizes at the
owner's ruling that the whole matrix runs only when unavoidable; the fastest
comparator per cell, median of three interleaved repetitions, millions of
operations per second:

| size | key choice | mix | 1 thr fastest | 2 thr fastest | 4 thr fastest |
|---|---|---|---|---|---|
| 1024 | uniform | read | 170.80 growt | 352.34 growt | 579.18 growt |
| 1024 | uniform | mostly-read | 147.27 growt | 112.80 growt | 182.06 growt |
| 1024 | uniform | balanced | 68.30 growt | 48.17 growt | 74.02 growt |
| 1024 | uniform | update | 80.64 growt | 51.51 growt | 83.16 growt |
| 1024 | uniform | churn | 31.86 dashmap | 22.03 scc | 32.17 growt |
| 1024 | uniform | grow | 22.93 growt | 39.48 growt | 47.83 growt |
| 1024 | zipf | read | 183.70 growt | 365.12 growt | 730.27 growt |
| 1024 | zipf | mostly-read | 144.39 growt | 113.19 growt | 170.70 growt |
| 1024 | zipf | balanced | 66.88 growt | 44.67 growt | 58.58 growt |
| 1024 | zipf | update | 82.09 growt | 48.22 growt | 53.62 growt |
| 1024 | one | update | 86.16 growt | 16.64 growt | 13.04 dashmap |
| 1048576 | uniform | read | 22.06 growt | 51.28 growt | 108.02 growt |
| 1048576 | uniform | mostly-read | 22.21 growt | 44.33 growt | 94.12 growt |
| 1048576 | uniform | balanced | 16.42 growt | 36.83 growt | 68.80 growt |
| 1048576 | uniform | update | 16.07 growt | 36.03 growt | 65.93 growt |
| 1048576 | uniform | churn | 10.98 dashmap | 18.01 scc | 36.05 scc |
| 1048576 | uniform | grow | 9.53 dashmap | 14.64 growt | 24.80 growt |
| 1048576 | zipf | read | 34.67 growt | 87.63 growt | 177.77 growt |
| 1048576 | zipf | mostly-read | 36.18 growt | 65.78 growt | 119.55 growt |
| 1048576 | zipf | balanced | 23.22 growt | 40.51 growt | 65.76 growt |
| 1048576 | zipf | update | 24.41 growt | 38.78 growt | 66.03 growt |
| 1048576 | one | update | 86.90 growt | 18.07 growt | 14.26 dashmap |

growt leads nearly every cell. Its update is a compare-and-swap loop and
its removed keys stay as tombstones until its next migration.

## The index

Judged in the `duel` profile against growt, DashMap and scc
(`python3 summarize.py --verdict wf-index`). The first duel, before the
lookup was made branch-free, gave 1 lead, 8 ties and 9 losses; the work
continues in the investigation's record.
