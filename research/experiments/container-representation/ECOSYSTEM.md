# Rust and C++ container comparison

## Question and scope

How does the current Whitefoot library perform against ordinary production
Rust and C++ containers when they complete the same application task, and
which observed differences deserve the next investigation?

The comparison was framed after the container delivery at `0f22b026b`; its
implementation now incorporates main `ad51e05df`. No timings were published
against the earlier baseline. Before timing, the benchmark implementation
was frozen at source revision
[`0c3203aa6111f14247aa950e3794e83082d4f29c`](https://github.com/mbbill/Whitefoot/tree/0c3203aa6111f14247aa950e3794e83082d4f29c),
which identifies the timing and allocation executions. Later report,
data-packaging and reducer commits do not change that identity. The map
replay relinked after removing one trailing
blank line in its shared oracle; that recorded source-file correction changes
no behavior. The comparison selects no language amendment
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
  counts, requested bytes, and peak live requested bytes. A requested-byte
  peak is not process RSS or allocator-resident memory.
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
The rebuilt current gate compiler SHA-256 is
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

- **Hash misses and table policy:** compare the same source at lower occupancy,
  recording native capacity geometry. The large aligned-hash miss gap persists
  while WF takes 0.76–0.79× direct C across both payloads. Probe behavior and
  occupancy need a discriminator before assigning this gap to WF lowering.
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
