# Shared-counter and owned-buffer arms

The arms of the registered experiment in
[`research/investigations/shared-counters/DESIGN.md`](../../investigations/shared-counters/DESIGN.md)
(section 8): how fast 64 contexts update one count through one shared object,
their own objects, a writer-built array of 16 shared objects, or one
`Shared<Shards<u64>>`, and what a single hardware add costs against private
increments. It also holds the C model W7 (sections 8.5 and 8.6), maximum
arms M1–M4, and A1, random map writes with owned shard buffers and a periodic
whole-state drain. The branch run is registered in section 8.7 before
measurement; section 8.8 registers the taken-yield attribution comparison.
Firn F0–F3 remain a separate downstream experiment.

The harness is ported from the runtime-contention witnesses of Firn-wf, branch
`claude/witness-contention`, directory
`research/experiments/runtime-contention/`, at commit
`6c0e9733ae9b147fe483cba9cb409c359411be40` (the commit DESIGN.md section 2.1
cites; the files this port starts from, `harness.wf`, `base.wf`, `private.wf`,
`shared.wf` and `run.sh`, are identical at the branch tip
`e07d04fb57e069d673696c19d2e8855b48e228b2`). Both repositories are the
owner's. Its gate, deadline, batches of 1024 operations, local linear
congruential step and totals are kept: every context first waits at a gate
that carries the common deadline, then runs batches of 1024 operations, each
advancing a context-local 64-bit number, with one clock reading per batch,
and adds its count to a totals object once. The elapsed time runs from the
gate's opening to after the last join. One run prints one line,
`witness=NAME contexts=N drivers=D ops=OPS ns=ELAPSED ops_per_s=RATE
check=C reads=R read_ns=T read_max_ns=M`. The last three fields are this
port's addition (zero except for reader/drain variants). In W3rd and W4rd
they carry the reader's counts and times. In A1 they carry drain counts and
times, and `check` is the number of bytes drained; writer totals do not XOR
their LCG state into A1's byte count. A1's `ops` counts completed writes,
including writes whose full log buffer cannot accept another byte.

## Arms

Each cell is 64 contexts (threads for W5 and W5p) for 5 s; W5pN and the C
model's arms W7, W7t and W7rd start one thread per CPU of the list instead.

| Arm | Entry or file | Per operation | Role |
|---|---|---|---|
| W0 | `w_base` (`base.wf`) | none | runtime floor |
| W1 | `w_private` (`private.wf`) | `+wrap 1` on the context's own `Shared<u64>` | ceiling for an uncontended statement |
| W2 | `w_shared` (`shared.wf`) | `+wrap 1` on one `Shared<u64>` every context names, through a value handle | the contended base |
| W2t | `w_shared` again | as W2 | the same source built a second time as a separate binary: noise control; `host.txt` records both binaries' hashes |
| W2r | `w_shared_param` (`shared_param.wf`) | as W2, the object reached through a reference parameter whose row only reads it | attributes the handle count: the compiler counts no handle for such a statement |
| W6 | `w_read_shared` (`read_shared.wf`) | a statement on the one `Shared<u64>` that writes nothing | ties the earlier read-only row to a source; value handle, as W2 |
| W3 | `w_shards` (`shards.wf`) | `+wrap 1` on element `index mod 16` of an `Array<Shared<u64>, 16>` | the shard array a writer can build today (DESIGN.md control d2) |
| W3rd | `w_shards_reader` (`shards_reader.wf`) | as W3, plus one extra reader context | the reader sleeps about 1 ms, then reads all 16 objects in one atomic statement naming 16 targets, and times it; its reads, total nanoseconds and longest read are the last three output fields |
| W4 | `w_sharded` (`sharded.wf`) | `+wrap 1` in a one-shard statement on one `Shared<Shards<u64>>`, through a value handle | the implemented sharded state |
| W4t | `w_sharded` again | as W4 | same source built separately as a second binary; its hash and W4's are recorded in `host.txt` |
| W4n256 | `w_sharded` with a diagnostic compiler | as W4, yield threshold 256 instead of 64 | attributes the threshold-driven switching path; never an acceptance arm |
| W4nmax | `w_sharded` with a diagnostic compiler | as W4, yield threshold 1,048,576 instead of 64 | low-yield attribution control; it can still yield in a 5 s cell and never defines accepted behavior |
| W4r | `w_sharded_param` (`sharded_param.wf`) | W4 through a read-only reference parameter | distinguishes handle-place lowering from the shard path |
| W4rd | `w_sharded_reader` (`sharded_reader.wf`) | W4 plus one extra context reading every shard in one whole-target statement about every 1 ms | writer retention and longest whole read |
| M1 | `w_max_exclusive` (`max_exclusive.wf`) | exclusive `imax` on one `Shared<u64>` | maximum control |
| M2 | `w_max_read` (`max_read.wf`) | read statement, then exclusive `imax` only when the candidate exceeds the snapshot | check then update, preserving concurrent increases |
| M3 | `w_max_cache` (`max_cache.wf`) | context-local cache, exclusive `imax` only above it | Firn's maximum pattern |
| M4 | `w_max_sharded` (`max_sharded.wf`) | one-shard `imax` on `Shared<Shards<u64>>` | sharded maximum |
| A1 | `w_aof` (`aof.wf`) | random write over 100,000 keys of a shared map, plus a byte append when room remains in its own shard, in the same atomic statement | 64 writers and one extra context swapping each shard buffer for a fresh one about every 10 ms, counting drained bytes |
| W5 | `w5.c` | `atomic_fetch_add` (sequentially consistent) on one 64-byte-aligned `_Atomic uint64_t`, 64 threads | prices DESIGN.md candidate (a) without implementing it |
| W5p | `w5.c` built with `-DPADDED=1` | the same, each thread on its own padded line | the private ceiling for W5 |
| W5pN | W5p's binary | as W5p, one thread per CPU of the list instead of 64 | C own-line at the C model's thread count; the gate's comparison |
| W7 | `w7.c` | one writer thread per CPU, each taking its shard's ticket lock, adding 1 to the count beside it and releasing; `len` = twice the CPUs | the C model of the planned shard update (DESIGN.md section 8.6) |
| W7t | `w7.c` again | as W7 | the same source built a second time as a separate binary: the C model's noise control; `host.txt` records both hashes |
| W7rd | W7's binary | as W7, plus one reader thread pinned to `READER_CPU` (default 16, an E-core) outside the list | every 1 ms the reader takes every shard's lock in index order, sums, releases and times the whole read; reads, total, longest, median and 99th percentile are the output's reader fields |

W3's source form. The target `&shards[slot]` is refused (SHARE-2,
`AtomicTargetNotShared`: the checker reads the index step as a map step on the
array, and finds an `Array`), so each context takes a reference to its element
and names that reference as the target:

```
let mine = &shards[slot];
atomic kept = &mine^ {
  set state = ixor(state, kept^);
  set kept^ = kept^ +wrap 1_u64;
}
```

Each context owns its own array of 16 handles, built with `shared_share` in a
loop and `slots_into_array` (`array_filled` requires a copy element type, and
a `Shared` handle is not copy). The per-function LLVM counts in `host.txt`
and retained `bin/*.ll` let the reader distinguish update statements from
setup and teardown; whole-program counts alone do not establish a statement's
counts. Counts are recorded for every selected Whitefoot arm, including its separate-build
twin: `wf__shared_share` retains a handle, and `wf__shared_release` releases
it. Lowering's `SharedRetain` operation uses the same `wf__shared_share`
symbol as explicit `shared_share`; cleanup emits `wf__shared_release`,
including outside a statement. The runtime defining them is
`compiler/src/backend/completion/bridge.c`.

W4's update statement must emit neither handle-count call. The requested W2
falsifier (retain/release in its update statement) is unavailable on this
branch: PR #409's handle-place liveness optimization also skips W2, whose
block neither overwrites nor moves its value handle. Whole-program nonzero
counts include context setup and destruction and cannot serve as that
falsifier. `run.sh` records counts rather than changing W2's workload. W4r
remains a descriptive control; no reading attributes W2/W4 differences to a
retained W2 statement count.

W3rd's statement over 16 targets names `h0 = &r0^, ... h15 = &r15^`, the
sixteen references `r0 ... r15` being `&shards[0_u64]` to `&shards[15_u64]`;
the language has no way to range a statement over a collection of handles, so
the sixteen are written out. The reader is one more context than the 64
writers, and its reads are not counted in `ops`.

W4 uses `any_shard()` and `&counter[mine]` on a value handle; W4r uses
`&counter^[mine]` on a read-only reference parameter. W4rd's reader takes
`&counter` as a whole target and sums a labelled loop over `all^.len`.
Like W3rd, it runs on the same drivers as the writers and its reads are not
operations. W4rd also reports `read_hist_ns`, 65 comma-separated counts of
per-read durations. Buckets 0–63 have inclusive upper bounds 1, 2, 4, ...,
2^63 nanoseconds; bucket 64 holds larger values. The histogram is updated
after the end clock reading, preserving the interval around the whole-target
statement after sleep returns. The reducer reports each cell's p50/p99 as
nearest-rank bucket upper bounds capped by the observed maximum, including
the overflow bucket, and its exact observed maximum. Missing or malformed histograms are
reported as unavailable; percentile estimates do not replace the maximum-read
criterion.

M1–M4 take `v = state % 1048576_u64` from the local LCG. M3
updates its cache to the observed shared maximum inside its exclusive statement.
These maximum arms are descriptive, without a performance rejection threshold.

A1's `AofPart` owns a `Box<Slots<u8>>` with capacity **1,048,576 bytes per
shard**. A 10 ms window would fill it only above 104,857,600 appends per
second on that shard; the map-write path also hashes and locks a random key.
This capacity is conservative headroom, not a measured upper bound; delayed
drains can still fill it. At 8 drivers the current layout has 16 shard
buffers, about 16 MiB of byte capacity. The drain allocates each fresh buffer
and swaps it under the whole-target statement, then counts the old buffer's
length. After all contexts join, a final residual drain outside the timed
interval is added to `check`, but not to the reader count or times. It does
no file I/O. `check` records bytes actually appended and drained; `check < ops`
exposes bytes skipped by full buffers. The summary reports drained/write ratios
and skipped bytes alongside A1's scaling of completed write statements, so
buffer saturation is visible separately from throughput. Drain times include
allocation, swap and release of old buffers as well as lock acquisition.
This arm exercises ownership and the shared
update path; it does not measure AOF durability.

W5 and W5p run 64 OS threads under the same `taskset` list as the other arms,
so more threads than CPUs time-slice, where the Whitefoot arms run 64
contexts on as many drivers as the list has CPUs. The comparison is of the
cost of a contended or private increment, not of the schedulers.

W5pN, W7, W7t and W7rd start one thread per CPU of the list, because in the
planned runtime a shard has exactly one writer, the thread running a driver
(DESIGN.md section 8.6 gives the reasons and the decision rule). `w7.c` prints
the harness's line with `shards=`, `read_med_ns=`, `read_p99_ns=` and
`reader_cpu=` added, and exits 1 if the shards' counts do not sum to the
writers' operations, if a whole read sees a smaller total than the one before
it, or if the reader made no read or ran on another CPU; it refuses (exit 2)
a reader CPU inside the writers' list.

## Files

- `modules.wfg`, `module.wfm`: one module program with an entry per
  Whitefoot arm.
- `harness.wf`: argument reading, the start gate, the totals object, the
  local operation, the report line. Shared by every arm.
- `base.wf`, `private.wf`, `shared.wf`, `shared_param.wf`, `read_shared.wf`,
  `shards.wf`, `shards_reader.wf`: the original Whitefoot arms.
  `sharded.wf`, `sharded_param.wf`, `sharded_reader.wf`: W4 and its variants;
  `max_exclusive.wf`, `max_read.wf`, `max_cache.wf`, `max_sharded.wf`:
  M1–M4; `aof.wf`: A1. `w5.c`: W5, W5p and W5pN.
  `w7.c`: W7, W7t and W7rd.
- `run.sh`: builds the arms named in `ARMS` (every Whitefoot arm with
  `--full-lto`, `w5.c` and `w7.c` with `clang -O2 -pthread`; a selection of C
  arms alone, such as the C model's `ARMS="W5p W5pN W7 W7t W7rd"`, needs no
  Whitefoot compiler) and runs them under `taskset -c`. CPU lists `0,2`,
  `0,2,4,6`, `0,2,4,6,8,10,12,14` (one hyperthread of each of 2, 4 and 8
  P-cores) and `0-15` (8 P-cores, both hyperthreads), `WF_DRIVERS` equal to
  the list's CPU count. `PILOT=1` runs one block at the 8-P-core list only.
  The implementation run uses twelve blocks (`BLOCKS`): each block runs every
  arm once per list, the arm order rotated by one position per block
  (`ORDER=alternate` also runs odd blocks' order backwards, as the C model's
  run does). It
  writes `raw.tsv` (one row per cell, columns in the script's header) and each
  cell's standard output and error. `host.txt` records the kernel, the CPU
  model and map, the governor, the compiler identity and the twins' hashes.
- `summarize.py`: medians and ranges per arm and list, the noise `e = max
  |median(W2t) / median(W2) - 1|`, the descriptive ratios, the reader's
  statement time, and the registered conditions these arms can decide. Every
  threshold is provisional, as DESIGN.md section 8.4 says; a ratio is decided
  only when it clears its threshold by more than `e`, and with `e > 0.03`
  (provisional) it is inconclusive.
  - Historical data without W4: R2, approximated. W1 stands in for W4 as the
    ceiling, and the summary reports W3/W1 at each list against 0.8. This is
    an inference: it decides R2 only in one direction, as the summary says.
  - R3: W5/W5p at 8 P-cores at least 0.5.
  - P3: W5/W5p at 8 P-cores at most 0.25.
  With W4 present, R2 uses W3/W4 directly. P5/R5 need the separate Firn
  run. M1–M4 remain descriptive.

  The implementation section prints PASS/FAIL/INCONCLUSIVE with paired-block
  medians, 95% bootstrap intervals and sample counts. It requires a valid
  W4t/W4 twin interval containing 1, and at least six paired blocks on every
  list used by a criterion. It checks W4/W5p ≥ 0.5 at 2, 4 and 8 P-cores,
  W4/W3 ≥ 1.25 at 8 P-cores and 16 hyperthreads, W4rd/W4 ≥ 0.95 and the
  longest W4rd read across cells ≤ 50 µs at 8 P-cores, W4(8P)/W4(2P) ≥ 3,
  and A1(4P)/A1(2P) and A1(8P)/A1(4P) each ≥ 1.6. Cross-list ratios pair
  the same block and need valid twins at both endpoints. Missing arms, missing
  cells, zero reader counts or too few blocks leave the affected criterion
  inconclusive. W4's twin controls this run; it does not measure A1-specific
  noise. Section 8.7 registers the decision rule; no implementation results
  are recorded here yet.

  The reducer infers block numbers from rows in `raw.tsv`; it cannot detect a
  block with no surviving row at all. Check the recorded `BLOCKS` in `host.txt`
  and the workflow status before accepting a main-run reading. The workflow
  fails when cells fail or a run is interrupted; a partial artifact is evidence
  for diagnosis, not a completed registered run.

  With the C model's arms present it also prints, per list, the ratios
  W7t / W7, W7 / W5pN, W7 / W5p, W5pN / W5p and W7rd / W7 paired within
  blocks with their medians and 95% bootstrap intervals, the reader's median,
  99th-percentile and longest whole read, and the readings of section 8.6:
  the twin's validity, the gate W7 / W5pN against 0.5 at 2, 4 and 8 P-cores,
  the reader criteria, and the outcome. `--smoke` labels a wiring run on
  another host as no verdict; `--pilot` labels an initial run on the 14900K
  as no verdict, including the historical conditions.

## Results

The arms ran on 2026-10-12 on the owner's i9-14900K (self-hosted runner
`14900k`, native Ubuntu, kernel 7.0.0-38, governor `performance`) with the
compiler release `wf-29de8a21eef8` and `/usr/bin/clang` 22.1.8:

| Directory | Run | What it is |
|---|---|---|
| [`results/38303980083/`](results/38303980083/) | [pilot](https://github.com/Ming-Research/Whitefoot/actions/runs/38303980083) | one block of every arm at the 8-P-core list |
| [`results/38304337014/`](results/38304337014/) | [main](https://github.com/Ming-Research/Whitefoot/actions/runs/38304337014) | six interleaved blocks at all four lists; `blocks` was left at 6 after the pilot |

Each directory keeps the run's `raw.tsv` (one row per cell), `host.txt` (the
host, the CPU map, the release and the twin's hashes) and `summary.txt`
(`summarize.py`'s output). The cells' own standard output and error, which
the workflow uploaded under `runs/`, are not kept: every number they carry is
in `raw.tsv`. The reading of these results against the registered conditions
is DESIGN.md
[section 9](../../investigations/shared-counters/DESIGN.md#9-results-the-arms-that-need-no-language-change).

## Running the branch implementation

The temporary `.github/workflows/tmp-shared-counters.yml` is dispatch-only and
runs on `[self-hosted, 14900k]`. It builds this branch's compiler on that
runner, builds the selected arms, records emitted LLVM handle-count calls,
runs cells with `ORDER=alternate`, checks the summary against synthetic
passing, failing and missing-evidence controls, summarizes and uploads the
output directory. Every heavy step uses `perl .github/run-check.pl`, with
budget records and a final budget verdict. The runner must
be idle (under 5% busy over 5 s) before timed cells. Tell the other sessions
before a long run. Only CI builds and runs these programs.

Inputs are `PILOT` (default `1`), `BLOCKS` (default `12`, main only) and
`ARMS` (the original ten arms plus W4, W4t, W4r, W4rd, M1–M4 and A1,
and the optional W4n256/W4nmax diagnostic arms).
A pilot runs one 5 s cell per selected arm at the 8-P-core list and decides
nothing; inspect it before dispatching the main run. A main uses all four
CPU lists above and twelve alternating blocks. The default 19-arm main has
912 cells, 76 minutes of nominal cell time, plus startup and builds; a cell
failure leaves no raw row and fails the workflow. A narrower `ARMS` selection
is allowed, but the summary makes a criterion inconclusive when a required
arm or twin is absent.

```
gh workflow run tmp-shared-counters.yml --ref <branch> -f PILOT=1
gh workflow run tmp-shared-counters.yml --ref <branch> -f PILOT=0 -f BLOCKS=12
```

The procedure and decision rule are registered in DESIGN.md section 8.7.
Keep `raw.tsv`, `host.txt` and `summary.txt` under `results/<run id>/` after
the run, since artifacts expire. Remove the temporary workflow before
marking the branch ready.

### Taken-yield attribution run

Section 8.8 registers a six-block comparison with
`ARMS="W4 W4t W4n256 W4nmax W4rd W5p"`. The variants use the same W4 source
and full-LTO options. `run.sh` builds additional compilers in disposable
copies of the checked-out revision, with isolated Cargo output, changing the
inline threshold and runtime definition/assertion together. Exactly-one
replacement checks fail the build if the expected source sites have moved.
Only W4's entry is compiled with these compilers. The build requires each
variant executable to differ from W4 and a repeat build with the unpatched
compiler to be byte-identical to W4, and records the hashes and checks.
It adds no compiler or runtime changes to the branch. All builds and cells
run in CI under the existing labeled stages.

The summary reports paired W4n256/W4 and W4nmax/W4 ratios, W4nmax/W5p against
the registered 0.70 prediction and below-0.60 falsifier, and W4n256 recovering
at least 0.50 of the W4-to-W4nmax throughput gap. Six complete paired blocks
and a valid W4t/W4 twin are required. Acceptance still uses N=64. A nominal
per-yield cost model is descriptive: the current scheduler report does not
count taken yields, so the registered falsifier about measured per-yield cost
growing from 2 to 8 drivers remains inconclusive without that evidence.
Reader p50/p99/max are reported per cell, without new acceptance thresholds.

```
gh workflow run tmp-shared-counters.yml --ref <branch> -f PILOT=0 -f BLOCKS=6 -f ARMS="W4 W4t W4n256 W4nmax W4rd W5p"
```

## Repeating the earlier counter run

The measurement ran through a temporary dispatch-only workflow on the
self-hosted runner `14900k`, removed after the run; it is in git history at
commit `34df4f505` as `.github/workflows/tmp-shared-counters.yml`. It checked
that the host was idle (under 5% busy over 5 s), downloaded the Whitefoot
compiler release given as `release` and checked it against the release's
`SHA256SUMS` and manifest and against `/usr/bin/clang`'s LLVM major, built
every arm with `run.sh --build-only`, ran the cells with `run.sh --skip-build`,
ran `summarize.py`, and uploaded `raw.tsv`, `summary.txt`, `host.txt` and the
cells' output as an artifact. To run it again, restore it on a work branch,
push, and dispatch it there:

```
git show 34df4f505:.github/workflows/tmp-shared-counters.yml > .github/workflows/tmp-shared-counters.yml
gh workflow run tmp-shared-counters.yml --ref <branch> -f mode=pilot
gh workflow run tmp-shared-counters.yml --ref <branch> -f mode=main
```

It needs the 14900K runner: the CPU lists name that machine's P-cores and
their hyperthreads, and a hosted runner has neither the cores nor a quiet
host. Tell the other sessions before a run; one run holds the machine at a
time. Inputs: `release` (default `wf-29de8a21eef8`), `mode` (`pilot`, the
default, or `main`) and `blocks` (default 6, main only). The pilot took about
a minute of cells and the main run about twenty-one minutes of cells (10 arms
times 5 s plus a second of start-up per cell; 6 blocks times 4 lists), plus
the download and the builds. Copy the new `raw.tsv`, `host.txt` and
`summary.txt` into `results/<run id>/`, because artifacts expire, and remove
the workflow again before the branch is ready. This directory stays as the
record of the arms until the sharded-state arms join it or the investigation
closes.

## Running the C model

The C model (W5p, W5pN, W7, W7t, W7rd; DESIGN.md section 8.6) ran through
the temporary dispatch-only workflow `.github/workflows/tmp-shared-counters.yml`,
which replaced the one above and needs no compiler release. The workflow was
removed after its results were recorded. To repeat it, restore the C-model
version from commit `66b8aa5ea` on a work branch, push, and dispatch it there:

```
git show 66b8aa5ea:.github/workflows/tmp-shared-counters.yml > .github/workflows/tmp-shared-counters.yml
gh workflow run tmp-shared-counters.yml --ref <branch> -f mode=smoke
gh workflow run tmp-shared-counters.yml --ref <branch> -f mode=pilot
gh workflow run tmp-shared-counters.yml --ref <branch> -f mode=main
```

`smoke` runs on a GitHub-hosted runner: it builds the arms, checks that `w7`
refuses a reader inside the writers' list, runs two blocks of 0.2 s cells on
the host's own CPUs and summarizes with `--smoke`; it decides nothing.
`pilot` (one block at the 8-P-core list, about half a minute of cells) and
`main` (`blocks`, default 12, at the four lists, about 21 minutes of cells:
12 blocks times 4 lists times 5 arms times about 5.2 s) run only on the
14900K runner and stop on any other host. They check that the host is idle,
build and run under `.github/run-check.pl`, and upload `raw.tsv`,
`summary.txt`, `host.txt` and the cells' output. Tell the other sessions
before a timed run, copy the results into `results/<run id>/` and remove the
workflow before the branch is ready.
