# C references for the one-object redesign

These programs give the "C program with the same guarantee by the best known
method" that `research/investigations/shared-object-model/ONE-OBJECT.md`
(sections 3, 4 and 12) judges Whitefoot against: throughput at least 0.9 of it,
longest latency at most twice its longest.

Everything runs on the self-hosted runner `14900k` through the temporary
workflow `.github/workflows/tmp-shared-object-c-ref.yml`; nothing is built or
measured elsewhere.

## Method

Matches the earlier shared-counter runs: CPU lists `0,2`, `0,2,4,6`,
`0,2,4,6,8,10,12,14` (2, 4, 8 P-cores) and `0-15` (8 P-cores, both
hyperthreads); one thread per listed CPU, pinned; a start gate; 5 s per cell;
batches of 1024 operations with one clock read per batch; total ops/s; 3 blocks
with the arm order rotated per block; medians. `/usr/bin/clang -O2
-march=native -pthread`. A3 (`w7.c`, copied unchanged from the
`claude/shared-counter-study` branch) runs under `taskset` with one thread per
CPU as in the earlier study.

## Arms

- A1 perthread-faa (run twice: A1 and twin A1t), A2 shared-faa.
- A3 shard-ticket (`w7.c`), without a reader and with the reader on CPU 16 (A3r).
- A4 shard-asym: writer with no read-modify-write (`in_cs` flag plus plain
  counter), reader on CPU 16 using `membarrier(PRIVATE_EXPEDITED)` (A4r).
- A5 layout: per-thread test-and-set part, variants a (16-byte mallocs),
  b (aligned 64), c/d/e (one allocation, parts 16/64/128 bytes apart).
- A6 atomic ops: load, fetch_add, exchange, CAS loop; on a shared word (s) and
  a per-thread line (p).
- A7 seq-structure: 100000 64-byte entries, per-thread 1 MiB ring, without
  (A7n) and with (A7s) a global `fetch_add` sequence.
- A8 timedwait: `FUTEX_WAIT_BITSET` with a 1 ms absolute deadline, lateness
  per wait; 1 thread on CPU 0 and 8 threads on the 8 P-cores.

`run.sh` writes `raw.tsv` (every cell); `summarize.py` produces `summary.txt`.

## Results

Run 38556548296 (workflow `tmp-shared-object-c-ref`, runner 14900k, i9-14900K,
governor `performance`, load 0.00 at start). 3 blocks, medians, M ops/s. Files
in `results/38556548296/cref-38556548296-1/`: `summary.txt`, `raw.tsv` (every
cell), `run.log`, `machine.txt`. Columns: 2, 4, 8 P-cores, 8 P-cores with both
hyperthreads.

Caution for all arms: the loop body carries a 64-bit multiply chain (the
same as w7.c), so cheap operations (loads, A4 writers) are bounded by that
chain at about 1 G ops/s per thread; read their numbers as "not the
bottleneck", not as the cost of the operation.

**A1 / A2 (fetch_add).** A1 per-thread: 553 / 1106 / 2213 / 1563. Twin A1t is
within 0.1% of A1 everywhere, so run-to-run noise is under 0.1%. At 8 P-cores
A1 is 2213 M/s against about 1994 M/s earlier: not reproduced exactly, about
11% higher (earlier runs used other code and a different day). A2 shared word:
114 / 136 / 121 / 92, so about 118 M/s at 8 P-cores is reproduced (121). One
shared word costs 18x a per-thread line at 8 P-cores.

**A3 shard-ticket (w7.c).** 390 / 781 / 1562 / 1927, which is 0.706 of A1 on
P-cores (a ticket lock does a fetch_add and a store, plus a load). With the
reader on CPU 16 throughput is unchanged (ratio 1.000). Reader (4999 reads per
cell): median whole read 0.2 / 0.4 / 0.8 / 1.4 us, p99 0.3 / 0.7 / 1.1 / 1.7 us;
longest read, median of blocks 89.4 / 0.9 / 1.5 / 2.1 us, worst of any block
101.0 / 65.5 / 1.5 / 39.0 us (the largest are rare outliers, one per cell at
most).

**A4 shard-asym.** Writers with no atomic read-modify-write: 1984 / 3968 /
7932 / 15494, 3.59x A1 on P-cores and 5x A3 (writer-bound by the multiply
chain, see above). With the membarrier reader: 1981 / 3961 / 7916 / 15388
(0.998 of no reader). Reads (4999 per cell): median 0.9 / 1.1 / 1.5 / 2.3 us,
p99 1.0 / 1.2 / 1.6 / 2.5 us; longest, median of blocks 2.9 / 2.0 / 2.5 / 5.5 us,
worst of any block 2.9 / 2.5 / 102.9 / 153.8 us. This is the ceiling for a
counter with exact reads: writers pay no atomic instruction and a read costs a
couple of microseconds, rarely over 150 us. A read is slower than the
ticket-lock read (A3r) by about 1 us, but writers are 4x to 8x faster.

**A5 layout.** Relative to (b) aligned 64 (376 / 753 / 1506 / 1943):
(d) 64 apart in one allocation 1.000 everywhere, (e) 128 apart 1.000 / 0.999 /
1.000 / 1.000. So one object with parts padded to a line matches separate
aligned objects exactly, and 128 padding buys nothing here. (c) packed 16
apart: 0.173 / 0.074 / 0.073 / 0.127, a 6x to 14x loss from false sharing.
(a) 16-byte mallocs: 1.000 at 2 threads and 4 threads, 0.575 at 8 P-cores and
0.673 with hyperthreads. The malloc'd parts are not at a fixed spacing: parts 0
and 1 were 4992 bytes apart, and the loss appearing only at 8 threads suggests
later parts sit about 32 bytes apart and share lines; I did not print every
part's address, so that explanation is inferred. For the redesign: lowering must
pad each unit's lock word to its own line (R1); with that, one object costs
nothing against separate objects, and without it the cost is large.

**A6 atomic ops.** Shared word: load 1983 / 3966 / 7931 / 15752 (a read-only
shared line scales, chain-bound), fetch_add 111 / 137 / 117 / 100, exchange
153 / 147 / 117 / 88, CAS loop 75 / 58 / 55 / 30. Per-thread line: load 1982 /
3964 / 7928 / 15744, fetch_add 553 / 1107 / 2213 / 1562, exchange 531 / 1062 /
2124 / 1754, CAS loop 311 / 622 / 1243 / 1850. Against A1 at 8 P-cores: shared
fetch_add 0.053, exchange 0.053, CAS loop 0.025; per-thread exchange 0.96, CAS
0.56. A hardware atomic on a contended word is 19x to 40x below per-thread
atomics, so atomic values suit flags and rare updates, and counters that must
scale need shards.

**A7 seq-structure.** noseq 330 / 594 / 1229 / 1520; seq 55 / 48 / 52 / 59.
seq/noseq 0.168 / 0.081 / 0.042 / 0.039. A global sequence taken inside the
entry lock caps the whole write path at about 50 M/s however many threads,
which is the cost a single shared sequence word puts on Firn's AOF path (a
24x loss at 8 P-cores). This is the reference for an atomic-value sequence in
a statement; a sharded or batched sequence would have to be compared to noseq.

**A8 timedwait.** Lateness of a 1 ms FUTEX_WAIT_BITSET deadline: 1 thread on
CPU 0: median 55.6 us, p99 56.1 us, longest 159 us (median of blocks), worst
1302 us. 8 threads on the 8 P-cores: median 51.1 us, p99 58.3 us, longest
400 us (median of blocks), worst 677 us. The 50-55 us floor is the kernel's
default timer slack, not queueing; a Whitefoot deadline wait on the same
primitive would be judged against about 55 us typical and a longest of a few
hundred microseconds.
