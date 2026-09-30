# Rust and C++ container comparison and optimization

## Question and scope

How does the current Whitefoot library perform against ordinary production
Rust and C++ containers when they complete the same application task, and
which observed differences deserve the next investigation? The follow-on
optimization asks whether each meaningful workload cell can run repeatably
faster than the slower ordinary Rust or C++ standard-library counterpart.

The comparison was framed after the container delivery at `0f22b026b`; its
implementation now incorporates main `ad51e05df`. No timings were published
against the earlier baseline. Before timing, the benchmark implementation
was frozen at source revision
[`0c3203aa6111f14247aa950e3794e83082d4f29c`](https://github.com/mbbill/Whitefoot/tree/0c3203aa6111f14247aa950e3794e83082d4f29c),
which identifies the timing and allocation executions. Later report,
data-packaging and reducer commits do not change that identity. The map
replay relinked after removing one trailing
blank line in its shared oracle; that recorded source-file correction changes
no behavior. That baseline comparison selected no language amendment
or library algorithm change. Existing C controls remain attribution tools,
not an asserted performance ceiling. The sources, commands, toolchain
identities, raw samples, and qualifications below travel with any ratio.

| Whitefoot family | Rust baseline | C++ baselines |
|---|---|---|
| GrowVector | `Vec` | `std::vector` |
| Deque | `VecDeque` | `std::deque` |
| HashMap | `std::collections::HashMap` | `std::unordered_map`, `absl::flat_hash_map` |
| PriorityQueue | `BinaryHeap` | standard heap algorithms over `std::vector` for returned move-only owners |
| OrderedMap | `BTreeMap` | `std::map`, `absl::btree_map` |

The first comparison reuses the five existing family drivers and their
independent behavior oracles. Slab, independently retained membership, and
the indexed composite require different comparison APIs and are outside this
first matrix. These synthetic traces provide discriminating costs, not a
real-application workload-frequency distribution.

The existing priority-queue traces consume the removed owner. C++
`std::priority_queue::top` exposes a const reference and `pop` returns nothing;
the native owning baseline therefore uses `make_heap`, `push_heap`, and
`pop_heap`, without implementing a private sift algorithm. Its two repairs for
replacement remain an API/algorithm difference. The existing raw-storage
setup/cleanup path does not heapify and is reported as a storage control,
outside the priority-queue ranking.

The deque's existing explicit rebase is not a common application operation:
`std::deque` has no corresponding reserve/rebase guarantee, and that old trace
does not use the extra capacity. A new common growth trace instead fills the
initial population, appends beyond it, and consumes all added values. Whitefoot
explicitly rebases its full deque; native deques use their ordinary growth.
Same-capacity hash rehash has no portable Rust standard-map operation and stays
outside the common ranking; reserve-for-more-entries is compared separately.

## Comparison contract

Practical comparisons preserve the requested application outcomes while
allowing each library its ordinary algorithms, representations, capacities,
growth policies, and optimized APIs. Do not implement the Whitefoot algorithm
inside a Rust or C++ wrapper and call it a standard-library comparison. Record
reference stability, iteration order, ownership of replaced/removed values,
logical capacity limits, and cleanup obligations for each trace. An adapter
needed to deliver the requested outcome belongs in its cost; an outcome that
the application does not require must not be imposed just to mimic Whitefoot.

Small scalars and wide inline values are separate cases. Wide bytes alone do
not establish nested-owner performance. Do not add a per-element allocation
to a competitor merely because its value is large. All consumed results must
contribute to the independent oracle, and every allocation must be reclaimed.

The existing map keys are `u64`. Returning an old value while retaining an
equal stored key produces the same outcome for those keys, but does not
establish equivalent behavior for distinct, comparator-equal owning keys.
Replacement at a logical ceiling must still succeed; use entry-style APIs
below the ceiling where available instead of imposing a redundant lookup.

Normal optimized builds provide the practical ranking. Retained public-helper
builds, optimized IR, and existing C variants are attribution evidence, with
their changed visibility and ABI conditions stated. Do not infer a causal
percentage by subtracting unrelated whole-trace timings.

Hash maps need two separately labelled questions: the ordinary default hasher
and an aligned hash calculation for attribution. Neither a cheap integer hash
nor a randomized default should silently stand in for the other. Different
table layouts and load policies remain visible in both series.

## Optimization criterion

The baseline at `c75520e9d` is retained unchanged. Optimize Vector, Deque,
HashMap, PriorityQueue and OrderedMap in that order, preserving their complete
traces, ownership outcomes, cleanup and independent oracles. For each payload,
operation and population separately, the target is WF elapsed time below the
slower of the Rust and C++ standard-library times. Select that peer by its
median within each order cohort; it may differ between cohorts. Abseil is an
additional reference, never an alternative denominator chosen for this target.
The six Vector suffix-zero and six priority storage-control cells remain
unranked, leaving 198 application cells. No family average replaces a cell.

Before candidate measurements, use this conservative sufficient test: in both
cohorts, the largest WF sample must be below the smallest sample of the
median-slower standard-library peer. Both selected comparisons must also pass
the existing duration and cohort-stability qualifications. This observed-range
test is not a confidence interval; outliers remain in the data. A strict median
win without range separation is descriptive, not a completed target. A robust
deficit has the smallest WF sample larger than the largest sample of
either standard peer in both cohorts. All other qualified cells remain
inconclusive. If close cells need a less conservative discriminator, record a
fresh paired A/A noise experiment and its selection criterion before running
it; do not choose a threshold after seeing a favorable ratio.

Each optimization needs a before/after comparison with the same caller,
inputs, toolchain, flags and harness, a predicted change in code or work, and
a falsifier. Inspect final optimized code before attributing source-shaped IR
copies to runtime cost. Rerun affected correctness and allocation observations
before timing; keep build, execution and measurement costs separate. General
compiler changes require regression cases outside research. Library changes
must use the ordinary public path, without benchmark-specific dispatch or
weaker proofs. Preserve baseline artifacts and samples rather than overwriting
them with a candidate.

Practical default-policy comparisons remain visible. HashMap attribution also
uses aligned hashing, actual exposed capacity geometry, fixed-population
occupancy sweeps and requested-memory comparisons, as preregistered in its
[family report](map-library/RESULTS.md). Equal reservation arguments do not
establish equal physical capacity or memory. No claim of matched conditions
may hide a library's unexposed geometry or a remaining policy difference.

The explicit `make ecosystem-targets` target reduces the final preserved
baseline series without rerunning timing. Its CSV retains both cohorts,
selected standard peers, sample bounds, qualification reasons and Abseil
references; it is not a correctness gate. Override `ECO_TARGET_INPUTS` with
a complete `family=path.csv` list to inspect a candidate. At the frozen
baseline, the sufficient range test gives:

| Family | Eligible cells | Pass | Deficit | Inconclusive |
|---|---:|---:|---:|---:|
| Vector | 36 | 0 | 36 | 0 |
| Deque | 24 | 12 | 4 | 8 |
| HashMap | 84 | 17 | 35 | 32 |
| PriorityQueue | 24 | 10 | 10 | 4 |
| OrderedMap | 30 | 7 | 17 | 6 |
| Total | 198 | 46 | 102 | 50 |

These are baseline classifications, not gains from an optimization. The
twelve controls remain in the output with an unranked status.

The later optimization pairs are separate experiments, not a fresh run of all
five families on one revision. Their supported conclusions are:

| Family | Supported result and remaining question |
|---|---|
| Vector | The [actual-compiler factor trial](vector-library/RESULTS.md#actual-compiler-factor-isolation-after-ownership-integration) finds that function-actual hints change no native code. Terminal traversal lowers elapsed time 6.26–30.90% in eight qualified scalar cells and 4.02–10.65% in five wide cells. Adverse wide suffix-one medians and strict wide empty-control losses remain, so it selects no production policy. These are isolated factor results, not cumulative gains from every change. |
| Deque | [Reusing the computed front slot](deque-library/RESULTS.md#production-lowering-reuse-a-front-placement-slot) lowers elapsed time 64.20–64.74% for scalar reverse churn at counts 16, 256 and 4096 in a matched pair. Other cells show no established improvement; scalar growth at 256 and 4096 remains below target. |
| HashMap | The [fixed-population occupancy sweep](map-library/RESULTS.md#frozen-source-occupancy-continuation) reduces missing-lookup complete-trace time by about 64% for scalar payloads and 72–74% for wide payloads when WF slots increase from 4096 to 8192. The preselected requested-memory comparisons retain substantial native deficits; wide replacement remains about 2.2 times direct C at 8192 slots. This establishes capacity sensitivity, not an adopted policy or an isolated probing cost. |
| PriorityQueue | The [four-ary trial](priority-library/RESULTS.md) does not remove the wide-value deficit. Movement and result handling remain hypotheses, not measured cost shares. |
| OrderedMap | The [selected drained-node cleanup rewrite](ordered-library/RESULTS.md#paired-timing-selects-the-cleanup-rewrite) removes the scalar/wide node copies and improves two wide build/cleanup cells, with no qualified loss in the 30-cell pair. The standard-peer target remains 8 passes, 19 deficits and 3 inconclusive cells; node occupancy and entry movement remain open cost leads. |

### Latest retained-image observations, 2026-09-28

The following snapshot retains each family's selected image as observed on
2026-09-28, excluding rejected candidates. The later selected Vector
[countdown caller composition](vector-library/RESULTS.md#countdown-batch-controller-balanced-pair-selects-caller-composition)
has separate balanced-order target reductions of 19/1/16 and 25/1/10; no
pooled count replaces them. These are not one measurement of the current
head: Vector, Deque and OrderedMap have later observations; HashMap and
PriorityQueue retain the original comparison. P/D/I means qualified sample-range pass, deficit or
inconclusive, with the same duration and cohort-stability screens described
above. Each individual-peer column applies those screens to that peer alone.
A pass against the slower standard peer need not beat both peers.

| Family | Against Rust P/D/I | Against C++ P/D/I | Slower-standard target P/D/I | Retained raw samples |
|---|---:|---:|---:|---|
| Vector | 0 / 16 / 20 | 15 / 5 / 16 | 15 / 5 / 16 | [Unchanged control of the rejected reserved-append trial](vector-library/ecosystem-checked-append-reserved-control-samples.csv) |
| Deque | 6 / 3 / 15 | 16 / 6 / 2 | 18 / 2 / 4 | [Front-slot reuse candidate](deque-library/ecosystem-boundary-reuse-candidate-samples.csv) |
| HashMap | 12 / 49 / 23 | 6 / 48 / 30 | 17 / 35 / 32 | [Original replay](map-library/ecosystem-replay-samples.csv) |
| PriorityQueue | 6 / 13 / 5 | 8 / 13 / 3 | 10 / 10 / 4 | [Original replay](priority-library/ecosystem-replay-samples.csv) |
| OrderedMap | 6 / 21 / 3 | 4 / 23 / 3 | 8 / 19 / 3 | [Selected drained-node cleanup candidate](ordered-library/ecosystem-drained-cleanup-candidate-samples.csv) |

Vector reuses the frozen `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`
both-factor image; the candidate from that later trial was rejected and its
source restored. Deque's candidate is
`5ae2cdd40793e617dbbe88d3fc38681db983166f`; HashMap and PriorityQueue use
`0c3203aa6111f14247aa950e3794e83082d4f29c`. OrderedMap uses the frozen
`7abd6bb34746b7b983b83103c68ee429b24859bb` compiler with the selected source
cleanup integrated in `846275596c74e1df011ab8f99b1ed61d9576e359`; its paired
C++ pass count falls from 5 to 4 within otherwise inconclusive changes, as
retained in the family report. The Vector factor trial's
16/1/19 target count and the later unchanged control's 15/5/16 are separate
observations, not an implementation regression or improvement. Vector and
PriorityQueue each exclude six controls from the ranking. HashMap's 84 cells
include 42 default-hash and 42 aligned-hash cells; capacity, layout and memory
differences remain as stated in its report. No new timing or combined
198-cell current-head result is claimed by this reduction.

In the most recent Vector diagnostics, a remaining deficit against a native
peer was incorrectly described as a regression from the candidate. Those are
different comparisons. The family report now separates them, preserves both
generations of the previously overwritten tail/truncate artifacts, and marks
the mixed accounting data unusable for candidate selection. The raw target
reductions themselves replay from their sample files; replaying a reduction
does not establish the identity or correctness of the image that produced it.

### Ordinary Vector controller composition: criterion

The next source-only discriminator starts from
`7abd6bb34746b7b983b83103c68ee429b24859bb`, after the owner-directed hint
withdrawal. Its freshly built compiler reproduces the retained T1H0 Vector
LLVM and timed object byte for byte. The hypothesis is that the private
trace-to-tail boundary repeats wide constructor setup and digest transport
that the ordinary Rust and C++ trace compositions keep outside their cycles.
The existing per-cycle calls and constant preparation are code observations;
their separate elapsed-time shares are not established.

Expand only `vector_library_tail_work` into the suffix cycle of
`vector_library_trace`, by parameter substitution and local renaming. Keep
the ordinary public append and truncate calls, all wrapping arithmetic,
ownership/consumption order, the zero-removal truncate call, retained prefix,
checksum recurrence and final drain/free. Leave the helper declaration and
all reserved/growth/reuse source branches unchanged initially. No new API,
compiler switch, attribute, metadata or special timed-count branch is part
of this comparison. This is a successor to the recorded tail-only forced
inlining experiment, whose wide suffix-two/count-16 loss remains a named
falsifier, not a newly discovered hypothesis.

Before construction, independently check the substituted block against the
helper and freeze the exact source patch and compiler identity. Compare the
optimized and native packages under the unchanged ordinary flags, including
unchanged-source branches, calls, frames, constants, within-record SIMD,
sections and placement. Require removal of the trace-to-tail call and wide
constant preparation outside the outer cycle; reject before timing if that
setup remains repeated or is replaced by comparable hot spills, a payload
snapshot, lost within-record SIMD or new helper work.

Both timed and accounting arms must pass the existing full correctness and
fault controls, with all 1,260 configurations / 8,820 executions per image
and identical 294-row instrumented allocation ledgers. That ledger describes
the instrumented image, not timed allocation traffic. Only after the code and
correctness screens pass, take one complete `measure 1048576 7` pair, both
cohorts, all seven samples and 42 cells / 4,116 rows per arm. Apply the
unchanged reducers, target and duration/stability qualifications; publish
all six unranked controls and native drift. Any qualified useful-cell loss
prevents selection. Overlap or unstable results do not complete the target.
A success selects only the ordinary source composition, not a general
inliner policy or a language amendment.

The first native screen found an ambiguity in "comparable hot spills" before
any linking, correctness execution or timing. The old nonempty tail prepares
14 literal vectors and stores seven pairs on every call; the expanded trace
prepares them once, then reloads seven pairs after every truncate. Thus 224
bytes of stack traffic remain per cycle, but the repeated literal preparation,
stack stores and tail call disappear. Counting residual stack bytes alone
does not compare the complete replaced work. An independent code review
confirmed this distinction; neither reading of the screen establishes a
timing result.

For the bounded continuation, compare the complete repeated setup, not only
its remaining stack traffic. Keep this qualification visible rather than
claiming an unequivocal pass of the original wording. The candidate may
advance to the unchanged correctness and single-pair timing stages because
the observed repeated work falls, within-record SIMD and direct backing
stores remain, and no owner-sized snapshot or new hot helper appears. The
wide trace frame grows from 352 to 400 bytes while the 288-byte tail frame
disappears; inspect final linked placement and preserve every changed branch
as a possible source of regressions. All prior falsifiers, full-matrix
requirements, native-drift qualifications and refusal of any qualified
useful-cell loss remain unchanged. This clarification precedes timing and
does not select the candidate.

The parallel HashMap occupancy discriminator used the same source/compiler
pin, a fresh separate build and preserved practical samples. Its family
report records the completed sweep, including memory pairs selected from
fresh geometry before timing. No load-policy, layout or indexing change
entered that sweep; it does not replace the practical matrix above.

The [complete Vector source pair](vector-library/RESULTS.md#ordinary-controller-composition-scalar-suffix-three-losses-prevent-selection)
now refuses this candidate: four useful cells improve, three scalar suffix-3
cells regress, 27 overlap and two are unstable. All correctness and release
observations pass, but the three qualified losses prevent selection. The
source remains unchanged. Both raw sample sets and the original screen's
qualification remain available; neither this trial nor the occupancy sweep
is a fresh five-family current-head result.

### Outer Vector batch controller: countdown criterion and balanced result

After the direct-composition and counted-batch refusals, the next source
criterion targeted only the private outer controller's live seed/trip-bound
state. The rejected inner terminal remaining-count spelling was not reused.
The native screen required the full scalar no-growth path to be no larger
than the original `21 + 16 × removed`, with no compensating hot branch,
owner transfer, spill or call; wide constants must remain outside repeated
cycles, with direct backing stores/SIMD and no new snapshot or hot spill.
The admitted source initializes a remaining-round count and rolling wrapping
seed, preserving all operations, zero-round/zero-suffix behavior, callback
order and final cleanup. No public API, compiler, policy or rule changes.

Current122 WF emission, SHA-256 `0118a820...`, admits the original source and
passes that native screen. The full original-control/candidate timed/account
matrix, ledger and checksum/cleanup fault checks pass using unchanged retained
runtime and native peers. Before timing, A1,B1,B2,A2 was fixed with A=original
unbatched control, B=qualified countdown, each `measure 1048576 7`. Both full
pairs use unchanged reducers and duration/stability screens; a combined
gain/loss must qualify in both orders and both cohorts. Any useful qualified
loss in either pair refuses selection; all other paired results remain
inconclusive, with every raw row and peer target reported separately.

The [complete balanced result](vector-library/RESULTS.md#countdown-batch-controller-balanced-pair-selects-caller-composition)
selects only caller composition: scalar suffix-2 at 16/256/4096 gains in both
orders, with zero qualified useful losses. The other 33 useful paired cells
are inconclusive, including wide suffix-1 whose large raw reductions fail
stability in the reverse ordering. Candidate peer-target P/D/I is 19/1/16 and
25/1/10; scalar growth/16 remains a deficit in both. This is current122 WF
source emission with retained harness/runtime/peers, not a fresh current-head
five-family result. The old refused sources and samples remain unchanged.

### Actual compiler factor isolation: criterion and result

The criterion recorded before construction was to integrate main's
ownership-surface migration at
`c84c4dd7ab46848f6a5b816fcf57fce32b98158e`, pin the resulting compiler,
library and harness revision, and construct a same-source two-factor ablation:
neither terminal consumption nor function-actual hints, terminal consumption
only, hints only, and both.
The earlier four-arm diagnostic edited LLVM directly; the actual compiler pair
measured both changes together. Neither attributes the current compiler's
wide-value loss to an individual change or their interaction.

The local variants disable only the call to `lower_terminal_consumption` or
the three existing `is_function_actual`-guarded hint emissions. Checked
function identities, source acceptance, library bodies and all native inputs
stay fixed. Freeze each source diff, compiler binary, emitted LLVM and native
input hashes before constructing the next variant. No variant switch enters
the work branch. Scratch artifacts belong to this bounded experiment and are
discardable after the reproducible result is published.

First compare each hint pair after removing only emitted `inlinehint` tokens;
other raw LLVM must agree. Inventory the terminal-lowering body changes and
explain anything outside the selected regions before proceeding. Each timed
and accounting image must pass the full existing behavior matrix and the
checksum/cleanup fault controls; the complete accounting outputs must agree.
These checks make no new claim about allocation exhaustion. Inspect the O3
objects and linked paths before timing, including calls, owner transfers,
frames, loop recurrence, empty-suffix guards, length stores and native-control
placement. Equivalent text/data and layout justify deduplicating timing, not
claiming a new measured result for an identical image.

For distinct images that pass those screens, any later timing retains the
complete `measure 1048576 7` matrix, both cohorts, the existing qualification
rules and all six empty-suffix controls. Compare each single change with
neither, and both with each single. A wide loss in either single-change arm
falsifies an interaction-only explanation; a loss confined to both supports
one only under these measured conditions. Preserve adverse unstable samples
and native drift. An overlapping range is not equivalence, target counts
alone select no policy, and this discriminator changes no pending amendment.

The experiment now pins `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`.
All four constructions pass the correctness screens. Both hint pairs have
identical native objects and loaded sections at identical addresses; only the
two distinct traversal settings were timed. Hints therefore explain neither
a gain nor an interaction in this build. The traversal rewrite has 13
qualified useful-cell gains, 21 overlapping comparisons and two unstable
comparisons, but all wide suffix-one medians are adverse and the three wide
suffix-zero controls strictly regress. The latter include final draining of
the retained prefix; they are not untouched code. Full observations and
reproduction are in the [Vector report](vector-library/RESULTS.md#actual-compiler-factor-isolation-after-ownership-integration).
This establishes a whole-build traversal effect, not separate costs for loop
instructions and the changed code placement, and selects no production policy.

The owner subsequently [withdrew the uniform hint preference](../../investigations/containers-and-resources/BEHAVIOR.md#ordinary-inlining-hints-for-supplied-functions).
Its implementation and pending amendment are removed, while the factor data
and terminal-consumption proposal retain their distinct conclusions. No new
performance execution is implied by that withdrawal.

### Remaining-count induction: criterion and code-screen rejection

Keep that pinned compiler, source library, harness, callback ABI, allocation
policy and ordinary hint settings. In a fresh local compiler variant, change
only the recognized traversal's generated induction: after the existing
nonempty guard, form `remaining = length - retained`; pass cursor and remaining
through the loop; after the unchanged consumer, increment cursor and decrement
remaining, continuing exactly when the latter is nonzero. Keep recognition,
ordinary fallback, forward handoff order, address qualification and final
length publication unchanged. No variant selector enters the work branch.

The arithmetic ground is `cursor + remaining = length` over mathematical
integers, with `retained <= cursor < length` and `remaining > 0` at each
handoff. Thus subtraction cannot underflow and the final cursor increment is
at most `u64::MAX`; zero-stride values impose no signed bound on logical
coordinates. Callback divergence still prevents later handoffs and final
publication. Extend the existing independent zero-stride oracle to a logical
length of `u64::MAX` with empty and small removed suffixes, alongside the
existing owner-order, release, observer and partial-exit cases.

Before timing, require the actual wide O3 loop to replace its separate
induction increment and `cmn` continuation with decrement-and-zero control,
while preserving forward payload progression. Inspect both payload widths
and reject the probe without timing if code is unchanged or adds a payload
snapshot, spill, call or empty-path length store. The complete existing
correctness/accounting matrix and fault controls must pass, account rows and
all native inputs must agree, and source, compiler and output identities must
be frozen. The historical `consumer-counter.patch` changed a whole LLVM body,
including address and transfer spelling; it did not isolate this countdown.
The new question is justified by the actual compiler's isolated traversal
effect, not by treating that historical replacement package as this change.

If the code and correctness screens pass, compare the frozen current `both`
image with the new actual-compiler image using the unchanged complete
`measure 1048576 7` protocol, all seven Vector samples, both cohorts, native
drift and all six suffix-zero controls. No timing, selection or native-target
success is implied by the smaller loop. Retain every adverse or inconclusive
cell; do not repeat on a loss, relax qualification or infer instruction-only
cost from changed code placement. This probe is not a claim that induction
alone explains the remaining Vector deficits.

The completed probe passed its correctness and accounting screens, but failed
the registered code screen: both timed and accounting WF objects are
byte-identical to the frozen current compiler's objects. Every linked section's
bytes and layout match as well. The countdown source spelling therefore gives
no native change on this toolchain. No timing was run, and neither the compiler
variant nor its test edit is promoted. The [source patch and phase results](vector-library/RESULTS.md#remaining-count-probe-rejected-before-timing)
retain the negative result without restarting the experiment under a new
criterion.

### Next discriminator: constructor saves on the growth edge

The [frozen wide-tail inspection](vector-library/RESULTS.md#wide-tail-setup-and-digest-handoff-deferred-discriminators)
identifies seven paired saves of constructor constants on every nonempty tail
call, although only its growth edge restores them. Test only their placement
in a scratch native diagnostic: move those same seven instructions, in order,
from entry to immediately before the existing growth call. Retain the literal
loads, frame, both hot calls, payload work, consumer, restores and function size.
The original and moved forms must have the same instruction multiset; only
the target's necessary internal branch retargeting may accompany the move.
This leaves local instruction placement as a possible timing contribution;
it is not an isolated memory-traffic percentage or a production compiler fix.
The earlier rejected inline packages changed several mechanisms together and
are not reopened by this narrower discriminator.

Preflight rejected the initial method before construction: the frozen timed
tail has a 288-byte frame and an outlined growth call, while the ordinary
accounting tail has a 368-byte frame and inlined growth. The same edit therefore
cannot be applied to both. Redirecting the timed allocator calls after O3 also
cannot reproduce the ordinary accounting ledger: the timed scalar suffix path
already elides its initial 16-byte empty backing, while the accounting path
retains it. For count 16, the disassembly implies one 152-byte request in the
timed path, versus the observed two requests totaling 168 bytes in the
accounting path. The former is a static prediction, not an observed ledger.
No native construction or timing preceded this correction, and no observer
events may be added to disguise that difference.

Use only the frozen `both` timed IR, exact clang flags and unchanged native
inputs from pin `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`. Before mutation,
round-trip that IR through assembly and require the control object's sections,
layout and relocation meaning, and linked sections/code/layout/unwind data,
to reproduce the frozen image; explain all non-executable metadata differences.
An unexplained difference stops construction. Candidate code must leave every
other function and its address unchanged, preserve section layout and unwind
data, and reverse textually to the exact control assembly. Verify the saved
registers remain unchanged until growth and every restore is dominated by the
relocated saves; empty/spare paths cannot read the unwritten private slots.

The maintained matrix alone cannot establish this: its suffix traces reserve
first, and its growth traces use a different helper. Add a scratch-only direct
tail witness using that same post-O3 code, valid owner layouts and distinct digest storage:
capacity/length/removed triples `(0,0,0)`, `(0,0,3)`, `(4,3,1)`, `(4,3,3)` and
`(8191,8191,1)`, each at seeds 0, 17 and `UINT64_MAX`. Independently check ordered
digest, retained prefix, final length/capacity, allocation count and cleanup.
Only in its correctness images, redirect the optimized assembly's allocator
calls to independent hooks without reoptimization. Cover malloc, free and
calloc, preserving the latter's zero initialization. The direct witness's
ledger follows its explicit initial backing and each actual growth, not the
ordinary account driver's pre-optimization formula. Control and candidate
must match this independently derived ledger exactly. Confirm that the WF
object changes only allocator relocations between its ordinary and observed
forms; all other instructions, layout and unwind data remain identical.
A release hook deliberately clobbers the saved caller-saved registers; inspect
that those clobbers reach the tail's restores. Placing the saves after growth
must fail the digest check, making the new witness discriminate preservation.
These instrumented witness images are never timed.

Both primary timed images must pass the unchanged complete behavior matrix
and checksum fault control, with unchanged native inputs and no observer hooks.
The direct witness must reject a missing release as well as the misplaced saves.
Earlier ordinary-account evidence remains a separate source-lifecycle check;
it does not validate this native edit or count the timed image's allocations.
Freeze source, patches, commands, direct statuses and artifacts.
Stop on a failed code or correctness screen;
timing needs a separate screen review. If admitted, use one full unchanged
`measure 1048576 7` pair with both cohorts, all seven samples, native drift and
all suffix-zero controls. No retries, threshold changes or selected-cell timing.
The scratch home is `vector-library/.build/constructor-save-edge/` under the
actual-ablation tree; its one-shot runner, growth witness and binaries retire
after reproducible inputs and useful evidence are published in this experiment.

The [completed single pair](vector-library/RESULTS.md#constructor-save-placement-native-code-discriminator)
passes the code and correctness screens. Among 36 useful cells, two wide
suffix-one cells have qualified gains, 32 overlap and two are unstable; all
six empty controls stay unranked. Standard-peer comparisons do not establish
parity or target completion. The exact native edit, witness and all samples
are retained; no production compiler change or general placement policy is
selected. A screen-script false positive was corrected in a recorded bounded
continuation using unchanged frozen objects, without rebuilding or retrying
performance measurements.

The [branch-weight discriminator](vector-library/RESULTS.md#branch-weight-growth-edge-discriminator-qualified-behavior-timing-inconclusive)
kept this frozen source and native link input set, adding only one LLVM IR
switch's `1/2000/1` weights for invalid/spare/full. Apple Clang moved the seven
wide-tail saves under the full/growth branch without larger frames or more
calls; the complete ordinary matrix and clobber/owner witness passed. One
unchanged full timing pair has no qualified useful gain or loss: count-16 wide
suffix-one medians improve about 41% but fail two-cohort range separation,
while counts 256/4096 are unstable. All three candidate wide suffix-one
standard-peer targets remain inconclusive. The separate unchanged-IR
`-sink-insts-to-avoid-spills` option produces byte-identical native code.
Neither diagnostic selects a production compiler policy or completes Vector.

The [nullable zero-extent owner trial](vector-library/RESULTS.md#nullable-zero-extent-owner-allocation-gain-useful-regressions-refuse-selection)
on a frozen current122 compiler removes each fresh empty owner request while
preserving real zero-capacity linked owners and the existing Vector content/
accounting oracles. Its one full pair has four qualified useful gains and 14
losses; scalar growth/count 16 improves about 8%, but repeated scalar reuse
roughly doubles. The preregistered no-loss condition refuses this physical
representation. An immutable READ-only fallback also fails its native
promotion/frame screen before any link or timing. The source patch, exact
identities and all raw samples are retained; neither variant is integrated.

The [canonical empty-header tag diagnostic](vector-library/RESULTS.md#canonical-empty-header-tag-balanced-diagnostic-misses-the-growth-criterion)
recovers baseline work/tail native operand streams but preserves both static
refusals: the external-tag trace frames grow, and known contents still add
16 bytes per trace. Its full ABBA run has two cross-order scalar suffix-two
gains and no qualified useful loss, but the required scalar growth/16 gain
overlaps in both pairs, so the primary criterion is not met. Growth medians
approach C++ with peer targets still inconclusive. The total diagnostic also
elides first-growth `memmove(0)`; it does not isolate allocation savings.
All four raw files and 42 outcomes are retained. This frozen pre-countdown
current122/retained-peer result is not additive with the selected caller,
merged-head qualification, or a production representation selection.

### Next discriminator: ordinary register-allocation spill splitting

The native save-placement result permits one code-generation screen, not a
compiler policy or another timing trial. Use the frozen `both` timed input
under `/private/tmp/whitefoot-vector-actual-ablation`, derived from raw LLVM at
pin `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`, and the same Apple clang at
`/usr/bin/clang` with `-O3`. Change only the candidate's native option
`-mllvm -split-spill-mode=default`. This spelling selects partition mode in
upstream LLVM; the unflagged Apple compiler's actual choice must be read from
its control option dump, not inferred from that spelling or upstream defaults.
The ordinary Whitefoot driver currently uses `-O2`; this screen retains the
ecosystem's `-O3` conditions and establishes nothing about the driver's default.

The scratch home is `vector-library/.build/split-spill-default-screen/` in the
actual-ablation tree. Its one-shot runner, assembly, objects and inspection
outputs serve this screen and retire after its concise result and reproduction
recipe are recorded here. No maintained tool, source, library, ABI or language
change is introduced. Preserve all original source/native inputs and the prior
save-edge artifacts with hashes before and after the command. Inspect the shared
guard owner before execution; a busy guard stops the attempt without retry.

From `/private/tmp/whitefoot-container-library-complete`, run exactly once:

```sh
perl .github/run-check.pl vector-split-spill-default-screen python3 /private/tmp/whitefoot-vector-actual-ablation/research/experiments/container-representation/vector-library/.build/split-spill-default-screen/run.py --code-screen
```

The runner's two LLVM-to-assembly commands, with working directory
`/private/tmp/whitefoot-vector-actual-ablation/research/experiments/container-representation/vector-library`, are:

```sh
/usr/bin/clang -O3 -Wno-override-module -mllvm -print-all-options -mllvm -print-before=greedy -mllvm -print-after=virtregrewriter -mllvm '-filter-print-funcs=wf_vector_library_tail_work$instance$c3abe4db44181f7a' -x ir -S .build/actual-ablation/both/ecosystem/whitefoot-timed.ll -o .build/split-spill-default-screen/control.s
/usr/bin/clang -O3 -Wno-override-module -mllvm -print-all-options -mllvm -print-before=greedy -mllvm -print-after=virtregrewriter -mllvm '-filter-print-funcs=wf_vector_library_tail_work$instance$c3abe4db44181f7a' -mllvm -split-spill-mode=default -x ir -S .build/actual-ablation/both/ecosystem/whitefoot-timed.ll -o .build/split-spill-default-screen/candidate.s
```

The shared printing options are diagnostic only. Record whether this installed
toolchain actually produces both requested machine-IR snapshots; absent dumps
are an instrumentation limitation, not permission for another compilation.
Record the control and candidate's printed option values. If the actual default
cannot be established, or is already partition mode, stop without a performance
inference. Assemble each unedited output with `/usr/bin/clang -O3
-Wno-override-module -x assembler -c INPUT -o OUTPUT`. Link using the exact timed
command in `actual-ablation/both-native-build.stdout`, replacing only its WF
object and output path; all 15 other object/archive inputs, order and flags stay
fixed. The linked outputs under the scratch home are `control/vector-costs-timed`
and `candidate/vector-costs-timed`. The unflagged control must reproduce the
frozen timed object and linked code, sections, relocation meaning, addresses,
CFI and unwind data, with any
UUID/path/signature-only differences explicitly explained. Stop on an unexplained
control difference.

Inspect the target and every changed WF body, reporting instruction sequences
and counts, calls, frame sizes, section bytes/layout, relocations, CFI and unwind
changes. The target passes only if its seven unconditional constructor-constant
saves disappear, the growth path still preserves the constants correctly, its
vectorized direct-to-backing payload and existing grow/truncate calls remain,
and no record snapshot or target frame growth appears. Every restore must be
dominated by a valid save, or its value must be correctly rematerialized; empty
and spare paths must not read unwritten slots. This compiler-option screen does
not presume that other functions or addresses remain identical: enumerate all
collateral changes for review rather than claim the earlier assembly edit's
isolation. A failed target criterion stops without correctness execution or
timing, and no alternative option or fallback is tried. Even a passing code
screen requires independent inspection and a separate next-step decision.
Record direct process statuses and separate code-generation, assembly, link and
inspection costs. Run no program, correctness matrix or performance measurement.

The [completed screen](vector-library/RESULTS.md#ordinary-spill-splitting-unchanged-target-rejected-before-execution)
confirms the installed `speed` default and explicit partition selection, but
leaves the target and its seven unconditional saves unchanged. It is rejected
without correctness execution, timing or another option trial; no policy follows.

### Next discriminator: checked append into reserved capacity

The next candidate is ordinary checked library source, not another native flag
or a manual assembly edit. A provisional `grow_vector_append_reserved` requires
`len < cap` and `len < ceiling`, calls `place_back`, and publishes length plus
unchanged capacity through the same narrow next-slot/length effects as
`deque_push_back`. Existing Vector append continues to handle growth. This
would let reserved batch builders and bounded-memory processing loops exclude
growth through a verified callable contract; it changes no language rule or
compiler mechanism. It is a candidate for evaluation, not an adopted public API.

The smallest source comparison changes only the suffix helper's append and the
capacity precondition, postcondition and loop invariants that justify it. The
existing reserve, prefix contents, constructor, ordered consumption, number of
cycles, capacity policy, seeds, digest and cleanup stay fixed. Mixed and growing
operation chains still use ordinary append. This differs from the earlier
runtime spare-branch rewrite: the candidate has no growth branch because its
caller proves room. It also differs from the historical LLVM-only reserved
append patch by requiring a checked source implementation and call sites.

Before native construction, verify scalar, wide and owning-element calls,
exactly filling capacity, empty iteration and complete cleanup; a caller without
sufficient capacity must fail the written requirement. Explicit finite proof
steps are allowed under the existing rules. Any implementation defect exposed
by this witness requires its own regression test; a new rule or primitive is
outside this discriminator. The library is embedded in whitefootc, so construct
one actual candidate compiler with the gate profile and two build jobs under
the shared guard, preserving the frozen control compiler and native inputs.
Do not replace the standard library through an unrecorded environment path.
Record compilation separately from checked-source and native execution costs.

Use the existing Vector O3 harness and frozen `both` control. The native screen
requires the executed suffix path to lose growth calls and the associated
constant-preservation stores, retaining direct backing construction and ordered
consumption, with no new hot helper or owner-sized staging. Inventory changed
bodies, calls, frames and linked placement instead of claiming instruction-only
isolation. If this screen passes, both complete behavior matrices, exact
instrumented ledgers and their checksum/cleanup negatives must pass before any
timing. Instrumented allocation counts remain qualified separately from timed
allocation traffic. A reduced exact-fit owning witness must also establish the
new callable boundary independently of the scalar/wide timed payloads.

Only after those screens may one complete control/candidate pair use the
established work, repeats, cohorts and reducers. Retain every cell and raw
sample; target passes, paired gains, overlaps and unstable cells are separate.
A useful-cell regression prevents selection on that pair. A successful source
variant still needs normal library tests, cross-family checks and a reviewed
amendment before final adoption. No performance outcome is assumed.

The [completed comparison](vector-library/RESULTS.md#checked-append-within-reserved-capacity-useful-regressions-prevent-selection)
passes the source and behavior screens but fails selection: six useful cells
gain, four regress, twenty overlap, three are too short and three are unstable.
The candidate source was restored after its exact patch and executable were
preserved. Removing the growth path also exposed cross-record loop
vectorization and new register saves; its elapsed-time contribution is not
isolated. No reserved-append API or compiler policy is selected.

Native adapters retain Rust `with_capacity`/`push` and C++ `reserve`/`emplace_back`;
they already reserve for the same trace. Do not add checks or artificial work to
those controls. Rust's similarly named
[`push_within_capacity`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.push_within_capacity)
is an experimental fallible interface, not the stable comparator's proved
operation. Report this comparison as a library/source variant using existing
proofs, preserving the practical baseline, not as same-source compiler-only
attribution or a result for unreserved callers.

### Final-code attribution before optimization

The frozen baseline's O3 timed images, not just the emitted unoptimized LLVM,
distinguish the following surviving work. These observations are not a claim
about subsequent candidate images. Inspect the complete trace callers: a public helper
definition may survive in an image even though every measured call was inlined.
These observations identify discriminators, not percentages of elapsed time.

| Family | WF code in the measured path | Rust/C++ comparison and next discriminator |
|---|---|---|
| Vector | Per-element append calls survive; wide suffix cycles construct a stack record, call append and pay its unconditional input snapshot. Truncate's suffix-one digest reads backing directly. | Both native fast paths construct directly in backing. C++ saves construction constants outside the suffix loop; WF repeats setup inside its tail helper. Compare source shapes with unchanged growth and consumption. |
| HashMap | Lookup, find and remove are already inlined; put, try-put and rebuild retain calls. | Native queries inline too, and mutation helpers remain in native images. Rust scans compact control bytes in groups; libc++ follows nodes with cached hashes. Separate probing, capacity geometry and index arithmetic. |
| Deque | Scalar endpoint operations inline, with descriptor reloads after front placement. | Rust retains descriptor state in registers. Neither churn loop uses integer division; test the exact predecessor arithmetic and update ordering. |
| PriorityQueue | Hot push, pop, sift and replace inline; wide sifting still moves complete records repeatedly. | Rust's wide pop can retain a call while C++ heap code inlines. Test ownership-preserving movement order before blaming call overhead. |
| OrderedMap | Query descent retains recursive lookup calls; exhausted-node cleanup still copies the complete node before freeing it; wide slot shifts copy one entry per iteration. | Native query descent uses loops. Source C remains close on several paths, so library traversal and layout must be separated from WF lowering. |

The [fixed-eight-slot find observation](../../investigations/result-registers/DESIGN.md#lowering)
is a distinct, still-valid lead: under Clang 18 on x86-64 at O2, early full
unrolling raised the inline cost and kept `find` out of its caller. A different
lowering inlined before peeling and ran the trace at 0.601 of the destination
baseline, versus 0.956 for the selected register-result lowering. That is not
the current dynamic-capacity map on Apple Clang 21/AArch64/O3: its measured
lookup and find calls have already disappeared. A global inline or unroll
policy must address the recorded cross-program losses; the historical ratio
does not select it.

For Vector, both Rust and C++ still retain a per-round work helper in reserved
and reuse traces. The contrast above is specifically the per-element append
fast path. Suffix-one has no rear-element exchange, making it a discriminator
without the forward-native versus rear-relocating-WF algorithm difference of
larger suffixes. Rust's drain range checks are still present; WF's proof-erased
checks do not explain its deficit in this case.

## Measurement criteria recorded before running

- Rebuild Whitefoot, native C controls, Rust, and C++ against one recorded
  source revision. Pin external-library versions; record compiler versions,
  flags, target, and allocator conditions. Historical samples are not the
  denominator for this run.
- Start with small, medium, and large populations already in each family
  driver, with scalar and wide values. Keep its complete trace, fixed input
  generation, correctness oracle, and consumption of results. Record any
  necessary trace change before using its measurements.
- Separate build time, correctness execution, allocation accounting, and
  timing. Practical timed builds use ordinary allocation without live
  accounting counters; separately instrumented executions report allocation
  counts, requested bytes, and peak live requested bytes for those instrumented
  images. Instrumentation can change inlining and prevent allocation elision;
  these counts are not automatically the timed image's allocation traffic.
  The [constructor-save preflight](#next-discriminator-constructor-saves-on-the-growth-edge)
  records a concrete difference. Verify optimized code or instrument after
  optimization before using such counts to attribute timed allocation cost.
  A requested-byte peak is not process RSS or allocator-resident memory.
- Use warmup and repeated paired samples with rotating implementation order,
  including a reverse-order cohort. Preserve all samples. A short or unstable
  cell is inconclusive until a longer bounded run resolves it. Aim for at
  least 1 ms per ranked sample; replay shorter cells with more work before
  making a close ranking. Report a cohort discrepancy above 10% in the ratio
  as unstable instead of merging the cohorts into one apparently precise
  number.
- Report each workload and payload independently. Use whole-trace elapsed
  time and per-operation normalization only where the denominator is defined;
  do not manufacture isolated lookup or growth latency by subtracting setup.
  Growth-focused traces locate a follow-up question, not a measured p99 pause.
- A repeated gap above 10% in both order cohorts is a triage signal, not a
  correctness gate or universal performance requirement. Smaller differences
  remain descriptive. Large memory differences and missing efficient APIs
  can justify investigation even where elapsed time is close.
- Attribute an observed gap only as far as evidence permits: application
  contract, hashing, algorithm, representation, allocation, or emitted code.
  A plausible explanation without a controlled discriminator stays a
  hypothesis. Record actionable unresolved questions in `docs/todo.md`.

## Reproduction and results

The explicit `ecosystem-build`, `ecosystem-check`, `ecosystem-account`, and
`ecosystem-measure` targets run the five family drivers sequentially. Wrap
them with the repository's `perl .github/run-check.pl <label> <command> ...`
guard and supply the prepared `WHITEFOOTC` and `ABSEIL_PREFIX`. These targets
neither rebuild the compiler nor download dependencies, and none is a
dependency of canonical correctness CI. All five families passed their
ordinary and accounting correctness executions, allocation checks, and
deliberate checksum/cleanup failures. Normal/retained C/WF checks passed
separately for vector, deque, priority and ordered, as did the original C map
checks. Older Whitefoot map overlays require their historical checkout.
Historical timings are not denominators here.

Run from a checkout with the measured benchmark sources and the report's
later reducer additions. Set `WHITEFOOTC` to the frozen compiler identified
below and `ABSEIL_PREFIX` to the pinned external installation:

```sh
set -eu
for phase in build check account measure; do
  perl .github/run-check.pl "ecosystem-$phase" \
    make -C research/experiments/container-representation "ecosystem-$phase" \
      WHITEFOOTC="$WHITEFOOTC" ABSEIL_PREFIX="$ABSEIL_PREFIX"
done
make -C research/experiments/container-representation ecosystem-summarize
```

The summarization target runs the reducer's self-test and `--complete` mode,
writing `.build/ecosystem-summary.csv` under the experiment. It checks each
family's complete driver matrix, both cohorts, fixed sample IDs (or at least
seven contiguous sequence samples), duplicate/missing rows, exact checksum
strings, positive durations and matched work settings. It reports min/median/
max per implementation and cohort, WF/comparator ratios, paired sample minima,
and `100 * (max cohort ratio / min cohort ratio - 1)` as the cohort spread.
Priority storage and vector suffix-zero overhead controls have separate
classifications. This reduction does not turn either into an ecosystem peer.

The initial work settings are vector/deque `ECO_WORK=1048576`, map
`ECO_WORK=262144`, priority `ECO_WORK=16`, and ordered `ECO_SCALE=16`.
The work units differ by family; the aggregate command rejects an `ECO_WORK`
override. The three bounded replays retain separate output files:

```sh
perl .github/run-check.pl priority-ecosystem-replay \
  make -C research/experiments/container-representation/priority-library ecosystem-measure \
    WHITEFOOTC="$WHITEFOOTC" ECO_WORK=64 \
    ECO_SAMPLE_FILE=.build/ecosystem/measurements-replay.csv
perl .github/run-check.pl ordered-ecosystem-replay \
  make -C research/experiments/container-representation/ordered-library ecosystem-measure-only \
    WHITEFOOTC="$WHITEFOOTC" ABSEIL_PREFIX="$ABSEIL_PREFIX" ECO_SCALE=64 \
    ECO_SAMPLE_FILE=.build/ecosystem/measurements-replay.csv
perl .github/run-check.pl map-ecosystem-replay \
  make -C research/experiments/container-representation/map-library ecosystem-measure \
    WHITEFOOTC="$WHITEFOOTC" ABSEIL_PREFIX="$ABSEIL_PREFIX" ECO_WORK=1048576 \
    ECO_SAMPLE_FILE=.build/ecosystem/measurements-replay.csv
```

Longer work changes setup amortization in query/update and reserved-churn
traces. Repeated complete traces also extend the seed sequence. These are
qualification runs with unchanged implementations, not optimization speedups;
never pool their samples with the initial work setting. Reproduce the final
reported medians from the preserved files without executing a benchmark:

```sh
cd research/experiments/container-representation
perl summarize-ecosystem.pl --complete \
  vector=vector-library/ecosystem-samples.csv \
  deque=deque-library/ecosystem-samples.csv \
  map=map-library/ecosystem-replay-samples.csv \
  priority=priority-library/ecosystem-replay-samples.csv \
  ordered=ordered-library/ecosystem-replay-samples.csv
```

The aggregate summarization target reads the initial `.build` files. The
explicit command above selects the longer map/priority/ordered series used
by their final family reports. Ordered sample 0 is preserved as warmup and
excluded from medians; every recorded sample in the other four families is
included. The reducer also supports partial inputs without `--complete`,
which do not establish complete matrix coverage or two-cohort stability.

### Current compiler integration

Main moved the libraries into compiler-bundled `std::collections` modules.
The five measured callers use record-local aliases and positional
`--emit-llvm <fixture>.wf`; they no longer bundle the removed
`lib/containers/*.wf` inputs. This retains all whole-trace scalar exports
without adding a module graph that would select only one entry closure.
Native runtime objects must come from the emitting compiler's revision.

The collection algorithms and public function signatures remain the same,
but module interfaces make representation fields readonly to clients. The
priority storage-only control therefore consumes its raw slots directly,
using the same reverse removal and cleanup operations as before, rather than
constructing a queue through its now-private representation or adding heapify.
Current enum constructors use `Enum<args>::Variant(...)`; match-arm labels
keep their ordinary contextual spelling.

Main also changed small aggregate results to register returns with internal
destination-form bodies. Fresh measurements include that implementation.
Scalar whole-trace C interfaces are unchanged. A retained-helper follow-up
must select qualified public standard-library symbols and exclude generated
`.body` definitions, or it would retain an extra implementation boundary.
This first practical matrix does not add retained native variants.

Frozen historical candidate measurements and their source identities remain
historical. Their replay requires the recorded checkout/compiler; this work
migrates the five current comparison callers rather than claiming that every
old experimental candidate accepts the new specification.

The prepared local toolchain is Apple M1 Pro, eight logical CPUs, 32 GiB,
Darwin 25.6.0 arm64; Apple Clang 21.0.0 and Rust 1.98.1 (LLVM 22.1.8).
Rust and Clang therefore do not share an LLVM version; their practical cost
comparison cannot alone attribute a difference to the source language.
The [shared construction settings](ecosystem.mk) use C11 with
`-O3 -Wall -Wextra -Werror`, C++20 with
`-O3 -DNDEBUG -Wall -Wextra -Werror`, and Rust edition 2024 with
`-C opt-level=3 -C panic=abort -D warnings`. The ecosystem preprocessor flag
is `-DECOSYSTEM`; native compilation and final linking use O3. Whitefoot's
emitted LLVM is compiled by Clang at O3. Ordinary timed images omit allocation
observers; separate `ACCOUNT_ONLY` images produce accounting data. Per-family
`configuration.txt` files record these settings and full compiler identities.
The initial practical-matrix gate compiler SHA-256 is
`cb918e191bb344733347e0602171d2ec53bd1d201044fdbc5dd7666468eea0a0`;
the specification SHA-256 is
`59951ec5e42c0daa46947d88448be3fae9ca14276ad3600c03af03a54ef74d83`.
Rebuilding the compiler after main integration took 69.28 s. The earlier
prepared compiler is not a timing baseline.

Abseil is pinned to release
[`20260817.0`](https://github.com/abseil/abseil-cpp/releases/tag/20260817.0),
commit `2065f4ded0558c6f89fee67c8e5228feb4eb960e`, built with CMake Release,
C++20, tests disabled and installation enabled. The downloaded official commit
tarball SHA-256 is
`db5de644b448f9c3de4c03fcf5ffc3da0dd0c15363d8af25463c25b2a5db8952`.
The library remains an external experiment dependency, not vendored source or
a new compiler/gate prerequisite. Preparation took 1.74 s to configure and
52.72 s to build/install Abseil; neither duration is a container execution
measurement.

### Actual compiler construction and native admission

The combined compiler implementation at
`9efd6c624d5d947f68e676a9181e58b3168d9a69` was built from unchanged practical
callers/library sources against preserved `.build/main-6bb-f` controls from
`f945eceecb3aacac20e76864c73edf8b7c87902b`. It implements the provisional
[terminal-consumption](../../../design/amendments/terminal-owned-consumption.md)
and [function-actual hint](../../investigations/containers-and-resources/BEHAVIOR.md#ordinary-inlining-hints-for-supplied-functions)
choices at that pin; the latter was subsequently withdrawn by the owner.
This is actual compiler output, distinct from Vector's raw K+hint
intervention. The later `0ebbb8cec408bc37ac2d1a60fe9cb2b237a10ffa` revision only
repairs a documentation link; it does not change compiler/lib/spec bytes.
The frozen candidate/control CLI SHA-256 values are
`b3bbef4357cf89dfc45c4bda17cecde0240cb2253a01679e3518b8eb13566f8c` /
`e77f0a97b85cf795aa3fe7e0afca88c00a6ea8307fa38fdf3bef368a9e27e4e4`.

Fresh `BUILD=.build/actual-forward-hint` construction preserves ordinary O3,
allocation, native controls and each family's existing visibility policy.
All 22 phase commands exit 0: CLI/family/corpus construction takes 79.017 s
(CLI 43.912 s), checks plus accounting 11.674 s, and five owning-program test
commands 21.495 s. The latter include helper construction inside the tests;
they are not pure native execution times. Each exact owning test passes once
and exercises both lowering modes. The wrapper ledger records 113.203 s in
one attempt. All existing checksum, cleanup and reserve fault controls remain
active. All 104 non-WF object/archive inputs match the controls, all 446
candidate-source pins held through construction, and every accounting CSV matches baseline
bytes. These focused observations do not substitute for the full repository gate.

| Family | Hinted physical definitions | Terminal regions selected | Accounting rows | Final native observation |
| --- | ---: | ---: | ---: | --- |
| Vector | 4 | 2 | 294 | Forward scalar/wide consumers; empty paths skip length publication |
| Deque | 4 | 0 | 120 | WF object and all linked sections identical |
| HashMap | 12 | 0 | 420 | WF object and all linked sections identical |
| PriorityQueue | 10 | 0 | 150 | WF object and all linked sections identical |
| OrderedMap | 11 | 0 | 210 | WF object and all linked sections identical |

The hints follow supplied checked function identities, including forwarding;
Priority's ten include two instances of imported `priority_queue_ignore_position`.
Other raw bodies stay unchanged. The identical probe-stack attribute declaration
moves, with whitespace changes; this is separate from function-body identity.
For all four unaffected families, section bytes, sizes and addresses, symbols,
retained calls, frames, copies/spills and native-control placement are unchanged.
Their complete executable hashes differ in link metadata, not section contents.
[Vector's native record](vector-library/RESULTS.md#actual-compiler-forward-consumption-with-ordinary-function-actual-hints)
reports the eliminated callback/copy mechanisms, surviving batch calls and
actual indexed loops that differ from raw K+hint. Its earlier timing cannot be
assigned to this compiler.

Before any timed invocation, the prospective all-five-family pair was reduced
to a fresh **complete Vector pair** because the other four implementations and
linked layouts are identical. Their prior baseline measurements remain retained
observations, not fresh candidate timings or a new latency claim. Vector still
uses `measure 1048576 7`, F then candidate, all 4,116 rows per arm, both cohorts,
36 useful cells and six unranked suffix-zero controls. No Vector cell is dropped.
The [fresh Vector result](vector-library/RESULTS.md#actual-compiler-paired-timing-scalar-gains-with-adverse-wide-results)
is adverse: raw 6 gains/1 loss/29 overlaps; qualified 6 gains/0 qualified losses/
27 overlaps/3 unstable cells. Wide suffix-one/16 retains a raw strict loss
(1.557/1.415×) even though its 10.0478% cohort spread narrowly exceeds the
unchanged 10% limit. The other wide suffix-one cells also regress in both
medians; all three wide suffix-zero controls strictly regress 14.1–20.1%.
Targets change from fresh F 13/9/14 to candidate 16/2/18 (pass/deficit/
inconclusive); neither that count nor the instability flags establishes the
required no-regression result. The complete table, raw pair, native drift and
qualifications remain in the family report. No candidate performance policy is
selected, and no separate time saving is attributed to either mechanism.

Replay construction from the pinned implementation checkout with the toolchain
and Abseil installation above, using a fresh output directory and the maintained
opt-in targets (from the repository root):

```sh
perl .github/run-check.pl actual-compiler-build cargo build \
  --manifest-path compiler/Cargo.toml --profile gate --bin whitefootc \
  --locked --offline -j2
perl .github/run-check.pl actual-container-checks make \
  -C research/experiments/container-representation \
  ecosystem-build ecosystem-check ecosystem-account BUILD=.build/actual-replay \
  WHITEFOOTC="$PWD/compiler/target/gate/whitefootc" \
  ABSEIL_PREFIX=/private/tmp/whitefoot-abseil-20260817.0-install
perl .github/run-check.pl actual-owning-build cargo test \
  --manifest-path compiler/Cargo.toml --test corpus --profile gate \
  --locked --offline -j2 --no-run
```

Invoke the five corresponding owning tests in
[`compiler/tests/programs/containers.rs`](../../../compiler/tests/programs/containers.rs)
separately under the guard, using the executable produced by that build; retain
construction and execution costs. Compare raw LLVM, ordinary
objects and final linked code before a new measurement; do not reuse optimized
LLVM through another O3 pass or rebuild the preserved controls. Local records
are under `/private/tmp/whitefoot-actual-compiler-validation/`: `manifest.json`,
`construction-1/{candidate-source.json,phase-times.json,comparisons.json}` and
`construction-1/native/{vector-audit.md,deque-report.md,map-native-audit.md,priority-native-audit.md,ordered-report.md}`.
The source revision and maintained targets above make the scratch runner optional;
research remains outside canonical gate dependencies.

### Preserved observations and execution cost

Each family report owns its exact contract, tables, qualifications and raw-file
identities. Counts below exclude the CSV header and include every recorded
warmup/sample row; accounting is a separate instrumented execution.

| Family report | Initial timing rows | Longer replay rows | Accounting rows |
| --- | --- | --- | --- |
| [Vector](vector-library/RESULTS.md#fresh-practical-timing) | [4,116](vector-library/ecosystem-samples.csv) | Not needed for native mutation comparisons | [294](vector-library/ecosystem-accounting.csv) |
| [Deque](deque-library/RESULTS.md#fresh-practical-timing) | [1,680](deque-library/ecosystem-samples.csv) | Not needed | [120](deque-library/ecosystem-accounting.csv) |
| [HashMap](map-library/RESULTS.md#current-rust-and-c-ecosystem-comparison) | [9,240](map-library/ecosystem-samples.csv) | [9,240](map-library/ecosystem-replay-samples.csv), work 1048576 | [420](map-library/ecosystem-accounting.csv) |
| [PriorityQueue](priority-library/RESULTS.md#practical-timing-results) | [2,100](priority-library/ecosystem-samples.csv) | [2,100](priority-library/ecosystem-replay-samples.csv), multiplier 64 | [150](priority-library/ecosystem-accounting.csv) |
| [OrderedMap](ordered-library/RESULTS.md#verified-practical-run) | [2,520](ordered-library/ecosystem-samples.csv) | [2,520](ordered-library/ecosystem-replay-samples.csv), scale 64 | [210](ordered-library/ecosystem-accounting.csv) |

Together the files preserve 33,516 timing observations, including the ordered
warmups, and 1,194 allocation observations. The reducer preserves the distinction
between element bytes and ordered key/value-pair bytes and keeps the map's
`native-default` and `aligned-hash` series separate.

The following wall times separate construction, correctness, allocation and
measurement. First successful build/check/account times are retained from the
original command observations; subsequent invocations reused their log paths.
Measurement phases include warmup, independent-oracle work and CSV output in
addition to the elapsed intervals recorded in rows.

| Family | First build s | Correctness s | Accounting s | Initial measurement s | Longer replay s |
| --- | ---: | ---: | ---: | ---: | ---: |
| Vector | 4.637 | 1.781 | 0.158 | 85.578 | — |
| Deque | 4.197 | 1.017 | 0.151 | 49.435 | — |
| HashMap | 7.140 | 1.660 | 3.610 | 76.333 | 307.876 |
| PriorityQueue | 3.876 | 1.486 | 0.155 | 15.036 | 56.063 |
| OrderedMap | 10.766 | 1.786 | 0.355 | 40.087 | 158.153 |

The five initial measurement phases sum to 266.469 s and the three replay
phases to 522.092 s. The separate premeasurement construction refreshes took
1.890, 1.882, 0.149, 0.130 and 0.149 s in table order. The separate historical
compatibility checks described above sum to 18.751 s. Compiler and Abseil
preparation times above are additional construction costs. None of these phase wall times is
an operation latency or a compiler-runtime performance ratio; canonical
repository verification remains separate from this experiment.

### Practical findings and qualifications

The ranges below span the two cohort ratios, not confidence intervals. Each
ratio divides WF's whole-trace median by the named comparator's median in the
same cohort; above 1 means WF took longer. Representative large populations
are 4,096 elements except the hash map's 3,584 entries. Ordered wide payloads
are 264-byte key/value pairs; other wide values are 256 bytes. The family
reports retain every path, size, payload and comparator separately.

| Family and final work setting | Representative observation | Qualification |
| --- | --- | --- |
| [Vector](vector-library/RESULTS.md#fresh-practical-timing), work 1048576 | All 36 mutating workload cells take WF more than 10% longer than both native vectors in both cohorts. Wide suffix-1 at 4,096 is 2.843–2.865× Rust and 2.714–2.728× C++. | Every native mutation comparison clears duration and stability criteria. Suffix-0 stays an unranked overhead control. Scalar suffix-2 at 256 against direct C remains unstable and supplies no attribution conclusion. |
| [Deque](deque-library/RESULTS.md#fresh-practical-timing), work 1048576 | Scalar reverse churn at 4,096 is 2.712–2.795× Rust and 1.390–1.427× C++; scalar forward churn is close to Rust. Wide forward/reverse churn is within 2% of Rust at this population. | All 24 application cells clear both criteria. Direction, population and growth policy change the outcome; wide digest work prevents interpreting close whole-trace times as pure movement parity. |
| [HashMap](map-library/RESULTS.md#longer-replay-and-large-population-results), work 1048576 | Large wide misses take 5.85–5.98× Rust, 13.17–13.26× C++ unordered and 19.82–20.81× Abseil with native defaults; aligned-hash ratios remain 7.17–14.65× across the native peers. | Every sample reaches 1.402 ms, but 15 comparator cells remain cohort-unstable and unranked. In particular, wide native-default hits at 3,584 are not ranked against any comparator. Both hash series remain separate. |
| [PriorityQueue](priority-library/RESULTS.md#practical-timing-results), multiplier 64 | Wide pop/push at 4,096 is 2.270–2.315× Rust and 2.162–2.225× the C++ heap algorithms. Wide growing fill/pop and heapify/pop also expose repeated native gaps. | The replay minimum is 1.166 ms; the maximum cohort-ratio spread is 6.604%. Raw-storage setup/cleanup is unranked. Replacement uses different native repair APIs, and scalar/population results do not imply a uniform queue ranking. |
| [OrderedMap](ordered-library/RESULTS.md#verified-practical-run), scale 64 | Wide churn at 4,096 is 1.53–1.58× Rust, 2.11–2.17× `std::map`, and 1.32–1.34× Abseil. At that population scalar churn favors WF over Rust, while scalar churn at 256 favors Rust. | The replay minimum is 1.647 ms and maximum cohort spread 9.5957%; no duration/stability flag remains. Payload and population still change rankings, and the map contract is qualified by indistinguishable `u64` keys. |

The hash-map replay leaves three unstable comparator cells at population 2
and twelve at 3,584, with none at 56. Its family tables mark each one; a large
displayed ratio does not override the stability criterion. Individual outliers
also remain in every raw series even when cohort medians qualify. These traces
provide no application-frequency weighting or tail-latency claim.

Requested storage adds a separate design question. During
[wide deque growth](deque-library/RESULTS.md#verified-correctness-and-allocation-observations),
WF's peak is 3,146,032 bytes, Rust's logical peak 4,194,304 and C++'s 2,113,536.
For the [large wide hash map](map-library/RESULTS.md#allocation-observations),
WF's filled-map peak is 1,114,128 bytes against
Rust's 2,170,888 and C++ unordered's 1,036,288; reserve raises them to
3,342,368, 6,512,656 and 1,101,824 respectively.
[Wide ordered churn](ordered-library/RESULTS.md#requested-allocation-storage) reaches
2,373,888 bytes for WF, 1,673,656 for Rust, 1,212,416 for `std::map` and
1,453,928 for Abseil. The linked family allocation tables distinguish request
counts, logical live peaks and possible realloc overlap. These observations
neither measure physical/RSS peaks nor identify an elapsed-time cause.

### Next discriminators

The result supports the following bounded follow-ups, recorded with the
existing questions in [docs/todo.md](../../../docs/todo.md). It selects no
representation, source rewrite, ABI change or compiler optimization.

- **Hash misses and table policy:** the completed same-source occupancy sweep
  establishes sensitivity but retains native deficits at preselected matched
  requested memory. Next compare an ordinary-library probing/layout candidate,
  preserving capacity, complete ownership outcomes and memory accounting;
  retain the separate wide argument/result-transfer question.
- **Wide returned values and short vector cycles:** inspect optimized transfers,
  initialization, cleanup and callback boundaries, then test an unchanged-source
  compiler variant with the same controls. Vector suffix-1 remains substantially
  slower than its transfer-order C control; wide map replacement takes
  1.95–2.25× direct C across populations and both hash series despite matching
  allocation totals. Their whole-trace differences do not identify a causal
  share by subtraction.
- **Deque endpoints and growth:** separate endpoint lowering from the native
  representation/API difference. Scalar reverse churn's WF/C gap is much
  smaller than its WF/Rust gap. The loop/bulk C growth pair supplies a copying
  discriminator, but selecting a WF change still requires a same-source test.
- **Priority sifting and replacement:** compare the swap/hole algorithm choice
  separately from unchanged-algorithm emitted-code differences. Wide pop/push
  remains much closer to swap C than hole C; replacement additionally exposes
  WF's downward sift, Rust's `peek_mut` repair and C++'s two heap repairs.
- **Ordered occupancy and wide transport:** observe splits, merges and node
  occupancy during fixed-cardinality churn. WF's peak node count grows from
  363 to 562. Separately, compare the scalar/wide replacement paths against
  source C under matched visibility before attributing their timing contrast
  to transfers or inlining.

The practical Whitefoot `.ll` files are compiler outputs before Clang O3.
Visible calls and copies there do not establish what survives optimization.
Any emitted-code attribution must inspect optimized IR or final native code
with the measured flags and use a same-source timing discriminator. Historical
retained O2 observations have different visibility and allocator conditions
and cannot supply causal percentages for these practical O3 gaps.

## Generic native-pipeline pilot: completed negative result

The code-only pilot rejects both tested candidates under its preregistered
screens: L produces no change beyond ordinary reoptimization, and U exceeds
the size screens despite exposing the predicted inlining opportunity. No
runtime measurements or production policy selection followed. The
[constitution's performance guidance](../../../docs/constitution.md#performance)
permits compilation work for runtime gains while requiring practical iteration;
it supplies no numeric budget. The provisional screens below were chosen before
this pilot, with the directed Vector probe's 11,992 to 17,060 bytes of WF text (about 42%
growth) known and neither general candidate measured. They are experiment
selection screens, not owner-approved performance ceilings or global policy.
No source-name hints, early `alwaysinline`, ownership/ABI changes or
benchmark-specific execution path were added.

The three frozen inputs are Vector source trial C's timed LLVM,
`tests/programs/containers/hashmap.wf` with its fixed eight-slot probe, and
`tests/programs/compute/records.wf` with the unchanged formal adapter binding
and both execution worlds. The saved C compiler has SHA-256
`02f18a296656c48f08e044fb635f06cab0871ce25ea75f57343434367959e21d`;
the Vector input has SHA-256
`4d6591a1eee0457b5edb2f46d312a03cd796b3d770c4ee9c04b71f3280ff4be6`.
All fixture and adapter source bytes match frozen revision
`47f9f91b63a484ba7f4924a63b5863b1b8b6f289`. The map and bound records LLVM
hashes are respectively
`e9eb0820adf88bd4d228aed698ef02726e0453dd3c9857fbad36f2dcad363fc1`
and `ebf3c2fb2ad2807bef98fac73a8fe5f130d928d33d54d56b58b0e80ee90f6f00`.
Source trial D and the concurrent Slots lowering changes are excluded.
Every arm uses Apple Clang O3:

| Arm | Construction from the same raw LLVM | Role |
| --- | --- | --- |
| S | One ordinary O3 compilation directly to object | Stock control |
| R | Ordinary O3 to optimized LLVM, then ordinary O3 to object | Reoptimization control |
| L | R's first stage; second O3 adds `-mllvm -enable-module-inliner=true -mllvm -inline-priority-mode=cost` | Costed module inlining after prior simplification |
| U | First O3 adds `-mllvm -unroll-full-max-count=0`; second O3 is ordinary | Postponed full-unroll hypothesis |

L and U are the only alternatives. Keep ordinary inline thresholds. The host
must support these flags and their effect must be verified. U's first-stage
fixed-eight loop
must remain unexpanded to instantiate the deferral test. Inspection-only LLVM
must never feed an extra optimization pass into an arm's native construction.

Code criteria precede timing. L should remove the wide Vector truncate call
beyond R, with fewer digest stack round trips or repeated constant setup and no extra
owner transfers. For U, predict that compact `find` and `remove` inline into
`map_trace` before full expansion. An absent baseline call makes that
opportunity unreproduced. Preserve scalar behavior and both records ASCII
loops; an absent stock loop leaves survival unqualified and requires the Linux
counterexample. Preserve destination-body simplification before wrapper
inlining. The early always-inline and merged-return forms in the
[result-register investigation](../../investigations/result-registers/DESIGN.md#lowering)
remain rejected. Unchanged target code, lost records structure or additional
wide movement stops that candidate before runtime measurement.

Screens compare candidates with S and report R separately. WF object text
excludes the native libraries and runtime:

- Summed WF text: at most 10% growth across the pilot; repeat over the full
  corpus before selection.
- Each module with at least 1,024 stock text bytes: at most 25% growth;
  smaller modules: at most 256 added bytes. Report absolute deltas and
  constant/unwind sections separately.
- Isolated native optimization/codegen: at most twice stock construction
  time. Charge both stages to R/L/U; exclude WF emission and inspection work.
  Retain timing resolution; short or unstable ratios leave this unresolved.
- Before production selection, cold and edited end-to-end builds may grow by
  at most 25% on the maintained module workloads; report warm reuse and
  source/native/link phases separately.

These bounds permit modest duplication and one extra optimization stage while
protecting iteration beyond one hot loop. Failure rejects this candidate under
these screens, not every costed late-inlining policy. A changed tradeoff needs
a new proposal before measurement, not a retrospective threshold increase.

The preregistered continuation rule permits only a positive code/size/cost
pilot to proceed to correctness images and then
timing, retaining Vector ownership/checksum/accounting, the map oracle and
records' independent complete-result checks. Selection additionally requires
the full program/container corpus, production O2, cached module/function
ThinLTO, full LTO and formal paired compute with its identical-image control.
`driver.rs`, `runtime.mk`, the formal performance Makefile and `ecosystem.mk`
supply flags separately: a driver-only change would miss consumers. A selected
pipeline must enter cache identity and receive a compiler design amendment;
changed result-body ordering or stock ThinLTO planning reopens those decisions.
No production or live-tree change was made.

The run completed on 2026-09-27 at 05:22 UTC, from worktree revision
`02b61a3056d9ca1bbd1c34bfaf5ca1100301b779`, using the frozen compiler and
inputs above. The host was Apple Clang 21.0.0 (`clang-2100.3.34.2`), targeting
`arm64-apple-darwin25.6.0`. All 29 emission, construction and inspection
stages returned status 0 directly; the guarded command returned 0 in 3.44 s
(child elapsed 3.41 s). Input hashes matched before and afterward. No
executables, correctness runs, accounting runs or runtime samples were made.

| Input | S WF text, bytes | R WF text | L WF text | U WF text | U / S growth |
| --- | ---: | ---: | ---: | ---: | ---: |
| Vector | 11,992 | 12,724 | 12,724 | 14,964 | 24.783% |
| Fixed-eight map | 3,408 | 3,128 | 3,128 | 6,100 | 78.991% |
| Records | 4,452 | 4,468 | 4,468 | 4,504 | 1.168% |
| Total | 19,852 | 20,320 | 20,320 | 25,568 | 28.793% |

L and R produce byte-identical objects for all three inputs. L leaves wide
Vector truncate out of line, with reported inline costs 1,170–1,205 against
250. Stock map `find` and `remove` expand before their callers and stay out
of line at costs 545 and 580 against 250; R and L retain those boundaries.
Thus L fails its code discriminator, independently of its acceptable size.

U instantiates the proposed ordering discriminator. Its first-stage `find`
retains the loop backedge and eight-slot termination test; compact `find` and
`remove` inline into `map_trace` at costs 105 and 140. Optimized IR removes
all seven `find` and three `remove` calls, and the native object has no call
relocations to either function. Wide Vector truncate likewise inlines at
cost 175 before later expansion. This is evidence for an inlining opportunity,
not a runtime gain. U fails both the 25% per-module map screen and the 10%
summed-text screen, so no runtime continuation is selected.

Both records ASCII inner loops survive in all four native arms: the parallel
chunk and sequential-world body retain a byte load, high-bit exit test and
backedge that bypasses continuation-state testing. In U these loops begin at
object text offsets `0xab0` and `0xcb4`, with backward branches at `0xac4`
and `0xcc8`. The rejected early always-inline and merged-return choices remain
rejected; this pilot does not reopen the result ABI or body-ordering decision.

Constant/literal bytes are unchanged between S, R and L: Vector 1,172, map 8,
records 68. U has 1,044, 96 and 36 respectively. Compact-unwind plus
exception-frame bytes stay at 2,280, 216 and 1,152 respectively in every arm;
records' four zero-fill bytes also stay unchanged. These sections are reported
separately from the text screens.

| Input | S construction, s | R construction | L construction | U construction |
| --- | ---: | ---: | ---: | ---: |
| Vector | 0.16 | 0.24 | 0.24 | 0.28 |
| Fixed-eight map | 0.05 | 0.08 | 0.08 | 0.11 |
| Records | 0.06 | 0.10 | 0.10 | 0.10 |

R/L/U include both stages, charging their shared first-stage work to each arm.
WF emission (map 0.55 s, records 0.03 s) and inspection-only runs are excluded.
These single short observations have 0.01 s reporting resolution; construction
cost remains unqualified, including U's nominal 2.2-times-stock map result.
The size failure already ends U under this pilot's selection rule.

Read-only linkage attribution separates retained helpers from reachable
caller growth without changing that verdict. The roots are Vector's
`wf_vector_library_word_trace` and `wf_vector_library_record_trace`; the map's
`wf_map_trace` and `main`; and records' `wf_bench_records` and
`wf_bench_records_release`. Follow all named references in optimized IR,
including function addresses and globals, from those roots. Every native
defined-function relocation edge agrees with that reference graph. The records
closure includes its address-taken
`wf__par_thunk__par_split_summarize_records.0.0` callback and the
`wf__par_split_summarize_records.0` body it invokes. Measure
each native symbol from its start to the next function, or the end of
`__text`; these extents include alignment and are not packed linked bytes.

| Input | S reachable text, bytes | U reachable text | S external text outside roots | U external text outside roots |
| --- | ---: | ---: | ---: | ---: |
| Vector | 7,408 | 9,168 | 4,584 | 5,796 |
| Fixed-eight map | 2,884 | 4,616 | 524 | 1,484 |
| Records | 1,960 | 1,972 | 2,492 | 2,532 |

Of U's 5,716 added object bytes, 2,212 are in the outside-root pool and 3,504
remain reachable growth. U's wide truncate and drain retain 700 and 608 bytes
with no module references; map insert/find/remove retain 960 bytes, while
reachable `map_trace` grows from 1,168 to 4,532 bytes. Even excluding every
outside-root body, map growth is 60.055% and combined growth 28.599%. This
attribution neither measures linker deletion savings nor rescues the original
object-text screen. Both records execution worlds retain the ASCII loop in
reachable bodies too: U's parallel split has the load/backedge at `0x918`/
`0x92c`, and the sequential body at `0xcb4`/`0xcc8`.

The [emitter](../../../compiler/src/backend/emitter.rs) gives ordinary
functions external linkage while retaining internal result bodies. The frozen
native Vector objects instead keep Rust work/growth helpers local and C++
anonymous-namespace helpers local, with surviving standard-library template
bodies weak/coalescible. A separate linked-footprint investigation could test
ordinary linker dead stripping before changing emitted visibility. This is
an opportunity, not a selected policy: raw LLVM foreign callers and callback
roots must remain available, and the [fragment splitter](../../../compiler/src/backend/fragments.rs)
keeps cross-fragment ownership, shared hidden helpers and stock ThinLTO
integration under the [incremental design](../../../design/compiler/incremental-compilation.md).
Actual linked size, layout/runtime effects, production O2 and cross-target
behavior remain unmeasured.

Reproduce this attribution from the pinned inputs and native construction
below using `nm -n` for function boundaries, `otool -tvV` for instructions,
`otool -rv` for reference edges and `size -m` for the final text extent. The
original local artifacts are `code/{vector,hash8,records}.stock.o` with
`code/{vector,hash8,records}.first.ll`, and `.postponed.o` with
`.postponed.opt.ll`, under the recorded private run directory. Diagnostic
LLVM is inspected only; it does not enter native construction.

Whether U's actual runtime gains would justify a different size/iteration
tradeoff remains unresolved. A later diagnostic runtime ceiling could be
preregistered to answer that question without authorizing adoption. That would
be a new investigation, not a retrospective increase of these screens. Full
corpus behavior and text, production O2, incremental build cost and paired
compute evidence are still missing. Neither negative result rules out every
costed late-inlining policy.

For reproduction, use a clean checkout of
`47f9f91b63a484ba7f4924a63b5863b1b8b6f289` on the recorded host toolchain.
Build its compiler separately with the guarded command below; this compiler
construction is outside the native cost table. Then run the second block from
that checkout in Bash. It creates a fresh scratch directory and retains each
Clang stage's direct status, remarks and `/usr/bin/time` output. Compare the
three emitted input hashes above before interpreting a replay. The private
run directory `/private/tmp/whitefoot-generic-pipeline-discriminator/` identifies
the original local evidence; the recipe does not depend on its script.

```sh
perl .github/run-check.pl pipeline-compiler \
  cargo build --manifest-path compiler/Cargo.toml --profile gate \
  --bin whitefootc --locked --offline -j 2
```

```sh
WHITEFOOT_CHECK_TIMEOUT=60 perl .github/run-check.pl pipeline-code bash -eu <<'SH'
out=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-pipeline.XXXXXX")
wfc="$PWD/compiler/target/gate/whitefootc"
fixtures=tests/programs/compute
"$wfc" --emit-llvm research/experiments/container-representation/vector-library/vector-library.wf -o "$out/vector.raw.ll"
sed 's/@main(/@wf_fixture_main(/g' "$out/vector.raw.ll" > "$out/vector.ll"
"$wfc" --emit-llvm tests/programs/containers/hashmap.wf -o "$out/hash8.ll"
"$wfc" --par --emit-llvm "$fixtures/records.wf" -o "$out/records.raw.ll"
awk -f "$fixtures/host-adapter.awk" "$out/records.raw.ll" "$fixtures/records_host.ll" > "$out/records.bound.ll"
sed -e 's/@main(/@wf_fixture_main(/g' -e 's/@wf__main_body(/@wf_fixture_body(/g' "$out/records.bound.ll" > "$out/records.ll"
shasum -a 256 "$out/vector.ll" "$out/hash8.ll" "$out/records.ll"
stage() {
  label=$1; shift
  if /usr/bin/time -p /usr/bin/clang -O3 -Wno-override-module -x ir \
      '-Rpass=(inline|loop-unroll)' '-Rpass-missed=(inline|loop-unroll)' \
      '-Rpass-analysis=(inline|loop-unroll)' "$@" \
      > "$out/$label.stdout" 2> "$out/$label.stderr"; then status=0; else status=$?; fi
  printf '%s\t%s\n' "$label" "$status" >> "$out/status.tsv"
  return "$status"
}
for unit in vector hash8 records; do
  file="$out/$unit"
  stage "$unit-S" -c "$file.ll" -o "$file.S.o"
  stage "$unit-first" -S -emit-llvm "$file.ll" -o "$file.first.ll"
  stage "$unit-R" -c "$file.first.ll" -o "$file.R.o"
  stage "$unit-L" -mllvm -enable-module-inliner=true -mllvm -inline-priority-mode=cost -c "$file.first.ll" -o "$file.L.o"
  stage "$unit-delayed" -mllvm -unroll-full-max-count=0 -S -emit-llvm "$file.ll" -o "$file.delayed.ll"
  stage "$unit-U" -c "$file.delayed.ll" -o "$file.U.o"
  for arm in S R L U; do
    size -m "$file.$arm.o" > "$file.$arm.size"
    nm -n "$file.$arm.o" > "$file.$arm.symbols"
    otool -tvV "$file.$arm.o" > "$file.$arm.native"
    otool -rv "$file.$arm.o" > "$file.$arm.relocations"
  done
done
printf 'Code-only artifacts: %s\n' "$out"
SH
```

To inspect final optimized LLVM, repeat an arm's last Clang invocation with
`-S -emit-llvm` instead of `-c`, saving a distinct diagnostic file. Never feed
that file back into the native construction or charge its time to the arm.
Stop at these artifacts; this reproduction contains no executable link or run.

### Frozen F runtime diagnostic preregistration

Preregistered on 2026-09-27 before native construction or runtime observation.
This asks a separate performance-first question: can U's demonstrated ordinary
costed inlining yield a useful runtime improvement that warrants further policy
study despite the known size growth? The preceding code-only pilot stays
negative under its original screens. This diagnostic neither changes those
screens nor selects a production policy, source-name hint or export change.

Use Vector source F, revision `3347fcb4b705990b5b3a9d66887a30ca9e632374`.
The saved compiler SHA-256 is
`fe856a7b9ab2827bb30515547a82bc01132b65e4e3110dd0371cdf0cfc88b3eb`;
the frozen timed and account LLVM hashes are respectively
`63038feb712920a11bb212eb06f858929186445eac6b2703afea392a9d74aa24` and
`2ee32650b03a6e1a42f3ef2fe55bc569066e150c86024dd3f6a64a479a0f6518`.
The frozen timed image is
`44660c2de9532af3392c3c5fefea363b1915abd03bc9b79f4ba39812425c05f2`;
its 294-row account CSV is
`ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`.
Local originals are under `/private/tmp/whitefoot-vector-single-insert-f/`
and the family's `.build/single-insert-f/{ecosystem,native}/`; the 31-file
manifest `runtime-f-inputs.sha256` in the previous private run directory pins
both drivers, native libraries, all runtime objects and reference artifacts.
Current G sources and the current default compiler are excluded.

Construct fresh S, R and U timed objects with the preceding arm definitions,
using only F's frozen raw LLVM. L is omitted because its three earlier objects
were byte-identical to R. Independently construct matching account objects
from F's frozen observer LLVM. That input uses exactly F's Makefile rewrite:
`malloc` to `wf_cost_allocate`, `free` to `wf_cost_release`, `main` to
`wf_fixture_main`, and `declare noalias nonnull ptr` for the allocation
declaration. Add no realloc policy or observer work to timed code. Link all
six with the corresponding frozen driver/Rust/C++ objects and unchanged
runtime/link order. No Whitefoot or Rust compiler rebuild belongs to this run.

Before timing, fresh S must match the frozen object bytes and linked function
placement, text and constant bytes; explain non-code identity differences and
stop on a substantive mismatch. Inspect actual linked assembly for all arms:
whether F reproduces C's wide truncate boundary, whether U removes it, the
wide tail boundary, scalar code, digest loads/stores, aggregate transfers,
stack frames and repeated constants. Preserve every word's checksum use.
Report object text and linked layout separately; no linked-footprint result
replaces the earlier size screen. The prior U object had no accounting image
and supplies no allocation evidence for this run.

Every timed/account image must pass F's original 1,260 configurations and
8,820 complete executions. All three account outputs must match the frozen
294 rows byte for byte. Each account image must reject the checksum and
unreleased-allocation controls with status 1 and the exact existing reason.
Keep successful construction, successful checks and expected rejection
statuses/times separate. Any input, oracle, cleanup or accounting mismatch
stops the diagnostic. Fixed-eight runtime preparation is outside this task;
its existing native mechanism evidence is sufficient here.

If separately granted after code admission, run one S/R/U trial using the
unchanged `measure 1048576 7`: 4,116 rows per image, both internal order
cohorts, all seven implementations, both widths, all three populations, all
seven paths and seeds 101–107. Report all 36 useful cells and six unranked
suffix-0 controls. There is no automatic reverse-order batch or replay.
Root reads the first trial before deciding whether a specific uncertainty
warrants another run; roughly 240 s of measurement and a 600 s guarded ceiling
are proposed, separately from construction and checks.

The prospective diagnostic-positive screen is at least one useful cell with
U/S median at most 0.95 in both cohorts and U's observed maximum below S's
minimum in both. Require samples of at least 1 ms and cohort-ratio spread at
most 10%; inter-arm native-control drift above 10% leaves attribution
unresolved. These are diagnostic screens, not owner performance ceilings or
confidence intervals. A positive first trial is a lead, not proof of
repeatability across image orders. Report U/R, R/S, the original slower-native
comparison and every useful-cell regression; unchanged scalar instructions
do not exclude layout effects. No average erases a counterexample or the
existing size failure. Production selection still requires O2, other admitted
targets/toolchains, all families and the full corpus, stable cold/edited/warm
compile cost, object/linked text, module/function ThinLTO, full LTO, formal
paired compute and its null control, exports/callbacks, both execution worlds,
result-body ordering, cache identity and a design amendment.

The initial grant covered one construction/check batch followed by read-only
native review, with no runtime measurements. Its local command was:

```sh
WHITEFOOT_CHECK_TIMEOUT=90 perl .github/run-check.pl generic-pipeline-f-build \
  bash /private/tmp/whitefoot-generic-pipeline-discriminator/build-runtime-f.sh
```

That scratch script is an original-run identifier, not a checkout dependency.
To reproduce its inputs, use a clean checkout of the F revision above and the
recorded native toolchain. Build its compiler with the earlier guarded Cargo
command, then run `make -C research/experiments/container-representation/vector-library
BUILD=.build/pipeline-f ecosystem-build ecosystem-check ecosystem-account`
under the guard. Verify emitted hashes before interpreting a replay. Use the
S/R/U Clang stage commands above for each of `whitefoot-timed.ll` and
`whitefoot-account.ll`, substituting the fresh F input and output paths. For
each arm and kind, the complete link command is:

```sh
# Run from that F checkout; arm is S/R/U and kind is timed/account.
family=research/experiments/container-representation/vector-library
base="$family/.build/pipeline-f"
runtime=(ordinary_values.o sched/core.o sched/entry.o completion/runtime.o
  completion/file_adapter.o completion/bridge.o wf_floor.o sched/prim_host.o
  completion/wait_host.o completion/file_posix.o completion/linux_io_uring.o
  ordinary_values_ir.o)
native=()
for object in "${runtime[@]}"; do native+=("$base/native/$object"); done
clang++ -O3 "$base/ecosystem/driver-$kind.o" "$out/$kind-$arm.o" \
  "$base/ecosystem/cpp-$kind.o" "$base/ecosystem/rust-$kind.a" \
  "${native[@]}" -pthread -lm -liconv -lSystem -lc -lm \
  -o "$out/vector-$kind-$arm"
```

Use `nm -n`, `otool -l`, `otool -s` and `otool -tvV` for stock identity and
linked code; run each image's `check`, each account image's `account`,
`fail-checksum` and `fail-cleanup`, reading statuses directly as above. Compare
account CSVs with `cmp`; the exact negative reasons are in F's Makefile.
Preserve logs/hashes and separate phase costs. Stop for code review before a
separately authorized `measure` invocation.

S/R/U construction and checks completed with guard status 0 in 9.62 s (child
9.51 s). All 58 successful phases returned 0 and all six expected negative
observations returned 1. Every image passed the complete matrix and all three
account CSVs matched F. Fresh S timed/account objects are byte-identical to
F. Linked section bytes and function placement match; every whole-image
difference is confined to UUID/signature metadata and its recorded extent.
The signature identifiers reflect the different output basenames. No runtime
samples were collected. Local artifacts are in `runtime-f/` under the recorded
private directory, including commands, phases, hashes and linked disassembly.

| F arm | Timed WF text, bytes | Native construction, s | Account construction, s |
| --- | ---: | ---: | ---: |
| S | 11,428 | 0.16 | 0.16 |
| R | 12,156 | 0.24 | 0.24 |
| U | 14,596 | 0.28 | 0.30 |

The two-stage arms charge both native phases. Six links took 0.30 s, six checks
4.75 s and three account executions 0.09 s in total; inspection is separate.
These short, single construction observations retain 0.01 s resolution and
do not qualify a compile-cost policy. U removes the wide truncate calls but
retains the wide tail call; R inlines tail but retains truncate. U also changes
register pressure, stack frames and scalar insertion code, so the future
runtime discriminator is not the cost of one isolated call.

At 06:16 UTC, before any runtime observation, add a code-only discriminator V:
compile raw F directly to native object with ordinary O3 plus
`-mllvm -unroll-full-max-count=0`, without a second LLVM optimization stage.
The grounds are already-observed U size/construction cost and its first-stage
remarks: wide truncate inlines at cost 175 against 250 before stage two; the
earlier map first stage likewise inlines find/remove at 105/140. Thus a second
complete O3 pipeline is not yet justified by the inlining witness alone.
This changes no old screen and proposes no production policy.

V must reproduce the wide boundary removal in actual linked code, preserve
all element consumption/owner transfers, and report surviving fixed-size loops,
helper calls, stack effects, object text and single-stage cost. Use the same
31 frozen inputs, two matching images, complete checks, exact F accounting
comparison and both negative controls. Keep S/R/U artifacts byte-frozen. No
fixed-eight or records rebuild and no runtime measurement is authorized here.
The guarded scratch command is `build-runtime-f-v.sh` under the same private
directory; the portable native step for each kind is:

```sh
clang -O3 -Wno-override-module -x ir -mllvm -unroll-full-max-count=0 \
  -c "$base/ecosystem/whitefoot-$kind.ll" -o "$out/$kind-V.o"
```

The unchanged link/check recipe above was used with `arm=V`, preserving direct
statuses and phase costs, followed by native review. The V code result had to
precede any decision about whether to time it.

V completed with guard status 0 in 3.01 s (child 2.99 s): eight successful
phases returned 0 and both negative controls returned 1 with the expected
reason. Both full matrices passed and its 294 account rows matched F. All
31 inputs and the S/R/U artifacts still matched their pinned hashes. Timed
construction took 0.16 s, account construction 0.17 s, the two links 0.10 s,
the two checks 1.68 s and account execution 0.03 s. These retain the same
short-observation qualification. V timed WF text is 12,564 bytes, 9.940% above
S, compared with U's 27.721%; constant/literal bytes are 84 versus S/R's 932
and U's 1,044. This is a Vector result, not a pass of the earlier summed
three-module screen or a selected policy. Artifacts are in `runtime-f-v/`.

Actual linked code confirms that U and V remove all wide truncate references;
S has five call sites and R seven across the emitted WF bodies, including
unreferenced public helpers. S's hot tail calls truncate at `0x10000cb48`,
R's retained tail body at `0x10000cdc0`; U and V tail bodies contain no such
call. R inlines tail into trace; S/U/V retain that boundary. Generic instance
`c3abe4db44181f7a` names the wide trace/work/tail functions and
`d6739d8f89f405bd` the wide truncate helper in these pinned images.

| Total wide linked stack frame, bytes | S | R | U | V |
| --- | ---: | ---: | ---: | ---: |
| Trace | 352 | 400 | 368 | 704 |
| Work | 688 | 688 | 720 | 928 |
| Tail | 288 | 288 | 368 | 672 |

These are total stack reservations, including saved registers. In wide work,
S/R reserve 592 local bytes plus 96 saved-register bytes, U reserves 560 plus
160, and V reserves 848 plus 80. A single `sub sp` in other prologues includes
their saved-register area; it is not an additional local allocation. No
execution-time share is inferred from these frame sizes or call counts.

V's smaller code retains a concrete aggregate cost. Tail construction zeros
a 256-byte stack array, fills it in four vector-loop iterations, copies it
to a second stack area, then copies it into backing storage. The fill loop
is at linked addresses `0x10000cb04`–`0x10000cb2c`. After taking the odd final
element, V copies its 256 bytes back to stack and checksums all 32 words
through the loop at `0x10000cd30`–`0x10000cd40` (eight-byte step, 256-byte
limit). U writes constructed vectors directly to the backing and its odd
consumption reads all words there with an expanded digest chain. U still has
spill traffic and repeated constant setup; removing the call alone does not
establish a gain. The second O3 stage therefore is unnecessary for this
inlining witness, but its later expansion removes materialization that V
retains. Neither arm is a call-only comparison.

The scalar trace/work/tail and truncate instructions match S against V, and
R against U, after resolving branch targets/relocations. R/U change scalar
insertion control flow from S/V; code placement also moves. Thus an observed
scalar difference cannot automatically be assigned to inlining. Frozen native
control objects remain identical in every image, but their linked placement
can change.

For audit, the timed image SHA-256 values for S/R/U/V are respectively
`21584c4979fd5b7cf2c3522022ee137cc22bc1447bac8de0ce925879ac280b52`,
`989be5dc5552c1e94999f9a46b67fdc8a5ebe304cd9e47084d7388a636524e38`,
`1d7bdef6b325059d54c37ca1f59def0c43e208e3ba3807add3a39bd6098802a5`
and `b498a3bed648da8d65a5d3fadd647786591e6bb6772f46b24046609225ad70b8`.
The saved `timed-*.linked-native.txt` comes from `otool -tvV` on those images;
`timed-*.object-symbols.txt`, `*.size.txt` and `phases.tsv` preserve the other
observations. Inspection LLVM is not counted as native code or fed to another
native stage.

The F preregistration required unchanged source-level ownership/consumption
and explicitly reported aggregate transfers, stack work and layout changes;
it set no native-transfer ceiling. V passed correctness and the inlining
witness while exposing additional native stack copies. The old C pilot's
stricter movement/size selection rule remained unchanged and its result stayed
negative. The unresolved question at this point was whether either complete
native pipeline would buy useful runtime despite its observed costs, with
every original cell and control retained. S/R/U/V were frozen pending the
separate timing grant recorded next.

After this native review and before any runtime observation, the root agent
authorized one S/R/U/V full-matrix trial in that order, retaining V to
discriminate the need for U's second pass. Use the unchanged 1,048,576 work / 7
sample command and 4,116 rows per arm; preserve every outlier. Apply the stated
first-trial diagnostic screens separately to U/S and V/S, and report U/V plus
the R control. This is no production selection or threshold revision. Check
all 31 frozen input hashes and every S/R/U/V construction artifact before and
after the trial. One 600 s guarded batch is allowed (about 320 s expected),
with direct per-arm status and elapsed time; no automatic rerun or new arm.
The local one-shot command is `measure-runtime-f.sh` in the recorded private
directory. The portable measured command is simply
`"$out/vector-timed-$arm" measure 1048576 7` for S, R, U, V, saving each stdout
to a distinct CSV and reading its exit status directly. Account images are
never timed.

The one trial ran on 2026-09-27 from 06:25:56 to 06:31:24 UTC. S/R/U/V
returned 0 in 80.12 / 80.33 / 81.15 / 85.68 s; the guard returned 0 in
328.00 s (child 327.91 s). These are complete-matrix execution costs, not
per-operation ratios. Each arm emitted all 4,116 unique expected rows
(16,464 total), with unchanged work/rounds and identical checksums across
arms. All 31 frozen input hashes and 26 construction artifacts matched before
and afterward. Every sample and outlier is retained in the committed raw
[S matrix](vector-library/ecosystem-pipeline-f-s-samples.csv),
[R matrix](vector-library/ecosystem-pipeline-f-r-samples.csv),
[U matrix](vector-library/ecosystem-pipeline-f-u-samples.csv) and
[V matrix](vector-library/ecosystem-pipeline-f-v-samples.csv).
The private `runtime-f-measure/` directory retains identical originals and
the phase/status/matrix checks. No replay or extra arm ran.

Each table entry gives the candidate/S median ratio in cohort 0 / cohort 1,
without pooling. Above one is slower than S. G and L mean all observed
samples are respectively faster or slower in both cohorts, after duration
and cohort-ratio qualification; O means overlap in at least one cohort and
Q fails qualification. Judgments use unrounded samples. A G is not
automatically the separate 5% diagnostic gain. The six suffix-0 controls
remain unranked and in every raw matrix.

| Bytes | Path | Count | R/S | U/S | V/S |
| ---: | --- | ---: | --- | --- | --- |
| 8 | reserved | 16 | 1.099/1.095 L | 1.001/0.994 O | 1.012/1.006 O |
| 8 | reserved | 256 | 1.012/0.970 O | 0.983/0.974 O | 0.986/0.972 O |
| 8 | reserved | 4096 | 0.944/0.995 O | 0.948/1.056 Q | 0.946/0.997 O |
| 8 | growth | 16 | 1.051/1.005 O | 1.000/1.027 O | 1.014/1.000 O |
| 8 | growth | 256 | 1.026/0.996 O | 1.066/1.040 O | 1.004/1.005 O |
| 8 | growth | 4096 | 0.999/0.997 O | 1.007/1.008 O | 1.001/1.003 O |
| 8 | reuse | 16 | 1.129/1.081 L | 0.980/0.995 O | 0.996/1.023 O |
| 8 | reuse | 256 | 0.967/0.992 O | 0.964/0.999 O | 0.969/1.003 O |
| 8 | reuse | 4096 | 0.998/1.001 O | 0.999/1.001 O | 0.999/1.001 O |
| 8 | suffix-1 | 16 | 0.999/0.997 O | 1.012/0.998 O | 1.001/1.031 O |
| 8 | suffix-1 | 256 | 1.000/1.001 O | 1.001/1.000 O | 1.002/1.001 O |
| 8 | suffix-1 | 4096 | 1.022/0.999 O | 0.997/1.001 O | 0.999/1.000 O |
| 8 | suffix-2 | 16 | 1.030/0.997 O | 0.988/0.995 O | 0.987/0.987 G |
| 8 | suffix-2 | 256 | 1.038/0.999 O | 0.995/0.996 O | 0.997/0.995 O |
| 8 | suffix-2 | 4096 | 1.037/0.996 O | 0.997/0.996 O | 0.999/0.994 O |
| 8 | suffix-3 | 16 | 1.003/0.999 O | 0.988/1.014 O | 0.985/0.983 O |
| 8 | suffix-3 | 256 | 1.000/1.002 O | 0.986/0.983 O | 1.016/0.987 O |
| 8 | suffix-3 | 4096 | 1.001/1.002 O | 1.029/0.984 O | 0.985/1.015 O |
| 256 | reserved | 16 | 1.000/1.002 O | 0.985/0.986 O | 1.364/1.366 L |
| 256 | reserved | 256 | 1.004/1.003 O | 0.996/0.998 O | 1.390/1.406 L |
| 256 | reserved | 4096 | 1.000/1.000 O | 0.998/1.000 O | 1.364/1.364 L |
| 256 | growth | 16 | 1.001/1.006 O | 0.998/1.001 O | 1.279/1.285 L |
| 256 | growth | 256 | 1.001/0.995 O | 0.999/1.000 O | 1.281/1.273 L |
| 256 | growth | 4096 | 1.001/0.996 O | 0.988/0.991 O | 1.273/1.272 L |
| 256 | reuse | 16 | 0.999/0.995 O | 0.985/0.994 O | 1.382/1.380 L |
| 256 | reuse | 256 | 0.999/1.004 O | 1.014/0.998 O | 1.392/1.397 L |
| 256 | reuse | 4096 | 0.998/1.003 O | 0.996/1.001 O | 1.361/1.363 L |
| 256 | suffix-1 | 16 | 0.750/0.751 G | 0.915/0.916 G | 2.135/2.141 L |
| 256 | suffix-1 | 256 | 0.754/0.749 G | 0.917/0.915 G | 2.138/2.139 L |
| 256 | suffix-1 | 4096 | 0.751/0.752 G | 0.918/0.916 G | 2.145/2.143 L |
| 256 | suffix-2 | 16 | 0.996/0.988 O | 1.451/1.447 L | 2.323/2.312 L |
| 256 | suffix-2 | 256 | 1.011/1.014 O | 1.489/1.487 L | 2.389/2.407 L |
| 256 | suffix-2 | 4096 | 1.021/0.980 O | 1.495/1.442 L | 2.414/2.307 L |
| 256 | suffix-3 | 16 | 1.320/1.321 L | 1.428/1.431 L | 2.030/2.034 L |
| 256 | suffix-3 | 256 | 1.321/1.321 L | 1.425/1.426 L | 2.026/2.030 L |
| 256 | suffix-3 | 4096 | 1.321/1.323 L | 1.445/1.425 L | 2.028/2.029 L |

U meets the first-trial gain screen in exactly three useful cells: wide
suffix-1 at each population. R meets the same screen there and is faster than
U: U/R is 1.216–1.221 across those six cohort comparisons. U has six clear
counterexamples: every wide suffix-2 and suffix-3 cell is at least 5% slower,
with disjoint sample ranges in both cohorts. R has five such counterexamples:
scalar reserved/reuse at 16 (9.5–9.9% and 8.1–12.9% slower), and wide
suffix-3 at every population (32.0–32.3% slower). V has one range-separated
gain, scalar suffix-2 at 16 (ratios 0.987075/0.987091), below the 5%
diagnostic threshold, so it has zero qualifying diagnostic gains. All 18
wide useful V cells are clear regressions (27.2–141.4% slower).
R/U/V have respectively 10/16/7 cells with lower medians in both cohorts,
11/8/24 with higher medians in both, and 15/12/5 with opposite median
directions. Small or opposite-direction differences are not promoted to
repeatable gains. The independent target reduction uses the observed-range
test against each cohort's median-slower Rust/C++ peer, with duration and
stability qualification. Its counts are:

| Arm | Qualified passes | Robust deficits | Inconclusive | Unranked controls |
| --- | ---: | ---: | ---: | ---: |
| S | 13 | 13 | 10 | 6 |
| R | 13 | 14 | 9 | 6 |
| U | 13 | 16 | 7 | 6 |
| V | 7 | 26 | 3 | 6 |

All 36 useful native comparisons in each arm pass the duration/cohort
qualifications; an inconclusive target has sample overlap or a tie. The
descriptive median-only wins are 14/14/14/7 and do not replace these counts.
No arm meets the every-cell native comparison. Target-class changes can
reflect native variation or overlap and are separate from WF/S changes.

All useful timing samples exceed 1 ms; the smallest WF one is 1.511 ms.
The 206/198/198/181 shorter rows in S/R/U/V are all suffix-0 controls.
For useful WF cohort cells, `(max - min) / median > 10%` occurs in S twice
(cohort 1, wide growth/256 and suffix-2/256), R once (cohort 1, wide
suffix-2/4096), U twice (cohort 0, scalar reuse/16; cohort 1, wide reuse/16)
and V never. U's scalar reserved/4096 has opposite median directions and
11.4% cohort-ratio spread. Rust/C++ inter-arm median drift versus S is at
most 5.65% for R, 6.30% for U and 6.40% for V, below the 10% screen. These
checks qualify this single outer order; they do not establish repeatability
across image orders or formal confidence intervals.

The discriminator separates code mechanisms from a general optimization
claim. V shows that the second pass is unnecessary to remove truncate calls,
but its retained loops/materialization accompany large wide regressions. U
improves on V yet loses to stock on wide suffix-2/3; R's different surviving
boundaries perform better on suffix-1. No measured loss is apportioned to one
call, stack frame, copy or compiler pass. These counterexamples reject using
any tested pipeline as a general solution here, while leaving other costed
inlining arrangements untested. The original code-only screen stays failed;
production O2, other targets, full-corpus and incremental-cost evidence remain
absent. No production policy or compiler source was changed.

Reproduce the native-target reduction with the maintained
`summarize-ecosystem.pl --targets`, supplying one raw arm at a time; for
example, from this experiment directory:

```sh
perl summarize-ecosystem.pl --targets \
  vector=vector-library/ecosystem-pipeline-f-s-samples.csv
```

For each candidate/S cell, the observed lower and upper ratios are
`minimum candidate / maximum S` and `maximum candidate / minimum S` within
each cohort. G requires both uppers below one; L requires both lowers above
one. Qualify using paired minimum at least 1 ms and
`max(cohort median ratio) / min(cohort median ratio) - 1 <= 0.10`.
The 5% diagnostic screen additionally requires both median ratios at most
0.95 and native drift within 10%. The local one-shot reducer
`reduce-runtime-f.py` and `runtime-f-reduction/{comparisons.csv,summary.json}`
under the original private directory preserve this reduction for independent
review; the maintained target reducer, formulas and committed raw matrices
are sufficient to reproduce it without that scratch helper.

Raw CSV SHA-256 values, in S/R/U/V order, are
`e83ac712ce9af0f9e619a77aaab8bb0c25e46d749aa7db296d2703df3ec3b90a`,
`4dc6693cead0a59e33dda2851b79e56b4a9c2ffc0eebb37a863e0c72bbd1929b`,
`0b6c190b4215cda83b4b6a3027f530428b825b949551d30ad11a173693f07d05`
and `7c64f2694a332de015631e0548200378751fabe5fd4f944a53b1a4c24b1e9b44`.
