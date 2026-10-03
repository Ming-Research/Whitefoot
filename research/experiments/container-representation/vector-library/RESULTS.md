# Growable vector library costs

Current execution order is one container and one API at a time. Vector append
is active; other Vector APIs and container families are paused. Complete-trace
performance qualification follows only after the individual APIs qualify.

| Active append path | Established evidence | Remaining qualification |
|---|---|---|
| Spare capacity | Frozen A/B images passed all six cells in the paired RAW-clock comparison; this does not qualify H or production code | Reconfirm on the final source-compiled implementation |
| Capacity growth | Repeated unchanged-H measurements pass three of six canonical cells; scalar4096, wide16 and wide256 remain unresolved. H is a hand-edited IR prototype. The exact-H C native probe supplies no new optimization selection | Qualify a source-compiled implementation and every matched cell; keep initial-capacity policy cells separately visible |

The [inline-owner experiment](#inline-runtime-slots-owner-registered-descriptor-placement-discriminator)
changes only descriptor placement relative to the retained split-payload
prototype, preserving its allocated empty placeholder. Neither prototype is
a production layout or a completed append optimization. Dated results below
apply to their recorded revisions and measurement conditions.

The current Vector library source retains [source discriminator F](#f-paired-timing-useful-improvements-without-a-separated-regression).
The current compiler retains provisional terminal-owned-consumption lowering.
The owner withdrew uniform function-actual `inlinehint` emission after the
[latest factor-isolation result](#actual-compiler-factor-isolation-after-ownership-integration)
found no independent native effect from it; the traversal choice retains its
recorded attribution and selection limits. The
[H1/H2 realloc probes](#h-realloc-for-runtime-slots-growth) remain rejected.
Earlier sections retain historical evidence under their recorded conditions.

## Current standard-container comparison

The opt-in `ecosystem-*` targets implement the question and criteria in
[ECOSYSTEM.md](../ECOSYSTEM.md). They import the current
[`std::collections::vector`](../../../../lib/std/collections/vector/module.wfm)
module and compare complete traces with Rust `Vec` and C++ `std::vector`.
The four C variants remain source-composition controls. All implementations
are freshly built at normal O3 against the same recorded source revision;
the historical O2 samples below are separate evidence.

The native adapters own the entire trace behind one C ABI call. `Vec` uses
its own push, insert, remove, swap-remove and drain operations. C++ uses its
ordinary vector operations, moving each removed value to the consumer before
erasing it because erase does not return an owner. Every word of the scalar
or 256-byte inline record enters the same ordered digest. Rust records are
neither `Copy` nor `Clone`; C++ records delete copy construction and assignment.
Neither native record adds an allocation. These values do not establish
nested-owner or expensive-destructor performance.

No trace keeps a reference into the collection across mutation. Required
order is the consumed result sequence, including the replacement by the last
item after swap removal. The retained prefix and backing survive suffix
cycles and are fully consumed and released afterward. Whitefoot's static
ceiling is 8193; tested logical populations never require an operation beyond
that ceiling, and native growth capacities remain unconstrained.

The ecosystem matrix includes reserved, growth, reuse, and suffix removal
counts zero through three, at populations 16, 256 and 4096, with both payloads.
The suffix-zero cell is an overhead control: it retains the initial prefix
and consumes it at the end, with no values removed during each cycle. Treat
it separately from comparisons of useful container mutations. Correctness
also includes populations 0, 1, 2, 3, 8, 63 and 8192, rounds 0, 1 and 3, and
seeds 0, 17 and `UINT64_MAX`. Its independent arithmetic oracle constructs
no vector and derives every consumed seed and word from the logical trace.

Run construction, correctness, accounting and timing as separate guarded
commands from the repository root, replacing `<target>` in this command:

```sh
perl .github/run-check.pl vector-ecosystem \
  make -C research/experiments/container-representation/vector-library <target>
```

The targets are `ecosystem-build`, `ecosystem-check`, `ecosystem-account`, and
`ecosystem-measure`. The last two write `.build/ecosystem/accounting.csv` and
`.build/ecosystem/measurements.csv`. The native compiler identities and flags
are in `.build/ecosystem/configuration.txt`. Keep them with source/compiler
identities and any reported samples. These files retire with this comparison
or a maintained successor that preserves its evidence.

`ECO_ACCOUNT` and `ECO_SAMPLE_FILE` override those output paths.
`ECO_WORK` defaults to 1048576 and `ECO_REPEATS` to 7. For the three full-chain
paths, rounds are `max(1, ECO_WORK / count)`. For suffix paths, rounds are
`max(1, ECO_WORK / max(removed, 1))`. Each timed call completes one trace.
Each cell warms every implementation with at least one round, otherwise one
sixteenth of the timed rounds; two cohorts then rotate implementation order
by sample, with the second cohort reversing that rotation. CSV rows retain
work, rounds, cohort and sample, so an extended bounded run remains explicit.
Whole-trace ratios include setup, mutation, consumption and cleanup; they
are not isolated append, growth or truncation latency. Check short cells
before treating their ratios as stable.

Timed images use ordinary allocation and have no observer hooks. Allocation
images use the same native algorithms with accounting hooks; they run three
rounds per cell. Requested-byte peaks exclude the observer's private header
and are neither RSS nor allocator-resident bytes. Rust's ordinary `System`
reallocation is preserved: `peak_bytes` is logical live requested storage,
while `peak_overlap_upper_bytes` also permits old/new requests to overlap at
reallocation and does not assert that this overlap actually occurred.
`requests` includes successful reallocations, `realloc_requests` counts them
separately, and `releases` counts final deallocations; a complete native trace
has `requests == releases + realloc_requests`. Whitefoot and C additionally
check independently calculated request, requested-byte and peak formulas.
The check target exercises a corrupted checksum and an unreleased allocation,
requiring the corresponding failures, and rejects observer symbols in the
timed image. No new specification rule is selected by this comparison.

### Verified correctness and allocation observations

The current guarded build and both correctness images completed successfully.
Each image passed 1,260 configurations and 8,820 complete trace
executions, for 17,640 executions across timed and accounting builds.
The negative checksum and cleanup controls produced their expected rejection
messages; the target also checked that observer hooks are absent from the
timed image. The allocation CSV contains 294 rows. Independently reading
the CSV confirms equal complete checksums within every application cell,
balanced allocation lifetimes, and the stated peak upper bound; every C
control's allocation columns equal Whitefoot's corresponding row.

The existing O2 normal and retained drivers also passed 1,260 configurations
and 6,300 executions each, totaling 12,600 historical-control executions with
the current module imports. Their retained-helper call checks passed. This
requalifies those reproduction paths without adding historical-series timing.

The preserved [allocation CSV](ecosystem-accounting.csv) SHA-256 is
`ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`. These are requested-storage observations
from the accounting build, separate from the timing samples below. Build/check
phase durations belong to the central [experiment record](../ECOSYSTEM.md).

At population 4096, the three-round growth trace produced the following
allocation totals. All four C controls have the Whitefoot entries shown.

| Payload bytes | Implementation | Requests | Reallocations | Releases | Requested bytes | Logical peak bytes | Possible-overlap upper bytes |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | Whitefoot / C controls | 45 | 0 | 45 | 393,912 | 98,336 | 98,336 |
| 8 | Rust Vec | 36 | 33 | 3 | 393,120 | 65,536 | 98,304 |
| 8 | C++ std::vector | 42 | 0 | 42 | 393,192 | 98,304 | 98,304 |
| 256 | Whitefoot / C controls | 45 | 0 | 45 | 12,582,864 | 3,145,760 | 3,145,760 |
| 256 | Rust Vec | 36 | 33 | 3 | 12,579,840 | 2,097,152 | 3,145,728 |
| 256 | C++ std::vector | 42 | 0 | 42 | 12,582,144 | 3,145,728 | 3,145,728 |

The 36 Rust requests consist of three initial allocations and 33 successful
reallocations. Three final releases therefore complete its allocation
lifetimes; comparing release counts alone with Whitefoot's 45 would be
misleading. Rust's lower logical peak does not establish a lower physical
peak: its possible-overlap bound is nearly the C++ peak and the observer
does not measure whether System realloc moved its backing.

Reserved traces at this population use six requests/releases for Whitefoot
and three for either native vector across three rounds. The requested totals
are 98,424 versus 98,328 bytes for scalars, and 3,146,592 versus 3,146,496 for
wide records. Whitefoot constructs and replaces its empty header-backed
window; the native reserve starts without that heap-owned empty header.
Reuse and all four suffix paths use two Whitefoot requests/releases and one
native request/release for the entire trace. Their peaks are 32,808 versus
32,776 bytes for scalars and 1,048,864 versus 1,048,832 for records. Thus the
extra initial request is visible even where large-population byte totals are
close. These policy and representation observations motivate timing; they
do not assign a causal elapsed percentage.

### Fresh practical timing

The complete [timing samples](ecosystem-samples.csv) and
[allocation observations](ecosystem-accounting.csv) are preserved beside this
record. They serve the standard-container comparison and remain its evidence
until it is retired or superseded with that evidence preserved. Timing source
revision is `0c3203aa6111f14247aa950e3794e83082d4f29c`. The measurement phase
completed in 85.578 seconds, separate from construction and correctness.
The timing CSV has 4,116 rows and SHA-256
`6d505c349a3820b28e1a707e6167aa2f8bd06bca970d8867ead928b2f4087898`.

All measured cells use `ECO_WORK=1048576`, seven ranked samples per
implementation per cohort, and separately executed warmup. An independent
read of the raw CSV verified the complete payload/path/population/variant
matrix, sample IDs, both cohorts, exact checksums, and the shared summarizer's
minimum, median and maximum values. These are fresh O3 controls and native
baselines; no historical sample supplies a denominator.

At population 4096, ranges below span the two cohort medians. Ratios divide
Whitefoot's median by the comparator's median in the same cohort: above one
means Whitefoot took longer. Time is the complete sample, including its stated
rounds/repetitions, setup, all consumed words and cleanup. Path names have
different operation denominators, so neither the time nor the ratio is an
isolated operation latency.

| Payload bytes | Path | WF whole trace ms | WF / Rust | WF / C++ | WF / take-swap C | WF / direct C |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | reserved | 3.343–3.375 | 2.136–2.169 | 1.578–1.609 | 1.585–1.613 | 1.941–2.055 |
| 8 | growth | 3.749–3.766 | 1.896–1.945 | 1.537–1.546 | 1.494–1.524 | 1.795–1.819 |
| 8 | reuse | 3.276–3.279 | 2.173–2.175 | 1.571–1.575 | 1.614–1.616 | 2.012–2.018 |
| 8 | suffix-0 (control) | 1.342–1.360 | 1.328–1.335 | 1.322–1.343 | 1.343–1.353 | 0.471–0.483 |
| 8 | suffix-1 | 4.550–4.599 | 2.892–2.922 | 1.512–1.540 | 1.382–1.383 | 1.610–1.622 |
| 8 | suffix-2 | 3.639–3.659 | 2.774–2.789 | 2.206–2.222 | 1.064–1.077 | 1.238–1.360 |
| 8 | suffix-3 | 2.769–2.809 | 2.106–2.131 | 1.582–1.594 | 1.146–1.180 | 1.966–1.982 |
| 256 | reserved | 50.688–50.995 | 1.232–1.233 | 1.157–1.165 | 1.172–1.179 | 1.176–1.183 |
| 256 | growth | 62.639–63.039 | 1.506–1.513 | 1.194–1.198 | 1.113–1.123 | 1.133–1.140 |
| 256 | reuse | 50.821–51.734 | 1.239–1.250 | 1.158–1.175 | 1.170–1.177 | 1.163–1.167 |
| 256 | suffix-0 (control) | 2.261–2.270 | 1.953–1.959 | 0.770–0.774 | 0.532–0.533 | 0.769–0.770 |
| 256 | suffix-1 | 45.574–45.687 | 2.843–2.865 | 2.714–2.728 | 2.523–2.554 | 2.878–2.886 |
| 256 | suffix-2 | 43.613–43.946 | 1.993–1.995 | 1.996–2.006 | 1.955–1.956 | 1.984–1.998 |
| 256 | suffix-3 | 43.844–43.881 | 1.728–1.730 | 1.667–1.669 | 1.398–1.398 | 1.734–1.734 |

All 36 mutating workload cells (six paths, two payloads, three populations)
show Whitefoot more than 10% slower than both native vectors in both cohorts.
This is a triage result for these synthetic traces, without an application
frequency weighting. The largest representative wide-value gap is suffix-1:
2.843–2.865 against Rust and 2.714–2.728 against C++. A gap of 2.523–2.554
also remains against the C take/swap control, which follows the library's
transfer order and allocation policy. That makes emitted transfers and
callback boundaries a useful next discriminator; it does not yet separate
WF source composition, ABI choices and optimizer behavior into causal shares.

The scalar reserved/reuse paths at population 4096 are 2.136–2.175 times Rust
and 1.585–1.616 times take/swap C. Their direct-C ratios of 1.941–2.055 show
that changing the consumption composition remains a separate comparison.
The older reverse-C composition happens to be much closer (1.164–1.177 here);
that one control cannot stand in for the ordinary native-library outcome.
No subtraction of those whole-trace times attributes a causal percentage.

Size matters. Scalar reuse is 2.881–2.890 times Rust at population 16 versus
2.173–2.175 at 4096. Scalar growth moves from 1.199–1.212 to 1.896–1.945;
wide growth moves from 1.206–1.208 to 1.506–1.513. Different capacities and
allocation/reallocation policies accompany those changes, so this is a
follow-up question, not proof that a specific allocation explains the gap.
Wide suffix-1 remains large across populations: the Rust ratio is
2.843–2.887 across all six cohort/population observations. The record's
32-word digest is included throughout; these figures do not isolate movement
bandwidth or establish nested-owner behavior.

Every native comparison for a mutating path has paired sample minima of at
least 1 ms and cohort-ratio spread below 10%. The only greater-than-10%
cohort spread among operational C comparisons is scalar suffix-2 at
population 256 against direct C: 1.574 versus 1.147, a 37.190% spread.
That particular attribution remains inconclusive pending a focused replay;
it supplies no conclusion above or in the population-4096 table.

Fourteen comparator cells have a paired minimum below 1 ms, all in suffix-0:
scalar values at all three populations against reverse C, take/swap C, Rust
and C++, plus wide values at populations 16 and 256 against Rust. Their
minimum samples are 0.975–0.997 ms. Suffix-0 is retained as an unranked
overhead control, including the displayed ratios; it makes no operational
performance claim. Replaying the whole matrix merely to promote this control
would not resolve a current native-container ranking question.

### Append placement experiment: criteria recorded before running

The next source trial keeps the comparison above and its raw files frozen.
It changes only append's library implementation: choose the existing growth
capacity when full, reserve once, then place the incoming owner at one shared
site.
The empty, doubled and saturated capacities, allocation sequence, public
contracts and every consuming callback remain unchanged. This is a general
library control-flow change for every element type and ceiling, with no
payload- or workload-specific branch. The selected suffix-consumption
algorithm and header-first storage representation are not changed.

Inspection of the baseline O3 native image gives a concrete discriminator.
Scalar append remains a call inside `vector_library_work`. Wide append copies
all 256 incoming bytes to a stack snapshot before its capacity test; its
spare-capacity branch then reads the original argument again. Wide suffix-1
has no first-half exchange, and the optimized truncate remainder reads the
backing directly into the digest. Its gap therefore does not by itself
indict the take/swap algorithm or establish a callback-copy cost. The four
placement branches are a plausible cause of append's retained call and
unnecessary hot-path snapshot, not yet a measured explanation.

Before selecting the candidate:

- Compile the ordinary library and pass the existing scalar, owning and
  must-consume vector program in both lowering modes. Pass the full ecosystem
  behavior and allocation checks, including the negative controls. The
  allocation columns must remain identical to the baseline for every cell.
  The formal scalar chain now appends a third value at ceiling three, checks
  that length and capacity, and removes/checks that owner before its existing
  insertion. This exercises append's saturation branch, which neither the
  previous program nor the ceiling-8193 timing fixture reached. It moves the
  previous scalar growth allocation earlier; owning insert still exercises
  saturation and the program's allocation expectation remains unchanged.
- Compare final O3 code with the same source fixture, flags and compiler
  implementation. Check whether append calls disappear from the fill/tail
  loops or whether the no-growth record snapshot disappears. Raw IR copies
  alone do not answer that question. Unchanged calls and snapshot falsify
  this particular explanation even if an elapsed-time difference appears.
- Measure the same complete matrix in both order cohorts with baseline and
  candidate artifacts kept separate. Wide suffix-1 and scalar reserved/reuse
  are the primary affected cells; growth and other suffix sizes expose costs
  from the changed control flow. Require the existing duration and cohort-stability
  qualifications before ranking. A reproducible target-cell improvement
  without a useful-cell regression supports retaining the simplification;
  otherwise record the failure and revisit the explanation. This first trial
  does not promise to reach the owner's final native-comparator target.

Insertion/removal still use the compiler's generic logical-index shift. A
contiguous Slots bulk move is a separate candidate for reserved/reuse, with
its own semantic and design review; combining it with append would prevent
this trial from isolating the branch-shape change. Direct forward consumption,
prefix rotation, optional-element storage and blanket inlining are not
selected by this diagnosis. The earlier refusal grounds below still apply.

The first source form did not reach code generation. After joining its
capacity-selection branches, `place_back` could not prove `len < cap`
(FN-8). The complete unchanged fixture instead first reported its caller's
fill-loop backedge (INV-1), because append's proof was unavailable to that
caller. A reduced caller with no postcondition or following mutation exposed
the callee's exact failure. Keeping reserve calls inside the capacity branches
and restoring the original `spare > 0` branch polarity still left the shared
placement obligation unproved. Explicit `len < cap` facts inside every branch
also failed to establish that relation after the join. These observations
concern source proof structure and diagnostic visibility, not measured
performance or a demonstrated acceptance defect.

A reduced helper variant compiled successfully: a private
`grow_vector_make_room` uses the existing PriorityQueue helper's control-flow
shape, proving `cap > len`, nondecreasing capacity and unchanged length at
each return. Append receives that ordinary call contract and places its
incoming owner once. The candidate now uses this shape. The helper owns only
capacity preparation, receives no element owner and adds no public interface.
Its cost is an additional potential helper boundary, so the final-code and
timing discriminators above still decide whether this separation helps.
No caller invariant, contract or runtime behavior was weakened, and no
compiler implementation or specification rule was changed for these probes.

### Helper candidate result: useful intermediate, not selected

The first measured candidate fails the no-useful-regression criterion.
Scalar reserved at population 16 takes 1.070–1.072 times the fresh baseline
in the two cohorts, while its Rust and C++ control medians are slightly
lower. Normalizing by those controls does not remove the regression:
the candidate/baseline ratio of WF/Rust is 1.103–1.107, and of WF/C++ is
1.075–1.099. This candidate is preserved as an intermediate observation,
not selected as the final implementation.

Its exact source is
[`f48e620b0aceaf592a57d2a91dd2ee5df0758a6e`](https://github.com/mbbill/Whitefoot/tree/f48e620b0aceaf592a57d2a91dd2ee5df0758a6e).
The [fresh baseline repeat](ecosystem-append-baseline-samples.csv) and
[helper samples](ecosystem-append-room-samples.csv) each contain 4,116 rows
with identical work, rounds, complete checksums and sample coverage.
Their SHA-256 hashes are, respectively,
`e8c7935478034909812795846fd6c2bdb18ea96f5baedc0b6edda820155b8a3b` and
`ea0b9176f2c89b16f6ce8d6db0ebc1b74763db8002a2277b7ce492c36e0da6c2`.
The first comparison above remains frozen. Its WF medians and the fresh
baseline repeat differ by factors 0.970–1.062 across the mutating cells;
those older samples corroborate the baseline but are not pooled or used as
the before/after denominator here.

Both ecosystem correctness images pass their full 1,260 configurations and
8,820 executions, including the expected checksum/cleanup failures. The
formal vector program passes in both lowering modes with its new append
saturation observation. The 294-row allocation CSV is byte-identical to the
preserved [baseline accounting](ecosystem-accounting.csv), so this trial
changes neither allocation policy nor requested storage. The native C driver,
C++ object and Rust archive are also byte-identical between the two builds.
Across all mutating cells/cohorts, unchanged Rust median drift is
0.942–1.042, C++ drift 0.929–1.042 and take/swap C drift 0.920–1.052.

Final O3 code satisfies the recorded discriminator: append inlines into the
scalar and wide fill/tail loops. The wide tail constructs directly in the
backing after `make_room`, eliminating its former record temporary and
snapshot. The standalone wide append still has a snapshot, but these loops
no longer call it. `make_room` remains a call per append, and the wide tail
and truncate remain calls per suffix cycle. This is evidence about the whole
source change; elapsed gains are not assigned solely to eliminated byte
traffic or one call boundary.

At population 4096, ranges below cover the two cohort medians. A/B divides
helper-candidate WF time by fresh-baseline WF time. Ratios against native
libraries are whole-trace observations under the original comparison contract.

| Payload bytes | Path | Candidate WF ms | A/B | WF / Rust | WF / C++ |
| ---: | --- | ---: | ---: | ---: | ---: |
| 8 | reserved | 3.336–3.358 | 0.983–1.012 | 2.179–2.186 | 1.580–1.640 |
| 8 | growth | 3.804–3.838 | 0.959–1.023 | 1.910–1.941 | 1.531–1.586 |
| 8 | reuse | 3.313–3.331 | 0.976–1.012 | 2.204–2.206 | 1.599–1.606 |
| 8 | suffix-1 | 2.883–2.960 | 0.632–0.635 | 1.852–1.882 | 0.972–1.001 |
| 8 | suffix-2 | 2.498–2.625 | 0.683–0.725 | 1.874–2.005 | 1.508–1.594 |
| 8 | suffix-3 | 2.258–2.423 | 0.818–0.845 | 1.713–1.801 | 1.286–1.346 |
| 256 | reserved | 42.661–42.744 | 0.832–0.835 | 1.026–1.033 | 0.967–0.970 |
| 256 | growth | 54.220–54.593 | 0.846–0.854 | 1.285–1.307 | 1.024–1.045 |
| 256 | reuse | 42.600–42.702 | 0.823–0.842 | 1.036–1.042 | 0.967–0.973 |
| 256 | suffix-1 | 23.699–23.725 | 0.513–0.519 | 1.486–1.491 | 1.411–1.412 |
| 256 | suffix-2 | 24.085–24.162 | 0.545–0.555 | 1.100–1.101 | 1.090–1.100 |
| 256 | suffix-3 | 28.005–28.032 | 0.633–0.635 | 1.090–1.100 | 1.053–1.053 |

Across all populations, 29 of the 36 mutating cells have lower WF medians in
both cohorts, two have higher medians and five have mixed directions. Besides
the scalar reserved regression, scalar growth at population 16 is 1.005–1.007
times baseline; that small difference is descriptive. Every wide path
improves in both cohorts: full-chain A/B ratios are 0.819–0.885, suffix-1
0.509–0.519, suffix-2 0.545–0.559 and suffix-3 0.627–0.635. Scalar suffixes
also improve, while scalar large reserved/reuse remain essentially unchanged.
At population 4096, wide suffix-1 still costs 1.332 times take/swap C, and
scalar reserved costs 1.607–1.621 times that matched control. The remaining
whole-trace gaps therefore require further source/code-generation comparison;
they are not explained by the native libraries' different growth policies.

Under the current target reducer's sample-separation qualification, three
cells pass against the slower standard comparator: wide growth at 256 and
wide reserved/reuse at 4096. Twenty remain deficits and thirteen are
inconclusive because their observed sample ranges overlap. All six suffix-0
cells remain unranked controls. Every mutating native comparison has samples
of at least 1 ms and cohort-ratio spread below 10%; the one unstable C
comparison is scalar suffix-2 at 256 against direct C (14.934%), which supports
no attribution here. Every sub-millisecond observation belongs to suffix-0.

### Next source trial: inline spare-capacity append

Before measuring the second source candidate, keep the first candidate's
compiler, native image and samples separate. Add an ordinary spare-capacity
test to append: place and return on that branch; call `make_room` and place
on the full-capacity branch. Two placement sites replace the original four;
the helper receives no incoming element owner and runs only when growth is
required. Its public contracts, proof obligations, growth policy, ownership,
callback order and benchmark inputs stay fixed.

The discriminators are removal of the helper call from the no-growth path,
recovery of the scalar population-16 reserved regression, and preservation
of the first candidate's wide-value gains. Inspect final code before timing:
duplicate placement could inhibit inlining or restore the wide snapshot,
which would falsify the proposed improvement. Pass the same complete
correctness and accounting matrix, then measure the entire previous timing
matrix in both cohorts rather than only the favorable suffix cells. A new
useful-cell regression prevents final selection. The extra initial header
allocation remains visible in the accounting above; this source trial makes
no allocation-policy change or claim about its causal time share.

### Spare-capacity candidate result: scalar recovery with wide regressions

The second source candidate also remains an intermediate result. It recovers
the scalar population-16 reserved regression: 4.197–4.276 ms, or
0.775–0.794 times the fresh original baseline and 0.725–0.741 times the
first helper candidate. However, at populations 256 and 4096, wide suffix-1
regresses to 1.485–1.493 times the helper candidate, suffix-2 to 1.420–1.430,
and suffix-3 to 1.232–1.248. Both cohorts show these regressions while native
controls remain stable. This fails the recorded wide-gain preservation
criterion and prevents final selection.

The measured source is
[`0b3a58dc55df950bf155236bf148b2c0ed86bc32`](https://github.com/mbbill/Whitefoot/tree/0b3a58dc55df950bf155236bf148b2c0ed86bc32).
The preserved [spare-capacity samples](ecosystem-append-fastpath-samples.csv)
contain the complete 4,116-row matrix; work, rounds, checksums and sample IDs
match both earlier runs. Their SHA-256 is
`b4a2821549beab92f7032ed8f51f05fcd68457f70f1ab30aa8c30c008fdcf8b3`.
The frozen compiler SHA-256 is
`b54e1664d077b08675f5fac1d5768ef261be3400e70bf05099a5e8b8e0448154`,
the timed native image is
`5c18615e2b6c7598cd93a0ce54e79f99df1ffc02a3d72b32ce3190a72f3faabc`,
and its rewritten WF IR is
`e9b46bc10bd342c5e94447a01527ee9569a2613fe558a51fb4500c2c639e74ea`.
Construction uses the existing family targets with
`BUILD=.build/append-fastpath`; timing keeps `ECO_WORK=1048576` and
`ECO_REPEATS=7`.

Guarded compiler construction took 8.25 s; ecosystem construction 5.00 s,
correctness 1.79 s and accounting 0.21 s. Corpus construction took 0.53 s
and the focused vector program 3.55 s; the complete validation command took
19.79 s. Timing ran separately for 81.53 s. All completed with exit zero.
Both ecosystem images again pass 1,260 configurations and 8,820 executions,
including their expected negative controls, and the formal owner program
passes both lowering modes. Accounting is byte-identical to the 294-row
baseline. The C driver, C++ object and Rust archive remain byte-identical.

Final code and optimization remarks explain the intended improvement without
a global inline directive. In the original source, reserve is expanded into
three append branches; append's reported scalar/wide inline costs are
485/495 against threshold 250. In this candidate, append costs 60/70 and
inlines into the fill/tail loops, while `make_room` costs 350 and stays behind
the full-capacity branch. The spare path constructs directly in the backing
with no helper call or record snapshot. These are this optimizer's cost
estimates, not instruction counts or a portable compiler policy.

The wide suffix regression has a separate concrete code difference. The
wide truncate body is identical in the two candidates, but record stores
change: the helper candidate starts with a scalar pair and 16-byte-aligned
vector pairs, while the spare-capacity candidate writes the first word and
then vector chunks at offsets 8, 24 and subsequent 16-byte steps. Immediate
paired-word consumption overlaps those stores differently. Store forwarding
is a hypothesis for the slowdown; neither these instructions nor elapsed
times alone identify a hardware stall or its causal share. The wide tail
frame shrinks from 320 to 288 bytes, which by itself does not predict elapsed
time.

Population-4096 cohort ranges follow. Baseline is the fresh original repeat;
helper is the first measured candidate. All ratios cover complete traces.

| Payload bytes | Path | Candidate WF ms | / baseline | / helper | WF / Rust | WF / C++ |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | reserved | 1.683–1.692 | 0.499–0.507 | 0.501–0.507 | 1.102–1.107 | 0.803–0.825 |
| 8 | growth | 2.084–2.095 | 0.521–0.564 | 0.543–0.551 | 1.075–1.079 | 0.838–0.862 |
| 8 | reuse | 1.652–1.680 | 0.495–0.502 | 0.496–0.507 | 1.098–1.108 | 0.796–0.814 |
| 8 | suffix-1 | 2.939–2.971 | 0.638–0.644 | 1.004–1.019 | 1.890–1.912 | 0.996–1.009 |
| 8 | suffix-2 | 2.417–2.497 | 0.668–0.683 | 0.921–1.000 | 1.839–1.857 | 1.463–1.477 |
| 8 | suffix-3 | 1.852–1.860 | 0.649–0.671 | 0.768–0.820 | 1.416–1.421 | 1.063–1.067 |
| 256 | reserved | 42.141–42.188 | 0.822–0.825 | 0.987–0.988 | 1.032–1.032 | 0.966–0.967 |
| 256 | growth | 53.631–53.789 | 0.839–0.840 | 0.982–0.992 | 1.296–1.299 | 1.027–1.036 |
| 256 | reuse | 42.049–42.076 | 0.812–0.829 | 0.985–0.987 | 1.031–1.033 | 0.965–0.966 |
| 256 | suffix-1 | 35.242–35.262 | 0.763–0.770 | 1.485–1.488 | 2.214–2.224 | 2.114–2.115 |
| 256 | suffix-2 | 34.379–34.399 | 0.778–0.789 | 1.423–1.428 | 1.577–1.582 | 1.578–1.579 |
| 256 | suffix-3 | 34.528–34.583 | 0.780–0.784 | 1.232–1.235 | 1.372–1.374 | 1.319–1.319 |

All 36 mutating cells improve over the original baseline in both cohorts.
Compared with the helper candidate, 25 have lower medians, eight higher and
three mixed. Wide suffixes at population 16 improve; the six medium/large
wide suffix cells above account for the material reversals. Against the
slower standard comparator, eleven cells pass the observed-sample-separation
criterion, fourteen remain deficits and eleven are inconclusive from sample
overlap. The six suffix-zero controls stay unranked.

Operational Rust medians are 0.940–1.020 times the fresh baseline and
0.961–1.032 times the helper run; C++ ranges are 0.929–1.060 and 0.947–1.047.
Every operational native comparison meets the 1 ms duration and 10% cohort
spread qualifications. Scalar suffix-2 direct-C comparisons at populations
256 and 4096 are unstable (59.809% and 23.857%); the wide suffix-zero
take/swap C control is also unstable (26.958%). They support no attribution.
All sub-millisecond observations occur in the unranked suffix-zero control.

After merging main at
[`549ec5597fd90e60ac5da6a6fc62bb6426c9b94a`](https://github.com/mbbill/Whitefoot/tree/549ec5597fd90e60ac5da6a6fc62bb6426c9b94a),
the unchanged candidate rebuilt with `BUILD=.build/main-fastpath` has image
SHA-256 `514a7cc2ec3e936151229a486c022621fcfcfe169780b8c0681749cd361b9d9d`.
Direct Mach-O section comparison establishes byte-identical executable text,
stubs, constants, data, TLS and unwind information, with identical section
addresses, extents and zero-fill layouts. The 691,784-byte text section has
SHA-256 `c7f075ff93bc8859cc3648d29ae32326a3bc972dd26bef5697270faa8b52360b`
in both images. Every file-byte difference belongs to symbol strings and
their offsets, the string-table size, UUID or code signature; the only
resolved-symbol differences are six debug object paths changing
`append-fastpath` to `main-fastpath`. Thus the code observations above also
describe the merged-main build. This comparison adds no timing samples;
the preceding measurements retain their original revision and image identity.

### Third source discriminator: one placement with a small capacity guard

Keep append's single placement from the first helper candidate, and separate
capacity preparation into a small `make_room` guard and a private `grow_full`
helper containing the existing growth cases. The latter requires full
capacity and retains the original length/capacity guarantees; the guard
publishes those guarantees on every return. No public API, allocation policy,
callback sequence, type-specific branch or benchmark input changes.

Before measuring, freeze one compiler implementation and the same fixtures,
native sources, flags and inputs for the paired source comparison. The code
discriminator requires both a call-free spare-capacity path and direct
construction at one placement site. Record the resulting record-store
offsets and widths to determine whether the first candidate's aligned layout
returns while the second candidate's inline guard remains. The timing
criterion is preservation of the first candidate's wide gains and the second
candidate's scalar recovery, with the complete earlier behavior, accounting
and two-cohort timing matrices. A repeatable useful-cell regression prevents
selection. A changed layout and restored timing would support a source
control-flow/code-shape explanation, but scheduling and register allocation
also change; it would not establish a specific hardware-stall percentage.

### Single-placement result: wide recovery with a scalar regression

The third candidate also remains an intermediate result. It restores the
first candidate's wide gains and the second candidate's scalar reserved
recovery, but fails the recorded no-useful-regression criterion. Scalar
suffix-3 at population 16 takes 1.909–1.911 ms, versus the second candidate's
1.819–1.820 ms: a 1.049–1.051 ratio in the two cohorts. The observed sample
ranges are separated in both cohorts, with candidate minima 1.019–1.023
times the earlier maxima. Rust control drift is 0.996–1.006 and C++ drift
0.999–1.000; normalizing by their medians leaves ratios 1.042–1.055 and
1.049–1.051, respectively. Scalar suffix-3 medians also increase at 256 and
4096, by factors 1.015–1.042 and 1.039–1.042, although their cohort-0 sample
ranges overlap. Gains against the original baseline do not erase this
counterexample to preserving the second candidate's useful-cell performance.

The measured source is
[`47f9f91b63a484ba7f4924a63b5863b1b8b6f289`](https://github.com/mbbill/Whitefoot/tree/47f9f91b63a484ba7f4924a63b5863b1b8b6f289).
The [single-placement samples](ecosystem-append-single-placement-samples.csv)
contain 4,116 rows; their SHA-256 is
`63622733c1d242cfe5a0548f9d2fdf571d871151bc8c9d77a43f558a9778586e`.
Every row's work, rounds, traces, checksum and sample identity agrees with
the fresh baseline, first helper candidate and second spare-capacity
candidate. The compiler, timed image and timed LLVM hashes are, respectively,
`02f18a296656c48f08e044fb635f06cab0871ce25ea75f57343434367959e21d`,
`ee6973459e4daaccc13b7df18b91563b86ea345b30f0389844ba5da17355ccab` and
`4d6591a1eee0457b5edb2f46d312a03cd796b3d770c4ee9c04b71f3280ff4be6`.
Construction uses `BUILD=.build/single-placement-guard`; timing retains
`ECO_WORK=1048576 ECO_REPEATS=7` and completed in 80.60 s, separately from
construction and correctness.

The first helper candidate was also rebuilt with the current compiler
implementation before this comparison. Its timed LLVM and WF object are
byte-identical to the preserved first-trial artifacts. This bridge took
19.37 s including restoration of the third source and compiler; the restored
compiler, timed image and LLVM retain the hashes above. Together with the
second candidate's merged-main comparison above, this checks the compiler
revision variable without adding or pooling timing samples.

The guarded third-candidate validation completed in 21.51 s: compiler
construction 7.66 s, ecosystem construction 4.81 s, behavior checks 1.73 s,
accounting 0.22 s, formal corpus construction 0.53 s and execution 3.67 s.
Both ecosystem images pass all 1,260 configurations and 8,820 executions,
including the checksum and cleanup falsifiers. All 294 accounting rows are
byte-identical to [baseline accounting](ecosystem-accounting.csv), as are
the timed C driver, C++ object and Rust archive relative to the second
candidate. The formal vector program passes both lowering modes with its
unchanged 25-allocation release expectation. A further one-shot check took
2.69 s: changing only the new expected saturation length from 3 to 2,
capacity from 3 to 2, or removed value from 55 to 54 produced exits 23, 24
and 25, respectively; the unchanged program exited 0. Each new observation
therefore demonstrated its own failure path. Scratch variants and their
runner were removed after those observations.

Final code passes the structural discriminator. The spare-capacity branch
has no append or capacity-helper call, and constructs the wide value directly
in backing storage. Relative to the record start, it writes scalar fields at
offsets 0 and 8, paired vector stores starting at 16, 48, 80, 112, 144, 176
and 208, then scalar fields at 240 and 248. This restores the first
candidate's vector-store grouping while retaining the second candidate's
inline capacity guard. The first scalar pair uses separate stores here.
Scalar and wide truncate bodies remain identical to the first candidate
after branch-address normalization: 44 and 168 instructions. The wide tail
still runs once per suffix cycle, sets up fourteen vector constants and
spills them for the cold growth path. Wide timing recovery alongside this
code shape supports a source control-flow explanation for the second
candidate's regression; scheduling and register allocation also changed,
so it does not isolate a hardware stall or assign an elapsed-time share.

At population 4096, the ranges below cover both cohort medians. Base is the
fresh original baseline, A the first helper candidate, B the second
spare-capacity candidate, and C this one-placement candidate.

| Payload bytes | Path | C WF ms | C / base | C / A | C / B | WF / Rust | WF / C++ |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | reserved | 1.680–1.681 | 0.495–0.506 | 0.500–0.504 | 0.993–0.998 | 1.094–1.097 | 0.797–0.798 |
| 8 | growth | 2.062–2.075 | 0.515–0.558 | 0.537–0.545 | 0.989–0.990 | 1.062–1.073 | 0.849–0.852 |
| 8 | reuse | 1.649–1.660 | 0.489–0.501 | 0.495–0.501 | 0.988–0.998 | 1.096–1.105 | 0.809–0.824 |
| 8 | suffix-1 | 2.958–2.985 | 0.641–0.648 | 1.008–1.026 | 1.005–1.006 | 1.861–1.905 | 0.979–1.007 |
| 8 | suffix-2 | 2.384–2.404 | 0.658–0.659 | 0.908–0.962 | 0.963–0.986 | 1.778–1.799 | 1.425–1.447 |
| 8 | suffix-3 | 1.929–1.933 | 0.674–0.698 | 0.798–0.854 | 1.039–1.042 | 1.472–1.475 | 1.104–1.105 |
| 256 | reserved | 40.787–40.974 | 0.794–0.802 | 0.954–0.960 | 0.967–0.972 | 0.997–1.003 | 0.935–0.940 |
| 256 | growth | 53.180–54.029 | 0.832–0.843 | 0.974–0.996 | 0.992–1.004 | 1.284–1.306 | 1.026–1.037 |
| 256 | reuse | 40.777–40.802 | 0.788–0.804 | 0.955–0.958 | 0.969–0.970 | 0.999–0.999 | 0.935–0.937 |
| 256 | suffix-1 | 22.673–22.704 | 0.490–0.496 | 0.957–0.957 | 0.643–0.644 | 1.425–1.432 | 1.359–1.360 |
| 256 | suffix-2 | 23.357–23.450 | 0.528–0.538 | 0.970–0.971 | 0.679–0.682 | 1.073–1.078 | 1.074–1.077 |
| 256 | suffix-3 | 26.477–26.599 | 0.598–0.603 | 0.945–0.950 | 0.767–0.769 | 1.052–1.055 | 1.012–1.013 |

Across all 36 mutating cells, C lowers both WF cohort medians relative to the
fresh original baseline. Relative to A, 33 cells have lower medians and three
have higher medians: scalar suffix-1 at each population, with overlapping
sample ranges. Every wide cell improves relative to A in both cohorts.
Relative to B, 23 cells have lower medians, seven have higher medians and six
have mixed directions. Scalar reserved at population 16 improves further to
3.631–3.666 ms, or 0.857–0.865 times B. Large wide suffix-1 recovers to
0.643–0.644 times B and 0.957 times A. These successes satisfy the two primary
recovery aims, but the scalar suffix-3 counterexample still prevents selection.

The complete target reduction yields 11 passes, 11 deficits and 14
inconclusive mutating cells, plus six unranked suffix-0 controls. Passes are
scalar growth/reserved at 256 and 4096, scalar reuse at every population,
wide growth at 256, wide reserved at 256 and 4096, and wide reuse at 4096.
Every mutating native comparison has samples of at least 1 ms and cohort-ratio
spread below 10%. The unstable comparison is scalar suffix-2 at 4096 against
direct C (49.814%); every sub-millisecond observation is a suffix-0 control.
Across mutating cells/cohorts, unchanged Rust/C++ control drifts are
0.943–1.021 / 0.932–1.026 relative to baseline, 0.960–1.024 / 0.946–1.030
relative to A, and 0.964–1.032 / 0.948–1.035 relative to B. At population
4096, wide suffix-1 still takes 1.279–1.283 times take/swap C, while scalar
reserved takes 0.818–0.819 times that control. The remaining native gaps
therefore need path-specific comparison. The accounting and source/C controls
do not yet assign causes to those remaining gaps.

### Remaining attribution and bounded lowering probes

Scalar suffix-2/3 already inline their append, truncate and scalar digest
calls. Their final loops still take/swap values and update length while the
native adapters consume the suffix forward. The source difference is real;
its elapsed contribution is unmeasured. A read-only alias audit finds that
scalar length and digest already follow register/SSA recurrences; the matched
take/swap C loop also retains a length store per pop. This rejects the
stronger hypothesis that missing header/payload separation uniquely keeps
WF length in memory. Whether alias facts could sink those stores remains
unproved and is not grounds for a new metadata family. A direct-backing
private helper was considered as an alias discriminator, but does not explain
these present costs: the outer owner and direct digest environment already
carry `noalias`, and the kernel take/place rows already receive direct backing
references with scoped alias metadata after inlining. Optimized truncate loads
the backing once and holds length and digest in registers within each loop;
only wide truncate reloads length between its two phases. The frozen G bodies
are strictly equal to F for this observation. Passing the run directly would
neither separate its header from its payload nor remove the selected movement.
Moreover, ordinary `fn take_run(run: &Slots<u64>)` is refused by TYPE-9,
as recorded in the existing
[conformance case](../../../../tests/conformance/cases/type9-neg-runtime-capacity-outside-box.wf).
No new alias assertion, language amendment or timing probe follows from this
falsified per-iteration-reload hypothesis. The existing
[ordinary-representation refusals](#v061-copy-and-consumption-trial) and
[selected consumption contract](../../../../design/language/data-model/vector-consumption.md)
still rule out silently replacing the generic API with optional slots,
prefix rotation or a callback that cannot consume an unconstrained owner.
Wide suffix-1 instead retains a tail frame, fourteen per-call vector constants
and a digest passed through stack storage. Native constant setup is outside
the suffix cycles. Each wide digest uses sixteen paired loads and 32
multiply-add instructions; these code differences do not establish which
part explains the remaining time.

The next scratch probe added only unsigned `nuw` facts to
six checked logical-window arithmetic sites in the frozen C LLVM: one
length increment in each scalar/wide `place_back`, and the address-index and
stored-length decrements in each scalar/wide `take_back`. The source domains
`len < cap <= u64::MAX` and `len > 0` justify them, including zero-stride
values with very large logical capacities. No signed `nsw` assertion or
Ring wrapping arithmetic changes. Exact function, header-load/store and
payload-address contexts identify the six sites, and reversing those edits
recovers every other byte of the control module. The criterion recorded
before compilation required fewer scalar suffix descriptor stores/reloads
or a simpler exit-length recurrence, with wide geometry monitored for
collateral changes; unchanged hot code would end the probe without timing.
Control and changed raw modules were independently compiled at O3 and linked
with the same frozen native objects. The result is negative: optimized LLVM
differs only in its first `ModuleID` comment, and all Mach-O section bytes
and layouts match each other and the measured C image. The 691,696-byte text
section has SHA-256
`ff6b63b13941f977cf6522b587f67f91c8f6e242a123b8c93d6cd65539418cba`.
Construction took 0.612 s and both complete checksum checks took 1.665 s;
each passed 1,260 configurations and 8,820 executions. No timing followed,
no compiler change was selected, and this probe explains no runtime gap.

### Directed late inlining: wide gain, useful scalar regressions

This diagnostic also fails the no-useful-regression criterion. It improves
wide suffix-1, but scalar reserved, growth and reuse at population 16 regress
against the unchanged second-O3 control, with separated sample ranges in both
cohorts. It selects no production inline rule and does not revise the source
selections above.

Freeze source C at `47f9f91b63a484ba7f4924a63b5863b1b8b6f289` and its
native inputs. The prerecorded discriminator compares three images: the
production C image, a second O3 pass over its already optimized LLVM, and
that same second pass with `alwaysinline` added only to the wide definitions
`wf_vector_library_tail_work$instance$c3abe4db44181f7a` and
`wf_std.collections.vector.grow_vector_truncate$instance$d6739d8f89f405bd`.
Before timing, require elimination of the wide tail/truncate boundaries and
repeated constant setup without changing direct construction, complete
checksum checks, then the unchanged full matrix and no useful-cell regression.
This is a directed diagnostic, independent of the scalar no-wrap probe.

The optimized input comes from `clang -O3 -Wno-override-module -x ir -S
-emit-llvm` on C's frozen raw module. Its SHA-256 is
`d2b6441c89e30f0df61c677885e072fd711e251d6877f2a12d60478f3f56a4aa`;
the two-attribute variant is
`c2d01ab4eb212c73a0940f6d751be02922040a35673cc23aace5863dd8c13156`.
Removing those two attributes recovers every control byte. Compile each input
with `clang -O3 -Wno-override-module -x ir -c`, then use the ecosystem link
command and exactly C's frozen C driver, Rust archive, C++ and runtime
objects. Both new images pass 1,260 configurations and 8,820 executions.
The production/second-pass/late image hashes are respectively
`ee6973459e4daaccc13b7df18b91563b86ea345b30f0389844ba5da17355ccab`,
`540636491ea57d79068d6e0ccba28954060e745386d43772b630a1e17dc06977` and
`125adf4238cf3a7ce5178403033c8261bce9fd0d3f1abdf16fbeb0c7a94e7034`.

The unmodified second pass already inlines the wide tail. The directed
variant additionally removes truncate calls, keeps the digest in registers
and hoists constants outside suffix cycles; direct construction geometry is
preserved. WF object text grows from 11,992 bytes in production C to 12,724
in the second-pass control and 17,060 in the variant: 34.1% over its proper
control. Scalar trace, work and round instructions are unchanged between
the latter two images after branch-address normalization, but helper
placement changes. For example, the 173-instruction scalar work function
moves from address modulo 64 of 48 to 24. Placement is a possible explanation
for scalar regressions, not an isolated cache effect or a changed algorithm.

Fresh [production-C](ecosystem-append-late-production-samples.csv),
[second-O3](ecosystem-append-second-o3-samples.csv) and
[late-inline](ecosystem-append-late-inline-samples.csv) samples each contain
4,116 rows, with identical keys, work, rounds, traces, checksums and sample
IDs 0–6. Their SHA-256 values are respectively
`3bd257f1c611a9a8beadd8fdd3e21cce9d1cf676693e520e48a4938618d7170d`,
`07d8272b8ed457d29d3a5a6d2e3496e59c9162955fe39d86922853e99c27cb44` and
`d4a8199cdfd00d4553096fa825238ae03d767d8b7addc0f55829fde426d9c4dd`.
Sequential guarded `measure 1048576 7` runs took 80.462, 80.719 and
79.941 s, all exit 0, separately from construction and checks. Image and
native-input hashes were unchanged afterward. The earlier C timing samples
are not pooled into these comparisons. No separate transformed accounting
image was constructed for this diagnostic.

Ranges below cover both cohort medians; every ratio compares complete traces.
P denotes this fresh production-C run, S the unmodified second pass, and L
the directed variant.

| Bytes | Population | Path | P ms | S ms | L ms | S / P | L / S | L / P |
| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 16 | reserved | 3.638–3.658 | 3.601–3.659 | 3.870–3.883 | 0.990–1.000 | 1.058–1.078 | 1.058–1.067 |
| 8 | 16 | growth | 10.773–10.808 | 10.806–10.963 | 11.296–11.340 | 1.003–1.014 | 1.030–1.049 | 1.045–1.053 |
| 8 | 16 | reuse | 1.657–1.657 | 1.635–1.660 | 1.795–1.848 | 0.987–1.002 | 1.081–1.130 | 1.083–1.115 |
| 256 | 4096 | suffix-0 control | 2.093–2.158 | 1.798–1.801 | 2.927–3.020 | 0.835–0.859 | 1.628–1.677 | 1.398–1.399 |
| 256 | 4096 | suffix-1 | 22.718–22.780 | 17.043–17.092 | 16.055–16.091 | 0.748–0.752 | 0.939–0.944 | 0.706–0.707 |
| 256 | 4096 | suffix-2 | 23.151–23.201 | 23.438–24.043 | 22.474–22.498 | 1.010–1.039 | 0.936–0.959 | 0.969–0.972 |
| 256 | 4096 | suffix-3 | 26.623–27.141 | 34.966–35.012 | 26.144–26.147 | 1.290–1.313 | 0.747–0.748 | 0.963–0.982 |

For the three scalar population-16 counterexamples, variant minima exceed
control maxima by factors 1.013–1.055, 1.009–1.014 and 1.010–1.064,
respectively. Reserved Rust/C++ median drift is 0.979–0.999 / 0.970–0.997,
so a common host slowdown does not explain that regression. Against fresh
production C, these three cells also have higher medians, but only reuse
has separated ranges in both cohorts. Wide suffix-3's roughly 25% gain
against S mostly recovers a regression introduced by S; it is not a fresh
25% gain against P. Wide suffix-0 also worsens, but remains an unranked
overhead control rather than a useful-target selection failure.

Complete target reductions give P 12 passes / 11 deficits / 13 inconclusive,
S 13 / 13 / 10, and L 14 / 7 / 15, each with six unranked controls. Across
all 36 useful cells, L has lower medians in both cohorts in 21 cells against
S, higher in six and mixed in nine; against P these counts are 20, seven
and nine. At all three populations, wide suffix-1 reaches median ratios
0.960–0.969 against the slower standard comparator, but remains inconclusive:
at least one cohort's observed upper ratio is 1.002–1.010. There is no
qualified wide suffix-1 pass or universal standard-library win.

All sub-millisecond samples belong to suffix-0. P has no comparison above
10% cohort-ratio spread. S does have unstable scalar suffix-1 comparisons
(including Rust at population 256), scalar suffix-2 direct-C comparisons,
and suffix-0 controls; L's remaining unstable comparisons are wide suffix-0
take/swap C at 16/256 and scalar suffix-2 direct C at 4096. Inter-arm native
drift is also cell-specific: S/P scalar suffix-1 at 4096 has Rust/C++ ratios
1.303/1.210 in cohort 0, reversed by L/S ratios 0.769/0.833. Those cells
support no unqualified timing attribution. The bounded wide result supports
investigating optimizer scheduling, with code growth and the useful scalar
regressions still preventing selection of this directed policy.

### Fourth source discriminator: one cursor-driven consumption loop

Fuse truncation's two loop controllers without changing its selected take/swap
algorithm. A cursor starts at `retained`. Each iteration takes the rear owner;
when the cursor is below the new length, exchange that owner with the cursor
slot and advance the cursor, then consume the local through one shared
callback site. The exchange condition holds for exactly the former first
half, so the remaining owners are still consumed from the reversed tail in
original order. This is ordinary factoring under the current consumption
decision: unchanged O(removed) work, constant local storage, retained prefix,
capacity, allocation policy and unconstrained linear owner type. There is no
dispatch on benchmark count or payload type, and the initial empty header
allocation remains unchanged.

Before timing, require fewer scalar loop-control or descriptor operations
and no extra wide owner transfers or snapshots; the shared callback's value
merge could make wide lowering worse, which falsifies this candidate.
Rebuild the compiler's gate profile with two jobs because it embeds the
library. Reuse the complete behavior/accounting matrix and the formal vector
program's existing empty, singleton, odd/even, prefix-preservation, callback
order and owner-release observations. Accounting must remain byte-identical.
If the code criterion passes, measure the unchanged full scalar/wide,
three-population, seven-path matrix in both cohorts, comparing each cell
with frozen C and the earlier source trials under the existing duration and
spread qualifications. Any repeatable useful-cell regression prevents
selection; a favorable suffix median alone is insufficient.

### Fused consumption result: rejected before timing

D fails the wide-transfer code criterion. Native disassembly of
`grow_vector_truncate$instance$d6739d8f89f405bd` changes from C's frameless
168 instructions with no stack accesses to 232 instructions and a 368-byte
frame. The take block loads all 32 payload fields before testing whether to
swap, spilling eleven fields (88 bytes) and reloading them for the digest.
Even suffix-1 takes this path without a swap. The wide tail still calls
truncate and preserves C's direct aligned append construction. Scalar
truncate shrinks statically from 44 to 27 instructions, but this does not
compensate for violating the recorded wide criterion. No D timing was run,
and no runtime benefit or regression is claimed.

For exact reproduction, start with C above and replace only the truncate
body after its unchanged `doc` statement with the following. The function's
signature, contracts and every other library function remain unchanged.

```text
  let cursor = retained;
  loop @truncate (
    invariant prefix: deref(values).storage.inner.len >= retained,
    invariant cursor_lo: cursor >= retained
  ) {
    if deref(values).storage.inner.len <= retained {
      invariant exhausted: deref(values).storage.inner.len == retained;
      break @truncate;
    }
    let value = take_back(window: &deref(values).storage.inner);
    if cursor < deref(values).storage.inner.len {
      swap(first: &deref(values).storage.inner[cursor], second: &value);
      set cursor = cursor + 1_u64;
    }
    VectorDrain::accept(env: env, value: move value);
  }
  return unit;
```

The rejected whole-library SHA-256 is
`2372c33f036b4fc22615f11682b60a098df66d033737686241e6e0612d587c40`.
Its gate compiler, timed image and timed LLVM hashes are respectively
`fa8211c2edd57bc4ec9957a0525a11e9d3f0417290a959c26dfd41c53c583fde`,
`df9b570ce76da5ae1a113def94c1a72940ec8f85da6036fa432927460f2d64fa` and
`ead462e36ffa2f772a7ea5da365a76ce46155238f56982e8283cde46579d9306`.
Build with `cargo build --manifest-path compiler/Cargo.toml --profile gate
--bin whitefootc --locked --offline -j 2`, then the family ecosystem
build/check/account targets with `BUILD=.build/fused-truncate` and the
existing formal vector corpus test. Native inspection uses
`llvm-objdump --macho --disassemble --no-show-raw-insn` on the timed image.

The guarded run passed: compiler construction 7.819 s, ecosystem
construction 4.866 s, behavior checks 1.730 s, accounting 0.233 s, formal
corpus construction 0.547 s and execution 3.364 s. Both ecosystem images
pass 1,260 configurations and 8,820 executions, including checksum/cleanup
falsifiers. All 294 accounting rows and the C driver, Rust archive and C++
object are byte-identical to C. The formal vector test passes both lowering
modes with its unchanged 25-allocation expectation. These correctness
observations do not override the failed performance discriminator.

Only truncate was restored afterward, leaving the library byte-identical to
C. The generated D compiler and `.build/fused-truncate` artifacts remain
identified as rejected-candidate outputs; a subsequent C or compiler trial
must rebuild the embedded library before using its compiler as a baseline.

### Source discriminator F: one insertion after capacity preparation

F reuses C's small `grow_vector_make_room` guard, then performs one
`insert_at`, replacing insert's four growth/placement branches. The helper
preserves length and publishes spare capacity, so the unchanged entry index
bound still authorizes insertion. Public contracts, growth policy, element
order, generic ownership, append and truncate remain unchanged. C's actual
mixed-trace code still calls insert; its wide body snapshots all 256 incoming
bytes before testing capacity. This is a source-factoring hypothesis, not a
measured insertion benefit or an attribution of the failed Slots trial.

Use the original C compiler lowering for both source arms: neither the Slots
candidate nor E's branch-local truncate is included. Keep fixtures, native
inputs and flags fixed. Before timing, require insert to inline in both mixed
traces, no capacity-helper call on the spare path, and elimination of the wide
entry snapshot without equivalent marker staging elsewhere across caller and
callee. Inspect the actual caller and unchanged shift lowering; an unchanged
hot path or displaced snapshot fails the discriminator. Cancellation of the
paired insert/remove shifts is not assumed or required. First pass complete
behavior, byte-identical accounting and the formal vector program with
unchanged index/order/release expectations. Only then time the full existing
two-cohort matrix against the fresh frozen-C series: reserved, growth and
reuse are affected paths; suffixes are independent controls. Any repeatable
useful-cell regression prevents selection. This candidate is preregistered
before compilation and makes no performance claim.

### F construction and correctness

F is pinned at
[`3347fcb4b705990b5b3a9d66887a30ca9e632374`](https://github.com/mbbill/Whitefoot/tree/3347fcb4b705990b5b3a9d66887a30ca9e632374)
on the restored original C compiler lowering; the rejected Slots
implementation is absent. The library SHA-256 is
`72ff83f18181651461e11f6dd95394556dce372710cb3a58376a6d4e099e7339`.
The gate compiler, timed image and timed LLVM hashes are respectively
`fe856a7b9ab2827bb30515547a82bc01132b65e4e3110dd0371cdf0cfc88b3eb`,
`44660c2de9532af3392c3c5fefea363b1915abd03bc9b79f4ba39812425c05f2` and
`63038feb712920a11bb212eb06f858929186445eac6b2703afea392a9d74aa24`.
`BUILD=.build/single-insert-f` preserves the earlier images. Compilation uses
the gate profile, `--locked --offline -j 2`; a separate CLI
`--emit-llvm vector-library.wf` preflight accepts the unchanged index,
length and capacity contracts without an added invariant or language change.

Guarded validation took 20.09 s: compiler construction 9.278 s, source
preflight 0.724 s, ecosystem construction 4.199 s, behavior 1.687 s,
accounting 0.148 s, formal corpus construction 0.508 s and execution
3.304 s. The preceding shared-guard queue took 165.478 s with 82 busy
retries; it is not construction or test time. Both ecosystem images pass
1,260 configurations and 8,820 executions, including checksum/cleanup
falsifiers. The formal vector program passes both lowering modes with its
unchanged 25-allocation release expectation. The C driver, C++ object, Rust
archive and all 294 accounting rows are byte-identical to C. The one-shot
runner was removed after freezing the artifacts. These are construction and
correctness observations, separate from performance measurement.

### F final-code discriminator

The recorded code criterion passes. Actual scalar and wide mixed-trace work
inline insertion; neither path calls `grow_vector_insert` or `insert_at`.
The spare-capacity branches skip `grow_full`. Wide marker construction no
longer stages a 256-byte caller block or enters a callee with a 256-byte
snapshot. Its fields feed the digest directly without reloading the marker
from the backing. Both insert/remove shift loops remain, so cancellation of
those transfers is neither observed nor credited to this source change.

Residual marker traffic remains measurable in the code. The common path
spills nine fields: 72 bytes of stores and 144 bytes of reloads, because
each is read once for placement and once for the digest. The cold full-capacity
branch saves and restores another fourteen fields, 112 bytes each way, around
growth. The common-path snapshots are not merely displaced into that caller.
Wide work's local stack area shrinks from 752 to 592 bytes, plus the unchanged
96 bytes for saved registers; the scalar frame remains 64 bytes. Caller
instruction counts grow from 173/410 to 209/441 after absorbing scalar/wide
insert, which is not an elapsed-cost comparison. All six trace/tail/truncate
bodies remain equal after normalizing branch targets and constant references.
Scalar trace placement is unchanged; wide tail moves +144 bytes and wide
truncate -168 bytes. These code and input checks preceded timing authorization
and do not establish a runtime gain.

### F paired timing: useful improvements without a separated regression

F passes this trial's prerecorded full-matrix no-useful-regression criterion.
Three of the 36 useful cells improve with nonoverlapping observed ranges in
both cohorts: scalar growth at 16, scalar reuse at 16, and wide reuse at 16.
No useful cell has a strictly separated regression in both cohorts; the
remaining 33 overlap in at least one. Across all useful cells, F medians are
lower in both cohorts for 24, higher for one, and mixed for eleven. These are
finite-sample observations, not confidence intervals or a universal native
performance result. F is the next working source base; the per-cell standard
target remains unfinished.

The pair runs the frozen source-C and F images identified above, sequentially
under one guard with `measure 1048576 7`, all seven implementations and both
order cohorts. C took 80.479 s and F 80.805 s, each exit 0; the outer guard
took 161.55 s. The preceding 575.142 s queue and 285 busy retries are separate
from program execution. Source, compiler, LLVM, image, native inputs and
accounting identities were checked before and after the pair. Construction
and correctness costs remain in the preceding ledger; no rebuild or check
is included in these timing durations.

The fresh [C control samples](ecosystem-insert-control-samples.csv) and
[F samples](ecosystem-insert-f-samples.csv) each contain 4,116 rows, with
SHA-256 values
`5c3131148a9b00f9fca6b5bdc7b811f2a84adb0af0520ec2b0cebeacbf0bbf76` and
`7578ef4f1f866412b0ddcbf854ec42d3e08dcc2287f31787bbdf4f63d43f9a17`.
All keys, work, rounds, traces, checksums and sample IDs 0–6 agree across
arms. Both pass `summarize-ecosystem.pl --complete` and `--complete --targets`.
Earlier C series remain separate evidence and are not pooled into this pair.

This table covers all 36 useful cells. Ranges span the two cohort medians;
ratios compare complete traces. The final column uses each cohort's slower
Rust/C++ standard median and does not itself establish sample separation.

| Bytes | Path | F / C at 16 | at 256 | at 4096 | F ms at 4096 | F / slower standard at 4096 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | reserved | 0.903–0.920 | 0.977–0.997 | 0.982–0.998 | 1.679–1.681 | 0.797–0.819 |
| 8 | growth | 0.939–0.958 | 0.952–0.986 | 0.965–0.996 | 2.069–2.104 | 0.854–0.855 |
| 8 | reuse | 0.932–0.937 | 0.979–0.994 | 0.970–1.025 | 1.658–1.697 | 0.798–0.817 |
| 8 | suffix-1 | 0.983–1.006 | 0.976–1.001 | 0.974–0.986 | 2.953–2.956 | 0.997–0.997 |
| 8 | suffix-2 | 0.987–1.002 | 0.988–1.032 | 0.988–1.003 | 2.385–2.405 | 1.452–1.466 |
| 8 | suffix-3 | 0.970–0.981 | 0.982–0.992 | 0.962–1.015 | 1.929–1.957 | 1.105–1.116 |
| 256 | reserved | 0.979–0.991 | 0.994–0.997 | 0.995–1.000 | 40.717–40.835 | 0.936–0.938 |
| 256 | growth | 1.003–1.007 | 0.990–0.991 | 0.984–0.991 | 52.810–52.821 | 1.013–1.024 |
| 256 | reuse | 0.980–0.980 | 0.993–0.995 | 0.998–0.999 | 40.711–40.752 | 0.931–0.936 |
| 256 | suffix-1 | 1.000–1.002 | 0.998–0.999 | 0.999–1.000 | 22.700–22.750 | 1.357–1.366 |
| 256 | suffix-2 | 0.983–0.984 | 0.992–1.024 | 0.991–1.003 | 22.812–23.424 | 1.046–1.074 |
| 256 | suffix-3 | 0.997–1.000 | 0.998–0.999 | 0.990–0.998 | 26.480–26.484 | 1.010–1.012 |

The strict gains' median ratios and conservative observed upper ratios
(`maximum F / minimum C`) are:

| Cell | F / C, cohorts 0 / 1 | Observed upper, cohorts 0 / 1 |
| --- | ---: | ---: |
| Scalar growth, 16 | 0.939268 / 0.958433 | 0.962037 / 0.992509 |
| Scalar reuse, 16 | 0.936527 / 0.931666 | 0.993891 / 0.959071 |
| Wide reuse, 16 | 0.980359 / 0.980335 | 0.989761 / 0.984408 |

The sole useful cell with higher medians in both cohorts is wide growth at
16: 1.002538/1.006987, with observed lower ratios 0.989250/0.984125 and upper
ratios 1.018182/1.038620. Its ranges overlap in both cohorts. Scalar reserved
at 16 has lower medians, 0.903–0.920, but does not separate in both cohorts.
Across useful comparisons, Rust median drift is 0.945–1.031 and C++ drift
0.962–1.015. In the three strictly improved cells, Rust drift is
0.993/0.994, 0.986/1.007 and 0.999/0.999 respectively; C++ drift is
0.987/1.000, 1.015/0.999 and 1.003/0.998. The native controls therefore do
not explain those three WF gains as a uniform environmental speedup.

The independent slower-standard target judgment gives fresh C
11 passes / 11 deficits / 14 inconclusive, and F 13 / 12 / 11, plus six
unranked suffix-zero controls for each. A target classification change can
reflect native variation or sample overlap; the extra deficit is not a
strict WF-versus-C regression. All useful native target comparisons meet
the duration and cohort-spread qualifications, with a minimum useful WF
sample of 1.507 ms. Every sub-millisecond observation is suffix-zero.
Fresh C's wide suffix-zero take/swap C comparisons at 16 and 256 are unstable
(35.923% and 29.147%), as is scalar suffix-2 direct C at 4096 (17.368%);
F's unstable comparison is wide suffix-zero take/swap C at 16 (26.889%).
Those observations support no ranking or attribution.

The source experiment measures the combined removal of insertion boundaries
and snapshots together with its register allocation and code-placement
changes. It does not assign an elapsed share to any one mechanism. The
unchanged suffix bodies remain useful controls, and their timing variation
is not an insertion benefit. F retains the same allocation policy and the
same 294 accounting rows; the separate known-capacity-construction proposal
is not part of this comparison.

### F native suffix consumption against the standard containers

Read-only inspection of the same retained F image above (SHA-256
`44660c2de9532af3392c3c5fefea363b1915abd03bc9b79f4ba39812425c05f2`)
separates the consumption algorithms from construction and helper placement.
The scalar WF suffix path in the actual trace has no tail/truncate call;
the wide trace still calls tail, which calls truncate. Rust and C++ consume
the suffix inline in their trace. These are final AArch64 instructions,
not copies inferred from unoptimized LLVM.

| Executed loop | Scalar instructions and inclusive addresses | Wide instructions and inclusive addresses |
| --- | --- | --- |
| WF take/exchange/consume | 12, `0x10000bd5c`–`0x10000bd88` | 88, `0x10000d650`–`0x10000d7ac` |
| WF remaining back-take/consume | 7, `0x10000bd9c`–`0x10000bdb4` | 53, `0x10000d7d0`–`0x10000d8a0` |
| Rust forward drain | 4, `0x100011588`–`0x100011594` | 51, `0x100011038`–`0x100011100` |
| C++ forward consumer | 4, `0x10000e528`–`0x10000e534` | 52, `0x10000eab4`–`0x10000eb80` |

Every wide callback performs sixteen paired loads and 32 multiply-adds.
Each WF first-half iteration additionally loads and stores the rear record
with sixteen 16-byte loads and sixteen 16-byte stores; the remainder later
reads that displaced record again. Each take stores the length. The native
consumers read forward without payload stores and shorten the vector once.
C++ also retains an erasure call to `memmove` (`0x10000ebb0` wide,
`0x10000e554` scalar): its source is the old end and its byte count is zero
on these suffix paths. That call is overhead, not an element transfer.

| Removed elements | WF wide payload bytes read / written | Rust/C++ payload bytes read / written | WF scalar consumption instructions |
| --- | ---: | ---: | ---: |
| 1 | 256 / 0 | 256 / 0 | 16 |
| 2 | 768 / 256 | 512 / 0 | 34 |
| 3 | 1024 / 256 | 768 / 0 | 41 |

The scalar totals follow `0x10000bd34` through the return-to-round branch
at `0x10000bdb8`, including loop entry and transition work. Payload counts
exclude append, headers, digest storage and the final retained-prefix drain;
they count executed load/store operands, not cache traffic. No elapsed-time
percentage follows from instruction or byte counts. In particular suffix-one
has no exchange to eliminate; its wide boundary, constant setup and code
placement remain separate costs.

The terminal-pair source trial can eliminate the last exchange for an even
suffix by holding two owners and consuming them in original order. It is not
native forward drain. With existing operations, `split_off` needs another
backing and copies the suffix, repeated `remove_at` at the retained boundary
shifts a quadratic number of elements, and `take_back` alone reverses callback
order. A Ring changes representation and cannot pop an interior suffix past
its retained prefix through its endpoint operations. A borrowed callback
changes the API's ability to consume arbitrary owned elements. The current
O(removed), constant-storage take/exchange algorithm therefore remains the
baseline while the bounded terminal-pair improvement is tested; this
inspection selects no new primitive, representation or language rule.

### Length-store dependence: read-only LLVM diagnosis

The remaining scalar length stores have a concrete intra-allocation
conservatism distinct from the outer-owner/environment hypothesis above.
LLVM 22.1.8 `aa-eval` and `print<memoryssa>` on the saved optimized scalar
truncate helper from WF source variant C report `MayAlias` between its
length address `%t1` and both back-take payload addresses `%t3.i` and `%t3.i7`;
each payload load uses the
immediately preceding length store's MemoryDef. The ordinary indexed left
address `%t8` is `NoAlias` with the header. The wide helper's base payload
address is already `NoAlias`, but the projection at record offset 240 still
has a `MayAlias` result against length. These helpers have the same observed
native truncate bodies as F; this is not a newly optimized full F module.

The take lowering does not bypass the selected physical-index fact emitter.
Frozen F's raw take rows already call `llvm.assume` on their actual address
index being signed nonnegative. `emit_run_taken` calls the shared
`element_pointer`, whose zero-stride normalization precedes that assertion.
PRE-1's positive take length, WIN-1's capacity bound and qualified complete
positive-stride extent justify it; a huge logical zero-stride index instead
addresses physical index zero. The printed take assertions disappear in the
saved optimized helpers, while the ordinary indexed-left assertion remains.

A four-site, analysis-only discriminator reasserted precisely those original
nonnegative take-index facts in the two extracted optimized helpers. The
criterion was `MayAlias` to `NoAlias` and removal of the corresponding
MemorySSA dependence before considering a native comparison. Neither changed:
both scalar take addresses and the wide offset-240 projections retained
`MayAlias`, and the scalar payload loads still depend on their length stores.
All four analyses returned zero; control AA/MemorySSA took 0.050/0.017 seconds
and reassertion AA/MemorySSA 0.051/0.017 seconds. No native build or timing was
run for this failed discriminator. It identifies conservative dependence but
does not establish the exact optimizer limitation or select extra assertions,
per-access alias metadata, a pass-order change or a production lowering fix.

### Source discriminator G: direct known-capacity construction

G adds the ordinary `grow_vector_with_capacity` API and uses it only where
the existing caller already knows its reservation. It constructs one empty
Slots backing at that capacity; its named returned owner publishes length
zero and the exact capacity through CALL-4. The existing zero-capacity
`grow_vector_new`, incremental growth policy, F append/insert, consumption
order and unconstrained linear-element support remain unchanged. This is an
additive library interface under the existing OP-9/STOR-6 allocation bounds
and total STOR-8 allocation, with no new language primitive or representation.

Before compilation, freeze F's compiler lowering, native implementations,
input streams, complete trace definitions and flags. Only known-capacity
WF construction and the accounting formulas change. The optimized F image
already removes its empty allocation/free on reuse and suffix paths. The
concrete native prediction is therefore narrower than the source accounting:
the reserved branch of the actual round should lose its surviving 16-byte
empty allocation/free and reach one requested-capacity allocation. Growth
must retain its empty backing and incremental policy. Reuse and suffixes
must gain no construction or hot-loop work. Inspect actual work, tail and
truncate bodies, helper calls, frames, spills and placement; report any
folding due to the stronger result contract separately. Symbol movement alone
neither passes nor fails the criterion. An unchanged reserved allocation
path, displaced allocation or new hot-path work falsifies the code prediction.

Require both complete ecosystem correctness images, the formal Vector
program in both lowering modes, and the historical ordinary/retained O2
checks. Formal coverage includes capacity zero, positive requested capacity,
zero ceiling, and must-consume owners without a fabricated element. In the
294-row accounting matrix, exactly 36 known-capacity WF rows must lose the
specified empty-header events, six WF growth rows and all 252 native rows
must remain unchanged, and every trace must finish with zero live allocations.
Reserved loses one request/release and 16 requested bytes per constructed
vector; reuse/suffix lose one such pair per trace. The source-level peak
loses 16 bytes. A balanced extra-header allocation/free must be rejected by
the independent formula even though cleanup succeeds. Existing checksum and
unreleased-owner controls remain. Observer-preserved source events must not
be described as physical timed allocations when ordinary optimization has
already removed them.

Only a passing code/correctness discriminator permits a separately authorized
full F/G timing pair. Retain all useful scalar/wide cells at 16, 256 and
4096, all six unranked suffix-zero controls, both cohorts and existing
duration/stability qualifications. Require a useful reserved improvement
without a repeatable useful-cell regression, and report every cell against
F as well as the slower standard comparator. No result is selected by an
aggregate or by accounting counts alone. This criterion is recorded before
G compilation; there is no measured benefit yet.

### G construction and correctness

G's source is pinned at
[`0c9ce44e9d640855d9479239b07fa936e4163071`](https://github.com/mbbill/Whitefoot/tree/0c9ce44e9d640855d9479239b07fa936e4163071).
Its library SHA-256 is
`a794890d707ed494aab3cc51e2cbc9a77e37d38c39ff0cf7880bd785751fe0de`,
and its public module record is
`0abae00637fc124e50289e1114fe0944ada28d3b978e67f739ed2d93feffba52`.
The rebuilt compiler, timed image and timed LLVM are respectively
`08eee3a508d22fcf7bb824b1bd47402b9caffe7d16f4d048dd8b3c239fbad899`,
`6ca34687d0efa797d6fa5a458777d6bbb9f29ebfa32e69788d16a027adf8f744` and
`69c8fa27687991d438808cbf9f5ddc833e0ed2dfa971fd2764838076b3375dd4`.
`BUILD=.build/capacity-g` preserves every earlier candidate. The compiler
implementation remains byte-identical to F; its executable changes because
it embeds the library. This is an API/caller comparison, not a same-source
compiler-lowering experiment. No specification rule or representation changes.

Compiler construction used the gate profile with `--locked --offline -j 2`
and took 7.707 s. The first CLI preflight failed in 0.551 s at a trailing
semicolon after the new value-if: GRAM-4 ends that initializer at its final
brace. Removing the fixture's semicolon repaired its spelling; no parser,
contract or proof requirement changed. The repaired preflight passed in
0.135 s without rebuilding the unchanged embedded library. Fresh ecosystem
construction took 4.163 s, correctness 1.837 s and accounting 0.145 s.
Formal corpus construction took 0.712 s and execution 3.325 s. Historical
O2 construction took 1.331 s and checks 1.105 s. The two enclosing guards
took 8.41 s for the first construction/preflight and 12.87 s for the repaired
preflight and remaining checks, with no queue wait or busy retry. These
costs exclude measurement.

Both ecosystem images pass 1,260 configurations / 8,820 executions. The
historical ordinary and retained O2 images each pass 1,260 configurations /
6,300 executions with the exact known-capacity WF-to-C accounting difference.
The formal program passes both lowering modes with 32 exact-once allocation
releases: the existing 25 events remain, the capacity-three linear-owner
chain adds one backing and five payloads, and the empty ceiling-zero Ticket
adds one backing. Its capacity-zero and capacity-three chains retain all
content, callback-order and cleanup observations. The existing checksum and
unreleased-allocation controls remain effective. The new balanced extra
16-byte allocation/free exits 1 at `independent allocation count, byte and
peak formula`, after passing cleanup and lifecycle balance.

Bounded scratch variants falsified the new formal observations without
changing the repository verdict. The unmodified source exits 0. Expecting
capacity one from the capacity-zero chain exits 22; expecting two from the
capacity-three chain exits 26. Expecting length one or capacity one from the
empty Ticket exits 27 or 28. All four variants compile successfully before
those native failures. The first scratch conditional used a nested else/if
and was rejected by GRAM-6; it was flattened to the required else-if form,
not counted as an observation failure. The corrected guarded command took
3.15 s: emission 0.983 s, native linking 0.793 s and five executions 1.268 s,
with no queue wait. The initial control and malformed variant took a separate
1.28 s guard. Both one-shot runners were removed after preserving their logs
and artifact identities.

The [G accounting matrix](ecosystem-capacity-g-accounting.csv) has 294 rows,
SHA-256 `66d7c100dcb96dff2f43a8eb0146852ef9f840e0afaae156cbb9f4f83ffa37cc`.
Exactly 36 known-capacity WF rows change by the prerecorded formula; all
six WF growth rows and all 252 native rows are identical to F. Every checksum,
round count and trace count is unchanged. Reserved removes one request and
release plus 16 requested bytes per round; reuse and each suffix remove one
pair and 16 requested bytes per trace. The peak and requested-overlap upper
bound each decrease by 16 bytes. G still owns a 16-byte backing header:

| Reserved, three rounds | WF requests | WF requested bytes | WF peak bytes | Rust/C++ requested / peak bytes |
| --- | ---: | ---: | ---: | ---: |
| Scalar, 16 | 3 | 456 | 152 | 408 / 136 |
| Wide, 16 | 3 | 13,104 | 4,368 | 13,056 / 4,352 |
| Scalar, 4096 | 3 | 98,376 | 32,792 | 98,328 / 32,776 |
| Wide, 4096 | 3 | 3,146,544 | 1,048,848 | 3,146,496 / 1,048,832 |

These are observer-preserved source events, not RSS or a count of optimized
timed allocations. The timed C driver, C++ object and Rust archive remain
byte-identical to F, as do the runtime objects.

### G final-code discriminator

The recorded native construction discriminator passes. Each scalar/wide
round wrapper shrinks from 41 to 33 instructions and from an 80-byte to a
64-byte frame. The reserved branch reaches one requested-capacity `malloc`
without the previous empty `calloc` and `free`; its wrapper path falls from
40 to 28 instructions, excluding called bodies. The growth branch still
allocates the 16-byte empty backing and follows the unchanged incremental
policy, while the shared wrapper's own path shrinks from 29 to 25 instructions.
This wrapper change is separate from removing an allocation on reserved.

Reuse and suffix already had only one timed construction in F and retain it
in G. All fourteen scalar/wide trace, work, tail, truncate, insert, remove
and growth-helper bodies compare equal after normalizing branch targets and
constant references; the actual mutation and consumption paths gain no
hot-loop work. The scratch comparison was tightened after review: it no
longer resurrects an overwritten address register's earlier constant page
or accepts raw instruction equality as a fallback. All fourteen bodies still
pass strict normalized comparison, and changing either a branch target or
a constant-load offset makes that check fail. Initial-length guards were
already absent from F's optimized code, so there
is no additional runtime-check removal to credit to the new ensures clauses.
Code placement changes remain: both trace entries move +168 bytes; scalar
work/tail +64; wide work/tail/truncate -72; scalar truncate -104; and the
growth helpers -40. These observations precede full-matrix timing and do
not themselves establish a runtime improvement or a saving per suffix cycle.

### G paired timing: growth regression prevents selection

G fails the prerecorded no-useful-regression criterion. Scalar growth at
256 takes 2.967/2.954 ms in cohorts 0/1, against F's 2.873/2.872 ms:
G/F is 1.032718/1.028552. The candidate minimum exceeds the control maximum
in both cohorts, by factors 1.022145/1.004096. Rust median drift in that
cell is 0.996986/0.991976 and C++ drift 1.000310/1.007123; a uniform native
slowdown does not explain the WF regression. The aggregate standard-target
count improves, but does not override this cell or the recorded criterion.

The hoped-for reserved gain appears in medians without strict separation in
both cohorts. Scalar reserved at 16 improves to 2.834/2.786 ms from
3.358/3.347 ms, ratios 0.843955/0.832387. Its first G sample in cohort 0 is
7.867 ms; all seven samples remain in the result, giving observed upper
ratios (`maximum G / minimum F`) 2.352572/0.844704. Scalar reserved at 256
has ratios 0.947399/0.944700 and upper ratios 1.008130/0.952962. Neither
qualifies as a strict two-cohort gain. No outlier is dropped, no warmup rule
is changed afterward, and favorable medians alone do not select G.

Across all 36 useful cells, there is one strict regression, zero strict gains
and 35 with overlap in at least one cohort. Nine cells have lower medians in
both cohorts, ten higher and seventeen mixed. The six suffix-zero cells
remain unranked controls. These are comparisons of finite observed sample
ranges, not confidence intervals. F remains the supported comparison base;
this record does not recommend the measured G caller migration for selection.

The fresh [F control samples](ecosystem-capacity-control-samples.csv) and
[G samples](ecosystem-capacity-g-samples.csv) each contain 4,116 rows.
Their SHA-256 values are
`c9b4b972216e150b6668898ce8e8a9aff9d2c6ae93dc5f886e86f0e92e19db6d` and
`74ef634b7161e8952e087677b41806f62657c20fa34058b5502bd127f1e01f40`.
All keys, work, rounds, traces, checksums and sample IDs 0–6 agree. Both
complete reductions and target reductions pass. The source pins are F's
`3347fcb4b705990b5b3a9d66887a30ca9e632374` and G's revision above; the
committed source bytes were checked against the frozen input manifest.
Earlier F series remain separate and are not pooled with this pair.

Both frozen images ran sequentially under one guard with
`measure 1048576 7`: F 80.284 s, then G 80.796 s, both exit 0.
The outer guard took 161.19 s with no queue wait or busy retry. Source,
compiler, LLVM, image, timed native inputs, runtime objects and accounting
hashes were unchanged before and after timing. The one-shot runner was
removed afterward. Construction, correctness and native inspection costs
are separate from these complete-matrix execution times.

The table covers every useful cell. Ranges span cohort medians and all
ratios describe whole traces. The last column compares G with each cohort's
slower standard median, separately from the target's sample-separation test.

| Bytes | Path | G / F at 16 | at 256 | at 4096 | G ms at 4096 | G / slower standard at 4096 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | reserved | 0.832–0.844 | 0.945–0.947 | 1.000–1.002 | 1.678–1.687 | 0.796–0.805 |
| 8 | growth | 1.050–1.053 | 1.029–1.033 | 0.996–0.998 | 2.070–2.071 | 0.856–0.878 |
| 8 | reuse | 0.992–0.997 | 0.999–1.007 | 1.003–1.003 | 1.653–1.656 | 0.797–0.799 |
| 8 | suffix-1 | 1.002–1.005 | 0.999–1.008 | 0.999–1.005 | 2.957–2.967 | 1.005–1.007 |
| 8 | suffix-2 | 0.996–1.003 | 0.958–1.007 | 0.982–0.998 | 2.379–2.387 | 1.454–1.460 |
| 8 | suffix-3 | 0.995–0.996 | 0.986–1.020 | 0.991–1.003 | 1.928–1.935 | 1.105–1.106 |
| 256 | reserved | 0.985–0.986 | 1.000–1.003 | 1.003–1.013 | 40.819–41.249 | 0.938–0.938 |
| 256 | growth | 0.995–1.005 | 1.001–1.004 | 0.988–0.994 | 52.291–52.565 | 1.012–1.017 |
| 256 | reuse | 0.998–1.002 | 1.004–1.008 | 0.996–0.998 | 40.620–40.694 | 0.935–0.938 |
| 256 | suffix-1 | 1.003–1.004 | 0.997–1.005 | 1.002–1.004 | 22.712–22.737 | 1.361–1.361 |
| 256 | suffix-2 | 0.999–1.016 | 0.990–1.001 | 0.983–1.001 | 23.021–23.294 | 1.057–1.068 |
| 256 | suffix-3 | 0.999–1.000 | 0.992–1.016 | 1.002–1.002 | 26.515–26.535 | 1.011–1.012 |

The slower-standard reduction gives fresh F 12 passes / 11 deficits /
13 inconclusive, and G 13 / 10 / 13, plus six unranked controls each.
Every useful target meets the duration and cohort-spread qualifications;
the minimum useful WF sample across the pair is 1.506 ms. Every
sub-millisecond observation is suffix-zero. F's scalar suffix-2 direct-C
comparison at 256 is unstable (103.961%); G's wide suffix-zero take/swap C
at 256 and scalar suffix-2 direct C at 4096 are unstable (24.707% and
23.995%). Those controls support no ranking or attribution. Across useful
cells, Rust median drift is 0.963–1.021 and C++ 0.959–1.036; some source C
controls vary more, including direct C 0.909–1.757.

The native evidence establishes the reserved allocation removal and the
smaller shared wrapper, while mutation/consumption bodies remain unchanged.
It does not assign the growth regression to allocation, code placement,
register scheduling or a hardware stall. The source-required accounting
reduction remains correct even though the optimized timed reuse and suffix
paths had already eliminated their empty allocation. Those two facts do
not establish a universal runtime benefit from the caller migration.

The additive API is correct; the tested API/caller combination fails the
performance criterion. Its complete source and validation remain reproducible
at the G pin above, rather than being treated as an unavailable language
capability. Reopen it with a concrete ordinary caller decomposition that
separates reserved construction from the shared unreserved wrapper, checking
the latter's native path before another full comparison. That new evidence
must still preserve all useful cells; neither a favorable reserved median
nor the source accounting reduction is sufficient. No such successor has
been compiled or measured here.

After this failed trial, only the seven G implementation, public-interface,
caller, test and accounting-formula files were restored byte-for-byte to
`afa117cd7b92b2314339198905c34d731ef29a44`, the F source state. G's reports,
accounting, full samples and source pin remain intact. This restores the
supported working base; it does not establish an intrinsic loss for the
correct additive API. The already built compiler and `.build/capacity-g`
artifacts still embed G and remain identified as that rejected trial. A new
source trial must rebuild the embedded library before using its compiler;
no replay-until-win or unchanged-hypothesis retiming is selected.

### Source discriminator J: consume the adjacent terminal pair directly

J starts from restored F, with library SHA-256
`72ff83f18181651461e11f6dd95394556dce372710cb3a58376a6d4e099e7339`.
Keep F's append/insert, allocation policy, public signatures and contracts,
compiler lowering, callers, native implementations and flags fixed. G's
constructor and caller migration, E's controller change and the rejected
Slots lowering are absent. The
[terminal-pair amendment](../../../../design/amendments/vector-terminal-pair.md)
initially proposed the one change to the recorded consumption algorithm; no live-tree
edit has been made. No specification change is proposed; the live-tree
decision is unchanged pending a ruling.

After the existing first-half pair proof, compute `gap = len - left`. When
it is two, take the rear owner into `second`, take its predecessor into
`first`, consume `first` then `second`, and break the first-half loop. The
unchanged remainder consumes the already reversed tail. For first-half
offset `i`, the existing invariant gives `gap = removed - 2*i`; every
positive even suffix reaches the adjacent pair once, and an odd suffix
never does. This avoids one rear-to-left source relocation per even suffix,
using two owned locals instead of one, still constant auxiliary storage.
There is no population/type dispatch, manufactured element, scratch backing,
copy/drop bound or changed callback order. EFF-5 excludes callback access
to the backing, so the earlier second length decrement is unobservable.
The existing pair bound admits both takes; an erased `remaining_prefix`
invariant establishes the remainder loop's entry fact before the break.
Formation must succeed under current rules without a fallback or weakened
contract. These are static grounds for a trial, not compiler acceptance or
a measured benefit.

Before timing, inspect actual scalar/wide truncate paths and their callers.
Require less executed scalar terminal-pair work than F and removal of the
wide 256-byte rear-to-left relocation without an equivalent or new payload
snapshot, spill/reload or other transfer. Keeping `second` live across the
first callback may defeat this prediction. Require no added suffix-one
work, and inspect the gap test's cost on odd and larger even suffixes.
Reject at the code criterion if the relocation merely moves to the stack;
source operation counts or a smaller retained symbol are insufficient.

The complete ecosystem correctness matrix must still pass 1,260
configurations / 8,820 executions per image, with all 294 accounting rows
byte-identical to F and existing checksum/cleanup falsifiers retained.
The formal program keeps its original 25-allocation chain and adds six
must-consume Tickets, retaining the first two while consuming 3/4/5/6,
then draining 1/2. Its independent base-257 sequence values are 51,189,266
and 3,380,999,830,293; check counts, retained owners, capacity and complete
cleanup in both lowering modes. Two backing allocations and six payloads
add eight exact-once releases, giving 33. Each new observation must reject
its altered expectation, and reversing the two terminal callbacks must
fail the independent order oracle. Preserve the compiler/API and existing
fixture requirements in those negative controls.

Only passing formation, correctness and native code permits timing in a
separately assigned coordinator slot for the entire existing matrix: scalar/wide, populations
16/256/4096, reserved/growth/reuse/suffix-zero through suffix-three, both
cohorts, seven samples and all seven variants. Keep suffix-zero unranked,
apply the existing duration/spread rules, compare every useful cell against
F and the slower standard, and accept no repeatable useful-cell regression.
An extra gap-test loss on an odd or larger suffix is not excused by a favorable
terminal-pair cell. Rebuild the embedded-library compiler with the gate
profile and two jobs, use fresh `BUILD=.build/terminal-pair-j`, and separate
construction, execution and queue costs. This criterion precedes J
compilation; no J native or timing benefit is claimed.

### J initial formation and explicit continuation proof

The first J compiler construction took 7.773 s. The unchanged workload then
failed CLI preflight in 0.692 s at `vector_library_work`'s existing empty
postcondition, FN-9. A guarded direct module check took 0.11 s and exposed
the callee obligation: the remainder loop's `len >= retained` base invariant
was unproved under INV-1. The terminal branch's written `remaining_prefix`
statement itself was accepted; no ecosystem image or corpus run followed.
The first guarded construction/preflight command took 8.60 s with no queue.

Adding `kept: len >= retained` to the counted header did not repair that
join: a direct module check took 0.068 s and rejected the same remainder
base, before any compiler rebuild. Moving the break's `remaining_prefix`
statement after both callbacks also proved that statement but rejected the
continuation (0.066 s). The callbacks therefore do not explain the loss.
The natural exit and twice-taken break carry different immutable length
images. ENT-5 compares canonical inequalities over those images; the current
join also drops a measure image when its incoming images differ. Matching
source spellings alone are insufficient. No caller requirement, invariant
or compiler rule was weakened, and these observations do not establish a
compiler defect.

### J revision: handle the terminal pair after a break-free loop

Before compiling the revised source, replace the rejected control shape as
follows. An empty suffix returns immediately. For a positive suffix, execute
`floor((removed - 1) / 2)` ordinary take/swap steps without a counted break.
An odd suffix then has no adjacent pair left to exchange; an even suffix has
exactly one, identified by a single post-loop `len - left == 2` test. Take
and consume that pair earlier-owner first. The old backward remainder loop
becomes one private helper with the same prefix and capacity contract, called
on both terminal branches before their returns. Its boundary publishes the
ordinary postcondition without joining different intermediate length images.

This changes the control-shape hypothesis, while retaining J's owner order
and single avoided relocation per positive even suffix. It removes the
per-iteration gap test but may add an unhelpful call boundary or change
inlining. The native criterion therefore additionally rejects a surviving
new hot remainder-helper call, and still requires less executed scalar
terminal-pair work, no added suffix-one work, and no displaced wide payload
snapshot or spill/reload. The early empty branch may change suffix-zero
overhead; retain and report every such unranked control. All original
correctness, accounting, negative
observation and complete-matrix regression criteria remain. These grounds
precede the revised module check; no code or timing result is assumed.

### J revised construction and falsifiable correctness

The revised live module passed in 0.041 s after removing a redundant `use`
block: PRF-1 required that removal because AUTO already proved the unchanged
terminal bound. The redundant-proof rejection took 0.063 s. No specification
or compiler rule changed. Rebuilding the embedded library took 8.186 s and
caller LLVM preflight 0.714 s. Ecosystem construction/check/account took
4.258/1.715/0.146 s; both images passed 1,260 configurations and 8,820
executions, including the existing checksum and cleanup rejections. All
294 accounting rows equal F byte-for-byte, as do the C driver, Rust archive,
C++ object and twelve runtime objects. Formal-test construction took 0.739 s
and its execution command 3.467 s; the original chain plus new linear-pair
trace passed with 33 exact-once allocations/releases in both lowering modes.

The unmodified formal source exited 0. Changing each of the seven new
sequence/count/prefix/capacity expectations separately compiled and linked,
then exited 26. Those eight programs' emit/link/execution phases totaled
2.045/1.344/2.033 s. A proposed fault construction that copied the private
implementation into caller source first hit FORM-2 from an extra newline,
then TYPE-2 because the public backing field is readonly outside its module.
Neither is a failure of the production source or evidence of a language gap.

The replacement order falsifier uses the accepted formal LLVM. It resolves
the new `linear_terminal_pair` trace's Ticket-with-ceiling-six truncate
instance, exchanges only the two adjacent terminal callback arguments, and
checks that reversing those two edits restores every byte. The recompiled
unchanged module exited 0; the reversed-order module exited 26, with all
other source and LLVM unchanged. The two links took 0.373 s and their
executions 0.630 s. The main guarded sequence took 24.91 s and the successful
final order control 1.18 s, with no queue wait; the intervening readonly
scratch rejection took 0.11 s. No benchmark measurement ran.

The frozen revised library SHA-256 is
`530efe0282ad2aa7f0bf446ca92feee194720381422ff50e1637b4ea2ec2cf92`;
its compiler is
`f6d0c59e150e589bf03b1318ec7130ef4efeacde7f476437d09b4bfe04f4ead2`,
raw timed LLVM
`54bfcc26c6ada4cbf95219422706819665469c1df231474cd364f682f677d0a5`,
and linked image
`37b53780e4db5f4338f617e26d87a3eea563ff428f279e478adbf2c2de5cd0ad`.
The source/artifact manifest was frozen after the controls, before any later
embedded compiler rebuild. Correctness alone did not select J; the separate
native criterion below rejected it before timing.

### J native gate: rejected before timing

J fails the prerecorded unchanged-suffix-one and no-new-hot-call criteria.
No timing ran. Inspection of the frozen final image confirms a local success:
the wide terminal pair reads both records directly from backing and removes
the 256-byte rear-to-left relocation without a replacement payload snapshot,
spill or reload. Only the later owner's final two fields remain in registers
across the first digest. This does not rescue the complete source form.

The wide truncate body now saves the frame/link registers in 16 bytes and
calls `grow_vector_consume_back` on every positive suffix, including one.
F's truncate is frameless and contains the remainder itself. The new call
at image address `0x10000d8ac` is reached both from the non-pair branch and
after the direct terminal-pair digest; the successful pair construction is
`0x10000d704` through `0x10000d8a8`. The helper boundary survived ordinary
O3 despite the unchanged callback interface and adds work precisely where
the preregistration prohibited it.

Scalar consumption remains inlined, but actual executed instruction counts
also fail the suffix-one criterion. Counting from digest-seed initialization
through the branch back to outer-round bookkeeping, excluding append, gives:

| Removed suffix | F | J | Change |
| ---: | ---: | ---: | ---: |
| 1 | 16 | 24 | +8 |
| 2 | 34 | 24 | -10 |
| 3 | 41 | 52 | +11 |

These counts follow the actual trace's selected loop/dispatch paths, rather
than comparing whole helper sizes. J begins this region at `0x10000bd2c`
and returns to `0x10000bc24`; F begins at `0x10000bd34` and returns to
`0x10000bc20`. The terminal-pair path improves locally, while the empty check,
terminal dispatch and remainder organization add other work. No elapsed
contribution or hypothetical timing benefit is assigned to those differences.

The complete rejected library, public documentation, owning fixture and
ledger change is preserved in [terminal-pair.patch](terminal-pair.patch),
SHA-256
`c2ba683da960683d4b750d4aa403140fae0e23035d84de01c230b6e420c109c4`.
It applies to base `9fed1c60c388610b8f7b091103bc6712d277844f`; apply it in an
isolated checkout and use the gate/two-job construction and fresh
`.build/terminal-pair-j` commands above. `git apply --check` passes after
restoring all four implementation/test files byte-for-byte to F; the restored
library again has SHA-256
`72ff83f18181651461e11f6dd95394556dce372710cb3a58376a6d4e099e7339`.
The identified J compiler, image, LLVM and passing evidence remain frozen.
Restoring source does not replace an already built embedded compiler.

The [pending amendment](../../../../design/amendments/vector-terminal-pair.md)
now recommends declining this tested form; that recommendation is not an
owner ruling. Reopen the terminal-pair idea only with a concrete ordinary
proof/control shape that avoids the retained helper boundary and added
suffix-one/odd-suffix work while preserving the demonstrated absence of
payload relocation. The initial proof join and the revised native failure
must both be addressed; neither a source-only operation count nor a rerun of
this image supplies new grounds.

### H: realloc for runtime Slots growth

H1 is rejected without timing: unconditional realloc restores an empty
allocation that F's optimizer had eliminated. H2 restricts realloc to full,
nonempty storage and passes its construction, ownership and native-code
criteria, but its subsequent full-matrix comparison has three separated
regressions. Neither is selected for production. These are raw-LLVM causal
probes on the pinned [F source](https://github.com/mbbill/Whitefoot/tree/3347fcb4b705990b5b3a9d66887a30ca9e632374),
using F's compiler and inputs identified above; no compiler, library,
specification or live design-tree change implements either policy.

Both transforms change exactly two of the 62 raw function bodies:
`wf_grow$instance$e194368921a477a7` and
`wf_grow$instance$04ee659886c12146`, for scalar and wide storage, plus a plain
nullable `realloc(ptr,i64)` declaration. H1 replaces malloc, initialized-prefix
memmove and old free with realloc. H2 selects that route only when
`old_len == old_cap && old_len > 0`; its other branch retains the original
malloc, `old_len * complete_stride` memmove and free. The selector runs only
inside grow, including equal-capacity growth, and adds no spare-append check.
Ring, every other raw body, WF source, native controls and runtime objects
remain unchanged. Neither patch adds an alias, nonnull or no-wrap promise.

H1's prerecorded native criterion requires growth to eliminate explicit
copy/free without stale old-pointer use, compensating payload traffic or a
new consumer call, while retaining F's single-allocation empty setup. Sparse
occupancies were a proposed adverse timing control for H1, not a promise
that unconditional realloc preserves length-only copying. H2 separately
requires its empty/sparse fallback to copy only initialized bytes.
Captured length and requested capacity are written through the returned
pointer before owner publication;
null reaches the existing heap resource floor with the old owner unchanged.
The 16-byte descriptor keeps requests positive even for zero-byte elements.
OP-10 already permits relocation by grow, and STOR-7 permits complete owning
representations to move. These observations introduce no language rule.

Only passing code and correctness observations admit timing. The timing
criterion retains every original useful cell and rejects a repeatable
regression; a growth gain cannot excuse a suffix, reuse or smaller-population
loss. H2 was preregistered separately after H1's rejection, without changing
H1's verdict or deleting its adverse empty/sparse cases.

### H construction, allocation and failure observations

Each trial's four core images passed 1,260 configurations and 8,820 executions
apiece. Both accounting arms use the same prefix-aware observer: it passes
the prefix base to libc realloc, captures old byte counts before the call,
updates the returned prefix on success, and never touches the superseded
pointer afterward. Timed images contain ordinary malloc/realloc/free and no
observer. A successful resize retires one allocation lifetime, so the exact
identity is `requests == releases + realloc_requests`; failed attempts add
no successful request. Logical requested peaks and possible old-plus-new
overlap bounds do not measure physical copying, in-place frequency or RSS.

Every accounting CSV contains 294 rows. Rebuilt controls are byte-identical
to frozen F. H1 changes all 42 WF rows; H2 changes only the six growth rows,
leaving 36 other WF rows and all 252 native rows identical. Successful request
counts, requested bytes, checksums and overlap upper bounds match control.
For H2's three-round growth observations, populations 16/256/4096 make
21/33/45 requests, 15/27/39 successful resizes and six explicit releases.
The wide n=4096 row requests 12,582,864 bytes, with a 2,097,168-byte logical
peak and 3,145,760-byte overlap upper bound. The formula retains the larger
of the first empty-growth overlap and final extent for tiny counts.

The independent ordinary-source direct witness covers empty 0→0 and 0→1,
full equal-capacity 1→1 and growth 1→2, and wide capacity 4096→8192 with
lengths 0/1/4096. It checks length, capacity and every initialized word.
Two padded 24-byte Tickets each own one distinct box; the witness checks both
identities through full 2→5 growth and exactly-once consumption. Huge
zero-byte storage grows through `2^63 + 1` to `u64::MAX` without a huge loop. H2 additionally
checks full zero-byte capacity 1, length 1→capacity 2, since the huge sparse
case does not exercise its realloc predicate.

Real allocation, forced movement and forced in-place execution each produce:

| Direct witness | Successful requests | Resizes | Explicit releases |
| --- | ---: | ---: | ---: |
| H1 control | 21 | 0 | 21 |
| H1 | 21 | 10 | 11 |
| H2 control, including full zero-byte case | 23 | 0 | 23 |
| H2 | 23 | 5 | 18 |

The exact extent multiset is also checked: H1 has byte extents
`8×2, 16×6, 24×4, 32×1, 64×1, 136×1, 1048592×3, 2097168×3`;
H2 changes only the 16-byte multiplicity to eight. Forced movement poisons
and quarantines the old identity. Forced in-place execution preserves one
physical address while changing logical generations; equal pointer bits
alone cannot identify stale provenance, so the forced-move and native
post-realloc-use checks remain separate obligations.

Each trial records all 26 direct process outcomes: eight successful
real/move/in-place/publication processes, four empty/live allocation failures,
six shared release faults and eight candidate-specific faults. Both arms'
empty and nonempty failures preserve owner, contents and successful ledger,
then print the witness line and exactly `{"resource":"heap"}` plus newline
before Darwin SIGABRT (subprocess status −6). H2's empty failure uses the
malloc fallback; its full nonempty failure uses realloc. All deliberate
faults exit 1 with these first distinguishing diagnostics, prefixed by
`H direct grow:` or `H2 direct grow:`:

| Fault | Diagnostic |
| --- | --- |
| Double release, or duplicate the second owning Ticket over the first | `allocation released twice` |
| Foreign release | `operation on foreign allocation` |
| Missing release | `terminal lifecycle balance` |
| Reclassify one resize as an explicit free | `successful resize count` |
| Omit fresh owner publication | `fresh owner publication` |
| Omit capacity publication | `published capacity` |
| Publish zero instead of captured length one | `published length` |
| Free the superseded backing after successful resize | `release of resized allocation` |
| Publish null before the failure witness | `failure preserved owner cell` |
| Return after the failure witness instead of reaching the floor | `resize failure returned`, after the preserved-owner witness |

The existing benchmark checksum and cleanup faults also fail, and a resize
misclassification that preserves the broad balance equation fails the
independent geometric lifecycle check. These are one-off validation
observations, not newly maintained regression coverage.

| Completed command phase | H1 seconds | H2 seconds |
| --- | ---: | ---: |
| Accepted direct-source preflight | 0.045 | 0.048 |
| Core native construction | 2.004 | 2.128 |
| Direct LLVM emission and native construction | 1.123 | 1.192 |
| Four core correctness images and observer faults | 3.224 | 3.227 |
| Accounting execution | 0.087 | 0.088 |
| Direct positive/fault execution | 1.926 | 1.865 |

All listed commands exited 0. Two preceding H1 preflights exited 2 in
0.397/0.031 s: FORM-2 required canonical formatting, then OWN-1 rejected
moving a copyable box field. Formatting and an ordinary box-consuming reader
repaired those fixture errors while retaining every predicate and consumption
order. No Rust compiler rebuild occurred. H2's guarded construction/check
command took 8.62 s; the table separates construction from execution and
does not describe correctness commands as isolated kernel timings.

### H native verdicts and reproducible evidence

H1's actual scalar/wide reuse and suffix constructors execute calloc plus
realloc where F executes one malloc: one extra allocator call and null check
per trace. This fails the recorded no-new-setup-work condition before timing,
despite smaller grow helpers. It is an adverse code result, not a measured
regression. H2 restores the single malloc and null check. Its full branch
uses only the returned owner after realloc; the fallback retains length-only
copying. The direct wide image confirms dispatch for lengths 0/1/4096.

| Native body | Control instructions / frame bytes | H1 | H2 |
| --- | ---: | ---: | ---: |
| grow, either width | 27 / 64 | 19 / 48 | 37 / 64 |
| grow_full, either width | 52 / 64 | 41 / 48 | 73 / 64 |
| scalar trace | 246 / 160 | 238 / 144 | 200 / 128 |
| wide trace | 190 / 352 | 198 / 352 | 190 / 352 |

The frozen H1 audit incorrectly described its grow frames as unchanged at
48 bytes; the table corrects them to **64→48**, while preserving the frozen
record and rejection. H2's generic grow retains cmp/ccmp/branch selection;
grow_full still compares length with capacity on nonempty routes. Scalar
trace growth becomes a grow_full call on its existing full-capacity edge,
changing outer control flow and register allocation. Scalar suffix's steady
spare path retains backing, digest and loop state in registers, removing
F's extra stack reloads before consumption (`0x10000bd28`–`0x10000bd38`).
Its exchange/take loops remain 12/7 instructions per iteration, at
`0x10000bc88`–`0x10000bcb4` and `0x10000bcc0`–`0x10000bcd8` in H2;
the consumption algorithm still executes. Wide trace is normalized-identical
to control. Both widths of append, tail_work, work, round, truncate, insert,
remove and swap_remove remain normalized-identical. There is no new payload
snapshot or consumer call. Later timings therefore include guard, inlining,
register and placement differences; they do not isolate libc realloc cost.

Native inspection uses LLVM 22.1.8
`llvm-objdump --macho --disassemble --no-show-raw-insn` on linked images,
with branch destinations and symbols normalized. For H2 scalar grow,
realloc at `0x10000de7c` publishes its returned address at `0x10000de8c`;
the wide pair is `0x10000df40`/`0x10000df50`. No old backing is read or
freed on the successful resize route. Shared native/runtime objects were
hash-checked, rather than exhaustively re-audited as changed code.

Frozen local evidence lives under `/private/tmp/whitefoot-vector-realloc-H`
and `/private/tmp/whitefoot-vector-realloc-H2`. H1's `frozen-H1.json`
SHA-256 is `3914a55a6f88d1a3c9ea4d75fefb30a8cee511ee3fb24c474f094ca683dd80d5`;
all 87 listed files were reverified. H2's construction `manifest.json` at the
pre-timing freeze had hash
`a3ab903db7184a2119c8410d404a59d8406546212876eed361a45c06b0bef82e`,
and its 113-input `pre-timing-manifest.json` is
`082c11a29ce5fdb17683bf11dac00c5a245ec851d2473e25e2444e2aabda14fb`.
After the pair's successful before/after checks, README.md and manifest.json
were updated with the results; the other 111 recorded paths still match.
The original two documents were reconstructed into distinct
`README.pre-timing.md` and `manifest.pre-timing.json` snapshots and verified
against their pre-recorded hashes. `post-timing-document-audit.json` records
that later documentation change; it is not a change to source, IR or images
during the pair.
These identify local validation records, not dependencies of a fresh checkout.

The exact small replay transformations are preserved here:

| Patch | SHA-256 |
| --- | --- |
| [H1 grow](realloc-h1-grow.patch) | `cb6e1801cdb21e759b88fc341b60b8525ed0d8052450a577a4c3ec2375d16ec3` |
| [H2 grow](realloc-h2-grow.patch) | `360f14b4b2a6c5e35d95665700cd9da8046740660302f24a48762e2b7bb55155` |
| [H1 observer](realloc-h1-observer.patch) | `cbcf24325700c2156d8438b96c38264e5a0a05a1e5b9ec764012fd048e421916` |
| [H2 observer](realloc-h2-observer.patch) | `1da50d90b82b2ecad39cec3f32d8fa88ab633beb86c437642a79203af333ca7a` |

Keep these four patches available outside an isolated F checkout, then use
the F construction recipe above to recreate
the native inputs and timed LLVM. Its required SHA-256 is
`63038feb712920a11bb212eb06f858929186445eac6b2703afea392a9d74aa24`.
Copy that file separately for control/H1/H2 and apply each grow patch to its
candidate copy with `patch -F 0 candidate.ll < realloc-hN-grow.patch`. These
archived grow patches use zero-context unified diffs, regenerated from the
exact pinned input to the unchanged candidates; verify the input hash first.
The resulting H1/H2 LLVM hashes must be
`1017fc5e0976c02f7d40da9305e8f6384a8aa0597b8987fc3f10595408bc6356` and
`6667a036ad1a9873f09faa7ce14e941da29c74e95b4b3723b956aa14a9c0b137`.
Compile each with `clang -O3 -Wno-override-module -x ir -c candidate.ll -o candidate.o`
and reuse the family Makefile's frozen timed driver, C++/Rust and runtime link
inputs with `clang++ -O3 -pthread -lm -liconv -lSystem -lc`. The constructed
fresh control, H1 and H2 image identities are respectively
`e0d7a7bc8b1f53864a50b2a47e995074c1f3ecf3b04f873cf628da6590169bfe`,
`eb6d38c2a774bd0386a0770c34e437311ad133f1810e1f00750a17e281e75845` and
`32f9985f72b079b9ac59c2b8c3e3dedfbe3e011737436191c0cdbe41c10f1589`.
The rebuilt control object is byte-identical to frozen F's object; linked
image identity also includes naming and placement.

For accounting, apply the corresponding observer patch to separate copies
of F's `vector-costs.c`. Compile the shared patched observer in both arms
with `-DECOSYSTEM -DACCOUNT_ONLY`; add `-DWF_GROW_REALLOC` for H1 or
`-DWF_GROW_FULL_REALLOC` for H2 only. Rename LLVM malloc/free/realloc symbols
to `wf_cost_allocate`/`wf_cost_release`/`wf_cost_reallocate`, without adding
attributes; realloc remains nullable. Link the frozen accounting native
controls and run each image's `check` and `account` commands. Run the existing
`fail-checksum`/`fail-cleanup` commands and each candidate's
`fail-realloc-account`, requiring exit 1 and the exact recorded diagnostics.
The two observer patches deliberately retain different independently derived
lifecycle formulas. They are not interchangeable.

All construction/check commands belong under the shared verification guard.
The one-off direct source, identity adapters and mutated images are described
above but are not imported as a maintained harness; this replay does not
claim to recreate those 26 processes from committed files. A future selected
growth policy would move the useful observations into the existing growth,
owning-growth, container-allocation and exhaustion test homes. The current
five target records have compatible 64-bit allocation parameters, but these
executions qualify only Darwin. Linux/Windows execution and failure evidence
remain required; the current Windows corpus selection does not run the
container group. The existing malloc/memmove/free storage decision remains
in force, with no specification amendment from this rejected probe.

### H2 paired timing: gains and three disqualifying regressions

One guarded control/H2 pair invoked each frozen image with
`measure 1048576 7`, taking 81.401/80.585 s, both exit 0. The outer command
took 162.25 s with no busy retry. All 113 frozen paths matched immediately
before and after execution, before the documentation updates described above.
No construction is included in these timing costs, and
there was no replay. The [fresh control](ecosystem-realloc-control-samples.csv)
and [H2 samples](ecosystem-realloc-h2-samples.csv) each contain all 4,116 rows,
with hashes `27b33972f54cc1a4a31c90b81bd65a6b6a52fb5d220e0859463ef8fe34b90ffc`
and `7d39be9ec7ab55b89831e82db8e68e9295c07b909c3a3276cb2be6162fa663e2`.
All keys, work, rounds, traces and checksums match, with seven samples in
each of 588 groups. Both files pass `summarize-ecosystem.pl --complete` and
`--complete --targets`.

Ratios below are H2/control, spanning the two cohort medians. G/L mean
strictly separated gain/loss in both cohorts using unrounded sample ranges;
O means overlap in at least one. These are finite observed ranges, not
confidence intervals. Every useful cell appears; suffix-zero remains an
unranked control.

| Bytes | Path | n=16 | n=256 | n=4096 |
| ---: | --- | ---: | ---: | ---: |
| 8 | reserved | 0.996–1.006 O | 1.003–1.006 O | 1.004–1.015 O |
| 8 | growth | 1.327–1.354 L | 1.114–1.146 L | 0.927–0.950 O |
| 8 | reuse | 0.984–1.021 O | 0.973–0.990 O | 0.978–0.980 O |
| 8 | suffix-1 | 0.643–0.660 G | 0.665–0.672 G | 0.659–0.672 G |
| 8 | suffix-2 | 0.704–0.707 G | 0.649–0.690 G | 0.664–0.685 G |
| 8 | suffix-3 | 0.765–0.787 G | 0.775–0.780 G | 0.782–0.789 G |
| 256 | reserved | 0.992–1.001 O | 0.981–0.997 O | 0.984–0.998 O |
| 256 | growth | 1.096–1.109 L | 0.831–0.832 G | 0.870–0.871 G |
| 256 | reuse | 0.997–1.000 O | 0.994–1.007 O | 0.997–1.004 O |
| 256 | suffix-1 | 0.993–1.005 O | 0.978–0.999 O | 0.987–0.990 O |
| 256 | suffix-2 | 0.987–0.991 O | 0.968–0.992 O | 0.999–1.044 O |
| 256 | suffix-3 | 0.985–1.005 O | 0.985–1.010 O | 0.994–1.001 O |

There are 11 separated gains, three separated losses and 22 overlaps.
Median directions alone give 21 lower, five higher and ten mixed cells.
The three losses are scalar growth at 16 and 256, and wide growth at 16;
their candidate-minimum/control-maximum ratios in the two cohorts are
1.307642/1.294199, 1.088481/1.059981 and 1.075105/1.064567. They fail the
prerecorded no-useful-regression criterion. The nine scalar suffix gains
execute no full nonempty resize, so they cannot be credited to faster
realloc calls; their scalar caller/code-placement changes remain part of
the intervention. Wide growth gains at 256/4096 likewise do not isolate
allocator copying from the surviving guard and caller changes.

The independent per-cell native target improves from control's
12 passes / 8 deficits / 16 inconclusive to H2's 19 / 5 / 12, each retaining
six unranked controls. This does not override the three regressions or
establish every requested target. Every useful WF comparison meets the
duration and cohort qualifications: minimum samples are 1.520/1.502 ms,
and maximum H2/control cohort-ratio spread is 6.4504%. All 136 control and
187 H2 sub-millisecond rows belong to suffix-zero.

Native-control drift is nonuniform: H2/control median ratios range
0.743876–1.029120 for Rust and 0.939024–1.032593 for C++ over useful cells.
Scalar n=16 reserved Rust shifts to 0.743876/0.759454 while its C++ control
stays at 0.986813/0.986854; no adjustment or sample deletion compensates for
that observation. In the three loss cells, Rust ratios span 0.955–1.000 and
C++ 0.949–1.009, so a common slowdown of all implementations does not explain
the WF losses. All four standard-library trace bodies and their relative
control flow are normalized-identical, but their entries and loop addresses
shift by +240 bytes. For example, Rust's scalar reserved loop moves from
`0x10001164c` to `0x10001173c`; inspected Rust/C++ scalar work helpers also
retain their instructions. This does not establish placement as the cause
of Rust's roughly 25% small-reserved drift. Allocator history in the rotated
same-process harness and other run-level variation remain unseparated.
Attribution comparisons that fail cohort stability remain
unranked: control scalar suffix-2 n=256 direct C (29.3132%), H2 scalar reuse
n=16 take/swap C (12.5351%), and H2 wide suffix-zero n=16 take/swap C
(23.7699%). Every original sample and outlier remains in the files.

Supplemental sparse/full timing was preregistered but not constructed or
run. Direct sparse/full correctness and native dispatch evidence do not
substitute for that missing performance observation. H1 remains untimed;
H2's recorded full-matrix failure is sufficient to reject this tested policy
without running another arm or silently introducing a capacity cutoff.

### Hbyte: allocation-byte attribution through one common caller

This separate diagnostic selects no growth policy or cutoff and leaves H2 rejected.
A is F's malloc/initialized-prefix-copy/free; R is H1's straight realloc;
P is H2's full/nonempty realloc with A otherwise. The extracted Darwin LLVM
preserves the F/H1/H2 inputs and grow patches identified above: only the exported
names of `wf_grow$instance$e194368921a477a7` change, with the existing resource
floor, nullable allocation, attributes and target layout retained. Source F is
`3347fcb4b705990b5b3a9d66887a30ca9e632374`; no current compiler build is needed.

Apple Clang 21.0.0, `-O3` and no LTO produce A/R/P helpers of 27/19/37 instructions
and 64/48/64-byte frames. One separately compiled C caller selects a runtime
function pointer. The original literal one-native-site criterion was not met:
Clang versions its loop into three sites for lengths 0, 1..7 and >=8. The explicit
pre-timing clarification requires identical caller instructions/site for all arms
within each fixed cell, which native inspection confirms; no arm specialization
or dispatch bridge appears. Two clocks enclose setup, initialization, grow,
content checks, checksum accumulation and free. These are complete grow traces,
not isolated allocator latency, physical-copy measurements or owning-value tests.

The preregistered grid is cap 0 and powers of two 1..131072, with distinct lengths
{0,1,cap}: 54 cells, old requested bytes `16+8*cap`, new cap 1 or twice old cap.
Calibration records one rounds 1/seed 101 warmup per arm/cell, then powers of two
through 65,536 in orders A,R,P and P,R,A, selecting the first count whose six
intervals all reach 2 ms; each child has a 30 s limit. The retained calibration has
162 warmups and 4,926 pilots. Selected counts are 32..65,536, with cell minima
2.146–3.908 ms. Ranked order is cohort, sample 0..6/seed 101..107, ascending cap/length,
then arm, reusing each cell's count: all 2,268 rows remain, without refitting.

The original runner stopped on its first valid zero-duration warmup (native exit 0,
checksum 102; runner exit 1, 0.291 s/guard 0.44 s). The routine v2 repair retains zero
intervals: they cannot qualify 2 ms, and ranked zeros remain unresolved, never gains.
Synthetic checks exercise both distinctions. V2 retains 63 zeros: 18 warmups and 45
pilots. No native input, schedule or bound changed; the failed v1 record is retained.
Construction/check/native inspection took 0.278/0.454/0.048 s; checks covered 324 traces
and six deliberately failing result oracles. A comment-only rebuild took 0.129 s and
kept object/image bytes. Corrected calibration took 19.855 s (guard 19.99), ranked
execution 26.602 s (guard 26.80), all exit 0. One live-guard rejection ran no data;
its inspection wait was unmeasured. All 36 frozen paths matched before/after ranking,
without sample retry. Intervals are 2.146–470.842 ms: no ranked cell is unresolved.

G/L require separated sample envelopes in both cohorts; O is overlap, U unstable.
The only U is R/A at cap 512/len 0 (old 4,112→new 8,208 B): median ratios 2.478036/2.728739,
cohort spread 10.117%, above the preregistered 10% limit. It stays in the raw file.

| Ratio, all 54 cells | G | L | O | U |
| --- | ---: | ---: | ---: | ---: |
| R/A | 15 | 31 | 7 | 1 |
| P/A | 5 | 11 | 38 | 0 |
| P/R | 25 | 10 | 19 | 0 |

Full cells give R/A 5G/7L/6O and P/A 5G/11L/2O; P/R overlaps all 18. Small-full
losses survive the common caller (old 32 B R/A 1.393/1.380), so H2 caller outlining
alone cannot explain them. Five full gains occur at sampled old extents 32,784,
65,552, 131,088, 262,160, 524,304 B (R/A 0.740–0.818); the largest 1,048,592 B overlaps again.
P/A overlaps all 36 empty/one-live cells (medians 0.9913–1.0218). At the largest
extent, straight R/A instead costs 140.786–144.106× empty and 135.434–143.142× with
one live element. The sampled behavior is nonmonotonic; it selects no threshold.
Helper placement, indirect-call cost and allocator/process variation remain part
of this Darwin observation. H2's separate owning, zero-stride and failure evidence
is not inferred from this copyable probe; no source/specification rule changes.

These four exact measured inputs/results remain here while this evidence is cited:

| Artifact | SHA-256 |
| --- | --- |
| [Caller](allocator-byte-driver.c) | `c94c914338f752a3f37e80a5c769d7a410cccb24cba2a7853406dd2fa3392d93` |
| [Extracted helpers](allocator-byte-grow.ll) | `b6ef4afef63e8d7a9685b35e267e2e2fc86f01605d12e8dced867b37613925e3` |
| [Ranked samples](allocator-byte-samples.csv) | `421c18282439acbcbceb3d35a152ae1c07d18655bf1676fba7006aad309a03b9` |
| [Calibration](allocator-byte-calibration.csv) | `4b4da63ec7779159fcb75f255340256cfcacde88eb4e3e555d1cc2afe38f51f6` |

Measured image SHA-256: `f370f0cd838272ab0b41fdd7594e7a07b4190b673e717519b81836be4b982880`.
The recipe's build/check/six faults passed in 0.238/0.439/0.090 s, without retiming.
Both objects reproduce exactly; only 16 UUID and 32 signature bytes differ in the image.
Opt-in replay from the repository root, on the recorded arm64 Darwin ABI; verify
the four hashes first. No Make target or correctness gate consumes these files.

```sh
d=research/experiments/container-representation/vector-library
b="$d/.build/allocator-byte"; mkdir -p "$b"
perl .github/run-check.pl allocator-byte-build sh -eu -c '
/usr/bin/clang -O3 -Wno-override-module -x ir -c "$1/allocator-byte-grow.ll" -o "$2/grow.o"
/usr/bin/clang -std=c11 -O3 -Wall -Wextra -Werror -fno-lto -c "$1/allocator-byte-driver.c" -o "$2/driver.o"
/usr/bin/clang -O3 -fno-lto "$2/grow.o" "$2/driver.o" -o "$2/hbyte"
' sh "$d" "$b"
perl .github/run-check.pl allocator-byte-check "$b/hbyte" check
perl .github/run-check.pl allocator-byte-replay python3 - "$b/hbyte" "$d/allocator-byte-samples.csv" "$b/replay.csv" <<'PY'
import csv, hashlib, pathlib, subprocess, sys, time
exe, source, output = sys.argv[1:]
assert hashlib.sha256(pathlib.Path(source).read_bytes()).hexdigest() == "421c18282439acbcbceb3d35a152ae1c07d18655bf1676fba7006aad309a03b9"
faults = {"content": "initialized payload", "publication": "published capacity",
          "length": "published length", "status": "grow status",
          "null": "published nonnull owner", "checksum": "independent checksum"}
for mode, message in faults.items():
    p = subprocess.run([exe, "fault", mode], capture_output=True, text=True, timeout=30)
    assert (p.returncode, p.stdout, p.stderr) == (1, "", "Hbyte: " + message + "\n")
with open(source, newline="") as stream: rows = list(csv.DictReader(stream))
assert len(rows) == 2268
fields = "arm capacity length new_capacity old_bytes new_bytes rounds seed elapsed_ns checksum".split()
with open(output, "x", newline="") as stream:
    writer = csv.DictWriter(stream, fieldnames=list(rows[0])); writer.writeheader()
    for row in rows:
        args = [exe, "run", *[row[k] for k in "arm capacity length rounds seed".split()]]
        start = time.monotonic()
        p = subprocess.run(args, capture_output=True, text=True, timeout=30)
        wall = time.monotonic() - start
        assert (p.returncode, p.stderr) == (0, ""), (p.returncode, p.stdout, p.stderr)
        values = list(csv.reader(p.stdout.splitlines()))
        assert len(values) == 1 and len(values[0]) == len(fields)
        actual = dict(zip(fields, values[0]))
        assert all(actual[k] == row[k] for k in fields if k != "elapsed_ns"), actual
        writer.writerow({**row, **actual, "wall_seconds": wall}); stream.flush()
PY
```

This replays the complete fixed schedule, not calibration or a subset. Retain
every fresh row; intervals below 1 ms, including zero, remain unresolved. Compare
cohort medians and sample envelopes with the same 10% spread qualification; never
subtract shared work or interpret a requested extent as physical bytes copied.

## Same-source forward-consumption diagnostic K — failed gates, mixed timing

K is not selected. It first **failed its preregistered native admission
criterion** because wide consumption gained one retained callback call per
owner. A separately recorded, post-native but pre-timing exploratory stage
then measured that tradeoff without changing the images or treating the
original gate as passed. The full pair has 10 strict useful gains, 13 strict
useful regressions and 13 overlaps, so it also fails the no-useful-regression
condition. Every gain is scalar and every regression is wide. No production
compiler, library, source rule or live-tree decision changes from this probe.

The question was whether an equivalent lowering of the existing ordered
consumption region can reduce its take/swap movement and controller cost.
Source F remains `3347fcb4b705990b5b3a9d66887a30ca9e632374`. The experiment
replaces only the scalar and 256-byte-record truncate **bodies** in its raw
LLVM, preserving signatures, attributes, callbacks and every other byte.
The candidate captures the backing and entry length, consumes each original
suffix owner in increasing index order through the existing callback ABI,
then stores the retained length. It allocates nothing, preserves the prefix
and capacity, and adds no inline hint, arithmetic flag, assumption or alias
metadata. The accounting module receives the same two replacements.

This is a manually qualified lowering diagnostic, not a by-name production
optimizer or evidence of a source-language gap. Its semantic boundary requires
a stable contiguous backing, the original callback/cleanup order, disjoint
callback effects, no intermediate backing observation and no partial normal
exit. EFF-5/OWN-9 disjointness and STOR-7 relocation support that reasoning;
`noalias` alone does not. A private ownership-state relation must account for
consumed slots until the final descriptor store. For example, after consuming
A from [A,B,C,D], the source algorithm leaves [D,B,C]. An intervening read or
partial return could observe D, whereas a naive forward traversal leaves stale
A. A future general recognizer must retain the original lowering or materialize
that state for such regions. This probe does not implement those fallback tests.

The original pre-generation criterion required no replacement owner copies or
new hot call, ordinary O3 natural inlining, complete correctness/accounting and
a full-matrix no-regression comparison. Its note has SHA-256
`b36b26ac2873909e35cadd3f8f386c7d42e4a77f286da26e8ea442cd29701fad`.
After native inspection and an independent method review, stage 2 explicitly
retained that failure and asked about the unknown net cost of removed movement
and controller work versus the new callback boundary. Its prospective note,
SHA-256 `47090e090ad6f626f60f6628788307da37aa0653490f64e4b0ffea9012e8c0bc`,
required exactly one complete paired run, all cells/cohorts/outliers and the
existing duration, stability and no-regression rules. No extra optimizer pass,
re-link, callback edit or inlining experiment intervened.

### Construction, independent observations and native code

Both arms were compiled once from raw LLVM with ordinary `clang -O3` and linked
in the same order against frozen F driver, Rust, C++ and runtime objects. The
rebuilt control is byte-identical to the original F executable. Relevant
SHA-256 identities are:

| Artifact | SHA-256 |
| --- | --- |
| frozen F compiler | `fe856a7b9ab2827bb30515547a82bc01132b65e4e3110dd0371cdf0cfc88b3eb` |
| F timed LLVM | `63038feb712920a11bb212eb06f858929186445eac6b2703afea392a9d74aa24` |
| K timed LLVM | `257167a7dd5605a204e403af98170d6ca03a4d7d46076b9b3628bab8702e1b6c` |
| F control executable | `44660c2de9532af3392c3c5fefea363b1915abd03bc9b79f4ba39812425c05f2` |
| K executable | `2fefebb0e52792d7cdf71f230b24118a7e4f40c11da42ba42b63e0680ce4e939` |
| both complete accounting CSVs | `ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7` |

All four timed/accounting images passed the unchanged 1,260-configuration,
8,820-execution matrix, and all 294 accounting rows are byte-identical to F.
Checksum and cleanup negative controls exited 1 with their exact independent
oracle diagnostics. Timed images contain no allocation-observer hooks.
The unchanged formal Vector fixture passed with 25 allocations released once
in both lowering modes, each using ordinary free and the dirty/quarantined
observer, for both LLVM arms. A separate scratch caller added the six-Ticket
witness preserved in [terminal-pair.patch](terminal-pair.patch), compiled with
F's unchanged library: it consumes IDs 3,4,5,6, preserves 1,2, then drains them,
checking independent fixed order, prefix, capacity and count observations.
That caller passed with 33 allocations in the same four arm/mode combinations
and both release routes. Reversing only that owning truncate's callback order
returned 26; omitting the owning callback's release preserved the digest but
failed the ledger with exit 1 and `every allocation is released exactly once`.
The observer's concurrent success and double/foreign/missing-release failures
also retained their required outcomes.

The formal substitutions cover scalar, affine Box and nodrop Ticket instances,
all with positive eight-byte element stride; the timed record has stride 256.
They do not prove arbitrary linear callbacks. A general transform still needs
a parametric ownership/permutation argument and witnesses for moving owners
into disjoint environment storage, padded/nested owners, zero-sized elements,
intermediate observations/partial exits, and effect-prefix preservation when
a callback diverges. Zero stride must not erase logical callbacks or truncate
large logical counts. No such untested generality is selected here.

Final native inspection finds the scalar consumer inlined with a four-instruction
forward loop and no callback call, relocation or per-element length store.
The actual scalar trace frame nevertheless grows 160 to 176 bytes and the
positive-suffix entry reloads the backing and five scalar stack slots. Wide
truncate also inlines, but the record callback remains out of line: it reads
backing directly using 16 paired loads and 32 digest `madd` instructions, with
one environment load/store and no payload staging or stack frame. Actual wide
tail/work/trace frames remain 288/688/352 bytes. Suffix-2/3 now execute two/three
callback calls instead of one truncate call; suffix-1 retains one call boundary.
The final full drain similarly calls once per retained owner. These caller
observations explain the original code-gate failure without establishing an
elapsed-time cost for any one instruction, transfer or boundary.

All four final Rust/C++ scalar/wide trace bodies retain normalized instruction
identity, and their object inputs are byte-identical. Other public Vector
helpers remain unchanged after branch/constant-reference normalization, but
locations and inlined copies inside changed callers differ. This is a complete
lowering/optimizer/layout comparison, not an isolated measurement of copying.

The guarded construction/check run took 18.18 s with no queue wait: native
construction 5.777 s, formal emission 0.855 s, correctness execution 8.845 s
and native inspection 2.259 s, with the remainder in orchestration. Every phase
status was retained; no timing was run until the separate stage-2 criterion.

### Complete exploratory timing

The frozen images each ran `measure 1048576 7` once in the same guard: control
80.750 s, K 81.452 s, both exit 0; the paired wrapper took 163.271 s with no
queue wait. All 332 frozen artifact hashes were rechecked afterward. Fresh
[F control samples](ecosystem-forward-control-samples.csv) and
[K samples](ecosystem-forward-k-samples.csv) each contain 4,116 rows and have
SHA-256 `36961e1f99c0a9cdc701d7e16c105715cd4d30e567c3d5031e523d7d80870f80`
and `398ae392862735819a2fb014f097461606ad2b6b122f9d3aabb768d515c12d7a`.
Keys, sample IDs 0–6, work, rounds, traces and checksums match exactly; reducer
self-tests, complete reductions and standard-target reductions pass. Earlier F
samples are not pooled into this pair.

The table covers all 36 useful cells; each range spans the two cohort medians.
Ratios compare whole traces. The last column is descriptive and does not itself
establish a qualified target pass.

| Bytes | Path | K / F at 16 | at 256 | at 4096 | K ms at 4096 | K / slower standard at 4096 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | reserved | 0.896–0.916 | 0.962–0.994 | 0.986–0.989 | 1.664–1.665 | 0.786–0.790 |
| 8 | growth | 0.973–0.991 | 0.972–0.997 | 0.981–0.991 | 2.056–2.067 | 0.844–0.859 |
| 8 | reuse | 0.779–0.808 | 0.974–0.987 | 0.981–0.993 | 1.632–1.644 | 0.791–0.806 |
| 8 | suffix-1 | 0.918–0.927 | 0.954–0.958 | 0.932–0.952 | 2.769–2.787 | 0.932–0.946 |
| 8 | suffix-2 | 0.614–0.615 | 0.584–0.586 | 0.581–0.595 | 1.419–1.424 | 0.854–0.865 |
| 8 | suffix-3 | 0.680–0.712 | 0.662–0.667 | 0.663–0.671 | 1.314–1.322 | 0.750–0.754 |
| 256 | reserved | 1.031–1.034 | 1.045–1.065 | 1.050–1.064 | 43.193–43.633 | 0.979–0.984 |
| 256 | growth | 1.010–1.022 | 1.033–1.033 | 1.028–1.030 | 54.816–54.822 | 1.037–1.040 |
| 256 | reuse | 1.025–1.029 | 1.049–1.057 | 1.054–1.055 | 43.119–43.210 | 0.983–0.987 |
| 256 | suffix-1 | 0.842–0.856 | 0.850–0.935 | 0.837–0.911 | 19.209–20.980 | 1.147–1.252 |
| 256 | suffix-2 | 1.436–1.453 | 1.432–1.437 | 1.424–1.457 | 33.713–33.716 | 1.531–1.532 |
| 256 | suffix-3 | 1.267–1.276 | 1.266–1.273 | 1.269–1.274 | 33.814–33.843 | 1.279–1.286 |

Strict gains require K's maximum sample below F's minimum in both cohorts;
strict regressions require the converse. The 10 gains are scalar reserved/reuse
at 16, suffix-1 at 16/4096 and suffix-2/3 at every population. The 13 losses are
wide reserved/reuse at every population, growth at 256 and suffix-2/3 at every
population. All remaining useful cells overlap in at least one cohort. For
example, wide suffix-2 at 4096 has observed K/F envelopes 1.411–1.491 and
1.408–1.511, while scalar suffix-2 there has 0.557–0.613 and 0.580–0.618.
Wide suffix-1 medians are lower, but its complete K ranges span 18.947–29.817 ms
across cells/cohorts and overlap F; none is a strict improvement.

The fresh standard-target reduction is F 14 passes / 11 deficits / 11
inconclusive versus K 16 / 9 / 11, with six unranked controls each. K's scalar
counts are 15 / 1 / 2 and wide counts 1 / 8 / 9; total passes cannot hide the
wide regressions. The minimum useful WF sample is 1.171 ms. All sub-millisecond
observations are suffix-zero controls. No K/F paired cohort ratio exceeds 10%,
and F's native comparisons have no unstable ratios. K wide suffix-1 at 256 is
unstable against C++, Rust, direct C, reverse C and swap/take C; the selected
standard ratio has 10.889% cohort spread and is inconclusive. Every outlier is
retained. The three wide suffix-zero controls strictly improve (median K/F
0.827–0.842); all three scalar suffix-zero controls overlap and remain unranked.

Across useful cells/cohorts, unchanged Rust controls drift 0.960–1.020 and C++
0.971–1.028; direct C spans 0.895–1.132, so it is not a universal fixed clock.
For the large wide suffix-2/3 losses, Rust/C++ drift stays within approximately
0.998–1.013, far smaller than K's 1.266–1.457 ratios. This supports a real
image-level regression, not a causal percentage assigned to the callback call.
The result supports further investigation of the scalar lowering opportunity,
but neither an all-type forward transform nor a generic recognizer is selected.
Reopening the wide case needs a separately controlled way to address the new
callback boundary and other caller changes; repeating this pair until it wins
would not supply that evidence.

### Exact body reproduction

[forward-consumption-k.patch](forward-consumption-k.patch), SHA-256
`7e456010a80d4fa2874e3401473ffa25cd1fe4b4f23aa848cb567d8e98a3cfa4`, preserves
both exact replacements. In an isolated F checkout, build the normal ecosystem
inputs with the documented O3 configuration. Copy each pinned timed/accounting
LLVM input separately to `whitefoot.ll` and apply the patch with
`patch -p0 --batch -i forward-consumption-k.patch`; the patch reproduces both
candidate modules byte-for-byte. The F accounting input hash is
`2ee32650b03a6e1a42f3ef2fe55bc569066e150c86024dd3f6a64a479a0f6518`, and the K
accounting result is `be11dd6b21b42e3175a94df87ba58303a19278528780675e7516d1ab6eee20da`.
Compile original and patched raw modules once using
`clang -O3 -Wno-override-module -x ir -c`, link against the same frozen objects
in the existing Makefile's order, and run complete checks/accounting before the
paired measurement. The separately emitted optimized LLVM is for inspection;
it is never linked as a second O3 pass. The raw patch is research evidence only
and is not wired into the compiler, fixture, default target or canonical gate.

### Explicit artifact exports: smaller code, native admission fails

The 2026-09-27 linkage discriminator is rejected before timing. Frozen F and K
each received broad/export-limited arms in both timed and accounting form.
Exactly 53 of 62 definition headers gained `internal`; all bodies, order,
signatures, attributes, globals and declarations stayed byte-identical.
The native demand inventory includes every object and Rust archive member.
It retains `wf_vector_library_word_trace`, `wf_vector_library_record_trace`
and `wf__main_body`, plus the existing weak `wf__floor_run` interposition.
This explicit artifact contract limits foreign calls/name lookup to those
roots; it changes neither WF source visibility nor the default source-bundle ABI.

Reproduce from the pinned F inputs and [exact K body patch](#exact-body-reproduction)
above, preserving broad copies. In export copies, add only the linkage token
to ordinary external definitions outside that four-root set; retain existing
private/internal/weak linkage. Removing those 53 added tokens must recover
each original module exactly. Compile each raw module once with
`clang -O3 -Wno-override-module -x ir -c`, then link the same frozen
driver/C++/Rust/runtime inputs in the family Makefile's order. There is no
additional optimizer pass, LTO, attribute hint or allocator change.
The export-limited timed LLVM hashes for F and K are respectively
`b538d6c2b80dcd9b983091facafc34570b36025694072984b2d26e853008af12` and
`b885181b2091ca66ef72cc86ded79304da33fb06fcb9708ab3c6c80e002def86`.

All four rebuilt broad objects and four broad images match frozen references
byte-for-byte. All eight full checks pass 1,260 configurations / 8,820 executions
each; four accounting outputs exactly match F's 294 rows. Checksum and cleanup
faults exit 1 with their original diagnostics. The independent scope verifier
rejects wrong root/helper linkage, altered body/attributes/declarations,
changed weak linkage and reordered definitions.
The phase ledger records 14.789 s total: compilation 1.253 s, linking 0.692 s,
full checks 6.617 s, with the remainder in identity/scope checks, accounting,
negative controls and inspection. The construction execution reported 14.99 s
outer elapsed. These costs are not program performance measurements.

| Timed arm | WF object `__text` bytes | Linked `__text` bytes |
| --- | ---: | ---: |
| F broad | 11,428 | 691,132 |
| F exports | 6,452 | 686,156 |
| K broad | 10,424 | 690,128 |
| K exports | 5,868 | 685,572 |

Admission required removal of K's per-owner acceptance boundaries throughout
the executed suffix, mixed-work and final-drain paths, without replacement
calls or payload staging. Linked K/exports improves its positive wide suffix
loop at `0x10000bfc4–0x10000c08c`: 51 instructions, 16 paired backing loads,
32 digest madds, no call or payload write. However, final drain still calls
`record_accept` at `0x10000c114`, and mixed work at
`0x10000cad0/0x10000caf8`, once per owner. The 52-instruction callback reads
the original backing and loads/stores its digest environment; no owner snapshot
replaces it. These surviving executed calls fail the complete criterion.

Both export arms also introduce hot `make_room` calls during prefix fill and
mixed append, even with spare capacity; broad arms called `grow_full` only
on the full edge. That helper has 48 instructions and a 48-byte total frame,
with 14 vector construction constants reloaded around calls. Repeated suffix
append retains its call-free spare path. Wide trace total frames grow
352→384 bytes for F and 352→400 for K as tail work moves into the caller;
wide mixed work remains 688 bytes. Saved constants are distinct from owner
payload, and disappearing standalone symbols do not mean disappearing work.
Three native standard-container traces normalize identically; C++ wide changes
15 constant-load addresses while the loaded bytes remain equal. Their entries
shift −4,976 bytes for F and −4,556 for K. Code/constant placement and caller
changes prevent an isolated callback-cost claim. No timing was run and no
production policy is selected; K's earlier failed gate and mixed timing stand.

Local evidence is `/private/tmp/whitefoot-vector-artifact-exports/construction-1`
(`phases.json`, saved checks, symbols, sizes and linked disassembly).
The adjacent final `native-audit.md` has SHA-256
`34bc17ccc5b9f2c2d850ae71c1b2003062730676fc5f9f9e7855782820b4d6ea`.
The input recipe above is portable; scratch tooling is not a checkout dependency.

### Entry-snapshot diagnostic: unchanged inline decisions

The next bounded F/K diagnostic rejected entry payload copies as the proposed
cause of K's retained callback boundaries. It selected no ABI change and ran no
performance timing. Reproduce from the same pinned **broad-linkage** F/K inputs
above, applying only these forms to `wf_vector_record_accept` in timed/account IR:
control unchanged; one-copy removes `%wf.slot.0` and its first memmove, then copies
`%wf.arg.v1` directly to private `%v2`; view removes both payload slots/memmoves and
uses `%wf.arg.v1` instead of `%v2` in the entry arm of `%v6`'s pointer phi.
Keep the environment slot, loop/digest, headers, attributes and every other byte.
The qualified body reads its 256-byte `[32 x i64]` payload, returns unit, and writes
only its disjoint environment; input storage lives through the synchronous call.
This does not generalize to copy-argument mutation, aliased result storage,
deferred uses or linear cleanup; their source/copy counterexamples still matter.
Use the guarded compile/link recipe above with the same native objects and no
linkage edits, LTO, hints or second O3 pass. Add `-Rpass=inline -Rpass-missed=inline`
only for diagnostic recompilation; its objects equal the ordinary objects.

| Timed raw variant | SHA-256 |
| --- | --- |
| F one-copy | `9d3bde29190bff26f1f4c20b88b72d7b8bbe97789eba61188839ef769376d845` |
| F view | `d0d090fc0efa5ac1067902fef5f6efe854378e83881c8123998f7a88f7bbd078` |
| K one-copy | `ae70b2a2e73c1942c43bb9539b4fa86c1c5826412e71f5e0ca8289dff6854bd5` |
| K view | `e245941537da0b458ab0fa3de029093d0045ec369e605ac1ad715cf09c08d83c` |

All eight candidate checks pass 1,260 configurations/8,820 executions each; four
accounting files equal F's 294 rows, with checksum/cleanup faults rejected.
All 74 phase statuses match expectations. Compile/link cost 1.750/0.681 s,
checks 6.124 s; guard total 15.564 s with no queue. One-copy images are byte-identical
to controls. Each view image changes only 14 `__text` bytes in the standalone
callback's first 20 bytes; caller instructions, calls, frames and section layout
remain identical. All three remark logs per source are byte-identical: staged
F/K sites stay 290/375; K backing sites stay 450/375 and final cleanup 440/375.
Both variants fail the complete native criterion, falsifying this specific
entry-copy explanation, not every copy optimization. Local records remain in
`/private/tmp/whitefoot-vector-parameter-staging`; its `native-audit.md` SHA-256 is
`cab9b893aa3c91f396f6f7d1a3152225b9c0ad7713ee91963980edec8a830aa5`.

### Ordinary behavior hints: combined K gains without useful-cell regression

This bounded diagnostic keeps broad linkage and the original two-copy parameter
bodies, adding ordinary `inlinehint` only to the four definitions supplied by
`WordElement`/`RecordElement`: `wf_vector_word_make`, `wf_vector_word_accept`,
`wf_vector_record_make` and `wf_vector_record_accept`. It includes both make and
accept actuals, not just observed missed sites. Source, ABI, specification,
allocator policy and native inputs are unchanged; no mandatory inlining, global
threshold, internalization, snapshot rewrite, LTO or second O3 pass is combined.
The C++ callback has no matching hint, so this is a backend preference diagnostic.

Before construction, the native criterion explicitly allowed replacing N owner
callback calls with one batch call, reporting suffix lengths 1/2/3 separately.
It required all K per-owner callback mechanisms to disappear without owner-sized
replacement staging, and preserved the full useful-cell no-regression criterion.
K+hint passes that native test: suffix, mixed-work and final cleanup call the
batch truncate; it reads directly from backing storage in a call-free, frameless
loop, with 16 paired loads and 32 digest multiply-adds per owner. Digest and length
publication occur once per nonempty batch. Suffix lengths 1/2/3 use one call each;
length one loses no dynamic boundary. The existing mixed-work swap-remove temporary
remains. Wide caller frames stay 352/288/688 bytes; wide mixed work gains an
environment-pointer spill/reload and changes scalar register allocation. The wide
callback cost 450 now fits the
observed threshold 487 instead of 375; the later 440-cost context disappears.
These are whole-image changes, not an isolated callback-time share.

F+hint timed/account objects and images are byte-identical to F, so the measured
pair is **F versus combined K+hint**, with no duplicate F+hint arm. Plain K's
failed native criterion and earlier 10-gain/13-loss result remain unchanged.
Four hint checks pass 1,260 configurations/8,820 executions each; both accounting
outputs equal F's 294 rows. Checksum/cleanup faults retain exit 1 and their exact
diagnostics. All 48 construction statuses are expected. Compilation/linking took
1.239/0.484 s, checks 3.534 s, accounting 0.075 s and faults 0.041 s; total guard
10.121 s includes remarks/inspection/identity checks, with no queue wait.

Exactly one full pair ran `measure 1048576 7`: F 81.834 s, K+hint 80.042 s,
both exit 0; the guard took 163.051 s without queueing. Each image retained
4,116 rows: seven implementations, two payloads, three populations, seven paths,
two cohorts and seven samples. All non-time fields/checksums agree, and all 208
frozen hashes match before/after. No sample was removed or selectively replayed.
The [F samples](ecosystem-behavior-hint-control-samples.csv) and
[K+hint samples](ecosystem-behavior-hint-k-samples.csv) have SHA-256 values
`3fd3b1a24af472cf0eb3e74834233fdea128bb1e0ce1177cfaffaef536396cba` and
`813ea85489cce03801a5572a83d09e841c94607fa2e286358d1369c5cdecf9eb`.

All 36 useful paired cells qualify: minimum WF sample 1.173 ms, largest
between-cohort ratio spread 4.165%. There are **13 strict gains, zero strict
losses and 23 overlaps**. Standard targets change from F's 13 pass/10 deficit/
13 inconclusive to K+hint's **22/4/10**. The four deficits are scalar growth at
16 and wide suffix-one at all three populations. Ten targets still overlap.
The table retains every cell: ratios span cohort medians; G/L/O are strict
gain/loss/overlap versus F using both full sample ranges; P/D/I are standard
target pass/deficit/inconclusive from F→K+hint, and U denotes unranked controls.

| Bytes | Path | K+hint/F at 16; target | at 256; target | at 4096; target |
| ---: | --- | ---: | ---: | ---: |
| 8 | reserved | 0.902–0.909 O; I→I | 0.979–0.983 O; P→P | 0.981–0.994 O; P→P |
| 8 | growth | 0.976–0.981 O; D→D | 0.956–0.964 G; P→P | 0.969–0.979 O; P→P |
| 8 | reuse | 0.776–0.792 G; P→P | 0.974–0.974 O; P→P | 0.982–0.992 O; P→P |
| 8 | suffix-0 | 0.954–1.014 O; U | 0.991–1.039 O; U | 1.009–1.039 O; U |
| 8 | suffix-1 | 0.927–0.966 O; I→P | 0.931–0.964 O; I→P | 0.906–0.941 G; I→P |
| 8 | suffix-2 | 0.592–0.609 G; D→P | 0.601–0.619 G; D→P | 0.577–0.597 G; D→P |
| 8 | suffix-3 | 0.678–0.678 G; D→P | 0.681–0.681 G; D→P | 0.658–0.680 G; D→P |
| 256 | reserved | 0.984–0.999 O; I→P | 0.995–0.997 O; P→P | 1.006–1.010 O; P→P |
| 256 | growth | 0.975–0.979 O; I→I | 0.990–0.991 O; P→I | 0.990–0.997 O; I→I |
| 256 | reuse | 0.982–0.986 O; P→P | 0.997–1.004 O; P→I | 0.996–1.006 O; P→P |
| 256 | suffix-0 | 1.143–1.162 L; U | 1.125–1.157 L; U | 1.146–1.155 L; U |
| 256 | suffix-1 | 0.831–0.842 O; D→D | 0.830–0.843 G; D→D | 0.831–0.838 G; D→D |
| 256 | suffix-2 | 0.935–0.944 O; I→I | 0.925–0.948 O; I→I | 0.935–0.941 O; I→I |
| 256 | suffix-3 | 0.959–0.966 O; I→I | 0.966–0.967 G; I→I | 0.940–0.957 G; I→P |

All three wide suffix-zero controls strictly slow down (1.125–1.162×); they
remain adverse overhead observations outside the useful target set.
Native inspection finds an unchanged-length store on K+hint's empty truncate
edge where F returns without it; both already call the batch helper. This
identifies extra work, not its isolated time share. Scalar
suffix-zero controls overlap and are below 1 ms. Wide suffix-one at 16 retains
cohort-0 sample 6 at 26.639 ms; despite lower medians, it has no strict paired
gain. Standard target comparisons have no duration/stability failures. Native
Rust/C++ median drift spans 0.955–1.055× / 0.958–1.040× across useful cells, so
unseparated changes are not credited as isolated compiler gains. Four useful
C-only comparisons are unstable: F scalar suffix-2@256/direct (30.589%) and
suffix-3@256/swap-take (16.855%); K+hint suffix-2@256/direct (12.221%) and
suffix-2@4096/direct (20.120%). These cannot support C-based attribution.

Reproduce from F source `3347fcb4b705990b5b3a9d66887a30ca9e632374` and the
[exact K patch](#exact-body-reproduction), preserving the original broad modules.
For each timed/account copy, add `inlinehint` immediately before `{` in exactly
the four definition headers listed above; removing those four tokens must
recover the original module byte-for-byte. Compile once with `/usr/bin/clang
-O3 -Wno-override-module -x ir -c`, link the frozen native objects in the existing
Makefile order, then use the same guarded check/account/fault and full-measure
commands. Do not substitute export-limited or entry-copy variants. K+hint timed/
account LLVM SHA-256 values are
`cc09151d5060e2a239b77fa20e45b8eef56b3ee65b9b93ffd69f6738e65b8c25` /
`9fa79e7237b4f8f835a8089c6987d38842b3c92db92c525d2b622dcba84717a5`;
its timed image is `37f2bbae21db9afe911491d57c143878a9f8c874e4c3b0759dc053834a43a2b6`,
versus F `44660c2de9532af3392c3c5fefea363b1915abd03bc9b79f4ba39812425c05f2`.
Local phase/native records are in `/private/tmp/whitefoot-vector-behavior-inlinehint`;
`native-audit.md` SHA-256 is
`0c00eca0ed00549328b82c5e1cf4dc11c3368aadd65c451823f272cc5793815f`.
The positive bounded result supports further implementation work, not a selected
generic hint policy, forward-consumption rule, or completion of the family goal.

### Consumer-counter diagnostic: the wide suffix-one gap is real but incomplete

This frozen-LLVM diagnostic starts from the actual forward-hint image at
`9efd6c624d5d947f68e676a9181e58b3168d9a69`. It replaces only the complete
wide `grow_vector_truncate` body with a counter-driven form taken from the
retained K body. The callback, native inputs, source ABI, allocation policy,
and every other body remain fixed. The control object and image are byte
identical to the pinned actual image. The independent native audit found one
and only one instruction-count change: the target function falls from 65 to
63 AArch64 instructions; no other function changes instruction count. The
candidate's text section is eight bytes shorter, with the remaining address
differences explained by that shift.

Both arms pass the maintained 1,260-configuration/8,820-execution check in
each timed and accounting image. All 294 accounting rows and every checksum
are identical. The complete timing pair used the existing `measure 1048576 7`
harness, both cohorts, seven implementations, both payload widths, three
populations and all seven paths: 4,116 rows per arm. The control and candidate
child runs took 82.23 s and 82.29 s; the guard reported successful exits. Raw
samples are preserved in
[`ecosystem-consumer-counter-control-samples.csv`](ecosystem-consumer-counter-control-samples.csv)
and [`ecosystem-consumer-counter-samples.csv`](ecosystem-consumer-counter-samples.csv),
with SHA-256 `cd6229878ef46d5c6a576826fb6f4fc4e03e3d98adb3ad6d4f43beb3ffd1f934`
and `c2a63b243b2e6029e3ac4fc9cf93ee4ccd31540c8a10e3d4f24311c5a5f3f`.

The changed body is causal for the earlier wide suffix-one anomaly: its
candidate/control median is `0.531–0.562` across cohorts and populations.
Against the contemporaneous slower Rust/C++ median, however, the candidate
still measures `1.143–1.202` in those cells. The other useful cells move only
within ordinary measurement variation (candidate/control medians mostly
`0.989–1.052`), and the candidate does not turn the remaining scalar
suffix-two deficits into wins. It therefore remains a rejected diagnostic:
the wide loop controller and digest work were a real part of the gap, but the
remaining take/length-publication/call-boundary cost still prevents the Vector
target. The exact source-only body patch is retained in
[`consumer-counter.patch`](consumer-counter.patch); no production lowering or
specification rule is selected.

### Wide tail-boundary `inlinehint`: completed and rejected

The preregistered discriminator added one ordinary `inlinehint` to the wide
`wf_vector_library_tail_work$instance$c3abe4db44181f7a` definition in both the
timed and accounting LLVM images. Source, ABI, callback, allocation policy,
native inputs and every scalar body stayed frozen. The exact two-file patch is
[`tail-boundary-inlinehint.patch`](tail-boundary-inlinehint.patch).

The code criterion failed before timing: optimized wide assembly still has the
same trace-to-tail call and the same separate tail body. The control and
candidate timed objects are byte-identical
(`7e9667d18069e0e196b98d2cce6f070f1e5b11a3d8ce6f69cfb3a3175e7b9278`); the
accounting objects are also byte-identical
(`89a078ac2aad2d324d540286b87c1501329f2dad914a7db1fb14a7fa4377831b`). The
linked timed images are byte-identical
(`7bc4c7fff9c9af06e281dcc65d8cb3842cd96bf0770619e5b3c4e6c5ab65f5d2`), as are
the accounting images (`26f22ccd09b5eb047fb301e6e61de82700227f8665bead7ad1512bea00ae22ca`).
The disassemblies differ only in their path header: the wide trace still calls
the tail at `0x10000c0b8`, whose separate definition remains at `0x10000c7dc`.
Thus no new frame, payload snapshot,
spill/reload or per-owner transfer was introduced, but there is also no
exposure of the boundary to measure. The candidate is rejected without a
4,116-row timing pair; the call/placement package remains unresolved and this
LLVM hint does not authorize a production policy.

### Counted consumer with wide tail-only `alwaysinline`: completed, near-parity but rejected

The preregistered candidate started from the measured counted-consumer body and
added `alwaysinline` only to the wide
`wf_vector_library_tail_work$instance$c3abe4db44181f7a` definition. The exact
timed/accounting LLVM patch is [`tail-alwaysinline.patch`](tail-alwaysinline.patch);
the two complete raw arms are
[`control samples`](ecosystem-tail-alwaysinline-control-samples.csv) and
[`candidate samples`](ecosystem-tail-alwaysinline-candidate-samples.csv).
The wide trace-to-tail call disappeared, while the independent
`grow_vector_truncate` call and its final length store remained. The work frame
stayed at `0x250`; no new 256B owner snapshot, append/helper call or spill was
introduced. The scalar and native inputs were frozen.

Both arms passed the full correctness matrix (1,260 configurations and 8,820
executions). Their accounting output has 295 rows and is byte-identical,
SHA-256 `ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`.
The child runs took `80.08 s` for control and `80.00 s` for candidate. The
control/candidate timed samples have SHA-256
`fd868bfe9e489dd013c9d4151e933c2065d1dffeaa919646a1b456fe08217ce1` /
`7e8d233832d9101c5147e8470ac6e0670b9dae2fc615fd2d881260dafab758b1`.
The timed object/image hashes are respectively
`9a4188f0ef1e96d08e8c8a3cd9b697c46f786bd94e130ed08bdead998d43f926` /
`78886ba0fb6c6e8411a8e671cdebca8c6922fe676b7cf6a1aab892675a8670b9` and
`903f7015d47a9d657639202ec17155b0c24ef4acb0d2a150fec3091169a57e06` /
`0e717967b36160d0fb2ce61c4af06a03c4f06f1e6f1d0076777564bda466f939`.

The wide suffix-one cells moved from robust deficits at `1.143–1.151` times
the slower standard peer to near parity at `0.992–1.006`; all three are still
inconclusive because the observed ranges overlap. The complete target summary
moved from 18 passes, 7 deficits and 11 inconclusive cells to 17 passes, 6
deficits and 13 inconclusive cells. Thus the tail boundary is a confirmed
large contributor, but the candidate does not win the required peer cells.
Separately, the paired Whitefoot samples establish a regression in wide
suffix-2 at population 16. Using all seven ranked samples, control medians
`21.566 / 21.550 ms` become `22.318 / 22.140 ms`, candidate/control ratios
`1.034869702 / 1.027378190`. Control ranges are
`21.551–21.943 / 21.538–22.043 ms`; candidate ranges are
`22.078–23.144 / 22.109–22.158 ms`. Both candidate minima exceed their
control maxima. The contemporaneous Rust median ratios are
`1.001112347 / 0.999444470`, and C++ ratios are
`1.000741737 / 0.996946141`. This paired loss is distinct from the target
counts, which compare each arm with its native peers. The candidate remains
rejected for selection; the remaining call/setup/placement and close-cell
work stays open.

### Counted consumer with wide tail and truncate `alwaysinline`: completed, improved but rejected

The preregistered candidate started from the counted-consumer plus wide
tail-only `alwaysinline` image and added `alwaysinline` only to the wide
`wf_std.collections.vector.grow_vector_truncate$instance$d6739d8f89f405bd`
definition. The exact patch is [`tail-truncate-inline.patch`](tail-truncate-inline.patch);
the paired raw samples are [`control`](ecosystem-tail-truncate-inline-control-samples.csv)
and [`candidate`](ecosystem-tail-truncate-inline-candidate-samples.csv). The
wide tail's truncate call disappeared and its final length store remained. The
wide work frame stayed at `0x250`, with no new owner snapshot or spill; the
wide drain call site was also exposed because it shares this monomorphized
truncate instance.

Both arms passed the full correctness matrix (1,260 configurations and 8,820
executions), and their 295-row accounting outputs were byte-identical with
SHA-256 `ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`.
The child runs took `80.04 s` for control and `79.65 s` for candidate. The
control/candidate samples have SHA-256
`08681c15b3cc9891380bd9587b901c23e32b10a69e06939131ca4ff5bc266123` /
`ff2430c3c92ab7ca3e870701610df617d4bc473f357544e6b53054ff7f821152`.
The timed object/image pairs are
`9a4188f0ef1e96d08e8c8a3cd9b697c46f786bd94e130ed08bdead998d43f926` /
`e1cf26b095999dc30fcde54d2357fcfdb0b317a1c97c27de4a250f3f10139143` and
`903f7015d47a9d657639202ec17155b0c24ef4acb0d2a150fec3091169a57e06` /
`3ca6538159bd32a15dd50ec6b7c1c352c5f342b66b1d4ad20826086a7644490f`.

Against its counted tail-only control, the target summary moved from 19
passes, 8 deficits and 9 inconclusive cells to 21 passes, 4 deficits and 11
inconclusive cells. Wide suffix-one medians are now `0.963–0.963` in the
first cohort and `0.989–1.012` in the second, with range overlap still leaving
all three cells inconclusive. The remaining four strict deficits are scalar
growth at population 16 and scalar suffix-two at populations 16, 256 and
4096. This is a substantial diagnostic improvement, but it does not satisfy
the per-cell target and is rejected as a production policy; scalar
reserved-append and suffix-two costs remain open.

### Next discriminator: scalar reserved append without the capacity branch

The remaining strict deficits are scalar growth at population 16 and scalar
suffix-two at 16, 256 and 4096. The next diagnostic is preregistered before
construction from the completed counted-consumer plus wide-tail-only image. In the scalar
`wf_vector_library_tail_work$instance$8f6b633c945d12a3` body only, replace the
inlined `grow_vector_append` call with its direct slot store and length
increment, removing that call's capacity/grow-full branch. The only caller is
`vector_library_trace`, which reserves `count + 1` before entering this tail;
the direct replacement is therefore a frozen benchmark witness for the
reserved-append cost, not a general unchecked API.

The code criterion is: the scalar tail contains no append/grow-full call or
capacity branch; scalar work/round and every wide body retain normalized code
identity; no new transfer, snapshot, spill or frame growth appears. A failure
stops without timing. If it passes, run both correctness/accounting images and
one complete 4,116-row pair. Accounting must remain byte-identical and every
checksum/cleanup fault must retain its verdict. Any scalar or wide useful-cell
regression rejects the diagnostic. This measures the cost of carrying a
proven spare-capacity fact across the helper boundary; it does not authorize
an unchecked public operation or a production policy by itself.

### Scalar reserved append: corrected tail+truncate pair

The candidate passed the pre-registered code criterion: only the scalar tail's
reserved append changed. Its capacity/grow-full call and branch became a direct
backing-slot store followed by a length increment; the scalar frame fell to
`0x0`. Wide bodies and their `0x250` frame, the account image and all other
normalized bodies retained their identities. The exact paired LLVM diff is
[`scalar-reserved-append.patch`](scalar-reserved-append.patch), with timed
samples in [`control`](ecosystem-scalar-reserved-append-control-samples.csv)
and [`candidate`](ecosystem-scalar-reserved-append-candidate-samples.csv).

Both images passed 1,260 configurations and 8,820 executions, and the
accounting CSV remained byte-identical to the retained ledger (SHA-256
`ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`). The
corrected guarded timed children took `79.90 s` and `79.79 s`; the complete
sample files have SHA-256
`187a7bd8809e75860df7fd4670d6075f1dfdba2cfb9683ff85f7493c91216878` and
`c6a7e2be1e85d2915b4a80f87f3a1701a6eca47cd918b67b329e3ae922cece2d`.

The corrected tail+truncate pair moved the target summary from 20 passes, 3
deficits and 13 inconclusive cells to 25 passes, 1 deficit and 10
inconclusive cells. Scalar suffix-two cells at 8-byte payloads moved from
`0.523–0.559` to `0.523–0.527` times the slower standard peer; other scalar
suffix cells also improved or stayed within overlap. The only strict deficit
left in this pair is the 8-byte growth-at-16 cell; the wide suffix-one cells
are no longer strict deficits under the current composite, although some
qualified ranges still overlap. The direct candidate remains a diagnostic
upper bound and is rejected as a production change because it removes a
public append check by hand.

For the remaining scalar growth-at-16 row, the same candidate's WF/direct-C
ratios are `1.018–1.024`, while WF/C++ is `1.085–1.107` and WF/Rust is
`1.152–1.154`. Direct C uses the matching header-first representation and is
therefore a closer source-shaped upper bound. This separates most of the
standard-library gap as the empty-header/first-growth representation and
algorithm contract; it does not make direct C a replacement for the Rust/C++
target, and the residual 2–3% against that control remains a compiler/lowering
question.

The earlier tail-only pair is retained as historical evidence under
[`tail-only control`](ecosystem-scalar-reserved-append-tail-only-control-samples.csv),
[`tail-only candidate`](ecosystem-scalar-reserved-append-tail-only-candidate-samples.csv),
and [`tail-only patch`](scalar-reserved-append-tail-only.patch). Its original
79.89/80.06-second timing and 17-to-19 pass-count change must not be compared
with the corrected composite totals.

### Withdrawn pointer/count trial: base identity was not current

Before the identity audit, a scalar pointer/end loop was built on the same
tail-only base and timed. It changed the three scalar suffix-two cells from
`0.523–0.529` to `0.623–0.628` and left the wide cells outside the intended
composite comparison. The trial is withdrawn rather than published as loop
evidence: its base was not the tail+truncate composite named by the preceding
plan. Its temporary samples remain only under `/private/tmp`; no loop
conclusion is drawn from them.

### Next discriminator: scalar truncate pointer/count loop (after the corrected append pair)

The corrected append witness leaves the growth-at-16 scalar deficit and does
not by itself isolate the consumer loop. Before construction, the next
diagnostic is therefore fixed to the scalar
`grow_vector_truncate$instance$0bdfc07e3035b4cc`
body in the corrected reserved-append composite. Keep its entry check exactly:
when the requested length is not below the current length, return without a
length store. In the admitted branch, replace only the indexed `index -> GEP
-> load -> index+1 -> compare` loop with a pointer/count loop equivalent to the
frozen K+hint body: compute the first element address once, decrement a finite
remaining count, advance the element pointer by one word, call the same
consumer, and store the requested length on the same exit. Do not change the
callback, source order, empty behavior, result code, or any wide body.

The code criterion is at most four loop-control/data instructions per scalar
element (pointer advance, load, callback argument and count/branch), no new
call, spill, snapshot or frame growth, and byte-identical wide/native bodies
apart from the intended scalar function. A mismatch stops without timing. If
it passes, run the full correctness/accounting images and one complete 4,116-
row pair. Accounting must remain byte-identical and every checksum/cleanup
fault must retain its verdict. Any useful-cell regression rejects the
diagnostic. The experiment measures the remaining scalar consumer-loop
component; it does not select a production loop form or imply that the
unmodified public truncate contract can omit its proof checks.

The corrected candidate compiled and linked with the frozen reserved-append
image, but its native scalar loop has five instructions per element after
inlining (`ldr`, digest update, pointer advance, decrement/flag update and
branch), not the preregistered maximum of four. The control has six. The
target function, entry guard, callback, empty behavior and terminal length
store otherwise match, and the only LLVM changes are the intended scalar
truncate loop. The code check therefore failed before timing, as required;
the scratch IR and guarded construction record remain under
`/private/tmp/whitefoot-scalar-pointer-count-current`. No pointer/count timing
or performance claim is made.

### Empty allocation: two-edge exposure does not remove the allocation

A separate F diagnostic changes only the scalar `work` instance
`8f6b633c945d12a3` and `grow_full` instance `8c4c85d67cb438a6`, adding either
ordinary `inlinehint` or diagnostic `alwaysinline` to their broad-linkage
definition headers. Its pre-construction criterion requires the executed
initial `calloc(1,16)` and corresponding free to disappear, not merely their
surrounding calls or the zero-length transfer. Native inputs and source are
unchanged. All three existing checks pass 1,260 configurations/8,820 executions
each; the fresh control object equals frozen F. Compile/link takes
0.253/0.226/0.243 s and checks 1.010/0.771/0.801 s; the guard takes 6.86 s.
WF text is 11,428/11,532/12,876 bytes for control/hint/mandatory.

The initial allocation/free survives both variants. Ordinary hints retain the
original boundaries. Mandatory exposure inlines work into round and grow_full
into make_room, but the enlarged make_room becomes an outlined call on every
append; the whole allocation lifetime is still not exposed. Its first growth
allocates 24 bytes and frees the original header; only the zero-byte transfer
disappears. This fails the allocation criterion and stops without timing. It
does not establish that a fully exposed lifetime cannot fold or select any
production inline policy. Exact paths and phase times remain under
`/private/tmp/whitefoot-vector-two-edge-f-e2125011`.

### Native empty-header A/B: the allocation pair explains most of the standard gap

The remaining strict Vector cell is scalar growth at eight-byte length 16.
Before changing the storage representation, a native control will keep the
existing `std::vector` growth and consumption algorithm but allocate and zero
a separate 16-byte empty header at construction, then release it after the
trace. The header is not connected to the vector and carries no payload; this
is a controlled source-equivalent cost injection, not a proposed library
implementation. The ordinary C++ source, seeds, driver ABI, work budget,
allocator-accounting image and operation order remain unchanged.

The code criterion was that the only optimized C++ difference is one matched
empty-header allocation and release per trace, with no changed vector
capacity, element movement, checksum or cleanup path. The accounting image
must show exactly one additional request and release per trace and the same
payload allocation sequence. A checksum, release-ledger or useful-cell
failure stops without timing. If the injected control closes most of the WF
versus C++ growth-at-16 gap while the other cells remain qualified, that is
evidence for the empty-state representation cost; if it does not, the
representation hypothesis is rejected as the main explanation and the next
comparison is the growth-helper ABI. This experiment cannot select a WF
representation by itself.

The preregistered control passed the criterion. Both timed and accounting
images passed the complete 1,260-configuration / 8,820-execution check, and
both checksum and cleanup fault injections still rejected. All 294 accounting
rows retain their checksum. For scalar growth-at-16, C++ changes from 18
requests / 18 releases / 1,512 requested bytes / 384 peak bytes to 21 / 21 /
1,560 / 400; every non-growth row is byte-identical. The timed guards took
`79.64 s` and `79.67 s`; raw samples and target summaries are [`baseline samples`](ecosystem-cpp-empty-header-baseline-samples.csv),
[`candidate samples`](ecosystem-cpp-empty-header-candidate-samples.csv),
[`baseline targets`](ecosystem-cpp-empty-header-baseline-targets.csv) and
[`candidate targets`](ecosystem-cpp-empty-header-candidate-targets.csv), with
accounting in [`A`](ecosystem-cpp-empty-header-a-account.csv) and
[`B`](ecosystem-cpp-empty-header-b-account.csv). The source criterion and
exact patch are [`criterion`](empty-header-ab-criterion.md) and
[`patch`](empty-header-ab.patch).

The baseline composite has 23 passes, 2 deficits and 11 inconclusive cells
(6 unranked); the injected-header composite has 24 passes, no deficits and
12 inconclusive cells (6 unranked). In scalar growth-at-16, the C++ median
increases by about `17.7–19.0 ns` per round, changing WF from `1.086–1.088`
times C++ to an overlapping `0.967–0.971`. In wide growth-at-16, it increases
by `28.2–30.9 ns` per round and changes the WF ratio from `1.026–1.029` to
`0.976–0.979`. This is causal evidence that the empty-header/first-growth
allocation contract accounts for most of the remaining standard-library gap.
It is not an exact per-instruction time split: the diagnostic header remains
live until the end of `work`, whereas WF releases its zero-capacity header at
the first growth. The result selects a lazy-empty representation as the next
production candidate to measure, but does not silently select its public API
or storage type.

### Next discriminator: merge the first growth into construction

The native A/B identifies the empty-header/first-growth pair, while a lazy
enum would change the public storage shape and the source contracts. The next
initial-capacity policy diagnostic keeps the `Box<Slots<T>>` storage shape
and public ABI, but changes `grow_vector_new`: for a positive caller ceiling
it constructs capacity one; the `ceiling == 0` instance remains capacity
zero. This is observable: `grow_vector_reserve` with `total: 0_u64` on a fresh
positive-ceiling vector returns one instead of zero. Call sites, the growth
formula and element order stay unchanged. The diagnostic eliminates the
first `grow` edge without adding a new representation.

The pre-construction criterion is that the diff contains only this conditional
construction and its documentation; the zero-ceiling program remains
unchanged. The complete correctness/accounting images must retain every
checksum and owner/release ledger, with exactly one fewer allocation
and release at each positive-ceiling fresh growth trace and no change to
reserved/reuse rows except their initial capacity. A useful-cell regression or
an unexpected field/contract verdict stops without timing. If the candidate
removes the strict growth deficit without regressing another qualified cell,
the changed initial-capacity policy is a production candidate; otherwise it is
rejected and the remaining choice is a lazy-empty representation or a
compiler/lowering optimization.

The historical criterion called the cleanup fault a "cleanup refusal".
The driver injects a missing release, not allocator exhaustion; its negative
controls establish checksum and release accounting coverage only.

The matched A/B was run with separately rebuilt gate compilers, because the
standard library is embedded in `whitefootc`; the earlier stale-binary timing
was discarded. Both arms passed the complete 1,260-configuration /
8,820-execution check, the candidate's 294 accounting rows, checksum and
cleanup fault injections. The candidate changed scalar growth at 16 from
`1.101858/1.097282` times the slower standard peer (cohorts 0/1) to
`0.952456/0.959992`, removing that strict deficit. Its fresh target reduction
was nevertheless 18 passes / 4 deficits / 14 inconclusive cells (six
unranked), versus the baseline's 18 / 5 / 13 (six unranked): scalar suffix-2
at 4096 moved from inconclusive (`1.034401/1.062385`) to a strict deficit
(`1.065974/1.065284`). That peer-status change does not establish a paired
Whitefoot regression. With all seven recorded samples ranked, its control
medians are `1.744 / 1.737 ms` and candidate medians are `1.745 / 1.746 ms`,
ratios `1.000573394 / 1.005181347`; ranges overlap in both cohorts.

A different cell does fail the preregistered no-regression criterion: wide
reserved at population 16. Control medians `42.075 / 42.102 ms` become
`43.426 / 43.464 ms`, candidate/control ratios `1.032109329 / 1.032350007`.
Control ranges are `42.028–42.226 / 42.061–43.270 ms`, while candidate ranges
are `43.393–44.013 / 43.382–43.621 ms`; both candidate minima exceed their
control maxima. The contemporaneous Rust median ratios are
`1.001203283 / 1.007866060`, and C++ ratios are
`0.997776580 / 0.998899396`. This cell remains a peer pass in both saved
target CSVs: its Whitefoot/slower-standard median ratios move from
`0.944953510 / 0.945666090` to `0.977468656 / 0.977334053`. It can therefore
still beat the native target while regressing against its paired Whitefoot
control. The candidate remains rejected as a production policy on that
paired-loss evidence.

Its ledger does confirm the intended causal change: scalar growth at 16 falls
from 21 to 18 requests and from 1,848 to 1,800 requested bytes per three-round trace,
with the same 416-byte peak; non-growth traces retain two requests but carry
one extra element slot in the initial header (scalar reserved-16: 504 to 528
bytes, peak 168 to 176). The frozen patch and complete raw evidence are
[`patch`](initial-capacity-ab.patch), [`baseline samples`](ecosystem-initial-cap-baseline-samples.csv),
[`candidate samples`](ecosystem-initial-cap-candidate-samples.csv),
[`baseline targets`](ecosystem-initial-cap-baseline-targets.csv),
[`candidate targets`](ecosystem-initial-cap-candidate-targets.csv),
[`baseline account`](ecosystem-initial-cap-baseline-account.csv) and
[`candidate account`](ecosystem-initial-cap-candidate-account.csv).

The initial-capacity policy trial does not make a lazy-empty
representation expressible under the current public shape. A minimal enum
`Pending<T> { Empty; Full(storage: Box<Slots<T>>); }` can run a 16-append
micro-witness, but the existing vector contracts reject the required facts:
`requires deref(values).storage.Full.storage.inner.len < 16_u64;` is
`TYPE-5`; a helper `requires lazy_len(values: values) < 16_u64;` is rejected
by `FN-8` (`InvalidRequires`); an inline scalar postcondition such as
`ensures deref(values).capacity > deref(values).length;` is rejected by
`FN-9` (`InvalidPostconditionRelation`); and the corresponding loop fact
`invariant values.length >= index` is rejected by `INV-1`
(`InvalidInvariant`). The micro-witness uses a 32-byte enum value versus the
old 8-byte vector descriptor and still needs runtime checks, so it is not a
library candidate. No Vector production source changed; the remaining choice
is an explicit compiler/lowering optimization or a separately recorded
representation-and-contract design decision.

### Fresh main integration: identical executable inputs, no retiming

At `f945eceecb3aacac20e76864c73edf8b7c87902b`, a fresh gate compiler
(`e77f0a97b85cf795aa3fe7e0afca88c00a6ea8307fa38fdf3bef368a9e27e4e4`)
built in 56.551 s. The complete guarded validation included that build, all five
families' build/check/account commands and five maintained owning fixtures:
123.798 s execution plus 12.166 s queued, every command exit 0.
Fresh `.build/main-6bb-f` raw LLVM, WF objects,
driver/C++/Rust inputs and 12 runtime objects per family match the appropriate
previous builds: Vector F, Map `.build/geometry`, and the other baselines.
Accounting remains exactly 294/120/420/150/210 rows for
Vector/Deque/Map/Priority/Ordered; Map's comparison uses its published accounting.
Vector's linked file differs, but all 15 sections' contents, addresses and layout
are identical. Differences are debug-object paths, related linkedit/string data,
UUID and code signature. The records and section comparison are under
`/private/tmp/whitefoot-main-6bb-f-validation`; these are construction,
correctness and identity observations. No performance timing was repeated.

### Actual compiler: forward consumption with ordinary function-actual hints

The actual compiler at `9efd6c624d5d947f68e676a9181e58b3168d9a69` passes
the prospective Vector native criterion. This is newly emitted code from the
unchanged source, compared with `.build/main-6bb-f`; it is not the raw K+hint
artifact or its measured result. The [five-family construction record](../ECOSYSTEM.md#actual-compiler-construction-and-native-admission)
owns shared identities, costs and reproduction. The fresh paired result below
is adverse; the earlier 13-gain/zero-loss raw K+hint result does not validate
this implementation's performance.

Exactly the scalar and wide `grow_vector_truncate` raw bodies change. The
checked terminal region becomes an ascending logical-index traversal with an
ordinary consumer call; retained length is published only after nonempty
completion. Other Vector bodies retain their lowering. Exactly four physical
make/accept definitions receive the ordinary hints from their checked supplied
bindings; all 57 exported object symbols match the control. There is no source,
callback ABI, allocation policy or specification change. The raw wide loop does
materialize a 256-byte owned local before acceptance; native inspection must,
and does, establish elimination of that copy.

No scalar/record accept call or branch survives in the linked image. The wide
batch helper at `0x10000d3c8` is frameless and call-free: its positive loop reads
backing directly in forward order, with 16 paired loads, 32 digest multiply-adds
and four cursor/count/branch instructions per owner. It writes no payload and
creates no replacement owner snapshot. Suffix, mixed cleanup and final prefix
drain each reach this consumer; suffix lengths 1/2/3 each retain one batch call.
Scalar consumers inline into the corresponding paths without owner relocation.
The pre-existing wide swap-remove temporary remains a separate cost.

The wide suffix-zero caller still invokes the helper. Its empty branch bypasses
payload reads, digest access and the same-value length store; scalar standalone
and inlined consumers retain the corresponding guard. Removing those stores
does not establish removal of all suffix-zero overhead or an isolated time saving.

| Native observation | Frozen F | Raw K+hint | Actual compiler |
| --- | ---: | ---: | ---: |
| Scalar trace instructions / frame bytes | 246 / 160 | 236 / 176 | 229 / 160 |
| Scalar positive consumer instructions per owner | rear/exchange walk | 4 | 6 |
| Wide truncate instructions | 168 | 63 | 65 |
| Wide positive consumer instructions per owner | take/exchange/remainder | 51 | 52 |
| WF object / linked text bytes | 11,428 / 691,132 | 10,432 / 690,136 | 10,432 / 690,136 |

The actual scalar loop uses indexed address formation/load/multiply-add/index
advance/compare/branch instead of raw K's pointer/count recurrence. Wide trace,
tail and mixed work normalize identically to K+hint, with frames 352/288/688 B;
the previously disclosed environment-pointer spill/register rearrangement versus
F remains. Growth calls stay on full-capacity edges, and tail construction writes
directly to backing. Equal text sizes do not make the two implementations equal.

Native driver/C++/Rust inputs are byte-identical to F. Their four standard trace
entries move −996 bytes versus F; 43 wide constant references relocate while
all loaded 16-byte values remain equal. Addresses/instructions match K+hint's
standard traces. The complete images are different, so placement effects remain
inside the whole-trace comparison below.

Actual timed LLVM, WF object and linked-image SHA-256 values are respectively
`49bc8ef4d39df3f24ce2314e237537c74675a1946a942166863cdba9c1b0a764`,
`7e9667d18069e0e196b98d2cce6f070f1e5b11a3d8ce6f69cfb3a3175e7b9278` and
`7bc4c7fff9c9af06e281dcc65d8cb3842cd96bf0770619e5b3c4e6c5ab65f5d2`.
The 294-row allocation ledger is byte-identical to F. The local read-only audit,
raw changed-body inventory, native dumps and constant qualification are under
`/private/tmp/whitefoot-actual-compiler-validation/construction-1/native/`;
`vector-audit.md` SHA-256 is
`c6a39fbfaa602f47b854a6ca8b1d0ee614868ac259c8d8cf3648e9a5ff486493`.

### Actual compiler paired timing: scalar gains with adverse wide results

The fresh actual-compiler pair does not establish the required useful-cell
no-regression result. All six strict gains are scalar suffix-two/three, but wide
suffix-one at 16 has a raw strict loss: candidate medians are 35.789/32.046 ms
versus F's 22.989/22.653 ms, ratios **1.556788/1.414647** in cohorts 0/1.
Even its candidate minima exceed F's maxima by 1.290859/1.046363. Its ratio
spread is **10.0478%**, just above the unchanged 10% stability limit, so it is
unqualified for a ranked verdict. That flag does not turn the large observed
regression into evidence of no regression. Wide suffix-one at 256 and 4096
also regresses in both cohort medians: 1.107953/1.446962 and
1.399245/1.578892, with spreads 30.5978% and 12.8389%; their ranges overlap.
All samples remain. No outlier removal, selective replay or revised cutoff follows.

Across 36 useful cells, raw range separation gives **6 gains, 1 loss and
29 overlaps**. Applying the recorded duration/stability qualifications gives
**6 gains, 0 qualified losses, 27 overlaps and 3 unstable cells**, all three
wide suffix-one. The minimum useful paired sample is 1.234 ms. Standard targets
move from fresh F's **13 pass/9 deficit/14 inconclusive** to the actual compiler's
**16/2/18**; its two qualified deficits are scalar growth at 16 and scalar
suffix-two at 256. The three wide suffix-one targets are inconclusive due to
instability, despite the adverse observations above. Counts do not select the
candidate or complete the family goal.

The table covers all 36 useful cells and six unranked controls. Ratios list
candidate/F cohort medians in order 0/1. Raw G/L/O require both complete sample
ranges to separate as gain/loss, or otherwise overlap. Qualified X is unstable,
S is shorter than 1 ms; target P/D/I/U means pass/deficit/inconclusive/unranked,
shown F→actual. These are observed ranges, not confidence intervals.

| Bytes | Path | n | Actual/F, 0 / 1 | Raw / qualified | Target F→actual |
| ---: | --- | ---: | ---: | :---: | :---: |
| 8 | reserved | 16 | 0.886 / 0.866 | O / O | I→I |
| 8 | reserved | 256 | 1.011 / 0.998 | O / O | P→P |
| 8 | reserved | 4096 | 1.019 / 0.978 | O / O | P→I |
| 8 | growth | 16 | 1.036 / 1.010 | O / O | D→D |
| 8 | growth | 256 | 1.004 / 1.003 | O / O | I→P |
| 8 | growth | 4096 | 0.986 / 0.955 | O / O | P→P |
| 8 | reuse | 16 | 0.808 / 0.844 | O / O | P→P |
| 8 | reuse | 256 | 0.981 / 0.969 | O / O | P→P |
| 8 | reuse | 4096 | 0.996 / 0.993 | O / O | P→P |
| 8 | suffix-1 | 16 | 0.957 / 0.979 | O / O | I→P |
| 8 | suffix-1 | 256 | 0.952 / 0.952 | O / O | I→I |
| 8 | suffix-1 | 4096 | 0.974 / 0.932 | O / O | I→I |
| 8 | suffix-2 | 16 | 0.757 / 0.744 | G / G | D→I |
| 8 | suffix-2 | 256 | 0.713 / 0.713 | G / G | D→D |
| 8 | suffix-2 | 4096 | 0.753 / 0.719 | G / G | D→I |
| 8 | suffix-3 | 16 | 0.713 / 0.717 | G / G | I→P |
| 8 | suffix-3 | 256 | 0.705 / 0.699 | G / G | D→P |
| 8 | suffix-3 | 4096 | 0.679 / 0.703 | G / G | I→P |
| 8 | suffix-0 | 16 | 1.039 / 0.968 | O / S | U |
| 8 | suffix-0 | 256 | 1.013 / 0.996 | O / S | U |
| 8 | suffix-0 | 4096 | 1.008 / 0.967 | O / S | U |
| 256 | reserved | 16 | 0.985 / 0.990 | O / O | P→I |
| 256 | reserved | 256 | 1.017 / 1.006 | O / O | P→P |
| 256 | reserved | 4096 | 0.999 / 1.008 | O / O | P→P |
| 256 | growth | 16 | 0.997 / 1.010 | O / O | I→I |
| 256 | growth | 256 | 0.989 / 1.000 | O / O | P→P |
| 256 | growth | 4096 | 0.980 / 0.992 | O / O | I→I |
| 256 | reuse | 16 | 0.976 / 0.990 | O / O | P→P |
| 256 | reuse | 256 | 0.984 / 1.005 | O / O | P→P |
| 256 | reuse | 4096 | 0.997 / 1.005 | O / O | P→P |
| 256 | suffix-1 | 16 | 1.557 / 1.415 | L / X | D→I |
| 256 | suffix-1 | 256 | 1.108 / 1.447 | O / X | D→I |
| 256 | suffix-1 | 4096 | 1.399 / 1.579 | O / X | D→I |
| 256 | suffix-2 | 16 | 0.943 / 0.954 | O / O | I→I |
| 256 | suffix-2 | 256 | 0.918 / 0.937 | O / O | D→I |
| 256 | suffix-2 | 4096 | 0.929 / 0.930 | O / O | I→I |
| 256 | suffix-3 | 16 | 0.958 / 0.985 | O / O | I→I |
| 256 | suffix-3 | 256 | 0.965 / 0.971 | O / O | I→I |
| 256 | suffix-3 | 4096 | 0.977 / 0.966 | O / O | I→I |
| 256 | suffix-0 | 16 | 1.162 / 1.178 | L / L | U |
| 256 | suffix-0 | 256 | 1.162 / 1.201 | L / L | U |
| 256 | suffix-0 | 4096 | 1.146 / 1.141 | L / L | U |

All three wide suffix-zero controls strictly regress, with median slowdowns
14.1–20.1%; removing the empty-path length store did not remove their measured
overhead. All three scalar suffix-zero controls overlap and each has a paired sample below 1 ms.
These six rows stay unranked but visible; they are not dropped from the outcome.

Useful native-control median drift spans Rust 0.942494–1.036989× and C++
0.927650–1.037607×. At wide suffix-one/16, Rust ratios are 0.971984/1.008166
and C++ 0.961146/1.037607; a uniform native slowdown does not explain the
candidate's 1.556788/1.414647 ratios. At 256/4096 the corresponding standard
ratios stay within 0.975032–0.999762×. Direct C is noisier: useful drift spans
0.896501–1.296531×, and within-arm scalar suffix-two comparisons are unstable
at F/256 (20.3089%) and candidate/4096 (48.3436%). Every candidate wide
suffix-one comparison against all six native variants is unstable. These flags
remain qualifications, not averaging corrections or isolated component costs.

The fresh [F control](ecosystem-actual-compiler-control-samples.csv) and
[actual compiler](ecosystem-actual-compiler-candidate-samples.csv) each retain
4,116 rows with identical non-time fields/checksums and sample IDs 0–6.
SHA-256 values are respectively
`a11aa33b754b3e410c9f004c39c702f624f3e526fcb86844b720f80bd6206025` and
`7d5eef6091d3be53ae683b3bd82f7d7e30b78d9bef8b85bc70f7c9d32bb71a08`.
The frozen F image ran first, then the frozen `9efd6c624` candidate, each with
`measure 1048576 7`: 81.360/81.040 s, both exit 0. The wrapper ledger records
164.167 s in one attempt; maintained reductions cost 0.835 s separately within
that guard. All seven phase commands and the later paired reduction exit 0.
The 1,361 frozen source/input/artifact hashes matched before and after the pair;
`freeze.json` SHA-256 is
`cfc9e1a580c8e80bd0b57a58c642c6b536d6463b81f1f0ea030f689f6bd8af09`.
These are newly paired controls, not pooled historical F or raw K measurements.

The [prospective scope revision](../ECOSYSTEM.md#actual-compiler-construction-and-native-admission)
preceded every timing/guard attempt and retained the complete Vector matrix;
the four unchanged families were not remeasured. Source, flags, allocation,
cohort order and the maintained reducer are unchanged. Reproduce reductions
from this directory, separately for `control` and `candidate`:

```sh
set -eu
for arm in control candidate; do
  perl ../summarize-ecosystem.pl --complete \
    "vector=ecosystem-actual-compiler-$arm-samples.csv" > "/tmp/actual-$arm-summary.csv"
  perl ../summarize-ecosystem.pl --targets \
    "vector=ecosystem-actual-compiler-$arm-samples.csv" > "/tmp/actual-$arm-targets.csv"
done
```

Use the preceding pinned construction recipe and frozen images for any new run;
no extra optimizer pass or benchmark variant is implied. Local `measurement-1/`
under `/private/tmp/whitefoot-actual-compiler-validation` retains phase/status,
posthash, full paired-cell and native-drift reductions. This combined experiment
establishes no separate latency share for hints, traversal or the empty-edge
change, and its adverse result does not select a production performance policy.

### Counted consumer with the actual empty guard: completed and rejected

The prospective experiment above was completed using the frozen `9efd6c624`
inputs. Its rebuilt control was byte-identical, the candidate retained the
distinct counted loop and actual empty bypass, and the native/correctness
criteria passed. The complete pair and its qualified result are recorded in
the [consumer-counter diagnostic](#consumer-counter-diagnostic-the-wide-suffix-one-gap-is-real-but-incomplete).
That result implicates the loop-controller/placement package but still leaves
the wide suffix-one target at `1.143–1.202` times the slower standard peer, so
it does not select a production lowering or close the Vector phase.

## Historical source-composition evidence

The later [same-source inactive-storage compiler comparison](../map-library/RESULTS.md#completed-comparison-gains-with-unresolved-regressions)
passes this experiment's complete correctness matrix and finds unchanged
native bodies in both modes, so it adds no Vector timing samples. Its v0.68
fixture migration removes only the former `own` signature annotation; the
historical measurements below retain their original conditions. The owner
rejected that compiler optimization on the Map and Slab evidence; Vector's
unchanged bodies do not establish a benefit or override those regressions.

This experiment bundles the current reusable
[`GrowVector`](../../../../lib/std/collections/vector/grow-vector.wf), not a second
benchmark-only implementation. The selection criteria precede measurement in
[X1-LIBRARY.md](../../../investigations/containers-and-resources/X1-LIBRARY.md#vector-consumption-trial).
The paired measurements use kernel v0.62's global heap and total allocation.
The later v0.63 clarification of Box descendant measure placement changes
neither the measured library source nor its lowering. The v0.61 diagnostic
trial and dated v0.60 measurements below retain their original compiler and
source identities. Measurements are descriptive evidence, outside correctness
CI, not a native-parity gate.

## Contract and controls

One work round fills an empty vector, inserts and removes a middle marker,
swap-removes the first element when present, consumes the suffix after the
midpoint, then drains the retained prefix. Both consuming operations call the
member in original element order. Every element contributes to an
order-sensitive checksum. The independent C oracle computes that logical
sequence without constructing or mutating a vector.

The three whole-chain paths are reserve-before-fill, growth by append, and reuse of one
reserved allocation across rounds. All have a final empty-owner release.
Lengths 16, 256 and 4096 are timed with `16384 / length` rounds per sample.
Elements are an 8-byte scalar or a 256-byte `nocopy` record of 32 words; each
record word contributes to the checksum. These are operation trials, not a
claim about real application frequencies or a measurement of Box-payload
allocation costs. Two additional paths retain all but one or three elements
at lengths 16 and 4096. They build the retained prefix once, then append and
consume the small suffix repeatedly, and finally drain the prefix and release
the backing. Each sample uses `65536 / removed` cycles. Its per-cycle time
includes the amortized prefix setup and final drain; it is not an isolated
truncate measurement. The original and revised libraries use the same
extended workload and controls.

- **Whitefoot:** the actual library's take-first, swap-local composition. The
  paired original-source run substitutes the saved reverse-suffix library
  while retaining the same compiler, workload and controls.
- **Reverse C:** the original library's algorithm, growth policy, element
  ownership transfer and callback sequence. Its difference from the
  original-source WF run measures compiler/lowering costs under that source
  shape, including ordinary native ABI differences.
- **Direct C:** the same contract with a direct ordered suffix consumer. The
  callback cannot observe the vector, so the control can traverse the suffix
  and shorten the length once. Its difference from reverse C measures the
  composition's cost, separately from WF lowering.
- **Swap/take C:** swaps the next suffix element with the last one, then
  immediately takes and consumes it; the second half is consumed from back.
- **Take/swap C:** takes the last element into a local, exchanges that local
  with the next suffix element, then consumes the local; the second half is
  consumed from back. The distinct statement order permits fewer optimized
  transfers without changing the ownership or callback contract.

Each implementation has one pointer owner and a header-first allocation:
16 bytes for length/capacity, then `capacity * sizeof(element)` bytes.
Growth allocates, copies the live run, and releases the previous backing;
there is no realloc-policy difference. Allocation counts, requested bytes,
peak live bytes and final zero live bytes must match before any timing is
accepted. The common accounting wrapper adds the same bookkeeping to all
five implementations; reported sizes exclude its private header. The two
interleaved controls were added for the v0.61 follow-up and do not occur in
the dated v0.60 timing datasets.

The ordinary mode permits normal Clang O2 inlining. The retained mode marks
library and workload helpers and the element make/accept functions noinline
on both sides. Compiler-owned primitive
operations remain eligible for inlining, just as C primitive operations do.
Generated optimized WF and C IR remain in `.build/` for inspection.

## Correctness and timing method

`make check` in this directory checks 1260 configurations per helper mode:
10 lengths (including 0, 1, 8192), three round counts (including 0), three
seeds (including u64 max), two element sizes, three whole-chain allocation
paths and four suffix paths removing zero through three elements (clamped to
the length). All five implementations run each configuration: 12,600 executions across
the two modes. This is an experiment check, not a daily gate dependency.

The formal corpus separately bundles the library and
[`grow-vector-program.wf`](../../../../tests/programs/containers/grow-vector-program.wf).
It executes sequential and parallel lowering, normally and with an allocator
observer that records every identity and detects stale/double release. Copy,
affine Box and nodrop owning chains check retained prefix, callback order,
empty/singleton cases, same-index swap-remove, reuse and 25 exact-once releases.
These regressions, rather than the research harness, run in canonical
`make check`. The observer synchronizes its ledger for parallel allocation
and release. The same parallel native image also runs a four-worker,
32-allocation cross-release control and rejects double, foreign and missing
release controls; these four executions add no WF compilation or native build.

Measurements run on Apple M1 Pro (8 logical CPUs), arm64 macOS 26.6.2,
Apple Clang 21.0.0 (`clang-2100.3.34.2`). Each executable has two cohorts of
11 samples; the second cohort reverses implementation order, and samples
rotate the first implementation. Each timing
still checks the oracle and complete release. Raw times cover the whole
operation chain or the stated suffix cycle, including amortized setup and
cleanup, not just drain. The 32-word checksum is material work in
large-record samples, so these timings do not isolate pure memory bandwidth.
Reported timings are medians in nanoseconds per round or suffix cycle;
ratios are ratios of those medians. The paired source experiment below uses
separate executables: implementations are interleaved within each executable,
not across the original and current WF sources. No cross-machine or
application-wide speed claim follows.

## v0.61 copy and consumption trial

The 2026-09-22 UTC follow-up uses a freshly built baseline at
`efe41016d10379325ed4513d0ac7457ec7f24c5b`, whose active specification SHA-256 is
`f61a42e815e23d6bd1c837790081800ef2cfe20a9e728d87db781be92f9181d9`.
The baseline compiler was built in a detached checkout, then the two-file
backend copy patch alone was applied there and rebuilt. This separates the
copy repair from simultaneous checker work in the integration branch. Both
builds used the gate profile under the shared verification guard.

| Isolated input | SHA-256 |
| --- | --- |
| Baseline compiler executable | `a63ad5603dcd9bfc580c203d7ebd5a73d39be6d706b2f686ab6ada46db1d6212` |
| Swap-only compiler executable | `7e6fa30e24b2633ac96b5a2d7ed13c6e47e124a075dc6e73e7f010e27ff12993` |
| Swap-only backend patch | `ef426a19e60f2e0ecc5b42900d813fe4776f0731d02959d814494c5f8818533c` |
| Original library | `c8ce9e0cdd850deebcb5c4d0cc91d5bc183631c19514c274834d59ff6e139eb8` |
| Take-first library candidate | `e13c9937621abc2179d2d939cdb23b4a403d312d0245a5c369858aa3aeb6acad` |
| Shared WF workload | `1b11932fe84f30c5d37d885ef5331d1f5b8944f586c228bf0720ff2049985e33` |
| Five-way C harness | `e11377b7fc835203441337243af264d9fa0082fa21fbe65bbd92fe925a8d3976` |

The original library with the baseline compiler, original library with the
swap-only compiler, and take-first candidate with the swap-only compiler each
passed all 5,400 executions, including unchanged allocation counts, requested
bytes, peak live bytes and exact release. Compiler builds took 45.91 and
44.31 seconds; the corresponding experiment construction and execution checks
took 3.54, 3.30 and 2.93 seconds. These are verification costs, not workload
performance samples. The earlier executable already present in the shared
worktree was a historical v0.60 build and was not used as this baseline.

The copy patch uses ordinary `llvm.memcpy` only in the compiler-owned `swap`
body. OP-11 establishes that its two targets are equal or disjoint, excluding
proper ancestry. Ordinary [LLVM memcpy](https://llvm.org/docs/LangRef.html#llvm-memcpy-intrinsic)
admits equality as well as disjointness; `memcpy.inline` has a different
requirement. Private snapshots are disjoint
from both exchange targets. No parameter receives a new `noalias` promise,
and other storage copies retain `memmove` because their general path can
include partially overlapping input/result storage.

The take-first candidate consumes `floor(removed / 2)` elements by taking the
last element, swapping that local with the next suffix position, and passing
the local to the callback. The reversed remainder is consumed from back.
At first-half offset `k`, `k < floor(removed / 2)` proves that the post-take
length still exceeds `retained + k`. A local PRF-1 certificate publishes this
bound without a runtime branch. The retained prefix and backing are unchanged,
and every callback observes the original suffix order.

The criterion before timing is fewer actual optimized record transfers. For
the retained 256-byte truncate, the original WF body has three transfers per
reverse pair and one per consumed record. The copy patch changes the pair's
`2 memcpy + 1 memmove` to `3 memcpy`, without changing that count. The
take-first WF candidate still has four transfers per first-half iteration
and one per remaining record. Matched take-first C has two and one; matched
swap-first C still has four and one. No timing was used to select the
unimproved WF source candidate.

Raw IR reveals additional immutable local snapshots. A bounded, IR-only
diagnosis left source, callbacks, control flow and noalias facts unchanged:

| Scratch IR change | First-half 256-byte transfers | Remainder transfers |
| --- | ---: | ---: |
| Unchanged take-first candidate | 4 | 1 |
| Early `captures(none)` on aggregate ABI inputs/results | 4 | 1 |
| Four separate local allocations instead of one 1,024-byte frame | 4 | 1 |
| Both changes | 4 | 1 |
| Non-underflow flag on take's length decrement, combined frame | 4 | 1 |
| Non-underflow flag on take's length decrement, separate locals | 4 | 1 |
| Take captures address, shortens descriptor, then transfers; combined frame | 4 | 1 |
| That take ordering with separate locals | 2 | 1 |

Clang O2 completed all three diagnostic optimizations in one guarded 0.20-second
run. Separate allocations forwarded two caller snapshots but exposed the
swap temporary, leaving the transfer count unchanged. These negative results
did not select a broader ABI attribute or a frame representation change.
The four following optimizations took 0.31 seconds. The take-order experiment
captured the old element address before writing the shortened descriptor,
then transferred the element. That ordering forwarded the last element
directly to the suffix slot; separate allocations also removed the sibling
frame snapshot before the callback. Both changes were needed for two transfers
in this fixture. No `captures(none)` or arithmetic-flag change was selected.
The compiler-produced raw and optimized modules are in the experiment's
`.build/followups-*` directories. The diagnostic transformations were written
outside the repository; they are not an alternate compiler path or checked-in
implementation.

The bounded frame/probe diagnostic took 0.21 seconds. Both modules qualified
the same 1,024 bytes of four 256-byte, alignment-8 local allocations before
optimization. On arm64 the original function used a 1,104-byte machine frame
(1,024 local bytes plus 80 saved-register bytes); the combined positive
diagnostic used 576 bytes (512 local bytes plus 64 saved-register bytes).
Both retained the stack-probe attribute and neither needed a probe call at
these sizes. These emitted frame sizes describe the fixture, not a target
layout guarantee.

The delivered compiler generalizes only the proved parts of that diagnostic.
OP-10 take is lowered as one operation: capture the old physical address,
update the Slots/Ring descriptor once, then transfer the element. Descriptor
and element storage are disjoint; there is no intervening callback, drop or
allocation. Independent entry allocations are selected only after validating
the complete facts-off frame extent, and only when every complete allocation
root has positive size, the same natural and requested alignment, and no
padding. Zero-sized, mixed-alignment and over-aligned frames retain the
contiguous form. The target plan supplies the emission recipe; it does not
recast accounting offsets as addresses of separate objects. Runtime lane
frames, ownership interference and storage coalescing are unchanged.

The integrated v0.62 compiler with local Apple Clang 21 produced the same
two/one retained transfer shape and passed the original 5,400-execution
workload before the suffix extension. In that optimized body the new
composition transfers
`removed + floor(removed / 2)` records, versus the original
`removed + 3 * floor(removed / 2)`. Direct C
needs one callback-argument transfer per removed record. The extra half-record
per element is a remaining cost of this composition, not a demonstrated lower
bound for all ordinary representations or a reason to weaken the API.
These are toolchain-specific bulk-transfer counts, not a compiler promise.
The existing intrinsic-count proxy omits scalar loads and stores. The hosted
Apple Clang 15.0.0 (`clang-1500.3.9.4`, arm64 Darwin 23.6.0) optimized a related
large-record regression to three 256-byte transfers, retaining an additional
consuming-call snapshot. Its old exact-count assertion stopped that run before
the native checksum test. The maintained regression now checks the compiler's
allocation and take-order guarantees and native checksum rather than requiring
every supported optimizer to produce two transfers. General consumed-local
snapshot forwarding remains a separate compiler improvement in
[`docs/todo.md`](../../../../docs/todo.md).

Ordinary representation alternatives must preserve the complete API. An
`Option<T>` slot representation permits a forward drain but cannot implement
the unchanged `remove(index) -> T` contract without an occupancy invariant
that rules out `None`; an impossible fallback or an optional result would
weaken the contract. Rotating a Ring's retained prefix before draining costs
O(retained + removed), violating O(removed) truncation when almost all values
are retained. An atomic update that consumes the old slot in a callback and
returns a replacement would avoid the local swap, but OP-12 admits only copy
or affine targets; unconstrained T is linear, so that form cannot serve the
existing nodrop-generic API. None of these alternatives was silently substituted
for the selected operation contract.

## Paired v0.62 source measurements

The 2026-09-22 UTC run compares the original reverse-suffix library with the
current take-first library through the same saved compiler, built from compiler
source at `99dc453737ec459fcd08e5efc53ff5c43a59d178`. The two builds use the same
extended workload, runtime, C harness, target flags and host. This isolates the
library-source change with the final lowering repairs present on both sides;
it is not a before/after compiler timing experiment.

| Input | SHA-256 |
| --- | --- |
| Saved compiler executable | `d0d291ce2343a078a0bfdb212dcee52f4523888ebcb6373bcbfbcb417c0f634c` |
| Active v0.62 specification | `bb2697f9c5a99d59917cc0371a4bea3b50a3445f30b62e943b0fcdb046804db2` |
| Original library | `c8ce9e0cdd850deebcb5c4d0cc91d5bc183631c19514c274834d59ff6e139eb8` |
| Current library | `07a5730d1888bd010f11abf288ed94a0c4c4ac04da6a36426a5ddc75733e7e65` |
| Extended WF workload | `5a04a0e7dcb7b09644cc626d0af2b0407e1a49b8dc2cc25cff6384fb5dd8c37c` |
| Five-way C harness | `4df575e42e046d45228634f0a453be59f590ec9d4924e14eb6b5fa97ec747684` |

Both source variants pass all 12,600 correctness executions. The suffix source
publishes `retained <= 8192` and `retained + removed <= 8192` as local
invariants before filling the prefix; these erased theorems establish the
existing append and cycle-helper requirements without an extra runtime guard.
It also carries the existing whole-chain/reuse construction observation:
`initial != 0` returns the checksum sentinel. Both libraries have the same
`new()` body without a result-length contract, and both source runs use this
same observation to establish emptiness. It is not an allocation-refusal
path or a workaround specific to take-first consumption. Any retained cost
from that common source shape is included here, not separately isolated.
Each timing dataset has 5,720 samples: 52 mode/width/path/length cells, two
cohorts, 11 samples and five implementations. Every timed checksum and final
release passed. A separate complete-matrix comparison confirmed identical
checksums, request counts, requested bytes and peak bytes across all controls
and both source variants. The optimized C modules are byte-identical across
the two builds.

The initial executable order was original ordinary, current ordinary,
original retained, current retained. Short scalar variation justified one
repeat of the same matrix in the exact reversed executable order; no workload
or sample count was widened. The raw
[`measurements-followups.csv`](measurements-followups.csv) contains all 22,880
samples with `run` (`initial` or `reversed`) and `library` (`original` or
`current`) columns. Its SHA-256 is
`a47707ffbe33f6415c4dd36e95604dcf45b2a774c74d2ad3168d8476e23459a4`.
The initial and reversed runs are retained separately, not pooled into a
single favorable median.

The current source improves the affected large-record workloads in both
source orders. To account for host variation, compute
`(current WF / current C) / (original WF / original C)` separately for every
cohort and each of the four C controls. A value below one favors the current
source. The ranges below include every such comparison in both runs; they
are observed ranges, not confidence intervals.

| Helpers | 256-byte workload | Lengths | Normalized current / original range |
| --- | --- | --- | ---: |
| ordinary | reserved, growth, reuse whole chains | 16, 256, 4096 | 0.852–0.981 |
| retained | reserved, growth, reuse whole chains | 16, 256, 4096 | 0.871–0.963 |
| ordinary | append/truncate suffix-3 cycle | 16, 4096 | 0.932–0.965 |
| retained | append/truncate suffix-3 cycle | 16, 4096 | 0.907–0.937 |
| ordinary | append/truncate suffix-1 cycle | 16, 4096 | 0.955–1.018 |
| retained | append/truncate suffix-1 cycle | 16, 4096 | 0.966–1.042 |

The one-element suffix has no reversed pair to remove and establishes no
reproducible source improvement. The three-element suffix does improve at
both lengths while preserving the retained prefix, supporting O(removed)
behavior rather than a hidden prefix walk. Setup and final prefix consumption
are still amortized into these cycle times, so they do not isolate truncation.

The initial whole-chain reuse medians below show both source change and
remaining costs against every C control. All times are ns/round; each C
column is current WF divided by that control. Other allocation paths and all
individual samples are in the raw dataset.

| Helpers | Bytes | Length | Original WF | Current WF | / Reverse C | / Direct C | / Swap/take C | / Take/swap C |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ordinary | 8 | 16 | 47.36 | 51.27 | 1.500 | 1.944 | 1.750 | 1.810 |
| ordinary | 8 | 4096 | 13,875 | 13,250 | 1.233 | 2.038 | 1.606 | 1.559 |
| retained | 8 | 16 | 62.50 | 60.55 | 0.939 | 0.984 | 0.992 | 0.984 |
| retained | 8 | 4096 | 20,750 | 20,500 | 0.965 | 1.031 | 1.031 | 1.025 |
| ordinary | 256 | 16 | 833.01 | 781.25 | 1.159 | 1.235 | 1.225 | 1.225 |
| ordinary | 256 | 4096 | 222,875 | 196,875 | 1.052 | 1.219 | 1.193 | 1.206 |
| retained | 256 | 16 | 862.30 | 794.43 | 0.929 | 1.018 | 0.835 | 1.018 |
| retained | 256 | 4096 | 227,625 | 201,250 | 0.927 | 1.050 | 0.827 | 1.051 |

Ordinary scalar reuse at length 16 regresses by 8.2% initially and 10.6% in
the reversed run (45.90 to 50.78 ns). Every cohort/control normalization for
that cell is above one, ranging from 1.022 to 1.128. Ordinary scalar suffix-3
changes direction between runs, and the retained short scalar suffix-3 C
controls vary enough to prevent a general improvement claim. Retained scalar
suffix-1 at length 16 is slightly slower after normalization in both runs;
the length-4096 result is less consistent. The source change therefore has a
measured small-scalar tradeoff, even though all large-record whole chains
improve. These observations do not select a per-size source specialization.

The large-record suffix medians make the ordinary-inlining gap explicit.
Times are ns/cycle, including append, checksum, amortized prefix setup and
final drain; ratios again compare current WF with each C control.

| Helpers | Removed | Length | Original WF | Current WF | / Reverse C | / Direct C | / Swap/take C | / Take/swap C |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ordinary | 1 | 16 | 43.40 | 42.94 | 2.698 | 2.781 | 2.698 | 2.714 |
| ordinary | 1 | 4096 | 46.66 | 45.81 | 2.477 | 2.609 | 2.513 | 2.532 |
| ordinary | 3 | 16 | 138.54 | 129.96 | 1.226 | 1.804 | 1.218 | 1.326 |
| ordinary | 3 | 4096 | 148.43 | 137.99 | 1.215 | 1.759 | 1.217 | 1.312 |
| retained | 1 | 16 | 41.49 | 41.12 | 0.725 | 0.755 | 0.718 | 0.705 |
| retained | 1 | 4096 | 44.34 | 44.01 | 0.756 | 0.800 | 0.731 | 0.749 |
| retained | 3 | 16 | 140.38 | 129.69 | 0.971 | 1.062 | 0.935 | 1.043 |
| retained | 3 | 4096 | 148.84 | 138.41 | 0.964 | 1.061 | 0.922 | 1.042 |

At length 4096 the reversed run retains a 2.60x direct-C cost for the ordinary
one-element record cycle and 1.78x for the three-element cycle. The latter's
retained result is 1.05x direct C. For the ordinary large-record whole chains,
the current/direct-C ratios remain 1.17–1.23 at length 4096 across both runs;
retained helpers give 1.05–1.06. The matched take/swap C comparison also
retains a gap, so the residual cost cannot all be attributed to the library
algorithm's extra relocation. Conversely, comparison with direct C includes
both composition and lowering costs. Inlining changes aggregate handling,
surrounding loops and checksum work; subtracting the two helper modes does
not isolate call overhead. The particular remaining optimizer causes were
not isolated by this source-only timing comparison. It supports the selected
large-record improvement, not general native parity or a minimum-cost API.

Local retained IR confirms the attribution's limited scope: original WF has
three bulk transfers per reversed pair plus one per callback, while current
WF and take/swap C have two per first-half iteration and one per remainder.
The current first half contains one memcpy and one memmove after Clang O2;
optimization may reconstruct memmove even though the compiler-owned swap
emits memcpy. The ordinary scalar paths are not measured by this bulk-copy
proxy. Both source builds retain 28 marked WF helpers; the optimized call
scan reports 14 ordinary and 56 retained WF library sites, and 32 retained C
append/truncate sites in its reverse/direct-family scan.

Construction and execution costs were recorded separately, in seconds:

| Stage | Original | Current |
| --- | ---: | ---: |
| Source admission and raw LLVM emission | 0.18 | 0.19 |
| Native executables and optimized IR, after emission | 1.92 | 2.05 |
| Cached experiment checks, both helper modes | 1.16 | 1.26 |
| Initial ordinary timing matrix | 2.03 | 1.99 |
| Initial retained timing matrix | 2.85 | 2.82 |
| Reversed ordinary timing matrix | 2.03 | 2.03 |
| Reversed retained timing matrix | 2.89 | 2.82 |

Every listed command exited zero under the shared verification guard. The
saved compiler was not rebuilt during this experiment. These stage costs
are local wall-clock observations, not program timing samples or compiler
performance comparisons. This measurement trial used the recorded v0.62
specification; the subsequent approved Box-placement clarification is described
under Source and proof boundaries below.

## Lowering attribution

This section preserves the historical v0.60 measurements and their original
three-way controls. The v0.61 follow-up above does not relabel these samples
as measurements of its newer compiler or source candidates.

The initial dataset is
[`measurements-x1-before-address.csv`](measurements-x1-before-address.csv).
It uses compiler `b3d323a7`, the current WF workload and C controls. Optimized
IR retains three unnecessary capacity-based wrap decisions in consuming
truncation: two per reverse pair and one per taken element. Slots has origin
zero and OP-4/OP-10 already prove each selected slot lies inside capacity;
only Ring needs head-relative wrap. Removing this Slots arithmetic changes
no source contract, backing layout, allocation, callback or element transfer.

For 4096 elements the initial WF/reverse-C ratios were:

| Helpers | Element bytes | Reserved | Growth | Reuse |
| --- | ---: | ---: | ---: | ---: |
| ordinary | 8 | 1.564 | 1.528 | 1.552 |
| retained | 8 | 1.182 | 1.176 | 1.190 |
| ordinary | 256 | 1.213 | 1.169 | 1.206 |
| retained | 256 | 1.063 | 1.057 | 1.058 |

The final samples are
[`measurements-x1.csv`](measurements-x1.csv), measured on 2026-09-21 PDT with
compiler `4d7a4c629` (including the address repair in `ad62a039f`). The workload,
C sources, inputs and harness are unchanged between the two datasets. The
address repair has a backend regression covering subscripts and back
placement/take for inline and boxed Slots; Ring retains wrap in both placements.
The final 4096-element WF/reverse-C ratios are:

| Helpers | Element bytes | Reserved | Growth | Reuse |
| --- | ---: | ---: | ---: | ---: |
| ordinary | 8 | 1.170 | 1.173 | 1.174 |
| retained | 8 | 0.988 | 0.989 | 0.988 |
| ordinary | 256 | 1.192 | 1.147 | 1.186 |
| retained | 256 | 1.043 | 1.036 | 1.041 |

For the reused 4096-element chain, absolute times separate the remaining
lowering gap from the cost of the source composition:

| Helpers | Element bytes | WF ns/round | Reverse C ns/round | Direct C ns/round | WF / direct C |
| --- | ---: | ---: | ---: | ---: | ---: |
| ordinary | 8 | 13,500 | 11,500 | 6,250 | 2.160 |
| retained | 8 | 20,500 | 20,750 | 19,500 | 1.051 |
| ordinary | 256 | 220,875 | 186,250 | 161,000 | 1.372 |
| retained | 256 | 226,500 | 217,500 | 191,250 | 1.184 |

The isolated code change removes all three Slots wrap decisions from retained
truncation. For the reused scalar chain, WF time falls 27.5 percent with
ordinary inlining and 18.0 percent with retained helpers; the corresponding
C controls change by 4.2 and 1.2 percent. This supports a real scalar benefit,
not attributing every timing difference to the patch. The large-record
change is much smaller. These short samples have no calibrated confidence
interval; the retained scalar result is approximate parity, not a speed win.

Optimized IR shows the remaining transfers directly:

| Retained 256-byte truncate | Per reversed pair | Per consumed record |
| --- | --- | --- |
| WF | 2 memcpy + 1 memmove, each 256 bytes | 1 memcpy into the callback argument |
| Reverse C | 3 memcpy, each 256 bytes | 1 memcpy into the callback argument |
| Direct C | none | 1 memcpy into the callback argument |

All three retain a direct callback call. WF's alias-permitting `swap` keeps
memmove and conservative element alignment where C keeps memcpy/alignment
facts. The transfer counts explain why removing wrap arithmetic cannot erase
the reversal cost; they do not isolate the timing contribution of alignment
or alias facts. In the retained large-record reuse case, reverse C is 26,250
ns slower than direct C and WF is a further 9,000 ns slower than reverse C.
Both are costs of the full chain. In ordinary mode inlining changes the
surrounding loops too, so subtracting retained and ordinary timings is not a
measurement of call overhead alone. The optimizer retains 7 ordinary and 42
retained WF library call sites; the retained C IR keeps 12 append/truncate
call sites.

The shortest scalar reuse case also remains significant: 49.8 ns WF versus
30.3 ns reverse C and 26.4 ns direct C at length 16 with ordinary inlining.
The 4096-element table must not stand for every length or element size; all
36 comparison cells are present in the raw samples.

The proposed library form is therefore a correct O(n), no-allocation baseline
with complete ownership cleanup, not the final minimum-transfer ordered
consumer. The measured gap reopens the blanket zero-extra-cost claim in
kernel minimality. Keep the operation inventory unchanged in this trial;
record direct ordered consumption and the residual ordinary-inlining cost
in `docs/todo.md`. A follow-up must compare representations or an operation
under the same callback/ownership contract before selecting language support.
Slab/Deque construction need not depend on a claim of Vector native parity.

## Source and proof boundaries

[Vector source obligations](../../../investigations/containers-and-resources/X1-LIBRARY.md#vector-source-obligations)
records the exact rejected fragments, rules and ordinary forms:

- ENT-5 removes a loop-header hypothesis at the loop exit; an INV-1 local
  theorem immediately before the break exports the required conclusion.
- FN-8's affine Signed Goal leaves are order comparisons. An unsigned
  `len <= 0` requirement expresses empty without a runtime check when the
  available proof is an invariant; equality works on the ordinary L0 route.
- The compiler wrongly rejected moving the sole linear field of a wrapper.
  The PROV-6 repair judges the unselected residual and includes regressions
  that still reject an abandoned generic or fieldless nodrop member.
- Destructuring a Box-containing wrapper previously lost the content measure
  fact. The approved v0.63 ENT-2/MSR-3 clarification explicitly carries current
  facts through exact owned fields, payloads and Box content, and includes the
  relative descendant projection in placement datum identity. The implementation
  carries that finite inventory through ownership moves and construction; focused
  [descriptor regressions](../../../../compiler/src/semantic/tests/descriptor_invalidation.rs)
  exercise it. Direct field consumption still serves this library without an
  artificial failure branch.

The amendment preserves existing invalidation and cross-function contract
boundaries. It does not infer window-element facts or add runtime checks. The
paired measurements above preceded this normative clarification and retain
their v0.62 compiler and specification identities.

## Reproduction and earlier evidence

Build the gate-profile compiler, then run from the repository root, one
shared verification owner at a time:

```sh
make -C compiler build
perl .github/run-check.pl vector-costs make -C research/experiments/container-representation/vector-library check
perl .github/run-check.pl vector-measure make -C research/experiments/container-representation/vector-library measure
```

`measure` writes `.build/measurements.csv`. `WHITEFOOTC=/path/to/whitefootc`
and a fresh `BUILD=/path/to/scratch` select another compiler without changing
the workload; use the `b3d323a7` compiler to reproduce the before-address case.
The checked-in dated samples retain the original run; a new run does not
silently replace them.

For the paired v0.62 trial, the original source is exactly
`git show efe41016d10379325ed4513d0ac7457ec7f24c5b:lib/containers/grow-vector.wf`.
Save it as `/tmp/whitefoot-vector-reverse.wf`, and use one saved compiler for
both builds. The executable hash above identifies the measured compiler;
a new build is a reproduction with its own identity. From the repository root,
after the gate-profile build:

```sh
vector_compiler="$(pwd)/compiler/target/gate/whitefootc"
perl .github/run-check.pl vector-original make -C research/experiments/container-representation/vector-library \
  BUILD=.build/final-v062-original WHITEFOOTC="$vector_compiler" \
  'SOURCES=/tmp/whitefoot-vector-reverse.wf vector-library.wf' check
perl .github/run-check.pl vector-current make -C research/experiments/container-representation/vector-library \
  BUILD=.build/final-v062-current WHITEFOOTC="$vector_compiler" check
perl .github/run-check.pl vector-paired sh -ec '
  experiment=research/experiments/container-representation/vector-library
  for pair in "original normal" "current normal" "original retained" "current retained"; do
    set -- $pair
    output="$experiment/.build/final-v062-$1"
    /usr/bin/time -p "$output/vector-costs-$2" measure > "$output/$2.csv"
  done
  for pair in "current retained" "original retained" "current normal" "original normal"; do
    set -- $pair
    output="$experiment/.build/final-v062-$1"
    /usr/bin/time -p "$output/vector-costs-$2" measure > "$output/$2-repeat.csv"
  done
'
```

To form the published dataset, concatenate each source's normal and retained
rows, preserving one header, and prefix every row with its run and library
identity. Validate 5,720 rows per run/source and identical
checksums and allocation metrics for every sample across the ten source/control
results before summarizing. The native runtime is constructed separately in
each build directory; the experiment adds no correctness-gate dependency.

The older [`measurements.csv`](measurements.csv) belongs to revision
`5fcf1ce2`, kernel v0.58, measured on 2026-09-14. Its provider/refusal contract,
32-byte descriptor and repeated front removal differ from this experiment.
Reproduce its code at that revision. Its four WF/matched-C ratios were
1.54/1.44 (ordinary reserve/growth) and 1.60/1.50 (retained); they are historical
evidence and are not current performance or refusal coverage.

## Contiguous Slots shift trial: criterion before implementation

Preparation starts at `a04ec2d6ca554f2e3a2bac849dfeb02af9e282ec`, with the
finalized source C from `47f9f91b63a484ba7f4924a63b5863b1b8b6f289` restored
after the separate source D trial. Both arms compile exactly that C library,
whose SHA-256 is
`5d0deaa41004b15b1def9c458df575634303df527b74d738916c9ad92b520197`;
the frozen control compiler and native image retain the identities in the
single-placement result above. Record the changed compiler identity before
measurement. The trial changes only the compiler's lowering of `Slots`
insertion/removal shifts. The same Vector source, append placement,
suffix-consumption algorithm, application
caller, native adapters, flags and oracle must serve both compiler images.
The candidate replaces the element walk by one overlapping transfer of the
complete contiguous suffix, including target padding and owning elements.
Ring keeps its logical walk; append, split and proved-empty release retain
their existing lowering. The proposal remains pending in the design amendment
until the owner rules on the evidence.

Insertion moves `[index, len)` to `[index + 1, len + 1)`; removal first
captures the removed value and then moves `[index + 1, len)` to
`[index, len - 1)`. OP-10 bounds both extents within capacity, including a
one-past pointer for a zero-count endpoint. The shared target-stride transfer
includes inter-element padding; `memmove` admits overlap without asserting
disjoint pointers. STOR-7 permits relocation of owning elements, and no call
or release can observe the intermediate bytes. Positive-stride extents fit
the already qualified complete allocation. Zero-stride pointers and byte
counts normalize to zero while logical indices and length updates remain
unchanged, even beyond the signed address domain.

Before timing, inspect the final O3 native code for fewer contiguous suffix
shift loops or per-element transfers in reserved/reuse. An unchanged final
shift falsifies this mechanism's proposed benefit; a raw-LLVM `memmove` alone
does not establish it. Pass fixed/runtime shape checks, zero-length endpoint
shifts, padded owning-value order and exact allocation/release observations,
and huge zero-byte logical counts with optional address facts both emitted
and withheld. The ordinary Vector program and complete ecosystem correctness
and accounting checks must remain valid before any measurement.

Retain the full matched matrix at the original populations 16, 256 and 4096
and payloads 8 and 256 bytes, with both order cohorts and unchanged native
algorithms. Reserved/reuse are the primary affected cells; all useful
mutating cells must be compared, with no repeatable regression accepted.
Keep suffix-zero controls separately labelled. Require the existing duration
and cohort-stability qualifications, preserve every baseline/candidate sample,
and assess the owner's per-cell native target separately from a speedup over
the compiler control. Wider shift populations may supplement this matrix to
distinguish transfer setup from per-element work, but cannot replace or remove
an original cell. This is a preregistered experiment, not a selected lowering
or a measured performance claim.

### Slots trial construction and correctness

The source-C hash above remains unchanged. Before timing, the candidate
compiler SHA-256 is
`bfd74a17222a39cc8e49b0b78bcead77ace8120f06287cb2cb6dbb2abce4e442`;
the final O3 Vector timed image is
`7acdec6b78a671ab7f3b1e483bf82e13d004e5fc008de3e7b51d9ae42a4b55f2`,
and its ordinary-allocation LLVM is
`032dcab2f679efc7f37192e1e45531bf7291a16b74d46f9cde99c6d8ebd96d38`.
Fresh `.build/slots-shift/` directories keep the source-C control images
unchanged. The candidate reuses `array.rs`'s complete-stride transfer helper
from the Slots arm of `runs.rs`; Ring and the other window operations retain
their previous lowering. These are candidate identities, not an adoption.

All three new backend tests in `compiler/src/backend/tests/windows.rs`
passed: fixed/runtime Slots versus Ring IR shape, padded owning values and
exact release order, and huge zero-byte logical lengths with native callers.
The last case exercises fixed length `2^63` and runtime length `2^64 - 2`,
including insertion/removal at the end, with optional address facts both
emitted and withheld. All nine formal container program tests passed.
Fresh timed and accounting correctness images passed for Vector, Deque,
HashMap, PriorityQueue and OrderedMap, including their existing rejection
controls. Accounting produced 294/120/420/150/210 rows respectively; Vector's
294 rows are byte-identical to the source-C control accounting file.

Construction and command elapsed times were recorded separately. Compiler
construction took 9.383 s. The first unit construction took 103.632 s, then
the focused command failed in 5.819 s because the zero-byte fixture nested
`Empty()` in an argument, contrary to GRAM-9. The fixture also needed separate
bindings for arithmetic inside comparisons under GRAM-6. Binding these
expressions repaired the fixture without changing the parser or language;
direct compiler emission then passed in 0.024 s. The repaired unit
construction took 56.001 s and the three-test command 6.392 s; its Cargo
test-body elapsed time of 5.75 s includes native construction inside test
helpers. Corpus construction took 0.568 s and the nine-test command 31.924 s.
Fresh ecosystem construction took 31.096 s, its correctness commands 7.543 s,
and accounting 4.220 s. No performance measurement is included in these
correctness timings.

Bounded scratch fault controls used the same fixture sources and independent
ledger/native witnesses. Each unmodified control passed first. Suppressing
release `F1` left the native child at exit 0 but made the exact-ledger oracle
fail with status 1. Giving the middle owning insertion the preceding
initialized element as its shift source produced native exit 43 with
`duplicate or unknown owner release`. Removing its transfer entirely had
instead faulted with signal 11 before reaching the ledger; that result was
not counted as a ledger rejection. Removing the huge fixed-window insertion's
length store produced native exit 3 at the first roundtrip witness. Changing
a normalized zero-stride GEP operand from `0` to `9223372036854775808`
failed the address-operand assertion with status 1, without executing a huge
walk. The successful scratch-control command took 1.936 s and required no
further Rust rebuild.

To reproduce the maintained correctness checks, use the current trial
compiler with `cargo test --manifest-path compiler/Cargo.toml --profile gate
--jobs 2 --locked --offline --lib _shifts_ -- --test-threads=1`, then the same
Cargo options with `--test corpus programs::containers::`. Construct and run
the explicit aggregate `ecosystem-build`, `ecosystem-check` and
`ecosystem-account` targets with `BUILD=.build/slots-shift`, that compiler's
`WHITEFOOTC` path and the pinned `ABSEIL_PREFIX`. Each command belongs under
the repository's shared verification guard; none invokes measurement or adds
research to the correctness gate.

### Slots final code and paired timing: regression prevents selection

The candidate fails the recorded no-useful-regression criterion. Scalar
reuse at population 16 takes 1.870–1.876 ms versus fresh C's 1.639–1.642 ms:
Slots/C ratios are 1.138855 and 1.144600. In both cohorts, the candidate
minimum exceeds the control maximum, by factors 1.107591 and 1.108603.
Rust median drift is 0.998388/1.005673 and C++ drift 1.035072/1.002789.
The regression survives the native-control comparison and is decisive
against production selection under the prerecorded criterion. This measured
lowering is not recommended; the pending design amendment is not an adopted
decision.

The final-code discriminator did pass. Inspecting actual mixed-trace work,
scalar instructions fall from 173 to 149; wide instructions change from
410 to 411. Both removal shift loops become `memmove` calls. Retained
scalar insert/remove helpers shrink from 160/37 to 109/20 instructions,
and wide helpers from 177/46 to 132/33. There is a competing wide-value
cost: preserving the removed value across the bulk call spills 25 payload
fields instead of eleven, adding 112 bytes of stack stores and 112 bytes
of reloads per removal. The wide work frame grows from `0x2f0` to `0x360`.
The existing 256-byte insert-entry snapshot remains. Loop elimination is
therefore established, but those observations alone do not assign the
measured time to transfer setup, spills or the surviving insertion boundary.

The six scalar/wide trace, tail and truncate bodies are unchanged after
normalizing branch targets and constant references. Scalar trace loop
addresses are also identical. Wide tail placement moves by -96 bytes and
wide truncate by -364 bytes; these remain image-level differences. Suffix
paths execute no insert/remove shift, so their timing changes cannot be
presented as a direct bulk-transfer benefit. Disassembly uses the same
`llvm-objdump --macho --disassemble --no-show-raw-insn` method as the preceding
trials, inspecting callers as well as retained helpers.

The pair uses unchanged library source C from
`47f9f91b63a484ba7f4924a63b5863b1b8b6f289`; its whole-library SHA-256 is
`5d0deaa41004b15b1def9c458df575634303df527b74d738916c9ad92b520197`.
The control is the frozen production-C image identified above; the candidate
compiler, LLVM and image are the identities in the construction section.
The C driver, Rust archive, C++ object and all 294 accounting rows are
byte-identical between arms. Both full matrices ran sequentially in one
guarded command, invoking each frozen image with `measure 1048576 7`.
The control took 80.725 s and Slots 80.401 s, both exit 0; the outer guard
took 161.34 s, with no queue wait or busy retry. The runner rechecked source,
compiler, native-input, accounting, LLVM and image hashes afterward and
removed its one-shot script. No construction or correctness time is included
in these measurement durations.

The fresh [C control samples](ecosystem-slots-control-samples.csv) and
[Slots samples](ecosystem-slots-shift-samples.csv) each contain 4,116 rows.
Their SHA-256 values are
`df92f6a209c74efc77e4fa29522816d96ccf1c505db2dd08ee9ceb5e5e4ca3b2` and
`c5e61ba1392d32344bd81126310968ec51cd9b5ea14958b1b057946302f50f24`.
All keys, work, rounds, traces, checksums and sample IDs 0–6 agree. Both
files pass `summarize-ecosystem.pl --complete` and `--complete --targets`.
Earlier source-C measurements remain separate evidence and are not pooled.

The table covers all 36 useful cells. Ranges span the two cohort medians;
ratios are whole-trace comparisons, not isolated insertion/removal latency.
The final column uses each cohort's slower Rust/C++ standard median and
does not itself establish sample separation.

| Bytes | Path | Slots / C at 16 | at 256 | at 4096 | Slots ms at 4096 | Slots / slower standard at 4096 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | reserved | 1.021–1.053 | 0.979–0.994 | 0.954–0.988 | 1.659–1.668 | 0.803–0.809 |
| 8 | growth | 1.039–1.049 | 0.993–0.995 | 0.952–0.993 | 2.049–2.058 | 0.858–0.858 |
| 8 | reuse | 1.139–1.145 | 0.985–0.992 | 0.990–0.995 | 1.629–1.640 | 0.790–0.795 |
| 8 | suffix-1 | 0.997–0.998 | 1.001–1.029 | 0.985–0.997 | 2.945–2.952 | 1.003–1.004 |
| 8 | suffix-2 | 0.991–0.997 | 0.996–1.009 | 0.990–0.997 | 2.379–2.381 | 1.452–1.455 |
| 8 | suffix-3 | 0.982–0.983 | 0.995–1.003 | 0.999–1.011 | 1.931–1.959 | 1.105–1.119 |
| 256 | reserved | 1.007–1.010 | 1.000–1.001 | 1.009–1.012 | 41.130–41.247 | 0.945–0.946 |
| 256 | growth | 0.995–1.003 | 1.000–1.001 | 0.998–0.999 | 53.006–53.231 | 1.016–1.026 |
| 256 | reuse | 1.006–1.008 | 1.006–1.009 | 1.005–1.006 | 41.005–41.030 | 0.944–0.945 |
| 256 | suffix-1 | 1.000–1.002 | 0.998–1.007 | 0.997–1.000 | 22.698–22.701 | 1.361–1.365 |
| 256 | suffix-2 | 0.999–1.035 | 0.981–0.993 | 1.013–1.032 | 23.120–23.877 | 1.061–1.095 |
| 256 | suffix-3 | 1.000–1.001 | 1.004–1.004 | 0.998–0.999 | 26.482–26.518 | 1.009–1.012 |

Fifteen useful cells have lower Slots medians in both cohorts, thirteen
higher and eight mixed. Applying the observed-range comparison separately
to every cell yields one strict regression, zero strict gains and 35 with
overlap in at least one cohort. Small scalar reserved and growth also have
higher medians in both cohorts, but their ranges do not establish strict
regressions in both. The target reduction is a separate judgment: fresh C
has 13 passes / 10 deficits / 13 inconclusive, while Slots has
12 / 11 / 13. Each arm retains six unranked suffix-zero controls.

All useful native target comparisons meet the duration and cohort-spread
qualifications; the minimum useful WF sample across the arms is 1.544 ms.
Every sub-millisecond observation is a suffix-zero control. Fresh C's
scalar suffix-2 direct-C comparisons at 256 and 4096 are unstable (46.609%
and 13.521%); Slots' only unstable comparison is wide suffix-zero take/swap C
at 16 (29.673%). They support no ranking or attribution. Across all useful
cells/cohorts, Rust control drift is 0.952–1.029 and C++ drift 0.962–1.035.
The same-source experiment measures the combined bulk-call, spill and
code-placement change; it does not isolate any of those costs. In particular,
the surviving insert snapshot is a concrete next discriminator, not an
established cause of the scalar reuse regression.

The rejected compiler implementation and its tests remain reproducible in
[slots-shift.patch](slots-shift.patch), SHA-256
`6c120ebbb4fa178a3ef6b56bd9a8207c51de491a3b33a92fb829024209af9485`.
It applies to base
`02b61a3056d9ca1bbd1c34bfaf5ca1100301b779`; apply it in an isolated checkout
before the construction commands above. After the failed comparison, its
three production emitter/test files were restored exactly to that base by
reversing the saved patch. The pending amendment retains the failed result
for disposition. Source restoration does not replace an already built
compiler: preserve the identified Slots artifacts and rebuild the embedded
compiler before the next source-only trial on original C lowering.

### Zero-capacity `Slots` sentinel: complete samples, selection unresolved

The candidate [`zero-slots.patch`](zero-slots.patch) makes a runtime-capacity
`Slots` constructed with capacity zero point at one private module global
`{ i64, i64 }` whose two words are zero. `grow` checks the old capacity before
freeing, and `free_empty` skips the global. `Ring` and positive-capacity
`Slots` retain the ordinary `malloc`/`free` path. The historical acceptance
criterion was: all 1,260 configurations and 8,820 executions pass with
unchanged checksums and refusal cleanup; the accounting must show no request
for a zero-capacity construction, one fewer request/release at each such
trace, and unchanged positive-capacity rows; optimized IR must contain the
shared header and no allocator call on the zero branch; and every qualified
useful cell must improve or remain qualified.

The original run report states that the unmodified compiler and candidate
were rebuilt separately from the same branch with the same native inputs,
`ECO_WORK=1048576` and seven samples. It reported construction times of 3.23 s
and 13.08 s, correctness times of 6.12 s and 3.14 s, and matrix times of
82.01 s and 81.60 s, with both full matrices and checksum/cleanup faults
passing. No frozen phase-time or exit-status logs for these last three trials
were found in the subsequent audit. Those times and exits remain original
reported observations; this audit reduced the saved CSVs and inspected
existing code, without rebuilding or rerunning correctness or timing.

The complete paired evidence remains in
[`baseline samples`](ecosystem-zero-slots-baseline-samples.csv) and
[`candidate samples`](ecosystem-zero-slots-candidate-samples.csv), with
[`baseline targets`](ecosystem-zero-slots-baseline-targets.csv),
[`candidate targets`](ecosystem-zero-slots-candidate-targets.csv),
[`baseline account`](ecosystem-zero-slots-baseline-account.csv) and
[`candidate account`](ecosystem-zero-slots-candidate-account.csv).
The ledgers record scalar reserved-16 falling from six requests/504 bytes to
three requests/456 bytes per three-round trace, and scalar growth-16 from
21 requests/1,848 bytes to 18/1,800. These are the expected removed empty
header allocations; the positive reserve still allocates its backing.

The candidate target reduction is 20 pass / 1 deficit / 15 inconclusive
useful cells, versus the baseline's 14 / 5 / 17; each has six unranked
suffix-zero controls. The remaining peer deficit is wide suffix-1 at 4096:
the candidate/slower-standard median ratios are `2.119030606` and
`2.023018210`. The former report mixed a median ratio with an observed upper
bound. A deficit against a native peer does not itself establish a regression
against the paired Whitefoot baseline. In this cell the candidate/baseline
median ratios are `1.223888297` and `1.236763787`, but the observed ranges
overlap in both cohorts: baseline/candidate ranges are
`24.699–33.846 / 22.355–36.121 ms` and
`19.120–36.727 / 23.961–35.996 ms`. No useful cell has a candidate minimum
above its baseline maximum in both cohorts. The previous rejection on a
claimed strict paired loss is therefore withdrawn.

The candidate remains unselected: the full peer target is incomplete. The
report's claimed refusal coverage also was not established. The ecosystem driver's
[`fail-checksum` and `fail-cleanup` commands](vector-costs.c) corrupt a checksum
and leave an allocation unreleased; they do not inject allocation exhaustion
or separately validate global-header sharing. The historical refusal wording
is not evidence about resource-exhaustion behavior and does not establish
that an optimization must preserve exact allocator request positions.
The historical proposed refinement was a direct pointer comparison with
`@wf_zero_slots_header` and specialization of a known-zero constructor. It
was not measured here, and these results neither select it nor close the
sentinel route.

### Historical discriminator: wide append boundary, premise invalidated

The historical plan assumed that the timed wide `tail_work` loop retained a
call to `grow_vector_append` carrying a 256-byte owner temporary. It proposed
`alwaysinline` on the wide append definition, with scalar input, truncate,
source contracts, allocation policy, native inputs and other attributes
unchanged. Its code criterion required removal of that call without a new
snapshot, spill or scalar body change, followed by matched correctness,
accounting and timing with no qualified useful-cell regression.

The actual O3 control-object inspection in the append-and-truncate trial
below invalidates that append-call premise: append is already inlined in the
wide tail. The earlier diagnostic IR and the absence of an append call in a
candidate do not establish that the timed control had such a boundary. The
historical criterion is retained as the plan, not as a passed discriminator.

### Wide append `alwaysinline`: contaminated accounting lineage, selection invalid

The frozen [`wide-append-inline.patch`](wide-append-inline.patch) adds
`alwaysinline` only to the wide `grow_vector_append` definition. The original
report claimed matched timed/accounting correctness and identical ledgers,
with matrix times of 82.01 s and 80.75 s. The retained artifacts do not
establish that matched comparison. The original samples and ledgers remain
unchanged so the contamination is visible:

| Published artifact | Byte-identical artifact | SHA-256 |
| --- | --- | --- |
| [Control samples](ecosystem-wide-append-inline-control-samples.csv) | [Zero-slots baseline samples](ecosystem-zero-slots-baseline-samples.csv) | `236dfb85f25c5e43e30aa53a61d5aecf0781d88c1662bae6cd32c2635781db1f` |
| [Control targets](ecosystem-wide-append-inline-control-targets.csv) | [Zero-slots baseline targets](ecosystem-zero-slots-baseline-targets.csv) | `07d4f371932a396b3f16c5d30a962ad64089642a868de3645a6867eb54aee33d` |
| [Control account](ecosystem-wide-append-inline-control-account.csv) and [candidate account](ecosystem-wide-append-inline-candidate-account.csv) | [Zero-slots candidate account](ecosystem-zero-slots-candidate-account.csv) | `61b5f6ead9d24608a8db781eab95a988f6f6f49c829f7be12cc95e68104e5b2b` |

The published control samples belong to the non-sentinel baseline; both
published ledgers record the sentinel candidate's reduced allocations. For
scalar reserved-16 they report three requests/456 bytes, whereas the
[baseline ledger](ecosystem-zero-slots-baseline-account.csv) records
six/504. The 2026-09-27 audit inspected the saved
`/private/tmp/vector-wide-append-baseline.raw.ll`,
SHA-256 `e396d7891d43ef1a8438628a9b4f40f86f8435eea6b1856d3587e1c5f63350f9`,
which contained no `wf_zero_slots_header` and was byte-identical to the later
append-and-truncate control raw module. The hash identifies the inspected
bytes; the scratch path records their location at inspection, not a
maintained artifact. Thus ledger equality is contamination,
not evidence of allocation preservation for the timed pair. The claimed
matched correctness/accounting lineage is withdrawn, and this trial is
invalid for policy selection.

The [candidate samples](ecosystem-wide-append-inline-candidate-samples.csv)
and [candidate targets](ecosystem-wide-append-inline-candidate-targets.csv)
still describe the recorded timings. The target counts are 14 pass / 5
deficit / 17 inconclusive for control and 17 / 5 / 14 for candidate. Wide
suffix-1 at 256 has candidate/slower-peer median ratios `2.157323034` and
`2.165020900`. The scalar peer deficits are growth at 16 and suffix-2 at
16, 256 and 4096. They are not measured Whitefoot regressions: candidate
Whitefoot medians are lower than control in both cohorts of all four cells,
with candidate/control ratios from `0.939459459` to `0.998297389`. The claims
of an append-boundary code win, unchanged owner snapshots and scalar
regressions are withdrawn. These artifacts establish neither a causal cost
of append inlining nor a need to change movement/result contracts.

### Append and truncate `alwaysinline`: truncate removed, paired scalar regression

This separate trial marked the 256-byte append and truncate definitions
`alwaysinline` in the same raw module. The edit is
[`append-truncate-inline.patch`](append-truncate-inline.patch), SHA-256
`03d29b222bf7fb3c5eedc00666502a58211959a854493610e5c2564efd079207`.
The patch leaves scalar input and allocation code unchanged; the recorded
harness uses `ECO_WORK=1048576` and seven samples. Its artifacts now have
distinct names because the original publication overwrote the earlier
counted-consumer tail-and-truncate patch and samples. Those three historical
files have been restored byte for byte from
`e130362daec2520d89b553509527d3729de60ea9`; the latest bytes are preserved
under the append-and-truncate names here.

The original report states that both images passed all 1,260 configurations
and 8,820 executions, checksum/cleanup faults and the allocation ledger, with
matrix times of 81.08 s and 80.94 s. The audit found the two scratch fault
message files, but no retained exit-status or phase-time logs to reverify
those run reports. The faults have the limited checksum/cleanup meaning
described above. The two saved 294-row account CSVs are byte-identical,
SHA-256 `ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`.
The complete evidence is preserved as
[`control samples`](ecosystem-append-truncate-inline-control-samples.csv),
[`candidate samples`](ecosystem-append-truncate-inline-candidate-samples.csv),
[`control targets`](ecosystem-append-truncate-inline-control-targets.csv),
[`candidate targets`](ecosystem-append-truncate-inline-candidate-targets.csv),
[`control account`](ecosystem-append-truncate-inline-control-account.csv) and
[`candidate account`](ecosystem-append-truncate-inline-candidate-account.csv).
The sample SHA-256 values are
`e7aaf8589d70162aaca7e14fc07df4ef53c80a83add0c6ec3445b3fbff854d46` and
`2a2991a22dbfd2536190cc2267027a4d578a5a3bb5422591bb393686f72d445a`.

The 2026-09-27 audit inspected the saved timed objects at
`/private/tmp/whitefoot-vector-double-inline/{control,candidate}/ecosystem/whitefoot-timed.o`,
with control/candidate SHA-256 values
`c7fc3dcf0d127d3586678f0c8aaabe5848da38a1e427f35c4f729261ba50e235` and
`643ddf139de86a741aeb21140e6be0ad872e97335edbbfc13bc502aefc923b92`.
These hashes identify the inspected bytes. The scratch paths are inspection
locations, not maintained artifacts; exact regeneration from frozen compiler
and raw-input provenance is not established by this record.
Their retained configuration records O3, matching the
[`ecosystem` object rule](Makefile). Disassembly with
`llvm-objdump --macho --disassemble --no-show-raw-insn` showed that the control
wide tail calls only `grow_vector_grow_full` and `grow_vector_truncate`;
the candidate calls only `grow_vector_grow_full`. Append was already
inlined in the control. This establishes removal of the truncate call in
that body, not removal of an append boundary. Both prologues reserve `0x120`
stack bytes; that alone does not establish unchanged owner snapshots or
spills. The separate `.opt.ll` files contain 79 and 230 wide-tail lines,
but their generation flags were not retained. Those diagnostic line counts
do not characterize the timed O3 object, and the previous code-expansion and
no-new-snapshot inferences are withdrawn.

The target reductions remain 17 pass / 4 deficit / 15 inconclusive for
control and 17 / 3 / 16 for candidate. The candidate's three remaining peer
deficits are wide suffix-1 at 16 (`1.866642810 / 1.963302752`), scalar growth
at 16 (`1.081922675 / 1.082418157`) and scalar suffix-2 at 4096
(`1.085015291 / 1.073067633`). They do not by themselves establish paired
regressions. A distinct cell, scalar suffix-1 at 256, does have separated
Whitefoot ranges in both cohorts:

| Cohort | Control median (ms) | Control range (ms) | Candidate median (ms) | Candidate range (ms) | Candidate/control median |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 2.873 | 2.836–2.918 | 3.029 | 2.947–3.096 | 1.054298643 |
| 1 | 2.880 | 2.844–2.919 | 2.955 | 2.931–3.033 | 1.026041667 |

Both candidate minima exceed the paired control maxima. The contemporaneous
Rust median ratios are `1.008877616 / 1.001937984`, and C++ ratios are
`1.030149051 / 1.004728132`; the Whitefoot median increase exceeds both
native median drifts in both cohorts. This is an observed collateral effect
of the changed image with unchanged scalar input, not an attribution to new
scalar capacity work, a particular spill or code placement. The paired
regression and incomplete peer target prevent selecting this diagnostic. No
production inline policy or movement/result contract change follows from
these trials.

### Actual compiler factor isolation after ownership integration

This bounded discriminator separates terminal-owned-consumption lowering (T)
from function-actual `inlinehint` emission (H). It selects no production policy.
The [criterion](../ECOSYSTEM.md#actual-compiler-factor-isolation-criterion-and-result) was recorded before construction and timing.
The source pin is `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`.
Four compiler variants were constructed from a fresh plain `git archive` snapshot:
neither (T0H0), terminal (T1H0), hint (T0H1), and both (T1H1, current source).
No production, specification, design-tree, helper, harness, or native flag changed.

H has no native effect for this source, compiler, and toolchain. In both H pairs,
raw LLVM differs only by four `inlinehint` tokens, on the word/record make/accept
definitions. The timed and accounting WF objects are byte-identical within each
pair; every linked section's bytes and layout and the full disassembly also match.
Different whole-image hashes arise from UUIDs, six Rust archive debug object paths,
string-table padding, corresponding LINKEDIT offsets/sizes, and code signatures.
Semantic symbol entries and indirect-symbol bytes match after path normalization.
Thus only neither and both were measured; hint and terminal are deduplicated,
unmeasured equivalents, not extra timing observations.

T changes exactly the scalar and wide `grow_vector_truncate` definitions in raw
LLVM; all 62 definitions remain present and text outside definitions is unchanged.
All 18 non-WF native inputs, including Rust/C++/driver and runtime objects, match
across all four arms. Configuration contents match after only compiler-path
normalization. The [account CSV](ecosystem-compiler-factor-accounting.csv) is byte-identical across arms (294 data rows).
Each arm's timed and accounting checks passed 1,260 configurations / 8,820 executions;
both checksum and cleanup fault controls failed with the required diagnostics.

The T native delta includes scalar truncate 44→17 instructions and wide truncate
168→65. Wide truncate remains frameless and call-free; the current forward loop
reads owned backing storage directly and performs no replacement payload stores.
Empty truncation skips payload, digest, and length writes in both images.
The suffix-zero trace still drains its retained prefix at the end; it is not
an untouched-code control. The suffix-one old first-half exchange is already
bypassed, so removing its copies cannot explain that cell. No calls
to either accept callback survive in either image. Wide trace/tail/mixed-work
instruction counts and frames remain unchanged after relocation qualification;
the wide mixed-work environment-pointer spill already exists in both images.
WF object and linked text shrink by 996 bytes, shifting native peer entries by
996 bytes. The wide tail still calls truncate, including on empty truncation.
These observations do not isolate traversal/controller costs from code placement.

The single timing attempt ran [neither](ecosystem-compiler-factor-neither-samples.csv) then [both](ecosystem-compiler-factor-both-samples.csv), each with
`measure 1048576 7`, preserving the full matrix, two cohorts and native controls.
Each CSV has 4,116 rows and 588 groups with samples 0–6. All seven samples enter
the maintained `--complete` / `--targets` reductions and the paired reduction;
Vector sample 0 is not warmup. Non-time columns and checksums match row for row.
The 148 frozen construction artifacts and all 765 source inputs match before and
after timing. There was no rerun, sample removal, rebuild, or threshold change.

A strict paired gain/loss requires disjoint seven-sample ranges in the same
direction in both cohorts. Qualification requires every paired WF sample ≥1 ms
and cohort median-ratio spread ≤10%; the six suffix-0 controls remain unranked.
Among 36 useful cells, raw outcomes are 13 gains, zero losses, and 23 overlaps.
Qualified outcomes are 13 gains, 21 overlaps, and two unstable wide suffix-1 cells.
The minimum useful WF sample is 1.214 ms. The 13 strict gains comprise eight
scalar cells and five wide suffix-2/3 cells; all cells and native drift remain in
the [complete paired CSV](ecosystem-compiler-factor-paired.csv), rather than selecting only improved cells.

| Wide suffix-1 count | Both/neither median, cohort 0 | Cohort 1 | Paired qualification |
|---|---:|---:|---|
| 16 | 1.325313 | 1.573929 | Overlap; unstable |
| 256 | 1.181342 | 1.330321 | Overlap; unstable |
| 4096 | 1.365537 | 1.290925 | Qualified overlap |

All six wide suffix-1 cohort medians are adverse, but none of these three cells
has a strict paired range loss. Instability does not establish absence of loss.
All three wide empty controls have strict losses meeting the duration/stability
screens: median ratios at
counts 16/256/4096 are respectively 1.167435/1.161914, 1.165732/1.162354, and
1.154983/1.145972 (cohorts 0/1). Scalar empty controls overlap and include <1 ms
samples, so remain unranked. The result rules out a native H interaction here:
the T-only image already has the same executable code and layout as both.
It attributes this whole-build contrast to T and its resulting placement changes,
without assigning isolated loop-cost percentages or selecting a production policy.

The maintained standard-peer target counts move from 12 pass / 8 deficit /
16 inconclusive to 16 pass / 1 deficit / 19 inconclusive, with six unranked controls
in each arm. These counts do not establish selection or target completion.
The current qualified deficit remains wide suffix-1/count 4096: WF/slower-standard
median ratios are 1.861924 and 1.780986 (C++ in both cohorts).
Useful-cell native median drift (both/neither) spans Rust 0.943246–1.051724,
C++ 0.935431–1.103050, reverse-C 0.943436–1.060588, direct-C 0.694860–1.768427,
swap-take-C 0.937053–1.083481, and take-swap-C 0.953809–1.061672. Identical input
objects do not make relocated native peers a timing-invariant reference.

The tracked source/harness/Makefile/reducer at the pinned revision are reproducible
inputs. Retained compilers, LLVM, objects, linked images, phase logs and inspection
JSONs are scratch evidence under `vector-library/.build/actual-ablation/` in
`/private/tmp/whitefoot-vector-actual-ablation`; they are not tracked repository files.
The scratch files serve this discriminator and can retire after publication and
reproducible input/sample retention in the existing vector-library experiment home.
`base-source.json`, each arm's `source.patch`, `source.json`, `artifacts.json`, and
`freeze.json` record exact source and artifact identity; `native/` records the screen.
The scratch paired recipe is `analyze.py`; it groups the seven samples by
payload/path/count/variant/cohort, takes the middle sample after sorting, and
applies the range and qualification rules above. Raw samples, shared accounting
and the complete paired CSV are retained beside this report; maintained outputs
and the scratch analysis remain under `measurement-1/`. This distinction matters:
reading the tracked CSVs reproduces the numeric reduction, not the prior binary
execution or native-code inspection.

The [factor switch patch](actual-compiler-factor-switches.patch) records every
source difference from the pinned current compiler. Apply only its builder
file for T0, only its emitter file for H0, both for neither, and neither for both.
Reconstruction applies only these guarded condition changes to the pinned source:
T0 prefixes `true || ` to `!builder.lower_terminal_consumption(function, functions)?`;
H0 prefixes `false && ` to the three `is_function_actual()` condition sites in
`backend/emitter.rs`, preserving the declaration condition where present. Each
exact site must match once; the arm patches record the complete differences.
Use a fresh archive of `compiler spec lib .github research/experiments/container-representation`.
Build each arm sequentially with `cargo build --manifest-path compiler/Cargo.toml
--target-dir compiler/target --profile gate --bin whitefootc --locked --offline -j2`,
copying its compiler before switching. Run unchanged vector Make `ecosystem-build`,
`ecosystem-check`, `ecosystem-account` with a fresh arm `BUILD`, its frozen `WHITEFOOTC`,
`CLANG=/usr/bin/clang`, and `CXX=/usr/bin/clang++`, under `.github/run-check.pl`.
Inspect code/layout before timing. From this family directory, reproduce
the maintained reductions with `perl ../summarize-ecosystem.pl --complete
vector=ecosystem-compiler-factor-neither-samples.csv` and `--targets`; substitute
`both` for the second arm. Apply the explicit paired rules above to either the
raw samples or the resulting minima/medians/maxima; `analyze.py` is the saved
scratch implementation, not a new maintained runner.
The one-shot guarded runners are `run.py --construct` and `measure.py`; their full
commands, direct exit statuses and wall times are retained in both `phase-times.json` files.
Toolchain: Apple clang 21.0.0 (clang-2100.3.34.2), arm64-apple-darwin25.6.0;
rustc 1.98.1 (48a229cea, 2026-09-01), Rust LLVM 22.1.8; native optimization is O3.

| Arm | Compiler build s | Native build s | Check s | Account s | Timing s |
|---|---:|---:|---:|---:|---:|
| neither | 61.897231 | 4.754754 | 1.743580 | 0.165880 | 81.453939 |
| terminal | 38.336617 | 4.956677 | 1.749854 | 0.167498 | Unmeasured |
| hint | 40.255096 | 5.010803 | 1.833778 | 0.174398 | Unmeasured |
| both | 39.946627 | 4.967135 | 1.789267 | 0.167887 | 80.901394 |

All direct phases exited 0. Four maintained reductions took 0.061–0.063 s each;
guarded construction and measurement totals were 209.29 s and 163.18 s (exit 0).
Primary SHA-256 evidence (all remaining identities are in the manifests):

| Artifact | SHA-256 |
|---|---|
| neither samples | `9e1c32edb9c5db8d7bc5e577e8e4e7fadf17bbdddb8c729320eb8dc9aea390d7` |
| both samples | `f8e19af7e6c57a8fe157de1c29dbdcddc713d181982c313c6e3e7fda8e8b3a88` |
| paired `analyze.py` | `bcba28a09897910f13c0b632fb912814b5dab001b8e6c28cffb00cbb1ec7a144` |
| account CSV, every arm | `ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7` |
| neither/hint timed WF object | `16522ed39dc4f6af947bf89108a8444d25839dd7aaa8db8a02316da55f3b2add` |
| terminal/both timed WF object | `c7fc3dcf0d127d3586678f0c8aaabe5848da38a1e427f35c4f729261ba50e235` |

### Remaining-count probe: rejected before timing

The [registered discriminator](../ECOSYSTEM.md#remaining-count-induction-criterion-and-code-screen-rejection)
replaced only the terminal traversal's induction with cursor plus remaining
count, keeping its recognizer, owner handoff, empty path and final publication.
The [complete scratch patch](actual-countdown-probe.patch) also extends the
independent zero-stride C oracle to logical length `UINT64_MAX`, removed counts
0–5. It applies to the same pinned `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`
source as the factor trial; the compiler variant and test edit remain unpromoted.

Both focused tests pass, including 513 owner/prefix/head/zero-stride cases per
overlap mode and
the existing release, observer, different-consumer and partial-exit checks.
Each timed/account image passes 1,260 configurations / 8,820 executions and
the checksum/cleanup fault controls. All 18 non-WF inputs match the frozen
`both` arm; its 294-row accounting CSV is byte-identical. Raw LLVM changes
exactly the two truncate definitions among 62, with no other definition or
module-text change. Yet both WF objects are byte-identical to `both`:
timed SHA-256 `c7fc3dcf0d127d3586678f0c8aaabe5848da38a1e427f35c4f729261ba50e235`,
account `414b22a0d3502f2236ef0a9d1b1e743d8130f84335b66d741e5b417fd0408c2b`.
All 15 linked sections per image match in bytes, addresses, sizes and alignment;
scalar/wide truncate remain 17/65 instructions, with the wide increment/`cmn`
controller intact. This fails the registered code screen. No timing ran and
no production performance conclusion follows from a source-only rewrite.

| Phase | Seconds | Exit |
|---|---:|---:|
| Gate compiler build | 62.340 | 0 |
| Gate library-test construction | 112.013 | 0 |
| Two focused tests | 3.990 | 0 |
| Native image construction | 2.654 | 0 |
| Complete matrix and fault controls | 1.883 | 0 |
| Accounting | 0.124 | 0 |

Reproduction uses the previous factor trial's pinned archive and build/check
commands after applying this patch, plus `cargo test --manifest-path
compiler/Cargo.toml --target-dir compiler/target --profile gate --lib --no-run
--locked --offline -j2` and the resulting binary's
`backend::tests::terminal_consumption::` filter. Commands, direct statuses,
source/artifact hashes and native inspection remain under
`vector-library/.build/countdown-probe/` in the scratch tree
`/private/tmp/whitefoot-vector-countdown-probe`; they are not tracked artifacts.
Only the reproducible source patch is retained beside this report. The scratch
runner and images can retire after publication; the rejected spelling can reopen
only if a changed toolchain or lowering produces the required native difference.

### Wide-tail setup and digest handoff: deferred discriminators

At frozen pin `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`, the saved linked
`native/both-timed.disassembly.txt` under
`/private/tmp/whitefoot-vector-actual-ablation/research/experiments/container-representation/vector-library/.build/actual-ablation/`
shows a 288-byte
wide-tail frame at `c7dc`; short addresses use prefix `0x10000`. Fourteen
Q-register literal loads at `c810–c87c` feed seven paired saves at `c880–c898`:
224 stack bytes per nonempty cycle, reloaded only after `grow_full` at `c934`.
These preserve constructor constants, not a record snapshot; append constructs
256 bytes directly in backing. Trace-to-tail (`c0b8`) and tail-to-truncate
(`c96c`) remain two hot calls. Truncate at `d3c8` consumes each record with
sixteen paired loads and 32 `madd`s; the digest store/load/store/load handoff
is at `c0a0`/`d3dc`/`d4bc`/`c0c0`. Source correspondence is in
[make/accept/trace](vector-library.wf) and [append/truncate](../../../../lib/std/collections/vector/grow-vector.wf).
[Rust](vector-ecosystem.rs) and [direct C](vector-costs.c) hoist constants
outside the cycle and keep the digest in a register. [C++](vector-ecosystem.cpp)
also hoists constant setup, but reloads constants and retains a zero-byte
`memmove` per nonempty cycle. These are observations of the saved scratch dump.

The same setup and handoff mechanisms occur in `native/neither-timed.disassembly.txt`;
their presence does not attribute T's measured adverse results. No new timing
was run. The [earlier inline packages](#counted-consumer-with-wide-tail-only-alwaysinline-completed-near-parity-but-rejected)
changed multiple mechanisms and include a paired regression; finding these
boundaries again does not revive those packages.

Two separate future diagnostics could test whether either mechanism contributes:

1. Move only the seven existing constant saves onto the cold growth edge,
   retaining the frame, literal loads, hot calls, payload work and consumer loop.
2. Pass the digest as a scalar value/result through the same two internal calls,
   retaining call depth, constructor setup and consumer work, only if a code
   screen isolates removal of the handoff without compensating spills.

The first diagnostic is tested below; digest handoff remains unmeasured.
Neither is an adopted production ABI proposal. Each needs an isolated code
change before a paired comparison; no qualified improvement would leave that
mechanism without a demonstrated material cost.

### Constructor-save placement: native-code discriminator

The [criterion](../ECOSYSTEM.md#next-discriminator-constructor-saves-on-the-growth-edge)
tests the first lead above using the same frozen `both` timed IR, source pin
and toolchain. This is a native attribution experiment, not a compiler change.
Seven paired stores of constructor constants move from the nonempty tail's
entry to immediately before its existing growth call. The literal loads,
288-byte frame, two hot calls, constructor, consumer and restores remain.
The 108-instruction multiset and 432-byte function extent are unchanged.
Internal branch retargeting and local instruction placement change with the
move; a timing difference would not isolate memory traffic alone.

Preflight rejected applying the edit to the ordinary accounting body: its
growth is inlined and its frame is 368 bytes. That observer also prevents
allocation elision present in timed code. The count-16 scalar suffix-zero
account row has two requests totaling 168 bytes; the timed disassembly has
one 152-byte request. The latter is a static inference, not a measured count.
The experiment therefore uses the optimized timed code as its only code
authority, with allocator call targets redirected after optimization solely
in the standalone correctness witness. No original account expectation or
ledger was changed, and no observer events were invented.

The IR-to-assembly-to-object control and its linked executable reproduce the
frozen originals byte for byte. Candidate object and linked changes are
confined to the target's instructions, plus linked UUID/signature metadata;
every symbol address, other function, section layout and unwind record stays
fixed. All 15 non-WF inputs to the timed link are reused (18 is the union of
timed and accounting inputs). The observer copies preserve all section bytes,
layout and unwind information, changing only allocator symbols and relocation
targets: 21 malloc, 24 free and eight calloc calls. They never enter timing.

A direct witness checks five capacity/length/removed triples at three seeds,
including repeated zero-capacity growth, a spare append before growth and
growth from 8191 to the 8193 ceiling. Its independent ordered digest, distinct
retained-prefix pattern, final shape, allocation extents/order/count, peak
and exact cleanup all agree between control and candidate. Root additionally
recomputed all 15 digests as a modular polynomial and checked the ledgers
against independently calculated extents. A release hook zeros q17–q30;
the growth helper returns without overwriting those clobbers, so moving the
saves after that call fails the ordered-digest check. Suppressing a growth
release fails the ledger; skipping final cleanup fails its separate check.

Both ordinary timed images pass the unchanged 1,260-configuration /
8,820-execution matrix and reject the checksum fault. The 25 continuation
phases have their expected statuses, including 14 deliberate fault exits.
This makes no claim about allocation-refusal coverage.

The first construction stopped on a screen-script false positive: its
linear scan included diagnostic calls after the release hook's normal return.
The actual successful continuation is fourteen clobbers followed by the GPR
epilogue and `ret`; the error block is reached only by earlier validation
branches. All 81 first-attempt artifacts, including that failure and runner,
were frozen. A bounded continuation corrected only the screen, demonstrated
rejection of an injected pre-return call, and completed the remaining links
and checks using the same objects. No candidate rebuild or code change
occurred. The continuation's 62 artifacts and the original 765 source inputs /
148 artifacts are frozen as well. Independent GPT-6 sol review and root
inspection passed these screens before admitting the single timing pair.

The single full pair ran control then candidate with unchanged
`measure 1048576 7`: [control samples](ecosystem-save-edge-control-samples.csv)
and [candidate samples](ecosystem-save-edge-candidate-samples.csv) each retain
4,116 rows, 588 groups and every sample 0–6. All non-time columns and checksums
match row for row. There was no rebuild, retry, selected-cell timing or
threshold change. Root independently recomputed all 42 paired cells' medians,
ranges, qualifications and native drift from the raw samples. The
[paired CSV](ecosystem-save-edge-paired.csv) uses the preceding factor trial's
rules and retains every cell; only its line endings were normalized.

Among 36 useful cells there are two qualified gains, 32 overlaps and two
unstable cells, with no strict paired losses. Every useful WF sample is at
least 1.219 ms. The unstable cells are scalar reserved/count 4096 and wide
suffix-one/count 256. All six suffix-zero controls overlap and remain unranked;
the three scalar controls have samples below 1 ms.

| Wide suffix-one count | Candidate/control median, cohort 0 | Cohort 1 | Qualification |
|---|---:|---:|---|
| 16 | 0.577088 | 0.587223 | Disjoint-range gain; spread 1.7563% |
| 256 | 0.564579 | 0.725420 | Unstable; spread 28.4885%, one cohort overlaps |
| 4096 | 0.642367 | 0.592423 | Disjoint-range gain; spread 8.4304% |

This identifies a material benefit from the bounded native placement change
in two short wide cycles. It does not explain the effect solely by the 224
stack bytes avoided: local instruction placement also changes. The suffix-two
and suffix-three cells overlap, so the result is not a uniform wide-value gain.
These are observations within one control/candidate execution pair, not
replicated fresh-process runs. Identical linked addresses do not establish
identical runtime ASLR or allocation addresses.

Useful native median drift (candidate/control) spans Rust 0.920305–1.056712,
C++ 0.907944–1.093898, reverse-C 0.915925–1.058361, direct-C 0.650017–1.570235,
swap/take-C 0.889605–1.089258 and take/swap-C 0.927927–1.046996. The large direct-C
extrema occur in scalar suffix-two/count 4096, in different cohorts. All these
controls have unchanged linked code and placement; their observed variation
must remain visible in interpretation.

The maintained standard-peer target reduction is control 8 pass / 0 deficit /
28 inconclusive and candidate 9 pass / 1 deficit / 26 inconclusive, with six
unranked controls each. These are this pair's results, not a replacement for
earlier trials or a current five-family total. Control's wide suffix-one cells
are inconclusive through instability, not established successes. Candidate's
wide suffix-one median ratios to the slower standard peer are respectively
0.977263/0.985498, 1.021195/0.986741 and 0.976792/0.998163 (cohorts 0/1 at
counts 16/256/4096), but every cell remains inconclusive by sample overlap.
The candidate's qualified deficit is scalar suffix-two/count 4096; paired WF
ranges overlap there, so that native-peer deficit is not an established
regression from this change. No standard-container target completion or
production optimization is selected. The next implementation question is a
general source/IR/lowering route to obtain this benefit, with its own code
screen and cross-program evidence, rather than shipping edited assembly or
recognizing this benchmark.

The [reproduction patch](native-constructor-save-edge.patch) retains the exact
seven-store move and compiled witness source; its assembly context belongs to
the pinned Apple Clang output. Reconstruct the frozen `both` arm using the
factor recipe above. Produce control assembly with `/usr/bin/clang -O3
-Wno-override-module -x ir -S whitefoot-timed.ll -o whitefoot-timed.s`, copy it
to a separate candidate directory, and apply the patch there with `patch -p1
-F 0`. Assemble both with the same flags and `-x assembler -c`; link each by
substituting only its WF object into the original timed link command.
For the witness copies only, replace `_malloc`, `_free` and `_calloc` BL targets
by `_wf_probe_malloc`, `_wf_probe_free` and `_wf_probe_calloc`, then assemble
without IR reoptimization. Compile `save-edge-witness.c` once with `-std=c11
-O2 -Wall -Wextra -Werror`; link it with each observed WF object and the frozen
runtime inputs, omitting the ordinary driver and Rust/C++ adapters. `check`
must reproduce the [15-row witness ledger](save-edge-witness-accounting.csv).
The source enumerates the individual fault modes; placing the same stores
after the growth call supplies the preservation fault. Root replayed the
tracked patch and recovered the screened assembly and witness byte for byte.
These retained experimental inputs retire with this discriminator; they do
not add a production assembler transform or a new maintained compiler path.

Scratch evidence is under `vector-library/.build/constructor-save-edge/` in
`/private/tmp/whitefoot-vector-actual-ablation`; the first and continuation
`result.json`, `phase-times.json` and artifact manifests retain both outcomes.
`measurement-1/` retains its command recipe, reducer, all reductions and
25 frozen artifacts. The first 81 and continuation 62 artifacts, and original
765 source inputs / 148 artifacts, still match after timing. Reading the
tracked CSVs reproduces the arithmetic, not the prior native execution.

| Phase | Seconds | Result |
|---|---:|---|
| First native construction | 0.985327 | All subprocesses exit 0 |
| First native screen | 1.284286 | Script stops on the documented false positive |
| Continuation linking | 0.173316 | Exit 0; no recompilation |
| Continuation native screen | 0.124367 | Exit 0; pre-return-call falsifier rejects |
| Correctness execution | 2.222965 | Both complete matrices and 15-case witnesses pass |
| Fault controls | 0.353768 | All 14 expected failures observed |
| Control timing | 84.928337 | Exit 0 |
| Candidate timing | 83.163108 | Exit 0 |
| Four maintained reductions | 0.251381 | All exit 0 |
| Paired reduction | 0.088852 | Exit 0; independently recomputed |

Primary raw sample SHA-256: control
`1927002fdc6fa5b31eaf504b9b236734c9bc02ea887eabeb35319c751e3ff2e3`,
candidate `5463f1f7367bbf62f7664aaabff914e9f09612877a0844e95a39631389a653ad`.
The reproduction patch is
`5a9954cdcbab5e81471081e88278d192036a1ad8e6e09924f5fe9802b2856267`;
the retained witness ledger is
`3eeb59d1360691ee9c9d7b0b122ffa3f62ad42c9b9512e32176c3815bf2d9cb9`.

### Branch-weight growth-edge discriminator: qualified behavior, timing inconclusive

The [save-placement result](#constructor-save-placement-native-code-discriminator)
left a general compiler route open. One diagnostic attached LLVM `!prof`
weights `1, 2000, 1` to the invalid, spare and full successors of the wide
make-room switch. The [two-hunk IR patch](branch-weight-cold-edge.patch) applies
to frozen `both/ecosystem/whitefoot-timed.ll` SHA-256
`8038674ab44bfabb89fa9d985dfe5ecb271157b1bc2c40df0b10b0dd44eecde6`
from source pin `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`. The fixed ratio
is a diagnostic choice, not an observed frequency. It changes no path, call,
value, proof or allocation, and selects no policy.

Using `/usr/bin/clang -O3 -Wno-override-module -x ir -S` and the same flags
with `-x assembler -c`, the unpatched object reproduces frozen SHA-256
`c7fc3dcf0d127d3586678f0c8aaabe5848da38a1e427f35c4f729261ba50e235`;
the candidate is `3d8317e9ad07defb693e83a4bccd2c4be2b958ecf0de2848ebd25e70e1238a8b`.
Only six wide WF function bodies change (trace, append, tail, work, make-room,
insert), with the same maximum frames and call/tail-call multisets. Scalar
trace/tail remain byte-identical. Wide tail moves seven Q-pair saves under
the full-capacity branch before `grow_full`; direct-to-backing construction,
restores, truncate and its 288-byte frame remain. Changed placement elsewhere
prevents interpreting this as an isolated stack-traffic percentage.

Both linked arms reused the frozen non-WF inputs and passed the unchanged
1,260-configuration/8,820-execution ordinary matrix and checksum fault.
Separate allocator-observed assembly preserves code and relocation meaning
after reversing only the symbol substitutions. The same independent 15-case
ordered-digest/owner ledger passes both arms; q17–q30 release clobbers reach
the growth return, while misplaced saves and the cleanup, release and seven
witness-field faults fail as expected. Runner-only bookkeeping, screen and
path corrections were preserved before the complete qualification; no native
candidate or criterion changed. No timing ran during qualification.

One full guarded `measure 1048576 7` pair ran control then candidate once,
81.354/80.893 s. [Control](ecosystem-branch-weight-control-samples.csv) and
[candidate](ecosystem-branch-weight-candidate-samples.csv) each retain 4,116
rows, 588 groups, both cohorts, all seven samples and six unranked suffix-zero
controls; non-time columns match exactly. Under the unchanged paired rules,
36 useful cells have **zero qualified gains or losses**, 34 overlaps and two
unstable cells. The wide suffix-one results are:

| Count | Candidate/control medians, cohorts 0/1 | Qualified result |
|---:|---:|---|
| 16 | 0.588678 / 0.592479 | Overlap: cohort 0 separates, cohort 1 overlaps after a retained 29.772 ms candidate sample |
| 256 | 0.755899 / 0.541548 | Unstable (>10% ratio spread) |
| 4096 | 0.625171 / 0.553925 | Unstable despite a raw two-cohort gain |

Standard-peer totals move from control 15 pass/1 deficit/20 inconclusive to
candidate 16/2/18, plus six unranked each. All three candidate wide
suffix-one median ratios fall below the selected slower peer (0.969–0.993),
but sample overlap leaves all three inconclusive. At count 16, candidate
medians are `1.039/1.041` times Rust and `0.977/0.977` times C++ across cohorts.
Control's wide/count-16 deficit becomes inconclusive; candidate deficits are scalar growth/count 16
and scalar suffix-two/count 256 (roughly 1.06 times the slower peer), neither
a qualified paired WF loss. Comparator drift includes direct C's useful
median ratio range 0.673–1.021. The raw sample SHA-256 values are control
`816b08972cbbbe069bd13d2d7ce14444f8860ea4aa3fa0ea8fb638c8b31704d6`
and candidate `b32bdcf945d7393ff72f3765902bc8e698711d009eb26fcdc0ee93eced60d434`.
No sample was dropped or timed again; the older-lineage signal does not
complete the Vector peer target or select a general branch hint.

A separate unchanged-IR screen added only `-mllvm -sink-insts-to-avoid-spills`
(the installed default is off). Candidate assembly and object were byte-identical
to control, so this option supplies no placement on this input/toolchain. A
general branch hint would need type/name-independent structural grounds and
measurement on growth-dense short-lived, exact-reserve and ceiling paths as
well as spare-heavy repeated insertion; 2000:1 is not universally expected.

### Ordinary spill splitting: unchanged target, rejected before execution

The [registered code-only screen](../ECOSYSTEM.md#next-discriminator-ordinary-register-allocation-spill-splitting)
used frozen `both` timed IR at O3 with Apple Clang 21.0.0 (`clang-2100.3.34.2`).
The installed option dumps establish control `speed (default: speed)` and
candidate `default (default: speed)` for `-mllvm -split-spill-mode=default`.
This tests the ecosystem's O3 build, not ordinary whitefootc's O2 default.

The target fails the screen: wide `vector_library_tail_work` remains exactly
108 instructions with a 288-byte frame and all seven unconditional saves.
Its literal loads, vectorized direct-to-backing payload, grow/truncate calls,
restores and CFI are unchanged. Both requested MIR snapshots were produced;
the combined before-greedy/after-virtual-register-rewriter dumps are
byte-identical between arms. Before greedy there are no spill-slot `STRQui`
instructions; after rewriting, fourteen already occupy the loop preheader.
This locates their introduction within that pass interval, not one exact pass.

All 57 emitted WF definitions were inspected. Only wide `vector_library_work`
changes: `lsr x19, x23, #1` moves before `str x23, [sp, #176]`, exchanging two
adjacent instructions at linked addresses `0x10000cb50` and `0x10000cb54`.
It retains 441 instructions, its 688-byte frame, calls and CFI. Section layouts,
symbol addresses, relocation meanings and unwind records match; every other
image byte matches after UUID/signature masking. The control object and image
are byte-identical to frozen `both`, and all 15 non-WF link inputs are unchanged.

All 11 subprocesses exit 0; the screen and global guard exit 1 for the failed
target criterion. The guard reports 2.50 seconds. Recorded phase costs are:

| Phase, both arms | Seconds |
|---|---:|
| Tool identity | 0.023153 |
| LLVM code generation | 0.324445 |
| Assembly | 0.114754 |
| Linking | 0.127043 |
| Native disassembly | 0.852760 |
| Final code comparison | 0.065385 |

No correctness execution, timing, retry or alternative option followed.
The original 765 source inputs / 148 artifacts and prior save-edge 81 / 62 / 25
artifact sets match before and after. The new 53-artifact freeze is under
`vector-library/.build/split-spill-default-screen/` in
`/private/tmp/whitefoot-vector-actual-ablation`; `option-evidence.json`,
`assembly.diff`, `phase-times.json` and `result.json` retain the evidence.
The criterion retains exact reproduction commands and linked output paths.
Root independently confirmed the findings. This option is rejected for the
requested mechanism; no compiler or language policy changes.

### Checked append within reserved capacity: useful regressions prevent selection

The [registered source comparison](../ECOSYSTEM.md#next-discriminator-checked-append-into-reserved-capacity)
passes source, native and correctness screens but fails selection on its one
complete timing pair. Four useful cells regress. The checked operation and
suffix rewrite are not selected by this result; no language, compiler or public
library policy changes.

The [reproduction patch](checked-append-reserved.patch) changes three source
files: a generic `grow_vector_append_reserved` requires `len < cap` and
`len < ceiling`, uses ordinary `place_back`, and preserves capacity; suffix
callers supply the capacity contract and loop invariants. Growing/mixed callers
retain ordinary append. Compiler source changes are zero. The control is frozen
`both` at `36e27e57f46ff4f5de3fc9da155ca33a2254fe7b`; the candidate applies the
patch to `08bc92e88fe54c689022f075668c681443a40ef2`. Compiler and affected source
paths have no committed difference between those revisions. Rebuilding the
embedded library produced actual whitefootc `75df1f29fb618deb40cdab0d10fd29071bbd9851f58f757ea49bb7b10f3e8a5a`.

The retained fixtures exercise scalar, 256-byte nocopy and owning nodrop values
at counts zero and three (exact fit), independently check every payload word in
order, and keep a prefix reference across append. Missing capacity and ceiling
requirements each reject with `FN-8`; missing length effects reject with
`EFF-2`. Reversed record words preserve the sum but make execution exit 4.
The existing allocation observer records fourteen allocations and fourteen
releases: six empty backings, three positive reserves, three boxed Tickets,
and the prefix witness's empty backing plus reserve. Each observer fault exits 1;
each missing source obligation also exits 1. One initially
redundant explicit proof block was removed after `PRF-1 RedundantUseBlock`:
AUTO already proved room. No rule or expected verdict changed.

Apple Clang 21.0.0 (`clang-2100.3.34.2`) compiles the harness LLVM at O3. The
candidate timed object exactly equals the object assembled from the independently
inspected `.s`. Three existing global WF bodies change and two leaf definitions
are added; the other 54 global bodies and two local resource helpers retain
their normalized instruction streams. Scalar tail becomes a 66-instruction
frameless leaf (previously 49 instructions, 64-byte frame). Wide tail loses
growth and the seven old unconditional Q-pair saves, but grows 108→258
instructions: its 176-byte frame includes 64 bytes of callee-save D registers
and 96 bytes of spills on the vector path. Truncate remains. Scalar trace
shrinks 229→206 instructions and 160→112 frame bytes, losing three static
malloc calls and one memmove. The two new leafs contain 7/22 instructions.
Timed/account WF text grows 692/436 bytes; 3,073/3,105 linked symbol addresses
move. This is a source/library comparison with changed code placement, not
isolated spill traffic; process ASLR and data addresses are not controlled.

All fifteen non-WF inputs per image retain their exact frozen bytes. Both arms'
timed/account matrices each pass 1,260 configurations / 8,820 executions.
Four checksum negatives and two cleanup negatives exit 1 with the exact expected
diagnostics. Both complete 294-row ledgers equal the existing
[factor-accounting CSV](ecosystem-compiler-factor-accounting.csv), SHA-256
`ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`;
these are instrumented lifecycle counts, not timed allocation traffic.
The first linked screen mistakenly compared weak `_wf__floor_run` with its
strong runtime replacement and stopped before checks. A preserved continuation
verified that same singleton resolution in both arms, rejected a nonweak-function
exception, and completed the checks without rebuilding or relinking.

The [control samples](ecosystem-checked-append-reserved-control-samples.csv),
[candidate samples](ecosystem-checked-append-reserved-candidate-samples.csv) and
[complete paired cells](ecosystem-checked-append-reserved-paired.csv) retain both
cohorts, all seven samples, all 42 cells and 4,116 rows per arm from exactly one
control→candidate `measure 1048576 7` pair. No rows or outliers were removed.

| Useful-cell outcome | Count | Cells |
|---|---:|---|
| Qualified gain | 6 | Scalar suffix-1 and suffix-3, all three counts |
| Qualified loss | 4 | Wide suffix-2 at 16/256/4096; wide suffix-3 at 256 |
| Overlap | 20 | Both widths' reserved/growth/reuse; wide suffix-3 at 16/4096 |
| Sub-1-ms | 3 | Scalar suffix-2 at every count; raw gains remain unqualified |
| Unstable | 3 | Wide suffix-1 at every count; maximum cohort spread 38.43% |

Scalar suffix-1 candidate/control median ratios span 0.446–0.461; suffix-3
spans 0.785–0.815. Wide suffix-2 losses span 1.055–1.069, and wide suffix-3
at 256 spans 1.026–1.048. Useful target pass/deficit/inconclusive counts are
15/5/16 for control and 17/3/16 for candidate. Among six unranked suffix-zero
cells, the three scalar cells are short and all three wide cells lose, with
ratios 1.526–1.652. Native useful-cell median drift ranges are reverse-C
0.979–1.062, direct-C 0.547–1.038, swap-take-C 0.937–1.156, take-swap-C
0.981–1.085, Rust 0.955–1.102 and C++ 0.976–1.164. These observations do not
establish standard-container target completion or an unreserved-call result.

Recorded stage costs: compiler build 9.725866 s; frontend emission 0.157408 s;
native assembly emission 0.160205 s; three native object commands 0.430535 s;
two links 0.121440 s; complete matrix checks 2.697221 s; harness faults
0.026614 s; ledger runs 0.072479 s. The timing runs take 81.203787/82.559742 s,
with 0.256440 s for four maintained reductions and 0.094822 s for paired
reduction. Check wall times are not performance observations. Exact historical
argv, statuses, costs and hashes remain in the local prototype audit at
`/private/tmp/whitefoot-append-reserved-prototype/measurement/`. The original
765 source / 148 native artifacts and the 47 construction, 32 continuation and
24 timing artifacts match their recorded freezes before and after timing.

To reconstruct inputs, use an isolated checkout of the candidate base, apply
the replay patch from the repository root with `patch -p1 -F 0`, and rebuild
`cargo build --manifest-path compiler/Cargo.toml --profile gate --locked
--offline --bin whitefootc --jobs 2` under the shared guard. The patch also
creates the five exact witnesses in this family's ignored
`.build/checked-append-reserved-witness/`. Emit raw LLVM with that compiler's
`--emit-llvm vector-library.wf -o vector-library.raw.ll`; use the existing
[Makefile](Makefile)'s main/allocator substitutions for timed/account LLVM.
Compile each with `/usr/bin/clang -O3 -Wno-override-module -x ir -c INPUT -o
OUTPUT`; emit the inspected timed `.s` with `-S` instead of `-c`, then assemble
it with `-x assembler -c` and require object equality. Reconstruct frozen
`both` peers using the factor recipe above and the existing native Makefile
rules, then preserve its exact clang++ link flags/order, substituting only each
candidate WF object and output. The historical commands reused all fifteen
non-WF inputs per image. Run `check` and `fail-checksum` in both modes for both arms,
plus `account` and `fail-cleanup` in each accounting image. Any new performance
execution needs its own authorization; the retained pair used
`measure 1048576 7`, then `../summarize-ecosystem.pl --complete` and `--targets`
on each raw CSV and the previously recorded paired-factor method. A paired
direction requires disjoint observed ranges in both cohorts; qualification
requires every paired WF sample at least 1 ms and cohort median-ratio spread
at most 10%. Replaying CSV arithmetic does not replay past native execution.

For the independent fixture observer, emit `acceptance.ll` and substitute
`@malloc(` → `@wf_observe_allocate(`, `@free(` → `@wf_observe_release(` and
`@main(` → `@wf_fixture_main(` everywhere, with no added allocation attributes.
Compile that IR with the existing
`tests/programs/containers/container-allocation-observer.c` using `/usr/bin/clang
-O3 -std=c11 -Wall -Wextra -Werror -Wno-override-module -I compiler/src/backend`,
the twelve `NATIVE_OBJECTS` from `compiler/runtime.mk`, and `-pthread -lm`.
These are the same frozen runtime objects used by the ecosystem links; their
reconstruction is owned by that Makefile. The executable with no arguments
must print fourteen allocations, each released exactly once, and exit 0;
`missing-release`, `double-release` and `foreign-release` must each exit 1 with
their respective complete-release, duplicate-release and foreign-address
diagnostics. Acceptance and the reversed-order fixture are also compiled by
ordinary whitefootc (its O2 default), with exits 0 and 4 respectively. This
separates the checked callable witness from the O3 timing harness.

### Ordinary controller composition: scalar suffix-three losses prevent selection

The [registered source comparison](../ECOSYSTEM.md#ordinary-vector-controller-composition-criterion)
is rejected after its single complete timing pair. Three qualified useful cells
regress: scalar suffix-3 at every count. Four other useful cells improve, but
the criterion refuses any qualified useful-cell loss. The production fixture,
compiler, library and native adapters are unchanged; this result selects no
source rewrite or compiler policy.

The exact [source patch](source-controller-composition.patch) applies to
`7abd6bb34746b7b983b83103c68ee429b24859bb`. It substitutes the private tail
helper's body into the suffix cycle, renaming locals and retaining ordinary
append, unconditional truncate (including zero removal), ownership order and
the original helper declaration. All other source bytes match. Independent
source review reconstructed that substitution and matched the complete file
and patch. The whole source archive and frozen compiler are identified in the
[identity and phase record](ecosystem-source-controller-identities.json).
The compiler SHA-256 is
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`;
it reproduces retained terminal/T1H0 raw LLVM and timed object bytes before
the source change. No compiler rebuild was part of this experiment.

The original code-screen recommendation was to stop under the phrase
"comparable hot spills": seven Q-pair reloads still transfer 224 bytes after
each nonempty cycle's truncate. That recommendation remains preserved. Before
linking, correctness execution or timing, root and independent review recorded
the ambiguity and the prospective continuation in
`22096b01dbd508d47643a1211a8feb950b26a3f1`. The candidate removes each cycle's
14 ADRP instructions, 14 Q literal loads, seven Q-pair stores and wide tail
call/frame, while retaining those seven reloads. Comparing the complete
repeated setup admitted the bounded continuation; it did not establish a
performance gain or relax correctness, target, stability or no-loss criteria.

Ordinary Clang O3 changes the two trace bodies. The baseline scalar trace
already inlines its tail call; direct source composition changes its CFG and
register use, with 229→223 static instructions and the same 160-byte frame.
The wide trace loses its tail call, grows 190→345 instructions and uses a
400-byte frame instead of 352 bytes; the separate 288-byte tail frame
disappears. Within-record SIMD and direct backing stores remain. There is no
two-record ZIP loop or owner-sized snapshot. Two unused tail definitions
disappear; the other 55 common global/local bodies retain their instruction
streams after local numbering normalization, with literal values checked
separately. Work, round and truncate bodies are among those unchanged bodies.

Timed object text shrinks 10,540→10,508 bytes and literal storage 912→688
bytes. Linked timed/account text shrinks 32/336 bytes. The C trace addresses
stay fixed; Rust/C++ trace addresses move by −32 bytes in the timed image and
−336 bytes in the accounting image. This is a source composition comparison
with changed code placement, not an isolated instruction-cost measurement;
runtime ASLR and data placement are uncontrolled. Each link reuses the exact
fifteen frozen non-WF inputs, whose source/flag identities were checked.
Linked inspection checks every unrelocated WF instruction byte and every
WF/local branch relocation against the screened objects. The sole weak
`floor_run` resolves to the same frozen strong runtime definition. Neither
timed image contains allocation-observer hooks.

All four timed/accounting images pass 1,260 configurations / 8,820 executions
each. Four checksum faults and two cleanup faults exit 1 with the exact
expected diagnostic; all other six harness commands exit 0. Both complete
294-row ledgers equal the retained
[factor accounting](ecosystem-compiler-factor-accounting.csv), SHA-256
`ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`.
These describe instrumented-image lifecycle counts, not timed allocation
traffic. All 48 frozen source/compiler/code/input/image identities match
before and after timing.

The [control samples](ecosystem-source-controller-control-samples.csv),
[candidate samples](ecosystem-source-controller-candidate-samples.csv) and
[all paired cells](ecosystem-source-controller-paired.csv) retain exactly one
control→candidate `measure 1048576 7` pair: both cohorts, all seven samples,
42 cells and 4,116 rows per arm. Paired non-time fields match exactly. No
sample or outlier was removed. Both direct measurement exits and the guard
exit are 0; both measurement stderr files are empty.

| Useful-cell outcome | Count | Cells |
|---|---:|---|
| Qualified gain | 4 | Scalar suffix-2 at 16/256; wide suffix-1 at 256/4096 |
| Qualified loss | 3 | Scalar suffix-3 at 16/256/4096 |
| Overlap | 27 | All 18 reserved/growth/reuse cells; remaining qualified suffix cells |
| Unstable | 2 | Wide suffix-1 and suffix-3 at 16 |
| Sub-1-ms | 0 | Minimum useful paired WF sample is 1.202 ms |

| Useful separated cell | Candidate/control medians, cohorts 0 / 1 | Qualification |
|---|---|---|
| Scalar suffix-2, 16 | 0.847438 / 0.858209 | Gain |
| Scalar suffix-2, 256 | 0.846774 / 0.850746 | Gain |
| Wide suffix-1, 16 | 0.727289 / 0.641560 | Unstable; 13.36% cohort spread |
| Wide suffix-1, 256 | 0.639763 / 0.658220 | Gain |
| Wide suffix-1, 4096 | 0.558696 / 0.523950 | Gain |
| Scalar suffix-3, 16 | 1.064140 / 1.067301 | Loss |
| Scalar suffix-3, 256 | 1.174453 / 1.188459 | Loss |
| Scalar suffix-3, 4096 | 1.188501 / 1.178649 | Loss |

For all three scalar suffix-three losses, each candidate minimum exceeds the
corresponding control maximum in both cohorts. Their median-ratio cohort
spreads are 0.30%, 1.19% and 0.84%. The wide suffix-three/count-16 cell has
22.94% spread and remains unstable, not a qualified direction.

| Unranked suffix-zero control | Candidate/control medians, cohorts 0 / 1 | Paired observation; remains unranked |
|---|---|---|
| Scalar, 16 | 0.990918 / 1.016260 | Short; ranges overlap |
| Scalar, 256 | 1.005086 / 1.031282 | Short; ranges overlap |
| Scalar, 4096 | 1.013131 / 1.011022 | Short; ranges overlap |
| Wide, 16 | 0.582180 / 0.569130 | Separated gain |
| Wide, 256 | 0.571738 / 0.563567 | Separated gain |
| Wide, 4096 | 0.588930 / 0.583367 | Ranges overlap |

The historical tail-only `alwaysinline` falsifier remains explicit: it lost
wide suffix-2/count-16 with ratios 1.034870 / 1.027378. The new ordinary-source
pair gives 1.029621 / 1.025312 at that cell. Cohort 0 loses with disjoint
ranges; cohort 1 overlaps, so the complete new pair is overlap, not a new
qualified loss or a reversal of the historical refusal.

Native useful-cell candidate/control median drift spans reverse-C
0.958–1.178, direct-C 0.732–1.142, swap-take-C 0.974–1.121, take-swap-C
0.866–1.139, Rust 0.894–1.163 and C++ 0.953–1.140. The complete per-cell
ratios remain in the paired CSV; they are not used to rescale WF times.
At the three scalar suffix-three losses specifically, Rust ratios span
0.989–1.035 and C++ 0.994–1.036. Useful target pass/deficit/inconclusive counts
are 10/2/24 for control and 9/0/27 for candidate, plus six unranked controls
each. The disappearance of two descriptive deficits does not complete the
target: 27 candidate useful cells remain inconclusive.

Source check/emission cost 0.650245/0.152705 s. Timed inspection emission,
timed object compilation, accounting object compilation and two links cost
0.525740/0.149678/0.154984/0.116855 s respectively. Positive matrix checks,
fault controls and ledger runs cost 2.760299/0.029313/0.073424 s. The two
timing arms cost 82.613026/84.462512 s; the guard reports 167.30 s total.
Four maintained reductions cost 0.248318 s and the unchanged paired reducer
0.075543 s. These phase costs are separate from cell performance results.

Reproduction starts from a complete archive of the source revision above,
applies the exact patch with `patch -p1 -F 0`, then uses ordinary `--check`
and `--emit-llvm` with the frozen compiler. The identity record contains exact
historical commands and statuses. Timed LLVM changes only the main symbol;
account LLVM uses the existing Makefile's observer substitutions. The native
commands use `/usr/bin/clang -O3 -Wno-override-module -x ir` and the recorded
native peer/runtime object order. The original screen, continuation, linked
inspection and full logs remain at
`/private/tmp/whitefoot-vector-source-controller/`; the rejected candidate's
source and binary inputs remain frozen there. CSV-only reproduction uses
`../summarize-ecosystem.pl --complete` and `--targets` per arm and the same
recorded paired rule: disjoint observed ranges in both cohorts, every paired
WF sample at least 1 ms, and at most 10% cohort median-ratio spread. No further
native execution, source repair or timing pair was run after this refusal.

#### Read-only scalar suffix-three attribution

The preserved scalar IR and assembly exclude a vectorizer/remainder explanation:
neither trace contains vector-typed IR or loop-vectorization metadata. Both
no-growth append loops have a three-instruction capacity test and a
seven-instruction append step; both digest loops have six instructions per
element. The raw trace attributes are identical and the baseline tail call was
already inlined. In the retained scratch, `screen/control.opt.ll:562` carries
backing pointer and length across the outer latch as PHIs, whereas
`screen/candidate.opt.ll:490` reloads both each cycle and line 603 writes the
owner pointer after every inner append loop. This appears directly at
`control.s:625` and `candidate.s:623,706`. Counting the complete no-growth
cycle gives `21 + 16 × removed` versus `23 + 16 × removed` instructions,
excluding prefix construction and final drain. That is 69→71 for suffix-three,
not a claim that two instructions explain the 6–19% loss: suffix-two gains
despite the same extra two instructions, and placement, dependencies and branch
behavior remain possible contributors.

Baseline inlining adds the helper's ordinary parameter `noalias` scopes
(`!15` for values, `!17` for digest) across append/truncate. The call's two
write-capable reference roots are checked disjoint, and neither reference
escapes. Direct composition removes that call boundary and its enclosing
scopes; the independent append/truncate scopes remain. This accompanies the
changed promotion but does not isolate its cause or establish a compiler
defect. It supplies no basis for reviving per-access alias metadata, whose
provenance/lifetime obligations remain refused by
[backend-facts](../../../../design/compiler/backend-facts.md).

The subsequent source discriminator uses a normal generic helper containing
the whole cycles loop, taking the existing vector by reference and returning
the checksum by value. It keeps ordinary append/truncate, zero removal, source
work and ownership order, leaving reserve/prefix/final drain outside. This is
ordinary batch-operation decomposition: one reference boundary per batch keeps loop setup in the same
callee and supplies existing reference facts, with no API, hint, metadata or
payload-specific branch. It is not a compiler scope-fact repair. The registered
code screen required outer pointer/length promotion while keeping wide preparation
outside the repeated cycle and adding no hot call or snapshot; failure would
have stopped the idea before timing. The same complete correctness, target and
useful-cell no-loss requirements apply. The completed comparison below retains those
requirements and rejects the candidate after its single timing pair.

### Ordinary batch helper: scalar suffix-three/count-16 loss prevents selection

The ordinary reference-taking batch helper is rejected under the registered
no-qualified-useful-loss rule. Scalar suffix-3/count-16 regresses by 3.6–4.0%
with separated sample ranges in both cohorts. Five useful cells improve, but
they cannot compensate for that loss. This is an equivalent caller/source
decomposition comparison, not a change to GrowVector's public operations or a
promise that all clients improve. The production caller, compiler, library,
native adapters and specification remain unchanged.

The [exact source patch](source-batch-controller.patch) applies to
`7abd6bb34746b7b983b83103c68ee429b24859bb`. It moves the already inspected
suffix cycles into one private generic helper taking the vector by reference
and checksum by value, returning the checksum. Reserve, prefix construction,
final drain/free and other branches remain outside. Ordinary append and
unconditional truncate, zero removal, zero rounds, fresh per-cycle digest,
wrapping arithmetic and construction/consumption order remain unchanged.
The old tail declaration remains in source. No type/count threshold, new API,
attribute, LLVM mutation or compiler switch is introduced.

The [identity and phase record](ecosystem-batch-controller-identities.json)
retains the prospective source/native and timing criteria, exact compiler,
source, patch, peer/runtime inputs, commands, direct statuses and artifact
hashes. The frozen compiler remains SHA-256
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`.
No compiler rebuild or archive extraction is part of this trial. Standalone
baseline emission exactly reproduces retained terminal/T1H0 raw LLVM,
`22e03e27bba9dd075d7d6f86a40e52b2f0caea746c36b50e60424aa11527a73d`,
qualifying the minimal input method with the compiler's embedded library.

The initial source check rejects an equality-form helper entry requirement
under FN-8: ENT-6's positive affine route handles ordered comparisons without
inventing an equality rewrite. The retained failed source and diagnostic are
followed by the original tail helper's equivalent `>=` and `<=` requirements.
This authoring correction changes neither predicate nor runtime body, weakens
no obligation and changes no language rule. The repaired source admits and
emits using the unchanged compiler.

The native structural prediction passes. Scalar backing pointer and length
again cross the outer latch as PHIs. The rejected direct expansion's
unconditional owner-slot store and start-of-cycle owner/length reloads vanish;
backing/length reloads before truncate remain, as in baseline. There are no
scalar helper allocas. Wide constructor constants and seven Q-pair stores
occur before the outer loop, while seven Q-pair reloads still transfer 224
bytes after every nonempty cycle's truncate. The only wide helper alloca is
the eight-byte digest. Direct backing stores and one-record SIMD remain;
there is no owner-sized snapshot, two-record ZIP loop or new per-cycle call.
Wide truncate remains out of line. One batch call occurs per suffix trace,
including zero rounds.

| Native timed body | Baseline instructions / frame | Candidate instructions / frame |
|---|---:|---:|
| Scalar trace | 229 / 160 B | 117 / 112 B |
| Wide trace | 190 / 352 B | 181 / 336 B |
| Scalar batch helper | Absent | 130 / 144 B |
| Wide batch helper | Absent | 183 / 368 B |
| Scalar tail definition | 49 / 64 B | Removed |
| Wide tail definition | 108 / 288 B | Removed |

The baseline scalar tail was already inlined into trace. Peak suffix frames
are therefore at least 160→256 bytes for scalar and 640→704 bytes for wide,
excluding truncate. The nonempty scalar hot cycle remains
`21 + 16 × removed` instructions in baseline versus `23 + 16 × removed` in
candidate: 69→71 for suffix-three. Recovered PHIs do not by themselves repair
the earlier loss. Fifty-five other common global/local function bodies,
including work, round and truncate, retain their normalized instruction
streams and literal values. Total timed object instructions grow 2,635→2,670;
text grows 10,540→10,680 bytes, with literal storage unchanged at 912 bytes.

Each candidate link reuses the exact fifteen frozen non-WF inputs, with all
59 relevant native source/header/build-rule files matching both retained
source trees. The qualified baseline images are reused byte for byte. Linked
WF instructions, local/global branch destinations and 57 literal loads per
image are checked against their objects; the known weak floor function resolves
to the frozen strong runtime body. No timed image contains observer hooks.
Linked timed/account text changes by +140/−72 bytes. C trace addresses remain
fixed; Rust/C++ trace addresses shift by +140/−72 bytes respectively. Code
placement and runtime ASLR remain possible contributors; this is not an
isolated two-instruction cost measurement.

All four images pass the full 1,260-configuration / 8,820-execution matrix,
including zero removal and zero rounds. Four checksum and two cleanup controls
exit 1 with the exact expected diagnostics. Both 294-row ledgers match the
retained [factor accounting](ecosystem-compiler-factor-accounting.csv), SHA-256
`ab3dd14d3e73fe437baac27dd09c88e59982e172902d3478378c8eb9a0d011b7`.
These are instrumented-image lifecycle observations, not optimized timed
allocation traffic. All 290 frozen inputs match before and after timing.

The complete [control](ecosystem-batch-controller-control-samples.csv),
[candidate](ecosystem-batch-controller-candidate-samples.csv) and
[paired cells](ecosystem-batch-controller-paired.csv) retain one
control→candidate `measure 1048576 7` pair: 42 cells, both cohorts, all seven
samples and native variants, 4,116 rows per arm. Non-time fields match exactly;
no sample or outlier is removed or native-scaled. Both direct timing exits and
the guard exit are 0, with empty measurement stderr.

| Useful-cell outcome | Count | Cells |
|---|---:|---|
| Qualified gain | 5 | Scalar suffix-2 at 16/256/4096; wide suffix-1 at 256/4096 |
| Qualified loss | 1 | Scalar suffix-3 at 16 |
| Overlap | 29 | All other useful cells except the unstable cell |
| Unstable | 1 | Wide suffix-1 at 16 |
| Sub-1-ms | 0 | Minimum useful paired WF sample is 1.198 ms |

| Separated useful cell | Candidate/control medians, cohorts 0 / 1 | Qualification |
|---|---|---|
| Scalar suffix-2, 16 | 0.843408 / 0.844867 | Gain |
| Scalar suffix-2, 256 | 0.849885 / 0.866051 | Gain |
| Scalar suffix-2, 4096 | 0.857719 / 0.846287 | Gain |
| Scalar suffix-3, 16 | 1.039735 / 1.035977 | Loss |
| Wide suffix-1, 16 | 0.631024 / 0.440731 | Unstable; 43.18% cohort spread |
| Wide suffix-1, 256 | 0.479531 / 0.446617 | Gain |
| Wide suffix-1, 4096 | 0.449277 / 0.448534 | Gain |

At the qualified loss, baseline ranges are 1.358–1.368 / 1.361–1.369 ms;
candidate ranges are 1.409–1.471 / 1.408–1.426 ms. The median-ratio cohort
spread is 0.363%. Rust ratios are 1.002308 / 1.000000, and C++ ratios are
1.000000 / 0.999424. Scalar suffix-three at 256/4096 remains overlap, with
ratios 1.034457 / 1.003554 and 1.034332 / 1.035062. Those are not qualified
reversals of the preceding trial's three losses. The historical tail-only
wide suffix-two/count-16 falsifier also remains explicit: this pair overlaps
at 0.999723 / 0.999028, without overturning the earlier qualified refusal.

| Unranked suffix-zero control | Candidate/control medians, cohorts 0 / 1 | Paired observation; remains unranked |
|---|---|---|
| Scalar, 16 | 0.996948 / 0.998986 | Short; ranges overlap |
| Scalar, 256 | 1.002041 / 0.994944 | Short; ranges overlap |
| Scalar, 4096 | 1.006110 / 1.000000 | Short; ranges overlap |
| Wide, 16 | 0.572871 / 0.575691 | Separated gain |
| Wide, 256 | 0.571990 / 0.569752 | Separated gain |
| Wide, 4096 | 0.596114 / 0.597349 | Separated gain |

Useful native median drift spans reverse-C 0.952–1.052, direct-C 0.661–1.088,
swap-take-C 0.967–1.049, take-swap-C 0.966–1.120, Rust 0.962–1.071 and C++
0.966–1.028. Every per-cell ratio is retained. Useful standard-peer target
pass/deficit/inconclusive counts change 15/6/15→23/1/12, plus six unranked
controls each. Scalar growth/count-16 remains a qualified target deficit;
the candidate neither completes the target nor meets the no-loss criterion.

The failed source check costs 0.101518 s; qualified baseline emission costs
0.148946 s. Repaired source check/emission cost 0.141186/0.134907 s, timed
optimized-IR/assembly emission 0.213665 s and timed object compilation
0.134915 s. Account object compilation and two links cost 0.151718/0.120559 s.
Positive matrices, negative controls and ledger executions cost
2.718769/0.028879/0.072334 s. The control/candidate timing arms take
80.885034/79.702437 s; the guard takes 161.001184 s. Four maintained reductions
cost 0.247307 s and the unchanged paired reducer 0.075574 s. Whole-arm and
check costs are not cell performance results.

Reproduction uses the source/compiler pin and zero-context publication patch
above, applied with `git apply --unidiff-zero source-batch-controller.patch`.
An isolated replay reproduces the measured candidate source byte for byte;
the identity record distinguishes this publication patch from the unchanged
original measured patch retained in scratch. Require
standalone baseline LLVM byte equality before relying on the embedded-library
input method. Emit candidate raw LLVM with ordinary `--check`/`--emit-llvm`;
change exactly one main symbol for timed LLVM and apply the existing Makefile
observer substitutions for account LLVM. Use ordinary
`/usr/bin/clang -O3 -Wno-override-module -x ir` and the exact retained link
arguments/order. The full immutable raw logs, source rejection/repair and
native artifacts remain under `/private/tmp/whitefoot-vector-batch-controller/`;
the public identity record documents portable prefixes for those locations.
CSV-only reproduction uses `../summarize-ecosystem.pl --complete` and
`--targets` per arm, then the unchanged paired rule: disjoint ranges in both
cohorts, every paired WF sample at least 1 ms, and at most 10% cohort
median-ratio spread. This rejected counted controller is not repaired or
retimed. The separately qualified countdown successor below changes the
outer live-state dataflow and preserves this original refusal.

### Countdown batch controller: balanced pair selects caller composition

The experiment witness now uses a private generic batch helper with a
remaining-round countdown and rolling wrapping seed. Three scalar suffix-two
cells gain in both process orders, with no qualified useful loss in either
pair. This selects caller/source decomposition only: public GrowVector
operations, compiler/library, capacity/growth policy and specification stay
unchanged. The standard-peer target remains incomplete.

The [exact two-hunk patch](source-batch-countdown.patch), against
`998f32d201f6946a8cc90fc3d6569b033282a8db`, preserves imports, all append/truncate
and callback/digest order, zero rounds/zero suffix, wrapping seeds and final
cleanup. The integrated source equals the measured candidate, SHA-256
`25f8827d4c959eef3d124fb3f477d59de12c55e78d56fe5b77f3be00a317bb88`.
This changes the outer controller, unlike the refused inner remaining-count
probe; the earlier direct-composition and counted-batch refusals remain.
The [identity record](ecosystem-batch-countdown-identities.json) retains the
prospective criteria, exact commands/statuses, input/image hashes and replay.

Frozen575 first rejects the source with FN-9; current122 CLI
`0118a8206ee7e811e7c903b6e7b340754c50107ff7e454aadb48751481dc9f74`
admits the original bytes unchanged. ENT-5 specifies the break-edge join in
both versions; no rule or compiler change is made here. These are current122
WF source emissions with retained harness/runtime/native peers, not an
all-current-main test. Original baseline assembly matches the retained image.

The preregistered native criterion passes: scalar no-growth work is exactly
`21 + 16 × removed` (69 at suffix-three, including the digest `mov`), without
new hot branch, owner-slot transfer, spill or call. Against the earlier batch,
only two of 59 functions change; scalar helper frame/instructions are
144 B/130→128 B/128, wide 368 B/183→352 B/178. Wide constants stay outside
cycles, direct stores/SIMD remain, and seven Q-pair reloads still transfer
224 bytes per nonempty truncate. No elapsed share follows from these counts.
All four full 1,260-configuration/8,820-execution checks pass; four checksum
and two cleanup faults reject as expected. Both 294-row instrumented ledgers
match retained accounting exactly; that is not timed allocation traffic.

Before timing, A1,B1,B2,A2 was fixed: A=original unbatched control, B=countdown,
each `measure 1048576 7`, one continuous 420-second guard. The raw
[A1](ecosystem-batch-countdown-control1-samples.csv),
[B1](ecosystem-batch-countdown-candidate1-samples.csv),
[B2](ecosystem-batch-countdown-candidate2-samples.csv) and
[A2](ecosystem-batch-countdown-control2-samples.csv) retain all 16,464 rows,
both cohorts, sample IDs 0–6 and all seven variants. Direct exits are all 0
at 81.959/80.767/80.011/80.910 s; outer exit 0/324.022 s, empty stderr and
231 unchanged timing pins. Construction/check costs remain separate in the
identity record. No sample is pooled, excluded or native-scaled.

The unchanged reducers qualify A1/B1 and A2/B2 independently: separated WF
ranges in both cohorts, every WF sample ≥1 ms, ratio spread ≤10%. A combined
gain/loss must qualify in both orders; overlap/instability stays inconclusive.
Any useful qualified loss in either pair would refuse selection. All
[42 paired outcomes](ecosystem-batch-countdown-paired.csv) remain available.

| Consistent useful gain | B1/A1, cohorts 0 / 1 | B2/A2, cohorts 0 / 1 |
|---|---:|---:|
| Scalar suffix-2, 16 | 0.861432 / 0.825542 | 0.852364 / 0.830821 |
| Scalar suffix-2, 256 | 0.831904 / 0.830028 | 0.839527 / 0.842226 |
| Scalar suffix-2, 4096 | 0.822899 / 0.810796 | 0.839747 / 0.835334 |

All other 33 useful paired cells are inconclusive: reserved/growth/reuse at
both widths/all populations (18), scalar suffix-1/3 at all populations (6),
and wide suffix-1/2/3 at all populations (9). No useful cell is short.
The old scalar suffix-3/16 loss now overlaps in both orders and passes both
peer targets; it is not a qualified paired gain. Wide suffix-1 has large raw
reductions, but reverse-order spreads of 15.617/23.868/21.103% at 16/256/4096
fail stability; pair 1 at 4096 also fails. Three wide suffix-zero controls
gain in both pairs but remain unranked; three scalar controls are short/overlap.

| Peer target P/D/I, plus six unranked each | A1 | B1 | A2 | B2 |
|---|---:|---:|---:|---:|
| Pass / deficit / inconclusive | 13 / 3 / 20 | 19 / 1 / 16 | 16 / 4 / 16 | 25 / 1 / 10 |

The sole candidate deficit in both orders is **scalar growth/16**, which
never reaches the changed suffix controller:

| Process / cohort | WF / Rust | WF / C++ |
|---|---:|---:|
| B1 / 0 | 1.122712 | 1.092160 |
| B1 / 1 | 1.102578 | 1.081357 |
| B2 / 0 | 1.124530 | 1.087458 |
| B2 / 1 | 1.122324 | 1.091259 |

Every candidate peer-target inconclusive cell in either order is listed below
(I=inconclusive, P=pass); all other useful cells pass except growth/16 above.

| Bytes | Path | Count | B1 | B2 |
|---:|---|---:|---:|---:|
| 8 | reserved | 16 | I | I |
| 8 | reserved | 256 | I | P |
| 8 | suffix-1 | 16 | I | P |
| 8 | suffix-1 | 256 | I | I |
| 8 | suffix-1 | 4096 | I | I |
| 8 | suffix-2 | 16 | I | P |
| 256 | reserved | 16 | I | P |
| 256 | growth | 16 | I | I |
| 256 | growth | 256 | I | P |
| 256 | growth | 4096 | I | I |
| 256 | reuse | 256 | P | I |
| 256 | suffix-1 | 16 | I | P |
| 256 | suffix-1 | 4096 | I | P |
| 256 | suffix-2 | 16 | I | I |
| 256 | suffix-2 | 256 | I | I |
| 256 | suffix-2 | 4096 | I | I |
| 256 | suffix-3 | 16 | I | I |

Native drift is retained, not used to normalize WF: Rust/C++ ranges are
0.964458–1.064744 / 0.943060–1.030963 in pair 1 and
0.965218–1.039604 / 0.957608–1.074914 in pair 2. Direct-C varies more,
0.733756–1.384268 and 0.965457–1.367265, especially scalar suffix-2/256.
All per-cell drift, bounds and qualification reasons remain in the identity
record. CSV replay uses the maintained `../summarize-ecosystem.pl --complete`
and `--targets` per process, then the unchanged paired reducer per full pair.
Exact O3/link/observer commands and the original failure/native/behavior logs
are pinned in the record. No rerun of this unchanged candidate is scheduled.

### Aggregate-only opening shifts: full paired refusal

The separate compiler trial keeps scalar, Box-pointer, closing and Ring shifts
unchanged, while replacing stored-aggregate Slots openings with one span move.
Its [complete Vector/Ordered result](../ordered-library/RESULTS.md#aggregate-opening-full-pair-refuses-selection)
rejects the factor: Vector has no qualified useful gain or loss, and Ordered
has three gains and two scalar losses. The unchanged-source Vector
[baseline](ecosystem-aggregate-opening-baseline-samples.csv) and
[candidate](ecosystem-aggregate-opening-candidate-samples.csv) retain all
4,116 rows per arm, including the six unranked suffix-zero controls. The
linked record publishes every cell, peer drift, construction/correctness
evidence and replay identities; the compiler factor remains unintegrated.

### Guarded aggregate opening: both Vector pairs remain unselected

The follow-up factor preserves the common walk's zero-trip guard and omits
zero-stride shift emission while retaining stored-aggregate Slots eligibility.
Its [completed full comparison](../ordered-library/RESULTS.md#guarded-aggregate-opening-full-matrix-still-refuses-selection)
refuses selection: Vector has no qualified useful gain in either independent
pair, and Ordered has three qualified useful losses despite five gains.
The compiler candidate remains an archived replay patch, with no production
integration or language change.

The shared [evidence archive](../ordered-library/guarded-aggregate-opening-evidence.tar.gz)
retains four separate 4,116-row Vector invocations and four Ordered cohort
invocations, original criteria, all hashes/statuses and every comparison.
Vector order is baseline first→candidate first→candidate second→baseline second;
all samples 0–6 are ranked, without pooling or sample renumbering. Six
suffix-zero cells remain unranked controls. A useful gain must qualify in
both invocation pairs; any qualified useful loss vetoes the complete trial.

| Vector disposition | First pair | Second pair | Both-pair disposition |
|---|---:|---:|---:|
| Qualified useful gain / loss | 0 / 0 | 0 / 0 | 0 / 0 |
| Useful overlap | 34 | 33 | 33 |
| Useful duration/spread inconclusive | 2 | 3 | 3 |

The six suffix-zero controls collapse to three overlaps and three duration/
spread inconclusive cells; every replicate remains in the raw and paired
tables. Useful slower-standard target pass/deficit/inconclusive counts are
16/2/18→18/4/14 in the first pair and 18/5/13→19/2/15 in the second.
Neither arm passes a useful cell against Rust in either pair. Native peers
remain unchanged images and their drift is reported separately: useful cohort
median ratios span reverse-C 0.962–1.041, direct-C 0.944–1.697, swap-take-C
0.950–1.065, take-swap-C 0.966–1.034, Rust 0.970–1.033 and C++ 0.948–1.047.
Only first-pair wide growth/16 separates favorably for swap-take-C and
take-swap-C. Large or unstable control drift is preserved, never subtracted.

The native gate removes empty suffix shift calls, but insert/grow-insert/work
frames still grow 272→304, 304→320 and 688→704 bytes versus baseline.
Passing owner, length, guard-bypass and accounting observations does not
establish acceptable runtime cost. The linked Ordered record reports all
construction/execution costs, direct statuses, three adverse scalar cells and
the exact CSV/source replay. This is a frozen compiler-lineage comparison,
not current-main performance evidence.

### Branch-first swap removal: local native savings fail the full pair

The ordinary library source factor is rejected: the complete pair has no
qualified useful gain, three qualified losses, 30 overlaps and three unstable
cells. No library, compiler, caller, API or specification change is selected.
The [two-file patch](swap-remove-branch-first.patch) applies to
`7abd6bb34746b7b983b83103c68ee429b24859bb`; its replay reproduces the measured
candidate bytes. It branches before `take_back`: a non-last selection takes,
swaps with the selected slot and returns; a last selection takes and returns
directly. Signatures, contracts, effects, ownership and callers are unchanged.

The earlier take-before-branch candidate remains refused: its standalone wide
non-last path retains four 256-byte transfers, despite an inlined improvement.
The distinct branch-first source removes the overwritten initial snapshot
without a new operation, alias promise or compiler change. Frozen CLI575 is
reused, not rebuilt. Both arms carry the selected library under `pkg::vector`
with eleven caller alias-prefix substitutions. Complete baseline assembly
matches the frozen terminal/T1H0 assembly after only that namespace change.
The [identity record](ecosystem-swap-remove-branch-first-identities.json)
retains both criteria, the first refusal, exact sources, commands, compiler,
peer/runtime inputs, direct statuses, hashes and reproduction instructions.

The native gate passes. Standalone wide non-last removal falls from four to
three transfers; last/singleton falls from four to one and needs no frame.
Inlined wide removal plus its complete digest reduces payload/temporary
traffic from 1,792 to 896 bytes, with 64 bytes of spills and 64 bytes of reloads
remaining. One additional eight-byte environment-pointer reload remains.
The frozen preliminary record and timing criterion also called its store
additional; inspection corrects that wording: both arms already store the
pointer at entry. The original records remain preserved with this correction.

| Changed native body | Instructions, baseline → candidate | Maximum frame |
|---|---:|---:|
| Scalar work | 157 → 161 | 64 → 64 B |
| Wide work | 441 → 451 | 688 → 432 B |
| Scalar standalone swap removal | 13 → 13 | 0 → 0 B |
| Wide standalone swap removal | 78 → 78 | 272 → 272 B; candidate last path 0 B |

The other 55 bodies and constants match; timed text grows 10,540→10,596 bytes
and literal storage remains 912 bytes. Fifteen frozen non-WF inputs per link
are reused. Linked body instructions, destinations and literals are checked
against each object, including the established strong runtime floor body;
timed images contain no observer hooks. All 113 direct correctness commands
match their expected statuses: four 1,260-configuration / 8,820-execution
matrices, checksum/cleanup faults, two equal 294-row ledgers, sequential and
parallel scalar/wide/owning/zero-size fixtures, full returned/surviving values,
last/singleton cases and empty-domain FN-8 rejections. Wrong return,
replacement and order controls fail in ordinary and observed execution.
The unchanged allocation observer checks exact release identities; its 25/8
allocation counts are instrumented-image observations. Two fixture FORM-2
spacing rejections are preserved; the repairs change no tokens or obligations.

The complete [control samples](ecosystem-swap-remove-branch-first-control-samples.csv),
[candidate samples](ecosystem-swap-remove-branch-first-candidate-samples.csv)
and [paired cells](ecosystem-swap-remove-branch-first-paired.csv) retain one
control→candidate `measure 1048576 7` pair, all 42 cells, both cohorts, seven
samples and seven variants: 4,116 rows per arm. All non-time fields match;
no sample is removed or native-scaled. Both direct exits are 0, stderr is
empty, and all 184 pinned inputs remain unchanged. The arms take
80.564833/80.483466 s; the guard takes 161.365254 s. Source admission costs
0.659381 s, fixture/native construction 16.512595 s, linked inspection
0.707969 s and correctness/accounting/fault execution 7.789442 s, separately
from performance measurements.

| Qualified useful loss | Candidate/control medians, cohorts 0 / 1 |
|---|---:|
| Scalar reuse, 16 | 1.037903 / 1.103226 |
| Scalar suffix-3, 16 | 1.045488 / 1.044820 |
| Wide reserved, 16 | 1.016408 / 1.019519 |

Every useful paired WF sample is at least 1.205 ms. Wide suffix-one at all
three counts is unstable, including a 93.96% cohort-ratio spread at count 16.
All six unranked suffix-zero controls overlap; the three scalar controls are
short, down to 0.975 ms. Standard-peer target pass/deficit/inconclusive counts
change 17/3/16→17/4/15, plus six unranked cells. Candidate scalar growth/16
remains a target deficit at 1.075688/1.069434 times the slower standard peer.
The earlier scalar suffix-three loss is repeated at count 16; counts 256/4096
overlap. Historical wide suffix-two/16 remains an overlap at
1.003527/0.996301, not a qualified reversal of its earlier refusal.

Useful native median drift spans reverse-C 0.948–1.110, direct-C 0.649–1.012,
swap-take-C 0.964–1.030, take-swap-C 0.961–1.058, Rust 0.956–1.060 and C++
0.958–1.073. The full per-cell drift is retained. At scalar suffix-three/16,
Rust is 1.000000/1.003072 and C++ 0.997701/1.000000; the WF loss is not removed
by those observations. The pair rejects the candidate under the original
criterion without identifying a hardware cause for every difference.

The subsequent read-only reachability check limits what the native gate
established. In `vector_library_work`, one swap removal follows the complete
fill and marker insertion/removal. Each round constructs and digests `N+1`
values and performs two middle shifts. The harness uses `1048576/N` rounds
for all three mixed paths. It does not call the standalone removal bodies.

| Measured paths | Inlined swap removals per trace at N=16 / 256 / 4096 |
|---|---:|
| Reserved, growth, reuse, each | 65,536 / 4,096 / 256 |
| Suffix-0, suffix-1, suffix-2, suffix-3 | 0 / 0 / 0 |

Every timed removal selects index zero with `N≥16`; last/singleton savings
are correctness coverage, not timed work. The wide saving is 896 bytes per
mixed round, or 56/3.5/0.21875 MiB of native memory-access bytes per sample.
Those are instruction-level byte counts, not cache or DRAM traffic. They
decrease relative to filling, shifting and digesting the whole vector as N
grows. The gate established a local improvement without establishing that
removal dominated the timed trace; that missing weighting weakens the original
expectation of an observable full-trace gain.

The three loss cells have different exposure. Scalar reuse/16 reaches the
changed scalar work body every round: its entry address stays fixed, but the
removal block moves 60 bytes, a singleton guard appears, and the surrounding
branch layout changes despite the removal/digest slice shrinking 13→11
instructions. Wide reserved/16 reaches the changed wide work body, shifted
16 bytes, with the smaller frame, extra conditional/join branches and residual
spills. Neither local byte saving proves a reduction in elapsed time.

Scalar suffix-three/16 reaches none of the changed removal/work bodies.
Its scalar trace has the same 229 instructions and unslid address
`0x10000badc`; all 37 instruction sites of the no-growth repeated cycle are
byte-identical at the same addresses, executing 69 instructions for three
elements. Its twelve changed machine words are relocated calls, outside that
repeated path or on untaken growth/other-path edges. C trace addresses remain
fixed while Rust/C++ bodies shift 56 bytes. Placement, runtime state and
interleaved execution remain unisolated conditions, not demonstrated causes
of the loss. The exact source/native lines and every differing word are in
the identity record. No new source variant or timing pair follows this result;
another optimization needs evidence about costs on the dominant repeated path.

### Empty storage: native scope and ownership audit

This read-only audit uses the unchanged source/compiler pin
`7abd6bb34746b7b983b83103c68ee429b24859bb`, frozen compiler `5753999f...`, and
the baseline raw/optimized LLVM and assembly under
`.build/swap-remove-source/native-2/`, identified in the
[branch-first record](ecosystem-swap-remove-branch-first-identities.json).
No source change, compilation or new timing follows from it. The preceding
branch-first refusal remains; its suffix loss is not attributable to execution
of the changed removal operation.

Both native `round` instances retain `calloc(1,16)`: reserved rounds release
that header during reserve, growing rounds at first growth. At population 16
this is 65,536 empty allocation/free pairs per measured trace. Reuse and suffix
traces instead allocate positive backing directly; their optimized construction
already omits the empty pair. The pre-O3 accounting ledger still observes that
pair and must not be read as optimized allocation traffic. The ordinary Rust
and C++ growth loops initialize inline descriptors without allocating an empty
heap header; payload work and their distinct growth policies remain intact.
The earlier [empty-header cost injection](#native-empty-header-ab-the-allocation-pair-explains-most-of-the-standard-gap)
establishes a material small-growth cost, not a promised speedup from a new
representation or a benefit on the already-elided suffix paths.

The reserved-arm explanation is narrower than an inlining failure. In
`baseline.opt.ll:1480`, the nullable calloc and its write/abort resource-floor
edge precede `reserve_first`; the reserved edge already exposes positive
allocation and `free(old)`, while the other edge carries the stored old pointer
to `work`. The initial pointer is therefore live on one branch. The same
file's trace body at line 433 removes the empty allocation and its failure edge
for unconditional new-then-reserve. Thus resource checks are not an independent
prohibition on removal. LLVM's [allocation-elision contract](https://llvm.org/docs/LangRef.html#function-attributes)
allows virtual allocation, and upstream
[LLVM 21 InstCombine](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Transforms/InstCombine/InstructionCombining.cpp#L3132)
can fold null comparisons when removing an allocation, but rejects a use that
stores the allocated pointer elsewhere. The surviving `store ptr %calloc...,
ptr %v4` is such a use. This supports a conditional-lifetime/use limitation;
it does not identify the exact failing pass in the vendor compiler. Growth
additionally crosses `round` to `work` to `grow_full`; existing `noalias` and
`captures(none)` do not establish that the pointee is dispensable.

The historical [sentinel patch](zero-slots.patch) is incomplete beyond the
previously recorded implicit/nested cleanup gap. PRE-1 permits grow-zero from
capacity zero, but its patched grow still allocates a fresh 16-byte heap header
which the patched `free_empty` skips. Repeated grow-zero also skips
old heap headers. Moreover, the unchanged
[run-transfer lowering](https://github.com/mbbill/Whitefoot/blob/7abd6bb34746b7b983b83103c68ee429b24859bb/compiler/src/backend/emitter/runs.rs#L980)
writes both length words for zero-count append/split-off. Its shared-header
claim therefore needs independent-owner and parallel-write coverage as well
as explicit, implicit and nested releases. These are deductions from the
patch and rules, not newly executed witnesses or retained-compiler defects.

STOR-7/8 remove address observations and put resource exhaustion outside source
outcomes; they do not by themselves establish a compatible shared-header ABI.
STOR-1/3 retain ownership/release obligations, and SCOPE-3/PRE-1 require ordinary
linked definitions to use the selected physical representation. A linked
constructor can supply a real zero-capacity allocation under the current ABI;
whole-Box replacement or exchange transports that owner. Capacity alone cannot
distinguish it from static storage. Source readonly fields also do not establish
that every compatible linked implementation omits physical zero-count stores.

A lazy library enum would change the public `storage: Box<Slots<T>>` path and
its measure/element/ownership observations; the [earlier proof-interface
witness](#next-discriminator-merge-the-first-growth-into-construction) remains
relevant. Conservative fresh-allocation coalescing could retain the current ABI
by materializing before unknown calls or escaping ownership, with no tag on
positive-capacity storage, but is only a hypothesis. A local fold does not cover
the surviving growth call chain, and this benchmark alone does not justify a
new interprocedural pass. The current WF API has no capacity-taking constructor:
its reserved caller uses new then reserve, while Rust uses `with_capacity` and
C++ reserves its empty descriptor. Such a WF convenience API could avoid
reserved setup's empty header; it would require caller adoption and would not
fix default growth-16. The [H1/H2 realloc refusals](#h-realloc-for-runtime-slots-growth)
and [common-caller small-allocation losses](#hbyte-allocation-byte-attribution-through-one-common-caller)
also remain in force. Reopening any alternative requires complete cleanup/ABI
coverage and explicit positive-path costs before another performance selection.

### Nullable zero-extent owner: allocation gain, useful regressions refuse selection

A frozen compiler prototype based on `1226083e2375044483109c17bcc46a7c652f44bf`
makes newly created zero-extent runtime Array/Slots/Ring owners physically
null while keeping logical ownership and the one-pointer Box layout. Here
zero extent means logical capacity/count zero: positive-capacity zero-sized
elements retain a real header. Nonnull includes real zero-capacity linked
owners, which still release; capacity is never the ownership tag. Central read, write, release and payload operations
handle null. This is an unselected physical ABI proposal, with no source rule,
specification or design-tree change. The [final compiler source patch](nullable-zero-owner.patch),
[accounting expectation delta](nullable-zero-owner-account.patch) and
[identities](ecosystem-nullable-owner-identities.json) pin the source, compiler,
toolchain, native inputs, commands and measured images. The final patch includes
the focused fixture's grammar/routing repairs: its test binary was rebuilt after
those fixture-only changes, while the measured emitter bytes were unchanged.

The linked lifecycle fixture passes its two focused `gate` tests with
`WF_WORKERS=0`: four positive mode/retained combinations preserve the exact
27-allocation/release ledger, and routing, guard, skip-release, double-release,
lost-owner and refusal controls fail as expected. Real-worker transport is not
qualified. Baseline and candidate timed/account images each pass the
unchanged 1,260-configuration/seven-variant content oracle; all four checksum
and two cleanup fault controls fail as intended. In the 294-row account output,
all 252 peer rows are identical and all 42 WF rows match the independently
derived one request/release and 16-byte reduction per source zero constructor,
including the registered peak exceptions. O3 timed round code removes one
`calloc` and one `free` for each width, but positive append adds a per-iteration
null test and length reload; wide constants also reload from stack. These
native observations do not isolate an elapsed-time percentage.

One full control-then-candidate `measure 1048576 7` pair ran without retry,
81.776/81.163 s, each retaining 4,116 raw rows and 588 groups. The
[control](ecosystem-nullable-owner-control-samples.csv) and
[candidate](ecosystem-nullable-owner-candidate-samples.csv) retain every sample
and outlier. All 36 useful cells satisfy the existing duration and stability
screens: **four qualified gains, 14 qualified losses, 18 overlaps**. Scalar
growth/count 16 improves to `0.920115/0.920393` times control across cohorts;
scalar reserved/count 16 worsens to `1.286597/1.291788`, and scalar reuse
spans `2.007–2.127` times control. All three wide suffix-one cells gain
(`0.545–0.673` times control), although the baseline already elides the empty
allocation on that path, so their gain is not attributed to the removed
header. The six suffix-zero controls remain unranked. Fresh slower-standard
totals move from 18 pass/3 deficit/15 inconclusive to 5/16/15, plus six
unranked each. The 14 useful losses refuse the unchanged no-loss criterion;
the representation is not selected. Remaining shape/transport coverage is
unqualified.

A separate [IR-only READ fallback](nullable-zero-read-fallback.patch) selects
a private immutable three-word zero header for null before loading; ownership,
write, release, payload and caller paths remain unchanged. Its patch applies
to the candidate timed raw IR identified in the identity record, not to
compiler source. Both full-module O3 native commands exit 0, but the
predeclared code screen fails: scalar and wide append use per-iteration
`cmp`/`csel`/`ldp` plus a separate real-header length load, scalar work's
frame grows 64→80 bytes and wide tail's 288→304. Wide constants recover
register residency, but the nonnull append path does not regain length
promotion. This variant stopped before linking, behavior execution or timing.
No shared mutable header, owner policy or fallback was adopted.

### Canonical empty-header tag: balanced diagnostic misses the growth criterion

This IR-only successor to the refused nullable prototype uses one externally
linked immutable three-word zero header as the empty-owner identity. Reads
load that header directly; release and possibly zero-count writes test its
identity. Ten writes whose operation domains prove a real destination become
direct stores. The [exact IR patch](canonical-empty-tag-ir.patch),
[runtime C definition](canonical-empty-header.c) and
[identity/replay record](ecosystem-canonical-tag-identities.json) reuse the
existing nullable compiler patch. They are frozen research inputs, consumed
only by scratch reproduction; consolidation must preserve their hashes,
failed criteria, raw samples and this result anchor.

Both static refusals remain. With an external declaration whose contents are
unknown, scalar/wide trace frames grow `160→176`/`352→384` bytes. Both variants
match the baseline's full scalar and wide work/tail operand streams,
normalizing only local assembly labels. With an `available_externally` zero
initializer, trace frames still grow `160→176`/`352→368`: the unchanged frame
criterion fails. Both widths' round bodies fall from 40 to 33 operand
instructions and 80 to 64 frame bytes. Counts exclude operandless `ret`;
they do not attribute elapsed time. The linked object references the one
actual C tag definition.

The saved baseline/candidate timed and account images each pass all 1,260
configurations and 8,820 executions. Four checksum and two cleanup faults
exit 1 with the required diagnostics; all 17 construction/qualification
commands have their expected direct statuses. Candidate accounting is
byte-identical to the qualified nullable delta: all 42 WF rows save one zero
request/release and 16 bytes per source zero constructor, while all 252 peer
rows stay unchanged. This covers the measured Vector path, not remaining
shape or real-worker ABI qualification.

The separate measured criterion required scalar growth/count 16 to gain in
both independent process orderings; any qualified useful loss in either pair
would refuse it. Before observation, A1,B1,B2,A2 was fixed, each
`measure 1048576 7`, using the same 1 ms floor, at most 10% cohort-ratio spread
and disjoint observed sample ranges. The guarded run exits 0 in 323.865 s;
all four process statuses are 0 and all 44 input pins stay unchanged.
[A1](ecosystem-canonical-tag-control1-samples.csv),
[B1](ecosystem-canonical-tag-candidate1-samples.csv),
[B2](ecosystem-canonical-tag-candidate2-samples.csv) and
[A2](ecosystem-canonical-tag-control2-samples.csv) preserve every one of the
16,464 rows, including all seven sample IDs and outliers. The
[42-cell reduction](ecosystem-canonical-tag-paired.csv) retains both pairs.

| Scalar cell | B/A, pair 1 cohorts | B/A, pair 2 cohorts | Combined |
|---|---|---|---|
| growth/16 | 0.939977 / 0.923010 | 0.929565 / 0.950725 | Overlap |
| suffix-2/256 | 0.838915 / 0.839378 | 0.840069 / 0.852194 | Gain |
| suffix-2/4096 | 0.836688 / 0.827374 | 0.837169 / 0.845580 | Gain |

The primary criterion is **not met**: growth/16 overlaps in both pairs.
Its candidate WF/C++ medians are `0.997641/0.983753` and
`0.988540/0.974202`, with ordinary peer targets inconclusive in both orders;
WF/Rust remains `1.024–1.033`. There are two cross-order useful gains,
34 inconclusive useful cells, zero qualified useful losses, and six unranked
suffix-zero controls. Separate A/B peer totals are `17/4/15 → 20/2/14` and
`17/3/16 → 17/1/18` (pass/deficit/inconclusive); all cells remain visible.

This total diagnostic also inherits the nullable prototype's first-growth
zero-length `memmove` elision; it does not isolate allocation savings from
tag, frame, transfer or layout costs. It uses frozen pre-countdown current122
WF IR with retained harness/runtime/peers, so the two suffix gains are not
additive with the selected countdown caller and do not establish merged-head
parity. No compiler, runtime, representation or source change is selected;
no additional timing is scheduled for this unchanged candidate.

### Per-API Vector comparison: spare-capacity append first

The next series follows one container and one API at a time: spare-capacity
append; reserve/grow; insert; remove/swap_remove; truncate/drain; construct/free.
Overall Vector timing follows only after every API passes; other containers
remain paused. Existing whole-trace modes and their historical results remain.

The first comparison uses ordinary `grow_vector_append`, `Vec::push` and
`std::vector::emplace_back`, with scalar u64 and move-only 256-byte records at
counts/capacities 16, 256 and 4096. Preparation reserves independent vectors
outside timing. One clock interval covers many external append-batch calls;
identical wrapping seed-plus-index and 32-word construction are included in
each language's batch. Inspection checks every value, length, capacity and
independent checksum, then resets and destroys outside timing. A separate
account image must observe zero allocation/reallocation/release/byte change
across append only; payload and allocation faults must fail that check.
Working-set size and empty-batch clock/call controls remain reported, especially
at count16; no guessed overhead is subtracted. Passing requires each cell to
be repeatably faster than the slower ordinary Rust/C++ peer in both sampling
orders under the established duration, stability and sample-range criteria.
Each peer is reported separately; indistinguishable results do not pass.

The timed and separate account images pass counts 0, 1, 16, 256 and 4096 for
both widths and all three APIs, including wrapping seeds. Exact prepared
capacity, every word, length, independent checksum and complete cleanup are
observed; the account snapshot spans append only and remains unchanged. The
deliberately wrong offered seed and allocation-inside-append faults exit 1
with their required value and allocation diagnostics. Native descriptors live
directly in aligned C-owned storage: no boxed peer descriptor or extra handle
lookup is timed, and no native vector field layout is exposed. WF uses its
ordinary generated single-pointer result and borrowed owner-slot ABI.

Each interval contains only the common C context loop and external append
batch calls, including equal wrapping input generation. Allocation/reserve,
warmup, validation/reset, checksum observation and cleanup are outside it.
Elapsed time sums these fixed intervals; rotating peer order is reversed in
the second cohort. The context count is capped at 1024: scalar counts
16/256/4096 use 1024/512/32 contexts, and wide counts use 256/16/1. Reported
payload is 128 KiB per scalar16 variant and 1 MiB otherwise; descriptor bytes
are C context storage, not complete allocator residency (WF backing headers
and allocator metadata are additional). Empty batch calls use the same
wrappers and intervals, are retained as `control=1`, and are never subtracted.

The [1M-operation run](ecosystem-spare-append-1m-samples.csv) retains all 504
rows: all three scalar cells are unqualified because WF or its selected peer
falls below 1 ms; all three wide cells pass the slower-peer target. The
[4M-operation run](ecosystem-spare-append-4m-samples.csv) also retains all 504
rows, including seven samples, both orders, both controls and every outlier.
It is the primary comparison: all six cells satisfy the 1 ms minimum and
10% cohort-ratio stability bound, with no sample filtering or noise
normalization. C++ is the slower median peer in every cell and cohort.

| Payload / count | WF/C++, cohorts 0 / 1 | WF/Rust, cohorts 0 / 1 | Slower-peer qualification |
|---|---|---|---|
| u64 / 16 | 0.472254 / 0.473033 | 1.023964 / 1.011988 | Pass |
| u64 / 256 | 0.539397 / 0.539979 | 1.164024 / 1.175125 | Pass |
| u64 / 4096 | 1.148838 / 1.161742 | 1.275510 / 1.226601 | Deficit |
| 256 B / 16 | 0.703558 / 0.702951 | 1.092599 / 1.086562 | Pass |
| 256 B / 256 | 0.633152 / 0.637639 | 1.028976 / 1.038434 | Pass |
| 256 B / 4096 | 0.630182 / 0.624518 | 1.031091 / 1.032847 | Pass |

Each pass has disjoint faster sample ranges in both orders; scalar4096 has
disjoint slower ranges against both peers. Against Rust alone, scalar16 and
wide256 overlap in both orders; scalar256 and wide16 are slower in both;
wide4096 overlaps in order 0 and is slower in order 1. The WF/C++ cohort-ratio
spread is at most 1.124%. WF empty-control/append median fractions are
16.19%/16.58% at scalar16, 0.986%/0.997% at scalar256,
0.044%/0.067% at scalar4096, 2.009%/1.981% at wide16,
0.333%/0.311% at wide256 and 0.122%/0.140% at wide4096. Scalar16 remains
sensitive to wrapper/timer overhead; its pass does not isolate single-append
latency. No control fraction is used to adjust a result.

All five premeasurement image/source hashes remain unchanged. The measured
image SHA256 is `11beeea9123a0e65736f37f2fbf56402d3a913fda2f0e0e6ab6cef5682d87a7b`;
the frozen f99 compiler is `bd4e5268f16b892d645bd64f352c6551041ffefc90b57948ffa04de7fb59dfd8`.
Harness source SHA256 identities (the compiler's pending emitter experiment
was not linked into this image) are:

```text
vector-costs.c          64cdc01bb7cc2713586362838c241cf0e143d883d2f8c151d30bdecccd4eec52
vector-library.wf      134b1209e4617a9b525be1686fdbcd3e894fbe28e1bf19140056594577013f6a
vector-ecosystem.cpp   1dc9bd24782a9f3bf35c641fb7a86a0f672d8e3a5a7514329f5c8c4d4050da37
vector-ecosystem.rs    2ba7a8c4bf8fee871adeed1fb31ff29e2ca30c82b685f3f76b1f0eebbb21bcb6
```

`ecosystem-append-measure` reuses this mode with `APPEND_WORK=4194304` and
`ECO_REPEATS=7`; the whole-trace `ECO_WORK` default stays unchanged. Raw files
serve this API's deficit attribution and are not gate inputs. Keep them and
the source identities when consolidating this record. **Append remains open:**
scalar4096's qualified deficit prevents advancing to the next API or an
overall Vector timing claim.

Three scalar native diagnostics retain that boundary. [Payload-base](append-payload-base.diff)
hoists the payload base; [postindexed](append-postindexed.diff) advances a payload
cursor in the store; [cached-capacity](append-cached-capacity.diff) hoists the
capacity load. Each recomputes its cached state after growth, preserving the
ordinary full-capacity path. These are assembly diagnostics, with no compiler,
library or source change selected. Their control object is byte-identical to
the measured baseline `whitefoot-timed.o`, SHA256
`2099a2fbf8bed6a46f7a9b9e8fdbb4f838ecbcb1e249176f8a88f5f7b5283bae`.
All three patches replay exactly against the saved baseline assembly.

Each qualification exits 0 with all 30 API cases and eight additional
empty/reserved/growing append chains; qualification times are 1.04, 1.04 and
1.03 seconds respectively. Measurements exit 0 in 20.99, 21.51 and 21.18
seconds. The [payload-base](ecosystem-spare-append-payload-base-4m-samples.csv),
[postindexed](ecosystem-spare-append-postindexed-4m-samples.csv) and
[cached-capacity](ecosystem-spare-append-cached-capacity-4m-samples.csv) files each
retain all 504 rows, both orders, seven samples and empty controls. The same
duration, stability and disjoint-range criteria apply without adjustment.
Ratios below are medians in cohort 0 / 1; B/A compares WF against the original
4M baseline above, while peer ratios use each diagnostic's unchanged peers.

| Diagnostic | Scalar256 B/A | Scalar4096 B/A | Scalar4096 WF/C++ | Scalar4096 WF/Rust |
|---|---|---|---|---|
| Payload-base | 0.826679 / 0.831514 | 1.121556 / 1.147033 | 1.263329 / 1.270638 | 1.380848 / 1.426471 |
| Postindexed | 0.843119 / 0.859516 | 1.105778 / 1.125167 | 1.237503 / 1.267085 | 1.318495 / 1.357104 |
| Cached-capacity | 0.966181 / 0.981490 | 1.008444 / 1.014502 | 1.168684 / 1.137603 | 1.254284 / 1.193752 |

Payload-base and postindexed each have a qualified scalar256 gain and a
qualified scalar4096 loss versus baseline; their other four cells are
inconclusive across orders. Cached-capacity has six inconclusive B/A cells.
Each diagnostic still has five slower-peer passes and the scalar4096 deficit,
so none passes the API criterion or is selected. Postindexed wide4096's Rust
median drifts +10.67% versus baseline in cohort 1, exceeding the 10% peer-drift
bound; that B/A cell remains inconclusive, and its samples are retained.

The [deferred-length diagnostic](append-deferred-length.diff) instead keeps
scalar length in a register, publishing it before growth and on batch exit;
the zero-count path stays unchanged. The same 30 API cases and eight growth
chains, followed by measurement, exit 0 in one 22.38-second guarded command.
Its [504 raw rows](ecosystem-spare-append-deferred-length-4m-samples.csv) give
six slower-peer passes in this run. Scalar4096 medians are
0.541687/0.528097 ns per append, B/A `0.504889/0.494199`, WF/C++
`0.577676/0.563327` and WF/Rust `0.587840/0.606019`; scalar16 and4096 have
disjoint B/A gains in both cohorts. Scalar256 B/A is `1.008924/1.022781`,
overlapping in both. All three wide cells are inconclusive across orders;
wide16 and wide256 have disjoint slower B/A ranges in cohort 1, with the
other cohort overlapping. These adverse observations remain visible despite
their slower-peer passes. The patch also replays exactly against baseline.

A scalar4096-only, 32-context [ABBA confirmation](ecosystem-spare-append-deferred-length-confirmation-samples.csv)
retains all 336 rows with an explicit run column in
control1/candidate1/candidate2/control2 order; the guarded command exits 0 in
2.71 seconds. Baseline medians are 1.063–1.080 ns and candidate medians
0.524–0.537 ns. Candidate1 cohort0 sample0 is retained at 5,729,000 ns
(1.365900 ns per append), overlapping baseline and both peers; the other
three candidate cohorts have disjoint faster ranges. Thus this confirmation
is **inconclusive** under the unchanged full-range criterion. Deferred length
is a promising native diagnostic, not an implemented compiler transform or
selected API result. Append remains open pending a general implementation
and qualification; neither medians nor the first run replace that requirement.

The predeclared [64M-operation native-floor confirmation](ecosystem-spare-append-deferred-length-64m-confirmation-samples.csv)
uses the same binaries, scalar4096, 32 contexts and ABBA order, retaining all
336 rows and all seven samples per cohort; its guarded command exits 0 in
31.23 seconds. Candidate medians are 0.528172–0.539482 ns per append against
1.057804–1.085922 ns baselines. All four candidate cohorts have disjoint
faster ranges against their baselines and both peers; the shortest WF sample
has 35.157 ms of summed elapsed time, and paired cohort-ratio spread is at
most 3.184%. Individual clock windows still cover one 32-context batch
(about 70 microseconds for the candidate), with validation/reset outside each window;
64M increases cycle count, not the uninterrupted timed interval.
Candidate WF/C++ medians span 0.569246–0.574594 and WF/Rust
0.618999–0.634572. No outlier is filtered, and the inconclusive 4M result
above remains intact. This confirms the native floor with larger summed
sample durations;
it selects no compiler implementation and does not close the append API.

A [driver-only context sweep](append-context-sweep.diff) then varies scalar4096
from 1 to 64 independent vectors, using the original three language objects.
The guarded build, 30 checks and seven measurements exit 0 in 5.19 seconds.
The [combined raw file](ecosystem-spare-append-context-sweep-4m-samples.csv)
concatenates all seven 84-row files, retaining 588 rows in context-count order.
Below are elapsed medians per appended element in ns, cohort 0 / 1; setup,
inspection/reset and cleanup remain outside timing.

| Contexts / logical payload | WF | Rust | C++ |
|---|---|---|---|
| 1 / 32 KiB | 0.521421 / 0.512600 | 0.420094 / 0.435829 | 0.997066 / 0.956535 |
| 2 / 64 KiB | 0.528097 / 0.524998 | 0.442982 / 0.423670 | 0.944614 / 0.960827 |
| 4 / 128 KiB | 0.538588 / 0.541925 | 0.444651 / 0.444651 | 0.953913 / 0.999689 |
| 8 / 256 KiB | 1.120806 / 1.081944 | 1.109600 / 1.085281 | 0.985861 / 0.933886 |
| 16 / 512 KiB | 1.068354 / 1.079798 | 0.932217 / 0.980616 | 0.941277 / 0.966549 |
| 32 / 1 MiB | 1.111269 / 1.087189 | 0.870228 / 0.920773 | 0.945568 / 0.959396 |
| 64 / 2 MiB | 1.120090 / 1.083612 | 0.940800 / 0.890493 | 0.966311 / 0.944853 |

This observes a working-set dependence, without assigning a hardware cause.
The target remains the original 32-context, 1 MiB cell; smaller contexts do
not replace its deficit. These raw files and small replay diffs serve this
API's deficit attribution in the existing experiment home, are not gate
inputs, and may be retired only with an evidence-preserving consolidation.
Append remains open; no next API or overall timing claim follows.

#### Registered actual-compiler append comparison

The compiler-generated length-residency implementation is compared with the
original f99 image, not with a patched native diagnostic. The emitting CLI
must remain SHA256 `309a04cbe1af4a61297fd2c8d678c34cbb56688ac7d8545b74b5452c95cb7e74`.
Let `P=.build/append-api` within this experiment and `R=$P/residency1`.
Full A is `$P/ecosystem/vector-costs-timed` (SHA256
`11beeea9123a0e65736f37f2fbf56402d3a913fda2f0e0e6ab6cef5682d87a7b`);
full B is `$R/ecosystem/vector-costs-timed` (SHA256
`2a6f0add2c564552576dfbb9ab82bbd2c96c217eeda6d56284d8f9f1e7607f11`). Scalar-only A is
`$P/append-context-sweep` (SHA256
`2ae674100c15fdbd1ea2ad01a4c105060f583ee770398202bab933bdb4496a47`);
scalar-only B is `$R/vector-costs-context-timed`. B images and their complete
qualification must be verified before timing; neither A image is overwritten.
The fresh full B image passes the maintained timed/account checks; its timed
driver, C++/Rust and all 12 runtime objects are byte-identical to A. The new
WF timed object is `970bb67f062c55aa5118ac1e6c9f44ce3e51c56fda5ed77ebc2300db5e567419`;
the scalar-only B link was pending at registration, using the saved context driver.

B must link the exact saved A C++/Rust and runtime objects, with the saved
full driver for the full comparison and the saved context-sweep driver for
the scalar confirmation. Rebuilt objects are usable only if byte-identical.
Before and after timing, verify these shared SHA256 identities under `P`:

```text
ecosystem/driver-timed.o       7a416b4882a1b9fc27403f7887e5921bf80f6876bbe01cefdb20a675994cc194
append-context-sweep-driver.o a39cbebaf5fae2a7a70016f9c1c8926742786d4d70203eca165d8d349f42f821
ecosystem/cpp-timed.o          b6c8555462a970dc3cee7ae4a1c4552e5c74d6da1f6de979539675d616849a3f
ecosystem/rust-timed.a         32caefe6685a0492bdf59d9201c3ff82a99de4a0aa37bdb98f5b6212ac2935e9
native/completion/bridge.o        a1def81793c4229e41586590b91d134a9a2375a6ad9d31189b8164e91aaf137b
native/completion/file_adapter.o  1bcbb49aa9a44031faf6397e01a960b2ce5d5d02f42cc2aaecb4edc0818d8ec6
native/completion/file_posix.o    f1fa09d7c0831333726c367986a3811d588e639f090435b1cbf1fce62758bf33
native/completion/linux_io_uring.o e389769ac8c9f38af9cd00e5bc5d56d3c180dff5985fb81ddfff63dfa89eb453
native/completion/runtime.o       75cb971977625c9b05adcf74b14989544ccbf61b0a81fe4843ec72226eb36c61
native/completion/wait_host.o     b9dbbb1485b2c3e47ef248568020130e7e27446e56be5a2c52f224bbd7a2f463
native/ordinary_values.o          07ebcae5ed98fa7ff1a218150170f56c961e9dbae682781b62a5878a094da58b
native/ordinary_values_ir.o       846bc08916aadbe77111a2c45aff15f33fd1fb7ffe816c198c2bf6f96dcad210
native/sched/core.o               0655633c69666fcce21bf67258101aedb49c914fc50a6cc8edbfe22a8315e645
native/sched/entry.o              e04734d7a1ddbe9083b47570ca2854963803b88a7197628bed2ca66cb4924d5c
native/sched/prim_host.o          c3a5140b8221a57d27a510995ae37c7b33a64ebc87ec9f9286b99ece649bce45
native/wf_floor.o                 8342e7ea0b8d6a0817d543e8c05e807c96527393945e95597718ba9a27ce722d
```

After explicit timing release, run these commands in the fixed order under
one `run-check.pl` reservation with a 180-second outer timeout (estimated
115 seconds), recording each direct status and wall time:

```sh
"$P/ecosystem/vector-costs-timed" api-measure 4194304 7 > "$R/actual-4m-control1.csv"
"$R/ecosystem/vector-costs-timed" api-measure 4194304 7 > "$R/actual-4m-candidate1.csv"
"$R/ecosystem/vector-costs-timed" api-measure 4194304 7 > "$R/actual-4m-candidate2.csv"
"$P/ecosystem/vector-costs-timed" api-measure 4194304 7 > "$R/actual-4m-control2.csv"
WF_APPEND_CONTEXTS=32 "$P/append-context-sweep" api-measure 67108864 7 > "$R/actual-64m-control1.csv"
WF_APPEND_CONTEXTS=32 "$R/vector-costs-context-timed" api-measure 67108864 7 > "$R/actual-64m-candidate1.csv"
WF_APPEND_CONTEXTS=32 "$R/vector-costs-context-timed" api-measure 67108864 7 > "$R/actual-64m-candidate2.csv"
WF_APPEND_CONTEXTS=32 "$P/append-context-sweep" api-measure 67108864 7 > "$R/actual-64m-control2.csv"
```

The full comparison retains 504 rows per image (all six cells, two cohorts,
seven samples, append and empty controls). The scalar4096 confirmation is
unconditional after the full comparison, retaining 84 rows per image at the
original 32-context, 1 MiB target: 2,352 raw rows total. Its extra cycles
increase summed sample duration; validation/reset remain outside each clock
window. Stop only for an actual command failure, preserve partial outputs,
and do not automatically retry, filter samples, subtract controls or choose
orders after seeing data.

Reduce control1/candidate1 and control2/candidate2 separately. Each of the six
API cells must beat its slower ordinary peer with disjoint faster sample
ranges in both pairs and both cohorts, retaining each peer's ratio separately.
The existing 1 ms duration floor, maximum/minimum cohort-ratio spread of 10%,
and 10% native-peer drift bound remain; overlap or failed qualification is
inconclusive, never a pass. Report raw WF-relative gain/loss/overlap against A
separately from the peer target, including adverse cells in either pair.
No reserve/grow, other-container or whole-trace timing is authorized here.

The registered sequence completes with direct exit 0 in 115.60 seconds under
one uninterrupted guard. Scalar linking and its 30-case API check exit 0 in
0.11/0.43 seconds; the scalar B image is SHA256
`a8dbb2d50f8e5b19bddb3215606a5af0a97521acbe7ffcd9dc967950de540621`.
All eight measurements exit 0: full4M clocks are 20.90/20.88/20.88/20.87
seconds and scalar64M clocks 7.97/7.56/7.62/8.06, in registered ABBA order.
All frozen compiler/production inputs, four images and matched shared objects
remain unchanged. [Full4M raw](ecosystem-spare-append-residency-4m-samples.csv)
retains 2,016 rows, and [scalar64M raw](ecosystem-spare-append-residency-64m-samples.csv)
336 rows; explicit run columns preserve each of the eight original outputs.
Both files serve the registered append experiment and remain with its evidence
when consolidated; neither is a gate input.

All six cells meet the slower-peer target in both pairs and both cohorts,
with disjoint faster ranges against C++, the slower median peer throughout.
The shortest candidate WF sample is 1.891 ms; maximum peer drift is 3.433%
and maximum target cohort-ratio spread 3.320%, within the unchanged bounds.
The table retains each pair separately; each entry is cohort 0 / 1.

| Payload / count | B1 WF/C++ | B2 WF/C++ | B1 WF/Rust | B2 WF/Rust | WF-relative result across both pairs |
|---|---|---|---|---|---|
| u64 / 16 | 0.446913 / 0.450508 | 0.459645 / 0.455903 | 0.952642 / 0.966059 | 0.966650 / 0.976638 | Inconclusive |
| u64 / 256 | 0.537585 / 0.543128 | 0.557369 / 0.541117 | 1.191251 / 1.190129 | 1.198886 / 1.195067 | Inconclusive |
| u64 / 4096 | 0.557365 / 0.561031 | 0.568992 / 0.550714 | 0.631776 / 0.626765 | 0.627887 / 0.618218 | Gain |
| 256 B / 16 | 0.693578 / 0.694784 | 0.686270 / 0.702485 | 1.073647 / 1.077812 | 1.096536 / 1.098686 | Inconclusive |
| 256 B / 256 | 0.620767 / 0.629904 | 0.623364 / 0.617961 | 1.023368 / 1.034014 | 1.035295 / 1.027629 | Inconclusive |
| 256 B / 4096 | 0.617182 / 0.615333 | 0.618525 / 0.615194 | 1.031079 / 1.022408 | 1.010828 / 1.006519 | Inconclusive |

Scalar4096 has disjoint faster ranges against baseline and both peers in all
four cohorts: candidate medians are 0.522375–0.529289 ns per append, B/A
`0.494471/0.499550` and `0.495054/0.494491`. Scalar256's adverse median
changes are +0.80% to +2.18%, but every B/A range overlaps. The three wide
cells also overlap baseline in every cohort; scalar16 has one single-cohort
gain and otherwise overlaps. There is no qualified baseline loss.
Against Rust alone, scalar4096 passes throughout; scalar16 is inconclusive
across orders, scalar256 and wide16 are slower throughout, and wide256/4096
mix slower and overlapping ranges. These observations are retained separately
from the slower-peer target. Candidate scalar16 empty-control fractions are
17.10–17.43%; they are not subtracted.

The unconditional scalar64M confirmation also passes all four cohorts against
baseline and both peers: medians 0.525877–0.529751 ns, B/A
`0.499333/0.496714` and `0.491697/0.497289`, WF/C++
`0.566324/0.563358` and `0.564187/0.562861`, WF/Rust
`0.627611/0.628748` and `0.622619/0.628406`. Its shortest candidate sample
has 35.049 ms of summed elapsed time; individual batch windows and validation
boundaries are unchanged. No sample is filtered or normalized. This meets
the registered append performance criterion for compiler-generated code with
the frozen f99 caller, peers and runtime. It establishes neither an overall
Vector result nor a design ruling. Other APIs remain unmeasured in this series.


The production candidate uses private typed-IR helper versions for a resolved
incoming scalar Slots place. Its emitted scalar loop has a length PHI and no
per-iteration header/cache store or helper call; the 48-byte frame, payload
writes, capacity checks and ordinary growth call remain. Before ordinary
calls and region exits it conditionally publishes length, then reloads after
ordinary calls. Unsupported paths retain ordinary lowering and public ABI.
The [pending amendment](../../../../design/amendments/window-length-residency.md)
records this bounded implementation choice; no source rule changes.

The focused regression batch passes two tests in 2.19 seconds after a
64-second gate-profile build (67.24 seconds guarded total). Executions in all
three lowering modes cover empty/nonempty starts, growth, observers, early
exit, a removing helper and whole-owner replacement. A mutant removing
publication must exit with failure. Outer-Box fallback is an IR-only assertion:
the newly authored executing fixture was rejected at FN-8 after wrapping the
inner owner; that constructor-fact question is recorded in TODO, and no guard
was added to manufacture admission. Independent read-only review found no
proven semantic defect and identified the replacement/fallback coverage gaps
addressed by these tests. These focused results do not replace the full gate.

A lint-only refactor subsequently groups the rewrite's shared parameters.
The resulting CLI is SHA256
`9a3ac1cec5eed4a7a41aeced4f12b31cf81b5d34d6d72b9fdfe2d27de2973da5`;
its complete emitted Vector LLVM compares byte-for-byte equal to the measured
compiler's output (both SHA256
`75b909699ef952625a132482c14453641daacef1a0ae9eb9dc90658c70a78d0f`).
The timed program therefore keeps the measured code after this refactor;
no second timing series was substituted.

#### Append that triggers growth: correctness and native checkpoint

Vector remains first: qualify one API, including its distinct spare/growth
paths, before the next API; whole-Vector timing follows per-API qualification.
This checkpoint adds ordinary scalar/wide `append_one` and nonmutating
length/capacity snapshots. Preparation fills an empty vector outside timing;
the new append-one entry accepts nonzero length. The old empty-only batch
contract is unchanged. Exact full prestate, new length, capacity, every word,
wrapping checksum and cleanup pass all 30 cases in both timed/account images.
Wrong offered payload and wrong expected postlength each fail with status 1
and their specific diagnostics in both images; existing spare checks/faults
also pass.

| Requested initial capacity | Observed initial capacity, all three | WF / Rust / C++ postcapacity, both widths |
|---|---|---|
| 0 | 0 | 1 / 4 / 1 |
| 1 | 1 | 2 / 4 / 2 |
| 16 / 256 / 4096 | 16 / 256 / 4096 | All double to 32 / 512 / 8192 |

The [30-row allocation record](ecosystem-append-growth-baseline-account.csv)
snapshots append only and verifies zero live bytes after destruction.
For matched nonempty capacity N and width E, WF requests `16+2NE` bytes
(one request, no realloc, one release); C++ requests `2NE` (1/0/1);
Rust requests `2NE` (one request counted as realloc, no release). The initial
0/1 cases retain their different ordinary policies. These accounting-image
events do not establish timed Rust's in-place-reallocation frequency.
Keep this raw file with the checkpoint when consolidating; it is not a gate.

The first direct `return grow_vector_append::<T, 8193>(...)` spelling failed
`FN-9: InvalidPostconditionReturn` at line336 (check exit1, 0.609 seconds;
emission unstarted). Binding the result with `let` before returning it in the
three new append-one functions preserves every contract and passes check/emit
in 0.162/0.164 seconds. Failed bytes and diagnostics remain in the existing
scratch home; no specification or compiler rule changes for this repair.

| Actual append-one native body | Direct frame | Full-growth path |
|---|---|---|
| WF, scalar/wide | 32 B | malloc, payload memmove, free; 16 B backing header |
| Rust, scalar/wide | 48 B | Allocator realloc; allocation when empty |
| C++, scalar/wide | 80 B | New backing, new value construction, prefix relocation, old release |

All three construct wide payload directly in its final slot; there is no WF
256 B staging copy. These are new append-one bodies, not old batch evidence;
frames and allocator calls do not attribute elapsed cost. Native codegen/opt
exit0 under a 1.114-second guard. Harness construction exits0 in 2.16 seconds;
growth checks/account/spare checks exit0 in 0.87/0.11/0.11 seconds, outer
guard0/3.44 seconds. Emitting CLI SHA256 is
`9a3ac1cec5eed4a7a41aeced4f12b31cf81b5d34d6d72b9fdfe2d27de2973da5`.

Growth timing was **not run or implemented at that checkpoint**. Next, inspect
and measure this baseline with two independent launches, each with both
cohorts and seven samples, covering all ten cells (0/1 policy-labelled;
16/256/4096 matched); reserve paired ABBA for an actual candidate. Keep the
1 ms floor for real appends, 10% spread/peer-drift bounds and disjoint-range
target. A separately labelled duration-only pilot starts at 64 MiB with one
sample; settle the final copied-bytes budget plus fixed per-call allowance
only from real-operation durations, without ratio tuning. Bound context
footprint, recreate every full owner before its next measured append, and
keep fill, post-oracle and destruction outside the clock. Read-only snapshot
controls remain raw, labelled and unsubtracted; their tiny durations are not
subject to the real-append floor. Neither this checkpoint nor the completed
spare path establishes whole append or whole Vector completion.

The timing loop is now implemented in the same driver. Its sizing unit is
`observed_initial_capacity * element_bytes + 256` bytes; the 256 B allowance
sizes operation counts, not an estimated allocation cost or time correction.
Contexts per clock batch are `max(1, min(1024, floor(1048576 / unit)))`;
cycles are `ceil(work_bytes / (contexts * unit))`. Each owner is prepared full
outside the clock and recreated before its next append. The clock encloses
only the common external append-one calls and returned-length stores;
snapshot controls use the same contexts and retain their raw durations.
Rows also retain observed initial/post capacity, old logical payload and
C descriptor bytes, cycles, budget and allowance. These byte columns do not
claim total live allocator footprint.

Before choosing the final budget, run exactly one duration-only pilot:
`vector-costs-timed growth-api-measure 67108864 1`, using the existing
`.build/append-growth-baseline/ecosystem` image under the shared check guard.
Retain all 120 rows (ten cells, two cohorts, three peers, real/snapshot);
review only real-append durations against the 1 ms floor. No peer ratio,
outlier removal or favourable order selects the budget. The maintained
`ecosystem-append-growth-measure` target accepts `GROWTH_WORK` and
`ECO_REPEATS`; its default budget is this 64 MiB pilot budget.

The first coarse-clock pilot exits0/1.71 s and retains 120 rows in scratch
`ecosystem/append-growth-duration-pilot.csv`; it is unqualified for
submicrosecond windows. A guarded one-shot clock probe exits0/0.43 s:
`CLOCK_MONOTONIC` reports 1000 ns resolution and 1000 ns minimum nonzero delta;
`CLOCK_MONOTONIC_RAW` reports 42 ns resolution and 41 ns minimum nonzero delta
over 200,000 reads each. Neither goes backwards. Summing many approximately
200 ns windows from the former clock does not qualify them; no performance
ratio conclusion is drawn from that pilot.

Darwin now uses RAW in the existing `nanos`; Linux retains MONOTONIC and
Windows retains QPC. Growth qualification observes 200,000 actual `nanos`
reads, requiring nondecreasing values and a minimum nonzero delta at most
100 ns. Quantizing those same readings to 1000 ns must fail the precision
check. These are measurement checks, not language acceptance rules. After
build/check, run one fresh RAW-clock 64 MiB/one-sample duration-only pilot;
preserve the coarse pilot and all new snapshot controls.

The first RAW build exits2 before execution because the POSIX feature macro
hides the Darwin clock extension. Its failed source/log remain in scratch;
the repair exposes Darwin APIs with `_DARWIN_C_SOURCE`, preserving Linux and
Windows clock selection.

Before formal timing, the optional fourth measure argument
(`GROWTH_LARGE_WORK` in Make) may enlarge only the 256 B/capacity4096 budget.
It applies equally to all three peers; the other nine cells keep the base
budget. Counts, context formula and sampling orders remain unchanged. This
duration-only adjustment avoids multiplying already-long cells by the
largest cell's needed work. The RAW pilot must be reviewed before either
final budget is chosen.

The RAW pilot exits0/1.74 s with 120 unique rows; nine cells' minimum real
append durations exceed 1 ms. At 256 B/capacity4096 the fastest real sample
is 10,704 ns, so duration-only calibration selects base `67108864` bytes and
large-cell `8589934592` bytes (128 times base, the first power of two raising
that observed duration above 1 ms). All three peers receive the same budget
and retain the fixed context formula. The coarse and RAW pilots remain
distinct scratch CSVs, without peer-ratio conclusions.

Before formal data, fix the reviewed clock-check bypass: the measurement
entry checks precision before any setup/measurement; the existing quantized
negative still exercises that check. Diagnostics go to stderr, preserving CSV stdout.
After rebuilding and checking, run exactly two independent baseline launches:
`vector-costs-timed growth-api-measure 67108864 7 8589934592`. Preserve scratch
`ecosystem/append-growth-raw-baseline-{1,2}.csv` and their guard logs, 840 rows
per launch (ten cells, two rotating cohorts, seven samples, three peers,
real/snapshot). No retry, outlier filtering, control subtraction or favourable
order selection follows an adverse result. Only after both launches, report
each peer's medians/ranges for six matched cells separately from four 0/1
capacity-policy cells. Require all real samples at least 1 ms, cohort-ratio
spread and peer drift at most 10%, and WF sample ranges disjoint below the
slower ordinary peer in both cohorts and both launches; otherwise the cell
does not pass. Both peer comparisons and all controls remain visible.

Current scope remains Vector append's spare/full paths. Reserve, insert,
remove, drain and whole-container timing wait until append comparison is done.


#### Growth-append RAW baseline: two launches, append remains open

Both fixed launches exit 0: guards 119.28/118.92 s, after final incremental
build/growth/spare checks exit 0 in 1.08/0.77/0.11 s (outer 2.06 s). Each
launch retains all 840 unique rows; no retries or data filtering occurred.
The measurement checkpoint also passes guarded `make static` in 33.04 s;
it does not repeat the compiler's full canonical gate or claim completion.
The [combined baseline CSV](ecosystem-append-growth-raw-baseline-samples.csv)
retains 1,680 rows with a launch column; the [clock-pilot CSV](ecosystem-append-growth-clock-pilot-samples.csv)
retains both distinct 120-row pilots, including the unqualified coarse-clock samples.
[Timing and identities](ecosystem-append-growth-raw-baseline-timing.txt)
retain both launch logs, instrument/build observations and all 22
source/image/native/runtime pins, equal across launches and unchanged after
execution. The timed image is SHA256
`71fac4c6dab4625cb6b1acb75b44970519e9b55790c104ab36641790c88a6b82`.
These files serve this append baseline and clock repair; retain them while
this evidence is cited, retiring them only with a superseding retained record.

The tables give the span of four launch/cohort medians, then the full range
of all 28 samples per peer in brackets; ratios span those four paired
medians. Times are raw elapsed/operation, with no snapshot subtraction.

| Payload / old capacity | WF ns/op | Rust ns/op | C++ ns/op | WF/Rust | WF/C++ | Slower-peer target |
|---|---:|---:|---:|---:|---:|---|
| 8 B / 16 | 28.26–28.44 [28.09–29.75] | 36.84–37.05 [36.60–38.98] | 25.60–25.74 [25.48–27.01] | 0.7634–0.7720 | 1.1015–1.1050 | Pass |
| 8 B / 256 | 91.18–94.43 [90.36–101.25] | 102.30–103.07 [101.56–110.73] | 91.96–94.96 [91.33–99.41] | 0.8873–0.9181 | 0.9687–1.0174 | Pass |
| 8 B / 4096 | 2212.02–2223.44 [2105.96–2353.51] | 639.54–752.99 [624.17–819.36] | 620.42–630.76 [615.55–718.60] | 2.9476–3.4715 | 3.5111–3.5785 | Not pass; peer drift |
| 256 B / 16 | 143.80–148.03 [142.17–160.80] | 150.53–156.64 [148.86–939.48] | 139.91–141.62 [139.05–265.56] | 0.9367–0.9652 | 1.0178–1.0505 | Overlap |
| 256 B / 256 | 1533.81–1539.10 [1506.32–1630.99] | 1315.17–1320.90 [1307.86–1388.49] | 1307.94–1324.40 [1297.30–1402.34] | 1.1620–1.1691 | 1.1607–1.1739 | Slower |
| 256 B / 4096 | 17548.30–17875.39 [16840.32–18424.21] | 153.64–162.10 [149.50–163.47] | 14786.60–14870.32 [14699.29–14901.03] | 108.8995–114.9201 | 1.1856–1.2089 | Slower |

All real-append samples exceed 1 ms (minimum 1.224527 ms). Cohort-ratio
spreads are at most 3.030%; peer launch drift is at most 5.504% in nine cells.
Scalar4096 Rust cohort1 drifts 17.740%, exceeding the registered 10% bound,
so the full baseline is **not globally qualified**. Its large WF gap remains
visible, without a qualified loss claim from that unstable cell. Only
scalar16 and scalar256 pass the matched slower-peer target (Rust is slower
there); wide16 overlaps, and wide256/4096 are disjoint slower. No append
completion or production selection follows.

The four policy cells preserve ordinary growth outputs: from old0, WF/C++
produce capacity1 while Rust produces4; from old1, WF/C++ produce2 while
Rust produces4. Their times include those different ordinary policies:

| Payload / old capacity | WF ns/op | Rust ns/op | C++ ns/op | WF/Rust | WF/C++ | Slower-peer target |
|---|---:|---:|---:|---:|---:|---|
| 8 B / 0 | 16.77–16.83 [16.67–17.29] | 11.80–11.98 [11.76–12.54] | 10.96–11.02 [10.92–11.38] | 1.4044–1.4227 | 1.5250–1.5305 | Slower |
| 8 B / 1 | 19.60–19.77 [19.47–20.47] | 32.35–32.49 [32.21–34.08] | 19.11–19.16 [19.00–19.82] | 0.6031–0.6113 | 1.0239–1.0318 | Disjoint faster |
| 256 B / 0 | 26.90–27.15 [26.82–28.02] | 22.40–22.64 [22.27–23.64] | 15.64–15.69 [15.57–16.11] | 1.1982–1.2009 | 1.7159–1.7359 | Slower |
| 256 B / 1 | 40.45–40.98 [40.17–41.75] | 52.71–52.98 [52.42–55.68] | 42.92–43.27 [42.70–45.41] | 0.7643–0.7775 | 0.9349–0.9472 | Disjoint faster |

Native code permits Rust realloc; these elapsed rows do not establish its
in-place frequency. Allocation-account observations remain separate from
these timed images. Scope stays Vector append's spare/full paths; the next
API and whole-Vector trace wait for append qualification.


#### Full-run realloc discriminator: rejected

The blanket full-Slots realloc candidate fails the fixed append target; it
is not selected. This is one provisional candidate launch against ordinary
peers, not a paired before/after qualification. It exits 0 under a 119.70 s
guard, retaining all 840 unique rows in [raw samples](ecosystem-append-growth-realloc-discriminator-samples.csv).
[Timing, identities and accounting](ecosystem-append-growth-realloc-discriminator-timing.txt)
retain the build/check log, launch log and all 30 observer rows. Replay uses
[the refused compiler patch](full-slots-reallocation.patch), a separate
`BUILD=.build/full-slots-reallocation`, `WF_FULL_SLOTS_REALLOC=1`, and the
same `growth-api-measure 67108864 7 8589934592` command. Keep these files while
this refusal is cited; retire them with a superseding retained record.

The frozen CLI is SHA256
`dd87ce830d631b75e1e9456134010fd3596e6ed28fcea036f147eaa74ae9d976`;
timed image `63500dad64d4f54a55f2c2d4be1e9b6e99a2eeab1484b20630b759df259caebf`.
Construction/growth/spare/account stages exit 0 in 2.81/0.86/0.11/0.11 s
(outer 4.09 s). Value/state/allocation/clock faults and allocator-byte/ledger
faults fail as required. Every WF append has one realloc request and no
separate release inside the operation; complete destruction leaves no live
backing. Observed images reallocate the allocator's original header, never
the shifted payload pointer. Their exact source-derived formulas retain
unchanged C-control allocation expectations; default flag 0 keeps baseline
compiler replay.

Native append entries keep 32 B frames and the grow helper 64 B. The taken
full edge calls realloc on the Slots header and avoids explicit prefix
memmove/free; the helper's partial branch still contains allocation,
transfer and release. Wide construction remains in its final slot. Timed
images have no observer hooks; peers and 12 runtime objects match baseline
bytes. The driver differs only in a cold clock-assertion line-number
immediate, 395→494; all other operand instructions match.

All real durations exceed 1 ms (minimum 1.305209 ms); maximum cohort-ratio
spread is 3.307%. All six matched cells' WF medians exceed their slower
ordinary peer in both cohorts. None passes the disjoint faster-range rule:

| Payload / old capacity | WF ns/op c0 / c1 | Rust ns/op c0 / c1 | C++ ns/op c0 / c1 | Slower-peer sample ranges c0 / c1 |
|---|---:|---:|---:|---|
| 8 B / 16 | 40.84 / 40.98 | 36.92 / 36.87 | 25.55 / 25.62 | slower / slower |
| 8 B / 256 | 109.55 / 108.97 | 104.07 / 103.83 | 93.47 / 93.76 | overlap / overlap |
| 8 B / 4096 | 3871.80 / 3902.82 | 692.65 / 685.81 | 655.57 / 652.29 | slower / slower |
| 256 B / 16 | 178.47 / 176.25 | 148.69 / 151.69 | 143.54 / 142.85 | slower / slower |
| 256 B / 256 | 1449.12 / 1448.47 | 1330.64 / 1320.21 | 1313.97 / 1343.40 | slower / overlap |
| 256 B / 4096 | 19102.19 / 19037.11 | 163.58 / 164.76 | 15079.54 / 15085.95 | slower / slower |

Ordinary 0/1 growth-policy differences remain, with all samples retained:

| Payload / old capacity | WF ns/op c0 / c1 | Rust ns/op c0 / c1 | C++ ns/op c0 / c1 | Slower-peer sample ranges c0 / c1 |
|---|---:|---:|---:|---|
| 8 B / 0 | 29.86 / 29.72 | 11.76 / 11.73 | 10.94 / 11.00 | slower / slower |
| 8 B / 1 | 18.54 / 18.72 | 32.38 / 32.55 | 19.31 / 19.36 | faster / faster |
| 256 B / 0 | 39.79 / 39.68 | 22.59 / 22.71 | 15.81 / 15.85 | slower / slower |
| 256 B / 1 | 53.24 / 53.21 | 53.03 / 52.96 | 43.10 / 43.05 | overlap / overlap |

Scalar4096 is 3871.80–3902.82 ns versus the retained baseline median span
2212.02–2223.44 ns; wide4096 is 19037.11–19102.19 ns versus 17548.30–17875.39 ns.
These historical comparisons are provisional and do not isolate a cause.
Paired qualification was deliberately not run after this fixed-target
failure. No realloc in-place frequency or allocator/header cause is inferred.
Append remains open; no other API or whole-Vector timing advances.

#### Append allocation extent and payload offset: registered discriminator

This plain-C attribution isolates six controls at old capacity 4096 and
8 B/256 B elements: malloc/copy/free or realloc, crossed with allocation
extra/payload offset (0,0), (16,0), (16,16). All call one external noinline
runtime-argument kernel, separately compiled without LTO; external descriptors
hold all metadata. Each append doubles capacity, preserves the entire old
payload and constructs one element. Setup, full content/state oracle and
cleanup lie outside the raw timed grow-and-append intervals. Native code must
show one shared kernel before timing. Last-old-payload corruption, wrong post
capacity and 1 us clock quantization must each fail their respective check.

Before measurement, fix contexts=max(1,min(1024,1048576/(old_payload+256))),
shared across all six arms; work is 64 MiB scalar and 8 GiB wide, with
cycles=ceil(work/(contexts*(old_payload+256))). Use seven samples and two
reverse/rotated cohorts, retaining every row without pilots or retries.
Darwin RAW clock must be nondecreasing with actual minimum nonzero increment
at most 100 ns. Each accumulated real sample must reach 1 ms; otherwise that
sample is unresolved. No interval subtraction is used. Disjoint sample
envelopes in the same direction in BOTH cohorts support an extent or offset
effect for this program; overlap in either cohort leaves that comparison
unresolved. Compare (0,0) with (16,0) for extent, and (16,0) with (16,16) for
offset, separately by route and element size. This is neither a causal share
of the WF gap nor a source-program performance gain.

A separate untimed observation uses allocator-requested extents unchanged,
Darwin malloc_size and an integer address captured before growth; it reports
whether the address moved, not physical bytes copied. The source
[append-allocation-layout.c](append-allocation-layout.c), its opt-in Make
targets and retained raw samples/log serve this allocation discriminator and
are retired with its superseding experiment. Ordinary WF/peer inputs and the
compiler are unchanged; the cached rejected compiler is not used.

The fixed launch retained [all 168 unique raw rows](append-allocation-layout-samples.csv)
and [build/fault/native/observation/timing records](append-allocation-layout-timing.txt).
It ended in 64.12 s with exit 1: twelve wide realloc(0,0) samples were below 1 ms
(minimum 0.895879 ms). Those samples and comparisons involving them remain
unresolved; there was no retiming. Other cells passed the duration threshold.
Scalar used 31 contexts × 66 cycles (2046 operations/sample); wide used 1 × 8191.
The RAW clock minimum nonzero increment was 41 ns. Both element widths and
all six arms passed the full content/state check; deliberate last-old-payload,
capacity and clock faults failed at their intended checks. Initial compilation
failed because strict POSIX feature selection hid Darwin declarations; the
retained log includes that failure and the corrected successful build.

The table gives cohort 0 / cohort 1 medians, in ns/append, followed by each
cohort's full seven-sample envelope. Route 0 is malloc/copy/free; route 1 is
realloc. All intervals include clock/call/loop overhead without subtraction.

| Element | Route | Extra / offset | Median c0 / c1 | Envelope c0 / c1 |
|---|---:|---|---|---|
| 8 B | 0 | 0 / 0 | 642.49 / 631.89 | 628.09–729.45 / 618.50–655.63 |
| 8 B | 0 | 16 / 0 | 2108.99 / 2056.65 | 2037.45–2219.80 / 2035.37–2152.60 |
| 8 B | 0 | 16 / 16 | 2171.78 / 2142.41 | 2127.18–2185.83 / 2111.56–2235.89 |
| 8 B | 1 | 0 / 0 | 686.77 / 679.15 | 671.57–712.85 / 673.00–762.42 |
| 8 B | 1 | 16 / 0 | 4070.65 / 3959.35 | 3954.39–4151.94 / 3945.12–4205.79 |
| 8 B | 1 | 16 / 16 | 4069.63 / 3968.70 | 3964.55–4113.43 / 3933.65–4215.38 |
| 256 B | 0 | 0 / 0 | 22773.06 / 22739.87 | 22628.57–23371.88 / 22714.01–22896.40 |
| 256 B | 0 | 16 / 0 | 22863.62 / 22800.49 | 22726.44–23162.89 / 22692.90–23063.87 |
| 256 B | 0 | 16 / 16 | 23141.12 / 23186.79 | 22862.65–23264.31 / 23054.12–26869.81 |
| 256 B | 1 | 0 / 0 | 110.85 / 111.00 | 109.37–125.77 / 109.78–163.98; under-duration |
| 256 B | 1 | 16 / 0 | 23172.58 / 23200.82 | 22990.97–23391.44 / 23119.24–23347.57 |
| 256 B | 1 | 16 / 16 | 23550.18 / 23442.90 | 23373.26–23589.26 / 23349.53–23602.86 |

Adding 16 allocation bytes at fixed offset 0 produces disjoint slower envelopes
in both scalar cohorts, for both allocation routes. The scalar offset change
has overlapping envelopes on both routes. Wide malloc/copy/free extent and
offset comparisons overlap. Wide realloc extent remains unresolved by the
duration criterion; its offset comparison overlaps in cohort 0, so also remains
unresolved. Thus this diagnostic supports a scalar allocation-extent effect,
without establishing an offset effect or resolving the wide-cell question.

The separate untimed trace contains one observation per arm, not a movement
frequency. Scalar requests 32768→65536 have usable sizes 32768→65536;
requests 32784→65552 have usable sizes 49152→81920. Wide requests 1048576→2097152
have usable sizes 1048576→2097152; adding 16 bytes gives 1064960→2113536.
All malloc/copy/free observations moved. Realloc moved for scalar(0,0) and
wide(16,0)/(16,16), and kept the address for scalar(16,0)/(16,16) and wide(0,0).
These are separate single traces, not the timed operations' allocation
histories, and do not measure physical copying.

This diagnostic used Clang -O2, while the actual API harness uses -O3. Native
inspection before timing found one shared kernel, runtime route/extent/offset
arguments, external malloc/realloc/memmove/free calls and the same kernel call
inside the timed loop. A later isolated -O3 helper build, without retiming,
showed matching successful-path instructions and relocations through return;
the sole instruction difference before return targets the cold malloc-failure
block, whose layout differs. The literal vector contents match at different
section addresses. The caller was not compared at -O3. No actual WF program
was changed, no fraction of its measured gap is attributed, and no compiler
optimization or peer-performance result follows from these C controls.

Reproduce from the repository root (no Whitefoot compiler build required):

```sh
perl .github/run-check.pl append-layout-build make -C research/experiments/container-representation/vector-library -j2 append-layout-check
nm research/experiments/container-representation/vector-library/.build/append-allocation-layout/append-layout
otool -tvV research/experiments/container-representation/vector-library/.build/append-allocation-layout/helper.o
otool -rv research/experiments/container-representation/vector-library/.build/append-allocation-layout/caller.o
perl .github/run-check.pl append-layout-observe research/experiments/container-representation/vector-library/.build/append-allocation-layout/append-layout observe
perl .github/run-check.pl append-layout-measure sh -c 'research/experiments/container-representation/vector-library/.build/append-allocation-layout/append-layout measure > /private/tmp/append-layout-replay.csv'
```

#### Separate payload allocation: registered actual-WF discriminator

The allocation-extent result reopens a measurement question, not a shipping
representation choice: does removing header bytes from the element allocation
make the existing WF append source competitive? The bounded prototype keeps a
runtime Slots Box as a one-word owner pointing to a stable descriptor containing
length, capacity and a payload pointer; only the payload allocation grows. Other
shape representations and source programs remain unchanged. This deliberately
violates the current TYPE-9/STOR-1 single-allocation representation and is a
throwaway compiler variant, never an admitted implementation claim. A fat owner
would additionally change OP-9's Box ceiling and is outside this discriminator.
The extra descriptor allocation and pointer access are costs to retain, not
optimize away by a benchmark-only Vector special case.
Zero-byte payloads retain an allocated one-byte placeholder: existing range
parameter lowering states `nonnull` even for empty ranges. A null payload
would contradict that ABI; weakening those attributes would add another
variable to the experiment. Positive payload extents remain exactly cap times
stride. The allocator ledger includes both allocations and the placeholder.

Before any timing, require complete existing append value/state checks plus
nested-owner cleanup, zero-capacity and zero-stride behavior, and allocation
failure before publication. Adapt the observer to the explicit two-allocation
layout without weakening its ownership checks. Inspect actual native append and
growth bodies for payload-only allocation sizes and unchanged old-prefix copy.
Use the existing O3 growth API caller, inputs, peers, fixed work, two cohorts and
seven samples for one screen; keep all samples and the direct terminal status.
A candidate merits paired baseline/candidate qualification only if every matched
cell beats the median-slower peer in both cohorts and satisfies the existing
instrument criteria. A failed cell stops that qualification; it does not trigger
extra samples. The zero/one-capacity policy differences remain separately
labelled. No causal share, general representation selection or whole-Vector gain
follows from a successful screen. Construction/destruction and wider owner ABI
costs remain separate API obligations before choosing a library representation.

The actual-WF screen did not pass the registered target. The compiler and
observer edits were restored after preserving the [exact prototype patch](split-slots-payload.patch),
[840 unique samples](ecosystem-append-growth-split-payload-samples.csv), and
[build, native qualification, allocation and timing evidence](ecosystem-append-growth-split-payload-timing.txt).
The patch reconstructs four compiler files, the candidate-only observer changes,
and two temporary native-fixture inputs byte-for-byte from
`f4fc3d5d5ad7715da3e48c200e04ebc06113e982`. It is deliberately nonconforming to
TYPE-9/STOR-1 and is not a compiler change selected for adoption. Its temporary
fixtures live under the ignored experiment build directory when applied.

The gate-profile CLI build took 15.87 s. Existing O3 API build, growth/spare
checks and accounting passed in 3.27 s. A separate native observer verified
nested Box contents and exact releases, partial-prefix copying without copying
spare bytes, zero capacity, zero stride, empty-range calls and explicit empty
release. Forced allocation failure exited 73 after checking the unchanged old
owner, descriptor and payload; permuting two valid nested Box pointers exited 1
at the source-content assertion. Normal corrected execution exited 0. These are
default-source-lowering/O3 checks, not a new three-mode or canonical-gate claim.
The retained authoring failures were an unresolved fixture spelling, a missing
read effect, a duplicate native entry symbol, and a floor-hook binding error
that called the real abort (134); their corrected checks are separately retained.
No production rule or expected verdict changed to repair those fixture errors.

The native growth functions allocate exactly new capacity times element stride,
copy the old initialized prefix, free the old payload, and update the stable
24-byte descriptor. There is no header addition in the allocation request.
The account image verifies one allocation and one release per measured WF
growth; its live bytes are payload plus 24, and the zero-capacity placeholder
is one byte. The descriptor and placeholder allocations occur during preparation outside
append timing; their costs still belong to the later construction/destruction
API comparison.

The fixed screen exited 0 in 118.83 s. All 420 real intervals exceeded 1 ms
(minimum 1.286459 ms); all 420 snapshot controls are also retained without
subtraction. The RAW clock minimum increment was 41 ns. The following values
are median [minimum–maximum] ns/append. A row passes only when WF's entire
sample range is below that cohort's median-slower peer's range. Both cohorts
must pass for a cell to qualify for further comparison.

| Element / old capacity | Cohort | WF | Rust | C++ | Screen |
|---|---:|---|---|---|---|
| 8 B / 16 | 0 | 23.11 [22.96–23.95] | 36.69 [36.63–37.03] | 25.48 [25.32–26.35] | Pass |
| 8 B / 16 | 1 | 22.95 [22.84–23.26] | 36.83 [36.63–38.13] | 25.62 [25.45–26.65] | Pass |
| 8 B / 256 | 0 | 89.63 [88.62–93.23] | 101.64 [101.02–113.42] | 96.15 [93.17–100.20] | Pass |
| 8 B / 256 | 1 | 89.32 [88.79–92.21] | 102.34 [100.85–106.03] | 93.62 [91.76–101.35] | Pass |
| 8 B / 4096 | 0 | 709.25 [663.04–806.00] | 636.57 [628.77–654.51] | 663.27 [643.78–738.82] | Overlap |
| 8 B / 4096 | 1 | 710.23 [669.54–764.83] | 637.46 [629.40–707.87] | 646.95 [643.69–671.23] | Overlap |
| 256 B / 16 | 0 | 141.59 [139.43–151.73] | 152.25 [149.75–157.31] | 143.29 [139.70–148.99] | Overlap |
| 256 B / 16 | 1 | 139.94 [139.79–143.80] | 151.40 [149.70–156.69] | 141.23 [138.87–153.18] | Pass |
| 256 B / 256 | 0 | 1324.27 [1300.96–1375.16] | 1319.04 [1313.56–1328.74] | 1322.59 [1302.37–1347.59] | Overlap |
| 256 B / 256 | 1 | 1321.46 [1303.38–1420.45] | 1319.17 [1312.12–1406.68] | 1321.62 [1312.20–1342.39] | Overlap |
| 256 B / 4096 | 0 | 14876.90 [14809.92–15355.12] | 159.86 [157.73–162.04] | 14840.87 [14812.25–15110.07] | Overlap |
| 256 B / 4096 | 1 | 14834.67 [14789.57–14950.12] | 160.53 [158.25–165.94] | 14842.57 [14820.48–14888.50] | Overlap |

Only scalar capacities 16 and 256 pass both cohorts. At scalar 4096 the WF
medians remain above both peers; wide 256/4096 now overlap C++ rather than
establishing a repeatable advantage. No paired baseline/candidate qualification
was launched. This single screen does not establish a baseline speedup, a
causal share of the earlier gap, or a generally preferred representation.
Rust's much cheaper wide-4096 reallocation path remains a separate growth-route
lead now that payload allocation extents match. The prototype is preserved for
that next discriminator, not adopted as the language representation.

Capacities zero and one retain their different initial-growth policies and are
reported separately. Entries are cohort 0 / cohort 1 medians, ns/append:

| Element / old capacity | WF | Rust | C++ |
|---|---|---|---|
| 8 B / 0 | 17.91 / 17.88 | 11.81 / 11.93 | 11.41 / 11.35 |
| 8 B / 1 | 16.63 / 16.63 | 32.26 / 32.49 | 19.20 / 19.18 |
| 256 B / 0 | 21.23 / 21.76 | 22.59 / 22.50 | 15.84 / 15.74 |
| 256 B / 1 | 36.92 / 37.18 | 53.32 / 53.13 | 43.36 / 43.43 |

Frozen identities (SHA-256):

- Prototype patch: `9a093df2fa61675e056a33c96b68128e326e6b8b97a27c5bf983adb0d282e14e`.
- Candidate CLI: `15fd4c3d66c7d477ecf9717bf5ea392d6c4dd96409391565a0f1f67a7452a302`.
- Timed image: `5724678f4be5c1c67ccb418d2d3c3b682c0b002875b07416b569e702ddd4fa16`.
- Samples: `3ad3c8617ba9ae2bef5124b7d222d859dc150267f28d5baf897a3487275392b8`.

Reproduce the screen in a fresh checkout of the named base: apply the preserved
patch, build the gate-profile CLI with at most two jobs under the repository
guard, then invoke the existing Vector experiment targets with an isolated
`BUILD`, that CLI as `WHITEFOOTC`, and
`ECO_CFLAGS='-std=c11 -O3 -Wall -Wextra -Werror -DWF_SPLIT_SLOTS_PAYLOAD=1'`.
Run `ecosystem-build ecosystem-append-growth-check ecosystem-append-check
 ecosystem-append-growth-account` before invoking the timed image as
`growth-api-measure 67108864 7 8589934592`, also guarded. The retained log records
the actual configuration and commands; the ordinary targets without the explicit
candidate macro continue to check the baseline representation.

For the focused native fixture, emit the patch's `focused-fixture.wf` to LLVM.
In the instrumented copy only, bind malloc/free calls to the observer functions,
replace the private resource-abort definition with an external declaration of
`wf_observe_abort`, and rename the generated main/entry symbols so the observer
owns the entry. Compile with the preserved `observer.c` and the ordinary runtime
objects using the exact O3 command in the log. Run normally, with `failure`, and
with a second build defining `CORRUPT_PRESERVED_ORDER`; require exits 0, 73 and 1,
respectively. This instrumentation observes the candidate; it is not a native
implementation or language mechanism. Retire these prototype artifacts with
their superseding representation experiment.

#### Full-run reallocation of the separate payload: registered next discriminator

The preceding split-payload screen leaves wide growth near C++ and far behind
Rust's reallocation route. The earlier rejected realloc variant requested
header-plus-payload bytes; this variant instead tests realloc at the now-matched
payload extent, keeping the same stable descriptor, source, capacity policy,
O3 API harness and peers. This changed extent is the reason to reopen that
experiment, not an adoption of the rejected blanket representation.

Only a full runtime Slots run (len equals cap) uses realloc of the payload.
Partial runs retain the previous malloc/initialized-prefix-copy/free path.
Both routes preserve descriptor identity and check allocation failure before
publishing the payload pointer or capacity. Zero extents retain the allocated
one-byte placeholder and nonnull range ABI. The prototype remains explicitly
nonconforming to TYPE-9/STOR-1; no specification or live-tree change is selected.

Before timing, require the existing API checks and an observer that forces both
in-place and moving reallocation, validates nested-owner contents and releases,
keeps the partial-run spare-byte check, and rejects the deliberate content fault.
A failed grow must leave the old owner, descriptor and payload intact before the
floor observer. The candidate-only allocation ledger distinguishes realloc from
fresh allocation; ordinary baseline expectations remain unchanged. Inspect the
actual native full branch for payload-only realloc and absence of an explicit
prefix copy/free; partial growth must still retain its copy/free path.

Run one screen with the same fixed command `growth-api-measure 67108864 7
8589934592`, two cohorts and seven samples, retaining every row. The previous
instrument and disjoint-range criteria still apply to every matched cell.
Proceed to paired qualification only if all matched cells pass; otherwise stop
qualification and attribute the remaining cells. Do not retry a failed screen,
subtract controls, infer a paired speedup, or select a representation from it.


Outcome: the registered screen ran once and did not qualify the complete API.
The gate-profile CLI build took 15.56 s; existing API build, correctness and
accounting checks passed in 3.37 s. The focused native observer passed all
12 in-place/moving cases (nested owners, partial growth, zero capacity, zero
stride, empty range and empty release). The failed realloc route exited 73
with the old owner, descriptor and payload intact; the deliberate nested-owner
order fault exited 1. The normal run exited 0. These are default source
lowering/O3 witnesses; the observer simulates allocator outcomes in small
physical allocations, not real allocator performance or a full semantic gate.

The full native growth path calls realloc on the payload, requesting exactly
new capacity times element stride, with no explicit prefix copy or free.
The partial path retains malloc, initialized-prefix memmove and free. Both
helpers retain a 64-byte frame and a len-versus-cap recheck. All ten WF account
rows have one request, one realloc and no explicit release; live storage is
payload plus the stable 24-byte descriptor. Constructor/placeholder allocations
remain outside this append interval and remain future API obligations.

The fixed screen exited 0 in 117.36 s. All 840 rows are retained: 420 real
intervals (minimum 1.270505 ms) and 420 snapshot controls, with no subtraction.
The RAW clock minimum increment was 41 ns. Values below are median
[minimum–maximum] ns/append; PASS requires the entire WF range to be below
the cohort's median-slower peer range, in both cohorts for a matched cell.

| Element / old capacity | Cohort | WF | Rust | C++ | Screen |
|---|---:|---|---|---|---|
| 8 B / 16 | 0 | 35.22 [35.09–36.08] | 36.90 [36.70–38.19] | 25.48 [25.36–26.99] | PASS |
| 8 B / 16 | 1 | 35.26 [35.00–36.65] | 36.76 [36.66–38.28] | 26.00 [25.38–26.53] | PASS |
| 8 B / 256 | 0 | 102.03 [101.18–106.03] | 102.61 [102.05–115.27] | 93.63 [91.95–99.58] | OVERLAP |
| 8 B / 256 | 1 | 101.47 [100.52–107.78] | 104.44 [102.55–108.51] | 93.25 [91.68–94.08] | OVERLAP |
| 8 B / 4096 | 0 | 750.63 [691.39–925.79] | 684.28 [671.41–738.80] | 653.00 [642.06–704.41] | OVERLAP |
| 8 B / 4096 | 1 | 712.73 [694.22–772.81] | 676.81 [666.63–719.37] | 655.43 [647.66–685.34] | OVERLAP |
| 256 B / 16 | 0 | 151.68 [146.66–170.79] | 148.22 [146.67–157.94] | 139.63 [138.03–146.48] | OVERLAP |
| 256 B / 16 | 1 | 148.38 [147.08–158.78] | 147.13 [145.91–159.58] | 139.16 [138.32–146.95] | OVERLAP |
| 256 B / 256 | 0 | 1322.22 [1313.01–1363.93] | 1315.34 [1308.50–1356.24] | 1317.48 [1291.87–1376.25] | OVERLAP |
| 256 B / 256 | 1 | 1319.00 [1315.82–1414.08] | 1323.55 [1309.30–1416.74] | 1319.32 [1295.54–1372.78] | OVERLAP |
| 256 B / 4096 | 0 | 159.47 [155.11–161.08] | 162.86 [161.31–165.77] | 14887.86 [14823.51–14932.98] | PASS |
| 256 B / 4096 | 1 | 156.37 [155.42–157.13] | 161.45 [160.31–163.52] | 14856.73 [14819.50–14902.17] | PASS |

Only scalar capacity 16 and wide capacity 4096 pass both cohorts. Wide-4096
WF is now near Rust and well below C++ in this screen, while scalar 256/4096
and wide 16/256 do not establish a repeatable advantage over the slower peer.
The complete append API therefore remains unqualified; no paired
baseline/candidate qualification was run and no representation is selected.
The difference from the preceding independent malloc/copy/free screen is not
reported as a paired speedup or causal percentage. Matching the payload
extent and realloc route is useful evidence for large growth, not grounds for
blanket adoption across sizes, construction/destruction or other APIs.

Initial-growth policy cells remain separate:

| Element / old capacity | Cohort | WF | Rust | C++ | Screen |
|---|---:|---|---|---|---|
| 8 B / 0 | 0 | 18.63 [18.52–19.78] | 11.86 [11.77–12.30] | 10.95 [10.92–11.31] | LOSE |
| 8 B / 0 | 1 | 18.56 [18.45–19.37] | 11.93 [11.77–12.38] | 11.07 [10.92–11.40] | LOSE |
| 8 B / 1 | 0 | 18.61 [18.55–18.70] | 32.34 [32.24–32.63] | 20.48 [20.37–21.05] | PASS |
| 8 B / 1 | 1 | 18.60 [18.56–19.58] | 32.48 [32.29–33.69] | 20.46 [20.38–20.51] | PASS |
| 256 B / 0 | 0 | 34.97 [34.72–35.98] | 22.76 [22.41–23.01] | 15.61 [15.52–15.86] | LOSE |
| 256 B / 0 | 1 | 34.88 [34.77–35.80] | 22.51 [22.47–23.60] | 15.69 [15.59–16.31] | LOSE |
| 256 B / 1 | 0 | 49.58 [49.29–50.40] | 52.75 [52.40–53.24] | 43.13 [42.99–43.85] | PASS |
| 256 B / 1 | 1 | 49.76 [49.42–51.10] | 52.54 [52.35–53.93] | 43.02 [42.70–44.26] | PASS |

Both zero-capacity cells lose to both peers. WF's allocated nonnull placeholder
and the peers' empty representations differ; this result cannot be hidden by
the matched-capacity table. Capacity-one policies also differ as the retained
account table records. No extra launch was used to seek a favorable result.

Evidence: [prototype patch](split-slots-payload-realloc.patch),
[all samples](ecosystem-append-growth-split-payload-realloc-samples.csv), and
[build/check/account/native/timing log](ecosystem-append-growth-split-payload-realloc-timing.txt).
The patch applies to `8eee797a53211e1086740950797c765a677ae64f`; independent
replay reproduced all seven files byte for byte. It preserves the four compiler
files, candidate ledger and both focused fixture inputs. The five implementation
files were restored to that base before publication; the patch is an explicitly
nonconforming research prototype, not a production compiler change.

Frozen SHA-256 identities:

- Patch: `3f51d23caab18567cc12de07274ea9d357d7651f13afae81b67690b76a8b89c0`.
- Candidate CLI: `1e43a2bec4fdf5e84011777c6838e39917fbebd265c6c97390156cf4209c2b7e`.
- Timed image: `5efc26fed2a63f2d8e6bc7e521bf64b0586491fc181de3f58070100e29673ab0`.
- Samples: `e33a17ab80c94daf7e787816379c62eae472dfea8c80e06bce3ce90de74a73b1`.

Reproduce using the preceding split-payload procedure on the named base with
this patch and isolated BUILD, adding both explicit observer flags
`-DWF_SPLIT_SLOTS_PAYLOAD=1 -DWF_SPLIT_SLOTS_REALLOC=1` to ECO_CFLAGS.
The existing build/check/account targets and fixed timing command are unchanged.
For the focused fixture, additionally bind realloc to wf_observe_reallocate;
the retained commands and observer supply the forced routes. These artifacts
serve the append allocation-route experiment and retire with its superseding
representation evidence. No specification, acceptance rule or live-tree
revision was made. A separately noticed redundant entry-contract branch is
recorded in TODO; it was not changed in this experiment.


#### Single-operation boundary length reuse: registered discriminator

The frozen payload-realloc image exposes one local emitter cost independent
of its proposed representation. Scalar append reads the descriptor length
again after storing the element; wide append does the same after its final
slot stores. The ordinary `emit_run_boundary` computes the touched slot using
one length read, then `move_run_boundary` reads it again. OP-10's one boundary
operation has no intervening user call or release, and the payload is disjoint
from its descriptor (a zero-stride element writes no bytes). Reuse that entry
length for the operation's final length update. This is not caching a measure
across a source call, changing an effect row, or eliminating the post-grow read.

Implement in the shared boundary emitter with its existing Slots/Ring rules,
then require a regression which observes one length read for back placement
and fails with the old implementation. Native complete-content/length and
cleanup checks must still pass, including aggregate and zero-size elements;
existing Ring boundary tests remain obligations because the helper is shared.
Inspect the optimized native single-append body for removal of the post-store
length load, while preserving the growth call and its needed reload.

Measure the changed primitive separately from a layout selection. First use
native correctness and assembly to establish the code change on the production
layout. Then use the same preserved payload-realloc overlay before and after
this emitter change for a paired append-growth comparison, keeping the exact
O3 harness, capacities, policies, seeds and both peer implementations. Record
both binary identities before running. Use the fixed command
`growth-api-measure 67108864 7 8589934592`, first A then B. Retain both
launches and every snapshot control without subtraction or retry. If every
matched B cell passes the existing instrument/drift conditions and its full
WF range is below its median-slower peer's range in both cohorts, continue
with B then A to complete the reverse-order qualification. Otherwise stop
qualification and retain the failed screen. This early-stop staging is
registered before either launch to avoid spending a second pair on an API
that already misses the target. An API win requires both pairs; a narrower
instruction reduction or cell gain cannot qualify the whole API. If native
code retains the load or correctness fails, do not start timing.
Spare append and the remaining APIs stay open; no other family is resumed.


Outcome: the local emitter change removes the redundant read in both production
and overlay native append code. It does not establish a timing gain. The shared
boundary operations now read entry length once and pass it to slot calculation
and descriptor update. No cache survives an ordinary source call; the native
append bodies still reload owner/length after the ordinary growth call as their
representation requires. Public APIs, allocation routes and source acceptance
are unchanged. This small emission cleanup is retained for its directly verified
instruction reduction, not as a qualified performance win or a layout selection.

The new regression covers fixed/runtime Slots with scalar, two-word Array,
zero-size Array and Box elements. The old emitter fails its emitted-code
assertion with two length reads against one expected; all eight instantiations
pass on the new emitter. The existing Ring front-placement check is strengthened
to require no post-payload descriptor reload, preserving its written-head check.
All 27 window tests pass, including native wrapping, zero-size and cleanup cases.
The first authored scalar fixture incorrectly used `move` on a copy value and
was rejected under OWN-1; that failure is retained and is not the counterfactual
proof. The corrected old-emitter assertion is separately recorded. One duplicate
build attempt was refused by the global runner with exit 75 and did not run.

| Stage | Wall time | Result |
|---|---:|---|
| Initial old-control test build / malformed source run | 72.39 s / 5.43 s | Build 0; OWN-1 rejection, not optimization evidence |
| Corrected old-control test build / test execution | 61.39 s / 0.78 s | Build 0; expected assertion failure, two reads versus one |
| Fixed test build / 27 window tests | 18.53 s / 6.71 s | Both exit 0 |
| Production CLI build / API build, checks and accounting | 14.79 s / 3.23 s | Both exit 0 |
| Overlay CLI build / API build, checks and accounting | 14.34 s / 3.14 s | Both exit 0 |
| Overlay forced allocator fixture | Separate subsecond stages in retained log | Normal 0, failed growth 73, content fault 1 |
| Fixed A / B timing launches | 117.18 s / 117.05 s | Both exit 0 |
| Restored-source static checks / all-target Clippy | 32.86 s / 16.12 s | Both exit 0 |

The overlay fixture again checks nested-owner contents/releases, partial-run
spare bytes, zero-capacity/zero-stride and empty ranges under forced in-place
and moving realloc, plus unchanged state before the failed-allocation floor.
Its deliberately permuted valid Box pointers still fail the source-content
assertion. Both production and overlay API checks retain all prior deliberate
value, state, allocator and clock failures; the 30-row ledgers retain their
respective allocation contracts.

Each timing launch has 840 unique rows: 420 real intervals and 420 snapshot
controls, all retained without subtraction. Minimum real durations are
1.276980 ms (A) and 1.273175 ms (B); both clocks have a 41 ns minimum increment.
The driver, C++ object and Rust archive are byte-identical between the paired
images. A/B below mean old/new emitter on the *same A2 representation*, not a
comparison between the production representation and the overlay. Values are
median [minimum–maximum] ns/append. The final columns report B versus A / B
versus its median-slower peer, and the largest Rust/C++ median drift from A.

| Element / old capacity | Cohort | A WF | B WF | B Rust | B C++ | B/A / peer | Peer drift |
|---|---:|---|---|---|---|---|---:|
| 8 B / 16 | 0 | 35.74 [34.91–36.15] | 34.80 [34.63–36.49] | 37.16 [36.62–37.94] | 25.66 [25.38–26.90] | Overlap / Pass | 0.97% |
| 8 B / 16 | 1 | 35.09 [34.91–36.87] | 34.87 [34.68–36.20] | 36.72 [36.61–36.89] | 25.55 [25.41–27.07] | Overlap / Pass | 0.35% |
| 8 B / 256 | 0 | 99.82 [99.62–104.99] | 101.80 [99.75–105.01] | 103.12 [101.26–107.84] | 94.26 [92.42–97.11] | Overlap / Overlap | 3.28% |
| 8 B / 256 | 1 | 100.28 [99.84–106.29] | 101.11 [99.54–104.37] | 102.71 [101.26–104.96] | 92.90 [92.04–93.65] | Overlap / Overlap | 2.13% |
| 8 B / 4096 | 0 | 691.88 [666.99–757.31] | 726.56 [720.55–803.97] | 728.39 [726.25–808.71] | 719.19 [714.75–816.55] | Overlap / Overlap | 14.04% |
| 8 B / 4096 | 1 | 677.30 [660.82–742.34] | 733.63 [713.12–839.91] | 727.88 [720.33–779.33] | 725.70 [711.96–730.14] | Overlap / Overlap | 13.74% |
| 256 B / 16 | 0 | 147.16 [146.07–169.39] | 150.14 [149.41–161.55] | 148.06 [146.70–159.50] | 142.59 [141.89–143.23] | Overlap / Overlap | 2.04% |
| 256 B / 16 | 1 | 150.39 [148.25–155.03] | 150.61 [148.94–160.33] | 147.11 [146.51–153.33] | 142.04 [141.99–145.98] | Overlap / Overlap | 1.25% |
| 256 B / 256 | 0 | 1312.32 [1302.74–1342.50] | 1322.86 [1313.49–1395.29] | 1311.88 [1308.14–1324.84] | 1306.36 [1296.90–1327.09] | Overlap / Overlap | 0.65% |
| 256 B / 256 | 1 | 1320.57 [1307.12–1411.68] | 1311.96 [1307.45–1416.27] | 1315.29 [1313.20–1331.07] | 1312.60 [1299.48–1389.93] | Overlap / Overlap | 0.56% |
| 256 B / 4096 | 0 | 156.97 [156.02–158.46] | 157.88 [156.14–158.37] | 162.52 [159.95–162.85] | 14813.47 [14795.40–14908.36] | Overlap / Pass | 0.51% |
| 256 B / 4096 | 1 | 157.51 [155.90–160.92] | 158.73 [155.44–167.50] | 161.24 [159.80–163.98] | 14796.11 [14753.89–15003.35] | Overlap / Pass | 0.98% |

Every B/A WF sample range overlaps: no favorable or adverse timing change is
qualified. Scalar-4096 additionally fails the 10% peer-drift bound, so its
higher candidate medians are not evidence of an emitter regression. Only
scalar-16 and wide-4096 beat the median-slower peer in both cohorts, as before.
The API criterion therefore fails; the conditional reverse B/A pair was not
run, and no full qualification or speedup percentage is claimed.

Initial-policy cells remain separate and are not rescued by the matched table:

| Element / old capacity | Cohort | A WF | B WF | B Rust | B C++ | B/A / peer | Peer drift |
|---|---:|---|---|---|---|---|---:|
| 8 B / 0 | 0 | 18.51 [18.38–18.74] | 18.45 [18.40–18.67] | 11.77 [11.72–12.32] | 10.96 [10.92–11.38] | Overlap / Lose | 0.42% |
| 8 B / 0 | 1 | 18.45 [18.38–19.14] | 18.41 [18.37–19.12] | 11.74 [11.70–12.17] | 10.95 [10.92–11.45] | Overlap / Lose | 0.82% |
| 8 B / 1 | 0 | 18.63 [18.51–18.74] | 18.61 [18.50–19.42] | 32.31 [32.20–32.83] | 20.49 [20.29–20.90] | Overlap / Pass | 0.40% |
| 8 B / 1 | 1 | 18.58 [18.52–18.76] | 18.62 [18.53–18.67] | 32.33 [32.22–32.44] | 20.42 [20.29–20.53] | Overlap / Pass | 0.30% |
| 256 B / 0 | 0 | 34.86 [34.78–35.33] | 34.75 [34.58–36.73] | 22.31 [22.25–22.90] | 15.74 [15.61–16.39] | Overlap / Lose | 0.81% |
| 256 B / 0 | 1 | 34.84 [34.74–36.39] | 34.65 [34.52–36.32] | 22.44 [22.41–24.04] | 15.71 [15.62–16.19] | Overlap / Lose | 0.72% |
| 256 B / 1 | 0 | 49.49 [49.33–49.87] | 49.37 [49.06–51.46] | 52.75 [52.45–53.90] | 43.46 [43.06–44.64] | Overlap / Pass | 0.46% |
| 256 B / 1 | 1 | 49.63 [49.46–52.03] | 49.25 [49.13–51.41] | 52.77 [52.63–55.85] | 43.17 [42.93–44.69] | Overlap / Pass | 0.28% |

Evidence: [old-emitter samples](ecosystem-append-growth-length-reuse-baseline-samples.csv),
[new-emitter samples](ecosystem-append-growth-length-reuse-candidate-samples.csv),
[checks, ledgers, native bodies and timings](ecosystem-append-growth-length-reuse-timing.txt),
and the [small emission patch](boundary-length-reuse.patch).
For reproduction, use base `d576072ccf3a60950b95e71662e38fc9cbf287c5` plus the
previous `split-slots-payload-realloc.patch` for A; additionally apply the small
emission patch for B. Both use the prior explicit overlay flags and existing
build/check/account targets. The focused native fixture remains the prior
patch's inputs. The small patch serves this paired discriminator and retires
with its superseding experiment; its implementation also lives in the compiler.
The overlay was removed after constructing B; the production source retains
only the shared primitive change and its regression tests. No specification
or live-tree revision is made, and no other API or container is qualified here.

SHA-256 identities:

- Emission patch: `c4bd6ee0d5deecd3b75545f5ed17a495641d82afca163f386ec5a7bcd60c0326`.
- Production CLI: `5e8d37585a06e7b7040b43032b1cd5d4f41cce16d0e91aa77b35e7d0e83c74ac`.
- Overlay CLI: `2b668d006477c4079c036b4baa38049276ec619573a9c583667cbf809679b48d`.
- A timed image: `5efc26fed2a63f2d8e6bc7e521bf64b0586491fc181de3f58070100e29673ab0`.
- B timed image: `b2beba8cf19113948643eba4aeac07a9fbba7191f397deccc530cbc469aafc6c`.
- A samples: `76c7596c5ed827447b86176ac415734eb3104ab73dbe908cbd27fcccfe4fb840`.
- B samples: `100175461d6878cd03c1bf195d77f06810a4ed7b3880b21714bc3c6b62c32d2f`.


#### Inline runtime-Slots owner: registered descriptor-placement discriminator

The append-only sequence continues with one representation variable. Compared
with the frozen split-payload/realloc overlay plus boundary-length reuse,
represent a runtime-capacity `Box<Slots<T>>` by an inline three-word
`{length, capacity, payload}` owner instead of a pointer to a separately
allocated three-word descriptor. Keep the same payload extent, one-byte
allocated empty/zero-stride placeholder, full-run realloc, partial-run
initialized-prefix copy, source, capacity policy and public operations. Do not
introduce nullable-empty storage in this comparison. Fixed Slots and other
shapes keep their existing representation. The hypothesis is that removing
the descriptor-pointer load and descriptor allocation improves append costs;
the falsifier is unchanged native dependencies or timings that still miss the
peer target. Wider owner transport is a cost to measure, not an assumed win.

This is a nonconforming research overlay: TYPE-9/STOR-1/STOR-3, OP-9's
Box ceiling and the storage-representation decision require separate owner
consideration before adoption. No specification, live tree, source acceptance
rule or production layout is changed by retaining its patch. Reuse the existing
stored-aggregate parameter/result machinery rather than a benchmark-specific
ABI. Ordinary and linked signatures must agree. Whole moves, exchanges, nested
cleanup and runtime-content references must refer to the inline descriptor;
cleanup may release its payload but never its stack descriptor.

Before timing, require existing spare/growth API correctness and allocator
accounting, forced moved/in-place/failed growth, nested owning elements, partial
growth, zero capacity/stride, and empty-range/empty-release checks. Add an
ordinary linked round trip for the widened owner and inspect its parameter
and result ABI; retain a deliberately wrong content/release observation that
fails. The native single-append paths must eliminate the extra descriptor
pointer load and preserve needed reloads across calls. If qualification fails,
fix the prototype or stop before measuring; do not change normative verdicts.
Real-worker lifetime qualification remains mandatory before representation
adoption and is not claimed by this append-only screen.

Use the existing RAW-clock instrument and fixed command
`growth-api-measure 67108864 7 8589934592` once, retaining both cohorts, seven
samples and every control without subtraction or retry. Apply the existing
duration, peer-drift and disjoint-range criteria to all six matched cells;
report the capacity-zero/one policy cells separately. Only if every matched
cell passes, proceed to a preregistered A/B/B/A comparison against the frozen
length-reuse split-payload image with the same driver and peer objects. A failed
screen cannot qualify append, let alone a layout or all of Vector. Spare
append still requires current-clock confirmation; no other API is advanced.

The overlay and focused fixtures belong to this existing Vector experiment;
retain the reproducible patch and observations here and retire them when a
superseding representation experiment makes them redundant. Scratch builds
stay in its ignored `.build/inline-slots-owner` directory.


Outcome: the screen does not qualify the complete append API. The prototype
removes the descriptor-pointer load and descriptor allocation, but only scalar
capacity 16 and wide capacity 4096 pass the registered slower-peer range
criterion in both cohorts. The four other matched cells overlap. The
zero-capacity cells still lose. No follow-up pair or retry ran; this independent
screen establishes neither a paired speedup nor a loss versus the previous
layout. In particular, removing the indirection is insufficient evidence to
select a widened owner.

The existing aggregate ABI passes owned arguments by content pointer and,
on the measured AArch64 host, returns these three scalar words in registers. The setup-only LLVM bridge
calls the ordinary public prepare entry and stores its result directly in
24-byte caller storage. Timed mutation receives that storage directly. Native
append now begins by loading length/capacity from its parameter rather than
loading a descriptor pointer first; the needed length reload after growth
remains. Full growth reallocates only the payload, while partial growth copies
the initialized prefix. All 30 accounting rows pass; WF's ten growth rows
observe one realloc, no explicit free, and no heap descriptor overhead. Rust
and C++ timed/account objects are byte-identical to the preceding overlay.

| Stage | Wall time | Result |
|---|---:|---|
| Gate-profile candidate CLI build, two jobs | 37.17 s | Exit 0 |
| API emission | 0.76 s | Exit 0 |
| API build, spare/growth correctness, accounting and faults | 3.08 s | Exit 0 |
| Focused native build / execution | 0.23 / 0.45 s | Exit 0 / 0 |
| Failed realloc observation | 0.17 s | Expected exit 73; caller descriptor and payload intact |
| Linked-counterpart build / execution | 0.26 / 0.36 s | Exit 0 / 0 |
| Valid-pointer order mutant build / execution | 0.26 / 0.35 s | Exit 0 / expected 1 |
| Registered growth screen | 116.93 s | Exit 0; target not met |
| Restored production CLI build | 37.99 s | Exit 0; bytes match frozen production |

Focused checks cover twelve in-place/moving cases, nested owners and exact
releases, partial growth with untouched spare bytes, zero capacity/stride,
empty ranges/free_empty, and a whole runtime-content exchange followed by
cleanup. The owned round trip runs once with a WF body and once with an
exact-public-signature LLVM bridge to a C body, checking transferred descriptor
words and complete payload. This is a **post-lowering linked ABI counterpart**;
it does not independently qualify source declaration formation or arbitrary
linked modules. Worker lifetimes and wider containing-layout ceilings remain
unqualified. No production-layout or specification adoption follows.

All 840 unique samples are retained: 420 real intervals and 420 snapshot
controls, with no subtraction. The minimum real duration is 1.279395 ms; the
RAW clock increment is 41 ns. This is one launch, so repeated-launch drift
is untested. Values below are median [minimum–maximum] ns/append. Capacity
zero and one are separate initial-policy cells, not matched-growth evidence.

| Bytes / capacity | Cohort | WF | Rust | C++ | Screen |
|---|---:|---|---|---|---|
| 8 / 16 | 0 | 35.07 [34.92–36.44] | 36.91 [36.74–38.51] | 25.50 [25.46–26.44] | PASS |
| 8 / 16 | 1 | 35.09 [34.84–35.26] | 36.81 [36.68–37.02] | 25.50 [25.44–26.26] | PASS |
| 256 / 16 | 0 | 148.01 [147.27–175.65] | 150.72 [149.04–162.01] | 141.38 [140.77–143.53] | OVERLAP |
| 256 / 16 | 1 | 148.42 [146.81–154.75] | 150.50 [149.36–156.22] | 143.31 [141.50–175.06] | OVERLAP |
| 8 / 256 | 0 | 100.90 [99.42–105.60] | 103.58 [101.42–106.49] | 93.27 [92.52–98.21] | OVERLAP |
| 8 / 256 | 1 | 100.53 [99.87–102.92] | 103.17 [101.06–106.18] | 94.08 [91.91–99.79] | OVERLAP |
| 256 / 256 | 0 | 1314.01 [1308.09–1321.74] | 1311.15 [1301.53–1325.80] | 1314.61 [1308.13–1339.45] | OVERLAP |
| 256 / 256 | 1 | 1346.18 [1306.36–1407.81] | 1330.85 [1307.53–1374.35] | 1321.70 [1293.84–1409.06] | OVERLAP |
| 8 / 4096 | 0 | 707.23 [691.02–808.57] | 679.82 [672.37–743.16] | 644.96 [638.60–736.72] | OVERLAP |
| 8 / 4096 | 1 | 703.14 [687.42–779.10] | 677.83 [669.86–760.20] | 645.06 [638.85–653.57] | OVERLAP |
| 256 / 4096 | 0 | 158.80 [157.20–159.00] | 160.01 [158.84–161.77] | 14817.90 [14806.48–14863.08] | PASS |
| 256 / 4096 | 1 | 159.74 [156.20–163.80] | 162.25 [160.17–163.21] | 14784.48 [14753.05–14850.48] | PASS |
| 8 / 0 | 0 | 18.57 [18.43–31.44] | 12.03 [11.74–12.36] | 11.00 [10.95–11.53] | LOSE |
| 8 / 0 | 1 | 18.48 [18.39–18.69] | 11.75 [11.71–11.78] | 11.02 [10.93–11.27] | LOSE |
| 256 / 0 | 0 | 34.30 [34.14–35.17] | 22.35 [22.28–23.42] | 15.55 [15.47–15.98] | LOSE |
| 256 / 0 | 1 | 34.42 [34.21–36.20] | 22.35 [22.24–23.46] | 15.58 [15.49–16.36] | LOSE |
| 8 / 1 | 0 | 18.62 [18.54–18.73] | 32.40 [32.28–33.29] | 19.19 [19.10–19.99] | PASS |
| 8 / 1 | 1 | 18.54 [18.50–18.85] | 32.58 [32.23–34.07] | 19.17 [19.11–20.05] | PASS |
| 256 / 1 | 0 | 49.25 [49.11–51.14] | 52.85 [52.73–54.26] | 42.99 [42.83–43.22] | PASS |
| 256 / 1 | 1 | 49.19 [48.81–50.60] | 52.70 [52.59–54.95] | 43.10 [42.81–45.01] | PASS |

The remaining work stays on append: distinguish its remaining growth-call
branches and empty-storage allocation route before choosing another change,
and reconfirm spare append with the current clock on the selected implementation.
This result does not justify returning to whole-container timing yet.

Evidence: [reproducible overlay](inline-slots-owner.patch),
[all samples](ecosystem-append-growth-inline-owner-samples.csv), and
[commands, native code, checks and accounting](ecosystem-append-growth-inline-owner-timing.txt).
The patch applies to `b7d386c193134cfe90505c80c445d2ef4d48ddae`; independent
replay reproduces all eleven files byte for byte. It includes seven compiler
files, the candidate-only C driver, the focused source/observer and the
setup-only bridge. All implementation inputs were restored before publication.
The frozen candidate CLI is `4a725290f8c1889a63540083cc5db03f255943397b6aa3965eaa053eae7d73a0`;
its timed image is `bef6ccb99661e944ce3d75cf92070d92d0178d4528955c07f42f9c1384f30fbe`.

To reproduce, apply the patch on the named base, build the gate CLI with two
jobs, and emit `vector-library.wf` into an isolated BUILD. Compile the supplied
`prepare-bridge.ll` with clang O3. Use the existing ecosystem build/spare/growth
check/account targets with `WF_SPLIT_SLOTS_PAYLOAD=1`,
`WF_SPLIT_SLOTS_REALLOC=1`, and `WF_INLINE_SLOTS_OWNER=1` in ECO_CFLAGS,
linking the bridge object through NATIVE_LINK_FLAGS along with ordinary runtime
flags. The retained check log gives exact commands. The focused fixture uses
the recorded allocator bindings and ordinary runtime objects; the linked
counterpart substitutes the bridge printed in the evidence log. Run only the
registered timing command after checks. This research patch is not conforming
production source; do not run normative gates expecting its Box layout to pass.
No specification or live-tree revision was made.

- SHA-256 `inline-slots-owner.patch`: `94b9510c97dad761fb625ef37bbc78b20d3ff63e8bf9df65dac12c9d95952a39`.

- SHA-256 `ecosystem-append-growth-inline-owner-samples.csv`: `f78d3be0a4de24302371acf1274d4e1d8430ccb10c68510806b3c6c636e82270`.

- SHA-256 `ecosystem-append-growth-inline-owner-timing.txt`: `36a6f349e9795f88af2c750d4a5eeade395dce21cf590e12628748005bc21ab3`.

The restored-source record passes guarded `make static` in 32.76 s. Independent
scoped review checked the implementation/fixture and retained evidence; this
checkpoint does not claim a new full local canonical gate.


#### Full-growth entry equality: registered fact-transport discriminator

The inline-owner screen still misses four matched cells. Its scalar4096 full
path reloads length and tests equality with capacity before payload realloc,
even though `grow_vector_grow_full` requires `cap <= len` and MSR-2's standing
window invariant supplies `len <= cap`. The helper makes no intervening
storage mutation before calling reserve. This proves equality at entry; the
capacity-doubling versus saturation branch remains necessary. Counting the
reachable native path also finds fewer WF branches/calls than Rust, so a
shorter instruction sequence alone is not a causal explanation of the gap.

First test a post-lowering diagnostic, not a new compiler pass. In the frozen
inline-owner timed and accounting modules, add an entry `llvm.assume` of
length equal to capacity to exactly the scalar and wide full-growth helper
bodies, loading both measures from their inline descriptor. Add no bound,
inline hint, allocator change, false-path rewrite, or persistent memory fact.
The assertion relates entry SSA loads only; ordinary later writes retain their
normal semantics. Every other function body, source, driver, peer object,
capacity policy, sentinel and compiler remains unchanged. This tests whether
transporting this one proved relation can change native code and append cost.
It selects no helper-name rule for the compiler: any production use must
retain source proofs and support/mutation boundaries generically.

Before timing, require the existing spare/growth correctness, exact allocation
ledgers and deliberate value/state/allocation faults. Require the optimized
full helpers to lose their length-versus-capacity branch while retaining
failure handling and the general reserve/partial-growth path. If the assume
fails to produce that native effect, retain the result and stop before timing.
Verify all other input function bodies and the Rust/C++ and driver objects are
unchanged. The full helper may simplify through ordinary inlining; inspect
and report any changed call boundaries rather than hiding them.

If the native discriminator passes, run one fixed screen with
`growth-api-measure 67108864 7 8589934592`, two cohorts and seven samples,
using the same RAW-clock instrument and existing duration/range conditions.
Keep all 840 rows and controls without subtraction or retry. Only if every
matched cell passes, proceed to a preregistered paired comparison against the
unmodified inline-owner image. Otherwise stop timing qualification; neither an
independent favorable median nor fewer instructions establishes a paired gain.
The current empty-capacity policy cells remain separate. No other API or
container is advanced. The patch, measured samples and instrument record stay
in this existing Vector experiment; ignored scratch uses
`.build/full-entry-equality` and retires with the superseding fact-transport
experiment.


Outcome: the entry relation removes the redundant length-versus-capacity
branch in both full-growth helpers, but the single screen still qualifies
only two of six matched cells. No paired comparison was opened, so this
establishes neither a gain nor a regression against the original inline-owner
image. It does not select production fact transport or a representation.

The timed and accounting IR each receive exactly two five-instruction entry
assertions. Removing those additions reproduces both frozen B inputs byte for
byte; all other input functions, the driver, Rust/C++ objects and setup bridge
are unchanged. The optimized full helpers retain allocation-failure handling,
the doubling/saturation branches and the ceiling return guard. General reserve
retains full realloc and partial initialized-prefix copying. Ordinary optimizer
inlining also removes full-helper calls from the two suffix-cycle bodies and
changes calls in two trace bodies; these are recorded native consequences,
not extra directives. The single-append functions still call the full helpers.
Thus this tests transport of the relation, not an isolated instruction deletion.

Both initial images passed correctness and negative controls, but their link
command ordered runtime objects differently from B. Before timing, both were
relinked in B's original object order and every check and negative control was
rerun. The retained log includes both generations, their exact commands and
actual exits. Both O3 IR compilations, both initial links and both repaired
links exited 0. Spare/growth API checks and allocator checks exited 0; all
deliberate value, state, allocation/accounting and clock faults exited 1.
The repaired accounting image retains 30 rows and B's allocation contract.
No failed correctness observation was discarded or weakened to start timing.

The one registered `growth-api-measure 67108864 7 8589934592` launch exited 0
in 116.79 s according to the guarded runner. All 840 unique rows remain:
420 real intervals and 420 snapshot controls, seven samples per cell. Minimum
real duration is 1.271836 ms; the clock's minimum nonzero increment is 41 ns.
No controls were subtracted and no launch was retried. Across the matched
cells, maximum cohort peer-median spread is 2.5144%, and maximum cohort
WF/peer-ratio spread is 3.2793%, within the existing 10% conditions. Cross-run
drift against B is untested because this is one screen, not a paired launch.

Only scalar-16 and wide-4096 have their complete WF ranges below the
median-slower peer in both cohorts. Scalar-4096 loses in both; scalar-256,
wide-16 and wide-256 overlap. Values below are recomputed from the raw CSV,
median [minimum–maximum] ns/append. Capacity zero and one stay separate
initial-policy cells and do not qualify matched growth.

| Bytes / capacity | Cohort | WF | Rust | C++ | Screen |
|---|---:|---|---|---|---|
| 8 / 16 | 0 | 34.63 [34.43–36.30] | 36.81 [36.68–37.99] | 25.47 [25.40–26.42] | PASS |
| 8 / 16 | 1 | 34.55 [34.38–34.94] | 36.88 [36.65–38.11] | 25.45 [25.33–25.71] | PASS |
| 256 / 16 | 0 | 147.95 [146.65–166.20] | 150.93 [149.13–167.25] | 143.01 [141.52–151.11] | OVERLAP |
| 256 / 16 | 1 | 147.88 [145.70–159.69] | 150.67 [149.11–158.92] | 142.46 [141.74–150.88] | OVERLAP |
| 8 / 256 | 0 | 100.65 [99.35–114.93] | 103.10 [101.56–106.70] | 92.12 [91.34–95.97] | OVERLAP |
| 8 / 256 | 1 | 100.20 [98.90–105.68] | 103.18 [102.30–108.01] | 91.07 [90.89–230.02] | OVERLAP |
| 256 / 256 | 0 | 1316.03 [1310.22–1347.34] | 1310.47 [1307.01–1330.15] | 1305.20 [1299.75–1322.59] | OVERLAP |
| 256 / 256 | 1 | 1337.24 [1309.38–1423.88] | 1318.72 [1311.22–1392.27] | 1319.81 [1311.63–1427.10] | OVERLAP |
| 8 / 4096 | 0 | 746.30 [737.13–809.24] | 629.40 [621.93–733.05] | 642.01 [622.60–695.77] | LOSE |
| 8 / 4096 | 1 | 754.58 [737.05–963.32] | 645.23 [621.62–714.61] | 628.52 [625.17–642.25] | LOSE |
| 256 / 4096 | 0 | 160.48 [159.66–162.29] | 161.63 [160.19–164.40] | 14831.65 [14795.27–14859.56] | PASS |
| 256 / 4096 | 1 | 161.89 [159.84–163.25] | 161.13 [159.01–164.77] | 14809.59 [14756.58–14877.09] | PASS |
| 8 / 0 | 0 | 18.32 [18.25–18.38] | 11.75 [11.72–12.03] | 10.97 [10.92–12.19] | LOSE |
| 8 / 0 | 1 | 18.35 [18.24–18.63] | 11.78 [11.73–12.08] | 11.00 [10.94–11.25] | LOSE |
| 256 / 0 | 0 | 34.07 [33.98–34.38] | 22.38 [22.31–22.77] | 15.66 [15.65–15.80] | LOSE |
| 256 / 0 | 1 | 34.03 [33.93–35.55] | 22.44 [22.23–23.25] | 15.73 [15.66–16.63] | LOSE |
| 8 / 1 | 0 | 18.17 [18.07–18.76] | 33.02 [32.35–33.90] | 19.19 [19.04–19.90] | PASS |
| 8 / 1 | 1 | 18.19 [18.06–18.61] | 32.36 [32.17–32.51] | 19.08 [19.05–19.57] | PASS |
| 256 / 1 | 0 | 49.23 [48.41–50.31] | 53.47 [52.58–54.46] | 43.10 [42.98–45.39] | PASS |
| 256 / 1 | 1 | 48.65 [48.43–50.25] | 52.90 [52.42–54.84] | 43.22 [43.06–43.71] | PASS |

Evidence: [LLVM input patch](full-entry-equality.patch),
[all raw samples](ecosystem-append-growth-entry-equality-samples.csv), and
[commands, checks, allocation ledger and native bodies](ecosystem-append-growth-entry-equality-timing.txt).
Reproduce the original inline-owner B as above, copy its timed/accounting LLVM
to a fresh directory, and apply the LLVM patch there before the recorded O3
compilation and repaired-order link. The log supplies both input hashes and
exact commands; the patch adds no compiler source recognizer. These three
artifacts serve this fact-transport discriminator and retire with superseding
evidence. The compiler and public source operations are unchanged by this
post-lowering diagnostic; the generic transport question remains open.

- SHA-256 `full-entry-equality.patch`: `2b1ce6238a7dbf7e3fbf7ec614a58830f888e65d3abf15bf7b6cff4ba04273d1`.
- SHA-256 `ecosystem-append-growth-entry-equality-samples.csv`: `a65f81da11ff5a71d91fa2915c0a1f1c1372e36f7649744a39eb0e49c68624a9`.
- SHA-256 `ecosystem-append-growth-entry-equality-timing.txt`: `d479422c3deff21a346112598244198979ebf5f0a4eb6e9ee1610c5d6d6a91ad`.

- Timed image SHA-256: `fefbd8e5bc725f4ac3dedca82bbecbbbd07813995528c356bdbc68615ab13fad`.


#### Allocation-free empty payload: registered independent discriminator

The inline-owner growth screen leaves both capacity-zero cells slower than
both peers. WF starts with an allocated one-byte payload and reallocates it;
the peers start without a backing allocation and allocate on first append.
The inline descriptor makes a different empty representation possible without
adding guards to metadata reads. This experiment is independent of the
entry-equality diagnostic: its baseline is the original inline-owner compiler,
without the entry assumptions.

For physical extent zero (`cap == 0` or target element stride zero), keep a
nonnull payload pointing to a private, explicitly target-aligned static
anchor. Preserve logical length/capacity and all proof/acceptance obligations.
Zero-to-positive growth allocates fresh payload and publishes it only after
success; positive full/partial growth keeps the existing routes. Zero-stride
growth changes logical capacity without allocating. Cleanup releases payload
only for positive physical extent; the anchor is never freed or reallocated.
Classify by physical extent, not pointer identity or length, so transported
owners work across modules and a cleared positive-capacity vector still owns
its allocation. Metadata and ordinary element/range pointer formation stay
straight-line. No nullable descriptor, new API or source spelling is added.

This is still a nonconforming layout experiment under TYPE-9/STOR-3/OP-14 and
the earlier Box ceiling limits, not a production interpretation of those rules.
The static anchor provides lifetime and alignment, while valid zero-extent
operations touch no payload bytes. The mapping must preserve range `nonnull`
and ordinary noalias promises; LLVM's [global storage](https://llvm.org/docs/LangRef.html#global-variables)
and [argument attributes](https://llvm.org/docs/LangRef.html#parameter-attributes)
define those target obligations. Verify target alignment from qualification,
not a benchmark-specific element whitelist or an unchecked numeric constant.

Before timing, require the existing API positives/negative controls and exact
revised allocation ledger. Focused cases must include normal/ZST zero capacity,
positive-capacity ZST logical length, zero-stride large logical capacity,
empty ranges/reslices crossing an ordinary linked call, two simultaneously
empty references, owner moves/swaps/cleanup, and failed first allocation with
the original empty owner intact. The allocator observer must reject any anchor
passed to free/realloc, with a deliberate wrong-release fault observed to fail.
Require native first append to allocate fresh payload, no per-access metadata
guard, and unchanged positive-capacity realloc/prefix-copy routes. Preserve
all initial fixture/instrument failures; repair neither source verdicts nor
requirements merely to run.

If qualification passes, run one fixed RAW-clock screen
`growth-api-measure 67108864 7 8589934592`, retaining all 840 rows and the
capacity-zero/one policies separately. No retry, subtraction or repeated
launch selection. Require the existing duration/range criteria, and no paired
or whole-API win follows from one screen. Only an all-matched-cell pass opens
paired qualification. This separates empty storage from descriptor placement
and contract facts. No other API/family is advanced. Retain its overlay,
fixtures and observations in this existing experiment; ignored scratch uses
`.build/empty-payload-anchor` and retires with superseding evidence.


Outcome: allocation-free empty payload passes the registered focused and API
checks. In the single timing screen, both capacity-zero element sizes have
complete WF ranges below both peers in both cohorts. The six matched growth
cells still pass only two of six, so append remains unqualified and the
conditional paired comparison was not opened. These observations establish no
paired gain or regression against B and select no production representation.

The candidate derives its private mutable anchor's size and alignment from
the concrete runtime-Slots element layouts under target qualification. Metadata
stays inline; no per-access null guard or pointer normalization is added.
Zero-stride growth changes capacity only, and release tests capacity and
physical stride, not length or anchor identity. The API accounting ledger
contains 30 rows: WF capacity-zero append starts with zero live heap bytes
and makes one fresh allocation, requesting 8 or 256 bytes respectively.
Positive full growth remains realloc; partial growth preserves only the
initialized prefix before releasing the old payload.

The source/native and post-lowering linked fixtures pass in-place and moving
realloc, partial-prefix/spare-byte preservation, normal and zero-stride empty
owners, positive and large logical ZST capacities, empty ranges/reslices, two
simultaneously empty references, owner moves and swaps, and cleanup after
exchanging an empty owner with a cleared positive-capacity owner. Independent
foreign anchors exercise transport without pointer-identity classification.
Realloc refusal exits 73 and first-allocation refusal exits 74 only after
checking that the caller's complete descriptor is unchanged. Deliberate
anchor free/realloc attempts and reordered nested contents each exit 1.
The linked bridge exercises ordinary public owner/range ABI transport; it is
not source-level `fn_sig` formation or worker-lifetime qualification.

The initial focused source had FORM-2 noncanonical single-line block trivia;
canonical multiline formatting repaired it. Its rejection and complete source
difference are retained. An initial timed-driver preprocessing comparison also
failed: placing six new macro lines early shifted Darwin's `assert` line
number from 513 to 519. Moving those accounting macros later restores exact
preprocessed identity; this was not a compiler or runtime test rejection.
The timed driver, Rust/C++ objects, setup bridge and runtime objects are
byte-identical to B, in the same link order. Accounting rebuilds only its
changed ledger driver. No F entry-equality assertion is present.

Native call placement changes through ordinary optimization. Both timed
append bodies use a 32-byte frame, inline the full-growth capacity policy and
call a separate 48-byte `grow` primitive. The wide public wrapper tail-branches
to that append specialization. B's 32-byte append bodies instead call 64-byte
full-growth bodies with the primitive inlined. A separately emitted Z
full-growth helper has a 16-byte frame and calls the 48-byte primitive, but it
is not on these timed append paths. The retained audit corrects the initial
summary that counted it there. No inline hint was changed. First positive
growth reaches malloc, and allocation-failure checks precede publication;
the positive full/partial routes remain visible. The screen therefore does
not isolate the allocator route's timing from changed call placement.

The gate CLI build exited 0 in 37.90 s. Focused/API emission, native builds,
positive checks and the revised ledger exited 0; deliberate API value,
state, allocation/accounting and clock faults exited 1. The one registered
`growth-api-measure 67108864 7 8589934592` launch exited 0 in 116.44 s by the
guarded runner. All 840 unique rows are retained: 420 real intervals and
420 controls, seven samples per cell. Minimum real duration is 1.297194 ms;
the clock's minimum nonzero increment is 41 ns. Across all displayed cells,
maximum cohort peer-median spread is 2.4997% and maximum cohort WF/peer-ratio
spread is 3.5064%, within the existing 10% conditions. No row was removed,
no control was subtracted and no launch was retried. Cross-run drift against
B remains untested.

Scalar-16 and wide-4096 pass in both cohorts. Scalar-4096 loses in both;
scalar-256, wide-16 and wide-256 overlap. Values below are recomputed from the
raw CSV, median [minimum–maximum] ns/append. Capacity-zero/one initial-policy
cells remain separately visible and do not qualify matched growth.

| Bytes / capacity | Cohort | WF | Rust | C++ | Screen |
|---|---:|---|---|---|---|
| 8 / 16 | 0 | 34.72 [34.58–35.91] | 36.88 [36.65–37.66] | 25.48 [25.41–25.65] | PASS |
| 8 / 16 | 1 | 34.81 [34.56–36.11] | 36.55 [36.52–38.55] | 25.52 [25.37–26.00] | PASS |
| 256 / 16 | 0 | 147.85 [144.64–156.06] | 148.77 [146.39–156.88] | 139.76 [138.66–140.42] | OVERLAP |
| 256 / 16 | 1 | 145.92 [144.99–152.99] | 148.04 [147.26–149.05] | 140.08 [139.67–140.86] | OVERLAP |
| 8 / 256 | 0 | 103.92 [99.58–106.22] | 103.36 [102.24–110.69] | 92.39 [91.63–99.61] | OVERLAP |
| 8 / 256 | 1 | 100.69 [99.94–105.27] | 102.07 [101.50–106.00] | 92.18 [91.78–96.40] | OVERLAP |
| 256 / 256 | 0 | 1319.37 [1302.66–1371.26] | 1343.68 [1312.56–1410.98] | 1326.54 [1308.58–1407.49] | OVERLAP |
| 256 / 256 | 1 | 1333.29 [1301.77–1349.47] | 1315.50 [1302.50–1362.28] | 1320.09 [1314.97–1417.23] | OVERLAP |
| 8 / 4096 | 0 | 729.82 [724.61–899.86] | 668.54 [659.34–678.09] | 650.48 [644.41–659.15] | LOSE |
| 8 / 4096 | 1 | 737.40 [723.89–809.38] | 670.86 [659.76–699.64] | 644.63 [639.40–695.81] | LOSE |
| 256 / 4096 | 0 | 159.64 [158.50–167.69] | 161.64 [160.15–163.66] | 14836.32 [14815.21–14983.35] | PASS |
| 256 / 4096 | 1 | 159.81 [158.37–160.85] | 161.50 [159.42–167.43] | 14830.34 [14776.97–14940.08] | PASS |
| 8 / 0 | 0 | 8.77 [8.44–9.13] | 11.88 [11.72–12.47] | 10.93 [10.93–11.47] | PASS |
| 8 / 0 | 1 | 8.49 [8.45–8.69] | 11.72 [11.71–11.85] | 10.95 [10.93–11.10] | PASS |
| 256 / 0 | 0 | 13.08 [13.02–14.18] | 22.93 [22.44–23.52] | 15.77 [15.73–17.11] | PASS |
| 256 / 0 | 1 | 13.05 [13.04–13.40] | 22.37 [22.28–23.04] | 15.76 [15.71–15.81] | PASS |
| 8 / 1 | 0 | 18.46 [18.46–18.54] | 32.38 [32.21–32.84] | 19.51 [19.40–19.59] | PASS |
| 8 / 1 | 1 | 18.51 [18.46–19.08] | 32.50 [32.33–33.82] | 19.54 [19.39–20.94] | PASS |
| 256 / 1 | 0 | 48.84 [48.65–51.43] | 52.74 [52.42–54.80] | 43.25 [43.03–43.48] | PASS |
| 256 / 1 | 1 | 48.82 [48.67–50.54] | 52.68 [52.41–55.21] | 43.38 [43.06–44.61] | PASS |

Evidence: [reproducible delta from inline-owner B](empty-payload-anchor.patch),
[all samples](ecosystem-append-growth-empty-anchor-samples.csv), and
[qualification, commands, ledgers and native bodies](ecosystem-append-growth-empty-anchor-timing.txt).
Apply `inline-slots-owner.patch` on its recorded base and then the anchor patch;
the retained replay compares all 14 relevant paths byte for byte. The second
patch changes four compiler files and candidate accounting, copies the focused
fixture/observer into this experiment's namespace, and adds its linked bridge.
It preserves the baseline artifacts. The log records exact build, linking,
check and timing commands, including both initial instrument failures. These
artifacts belong to this empty-storage discriminator and retire with
superseding evidence.

All seven compiler source files and the C driver were restored byte for byte
to production before publication. The separate production gate CLI rebuild
exited 0 in 37.51 s and reproduced its prior hash. No specification, live tree,
public operation or normative expectation changed. This append-only result
does not qualify worker lifetimes, containing-layout ceilings, spare append
with the current clock, another API, or whole-container performance.

- SHA-256 `empty-payload-anchor.patch`: `c8c6ce7345039f20860198f79236ea8fc8fecfa201eaffdc831ff3adb99c55fb`.
- SHA-256 `ecosystem-append-growth-empty-anchor-samples.csv`: `097f4888f9fba48284c768fcd0e9cee2f523091d19bc5e643a5e843aa4e4ff97`.
- SHA-256 `ecosystem-append-growth-empty-anchor-timing.txt`: `aacfb3e52c0a8072b3999b2dbb6a49db186086688dffd4c1be35ab4e9e1e36a4`.
- Candidate CLI SHA-256: `d01ae503322ea77fe208ca28dacb747c0b689fa4a7b5257dccd1054944252754`.
- Timed image SHA-256: `98fe87e2f530b9b8ff8327742e132ec6df82e200b994f8ba8c6383173fe32c75`.
- Restored production CLI SHA-256: `5e8d37585a06e7b7040b43032b1cd5d4f41cce16d0e91aa77b35e7d0e83c74ac`.


#### Scalar-4096 native allocation census: registered discriminator

The empty-anchor screen still loses scalar-4096 growth despite equal requested
payload extents (32768 to 65536 bytes). The timed public WF path reaches the
same libc realloc route as Rust; the WF path has fewer calls and instructions.
This does not prove lower latency, but does not support a simple excess-call
explanation. Requested-byte accounting has not observed native usable extents
or whether realloc preserves the payload address.

Before another layout or lowering change, run one untimed native census using
the frozen empty-anchor WF and peer objects. Reuse the existing preparation,
append, complete-content oracle and destruction operations at scalar capacity
4096, 31 contexts, seed 101, one cycle, with the two existing cohort orders
(WF/Rust/C++ and C++/Rust/WF). Record each old/new payload address as an integer,
requested and native usable bytes, and whether the address changed. Decode
only these frozen native descriptor ABIs; cross-check their length/capacity
against the ordinary snapshot operation before and after append. Allocate all
observation storage before preparing the context batch, print only after its
cleanup, and never inspect an old pointer after its allocation is released.
Do not insert allocator headers, replace malloc/realloc or take timings.

Equal native size classes and address-change profiles would refute the simple
hypothesis that WF loses because it crosses a larger class or moves more often
in this census. A difference identifies setup/allocator state to investigate;
it does not establish any fraction of the timed gap. This short instrumented
run cannot establish the timed runs' movement frequencies or refute cache-state
effects. Keep every context, both peers and both orders; no selection or retry.
The census adapter is a research-only patch over the existing Vector driver,
with a reproducible build/check command and an explicit command-line caller.
It and its observations live in this experiment and retire with superseding
allocation-attribution evidence. A corrupted descriptor observation must fail
before the positive census is credited. No compiler or language change is
selected, and no other API or container is advanced.


#### Scalar-4096 census outcome: equal extents and movement in both orders

The [driver-only patch](empty-payload-census.patch),
[complete observations](ecosystem-append-growth-empty-census-observations.csv) and
[commands, input hashes and raw runner output](ecosystem-append-growth-empty-census-record.txt)
retain the single registered census. The patch replays exactly over Z's frozen
measured driver; WF, Rust, C++ and runtime objects remain frozen. Compile and
link exit 0; the deliberately corrupted descriptor observation exits 1 before
the positive census exits 0. The clean CSV is the contiguous header and all 186
rows extracted from that guarded run, without a rerun.

| Cohort | Position | Variant | Contexts | Changed addresses | Usable bytes, before → after |
|---|---:|---|---:|---:|---:|
| 0 | 0 | whitefoot | 31 | 31 | 32768 → 65536 |
| 0 | 1 | rust-vec | 31 | 31 | 32768 → 65536 |
| 0 | 2 | cpp-std-vector | 31 | 31 | 32768 → 65536 |
| 1 | 0 | cpp-std-vector | 31 | 31 | 32768 → 65536 |
| 1 | 1 | rust-vec | 31 | 31 | 32768 → 65536 |
| 1 | 2 | whitefoot | 31 | 31 | 32768 → 65536 |

Every row changes length 4096 → 4097 and capacity 4096 → 8192; requested bytes
also change 32768 → 65536. Thus WF neither occupies a larger native usable
extent nor changes addresses more often than either peer in this census.
This refutes those two simple explanations for these observations only. It
establishes neither the timed runs' movement frequencies nor the absence of
allocator, preparation-history or cache effects. No timing, compiler change,
source rule or representation adoption follows; append remains unqualified.


#### Scalar-4096 preparation history: registered discriminator

The native census found the same usable extents and address changes in every
WF/Rust/C++ context. It does not explain the remaining scalar-4096 timing gap.
Preparation still differs: WF and Rust populate through their own generated
loops, so equal contents and allocation requests do not imply equal cache
state on entry to append. Test this dependency without another compiler change.

Use one diagnostic image with the frozen empty-anchor WF and native peers,
the same scalar-4096 append operation, 31 contexts, existing seeds and 64 MiB
work budget. Compare the unchanged preparation against a common C read of all
initialized payload words immediately before the timed batch. The read uses
only the frozen live-payload descriptor ABI, visits each word through volatile
loads in the same order for all peers, and verifies its checksum against the
independent existing oracle. It allocates nothing and mutates neither owner
nor payload. Preparation, this optional read, complete post-call checking and
cleanup remain outside timing. A deliberately corrupted warm-read checksum
must fail. Use the existing RAW-clock check and retain snapshot controls.

Run one fixed launch, two opposite cohort orders, seven samples per history
condition and peer, alternating which history condition runs first by sample.
Only this scalar-4096 diagnostic cell runs: no whole-API qualification follows.
Retain all 168 real/control rows (84 real), require the existing duration and
spread conditions, and do not subtract controls or retry. Report both peers
and both histories. If the gap changes materially between the two histories
with separated sample envelopes in both cohorts, preparation history is a
performance dependency worth isolating; this does not identify a particular
cache level or explain the original gap's entire cost. If ranges overlap,
record the dependency as unresolved. The ordinary-preparation arm must still
show the original gap before a disappearance can support that attribution.
No warm-read result replaces the normal API benchmark or selects a source
change. The temporary driver patch, samples and commands belong beside the
census in this experiment and retire with superseding attribution evidence.


#### Scalar-4096 history outcome: attribution remains unresolved

The [independent driver patch](empty-payload-history.patch),
[all 168 samples](ecosystem-append-growth-empty-history-samples.csv) and
[instrument, native excerpt, hashes and commands](ecosystem-append-growth-empty-history-record.txt)
retain the single fixed launch. The patch replays exactly against Z's measured
driver, independently of the census patch, with unchanged WF/peer/runtime
objects. Build, link and the three-peer positive check exit 0; corrupted
checksum and coarsened-clock checks each exit 1. The screen exits 0 in 5.453 s.

All 84 real rows exceed 1 ms (minimum 1.279747 ms); all 84 snapshot controls
remain in the CSV (minimum 1,500 ns). The RAW clock observes a 41 ns minimum
step. Maximum cohort peer-median spread is 0.7274%, and maximum WF/peer-ratio
spread is 0.7737%, within the existing 10% bounds. Values below are medians
and complete seven-sample ranges, without control subtraction.

| Preparation | Cohort | WF ns/append [range] | Rust ns/append [range] | C++ ns/append [range] |
|---|---:|---:|---:|---:|
| Ordinary | 0 | 711.32 [683.25–777.52] | 658.08 [642.15–765.78] | 655.47 [648.01–697.03] |
| Ordinary | 1 | 708.64 [666.89–847.67] | 656.91 [641.23–712.51] | 652.51 [648.95–668.50] |
| Common read | 0 | 668.40 [665.02–708.07] | 632.96 [625.49–640.31] | 651.85 [646.34–764.89] |
| Common read | 1 | 668.09 [663.27–727.96] | 629.74 [626.89–634.39] | 656.59 [652.07–702.04] |

**Unresolved under the registered criterion.** The ordinary-preparation WF
ranges overlap both peers in both cohorts, so this diagnostic image does not
reproduce the earlier disjoint scalar-4096 gap. WF's common-read and ordinary
ranges also overlap in both cohorts. The common-read arm still places WF
disjointly above Rust in both cohorts and overlaps C++. Its lower WF medians
therefore do not establish the proposed preparation-history attribution.

The common-read arm adds an ordinary snapshot, descriptor/metadata checks,
a volatile full-payload read and an independent checksum oracle before each
timed batch. The native helper retains the load loop and oracle. This changes
the complete pre-call sequence; it does not isolate a cache level. No retry,
criterion change or cause claim follows. The result neither replaces the
normal API screen nor qualifies append or any representation for adoption.


#### Post-call preserved length: registered fact-transport discriminator

The actual empty-anchor append paths reload owner length after growth; Rust
retains its pre-growth length. The ordinary `grow_vector_make_room` contract
explicitly proves exit length equal to entry length, as do `grow_full` and
`reserve` (grow-vector.wf). FN-9 checks that normal-return relation and
ENT-3.S12 publishes it after the call's effects and invalidations. This is a
different fact from the earlier F entry equality between length and capacity.

In frozen Z timed/accounting LLVM only, surround the existing make_room call
in exactly the scalar and wide ordinary grow_vector_append bodies with a
length load immediately before, another immediately after, and an llvm.assume
that those two SSA values are equal. The stable inline descriptor has length
at byte zero. Keep the existing call, place_back and returned-length code
unchanged. Add no full-entry equality, capacity bound, invariant-memory tag,
inline hint or allocator change; removing the four added instructions at
each site must recover each input module byte for byte. Every other function
body, peer/driver object and link order remains unchanged.

The relation applies only at normal continuation of this call. Later append
changes length; post-append length is not equal to entry length. Capacity and
payload may change and must be loaded afresh as needed. Generic production
transport would need newly resolved post-call places and verified relation
support, including whole-owner replacement with equal length but a different
backing; this diagnostic selects no callee-name rule or stale pointer reuse.

Before timing require both actual scalar/wide append paths to lose their
post-growth owner-length load, use the preserved number for indexing and
increment, and emit no new runtime comparison. Inspect reached call paths,
register saves/spills and frame changes. Keep the changed payload reads,
first malloc, positive realloc, partial-prefix copy/free and allocation-failure
handling. Require existing spare/growth content, state, allocator and negative
controls. If native loads remain or checks fail, retain the failed result and
stop before timing. Fewer loads alone is not a performance conclusion.

Only after those checks, run one fixed ordinary-preparation screen with
`growth-api-measure 67108864 7 8589934592`, keeping all 840 rows, both peers,
controls and separate initial-capacity policy cells under the existing RAW,
duration and spread conditions. Do not introduce the history read, subtract
controls or retry. Only an all-matched-cell pass opens paired qualification;
otherwise retain the failure without an unpaired speedup claim. Patch, native
evidence and any samples belong to this Vector experiment and retire with
superseding generic-contract transport evidence. No source rule, compiler
implementation, other API or container is advanced by this diagnostic.


#### Post-call preserved-length outcome: load removed, append remains unqualified

The [replay patch](post-call-length.patch), [complete samples](ecosystem-append-growth-post-call-length-samples.csv)
and [commands, identities, checks and native evidence](ecosystem-append-growth-post-call-length-timing.txt)
retain the single diagnostic. In each frozen Z module, stripping four added
instructions at each of two call sites recovers the input byte for byte.
All other IR and frozen driver/peer/runtime objects are unchanged. All 19
build/check stages have their expected exits; all 30 accounting rows match Z.

Both actual append paths lose their post-growth length load, preserve entry
length in `x21` for indexing and increment, and reload the payload pointer.
The added equality produces no runtime comparison. Saving `x21`/`x22` expands
each append frame from 32 to 48 bytes; the called growth primitive remains
48 bytes. Allocation and failure paths remain intact. Thus the relation
changes the desired native dependency while also changing register saves.

The fixed screen exits 0 in 116.602 s with all 840 rows: 420 real and 420
snapshot controls. Minimum real duration is 1.290682 ms; minimum control is
1,454 ns; the RAW clock observes 41 ns. Maximum cohort peer-median spread is
2.3283%, and maximum WF/peer-ratio spread is 2.1905%, within the existing 10%
bounds. Values are medians and complete seven-sample ranges; controls are
retained without subtraction. Initial-capacity 0/1 policy cells remain separate.

| Element / initial capacity | Cohort | WF ns/append [range] | Rust ns/append [range] | C++ ns/append [range] | Against slower peer |
|---|---:|---:|---:|---:|---|
| 8 B / 0 | 0 | 8.46 [8.45–8.77] | 11.73 [11.70–12.11] | 10.94 [10.91–11.01] | Pass |
| 8 B / 0 | 1 | 8.48 [8.46–8.98] | 11.79 [11.74–12.46] | 10.95 [10.92–11.45] | Pass |
| 8 B / 1 | 0 | 18.47 [18.45–18.76] | 32.55 [32.24–33.49] | 19.18 [19.06–19.31] | Pass |
| 8 B / 1 | 1 | 18.56 [18.49–19.91] | 32.46 [32.30–32.76] | 19.18 [19.07–19.43] | Pass |
| 8 B / 16 | 0 | 35.38 [34.50–37.27] | 36.90 [36.70–38.62] | 25.60 [25.47–26.48] | Overlap |
| 8 B / 16 | 1 | 34.98 [34.58–36.62] | 36.88 [36.71–39.03] | 25.62 [25.42–27.15] | Pass |
| 8 B / 256 | 0 | 101.59 [100.73–104.13] | 103.64 [102.41–107.95] | 95.24 [92.53–97.19] | Overlap |
| 8 B / 256 | 1 | 101.46 [100.99–103.50] | 102.77 [102.07–107.47] | 93.07 [91.50–97.47] | Overlap |
| 8 B / 4096 | 0 | 715.30 [680.90–842.99] | 661.80 [655.77–805.47] | 655.22 [648.26–677.89] | Overlap |
| 8 B / 4096 | 1 | 707.87 [699.35–836.47] | 663.82 [653.47–733.44] | 653.02 [647.75–676.12] | Overlap |
| 256 B / 0 | 0 | 12.93 [12.90–13.42] | 22.34 [22.29–23.40] | 15.65 [15.58–16.31] | Pass |
| 256 B / 0 | 1 | 12.92 [12.87–13.23] | 22.34 [22.26–23.28] | 15.70 [15.57–16.25] | Pass |
| 256 B / 1 | 0 | 49.44 [48.96–51.20] | 53.20 [52.54–55.30] | 43.11 [42.86–44.57] | Pass |
| 256 B / 1 | 1 | 49.39 [49.07–50.36] | 52.92 [52.74–54.33] | 43.27 [42.85–44.87] | Pass |
| 256 B / 16 | 0 | 150.99 [149.55–164.36] | 151.95 [149.13–162.13] | 143.57 [141.04–160.88] | Overlap |
| 256 B / 16 | 1 | 149.45 [148.00–155.42] | 150.36 [149.19–162.78] | 142.31 [141.29–144.20] | Overlap |
| 256 B / 256 | 0 | 1318.24 [1296.70–1485.27] | 1312.93 [1309.75–1393.43] | 1311.56 [1292.15–1400.77] | Overlap |
| 256 B / 256 | 1 | 1321.54 [1306.41–1342.71] | 1315.06 [1309.70–1359.74] | 1315.06 [1306.28–1327.58] | Overlap |
| 256 B / 4096 | 0 | 161.37 [159.61–162.48] | 162.85 [161.11–165.91] | 14866.62 [14766.19–15067.78] | Pass |
| 256 B / 4096 | 1 | 161.09 [158.18–163.01] | 162.40 [157.57–163.98] | 14885.08 [14822.21–14907.27] | Pass |

**Only one of six matched cells passes both cohorts:** wide-4096. Scalar-16
overlaps in cohort 0 and passes in cohort 1; every other matched cell overlaps
in both. Scalar-4096 overlaps Rust while remaining disjointly slower than C++
in both cohorts. All 0/1 policy rows pass the slower-peer comparison. The
all-matched criterion fails, so no paired qualification follows. This result
establishes no gain or regression against earlier unpaired screens.

The supplied relation is `make_room` exit length equal to entry length on
normal return, distinct from F's full-entry `len == cap`. A general lowering
would need newly resolved post-call places and verified relation support;
capacity, payload and backing identity may change, and append later changes
length. Native load removal alone does not select production fact transport,
the nonconforming Z representation, or a source rule. Other APIs remain paused.


#### Shared growth-body exposure: registered append discriminator

Frozen Z's append paths test that the window is full, then call a generic
`wf_grow` body that tests full versus partial again. Test whether exposing
that body to LLVM eliminates the redundant dispatch, without supplying any
new proof facts. This follows the specialization-versus-duplication question
in `compiler/prelude-records`; it is independent of the withdrawn uniform
function-actual hint and the entry/post-call assumption trials.

In the two frozen Z timed/account modules, add only `alwaysinline` to the
scalar and wide `wf_grow` definitions. Removing those two tokens must recover
each original module byte for byte. A forced local diagnostic answers body
exposure directly; an optional hint could leave the boundary unchanged.
The recorded early-inliner cost for aggregate-result wrappers remains a
reason to inspect the output, not a production policy selected here. Keep
all bodies, driver/peer/runtime objects, allocator operations and link order.

Before timing, require both actual append paths to lose the growth-primitive
call and its repeated full/partial comparison. Keep the valid empty-to-malloc
and positive-full-to-realloc paths, null failure behavior, current payload
publication, and partial-copy growth in ordinary reserve. Run the existing
positive and deliberate-negative API/allocator/clock checks; allocation rows
must equal Z. Record actual call paths, frames, append instruction counts,
and total emitted WF instruction bytes. The attribute affects other callers
of the same primitive: record duplication and placement changes rather than
claiming to isolate the latency of one removed call. If the native criterion
fails, retain the result and do not time it.

If the native criterion passes, run one fixed ordinary-preparation screen:
`growth-api-measure 67108864 7 8589934592`, all 840 rows, both peers, separate
initial-capacity policy cells, and the existing RAW/duration/spread criteria.
No new assumptions, history read, control subtraction, size threshold or
retry. Only an all-matched-cell pass permits paired qualification. This
experiment selects neither primitive inlining nor Z's nonconforming layout
for production. The patch, samples and evidence live in this Vector home
and retire with a superseding qualified lowering result; transient inputs
live in `.build/growth-body-exposure`. Other APIs and containers stay paused.


#### Growth-chain exposure: registered follow-up

Use frozen Z again, independently of the failed two-definition trial. Add
`alwaysinline` only to `grow_full`, `reserve`, and the growth primitive for
each of the two element widths: six definition tokens per module. Removing
those six tokens must restore Z exactly. This is the complete chain below
`make_room`'s existing full-window branch. Unlike exposing only the primitive,
it puts the fact-producing branch and allocation in one optimization body;
`make_room` may remain outlined without losing that local relation.

Keep public measured API functions, `grow_vector_append`, make/accept,
placement operations and resident clones untagged. No new assumptions,
allocator changes or source edits. Before timing, follow both actual append
paths and require one full-window predicate with no second full/partial
dispatch between that predicate and allocation. A remaining `make_room`
call is allowed: this trial tests local specialization inside that helper,
not elimination of every helper call. Retain valid empty allocation, full
reallocation, null failure, and general partial reserve. Inspect wide aggregate
copies, frames, call boundaries and total WF instruction bytes; all prior
API/fault checks and exact Z allocation accounting must still pass.

If this native criterion fails, retain it without timing. If it passes, use
the preceding single-screen protocol unchanged, with no retries or control
subtraction; all matched cells must pass before paired qualification. Keep
both exposure patches and outcomes together in this experiment's record.
Transient follow-up inputs live in `.build/growth-chain-exposure`. No
production inlining or representation policy follows from this diagnostic.


#### Complete library append exposure: registered final boundary trial

The six-definition chain removes the duplicate full test but introduces a
224-byte wide payload spill across `make_room`; its single screen does not
meet the all-matched target. Test the entire library append implementation
as one optimization body, retaining the ordinary public measured entry.
Starting from Z, add `alwaysinline` to the preceding six definitions plus
both `make_room` and both `grow_vector_append` definitions: ten tokens per
module, with exact stripping back to Z. This directly exposes the two remaining
aggregate/call boundaries; eight tokens would leave one to the heuristic.

Do not tag public API entries, record construction, place_back, resident
clones or other functions. The concrete record constructor computes values
from a seed; ordinary LLVM may sink those computations after allocation.
Do not manually remove copies or reorder arbitrary source constructors.
Before timing require the reached public append implementation to contain
capacity selection, allocation and placement without an internal append,
make_room or grow dispatch call, no repeated full/partial test, and no
224-byte pre-growth payload spill/reload. A public tail to its instance is
allowed. Preserve empty malloc, full realloc, null failure, general partial
reserve and all API/account/fault checks. Record frames, calls, aggregate
moves and complete WF instruction bytes. If record_make or place_back
becomes outlined, or the specified spill remains, retain the failure; do not
extend the forced spine to more functions.

Only a native/check pass permits one unchanged ordinary 840-row screen with
the preceding duration, clock, spread and all-matched criteria. No retry or
paired claim from separate screens. Keep this result alongside the first two
exposure trials; `.build/append-spine-exposure` owns transient inputs. No
production policy or language/representation change is selected by exposure.


#### Growth exposure outcomes: dispatch removed, append remains unqualified

The [two-token body patch](growth-body-exposure.patch),
[six-token chain patch](growth-chain-exposure.patch) and
[ten-token append patch](append-spine-exposure.patch) replay independently on
frozen Z. Each token removal restores its timed/account input byte for byte.
The [shared evidence record](ecosystem-append-growth-exposure-timing.txt)
preserves input identities, exact command templates, all stage exits and native
excerpts; the [complete CSV](ecosystem-append-growth-exposure-samples.csv)
contains the chain trial's single screen; the [separate spine CSV](ecosystem-append-growth-spine-samples.csv)
retains its single screen. All three trials pass their 19
build/check stages with expected negative exits and retain all 30 Z accounting
rows unchanged. No new proof assumptions or allocator changes are introduced.

**Body-only exposure fails before timing.** Actual append now calls `reserve`,
where the repeated full/partial comparison survives. The compiler moves the
opaque boundary; it does not meet the registered native condition. No timing
is run for this trial.

**Chain exposure passes the native condition but fails the append target.**
Actual append calls `make_room`; its one full-window predicate reaches
allocation without a second full/partial dispatch. Empty malloc, full realloc,
null failure and partial-copy general reserve remain. The changed wide append
body stores and reloads 224 bytes of vector values across `make_room`, with
remaining scalar values in registers. Its frame expands from 32 to 288 bytes.

| Native observation | Z | Body only | Growth chain | Complete append |
|---|---:|---:|---:|---:|
| Reached scalar append instructions | 28 | 23 | 13 | 43 |
| Reached wide append instructions | 92 | 87 | 103 | 107 |
| Scalar append frame, bytes | 32 | 32 | 32 | 48 |
| Wide append frame, bytes | 32 | 32 | 288 | 48 |
| Total WF instruction bytes | 16160 | 15868 | 17356 | 17860 |

Counts cover complete emitted bodies, including unexecuted branches. The
Z/body wide public entry tail-branches to the counted instance; the chain
public entry contains the full body. Complete-append public entries tail to
the counted ordinary instances. Total instruction bytes include alignment
nops and exclude data and other linked objects. These observations do not
isolate a removed call's or added spill's latency.

The single chain screen exits 0 in 117.111 s. All 840 unique rows remain:
420 real and 420 controls; minimum real duration 1.298834 ms, minimum control
1,372 ns, RAW clock minimum 41 ns. Maximum cohort WF/peer-ratio spread is
5.2834% and peer-median spread is 2.0413%, within the existing 10% bounds.
Medians and full seven-sample ranges follow; controls are not subtracted.

| Element / initial capacity | Cohort | WF ns/append [range] | Rust ns/append [range] | C++ ns/append [range] | Against slower peer |
|---|---:|---:|---:|---:|---|
| 8 B / 0 | 0 | 7.84 [7.81–8.23] | 11.72 [11.70–12.03] | 10.93 [10.92–11.24] | Pass |
| 8 B / 0 | 1 | 7.87 [7.81–8.13] | 11.76 [11.71–12.37] | 11.02 [10.91–11.28] | Pass |
| 8 B / 1 | 0 | 18.13 [18.09–19.21] | 32.29 [32.20–33.34] | 20.47 [20.39–21.46] | Pass |
| 8 B / 1 | 1 | 18.15 [18.11–18.24] | 32.43 [32.22–33.07] | 20.32 [20.30–20.70] | Pass |
| 8 B / 16 | 0 | 34.42 [34.19–35.96] | 36.66 [36.55–36.87] | 25.73 [25.45–26.89] | Pass |
| 8 B / 16 | 1 | 34.29 [34.14–35.77] | 36.69 [36.54–36.76] | 25.51 [25.38–26.53] | Pass |
| 8 B / 256 | 0 | 101.75 [98.14–109.30] | 102.06 [100.99–105.93] | 93.66 [92.17–103.44] | Overlap |
| 8 B / 256 | 1 | 98.61 [98.14–113.59] | 102.46 [101.20–105.68] | 95.57 [92.33–100.59] | Overlap |
| 8 B / 4096 | 0 | 728.17 [696.57–831.01] | 743.26 [734.60–826.70] | 704.67 [702.43–766.93] | Overlap |
| 8 B / 4096 | 1 | 744.36 [693.22–791.89] | 745.73 [735.42–819.16] | 710.80 [701.65–763.44] | Overlap |
| 256 B / 0 | 0 | 15.07 [15.05–15.87] | 22.45 [22.23–22.96] | 15.74 [15.69–15.78] | Pass |
| 256 B / 0 | 1 | 15.10 [15.06–15.88] | 22.39 [22.33–23.26] | 15.66 [15.64–16.34] | Pass |
| 256 B / 1 | 0 | 50.65 [50.42–51.71] | 52.68 [52.55–53.47] | 43.23 [42.97–43.58] | Pass |
| 256 B / 1 | 1 | 50.50 [50.30–51.53] | 52.62 [52.39–54.86] | 43.12 [43.02–45.50] | Pass |
| 256 B / 16 | 0 | 151.66 [149.70–163.60] | 147.27 [145.90–150.19] | 140.89 [138.57–148.48] | Overlap |
| 256 B / 16 | 1 | 150.61 [149.33–163.20] | 147.17 [145.88–150.16] | 140.68 [138.00–153.47] | Overlap |
| 256 B / 256 | 0 | 1319.84 [1310.15–1363.57] | 1314.05 [1303.34–1345.54] | 1320.61 [1298.63–1338.09] | Overlap |
| 256 B / 256 | 1 | 1322.18 [1308.45–1402.42] | 1313.32 [1306.32–1319.93] | 1317.67 [1299.52–1422.63] | Overlap |
| 256 B / 4096 | 0 | 164.82 [164.24–169.01] | 161.34 [159.67–166.41] | 14814.78 [14785.13–15031.02] | Pass |
| 256 B / 4096 | 1 | 164.98 [163.41–168.15] | 159.88 [158.57–162.02] | 14820.16 [14765.25–14869.15] | Pass |

Only scalar-16 and wide-4096 pass both cohorts: **two of six matched cells**.
The other four matched cells overlap both cohorts. All initial-capacity 0/1
policy rows pass the slower peer and remain separate from that decision.
The all-matched criterion fails; no paired qualification follows. Different
peer timings across unpaired screens establish no causal improvement or
regression. Neither primitive/chain inlining nor Z's nonconforming layout is
selected for production. Other APIs and containers remain paused.

**Complete append exposure also fails the timing target after a native pass.**
Both reached bodies now contain capacity selection, allocation and placement
without internal append, `make_room`, growth, record-construction or placement
calls. The wide 224-byte vector spill/reload disappears; seed-derived record
construction follows allocation. Both frames are 48 bytes, retaining entry
length without a new assumption. Fresh payload reads, allocation failures and
general partial reserve remain. WF instruction bytes increase to 17,860.

The single final screen exits 0 in 116.183 s and retains all 840 unique rows:
420 real and 420 controls. Minimum real duration is 1.268254 ms; minimum
control is 1,536 ns; RAW minimum is 41 ns. Maximum cohort WF/peer-ratio spread
is 7.7350% and peer-median spread is 5.7915%, within 10%. These are within-run
checks; cross-launch drift remains untested. Wide-16 cohort 1 peer outliers
remain in the full ranges (Rust maximum 538.13 ns, C++ 306.94 ns). Medians
and full ranges follow.

| Element / initial capacity | Cohort | WF ns/append [range] | Rust ns/append [range] | C++ ns/append [range] | Against slower peer |
|---|---:|---:|---:|---:|---|
| 8 B / 0 | 0 | 7.75 [7.50–7.97] | 12.07 [11.78–12.70] | 10.96 [10.92–11.58] | Pass |
| 8 B / 0 | 1 | 7.55 [7.49–7.71] | 11.85 [11.78–12.38] | 10.96 [10.92–11.56] | Pass |
| 8 B / 1 | 0 | 17.94 [17.87–18.00] | 32.42 [32.28–32.87] | 19.09 [19.04–19.23] | Pass |
| 8 B / 1 | 1 | 17.91 [17.87–18.43] | 32.25 [32.22–33.70] | 19.32 [19.05–19.96] | Pass |
| 8 B / 16 | 0 | 33.97 [33.92–34.98] | 36.61 [36.49–37.58] | 25.57 [25.27–26.26] | Pass |
| 8 B / 16 | 1 | 34.13 [33.99–35.09] | 36.80 [36.53–38.54] | 25.44 [25.38–26.94] | Pass |
| 8 B / 256 | 0 | 101.54 [99.66–104.38] | 104.07 [102.53–110.92] | 92.38 [91.88–97.78] | Overlap |
| 8 B / 256 | 1 | 99.64 [99.23–105.00] | 102.98 [102.42–108.89] | 91.60 [91.03–96.35] | Overlap |
| 8 B / 4096 | 0 | 720.61 [687.09–810.14] | 726.50 [722.90–752.75] | 726.13 [684.57–773.91] | Overlap |
| 8 B / 4096 | 1 | 733.85 [683.73–803.15] | 729.96 [725.03–738.43] | 686.38 [681.73–734.93] | Overlap |
| 256 B / 0 | 0 | 11.83 [11.79–12.62] | 22.40 [22.27–23.93] | 15.69 [15.63–16.32] | Pass |
| 256 B / 0 | 1 | 11.82 [11.77–12.24] | 22.48 [22.33–23.11] | 15.65 [15.57–15.74] | Pass |
| 256 B / 1 | 0 | 48.37 [48.11–50.63] | 53.16 [52.85–54.64] | 43.30 [43.06–44.01] | Pass |
| 256 B / 1 | 1 | 48.27 [48.11–49.53] | 52.96 [52.76–53.16] | 43.16 [43.05–44.67] | Pass |
| 256 B / 16 | 0 | 159.54 [146.57–162.88] | 153.27 [150.34–167.69] | 141.58 [139.09–155.69] | Overlap |
| 256 B / 16 | 1 | 150.17 [146.54–163.41] | 154.09 [151.05–538.13] | 141.54 [138.99–306.94] | Overlap |
| 256 B / 256 | 0 | 1316.43 [1305.35–1397.79] | 1315.10 [1300.44–1324.43] | 1313.93 [1297.59–1429.91] | Overlap |
| 256 B / 256 | 1 | 1318.40 [1312.03–1333.34] | 1319.90 [1308.85–1322.38] | 1330.59 [1301.57–1358.57] | Overlap |
| 256 B / 4096 | 0 | 158.97 [156.84–160.72] | 163.87 [160.21–172.23] | 14803.44 [14754.26–14827.33] | Pass |
| 256 B / 4096 | 1 | 158.07 [154.84–163.37] | 161.41 [160.47–166.51] | 14792.38 [14757.17–15065.74] | Pass |

Only scalar-16 and wide-4096 pass both cohorts: **two of six matched cells**.
The remaining matched cells overlap both cohorts; all 0/1 policy rows pass
the slower peer. No all-matched pass, paired qualification, retry or further
exposure variant follows. Removing the specified calls and spills is a native
result, not a causal timing gain. The three trials select no production policy,
source rule or representation; other APIs and containers remain paused.


#### Full-growth route comparison: registered attribution experiment

Complete append exposure removes the selected calls and spills without
meeting the all-matched target. Compare allocation/movement routes next,
without selecting a size threshold or sacrificing the wide realloc case.
A is the frozen complete-append exposure image. B starts from its exact
modules and changes only the two full-positive growth blocks: malloc the
same new extent, check NULL before modifying any old state, copy exactly
old length times stride with llvm.memcpy, free the old block, then use the
existing descriptor publication. Keep empty and partial paths, all ten
exposure attributes, capacity policy, driver/peer/runtime objects and link
order. Both modules already declare the memcpy intrinsic. Successful fresh
allocation is disjoint from the still-live positive old allocation; these
instances have strides 8 and 256, and the full predicate proves the copied
prefix initialized and within the new extent. No new alias/alignment facts.
This compares the complete route, including copy choice and generated code,
not allocator internals alone.

Timed driver bytes remain unchanged. An explicit account-only route flag
changes expected WF positive-full rows from one realloc/no retirement free
to one malloc/one retirement free. Request count, requested bytes, capacities
and final live bytes remain equal; peer and capacity-zero rows remain equal.
Peak simultaneously live bytes are old plus new. Keep the existing 19
checks and deliberately wrong value/state/account/allocation/clock exits.
The synthetic extra-allocation check is not an OOM injection witness.

Before timing, separately inject NULL at the new full-growth malloc. The
focused observer must see the original descriptor and initialized payload
still live and unchanged, with no copy, free or publication. Cover scalar
and nested owning payload relocation and exact cleanup; retain empty, ZST
and partial checks. Deliberate early-free, early-descriptor-write and wrong
copy-order mutants must fail their respective observation. Failures and
negative controls stay visible; no expected outcome comes from the candidate.

Inspect actual scalar/wide append paths before measurement. They must use
the intended route with equal new extents, without a new internal growth
call, payload spill or dispatch that confounds the comparison. General partial
reserve and null-failure handling must survive. Record code sizes, frames,
copy calls and all collateral changes. If these conditions fail, retain the
result without timing or broadening the change.

This is paired route attribution, not qualification of an already-winning
container. If native and correctness prerequisites pass, use one fixed
A-B-B-A sequence of the existing ordinary-preparation
`growth-api-measure 67108864 7 8589934592` screen. Retain all four 840-row
outputs, every control and both peers, with unchanged RAW/duration/spread
checks. No retry, subtraction or selecting samples. Require the same direction
of disjoint seven-sample WF envelopes in both cohorts of both adjacent A/B
pairs before claiming a route improvement/loss in that cell; report overlaps
as unresolved. A peer's matched median drift above 10% across an adjacent pair
makes that pair's attribution unresolved. The wide-4096 counterexample is
mandatory: scalar benefits cannot select a uniform copy policy that loses
its realloc advantage. No threshold or production choice follows automatically.

Transient variants, account-only driver and failure observers belong to
`.build/full-growth-route`; retain replay patches, complete samples and a
compact command/observation record in this Vector home until superseded by
a qualified growth implementation. No source/compiler/specification or other
API is changed by this experiment.


#### Full-growth route outcome: scalar gains and a wide realloc counterexample

The [combined replay patch](full-growth-route.patch) preserves the two LLVM
route changes, account-only driver adjustment and focused source/observer
qualification against prior frozen inputs. The [compact record](ecosystem-append-growth-full-route-timing.txt)
gives dependency identities, commands and all 20 API/account plus 24 focused
stage exits. The [combined CSV](ecosystem-append-growth-full-route-samples.csv)
retains every field of all four 840-row outputs, with a run label; removing
that label reconstructs each original file byte for byte and matches its hash.

All prerequisites pass. Genuine NULL injection at the new full malloc preserves
the caller descriptor, initialized payload and nested owners without copy,
retirement or publication. Early-free, early-descriptor-write and wrong-copy-order
mutants fail their observations. Linked ownership checks remain post-lowering
ABI evidence. They do not establish source `fn_sig` formation. Timed driver and
peer/runtime objects stay fixed; only the account driver changes expectations.
Initial preprocessor comparison differed in embedded source basenames; comparison
through the same stdin filename passes, and no timed object was replaced.

Both actual append paths use the intended malloc/memcpy/free route with the
same byte extents and no new internal growth call or payload spill. Register
preservation increases frames from 48 to 80 bytes; scalar/wide reached bodies
increase from 43/107 to 59/123 instructions, and WF instruction bytes from
17,860 to 18,996. These code changes are part of the route comparison.

The fixed A1–B1–B2–A2 sequence completes with exit 0 for every run, preserving
all 3,360 unique rows (1,680 real and 1,680 controls). Every real interval
exceeds 1 ms and every RAW clock probe observes 41 ns; all within-run peer and
WF/peer cohort spreads stay below 10%. No retry, subtraction or selection occurs.

WF medians below are **ns/append, cohort 0 / cohort 1**. Pair 1 is A1/B1;
pair 2 is B2/A2, keeping A as baseline. F/S/O means B's full sample envelope
is faster/slower/overlapping A's; each pair shows cohort 0 / cohort 1. Peer
drift is the largest `abs(B/A - 1)` of either peer across the two pairs/cohorts.

| Element / initial capacity | A1 WF | B1 WF | B2 WF | A2 WF | Pair 1; pair 2 | Max peer drift | Attribution |
|---|---:|---:|---:|---:|---|---:|---|
| 8 B / 0 (policy) | 7.57 / 7.49 | 7.80 / 7.94 | 7.83 / 7.84 | 7.56 / 7.51 | O/S; O/S | 0.854% | Unresolved |
| 8 B / 1 (policy) | 17.91 / 17.93 | 16.50 / 16.52 | 16.56 / 16.44 | 17.91 / 17.87 | F/F; F/F | 6.794% | B faster |
| 8 B / 16 | 34.15 / 34.14 | 22.54 / 22.53 | 22.54 / 22.44 | 34.03 / 33.99 | F/F; F/F | 0.473% | B faster |
| 8 B / 256 | 100.31 / 101.18 | 89.88 / 90.20 | 88.71 / 88.19 | 100.13 / 100.66 | F/F; F/F | 2.672% | B faster |
| 8 B / 4096 | 710.70 / 710.33 | 729.18 / 679.56 | 708.17 / 704.16 | 723.30 / 717.25 | O/O; O/O | 13.529% | Unresolved: drift |
| 256 B / 0 (policy) | 11.83 / 11.87 | 12.18 / 12.16 | 12.15 / 12.14 | 11.78 / 11.82 | O/S; O/S | 0.784% | Unresolved |
| 256 B / 1 (policy) | 48.50 / 48.31 | 36.92 / 36.97 | 37.03 / 37.07 | 48.39 / 48.29 | F/F; F/F | 0.984% | B faster |
| 256 B / 16 | 150.22 / 149.42 | 140.17 / 140.04 | 136.87 / 136.04 | 147.17 / 146.99 | O/O; O/F | 2.057% | Unresolved |
| 256 B / 256 | 1314.89 / 1320.70 | 1316.11 / 1316.14 | 1313.09 / 1310.23 | 1319.85 / 1316.43 | O/O; O/O | 1.507% | Unresolved |
| 256 B / 4096 | 156.66 / 152.61 | 14905.61 / 14919.30 | 14834.43 / 14809.45 | 157.77 / 157.58 | S/S; S/S | 5.688% | B slower |

The matched scalar-16 and scalar-256 gains have separated envelopes in both
cohorts of both pairs, with peer drift below 10%. Wide-4096 loses in all four
comparisons: replacing realloc's route with a full copy removes its observed
advantage in this cell. Scalar-4096 remains unresolved through overlapping
ranges and 13.529% peer drift; wide-16 and wide-256 also remain unresolved.
Capacity-1 policy cells favor B; capacity-zero policy cells are unresolved.

This rejects a uniform malloc/copy replacement as the append answer. It does
not select a threshold, a production route or an allocator-internal cause.
Spare append still needs its own cost qualification before the API is complete.
No compiler/library/specification revision or other API advances; no cumulative
production speedup is claimed.


#### Spare append after growth exposure: registered API cross-check

Before another growth-route variant, check the spare path of the same public
append API on the frozen complete-exposure A and malloc/copy B images. Their
growth comparison does not establish spare-path cost: both images materialize
the same append instance inside the batch wrapper, and growth lowering can
change its surrounding loop and register preservation. No other API advances.

Use the existing `api-measure 4194304 7` workload, ordinary preparation,
all six scalar/wide counts 16/256/4096, both peers, both cohorts and empty-batch
controls. The work count is the already retained duration-qualified spare
budget, not selected from candidate timings. Keep the exact binaries, inputs,
contracts, generator and oracles from the full-growth route comparison.
Before measurement inspect the actual batch bodies, record A/B loop changes
and peer call/copy paths, and verify RAW precision using each frozen image's
existing clock check. Their existing account API checks must preserve zero
allocation across spare append. No source, compiler or driver change is made.

Run one fixed A1-B1-B2-A2 sequence, retaining all four 504-row outputs and
direct exits, without retry, filtering, subtraction or budget changes selected
from peer ratios. Each real sample must meet the existing 1 ms floor; existing
10% cohort stability rules remain. For each image, a cell qualifies only if
both launches and both cohorts separate its complete sample range below the
median-slower peer; report both peers separately. A/B route attribution also
requires same-direction separated ranges in both cohorts of both adjacent
pairs, with peer drift `abs(B/A - 1)` at most 10%; overlap stays unresolved.
A passed spare cell cannot stand in for growth, and neither a failed nor a
passed screen selects an allocation threshold or a production representation.

Transient output belongs in `.build/spare-route-crosscheck`; retain one
combined losslessly reconstructible CSV and a compact native/command/result
record in this Vector home, linked here, until superseded by final append
qualification. Reuse the published full-growth replay dependencies instead of
copying their source patches or build inventory. This answers whether the
measured growth alternatives preserve the spare path and fills its outstanding
RAW-clock evidence; it is not a whole-container benchmark.


#### Spare append cross-check outcome: both frozen images pass the spare target

The [complete combined CSV](ecosystem-append-spare-route-samples.csv) retains
all 2,016 rows from the fixed A1–B1–B2–A2 sequence: 1,008 real and 1,008
empty-batch controls. The [compact native/command record](ecosystem-append-spare-route-timing.txt)
gives exact per-launch/cohort range checks and lossless reconstruction hashes,
reusing the full-growth route dependencies without another source patch.
Both positive RAW probes and all four screens exit 0. Every real sample
exceeds 1 ms (minimum 1.332253 ms), both clocks observe 41 ns, and all
within-run cohort spreads remain below 10%. No samples or controls are dropped.

**Each image passes all six spare cells** in both launches and both cohorts
against the median-slower peer, C++ in every group. The table shows
**ns/append ranges across four cohort medians**, separately for each peer and
image. These summary ranges do not replace the full sample envelopes used
for qualification, which are retained in the record.

| Element / count | WF A | WF B | Rust in A | Rust in B | C++ in A | C++ in B | Spare qualification A / B |
|---|---:|---:|---:|---:|---:|---:|---|
| 8 B / 16 | 0.468–0.471 | 0.468–0.474 | 0.468–0.474 | 0.472–0.485 | 1.009–1.012 | 1.009–1.021 | Pass / Pass |
| 8 B / 256 | 0.372–0.373 | 0.372–0.374 | 0.423–0.427 | 0.425–0.433 | 0.931–0.939 | 0.933–0.936 | Pass / Pass |
| 8 B / 4096 | 0.319–0.323 | 0.318–0.333 | 0.836–0.867 | 0.833–0.895 | 0.926–0.950 | 0.925–0.962 | Pass / Pass |
| 256 B / 16 | 4.508–4.568 | 4.540–4.574 | 4.545–4.578 | 4.525–4.613 | 7.128–7.155 | 7.101–7.136 | Pass / Pass |
| 256 B / 256 | 4.277–4.289 | 4.280–4.332 | 4.308–4.322 | 4.309–4.402 | 7.073–7.121 | 7.038–7.086 | Pass / Pass |
| 256 B / 4096 | 4.262–4.345 | 4.247–4.330 | 4.272–4.302 | 4.289–4.333 | 7.034–7.149 | 7.078–7.161 | Pass / Pass |

A/B route attribution remains unresolved in every spare cell: the required
same-direction separation is absent across the two pairs. Scalar-4096 has
one B-slower comparison and three overlaps; the other cells overlap throughout.
Peer drift stays within 10%, but this establishes no spare-path gain, loss or
equivalence between A and B.

Native scalar A/B frames are both 80 bytes and use the same eight-instruction
spare loop. Wide frames are 336/352 bytes; their 224-byte constant saves happen
once before the loop, with reloads after growth, rather than per appended value.
The existing account checks preserve zero allocation during spare append.
This completes the registered spare screen for these frozen research images;
growth remains unqualified, so append as a whole and the representation remain
unselected. No other API, compiler or specification change follows.


#### Concrete relocation loop: registered append code-generation trial

The remaining growth cells have no established extra payload copy or hidden
WF helper dispatch. Test one concrete alternative to B's generic memcpy call,
without changing the allocator route or selecting a size threshold. Start from
exact full-growth-route B and replace only its two full-positive copies with
one internal always-inline helper: guarded 64-byte chunks (four 16-byte vector
loads followed by four stores), then guarded 8-byte words for the remaining
prefix. Use ordinary nonvolatile accesses with conservative alignment and no
new alias promises or optimizer barriers. Both frozen element representations
are integral u64 words: i64 and `{[32 x i64]}`, with no padding or pointers.
The byte count is a multiple of eight; do not assume a multiple of 64. Each
access is guarded by its remaining extent. This concrete trial establishes
nothing about padded, pointer-bearing or other target representations.

Keep malloc, NULL refusal before copying, old owner on refusal, exact extents,
free and descriptor publication unchanged. Empty and partial growth remain as
B. The question is whether exposing this loop improves actual relocation code;
no runtime binding from libSystem memcpy to a particular inspected platform
implementation has been established, so do not claim its internal dispatch
cost as the cause.

Compile this single candidate and inspect both reached append bodies first.
The native prerequisite is an inline vector/word relocation loop, no external
copy call or new helper boundary on the changed full route, no new payload
spill, and unchanged allocation/failure/publication order. Record branches,
frames and total WF code size. If LLVM restores memcpy or these conditions
fail, retain the failure without timing or adding barriers to rescue it.

If native prerequisites pass, reuse the existing API/account and failure
checks and additionally exercise scalar lengths around chunk boundaries
(2,7,8,9,15,17,63,64,65) with unchanged independent content/capacity/cleanup
oracles. Keep the original checks; these are extra correctness cases, not a
smaller timing workload. A sanitized candidate must detect deliberate missing
tail content and an extra-word read beyond the old allocation before timing.
No owning-pointer copy claim follows from reusing B's unchanged owner tests.

Only after these prerequisites, one existing all-cell growth screen
`growth-api-measure 67108864 7 8589934592` may test the candidate, retaining all
840 rows and both peers, including the mandatory 1 MiB copy counterexample.
The original duration, stability and range-separation targets remain. A single
screen cannot establish a paired gain or select a production copy policy; a
failure selects no further loop permutation. Pairing needs a separately
recorded discriminator if the screen supplies a reason to continue.

Transient modules and expanded correctness driver belong to
`.build/concrete-relocation-loop`. Retain a replay patch and compact native,
correctness and any timing record in this Vector home until superseded by
qualified relocation lowering. This experiment changes neither production
code, source acceptance, a language rule, nor another API.


#### Concrete relocation loop outcome: native prerequisite passes, growth target fails

The [replay patch](concrete-relocation-loop.patch) and
[compact native/check/timing record](ecosystem-append-growth-relocation-loop-timing.txt)
reproduce the integral-word trial from frozen full-growth B. Native copy calls
become inline vector/word loops, with no new payload spill and unchanged
80-byte frames. Complete scalar/wide bodies grow from 59/123 to 157/139
instructions; total WF instruction bytes grow 18,996 to 24,888 (**31.02%**).
This qualifies neither pointer-bearing relocation nor production lowering.

Boundary and sanitized positive checks pass; missing-tail content fails the
independent oracle. The extra-word mutant reports an actual ASan eight-byte
out-of-bounds read. Its runner expected exit 1 but received 134, so the
orchestrator exits 1; that mismatch and diagnostic are retained without retry.
Focused real NULL checks preserve scalar/full and empty owners (exits 73/74).
All 30 account rows match B. The record preserves all direct stage exits.

The single fixed screen exits 0 in 118.771 s. The
[complete CSV](ecosystem-append-growth-relocation-loop-samples.csv) retains all
840 rows (420 real, 420 controls), with minimum real duration 1.313533 ms and
RAW precision 41 ns. Maximum cohort ratio/peer-median spreads are 3.050%/2.303%,
within the original 10% bounds. The table gives ranges of the two cohort
medians in ns/append; qualification uses complete sample ranges, retained in
the record, against the median-slower peer in both cohorts.

| Element / full capacity | WF | Rust | C++ | Growth qualification |
|---|---:|---:|---:|---|
| 8 B / 16 | 20.39–20.40 | 36.71–36.80 | 25.44–25.52 | Pass |
| 8 B / 256 | 84.07–85.51 | 104.62–105.04 | 91.33–92.98 | Pass |
| 8 B / 4096 | 787.41–799.41 | 736.90–737.09 | 723.81–725.64 | Fail |
| 8 B / 0 | 7.80–7.83 | 11.79–12.06 | 10.93–11.00 | Policy pass |
| 8 B / 1 | 16.30–16.34 | 32.41–32.53 | 19.31–19.42 | Policy pass |
| 256 B / 16 | 146.22–147.40 | 150.11–153.45 | 143.52–146.18 | Overlap |
| 256 B / 256 | 1486.39–1519.57 | 1322.14–1327.37 | 1319.16–1330.88 | Fail |
| 256 B / 4096 | 22197.34–22226.24 | 161.62–162.57 | 14833.86–14857.15 | Fail |
| 256 B / 0 | 12.13–12.14 | 22.31–22.38 | 15.70–15.74 | Policy pass |
| 256 B / 1 | 32.56–32.77 | 52.80–52.89 | 42.97–43.14 | Policy pass |

**Reject this loop:** only scalar 16/256 pass the matched target (**2/6**).
The four capacity-0/1 policy cells pass separately; the mandatory 1 MiB copy
counterexample remains. This single screen establishes no paired gain or
regression against B, no memcpy dispatch-cost cause, and no copy threshold.
No further loop permutation is selected. Append growth remains unqualified;
no representation, production/specification change or other API is advanced.


#### Fresh-element placement: registered append ordering discriminator

The reached full-growth B paths perform 38 scalar / 102 wide instructions
outside allocator and copy bodies, versus C++ 79 / 141 and Rust 96 / 160.
All three construct the same integral record; the suspected WF-only constant
loads and odd vector-store grouping also occur in both peers. No remaining
runtime constructor copy or redundant hot-path dispatch was found. These
counts do not measure allocator internals or establish an unavoidable floor.

One actual ordering difference remains: C++ constructs the appended value in
fresh storage before copying the old prefix; B constructs it after prefix
copying, old-payload free and descriptor publication. Test that difference
alone for the frozen i64 and {[32 x i64]} payloads. In each full-positive
success path, move the new element's nontrapping integral construction and
stores to fresh + old_length * stride, after malloc's NULL check and before
prefix memcpy. The appended element is disjoint from the copied prefix and
within doubled or ceiling-saturated capacity. Preserve allocation extents,
copy bytes, all allocator/copy calls, free and owner metadata publication
order, refusal behavior, and empty/partial/spare logical behavior. Write the
new element once. No pointer-bearing or effectful construction rule follows.

Use matched optimized-IR baseline and candidate inputs if required to expose
the native ordering. Record any effect of the extra optimization pass before
attributing the candidate; freeze both controls and unchanged peer/runtime
objects. The native prerequisite is earlier fresh-element stores, no additional
payload copy, spill, helper/library call or executed conditional branch on the
full-positive path, and no larger frame. Report total code size and duplicated
construction code rather than hiding them. If these prerequisites fail, retain
the result without timing or another scheduling permutation.

Only after native qualification, rerun independent contents, bounds, exact
allocation/release accounting and real allocation-refusal checks. Any later
performance screen must be authorized separately: this registration authorizes
native inspection and correctness, not timing or production selection. The
next comparison will use balanced variant rotations, retaining old seven-sample
results under their original instrument. The owner target is unchanged.

Transient files belong to `.build/fresh-element-placement`, serving this one
append ordering experiment and removed when its evidence is retained or it is
superseded. A compact replay/native outcome belongs in this Vector record's
existing home; no production compiler, source acceptance or other API changes.


If the registered native and correctness prerequisites pass, authorize one
candidate screen with the unchanged harness and byte budgets, using
`growth-api-measure 67108864 9 8589934592`. Nine samples balance each variant's
first/middle/last positions (three each per cohort); retain all 1,080 rows,
including the four policy cells and the 1 MiB matched case. Keep the original
RAW-clock, minimum duration, 10% stability and complete-range separation
criteria; report both peers. This is a prospective screen on the optimized-IR
candidate, not a paired improvement claim or qualification of older images.
No repeat or alternate schedule follows a failed or inconclusive screen. A
paired gain and production choice still require separate evidence.


#### Fresh-element placement outcome: ordering changes, growth remains unqualified

The [replay patch](fresh-element-placement.patch) and
[native/check/timing record](ecosystem-append-growth-placement-timing.txt) retain
the matched optimized-IR control, candidate and focused ordering observer.
Only the two scalar/wide append definitions change per candidate module.
Successful full growth writes the new element before prefix copying, without
extra runtime construction, calls, payload spills or conditional branches.
Both frames remain 80 bytes. Complete functions grow 59/123 → 61/253
instructions through static construction duplication, while executed matched
paths shrink 38/102 → 36/100, including public tails but excluding library
bodies. Native post-free pointer/capacity publication combines into an `stp`.
WF instruction bytes are 18,996 for frozen B, 19,016 after the extra control
optimization pass, and 19,544 for the candidate. These optimizer consequences
are part of the experiment; this is not an instruction-order-only binary change.

All 30 API/account executions and 14 focused stages return expected exits.
Both 30-row account tables match B. The focused observer checks full contents,
spare/descriptor guards and exact allocation/copy/release order for scalar/wide
doubling and saturation. Four real NULL cases preserve old state (exit 73);
four baseline ordering controls fail (exit 1). Its final C cleanup is distinct
from the ordinary library cleanup covered by complete API checks.

The single balanced nine-sample screen exits 0 in 151.639 s. The
[complete CSV](ecosystem-append-growth-placement-samples.csv) retains all
1,080 rows (540 real, 540 controls). Every variant occupies each position
three times per cohort/cell. Minimum real duration is 1.311726 ms, RAW
precision is 41 ns, and maximum cohort ratio/peer-median spreads are
1.541%/1.606%, within the unchanged 10% bounds. The table gives the two
cohort-median ranges in ns/append; qualification uses full sample ranges
against the median-slower peer in both cohorts, retained in the record.

| Element / full capacity | WF | Rust | C++ | Growth qualification |
|---|---:|---:|---:|---|
| 8 B / 16 | 22.42–22.51 | 36.72–36.77 | 25.52–25.59 | Pass |
| 8 B / 256 | 88.57–88.73 | 103.72–104.71 | 92.98–93.94 | Pass |
| 8 B / 4096 | 685.46–698.66 | 685.24–690.96 | 662.67–665.41 | Overlap |
| 8 B / 0 | 7.58–7.63 | 11.82–11.84 | 10.96–10.96 | Policy pass |
| 8 B / 1 | 16.22–16.23 | 32.41–32.47 | 19.23–19.30 | Policy pass |
| 256 B / 16 | 136.69–139.03 | 151.52–151.81 | 139.20–139.43 | Overlap |
| 256 B / 256 | 1306.32–1325.53 | 1310.39–1324.52 | 1308.93–1329.95 | Overlap |
| 256 B / 4096 | 14815.82–14831.27 | 162.49–163.32 | 14809.33–14816.00 | Overlap |
| 256 B / 0 | 12.60–12.68 | 22.41–22.55 | 15.64–15.77 | Policy pass |
| 256 B / 1 | 38.82–39.32 | 52.74–52.93 | 43.03–43.10 | Policy pass |

Only scalar 16/256 qualify (**2/6 matched cells**); the four policy cells pass
separately. Every other matched cell overlaps its selected slower peer in at
least one cohort, so this is no disjoint loss to that peer. Wide-4096 still
has a large deficit to Rust and overlaps C++. Complete append growth remains
unqualified. This unpaired screen establishes neither an improvement against
B/control nor a general refutation of earlier placement. No production rule,
representation, next ordering permutation or other API is selected.


#### Native allocation binding: registered append attribution audit

The next question is which native entries the frozen append executable really
calls. Cached-library disassembly shows a possible distinction: libc++abi's
operator new supplies a type identifier to malloc_type_malloc, while WF's
malloc entry derives a caller identifier. On modern zone versions both can
reach the same default-zone typed-allocation function. Static imports and
weak symbol coalescing do not prove runtime resolution, zone selection, or a
performance cause.

Run one guarded, untimed `growth-api-check` on the frozen full-growth-route B
image with `DYLD_PRINT_BINDINGS=1` and `DYLD_PRINT_LIBRARIES=1`. Preserve its
hash, exact command, exit, and diagnostic output. Require actual binding lines
for operator new/delete and malloc/free/realloc before claiming their resolved
entry images; missing output leaves that binding unresolved. Successful value
checks do not establish which zone is used. This diagnostic changes neither
the API workload nor its ranking, and its elapsed time is not a performance
sample. Do not infer allocator cost or distinct backends from entry names.

Transient output belongs to `.build/append-allocation-binding`, until a compact
binding outcome is retained in this Vector record. Any further live dispatch
inspection or profiling needs its own bounded method before execution; no
allocator policy, new code-generation variant or other API is selected here.

The single diagnostic completed with exit 0; the wrapper reported 0.11 s
(untimed diagnostic, not an append performance sample). Its before/after
executable SHA-256 was unchanged:
`8e583b5524e22ff8190095908f69308d660f2f203ab1be3356bf5848e73043a8`.
`growth-api-check` passed scalar/256-byte payloads, five capacities and all
three APIs. The actual dyld bindings were:

| Executable import | Resolved image | Runtime address in this launch |
| --- | --- | --- |
| `_malloc` | `libsystem_malloc.dylib` | `0x186499ce8` |
| `_free` | `libsystem_malloc.dylib` | `0x18649a6b8` |
| `_realloc` | `libsystem_malloc.dylib` | `0x18649b888` |
| `__Znwm` | `libc++abi.dylib` | `0x18665e7b4` |
| `__ZdlPv` | `libc++abi.dylib` | `0x1866494b4` |

Loaded image UUIDs were `D969A907-3E43-3951-9365-8C2DB3812E9D`
for libsystem_malloc and `F38A9C58-22AB-3798-BBAE-8DCD9CC0CE27`
for libc++abi. The weak new/delete imports resolved to those cached bodies;
no unresolved binding remains among the five entries. These observations do
not identify the live zone callback or attribute any timing gap to allocation.
They select no allocator, layout or compiler change. Growth remains unqualified.

Reproduce after reconstructing the frozen B image above:

```sh
WHITEFOOT_CHECK_TIMEOUT=120 perl .github/run-check.pl append-allocation-binding env DYLD_PRINT_BINDINGS=1 DYLD_PRINT_LIBRARIES=1 research/experiments/container-representation/vector-library/.build/full-growth-route/vector-costs-timed growth-api-check
```

The transient directory holds `binding-status.json`, `binding.stdout` and
`binding.stderr`; addresses are launch-specific and are not a regression oracle.
No ranked timing was taken, and no other API was exercised in this audit.


#### Scalar growth attribution: registered bounded stack sample

The next discriminator asks where the frozen full-growth-route B append path
spends execution time: native allocation, prefix copying, retirement, or the
container/helper body. It does not rank implementations or select an allocator.
Use the same frozen WF/Rust/C++ and runtime objects, with only a scratch copy
of B's driver extended by a diagnostic mode. Preserve scalar capacity 4096,
31 contexts, the ordinary append ABI, seed `101 + cycle + context`, complete
preparation, independent post-state/value oracle and destruction. A noinline
batch marker encloses only the public append calls and result stores; inspect
its native indirect-call return PC before using it as a sample boundary.

Compile only that copied driver at O3 without LTO, retaining B's object order.
First run one batch per peer and a deliberately wrong offered value per peer:
positives must exit 0 and negatives must fail the existing content oracle.
Freeze the image and input hashes before profiling. Then launch each peer
once, serialized under the host guard, for a fixed 75-second diagnostic loop.
After its readiness signal, sample only that owned child with
`sample PID 60 1 -mayDie -file PROFILE.txt`. No adaptive extension or rerun.
Each peer has a 120-second guard. Preserve direct child/profiler statuses,
complete profiles, cycle counts and hashes. These durations are diagnostic
costs, not performance samples.

Interpret broad category shares only with reliable unwinding through the
inspected append boundary and at least 1,000 filtered append samples per peer.
Exclude preparation, validation, destruction and all unrelated stacks. Mark
ambiguous or truncated stacks separately; do not assign them to allocator or
container overhead. Failure to meet either prerequisite leaves attribution
unresolved. Periodic sampling and process suspension can bias the profile,
so even a qualifying profile cannot establish a nanosecond peer difference or
a causal speedup. A code change still requires matched before/after evidence.

Transient source, patch, commands, profiles and manifests belong under
`.build/append-profile`; retain a compact outcome and reproducible driver delta
here once the observation is complete. No production code, ranked benchmark,
language rule, other API or growth policy changes in this experiment.


The first sandboxed WF sampler could not attach to its owned child and
produced no profile. Cancel the not-yet-started Rust/C++ attempts; retain this
failure rather than treating it as an unwind or performance result. A separate
one-second capability probe outside the tool sandbox successfully sampled an
owned `sleep` child, without sudo or accessing another process. This establishes
that the available host invocation can sample an owned process, not that the
WF stack unwinds correctly.

Before collecting any append samples, revise only the launch environment:
run the same three fixed peer diagnostics outside the tool sandbox, under the
same serialized 120-second guards and the same 75/60-second child/sample
limits. Retain the failed attempt separately and preserve all new outputs
under `.build/append-profile/host-permission`. No measured result is being
repeated or selected: the first attempt yielded no samples. Keep the binary,
input hashes, sample count/unwind requirements, exclusion rules and no-retry
rule unchanged. Do not use sudo or attach to unrelated processes.


#### Scalar growth profile outcome: WF attribution remains unresolved

The [75-line driver delta](append-profile.patch) changes only the frozen B
diagnostic driver. Its compile/link exits are 0 (0.963/0.125 s); all three
one-batch positives exit 0 and all three wrong-value controls exit 1 at the
existing full-values/checksum oracle. All 17 frozen input hashes match.
The diagnostic executable SHA-256 is
`0f1b3e8ffa9f371d995efb92b028b056d376a309d13a5dd84d86b9a43000facf`.
O3 unrolls the marker into 31 indirect calls, with return PCs
`0x10000f8d4 + 20*k` for k=0..30. The marker establishes x29; the WF append
body saves x29/LR but does not establish an x29 frame. Setup, oracle and
cleanup stay outside this marker.

The initial sandbox WF child completed (0), but its sampler returned 255
without a profile. Rust started before cancellation took effect: its sampler
also returned 255, its child was terminated (-15), and the outer orchestrator
returned 130; its guard's terminal exit was not captured. C++ never started
in that attempt. A host precheck returned 1 for the missing guard END record;
verified absence of every owned PID resolved termination without inventing an
exit. The first host launch then returned guard 75 before starting a workload,
because cancellation left the recorded Rust lock. The lock was preserved and
removed only after checking its absent PID and matching command. These are
infrastructure outcomes, not samples. The subsequent host batch stops on any
nonzero stage and completed exactly once per peer.

[Complete evidence](ecosystem-append-profile.txt) retains the input identities,
portable commands, stage results, raw profiles and native marker. All three
host child/sampler/guard exits are 0, with unchanged executable hashes.
Guard costs are 75.118/75.163/75.173 s for WF/Rust/C++; sampling costs are
60.197/60.235/60.171 s. Cycle counts and total elapsed times include preparation
and verification, so neither is an append-speed comparison.

| Peer | Total stack samples | Complete marker-to-append samples | Marker self excluded | Result |
| --- | ---: | ---: | ---: | --- |
| WF | 49713 | 2 | 5 | Fails unwind/count prerequisite |
| Rust | 49653 | 2639 | 1 | Broad shares only |
| C++ | 49751 | 2597 | 3 | Broad shares only |

WF has 3,498 additional samples in the marker caller's scope without the
marker frame, 3,488 of them showing WF append. They remain excluded; the
caller PC alone does not satisfy the registered full-chain criterion. Rust
excludes two missing-marker samples; C++ excludes three, plus seven marker
descendants without the public append frame. No WF cost percentages follow.

Count each node exclusively as its inclusive count minus its immediate child
counts; do not sum inclusive frames. Within the qualified Rust paths, 2,124
samples are copying (80.49%), 489 are the remaining allocator route including
realloc-internal retirement (18.53%), 22 are helper self and four unresolved.
Within the qualified C++ paths, 2,137 are copying (82.29%), 344 allocation,
101 retirement, ten helper self and five unresolved. C++'s deduplicated helper
is resolved by its native call return PCs for new/memcpy/delete; symbol-name
guesses alone do not assign a category. These are conditional sampled shares,
subject to periodic-sampling and suspension bias. They establish neither peer
latencies nor the cause of the WF gap.

The linked body already has unwind information: its FDE covers
`0x10001148c..0x100011578` and records CFA=SP+80, saved LR at CFA-8 and
saved FP at CFA-16 at the allocator-call return PCs. These match the native
prologue. Missing body metadata is therefore not an established explanation;
the local sampler documentation does not specify its unwind policy. A future
frame-pointer diagnostic would change instructions and could not serve as an
unchanged-code timing comparison.

Reliable WF stack recovery remains deferred in TODO while the append
growth-route investigation proceeds. No representation, inlining or growth
policy is selected, and append growth remains unqualified. No ranked
performance rerun, other API, compiler change or specification amendment was
made in this profiling diagnostic.


#### Equal-byte growth ladder: registered route discriminator

The paired A/B results justify different route costs at the observed extremes,
but not a monotone size crossover or a production cutoff. A page-size switch
would be a heuristic, not an allocator guarantee. Before introducing dispatch,
test whether route direction agrees for equal physical extents represented
by different element widths. Keep frozen A (full-positive realloc) and B
(full-positive malloc/copy/free), their ordinary public append helpers,
compiler objects and runtime/peer inputs. Change only a copied driver.

Use previously unpaired old byte extents 512, 1536, 3072 and 8192: scalar
capacities 64, 192, 384 and 1024; wide capacities 2, 6, 12 and 32. Include
wide capacity 4096 as the mandatory 1 MiB realloc counterexample. Require
actual old capacity N and resulting capacity 2N for every peer in each cell;
the wide-2 cell must also satisfy that equality rather than entering a
policy-comparison category. Preserve complete initialization, seed schedule,
post-state/value oracle, cleanup and snapshot controls. No capacity, hidden
reservation, allocator or owner-layout changes.

Before timing, check every new cell through all three public APIs on both
images, including deliberate wrong offered-value and post-state controls.
Inspect the linked append bodies and verify all frozen inputs and the intended
routes; changed driver code may change addresses but must not rebuild or
alter library objects. Record build and execution costs separately. Do not
proceed past a correctness or native prerequisite failure.

Run one fixed A-then-B exploratory screen, nine samples in each of two
reversed/rotated cohorts, with all peers equally often in each position.
Keep the existing 64 MiB byte budget and 8 GiB wide-4096 budget, RAW clock
check, duration minimum of 1 ms, cohort-ratio bound of 10%, and at most 10%
peer-median drift between images. Retain every raw sample and control; no
subtraction, adaptive sample count or rerun. Compare complete WF observed
ranges in each cohort. The same separated direction in both cohorts is only
a single-pair route observation, not repeatable qualification. Overlap or a
failed prerequisite leaves the cell unresolved.

Opposite qualified route directions at the same old byte extent falsify a
size-only explanation for these instances. Consistent directions can motivate
one later, fixed policy with unseen neighboring sizes and its actual dispatch
cost measured; they do not select that policy here. Preserve the large realloc
counterexample and all adverse results. Neither peer target nor append
completion changes, and other APIs remain paused.

Transient drivers, manifests and executions belong to
`.build/append-byte-ladder`; retain the reproducible driver delta, complete
samples and compact outcome in this Vector home until superseded by the
qualified growth implementation. This changes no compiler, specification,
library API or production growth policy.


#### Equal-byte growth ladder: outcome

The copied-driver [delta](append-byte-ladder.patch), complete
[samples](ecosystem-append-growth-byte-ladder-samples.csv), and
[commands, identities, native excerpts, ledgers and ranges](ecosystem-append-growth-byte-ladder-timing.txt)
retain the single A-then-B experiment. A uses full-positive realloc; B uses
malloc/copy/free. All compiler, peer and runtime object inputs were frozen;
only the diagnostic driver changed.

All 25 guarded prerequisite stages returned their expected statuses. Positive
checks covered all nine cells and three peers on each image. Wrong-value,
post-state and quantized-clock controls failed their intended oracles. All
54 accounting rows confirmed exact initial capacity N, resulting capacity
2N, request bytes, route-specific realloc/release counts and complete cleanup.
This includes wide capacity 2 growing to 4. Build/link cost was 3.080 s,
execution checks 3.312 s and native extraction 0.484 s. Linked full-growth
paths retained A's realloc and 48 B frame and B's malloc/memcpy/free and 80 B
frame; no library object was rebuilt.

The two screens returned 0 in 151.190/153.401 s, with unchanged before/after
image hashes. Each retained 972 rows: 486 real intervals and 486 snapshot
controls. Both RAW probes had a 41 ns minimum nonzero tick; minimum real
intervals were 1.274/1.305 ms. Each peer occupied each position three times
per cohort. Maximum between-image peer median drift was 5.17%; no cell failed
that limit. Scalar capacity 1024 did fail the cohort-ratio limit: A's WF/C++
ratio varied by 11.54%. Its overlapping ranges independently leave it unresolved.

Medians below are ns/append, cohort 0/cohort 1; each peer remains separate.
Directions require separated complete WF ranges in both cohorts and the
registered prerequisites, not merely the median ordering. Complete ranges
and per-cell duration, ratio-spread and drift values are in the evidence file.

| Element × capacity (old bytes) | WF A | WF B | Rust A → B | C++ A → B | Single-pair route observation |
| --- | ---: | ---: | ---: | ---: | --- |
| 8 B × 64 (512 B) | 57.42/56.08 | 46.06/45.93 | 59.46/58.95 → 59.26/59.18 | 49.44/49.32 → 49.89/50.28 | B faster |
| 8 B × 192 (1536 B) | 88.37/85.56 | 73.22/73.52 | 92.26/87.96 → 87.49/87.01 | 80.09/78.45 → 77.43/76.35 | B faster |
| 8 B × 384 (3072 B) | 125.34/126.74 | 112.63/113.03 | 125.98/129.12 → 124.94/124.95 | 117.12/115.60 → 115.88/115.68 | unresolved |
| 8 B × 1024 (8192 B) | 254.61/228.73 | 223.61/222.01 | 237.62/228.37 → 237.60/229.52 | 215.07/215.52 → 221.39/216.89 | unresolved |
| 256 B × 2 (512 B) | 60.30/59.51 | 50.51/50.16 | 64.02/62.98 → 62.30/62.34 | 54.21/53.61 → 53.06/53.10 | B faster |
| 256 B × 6 (1536 B) | 89.50/87.35 | 76.68/76.76 | 90.54/89.71 → 90.27/90.19 | 81.16/80.25 → 79.25/79.14 | B faster |
| 256 B × 12 (3072 B) | 125.28/125.11 | 115.29/114.22 | 128.50/127.43 → 127.20/127.04 | 117.96/117.35 → 114.88/114.97 | B faster |
| 256 B × 32 (8192 B) | 225.84/225.28 | 215.25/215.33 | 229.65/228.71 → 227.02/227.84 | 216.90/219.72 → 216.51/216.51 | unresolved |
| 256 B × 4096 (1048576 B) | 158.71/158.79 | 14844.48/14852.12 | 161.50/163.89 → 162.57/168.70 | 14839.73/14856.19 → 14946.61/14909.47 | A faster |

B was separated faster at 512 and 1536 old bytes for both widths, and at
3072 bytes for the wide element. Scalar 3072 bytes overlapped in cohort 0;
both 8192-byte instances overlapped. The mandatory 1 MiB cell strongly favored
A, preserving the large realloc counterexample. No equal-byte pair had
opposite qualified directions: this experiment does not falsify a size-only
explanation, but it does not establish one or choose a crossover or cutoff.
One exploratory pair is not repeatable performance qualification, and no
production policy follows.

Work remains confined to Vector append, with spare-capacity and growth paths
assessed separately. No other API starts until append qualifies; every API
must qualify before measuring the full container workload. This checkpoint
changes no compiler, library, specification or production growth policy.


#### Small-copy growth candidate: registered discriminator

Test one provisional full-positive growth route: copy when the initialized old
payload is at most 2048 physical bytes, otherwise realloc. The last paired
small winner supports 2048; the equal-byte ladder supports smaller extents
across both widths. It supplies no ground for extending copying to 4096 or a
host page and does not establish an allocator threshold. This experiment
combines the observed route advantages; it does not claim to repair every
remaining append cell or select a production policy.

Start from frozen full-spine A and preserve its representation, empty and
partial-growth routes, public APIs, peer objects and initialization schedule.
Only the two generic full-positive grow bodies acquire the byte comparison.
Small growth allocates the same new extent, checks NULL, copies the complete
initialized prefix, frees the old payload and publishes. Large growth retains
realloc and its failure check. Both publish only after success. The accounting
driver selects the existing route-specific ledger expectation by that same
byte predicate; independent value/state and failure observers stay decisive.
No benchmark name, element type or observed capacity selects a route.

First build and inspect native code, exact allocation ledgers, complete
post-state/value checks and deliberate incorrect controls. Include both sides
and the exact cutoff, empty and partial growth, real allocation failure and
nested cleanup. Confirm source-object identities, no introduced growth-helper
call or wide offered-value spill, and the branch's executed register-save and
frame consequences. Correctness or intended-route failure stops timing. Keep
all outcomes; a code-generation failure is not repaired by weakening the
criterion. Temporary material lives under .build/small-copy-growth and is
removed or superseded with this experiment. Retain a replayable delta and
compact complete evidence in this existing Vector home.

If those prerequisites pass, use one fixed A-then-H exploratory pair, nine
samples in each of two balanced reverse/rotated cohorts. H is the candidate.
Keep the existing 64 MiB budget and 8 GiB wide-4096 budget. Preserve the six
canonical matched growth cells (capacities 16, 256 and 4096 at both widths),
and add neighboring old-byte extents 1792, 2048 and 2304: scalar capacities
224, 256 and 288; wide capacities 7, 8 and 9. Deduplicate scalar256. Every
peer starts at exactly N and ends at exactly 2N, including small wide cells.
Report both peers, all raw controls, actual durations, RAW resolution and
complete observed ranges; no control subtraction or repeated valid screen.
Use the same 1 ms real-duration minimum, RAW <=100 ns, WF/peer cohort-ratio
spread <=10% and between-image peer-median drift <=10%. A separated direction
in both cohorts is a single-pair observation, not repeatable qualification.
Overlap or a failed prerequisite stays unresolved. No cutoff sweep follows.

The candidate should retain small-copy gains at the held-out small neighbor
and avoid the uniform-copy loss at 1 MiB. A separated loss on unchanged large
realloc routes identifies actual dispatch/layout overhead; the native record
must explain whether frame or instruction changes accompany it. All original
peer targets remain: a candidate that misses them leaves append unfinished.
Inspect spare native paths now, and qualify their timings before any adoption;
old A/B spare results do not qualify H. Continue only Vector append. Other
APIs and whole-container timings remain deferred until their stated turn.


#### Small-copy growth candidate: outcome

The [prototype and observer delta](small-copy-growth.patch), complete
[samples](ecosystem-append-growth-small-copy-samples.csv), and
[commands, pins, native paths, ledgers and ranges](ecosystem-append-growth-small-copy-timing.txt)
retain one fixed A→H pair. H copies at most 2048 initialized old bytes and
otherwise reallocates; the exact two-definition transform preserves every
other timed/account IR body and the frozen peer/runtime inputs. No compiler
or WF library source was changed.

All 71 guarded prerequisite stages returned their expected statuses.
The 90 allocation rows include untimed 0/1 cases, exact capacities and route
ledgers, cutoff neighbors, and complete cleanup. Actual H public-append
observers passed full-content/state checks and returned-NULL tests below,
at and above the cutoff at both widths, plus wide4096. Early publication and
copy corruption failed independent observations; the reused nested fixture
retained small Box/u8/u64 growth, partial-prefix, empty/ZST, cleanup, and
first-allocation failure checks with early-free/publication/copy-order
negatives. Successful realloc was forced moved, not in-place. This is bounded
runtime evidence, not a new generic source ABI qualification.

Native H append adds one cutoff comparison/branch, with no internal growth
helper call or offered-value spill. Scalar/wide frames are 64 B versus A's
48 B; static append instruction counts are 59/123 versus 43/107. On successful
ordinary full growth, H's large route adds five executed instructions and one
conditional branch, excluding the outer forwarding branch and allocator internals. Spare
batch frames remain 80 B scalar and 336 B wide; their timings are not qualified
by this checkpoint. Build/link cost was 5.582 s, prerequisite execution
10.544 s and native extraction 0.529 s.

A/H screens returned 0 in 155.417/154.704 s with unchanged image hashes.
All 2376 rows are retained, including 1188 real intervals and 1188 snapshot
controls. Minimum real intervals were 1.241/1.248 ms, both RAW probes observed
41 ns, and every peer occupied each position three times per cohort. Maximum
WF/peer cohort-ratio spread was 8.58%. Between-image drift exceeded 10% for
scalar224 C++ cohort 0 (10.63%) and scalar4096 Rust in both cohorts
(10.89%/13.43%); those cells remain unresolved despite any favorable medians.

Medians are ns/append, cohort 0/cohort 1. A/H directions use complete range
separation in both cohorts and all registered prerequisites; the canonical
H target uses the median-slower peer's complete range. Full per-peer ranges
and individual qualifications are retained in the timing record.

| Element × capacity | WF A | WF H | Rust A → H | C++ A → H | Observation |
| --- | ---: | ---: | ---: | ---: | --- |
| 8 B × 16 | 35.06/34.59 | 22.54/22.56 | 37.52/37.36 → 37.42/36.96 | 26.39/26.61 → 25.84/26.06 | H faster; H target pass |
| 8 B × 224 | 103.77/100.20 | 83.27/82.65 | 106.13/101.58 → 98.43/97.89 | 98.41/92.65 → 87.95/90.19 | unresolved; peer drift |
| 8 B × 256 | 106.95/104.10 | 89.99/89.09 | 109.97/106.84 → 103.85/103.63 | 100.93/97.86 → 94.82/94.30 | H faster; H target pass |
| 8 B × 288 | 114.83/112.87 | 111.54/107.98 | 116.77/118.31 → 115.18/109.62 | 104.22/104.09 → 99.13/99.22 | unresolved |
| 8 B × 4096 | 731.69/757.60 | 747.35/715.30 | 773.40/787.01 → 689.15/681.31 | 666.40/667.20 → 675.65/666.46 | unresolved; peer drift; H target unqualified |
| 256 B × 7 | 106.71/100.04 | 87.47/86.33 | 109.47/105.85 → 102.80/101.14 | 98.93/94.65 → 90.63/89.72 | H faster |
| 256 B × 8 | 108.39/109.22 | 91.81/92.28 | 113.39/114.44 → 106.31/107.99 | 103.47/103.30 → 95.16/94.75 | H faster |
| 256 B × 9 | 115.00/115.13 | 108.56/108.23 | 118.00/118.28 → 111.16/111.89 | 105.05/106.82 → 97.69/97.70 | unresolved |
| 256 B × 16 | 159.27/156.89 | 147.81/148.75 | 159.59/161.45 → 151.72/152.17 | 151.92/151.43 → 141.92/142.48 | unresolved; H target unqualified |
| 256 B × 256 | 1350.08/1356.93 | 1312.88/1309.78 | 1355.80/1363.60 → 1312.44/1310.63 | 1375.04/1352.17 → 1312.56/1326.72 | unresolved; H target unqualified |
| 256 B × 4096 | 156.07/167.69 | 161.45/162.06 | 160.79/171.89 → 163.31/170.09 | 15251.02/15091.64 → 15072.80/15018.48 | unresolved; H target pass |

H satisfies the selected-peer range target in **3/6 canonical cells**:
scalar16, scalar256 and wide4096. A satisfies 2/6 in this pair, scalar16 and
wide4096. Scalar4096, wide16 and wide256 leave append growth unfinished.
Qualified single-pair H gains occur at scalar16/256 and wide7/8. Scalar224
has separated H ranges in both cohorts but fails peer drift. No cell shows a
qualified H loss; the remaining overlaps establish neither gain nor equivalence.
Wide4096 H medians remain about 161/162 ns and overlap A, preserving the large
realloc behavior without the uniform-copy scale observed previously.

This is one exploratory pair, not repeatable qualification or selection of a
production cutoff. No B timing, cutoff sweep, H spare timing or other API was
run. Keep Vector append's spare and growth paths separate; H needs its own
spare timing qualification before adoption. Other APIs remain paused until
append qualifies, and all APIs precede a full container workload measurement.


#### Small-copy outlining: registered native-first discriminator

The remaining H cells take realloc at the same extents as Rust, whose linked
path includes more helper calls and instructions. H has no extra prefix copy
or offered-value spill on these paths. Counts alone therefore do not explain
the timings. A concrete local cost remains: the small-copy edge keeps old and
fresh pointers live across its allocator/copy/free calls, and the common
append prologue saves an additional register pair even on large realloc.

Test exactly one J variant of frozen H: put only the positive-full small-copy
transaction in one internal noinline helper taking old pointer, old initialized
bytes and new allocation bytes and returning the new pointer. It performs
malloc, checks NULL, copies the initialized prefix, frees old storage and
returns. On failure it enters the existing exhaustion floor before any copy,
retirement or publication. The caller retains its existing publication join.
Keep the 2048-byte cutoff, full/partial/empty distinctions, representation,
all other IR bodies, driver, seeds, peer/runtime objects and target flags. No
cold attributes, branch weights, new optimizer assumptions or threshold sweep.
This is a code-generation probe, not a selected compiler structure or policy.

Native gate before timing: the actual scalar and wide large-realloc append
paths must use 48-byte frames instead of H's 64, and fewer executed
instructions than H's 33/97 including the unchanged outer forwarding branch
but excluding allocator internals. Large, empty and spare paths must not call
the new helper; add no large-path conditional and no offered-value spill.
Record the small path's extra call and complete caller-plus-helper work and
frames, along with spare batch code. Stop without timing if this gate fails;
fewer instructions are a prerequisite, not a runtime-gain claim.

If it passes, first run the existing complete value/state/allocation checks,
exact request/release ledgers, cutoff-neighbor and returned-NULL observers,
and nested/partial/empty/ZST controls with their deliberate failures on the
new code. Preserve the bounded observer scope: moved-only realloc success
and the separate nested fixture do not qualify a generic source ABI. Failure
or an unintended native route stops timing rather than relaxing a check.

Then run one fixed H-then-J pair using the same eleven-cell driver and byte
budgets, nine samples in each of two balanced cohorts, and the existing
RAW/duration/ratio-spread/peer-drift criteria. Keep complete raw observations
and snapshot controls, every small canonical/neighbor cell and the 1 MiB
realloc counterexample. No repeated valid screen or adaptive sample count.
Both peers remain separate; range overlap or failed prerequisites stay
unresolved. Report small-path losses as well as large-path gains. One pair
is exploratory, never repeatable qualification or grounds to advance beyond
append. J needs its own spare timing before adoption.

Transient work belongs to .build/small-copy-outline in this Vector experiment;
retain the replay delta and compact full evidence here until superseded by
the qualified implementation. No compiler, library, specification, live tree
or other API changes in this probe. A failed native or timing hypothesis
does not authorize another register-allocation permutation.

#### Small-copy outlining: outcome

[Replay patch](small-copy-outline.patch), [complete samples](ecosystem-append-growth-small-copy-outline-samples.csv)
and [timing/native/correctness evidence](ecosystem-append-growth-small-copy-outline-timing.txt)
retain this single H→J pair. The native prerequisite passed: large scalar/wide
frames fell from 64 to 48 bytes and executed paths from 33/97 to 30/94 instructions,
including the public forwarding branch. Both keep five conditionals and direct
realloc; empty/spare paths bypass the helper. Small-path work instead rose
from 39/103 to 49/113 instructions and adds one internal call, with two 48-byte
frames instead of H's 64. Batch frames remain 80/336 bytes. Emitted WF text fell
from 19,444 to 18,720 bytes; these counts alone establish no runtime improvement.

All 53 guarded prerequisite stages had their expected exits, including real
returned-NULL preservation on both routes and deliberate value/state/allocation,
clock, copy, early-free and early-publication failures. The 45-row accounting
output is byte-identical to H. The observer retains its stated moved-only
realloc and separate nested-fixture scope. A read-only native-reduction syntax
error and its correction are retained; it changed no native input or result.

H and J each terminated 0 in 155.727 and 154.126 seconds respectively, with
unchanged executable hashes. Each retains 1,188 rows:594 real and 594 snapshot
controls. Positions are balanced three each per cohort. Both RAW probes report
41 ns; minimum real durations are 1.219071/1.244268 ms. Maximum WF/peer cohort-ratio
spread is 7.446%, between-image peer median drift 7.686%, and peer within-image
cohort spread 7.788%; all registered bounds pass. No observations were retried,
removed or control-subtracted.

Medians below are ns/append, cohort 0/cohort 1. Each peer remains separate; the
linked record contains every minimum/maximum and qualification calculation.

| Cell | H WF | J WF | Rust H→J | C++ H→J |
| --- | ---: | ---: | ---: | ---: |
| 8 B × 16 | 22.51/22.45 | 23.10/23.33 | 37.59/36.81 → 37.53/37.54 | 25.90/25.85 → 26.22/26.14 |
| 8 B × 224 | 86.64/84.83 | 89.45/88.30 | 102.46/98.49 → 104.16/99.89 | 95.05/88.18 → 95.85/93.77 |
| 8 B × 256 | 90.23/89.82 | 92.62/91.40 | 106.31/104.02 → 106.49/105.66 | 94.80/94.33 → 97.69/99.81 |
| 8 B × 288 | 106.24/105.87 | 108.56/107.05 | 107.83/108.15 → 110.09/109.95 | 96.63/96.64 → 98.94/96.94 |
| 8 B × 4096 | 721.16/688.78 | 758.94/720.59 | 692.24/707.74 → 699.92/706.38 | 629.24/645.73 → 653.76/657.36 |
| 256 B × 7 | 91.77/86.02 | 88.53/88.06 | 104.45/100.60 → 101.39/101.19 | 96.09/89.35 → 91.76/90.22 |
| 256 B × 8 | 93.57/95.42 | 93.30/93.84 | 108.56/109.59 → 106.67/106.33 | 95.77/98.52 → 96.87/97.79 |
| 256 B × 9 | 107.96/108.21 | 110.89/108.22 | 111.64/110.37 → 113.54/110.78 | 97.48/97.44 → 100.04/99.60 |
| 256 B × 16 | 150.22/150.64 | 148.87/149.71 | 151.67/150.97 → 151.26/151.35 | 143.45/142.87 → 143.76/144.58 |
| 256 B × 256 | 1335.03/1342.67 | 1322.10/1319.53 | 1341.22/1334.46 → 1340.75/1331.00 | 1331.76/1339.85 → 1323.99/1326.85 |
| 256 B × 4096 | 168.71/170.98 | 155.00/155.78 | 174.44/170.97 → 161.03/161.34 | 15227.99/15262.03 → 14893.50/14934.87 |

All eleven H/J full ranges overlap in both cohorts. This pair demonstrates
neither a J gain nor a J loss, and does not select J. Both images meet the
selected-peer range target in 3/6 canonical cells: scalar 16/256 and wide 4096.
For J, scalar 16 overlaps C++ in cohort 1, scalar 256 overlaps C++ in both,
and wide 4096 overlaps Rust in both; scalar 4096, wide 16 and wide 256 overlap
both peers and remain unqualified. The reduced large-path work has not resolved
append growth. No new permutation, cutoff sweep or spare timing followed.

This is one exploratory pair, not repeatable qualification. Any future adopted
candidate needs its own spare-path timing. Only Vector append spare/growth is
in scope; other APIs remain paused until append qualifies, and all APIs must
qualify before a full container workload measurement.

#### Frozen-H stability: registered A/A diagnostic

The H→J pair left every H/J range overlapping, despite J's smaller large-path
frame. Inspecting the actual timed caller found one common seven-instruction
append loop for all peers; preparation, reset and destruction remain outside
the clock. Before another implementation change, test whether longer aggregate
observations of unchanged H resolve or reproduce the remaining growth cells.
This diagnoses measurement stability, not an optimization, a new ranking rule
or a retroactive revision of any earlier verdict.

Use the existing `.build/small-copy-growth/H-timed` executable, SHA256
`26e66b2e9b0f5aa020384d64f0c53010f107f364cca4c6850fba6d1735edea1e`,
without rebuilding or changing the driver, WF/peer/runtime objects, allocator,
seeds, call ABI or preparation. Run exactly two sequential fresh processes,
H1 then H2, each with `growth-api-measure 268435456 9 8589934592` under the
exclusive heavy-command guard. Set the limit to 1,200 seconds per process
before launch, conservatively above four times the preceding approximately
155-second screen. Preserve direct exits, wall time and before/after hashes.
Root review of this preregistration precedes heavy execution.

The existing eleven cells remain unchanged: scalar 16/224/256/288/4096 and
wide 7/8/9/16/256/4096. The ordinary byte budget rises from 64 to 256 MiB;
wide4096 remains at 8 GiB. This increases the number of freshly prepared
batches summed into each sample, through the existing ceiling formula. It
does not enlarge an individual timed batch, its live footprint or its clock
interval. In particular, scalar4096/wide16/wide256 still have 31/240/15 contexts.
It also does not increase the nine statistical samples per cohort or assert
that sequential allocator histories are statistically independent.

Retain both rotating/reverse cohorts, three occurrences of each implementation
position per cohort, all three implementations and all snapshot controls. Each launch
must retain all 1,188 rows, including 594 real and 594 control rows. Do not
subtract controls. Ordinary-cell real aggregate intervals must actually reach
at least 4 ms; wide4096 must reach at least 1 ms. The existing nondecreasing
RAW-clock probe must report a nonzero increment at most 100 ns. Keep the
existing per-cell WF/peer cohort-median-ratio spread at most 10% for each peer,
and require each peer's corresponding cohort median to drift at most 10%
between H1 and H2, using H1 as denominator. Report every failed prerequisite;
longer prescribed work does not excuse an observed duration or stability miss.

Apply the unchanged [conservative target](../ECOSYSTEM.md#optimization-criterion)
separately to every cell and launch: select the median-slower Rust/C++ peer
within each cohort, then require the complete WF range below that peer's
complete range in both cohorts. Report both peers separately, including
one-peer wins. A repeatable target pass in this diagnostic requires both H1
and H2 to pass with all prerequisites. A robust deficit requires the complete
WF range above both standard-peer ranges in both cohorts; report whether that
also repeats across launches. All overlaps and failed qualifications remain
unresolved. Also report H1/H2 range directions to expose drift in the unchanged
implementation. Observed ranges are not confidence intervals; no favorable
median, pooling, outlier removal or post-hoc tolerance replaces this test.

Reuse H's recorded correctness and ownership evidence. Before these launches,
verify its input/image hashes and rerun only the existing timed
`growth-api-check` and account `api-check` positive commands under the guard;
no new observer framework or compiler build is needed. Any actual prerequisite
failure stops the diagnostic with its evidence retained. Execute only these
two fixed launches, with no adaptive retry, filtering, threshold choice or
follow-on implementation variant.

Transient outputs belong to `.build/append-stability`. Retain complete raw
samples and one compact replay/evidence record in this Vector experiment when
finished; no source patch exists because executable code is unchanged. This
diagnostic does not qualify H's spare timing or generic source ABI, select a
production policy, or authorize another API. Vector append's spare and growth
paths remain separate; other APIs wait until append qualifies, and all APIs
precede a full container workload measurement.

#### Frozen-H stability: outcome

[Complete observations](ecosystem-append-growth-stability-samples.csv) and the
[compact timing record](ecosystem-append-growth-stability-timing.txt) retain the
two unchanged-image launches. H1/H2 terminated 0 in 204.999/203.839 seconds;
all frozen pins and before/after executable hashes matched. The two bounded
positive checks passed. No code was rebuilt and no source patch exists.

Each launch retains 1,188 rows, including 594 real and 594 snapshot controls,
with exact capacities, registered cycles and balanced positions. Both RAW
probes report 41 ns. Minimum ordinary real aggregate intervals were
5.312253/5.158587 ms; wide4096 minima were 1.189559/1.246619 ms. Maximum
WF/peer cohort-ratio spread was 5.760%, and interlaunch peer median drift
6.903%. All registered instrument and stability prerequisites passed; the
individual timed-batch footprint remained unchanged.

Medians are ns/append, cohort 0/cohort 1. Full ranges and separate peer verdicts
are retained in the timing record. P means the registered target passes in
both cohorts of that launch; U means unresolved.

| Cell | H1 WF | H2 WF | Rust H1→H2 | C++ H1→H2 | H1/H2 target |
| --- | ---: | ---: | ---: | ---: | --- |
| 8 B × 16 | 22.70/22.59 | 23.00/23.19 | 37.49/37.21 → 38.10/37.69 | 26.01/26.13 → 26.36/26.28 | P/P |
| 8 B × 224 | 83.36/84.12 | 87.58/87.22 | 98.53/98.82 → 101.40/99.99 | 90.79/89.30 → 92.37/94.37 | P/U |
| 8 B × 256 | 89.21/90.54 | 91.29/91.84 | 103.93/103.89 → 106.28/105.49 | 95.71/95.76 → 96.67/96.63 | P/P |
| 8 B × 288 | 113.08/108.48 | 107.23/107.81 | 111.97/108.87 → 111.40/111.88 | 101.54/98.36 → 96.35/99.15 | U/U |
| 8 B × 4096 | 727.06/729.55 | 712.31/692.54 | 695.64/707.93 → 691.78/707.03 | 665.08/658.81 → 648.53/643.65 | U/U |
| 256 B × 7 | 87.81/89.21 | 87.35/87.88 | 100.98/103.30 → 101.64/104.01 | 91.31/92.72 → 90.25/91.14 | P/P |
| 256 B × 8 | 93.09/92.25 | 92.95/95.30 | 107.39/108.08 → 107.63/107.62 | 95.23/95.31 → 96.16/96.05 | P/P |
| 256 B × 9 | 110.96/108.62 | 108.94/109.62 | 111.19/111.34 → 111.03/111.02 | 101.43/98.81 → 99.83/100.00 | U/U |
| 256 B × 16 | 155.30/148.78 | 152.39/154.21 | 154.98/156.49 → 154.13/153.92 | 146.03/145.92 → 147.79/145.70 | U/U |
| 256 B × 256 | 1347.98/1344.68 | 1348.66/1343.37 | 1357.61/1343.39 → 1329.91/1339.69 | 1368.96/1357.10 → 1352.73/1338.21 | U/U |
| 256 B × 4096 | 158.47/154.33 | 157.77/158.44 | 169.91/156.46 → 167.72/167.25 | 15047.05/14823.83 → 14906.53/14833.81 | P/P |

The same 3/6 canonical targets pass in both launches: scalar16/256 and
wide4096. Neighbor wide7/8 also pass twice. Scalar224 passes H1 but remains
unresolved in H2; its 144.70 ns high sample is retained. All eleven H1/H2 WF
ranges overlap in both cohorts. Longer aggregate observations therefore did
not resolve scalar4096, wide16 or wide256, which still overlap Rust throughout.
Scalar4096 is separated slower than C++ in both H1 cohorts and H2 cohort1,
but overlaps C++ in H2 cohort0. No cell meets the robust deficit test against
both standard peers. Overlap does not establish equivalence.

This confirms the stated passes only within this registered two-launch
diagnostic. It selects no optimization or production policy, relaxes no
criterion and changes no earlier verdict. No further run followed. H's spare
timing and generic source ABI remain unqualified; other APIs remain paused
until append qualifies, with all APIs preceding a full container workload.

#### Exact-H C append twin: registered native-only discriminator

The retained C controls do not implement H's exact public append operation:
they use header storage, runtime layout arguments or a different timed
boundary. Test one ordinary typed C expression of H as an independent lowering
reference, not a guaranteed performance floor. No timing is authorized.

Use specialized scalar/256-byte entry points with the same descriptor address
and seed ABI and exact 24-byte `{len, cap, payload}` layout. Preserve H's valid
domain, 8193 capacity ceiling, empty-anchor behavior, doubling capped at 8193
(including full capacity 4097), full-positive copy at old bytes at most 2048
and realloc above it. Preserve NULL exhaustion before retirement/publication,
old-prefix copy extent, free-before-publication, payload/cap publication before
new-value construction, and final length publication/return. Spare append must
allocate nothing. Use no restrict, optimizer assumptions, explicit cold hints,
branch weights or new allocator policy. Ordinary C local values may express
pointer flow and conditional capacity selection; no facts are added to WF.

Compile this one source directly at the same native Clang -O3 target settings
as H, separately emitting optimized LLVM for inspection. Do not compile that
optimized LLVM again to produce the measured native object: this avoids an
extra optimizer pass. Under the serialized guard, inspect the native object
against frozen H's reached public append paths, including its forwarding
branch. Record small, large, saturating, empty and spare paths: executed
loads/stores, conditionals, calls, register saves/frames, constructor stores
and publication order. Compare all emitted instructions separately from
executed successful paths; no allocator-internal cost follows from counts.

A useful new lead must remove concrete executed memory traffic, a payload
copy/spill, helper call or redundant branch while preserving the contract.
A J-sized register shuffle or three-instruction saving alone supplies no
new timing rationale; stop the native probe without another C permutation.
Any route, extent, publication or failure mismatch also stops. Report source
and object hashes, exact compile commands, direct statuses and native evidence
to the root before any correctness or timing expansion.

The source and native artifacts belong only to `.build/append-c-twin` in this
Vector experiment. They serve this independent lowering-quality control and
are removed or superseded when a production append implementation qualifies.
No production compiler, library, specification, tree, other API or ranked
harness changes are part of this probe.

#### Exact-H C append twin: native-only outcome

The retained [63-line C reference](append-c-twin.c) and
[native evidence/replay commands](ecosystem-append-c-twin-native.txt) show no
useful overall native advantage over H. After the native-only stop, the exact
source and compact record are retained here for replay; compiled intermediates
remain in scratch. This retention changes no experiment criterion. C removes
one full-path payload reload,
coalesces cap/payload stores, and replaces the saturation branch with conditional
selection. However, its frame grows from 64 to 80 bytes, adding a saved register
pair and 32 bytes of stack save/load traffic. Allocator calls and the wide
constructor's 15 vector stores plus two scalar stores remain unchanged.

| Successful path | H scalar/wide instructions | C scalar/wide instructions | H/C conditionals |
| --- | ---: | ---: | ---: |
| Large realloc | 33/97 | 35/99 | 5/4 |
| Small copy | 39/103 | 45/109 | 5/4 |
| Saturating full capacity | 33/97 | 35/99 | 5/4 |
| Empty | 27/91 | 33/97 | 3/3 |
| Spare | 19/83 | 21/85 | 1/1 |

Counts include H's public forwarding branch and exclude allocator internals.
The evidence retains every reached PC, frames, publication and constructor
shape, optimized C IR observations and call relocations. Smaller static C bodies
are not fewer executed instructions. No missing allocator attribute or TBAA
cause is inferred.

All six guarded compile/inspection stages terminated 0. Two were a comment-only
recompile after correcting the valid domain to `len<=cap`, `len<8193` and
qualified backing: larger-cap spare calls already worked unchanged. Both native
object and optimized IR reproduced byte-identically. A read-only reducer parsing
failure and its correction are retained separately. No runtime correctness,
NULL-preservation, ownership or timing tests were run, and no C permutation
followed. C is an independent reference, not a proven performance floor or a
selected implementation. Vector append remains unqualified; other APIs do not
advance.


#### Ordinary source compilation of inline Slots ownership

Implement the descriptor-placement candidate in the normal compiler, keeping
allocated one-byte zero extents and the existing allocation/copy/free growth
route. The isolated earlier control was a separately allocated descriptor,
not the current header-first implementation; those results do not establish
a win over the current compiler. This comparison uses the retained production
compiler from the boundary-length-reuse experiment as its control and identical
ordinary WF source, including setup-only destination-return adapters. Neither
image receives hand-edited optimization attributes or a rewritten function body.
The expected native difference is direct owner metadata access and payload-only
allocation, with wider owner transport remaining a possible cost.

Before timing, require spare/growth value and state checks, the exact allocation
ledger, retained-call and nested-owner regressions, and failed-growth preservation.
Inspect actual public signatures and native append paths; confirm identical peer
and runtime inputs. Use the existing nine-sample balanced two-cohort schedule,
RAW-clock controls and unchanged range-separation criterion. Begin with baseline
then candidate: spare `api-measure 4194304 9`, growth
`growth-api-measure 67108864 9 8589934592`. This is an initial comparison, not
repeatability qualification; a candidate that clears all cells requires a second
fixed candidate/baseline pair. Retain every sample, including losses and overlaps.
Do not infer allocator costs from instruction counts. No layout adoption or
completion of append follows merely from successful compilation or fewer loads.

#### Ordinary source inline ownership: initial append outcome

The [ordinary source/compiler replay patch](ordinary-inline-owner.patch),
[spare samples](ecosystem-append-inline-owner-spare-samples.csv),
[growth samples](ecosystem-append-inline-owner-growth-samples.csv), and
[commands, statuses, pins, ledgers and full per-peer ranges](ecosystem-append-inline-owner-timing.json)
retain this first source-compiled comparison. The compiler replay delta is
explicitly reconstructed from the frozen compiler implementation before later
copy/shift changes; it is not claimed to be an original source snapshot or a
byte-identical rebuilt executable. The measured CLI and image hashes are retained.

Both ordinary-source builds, append checks/accounting and existing whole-trace
consumer correctness/accounting checks passed. The initial baseline compile
rejected an unsupported explanatory WF comment; removing only that comment
fixed the syntax, and the failure is retained. No whole-trace timing ran.
The ordinary destination-first prepare signatures agree across both layouts;
append owner references widen from 8 to 24 bytes. All 16 peer/runtime object inputs
are byte-identical. Append callers retain 32-byte frames and the 64-byte growth
helper; full growth still executes malloc, initialized-prefix memmove and free.
Wide append-one has no payload spill; both wide batch callers retain 272-byte
frames with 224 bytes of saved vector constant state. This comparison changes
owner placement and payload allocation extent, without importing the earlier
anchor, realloc or forced-inlining prototypes.

All four screens exited 0: spare baseline/candidate 27.399/26.964 s and growth
153.988/152.447 s. All 3456 rows are retained, half real and half controls, with
nine balanced samples per implementation/cohort. Minimum real intervals are
1.768/1.479 ms for spare and 1.298/1.229 ms for growth. Both growth RAW probes
observed 41 ns. Maximum WF/peer cohort-ratio spread is 5.611%; maximum between-image
peer median drift is 9.345%. No samples were filtered or repeated.

Medians below are **ns/append, cohort 0/cohort 1**. The final column is the
candidate sufficient range target against the median-slower peer, not a median
ranking or a claim of repeatability. Full ranges and both baseline peers remain
in the record.

| Path / element / count | WF baseline | WF inline | Rust inline | C++ inline | Target |
| --- | ---: | ---: | ---: | ---: | --- |
| spare / 8 B / 16 | 0.478/0.477 | 0.487/0.479 | 0.478/0.477 | 1.028/1.019 | unresolved |
| spare / 8 B / 256 | 0.505/0.510 | 0.397/0.400 | 0.426/0.426 | 0.939/0.936 | pass |
| spare / 8 B / 4096 | 0.527/0.570 | 0.354/0.370 | 0.849/0.859 | 0.927/0.932 | pass |
| spare / 256 B / 16 | 5.072/4.990 | 4.702/4.631 | 4.585/4.579 | 7.342/7.316 | pass |
| spare / 256 B / 256 | 4.490/4.807 | 4.420/4.426 | 4.341/4.382 | 7.080/7.121 | pass |
| spare / 256 B / 4096 | 4.594/4.435 | 4.415/4.376 | 4.318/4.321 | 7.073/7.112 | pass |
| growth / 8 B / 16 | 28.743/28.871 | 22.976/22.764 | 37.232/36.918 | 25.912/25.557 | pass |
| growth / 8 B / 256 | 93.546/91.530 | 89.100/87.908 | 101.840/101.710 | 94.511/94.349 | pass |
| growth / 8 B / 4096 | 2221.876/2226.557 | 706.337/696.237 | 735.194/736.028 | 627.812/639.890 | unresolved |
| growth / 256 B / 16 | 145.520/147.367 | 137.302/138.064 | 151.467/148.032 | 146.797/144.161 | unresolved |
| growth / 256 B / 256 | 1544.930/1528.068 | 1306.882/1309.783 | 1314.572/1314.332 | 1319.441/1318.120 | unresolved |
| growth / 256 B / 4096 | 17702.229/17733.905 | 14864.965/14814.807 | 157.691/155.709 | 14881.866/14778.903 | unresolved |

Spare target coverage is baseline 6/6 versus inline 5/6: inline scalar 16 retains
a cohort 0 WF maximum 2.078 ns, crossing C++'s minimum 1.012 ns. Scalar 256/4096
spare before/after ranges separate in both cohorts; the other four overlap.
Growth stays 2/6 matched cells. The inline scalar 16/4096 and wide 256/4096
growth ranges separate below baseline in both cohorts; scalar 256 and wide 16
before/after ranges overlap. The remaining peer targets remain unresolved.
Wide 4096 is still separated slower than Rust, while overlapping C++; its
explicit 1 MiB relocation remains, unlike Rust's realloc route. Empty/capacity-one
policy cells are separate: inline 3/4 targets versus baseline 2/4, without matched
capacity-growth claims.

This initial pair does not qualify append or select another growth policy.
The registered all-cell condition for a reverse qualification pair is unmet.
The host resolves memcpy and memmove to the same address (reproducible check
in the record), so changing the call name alone supplies no dynamic speedup
claim; a later compiler change requires its own native and timing evidence.
Keep Vector append spare and growth separate; no other API or full-container
timing substitutes for their qualification.

#### Single-reserve full-growth source screen

The [current library edit](../../../../lib/std/collections/vector/grow-vector.wf)
selects `1`, doubled capacity when it fits, or the ceiling before one
`grow_vector_reserve` call. The three previous reserve sites duplicated the
allocation/copy/free blocks in the optimized growth helper. This is the same
capacity policy and public contract; it changes no payload movement rule.

The retained inline-owner compiler embeds the older library source, so simply
recompiling the unchanged Vector program with that binary emitted byte-identical
raw LLVM and did not test this edit. For a source-admissibility screen, an exact
copy of the edited generic body was compiled as a local `trial_grow_full`, with
a monomorphic `GrowVector<u64, 8193>` caller carrying the same preconditions.
The first scratch assembly had one extra blank line and failed `FORM-2`; after
canonical spacing, the frozen compiler's `--emit-llvm` exited 0. Apple clang
`-O3 -x ir -c` also exited 0. In that one object, the trial function has 40
instructions and four calls versus 61 and ten in the separately emitted old
embedded helper; its selected growth path has one malloc/memmove/free sequence
and one resource-abort edge. These are static code observations, not timing or
a comparison of actual new library images.

The subsequently rebuilt malloc/copy/free control compiler (`4685a08a…abd4`)
embeds the edited library (`63286949…5ca83`). Ordinary Vector source emission
and Apple clang `-O3` object compilation both exited 0. Each actual scalar/wide
`grow_full` instance has one reserve call in raw LLVM and 40 static instructions
with four calls in the optimized object, versus 61 instructions and ten calls
in the previous inline-owner object. This confirms the embedded-library change
in an actual compiler; it does not supply timing or substitute for the pending
full correctness/accounting comparison.

#### Full-positive reallocation: registered ordinary-source comparison

Compare two actual rebuilt compilers containing the same updated Vector
library and inline runtime-Slots owner representation. The control retains
malloc/copy/free growth; the candidate reallocates only when the old run is
full and its old payload extent is positive. Empty, partial and zero-stride
runs retain fresh allocation, initialized-prefix copy and retirement. The
allocated one-byte zero extent remains. There is no byte cutoff, source-name
specialization, local copy of the standard library or hand-edited LLVM body.
The older frozen inline compiler embeds the old library and is not this
comparison's control. Freeze both compiler implementation deltas, embedded
library inputs, emitted modules and linked images before measurement.

The concrete target is the current wide-4096 growth path's explicit 1 MiB
copy, which remains about 94 times Rust's realloc time while overlapping C++.
Earlier route trials predict a small-scalar tradeoff; they do not establish a
universal gain or select this production policy. Using the same updated
single-reserve library in both newly built compilers keeps that source edit
out of the allocation-route comparison. Inspect actual append paths, including
spare loops, helper boundaries, frames, copies and retirement/publication order.
Do not infer cost from a changed memcpy/memmove symbol name: they resolve to
the same address on the current host, although LLVM's treatment may differ.

Require the existing ordinary-source append value/state checks, exact growth
allocation ledger and complete cleanup, zero-allocation spare checks, negative
value/state/allocation/clock observations, and maintained failed-growth and
nested-owner regressions before timing. Observe positive-full realloc,
cap-zero fresh allocation and partial fresh allocation explicitly; preserve
old owner/payload on real allocation failure and retain both moved and in-place
realloc witnesses. Check that both ordinary public adapters have the intended
ABI and that peer/runtime linked inputs are identical. Existing consumer
correctness/accounting checks may validate the migrated ledger without timing
another API or the whole workload.

Then run only one fixed control/candidate pair for each append path, serialized
under the guard: control spare, candidate spare, control growth, candidate
growth. Use `api-measure 4194304 9` and
`growth-api-measure 67108864 9 8589934592`, unchanged two cohorts and balanced
three positions per peer. Retain all 648 spare rows and 1080 growth rows per
image, including every empty/snapshot control and the separate capacity-0/1
policy cells. Every real ranked sample must reach 1 ms; keep controls unranked
and never subtract them. Require a nondecreasing RAW clock with observed
nonzero resolution at most 100 ns, WF/peer cohort-ratio spread at most 10%, and
report between-image peer median drift above 10% as unresolved attribution.
Use the existing full-range target independently in both cohorts against the
median-slower peer; show Rust and C++ separately. Overlapping before/after
ranges remain unresolved, and all real samples remain in the envelopes.

No retries, cutoff sweep or extra samples follow an adverse result. Passing
only a subset does not qualify append. A reverse fixed candidate/control pair
is considered only after all append cells pass the registered prerequisites
and range target. Preserve direct exits, elapsed times, raw samples, source and
image hashes, and any failed prerequisite. Reuse `.build/ordinary-inline-owner`
with distinct route-control/route-candidate outputs; retain compact evidence
in this existing Vector home until production append qualification supersedes
it. No other API or full-container timing is authorized by this comparison.

#### Full-positive reallocation: original-link-order outcome

Both ordinary-source compilers passed append spare/growth correctness, the
exact allocation ledger, fault controls and consumer correctness/accounting.
The same updated embedded library was used by both actual rebuilt compilers;
[the control patch](full-positive-reallocation-control.patch) reconstructs the
malloc/copy/free compiler from the recorded source revision. No LLVM edit or
source-name optimization was used. All four fixed launches exited 0: MCF/R
spare took 27.781/28.025 s and MCF/R growth 157.678/155.515 s. Their frozen image
hashes, direct command/status records, source pins and native excerpts are in
[the evidence record](ecosystem-append-full-positive-timing.json).

The following are growth medians in ns/append, cohort 0 / cohort 1. R is the
full-positive realloc candidate; both peers shown are from its same image.
Complete per-peer ranges and both control-image peers are retained in the
record and [all 2160 growth rows](ecosystem-append-full-positive-growth-samples.csv).

| Element / capacity | WF MCF | WF R | Rust R-image | C++ R-image | MCF→R range observation |
|---|---:|---:|---:|---:|---|
| 8 B / 16 | 23.00 / 23.02 | 35.16 / 34.97 | 36.12 / 36.61 | 25.59 / 26.00 | R slower |
| 8 B / 256 | 89.89 / 90.09 | 102.19 / 99.76 | 103.30 / 102.49 | 96.04 / 94.92 | R slower |
| 8 B / 4096 | 706.54 / 718.38 | 679.35 / 687.36 | 732.71 / 736.46 | 668.11 / 655.06 | Overlap; peer drift 11.66% |
| 256 B / 16 | 137.06 / 138.48 | 150.40 / 149.06 | 150.21 / 149.73 | 140.01 / 139.54 | R slower |
| 256 B / 256 | 1340.78 / 1329.43 | 1321.54 / 1320.37 | 1324.28 / 1315.66 | 1315.18 / 1328.38 | Overlap |
| 256 B / 4096 | 14852.69 / 14855.51 | 161.20 / 161.14 | 163.88 / 166.05 | 14833.13 / 14877.75 | R faster; peer drift 12.34% |

MCF meets the registered range target in 3/6 matched growth cells; R meets it
only at wide-4096 (1/6). R's medians closely approach Rust across these cells,
but overlapping full ranges do not pass. The approximately 92-fold wide-4096
WF difference is an observed magnitude, not qualified paired attribution:
Rust drift exceeds 10%. Scalar-4096 also fails that drift condition. The small
scalar and wide-16 regressions are separated in both cohorts with peer drift
within the limit. Capacity-0/1 policy cells remain separate: both arms pass
3/4, with the wide-capacity-1 realloc route also slower.

Spare meets the range target in 6/6 MCF cells and 5/6 R cells: R wide-256 has
a retained cohort-1 outlier crossing C++. The
[1296 spare rows](ecosystem-append-full-positive-spare-samples.csv) also expose
39–63% between-image Rust scalar median drift. Although all 17 shared linked
inputs are byte-identical, the original link order puts changing WF code
before the peers. The Rust scalar batch loop moves by 128 bytes and crosses a
4 KiB address boundary; this identifies a placement confound without proving
its hardware cause. A separately registered peer-first comparison must retain
this adverse result rather than replace it.

All real intervals exceed 1 ms: minima are 1.634/1.321 ms for MCF/R spare and
1.352/1.302 ms for growth. Both growth RAW probes observe 41 ns, nondecreasing;
all rows, controls and cohort spreads remain in the record. No samples were
retried, removed or subtracted. Append remains unqualified.

Actual reached scalar/wide append bodies contain 45/109 static instructions
for MCF and 47/111 for R, with 80/64-byte frames respectively (the wide public
forwarding branch is additional). Both already expose allocation directly:
MCF calls malloc/copy/free, R calls realloc on positive full growth, and both
retain length across allocation. Surviving out-of-line `grow_full` symbols are
not the measured call path. R has no extra grow-helper call or record spill
to remove; Rust and C++ still use their own growth helpers. These instructions
do not assign the small realloc loss to compiler overhead or establish a
universal allocator policy. No additional native variant follows this result.

#### Matched edit APIs: inline owner, zero extents and local swap-remove

The current source is `d03ee3709c437cf20d8c0271f57f9c0e87638a97`.
The frozen compiler and source hashes, per-peer medians and complete ranges are
in [the API evidence](ecosystem-edit-swap-final.json), with
[all 3888 samples](ecosystem-edit-swap-final-samples.csv). The fixed nine-sample
launch exited 0 in 283.47 s. All real intervals exceed 1 ms (minimum
1.283459 ms). Four cells fail the 10% cohort-ratio stability condition.

Each row below comprises six width/count cells: 8/256-byte values and initial
counts 16/256/4096. The median column compares WF with the slower standard
peer's combined median: less than 1 is faster. This descriptive ratio is not
the qualification criterion. The final column counts cells whose WF complete
range is below that cohort's median-slower peer in both cohorts, with duration
and stability also passing. This one launch establishes no repeatability.

| Operation | WF/slower-peer median ratio range | Qualified cells |
|---|---:|---:|
| reserve-noop | 0.987–1.008 | 0/6 |
| reserve-grow | 0.935–0.997 | 0/6 |
| insert-front-spare | 0.860–1.018 | 2/6 |
| insert-mid-spare | 0.433–1.026 | 1/6 |
| insert-front-full | 0.948–0.997 | 1/6 |
| insert-mid-full | 0.927–1.018 | 1/6 |
| remove-front | 0.964–1.049 | 1/6 |
| remove-mid | 0.931–1.069 | 1/6 |
| remove-back-chain | 0.834–1.031 | 3/6 |
| drain-all | 0.833–1.012 | 1/6 |
| swap-remove-front-chain | 0.951–1.182 | 0/6 |
| truncate-half | 0.786–1.007 | 1/6 |

Overall, 50/72 combined medians are no slower than the slower peer; only
12/72 cells meet the complete-range, duration and stability criterion. Rust
and C++ separately have 6/72 and 9/72 complete-range wins for WF. This does
not qualify Vector. Construction and `free_empty` have no dedicated API
timing rows, and current whole-Vector performance qualification is missing.
The current compiler emits contiguous Slots shifts under the inline owner.
This panel does not compare them against a walk-only compiler with that same
layout. The [earlier header-first trial](#slots-final-code-and-paired-timing-regression-prevents-selection)
failed its no-regression criterion; the changed representation does not by
itself resolve that adverse observation or establish a shift speedup.
Back-pop and swap-remove measure complete removal chains from the stated
initial count; their averages do not identify first-removal latency. Setup,
independent final-state verification and cleanup are outside the operation
clock; calls include the ordinary adapter and owned-value consumption.

The [earlier 3888-row panel](ecosystem-edit-precallback-samples.csv) is retained
as adverse, nonmatching evidence for drain/truncate: its WF callback carried
edit-shape dispatch absent from the peers. The current four-field sequential
consume callback retains value and checksum checks without that dispatch.
Separately, public `swap_remove` now takes the last owner before exchanging
with an earlier selected slot. Reached native code removes the retired-slot
write/reload; the independent source program checks first, middle, last and
owning cases, and the allocator ledger remains exact. Both panels ran once;
no adverse sample was removed or retried.

Native inspection also finds remaining costs: wide insertion materializes a
256-byte record before its bulk shift and spills 240 bytes across `memmove`;
Rust constructs those fields after shifting. Full insertion in WF and Rust
resizes then shifts, while C++ can copy the old prefix and suffix directly
into final positions. These observations identify operations, not a universal
allocator or instruction-count performance prediction. The remaining wide
swap-remove local materialization and insertion costs are retained in TODO.

The scalar reserve-noop cell at count/capacity 4096 exposes a separate fast-path
cost. Filtering the retained CSV by `operation == reserve-noop`,
`element_bytes == 8` and `count == 4096` gives 54 rows: nine samples per peer
in each cohort. Dividing `elapsed_ns` by `operations` (4194304 per sample)
gives these medians and complete ranges in ns/op:

| Peer | Cohort 0 median [range] | Cohort 1 median [range] |
|---|---:|---:|
| WF | 1.260906458 [1.256783962–1.315335512] | 1.283277988 [1.258641481–1.502474308] |
| C++ | 1.263648272 [1.259366512–1.308917999] | 1.305937767 [1.260131598–1.397977352] |
| Rust | 0.954945803 [0.949462175–1.008977413] | 0.964005709 [0.949362755–0.980069160] |

All intervals in this cell exceed 3.98 ms and its cohort-ratio stability checks
pass. WF overlaps C++ and is separated slower than Rust in both cohorts; the
cell remains unqualified. This is still one launch, not repeatability evidence.
The common driver times ordinary reserve calls and returned-capacity stores;
setup, snapshot controls, verification and cleanup are outside that interval.

The frozen `swap-final/ecosystem/vector-costs-timed` image has SHA256
`df7099c62d0aad27d7ea8605da142e88bb22eb07f976a62a44cfe07e69e1e20a`,
also pinned in the API evidence. Its reached WF reserve entry at `0x1000af6dc`
saves four register pairs before loading capacity and testing `cap >= total`
at `0x1000af6f0`; the no-op return restores them. C++ at `0x100012eb8` likewise
eagerly saves 64 bytes. Rust at `0x100017280` returns without stack traffic;
its growth-only saves begin at `0x1000172a8`. Counting the executed no-op paths
gives WF 15 instructions/64-byte frame, C++ 17/64 and Rust 10/no frame. None
allocates, copies payload or mutates the vector on this path. Reproduce the
inspection on that exact image with:

```sh
llvm-objdump -d --no-show-raw-insn \
  --disassemble-symbols=_wf_vector_api_word_reserve,_cpp_vector_api_word_reserve,_rust_vector_api_word_reserve \
  <scratch-root>/ordinary-inline-owner/swap-final/ecosystem/vector-costs-timed
```

Ordinary `grow_vector_reserve` already returns before `grow` when capacity
suffices. General fast-path frame placement is therefore a concrete remaining
code-generation hypothesis. These instruction counts do not attribute the
timing difference or promise a gain; overlapping C++ ranges do not establish
an unavoidable performance floor or justify relaxing qualification.

The final linked timed/accounting correctness checks and deliberate oracle
fault controls pass. The source revision passes all canonical Linux/macOS
gate groups and Linux/Windows native-host CI. Those functional results do not
turn overlapping timing ranges into performance wins.


#### Current-source append after the API fixes

The same frozen current-source image was measured once with the retained
append work budgets: spare `api-measure 4194304 9` and growth
`growth-api-measure 67108864 9 8589934592`. Direct exits are both 0; elapsed
times are 27.871 and 156.158 s. [The evidence record](ecosystem-append-swap-final.json)
contains both peer ranges, command/status records, identities and placement
checks; [648 spare rows](ecosystem-append-swap-final-spare-samples.csv) and
[1080 growth rows](ecosystem-append-swap-final-growth-samples.csv) retain every
sample and control. Minimum real intervals are 1.322 and 1.224 ms; the RAW
clock probe observes 41 ns and maximum cohort-ratio spread is 6.66%.

Spare append meets the selected-peer range target in 6/6 cells. Full growth
meets it in 2/6 canonical cells (8-byte/capacity16 and 256-byte/capacity4096);
the other four overlap. The separate capacity0/1 policy cells pass 4/4.
Scalar capacity16 still loses to C++ despite beating the median-slower Rust
control. Thus append as a whole remains unqualified.

The common driver and peer objects, and all 52 peer entry addresses, match
the preceding image. Constant-pool operands still move: this is a current
image comparison, not clean causal attribution of an old/new speedup.
