# Growable vector library costs

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
unproved and is not grounds for a new metadata family. The existing
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
