# Shared-counter arms (the measurements that need no language change)

The arms of the registered experiment in
[`research/investigations/shared-counters/DESIGN.md`](../../investigations/shared-counters/DESIGN.md)
(section 8) that run on today's language and compiler: how fast 64 contexts
update one count through one shared object, through their own objects, and
through a writer-built array of 16 shared objects, and what a single hardware
add costs against private increments. It also holds the C model of the
planned sharded state (W7, DESIGN.md sections 8.5 and 8.6), which gates its
implementation. The arms that need the sharded state (W4, W4r, the maximum
arms M1-M4 and the Firn arms) are not here; they wait for an implementation.

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
port's addition (zero except for W3rd) and carry the reader context's counts.

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
a `Shared` handle is not copy). Whether the compiler counts a handle for a
statement through such a local reference is recorded by `run.sh` as the number
of `wf__shared_share` calls in each arm's emitted LLVM (`host.txt`); W2r's
count is the reference for a statement with no handle count.

W3rd's statement over 16 targets names `h0 = &r0^, ... h15 = &r15^`, the
sixteen references `r0 ... r15` being `&shards[0_u64]` to `&shards[15_u64]`;
the language has no way to range a statement over a collection of handles, so
the sixteen are written out. The reader is one more context than the 64
writers, and its reads are not counted in `ops`.

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
  `shards.wf`, `shards_reader.wf`: one arm each. `w5.c`: W5, W5p and W5pN.
  `w7.c`: W7, W7t and W7rd.
- `run.sh`: builds the arms named in `ARMS` (every Whitefoot arm with
  `--full-lto`, `w5.c` and `w7.c` with `clang -O2 -pthread`; a selection of C
  arms alone, such as the C model's `ARMS="W5p W5pN W7 W7t W7rd"`, needs no
  Whitefoot compiler) and runs them under `taskset -c`. CPU lists `0,2`,
  `0,2,4,6`, `0,2,4,6,8,10,12,14` (one hyperthread of each of 2, 4 and 8
  P-cores) and `0-15` (8 P-cores, both hyperthreads), `WF_DRIVERS` equal to
  the list's CPU count. `PILOT=1` runs one block at the 8-P-core list only.
  Otherwise it runs six blocks (`BLOCKS`, provisional): each block runs every
  arm once per list, the arm order rotated by one position per block
  (`ORDER=alternate` also runs odd blocks' order backwards, as the C model's
  run does). It
  writes `raw.tsv` (one row per cell, columns in the script's header) and each
  cell's standard output and error. `host.txt` records the kernel, the CPU
  model and map, the governor, the compiler release and the twin's hashes.
- `summarize.py`: medians and ranges per arm and list, the noise `e = max
  |median(W2t) / median(W2) - 1|`, the descriptive ratios, the reader's
  statement time, and the registered conditions these arms can decide. Every
  threshold is provisional, as DESIGN.md section 8.4 says; a ratio is decided
  only when it clears its threshold by more than `e`, and with `e > 0.03`
  (provisional) it is inconclusive.
  - R2, approximated. No sharded state exists, so W1 stands in for W4 as the
    ceiling, and the summary reports W3/W1 at each list against 0.8. This is
    an inference: it decides R2 only in one direction, as the summary says.
  - R3: W5/W5p at 8 P-cores at least 0.5.
  - P3: W5/W5p at 8 P-cores at most 0.25.
  Not decided here: P1, P2, P4, P5, R1, R4, R5 and the descriptive M1-M4 and
  F-heap arms.

  With the C model's arms present it also prints, per list, the ratios
  W7t / W7, W7 / W5pN, W7 / W5p, W5pN / W5p and W7rd / W7 paired within
  blocks with their medians and 95% bootstrap intervals, the reader's median,
  99th-percentile and longest whole read, and the readings of section 8.6:
  the twin's validity, the gate W7 / W5pN against 0.5 at 2, 4 and 8 P-cores,
  the reader criteria, and the outcome. `--smoke` labels a wiring run on
  another host as no verdict.

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

## Running it again

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
