# The prior compute-runtime bundle

The current-stack compute runtime was selected on 2026-09-09 from a comparison
that is not in this tree. The implementation landed; the justification did not.
This file carries what that bundle decided and measured, so the branch holding
it can be deleted without the reasoning going with it.

Everything below comes from `codex/compute-runtime@70aa8e5b` (pull request 28),
which carried `research/investigations/compute-runtime/` — `README.md` at 3,890
lines, `PURE-COMPUTE-COMPARISON.md`, `BASELINES.md`, `WORKLOADS.md` and a
660-row comparison table — together with a 76-file
`research/experiments/compute-runtime/` measurement bundle and a restorable
checkpoint of the retired unified scheduler. Each section names the document and
heading it comes from. Measured revisions are quoted where the source records
them; they are text here, not reachable objects. The branch head `70aa8e5b` is
one parent of the archive commit on `archive/codex-io-2026-09-11`, so it and
everything it reaches stay reachable after the branch name is deleted.

[mcts_mem/whitefoot/parallelism.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/parallelism.md) already records the outcome — the
2026-09-10 boundary entry retiring the park-on-miss scheduler, and the
measurements that followed it. What it points at for the 2026-09-09 selection
("the owner reviewed the comparison") is the document reproduced below.

## The decision this bundle justified

The owner reviewed the pure-compute comparison and authorized production
integration on 2026-09-09: take historical main `9051576f`'s ordinary worker
stacks with current-stack join, help and steal as the base, with atomic deque
cells, corrected thief ordering and startup and lifetime fixes, delivered
through ordinary `whitefootc` in `compiler/`, as one runtime for all programs.
The constraints attached to that direction are the part worth preserving,
because they are what the implementation is accountable to:

Compute workers must not run arbitrary may-suspend user calls. Such calls
execute ordinarily; direct typed I/O operations still submit and join through
the existing io_uring, IOCP or helper backends, waiting on the current stack. No
managed-stack pool, continuation migration or ready queue belongs in the compute
path, and the public ordinary-call ABI and proof judgments stay unchanged. This
deliberately retires generic staged user-call concurrency: network fanout
needing independently suspended activations is deferred, while sequential TCP,
direct independent I/O submission, result and error handling, cleanup and
exhaustion remain required. Tests of the retired mechanism must be replaced or
retired with that explanation, never silently treated as equivalent
functionality. Existing workload and reference panels were to be reused for
five-target native qualification rather than a second benchmark framework being
built, and all-platform performance qualification was explicitly left open.

*Source:* `codex/compute-runtime@70aa8e5b`,
`research/investigations/compute-runtime/README.md`, *Current owner direction
(2026-09-09)*.

## The two pure-compute runtimes compared

The question was narrow and stated as such: compare the last main POSIX compute
runtime before shared compute and I/O scheduling
(`9051576f:compiler/src/backend/par_runtime.c`, 947 lines) with the recovered
research runtime (`d858008f:research/experiments/compute-runtime/runtime.c`, 659
lines), holding the emitted object and scalar arithmetic fixed. The report is
explicit that the recovery cites `fee3356`, which is not an ancestor of
`9051576f`, so no linear ancestry should be invented; the comparison is of file
bytes.

They share the same central scheduler. Task representation is 256-byte frames
with 16-byte alignment and 64 owner-local slots per lane in both; the owner
deque with the owner's newest task called directly by join is the same; a
stolen-task join runs local work and then steals on the current stack in both;
idle behaviour is 4,096 searches, then 16 yields, then a condition wait in both;
and the loop budget is a work floor of 1,200,000 with up to 16 chunks per lane
in both. Four differences matter. The research file uses relaxed atomic accesses
for ring-pointer cells where the historical file uses plain pointer accesses,
repairing a delayed-thief and ring-reuse data race. Both use acquire loads for
the thief index and **both need an ordering repair before adoption**: with only
acquire reads a later thief can observe a new top with an older bottom after an
owner pop and another thief advance, so the owner and that thief can both
believe they own the same task. The research file initializes the owner wait
station before workers and cleans up on failure, which is easier to justify than
initializing it after. And the historical file keeps a shared successful-steal
counter always enabled, where the research file has an optional counter, event
hooks and budget experiments — useful evidence controls that do not belong in a
production core.

The measurement boundary is stated rather than assumed. A comparison-only patch
gave both scratch copies sequentially consistent thief top and bottom loads, and
gave the old copy atomic ring cells and read-only observer adapters; no formal
compiler or frozen research runtime was edited. So the timings compare **old
with repairs** against **research with repairs**, not unsafe original bytes, and
the report says plainly that stress-test success is not a C11 ownership proof
and that startup-failure behaviour still differs and is not covered.

*Source:* `codex/compute-runtime@70aa8e5b`,
`research/investigations/compute-runtime/PURE-COMPUTE-COMPARISON.md`, *Exact
comparison inputs*, *Architecture and quality*, *Correctness and measurement
boundary*.

## What the comparison measured

At `4ff01e88` all four pure-comparison CI jobs passed their build, output,
participation and complete-matrix checks across 4,400 processes: Linux x86-64
(EPYC 7763, two cores and four SMT threads, 1,200 processes), Linux AArch64
(Neoverse-N2, four cores, 1,200), macOS AArch64 (virtual M1, three CPUs, 800)
and macOS x86-64 (i7-8700B, four exposed CPUs, 1,200). Every artifact archive
digest matched, all 43 source, object, compiler and binary hashes per artifact
matched, and recomputing each summary from raw process data reproduced it
exactly. The complete 660-row process comparison table is retained on the branch
as `pure-comparison-4ff01e88.tsv`.

Most coarse cells are close. FIR warm core medians of five paired process
medians at 16 taps and tile 64, old divided by research, are 1.0066 and 0.9821 on
Linux x86-64 at 4,096 and 65,536 outputs, 0.9035 and 0.9963 on Linux AArch64,
0.9926 and 1.0014 on macOS AArch64, and 0.8901 and 1.1357 on macOS x86-64 — with
identical-image replica ranges wide enough on both macOS targets to prevent a
ranking there. Mandelbrot whole-process examples at 65,536 points and 16
repetitions are within a few percent everywhere: 1.0064 and 1.0012 on Linux
x86-64 for the plane and interior-first shapes, 0.9778 and 1.0012 on Linux
AArch64, 1.0025 on macOS AArch64 and 1.0703 on macOS x86-64.

There is one real exception, and the report refuses to smooth it. On Linux
x86-64 at 4,096 outputs, **tile 16**, four participants, old divided by research
is 0.7928 [0.6749, 0.8575] on core time, 0.8125 [0.7746, 0.8775] on process
wall, 0.8113 on CPU and 0.9876 on RSS, with 1,645 fewer process context switches
in the median paired delta, and all five pairs favour the historical file. The
identical-image replica ranges are 0.8526..1.1190 on core and 0.9454..1.0558 on
wall, so the host is not perfectly stable, but the advantage is larger than that.
The same machine's 4,096/tile-64 case has nearly equal core time yet old/research
process wall 0.9142 [0.8793, 0.9479] and CPU 0.8539, with the batch interval that
excludes process launch also favouring old at 0.9108 and the warm full-call cycle
ratio at 0.8714 — so startup alone does not explain it, and reporting only the
core interval would erase the result.

Object inspection constrains the explanation usefully: in the Linux x86-64
artifact the unrelocated instruction bytes of the publish, join, release,
worker-main and split-budget functions **match** between old and research, while
their positions, relocations and startup differ. The FIR difference is therefore
not evidence that the research file introduced a different join or steal
algorithm; layout, startup state and host interaction remain possible causes and
none is proven.

Two limits are stated and are worth carrying. Counter-off core ratios on Linux
x86-64 are 1.0148 and 0.9965, and the small macOS ARM counter-off cell is
0.8457 [0.6430, 0.8935] with a wide identical-image range — so no counter policy
is selected by this panel. And both candidates share a visible policy limit:
every 4,096-point Mandelbrot case leaves the pool unstarted even when four
participants are requested, the research whole-command median being 58.273 ms at
one participant and 58.066 ms at four, while at 65,536 points the same shape does
start the pool and scales from 885.273 ms to 239.308 ms. That is evidence to
revisit the shared split-cost estimate after choosing a runtime base, not
evidence that one candidate has a better deque.

*Source:* `codex/compute-runtime@70aa8e5b`,
`research/investigations/compute-runtime/PURE-COMPUTE-COMPARISON.md`,
*Performance evidence*, *Four-target native CI*, *Local Apple Silicon diagnostic
cohort*.

## Native results after restoration

Revision `2861607f` restored the maintained core exactly to `4fabd264` while
retaining an exhaustion-floor repair. Its canonical check passed, as did all
twelve gate jobs, both completion jobs and all four I/O benchmark jobs, and the
compute cohort completed all five native compiler and ordinary-command
correctness steps. Replay verifies 2,940 formal processes with 6,776,700 checked
FIR calls and 7,620 whole-command processes; captured sources, previous headers,
repaired controls, raw results and original reducers match, and the ordinary
compiler images embed the maintained scheduler, platform and exhaustion-floor
sources. The report is careful that these establish execution and artifact
integrity, not performance parity.

**The clearest remaining runtime loss** is Linux x86-64 FIR at four workers,
4,096 outputs, tile 64. Against the repaired research control, candidate and
replica full-call medians are 1.0631 and 1.0532 and batch CPU medians are 1.0710
and 1.0641, with all ten full-call and CPU pairs losing, while candidate core
time is 0.9998. Core parity alone therefore misses the full-call cost; these
measurements include result handling and checks and do not isolate scheduler
CPU. Linux ARM does not reproduce the loss, macOS timing variability prevents a
resolved ranking, and Windows has no executed research-runtime control.

**The grain panel** is the larger whole-program gap, and it is the most
actionable number in the bundle. On small, all-interior Mandelbrot inputs with
scalar arithmetic, the same input and recurrence and the same participant
budget, five-pair median wall ratios of the compiled Whitefoot program against
static partitioning at 4,096 points, 32 repetitions and iteration limit 256 are:

| Native target | Participants | Default / static | Split-work 60000 / static |
| --- | ---: | ---: | ---: |
| Linux x86-64 | 4 | 3.3209 | 1.3001 |
| Linux ARM | 4 | 3.7076 | 1.5168 |
| Mac Intel | 4 | 1.9271 | 1.1260 |
| Mac ARM | 2 | 1.9159 | 0.9980 |
| Windows x86-64 | 4 | 1.8033 | 0.8913 |

The right-hand column changes only the split-work threshold on the same image
and is a diagnostic, not a proposed default. Separate diagnostics report zero
started helpers for every default row, which is the mechanism: the publication
threshold refuses to split this input at all. The report states that macOS A/A
variability prevents treating those medians as qualified rankings, that static
partitioning is a regular-work reference and not a ceiling for skewed work, and
that a smaller threshold improves these inputs without selecting a generally
best publication policy. Linux x86-64's roughly two-millisecond all-exterior
command separately has candidate/previous wall medians 1.0619 and 1.1013 with an
A/A range of 0.7570..1.3367, left unresolved rather than dismissed.

**The screens that remain red** are listed rather than buried. The original
formal long and first64 screens report investigate cells at Linux x86-64 3/120
and 27/120, Linux ARM 0/96 and 3/96, Mac Intel 39/120 and 49/120, Mac ARM 17/72
and 26/72, and Windows 3/72 and 9/72. Every ordinary-command screen also remains
red, and quadrature's formal/research parity screen remains red after its
correctness checks pass, its center-peak/leaf-sequential/four-worker cell having
wall and CPU medians 1.0695 and 1.0448 with a wall range of 1.0024..1.1554. No
screen or threshold was relaxed.

*Source:* `codex/compute-runtime@70aa8e5b`,
`research/investigations/compute-runtime/README.md`, *Native results after
restoration*.

## The recursion frontier

This is the bundle's strongest positive compute result, and it reached the
normal compiler.

The question came from a sampling screen on adaptive recursive quadrature, where
generated join and thread-local lookup symbols were frequent in sampled stacks
and a native matched-grain form at spawn depth eight had generated-leaf-relative
wall ratios of 0.666 and 0.659 while depth 24 had 1.061 and 1.091. Sampling
itself raised paired median wall by 9.7 to 19.4 percent and all 24 sampled runs
were slower than their plain pairs, so the screen motivated a control rather
than assigning a cost to a symbol.

A private control then took the actual emitted leaf-host module, selected the
sole self-recursive function containing a compute offer, and created internal
function and callback layers at limits 4, 8, 12 and 24, with both direct
recursive calls and the published callback entering the next layer and the
frontier entering the existing same-signature sequential clone; arithmetic,
depth and convergence tests, operand order, the 88-byte frame, null fallback,
join and release were preserved, and exact reversal of all 48 layer pairs was
checked. At depth 8 against stock, generated wall falls from 15.750 to 10.099
microseconds on the center peak, 14.173 to 8.824 on the left peak, 12.910 to
8.205 on the right peak and 26.379 to 16.613 on the depth cap — paired ratios
0.657, 0.633, 0.658 and 0.640, winning all five wall and CPU pairs on every
input, with CPU ratios 0.661, 0.607, 0.634 and 0.623. Against the same image's
Parlay-left depth-8 reference the wall ratios are 1.009, 0.915, 0.996 and 0.945
with 2, 5, 3 and 4 of five pairs faster: parity, not a sweep. Depth 4 favours the
depth-cap input but loses all five Parlay pairs on both skewed inputs, and depth
12 loses all five on every input. The control is honest that it combines grain
selection with static specialization, inlining and image layout — stock, depth-8
and depth-24 instruction-section sizes are 33,156, 37,228 and 46,508 bytes — and
that depth 24 retains every original offer opportunity for these fixtures, so
its 0.951/0.919/0.979/0.928 ratios against stock cannot be credited to fewer
offers.

The normal compiler then exposed the same transformation as an opt-in control,
and it reproduced the benefit through ordinary output: frontier against
generated leaf gives wall 0.648, 0.630, 0.639 and 0.652 on the four inputs —
paired median reductions of 34.8 to 37.0 percent, winning all five pairs on every
input — with CPU ratios 0.589, 0.593, 0.610 and 0.643. Against Parlay-left depth
8 the CPU ratios are 0.847, 0.862, 0.901 and 0.922 with 5, 5, 4 and 5 lower
pairs, while the wall comparison stays mixed. The task accounting is exact: the
center-peak frontier run publishes 247 tasks, and under full owner-slot
exhaustion it reports zero publications and exactly 247 refusals, checked
against an independent explicit-stack oracle's internal node count above the
frontier. Single-worker frontier-sequential against leaf-sequential wall ratios
are 0.997, 0.993, 1.009 and 1.001 with no uniform speedup, and with a worker
request of four those sequential forms still start no pool.

The Linux replication holds: plain frontier against generated leaf gives wall
0.659, 0.683, 0.681 and 0.655 and CPU 0.637, 0.646, 0.631 and 0.637, every cell
improving in all five pairs, for paired wall reductions of 31.7 to 34.5 percent;
against Parlay-left depth 8 the wall ratios are 0.866, 0.828, 0.887 and 0.938.
The limits are stated: a native matched-grain form at depth 8 remains a
near-parity comparison (plain right wall 1.006 with two lower pairs, depth cap
0.995 with three), Parlay-left depth 4 consumes less CPU on right skew in all
five pairs despite frontier's lower wall (1.247 plain), all four software events
report 100% running while six requested hardware events are unavailable so no
instruction-level attribution exists, and no fixed depth is selected as a
default.

*Source:* `codex/compute-runtime@70aa8e5b`,
`research/experiments/compute-runtime/README.md`, *Private generated recursion
frontier*, *Compiler-generated recursion frontier*, *Linux compiler-frontier
replication*.

## The scalar-leaf offer control

This is the calibration that did land: the compiler's scalar-leaf limit of 16
under `--par`. After the normal checks and lowering it identifies one-block
returning functions with scalar parameters and results, no drops and only
constants or scalar arithmetic, boolean, conversion and reinterpretation
operations, excluding any function with a call, memory operation, control-flow
edge, aggregate or loop; constants do not count toward the limit. It removes
only selected handed-out members from already-permitted groups, keeps the
original source-last join site and drops singleton groups, so every source call
stays at its original position with no worker or result lifetime change.

For quadrature this removes the density and Simpson offers while preserving the
recursive pair and its 88-byte task frame: the filtered instrumented centered-peak
run publishes 1,643 tasks against 8,219 in the original, with identical per-node
work and the exact binary64 result. The five-form screen on Apple Silicon, in
microsecond medians over five process warm means at a requested width of four,
gives sequential, original parallel and filtered parallel of 21.844/39.547/16.042
for the center peak, 16.380/32.469/13.359 for the left peak,
16.281/32.276/12.735 for the right peak, 3.375/10.792/4.875 for the outside peak
and 52.547/77.292/26.313 for the depth cap. Against single-worker sequential the
filtered four-worker paired ratios are 0.734, 0.819, 0.785 and 0.499 on the four
heavy inputs, faster in all five pairs; against the original four-worker form
they are 0.400, 0.411, 0.382 and 0.332, also all five.

The Linux cohort is the reason this is a provisional cost choice and not a
universal one: leaf at four workers improves over the original in all five pairs
for every case, but against single-worker sequential the centered, left and right
peaks and the depth cap give 1.094, 1.115, 1.095 and 1.115 with only 1, 0, 0 and
1 pairs improving — center medians 32.521 microseconds sequential, 81.990
original parallel and 35.600 filtered parallel. The Apple Silicon speedup over
sequential does not repeat there, and no operating-system cause was assigned.
The frame evidence also blocks the simple explanation: the filtered recursive
activation allocates 160 bytes against 144 original and 128 sequential, so the
improvement is not a smaller stack frame, and suppression changes inlining and
register allocation too.

*Source:* `codex/compute-runtime@70aa8e5b`,
`research/experiments/compute-runtime/README.md`, *Scalar leaf offer control*.

## The inventories, and the restorable checkpoint

Two documents on the branch are inventories rather than results, and both state
gaps that are still open. `WORKLOADS.md` owns Whitefoot-side workload selection
and says the thing worth repeating: a small tree-layout example is a regression
sample, and neither a larger tree nor more repetitions turns it into
representative application coverage. It carries an implemented family — a
variable-input causal FIR with output and parallel-execution qualification, a
variable-length UTF-8 record batch, Mandelbrot point rendering and adaptive
recursive quadrature — and five application rows that remain source seeds or
explicit gaps: in-memory text search and record processing, independent raw
stream decoding, batched signal and image processing, ordering and grouped
aggregation, and irregular traversal or spatial-query batches. Its evidence
ladder is source seed, implemented program, qualified correctness and
actualization, then measured application with native references, and it states
that a geometric mean or a win count cannot erase a regression.

`BASELINES.md` owns the reference matrix: optimized native serial and a
hand-tuned static partition as mandatory anchors, Rayon, OpenCilk and oneTBB as
mandatory dynamic references, ParlayLib, Taskflow and Go as additional
candidates, each row carrying a mechanism hypothesis and a discriminating
measurement rather than a ranking. Its standing cautions are that a deliberately
scalar loop is only a diagnostic when vectorization is legal, that static
division cannot repair unknown skew, that Parlay must have its native backend
forced and recorded because the same interface can select other backends, and
that support beyond x86-64 is an explicit qualification gap there.

The branch also carries
`research/investigations/io-model/UNIFIED-RUNTIME-CHECKPOINT.md` and a 4,704-line
patch that reconstructs the retired unified scheduler from base `30168426`,
verified to rebuild all 307 compiler files. That is the only restorable form of
the runtime the I/O experiments measured. It is not carried into this tree: the
scheduler is retired, the digest at
`research/investigations/io-model/SCHEDULER-FINDINGS.md` records what it
established, and a patch that reconstructs a retired runtime is an artifact to
leave reachable on the archive branch rather than a file to maintain here.

*Source:* `codex/compute-runtime@70aa8e5b`,
`research/investigations/compute-runtime/WORKLOADS.md` and `BASELINES.md`;
`research/investigations/io-model/UNIFIED-RUNTIME-CHECKPOINT.md`.

## What the new scoreboard replaced, and why it is not carried

`research/experiments/compute-bench` answers one question — for each kernel, at
each width, is the Whitefoot program built by this tree's compiler with plain
`--par` the fastest thing in the row — and it deliberately keeps nothing from
this bundle that cannot serve that question. The reasons it gives are the reason
the bundle's measurement files are not salvaged here:

Its `wf` row is the module the compiler emits under plain `--par --emit-llvm`
and no other flag, linked with the scheduler and floor sources from the same
tree through a host adapter of at most eighteen lines of LLVM IR. **No research
copy of the runtime is carried**, and no C adapter to any runtime is labelled
`wf`. That retires the bundle's whole historical-control layer, including the
frozen research runtime and the pure-comparison harness built on it: a number
taken through a runtime that is not the tree's runtime is not a number about the
tree's compiler, whatever else it is good for. The comparison reproduced above
keeps its value as the record of a decision, not as a standing measurement.

The bundle's FIR representation is not carried either, and the scoreboard says
why: the old `fir.wf`/`fir_direct.wf` pair parallelizes by recursive tile
halving into a boxed tile tree whose every sample is read through an accessor
call, and the bundle's own record is explicit that the accessor, not the
scheduler, is why the grouped form won every core pair and lost every cycle pair.
The flat replacement makes the program's observable work the same as every
reference's, preserving the per-output arithmetic and its operation order bit
for bit. **No recorded FIR number in the old bundle is comparable with anything
in the scoreboard**, which is why this file quotes the FIR results as evidence
about two runtimes rather than as a standing kernel result.

The bundle's one compiled-Whitefoot records standing is carried into the
scoreboard only as a refusal. At 256 long-Unicode records on an EPYC 7763 VM,
warm core medians were 7,869.056, 7,910.887 and 7,931.259 microseconds at
requested zero, two and four lanes against native anchors of 7,205.072 and
5,230.540, and every two- and four-lane sample reported no started pool and no
steals — because at split weight 812 the work divisor is 1,478 and 256 records
is far below the threshold, so the loop was never split and the two parallel
columns are the sequential column measured three times. The scoreboard records
that as a refusal rather than a result and sizes its own inputs above the
threshold.

Also not carried: the bundle's diagnostic layer (the scheduler and event traces,
protocol-cost and thread-placement probes, the selector), every calibration
script and its result table, the whole-process command panel, and the
4,400-line experiments README itself. The scoreboard's own removal rule is the
principle to keep: every file serves a kernel's program, a kernel's oracle, a
reference, the harness, the reducer, the build or the record, and nothing
outlives its row.

*Source:* `compute/scoreboard@c0ff6d4e`,
`research/experiments/compute-bench/README.md`, *What "WF" means here, and what
it does not*, *The four kernels*, *Removal conditions*;
`codex/compute-runtime@70aa8e5b`,
`research/experiments/compute-runtime/README.md` for the quoted records standing.

## Open after this record

**Were the bundle's red screens resolved by what landed?** The branch records
that every ordinary-command screen remains red, that the formal long and first64
screens report the investigate-cell counts listed above, and that quadrature's
formal/research parity screen is red after its correctness checks pass, with the
center-peak/leaf-sequential/four-worker cell at wall and CPU medians 1.0695 and
1.0448. Whether the integration that landed as pull requests 32 and 33 resolved
those cells, or whether they remain open against the current runtime, is not
answerable from the branch text: its screens compared the candidate against a
frozen research control that is no longer the tree's runtime.
`research/investigations/io-model/RESULTS.md` covers the Windows stability
criterion but not these compute screens. Settle it by rerunning the surviving
kernels through `research/experiments/compute-bench` against the current
compiler and recording which cells still investigate, rather than by carrying
the old screens forward.

**Does the grain panel still hold?** The default-against-static wall ratios of
3.3209, 3.7076, 1.9271, 1.9159 and 1.8033 on small all-interior Mandelbrot
inputs were measured against a split-budget policy whose threshold knob does not
exist in this tree's compiler. The mechanism — a publication threshold that
refuses to split a small input, leaving every parallel column equal to the
sequential one — is a compiler policy the tree still has, and the scoreboard's
sizing window exists precisely because of it. Settle it by measuring the same
shape at sizes above and below the current threshold with the current compiler,
and recording the threshold's effect rather than a tuned value.

**Is the thief-ordering repair in place?** Both frozen runtimes needed a
sequentially consistent thief top and bottom load before adoption, and the
comparison applied it only to scratch copies. The integrated runtime should be
checked against that requirement directly; stress-test success is not a proof,
and the comparison never claimed one.
