# Cold build speed of Snowghost's `--par` layout_oracle

## Question

A cold build of Snowghost's `--par` `layout_oracle`, the executable
Snowghost-wf's `make binary-size-check` builds, takes about 317 s on a
GitHub-hosted 4-vCPU runner. Snowghost needs it at or below 120 s, the median
of three cold runs on the same runner class. Where does the time go, why does
the build use one processor out of four, and which compiler changes would
reach 120 s without changing a verdict, a diagnostic, a cache key's meaning
or the emitted program?

This record holds the analysis and a plan. No compiler code changes here.
It continues [Cold compile speed](../compile-speed/DESIGN.md), which made the
entry *check* concurrent and cut its proof work. That work did not reach the
build path, and it did not measure lowering, emission, clang or the link.

## Data

Workload: Snowghost-wf `466e9843`, `renderer/modules.wfg`, built with the
compiler release Snowghost pins, `wf-d837678c6905` (main commit `d837678c6`,
the `gate` Cargo profile). The command is the Makefile rule for
`$(ENGINE_BINARY)` plus `--report`:

```sh
whitefootc --cache build/whitefoot-cache --fragments function --report \
  --par --graph modules.wfg --entry layout_oracle -o build/layout_oracle_par
```

Host: GitHub-hosted `ubuntu-24.04`, AMD EPYC 9V45, 4 vCPU, 16 GB, clang and
ld.lld 22.1.8. The run is
[38335866896](https://github.com/Ming-Research/Whitefoot/actions/runs/38335866896)
of the temporary workflow `sg-cold-build.yml` on branch
`claude/sg-cold-build`. The cache directory was emptied before each cold run.

| Run | wall s | user s | sys s | front end s | split s | compile s | link s | peak RSS |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| cold 1 | 311.5 | 315.5 | 39.6 | 190.0 | 0.10 | 90.4 | 29.4 | 4.70 GB |
| cold 2 | 316.0 | 320.3 | 40.1 | 191.5 | 0.10 | 92.5 | 30.1 | 4.71 GB |
| cold 3 | 323.3 | 328.2 | 40.5 | 194.6 | 0.10 | 96.4 | 30.5 | 4.71 GB |
| warm | 1.55 | 0.59 | 1.01 | 0.12 | 0.18 | 0.51 | 0.28 | 0.85 GB |

The phases are the `--report` fields: *front end* is parsing, module
verdicts, the entry's composition check, proofs, lowering and LLVM emission;
*compile* is clang on 5,400 fragments plus 15 runtime units (5,415 objects);
*link* is the ThinLTO link through clang and ld.lld. Each cold run reported
913 analyses reused and 3,324 recorded.

Two further cold runs, without a cache:

| Command | wall s | user s | sys s | peak RSS |
|---|---:|---:|---:|---:|
| `--graph modules.wfg --check-modules` | 270.3 | 296.6 | 2.4 | 4.77 GB |
| `--par --graph modules.wfg --entry layout_oracle --emit-llvm` | 199.0 | 214.3 | 1.5 | 4.68 GB |

Size of the input and output, counted at Snowghost-wf `466e9843` and from the
`--emit-llvm` artifact of the same run:

- `layout_oracle`'s closure is 21 program modules and 5 standard-library
  modules (`std::fs`, `std::io`, `std::process`, `std::text`, `std::time`).
  The program modules hold 91,482 lines in their `.wf` and `.wfm` files and
  2,376 `fn` definitions by a line-start regex count. The largest are
  `pkg::layout` (21,069 lines), `pkg::style` (15,879),
  `pkg::html::tree_builder` (10,011), `pkg::css::values` (9,257),
  `pkg::css::selectors` (8,605) and `pkg::font` (7,459). The whole renderer
  is 280 `.wf` files, 126,726 lines, in 52 program modules and 26 entries.
- The emitted module is 888,494 lines and 44.6 MB of LLVM IR with 4,734
  function definitions; the fragment split makes 5,400 fragments of it. The
  executable is 4.48 MB.

## Analysis

### The whole build runs on one processor

User time is within 1.5% of wall time in every cold run. The only places the
compiler runs work concurrently are two calls of `in_parallel`
(`compiler/src/lib.rs:57`), which takes one worker per further available
processor:

- the checker's analyses of one postcondition level
  (`compiler/src/semantic/check/schedule.rs:99` and `:266`);
- the module verdicts of an entry *check*, `entry_verdict`
  (`compiler/src/driver.rs:1820`, main `:1822`).

A build does not take the second path. Every other stage is a plain loop on
the driver thread. Line numbers are at `d837678c6`; main at `58743a931` has
the same code, shifted by the line given in parentheses where it differs.

1. **Module verdicts in a build run one after another.**
   `build_module_entry_for_emission` calls `require_module_verdicts`, whose
   `for module in modules` loop (`compiler/src/driver.rs:1608`, main `:1610`)
   checks each of the 26 closure modules in turn. The approved
   [incremental-compilation decision](../../../design/compiler/incremental-compilation.md)
   says an entry composition computes its module verdicts concurrently, on
   one thread per processor. The check path does
   (`driver.rs:1820`); the build path does not. This is a gap between an
   approved decision and the code, not a new design question.
2. **The composition check, lowering and emission are one thread.** After
   the verdicts, `compile_selected` (`driver.rs:2463`) parses, resolves and
   checks the whole closure again (`with_checked_program_using`,
   `driver.rs:2726`), then `lower_selected` (`driver.rs:2821`) lowers and
   emits it. Lowering (`compiler/src/lowering/builder.rs:62`) and emission
   (`emit_module`, its function loop at `compiler/src/backend/emitter.rs:288`)
   walk all functions in order and share mutable module state: the intrinsic
   set, the parallel-thunk table and the linked-declaration map. Only the
   proof analyses inside the check use the other processors.
3. **Under `--par` the composition reuses no proof receipts.**
   `receipts_for` (`driver.rs:548`-`553`) hands the composition the cache's
   receipts only when overlap lowering is off, because a receipt does not
   retain the entailment detail the permission table reads. With `--par`,
   every function body the module verdicts analyzed is analyzed again in the
   composition. This is why the cold build with a cache (front end 192.0 s)
   is barely faster than `--emit-llvm` without one (199.0 s): the cache
   cannot help the `--par` composition. The 913 reused analyses then can
   only be module verdicts reusing receipts that earlier verdicts of the
   same build recorded; this is read from the code, and E1's build without
   `--par` checks it.
4. **clang runs on one fragment at a time.** `link_cached_objects`
   (`compiler/src/bin/whitefootc.rs:1021`) loops over the runtime units
   (`:1047`) and then the 5,400 fragments (`:1078`). For each one
   `cached_object` (`:1196`) starts `/usr/bin/clang -pthread -flto=thin
   -Wno-override-module -x ir fragment-N.ll -O2 -falign-functions=64 -c -o
   program-N.o` and waits for it (`:1231`) before starting the next. No job
   limit exists because there is no concurrency to limit. 93.1 s over 5,415
   objects is 17.2 ms per clang process, most of it probably process start
   and LLVM setup rather than optimization, since a fragment holds about one
   function. The 40 s of system time, against 1.5 to 2.4 s in the runs
   without clang, also points at 5,415 process starts.
5. **The ThinLTO link takes lld's default job count.** The link command
   (`whitefootc.rs:1115`-`1136`) passes `-fuse-ld=lld -flto=thin
   -Wl,--thinlto-cache-dir=...` and no `--thinlto-jobs`. lld's default is
   LLVM's "heavyweight" concurrency, the number of physical cores. On a
   4-vCPU runner with two hardware threads per core that is 2 jobs; the
   measurement plan records `lscpu` to settle it. Across the whole build,
   user plus system time exceeds wall time by 44 s, which bounds all the
   overlap the build has, the link's included.

So the answer to "does the compiler already use all processors in this
step" is no: only proof analyses inside one check do, and a build spends most
of its time elsewhere.

### Why `--check-modules` is slower than `--emit-llvm`

The two commands do different work, so the comparison says nothing about a
missing reuse inside one check:

- `--check-modules` (`whitefootc.rs:424`-`458`) checks all 52 program
  modules of the graph, 31 of which `layout_oracle` does not read, one after
  another, and then the composition of each of the 26 entries.
- Without `--cache`, each `entry_verdict` takes the module verdicts already
  computed (`known`) but checks its whole composition again with no receipts
  (`driver.rs:1835`-`1844`), so every body of every entry's closure is
  analyzed once per entry that reads it, in addition to its module verdict.
- `--emit-llvm` with `--entry layout_oracle` checks only that closure's 26
  modules and one composition, then lowers and emits it.

`--check-modules` therefore did 52 module checks and 26 compositions in
270 s on mostly one thread; the build did 26 module checks, one composition
and the lowering and emission in about 192 s. With a cache, the
compositions would reuse the module verdicts' receipts (their overlap
lowering is off) and the command would be much cheaper; Snowghost's
`--check-modules` call without `--cache` is a Snowghost-side choice.

### What the front end's 192 s is made of

Not known yet, and no released flag splits it. Write the front end as

    front end = M + C + L

where M is the serial module verdicts, C the composition's parse,
resolution, check and proofs, and L lowering, dispatch layout and emission.
What the data does bound:

- The fresh-cache *check* of `layout_oracle` took 76.7 s on one processor
  and 34.3 s on four in the
  [compile-speed results](../compile-speed/DESIGN.md#results) (Snowghost
  `29f89af`, an older compiler, a slower 2.1 GHz Xeon). That check ran
  M + C with receipts in C. The closure has grown by 8% since (84,579 to
  91,482 lines), and the compiler has gained proof reductions and features
  since. If today's M + C with receipts is still near 77 s on one
  processor, then C without receipts plus L make up most of the remaining
  115 s.
- `--check-modules`, which includes every module verdict of the closure
  plus 26 more verdicts and 26 compositions, took 270 s. The closure's 26
  verdicts are therefore well under 270 s.

The hypotheses the measurement plan decides:

- **H1**: M is at least 20% of the front end (38 s).
- **H2**: L is at least 30% of the front end (58 s).
- **H3**: re-analysis in C under `--par` (the work receipts would remove) is
  at least 15% of the front end (29 s).

### Instrumentation that exists

- `--report` prints the four phases and the analysis counts above; it does
  not split the front end.
- `WHITEFOOT_CHECK_WORK=<file>` (`compiler/src/semantic/entailment/work.rs`)
  appends per-analysis work counters (joins, L0 pairs, interning calls,
  affine-index work) by function name, in the release too. It records no
  time: it says which Whitefoot functions carry proof work, not how long it
  took.
- `WHITEFOOT_TEST_TIMINGS` times the shared test helpers only; it is not in
  the `whitefootc` binary's path.
- Flags that split the work by running it separately: `--check-module M`
  (one module verdict), `--check --entry E` (verdicts plus composition, no
  lowering), `--emit-llvm` (front end without clang, which `--fragments`
  refuses, so it emits without the fragment dispatch mode).
- `taskset -c 0` pins a run to one processor; `in_parallel` reads
  `available_parallelism`, which follows the affinity mask on Linux, so the
  pinned run is fully serial.
- `perf` on a build of `d837678c6` with frame pointers and line tables
  attributes samples to stage functions by call stack.

Per-Whitefoot-function *time* needs a small temporary patch: an `elapsed_ns`
metric written beside the counters `work.rs` already emits per analysis run
(start an `Instant` in `function()`, write it in `FunctionScope::drop`). A
lasting improvement would be for `--report` to split `front_end_ms` into
module verdicts, composition check, lowering and emission; it is four timers
in `build_module_entry_for_emission` and `lower_selected`.

## Proposals

Ordered by expected gain per effort. Each prediction states its arithmetic
and the measurement that replaces its assumption. *k* is the speedup four
workers give on this runner's 4 vCPUs; with two hardware threads per core
it is likely 2.5 to 3.3, written k = 3 below until E4 measures it.

### F1. Compile fragments concurrently

Run the fragment and runtime-unit compiles as a pool of
`available_parallelism` clang processes in `link_cached_objects`, collect
the objects by index, and keep the link's object order.

- Predicted: compile 93.1 s → 93.1 / k ≈ 31 s, saving about 62 s.
  E4's `p4` replay replaces the assumption.
- Correctness: each object depends only on its own input, arguments and
  host compiler, so object bytes, cache keys and the link's object order are
  unchanged. A failing clang must be reported as the first failure in
  fragment order, as the sequential loop reports it. Cache publication is
  per object and already tolerates failure.
- Effort: small, in `compiler/src/bin/whitefootc.rs` only. Not the checker.

### F2. Compile many fragments per clang process

Pass a batch of fragments to one clang call (`clang ... -c -x ir
fragment-1.ll ... fragment-100.ll`), which writes each object beside its
input and runs every compile in one process. The cache stays per fragment.

- Predicted: if one clang process costs *f* ms fixed (E4's `tiny-x200`
  replay measures it), batching 100 per process removes 5,415 × f × 0.99 ms
  of CPU; at f = 10 ms that is 54 s of CPU, about 18 s of wall at k = 3 on
  top of F1. The 40 s of system time should fall with it.
- Correctness: requires the batched objects to be byte-identical to
  single-call objects (E4 compares all 5,400). If they are, the cache key
  stays as it is; if not, the key's argument text must change and the
  difference must be explained.
- Effort: small, same file; builds on F1.

### F3. Give the ThinLTO link every processor

Pass `-Wl,--thinlto-jobs=all` (and the equivalent on macOS and Windows)
explicitly.

- Predicted: if lld runs 2 jobs today, link 30.0 s → between 30 s (the link
  is bound by its serial thin-link step) and about 20 s. E4's link replay
  with default, `all` and 1 job decides it, and `--time-trace` splits the
  link into its serial and parallel parts.
- Correctness: ThinLTO backends are independent per module and their output
  does not depend on the job count; E4 compares the executables byte for
  byte.
- Effort: one argument. Not the checker.

### F4. Module verdicts of a build concurrently

Give `require_module_verdicts` the same `in_parallel` call `entry_verdict`
already uses, with the same rule: every module is checked, and the first
rejection or failure in module order is reported.

- Predicted: saves M minus the longest single module verdict, since four
  workers cannot finish before the slowest module (earlier measurements put
  `pkg::style` and `pkg::html::tree_builder` on that critical path). With
  E1's per-module times *t_i*: saving ≈ Σ t_i − max(Σ t_i / k, max t_i).
- Correctness: this is the approved decision's behavior, already in the
  check path, so verdicts and reported failures follow its rule. Peak memory
  rises because module checks are held at once: the compile-speed record saw
  the check's peak go from 2.9 to 4.1 GB; today's build peaks at 4.7 GB of
  16 GB. Receipts recorded concurrently are content-keyed, so the
  composition reads the same set; `analyses_recorded` may differ slightly
  between runs if two modules record the same analysis at once, which
  changes a count, not an output.
- Effort: small, `compiler/src/driver.rs`. It is driver code, not the
  checker, but the checker line should know because it shares the worker
  budget with the analyses.

### F5. Receipts the `--par` composition can use

Make a proof receipt retain what the permission table reads, or compute
the permission table from what receipts already hold, so that
`receipts_for` can hand receipts to an overlap lowering.

- Predicted: removes the composition's re-analysis of every body the module
  verdicts analyzed. E1 bounds it: front end of the build without `--par`
  (`build-nopar`, receipts used) against with `--par`, with the lowering
  difference read from the profile. If H3 holds, at least 29 s of CPU; on
  the wall clock less, since level analyses already run concurrently.
- Correctness: this changes what a receipt certifies, so the
  [MOD-8] receipt rule and the cache key's meaning are at stake; the
  permission ledger of a `--par` build with and without receipts must be
  identical line for line, and the LLVM byte-identical.
- Effort: medium to large, in `compiler/src/semantic` and the receipt
  format. **This touches the checker, which the checker line owns; it needs
  coordination and probably a design card.**

### F6. Lower and emit functions concurrently

Lower and emit each function into its own buffer on the shared worker
budget, then merge in function order: intrinsics as a set union, thunk
names numbered after the merge in function order, linked declarations
deduplicated in order.

- Predicted: L × (1 − 1/k); at L = 60 s and k = 3, 40 s. E2 measures L.
- Correctness: output must stay byte-identical, which requires every name
  or number a function's emission allocates from shared state to be
  assigned in function order, not in completion order. Diagnostics of a
  lowering failure must stay the first in function order. The
  [structured-emission](../../../design/compiler/structured-emission.md)
  decision is the place to record it.
- Effort: large, `compiler/src/lowering` and `compiler/src/backend/emitter`.
  Not the checker.

### F7. Ship the compiler without debug assertions

Releases use the `gate` profile, with debug assertions and overflow checks
on ([downstream releases](../../../design/compiler/downstream-releases.md)).
Some debug-only work sits in proof flow: with debug assertions,
`record_continuing` keeps every kill applied inside a loop and the back-edge
check scans the loop summary for each one
(`compiler/src/semantic/entailment/flow/events.rs:1493`,
`flow/loop_summary.rs:297`).

- Predicted: unknown; E6 measures the `release` profile against `gate` on
  the same commit and runner.
- Correctness: the shipped compiler would no longer stop on those internal
  checks or on arithmetic overflow, so a compiler defect they catch would go
  unnoticed. It reverses part of an approved decision: **an owner decision,
  not a fix.** Only worth a card if E6 shows at least 10%.

### F8. Overlap clang with emission

Start compiling a function's fragment while later functions are emitted.
It needs emission to produce fragments as it goes rather than one module
split afterward, so it follows F6. Saves up to min(L, compile time after
F1/F2). Not proposed before F1 to F6 are measured.

### Can 120 s be reached

The cold build spends 361 s of CPU (user 321 s plus system 40 s, mean of
three runs). Even with perfect parallelism, four vCPUs at k = 3 need
361 / 3 ≈ 120 s, so 120 s needs both less CPU and much more concurrency:

- F1 + F3 (backend, small): 316.9 − 62 − 5 ≈ 250 s.
- + F2: compile falls from about 31 s toward 13 s: ≈ 232 s.
- + F4: Σ t_i is unmeasured; if M = 50 s and the slowest module takes 15 s,
  the saving is 50 − 17 = 33 s: ≈ 199 s.
- + F5 and F6: with C's re-analysis at 30 s CPU and L = 60 s, ≈ 199 − 20 − 40
  ≈ 139 s.

These numbers rest on the assumed values of k, f, M, the slowest module,
the re-analysis share and L, which E1, E2 and E4 replace. On those
assumptions, the driver-only fixes F1 to F4 reach about 200 s; 120 s also
needs F5 and F6, and possibly F7 or F8. E1 to E6 decide the order of the
larger fixes before any of them starts.

### What else was found

- `report.compile` subtracts `report.split` from a timer that starts after
  the split (`whitefootc.rs:1106`), so `compile_ms` understates the compile
  by `split_ms` (0.1 s cold, 0.18 s warm): small, fix with F1.
- Snowghost's `--check-modules` gate step runs without `--cache`, so each of
  its 26 compositions analyzes its closure again (see above): a Snowghost
  choice to raise with that project, not a compiler defect.

## Measurement plan

Workflow `.github/workflows/sg-cold-attrib.yml` on branch
`claude/sg-cold-build`, with the stack roll-up `.github/sg-cold-phases.py`
and the cold-build comparison reporter `.github/sg-cold-compare.py`.
All three are temporary and are removed with the branch. The existing
attribution jobs use release
`wf-d837678c6905`, and commit `d837678c6` built on the runner in three
ways: the `gate` profile, the plain `release` profile, and the `gate`
profile with frame pointers and line tables for profiling. Snowghost-wf is
`466e9843`. Single runs, except E6; the three cold runs above spread by
3.8% (311.5 to 323.3 s), so a difference under 10% from one run is not
evidence.

Job `frontend`:

- **E1, decomposition by flags.** Each of the 26 closure modules with
  `--check-module` (no cache), giving *t_i*; `--check --entry
  layout_oracle` with a fresh cache, pinned with `taskset -c 0` and on all
  four processors; the full build pinned to one processor with `--report`;
  the full build without `--par`. Reads: Σ t_i against the pinned check
  (M + C with receipts); pinned build `front_end_ms` minus pinned check
  ≈ L plus C's re-analysis; with and without `--par` ≈ C's re-analysis plus
  the lowering difference.
- **E2, profile.** `perf record -F 199 -g --call-graph fp` of the cold build
  with the profiling compiler. The roll-up assigns each sample to the
  outermost stage function on its stack (parse, resolve, check, lowering,
  dispatch layout, emission, split, objects and link), marks samples under
  `analyze_function*` as proof, separates module verdicts from the entry
  composition by `require_module_verdicts` on the stack, and counts
  samples per thread and per command (whitefootc, clang, ld.lld).
- **E3, proof work by function.** `WHITEFOOT_CHECK_WORK` over the cold
  build; the top 40 Whitefoot functions by analysis runs, joins, interning
  calls, affine attempts and evaluated L0 pairs.
- **E6, build profile.** `--emit-llvm --par`, no cache, `gate` and `release`
  profiles interleaved three times each; the two LLVM outputs compared.

Job `backend`:

- **E4, capture and replay.** One cold build with a logging wrapper in place
  of `/usr/bin/clang` that keeps each fragment and every link object (its
  own cost makes that build's times unusable). Then, without the wrapper:
  the 5,400 fragment compiles serially (twice, as a twin control), with
  `xargs -P2`, `-P4`, and 100 fragments per call at 4 jobs, with batched and
  single-call objects compared byte for byte; 200 compiles of a
  one-function module for the fixed per-process cost; the link with the
  default job count (twice), `--thinlto-jobs=all`, `=1`, and a
  `--time-trace`, each with an empty ThinLTO cache and the executables
  compared.
- **E5, external references on the same runner.** `clang -O2 -c` of the
  SQLite 3.50.4 amalgamation, one translation unit of about a quarter of a
  million lines with comments; and `cargo build --release -j 4` of ripgrep
  14.1.1 with its dependencies, with the lines of every Rust source the
  build compiled counted from its dep-info files. No credible published
  times for either on this runner class were found, so both are measured.
  They place 91,482 lines of Whitefoot at 317 s against a C compiler on one
  file and a Rust build using four processors; the comparison is about
  orders of magnitude, since the languages' work per line differs.

Job `comparison`, added for the implemented fragment pool and ThinLTO job
limit:

- Resolve the base as the branch head's merge base with fetched `main`,
  record both exact compiler revisions, and build both from source with
  the same Rust toolchain, `gate` profile, LLVM major and four available
  processors on one hosted `ubuntu-24.04` runner. The twin is a copy of the
  base compiler binary with the same digest. Compiler construction is not
  part of the measured Snowghost build.
- Build the same pinned Snowghost input with `--par`,
  `--fragments function`, `--report` and a new empty compiler cache for
  every sample. This also gives each sample a new ThinLTO cache; operating
  system file caches are not flushed. The output pathname stays fixed.
  Start with two rounds: base, twin, head; then head, twin, base. Print
  each completed sample and the first round before collecting the second.
- Report wall/user/system time, peak RSS, every build phase and reuse
  count, and executable hashes. Retain the raw reported phases. When the
  selected revision has the known compile timer that subtracts split time
  from a timer started after splitting, add that split time back for the
  comparison and record the correction explicitly.
- Print the sample count, minimum, median, maximum and range divided by
  median for every variant and phase. Screen F1 using the head's compile
  phase at most 50% of both base medians, with base/twin agreement within
  10% for each pair and across all four control repeats; screen F3 using
  the head's link phase at most 85% of both base
  medians and identical executable bytes. Failed builds or missing phase
  data are invalid measurements, and unstable twin controls or unequal
  base/twin images make attribution inconclusive rather than a claimed
  saving. These full-build compile phases include runtime objects; the
  existing E4 replays remain the isolated fragment and link comparisons
  named by the prior criteria below.
- Two rounds are an initial screen, not the three-run endpoint test or a
  precise timing claim. Read the spread before choosing more rounds. The
  reporter's synthetic controls run in this workflow before compiler
  construction and check missing data, wrong thresholds, unstable twins
  and executable differences; they run no compiler.

The front-end, fragment/link replay and external-reference jobs remain:
they answer E1 to E6 independently of the new same-source comparison. Their
command exit statuses are captured before displaying logs, and top-row
readers consume saved complete output so that `pipefail` cannot mistake an
early reader close for an experiment failure.
The replay reporter prints separate verdicts for the original F1 serial
versus p4 comparison and F3 default versus all-jobs link comparison. Failed
commands, unequal object counts and missing records are invalid data; a
valid batch-byte difference rejects F2 without suppressing the remaining
link and external-reference measurements.

The new experimental stages have no measured wall-time budget yet. This
initial pilot retains the workflow's existing 2,700-second execution
deadline per owned command group; that is an execution limit, not an
invented timing measurement or a claimed verification budget. Use the
first CI samples to establish the labeled-stage budgets under the existing
budget method before turning these experiments into repeated verification.

Prior criteria, fixed before the run:

- H1 holds if Σ t_i is at least 38 s; F4 is then worth doing first among the
  front-end fixes. F4 is done regardless, as the approved decision requires,
  but its predicted saving is Σ t_i − max(Σ t_i / k, max t_i).
- H2 holds if E2 puts at least 30% of whitefootc's samples in lowering,
  dispatch layout and emission; F6 then precedes F5. If under 15%, F6 is
  dropped from the plan.
- H3 holds if the no-`--par` build's front end is at least 29 s shorter
  than the `--par` build's, and E2 puts the difference in the composition's
  proof samples; F5 then goes to the checker line as a request. If the
  difference is under 10 s, F5 is dropped.
- F1 is kept if the `p4` replay takes at most half the serial replay, and
  the two serial replays agree within 10%.
- F2 is kept if the batched replay takes at most 70% of the `p4` replay and
  no batched object differs from its single-call object.
- F3 is kept if the `all` link takes at most 85% of the default and of its
  repeat, and all executables are identical.
  Results for F1 and F3 are pending the owner's attribution comparison;
  correctness CI does not establish either performance criterion, and no
  measured saving is claimed for the implementation.
- F7 goes to the owner as a card only if every `release` run is faster than
  every `gate` run and the median is at least 10% lower, with identical
  LLVM.

The implementation shares one in-process budget for additional workers
beyond existing caller threads. A fragment worker returns its share when
its work ends, and ThinLTO reacquires the free share for the lifetime of its
link process. Independently created root caller threads and separate
compiler processes are not counted by that existing mechanism. If a
consumer needs coordinated concurrent builds, the proposed follow-up is
participation in the caller's [Cargo](https://doc.rust-lang.org/cargo/reference/build-scripts.html#jobserver)
or [Make jobserver](https://www.gnu.org/software/make/manual/html_node/Job-Slots.html):
shared tokens constrain the aggregate work, whereas a fixed per-process
environment limit would require the caller to divide capacity among its
processes. Cross-process coordination belongs in the backlog rather than
this change; it requires a concrete invoking build system, safe token
inheritance and return, and a separate design for independently created
root callers. No cross-process mechanism is implemented here.

Completion, for the fixes once built: the median of three cold runs of
the documented parallel `layout_oracle` build command at or below 120 s on
the hosted 4-vCPU runner,
the same verdicts, an executable identical to the base compiler's (or, where
a fix changes link inputs on purpose, identical LLVM and passing Snowghost
oracles), and each fix's saving attributed with interleaved runs of the base,
a twin of the base and the head.

## Uncertainties

- The front-end split (M, C, L) is inferred, not measured; E1 and E2
  measure it. The older 76.7 s pinned check is from another compiler,
  program revision and processor.
- k, the gain from four vCPUs, depends on whether they are two cores with
  two hardware threads each; E4 records `lscpu` and measures it for clang.
- lld's default ThinLTO job count is taken from LLVM's heavyweight
  concurrency rule (physical cores); the link replay checks it.
- The fixed cost of a clang process is assumed to dominate the 17.2 ms per
  fragment; the one-function replay measures it.
- Batched clang calls are assumed to produce identical objects; E4 checks
  all 5,400.
- Perf attribution relies on stage functions not being inlined into their
  callers in the frame-pointer build; samples left in "other" show how much
  this misses.
- F5's size depends on how much of the composition's proof work the module
  verdicts already did; generic instances analyzed only in the composition
  would stay.
