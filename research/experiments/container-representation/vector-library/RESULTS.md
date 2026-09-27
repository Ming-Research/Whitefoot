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
