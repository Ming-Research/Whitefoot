# Halo P1 baseline — 2026-10-05

P1 fails on all six unscaled workloads and on the scaled binary-trees
workload. Every timed pair has matching checksums. No performance candidate
was implemented or selected; no compiler, Halo library, specification or
conformance file changed.

## Environment and identity

Apple M1 Pro, 32 GiB RAM, macOS 26.6.2 (25G83), native ARM64. The reference
is the supplied Redis 7.0.15 Lua interpreter; `lua -v` confirms Lua 5.1.5.
No network or Cargo command was used. All work and generated outputs stayed
in this worktree, apart from the existing host-wide check lock.

The existing `compiler/target/gate/whitefootc` compiled `modules.wfg`, entry
`bench`, with `--full-lto`, without a compiler cache. Build exit 0; wrapper
wall 478.87 s (tool wall 478.77 s, user 374.60 s, system 50.34 s). The
compiler was still active at 4:33; no linker was then active. Another
compiler process was observed during construction, so this single build
is not a clean compiler-cost measurement. Before timing, a process listing
found no compiler, clang or Cargo process. The measurement session held the
host-wide lock; background OS activity was not eliminated.

The native host and library source are reproduced by commit
`5d2ab819af3b822f5f051efbc657632c40dbbe82`. Launch records name the then-current
HEAD, usually `2e6152e4d95861c87c4cbb7c2e46277584e0662a`, while the host's
working changes were not yet committed. Their host-source hashes match
that later commit and the delivered host. The library/compiler sources
were unchanged from the task base
`75c9d7b48ae29642a36c078edd00b0048f0a2fd3`; binary hashes, rather than an
assumption about how the supplied compiler was built, identify the tools:

- Native Halo: `fe06a41d7a4955132abc0426b2b613e95abf51c9468757efc8e5328c8965e5fd`.
- Compiler: `c71614ecb1da4ab8b5c4cfea7bec7afa3a39aeb967657e1aaa2019c76dbf3fc5`.
- PUC: `dd2f2bb469b8c423292e23a5ab0dea2c4f3ea23658b76d55cd5f296d5d94e91e`.

[measurements.json](measurements.json) retains all calibration, baseline,
budget and profiler launch observations, exits, checksums, hashes, scales,
spread and profile counts. Machine-local paths use role placeholders.
[profiles.txt](profiles.txt) retains sample metadata, complete call graphs
and collapsed leaf counts for the 22 primary profiles and three short
controls, excluding system-image inventories. The excerpt hashes describe
the retained, redacted bytes. These evidence files serve baseline and
attribution reproduction and remain until superseded without that need.

## Sizing and checksum evidence

Fib was launched once, then in three alternating pairs. Its first cold Halo
launch was 0.454588 s versus a three-pair median of 0.223491 s; the first
launch is calibration, not a baseline datum. Fib's three-pair relative
ranges were PUC 0.16%, Halo 2.95%. Each other kernel received one PUC sizing
launch, one paired correctness/timing launch and three alternating pairs
before choosing six pairs. Calibration relative ranges (PUC / Halo) were:
loop 3.52% / 1.41%; integer-table 0.23% / 2.08%; string-key 5.68% / 4.26%;
concat 1.03% / 2.52%; sort 1.66% / 2.08%; binary-trees depth 14
1.12% / 0.78%. Six pairs suffice to distinguish both 1.0 and 1.5; even the
largest final PUC range, 9.21% on the short string-key launch, cannot move
its ratio near either threshold. No baseline repetitions were discarded.

PUC at binary-trees depth 16 took 6.083905 s, so depth was reduced to 14;
one PUC launch at 14 took 1.270769 s. All other counts are P1's counts.
The tree workload includes stretch, retained and paired temporary trees.
The depth-14 result does not establish Halo's time at depth 16.

The equal outputs in every baseline pair are:

| Kernel | Checksum | Completed Halo collections per launch |
|---|---|---:|
| fib | `832040` | 0 |
| loop | `5.00000005e+15` | 0 |
| integer-table | `50000005000000` | 5 |
| string-key | `28500000` | 0 |
| concat | `5000000` | 0 |
| sort | `1.0733795172001e+15` | 3 |
| binary-trees depth 14 | `-43682` | 176 |

Sort also asserts nondecreasing order. The comparison checks exact printed
bytes, not hidden VM results; six injected wrong/missing/extra checksum,
unexpected-suspension and malformed-stat controls were rejected. Profiles
add missing, zero and ambiguous-worker controls; the genuinely empty
string-key sample is also rejected by the repaired validator. Both VMs
use their ordinary collectors; Halo stress mode is off and its logical
live-heap limit is 2 GiB. The four-key string and concat fixtures exercise
repeated present-key lookup and interned short results, not unique-string
growth or a broad table-key distribution.

## Baseline medians

Wall time is process launch through exit, including source loading,
compilation, initialization, execution and teardown. Both VMs read identical
stdin source bytes. Launch order alternates PUC/Halo then Halo/PUC. Spread
below is min–max; relative ranges are retained in the JSON. Every native
and PUC launch exits 0. Every large-budget launch has zero suspensions.

| Kernel (count) | PUC median s | Halo median s | Halo / PUC | PUC min–max s | Halo min–max s | Pairs |
|---|---:|---:|---:|---|---|---:|
| fib (30) | 0.077791 | 0.223039 | 2.867 | 0.077257–0.079849 | 0.221508–0.226294 | 6 |
| loop (100,000,000) | 0.460990 | 1.363583 | 2.958 | 0.460242–0.466331 | 1.354988–1.390903 | 6 |
| integer-table (10,000,000 fill + read) | 0.201466 | 0.822095 | 4.081 | 0.199138–0.203031 | 0.812461–0.837778 | 6 |
| string-key (1,000,000) | 0.026168 | 0.057557 | 2.200 | 0.025944–0.028356 | 0.056842–0.058564 | 6 |
| concat (1,000,000) | 0.055690 | 0.208976 | 3.752 | 0.055315–0.057728 | 0.202937–0.211831 | 6 |
| sort (1,000,000) | 0.379271 | 0.744996 | 1.964 | 0.377892–0.380363 | 0.742024–0.753705 | 6 |
| binary-trees (depth 14) | 1.278944 | 2.442658 | 1.910 | 1.274211–1.283148 | 2.438182–2.448181 | 6 |

## Budget 1000

Budget 2^64−1 is Halo's unlimited sentinel: it skips decrementing. Budget
1000 is the corpus default. One run and a three-pair spread check preceded
six selected pairs; the three-run Halo relative range was 1.02%.

| Mode | PUC median s | Halo median s | Halo min–max s | Suspensions | Collections | Pairs |
|---|---:|---:|---|---:|---:|---:|
| Unlimited | 0.460990 | 1.363583 | 1.354988–1.390903 | 0 | 0 | 6 |
| 1000 | 0.460766 | 3.043277 | 3.031091–3.071511 | 100000 | 0 | 6 |

The budget-1000 Halo median is 2.232× unlimited (+123.18%, +1.679694 s).
This is total decrement plus suspension/resume overhead in this embedding,
not an isolated counter-cost measurement or a real Redis end-to-end run.
The two modes were measured in separate batches, each interleaved with PUC;
PUC medians stayed within 0.05%. The large difference is resolved, but these
runs do not determine a sub-1% cost as P3 ultimately requires; measured total resume cost here exceeds its target.

One profiled budget-1000 run has 2355 worker samples: 1267 (53.8%) in
`embed.refresh_roots` or its descendants, including 689 in memmove/stub
copying, 248 in free descendants and 196 in allocation descendants; 912
(38.7%) in dispatch arms and 163 (6.9%) in the GC safepoint path. It
completed zero collections and 100000 suspensions. The source
`embed/engine.wf::refresh_roots` rebuilds the constant/pin bridge on each
resume, and `append_value` reserves one additional slot when full. The
measured attribution is repeated bridge copying/allocation, not collection
or a guess that arithmetic decrements alone explain the cost. A same-source
control would be needed to measure the gain from changing that mechanism.

## Attribution of ratios above 1.5

`/usr/bin/sample PID 10 1 -file REPORT` sampled at a requested 1 ms interval
until the process exited. The first sandboxed attempt returned 255 and no
report (runner exit 1); it is retained as a failed attempt. Local
process-inspection access then worked without network. An original
string-key launch returned profiler exit 0 but had an empty call graph;
the runner now refuses that condition. Fib(30) and concat at 1e6 captured
only 98 and 82 worker samples, so attribution alone lengthened fib to 34,
string-key to 20,000,000, and concat to 10,000,000. Baseline timings above
remain at the requested counts. Each extended workload was first run once.
A one/three-loop profiler calibration found a 2.88% Halo range; three
profiles per workload provided stable broad work categories. The extended
three-run Halo ranges were 0.98%, 3.69% and 3.25%, respectively. Primary
profile timings are excluded from baseline medians.

Counts use the execution worker under `wf__main_body`; the main thread's
matching `__ulock_wait` samples are excluded. For each call-graph node,
exclusive count is its count minus immediate children's counts. The
exclusive counts sum to that worker's total in each of the 22 profiles.
Groups give GC/safepoint and root-refresh ancestors priority, then sorting,
concat/intern, Lua frame helpers, slow executor, table heap and dispatch;
this keeps categories disjoint. `measurements.json` retains both the groups
and all exclusive symbol counts, so the grouping can be independently
recomputed from the complete call graphs. These are sampled occupancy,
not dispatch/call counts or causal fractions of the Halo–PUC difference.

| Workload profiled | Worker samples, three runs | Measured broad work (range across runs) |
|---|---|---|
| fib(34) | 1117, 1108, 1123 | Dispatch arms 48.2–51.8%; Lua call/frame helpers 43.8–47.8%; safepoint path 3.4–4.4%; no completed GC |
| loop, 1e8 | 1026, 999, 1019 | AddRR/ForLoop arms 85.7–87.8%; safepoint predicate 12.2–14.3%; no completed GC or sampled slow executor |
| integer-table, 1e7 | 551, 556, 555 | Table heap 35.7–37.9% (including rehash); dispatch 39.0–43.4%; slow executor 8.0–10.3%; GC/safepoint 7.7–8.8% |
| string-key, 20e6 | 792, 690, 798 | Dispatch arms 89.0–91.3%; table heap 5.6–6.5%; safepoint 2.8–4.5%; no completed GC |
| concat, 10e6 | 1508, 1562, 1563 | Concat/intern plus slow executor 60.0–66.8%; dispatch 31.5–37.1%; no completed GC |
| sort, 1e6 | 508, 513, 514 | Sorting library 73.7–76.2%; dispatch 9.1–10.5%; GC/safepoint 0.4–0.8% |
| binary-trees, depth 14 | 1845, 1854, 1848 | Table heap/allocation 28.4–29.4%; dispatch 28.7–30.2%; Lua calls/frames 19.0–20.3%; GC/safepoint 12.6–14.0%; slow executor 6.6–7.4% |

Sampling attaches after launch, omits the earliest part of execution, and
can alias very short hot loops. Tail calls and inlining erase some caller
ancestry; a table helper sampled at the root cannot be assigned to a
particular slow call. The table/sort workloads have allocation and execution
phases, so omitted early samples can bias phase shares. The three runs
support broad locations of work, not precise instruction-level costs.
We did not profile PUC or run an isolating before/after implementation pair,
so these percentages do not measure how much of the gap each cause explains.

### Value width, handles and native dispatch inspected first

`xcrun llvm-objdump --macho --disassemble --no-show-raw-insn` inspected both
hashed native binaries; exit 0 for both. The Halo Move arm loads/stores
`q0` (16 bytes) with `lsl #4`; PUC `luaV_execute` derives register addresses
with `uxtb #4` and `lsl #4`. Both value-slot strides are 16 bytes. Value
width by itself is therefore not a demonstrated difference from PUC.
Numeric AddRR still copies both 16-byte operands to temporary stack slots,
reads their tags and payloads back, and copies `Step`/continuation fields:

```text
Halo run arm 20 (AddRR):
100031c74: ldr q1, [x11, x9, lsl #4]
100031c7c: ldr q0, [x11, x9, lsl #4]
100031c80: stp q0, q1, [sp, #0x70]
100031c84: ldr w9, [sp, #0x80]
100031c88: cmp w9, #0x3
100031c94: ldr w9, [sp, #0x70]
100031c98: cmp w9, #0x3
100031ca4: ldr d0, [sp, #0x88]
100031ca8: ldr d1, [sp, #0x78]
100031cac: fadd d0, d0, d1
```

Sampled AddRR offsets include +76/+84 (the temporary store and tag compare),
but the collapsed reports do not give separate per-instruction counts.
This establishes that traffic/tests remain on the numeric hot path; it does
not assign a percentage or measured gain to eliminating them.

Numeric AddRR and ForLoop use no table/string handle. Fib has closure and
upvalue access; string-key GetTableR calls `node_find`, whose exclusive
samples account for most of the reported table-heap group. The source
GetTableR/GetTableK handlers check table handle bounds and `live`, and string
key paths check string handles. Those checks occur inside sampled arms and
helpers and cannot be separated from lookup, tags or payload access by these
profiles. Thus handle-check overhead remains unquantified; it is not asserted
to be the numeric-loop cause. No completed GC on fib or loop rules out full
collection work there, while safepoint predicate cost remains visible.

The current native build has 73 `run.body.arm.N` symbols and indirect jumps
in the hot arms: handlers have been inlined and source joining does not
produce only one native dispatch point. For example, AddRR's epilogue still
checks the frame and constant-pool windows, moves continuation fields, and
then dispatches directly:

```text
100031dc0: add x8, x27, #0x100
100031dcc: cmp x8, x9          ; stack window
100031ddc: add x8, x28, #0x100
100031de8: cmp x8, x9          ; constant window
100031e08: ldur q0, [x19, #0x58]
100031e0c: str q0, [x19, #0x70]
100031e14: ldp x26, x27, [x19, #0x70]
100031e18: mov w8, #0xc        ; 12-byte Cell stride
100031e20: madd x1, x26, x8, x0
100031e24: ldr w8, [x1, #0x10]!
100031e28: ldr x4, [x2, x8, lsl #3]
100031e38: br x4
```

ForLoop arm 66 calls `collect_if_due` on its hot path; sample occupancy
corroborates that call even with zero collections. Cell stride is 12 bytes,
where VM.md section 4 proposed 8. PUC fetches a four-byte instruction
(`ldr w23, [x28], #0x4` at `1000163d0`). This is a measured representation
discrepancy, not a demonstrated throughput attribution or a proposal to
weaken any bounds/handle condition.

### C1–C6 implications, without selection

- C1: current native hot arms already end in indirect tail jumps. Source
  epilogue changes might still change continuation/guard traffic, but the
  predicted gain from replacing one shared native dispatch point is not
  supported on this build; a matched source pair is still needed.
- C2 and C3: numeric profiles and the native AddRR spills/stores point to
  testing accumulator/pinned-local traffic reduction, while preserving
  safepoint stack roots. No measured gain or register-budget result here.
- C4: native AddRR and ForLoop retain numeric tag tests on the sampled paths.
  Specialization is a plausible test; these profiles do not isolate tag cost.
- C5: the kernels largely keep hot state in locals. Sort's checksum phase
  reads global `assert`, but sorting dominates its profile; there is no Redis
  corpus measurement here to select a globals fast path.
- C6: indexed frame/code address calculation, 12-byte Cell stride, repeated
  window tests and continuation traffic are observed on hot paths. This
  supports a representation/lowering experiment, without selecting one.

The budget root bridge, table rehash/allocation, concat helpers and sorting
library also need their own experiments; C1–C6 do not directly cover all
these measured costs. Profile shares cannot predict same-source speedups.

## Commands and exits

Executed commands (reference path shown as `<reference-root>`):

- Full-LTO build shown above: exit 0. Early host authoring probes exited 1
  on FORM-2 formatting, EFF-2 rows and OWN-1 move requirements; these were
  fixed before the successful build or any native timing. Lock-contention
  attempts exited 75 and ran no build or benchmark.
- Under `perl .github/run-check.pl halo-bench-measure /bin/zsh -f`,
  `python3 research/experiments/halo-bench/run.py --lua <reference-root>/redis/deps/lua/src/lua --kernels K --runs 1|3|6 --out ...`:
  exit 0 for every calibration and baseline checksum pair. Initial PUC sizing
  adds `--reference-only`; binary-trees adds `--scale binary-trees=14` after
  the depth-16 PUC sizing run. Final baseline: seven kernels × six pairs.
- Loop budget checks add `--budget realistic --runs 1|3|6`: exit 0,
  checksum unchanged, 100000 suspensions per launch.
- Profiler checks under the same wrapper pattern with label
  `halo-bench-profile` add `--profile`; primary runs use `--runs 3` and the
  attribution scales above: all native, reference and profiler exits 0.
  The budget profile uses `--budget realistic --runs 1 --profile`.
- First sandboxed `sample`: exit 255, runner exit 1, no report. The early
  empty string-key profiler returned 0; inspection found no worker samples,
  and the repaired validator rejects that report. Neither enters attribution.
- `python3 -m py_compile research/experiments/halo-bench/run.py`, validator
  valid/invalid controls, and native disassembly: exit 0.

`make design-lint` calibration passed (exit 0, wrapper wall 7.86 s).
`make static` passed all seven stages (exit 0): repository invariants, spec
archives, README translation, spec prose integrity, guidance, source size
and design lint. `git diff --check` passed (exit 0). These checks cover the
results and TODO/status edits in this working tree on parent
`5d2ab819af3b822f5f051efbc657632c40dbbe82`; the native host hashes are
unchanged. Independent review is pending. The canonical `make check` was
not run: it invokes Cargo, which this task prohibits. No specification rules or design-tree decisions changed;
no decision card is needed for measuring the already requested P1 baseline.

## Found along the way

- Fixed within the runner: a successful profiler exit can have an empty call
  graph. It now requires one nonzero sampled execution worker; valid, empty,
  zero and ambiguous controls distinguish the failure.
- Deferred in `docs/todo.md`: repeated resume root-bridge copying/allocation;
  Cell width differs from the proposed layout; baseline hot-path costs and
  stale single-native-dispatch attribution. No compiler or VM fix was made.
- The pre-existing full-package check cost remains open; this build's total
  time and contention do not isolate that cause.
