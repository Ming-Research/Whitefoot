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
`make -C compiler concurrent-map-test`, fails on each of these copies of
the cell design: an update that does not run its edit, a remove that
leaves the key, a writer that skips the cell's lock, a writer that keeps a
cell it locked after a move began, a claim given back as empty rather than
removed, a read that does not wait for a locked cell, and a reclamation
that frees tables users are still in. The test includes the map's source,
so it drives the interleaving a claim given back needs step by step; it
also passes under AddressSanitizer with UndefinedBehaviorSanitizer and
under ThreadSanitizer.

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
(`python3 summarize.py --verdict wf-index`). The first duel, of the bucket
design before its lookup was made branch-free, gave 1 lead, 8 ties and 9
losses; the second, of the cell design, 2 leads, 12 ties and 4 losses,
with `churn` at one thread completing no operation; the third, after moved
tables were freed and reused, one writer made each table and waiting
writers backed off longer, 1 lead, 17 ties and no loss. The fourth, after a
read stopped loading the key word twice and a move stopped marking each
cell, on 2026-10-01, medians of three interleaved repetitions, millions of
operations a second:

| cell | threads | wf-index | fastest comparator | ratio | spread | verdict |
|---|---|---|---|---|---|---|
| one update | 1 | 84.19 | 88.24 growt | 0.95 | 0.13 | tie |
| one update | 4 | 18.85 | 14.53 dashmap | 1.30 | 0.57 | tie |
| uniform balanced | 1 | 20.35 | 16.36 growt | 1.24 | 0.25 | tie |
| uniform balanced | 4 | 77.56 | 61.93 growt | 1.25 | 0.18 | lead |
| uniform churn | 1 | 5.56 | 11.26 dashmap | 0.49 | 1.40 | tie |
| uniform churn | 4 | 34.01 | 34.02 scc | 1.00 | 0.19 | tie |
| uniform grow | 1 | 11.67 | 10.32 dashmap | 1.13 | 0.25 | tie |
| uniform grow | 4 | 18.74 | 19.00 growt | 0.99 | 0.69 | tie |
| uniform mostly-read | 1 | 25.29 | 22.80 growt | 1.11 | 0.15 | tie |
| uniform mostly-read | 4 | 105.14 | 92.99 growt | 1.13 | 0.13 | lead |
| uniform read | 1 | 25.13 | 24.94 growt | 1.01 | 0.33 | tie |
| uniform read | 4 | 119.15 | 112.84 growt | 1.06 | 0.26 | tie |
| uniform update | 1 | 18.68 | 16.79 growt | 1.11 | 0.18 | tie |
| uniform update | 4 | 83.58 | 76.06 growt | 1.10 | 0.16 | tie |
| Zipf balanced | 1 | 29.41 | 25.24 growt | 1.17 | 0.15 | lead |
| Zipf balanced | 4 | 67.53 | 72.69 growt | 0.93 | 0.10 | tie |
| Zipf mostly-read | 1 | 39.19 | 32.62 growt | 1.20 | 0.44 | tie |
| Zipf mostly-read | 4 | 129.28 | 135.74 growt | 0.95 | 0.06 | tie |

3 leads, 15 ties, no loss, at N = 2^20. Every cell's spread is wide on this
host, from 0.06 to 1.40, so most cells are ties whichever way their medians
lean. At one thread the index reaches `mutex-flat` in every mix but
`churn`, 5.56 against 11.41: that cell's first moves go to fresh cells,
which this host serves cold
([the investigation](../../investigations/concurrent-map/DESIGN.md#this-hosts-fresh-memory)),
and its three repetitions ran from 0.16 to 7.94.
