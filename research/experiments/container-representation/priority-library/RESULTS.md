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
for the new comparison. Current execution results are pending.

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

**Design suitability.** The boxed prefix supports arbitrary consumption,
growth and bottom-up construction without another storage mechanism.
Whole-slot sifting has a measured wide movement cost against hole C.
The remaining scalar Result boundary and small wide replacement questions
need bounded follow-up; this evidence supports an executable reusable heap,
not a claim that those costs are solved or that a new language mechanism
has been selected.
