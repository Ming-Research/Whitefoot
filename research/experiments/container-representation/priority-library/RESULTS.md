# Reusable PriorityQueue cost comparison

This experiment asks how the complete owning binary-heap library compares
with matched native heaps for scalar and wide inline values. Its prospective
matrix and selection criterion are recorded in the
[PriorityQueue investigation](../../../investigations/containers-and-resources/X1-LIBRARY.md#reusable-priorityqueue-trial).
The explicit Makefile consumes the maintained library and the local
sources. It is not part of an ordinary correctness gate. This record owns
the comparison; retire the replay sources and harness when a superseding
experiment replaces every maintained claim depending on them.

## Practical Rust and C++ comparison

The opt-in `ecosystem-*` targets implement the contract and premeasurement
criteria in [ECOSYSTEM.md](../ECOSYSTEM.md). The current compiler imports
`std::collections::priority_queue` through aliases in `priority-library.wf`;
the single source passed to `--emit-llvm` keeps the two complete-trace C ABI
entry points. The historical data below is unchanged and is not a denominator
for the new comparison. The current measurements and their limits follow the
reproduction contract below.

```sh
perl .github/run-check.pl priority-ecosystem-build \
  make -C research/experiments/container-representation/priority-library ecosystem-build
perl .github/run-check.pl priority-ecosystem-check \
  make -C research/experiments/container-representation/priority-library ecosystem-check
perl .github/run-check.pl priority-ecosystem-account \
  make -C research/experiments/container-representation/priority-library ecosystem-account
perl .github/run-check.pl priority-ecosystem-measure \
  make -C research/experiments/container-representation/priority-library ecosystem-measure
```

Supply `WHITEFOOTC` to select a frozen compiler. Construction, checks,
allocation observations and timing are separate commands; run both checks
before timing. `ecosystem-check` checks the normal and accounting images
against the existing independent sorted-sequence oracle. It also requires a
deliberately corrupted checksum and a simulated unreleased allocation to exit
with the corresponding diagnostic. These two commands never enter a sample.

The practical queue ranking compares Whitefoot with Rust
`BinaryHeap<Reverse<T>>` and C++ `std::vector<T>` using only standard
`make_heap`, `push_heap` and `pop_heap`. Removed and replaced owners are
returned and consumed. `std::priority_queue` cannot expose that move-only
ownership outcome through its const `top()` and void `pop()`. C++ replacement
therefore performs two standard heap repairs; Rust uses `peek_mut` and its
ordinary repair on guard release. This is an API and algorithm difference,
not a separate language cost. The existing swap and hole C heaps are labelled
`c-control` and remain attribution controls.

Each queue has the shared logical ceiling of 4096, with full-capacity refusal
returning the offered value for retry. Native containers choose their ordinary
capacity, growth and allocation behavior. The four queue paths are reserved
pop/push, reserved replacement, growing fill/pop, and heapify/pop. They include
construction, consumption and cleanup. No trace retains element references,
requires stable addresses, or observes equal-priority stability. A borrowed
minimum key contributes once to each reserved trace's checksum. Every word of
every consumed wide record contributes to the sequence-dependent checksum.
The 256-byte payload is noncopy inline storage with no per-element allocation;
these timings do not establish nested-owner costs.

The fifth path is labelled `storage-control` for every implementation and is
outside the queue ranking. It fills a raw prefix and consumes reverse physical
slots without building a heap. The old experiment constructed a queue from
that prefix solely to invoke its physical cleanup. The current module makes
the storage field read-only to callers, so this path now performs the same
`take_back`/consume/`free_empty` loop directly on the raw `Box<Slots<T>>`.
It preserves allocation, reverse consumption and the absence of heap
comparisons without changing the production library or specification.

Practical builds use Clang `-O3` and Rust `-C opt-level=3`; only complete traces
cross language boundaries, and no public queue operation or callback is
forced out of line. The timed WF IR keeps ordinary `malloc` and `free`, native
C uses those calls directly, and Rust/C++ retain ordinary default allocators.
The separate `ACCOUNT_ONLY` image redirects WF/C allocation and enables the
shared native observers. `accounting.csv` reports one complete trace per cell,
including requests, reallocations, deallocations, total requested bytes and
peak live requested bytes. Rust `System::realloc` stays a realloc operation:
its logical live-request peak and the possible old-plus-new overlap upper
bound are separate columns. Neither column is RSS or allocator-resident
memory. All observed live bytes must return to zero.

Timing uses lengths 16, 256 and 4096, 8- and 256-byte payloads, seven sample
seeds 101 through 107, one full warmup per implementation and cell, rotating
implementation order, and a second cohort with that order reversed.
`ECO_WORK` scales the original 16384-scalar/4096-wide work target, defaults to
16, and accepts 1 through 64 for bounded follow-up runs. Reserved traces use
that work as churn rounds; the other paths repeat complete traces, advancing
the seed between repetitions. The `work_multiplier`, `rounds`, `traces` and
`seed` columns make that denominator explicit. For per-item normalization use
`count * (rounds == 0 ? traces : rounds)`, including the full setup and cleanup
cost in the numerator. No setup subtraction estimates an isolated operation.

Default outputs are `.build/ecosystem/measurements.csv` and
`.build/ecosystem/accounting.csv`; `ECO_SAMPLE_FILE` and `ECO_ACCOUNT` override them.
`configuration.txt` records compiler identities and construction flags beside
those outputs. Retain every sample when reporting per-cell cohort medians;
short or unstable cells need a longer bounded run before a ranking claim.

### Recorded practical execution

The 2026-09-26 run used source revision
`0c3203aa6111f14247aa950e3794e83082d4f29c`, the frozen current compiler and
toolchain identities in [ECOSYSTEM.md](../ECOSYSTEM.md), and `ECO_WORK=16`.
The recorded family build took 3.876 s, correctness execution 1.486 s,
allocation execution 0.155 s and the two-cohort measurement phase 15.036 s.
An immediately preceding up-to-date construction check took 0.130 s; it is
not part of container execution time. The timing intervals sum to 10.919 s;
the phase also includes warmup, the independent oracle and output.

Both timed and accounting images passed 3,600 complete scalar/wide traces
and eight native refusal/retry chains. The deliberately corrupted checksum
and simulated unreleased allocation each produced the expected nonzero
failure. The current legacy normal/retained checks also passed; their
different optimization and allocator conditions stay outside this ranking.
The preserved practical files are:

| Artifact | Rows | SHA-256 |
| --- | ---: | --- |
| [ecosystem-samples.csv](ecosystem-samples.csv) | 2,100 | `8ef7d3e0380b2631ab9db3e784cd08c5c064137a2c3fad3f17d86a8dd51f5b5c` |
| [ecosystem-replay-samples.csv](ecosystem-replay-samples.csv) | 2,100 | `537ffa3be6240ebbf02d29b8c567c7fb95686dfb39b7e153422fc65ef7fc6a90` |
| [ecosystem-accounting.csv](ecosystem-accounting.csv) | 150 | `ed77d5bf20c8c3c0a120b052de682e7e49d5692705b1933707ff8dd16cc667fc` |

In each timing series, all 300 groups contain all seven seeds. The exact
checksums agree across implementations and cohorts for all 210 workload/seed
pairs. The accounting checksums agree across all five implementations for each of
its 30 workloads. Every accounted live-byte total returns to zero, and each
allocation request is matched by a deallocation or successful reallocation.

At work multiplier 16, 339 sample rows are below 1 ms. They occur in 50 of
the 300 implementation/cohort groups: scalar replacement and scalar raw
cleanup. A paired comparison is short when either participant has such a
sample; this marks 12 practical queue comparison groups, all scalar
replacement. None of the cohort median ratios differs by more than 10%:
the largest practical discrepancy is 8.755%, and the largest including C
controls is 8.878%. This does not erase individual outliers: 19 groups have
maximum/minimum above 1.2, and the scalar n=4096 replacement swap-C samples
in cohort 1 range from 0.887 ms to 42.339 ms. The corresponding WF group
ranges from 0.956 ms to 2.886 ms. All samples remain in the CSV.

The same-source `ECO_WORK=64` replay took 56.063 s, with 42.544 s in its timing
intervals. All 2,100 sample rows exceed 1 ms; the minimum is 1.166 ms. None
of its 120 WF/comparator pairs has a cohort ratio discrepancy above 10%;
the maximum is 6.604%. Fourteen implementation/cohort groups still have
maximum/minimum above 1.2. Their samples remain preserved, including the
wide n=4096 Rust pop/push group with a 32.380 ms minimum, 34.032 ms median
and 70.091 ms maximum. The two-cohort medians below use this longer series.

The replay increases the scalar denominator to 1,048,576 items and the wide
denominator to 262,144 items. It is an extended-work series because longer
reserved churn changes setup amortization and the length of the evolving
input stream. At n=4096, WF replacement changes from 3.792–3.868 to
1.920–1.925 ns/item for scalars and from 78.674–79.102 to 52.505–53.253 for
wide values without changing any implementation. The two work levels are
not pooled or described as an optimization speedup. Reproduce the replay
with `ecosystem-measure ECO_WORK=64` and a separate `ECO_SAMPLE_FILE`.

### Practical timing results

These are the minimum and maximum of the two cohort medians at n=4096,
using work multiplier 64, not confidence intervals. Ratios divide medians
within the same cohort; values above one mean WF took longer. The two C
columns are algorithm and implementation controls. The raw-storage path is
excluded from this table.

| Bytes | Queue trace | WF ns/item | WF / Rust | WF / C++ | WF / swap C | WF / hole C |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | pop-push | 49.404–49.528 | 0.833–0.838 | 1.147–1.151 | 1.039–1.043 | 1.060–1.063 |
| 8 | replace-top | 1.920–1.925 | 0.861–0.882 | 0.046–0.046 | 1.045–1.053 | 0.852–0.854 |
| 8 | grow-pop | 33.027–33.503 | 1.099–1.118 | 0.937–0.948 | 1.033–1.045 | 1.058–1.075 |
| 8 | heapify-pop | 25.017–25.125 | 0.979–0.982 | 0.844–0.880 | 1.073–1.080 | 1.091–1.094 |
| 256 | pop-push | 293.694–300.560 | 2.270–2.315 | 2.162–2.225 | 1.070–1.096 | 1.827–1.882 |
| 256 | replace-top | 52.505–53.253 | 1.330–1.371 | 0.401–0.402 | 1.240–1.271 | 1.314–1.316 |
| 256 | grow-pop | 174.824–175.732 | 1.445–1.497 | 1.684–1.688 | 1.070–1.083 | 1.377–1.420 |
| 256 | heapify-pop | 151.886–154.160 | 1.318–1.319 | 1.605–1.620 | 0.993–1.010 | 1.518–1.538 |

Wide pop/push is the clearest repeated follow-up: at n=16 WF/Rust is
2.022–2.045 and WF/C++ is 1.686–1.692; at n=256 those ratios are
2.257–2.303 and 2.061–2.080. Wide growing fill/pop at n=256 is
1.436–1.439 times Rust and 1.777–1.783 times C++. Wide heapify/pop at n=16
is faster than Rust (0.919–0.920) and slower than C++ (1.212–1.236),
so the large-population result is not a uniform library ranking.

Scalar pop/push costs 1.141–1.167 times C++ at n=16 and 1.216–1.221 at
n=256, but its Rust ratios vary with population. Scalar growing fill/pop
is 1.142 times Rust in both cohorts at n=16 and 0.975–0.983 at n=256. Scalar
heapify/pop at n=256 is 0.837–0.838 times Rust and 0.879–0.880 times C++.
Scalar replacement at n=16 is 0.837–0.852 times Rust and at n=256 is
0.828–0.830; the longer replay resolves the initial short-sample concern
under its stated churn duration.

The separate wide raw-storage control at n=4096 is 1.099–1.104 times Rust
and 0.956–0.964 times C++. Scalar raw storage at n=16 is 1.246–1.290 times
Rust and 0.848–0.875 times C++; at n=4096 its two native ratios span
0.994–1.057. These are complete construction/physical-cleanup observations, not
amounts to subtract from a heap trace or entries in the queue ranking.

### Allocation results and bounded attribution

At n=4096, reserved WF traces make two requests: 32,800 scalar or 1,048,608
wide requested bytes and the same peak. Rust/C++ make one request for the
payload alone, 32,768 or 1,048,576 bytes. WF heapify/pop and raw cleanup each
make one request containing a 16-byte header; native equivalents request
only the payload. The C controls match every WF count, byte total and peak.
Growing fill/pop exposes the larger allocation-policy difference:

| Bytes | Implementation | Requests | Reallocations | Requested bytes | Logical peak bytes | Realloc overlap upper bound |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | WF | 14 | 0 | 65,752 | 49,184 | 49,184 |
| 8 | Rust | 11 | 10 | 65,504 | 32,768 | 49,152 |
| 8 | C++ | 13 | 0 | 65,528 | 49,152 | 49,152 |
| 256 | WF | 14 | 0 | 2,097,120 | 1,572,896 | 1,572,896 |
| 256 | Rust | 11 | 10 | 2,096,128 | 1,048,576 | 1,572,864 |
| 256 | C++ | 13 | 0 | 2,096,896 | 1,572,864 | 1,572,864 |

WF and the C controls explicitly allocate a replacement, move the initialized
prefix and free the old backing. Rust's allocator performs its ordinary
reallocations; the possible transient old-plus-new footprint is unobserved
and has only the stated upper bound. These allocation differences are
measured, but their share of elapsed time is not isolated.

The same-source swap/hole C pair keeps storage, growth, comparison and trace
conditions aligned while changing sifting movement. WF's wide pop/push is
1.070–1.132 times swap C across the tested populations and 1.579–1.882 times
hole C; that makes the sift
algorithm a useful next discriminator without assigning a causal percentage
to whole-slot movement. The optimized inspection below identifies a source
candidate; a measured WF comparison remains necessary before selection.

Replacement has another source-visible distinction: WF performs one downward
sift, Rust repairs through `peek_mut`, and C++ uses `pop_heap` followed by
`push_heap`. This explains why the comparison includes different algorithms;
it does not assign the timing ratio to a language. Wide replacement remains
slower than Rust and both C controls, including 1.264–1.277 times Rust at
n=256. The inspection below checks the current optimized aggregate transfers
and surviving public/callback boundaries with unchanged algorithms.
The practical `.ll` files are pre-O3 compiler outputs, so their visible
copies and calls do not establish what survives optimization. Historical
retained O2 observations have different visibility and allocator conditions
and are not causal percentages of the practical O3 gaps.

### Optimized baseline and unselected source candidates

Read-only `nm -n` and `otool -tvV` inspection used the measured
`.build/ecosystem/priority-timed`, SHA-256
`880540053affdb13787f4e832ec048e97ea4fbdd6d1d48c5e1b7e1012fcb3bf0`.
Its source baseline is `0c3203aa6111f14247aa950e3794e83082d4f29c`;
priority production and benchmark sources are unchanged through
`c75520e9d59d74e19ba158e1cae5f394b3a2d874`. These are final arm64 O3
instructions under the recorded Apple Clang 21.0.0 and rustc 1.98.1 toolchains,
not the pre-O3 `.ll` files. Addresses below identify this binary only.

The wide WF entry `_wf_priority_cost_record_trace` at `0x100009c38`
branches to `_wf_priority_cost_trace$instance$4dfceafcd9f4259b` at
`0x10000a3a4`. Its hot pop/push loop has no surviving pop, push, sift,
comparator or no-op position-reporter call. The pop sink still performs
three complete 256-byte transfers per exchange at
`0x10000b490`--`0x10000b590`; the push rise repeats that pattern from
`0x10000ba20`. These implement the slot swaps at lines 86 and 156 of
[priority-queue.wf](../../../../lib/std/collections/priority_queue/priority-queue.wf).
Comparisons load keys directly. `make_room` calls remain at `0x10000a930`
and `0x10000b984`, along with setup, final drain and cleanup calls.

Native inlining is not uniform. `_priority_rust_record_trace` at
`0x100012534` retains a call at `0x1000130dc` to the
`BinaryHeap<Reverse<Record>>::pop` specialization at `0x100011f98`.
`_priority_cpp_record_trace` at `0x10000e240` has inlined heap algorithms
but retains vector growth calls. The C controls `_record_swap_trace`
and `_record_hole_trace`, at `0x100004b08` and `0x100006718`, retain
their push helpers. This evidence does not support a blanket forced-inlining
change. The C movement discriminator remains the otherwise matched rise/sink
pair in [priority-costs.c](priority-costs.c), lines 200 and 227.

Two source hypotheses are **unimplemented, unmeasured and unselected**:

- **Delayed exchange through a local owner.** For pop/replacement, find the
  destination by read-only comparisons against the incoming owner, then
  walk destination to root, exchanging each slot with that local owner and
  returning the final displaced root. All places remain initialized. A local
  carry might stay in registers instead of repeatedly staging a slot swap;
  spills or the second traversal could remove the benefit. Push would need
  the corresponding forward rotation along its ancestor path before final
  append, with separately justified path storage or arithmetic.
- **Shared four-ary, then eight-ary sifting.** Fewer levels trade full-owner
  exchanges for additional child comparisons. Prove bounded child scans and
  progress, including zero-byte elements and maximal ceilings; for `count > 1`,
  `(count - 2) / D + 1` avoids overflow in the parent count. A changed fanout
  would revise the recorded binary-heap choice, not establish native parity
  by itself.

The [shared-core decision](../../../../design/language/data-model/priority-queue-storage.md)
constrains both candidates. Preserve plain/indexed source sharing and the
published placement-reporting protocol; delayed reporting is observable and
cannot silently replace initial/both-position reports. Every candidate must
retain unique owners, refusal/retry, unchanged length/capacity guarantees and
bounded progress independently of comparator consistency. A direct C-hole
transcription is inadmissible under WIN-3 and OP-11; it is not a selected WF
implementation or a reason to weaken ownership.

The direct reverse-carrier rotation fails that reporting constraint. Replacing
the root of the min-heap `[1, 3, 2, 7, 5, 6, 4]` by `9` reports `(value, index)`
as `(9, 0), (2, 0), (9, 2), (4, 2), (9, 6)` through indexed replacement and the
current shared sink. Rotating destination-to-root writes positions 6, 2, 0 and
naturally reports `(9, 6), (4, 2), (2, 0)`. The final heap agrees but an ordered
reporter distinguishes them. Replaying the old stream afterward would report
`9` at position 2 while it resides at 6, and would still change chronology.
Keeping the original forward resident swaps restores the protocol and the
movement being targeted. Heapify and arbitrary indexed repair also lack the
incoming owner that pop/replacement can carry outside the initialized prefix.
No direct delayed-carrier variant is admitted or constructed from this idea.

The discriminator changes one source algorithm under the frozen compiler,
adapters, allocator policy and workload. First require the independent sorted
oracle, complete wide-owner/refusal checks, nested-owner consumption, indexed
membership/placement checks and balanced accounting. Then inspect final O3
movement and measure the complete scalar/wide matrix in both cohorts,
retaining the C controls, both work settings and every sample. Failure to
reduce the predicted transfers, improvement lost to variation, or a required
cell still missing the stated performance target rejects the candidate as a
solution. Raw physical cleanup remains outside queue rankings. No candidate
build, timing, production change or tree revision accompanies this inspection.

### Source-only diagnostic: duplicate sink after pop

The current `priority_queue_pop` body exchanges the root with the last
element, then calls `priority_queue_sift_sink` twice with the same `start`,
`count` and comparator. The second call observes the heap after the first call
has restored its invariant and should therefore return at the root; it adds no
semantic work and is absent from the matched C controls. The candidate removes
only that duplicate call. Before adoption it must pass the independent sorted
oracle, all wide-owner/refusal and cleanup checks, and the allocator ledger.
The optimized pop/push, grow/pop and heapify/pop paths must show one fewer
sift invocation and no new transfer or call boundary. The complete 30-cell
matrix at both recorded work settings must be measured with the same seeds and
native controls; no useful cell may regress and every newly eligible cell must
beat the slower standard peer. This is a source correction under the current
algorithm, not a new API or specification choice.

#### Audit correction

The preregistration above was based on a stale or misread source fragment. The
current branch's `priority_queue_pop` has exactly one
`priority_queue_sift_sink` call at `lib/std/collections/priority_queue/priority-queue.wf:230`; the neighboring call at line 242 belongs to
`priority_queue_replace_top`. A fresh `rg` and `git show HEAD:` inspection found
no duplicate pop call. No source candidate, compiler build, timing or verdict
change was made. The preregistration is retained only as an audit trail and
must not be treated as evidence of a PriorityQueue defect.

## Historical matched C comparison

Run from the repository root, with a built compiler or `WHITEFOOTC` override:

```sh
perl .github/run-check.pl priority-native \
  make -C research/experiments/container-representation/priority-library native-check
perl .github/run-check.pl priority-build \
  make -C research/experiments/container-representation/priority-library build
perl .github/run-check.pl priority-check \
  make -C research/experiments/container-representation/priority-library check events
perl .github/run-check.pl priority-measure \
  make -C research/experiments/container-representation/priority-library measure summarize
```

Construction and execution are measured separately. The initial construction
budget and the complete timing budget are each 60 seconds; an overrun needs
investigation before extending either. Native C checks precede cost-source WF admission
and all correctness checks precede timings. The functional caller under
`tests/programs/containers` independently covers nested Box release identities;
the timed 256-byte `nodrop` record has no per-element allocation.

The timing matrix contains 30 cells: 8- and 256-byte elements, lengths 16,
256 and 4096, and five complete traces. Reserved pop/push and replace-top
include fill, one borrowed peek, repeated operations, ordered drain and final
release. Growing fill/pop includes the zero-capacity allocation, geometric
growth and ordered drain. Heapify/pop starts with a raw initialized prefix,
builds bottom-up, and drains in priority order. Setup/cleanup fills the raw
prefix and consumes reverse physical slots without heap construction. That
last path isolates construction and cleanup as a complete trace, without
subtracting it from the other paths.

Each cell has ordinary optimization and retained public queue operations
(`new`, `len`, `reserve`, `push`, `peek`, `pop`, `replace_top`, `heapify`,
`drain`, `free`) and element callbacks. Private child-selection, room-making
and sift helpers remain ordinarily optimizable in both languages. The check
prints their actual surviving call sites; no extra private boundary is forced.
A normal/retained difference does not isolate call latency. WF uses full-slot
swaps. `swap-c` follows
that source algorithm; `hole-c` carries one pending value through a private
sift hole. The latter is an algorithmic control, not an attribution of its
advantage to the language. A common accounting allocator includes all timed
allocations, and both native layouts match WF's actual 16-byte Slots header,
8- or 256-byte element stride, capacity and old/new overlap during growth.
The allocator's private tracking header is identical across variants and is
excluded from reported requested backing bytes.

The input is a full-width deterministic LCG, ordered by its unsigned word
(the wide record's first word). Every record word is consumed into a
sequence-dependent checksum. The independent oracle sorts a sequence and
updates it by ordered insertion, without heap operations. It checks every
timed row as well as the zero, singleton, irregular and large correctness
matrix. The seven sample seeds are 101 through 107. Within a cohort the
variant order rotates; the second cohort reverses it and reverses normal
and retained mode order. Work targets 16,384 scalar or 4,096 wide items per
sample. Reserved paths amortize setup with that many churn operations; the
other paths repeat complete traces. The reported ns/item denominator does
not subtract setup, final drain or cleanup from reserved paths.

`events.csv` counts comparisons and C element assignments in a separate
instrumented native build, with one churn round, seed 101 and the same
population matrix. Sift exchanges count three assignments, pending-value
loads/final stores each count one, backing growth counts the live prefix,
and explicit entry/removal assignments count one. Payload construction,
consumption and native argument/result ABI copies are outside that counter.
These are algorithm-level counts, not optimized machine transfer counts or
timing instrumentation.
Emitted-code inspection must accompany any claim about actual generated
loads, stores or aggregate copies.

## Recorded execution

The 2026-09-23 run used the compiler built from main
`345e2966a45c995d6cebbb7f6b128a66235cd20f` (v0.68), on arm64
Darwin 25.6.0 with Apple Clang 21.0.0 (`clang-2100.3.34.2`), at `-O2`.
This compiler includes no PR #101 inactive-storage change. The maintained library and
this experiment were uncommitted work atop `26ae33c12e` when measured;
the hashes below identify the exact measured inputs.

| Input | SHA-256 |
| --- | --- |
| Frozen compiler executable | `cbffd4dd1ae8641ef03790457181188988bf70cc4af1a53c50c1f406307bb7f9` |
| `lib/containers/priority-queue.wf` | `8a9f7a529ec61db4b888ce866f4b687b35f8226ccce94de668ee68f35cee08fe` |
| `priority-library.wf` | `f26317a241e7cadfa7e12a6dc2156468ebb5a97e9a05341dc57548db39d0a96d` |
| `priority-costs.c` | `323e4bb4ff4b59bf0996a3c9b19974b3f5f1b461d917bc570a6198b97ad9929b` |
| `Makefile` | `cde676f0c6b54fa8308f047039f26480f3a855607a27d6be537f4669a56516a5` |

Initial native C construction took 0.57 seconds and its independent check
1.16 seconds. WF emission, optimization, linking, runtime construction and
the event-counter binary took 2.56 seconds. The following complete
check/measurement/count/summary phase took 4.09 seconds; its timed intervals
sum to 1.166 seconds, with oracle computation and correctness execution outside
those intervals. The combined successful WF construction and execution guard
took 6.67 seconds. No compiler rebuild or budget extension was needed.

Each native-only mode passed 1,440 full scalar/owning trace executions and
four full-capacity refusal/retry chains. Each linked WF/C mode passed 2,160
full traces against the independent oracle and allocation formulas, plus
the same four native refusal/retry chains. All 2,520 measured rows passed
their checksum and complete-cleanup checks. Retained IR contains the requested
public-operation and callback calls; no calls to private child, room-making
or sift helpers survive in either retained module.

The retained [measurements.csv](measurements.csv) has SHA-256
`006234d7fd866263088b1c7c5fee40f2ef80520f53468ffe537a96e0cb3fe62f`.
The 60 algorithm-count rows in [events.csv](events.csv) have SHA-256
`5511b9318e760b9a37b5010642cf31aae1d2077686f8490498f98fed424500f0`.
Regenerate grouped medians from the retained data with:

```sh
make -C research/experiments/container-representation/priority-library \
  .build summarize SAMPLES=measurements.csv
```

## Timing results

Each range below is the minimum and maximum of the **two cohort medians**,
not a confidence interval or the minimum/maximum sample. Ratios divide
medians within the same cohort. Raw samples preserve each paired seed and
order. Observed elapsed values have one-microsecond granularity; the shortest
scalar replacement samples last only 21 microseconds. Small differences at
that scale are not an optimization result.

Normal optimization:

| Bytes | n | Trace | WF ns/item | WF / swap C | WF / hole C |
| ---: | ---: | --- | ---: | ---: | ---: |
| 8 | 16 | pop-push | 13.062–13.916 | 1.005–1.022 | 1.024–1.036 |
| 8 | 256 | pop-push | 32.227–33.997 | 1.160–1.206 | 1.107–1.114 |
| 8 | 4096 | pop-push | 48.340–49.255 | 0.990–0.993 | 0.976–0.981 |
| 8 | 16 | replace-top | 1.282 | 0.955–1.000 | 0.913 |
| 8 | 256 | replace-top | 3.052 | 1.020 | 1.000 |
| 8 | 4096 | replace-top | 21.240–21.301 | 1.064–1.080 | 1.048–1.087 |
| 8 | 16 | grow-pop | 28.564–28.625 | 0.963–0.969 | 0.959–0.967 |
| 8 | 256 | grow-pop | 24.170 | 0.900–0.902 | 0.892–0.896 |
| 8 | 4096 | grow-pop | 30.029–31.006 | 0.952–0.957 | 0.982–0.983 |
| 8 | 16 | heapify-pop | 14.832–15.686 | 1.090–1.098 | 1.075–1.094 |
| 8 | 256 | heapify-pop | 17.883–19.592 | 0.973–1.016 | 0.948–1.000 |
| 8 | 4096 | heapify-pop | 24.719–26.184 | 1.071–1.097 | 1.083–1.135 |
| 8 | 16 | setup-cleanup | 2.380–2.441 | 0.975–0.976 | 1.000 |
| 8 | 256 | setup-cleanup | 1.953–2.014 | 1.000–1.032 | 1.000–1.031 |
| 8 | 4096 | setup-cleanup | 1.953–2.014 | 1.000–1.032 | 1.000–1.032 |
| 256 | 16 | pop-push | 119.141–122.070 | 1.104–1.109 | 1.471–1.511 |
| 256 | 256 | pop-push | 194.824–201.660 | 1.052–1.063 | 1.535–1.553 |
| 256 | 4096 | pop-push | 369.141–381.592 | 0.956–0.978 | 1.276–1.302 |
| 256 | 16 | replace-top | 42.480–43.701 | 1.279–1.289 | 1.288–1.289 |
| 256 | 256 | replace-top | 59.815–61.768 | 1.178–1.193 | 1.416–1.454 |
| 256 | 4096 | replace-top | 289.795–290.771 | 0.975 | 1.369 |
| 256 | 16 | grow-pop | 80.811–82.275 | 1.009–1.012 | 1.094–1.107 |
| 256 | 256 | grow-pop | 117.432–122.559 | 0.916–0.918 | 1.243–1.272 |
| 256 | 4096 | grow-pop | 169.189–177.734 | 0.870–0.880 | 1.197–1.212 |
| 256 | 16 | heapify-pop | 57.861–62.256 | 0.988–0.992 | 1.179–1.192 |
| 256 | 256 | heapify-pop | 96.924–100.586 | 0.896–0.907 | 1.293–1.338 |
| 256 | 4096 | heapify-pop | 150.391 | 0.851–0.853 | 1.210–1.215 |
| 256 | 16 | setup-cleanup | 34.668–34.912 | 0.973–1.007 | 1.000 |
| 256 | 256 | setup-cleanup | 34.424–34.668 | 0.986–0.993 | 0.986–0.993 |
| 256 | 4096 | setup-cleanup | 35.156–35.400 | 0.993–1.000 | 0.986–0.993 |

Retained public operations and callbacks:

| Bytes | n | Trace | WF ns/item | WF / swap C | WF / hole C |
| ---: | ---: | --- | ---: | ---: | ---: |
| 8 | 16 | pop-push | 23.498–24.353 | 1.510–1.511 | 1.510–1.517 |
| 8 | 256 | pop-push | 48.401–51.819 | 1.673–1.722 | 1.684–1.726 |
| 8 | 4096 | pop-push | 82.703–83.069 | 1.162–1.177 | 1.182–1.183 |
| 8 | 16 | replace-top | 4.639–4.700 | 0.884–0.906 | 0.854–0.875 |
| 8 | 256 | replace-top | 6.958–7.019 | 0.826–0.833 | 0.797–0.799 |
| 8 | 4096 | replace-top | 38.696–38.818 | 0.795–0.801 | 0.796–0.797 |
| 8 | 16 | grow-pop | 33.142–33.997 | 0.993–0.995 | 0.974–0.978 |
| 8 | 256 | grow-pop | 40.833–42.114 | 0.813–0.816 | 0.818–0.821 |
| 8 | 4096 | grow-pop | 56.885–56.946 | 0.786–0.802 | 0.800–0.802 |
| 8 | 16 | heapify-pop | 19.226–20.569 | 0.903–0.918 | 0.890–0.908 |
| 8 | 256 | heapify-pop | 31.738–34.119 | 0.725–0.728 | 0.728–0.732 |
| 8 | 4096 | heapify-pop | 47.546–50.659 | 0.729–0.739 | 0.731 |
| 8 | 16 | setup-cleanup | 3.113–3.174 | 0.962–0.963 | 0.962–0.963 |
| 8 | 256 | setup-cleanup | 3.540–3.601 | 0.983–1.000 | 0.983–1.000 |
| 8 | 4096 | setup-cleanup | 3.601 | 0.983 | 0.983–1.000 |
| 256 | 16 | pop-push | 125.732–126.465 | 1.082–1.086 | 1.585–1.589 |
| 256 | 256 | pop-push | 204.102–205.566 | 1.005–1.033 | 1.701–1.738 |
| 256 | 4096 | pop-push | 412.109–413.574 | 0.905–0.908 | 1.242–1.255 |
| 256 | 16 | replace-top | 38.574–38.818 | 0.749–0.768 | 0.648–0.657 |
| 256 | 256 | replace-top | 57.617–59.815 | 0.784–0.785 | 0.749–0.754 |
| 256 | 4096 | replace-top | 306.152–318.848 | 0.812–0.839 | 1.020–1.027 |
| 256 | 16 | grow-pop | 100.098–100.830 | 1.010–1.017 | 1.043–1.076 |
| 256 | 256 | grow-pop | 132.324–136.719 | 0.908–0.911 | 1.085–1.088 |
| 256 | 4096 | grow-pop | 186.035–186.279 | 0.867–0.870 | 1.092 |
| 256 | 16 | heapify-pop | 71.289–74.219 | 0.921–0.924 | 0.987–1.010 |
| 256 | 256 | heapify-pop | 110.107–115.234 | 0.845–0.855 | 1.044–1.051 |
| 256 | 4096 | heapify-pop | 165.283–165.771 | 0.813–0.817 | 1.041–1.051 |
| 256 | 16 | setup-cleanup | 40.527–41.748 | 0.988 | 0.988 |
| 256 | 256 | setup-cleanup | 40.527–41.504 | 1.000–1.006 | 1.006 |
| 256 | 4096 | setup-cleanup | 41.016–41.260 | 0.994–1.000 | 0.988–1.000 |

## Storage, comparisons and transfers

Every variant has the same actual backing layout: 16 header bytes and an
8- or 256-byte slot, without a per-slot tag. The successful/refused push result
occupies 16 bytes for a word and 264 for the record in both implementations;
its initialization and calling convention differ below. At n=4096, the
per-complete-trace backing accounting is:

| Trace | Requests | Scalar requested bytes | Scalar peak bytes | Wide requested bytes | Wide peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Reserved churn or replacement | 2 | 32,800 | 32,800 | 1,048,608 | 1,048,608 |
| Growing fill/pop | 14 | 65,752 | 49,184 | 2,097,120 | 1,572,896 |
| Heapify/pop or raw setup/cleanup | 1 | 32,784 | 32,784 | 1,048,592 | 1,048,592 |

Requested bytes sum all allocations; peak bytes include the zero-capacity
header at the first replacement and simultaneous backings during later growth.
All live bytes return to zero. Repeated samples multiply requests and total
requested bytes by the trace count, not the peak.

For seed 101 and n=4096, the counted native algorithms make identical
comparisons, while hole sifting reduces element assignments on the heap paths:

| Complete trace | Comparisons, either C | Swap element assignments | Hole element assignments |
| --- | ---: | ---: | ---: |
| Pop/push, one churn round | 195,042 | 350,278 | 176,830 |
| Replace top, one churn round | 148,521 | 247,226 | 128,826 |
| Growing fill/pop | 87,325 | 155,474 | 84,588 |
| Heapify/pop | 85,670 | 144,677 | 74,163 |
| Raw setup/cleanup | 0 | 8,192 | 8,192 |

Counts are the same for both payload widths. Multiplication by the actual
stride gives the explicit assignment bytes in `events.csv`; it does not
include argument/result copies or imply that each source assignment becomes
one memory copy. The whole-chain counts do not separately measure heapify's
complexity; its bottom-up construction argument belongs to the library and
investigation.

Optimized LLVM IR establishes these concrete differences:

- A wide WF sift exchange contains a 256-byte `memcpy` to its temporary,
  a 256-byte `memmove` between slots, and a 256-byte `memcpy` back. The
  swap C loop contains three 256-byte `memcpy` operations. The hole C loop
  contains one 256-byte copy per advancing step, plus pending-value setup
  and final placement. Thus fewer comparisons do not explain the wide
  hole-control advantage; its different movement is directly visible.
- Retained WF word push returns through an output pointer and clears the
  16-byte successful `Result`; C returns two i64 registers and does not
  clear the inactive payload. Retained wide WF push first copies its
  256-byte argument and clears the 264-byte successful result. The matched
  C push writes the tag and leaves the inactive payload alone. These are
  present costs, not a measured allocation of the timing gap to each cause.
- Wide WF pop and replacement retain 256-byte result copies, and the callers
  retain additional aggregate transfers at consuming boundaries. The C
  compiler supplies its ordinary aggregate ABI and optimization. Neither
  the declaration-level ownership mode nor the C assignment counter alone
  predicts these emitted transfers.
- Normal optimization chooses different surviving public boundaries:
  WF scalar traces inline the public queue calls, while C scalar traces
  retain two push call sites. The WF wide trace retains two drain and one
  free call site; C wide retains two push, two drain and two free call sites.
  The fully retained variant preserves public calls in both. These facts
  preclude interpreting the normal/retained delta as isolated call latency.

These observations can be replayed in `.build/priority-library-*.opt.ll`
and `.build/control-*.opt.ll`, generated by `make build`. They describe
optimized IR, not dynamic machine-instruction counts or a causal ablation.

## Interpretation and remaining work

The full arbitrary-owner chain is executable with the selected boxed prefix.
At n=4096, normal scalar pop/push and growth are comparable to or faster than
both native controls in this cohort pair; scalar heapify/pop and replacement
retain smaller gaps. Wide WF is faster than the source-shaped swap control
on all four n=4096 heap paths, yet remains 1.197–1.369 times the hole control.
The matched comparison and transfer counts identify a real algorithmic
movement tradeoff without selecting a new storage mechanism.

There is no uniform native-parity result. Retained scalar pop/push is
1.510–1.722 times swap C at n=16/256 and 1.162–1.177 at n=4096, in the same
direction in both cohorts. The output-pointer/Result-clear difference is a
specific next discriminator, not a proven complete explanation. Wide
pop/push remains 1.585–1.738 times hole C in the retained small/medium cells,
where both algorithmic movement and aggregate boundaries matter. Normal
wide replacement at n=16/256 costs 1.178–1.289 times swap C; retained
replacement reverses that direction. The contribution of inlining, aggregate
traffic and final code placement to that reversal is unresolved.

No optimization is selected from these measurements. Reopen the retained
scalar push boundary with unchanged-source compiler variants and unchanged
C controls, retaining these chains and both cohorts; require repeated
improvement beyond control variation and inspect the result stores/calling
convention. Reopen normal small/medium wide replacement with a bounded
comparison of its surviving calls and emitted transfers before claiming
that a source or lowering change fixes it. The raw setup/cleanup results
remain near the controls; they do not cancel the operation-path gaps.
The [maintained compiler TODO](../../../../docs/todo.md) owns follow-up
validation of these unresolved costs.

### Preregistered source discriminator: four-ary sift (scratch only)

The wide gap is dominated by full-owner exchanges, while the selected design
already records a binary heap as the reusable baseline. Before any source
change, a scratch copy will change only the private parent/child arithmetic:
each node has up to four children, and `priority_queue_child` compares them in
order before the existing `swap` and position-reporting calls. The queue's
public signatures, boxed prefix, ownership/refusal protocol, comparator
interface, indexed reports and cleanup remain unchanged. The candidate is not
production code and must not revise the binary-heap decision without owner
selection.

The code screen is an independent sorted oracle, all wide-owner/refusal and
nested-owner checks, exact allocation/release accounting, and unchanged
retained helper calls. The candidate must have no new transfer operation,
unchecked arithmetic, or extra result boundary. The timing screen is the full
existing PriorityQueue matrix (scalar and 256-byte values, both cohorts and
the same native swap/hole/Rust/C++ controls); it must reduce wide pop/push and
replacement movement without a useful scalar, growth, heapify or cleanup
regression. A failed proof, an overflow-sensitive bound, an extra comparison
cost that loses the target cells, or a residual gap to the slower standard
peer rejects the candidate. Any successful candidate remains a measured
algorithm alternative until the owner revisits the binary-heap decision.

### Four-ary sift result: rejected

The scratch source was measured after the preregistration above and then
restored; no production source or compiler code changed. It passed the same
`make check`-equivalent PriorityQueue build and execution screen: 1,440 native
trace executions, 2,160 scalar/owning WF executions in each image, four
refusal chains per image, and the complete ecosystem screen of 3,600 traces
plus eight refusal chains. Checksums and release ledgers agreed. The preserved
candidate files are [ecosystem-fourary-replay-samples.csv](ecosystem-fourary-replay-samples.csv)
and [ecosystem-fourary-accounting.csv](ecosystem-fourary-accounting.csv); their
SHA-256 values are respectively
`07ce82eb26a3050f0858c5a4cbb6921c7cd903da0b072293c22652dc322b39a6` and
`ed77d5bf20c8c3c0a120b052de682e7e49d5692705b1933707ff8dd16cc667fc`.

The target reducer classified 12 pass, 10 deficit and two inconclusive useful
cells (six setup/cleanup controls remain unranked). The candidate did not
remove the wide movement deficit: pop/push at 16/256/4096 was respectively
1.68–1.71, 2.06–2.12 and 2.21–2.28 times the slower standard peer; growing
fill/pop was 1.43–1.50 at 256/4096, and heapify/pop was 1.28–1.39 at
256/4096. Four-ary arithmetic reduced the number of levels but retained the
three full-owner transfers per exchange and added child comparisons. A paired
same-source comparison against the recorded binary image shows only a
2–4% median reduction in the wide cells (and no uniform scalar gain), so this
candidate cannot meet the owner target. The binary-heap design remains in
force; the next PriorityQueue discriminator must change movement or result
boundaries, not only fanout.

**Design suitability.** The boxed prefix supports arbitrary consumption,
growth and bottom-up construction without another storage mechanism.
Whole-slot sifting has a measured wide movement cost against hole C.
The remaining scalar Result boundary and small wide replacement questions
need bounded follow-up; this evidence supports an executable reusable heap,
not a claim that those costs are solved or that a new language mechanism
has been selected.

### Prospective fixed-storage exchange with bounded scratch

Before native construction or measurement, this candidate changes only the
compiler's ordinary OP-11 fixed-storage aggregate exchange. Its code patch
starts from `22096b01dbd508d47643a1211a8feb950b26a3f1`; library and workload
sources, public interfaces, binary sifting, ownership and cleanup remain
unchanged. The [pending storage-placement amendment](../../../../design/amendments/fixed-storage-exchange.md)
is unselected. No source-language or live-tree rule changes here.

The candidate retains one exchange IR operation and plans one raw temporary
of at most 16 bytes through the target frame plan. A constant-trip loop copies
A to scratch, B to A, then scratch to B before advancing, followed by the
exact remainder. Ordinary memcpy preserves equal targets, representation
padding and inactive bytes; zero-size storage transfers nothing. Scalar and
runtime-content pointer-slot exchanges and generic snapshot copies remain
unchanged. Current qualified targets have eight-byte, eight-aligned pointers,
whose complete fields fit these chunk boundaries. A future target must
establish its representation-preserving copy granule before reusing this
choice. No integer reinterpretation, volatile operation, barrier, alias
assertion, new inline hint or runtime alias branch forces the result.
The existing frame plan owns scratch extent and alignment; lowering and the
backend share the stored-aggregate classification. The loop bounds emitted IR
for large aggregates while native unrolling/register allocation remain screen
outcomes. Generic copying, result reuse and ownership checking stay unchanged.

The first screen builds one gate-profile CLI with jobs=2 under the shared
guard, preserving the baseline CLI `5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`
and the separate Deque-only CLI. Since the integrated source also contains
the pending Ring entry-capacity fact, require unchanged-source Deque raw LLVM
to equal its frozen Deque-only candidate, and require PriorityQueue raw LLVM
from the baseline and Deque-only CLIs to agree. Combined PriorityQueue LLVM
may change only fixed-storage swap bodies. Cross-factor changes require a
separate comparison. Record exact patches, inputs, commands, hashes, exits
and separate construction/check costs.

Compile the unchanged practical timed LLVM with the recorded ordinary Apple
Clang O3 flags and inspect every changed WF body. Both wide hot exchanges
must lose complete 256-byte stack staging, preserve vector movement and heap
repair work, introduce no hot copy helper, and not grow the complete wide
trace frame. Record instructions, branches, text, relocations, placement and
unwind changes. A smaller temporary with unchanged whole-exchange stack
traffic fails; a failed mechanism screen stops before timing.

After independent admission, the existing ordinary/retained checks and
semantic regressions must pass: dynamic equal/disjoint wrapped 264-byte
owners, ordered words and pointer remainder, padded-enum bytes and active
cleanup, zero-size canaries, and unchanged runtime-content exchange. Retain
negative controls for omitted chunks/remainder, padding, canaries and release.
Complete practical checks preserve 3,600 traces and eight refusal/retry chains
per image, all 150 accounting rows, and existing checksum/accounting faults.
Account equality describes those instrumented images only.

Timing requires a further explicit admission: one complete control/candidate
pair of `measure 0 64` and `measure 1 64`, all seven samples and all five
implementations, retaining 2,100 rows per arm. Report every one of 24 useful
cells and the six unranked storage controls under the current ECOSYSTEM
duration, stability, native-drift and observed-range criteria. No selected
subset, extra variant or policy conclusion follows from a partial result.

The 2026-09-28 native screen remains unselected. The integrated CLI
`afa4e18e6cf813b9c792074b163bdffd2b73103ada1cf9b59fea94f04bf1d0c7`
preserves the Deque-only raw LLVM exactly; baseline and Deque-only CLIs emit
identical PriorityQueue LLVM, and its sole combined raw change is the wide
`wf_swap$instance$d8e29ae129ba2326` body. Native code changes 11 of 68 bodies,
all wide; the scalar trace is unchanged. The standalone swap frame falls
from 272 to 16 bytes, but 13 of 16 chunks still store and reload scratch:
416 bytes per exchange versus 512 before, with 54 to 93 instructions.
Both indexed sift helpers retain that same 416-byte traffic. The wide trace
frame falls from 3,296 to 3,072 bytes, while four static calls to existing
indexed sift helpers replace previously inlined work. These control costs
and residual copies are adverse observations, not a preregistered automatic
refusal. Before any timing, the necessary native screen is admitted: complete
256-byte staging disappears, stack traffic falls from 512 to 416 bytes,
vector movement remains, no hot copy helper is added, and the complete trace
frame shrinks. The unchanged full correctness/accounting phase must pass
before a separately authorized complete timing pair decides that tradeoff.
No criterion or compiler variant changes. Text falls from
16,788 to 14,032 bytes; text relocations rise from 178 to 189, 51 symbols move,
and unwind contents change. Constant-section bytes remain identical.

Installed Apple Clang O3 LLVM retains all 48 small memcpy operations through
an independent 16-byte alloca. Its filtered pass trace shows SROA after full
unrolling, MemCpyOpt, and late SROA all retaining the copies. This is residual
memory-copy work, not demonstrated register-pressure spilling or failure to
split the target frame. The exact later three-chunk forwarding limit is not
established. The diagnostic's printer filter also limits its final LLVM
output; an initial whole-module equality assertion stopped orchestration,
and the corrected read-only comparison found the selected body byte-identical.

The first shared unit build passed in 88.714 s. Its new fixture sources failed
before backend execution: one Deque canonical-form error, plus three swap
fixtures with missing canonical trivia or arithmetic written directly as a
comparison operand. Existing tests passed 24/29/7 in the windows/owned-places/runtime-swap
filters; direct filter exits were 101/101/0. Only source-authoring repairs
followed, preserving the work, checks and fault controls; frozen-CLI source
checks passed. One repaired unit rebuild took 57.909 s. The same filters then
passed 25/32/7 tests with direct exits 0/0/0 and empty stderr, taking
6.629/15.543/4.774 s respectively. These include the wrapped 264-byte owner,
padding/inactive-byte, zero-size, retained-call and deliberate copy/cleanup
fault checks. All 417 pinned source inputs stayed unchanged during each run.

The initial CLI build took 54.222 s; baseline/candidate native assembly took
0.246/0.197 s and assembly-to-object construction 0.068/0.057 s. Exact commands,
hashes, full changed-body inventory, diagnostic and failed/passing logs remain
in `/private/tmp/whitefoot-fixed-storage-swap/` (`screen/`, `cause/`,
`semantics/`, `semantics-continuation/`, `fixture-authoring/`, and
`semantics-repaired/`).

The subsequent guarded practical phase passed all 16 check/ledger/fault
commands. Each arm passed 2,160 ordinary and 2,160 retained traces with four
native refusal/retry chains per image, then 3,600 practical traces and eight
chains in each timed/account image. The unchanged C-only images each passed
1,440 traces and four chains. All ten public queue operations and four
callbacks survive in both retained WF modules and the C control. Both
150-row ledgers are byte-identical to the existing accounting reference,
SHA-256 `ed77d5bf20c8c3c0a120b052de682e7e49d5692705b1933707ff8dd16cc667fc`;
this equality concerns instrumented images only. Both arms' checksum and
accounting faults exit 1 with the required diagnostics. The 479 pinned inputs
remain unchanged; neither CLI nor ecosystem peer was rebuilt. The newly
linked control timed/account images are byte-identical to the existing
baseline images. Native fixture construction took 2.451 s, symbol inspection
0.036 s, normal checks/accounting 5.466 s and fault execution 0.017 s
(guard wall time 8.292 s). Exact commands, statuses and hashes are in
`/private/tmp/whitefoot-fixed-storage-swap/practical/`. That correctness phase
ran no performance timing and changed no language or live design-tree rule.


### Fixed-storage exchange timing: three gains and six useful losses

The single authorized 2026-09-28 pair rejects selection of this bounded-scratch
lowering. Among 24 useful cells, three have qualified observed-range gains,
six have qualified losses and fifteen overlap. All six storage controls
overlap and remain unranked. Wide pop/push improves by 3.13–3.14% at 16,
8.08–9.46% at 256 and 10.97–11.13% at 4,096, measured as elapsed-time
reduction. Wide replace-top regresses at every population by 13.54–15.37%;
wide grow-pop/16 by 12.43–13.42%; wide heapify-pop/16 by 12.26–12.93% and
heapify-pop/256 by 2.75–3.12%. No scalar useful-cell gain or loss is established.

The current slower-standard target moves from 11 pass / 10 deficit / 3
inconclusive to 10 / 10 / 4. Wide heapify-pop/16 loses its pass. The three
pop/push improvements still miss that target: candidate/C++ medians are
1.643–1.646, 1.896–1.919 and 1.981–1.982 at the three populations;
candidate/Rust is 1.987–1.989, 2.082–2.094 and 2.081–2.088. Replacement's
remaining native wins do not erase its regressions from the control.

The [full control samples](ecosystem-fixed-storage-swap-control-samples.csv)
and [full candidate samples](ecosystem-fixed-storage-swap-candidate-samples.csv)
retain 2,100 rows each. The [paired cells](ecosystem-fixed-storage-swap-paired.csv)
retain both cohort medians and all observed bounds, qualifications and four
native-control drifts. The table below reports every cell; a ratio below one
means the candidate took less time. P/D/I is the separately qualified
slower-standard target, not the before/after result.

| Bytes | Path | Count | Candidate/control c0 / c1 | Paired result | Native target control → candidate |
|---:|---|---:|---:|---|---|
| 8 | pop-push | 16 | 1.0135 / 0.9740 | overlap | I → I |
| 8 | pop-push | 256 | 1.0228 / 0.9989 | overlap | D → D |
| 8 | pop-push | 4096 | 0.9980 / 1.0071 | overlap | P → P |
| 8 | replace-top | 16 | 0.9992 / 1.0264 | overlap | P → P |
| 8 | replace-top | 256 | 1.0008 / 1.0008 | overlap | P → P |
| 8 | replace-top | 4096 | 0.9965 / 0.9965 | overlap | P → P |
| 8 | grow-pop | 16 | 0.9775 / 0.9973 | overlap | D → D |
| 8 | grow-pop | 256 | 0.9995 / 1.0043 | overlap | I → I |
| 8 | grow-pop | 4096 | 0.9958 / 0.9968 | overlap | P → P |
| 8 | heapify-pop | 16 | 0.9977 / 1.0010 | overlap | I → I |
| 8 | heapify-pop | 256 | 0.9984 / 1.0028 | overlap | P → P |
| 8 | heapify-pop | 4096 | 1.0010 / 1.0009 | overlap | P → P |
| 8 | setup-cleanup | 16 | 0.9983 / 1.0046 | overlap | unranked |
| 8 | setup-cleanup | 256 | 0.9985 / 0.9975 | overlap | unranked |
| 8 | setup-cleanup | 4096 | 0.9971 / 1.0055 | overlap | unranked |
| 256 | pop-push | 16 | 0.9686 / 0.9687 | gain | D → D |
| 256 | pop-push | 256 | 0.9054 / 0.9192 | gain | D → D |
| 256 | pop-push | 4096 | 0.8887 / 0.8903 | gain | D → D |
| 256 | replace-top | 16 | 1.1448 / 1.1387 | loss | P → P |
| 256 | replace-top | 256 | 1.1354 / 1.1537 | loss | P → P |
| 256 | replace-top | 4096 | 1.1363 / 1.1363 | loss | P → P |
| 256 | grow-pop | 16 | 1.1342 / 1.1243 | loss | D → D |
| 256 | grow-pop | 256 | 1.0244 / 1.0277 | overlap | D → D |
| 256 | grow-pop | 4096 | 1.0066 / 1.0106 | overlap | D → D |
| 256 | heapify-pop | 16 | 1.1226 / 1.1293 | loss | P → I |
| 256 | heapify-pop | 256 | 1.0275 / 1.0312 | loss | D → D |
| 256 | heapify-pop | 4096 | 1.0180 / 1.0202 | overlap | D → D |
| 256 | setup-cleanup | 16 | 1.0032 / 1.0020 | overlap | unranked |
| 256 | setup-cleanup | 256 | 0.9963 / 1.0008 | overlap | unranked |
| 256 | setup-cleanup | 4096 | 0.9958 / 0.9977 | overlap | unranked |

Every recorded sample, including native controls, exceeds 1 ms: the minima
are 1.163 ms for control and 1.162 ms for candidate. No maintained native
comparison is duration- or cohort-unstable. The useful WF before/after minimum
is 1.206 ms and maximum cohort-ratio spread is 4.055%. Inter-arm native median
ratios over useful cells are 0.9771–1.0308 for swap C, 0.9721–1.0403 for hole C,
0.9866–1.0349 for Rust and 0.9512–1.0281 for C++; no cell exceeds the recorded
10% drift screen. All outliers remain. The observed bounds are sufficient
separation tests, not confidence intervals, and overlapping samples do not
establish equivalence.

The pair uses the already checked images: control SHA-256
`ba7726f586341071cc3364fd6f037e501660e0517788c3443bb2240005b5150f`
and candidate `39db8b84322f6dfee9cd63a0a628ba2f25a206a92fb63dc78cd76b4eed775abf`.
There was no rebuild, extra factor, replay or selected-cell run. The four
direct commands were control `measure 0 64`, control `measure 1 64`, candidate
`measure 0 64`, candidate `measure 1 64`, under one shared guard with jobs=2.
All exited 0, with empty stderr; all non-time fields match across arms and
all 245 pinned artifacts/inputs remained unchanged. Their respective wall
costs were 27.409152, 27.433780, 27.394347 and 27.347373 s; reduction took
0.212859 s and the complete guarded phase 110.183258 s. These costs include
oracle/warmup/output work and are not operation latencies or selection grounds.

Native code supports a mixed mechanism: whole-owner staging is removed, but
416 bytes of scratch traffic remain and ordinary inlining changes introduce
four static sift calls. Replacement's sift remains inline in both trace
bodies. Its preceding root exchange instead grows from 41 to 86 instructions:
the control directly saves the old root and writes the incoming owner, while
the candidate first copies the complete incoming owner into the carrier and
retains thirteen scratch store/reload pairs. Those matched blocks contain
1,024 versus 1,952 bytes of vector load/store operands, before any downward
exchange (`screen/control.s:2086` and `screen/candidate.s:1958` in the retained
local evidence). These are instruction operand totals, not measured cache or
memory-system traffic. The growing-fill path adds a rise call per accepted
push; heapify adds a sink call per parent; final drain now calls pop, which
calls sink. Their old bodies contained those repairs inline. This explains
which extra work accompanies the small-population losses, without assigning
it an isolated time cost. This paired trial does not isolate the separate cost
of scratch traffic, call placement, instruction growth or code layout. Static
addresses do not establish equal runtime ASLR or data addresses across
processes. The evidence covers this Apple Clang 21 O3 arm64 host and fixed
order pair; it selects no cross-target or general performance policy. The
recommendation is to retain the prior lowering and generic representation/
ownership tests. The surgical withdrawal removes only this unselected
production machinery. Dynamic equal/disjoint wrapped 264-byte ownership,
all-word/pointer checks, zero-size canaries, padded-enum bytes and active
cleanup remain covered in ordinary and retained forms. The 16-byte scratch/
8-byte remainder and zero-byte no-frame IR assertions, plus faults that
remove the candidate's 8/16-byte copies, are retired because they describe
the refused mechanism. The replay patch preserves those experimental
observations; no language or generic behavior requirement is retired.

**Reconstruction.** The [seven-file replay patch](ecosystem-fixed-storage-swap-replay.patch)
applies with `git apply --unidiff-zero` to
`22096b01dbd508d47643a1211a8feb950b26a3f1` and preserves the exact
PQ compiler factor plus the corrected semantic fixtures described above.
It does not include Deque or projection changes. The measured combined CLI
also contained the separate Deque factor; the recorded baseline/Deque-only
PQ LLVM equality establishes that factor's neutrality for this module.
The original combined CLI source patch, individual compiler binaries and
417-file build pin remain in the local `screen/` evidence. Do not infer
replayed CLI byte identity from that module-level isolation.

Build control and patched candidate CLIs in separate isolated source trees
with the ordinary `cargo build --manifest-path compiler/Cargo.toml --profile
gate --bin whitefootc --locked --offline -j 2` under the shared guard. The
unchanged [family Makefile](Makefile), [ecosystem recipe](../ecosystem.mk) and
[runtime recipe](../../../../compiler/runtime.mk) reconstruct ordinary,
retained, timed and accounting images and native inputs. Use `check
ecosystem-check ecosystem-account` with a fresh `BUILD` and the corresponding
`WHITEFOOTC`. Timed LLVM renames `main`, retains exactly the two whole-trace
boundaries, and receives one ordinary `clang -O3 -Wno-override-module -x ir`
compilation. Accounting applies the existing malloc/free substitutions and
allocator return attributes before that compilation. In the measured pair,
the 15 non-WF link inputs per image, their order and flags remained fixed.
Raw PQ LLVM hashes are control
`c6725edd5e9f4d8d0267eebc150b2d8fea9926727d44625e43ee3b0b4a5e5307`
and candidate `06abf69a74e4ef89dac9c9720476c655c27c39f9286c85e2409bda42ee2b7094`;
O3 object hashes are `8c3feb1e1cd0a6921b44470460ce1b57798bef9d964d5b607f2ddd051d57d2e7`
and `89505fcefbb77767f8fcbd0f36e10172816cbe8f0a95a44d1ce5a227f72ef4db`.

Reduce either published arm with `perl ../summarize-ecosystem.pl --complete
priority=FILE` and `--targets priority=FILE` from this directory. For each
paired cell/cohort, use candidate median / control median; the observed lower
and upper are candidate minimum / control maximum and candidate maximum /
control minimum. A gain requires both uppers below one; a loss requires both
lowers above one. Qualify with minimum sample at least 1 ms and
`100 * (max cohort ratio / min cohort ratio - 1) <= 10`, and retain native
inter-arm drift separately. No new maintained runner is needed. Exact commands,
phase exits, freezes and the small one-shot reduction remain in
`/private/tmp/whitefoot-fixed-storage-swap/timing/` for this trial.

Published SHA-256 values, in control/candidate/paired/replay-patch order, are
`1a4a4a845c694b28d326e7ffe9db4c44e82ab7bfbfc67d500c1ba046be7586f3`,
`94ee4bf36759688deb32f8da6f49fd805029c773c468e1010cf698325785e0d0`,
`ee652df6e827da841f116c94694db0b7f8fb61410e016c734de43c4273461d83`, and
`27bca60c8c0896ae25a67d7863cfba0b767e0f92e9477e2880aec0a6e69404dc`.

### Existing consumed-projection factor: unchanged PriorityQueue code

The 2026-09-28 native-only screen compared the same unchanged witness with
frozen CLI `5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`
and the clean projection CLI
`f575555aa641f6cbd2aa70f08f3ff5a1969381c13e9811a4e3958715b044270c`.
The prediction recorded before emission was no applicability: the consumed
`PriorityCostRecord.words` projection has a parameter parent, excluded by the
new planner, and both push-result matches are direct expressions without a
consumed binding root. Whole-owner swaps/takes lie outside that factor.

Raw LLVM, the unchanged timed transformation, complete assembly and complete
object are byte-identical between arms and to the earlier frozen baseline.
All 74 LLVM definitions and 26 types agree, including all 4 memcpy and 32
memmove call sites. All 68 native bodies agree: 4,197 instructions and 82
static calls, with unchanged ABI, owning calls, frame sizes, literal bytes,
relocations, CFI and layout. The scalar/wide trace frames remain 192/3,296
bytes. `record_accept` already reads directly from its input in native code,
with 53 instructions, no calls and no stack allocation. There is no native
code change to measure and no PriorityQueue result to credit to this factor.

All 13 direct construction/inspection commands exited 0 under one guard,
reported at 1.20 s; the script recorded 1.056751 s. No compiler build, linking,
correctness execution or timing ran. Twenty new pre/post pins agree; a separate
post-screen audit finds all 12 module declarations/catalog files equal to
both earlier frozen source manifests. The prior raw/assembly/object hashes
remain `c6725edd...`, `2013a9ee...` and `8c3feb1e...`, with complete values above.
The preregistration, exact commands, direct statuses and all-body inventory
are local one-shot evidence in `.build/consumed-projection-native/`.
Its first read-only instruction counter omitted operand-free `ret`; correcting
that parser required no re-emission and reproduces the object text size.

### Preregistered ordinary exchange-helper source screen

Before source admission on 2026-09-28, freeze the two-helper candidate in
`.build/exchange-alias-source/criterion.md` and `source.patch`. Replace only
the two swaps in the shared indexed sifts with an ordinary private call over
the already proved-disjoint parent/child references. It atomically updates
one referent through an owned-element helper that swaps its local with the
other referent and returns that local. Keep unconstrained T, both plain and
indexed callers, all public interfaces, root/local swaps, comparisons,
heapify order and the two immediate position callbacks in their exact order.
This tests the ordinary reference alias boundary, not a new lowering rule;
consumed-input/result placement is not assumed to apply to a PlaceRead.

Use frozen CLI575 and the existing Apple clang O3 flags. Qualify an identical
package carrier by requiring its entire baseline assembly to equal the frozen
standard-module assembly after only `std.collections.priority_queue.` becomes
`priority_queue.`. Both arms use the same ten caller alias substitutions,
module graph/interface and unchanged timed whole-trace boundaries. Source
refusal or a carrier mismatch stops without a fallback or narrowing T.

Require less complete-owner movement in both wide shared sifts and their trace
paths, without owner-sized replacement staging, new hot helper/copy calls,
frame growth, scalar mechanism regression or expansion of replacement's root
exchange. Preserve SIMD movement, owning ABI and allocation/cleanup call
domains; inventory all other bodies, frames, transfers, calls, CFI and layout.
Keep direct statuses, hashes and source/native costs under the shared guard.
This authorizes source/IR/assembly/object inspection only: no execution,
accounting, timing or selection. The exact one-shot command and prospective
continuation conditions are in the frozen scratch criterion.

The first guarded stage stopped at that carrier assertion (direct outer
exit 1, 0.88 s; script 0.765827 s). Baseline source check, LLVM emission and
O3 assembly exited 0 in 0.156232, 0.347899 and 0.228121 s. All 43 input pins
held. Candidate admission/emission, object construction and execution did not
run. The saved diff changes twelve assembly lines: only the scalar/wide
indexed rise/sink instance names and their comments. Their function-actual
argument names the copied private `ignore_position`, so module-qualified
generic identity changes its digest. Four exact additional sift-symbol maps
make the complete assembly equal; this was discovered after, and does not
rewrite, the failed one-prefix criterion. Raw LLVM's 74 definitions also
agree after those maps and two same-layout `{ ptr }` type-name maps; its only
other difference is eighteen unused external declarations and blank lines.
`baseline-mismatch.diff`, `baseline-raw-mismatch.diff` and
`carrier-analysis.json` preserve that read-only diagnosis. No normalization
change or continuation is inferred from it; the candidate remains untested.

After independent verification, the corrected setup criterion explicitly
admits only the four documented full sift-symbol mappings (rise and sink,
scalar `69e12dced9658a85` to `c3d7b709dd11a924`, wide `5786705437533435` to
`a18d9435e766a12a`) and the two documented same-layout raw type names. Complete
assembly and all 74 raw definitions must agree; no broader normalization is
allowed. `continuation-criterion.md` records this before candidate admission,
with instruction/symbol falsifiers. The original failure remains frozen; its
baseline outputs are reused. The same candidate and native mechanism gate
continue under a new guarded command, still without runtime or timing.

That continuation qualified the carrier and rejected both synthetic identity
and instruction faults, then stopped at candidate source admission. CLI575
returned exit 1 in 0.038451 s at `priority_queue.wf:65:7`:
`error[WIN-3]: LinearAssignmentTarget`, target type `T`, on
`set first^ = priority_queue_exchange_owned::<T>(held: move first^, other: second);`.
The generic target may be linear; OP-12's affine/copy update does not supply
the required unconstrained-owner operation. No `drop` bound, owner-domain
narrowing or language/compiler change was substituted. This does not test the
separate unverified dynamic-index OP-12 concern. The outer command exited 1
in 0.11 s; the script recorded 0.060951 s. All 65 input/prior-artifact pins
held. No candidate LLVM, optimized IR, assembly, object or program ran, and
the pending baseline native commands were skipped. The source patch and both
failed-stage records remain in the same scratch home.

### Preregistered distinct-reference helper screen

The next ordinary-source factor adds one private generic helper over two
write references; its body calls the unchanged builtin `swap` and returns
unit. Only the two common indexed-sift calls use it, with their existing strict
index-separation proofs and unchanged immediate reporter order. There is no
owned local or atomic update, so unconstrained/nodrop T remains in scope.
Root/local swaps, public APIs, scalar instances, heapify and cleanup are
controls. The current queue parameter's `noalias` does not distinguish two
subplaces reached through it. The ordinary two-reference helper can instead
carry EFF-5's disjointness for each parameter; the builtin swap still permits
equal addresses and retains its existing ABI (`backend/emitter.rs`'s
`aliasing_admitted_row` and `reference_parameter_facts`). Whether O3 propagates
that fact far enough to remove staging remains the native discriminator.

Before source admission, `.build/exchange-distinct-source/criterion.md`
records the unchanged full movement/frame/SIMD/scalar/root-exchange/hot-call
gate, exact qualified carrier, CLI575, O3 commands and input pins. Reuse the
saved baseline; do not rebuild a compiler or peers. One short guarded
source/IR/native stage follows Slots' complete timing pair, with no simultaneous
compiler commands. Stop before any runtime or timing. A later admitted phase
must include complete ownership and indexed reporter-order checks before the
unchanged full-matrix comparison; this preparation makes no selection claim.

### Distinct-reference helper result: native movement gate failed

The single native-only stage admitted unconstrained T and completed all 13
direct commands with exit 0. All 81 input/prior-artifact pins held. Source
check/emission cost 0.161602/0.361218 s; native construction/inspection cost
0.355299/0.343253 s. The guard reported 1.31 s and the script 1.246599 s.
The saved baseline was reused; no compiler or peer was rebuilt. No image was
linked or executed, and no correctness, accounting or timing phase followed.

The new helper carries both ordinary `noalias` parameters, and O3 propagates
their alias scopes into the shared sifts. This changes the middle 256-byte
transfer from memmove to memcpy and uses fewer simultaneous vector registers.
It retains the complete 256-byte private snapshot, the first transfer into
it and the final transfer out: three 256-byte copies and 512 bytes of stack
payload traffic per exchange. The new wide helper remains 54 instructions
with a 272-byte frame; scalar is 6 instructions with no frame. Wide indexed
sink/rise remain 82/68 instructions and 272-byte frames. The complete wide
trace remains 1,525 instructions with a 3,296-byte frame; scalar remains
446 instructions and 192 bytes. Required movement reduction therefore fails.
This is an observed use of the new alias fact without the required movement
benefit, not evidence that the fact was missing or that runtime would improve.

Raw LLVM changes exactly the four scalar/wide shared-sift call targets and
adds two helper instances (74 to 76 definitions). Ten existing native bodies
change their wide-copy scheduling: the wide trace, push, pop, replace_top,
heapify, drain, ordinary rise/sink and indexed rise/sink. Every existing body's
instruction, SIMD load/store and call counts, stack allocation and CFI stay
the same; scalar bodies remain exact. There are 77 branch-link calls plus
5 tail calls in each arm. Replacement's initial root exchange is unchanged.
The two added leaf bodies grow text from 16,788 to 17,028 bytes and total
instructions from 4,197 to 4,257; native definitions increase from 68 to 70.
Fifteen existing symbols move by 24 or 240 bytes. Literal16/const bytes and
eh_frame stay exact; compact-unwind changes only five function-start fields.
Section relocation counts and symbol-relative meanings agree throughout.

The exact patch and complete body/layout/relocation inventory remain in
`.build/exchange-distinct-source/{source.patch,native-screen.json}`. The
candidate raw LLVM, assembly and object SHA-256 values are
`a61ecf4d6905775e39f77aba13d5820b232e54a6afc05d3d1416b0eabe22e048`,
`bbdefa07717193ff12bb405e69207e0066984b7079970abcd3b60ddebfa2ca45`, and
`a04f623d80b732418b2fa7d16178aa7de2a64191a3fe107e0e3d1cd6001e51d7`.
The first read-only inventory's extra helper-byte-equality assertion failed:
the real scheduling differences are retained in `changed-native.diff`, not
normalized away. Correcting that inventory required no native rerun. The
source candidate stays in scratch; no library, compiler or language change
is selected from this failed gate.

### Replacement result placement: unselected lowering diagnosis

Read-only inspection separates a replacement cost from the shared sift's
three-copy algorithm. In the qualified carrier's frozen
`.build/exchange-distinct-source/screen/baseline.opt.ll`, lines 1909–1912
save the old 256-byte root in `%v3.sroa.0.i` and install the incoming record;
the saved root stays live through the complete sink loop. Line 1963 copies
it into `%wf.slot.9`, then ends its lifetime before the ordered digest.
The independent swap temporary at lines 1954–1958 is a different lifetime.
The retained O2 baseline also keeps the old root across its comparator calls
and copies it into `%wf.result` at line 1572 of
`/private/tmp/whitefoot-fixed-storage-swap/practical/control/retained.opt.ll`.
This is code attribution, not a measured causal share or a hole-sift proposal.

The frozen raw `screen/control.raw.ll:2295` in that same scratch experiment
has an owned-input snapshot `%wf.slot.0`, a separate addressed binding `%v3`,
and a final whole-binding load into `%wf.result`. The checked lowering path
explains those objects: `lowering/builder/storage.rs` promotes the borrowed
owned parameter with `AddressOf`; `binding_value`/`load_storage_value` makes
the final value read a `Load`. `backend/storage.rs` gives that load its own
aggregate group. `returned_storage_slot` already chooses that group for the
result, while `FunctionFramePlan` still allocates `FunctionSlot::Address`.
`load_place_result` therefore emits the surviving binding-to-result copy.
These relevant lowering, destination-selection and load-emission functions
match baseline `22096b01dbd508d47643a1211a8feb950b26a3f1`. Existing fresh-binding
forwarding only redirects an earlier producer into its initializer's address;
it does not place a mutable binding in its eventual result. Entry parameters
also fail its same-block producer test. Exposed aggregate groups are frozen,
and ordinary `Load` is deliberately not an input/result reuse edge.

The proposed next factor is general whole-binding result placement, pending
review before implementation. Qualify one acyclic private `AddressOf` root
of the complete result type, dominating its uses, whose terminal whole loads
supply every return. Require that the returned group has only those terminal
producers, no competing definition, entry parameter, field/projection backing
or exposed snapshot, and no deferred address lifetime. Preserve all earlier
value snapshots, complete CFG interference, target extent/alignment, and
entry capture before the binding first writes result storage. Only that
address allocation uses the existing result destination; no source name,
record-size rule, swap change, new alias fact or projected Call operand is
involved. Partial returns, competing roots and unproved address transports
retain their current storage.

Early result writes also need the existing caller-placement proof: calls use
private result groups; whole/field reuse excludes exposed backing and keeps
complete interference checks. A caller-visible reference to an old target,
including an OP-12 environment read before commit, must never become an alias
of this early result. Owned-input capture alone does not prove that condition.
Tests must distinguish same/distinct owned-input result destinations, late
old-target reads, another input captured before result writes, intermediate
snapshots surviving mutation, and competing/backedge returns. Full payload,
padded owning enum, smaller-return canary and zero-size behavior need normal
and retained execution, cleanup faults, and unchanged indexed reporter order.

The prospective native discriminator is removal of the ordinary wide
replacement's extra complete transfer, counting all replacement setup and
return traffic rather than just the final copy. A retained helper may instead
need an incoming snapshot when its result aliases its owned input; record
that separately and require no additional complete transfers, hot calls or
frame growth. Keep SIMD, scalar and shared-sift mechanisms, ABI and callback
order; inventory every changed body. Failure stops before runtime/timing.
Only an admitted candidate proceeds to the existing complete owning/refusal,
normal/retained and 150-row accounting checks, then a separately authorized
full practical pair. No implementation, build or new measurement ran here.
