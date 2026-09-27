# Deque library costs

## Current standard-container comparison

The opt-in `ecosystem-*` targets implement [ECOSYSTEM.md](../ECOSYSTEM.md) with
the current [`std::collections::deque`](../../../../lib/std/collections/deque/module.wfm)
module, Rust `VecDeque`, and C++ `std::deque`. The C loop and bulk variants
remain attribution controls. This fresh O3 comparison is separate from the
historical O2 and retained-helper measurements below.

The whole trace stays inside each implementation behind one C ABI call;
there is no per-operation foreign call. The common application tasks are
forward churn, reverse churn, actual growth, and setup plus cleanup. No trace
keeps references into the deque across mutation. All removed owners and all
remaining elements are consumed in logical order and every payload word
enters the digest. Native values are an 8-byte scalar or a 256-byte inline
record, with Rust `Copy`/`Clone` absent and C++ copies deleted. There is no
per-element allocation and no nested-owner performance claim.

Growth (source path 3) fills `count`, then appends `count + 1` further values
and consumes all `2 * count + 1` owners. Whitefoot explicitly rebases its full
ring to that capacity. Rust starts with capacity for `count` and grows through
its ordinary deque policy. C++ deque has no reserve operation and keeps its
ordinary block policy. Native capacities and reference stability guarantees
are not made to imitate Whitefoot. The former wrapped-rebase trace (source
path 2) is still checked for Whitefoot/C, but excluded from native ranking:
it never uses its extra capacity and neither native API promises its exact
conversion. Whitefoot's fixture bounds the original population by 4096 and
the larger backing by 8193.

The independent oracle derives the logical sequence arithmetically without
constructing a deque. Correctness includes counts 0, 1, 2, 3, 16, 63, 256 and
4096; rounds 0, 1 and 3; seeds 0, 17 and `UINT64_MAX`; and both payloads. The
zero-count growth trace still appends and consumes one value whenever rounds
are nonzero. The measured populations are 16, 256 and 4096.

Run the phases separately under the repository guard from the root:

```sh
perl .github/run-check.pl deque-ecosystem \
  make -C research/experiments/container-representation/deque-library <target>
```

Replace `<target>` by `ecosystem-build`, `ecosystem-check`,
`ecosystem-account`, or `ecosystem-measure`. Outputs live in
`.build/ecosystem/`: `configuration.txt` records native flags and toolchains,
`accounting.csv` contains allocation observations, and `measurements.csv`
contains raw timed samples. Keep these with recorded source/compiler
identities. These adapters and targets retire with this experiment or a
maintained successor that preserves its evidence.

`ECO_ACCOUNT` and `ECO_SAMPLE_FILE` override those output paths.
`ECO_WORK` defaults to 1048576 and `ECO_REPEATS` to 7. Each active trace uses
`max(1, ECO_WORK / count)` rounds. A churn round performs `count` pop/push
pairs; a growth round constructs and consumes `2 * count + 1` values.
Setup-cleanup instead has zero rounds and repeats that many complete
fill/drain/free traces. Thus equal work does not mean equal individual
operation counts between path names. Warmup runs every implementation with
one sixteenth of the nonzero rounds or trace repetitions, clamped to one.
Two cohorts rotate implementation order by sample and reverse it in the
second cohort. The CSV records work, rounds and repetitions. Compare whole
trace costs; do not subtract setup or call these isolated growth pauses.
Extend bounded runs where a cell remains too short or unstable.

Timed images use ordinary allocation and omit all observer hooks. Separate
allocation images execute three rounds (zero for setup) and one trace per
cell. Whitefoot/C request, requested-byte and peak formulas are independent
of observed counters; native policies are measured without forcing those
formulas. All paths require complete cleanup and balanced allocation lifetimes.
`requests` includes successful reallocations, `realloc_requests` counts them
separately, and final deallocations are `releases`; native totals must satisfy
`requests == releases + realloc_requests`. `peak_bytes` records logical live
requested storage. `peak_overlap_upper_bytes` permits old and new requests
to overlap at Rust realloc without claiming that hidden allocator storage
did overlap. Both exclude private observer headers, RSS and allocator-resident
memory. `ecosystem-check` requires checksum-corruption and unreleased-allocation
controls to fail and verifies that timed images contain no observer symbols.
No specification rule changes for this comparison.

### Verified correctness and allocation observations

The current guarded build and both correctness images completed successfully.
Each image passed 576 configurations and 2,592 complete trace
executions, for 5,184 executions across timed and accounting builds.
The negative checksum and cleanup controls produced their expected rejection
messages; the target also checked that observer hooks are absent from the
timed image. The allocation CSV contains 120 rows. Independently reading
the CSV confirms equal complete checksums within every application cell,
balanced allocation lifetimes, and the stated peak upper bound; every C
control's allocation columns equal Whitefoot's corresponding row.

The existing O2 normal and retained drivers also passed 432 configurations
and 1,296 executions each, totaling 2,592 historical-control executions with
the current module imports. Their retained-helper call checks passed. This
requalifies those reproduction paths without adding historical-series timing.

A separate scratch mutation omitted the final newly appended owner from the
C growth loop while leaving the oracle and allocation schedule unchanged.
The check rejected it with `independent logical-order checksum`, exit 1, at
`variant=1 path=3 count=0 rounds=1 seed=17 wide=0`. This observes the missing
new owner in an initially empty deque and establishes that the growth oracle
checks the added values, beyond the generic checksum-bit corruption control.
The scratch mutation is outside the maintained driver and adds no timing data.

The preserved [allocation CSV](ecosystem-accounting.csv) SHA-256 is
`0ac88c02c5e9ca9517a584cdb75236a3ce1cc09497c4bd2f1e64ae9c835a2fcc`. These are requested-storage observations
from the accounting build, separate from the timing samples below. Build/check
phase durations belong to the central [experiment record](../ECOSYSTEM.md).

At population 4096, forward churn, reverse churn and setup-cleanup have
identical allocation columns within each implementation and payload, despite
their different consumed sequences and round counts. The table shows forward
churn; both C controls have the Whitefoot entries shown.

| Payload bytes | Implementation | Requests | Reallocations | Releases | Requested bytes | Logical peak bytes | Possible-overlap upper bytes |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | Whitefoot / C controls | 1 | 0 | 1 | 32,792 | 32,792 | 32,792 |
| 8 | Rust VecDeque | 1 | 0 | 1 | 32,768 | 32,768 | 32,768 |
| 8 | C++ std::deque | 14 | 0 | 14 | 37,112 | 37,056 | 37,056 |
| 256 | Whitefoot / C controls | 1 | 0 | 1 | 1,048,600 | 1,048,600 | 1,048,600 |
| 256 | Rust VecDeque | 1 | 0 | 1 | 1,048,576 | 1,048,576 | 1,048,576 |
| 256 | C++ std::deque | 267 | 0 | 267 | 1,060,856 | 1,058,816 | 1,058,816 |

That equality does not generalize to every smaller population. With 16 scalar
items, C++ forward churn requests 4,104 bytes in two requests, while reverse
churn requests 8,216 in four. With 256 scalar items, setup-cleanup requests
4,104 bytes in two requests but churn requests 8,216 in four. Native block
policy therefore remains an observable part of the path, rather than a fixed
per-container adjustment. At 16 scalar items, Whitefoot and Rust forward
churn request 152 and 128 bytes respectively; the large-population ratios
would conceal this small-population overhead.

The three-round growth trace at population 4096 has different allocation
tradeoffs:

| Payload bytes | Implementation | Requests | Reallocations | Releases | Requested bytes | Logical peak bytes | Possible-overlap upper bytes |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | Whitefoot / C controls | 6 | 0 | 6 | 295,080 | 98,360 | 98,360 |
| 8 | Rust VecDeque | 9 | 6 | 3 | 688,128 | 131,072 | 196,608 |
| 8 | C++ std::deque | 69 | 0 | 69 | 210,408 | 70,016 | 70,016 |
| 256 | Whitefoot / C controls | 6 | 0 | 6 | 9,438,096 | 3,146,032 | 3,146,032 |
| 256 | Rust VecDeque | 9 | 6 | 3 | 22,020,096 | 4,194,304 | 6,291,456 |
| 256 | C++ std::deque | 1,572 | 0 | 1,572 | 6,352,872 | 2,113,536 | 2,113,536 |

Rust records one initial allocation and two reallocations per growth round.
Its logical peak corresponds to storage for 16,384 elements for the required
8,193, consistent with the native growth policy crossing that capacity.
Whitefoot explicitly chooses 8,193 slots and holds the old 4,096-slot backing
while rebasing. C++ growth makes substantially more requests for wide values,
yet has a lower logical requested-byte peak than either contiguous-ring
implementation in this cell. Request count, cumulative requested bytes and
live peak answer different questions; none is an elapsed-cost or RSS result.
Rust's possible-overlap column is an upper bound around realloc, so its
relationship to Whitefoot's observed old/new overlap cannot establish a
physical-memory ratio.

### Fresh practical timing

The complete [timing samples](ecosystem-samples.csv) and
[allocation observations](ecosystem-accounting.csv) are preserved beside this
record. They serve the standard-container comparison and remain its evidence
until it is retired or superseded with that evidence preserved. Timing source
revision is `0c3203aa6111f14247aa950e3794e83082d4f29c`. The measurement phase
completed in 49.435 seconds, separate from construction and correctness.
The timing CSV has 1,680 rows and SHA-256
`3787a2e45c30fa4bb2ab2d1f11b852d8e76b87afdc560802ad406f145230a591`.

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

| Payload bytes | Path | WF whole trace ms | WF / Rust | WF / C++ | WF / loop C | WF / bulk C |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | forward-churn | 1.649–1.649 | 0.979–0.983 | 1.462–1.463 | 1.228–1.230 | 1.231–1.232 |
| 8 | reverse-churn | 3.531–3.636 | 2.712–2.795 | 1.390–1.427 | 1.172–1.185 | 1.185–1.207 |
| 8 | growth | 5.034–5.090 | 1.266–1.271 | 1.391–1.406 | 1.385–1.400 | 1.809–1.829 |
| 8 | setup-cleanup | 1.614–1.615 | 0.865–0.870 | 0.865–0.881 | 1.394–1.407 | 1.404–1.412 |
| 256 | forward-churn | 32.063–32.278 | 0.988–0.993 | 0.936–0.937 | 0.980–0.984 | 1.002–1.002 |
| 256 | reverse-churn | 31.886–32.206 | 0.993–0.998 | 0.929–0.934 | 0.995–0.998 | 0.999–0.999 |
| 256 | growth | 81.414–82.914 | 1.014–1.019 | 0.980–0.982 | 0.991–0.996 | 1.044–1.049 |
| 256 | setup-cleanup | 36.228–36.496 | 0.995–1.001 | 0.874–0.883 | 0.964–0.974 | 0.971–0.972 |

Scalar reverse churn is the clearest native-library gap: Whitefoot takes
2.712–2.795 times Rust and 1.390–1.427 times C++ at population 4096.
Across all tested populations, the Rust ratio remains 2.712–2.795. The fresh
C loop control follows Whitefoot's ring layout, allocation policy and endpoint
composition, yet its ratio is only 1.172–1.185 at population 4096. Endpoint
lowering and native representation/API differences are therefore separate
follow-up questions; the present matrix does not assign either a causal
percentage. Scalar forward churn is close to Rust (0.979–0.983 at 4096) while
still slower than C++ (1.462–1.463), which prevents a single scalar-deque
ranking from representing both directions.

Wide forward/reverse churn stays within 2% of Rust at population 4096 and
within 10% of C++ throughout the tested populations. The same fresh C
controls are close as well. The 32-word ordered checksum is material work
in these cells; closeness here is not a general native-parity or pure data
movement result. Setup-cleanup is measured separately and is never subtracted
from a mutation trace to create an isolated operation latency.

Growth changes with population. Scalar Whitefoot/Rust is 0.614–0.621 at 16,
1.118–1.120 at 256 and 1.266–1.271 at 4096. The wide-value ratio is
0.875–0.880, 1.136–1.138 and 1.014–1.019 respectively. Allocation observations
show different capacity policies and request counts, but they do not isolate
why those elapsed ratios change. At population 4096, scalar growth also
costs 1.385–1.400 times loop C and 1.809–1.829 times bulk C. The loop/bulk
pair changes rebase copying while preserving the application result; a
same-source Whitefoot discriminator is still needed before attributing its
cost to that composition or choosing an optimization.

All 24 application cells and every native/C comparator pass the recorded
sample-length and cohort-spread criteria: no paired minimum is below 1 ms
and no ratio changes more than 10% between cohorts. Individual outliers are
preserved in the raw samples; stable medians do not establish tail latency.
No deque cell needs a longer replay for the conclusions stated here.

### Native-code follow-up: unmeasured endpoint candidates

Read-only inspection on 2026-09-26 used the baseline preserved at
`c75520e9d59d74e19ba158e1cae5f394b3a2d874`; its compiler, Deque library and
measurement sources are unchanged from the timing revision above. The emitting
compiler SHA-256 is
`cb918e191bb344733347e0602171d2ec53bd1d201044fdbc5dd7666468eea0a0`,
verified against the retained compiler, with the recorded Apple Clang 21.0.0
and Rust 1.98.1 settings. The inspected `.build/ecosystem/deque-costs-timed`
SHA-256 is `f4ffd2d0da12d90272702f4619a6de6af25f01c4f3e2d1ce242f79275737574f`;
its pre-optimization `whitefoot-timed.ll` SHA-256 is
`89622404e240d377a5be6e330a631173c1d26babf07c6342f579e0e34dd854f7`.
Use that checkout and emitter with the phase commands above for reproduction;
a compiler rebuilt with changed bundled library sources is a different emitter.

In the final scalar trace symbol
`_wf_deque_library_trace$instance$29cf4bd076d714eb`, reverse churn's hot loop
at `0x10000637c`–`0x1000063d4` stores the new element, reloads all three
descriptor words (`ldp` plus `ldr`), and recomputes the front predecessor.
Each iteration also stores length twice and head once. Forward churn at
`0x100006420`–`0x100006458` keeps those words in registers. Rust's reverse loop
at `0x10000b7e4`–`0x10000b814`, in `_rust_deque_word_trace`, also keeps its
descriptor state in registers. Both reverse loops use conditional arithmetic,
with no integer division or per-element calls. These addresses identify the
hashed image; symbol names locate the bodies after a rebuild. For example,
from the repository root:

```sh
deque_image=research/experiments/container-representation/deque-library/.build/ecosystem/deque-costs-timed
/Library/Developer/CommandLineTools/usr/bin/llvm-objdump \
  --no-show-raw-insn --disassemble \
  --start-address=0x10000637c --stop-address=0x10000645c "$deque_image"
```

The ordinary Deque endpoint wrappers already call only their primitive and
return the required value. In [`runs.rs`](../../../../compiler/src/backend/emitter/runs.rs),
`emit_run_boundary` stores the payload before `move_run_boundary` reloads
the descriptor; its `PlaceFront` arm calls `boundary_slot` again. The final
`_wf_place_front$instance$4c5620a98e7f6ca7` body retains the same reload and
recomputation. This is a concrete lowering lead, not a measured explanation
of the whole gap: the same-layout C control also retains descriptor traffic.

Two proposed experiments must remain separate, using the same caller, inputs,
toolchain, flags and oracle before and after each change:

- **A: qualify one unsigned subtraction.** In `boundary_slot`'s `PlaceFront`
  arm, try `sub nuw` only for `select(head == 0, cap, head) - 1`.
  Admitted placement proves `len < cap`, hence positive capacity; the selected
  operand is therefore positive whether head is zero or nonzero. This proves
  unsigned non-underflow without extending the flag to any wrapping sum or
  unused subtraction alternative. It does not prove `nsw`: a zero-stride Ring
  can have head `2^63`, whose decrement crosses the signed boundary. The
  hypothesis is that this exact fact removes the observed descriptor reloads;
  unchanged final code or no repeatable same-source improvement falsifies
  that proposed benefit.
- **B: capture the placement update once.** Separately, calculate the new
  descriptor words before the payload store and reuse the already computed
  front physical slot as the new head, initially preserving payload-then-header
  store order. Test whether this removes the duplicate loads/calculation in
  the final loop and improves the same trace. Persistent work or no repeatable
  improvement falsifies its proposed benefit. Do not bundle B with A and
  attribute the combined result to either alone.

The [backend-fact grounds](../../../../design/compiler/backend-facts.md)
require the complete target promise and reject broad Ring no-wrap assertions;
A proposes only the positive-predecessor domain. The
[storage representation](../../../../design/compiler/storage-representation.md)
normalizes zero-stride addresses, not logical coordinates. Regression coverage
must retain huge zero-size capacities, head-zero/nonzero transitions, fixed
and runtime shapes, all endpoints, and owned/nodrop cleanup. The existing
`zero_sized_takes_update_slots_and_wrapped_ring_boundaries_once` test already
crosses the signed boundary on its second front placement at capacity
`2^63 + 1`; add its fixed-capacity counterpart and observe both ordinary and
retained calls. Preserve the selected
[take ordering](../../../../design/compiler/storage-placement.md) and
[Deque contract](../../../../design/language/data-model/deque-rebase.md).
Neither proposal has been built or measured. Correctness and accounting must
pass before timing under the central per-cell criterion; growth and close
forward cells remain separate questions. No speedup or design selection follows
from this inspection.

## Historical source-composition evidence

This explicit experiment bundles [`deque.wf`](../../../../lib/std/collections/deque/deque.wf).
It is outside daily correctness CI. `make check` verifies the operation traces;
`make measure` checks them first and writes interleaved timing samples under
`.build/`. Retire this experiment when a maintained successor covers the same
contract and preserves its comparison evidence.

## Contract and criterion before timing

The selected Deque has reference-taking endpoint mutations and an explicit
consuming rebase that creates a genuinely new backing. It does not promise
automatic growth or a zero-copy two-span interface. Compare the actual library
with C using the same header, inline payloads, allocation policy, callback
order, and retained helper boundaries. The additional bulk C control replaces
the element-by-element rebase with at most two contiguous copies, preserving
that conversion's contract. Its difference from the loop C control prices
source composition separately from WF lowering.

The three traces are forward churn (`pop_front`, `push_back`), reverse churn
(`pop_back`, `push_front`), and rebase after a half-length rotation has wrapped
the live window. Churn reuses one allocation. Rebase grows from capacity `n`
to `2*n+1`, preserves logical order, resets the head, drains, and releases both
owners. Every payload word contributes to an order-sensitive checksum. An
independent arithmetic oracle derives logical element order without using a
deque. Timed traces include setup and final cleanup; per-round figures are
amortized whole-trace costs, not isolated operation latency.

The matrix uses lengths 16, 256, and 4096 and 8-byte scalars or 256-byte
`nocopy` inline records. Correctness adds empty, singleton, and irregular
lengths, zero rounds, and wrapping arithmetic seeds. Allocation counts,
requested bytes, peak bytes, and final zero live bytes must match before a
timing is accepted. Normal and retained modes save optimized WF/C IR; retained
helper calls must survive. Element transfers are read from those artifacts
separately from layout and timing. No universal parity threshold or application
frequency is assumed.

A separate `setup-cleanup` measurement uses zero churn rounds and repeats
`32768 / count` complete allocate/fill/drain/free traces per sample. This
prices construction, initial materialization and cleanup together. Rebase at
zero rounds allocates nothing and is not used as a construction baseline.
Setup timings are not subtracted from whole traces to claim isolated operation
latencies.

## Source admission observations

The first caller selected direction inside each iteration:

```wf
let removed = if path != 0_u64 {
  let value = deque_pop_back::<T>(values: &values);
  invariant popped: values.inner.len + 1_u64 == count;
  give move value;
} else {
  let value = deque_pop_front::<T>(values: &values);
  invariant popped: values.inner.len + 1_u64 == count;
  give move value;
}
DequeElement::accept(env: &digest, value: move removed);
let next = base +wrap index;
let value = DequeElement::make(seed: next);
if path != 0_u64 {
  let length = deque_push_front::<T>(values: &values, value: move value);
  invariant restored: values.inner.len == count;
} else {
  let length = deque_push_back::<T>(values: &values, value: move value);
  invariant restored: values.inner.len == count;
}
```

The v0.63 baseline compiler rejected `restored` under INV-1 after both
`popped` assertions were proved. Without those assertions it instead rejected
the enclosing loop's same-length backedge invariant. Independent reduction
classified this as a limit of the written proof formulation: ENT-5's counted
continuing kill removes the earlier equality, and the loop headers publish
affine relations over immutable measure images. Each branch mutation creates
a different image; the same-spelled `popped` assertions do not become one
canonical relation under ENT-6's intersection and INV-1's capture rule. A
direct common equality available to the automatic equality family is a
different case. This does not establish a general Deque interface limit.
The current caller chooses its immutable direction before the complete loops,
removing that proof join. The C traces use the same hoist; endpoint calls,
payload construction and callback order are unchanged. No additional runtime
validation or fake fallback was added.

## Results

The corrected baseline has 6,336 accepted timing samples in
[`measurements.csv`](measurements.csv), preserved at checkpoint `5a5481b1`
before the merge of main `7127bcb6`. Both normal and retained executables
passed 432 correctness configurations, each through all three implementations:
2,592 executions in total. Every execution checked the independent checksum,
allocation requests, requested/peak bytes and final zero live bytes. Retained
optimized IR contains 20 WF helper call sites and 40 C sites across the two
controls.

### Storage and allocation

All implementations allocate a 24-byte header and inline payloads of stride
8 or 256 bytes. Churn has exactly one allocation of `24 + n*stride` requested
and peak bytes. Each rebase trace round allocates both the initial capacity
`n` and replacement capacity `2*n+1`: its requests are `2*rounds`, requested
bytes are `rounds * (48 + (3*n+1)*stride)`, and peak bytes are one such pair
(zero for zero rounds). Setup repetition multiplies requests and total bytes,
not peak bytes. Observer headers and allocator metadata are excluded equally.
These formulas are checked independently for every accepted sample.

### Amortized trace costs

The time column is WF nanoseconds per logical endpoint step or rebased element
at `count = 4096`, including setup and cleanup. Ratio ranges cover all three
counts and the four run/cohort medians. Values above 1 mean WF took longer.
Ranges describe measured cohorts, not confidence intervals.

| Mode | Payload bytes | Trace | WF ns/step, n=4096 | WF / loop C | WF / bulk C |
| --- | ---: | --- | ---: | ---: | ---: |
| normal | 8 | forward churn | 3.204 | 2.283–2.341 | 2.283–2.341 |
| normal | 8 | reverse churn | 3.571–3.723 | 1.175–1.220 | 1.188–1.219 |
| normal | 8 | wrapped rebase | 4.883 | 1.650–1.943 | 1.669–2.672 |
| retained | 8 | forward churn | 4.791–4.944 | 1.000–1.015 | 0.993–1.015 |
| retained | 8 | reverse churn | 5.157–5.310 | 1.074–1.088 | 1.064–1.101 |
| retained | 8 | wrapped rebase | 8.209–8.270 | 1.051–1.082 | 1.093–1.183 |
| normal | 256 | forward churn | 34.363–34.576 | 0.975–1.011 | 0.981–1.000 |
| normal | 256 | reverse churn | 34.393–35.034 | 0.982–1.005 | 0.984–1.007 |
| normal | 256 | wrapped rebase | 47.150–47.363 | 0.985–1.007 | 1.018–1.086 |
| retained | 256 | forward churn | 38.177–38.452 | 0.956–1.017 | 0.959–0.993 |
| retained | 256 | reverse churn | 38.055–38.452 | 0.939–0.976 | 0.939–0.981 |
| retained | 256 | wrapped rebase | 57.190–59.448 | 0.953–0.975 | 0.978–1.030 |

The wide-record workload is competitive under both boundary treatments;
scalar normal-mode churn and rebase are not at native parity. The scalar
rebase loop/bulk comparison exposes a separate source-composition opportunity,
but these complete traces do not price just the copy phase. Keeping the
explicit consuming rebase is useful as a library capability without claiming
an efficient automatic growth policy or a borrowed two-span interface.

### Zero-round setup and cleanup

Times are WF microseconds per allocate/fill/drain/free trace. Ratios span all
three counts. Cleanup visits every payload word through the checksum callback.

| Mode | Payload bytes | n=16, us/trace | n=256, us/trace | n=4096, us/trace | WF / loop C |
| --- | ---: | ---: | ---: | ---: | ---: |
| normal | 8 | 0.047–0.048 | 0.539–0.562 | 8.500–8.875 | 1.702–2.000 |
| retained | 8 | 0.067 | 1.211–1.219 | 19.625 | 0.958–1.000 |
| normal | 256 | 0.545–0.549 | 8.656–8.914 | 141.375–148.000 | 0.961–1.006 |
| retained | 256 | 0.686–0.693 | 10.805–10.953 | 175.625–181.375 | 0.957–0.992 |

### Optimized transfers and scalar loop evidence

| Retained record path | WF copies per element | Loop C copies per element |
| --- | --- | --- |
| Pop front or back | 1 x 256-byte memcpy | 1 x 256-byte memcpy |
| Push front or back | 1 x 256-byte memmove | 1 x 256-byte memcpy |
| Rebase transfer loop | 1 x 256-byte memcpy | 1 x 256-byte memcpy |
| Drain to consumer | 1 x 256-byte memcpy | 1 x 256-byte memcpy |

The WF retained functions are `wf_deque_pop_front$instance$50`,
`wf_deque_push_back$instance$51`, `wf_deque_rebase$instance$52`,
`wf_deque_drain$instance$53`, `wf_deque_pop_back$instance$55`, and
`wf_deque_push_front$instance$56`. Neither retained trace adds another record
bulk copy at the endpoint boundary. Bulk C uses at most two dynamic extent
copies during rebase. WF memory operands often have alignment 1 where native
C has alignment 8; the generated ordinary WF ABI and C's internal fastcc
helpers also differ. Equal copy counts therefore do not imply equal lowering.

The scalar forward gap persists after the instrumentation correction and
reversed-order repeat. In normal optimized WF IR,
`wf_deque_library_trace$instance$37` block `bb32` has four i64 loads (head,
capacity, payload, length) and four stores (decremented length, advanced head,
payload, restored length) per hot-loop iteration. In optimized C,
`word_loop_library_trace` block `272` keeps header state in SSA, performs one
payload load and one payload store, and commits the header after the inner
loop. Both hot loops are scalar; SIMD vectorization is not their difference.
C's initial fill does vectorize, while WF retains per-element descriptor
traffic there too. Correct allocator information permits more source-header
hoisting in rebase, but destination-header traffic remains in WF's loop.

This is a concrete lowering improvement opportunity, not an attribution of
every nanosecond to four stores. Normal versus retained changes call visibility,
inlining and native conventions as well as transfers; it is not a pure
copy-cost experiment. The ordinary reference-taking interface can stay while
that issue is tested, and the measured baseline must remain available for the
same-source comparison.

A subsequent controlled IR probe isolated the relevant optimization fact for
this scalar specialization. Starting from the corrected normal LLVM, it either
added only `nuw` to the four dynamic u64 Ring payload GEPs, or split those same
GEPs into a header projection and payload indexing without adding any facts.
After Apple Clang 21 O2:

| IR variant | Scalar forward hot-loop loads / stores | Independent correctness executions |
| --- | --- | ---: |
| Baseline | 4 / 4 | 1,296 passed |
| Unsigned non-wrapping GEP fact only | 1 payload load / 1 payload store; header state in SSA | 1,296 passed |
| Split GEP without new fact | 4 / 4 | 1,296 passed |

The fact-only variant also permits initial-fill vectorization. It identifies
an omitted fact sufficient to eliminate this specialization's metadata traffic;
it supplies no measured timing recovery percentage. Construction and all three
oracle checks took 2.26 seconds under the shared guard. Production lowering is unchanged. A
general improvement still needs qualification over all admitted storage/index
domains, including zero-sized elements, a compatible path for older LLVM,
and the same-source timing matrix. The bounded positive-stride u64 probe does
not establish that those other cases may receive the flag.

The maintained optional reproduction is
`make -C research/experiments/container-representation/deque-library probe-scalar-gep`,
under the shared guard. It refuses drift from the exact four selected u64
GEPs, saves the optimized IR variants and their oracle results under
`.build/`, and requires a local Clang that accepts GEP `nuw`. It is outside
both daily checks and the ordinary benchmark targets. It changes no compiler
emission. The target was added after baseline timing: the measured Makefile
hash below predates it; the original three-variant target's Makefile was
`1facd9eafce1baabac65e456df6ba962b46abd45a944af4eeeaf5f0afbd4e020`.

#### Nonnegative-index assumption candidate

The next bounded probe adds a fourth variant to the same target. Immediately
before each of the same four positive-stride u64 payload GEPs, it inserts
`icmp sge i64 <actual normalized address index>, 0` and
`call void @llvm.assume(i1 <comparison>)`, leaving the original `inbounds`
GEP intact. One intrinsic declaration is added. The target requires exactly
four replacements and refuses an input already containing `llvm.assume` or
the probe's temporary-name suffix. This is a transformation of a fixed
baseline module, not a general lowering implementation.

The 2026-09-22 run used the unchanged Deque library, workload and C driver
identified in this report, and the v0.65 baseline compiler corresponding to merged
`1b916975`, SHA-256
`0ab0f5730590828c511d0a0d4d90d3131654377ea020e5634f5a0800ff9edb96`.
Its raw WF output is
`a32c7d14640f6f855f67149281fa839c65d3d30597c96bc50d8de8e012fa32b6`.
The extended Makefile is
`c5d94d9e71160a37894d11f3deca02b2ec659b8a017a1543646a3d739b9c77bc`.
Reproduction pins that baseline compiler rather than a future compiler that
may already emit the assumption:

```sh
BASELINE_WHITEFOOTC=/path/to/baseline/whitefootc
perl .github/run-check.pl ring-gep-portable-probe \
  make -C research/experiments/container-representation/deque-library \
  probe-scalar-gep \
  WHITEFOOTC="$BASELINE_WHITEFOOTC" \
  CLANG=/usr/bin/clang
```

Set the variable to a saved compiler with the recorded baseline identity. Apple
Clang 21.0.0 (`clang-2100.3.34.2`) on the recorded arm64 host optimized,
linked and executed all four variants. Each passed 432 configurations and
1,296 independent checksum/allocation-ledger executions, for 5,184 executions
in total. No timing samples were collected. Probe construction and correctness
took 4.07 seconds, including rebuilt runtime objects; the complete shared
guard interval took 4.56 seconds including assembly generation. Assembly was
generated directly from each transformed input with
`clang -O2 -Wno-override-module -S -x ir`, matching the native executable's
input rather than reoptimizing the already optimized module.

Here `trace` means `wf_deque_library_trace$instance$37`, and the hot loop is
its forward-churn `bb32`. Branch counts are static instruction counts, not
dynamic branch frequencies. The full trace includes fill, reverse churn,
rebase and cleanup as well as that hot loop.

| Variant | Hot-loop i64 loads / stores | Optimized IR assume calls, trace / hot loop | AArch64 trace calls / conditional branches | AArch64 hot-loop calls / branches |
| --- | --- | --- | --- | --- |
| Baseline | 4 / 4 | 0 / 0 | 6 / 24 | 0 / 1 |
| GEP `nuw` | 1 / 1 | 0 / 0 | 6 / 30 | 0 / 1 |
| Split only | 4 / 4 | 0 / 0 | 6 / 24 | 0 / 1 |
| Nonnegative-index `assume` | 1 / 1 | 22 / 1 | 6 / 30 | 0 / 1 |

The assumption variant retains 34 intrinsic calls across the optimized
module, but none becomes an assembly call or a branch checking the sign.
The two fact-bearing variants have identical forward hot-loop instructions:
one payload load, one payload store, no call, and one loop backedge branch.
Both also allow initial-fill vectorization; their scalar trace has eight
`<2 x i64>` store instructions, versus none in the baseline or split control.
Their additional trace branches come with the same vectorized/loop forms,
not an extra assumption check. The complete scalar functions are nevertheless
different: the `nuw` variant has 310 machine instructions and the assumption
variant 305, with other-path register allocation and scheduling differences.
This is not a whole-function equivalence or performance claim. The record
trace's complete assembly is equal after removing comments and blank lines
(481 instructions), since this bounded probe does not change record payload
addresses.

| Variant | Optimized LLVM SHA-256 |
| --- | --- |
| Baseline | `912fa2d161292a9d600f423d582ccbb3a20ba18606096b3dc55ebc86caa856c9` |
| GEP `nuw` | `a59d9fec4de6587daff422e9aca58590bb98b40fdde1c2137eae8a205e256116` |
| Split only | `2a8736557f16554218531368eb22d429a95225045bbf044fb3a9ca64070e2f7d` |
| Nonnegative-index `assume` | `a06f39c34ed65e80cd1c6fd1eaeea72668da4020771a588ad5ea85b4b817fb13` |

This evidence selects the normalized-index assumption as the production
candidate to evaluate: it communicates the useful local fact without
requiring newer GEP-flag syntax or making emitted syntax depend on a
build-time compiler that may differ from the actual IR consumer. It does not
establish that the two candidates have equal full-matrix cost. The target's
four-case `nuw` comparison still needs a recent LLVM; this run does not test
LLVM 18 or Apple Clang 15 compatibility of production output. The paired
production comparison below follows this probe; actual older-consumer
qualification remains a separate obligation. General padded-header/extent/index
qualification needs its own proof; this four-site experiment does not
establish it. These selected u64 sites do not qualify every alignment,
zero-stride step or maximum admitted index, and the preserved historical
CSV files are not measurements of this candidate.

#### Paired production lowering matrix, 2026-09-22

The production candidate in `302845b8` emits the nonnegative-index assumption
after ordinary target qualification. It is compared with the saved v0.65
compiler above, using the same library, workload, C source, seeds, allocator
observer and unchanged `check`/`measure` targets. The baseline compiler source
tree at `1b916975` is identical to `4e0903e9`, which produced that saved
compiler. The candidate compiler SHA-256 is
`9ad894ff6733c5e3efb489e98544bfcc61a5716dee5b976ac68d02713991a182`.
No source rule, container interface or payload representation changes in
this comparison. Its selection criterion was recorded before timing in
[X1's address investigation](../../../investigations/containers-and-resources/X1-LIBRARY.md#ring-payload-address-qualification).

Four independent build directories ran in the order baseline-A, candidate-A,
candidate-B, baseline-B. All construction preceded those four executions.
Within each execution the existing driver still reverses normal/retained
order, reverses C/WF cohort order, and rotates the first implementation over
eleven samples. All 10,368 correctness executions passed, including checksum,
exact request/byte/peak ledgers and zero live bytes. All 25,344 timing rows
have matching independent checksums and exact ledgers. Timed allocations
succeed; these rows do not inject allocation refusal or replace the formal
tests of the language's resource-abort outcome.

The compressed [paired samples](measurements-ring-addressing.csv.gz) add
`experiment,build,sweep,order` to the unchanged fifteen-column CSV.
`experiment=production` contains the 25,344 main-matrix rows; removing those
four columns reconstructs each original CSV byte-for-byte. Each main file
contains 576 groups of eleven samples. The later placement discriminator
adds 6,336 rows with `experiment=placement`, described below; these are not
pooled into the main table. For each matched run/cohort/mode/payload/path/
capacity, first take the eleven-sample median; then compute candidate/baseline.
The normalized ratio is `(candidate WF / candidate loop-C) /
(baseline WF / baseline loop-C)`. Each table row summarizes 24 such ratios
(two sweeps, two runs, two cohorts, three capacities), without weighting by
an assumed application mix. Entries are median [minimum, maximum]. Values
below one favor the candidate. The final column is the median candidate
WF/loop-C cost, not a transfer-only or whole-language parity measure.

| Mode | Payload bytes | Path | Candidate/baseline | Normalized candidate/baseline | Candidate/loop-C |
| --- | ---: | --- | --- | --- | ---: |
| Normal | 8 | Forward churn | 0.526 [0.495, 0.562] | 0.533 [0.515, 0.552] | 1.233 |
| Normal | 8 | Reverse churn | 0.991 [0.943, 1.064] | 0.991 [0.972, 1.018] | 1.206 |
| Normal | 8 | Wrapped rebase | 0.753 [0.708, 0.801] | 0.753 [0.731, 0.778] | 1.414 |
| Normal | 8 | Setup/cleanup | 0.700 [0.604, 0.735] | 0.713 [0.614, 0.735] | 1.380 |
| Normal | 256 | Forward churn | 0.998 [0.960, 1.040] | 0.999 [0.988, 1.008] | 0.993 |
| Normal | 256 | Reverse churn | 1.000 [0.994, 1.055] | 1.000 [0.995, 1.019] | 0.998 |
| Normal | 256 | Wrapped rebase | 0.975 [0.949, 1.010] | 0.974 [0.958, 1.000] | 0.972 |
| Normal | 256 | Setup/cleanup | 0.989 [0.956, 1.045] | 0.988 [0.975, 1.013] | 0.960 |
| Retained | 8 | Forward churn | 1.006 [0.975, 1.073] | 1.000 [0.971, 1.022] | 1.007 |
| Retained | 8 | Reverse churn | 1.071 [1.037, 1.130] | 1.071 [1.048, 1.088] | 1.087 |
| Retained | 8 | Wrapped rebase | 0.998 [0.907, 1.037] | 0.996 [0.912, 1.012] | 1.058 |
| Retained | 8 | Setup/cleanup | 0.975 [0.927, 1.006] | 0.987 [0.921, 1.000] | 0.987 |
| Retained | 256 | Forward churn | 1.004 [0.946, 1.044] | 1.002 [0.982, 1.024] | 0.986 |
| Retained | 256 | Reverse churn | 0.999 [0.959, 1.055] | 1.001 [0.962, 1.033] | 0.955 |
| Retained | 256 | Wrapped rebase | 0.982 [0.942, 1.026] | 0.979 [0.925, 0.989] | 0.944 |
| Retained | 256 | Setup/cleanup | 1.011 [0.970, 1.040] | 1.005 [0.992, 1.017] | 0.971 |

The main scalar forward result repeats at both larger capacities in both
version orders. At 256 elements, baseline WF/loop-C is 2.302–2.349 and the
candidate is 1.209–1.238; at 4096, these are 2.280–2.304 and 1.204–1.261.
Scalar `wf_deque_library_trace$instance$37`'s optimized forward loop `bb32`
changes from four i64 loads and four stores to one payload load and one
payload store. Its header values remain in SSA until the loop exits. The
remaining approximately 20–26% gap against C is not attributed by this
comparison. Retained forward results near one show that this is primarily
an inlined-path optimization, not a necessary reference-call transfer cost.

There is also a repeatable adverse result: retained scalar reverse churn
is slower in every matched group. Its normalized median is 1.072 in sweep A
and 1.071 in the reversed sweep B. Per-capacity normalized ranges are
1.072–1.088 at 16, 1.053–1.086 at 256, and 1.048–1.070 at 4096.
Disassembly of the measured retained executables finds identical instruction
sequences in scalar `pop_back$46` (12 instructions), `push_front$47`
(15 instructions), and `trace$37` (130 instructions, with local branch
targets compared relative to its entry), as well as the scalar make/accept
helpers. The trace entry is unchanged, but the two endpoint entries move
from `0x100002c54`/`0x100002c84` to `0x100002c34`/`0x100002c64`.
Code placement is therefore a concrete confounder; it has not yet been
isolated as the cause. There is no added instruction or data dependency in
those retained scalar bodies that establishes an intrinsic assumption cost.
This unresolved loss prevents claiming that the predeclared selection
criterion is fully met. It must not be averaged away by the forward gain.

A scratch layout discriminator added only `align 128` to scalar
`pop_back$46` in each retained input, preserving every other input byte.
The two resulting images place `pop_back`/`push_front` at the same addresses,
`0x100002d00`/`0x100002d30`, with the same instructions. However, both also
move the trace and make/accept callbacks by 96 bytes relative to their
respective originals. The trace becomes `0x10000266c`; the callbacks become
`0x100002400`/`0x100002404`. All five functions still have identical
instruction sequences across all four images. This fails the predeclared
control that the original caller position remain unchanged. Construction
took 0.193/0.187 seconds; the guarded audit stopped after 0.53 seconds with
exit 255, before any oracle or timing execution. It produced no timing CSV
and neither establishes nor refutes the placement explanation. The local
`.build/ring-align128-clang21/isolation.txt` audit has SHA-256
`14f1669e875e8de6e9d5993ca5cc62fb2eebe7c214cf1a07ff76141bd64d44f3`.
This experimental alignment is not a production policy.

A final scratch discriminator inserted eight unexecuted AArch64 `nop`
instructions immediately before the candidate's scalar `pop_back` symbol,
after the preceding function's `ret`, without raising section alignment.
Assembly-and-link controls with no padding first matched all 269 function
symbols' addresses and instructions against the original executables. With
the 32-byte gap, the five relevant candidate bodies match the original
baseline's addresses and instructions: both endpoints return to
`0x100002c54`/`0x100002c84`, and the trace and callbacks retain their original
positions. This instrument passed its isolation control and both 1,296-case
native oracles before measurement.

The unchanged retained driver then ran baseline-control A, candidate-pad32 A,
candidate-pad32 B, baseline-control B, collecting 1,584 rows per invocation.
The same eleven-sample medians and loop-C normalization give these scalar
reverse-churn ratios; ranges cover the two cohorts in each sweep:

| Capacity | Sweep A raw | Sweep A normalized | Sweep B raw | Sweep B normalized |
| ---: | --- | --- | --- | --- |
| 16 | 1.072–1.072 | 1.072–1.072 | 1.000–1.000 | 1.000–1.000 |
| 256 | 1.079–1.079 | 1.079–1.079 | 1.007–1.007 | 1.007–1.007 |
| 4096 | 1.057–1.063 | 1.056–1.057 | 1.000–1.000 | 0.994–1.000 |

The loss therefore does **not** collapse in both orders. Loop-C drift is
only 1.000–1.006; the final baseline-control itself becomes slower relative
to C, while the padded candidate remains around 1.088 times C. Across the
twelve pairs the normalized median is 1.031 [0.994, 1.079], but that pooled
number hides the order difference. This does not isolate the original loss
as an endpoint-placement cost. It leaves the approximately seven-percent
production result unexplained; no further instrument was selected here.
The eight inserted instructions are an experiment, not production lowering.

Construction took 0.783 seconds, the two oracles 0.871 seconds, and four
measurement invocations 6.159 seconds; the complete guard took 7.96 seconds
and exited zero. All 6,336 added rows pass the same ledger/checksum/group
checks. They use synthetic `run=0` because the direct driver's fourteen-column
CSV has no outer Makefile run. Removing the four metadata columns and that
synthetic run reconstructs the four driver files exactly. Their identities,
in execution order, are:

| Placement CSV | SHA-256 |
| --- | --- |
| Baseline A | `258289076c757d1157ee2e6b39d4c7f0e14cf3228c77bb090d43c00a7815e7dd` |
| Candidate A | `65e46fb51712c2914e39c0ca2ce4d435f54a2eb57d1272419b44c5f8af5a305c` |
| Candidate B | `61a176e0fbfe938626e2b13d74e38488e8364e51ace8c7b3113cc69260d5fceb` |
| Baseline B | `6815e72005ab437791e166821b5b79070637c3432beba4aee199c76524d1c264` |

The other production-matrix cells do not show a similarly material slowdown
in every matched group. Retained wide setup at capacity 256 has a 1.006–1.016 normalized
ratio, but raw ratios straddle one. All elapsed samples are quantized to
1,000 ns. Small changes near one percent, and wide traces dominated by the
32-word checksum, do not establish a precise gain or parity. Setup figures
include materialization and cleanup; they are not subtracted to manufacture
isolated endpoint latency. Normal/retained differences include inlining,
visibility and ABI effects, rather than isolating copies alone.

For this workload, deleting only the candidate's one `llvm.assume`
declaration, eight `%*.nonnegative = icmp sge i64 ..., 0` instructions and
eight corresponding intrinsic calls makes its **raw** LLVM byte-identical
to the baseline. That comparison does not normalize optimized instructions.
The zero-capacity layout repair does not alter this positive-capacity
workload's emission. A/B native binaries are byte-identical for each version
and mode; their optimized WF files differ only in the `ModuleID` path.
Both C optimized controls are byte-identical in all four builds. Raw hashes:

| Artifact | SHA-256 |
| --- | --- |
| Baseline raw WF | `a32c7d14640f6f855f67149281fa839c65d3d30597c96bc50d8de8e012fa32b6` |
| Candidate raw WF | `5b5b17dc021f3425262827b53102b2575a443195a70162cdf914c7a4c9c9f3d1` |
| Baseline-A CSV | `e0ebeae12a0f306aba09b0bdc4c933bf05d9268086ab29905074eacbcc2de9fe` |
| Candidate-A CSV | `caf683e900ab2e093795f4935c8eae1b9580ca2a8b6167b9d2cfe5c8c74aeb59` |
| Candidate-B CSV | `66eed1aab51f17f5ea53884769e42dd6cca0dc286cb0f3985cc88b01aa85bc67` |
| Baseline-B CSV | `0d22c847124a86cd57dae986e5c96fdef50a254ef24d1984b2851077eca0d811` |
| Paired `.csv.gz`, 31,680 production/placement rows | `4d81bf2d9f4c17ea1f0336b4e2042edd3b2f5746e38f4be26438bcf76bf5e16b` |

Reproduction uses the existing targets, not a second harness. Set
`BASELINE_WHITEFOOTC` and `CANDIDATE_WHITEFOOTC` to the binaries with the
identities above. Under one repository guard, construct the four independent
`BUILD=.build/ring-<version>-<sweep>-clang21` directories using each version's
compiler and `CLANG=/usr/bin/clang`, requesting these six existing targets:

```sh
make -C research/experiments/container-representation/deque-library \
  WHITEFOOTC="$WHITEFOOTC" CLANG=/usr/bin/clang BUILD="$BUILD" \
  "$BUILD/deque-costs-normal" "$BUILD/deque-costs-retained" \
  "$BUILD/deque-library-normal.opt.ll" "$BUILD/deque-library-retained.opt.ll" \
  "$BUILD/control-normal.opt.ll" "$BUILD/control-retained.opt.ll"
```

After all four constructions, invoke the unchanged `measure` target in
baseline-A, candidate-A, candidate-B, baseline-B order, with the same compiler
and build arguments. Retain each `BUILD/measurements.csv` and its identity.
Use `perl .github/run-check.pl <label> /bin/sh -c '<commands>'` around the
whole sequence. This read-only check verifies the packed sample groups,
allocation formulas and cross-implementation checksum agreement; the native
`check` target remains the independent logical-order and live-allocation oracle:

```sh
ruby -rcsv -rzlib <<'RUBY'
path = 'research/experiments/container-representation/deque-library/measurements-ring-addressing.csv.gz'
rows = CSV.parse(Zlib::GzipReader.open(path, &:read), headers: true)
keys = rows.headers - %w[sample elapsed_ns checksum]
groups = rows.group_by { |r| keys.map { |k| r[k] } }
raise 'sample groups' unless groups.values.all? { |g| g.map { |r| r['sample'].to_i }.sort == (0..10).to_a }
checksums = {}
rows.each do |r|
  n, width, rounds, traces = %w[count element_bytes rounds traces].map { |k| Integer(r[k]) }
  expected = if r['path'] == 'wrapped-rebase'
    bytes = 48 + (3*n + 1)*width
    [2*rounds*traces, rounds*bytes*traces, rounds.zero? ? 0 : bytes]
  else
    [traces, (24 + n*width)*traces, 24 + n*width]
  end
  raise 'allocation ledger' unless %w[requests requested_bytes peak_bytes].map { |k| Integer(r[k]) } == expected
  key = %w[element_bytes path count sample rounds traces].map { |k| r[k] }
  raise 'checksum disagreement' if checksums.key?(key) && checksums[key] != r['checksum']
  checksums[key] = r['checksum']
end
puts "#{rows.size} rows: sample groups, allocation ledgers and checksums agree"
RUBY
```

The observed costs below separate construction from the
correctness phase and the following timed-driver phase; the latter two
include their Make/driver overhead.

| Version/order | Construction seconds | Correctness seconds | Timing phase seconds |
| --- | ---: | ---: | ---: |
| Baseline-A | 1.64 | 0.711 | 5.687 |
| Candidate-A | 1.90 | 0.652 | 5.658 |
| Candidate-B | 1.63 | 0.492 | 5.662 |
| Baseline-B | 1.62 | 0.527 | 5.683 |

The complete guarded invocation took 33.55 seconds and exited 2 **after**
the four successful Clang 21 matrices: its subsequent LLVM 22 native
baseline construction failed in `deque-costs-normal` code generation with
`fatal error: error in backend: Unsupported stack probing method` and
`clang frontend command failed with exit code 70`. The generated baseline
LLVM retains `"probe-stack"="__chkstk_darwin"`; it was not removed to obtain
a result. No LLVM 22 native correctness or timing result is claimed.

A separate 0.96-second guarded invocation used LLVM 22.1.8, commit
`ca7933e47d3a3451d81e72ac174dcb5aa28b59d1`, to build only the existing four
optimized-IR targets for baseline and candidate. All eight unmodified WF/C
modules parsed and optimized. Its scalar forward loop likewise changes
from four loads/stores to one each. This qualifies parsing and optimization
only; Apple Clang 21 is the sole native/timing toolchain of this matrix.
Neither result claims local Apple Clang 15 availability or LLVM 18 native
qualification. The historical `measurements.csv` and
`measurements-v0.64.csv` remain unchanged.

### Instrumentation correction

The first adapter renamed `malloc` to an accounting wrapper while leaving
WF's declaration as `declare ptr @wf_cost_allocate(i64)`. Clang inferred
`noalias nonnull` from the C wrapper's body but the separately optimized WF
module lost the allocator information associated with the `malloc` name.
Its rebase then had to consider aliasing between the old and fresh backings.
The provisional timing gap cannot be called an ordinary compiler cost.
The adapter now preserves the wrapper's true fresh, nonnull return contract
with `declare noalias nonnull ptr @wf_cost_allocate(i64)`: the wrapper adds
a private header to a new allocation and exits on failure. No argument alias
attribute or memory-effect promise is added. Independent checksum and exact
allocation formulas are rerun before accepting corrected measurements.

### Reproduction and units

The host is Apple M1 Pro, eight logical CPUs, arm64 macOS 26.6.2 / Darwin
25.6.0, with Apple Clang 21.0.0 (`clang-2100.3.34.2`). The guarded local command
is `make -C research/experiments/container-representation/deque-library measure`.
Normal mode permits O2 inlining. Retained mode marks the library, workload,
make and consume helpers noinline; primitive operations inside rebase/drain
remain ordinary optimized element operations. C endpoint helpers take the
address of the pointer owner, just as WF takes `&Box<Ring<T>>`, and return the
new length where WF does. Both retain the final `free_empty` boundary. C keeps
one external trace wrapper in normal mode and the additional generic trace
boundary in retained mode, matching the WF translation-unit boundary.

Two complete runs reverse normal/retained executable order. Each includes
two cohorts with reversed implementation order and eleven samples rotating
the first implementation. The CSV's `run`, `cohort`, and `traces` fields
preserve those distinctions. Nonzero-round figures divide elapsed time by
`rounds * count` and describe amortized complete churn/rebase traces per
element, including setup and final cleanup. Setup figures divide by `traces`
and describe one complete zero-round trace. Ratios use medians within a
run/cohort. Every word of a wide record enters the checksum, whose cost is
included; close record ratios are not a universal native-parity result.

Incremental experiment construction took 0.55 seconds (WF emission, native
compilation/linking and optimized IR), then `make measure` including its
correctness checks took 6.63 seconds. This excludes a Rust compiler build and
a cold runtime-object build. `CLOCK_MONOTONIC` samples on this host have
microsecond granularity; normalized decimal places are not clock precision.
Ratios around one percent from equality are not a precise gain or parity
claim: quantization, cohort variation and the checksum's substantial share of
record work limit that inference.

The measured compiler SHA-256 was
`4311534e72d2df6ccb2f6043465a801f42f98a70f940f1249755267da2b8c31e`.
The subsequent const-forwarding repair's compiler
`5d3a09f7f07b09ac333615830340878a24daa81ef42d5e61c3a5200098aa84c5`
emitted byte-identical raw LLVM for this source. No specification change is
part of this experiment. Artifact SHA-256 values fix the corrected baseline:

| Artifact | SHA-256 |
| --- | --- |
| `spec/kernel-spec.md` | `2b4df9e688befb4d7ea1a58d7b95dfe56f436ab0739caee9db934763abd21b35` |
| `lib/containers/deque.wf` | `4f4e617e4e33733cea6d57367e1933e0d82a07d3983c47e00fb144e1c812aa82` |
| `deque-library.wf` | `5ba91e4e58c9eddd89c1d781ae1496ba673a6ac684c14890fc331579217344f2` |
| `deque-costs.c` | `85e1d18de6445a76763054b4e5d367a63e61a724593c5a367287b208e186c0bc` |
| `Makefile` | `7fa03e4534e1e3062276b2a40008485d6d1783bcb82171bb901015b4ba54580c` |
| Raw compiler LLVM | `723ab1d7b246e941e8ec4199619068cc9e59171c1ca4d2944cf70f49acce9c08` |
| `measurements.csv` | `42b44b68328776a6f12259c25f1f1ee449b01629cf60b852f56f961197919736` |

### Integration verification at dcbfdc0f

After merging main `7127bcb6`, the freshly built compiler at `dcbfdc0f` had
SHA-256 `9653362a116997dd34558b9da6436dfb343018631b4fabcaa255e420a0c80005`.
The library, workload and C driver retain their baseline source hashes above.
The raw WF output and both normal/retained optimized modules are byte-identical
to the preserved baseline, as are both C controls' optimized IR. Each old/current
optimized WF module was also compiled with Apple Clang 21.0.0 on the same
arm64 host using `clang -O2 -Wno-override-module -S -x ir`; the complete
assembly files compare byte-for-byte equal, without normalization.

| Current artifact | SHA-256 |
| --- | --- |
| Normal optimized LLVM | `fa94c2c058791b408fdff75be414408bd830f54e4bfa532e12ad21325ca61cb0` |
| Retained optimized LLVM | `9a314178b5abc2f1b631f65427bd7ce16b985bfc11b5521138c2ac60391349fd` |
| Normal old/current assembly | `6ffaa7605a55f50d54cf89308f9f12e92cd31e1d1835e8cc7fc3afcb03c3ec4d` |
| Retained old/current assembly | `bbb59f2653b7818e0f78b3ae857c933776e96384b09565b7f3ba1372f28cc096` |

This is emitted-module evidence from optimized IR, not a byte-identity claim
for the complete linked native image. The runtime's C, header and LLVM
sources and `compiler/runtime.mk` were unchanged. Current normal/retained
harnesses passed all 2,592 correctness executions again. Incremental
construction took 0.80 seconds and the executions plus retained-call checks
took 0.73 seconds. No new timings were collected: the original CSV remains
the dated baseline, with no claim of a fresh measurement, a different
toolchain's output, or untested instantiations.

The maintained `probe-scalar-gep` target was executed once in the same guarded
interval, taking 2.13 seconds for construction and checks. Its baseline,
fact-only and split-only variants each passed 1,296 oracle executions; reading
the current scalar `bb32` reproduces 4/4, 1/1 and 4/4 i64 loads/stores. The
target saves `deque-gep-{baseline,nuw,split}.opt.ll` and corresponding
`.check.txt` files under `.build/`. Their optimized-IR hashes are:

| Probe variant | SHA-256 |
| --- | --- |
| Baseline | `50b2c6766470eea22411ad6664d9bc97cbb5835b1ff5bcbeb997f023df490a4a` |
| Unsigned fact only | `5b42af64e26d2717df662a72cb9346b0419b83f9fb50a63eb87802f047591e2c` |
| Split only | `db139c31763151222cbb33b39630699a548eded6192d1ad939c53db028e4a5e6` |

The complete Slab/Deque integration interval took 7.51 seconds. This confirms
the optional reproduction wiring and bounded code-generation observation;
it does not add the flag to production lowering or measure a speedup.

Requalification after main PR #80 used merge `4da1710e` (main `95b21cfd`)
and compiler SHA-256
`cb399df104e975c607973f151dd87a4caf2e4a0e9606a468b4ea69d683345a48`.
The library, workload, C driver, Makefile, runtime inputs and CSV were
unchanged. Fresh raw, normal-optimized and retained-optimized WF LLVM were
byte-identical to the saved `dcbfdc0f` artifacts; both C-control optimized
modules retained their exact hashes. The recorded raw/optimized identities
therefore also identify this fresh emission, without normalization.

The guarded `slab-deque-pr80-emission` command requested these Make targets:

```sh
make -C research/experiments/container-representation/deque-library \
  .build/deque-library-normal.opt.ll .build/deque-library-retained.opt.ll \
  .build/control-normal.opt.ll .build/control-retained.opt.ll
```

Deque emission/optimization took 0.28 seconds; the combined Slab/Deque guard
interval took 1.03 seconds. The C targets were already current. The earlier
native checks, assembly comparison and optional-probe evidence are reused
because their inputs are unchanged. No native relink, execution, probe or
timing was repeated. The original CSV remains historical timing evidence,
with no fresh performance or wider toolchain claim.

#### v0.64 predecessor-lowering comparison

Main PR #88, merged as `ce9a3870` from `e8e1c411`, releases v0.64 and changes
`place_front`'s predecessor calculation to `(head == 0 ? capacity : head) - 1`.
This avoids an overflowing intermediate for large header-only capacities,
but also changes positive-stride scalar and record endpoint code. Unlike the
earlier integrations, this requires a fresh timing comparison. The library,
workload, C driver and controls have not changed. The old
[`measurements.csv`](measurements.csv) remains the pre-v0.64 baseline.

The gate compiler SHA-256 is
`b68db16443f0606ef5d3217014fbdb1bdae0ea01bc23452eff27e2626bbc24ff`;
the active specification is
`bc4d465d698a63518d4c768bfa0b4147afa15e328e32f8adac3f27980c7a21ed`.
The current Makefile is
`1facd9eafce1baabac65e456df6ba962b46abd45a944af4eeeaf5f0afbd4e020`:
its difference from the original measured Makefile is the optional probe,
not the check or measurement matrix.

| v0.64 artifact | SHA-256 |
| --- | --- |
| Raw compiler LLVM | `a32c7d14640f6f855f67149281fa839c65d3d30597c96bc50d8de8e012fa32b6` |
| Normal optimized LLVM | `e36eeb449413e522d28c2faeccb5fedf5e0f28e98de76888a826cda25e08727e` |
| Retained optimized LLVM | `42db1665f36ee102687df07507c7278a51d07af906a08d1357a36d482e5301dd` |
| Normal current assembly | `ab1f40e2a8871e2377b2999d330d9bf910663cda828ac0f30a8c47ed3bc06040` |
| Retained current assembly | `bf028f3e74c9f816827456d4fa6e04f8de5a5ab2671930e3804d8bab65cfe08b` |

Comparison with the saved `dcbfdc0f` optimized modules isolates retained
changes to `wf_deque_push_front$instance$47` (scalar),
`wf_deque_push_front$instance$56` (record), and their primitive bodies
`wf_place_front$instance$63` and `$68`. Their predecessor arithmetic and
address scheduling differ in generated assembly; normal mode contains the
corresponding inlined reverse-churn change. The retained record-copy counts
above are unchanged. The new raw module also contains the POSIX resource
writer's EINTR retry helper, which is removed by optimization under the
accounting allocator's nonnull contract; it is not a timed-path difference.
Both C-control optimized modules and the linked runtime's C/header/LLVM
sources and `compiler/runtime.mk` are unchanged. The assembly comparison
uses the same optimized-IR-to-arm64 command as above and concerns the emitted
WF module, not the complete linked image.

The current scalar forward hot block still has four i64 loads and four stores.
The maintained probe still structurally selects exactly four positive-stride
u64 Ring payload GEPs. The historical `dcbfdc0f` probe result is reused, not
rerun or relabelled as a v0.64 execution: the changed predecessor is in the
other endpoint path and does not supply the missing unsigned address fact.
No production GEP flag or container API change is part of this comparison.

Compiler construction with `make -C compiler build` took 46.94 seconds;
Deque emission/optimization took 0.28 seconds. The complete guarded
Slab/Deque build, emission and conditional assembly comparison took
48.38 seconds. These construction figures are separate from the native
check and timing run below.

The new [`measurements-v0.64.csv`](measurements-v0.64.csv), collected on
2026-09-22 on the same host/toolchain, contains all 6,336 samples from the
unchanged matrix. Its SHA-256 is
`e297a90a7ce0f56f116c9697aece4f10dcf0a4d6c855e97e0a7b1cb89db26638`.
Reproduce through the existing target, then preserve its output separately:

```sh
perl .github/run-check.pl deque-v64-measure \
  make -C research/experiments/container-representation/deque-library measure
cp research/experiments/container-representation/deque-library/.build/measurements.csv \
  research/experiments/container-representation/deque-library/measurements-v0.64.csv
```

Both modes again passed 2,592 correctness executions in total, and retained
IR still has 20 WF and 40 C helper call sites. Every timing row passed the
same checksum and allocation ledger checks. Incremental native construction
took 0.58 seconds. Observing the existing `make measure` output boundary
after its retained-call check separates 0.790 seconds for correctness from
5.740 seconds for the timing matrix, including each phase's Make/output
overhead; total check plus measurement was 6.529 seconds. The whole guarded
construction/check/measurement/copy interval took 7.14 seconds.

The following table uses the same units and per-run/cohort median method as
the original table. It is a new complete cohort, not a replacement of the
historical values or an isolated instruction-latency comparison.

| Mode | Payload bytes | Trace | WF ns/step, n=4096 | WF / loop C | WF / bulk C |
| --- | ---: | --- | ---: | ---: | ---: |
| normal | 8 | forward churn | 3.204 | 2.256–2.405 | 2.283–2.349 |
| normal | 8 | reverse churn | 3.601–3.754 | 1.196–1.230 | 1.194–1.240 |
| normal | 8 | wrapped rebase | 4.883–5.066 | 1.648–1.909 | 1.648–2.683 |
| retained | 8 | forward churn | 4.791–5.127 | 0.994–1.035 | 1.000–1.028 |
| retained | 8 | reverse churn | 5.157–5.554 | 1.032–1.158 | 1.013–1.158 |
| retained | 8 | wrapped rebase | 8.240–8.392 | 1.028–1.098 | 1.089–1.201 |
| normal | 256 | forward churn | 34.363–34.637 | 0.978–1.026 | 0.987–1.014 |
| normal | 256 | reverse churn | 34.424–34.821 | 0.986–1.008 | 0.977–1.006 |
| normal | 256 | wrapped rebase | 46.997–48.676 | 0.976–1.025 | 0.993–1.108 |
| retained | 256 | forward churn | 38.086–39.795 | 0.948–0.991 | 0.952–0.992 |
| retained | 256 | reverse churn | 37.994–38.422 | 0.932–0.973 | 0.937–0.979 |
| retained | 256 | wrapped rebase | 57.159–58.472 | 0.919–0.989 | 0.977–1.041 |

The zero-round setup/cleanup cohort is preserved too; times are microseconds
per complete trace, with no subtraction from the operation traces:

| Mode | Payload bytes | n=16, us/trace | n=256, us/trace | n=4096, us/trace | WF / loop C |
| --- | ---: | ---: | ---: | ---: | ---: |
| normal | 8 | 0.047–0.048 | 0.539 | 8.500 | 1.690–1.971 |
| retained | 8 | 0.067–0.069 | 1.211–1.273 | 19.625–21.250 | 0.959–1.024 |
| normal | 256 | 0.546–0.565 | 8.625–8.695 | 142.250–143.875 | 0.942–0.996 |
| retained | 256 | 0.689–0.711 | 10.852–10.906 | 175.875–181.500 | 0.948–0.970 |

The changed reverse path still has a scalar normal-mode gap (1.196–1.230
against loop C, previously 1.175–1.220), while record traces remain
competitive in this matrix. The unchanged scalar forward code still costs
2.256–2.405 times loop C. These ranges do not isolate a causal percentage
for the predecessor rewrite: the old and new cohorts ran at different times,
retained scalar reverse results vary more in the new cohort, and the samples
still have one-microsecond granularity. The evidence supports retaining the
same lowering-improvement questions, not asserting a speedup, regression
threshold or general native parity from small ratio differences.

Requalification after main PR #87 used merge `fd41dcc8` (main `8d6da723`),
which releases v0.65, and compiler SHA-256
`0ab0f5730590828c511d0a0d4d90d3131654377ea020e5634f5a0800ff9edb96`.
Fresh normal and retained optimized WF modules compare byte-for-byte equal
to the saved v0.64 artifacts in the table above, without normalization;
both C-control modules are unchanged too. Library, workload, harness,
native runtime inputs and both CSVs retain their identities. Gate-profile
compiler construction took 46.75 seconds; Deque emission/optimization
through the same four Make targets took 0.27 seconds, and the shared
`slab-deque-v65-emission` interval took 1.14 seconds. This confirms emitted
WF module correspondence on the recorded Apple Clang 21 / arm64
configuration. No native relink, execution, probe or timing was repeated;
the v0.64 CSV remains the dated measured cohort, not a fresh v0.65 result.

### Next scalar reverse-churn discriminator: precise endpoint effects

The current optimized scalar reverse loop reloads `cap` and `head` around the
`pop_back`/`push_front` pair even though the endpoint contracts leave capacity
unchanged. The next source-only diagnostic narrows `deque_push_front` from
`writes(values.inner)` to the fields it actually changes: `head`, `next` and
`len`. Its algorithm, ownership transfer, public arguments, and checksums stay
unchanged; `deque_pop_back` is untouched. The hypothesis is that the narrower
row lets the emitter or LLVM retain the invariant capacity across the pair.

Before timing, the rebuilt candidate must pass the full Deque correctness and
allocation images. The optimized scalar reverse loop must show at least one
fewer capacity load per inner iteration, with no new call, spill, frame growth,
or extra payload transfer; forward, wide, rebase and cleanup bodies must be
unchanged except for symbol/hash renaming. A code or ledger mismatch stops
without timing. If the code criterion passes, run the complete ecosystem
matrix with the existing 1 ms/10% stability qualification; any useful-cell
regression rejects the row change. This experiment measures whether effect-row
precision exposes an existing invariant; it does not amend the specification
or promise a new operation.
