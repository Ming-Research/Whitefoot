# Halo P1 baseline — 2026-10-05

The initial P1 baseline below fails on all six unscaled workloads and on
the scaled binary-trees workload. Every timed pair has matching checksums.
At baseline no performance candidate was implemented or selected; no compiler,
Halo library, specification or conformance file changed. The later Halo cost
repair records its criteria and comparisons separately below.

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
unchanged. The canonical `make check` was not run: it invokes Cargo, which this task prohibits. No specification rules or design-tree decisions changed;
no decision card is needed for measuring the already requested P1 baseline.

## Independent review

A separate read-only reviewer, configured as GPT-6.1-sol, reviewed
`75c9d7b48ae29642a36c078edd00b0048f0a2fd3..d993ea49f7d670bc401d6115c1cd099c6015cb83`
against checklist groups A, D, R, M and V, including the design-tree
correspondence checks. C and T were not applicable because compiler,
library, formal tests, specification and gate wiring are unchanged;
publication was waived by the explicit no-push/no-PR instruction.

The reviewer read all changed artifacts, the VM commitments, research method,
compiler root and dispatch/self-tail decisions, and the embedding root-bridge
implementation. It independently recomputed accepted pair ordering,
checksums/stats/exits, medians/ranges/ratios and budget arithmetic; verified
source and available binary hashes; reconstructed every primary profile's
exclusive symbol counts and group totals from raw call graphs; and checked
all 22 primary excerpt hashes. No findings within scope. No suites were
rerun, and no file was edited by the reviewer. The remaining depth-16,
causal-speedup, isolated-decrement and full-gate limits remain open.

## Found along the way

- Fixed within the runner: a successful profiler exit can have an empty call
  graph. It now requires one nonzero sampled execution worker; valid, empty,
  zero and ambiguous controls distinguish the failure.
- Deferred in `docs/todo.md`: repeated resume root-bridge copying/allocation;
  Cell width differs from the proposed layout; baseline hot-path costs and
  stale single-native-dispatch attribution. No compiler or VM fix was made.
- The pre-existing full-package check cost remains open; this build's total
  time and contention do not isolate that cause.

## Halo cost repair criteria (recorded before measurement)

Requested scope: persistent embedding root bridges and an inline not-due
collector path; no compiler, language, oracle or normative expectation change.
Keep a candidate only after the unchanged 80 scripts at budgets 1, 7, 1000
pass all 240 comparisons. Stress replies and per-case collection counts must
remain identical at those budgets. Recheck local roots, the isolated frame
and suspended-stack witnesses, the embedding lifecycle probe, and a missing
root negative control.

1. Roots-only: loop(100,000,000), budget 1000, median at most 1.15 times
   the same binary's unlimited median. Report `(budget time - unlimited
   time) / 100000` before and after as total incremental time per suspension,
   including budget charging, rather than isolated resume latency.
2. Inline safepoint: loop unlimited median at least 5% lower than the
   roots-only binary in alternating same-source pairs; otherwise restore the
   safepoint change and retain its negative result.
3. Final P1: unchanged seven kernels, binary-trees depth 14, six alternating
   PUC/Halo pairs each, with matching checksums and unchanged collection counts.

Calibrate each selected comparison with one pair then three pairs, inspect
spread, and use six pairs unless uncertainty crosses a decision threshold.
Native timing builds use the supplied gate compiler and full LTO. Build
observations are separate from program timings. Before/after native pairs
use identical stdin bytes and alternate launch order, with independent PUC
checksum validation. Generated logs/binaries stay in `target/`; retained raw
observations serve this experiment until reproduction is no longer needed.

Structural assessment before implementation: keep the bridge in its existing
constant-pool tail so the collector's one constants argument and VM roots stay
unchanged. Per-script dirty flags invalidate all tails on root-set edits;
boolean invalidation cannot wrap as a generation can. Rebuild directly from
original constant lengths and pins, with geometric reserve; stale inactive
tails are never sources. A separate root registry and collector argument would
change that boundary without improving this experiment's required observation.
Keep the collection body separate and gate it at its callers using the exact
existing three-trigger predicate; a combined allocation/budget counter would
couple separate semantics and is not selected here. Q1, raised during the invalidation audit: make root storage public readonly,
retaining inspection while routing mutations through compile/pin/unpin/forget_all.
Otherwise external writes could bypass invalidation and lose a root. This is
the recommendation under test; owner ruling remains open. No mutable-constants
API is introduced without a concrete consumer.

### Fresh before control

The retained P1 timing binary, gate compiler and Redis Lua hashes match the
baseline identities above. A fresh full-LTO embedding host built successfully
in 464.06 s (exit 0); this construction scale was sized by the existing P1 and
F4 full-package build observations, rather than opening an hours-long batch.
Its one-script cold stress calibration passed in 0.245067 s. Warm whole-corpus
execution is sized separately from construction. The fresh before control
passes 240/240 ordinary comparisons and 240/240 stress comparisons. Stress
collections total 20,450 at each budget; local roots total 25 at each budget;
the isolated frame and parked-stack witnesses have 9 and 14 respectively.
The lifecycle probe exits 0. These fresh per-case counts, rather than an
assumption about historical outputs, are the preservation reference.

The loop budget comparison used one pair (cold unlimited 1.610025 s), then
three warm pairs (relative ranges 0.84% unlimited / 0.35% budget 1000).
Six selected pairs give 1.356240 s unlimited (1.355378–1.367506) and
3.044296 s at budget 1000 (3.035735–3.052223), ratio 2.245×, 100,000
suspensions and no collections per budgeted launch. Incremental time per
suspension is 16.881 microseconds. Calibration is retained, not pooled into
the selected medians. Every checksum agrees with independently executed PUC.

### Roots-only correctness milestone

The roots-only timing host and embedding host built with full LTO, exits 0,
448.25 s and 453.77 s respectively. Ordinary oracle 240/240, stress oracle
240/240, local roots 9/9, isolated frame 1/1 and parked snapshot 1/1 pass.
A tool comparison of every case/budget/status/collection row against the
fresh before control is identical, including 20,450 stress collections at
all three budgets, local totals 25, frame 9 and parked snapshot 14.
The extended lifecycle probe exits 0, checking that pin and compile while
suspended add roots before collection, and unpin while suspended releases its
sole pinned value at the next collection. Final selection awaits the complete
six-pair criterion measurement; the first same-binary budget pair is 1.007×.

The three-pair native before/roots calibration completed (exit 0), with a
0.43% before range and 5.92% roots range; even that full range separates the
large budgeted gain. A subsequent budget three-pair run was interrupted
(exit 129; outer interactive measurement shell exit 143) before its last
launch completed and produced no result JSON. Its partial log is retained
but none of those launches enters a selected median. The interruption's
cause is unknown; the repeat uses a noninteractive wrapper.

### Roots-only selection

Six alternating before/roots pairs at budget 1000 give before 3.049088 s
(3.032233–3.079309), roots 1.360235 s (1.355972–1.376209), a 55.39% reduction.
Six separate alternating unlimited/budget pairs on the roots binary give
1.359659 s (1.353488–1.363208) and 1.361283 s (1.358142–1.367504), ratio
1.001194×: **passes 1.15×; retain the roots change**. Relative ranges are
0.71% unlimited and 0.69% budgeted, far from the decision boundary.
Every launch exits 0, matches independent PUC, has zero collections and
100,000 budgeted suspensions. Incremental time per suspension changes from
16.881 microseconds in the fresh before control to 0.016 microseconds here.
That after point estimate is smaller than launch variation; it is not a
precise isolated resume or decrement latency, nor a proof of P3's 1% target.
The resolved resume-copying TODO is removed; full-package construction cost
and unrelated P1 hot paths remain deferred.

The initial safepoint authoring build exited 1 in 1.39 s at GRAM-9, refusing
nested `bor` calls in atom positions. A second authoring build exited 1 in 1.39 s at GRAM-5 because comparisons
also require binders in call argument positions. Binding all three comparisons
and the intermediate boolean values preserves the proposed predicate; no timing from that failed build is used.

### Inline safepoint measurement

The full-LTO timing build exits 0 in 477.21 s. The first new-binary launch
(1.485965 s) is calibration only. Three warm pairs give relative ranges
0.53% roots-only / 0.54% inline. Six selected alternating pairs give
roots-only 1.387613 s (1.383352–1.389477), inline 1.245011 s
(1.237761–1.248955), ratio 0.897232×, **10.28% faster: passes the 5%
performance criterion**. Relative ranges 0.44% / 0.90% cannot move the
conclusion across the threshold. Both sides use identical loop source bytes,
match independently executed PUC, exit 0, suspend zero times and collect zero
times. The final embedding oracle and stress checks below also pass; retain the
inline safepoint change.

The three tests of the paired runner distinguish valid data (exit 0), unequal
collection counts (exit 1), and wrong native checksums (exit 1); agreeing wrong
native outputs also fail against independent PUC (exit 1). Missing required
root-control flags fail with argparse exit 2. Lock-contention exits 75 launch
no build or benchmark; retrying acquisition never bypasses the host-wide lock.

### Final correctness and retained changes

Both changes are retained. The final full-LTO embedding build exits 0 in
457.89 s. Final ordinary oracle 240/240, stress oracle 240/240, local roots
9/9, frame isolation 1/1, suspended snapshot 1/1 and the extended lifecycle
probe pass. A tool compares every case/budget/status/collection row with the
fresh before control and finds identical outcomes and counts. Stress totals
remain 20,450 at each budget; local-root totals 25, isolated frame 9 and
parked snapshot 14. The independent every-allocation verifier remains open.

The parked-root omission control returns native exit 0 with a Lua error,
`attempt to index a function value`, instead of the unchanged expected bulk
`zzz`; the comparison exits 1 as required (5 collections before the error).
The positive witness returns `zzz` with 14 collections. Only the harness's
parked root visibility changes during the synthetic collection; the snapshot
is restored before resume and the ordinary collector and continuation are
used. No expected reply or oracle source is changed.

The 64 MiB memory exhaustion and same-engine/store recovery witnesses pass
with stress off, before and after, at budgets 1, 7 and 1000: the same
`not enough memory` error followed by bulk `alive`, 8 completed collections
and 9,846 recovered heap bytes at every budget. This separately exercises the
nonzero-limit path rather than hiding it behind stress's always-due trigger.
These existing F4 fixtures were sized by their recorded sub-second executions;
construction and execution remain separate. Q1 is still open: the readonly
root-storage boundary is a recommendation in the live tree, not an approval.

### P1 rerun after the retained changes

Every workload was first launched once and then in three alternating pairs
at the unchanged P1 counts (binary-trees depth 14). Calibration relative ranges
(PUC / Halo):

| Kernel | PUC range % | Halo range % |
|---|---:|---:|
| fib | 0.62 | 0.45 |
| loop | 16.71 | 2.45 |
| integer-table | 1.73 | 0.81 |
| string-key | 1.51 | 2.24 |
| concat | 0.76 | 2.41 |
| sort | 0.92 | 2.85 |
| binary-trees | 0.75 | 1.45 |

The loop reference's 16.71% calibration range does not approach the P1 ratio
boundary; six pairs still distinguish P1 failure on every workload. These
calibrations are retained and excluded from the table. The final six-pair
run alternates PUC/Halo and Halo/PUC and verifies equal source hashes,
checksums and completed collections against the initial baseline.

| Kernel | Before PUC s | Before Halo s | Before ratio | After PUC s | After Halo s | After ratio | After PUC min–max s | After Halo min–max s | Pairs |
|---|---:|---:|---:|---:|---:|---:|---|---|---:|
| fib | 0.077791 | 0.223039 | 2.867 | 0.080212 | 0.227613 | 2.838 | 0.080107–0.080977 | 0.225034–0.228453 | 6 |
| loop | 0.460990 | 1.363583 | 2.958 | 0.473806 | 1.249755 | 2.638 | 0.472441–0.474899 | 1.247932–1.274765 | 6 |
| integer-table | 0.201466 | 0.822095 | 4.081 | 0.206035 | 0.802504 | 3.895 | 0.204913–0.207563 | 0.801091–0.934778 | 6 |
| string-key | 0.026168 | 0.057557 | 2.200 | 0.027376 | 0.059457 | 2.172 | 0.026830–0.028418 | 0.058630–0.059922 | 6 |
| concat | 0.055690 | 0.208976 | 3.752 | 0.057729 | 0.213089 | 3.691 | 0.057574–0.058078 | 0.205546–0.216955 | 6 |
| sort | 0.379271 | 0.744996 | 1.964 | 0.391232 | 0.759584 | 1.942 | 0.387766–0.400727 | 0.752823–0.768441 | 6 |
| binary-trees | 1.278944 | 2.442658 | 1.910 | 1.326630 | 2.491015 | 1.878 | 1.314767–1.331011 | 2.476898–2.513080 | 6 |

All native and reference exits are 0; every unlimited launch has zero
suspensions. Collections remain fib 0, loop 0, integer-table 5, string-key 0,
concat 0, sort 3 and binary-trees 176 in all six launches. P1 still fails on
all seven workloads. Before and after P1 tables are separate sessions;
reference medians changed too, so their absolute differences do not isolate
causal gains on the other kernels. Only the alternating native source pairs
above isolate the selected mechanisms. No post-change profiles were collected;
baseline sample percentages are not current occupancies. Depth 16 and real
Redis end-to-end performance remain unmeasured here.

### Budget comparison before and after

Final-binary calibration uses one pair and then three; the latter ranges are
1.15% unlimited and 4.02% budgeted. Even the observed extremes are below the
1.15× limit, so six pairs suffice for the requested criterion. All selected
budgeted launches have exactly 100,000 suspensions and zero collections;
unlimited launches suspend and collect zero times.

| Version | Unlimited median s | Budget 1000 median s | Budget / unlimited | Incremental µs / suspension | Unlimited min–max s | Budget min–max s | Pairs |
|---|---:|---:|---:|---:|---|---|---:|
| Before | 1.356240 | 3.044296 | 2.244658 | 16.881 | 1.355378–1.367506 | 3.035735–3.052223 | 6 |
| Roots only | 1.359659 | 1.361283 | 1.001194 | 0.016 | 1.353488–1.363208 | 1.358142–1.367504 | 6 |
| Both retained changes | 1.255465 | 1.279899 | 1.019462 | 0.244 | 1.245278–1.265214 | 1.255019–1.294559 | 6 |

The final binary's ratio 1.019462× also passes the root criterion. Its
incremental estimate is 0.244 microseconds per suspension, including all
100 million budget charges, startup and embedding work divided by 100,000
suspensions. Final relative ranges are 1.59% unlimited / 3.09% budgeted;
paired delta variation and the roots-only estimate's near-zero size prevent
interpreting these as isolated resume latencies. The final median budget cost
is 1.95%, so this experiment does not establish P3's under-1% target. No extra
runs were selected to answer that separate question.

### Repair commands, identity and validation

[Cost measurements](cost-measurements.json) retain all 30 calibration/selected
run records with launch order, source/tool hashes, exits, checksums, stats and
spreads, complete oracle/root reports, memory observations, construction logs
and admission controls. This file serves reproducibility of the retained
repairs and is removed when that need ends. The final library manifest names
every Halo source digest. Reused binaries are identified by their SHA-256,
not an assumption that a launch record's HEAD describes uncommitted code.
Before/roots native sources correspond to the initial baseline and the roots
milestone respectively; final timing sources include only the safepoint code
change beyond the roots implementation, plus a doc clarification. The final
embedding host also includes the new root-omission control and lifecycle probe.
The supplemental before-memory report's working-tree source digest is from the
later runner; its reused binary hash identifies the original before host.

Commands below ran locally; `<reference-root>` is the existing Redis 7.0.15
checkout. Native construction and measurement run through
`perl .github/run-check.pl LABEL COMMAND ...`. Lock acquisition retries only
exit 75, never a failed executed command. Scratch fixture files are directed
into the benchmark's existing `target/` (earlier runs used a one-shot in-process
temporary-directory redirect; later ones use `--scratch-root`). No network,
Cargo, push or PR command was used.

- `compiler/target/gate/whitefootc --graph research/experiments/halo-e2e/modules.wfg --entry test --full-lto -o research/experiments/halo-bench/target/e2e-before|e2e-roots|e2e-after`: all exit 0, wrapper walls 464.06 / 453.77 / 457.89 s.
- `compiler/target/gate/whitefootc --graph research/experiments/halo-bench/modules.wfg --entry bench --full-lto -o research/experiments/halo-bench/target/halo-roots|halo-after`: exits 0, 448.25 / 477.21 s. The before timing binary is the retained P1 full-LTO binary. Two preliminary syntax probes exit 1 (GRAM-9 and GRAM-5, each 1.39 s), with no timing admitted.
- `python3 -B research/experiments/halo-bench/run.py --lua <reference-root>/redis/deps/lua/src/lua --before-binary BEFORE --binary AFTER --kernels loop --runs 1|3|6 --out target/NAME.json`, adding `--before-budget large|realistic --budget large|realistic` for the mode pairs: every completed run exits 0. Selected files are `before-budget-six`, `roots-six`, `roots-budget-six`, `safepoint-six` and `after-budget-six`. The incomplete budget calibration exits 129, outer shell 143; those observations are excluded. Native wrong-checksum/count admission controls exit 1 as required; valid data exit 0.
- `python3 -B research/experiments/halo-bench/run.py --lua <reference-root>/redis/deps/lua/src/lua --binary research/experiments/halo-bench/target/halo-after --kernels fib,loop,integer-table,string-key,concat,sort,binary-trees --scale binary-trees=14 --runs 6 --out research/experiments/halo-bench/target/after-p1.json`: exit 0. Per-kernel one/three calibration uses the same flags and selected single kernel; all exits 0.
- `python3 -B research/experiments/halo-e2e/run.py --scratch-root research/experiments/halo-bench/target --compiler compiler/target/gate/whitefootc --binary research/experiments/halo-bench/target/e2e-after --budgets 1,7,1000 --report target/REPORT.md`: exit 0, 240/240; add `--gc-stress`: exit 0, 240/240. The before and roots hosts also pass both 240-comparison modes.
- Same runner with `--cases research/experiments/halo-gc/cases --gc-stress --budgets 1,7,1000`: exit 0, 9/9. Add `--filter gc/frame-closure --isolate-frames --budgets 1`: exit 0, 1/1. Add `--filter gc/suspended-stack --collect-suspended --budgets 1`: exit 0, 1/1. These pass on before, roots and final hosts. Adding `--omit-suspended-root` to the final suspended run exits 1 on the intended reply mismatch, native exit 0. Missing required omission flags exit 2 (expected).
- `research/experiments/halo-bench/target/e2e-before|e2e-roots|e2e-after a b c`: lifecycle probes exit 0 (roots/final include the suspended root-set update extension).
- Same e2e runner with before/final host, `--filter lua-core/counter-closure --verify-memory --budgets 1,7,1000`: exits 0, independently expected exhaustion and recovery replies and equal counts/bytes.
- RESP2 sensitivity and paired validator controls: exit 0 for the checking harness; deliberate rejected invocations have the exits described above. `git diff --check`: exit 0.
- `make design-lint`: exit 0, 7.99 s sizing run; `make static`: all seven stages exit 0, wrapper 32.77 s. Static uses a task scratch directory for native temporary files. No `make check` ran because it invokes prohibited Cargo; no ready/merge check or approval log was written. No specification or conformance rules changed.

Validation above ran on working source over parent `41a6fee1c9ddeae319d31aa1feef32e0c9954d2d`; binary and
source hashes identify the tested content. The runtime changes and result prose are committed separately from the
subsequent independent review record. Publication is explicitly out of scope.

### Found during the repair

- Fixed: repeated root bridge allocation/copying; one-slot append growth;
  unconditional collector-helper calls when no trigger holds. Their two
  requested criteria and independent replies/counts select the changes.
- Fixed on recommendation Q1: public root-storage writes could bypass
  invalidation. Root fields are public readonly; inspection remains available
  and embedding operations own writes. The owner ruling is still open.
- Fixed in the existing experiment home: suspended pin/compile/unpin changes
  lacked a lifecycle observation; the probe now covers additions and release.
  A parked-root omission control separates correct rooting from a reply pass
  without collection. A scratch-root option keeps fixture outputs in this
  worktree. Each remains until a maintained test takes over or this experiment
  is retired.
- Removed the resolved resume-copying TODO and updated the P1 TODO/status;
  other hot paths, Cell stride, the every-allocation verifier and compiler
  construction cost remain recorded in `docs/todo.md`. This task does not
  select C1–C6 or claim current profile occupancy for them.

### Review repair criteria

The independent review found that pin followed by compile before resume could
mask either missing invalidation: both operations dirtied the same bridge.
The corrected lifecycle probe requires a collection/survival observation after
pin alone, then after compile alone (while the bridge is valid again), then a
collection/release observation after unpin alone. Before execution, negative
controls are fixed to bypass only the corresponding embedding refresh using
the existing VM resume with its stale constant-pool bridge: pin must exit 59,
compile 69 and unpin 62; ordinary smoke must exit 0. No heap, VM, embedding
implementation, benchmark source or timing binary changes for this repair.
The final e2e host is rebuilt, and the changed CLI's ordinary comparisons and
root controls are rechecked. The interrupted command's reference path is also
redacted to `<reference-root>`; its observed timings and exits are preserved.

The corrected full-LTO e2e host builds with exit 0 in 461.87 s. Ordinary
smoke exits 0; omission of the pin, compile and unpin refreshes independently
fails with exits 59, 69 and 62 respectively, exactly the recorded observations.
The corrected host also passes ordinary 240/240, stress 240/240, local roots
9/9, frame 1/1 and suspended snapshot 1/1; a tool rechecks every row against
before and finds equal outcomes/counts. Memory recovery and the parked-root
omission control retain their earlier expected results. The complete repair
check batch exits 0 in 5.52 s. This supplies the independent invalidator
coverage the initial combined probe lacked.

### Repair independent review

A separate read-only reviewer, configured as GPT-6.1-sol, reviewed
`6cab1f2dcb7fe858846e68758fe210380e7fd7e0..ec69af2c494b5d26ede8cf319e5c73c1d9efc29f`
with A, D, C, R, M and V, including design checks G1–G3 and correspondence
DC1–DC4. T was not triggered; publication was excluded by the user's explicit
constraint. The reviewer reran no green suites. It read the full diff and
owners, independently checked launch ordering, medians/ratios, workload and
tool/binary hashes, all 71 library hashes and e2e source digests, every
case/count comparison and memory observations. It checked the compiler
ancestor and Halo siblings: 114→115 nodes, 550→553 decisions, depth 3 and
526 rejected alternatives unchanged.

Findings fixed: pin and compile mutually masked invalidation in the new
probe (C1/C2, DC4), now separated and each falsified by its own stale-bridge
control; a prospective review-record statement (D3/V2), now replaced by this
actual record; and a machine-local reference path in the interrupted log (A4),
now a role placeholder. The logic repair received the narrow follow-up review below. Q1 remains provisional.

The same read-only reviewer, configured as GPT-6.1-sol, reviewed the repair
`ec69af2c494b5d26ede8cf319e5c73c1d9efc29f..f069b49eba236d68e1f79b5370fd2742e3b98975`
under applicable A, D, C, R, M and V items. It independently verified control
routing, correct and stale-bridge resumes, updated host/source hashes, case
rows/counts, construction and control logs, unchanged timing evidence and
binaries, memory recovery and redaction. Each mutation starts with a valid
bridge and no intervening invalidator; each omitted refresh fails after an
observed collection at its own pin/cached-string/release observation. All
three original findings are resolved; none within narrow scope. No green
suite was rerun and no reviewer edited a file. Full-gate and publication
verification remain outside this explicitly constrained task.

Final `make static` on the reviewed probe repair passes all seven stages,
exit 0 in 33.59 s; `git diff --check` passes after the review record. The
handoff leaves the work branch local and the readonly boundary Q1 open;
no specification change, approval record, PR, push or merge is made.

## C1 bounded per-arm continuation experiment

Criterion fixed before any C1 measurement: keep C1 only if both the numeric
loop and fib improve by at least 10% in six interleaved same-source
before/after pairs (full LTO), and the median of three uncached
`whitefootc --graph lib/halo/modules.wfg --check-modules` runs grows by at
most 1.5 times. Otherwise revert the candidate and retain the observations.
Improvement is `1 - median(after) / median(before)`; individual paired ratios
are also retained to expose drift or spread near 10%. The seven P1 sources
remain identical within each pair; binary-trees uses the previously sized
depth 14. The oracle must pass under budgets 1, 7 and 1000, both normally
and with GC stress. The experiment changes only Halo dispatch/handler source,
not the compiler, language, or oracle.

A single accepted module-check run sizes each side before the remaining two
runs. One pair and then three pairs size each kernel before the selected
six. Failed formation or proof attempts are reported separately and do not
enter the check-cost statistic. Each heavy command holds the host-wide lock
separately; exit 75 waits and retries that same command. C1 generated outputs
and compiler temporary files live in this experiment's existing ignored
`target/`; retained observations serve reproduction and are retired when
this comparison no longer needs reproduction.

### Checked source boundary

The retained P1 leaf counts selected 18 arms: Move, LoadK, GetUpval,
GetTableR/K, SetTableRR/KR, AddRR/RK, SubRR/RK, MulKR, ModRK, EqJmpRK,
LtJmpRK/KR, LeJmpRR and ForLoop. GetUpval is prominent in fib; MulKR and
SubRR occur in binary-trees; ModRK occurs in string-key, concat and sort.
Unselected siblings, Call and Return retain the joined Step epilogue.
The latter two change frame bases, so this unchanged-base trial leaves
`prepare`, `enter_lua` and `finish` unchanged.

Move, LoadK, GetUpval and ForLoop return `Result<u64, Step>`; success carries
the next pc and checked stack-window postconditions. The 14 handlers with
callback slow paths get separate callback-free variants returning
`Result<FastCursor, Step>`. A cursor contains a pc and a Bool saying whether
the instruction was handled. A fast miss returns the current pc with that
flag false before mutation, and the arm invokes the unchanged full handler.
Success tail-calls directly from the arm; errors and suspension forward the
original Step to the shared epilogue. Table-write failures unwind once, as
before. Numeric comparisons only read the stack and preserve its window
fact directly; slot-writing variants publish it. The read-only constant
window keeps its caller fact and needs no new postcondition.

This boundary follows [FN-9](../../../spec/kernel-spec.md): a full handler's
callback can re-enter `run`, so its postconditions are unavailable within
that recursive component. The callback-free variants are outside that
component and their checked summaries reach the arms. Putting entire
arithmetic/table handlers inside `run` would instead expand its proof body;
small leaf variants keep operation proofs separate from dispatch control.
The experiment does not change the recursive-summary rule. P1 already
showed native per-arm dispatch in the joined source, so this trial tests
window checks and continuation traffic, rather than creating that native
split. Native layout and other traffic can also change.

### Check and kernel observations

The requested graph-check command checks the whole selected package graph;
these are its process wall times, not isolated vm-stage timings. Its only
changed inputs are the two VM implementation files. Waiting for the lock
is excluded. All six admitted checks exited 0.

| Check | Before s | After s |
|---|---:|---:|
| Run 1 | 216.28 | 241.00 |
| Run 2 | 210.07 | 241.95 |
| Run 3 | 206.35 | 239.36 |
| Median | 210.07 | 241.00 |

After/before is **1.147**: the 1.5-times check-cost criterion passes.

The first fib candidate launch took 0.573571 s versus 0.228337 s before;
this cold launch is calibration. The three warm-pair fib ratio was 0.8764,
with before/after relative ranges 2.52%/0.38%; loop's ratio was 0.4542,
with ranges 0.81%/0.23%. Every other kernel also received one pair and three
pairs before selection. The calibration and final raw launches, source
hashes, native/compiler/reference hashes, exits, checksums and spreads are
in [c1-measurements.json](c1-measurements.json).

Six alternating Before/After then After/Before pairs use identical stdin
bytes and full-LTO binaries. Each kernel also gets an independent PUC
checksum launch. Both native collection counts agree in every pair, every
native/reference exit is 0, and all unlimited-budget suspension counts are
zero. Both VMs use the normal collector and the benchmark's 2 GiB limit.

| Kernel | Before median s | After median s | Improvement | Before min–max s | After min–max s |
|---|---:|---:|---:|---|---|
| fib | 0.220752 | 0.193442 | 12.37% | 0.218624–0.221286 | 0.193185–0.201287 |
| loop | 1.244630 | 0.568529 | 54.32% | 1.238197–1.253410 | 0.566335–0.570564 |
| integer-table | 0.800244 | 0.754434 | 5.72% | 0.792646–0.808324 | 0.752455–0.758918 |
| string-key | 0.059308 | 0.039479 | 33.43% | 0.059137–0.061065 | 0.039038–0.039885 |
| concat | 0.214733 | 0.182682 | 14.93% | 0.211864–0.217162 | 0.174099–0.186722 |
| sort | 0.752426 | 0.734345 | 2.40% | 0.750257–0.782200 | 0.729774–0.740241 |
| binary-trees | 2.481321 | 2.448951 | 1.30% | 2.477680–2.490589 | 2.437143–2.452813 |

Both numeric median criteria pass. Fib's individual paired ratios range
from 0.8734 to 0.9123: five of six pairs clear 10%, while one improves only
8.77%. The agreed statistic is the ratio of medians, and both the three-
and six-pair results put that gain near 12.4%. The loop's six paired ratios
range from 0.4552 to 0.4592. Other kernel gains are observations, not
additional selection thresholds. This does not measure a new PUC-relative
P1 baseline, isolate window-check cost from layout/other continuation
traffic, or qualify other hosts, compilers, budgets or binary-trees depth 16.

### Outcome and validation

**Kept.** The fixed numeric and check-cost criteria pass, and both oracle
modes pass 240/240 comparisons: 80 unchanged scripts at each of budgets
1, 7 and 1000. GC stress performed at least one collection in every row.
No source or expected reply in the oracle changed. The normal and stress
samples each passed 3/3 before the full runs. The native source revision is
`bcb2ab4f35bf112471521875dea9c02aaafbdaae`; only `dispatch.wf` and
`handlers.wf` differ from the before library. The existing before benchmark
binary was built at `ec69af2c4` (full identity in the JSON), and a tool
comparison verified its Halo, benchmark host/graph, compiler, standard
library, JSON and MessagePack sources equal the before worktree. Its build
log records full LTO. Compiler hash stayed
`c71614ecb1da4ab8b5c4cfea7bec7afa3a39aeb967657e1aaa2019c76dbf3fc5`.

Every heavy command below was a separate
`perl .github/run-check.pl <label> <command>` invocation. Full-LTO builds
set TMPDIR to the existing benchmark target directory. Busy-lock attempts
returned 75 and retried the same command; none entered an admitted timing.
The host was the same M1 Pro/macOS environment as P1; the lock excluded
other cooperating heavy commands, not background OS activity.

| Command stage | Wall s | Exit | Observation |
|---|---:|---:|---|
| check-before-1 | 216.28 | 0 | whole graph accepted |
| check-before-2 | 210.07 | 0 | whole graph accepted |
| check-before-3 | 206.35 | 0 | whole graph accepted |
| check-after-1 | 241.00 | 0 | whole graph accepted |
| check-after-2 | 241.95 | 0 | whole graph accepted |
| check-after-3 | 239.36 | 0 | whole graph accepted |
| bench-build | 543.17 | 0 | full LTO, entry bench |
| fib-one | 1.03 | 0 | 1 cold sizing pair |
| others-one | 12.61 | 0 | 1 sizing pair per remaining kernel |
| three | 33.83 | 0 | 3 warm interleaved pairs per kernel |
| six | 66.84 | 0 | 6 selected interleaved pairs per kernel |
| e2e-build | 535.89 | 0 | full LTO, entry test |
| oracle-sample | 0.39 | 0 | 3/3 normal |
| stress-sample | 0.09 | 0 | 3/3 stress |
| oracle | 0.98 | 0 | 240/240 normal |
| stress | 1.05 | 0 | 240/240 stress |
| design-lint | 7.67 | 0 | smallest static sizing sample |
| static | 33.62 | 0 | every component within its macOS budget |

The JSON retains exact command arguments with `<PUC_LUA>` as the supplied
local Redis 7.0.15 Lua path. Reproduction commands use:

```sh
perl .github/run-check.pl halo-c1-check compiler/target/gate/whitefootc --graph lib/halo/modules.wfg --check-modules
perl .github/run-check.pl halo-c1-bench-build compiler/target/gate/whitefootc --graph research/experiments/halo-bench/modules.wfg --entry bench --full-lto -o research/experiments/halo-bench/target/halo-c1
perl .github/run-check.pl halo-c1-six python3 -B research/experiments/halo-bench/run.py --lua /path/to/redis/deps/lua/src/lua --before-binary research/experiments/halo-bench/target/halo-after --binary research/experiments/halo-bench/target/halo-c1 --kernels fib,loop,integer-table,string-key,concat,sort,binary-trees --scale binary-trees=14 --runs 6 --out research/experiments/halo-bench/target/c1-six.json
perl .github/run-check.pl halo-c1-e2e-build compiler/target/gate/whitefootc --graph research/experiments/halo-e2e/modules.wfg --entry test --full-lto -o research/experiments/halo-bench/target/c1-e2e
perl .github/run-check.pl halo-c1-oracle python3 -B research/experiments/halo-e2e/run.py --compiler compiler/target/gate/whitefootc --binary research/experiments/halo-bench/target/c1-e2e --full-lto --scratch-root research/experiments/halo-bench/target --budgets 1,7,1000 --report research/experiments/halo-bench/target/c1-oracle.md
```

Add `--gc-stress` and use a separate report for the stress run. Substitute
the supplied Lua path for `<PUC_LUA>`; set TMPDIR to the worktree's existing
target directory for builds. The before binary must be rebuilt from its
recorded source if absent, not substituted with an older pre-repair binary.

Five excluded check attempts exited 1: FN-9 postcondition formation
(6.33 s), FN-8 unavailable recursive summary (381.27 s), FORM-4 comment
(2.59 s), GRAM-9 nested Bool construction (3.48 s), and EFF-2 unused
constant reads (6.31 s). The accepted boundary above resolves all five;
formation fixes add no language rule. `make design-lint` and `make static`
passed on the measured implementation and the accompanying evidence/tree
edits. The latter checked repository invariants, archives, translation,
prose, guidance, source size and tree form. `make check` and CI were not run:
the task prohibits Cargo and network. No Cargo, network, PR or push command
was issued for C1. A concurrent process committed and pushed the
pre-existing DESIGN.md edit after the criterion milestone; the C1
implementation commits remained local.

Found along the way: the C1 row's single-native-dispatch premise was stale
against P1 disassembly and is corrected; callback-free summaries are
required by FN-9, not a checker defect. Duplicate fast/full operation logic
and repeated work on fast misses are recorded in docs/todo.md for a later
measured factoring. No specification or conformance rule changed. The
new design node is provisional; no approval log or readiness action is
part of this local experiment.


Independent read-only review covered
`7b410745caba81d0c0c6df187198f7e8576fbf99..d282a76c8714fc24075d68fe098d185db897c41b`,
with requested model `gpt-6-sol`, excluding the unrelated pre-existing
DESIGN.md edit. It checked groups A, D, C, R, M and V; T was not triggered,
and publication/PR checks were outside the task. For M1 it applied G1–G3
and DC1–DC4 to the relevant compiler/language ancestors and the new node.
It read the complete C1 diff and surrounding dispatch, handlers, continuation
checks, results, raw measurements and reports; `git diff --check` passed.
It independently recomputed the medians, checked alternating launch order,
oracle counts and hashes, and inspected the static logs without rerunning
green suites. Findings: **none within scope**, so no finding required a fix.
The implementing agent also rechecked the raw medians, 80-by-three oracle
rows, after source hashes, native/compiler/report hashes and source equality
at handoff. Only this review/result record changed after the reviewed
revision; the measured implementation remains unchanged. Full `make check`
and CI are unverified as stated above.

## C2/C3 profile-directed bounded experiment

Criteria fixed before measurement against the C1 source at
`dc39d44d9811d65f92fc4262c7b7f5f8359a1812`: C2 is kept only if both fib(30)
and loop(100,000,000) improve by at least 10%; C3 is kept only if each
additional pinned local improves both numeric kernels by at least 5%.
Improvement is `1 - median(after) / median(before)` from six alternating
same-source full-LTO pairs. Measure all seven existing kernels (binary-trees
depth 14), preserving independent PUC checksums and native collection counts.
The median of three uncached `--graph lib/halo/modules.wfg --check-modules`
runs may be at most 1.25 times C1's 241 s median (301.25 s). This is whole
graph wall time, not an isolated vm stage. Size each command with one run
and inspect three-run spread before selecting six pairs.

First re-profile fib and integer-table on the C1 binary, with one sizing
profile then three profiles, using the previously sized fib(34) attribution
count. Profiles choose which candidate to try first; if frame work or table
access is indicated instead, record that alternative's criterion before
implementing or measuring it. Profile occupancy alone does not select a
change. Stack slots remain authoritative at every safepoint and slow path.
The unchanged oracle must pass all 240 comparisons at budgets 1, 7 and
1000 in ordinary and GC-stress modes, and the local halo-gc witnesses and
root omission controls must remain discriminating. No compiler, specification,
or expected reply changes are in scope.

Structural assessment: use the existing C1 callback-free boundary for any
hot-path trial, preserving shared slow execution and collector enumeration.
Assess a profile-selected frame/table alternative in its current owner
before implementation. Generated binaries, profiles and logs belong in the
existing ignored benchmark `target/`; retained observations belong in this
experiment and remain only while the comparison needs reproduction. Work
stays local: no network, Cargo, PR or push.
