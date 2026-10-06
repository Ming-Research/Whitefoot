# compute-bench

One question, one table per host:

> **For each kernel, at each width, is the Whitefoot program built by this
> tree's `whitefootc` with plain `--par` the fastest thing in the row?**

The framework scoreboard exists to make that comparison honest. The separate
helper-pricing, runtime-DAG and first-index experiments below use the same
native build support.
**No number in a framework scoreboard table
fails a build or a check**: there is no band, no threshold, no timeout, no
budget and no heuristic anywhere that selects a row, a ranking or a ratio, and
`compare` fails only on a missing or malformed row.

Automatic compiler performance regression is maintained separately in
[`tests/performance`](../../../tests/performance/README.md). This framework
scoreboard is manually invoked; its legacy `verdict` reproduces historical
A/B tables and does not decide a pull-request check.

The narrow [Box-array helper-pricing experiment](../../investigations/compute-model/DESIGN.md#read-only-box-array-helper-work-pricing)
also retains its WF source, small C oracle and ordinary LLVM adapter here.
They are owned by that experiment and are retired together when superseded.
They have no daily gate or framework-scoreboard dependency. Build an image
from a prebuilt compiler with the explicit manual target, using a different
scratch directory for each arm:

```sh
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl array-reference-build \
  make -C research/experiments/compute-bench -j2 array-reference-build \
  WFC=/absolute/path/to/whitefootc WORK=/absolute/scratch/array-reference-arm
```

Run the image separately from construction:

```sh
WHITEFOOT_CHECK_TIMEOUT=10 perl .github/run-check.pl array-reference-oracle \
  env WF_WORKERS=4 /absolute/scratch/array-reference-arm/build/array_reference \
  candidate 1 16384 512
```

The arguments are arm label, pass number, input words and output rows. Each
process checks one warmup and five measured calls against a separate
complete-output oracle and checks the unchanged input after every call.
The TSV fields are arm, workers,
pass, words, rows, sample, wall nanoseconds, process CPU nanoseconds and actual
steals; sample zero is the warmup. Allocation, oracle calculation, checking
and release are outside the interval. The LLVM adapter is needed for the
ordinary range argument ABI and uses the formal host adapter's world selection.
The recorded five-pass rotation, null comparison, medians and selection
criterion are in the investigation. The [retained stream](array-reference-work-2026-09-22.tsv)
adds a leading `comparison` column. Individual tiny calls have unresolved wall
intervals on this host and receive no ratio verdict. The separate resolution
diagnostic builds with `ARRAY_REFERENCE_REPEATS=4096` and runs only `17 2` at
W1/W4. Its
`batched-null-4096` and `batched-matched-4096` rows record interval totals;
divide wall and CPU by 4,096 for per-call observations. The allocations,
inputs, WF code and checks surrounding each interval are unchanged. The
default remains one call per interval, with no LTO or floating-point changes.
The diagnostic reports absolute per-call wall and CPU differences separately;
the recorded CPU interval quantum is 1,000 ns, or 0.244 ns per call after
normalization. Its fixed paired order and results are in the investigation.

The separate [runtime-DAG fan-in trial](../../investigations/compute-model/DESIGN.md#runtime-dag-fan-in-source-trial-2026-09-23)
uses `dag_fanin_probe.cpp`, its LLVM host adapter and `dag_fanin_trace.awk`.
These files serve only the explicit targets below and retire with that trial.
They do not enter the framework scoreboard or daily correctness checks.
The [retained qualification stream](dag-fanin-2026-09-23.tsv) records artifact
identities, complete-matrix checks, selected passive observations and separate
construction/execution phase costs. Those costs are not benchmark intervals.
The separately selected cost mode and `dag_fanin_cost.pl` follow the
[prospective bridge cost protocol](../../investigations/compute-model/DESIGN.md#prospective-cost-qualification-of-the-final-bridge).
They use the same probe/ABI, with one added four-independent-call control;
the frozen earlier correctness and overlap records retain their original
matrix and input identities. The ordinary `dag-fanin-run` target remains an
untimed correctness run.
The separate [CPU-accounting diagnostic result and replay recipe](../../investigations/compute-model/DESIGN.md#baseline-cpu-accounting-result-short-interval-attribution-failure)
use an exact host-only patch retained in the same dated stream. They add no
maintained probe mode or daily check; offline extraction reconstructs its raw
data, reduction and source, while native relinking requires the named frozen
objects and dependency cache.
The probe compares every task output and exactly-once count with a serial
Kahn oracle over the original graph edges, and checks input preservation,
boundary canaries and each notification owner's received source mask/count.
Its oneTBB reference builds `continue_node` edges directly from the same
original graph, independently of the WF decompositions and oracle schedule.
Registering edges supplies the notification thresholds; no constructor
predecessor count is added a second time.

Cost construction uses the separately frozen main/final compiler emissions
and ordinary runtime objects. Current compiler/runtime sources and their
defaults contain restored main behavior; they do not recreate the withdrawn
candidate arms. Historical reproduction requires explicitly selecting the
retained compiler and runtime inputs identified by each arm's recorded
manifests. Set `DAG_PROBE_OBJECT` to the same absolute
scratch object path for both native builds. The probe's plain-only `measure`
mode enters the ordinary WF floor without a oneTBB arena. Its fixed batch
checks final values, cumulative task counts, input preservation and canaries
outside timing; all measured intervals must reach the published 1 ms floor.
`make dag-fanin-cost-check` runs only synthetic instrument checks, with no
compiler build or WF timing. The explicit campaign entry is:

```sh
make -C research/experiments/compute-bench dag-fanin-cost-run \
  DAG_COST_STAGE=verify DAG_COST_BASE=/absolute/main_plain \
  DAG_COST_CANDIDATE=/absolute/final_plain DAG_COST_RESULTS=/absolute/fresh/verify
```

The driver guards each execution phase separately with a 30-second cap; do
not wrap the entire multi-phase campaign in a second phase deadline. After
`verify`, invoke `null` with the baseline image for both paths, then `slow`
with that same image, then `bridge` with baseline/final paths, always using
fresh result directories. The driver prints each phase status and retains
process stdout/stderr, raw samples including labelled warmups, complete
paired rows and the verdict. Stages are explicit so a failed control cannot
silently become a candidate comparison. Follow the published stop rules:
technical/control failure ends all cost work; no retry or batch adjustment
is allowed. A complete but unqualified bridge comparison permits exactly
one separately constructed main-LLVM/DONE-first image, `verify-runtime`
(two candidate correctness processes), and `runtime` comparison. Its cost
protection has no A/D speed requirement. This driver belongs only to this
selected investigation and is not a general benchmark framework or a daily
performance selection entry.

The published bridge cost run stopped at the first null measured batch:
65,536 cheap N0/W1 calls took 333,000 ns wall and 331,000 ns CPU, below the
selected 1 ms interval floor. The four preceding correctness matrices passed
117 cases and 964 task rows each. No null ratio, slowdown control, compiler
comparison or conditional runtime-only comparison followed, and batching
was not adjusted. Both candidates lack cost qualification from that session.
A [separate prospective amendment](../../investigations/compute-model/DESIGN.md#prospective-amendment-one-interval-resolution-correction)
permits exactly one corrected-resolution campaign before any paired or
candidate result, with separate data and unchanged thresholds/stop controls.
It does not relabel the stopped run as a pass. See the
[cost result](../../investigations/compute-model/DESIGN.md#cost-qualification-result-interval-floor-stop)
for exact identities, setup-access refusal, separate phase costs and limits.

The one amended campaign completed all forty identical-image cells with the
larger fixed batches and valid intervals. Its symmetric null failed N12/W4
on CPU ratio 1.2979956932 and spine-8-1/W4 on wall 1.1949915221 and CPU
1.1825007904. The other thirty-eight cells stayed within the bands. No
doubled-work control or candidate/runtime-only comparison followed, and no
further correction or rerun is selected. The final disposition withdraws both
production candidates for lack of cost qualification; it does not claim an
implementation regression or identify the cause of identical-input
variation. The [amended result](../../investigations/compute-model/DESIGN.md#amended-cost-result-identical-image-control-failure)
retains all forty rows, raw identities and separate construction/execution
totals. Ordinary main scheduler behavior and its original tests are restored.

Use an existing compiler, a dedicated scratch work directory, and the existing
oneTBB cache pinned by `deps.sh` (`3046c8b0c29df995980003ea24f4d78c80ec0c8d`).
The targets never download dependencies or build a compiler/framework. Run
source analysis/emission and native construction as separate guarded phases:

```sh
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl dag-fanin-emit \
  make -C research/experiments/compute-bench -j2 dag-fanin-emit \
  WFC=/absolute/path/to/whitefootc WORK=/absolute/scratch/dag-fanin
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl dag-fanin-build \
  make -C research/experiments/compute-bench -j2 dag-fanin-build \
  WORK=/absolute/scratch/dag-fanin DAG_TBB_PREFIX=/absolute/existing/deps
```

The emitted module and complete ordinary runtime use the CLI's `-O2` flags;
the C++ probe also uses `-O2`, with no historical placement or vectorization
controls. The cached oneTBB library keeps its recorded build flags.
The `*.sha256` and `*-flags.txt` files retain construction identities and
options. Before interpreting trace events, inspect the optimized code to
confirm that the recurrence consumes the begin hook's returned seed and the
end hook consumes the recurrence result. The hooks are opaque to WF, and
the C++ begin hook is noinline with a volatile seed readback.

Run each engine/image/width process once, retaining every result. Each command
below is a complete matrix, not an individual fixture. The WF matrix contains
115 cases/956 task rows; the oneTBB matrix contains 67 cases/764 task rows,
because it runs the original N graph once per weight assignment rather than
repeating the four WF source orders. Neither target times or retries calls:

```sh
for engine in wf tbb; do
  for image in plain trace; do
    for workers in 1 4; do
      WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl \
        "dag-fanin-$engine-$image-W$workers" \
        make -C research/experiments/compute-bench dag-fanin-run \
        WORK=/absolute/scratch/dag-fanin DAG_ENGINE="$engine" \
        DAG_IMAGE="$image" DAG_WORKERS="$workers" || exit
    done
  done
done
```

The separately selected spine-first comparator uses `DAG_VARIANT=phased`
with the ordinary `DAG_EMIT_FLAGS=--par`; build it in another dedicated work
directory. `DAG_ENGINE=wf-phased` selects only the existing 34 spine cases
(632 task rows), with the same graph oracle and recurrence observer. Run its
plain and traced images once each at W1/W4. It computes every spine task before
mapping the original leaf ranges, so the investigation charges those added
precedences. It introduces neither a worker-count source parameter nor a grain
override; permission, generated splitting, actual steals and observed overlap
remain separate facts.

The captured-scalar availability control uses `DAG_VARIANT=scalar` with the
ordinary `--par` emission and `DAG_ENGINE=wf-scalar`. It selects six existing
spine graphs: lengths 0, 1 and 32, with uniform leaf steps 1 or 65,536 and
spine steps 1 (132 task rows). Its one u64 leaf-cost scalar drives the real
recurrence. The original per-task cost vector remains oracle metadata and is
not passed to WF; `input` rows report these payloads separately. Use a separate
work directory and run plain/traced W1/W4 once each. The task contract,
spine-first phase order and observer are unchanged, with no grain hint.

The [runtime-adjacency probe](../../investigations/compute-model/DESIGN.md#runtime-adjacency-all-predecessor-probe)
uses `DAG_ENGINE=wf-runtime-loop`, `wf-runtime-tree` or `tbb-runtime` in a
separate `DAG_VARIANT=runtime` work directory. Its source baseline uses the
frozen main compiler and main C runtime, independently of the rolling-call
compiler/runtime trials. Keep the runtime inputs frozen through `ROOT` when
building from an isolated input tree; `DAG_SOURCE` and `WFC` can be supplied
explicitly. Qualify source admission and both forms' permission/emission
ledgers before native construction or execution. Run each selected engine's
plain/traced W1/W4 matrix once under its own guard after that qualification.

Both WF forms run 1,471 valid cases and 7,167 task rows: every forward graph
through five vertices with indegree/outdegree at most two, expanded over
`C=1,2,4` where valid (`C=0` for empty input), followed by the reverse-arrival
and unit/costly progress witnesses. Empty, singleton, triangle and disconnected
controls are labelled within the exhaustive set. The oneTBB matrix runs the
495 distinct graph/work assignments once each, for 2,399 task rows; owner
counts and WF forms do not duplicate its cases. WF also runs 13 malformed
input/configuration controls, checking `{status, rounds, notices}={1,0,0}`,
unchanged input/output bytes and no task events.

The ordinary Kahn oracle still owns every expected task value and count.
A separate calculation over the original edges checks the exact number of
cross-owner notices and `1 + maximum cross-owner edges on a path` processing
rounds, with zero rounds for empty input and `R <= C`. Valid WF calls start
with nonzero task cells and an extra in-slice sentinel, checking initialization
of exactly `N` cells. `routing` rows contain checked returned counters;
`routing-model` rows charge source initialization, routing and head/report
operations, excluding headers, stack frames and allocator metadata. They are
source operation counts rather than physical memory measurements. Progress
rows report actual task-0/task-3 overlap and retain any owner-round delay.
The new engines reuse the existing recurrence, observer and corruption controls.

The [retained qualification](dag-fanin-2026-09-23.tsv) passed all twelve
configurations and rejected all malformed-input and corruption controls. In
the traced W4 costly witness, the recursive source mapping overlapped tasks
2 and 3, while task 0/task 3 overlap remained absent across its owner rounds.
The direct native graph overlapped tasks 0 and 3 in its single retained run.
These observations establish functionality and expose the source ordering
cost; they do not measure a performance benefit.

Those original aggregate-result calls used the old binder: W1 had an inactive
pool but still entered the parallel body. The later
[adapter qualification](../../investigations/compute-model/DESIGN.md#indirect-aggregate-result-adapter-repair-2026-09-23)
separately checked actual sequential/parallel entry at W1/W4 with unchanged
task and routing oracles; it adds no timing result.

The traced image records one begin/end pair per serial task recurrence and
checks exact event counts, IDs, inputs, results, native thread identity and
completion of every original prerequisite before its consumer begins.
Overlapping intervals on distinct threads establish observed task overlap;
the observer's atomics affect scheduling, so these are not timing results.
Notification owner events begin after notice folding and cover task work only.
Separate deliberate output-bit and event-deletion controls must be rejected.
W1/W4 mean total participants; native mode uses a matching oneTBB arena and
does not start a WF worker pool. Source and observer byte counts are logical
payloads, excluding allocator metadata, oneTBB internal nodes, runtime lanes,
native stacks and canaries; no physical peak-memory measurement is claimed.
`actual_steals` counts tasks actually stolen through the runtime's legacy
`wf__par_grants` API. The saved initial default rows named that same counter
`grants`; it does not count offer attempts or successful lane acquisition.
`DAG_VARIANT` and `DAG_EMIT_FLAGS` allow a separately labelled, explicitly
selected compiler control without overwriting the ordinary default evidence.

## First-index expression probe

The [first-index investigation](../../investigations/compute-model/DESIGN.md#first-index-search-expression-probe-2026-09-22)
uses `first_index_probe.c` and `first_index_host.ll` here with its WF source
beside the investigation. The caller checks plain and diagnostic sequential,
fixed-batch, doubling-batch and local-return-helper forms against an
independent `memchr` first-index oracle. It checks complete diagnostic traces
and unchanged inputs; counters describe source-level work, not physical byte
traffic. The [retained results](first-index-2026-09-22.tsv) include the failed
construction attempts and the intentional bad-trace comparison control.
These files have no daily test, scoreboard or timing caller. Retain them as
reproducible evidence until superseded or no longer supporting this question.

From the repository root, use a prebuilt compiler and a fresh scratch path.
Run each stage separately; no target below builds a third-party dependency:

```sh
search_wfc=/absolute/path/to/whitefootc
search_build=/absolute/scratch/first-index
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl first-index-emit \
  make -C research/experiments/compute-bench -j2 first-index-emit \
  WFC="$search_wfc" BUILD="$search_build" WF_ALIGN=
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl first-index-build \
  make -C research/experiments/compute-bench -j2 first-index-build \
  WFC="$search_wfc" BUILD="$search_build" WF_ALIGN=
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl first-index-verify \
  make -C research/experiments/compute-bench first-index-verify BUILD="$search_build"
```

The verification stage runs `--no-overlap` at W1 and ordinary `--par` at
W1/W4. To check the comparison path separately, run the following command;
it deliberately corrupts one returned trace field and must report expected
1 versus actual 0 at position 4, with exit status 2:

```sh
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl first-index-self-control \
  env WF_WORKERS=4 "$search_build/first-index-par" par self-control
```

The [adjacent-helper qualification](../../investigations/compute-model/DESIGN.md#adjacent-helper-pair-native-result)
adds plain mode 4 with the same two local-return blocks. It compares the
original counted-block form, its diagnostic trace, and a native two-block
reference against the same independent outcomes. After emission above, build
the ordinary images and a separate predicate-event observer, then run the
fixed pass:

```sh
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl first-index-pair-build \
  make -C research/experiments/compute-bench -j2 first-index-pair-build \
  WFC="$search_wfc" BUILD="$search_build" WF_ALIGN=
WHITEFOOT_CHECK_TIMEOUT=30 perl .github/run-check.pl first-index-pair-verify \
  make -C research/experiments/compute-bench first-index-pair-verify \
  BUILD="$search_build"
```

The target runs `seq pair` at W1 and `par pair` at W1/W4, each over the 1,129
original fixtures plus twelve fixed controls. It then runs
`observed pair-observe` at W1/W4 over only those twelve controls. Every selected
invocation runs once. The matrix checks the plain pair's index and unchanged
inputs; trace fields belong to the unchanged mode 3 diagnostic and native
reference. Actual plain predicate prefixes are directly observed only on
the twelve positive-length controls. The observed W1 stream also supplies
the copied-stream missing-completion control, which must reject eight expected
events versus seven supplied without repeating a search. Raw events identify
records and thread identities; the observed image is not a timing instrument. The native
reference creates one pthread helper per wave, joins both blocks and validates
their private prefixes. It establishes the same first-index/work contract,
not pool competitiveness. None of these targets is a daily gate or benchmark.

## What "WF" means here, and what it does not

The range-loan consumer is selected with `make KERNELS=stencil verify` or
`make KERNELS=stencil compare`. Its runtime-sized grids use distinct positive
finite bit patterns derived from squared positions and retain their initial
boundary values across repeated Jacobi steps. The C oracle uses column-major storage and compares every
binary64 result bit with the row-major implementations. The compiler's native
tests run that same dimension/step matrix in sequential and parallel modes at
one, two, and four workers; `programs-check` also includes `stencil.wf` and the
recursive `range_split.wf` consumer.

The default timed stencil is 1024 by 4096 for 16 steps. Set
`WFB_STENCIL_GRID=original` for the initial 1024 by 2048 comparison, or
`WFB_STENCIL_GRID=small` for the separate 17 by 13, three-step overhead
measurement. The default `large` fixture affords four row chunks under the
shipped split floor; the original fixture affords two after the direct-view
permission correction. The exact dimensions are printed in every process's
workload header. Both WF and references allocate
and zero two grids, initialize them, compute and join every step, and release
the inactive grid inside the timed interval. The returned grid is checked and
released outside it. Native references submit one interior row per callback;
WF uses the compiler's default decomposition. Several nested loops and
initialization have different spans, so the stencil reports no single chunk
count, while actual scheduler grants are still measured.
`WFB_STENCIL_WIDTH` and `WFB_STENCIL_HEIGHT` may override those dimensions
within 3..4096 for an explicit size sweep; the workload header records the
actual values. The original, large, and small presets remain unchanged.

Stencil is included in the default timed comparison now that the baseline
compiler admits range loans. Cross-version twins still use the entry and
native-library adapters described below where their interfaces differ.
Stencil's original native comparison and evidence are recorded in
[`range-loans/DESIGN.md`](../../investigations/range-loans/DESIGN.md).

Select `KERNELS='prefix histogram'` for the blocked scan and privatized
histogram comparisons. Their runtime input, block size, and bucket count also
run through independent one-pass C oracles in the compiler's native tests,
including empty inputs, partial blocks, wraparound scan values, repeated keys,
and skew. The ordinary `programs-check` target compiles both consumers.
They require the runtime division images investigated in
[`compute-model/DESIGN.md`](../../investigations/compute-model/DESIGN.md), so
they are not yet selected by the default baseline twin.

The default blocked fixture has 4,194,321 input words and blocks of 4,096 words;
histogram uses 256 buckets. `WFB_BLOCKED_GRID=small|fine|coarse|skew` selects
the 17-word tail fixture, 256-word blocks, 65,536-word blocks, or a mostly
single-key distribution respectively. Every process prints the actual input
dimensions and distribution. Inputs and independent expected results are
prepared outside timing. WF and the parallel references allocate and release
their temporary block storage and allocate their result inside timing; result
checking and release happen outside. The native `serial` reference uses the
direct one-pass algorithm, while parallel references use complete blocks plus
a tail. Worker counts never appear as an unrolled source decomposition.

`KERNELS=merge_sort` selects the comparison sort with recursively partitioned
merges. Binary-search ranks determine disjoint destination views. The native
parallel references share this algorithm and its 64-element merge leaf;
`serial` uses `qsort`. The default is 1,048,593 random keys, with
`WFB_SORT_GRID=small|skew|equal` retaining a 257-key input, mostly repeated
keys, or all-equal keys. Two work buffers are allocated inside timing;
native copies use `memcpy` while Whitefoot copies ordinary element loops.

`KERNELS=bfs` selects the bounded-degree sparse graph experiment.
`WFB_BFS_MODE=sparse|pull` selects intrusive frontier lists or a full-vertex
pull round; `WFB_BFS_GRAPH=tree|chain|grid|disconnected` selects the fixture.
The graph and independent FIFO distances are prepared outside timing. Both
Whitefoot modes use two vertex-sized work arrays, allocated inside timing.
The native `serial` row always uses FIFO; the parallel native rows use FIFO
in sparse mode and the same pull algorithm in pull mode. Each process prints
levels, reachable vertices, sparse adjacency slots, and pull vertex visits.
The graph family is undirected with at most four adjacency entries per vertex;
it does not stand in for arbitrary CSR or high-degree graphs. The compiler
oracle covers 86 graph/mode configurations and checks every distance and
unchanged edge. The sort and graph consumers are opt-in for timing and always
included in `programs-check`.

`KERNELS=radix_scatter` selects stable distribution by one runtime-selected
bit. Whitefoot partitions input into 256-element local runs, packs them into
two intermediate digit streams through a recursive chain of exclusive
ranges, and copies their populated prefixes into the result. All allocations,
initialization, packing, and temporary releases are inside timing; releasing
the checked result is outside it. With `p = 256 * (floor(n/256) + 1)`, payload
workspace is `4p+n` words, plus chunk metadata and the recursive call stack.
The 256-element capacity is data decomposition, independent of worker count;
this consumer establishes neither wider radix digits nor runtime block sizes.

The default native parallel rows use the same block/chain decomposition.
`WFB_SCATTER_NATIVE=direct` selects a native count/prefix/scatter control with
two counts per block and no local element streams or packing chain. It changes
the native algorithm only, is printed as `native=direct` in the workload,
and is not a compiler A/B. The native `serial` row always uses two ordered
input scans. `WFB_SCATTER_GRID=large|small|skew|low|high` selects 1,048,593
mixed keys, 257 keys, a skewed digit, all-low digits, or all-high digits at
bit 63. The independent oracle covers tails, empty input, stable order,
unchanged input, and bits 0, 7, and 63. Its separately instrumented correctness
module checks nonempty input partitions and output copies completed on
another thread, resetting its observer only after the earlier maps join;
the timed module has no such instrumentation. This consumer is opt-in for
timing and included in `programs-check` and the compiler's native oracle suite.

[`radix-scatter-reference-2026-09-20.tsv`](radix-scatter-reference-2026-09-20.tsv)
retains the owned/reference source attribution on PR #70's model. It includes
separate tally and packing arms, identical-image host controls, and native
chain/direct context, with source/image hashes and all raw intervals. The
[interpretation and reproduction boundaries](../../investigations/compute-model/DESIGN.md#reference-model-scatter-result-2026-09-20)
explain the shared helper shape, restored input range map, qualified large-input
improvements and inconclusive small-input widths. These are opt-in research
measurements, separate from the formal performance-regression workflow.

[`radix-scatter-2026-09-14.tsv`](radix-scatter-2026-09-14.tsv) retains the
stable-scatter trial at `e2ced20c` in the same fixture/record-kind format as
the other compute-model evidence below. `steady-*` has zero call gap;
`gap500-*` is an explicit sparse-cadence diagnostic. Each cadence retains
large, small and skewed inputs with native chain, plus large input with native
direct scatter. Raw numerical rows are unchanged, metadata paths are
normalized, and before/after image hashes match. The
[interpretation](../../investigations/compute-model/DESIGN.md#stable-scatter-result-2026-09-14)
records the successful expression and failed competitive-cost criterion.
Keep these rows while the scatter investigation cites them.

[`compute-model-2026-09-13.tsv`](compute-model-2026-09-13.tsv) retains the
compute-model main run and diagnostic fixtures, with an interpretation in the
[investigation](../../investigations/compute-model/DESIGN.md#measurements-and-assessment-2026-09-13).
Each line starts with a fixture name and record kind (`manifest`, `raw`, or
`table`), followed by the original line. Metadata normalizes host and scratch
paths; manifests and tables omit empty rendered lines and terminal padding.
Raw numerical rows are unchanged. For example, extract the main raw
stream with `awk -F '\t' '$1 == "main" && $2 == "raw" { sub(/^[^\t]*\t[^\t]*\t/, ""); print }' compute-model-2026-09-13.tsv`.
The manifest records the 10,000-unit runtime control; pass
`-v runtimecontrol=-DWF_PAR_SPLIT_WORK_UNIT=10000` when reducing these rows.
The `initial-pooloff-*` groups instead record the earlier adapter-only control
and are explicitly inconclusive. This dated evidence is retained while its
measurement-method and grain decisions cite it; new runs do not overwrite it.

[`runtime-extent-2026-09-13.tsv`](runtime-extent-2026-09-13.tsv) uses the same
record format for the subsequent compiler-estimate trial: candidate `f851c65c`
against compiler `0d571cac`, with the runtime constants unchanged. It retains
the main run, protected adverse fixtures, threshold sweep and isolated
histogram repeat, including the failed chain-pull control. The
[interpretation](../../investigations/compute-model/DESIGN.md#runtime-extent-trial-result)
states why this trial fails its policy criterion despite the mechanism's
provisional retention. These rows precede the continuation-accounting fix and
the ordinary-callable/container integration and do not measure their
performance. Runtime-valued estimates are reported
as `chunks=na` by the harness's static header extraction; `steals` remains the
actual runtime observation. Retain these rows while the estimate investigation
or a design decision cites them; subsequent trials do not replace them.

[`compute-baseline-2026-09-21.tsv`](compute-baseline-2026-09-21.tsv) retains the
frozen `6fdb6768` baseline after the continuation correction, with the
`f19d53e2` retained-handle caller adaptation. Its `main-*` and `null-*` groups
contain all 4,290 checked calls from ten fixed fixtures; `calibration-*` groups
are prior one-call cost checks. Common provenance, source/image hashes,
construction and execution costs appear once under `context`. The
[interpretation and pinned reproduction commands](../../investigations/compute-model/DESIGN.md#frozen-compute-baseline-2026-09-21)
retain the noisy nulls and the native scalar/WF optimizing flag asymmetry.
The null invokes one identical image path twice: it qualifies host/cadence,
not separately linked layouts, despite the reducer's generic twin boilerplate.
These rows do not establish native competitiveness or isolate the historical
continuation fix. Keep them while the investigation or its dependents cite them.

[`native-kernel-vectorization-2026-09-22.tsv`](native-kernel-vectorization-2026-09-22.tsv)
adds the bounded wide-stencil/FIR W1/W4 control against those same frozen
objects. It retains all 1,200 checked timing calls, full oracle results,
identical-image and unchanged-WF-object controls, optimized callback excerpts,
and the exact one-shot construction recipe. The complete native kernel C
translation units permit vectorization; FIR also removes its local tap-loop
prohibition. Cached scheduler libraries retain their original flags. Both
fixtures cross the predeclared native-improvement criterion; the
[interpretation](../../investigations/compute-model/DESIGN.md#native-kernel-vectorization-control)
keeps the noisy stencil null, the remaining stencil W4 gap, and the limits on
causal attribution. No default build flags change.

The `wf` row is the module `whitefootc` emits from the kernel's `.wf` source
under **plain `--par --emit-llvm` and no other flag**, linked with
the complete ordinary native library under `compiler/src/backend/`
**from this same tree**, built with the driver's clang flags plus the symmetric
x86_64 `WF_ALIGN` placement control documented below, and called through a
descriptor-only LLVM IR host adapter. `host-adapter.awk` binds its calls to the
same execution world as entry: the emitted sequential clone when the worker
pool is off, and the ordinary parallel symbol otherwise. Selection occurs at
each host entry, outside the kernel algorithm, for assigned results and
unassigned void calls carrying an explicit result pointer. Calls without a
matching clone remain unchanged. The binder accepts straight-line wrappers;
it does not rewrite existing control-flow edges. The
[aggregate-result repair and qualification](../../investigations/compute-model/DESIGN.md#indirect-aggregate-result-adapter-repair-2026-09-23)
preserve older results under their recorded binder. LLVM IR also supports versions
whose source functions have internal linkage. The two emissions are isolated by renaming
their defined functions and all corresponding references; this preserves
linkage and optimization attributes and leaves library imports and weak
runtime hooks unchanged. Changing a strong definition to internal linkage
would change the compiler optimization being measured.

Owned array results cross the host adapter as a borrowed element pointer and
length plus a retained `Box` handle. Every caller passes that handle to the
generated release entry after checking the result, outside the timed interval.
Native results keep their allocation pointer as the equivalent release handle.
This follows the formal host ABI without changing the algorithms, fixtures,
allocation boundaries, or result checks. Scalar quadrature needs no handle.

- No C adapter to any runtime is labelled WF.
- No research copy of the Whitefoot runtime is carried here; the runtime
  sources come from `../../../compiler/` by relative path.
- Nothing selects the chunk count: no flag, no environment variable, no source
  edit. The row is the program the compiler produces or it is nothing. The two
  A/B handles below can build a different compiler control or a different
  runtime constant, and a table taken under either says so in its own header
  and is never recorded as a plain one.

Quadrature's recursive component is cut by the recursion budget the runtime
answers at its entry, and `--par-recursive-frontier off` is the control that
withholds that cut; the three map kernels reach the runtime through a
synthesized splitter, which no budget can touch, so their modules are
byte-identical under every setting of it.

`wf-seq` is the same source compiled `--no-overlap --emit-llvm` and run at width
one. It is the sequential **control**, not a reference: it never enters the
ratio column. It answers "did `--par` buy anything at all", which is a different
question from "is `--par` the fastest".

### The three A/B handles, and why none of them appears in a recorded table

`WF_PAR_CONTROL_FLAGS` is appended to the `--par` emission of the **twin image**
described in the next subsection, and is **empty by
default**. It exists so that one tree can answer "what would this compiler
control be worth here" — two images differing in exactly that flag, built from
one tree, measured in one `compare` run so the reference rows are shared:

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/ab-off \
     WF_PAR_CONTROL_FLAGS='--par-recursive-frontier off'
```

The emitted module depends on this value the way it depends on the source and
on the compiler binary: the Makefile keeps the value in a stamp file the module
rule reads, so setting the variable or clearing it again re-emits, and a
control-flag run can never silently re-time the plain image the last run left.

It is a measurement control and not a grain knob. **A table recorded in
`RESULTS.md` is always taken with it empty**, because the `wf` row of a
recorded table is the program plain `--par` produces and nothing else; a
setting that made a row nicer would be measuring a program no Whitefoot user
gets. Two things keep the two kinds of table apart without anyone having to
remember: `manifest.txt` records `WF_PAR_CONTROL_FLAGS` on every run, empty or
not, and a non-empty setting adds a `WF --par control flags=` line to the table
header saying in the table itself that it is not the plain program. The
bundle's gate target, `programs-check`, never reads the variable.

`WF_RUNTIME_CONTROL_FLAGS` is the same handle on the other side of the link,
under the same discipline. It is appended to all twelve native-library
translation units of the twin, including the floor, scheduler, completion and
ordinary-value bodies, so one tree can answer "what would this runtime constant be
worth on this block of kernels" without editing the constant per run:

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/unit-300k \
     WF_RUNTIME_CONTROL_FLAGS=-DWF_PAR_SPLIT_WORK_UNIT=300000
```

Not every value it carries is a constant to sweep.
`-DWF_PAR_IDLE_WINDOW_ON_ASYMMETRIC=1` is a twin knob rather than a number. The
runtime withholds the idle window from a machine whose CPUs are not all of one
performance level; this flag withdraws that test and nothing else, so on such a
machine the twin arm is the older behaviour and the `wf` row is the current one,
and `A/B  wf-b/wf  wall` above 1.000 reads the current rule ahead. On a machine
whose CPUs are alike the flag changes nothing and the pair is a null. The Apple
M1 Pro section at `8c296c91` in
[`../../investigations/compute-runtime/RESULTS.md`](../../investigations/compute-runtime/RESULTS.md)
is the table it was built for.

It is **empty by default**, kept in its own stamp file that the twin's four
runtime object rules depend on — so setting it or clearing it recompiles and
relinks the twin rather than re-timing the one the last run left — recorded in
`manifest.txt` on every run, and announced in the table header as a
`WF runtime control flags=` line when it is not empty. **A table recorded in
`RESULTS.md` is always taken with both variables empty**, for the same reason:
the `wf` row of a recorded table is the program plain `--par` produces linked
with the runtime this tree ships. It reaches no reference, no oracle and not
the harness, because none of those is the Whitefoot runtime; the harness asks
the linked runtime for the split at run time instead of carrying a copy of the
rule, so the `note` column's chunk count and the two split verify fixtures
follow the image they describe — under a changed work unit and under a changed
oversubscription cap alike.

`WF_MODULE_CONTROL_FLAGS` is the third handle, and the one that moves how the
Whitefoot side of the twin is **compiled** rather than what it says. It is
appended to the twin's compile of the emitted `--par` module object and of its
Whitefoot native-library units — both sides of the link that `WF_FLAGS` owns — so
one tree can answer "what would this code generation flag be worth on exactly
the translation units `whitefootc` builds":

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/fp \
     WF_MODULE_CONTROL_FLAGS='-fno-omit-frame-pointer'
```

It is **empty by default**, kept in its own stamp file that the twin's module
object and its native-library objects depend on — so setting it or clearing it
recompiles and relinks the twin rather than re-timing the one the last run left
— recorded in `manifest.txt` on every run, and announced in the table header as
a `WF module control flags=` line when it is not empty. **A table recorded in
`RESULTS.md` is always taken with all three variables empty**, for the same
reason as the two above: the `wf` row of a recorded table is the program plain
`--par` produces, built the way the bundle builds the Whitefoot side for both
arms. It reaches no reference, no oracle, not the harness and not the shared
`--no-overlap` control object, because none of those is the Whitefoot side of
the link.

Its first use was the alignment question, and that question has since been
settled the other way. It read all sixteen twin lines inside [0.954, 1.010] on
this bundle's development host — nothing worse than one percent at any width,
and less than the same host's own placement spread in either direction — and
`whitefootc`'s own driver was left alone on that reading
([`RESULTS.md`](../../investigations/compute-runtime/RESULTS.md), loop and
function alignment on the Whitefoot side). The driver is still unchanged, but
the **bundle** now gives both arms those flags unconditionally through
`WF_ALIGN`, as placement control for the regression gate rather than as a
performance choice, so asking this handle for them again would set them twice
on one arm and change nothing. "The two flag sets" below is where that sits.

### The placement-only arm

Every candidate the twin has measured so far differed in bytes as well as in
behaviour, so a reading could never be attributed to one rather than the other.
`compiler/src/backend/sched/core.c` therefore carries a never-called,
non-inlined 587-byte function under `#ifdef WF_PLACEMENT_PAD`, which nothing
that ships defines and `whitefootc` never passes. Through the runtime handle it
gives the twin an arm that is **identical in behaviour and shifted in
placement**:

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/null-shift \
     PASSES=5 CALLS=5 WF_RUNTIME_CONTROL_FLAGS='-DWF_PLACEMENT_PAD=1'
```

The pad sits ahead of every function in the scheduler core, so the core's own
functions and the library objects the link places after it — scheduler
primitives and entry, completion and ordinary-value bodies — move by its size, and nothing else
about either image changes. What it does **not** move is the emitted module:
that object is linked ahead of the native-library ones, so the kernel's own code
sits at the same offsets in both arms and what the arm shifts is the scheduler
the kernel calls into. Read against `WF_AB=1`, whose arms are byte-identical,
the difference between the two is what that placement alone is worth on the
host. A quadrature row is the cleanest reading of it: that kernel emits no split
call, so its two arms do identical work in every pass whatever the runtime
handle carries. What it read here is in
[`RESULTS.md`](../../investigations/compute-runtime/RESULTS.md), what code
placement alone is worth, with a shifted null arm: a block median moved 11.7
percent and one kernel's five paired readings spanned 0.68 to 1.30 over
identical work, which is the bound on what a single A/B line can select on this
host.

### The A/B twin: what the controls actually build, and how to read it

No control moves the plain image. Setting any of the three — or setting
`WF_AB=1` with none of them — builds a **second image per kernel**,
`$(BUILD)/<kernel>-b`, from the same sources with the controls applied, and
times it in the same passes as
the form **`wf-b`**. `$(BUILD)/<kernel>` is always built with all three
controls empty.

**Why a twin and not a second run.** On a host whose run-to-run spread is wider
than the effect being looked for, two separate `compare` runs cannot select
between two candidates, and this bundle's development host is such a host: six
runs of *byte-identical* images read the quadrature W=4 ratio anywhere from
0.916 to 1.035 — thirteen percent — while records and fir W=4 spread 8.5 and 8.8
percent over the same six. The reducer already solved this for the references.
Within each pass every form runs as its own process in a rotated, alternating
order, and the `wf` row's `ratio` is the median of **within-pass matched pairs**
rather than a quotient of two run medians. The twin puts a second Whitefoot
image into those same passes, so the two arms of an A/B are separated by
minutes of one machine's own schedule instead of by two runs.

```sh
# the oversubscription cap and the work unit, one candidate against the shipped
# runtime, in one set of passes
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/cap-256 \
     PASSES=5 CALLS=5 \
     WF_RUNTIME_CONTROL_FLAGS='-DWF_PAR_SPLIT_OVERSUBSCRIBE=256 -DWF_PAR_SPLIT_WORK_UNIT=75000'

# the instrument's own null check: a twin that differs in nothing
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/null \
     PASSES=2 WF_AB=1
```

**What is doubled and what is shared.** The twin gets its own emitted module
(`<kernel>-par-b.ll`), its own object from it, its own kernel object with the
weight read out of *that* module (`$(BUILD)/b/<kernel>_split.h`), and its own
complete set of Whitefoot native-library objects. It shares `harness.o`, every
`backend_*.o` and the `--no-overlap` control object with the plain image,
because no control reaches those and a second copy of the same bytes under
another name would move code placement, which is this bundle's largest
confound. The two link-time assertions described just below are applied to the
twin exactly as to the plain image: a twin that silently lost the scheduler
would read as a runtime constant worth a great deal.

**How the table reads.** `wf-b` gets a row in every block, with its own
`ratio`, `cpu_r`, `lower`, `steals` and `note`, computed against the best
reference of each pass exactly as `wf`'s are — neither Whitefoot row is ever a
reference for the other, so `wf`'s ratio is the number it would have been with
no twin in the run. Under each block, one line gives the twin verdict directly:

```
mandelbrot   4 A/B  wf-b/wf  wall 0.981 [0.96-1.01]  lower 4/5  cpu 1.002
```

`wall` is the **median of the within-pass paired ratios** of `wf-b` to `wf` with
its `[min-max]`, `lower` is how many of those pairs had the twin faster, and
`cpu` is the same pairing over process CPU. It is printed at every width the
block carries, width one included, because "the twin changed nothing at width
one" is a thing an A/B run has to be able to say. `BEST REFERENCE`, `FASTEST`
and `WF fastest` skip the twin: they answer whether the program *this tree*
produces is the fastest thing in its row, and a twin built from a control flag
is not a thing this tree produces.

**A recorded table never contains a `wf-b` row.** The twin is an instrument, not
a result: the tables copied into `RESULTS.md` as the record of what this tree
produces are taken with all three controls empty and `WF_AB` unset, which builds no
twin, runs no extra process and leaves the bundle exactly as it was. An A/B run
is recorded as the *arms of an experiment*, with the control flags in its
heading and the `A/B` lines quoted in its reading. A manual dispatch of
`.github/workflows/compute-bench.yml` takes the three controls and call cadence
as inputs. A control pair is an experiment, not a recorded plain table.
The optional `programs-check` and legacy `verdict-test` commands read none
of these handles; neither is invoked by the repository gate.

**A fourth handle hands the same twin a different tree.** `WF_B_SCHED_DIR`,
`WF_B_FLOOR` and `WF_B_WFC` name where the twin's Whitefoot runtime and its
emitting compiler come from, each defaulting to this tree's own. Pointed at a
worktree of the merge base they make `wf-b` the program the branch started
from, which is the whole of [the compute regression
check](#the-compute-regression-check). The twin retains the same algorithm, oracle, runtime checks and
measurement protocol. Its entry and native components follow its interface.

The explicit `WF_B_INTERFACE=ordinary|command` selects only the entry and
native-link boundary. `ordinary` requires the complete native library beside
`WF_B_SCHED_DIR`, including `ordinary_values.c` and `ordinary_values.ll`;
missing units fail instead of selecting another profile. `command` uses the
baseline floor, scheduler core, primitives and entry units. The workflow reads
the baseline's exact nullary main header to select the profile and rejects an
unknown interface. `baseline-entry.sh` adapts the **current** program by
prefixing only that header with `command`; reversal must be byte-identical.
Other source incompatibilities still fail the build. Remove this adapter when
that older interface is no longer a comparison baseline.

Same-interface twins share the sequential control object. A command baseline
also compiles its own sequential control from the adapted current source,
because the current object imports ordinary functions absent from the old
library. That control is not timed as `wf-b`, but it changes image placement;
the manifest identifies the exception and hashes both modules and adapted
source. It does not substitute a same-version comparison or exempt a verdict.
The C references, host kernel adapters, flags, widths, seeds, five alternating
pairs, strong-runtime assertions and `verdict.awk` are unchanged.

Two link-time assertions make a `wf` row a `wf` row. The emitted module carries
**weak no-op stubs for every `wf__par_*` symbol**, so a link that loses the
scheduler sources would still link, still run, still produce correct output, and
be silently sequential. The harness therefore references `wf__par_grants()`
and `wf__sched_split_work()`, neither of which has a weak stub, and the
Makefile requires a strong definition of `wf__par_publish`,
`wf__par_split_budget` and `wf__par_recursion_budget` in every image after the
link — the twin's image included. `wf__par_split_budget`, which the harness
also calls for the `note` column's chunk count, *does* have a weak stub, so it
is that post-link strong-binding assertion and not the reference that keeps the
count from being a stub's zero.

## Formal performance regression

The automatic WF baseline/candidate comparison is owned by
[`tests/performance`](../../../tests/performance/README.md) and
`.github/workflows/compute-regression.yml`. It uses formal current fixtures,
independent result oracles and each arm's own compiler/runtime, without this
bundle or comparison frameworks. Its crafted-data integrity checks belong to
the ordinary gate; its compiler pair and measurements do not.

This bundle remains an explicitly requested framework comparison. Its legacy
`verdict` and `verdict-test` commands reproduce the older table protocol; they
are not the automatic regression verdict or part of `make check`. Historical
measurements retain their recorded flags, workloads and limitations.

## Running it locally

```sh
export WHITEFOOT_SCRATCH_ROOT=${TMPDIR:-/tmp}/whitefoot
base=$(git merge-base origin/main HEAD)
git worktree add --detach "$WHITEFOOT_SCRATCH_ROOT/baseline" "$base"
cargo build --manifest-path "$WHITEFOOT_SCRATCH_ROOT/baseline/compiler/Cargo.toml" \
    --profile gate --bin whitefootc --locked --offline

cd research/experiments/compute-bench
export WF_B_SCHED_DIR="$WHITEFOOT_SCRATCH_ROOT/baseline/compiler/src/backend/sched"
export WF_B_FLOOR="$WHITEFOOT_SCRATCH_ROOT/baseline/compiler/src/backend/wf_floor.c"
export WF_B_WFC="$WHITEFOOT_SCRATCH_ROOT/baseline/compiler/target/gate/whitefootc"
export WF_B_SOURCE="$base"
export WF_B_INTERFACE=$(sh baseline-entry.sh profile \
  "$WHITEFOOT_SCRATCH_ROOT/baseline/research/experiments/compute-bench/programs/quadrature.wf")
make build && make verify
make compare PASSES=5 CALLS=5 RESULTS="$WHITEFOOT_SCRATCH_ROOT/regression"
make verdict RESULTS="$WHITEFOOT_SCRATCH_ROOT/regression"
```

`verdict` is a separate make invocation, so it has to be told which run to
read: `RESULTS` is a fresh timestamped directory per invocation, and the
default table it would otherwise look for is one that was never written. Pass
the `compare` run's own directory, or a table directly with
`VERDICT_TABLE=<path>/table.txt`. The bands are variables too —
`VERDICT_WIDTHS`, `VERDICT_WALL`, `VERDICT_CPU`, `VERDICT_LOWER`,
`VERDICT_PAIRS` — so a local reading can be taken at another width or another
band; the workflow sets none of them and gets the rule above.

A local run on a quiet machine is worth more than a hosted one and is the right
place to check a suspected regression by hand. A hosted verdict is evidence
about a hosted runner, and the `raw.tsv` behind it is uploaded with every run.

## Running it

Prerequisites: `clang`, `clang++`, `cmake`, a stable Rust toolchain, and one
network-enabled run of `make deps` (its `fetch` step is the only step in the
bundle that touches the network; everything after it is offline).

```sh
cd research/experiments/compute-bench
export WHITEFOOT_SCRATCH_ROOT=${TMPDIR:-/tmp}/whitefoot   # the default; nothing is
                                                          # ever written in the repo
make deps      # fetch (network) and build the pinned schedulers
make build     # the compiler, both modules per kernel, one image per kernel
make verify    # every form at every emitted width, correctness only
make compare   # the timed passes and the table, into a fresh RESULTS
```

The default scratch root is the system temporary directory, which the host may
clear on reboot: the fetched and built dependencies and any `RESULTS` left
there are then gone and have to be rebuilt, so copy out anything worth keeping
(the tables that matter are transcribed into `RESULTS.md` by hand anyway), or
set `WHITEFOOT_SCRATCH_ROOT` to a durable directory outside the repository.

`make deps`, `make build` and `make verify` are the only targets that can fail
on anything other than a missing or malformed row.

Each kernel image also answers directly:

```sh
BUILD=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/build
$BUILD/mandelbrot list                     # forms, grain policies, widths
WF_WORKERS=4 $BUILD/mandelbrot verify wf 4
WF_WORKERS=4 $BUILD/mandelbrot time wf 4 0 5
WF_WORKERS=4 WFB_GAP_US=2000 $BUILD/mandelbrot time wf 4 0 5   # a 2 ms gap
```

`WF_WORKERS` must be set and must equal the `WIDTH` argument for every form,
native ones included; the driver cross-checks the two before anything else
happens, because a disagreement would silently compare two different widths.
`WFB_GAP_US` is the other variable the table sets and is optional: unset is the
back-to-back cadence, and "The gap between calls" below is what a value does.

`make programs-check` and `make verdict-test` are optional manual checks;
neither times anything. `programs-check`
compiles each program in exactly the two modes the table uses and asserts that
`--par` emits a publish site and `--no-overlap` emits none. For timed kernels,
including stencil, it also links both emissions with the complete ordinary
library, checking adapter symbols and strong runtime bindings. The untimed
`range_split` witness uses its ordinary entry: both modes are built and
executed, and the parallel executable's strong runtime bindings are checked.
A small executed symbol test checks quoted and bare
references, imports and weak overrides without timing a kernel. These checks
need Clang but no third-party scheduler dependencies; kernel numerical
correctness remains `verify`'s job. `verdict-test` feeds the
regression rule fourteen crafted table fragments and asserts the exit status
and the verdict text of each — the pass, the two-block failure, the one-block
`suspect`, one adverse block in each of two kernels, both halves of the adverse
test on their own, the two exclusions, the five refusals and the CPU report;
it needs no compiler at all.

## Where results go, and the fresh-directory rule

`make compare` writes `manifest.txt`, `raw.tsv`, `table.txt` and `logs/` into
`$(RESULTS)`, which defaults to a UTC-timestamped directory under the scratch
root. **A run never overwrites an earlier table: `compare` refuses a `RESULTS`
path that already exists.** Re-running by hand therefore needs a fresh
`RESULTS=<path>` — that refusal is the rule, not a bug:

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/try-2
```

`make clean` also clears the way, and it is the wrong reach for that: it is
`rm -rf $WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench`, which takes the build
tree, the installed dependencies and **every earlier table under
`results/`** with it. Use it to start from nothing, not to free up one path.

Nothing is written inside the repository. Job logs and artifacts expire, so a
table that matters is copied by hand, with its CI run id, into
[`../../investigations/compute-runtime/RESULTS.md`](../../investigations/compute-runtime/RESULTS.md),
which also carries the reproduce recipe and the two non-pooling rules.

`manifest.txt` records `uname -a`, the online CPU count, the CPU topology — one
`topology: cpu<N> core=<id> package=<id> siblings=<list>` line per online CPU on
Linux, `topology: physicalcpu=<n> logicalcpu=<n>` on macOS, and `unqualified`
where neither source is readable, beside an `smt:` line read from
`/sys/devices/system/cpu/smt/active` — the **inherited** CPU
mask and cgroup state (recorded, never narrowed, and marked `unqualified` when
a file is absent rather than reported as "no limit"), the compiler revision,
every toolchain version — the whole of `rustc -vV`, host triple, commit and
LLVM version included, one `rustc: ` line each — the exact flag strings, all
three A/B control variables whether they were set or empty, what the twin was
built from — `WF_B_SOURCE` on every run, and on a baseline run the three paths
and one `WF_B_SOURCE_SHA256` line per baseline source — the
three dependency pins, `BENCH_ARCH`, and the SHA-256 of every kernel image
and every emitted `.ll` before and after the run. If any of those hashes moved
during the run the table is not a measurement of one build and `compare` says
so and fails.

The topology and `smt:` lines are there because hosted runs land on
`ubuntu-24.04` runners of more than one machine class, and four online CPUs may
be four cores or two cores with SMT siblings. Without them a recorded table
cannot be classified by runner, and the lane-placement question — whether
Whitefoot's two lanes landed on the SMT siblings of one core, which is what a
W=2 row spending two lanes' CPU on one lane's work would look like — cannot be
asked of a run after the fact.

### Reproduce recipe

```sh
export WHITEFOOT_SCRATCH_ROOT=${TMPDIR:-/tmp}/whitefoot
make -C research/experiments/compute-bench deps     # once, with a network
make -C research/experiments/compute-bench build
make -C research/experiments/compute-bench verify
make -C research/experiments/compute-bench compare PASSES=5 CALLS=5
```

Manual dispatch runs these commands in `.github/workflows/compute-bench.yml`
on `ubuntu-24.04` and `macos-14` and publishes the requested tables. There is
no push trigger. Automatic performance verdicts use the formal paired runner
in `compute-regression.yml`.

## The gap between calls

Five calls per process, back to back, with microseconds between them: that is
the cadence every recorded table is taken at, and it is the cadence a runtime
that keeps an idle lane hot for a window before it parks is measured at its
best in, because the next call always arrives while the lanes are still hot.
Real programs have gaps between their parallel regions.

`WFB_GAP_US` is that gap, in microseconds, per process. It is **zero by
default**, which is the back-to-back cadence and exactly what the bundle did
before the mode existed.

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/gap-2ms \
     PASSES=5 CALLS=5 WFB_GAP_US=2000
```

Between two consecutive timed calls the driver waits that many microseconds
**outside the measured interval**, on a monotonic-clock busy-wait and not a
sleep: the point of the mode is that the calling thread stays running, as a
program doing its own sequential work between parallel regions does, while the
runtime's helper lanes go idle and park. A driver that slept would hand its CPU
back and measure something else. The wait is the last thing before the clock
starts — after the previous call's verification and its printf — so no part of
a gap is inside the wall bracket. CPU reads also follow the gap, but their
placement alone does not establish that a short counter delta excludes work
accounted from an earlier interval; see the CPU attribution qualification below.

**It reaches every form identically, references included.** A gap that reached
only the `wf` row would compare one scheduler's idle policy against another
scheduler's warm one, which is not a comparison; what a gapped table reads is
how the compiled program and each reference alike behave when their work
arrives sparsely.

It is a **run-time setting and nothing else**: no stamp, no emission, no
compile and no link reads it, on the plain image or on the twin, so changing it
rebuilds nothing and the images a gapped run times are byte-identical to the
ones a gap-free run times. That is what lets it compose with the A/B twin, and
the composition is the run the mode was added for:

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/gap-2ms-nowindow \
     PASSES=5 CALLS=5 \
     WFB_GAP_US=2000 WF_RUNTIME_CONTROL_FLAGS=-DWF_PAR_IDLE_WINDOW_US=0
```

— the runtime this tree ships against the same runtime with its idle window
withheld, both at a 2 ms gap, in one set of passes, paired within each pass by
the `A/B wf-b/wf` line. That is the sparse-cadence reading
[`RESULTS.md`](../../investigations/compute-runtime/RESULTS.md) records as open
beside the idle window, and the question the bundle could not ask before: how
much of a win measured on back-to-back calls survives a gap, and what the
references do across the same gap.

**A table taken at a non-zero gap is never a candidate record for the W
blocks**, on exactly the terms of the three control flags above: the recorded
table for a host is its highest non-oversubscribed block at the back-to-back
cadence, and a table at another cadence measures a different question about the
same programs. Three things keep the two apart without anyone having to
remember. `manifest.txt` records `WFB_GAP_US` on every run, empty or not. Every
process writes the gap it ran at into its own header and trailer in `raw.tsv`,
and the reducer **refuses to put two cadences in one table** — a disagreement
is a malformed stream, refused exactly as a header/trailer mismatch is, and
never a judgement about a measurement. And a non-zero gap puts `gap_us=` on the
table's `passes=` line with a disclosure line above it, saying in the table
itself that it is not the back-to-back one.

`verify` is handed the variable too and **waits nothing**. It drives no timed
call loop: the whole fixture sweep is one call into the kernel's own `verify`,
which no clock brackets and no table reports, so the only place a wait could go
is inside a grid that measures nothing, where it would lengthen `verify` and
check nothing. What `verify` does with it is read it, report it in its
`VERIFY PASS` line, and refuse a malformed value cheaply, before a long
`compare` pays for the same mistake. `programs-check` reads it no more than it
reads the three controls: it performs untimed emission and link checks.

A hosted run takes the gap as the `gap_us` input of
`.github/workflows/compute-bench.yml`, beside the three control inputs.

## The lane trace

A gapped run says the compiled form gives up wall time at a sparse cadence on
some hosted runners; it does not say where that time goes, and a table cannot.
`WF_PAR_TRACE` is the runtime's own answer: defined,
`compiler/src/backend/sched/core.c` records a fixed per-lane ring of scheduler
events and prints it to **stderr** at process exit, one line per event. What it
found is in [`RESULTS.md`](../../investigations/compute-runtime/RESULTS.md),
the three `bench/wake-trace` sections.

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/gap-2ms-trace \
     PASSES=5 CALLS=5 \
     WFB_GAP_US=2000 WF_RUNTIME_CONTROL_FLAGS=-DWF_PAR_TRACE=1
```

It is a runtime control like any other, so it reaches the **twin only**: the
`wf-b` cells trace and the `wf` cells are the runtime this tree ships, and the
`A/B wf-b/wf` line of the same table is what says whether carrying the
instrument moved the reading. Nothing else changes. The events go to stderr,
which `compare` already keeps per cell in
`results/logs/<kernel>-wf-b-w<W>-p<pass>.log`, so the parsed stdout stream and
`raw.tsv` are untouched and the workflow uploads the trace with the rest of
`logs/`.

```
wf-trace lane=<n> ev=<kind> t_us=<monotonic us> cpu=<sched_getcpu> v=<payload> seq=<n>
```

| kind | when | `v` | `seq` |
| --- | --- | --- | --- |
| `call_head` | the splitter's entry query, once per top-level call | the idle mask then | 0 |
| `root_publish_first` | the first publish of a burst that finds lanes parked | the idle mask | 0 |
| `park` | entering the condvar wait, in either idle loop | spin/yield rounds done | 0 |
| `wake` | returning from that wait | the publish epoch seen | 0 |
| `steal_ok` | the lane's first successful steal after a wake or a call head | 0 | 0 |
| `chunk` | one executed callback; the timestamp is when it finished | its duration in ns | its index in the call |
| `probe` | a fixed dependent chain of 20,000 steps, timed | its duration in ns | 0 straight out of a park and at a call head, else the chunk it followed |
| `root_join_done` | the release that closes the call on the offering lane | 0 | 0 |

Per call and per lane that gives wake latency (`wake` minus `call_head`), steal
latency, how long the lane had been parked, the CPU it parked on against the
CPU it woke on and the offering lane's own, the work it did as a time series of
chunk durations against their position in the call, and — from `probe` — how
fast the core it is on was running at each of those points. Lines are grouped
by lane and ordered within a lane; sort by `t_us` to interleave them. A final
`wf-trace-probe sink=<n> iterations=<n>` line carries the chain's accumulated
result, so its work is visibly consumed.

The probe rides on chunks 1, 2, 4, 8, … so its cost grows with the logarithm of
the chunk count; on the hosted runners it added **3 to 4 %** to a `wf-b` cell at
two and four lanes. It also lengthens every latency measured from the call
head: the offering lane runs one probe before it publishes anything, and each
helper runs one before its first scan, so head-to-`wake` reads about 39 µs where
a build without the probe reads 21, and head-to-`steal_ok` 57 where it reads 22.
**Read wake and steal latency off a run built without the probe.** `chunk`
events are capped at 64 per lane per call, above what any map kernel here
reaches; a recursive kernel that runs a thousand leaves a call is truncated at
that count rather than filling the ring, and shows it by its chunk events
stopping while its park, wake and join events continue.

It is a **measurement instrument**. No shipped build, gate target or test
defines it, `whitefootc` never passes it, and with it undefined the scheduler
core and the host primitives compile to the same object bytes they did before
the instrument existed.

Pairing it with a wider idle window asks whether any of it is the parking:

```sh
make compare RESULTS=$WHITEFOOT_SCRATCH_ROOT/whitefoot-compute-bench/results/gap-2ms-trace-hot \
     PASSES=5 CALLS=5 WFB_GAP_US=2000 \
     WF_RUNTIME_CONTROL_FLAGS="-DWF_PAR_TRACE=1 -DWF_PAR_IDLE_WINDOW_US=3000"
```

— the same instrument over lanes that stay hot across the gap, so the two runs
differ in whether a lane parked and in nothing else.

## How to read the table

```
kernel       w form              median_us  mad%  p10..p90_us  cpu_us  ratio  cpu_r  lower  steals  note
...one row per form of this kernel at this width, ascending by median...
<kernel>   <w> BEST REFERENCE = <form>   FASTEST = <form>   WF fastest: yes|no
  <form>: <that form's grain policy, the whole string the binary printed>
  <form>: ...
```

- **`median_us`** is a median of medians: the median over the five passes of
  each process's median over its five recorded calls. A pass is one sweep over
  every cell; each cell is a fresh process; the sweep order is rotated by
  `(pass + kernel index)` and reversed on odd passes, so no implementation ever
  holds a fixed position in the sequence.
- **`mad%`** and **`p10..p90_us`** are spread, as a percentage of the median and
  in microseconds. **Nothing fails on either.** Spread is information: the
  end-to-end Mandelbrot check behind this bundle's sizing spread 11.3 to 13.4 ms
  on a quiet four-CPU box at width four, which is about eighteen percent.
- **`cpu_us`** is the same median of medians over **process CPU time** rather
  than wall, using counter deltas read around the same call. The intended
  coverage is CPU spent across the process's threads, including spinning
  workers; precise attribution to that call needs the qualification below.
  The source is chosen per host and the driver line of `raw.tsv` names the one
  read as `cpu_clock=`: `CLOCK_PROCESS_CPUTIME_ID` on Linux, **`task_info` on
  Darwin**, `getrusage(RUSAGE_SELF)` as the fallback anywhere the chosen source
  is absent or refuses. Darwin is not on the POSIX clock because it answers that
  clock from the task's accounting for threads that have already exited: a pool
  whose workers are still alive contributes nothing, so the column read about
  one lane's worth however many lanes ran, and the M1 Pro tables in `RESULTS.md`
  recorded `tbb` spending 5,896 us of CPU for a 2,441 us wall on eight threads
  beside a four-lane `static` row spending 2,904 us against a 2,866 us wall. The
  Darwin source is therefore the task-level pair that does consult the live
  threads when asked: `task_info(TASK_THREAD_TIMES_INFO)` for the threads that
  still exist plus `task_info(TASK_BASIC_INFO)` for the ones that have exited,
  each `time_value_t` seconds and microseconds, summed. These separate reads
  cover both categories but do not form an atomic snapshot across thread exit.
  That source is held on a reading and not on its documentation: the hosted
  `macos-14` leg of run
  34668036736, a three-CPU runner, printed `cpu_clock=task_info` on every driver
  line and returned CPU that grows with the lanes and stops where the CPUs do —
  mandelbrot `static` at W=4 read **107,878 us of CPU against a 36,495 us wall**
  and `tbb` at W=4 **26,072 against 8,887**, 2.96 and 2.93 times their own walls
  on three CPUs, while the four W=1 serial rows sat inside half a percent of
  their own wall. `proc_pid_rusage` was tried between the two and rejected on
  the same kind of evidence: on the hosted `macos-14` runner of run 34667394566
  its figures read 0.02 to 0.07 times their own wall and hardly moved with the
  work, which is not a CPU figure and is not a unit error either. The wall clock
  stays the outermost pair and the two CPU reads are nested inside it, enclosing
  call execution; the nested pair was timed at 757 ns on the Linux host where
  that was measured (`RESULTS.md`, 2026-09-11),
  against per-call intervals of milliseconds.

  Those historical longer-call observations support cumulative worker-CPU
  coverage, not precise short-interval attribution. Darwin's live-thread API
  qualifies its totals as accurate only when suspended. The
  [retained-data diagnosis](../../investigations/compute-model/DESIGN.md#follow-up-diagnosis-of-retained-identical-image-variation)
  observes CPU deltas exceeding the host's physical interval capacity. These
  short deltas therefore cannot establish worker idleness or candidate CPU
  cost. The separate [fixed accounting diagnostic](../../investigations/compute-model/DESIGN.md#baseline-cpu-accounting-result-short-interval-attribution-failure)
  also violates that bound in gap counter differences, despite compatible
  enclosing and lifetime totals; accuracy at longer intervals remains open.
  This qualification concerns CPU
  interpretation; the clock choice, raw data and wall measurements are retained.
  This column is a measurement, never a pass/fail input.
- **`ratio`** is filled only on `wf` rows. It is the **median of within-pass
  matched pairs**: for each pass, WF's process median divided by the lowest
  process median among the parallel references at that same width, printed with
  its `[min-max]` and, on the verdict line, the modal name of the reference that
  supplied the minimum. **Recomputing a ratio from two printed medians does not
  reproduce this number**, and it is not meant to. `serial` and `wf-seq` live in
  the width-one block and never enter the ratio: a parallel row beating a serial
  row answers nothing.
- **`cpu_r`** is the CPU ratio, paired exactly as `ratio` is: within each pass,
  WF's process CPU median divided by the CPU median of **the reference that was
  fastest by wall in that pass**, and the median of those pairs. The two ratios
  are therefore about the same pairs and can be read side by side --- `ratio`
  says whether WF finished first, while `cpu_r` compares the reported CPU
  deltas subject to the attribution limit above. Pairing CPU
  against whichever reference happened to burn least CPU would answer a
  different question and would not line up with the verdict line. It sits beside
  `ratio` as a column rather than in `note` because it is the number the
  process-CPU target is read off, and a reader comparing it with the wall ratio
  should not have to cross the row to do it; the two extra columns take a data
  row to at most 141 characters -- the widest a run has printed, the `static`
  row carrying its own mark -- well inside the width the legend below was
  introduced to protect.
- **`lower`** is how many of the paired passes had WF lower, because a median
  can hide adverse pairs — a wall ratio of 0.999 with three of five pairs lower
  is on the record from the earlier bundle.
- **`steals`** is the median per-call `wf__par_grants()` delta on the `wf` row.
  A `wf` row with a zero steal median above width one is marked `no-lanes` in
  `note`, so a compiled program that did not split reads as that rather than as
  a slow scheduler. It is a mark, never a failure, and it must never be
  "repaired" by reaching for a split-budget environment variable: there is none
  in this compiler, and inventing one would stop the row being the plain-`--par`
  program the table is about.
- **`note`** carries the WF row's emitted chunk count, so a reader never has to
  recompute the split formula to know what was compared, and a reference's own
  fixed mark where it has one. Today the only reference mark is the `static`
  row's `excursions retained`: that reference has no stealing, its skewed-shape
  medians carry multi-millisecond excursions, and nothing in this bundle
  removes an outlier or discards a process.
- Each block ends with `BEST REFERENCE = ... FASTEST = ... WF fastest: yes|no`,
  and under that line a **legend**: one line per form in the block, in the same
  order as the rows above it, reading `  <form>: <grain policy>`. The policy is
  the string the binary itself prints and is **never truncated**, so a
  reference's policy and its role both survive into `table.txt`. The `wf` entry
  reads `compiler-chosen`, because it is. It is a legend and not a column
  because a column has to be as wide as the longest policy string — the
  `static` row's, which is a whole sentence — and that made every data row
  about 200 characters wide, past any terminal that has to read the numbers.
  Nothing is abbreviated to win that width back: the rows carry the numbers and
  the legend carries the sentences, once per block instead of once per row.

The reducer's exit status is nonzero only on a missing or malformed row — a
planned cell with no process, a wrong row count, a non-dense call index, a
`first` phase in the wrong place, a header/trailer mismatch. Never on a ratio,
never on spread, never on an ordering.

### Widths, and which block is the recorded one

The width set is `{1, 2, 4, 8, 16, 32}`. The harness reads the online CPU count
once per process and emits every width at most that count, plus the smallest
width above it when there is one; 16 and 32 exist for hosts with that many
logical CPUs (a 32-thread desktop records `W=32`) and change nothing on a
smaller host, which still emits its own widths and one oversubscribed block. A
width above the count is still emitted, but its header carries
`oversubscribed=1` and the reducer prints `n/a (oversubscribed)` instead
of a verdict: oversubscription rewards schedulers that yield and changes which
one wins, so such a block cannot answer this bundle's question.

**The recorded table for a host is its highest width block that is not
oversubscribed.** A four-CPU host runs 1, 2, 4 and an oversubscribed 8 and
records `W=4`; a three-CPU host runs 1, 2 and an oversubscribed 4 and records
`W=2`; an eight-CPU host records `W=8`. Neither hosted CI leg produces a
qualified `W=8` block; that needs a larger runner or a local host.

`explicit_shutdown` in each process trailer is 1 where the pool can be joined
and torn down (`static`, `parlay`) and 0 where it cannot (`tbb`'s arena,
Rayon's `use_current_thread` registry). `shutdown_ns` is timed after the batch
and is never inside a measured interval.

## Sizes, chunk counts and the sizing window

Each kernel has exactly one size constant. It is chosen so the Whitefoot loop
reaches a chunk count worth timing at the highest width the table records,
subject to two bounds: the `wf-seq` median at width one lands in **[5 ms,
60 ms]**, and the `wf` median at the highest recorded width stays **above
1 ms**, so fork, join and dispatch cannot dominate the thing being compared. It
is never chosen from the minimum admission threshold: a loop that is merely
admitted may still get two chunks. Which of the two terms in the rule below
actually sets a count depends on the work unit the runtime ships; at the
current one it is mostly the cap, as the note under the table says.

The compiler splits an independent map into `2^budget` chunks with

```
chunks = 2^floor(log2(min(16 * lanes, span / ceil(150,000 / weight))))
```

so the oversubscription term scales with the width, and the result is rounded
**down** to a power of two — 143 affordable chunks are 128 chunks.

The `affordable` column below is the work term alone, `span / ceil(150,000 /
weight)`; the per-width columns are the whole formula.

| kernel | shipped size | emitted weight | affordable | W=2 | W=4 | W=8 | W=16 | W=32 | reference chunks |
|---|---|---|---|---|---|---|---|---|---|
| mandelbrot | 98,304 points, limit 256, shape `trailing` | 219 | 143 | 32 | 64 | 128 | 128 | 128 | 1,536 at grain 64 |
| records | 131,072 records, `max_length` 255, shape `unicode` | 812 | 708 | 32 | 64 | 128 | 256 | 512 | 8,192 at grain 16 |
| fir | K=64 taps, N=524,288 outputs | 150 | 524 | 32 | 64 | 128 | 256 | 512 | 2,048 at grain 256 |
| quadrature | M=64 integrations, tol `0x1p-54`, depth 24 | none | n/a | n/a | n/a | n/a | n/a | n/a | frontier depth 8 |

**Which term binds, and therefore what a size constant still controls.** At the
shipped work unit of 150,000 the oversubscription cap `16 * lanes` is the
binding term for all three maps at W=2, W=4 and W=8, and for records and fir at
W=16 and W=32 as well; only mandelbrot returns to the work term, from W=16 up,
where its 143 affordable chunks sit under the cap and hold the count at 128. So
at the widths a four- or eight-CPU host records, these counts follow the lane
count rather than the size: a size step moves one only where it drops the
affordable count below the cap. The size constants therefore stand on the two
window bounds alone, and they are unchanged.

**`max_length` is 255, not 256, and that is a contract bound rather than a
transcription slip.** `summarize_records` in `tests/programs/compute/records.wf` carries
`requires input_count <= 16777216` on the byte buffer, which is proof-only
source evidence erased before lowering, so the C caller is what has to
establish it. At 131,072 records seeded 812381 a limit of 256 generates
16,799,739 bytes — 22,523 over that precondition — while 255 generates
16,698,303 bytes and leaves 78,913 bytes of margin, and it is the largest value
of the constant for which the shipped fixture satisfies the source's own
requirement. Nothing else moves: the record count, and therefore the chunk
count, is untouched, and `prepare()` re-derives the byte total from the batch
its generator actually produced and fails on it rather than trusting this
paragraph. The table header and the `workload` string both print
`max_length=255`, so a recorded row never disagrees with this line.

**The weights are measured facts, not constants.** They were read from modules
emitted by this tree's `whitefootc` on 2026-09-11, and the Makefile re-derives
each one from the emitted `--par` module at every build, into
`$(BUILD)/<kernel>_split.h`, so a codegen change moves the reported chunk count
instead of silently invalidating a number written here. Quadrature emits no
`wf__par_split_budget` call at all: it parallelizes by recursion structure and
has no minimum-size admission threshold. What it asks the runtime instead is
`wf__par_recursion_budget()`, once per call into its recursive component, for
the levels below which that component runs its sequential clone.

**Confirm the window on the host that records a table**, before recording it:
the `wf-seq` median inside [5 ms, 60 ms], the `wf` median above 1 ms at the
recorded width, and `steals > 0` on the `wf` row at every parallel width. If a
bound fails, change that one size constant by the smallest power-of-two step
that fixes it, re-derive the chunk count from the formula — the affordable
column and every width column — and record the new counts here and in the
table header. A step may leave the recorded counts unchanged, because the cap
binds below W=16; record what the formula gives either way. Nothing else
moves.

**The window was confirmed on the first host to record a table**, a
four-logical-CPU `x86_64` Linux machine with mask `0-3`, on 2026-09-11 at
compiler revision `33ed2c00`; the run is the dated section in
[`../../investigations/compute-runtime/RESULTS.md`](../../investigations/compute-runtime/RESULTS.md).
Its measured `wf-seq` medians at width one were **mandelbrot 26,674.2 us,
quadrature 16,398.7 us, records 36,113.0 us and fir 26,803.3 us** — all four
inside [5 ms, 60 ms]. Its `wf` medians at the recorded W=4 were 7,154.4 /
24,327.9 / 9,508.3 / 7,398.1 us, all above 1 ms, and every `wf` row reported
`steals > 0` at every parallel width. No size constant moved. These are that
host's numbers, not a property of the sizes: another host confirms the window
again before it records a table, and the specification's own derivation, on a
four-logical-CPU Intel Xeon 2.80 GHz Linux host, was `--no-overlap` 29.6-30.1 ms
for 65,536 Mandelbrot points at limit 256, scaling to about 45 ms at the shipped
98,304.

## Grain: a fixed policy on one side, the compiler's choice on the other

Each reference keeps **one fixed grain policy, not calibrated per host**. The WF
row's chunk count is whatever the compiler chooses. **The table compares
programs, not grains.** Per-host grain calibration is refused deliberately: it
would put a tuning loop in front of every recorded table and make two hosts'
rows incomparable.

| form | grain policy |
|---|---|
| `serial` | none: one thread, a loop over all callbacks. Width 1 only. |
| `static` | `equal contiguous partition, persistent helpers, no stealing: a regular-work reference, not a dynamic-scheduling ceiling for skew` — quoted verbatim from the string the binary prints, because that role is the half readers of this row keep getting wrong. Its helpers are heap-allocated at the first run and sized from `width`, there is one dispatch per call, and idle lanes busy-poll: that CPU is charged to this reference, not hidden. Its `note` column reads `excursions retained`. |
| `tbb` | oneTBB `parallel_for` with `auto_partitioner` and range grain 1, in a process-lifetime `task_arena(width, 1)` under `global_control`. |
| `parlay` / `parlay-left` | ParlayLib native scheduler, `parallel_for` granularity 1, fixed (not automatic) grain; right- and left-offer fork. |
| `rayon-join` / `rayon-join-left` | rayon 1.12.0 `join`, bisecting the chunk range to one callback per leaf — structurally the fork shape the compiler's splitter emits; right- and left-offer. |
| `rayon-iter` | rayon's own adaptive parallel-iterator splitting. Not a form of quadrature; see that kernel's block below. |

**What the three callback-grain numbers are, and what they are not.** They are
not per-host optima and no calibration was run:

- **Mandelbrot, 64 points per callback** is the old bundle's fixed-grain panel
  setting. That bundle's own per-shape optima for the timed `trailing` shape
  were **16** points per callback on its Linux x86-64 EPYC 7763 host (W=4 core
  median 316.817 us) and **256** on its Apple M1 Pro (227.162 us), over a sweep
  of 1/16/64/256/1024.
- **FIR, 256 outputs per callback** is a choice made here, with no measurement
  behind it at all. The only FIR grain evidence in the old bundle is a WF
  `tile_size` sweep of 257/4,096/65,536 whose best at K=64, N=262,144 was 4,096,
  and that knob belongs to a tile-tree representation this bundle does not carry.
- **records, 16 records per callback** is the one number with a same-shaped
  screen behind it: the old bundle's `group4`/`group16` comparison moved the
  result by about a factor of three in either direction depending on shape.

Both fork directions are reported for every library that has one. Fork direction
is a first-order effect on skewed input, not noise: at M1 depth 4 the reciprocal
ParlayLib ratios were 2.138 and 0.470. Keeping only the direction that matches
generated Whitefoot would flatter the WF row, and "the best reference" would
stop being the best reference.

The automatic-granularity ParlayLib variant is not carried in any kernel: the
old bundle's own grain panel records it 2.865x [2.773-3.071] slower than fixed
grain on trailing input in all five pairs.

## The two flag sets, and the asymmetry between them

This is the one place every compiler flag is written, quoted from the
`Makefile` with its longer comments trimmed:

```make
ROOT   := $(abspath ../../..)
WHITEFOOT_SCRATCH_ROOT ?= $(patsubst %/,%,$(if $(TMPDIR),$(TMPDIR),/tmp))/whitefoot
WORK   := $(WHITEFOOT_SCRATCH_ROOT)/whitefoot-compute-bench
BUILD  := $(WORK)/build
DEPS   := $(WORK)/deps
RESULTS ?= $(WORK)/results/$(shell date -u +%Y%m%dT%H%M%SZ)
WFC    := $(ROOT)/compiler/target/gate/whitefootc
FLOOR  := $(ROOT)/compiler/src/backend/wf_floor.c
SCHED  := $(ROOT)/compiler/src/backend/sched
CC     := /usr/bin/clang
CXX    := /usr/bin/clang++
ARCH   := $(shell uname -m)

# Never -march=native: Clang 18 infers an invalid AVX10 combination on one
# hosted runner and that failure is on the record.
BENCH_ARCH := $(if $(filter x86_64,$(ARCH)),-march=x86-64-v3,)
LOOP       := $(if $(filter x86_64,$(ARCH)),-falign-loops=32,)
TARGET_CPU := $(if $(filter x86_64,$(ARCH)),-C target-cpu=x86-64-v3,)

WARN   := -Wall -Wextra -Werror -Wpedantic
SCALAR := -fno-fast-math -ffp-contract=off -fno-vectorize -fno-slp-vectorize \
          -fno-lto
BASE   := -O3 -g $(WARN) -pthread $(LOOP) $(SCALAR) $(BENCH_ARCH)
CFLAGS   := -std=c11 $(BASE)
CXXFLAGS := -std=c++17 -DNDEBUG $(BASE)
RFLAGS := -C no-vectorize-loops -C no-vectorize-slp -C lto=off \
          -C symbol-mangling-version=v0 $(TARGET_CPU)

# Placement control for the gate's paired comparison, on both arms alike.
WF_ALIGN := $(if $(filter x86_64,$(ARCH)),-falign-functions=64 -falign-loops=32,)

# What whitefootc itself passes clang, plus the placement control.
WF_FLAGS := -std=c11 -pthread -O2 -Wno-override-module $(WF_ALIGN)

KERNELS ?= mandelbrot quadrature records fir
PASSES ?= 5
CALLS  ?= 5
WFB_GAP_US ?=
```

`-fno-lto` is repeated on every link line, not only on compiles.

**The two flag sets differ.** `WF_FLAGS` applies to `<kernel>-par.o`,
`<kernel>-seq.o` and the full Whitefoot native library — of the plain image and
of the twin alike, with the same base flags for both. `CFLAGS`/`CXXFLAGS`
apply to the C/C++ reference code, oracles and harness. The WF module and
runtime use the driver's `-O2` without `-march` or flags disabling vectorization. Native C/C++
references use `-O3` with vectorization disabled; their `-march=x86-64-v3`
override applies only on x86_64. Rust reference kernels also disable loop and
SLP vectorization. These differences have no uniform direction: WF can benefit
from vectorization that the reference flags prevent. The resulting comparisons
include code generation, representation and decomposition costs; they do not
establish competitiveness against optimized native implementations.

**`WF_ALIGN` controls placement within the WF A/B pair.** On x86_64 it appends
`-falign-functions=64 -falign-loops=32` to `WF_FLAGS`, so every Whitefoot C
runtime unit of **both** images starts its functions on a 64-byte boundary and
every Whitefoot translation unit its hot loops on a 32-byte one. The emitted
module is LLVM input, for which clang ignores `-falign-functions`, so until the
compiler aligned its own definitions
([code placement](../../investigations/code-placement/DESIGN.md#clang-does-not-align-llvm-input-with--falign-functions))
no kernel function was aligned by this variable; since then `whitefootc`
itself starts every emitted and runtime function on a 64-byte boundary, and
only `-falign-loops=32` is a flag the driver does not pass. `WF_ALIGN` exists for the regression gate's paired
comparison: the gate's two arms are separate images linked from separate
objects, their functions do not land at the same offsets even from
byte-identical sources, and under these flags **those translation units'**
internal alignment is identical **by construction** and the residual offset
difference between the arms is a multiple of the boundary rather than an
arbitrary number of bytes. It reaches no further into either image: the
harness, the kernel object, the references and the prebuilt libraries are built
at `CFLAGS`/`CXXFLAGS`, with `-falign-loops=32` and no function alignment, and
all of those but the kernel object are literally the same objects in both arms.
There is no way to set it on one arm; it is not an A/B handle and the three
handles above are not how to ask for it.

Two things it is **not**. It does not make code placement a solved confound —
with both arms aligned and one shifted by a deliberate 587-byte pad the twin
lines read no closer to 1.000 than unaligned, and the mechanism says why: the
pad cannot change any function's alignment modulo 64, so what still moves is
not function or loop alignment ([`RESULTS.md`](../../investigations/compute-runtime/RESULTS.md),
the same shifted null arm with both arms aligned). The verdict rule's
two-width requirement is what covers the rest. And it does **not** leave the
recorded scoreboard measuring what it measured before: the `wf` row of a table
recorded from the commit that added this variable onward is built at flags
`whitefootc` does not pass clang, which is a real change to what the row times
even though the measured cost of those flags on this bundle's development host
was inside [0.954, 1.010] across sixteen twin lines. `RESULTS.md`'s status
paragraph carries that as a dated note; no recorded table was touched.

Loop alignment is not cosmetic on either side of the link: code placement is
this bundle's largest confound, and six byte-identical kernel bodies at
different offsets once split a width-one median 7.3 ms against 10.7 ms with no
scheduler involved.

**Function alignment is another difference between WF and the references.**
On x86_64, function alignment reaches the Whitefoot runtime and, through its own
definitions, the emitted module, and no reference, oracle or harness. Its effect is part of each WF/reference
comparison; the bundle's historical placement results do not establish a
direction or size for that effect in a new image. The retained tables keep
their original flags and measurements.

**Remaining build differences.** On x86_64, `-march=x86-64-v3` reaches the C and
C++ kernels and no Whitefoot translation unit, and function alignment reaches
the Whitefoot units and nothing else. The separately built oneTBB
shared library gets only the three scalar flags, with no `-march` and no
alignment. The Rust staticlib gets only its four rustflags plus `target-cpu`
where `BENCH_ARCH` names it, and Rust's precompiled standard library is not
rebuilt with them. Library scheduler internals are therefore not
code-placement-matched with the kernels.

**Strict floating-point operations do not prohibit independent-lane
vectorization.** They preserve the required operation order and forbid
reassociation and multiply/add contraction; vector operations can still
compute independent lanes with those semantics. Inspection of the frozen
`6fdb6768` objects in the [2026-09-21 baseline](compute-baseline-2026-09-21.tsv)
finds packed `fadd.2d`/`fmul.2d` in WF stencil and packed multiplication followed
by ordered scalar additions in WF FIR. The corresponding retained native
kernel objects are scalar. The bounded
[2026-09-22 control](native-kernel-vectorization-2026-09-22.tsv) permits native
vectorization on these two fixtures and confirms packed arithmetic while
retaining their numerical contracts. It improves the native baseline without
isolating a WF scheduling or code-generation cause.
The bundle's 2026-09-11 description (`aefb2bee`) reported roughly ninefold SLP
gain for its bit-identical, lane-blocked FIR form. That observation remains
evidence about its historical fixture and toolchain, not these images or a
current native performance bound. That form is absent from the retained
scalar comparison; no old rows are rewritten by this clarification.

## The four kernels

**quadrature** — recursive, unequal subtrees. Adaptive Simpson over a left-peak
Lorentz profile, run as M=64 independent integrations **in sequence**, each one
internally parallel. That restriction binds the references too: a form that ran
the M integrations in parallel with each other would have replaced this kernel's
decomposition with an independent map, and the panel already has three of those.
`rayon-iter` is **absent by construction**: a parallel iterator needs an
enumerable range, an adaptive recursion has none until it is flattened, and a
flattened-frontier iterator is a different algorithm. It is not silently
dropped. `quadrature list` prints it as a row with class `n/a` and that reason
in place of a grain, `make compare` saves that listing as
`$(RESULTS)/logs/quadrature.forms`, and the cell list drops every `n/a` row, so
the form is disclosed and never run. It carries **no row in `table.txt`**,
because nothing timed it and this bundle does not put unmeasured rows in a
table of measurements. The fact is stated in `quadrature_bench.c`, about this
kernel's decomposition; the same rayon backend is a real reference for the three
independent maps, and nothing in the driver reads a form's name to decide it.
The tolerance cannot
be the size knob — the tree saturates at 149,831 nodes because `combined - whole`
underflows to exactly zero — and `depth` cannot be raised past the source's own
`requires depth <= 24`.

**records** — variable-length UTF-8 record batches, irregular per item.

> **The one compiled-Whitefoot records standing the old bundle left, quoted from
> `research/experiments/compute-runtime/README.md:900-908` as of commit
> `70aa8e5b` on branch `codex/compute-runtime` — that tree is not on this
> branch, so read it with
> `git show 70aa8e5b:research/experiments/compute-runtime/README.md`:** on an
> EPYC 7763 VM (two physical cores, four SMT logical CPUs, mask 0-3), at **256
> long-Unicode records**, the compiled Whitefoot program's warm core medians
> were **7,869.056 / 7,910.887 / 7,931.259 microseconds** at requested
> **0 / 2 / 4** lanes, against native `state` and `word` anchors of
> **7,205.072** and **5,230.540 microseconds**. **Every two- and four-lane
> sample in that cell reported no started pool and no steals.**

That is the record of a **refusal, not a result**, and it is not a prior for
anything here. The reason is the admission rule, not the scheduler: at weight
812 the work divisor is 1,478, so 256 records is far below the 2,956-record
threshold, the loop was never split, and the two parallel columns are the
sequential column measured three times. 65,536 records is the first size at
which the compiled program is asked the question at all, and the shipped 131,072
is the first at which it is asked it at the width the table records. The old
bundle has no other compiled-Whitefoot records standing.

**fir** — data-parallel loop, small tasks. `tests/programs/compute/fir.wf` is the one
Whitefoot source here that is not carried verbatim: the old bundle's
`fir.wf`/`fir_direct.wf` pair parallelizes by recursive tile halving into a
`box<FirTiles>` tree whose every sample is read through an accessor call, and
that bundle's own record is explicit that the accessor, not the scheduler, is
why the grouped WF form won every core pair and lost every cycle pair. The flat
form here makes the WF program's observable work the same as every reference's;
the per-output arithmetic and its operation order are preserved and checked bit
for bit, only the representation differs. **Consequence: no recorded FIR number
in the old bundle is comparable with anything here, and its tile form is not
carried.**

**mandelbrot** — loop with imbalance, timed on the `trailing` shape where the
interior points come last. Granularity, not the dispatcher, decides this row,
and which term sets the granularity has changed. Under the work unit of
1,200,000 this fixture could afford only 17 chunks, so `--par` emitted 16 at
every width — on `trailing` that is four heavy chunks, and at W=8 a ceiling no
lane count could lift; an eight-CPU host's W=8 wall equalled its W=4 wall
there, which is why the work unit moved to 150,000 (the scheduler's own note
beside `WF_PAR_SPLIT_WORK_UNIT`). At 150,000 the same 98,304 points afford 143,
so the count follows the lanes: 32 chunks at W=2, 64 at W=4, 128 at W=8, and
128 again at W=16 and W=32, where the work term binds. The size constant did
not move; it stands on the [5 ms, 60 ms] `wf-seq` window. **At W=8 the heavy
interior quarter lands in 32 of those 128 chunks, four per lane, which is a
property of the fixture**, and this block says so rather than letting a reader
take a chunk count for a scheduling result. The `static` row here is a strong
**regular-work** reference and not a dynamic scheduling ceiling for skew — which is exactly why the skewed
shape is the timed one — and its medians in the old bundle carried
multi-millisecond excursions with 65-117 involuntary context switches.
**Excursions are retained, never discarded**: there is no outlier removal and no
discarded process anywhere in this bundle.

## Equality

Bit-for-bit against one independent oracle, per call, after the timer stops.
Never a tolerance, never one implementation against another — comparing every
implementation to the same oracle makes cross-implementation equality transitive
and exact for free. No expected count, PASS string or fixture total is stored:
every count is re-derived at run time and printed, and `verify` requires only
that something was compared.

**One verify fixture per flat-map kernel is above the split admission floor.**
The linked scheduler splits an independent map only from
`2 * ceil(150,000 / weight)` iterations upward, and the smaller fixtures in
Mandelbrot's and FIR's grids sit below that floor at every width (at the
earlier work unit of 1,200,000 every grid fixture did). Without a fixture the
floor is known to admit, the `--par` module could take its unsplit path through
the whole of `verify`, and the chunked path the table times would never be the
path the oracle checks. So each of those two kernels carries **one extra
fixture** sized from the floor its own emitted module implies: nothing is
stored, a change of weight moves the fixture rather than quietly dropping it
back below the floor, and `verify` prints the weight, the floor, the size and
the chunk count the run produced —

```
# mandelbrot split fixture: weight=219 floor=1370 points=1408 chunks=2
# fir split fixture: weight=150 floor=2000 outputs=2048 chunks=2
```

— `chunks=0` at width one, where the scheduler does not split at all. This is
the one place the two verify grids go beyond the fixture lists the bundle's
implementation specification names.

The timed interval is identical for every implementation of a kernel:
**allocate the output buffer, zero-fill it, compute, join every participant.**
Release and free are outside it. The allocation and the zero-fill are inside
because Whitefoot's `buffer_new(count, 0)` allocates and zero-fills, so every
reference calls `calloc`, or `malloc` plus `memset`, inside the interval. A
re-cut that quietly lets the native path allocate uninitialized compares
allocators, not schedulers.

## Removal conditions

Every file here serves one of: a kernel's Whitefoot program, a kernel's oracle,
a reference implementation, the harness, the reducer, the build, the regression
rule, or the record. If a reference is ever judged uninformative, its
`backend_*` file goes and the table loses one row. If a kernel is ever dropped,
its `programs/*.wf`, `*_bench.c` and `*_host.ll` go together. `verdict.awk` and
`verdict-test.sh` go together with `.github/workflows/compute-regression.yml`,
whenever that check stops being worth its runner minutes; the `WF_B_*` variables
go with them, because nothing else builds a twin from another tree. The bundle
goes when the question at the top of this file stops being worth asking.
Nothing here outlives its row.
