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
