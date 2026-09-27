# Owning map library costs

This explicit experiment belongs to the generic owning-map trial in
[X1-LIBRARY.md](../../../investigations/containers-and-resources/X1-LIBRARY.md#generic-owning-map-trial-after-the-ring-comparison).
It stays outside daily correctness CI. The first maintained caller is
`make -C research/experiments/container-representation map-native-check`,
which checks the native controls only. `map-check` also executes the source
candidates and owning-child callers in three CLI configurations, checks their
exact allocation identities with the existing formal observer, and compares
WF/C trace contents and allocation totals. `map-measure` is the explicit
timing caller; it is not a daily gate. The original eight-path source/control
checks and first paired timing matrix split the representation tradeoff:
sparse lookup/edit and dense wide rebuilding won different traces. That
original stage reopened the ordinary single-slot direct-migration alternative
without selecting a library representation. The later
[constant-interface comparison](#constant-interface-comparison-and-selected-helper-body)
selected the maintained enum-bucket library with a shared exchange helper and
compact returned-pair result. Its remaining initialization cost is now the
consumer for the [same-source compiler comparison](#same-source-inactive-storage-lowering-comparison).
Keep this experiment while it owns these comparisons; remove it when a
maintained successor preserves the same contracts and evidence.

## Current Rust and C++ ecosystem comparison

The explicit `ecosystem-*` targets implement the separate
[ecosystem comparison contract](../ECOSYSTEM.md). Their sources are
[the current Whitefoot fixture](map-library.wf), [Rust adapter](map-ecosystem.rs),
[C++ adapters](map-ecosystem.cpp), and [driver](map-ecosystem.c).
The driver and historical C controls share [the key-ID oracle](map-oracle.h).
The ordinary direct sparse C control is rebuilt from [map-costs.c](map-costs.c)
with two whole-trace exports, reusing its existing algorithm. Its rows are
labelled `c-sparse-direct`: an attribution control with the supplied mix64
protocol, not a native default-map member or a clone of Whitefoot's source.
The measured source revision is `0c3203aa6111f14247aa950e3794e83082d4f29c`.
Both ordinary and accounting images passed 18,390 independently checked traces;
the corrupted checksum, simulated unreleased allocation, and eight omitted
reserve controls failed with the expected reasons. The
[baseline timing samples](ecosystem-samples.csv) contain 9,240 data rows, and
the [allocation observations](ecosystem-accounting.csv) contain 420. Every
allocation row balances requests and releases and ends with zero live bytes;
its checksum matches sample zero in both baseline timing cohorts. The historical
timings below are separate observations at their recorded source revisions.

The measured capacity/population pairs are 3/2, 64/56 and 4096/3584, each with
an 8-byte scalar or a move-only 256-byte inline value. The application observes
lookup hit/miss, the complete displaced value on replacement, remove/miss/put
churn, in-place first-word editing, fill/free, and reserve for more entries.
All final values contribute to an order-independent digest before cleanup.
No reference escapes an operation, and neither reference stability nor
iteration order is an application requirement. Inline width is not evidence
about nested owning values.

Native capacities, bucket counts, load limits and growth remain the libraries'
ordinary policies. The capacity column is an application request, not a claim
of equal bucket capacity. Rust uses `with_capacity_and_hasher` and reserves
additional entries relative to length; C++ uses `reserve` with a total entry
count. Whitefoot reserves an exact bucket count. Each wrapper enforces the
same logical entry ceiling; replacement at that ceiling succeeds and only a
missing insertion returns its offered value. Entry/`try_emplace` paths avoid
a preliminary lookup below the ceiling. The untimed ceiling witness fixes
capacity and population at three and consumes both replaced and refused
values. Equal `u64` keys are indistinguishable, so native retention of the
stored key matches this trace without establishing equivalence for distinct,
comparator-equal owning keys.

`native-default` uses Rust `RandomState`, `std::hash<u64>`, and Abseil's default
hasher. Whitefoot's library requires a supplied protocol; its salted mix64
protocol is explicitly labelled in both series. `aligned-hash` supplies that
same mix64 calculation to every implementation, with native layouts and load
policies still intact. Forced collisions occur only in aligned correctness
checks. Changing hash protocol also changes bucket distribution and hash-state
construction; a default/aligned timing difference is not an isolated cost of
hash instructions. Rust has no portable same-capacity rehash API, so that
historical path is excluded from this common ranking.

Reserve measurements request more entry capacity and verify existing values;
they do not insert the additional population or measure a growth pause.
Untimed reserve witnesses observe the public capacity floor. Eight negative
controls deliberately omit reserve for each implementation/payload at 3/2;
each must fail, so prior overallocation cannot silently make this witness
vacuous. A corrupted digest and a simulated live allocation must also fail
their respective checks with the expected reason.
The C attribution control participates in the seven ordinary paths and their
oracle/accounting checks; its existing algorithm has no new ceiling or
reserve-diagnostic entry path and is excluded from those added witnesses.

Normal images use Clang `-O3`, Rust `opt-level=3`, ordinary allocation and a
single C ABI call per complete trace. Operations have no forced helper
barriers. Accounting uses separate images and reports requested allocations,
requested bytes and peak live requested bytes, including native nodes and
backings. Those peaks exclude allocator metadata and do not measure RSS.
The default work count is 262144 item-rounds; reserve and fill/free distribute
that count over complete traces. Each cell has two checked warmups and eleven
rotating-order samples in each of two reversed cohorts. Short or unstable
cells remain inconclusive under the shared measurement criteria.

Set `ABSEIL_PREFIX` to the external pinned Abseil installation. Run build,
correctness, accounting and measurement as separate guarded stages from the
repository root:

```sh
perl .github/run-check.pl map-ecosystem-build make -C research/experiments/container-representation/map-library ecosystem-build ABSEIL_PREFIX="$ABSEIL_PREFIX"
perl .github/run-check.pl map-ecosystem-check make -C research/experiments/container-representation/map-library ecosystem-check ABSEIL_PREFIX="$ABSEIL_PREFIX"
perl .github/run-check.pl map-ecosystem-account make -C research/experiments/container-representation/map-library ecosystem-account ABSEIL_PREFIX="$ABSEIL_PREFIX"
perl .github/run-check.pl map-ecosystem-measure make -C research/experiments/container-representation/map-library ecosystem-measure ABSEIL_PREFIX="$ABSEIL_PREFIX"
```

`WHITEFOOTC`, `ECO_WORK`, `ECO_BUILD`, `ECO_SAMPLE_FILE` and `ECO_ACCOUNT` are
overridable. By default `.build/ecosystem/` owns configuration, ordinary and
accounting images, separate cohort samples, combined `measurements.csv` and
`accounting.csv`. None is part of canonical correctness CI.

The older candidate bindings and traces now live in
[map-candidates.wf](map-candidates.wf), included only by the historical
representation targets. The actual library trace has one owner in
`map-library.wf` and imports `std::collections::hash_map`. Historical patches,
archives and measured source identities below remain unchanged; reproduce
those results from their recorded revisions. This task does not migrate the
older candidate libraries or retired `lib/containers` overlay controls to the
current module and enum-constructor syntax.

### Baseline sample qualification

The initial run used `ECO_WORK=262144`, with two unrecorded checked warmups
and eleven recorded samples per cell in each cohort. The complete timing
phase took 76.333 seconds, separately from construction and correctness.
Among 336 comparator cells across payload, population, path and hash series,
114 include a sample below 1 ms and 14 have a cohort ratio spread above 10%;
seven meet both conditions. Those cells are not ranked. The minimum observed
sample is 351,000 ns. The completed same-source replay below uses `ECO_WORK=1048576` to resolve
short samples and reassess cohort stability. More rounds also reduce the setup
fraction of query/update batches, so ratios across work levels are not a
causal attribution experiment.

The baseline raw sample SHA-256 is
`5a142672b1b5fe6f290adbdfd99f327f27606a4b20a79673363e9ee09c110b5c`;
the accounting SHA-256 is
`0a4729815dc1d2dba8d99fcc0868dff72daeb42910dda834b4b04c35c94b29b4`.
Keep these files with this comparison; a successor may retire them only while
preserving the dated evidence and its protocol.

### Longer replay and large-population results

The [longer replay](ecosystem-replay-samples.csv) has 9,240 data rows at
`ECO_WORK=1048576` and took 307.876 seconds. Its source behavior is unchanged
from the baseline; a trailing blank-line correction in the shared oracle
caused a relink without changing the benchmark. The replay SHA-256 is
`7898181381db9c5422f4244c8aec3a9e991f829be8b2f64d982dc7dd4b0e16fb`.
All observed samples are at least 1,402,000 ns. Fifteen comparator cells still
exceed the 10% cohort ratio-spread criterion: three at population 2, none at
56, and twelve at 3584. They remain unranked; additional replay is not used to
select a preferred answer.

The following tables use only the longer replay at capacity/population
4096/3584. Query, replacement, churn and edit batches execute one full trace
with 292 rounds; fill/free executes 292 full traces, and reserve executes 292
full traces with one reserve each. WF batch milliseconds include construction,
all operations, full-value consumption and cleanup. Each cell is the range of
the two cohort medians, not a confidence interval. Ratios are WF elapsed time
divided by the named comparator's elapsed time: above 1 means WF took longer.
Differences below the shared 10% triage threshold remain descriptive. A dagger
marks an unstable comparison and excludes it from ranking. The final column
is a fresh algorithm attribution control, separate from the native-library
comparison.

#### 8-byte values, native defaults

| Path | WF batch ms | WF/Rust | WF/C++ unordered | WF/Abseil flat | WF/direct C attribution |
|---|---:|---:|---:|---:|---:|
| Hit | 10.33–10.46 | 1.45–1.45 | 5.24–5.58 | 4.97–5.06 | 0.85–0.90 |
| Miss | 34.28–34.34 | 5.13–5.15 | 13.13–14.25 | 18.89–19.00 | 0.77–0.77 |
| Replace old value | 11.17–11.44 | 1.26–1.30 | 2.77–2.87 | 3.39–3.41 | 1.04–1.06 |
| Remove/churn | 98.15–99.16 | 3.14–3.18 | 3.53–3.69 | 6.31–6.43 | 0.80–0.82 |
| Edit first word | 10.74–11.49 | 1.28–1.37 | 4.85–5.77† | 4.64–5.08 | 0.88–0.99† |
| Fill/free | 19.74–19.76 | 1.52–1.52 | 0.74–0.75 | 2.53–2.61 | 0.99–0.99 |
| Reserve more entries | 40.21–40.66 | 1.25–1.26 | 1.29–1.36 | 1.52–1.56 | 1.06–1.12 |

#### 8-byte values, aligned hash

| Path | WF batch ms | WF/Rust | WF/C++ unordered | WF/Abseil flat | WF/direct C attribution |
|---|---:|---:|---:|---:|---:|
| Hit | 9.98–10.72 | 3.74–4.06 | 1.94–2.24† | 3.65–3.90 | 0.84–0.86 |
| Miss | 34.20–34.20 | 13.08–13.13 | 7.31–7.69 | 13.30–13.34 | 0.76–0.76 |
| Replace old value | 11.78–11.80 | 2.67–2.76 | 1.24–1.33 | 2.19–2.25 | 0.99–1.00 |
| Remove/churn | 98.13–98.92 | 6.66–6.80 | 2.30–2.35 | 4.95–4.97 | 0.81–0.82 |
| Edit first word | 11.58–12.68 | 4.00–4.45† | 2.04–2.32† | 3.97–4.35 | 1.00–1.11† |
| Fill/free | 19.92–20.44 | 2.91–3.00 | 0.61–0.64 | 2.03–2.23 | 0.98–1.02 |
| Reserve more entries | 38.98–42.03 | 2.44–2.52 | 0.94–0.99 | 1.42–1.42 | 1.06–1.07 |

#### 256-byte values, native defaults

| Path | WF batch ms | WF/Rust | WF/C++ unordered | WF/Abseil flat | WF/direct C attribution |
|---|---:|---:|---:|---:|---:|
| Hit | 12.27–14.61 | 1.58–1.89† | 5.60–6.69† | 5.64–7.00† | 0.87–1.01† |
| Miss | 40.36–40.62 | 5.85–5.98 | 13.17–13.26 | 19.82–20.81 | 0.78–0.79 |
| Replace old value | 76.52–77.67 | 2.18–2.21 | 3.33–3.36 | 2.82–2.87 | 1.95–2.01 |
| Remove/churn | 159.25–162.03 | 2.73–2.74 | 3.13–3.14 | 4.16–4.24 | 1.04–1.07 |
| Edit first word | 11.38–12.43 | 1.20–1.32 | 4.41–4.82 | 4.97–5.32 | 0.76–0.88† |
| Fill/free | 71.88–71.91 | 1.92–1.92 | 1.29–1.30 | 2.26–2.27 | 1.51–1.51 |
| Reserve more entries | 123.77–124.47 | 1.77–1.78 | 1.96–1.97 | 2.29–2.30 | 1.62–1.63 |

#### 256-byte values, aligned hash

| Path | WF batch ms | WF/Rust | WF/C++ unordered | WF/Abseil flat | WF/direct C attribution |
|---|---:|---:|---:|---:|---:|
| Hit | 13.41–14.42 | 4.38–4.54 | 2.56–2.79 | 4.71–5.19† | 0.91–1.00 |
| Miss | 40.38–40.62 | 14.12–14.65 | 7.17–7.51 | 14.45–14.64 | 0.79–0.79 |
| Replace old value | 76.70–76.73 | 2.79–2.81 | 2.52–2.52 | 2.62–2.63 | 1.95–1.96 |
| Remove/churn | 158.98–160.49 | 4.51–4.55 | 2.24–2.25 | 3.32–3.40 | 1.05–1.05 |
| Edit first word | 11.52–11.69 | 3.36–3.37 | 1.92–1.95 | 3.79–3.84 | 0.74–0.76 |
| Fill/free | 71.65–71.74 | 2.14–2.14 | 1.13–1.13 | 2.21–2.22 | 1.50–1.52 |
| Reserve more entries | 123.99–124.11 | 2.20–2.20 | 1.63–1.63 | 2.17–2.21 | 1.60–1.61 |

The three unstable population-2 comparisons are scalar aligned-hash miss
against C++ unordered, scalar native-default reserve against Rust, and wide
native-default miss against direct C. Large-cell instabilities are marked in
the tables. In particular, the wide native-default hit row is not ranked
against any comparator. All original and replay samples remain available;
the two work levels are never combined into one median.

### Size, hashing and follow-up interpretation

Size changes the practical result. For scalar hits with native defaults,
WF/Rust is 0.37 at population 2, 0.77 at 56, and 1.45 at 3584. With aligned
hashing, the same ratios are 1.04–1.05, 2.10–2.11 and 3.74–4.06. Scalar
replacement against Rust defaults similarly changes from 0.47–0.48 through
0.74–0.75 to 1.26–1.30. These are separately measured outcomes, not a pure
hash-cost decomposition. Small scalar churn is favorable to WF: its C++
ratio is 0.42 with defaults and 0.39 aligned, whereas the small wide churn
ratios are 1.32–1.34 and 1.21. A scalar result does not describe wide owners.

Large misses are the strongest table-policy follow-up. The penalty persists
with aligned hashing: both payloads take 7.17–14.65 times the native elapsed
time across the three libraries and both cohorts. WF/direct-C is instead
0.76–0.79. That control does not identify the cause, but it makes probing,
occupancy and native capacity policy a better first discriminator than a
claim that generated WF code alone causes the complete miss gap. A same-source
comparison at a lower occupancy, with native capacity geometry recorded,
would separate part of that question before changing a library algorithm.

Wide replacement is a separate emitted-code and ownership-transfer question.
Across all populations and both hash series, WF/direct-C is 1.95–2.25;
large wide fill/free is 1.50–1.52 and reserve is 1.60–1.63. The corresponding
large scalar fill/free comparison is near 1.00. Conversely, wide in-place edit
against aligned direct C is 0.74–0.76 at the large population. Allocation
counts and requested bytes match WF and direct C, so request totals alone do
not explain the owned-value path differences. Inspecting normal optimized
transfers, inactive initialization and cleanup is the next bounded question;
any causal attribution still needs a same-source before/after discriminator.
No percentage of the native-library gap is assigned to a mechanism by
subtracting these different implementations' timings.

### Allocation observations

Accounting uses the baseline work count and observes ordinary container
requests in separate instrumented images. Hash choice leaves all recorded
allocation counts and byte totals unchanged in this run. At population 3584,
peak live requested bytes are:

| Value | Phase | Whitefoot | Rust | C++ unordered | Abseil flat | Direct C |
|---|---|---:|---:|---:|---:|---:|
| 8 B | Filled map | 98,320 | 139,272 | 147,456 | 139,184 | 98,320 |
| 8 B | Reserve for more entries | 294,944 | 417,808 | 212,992 | 417,712 | 294,944 |
| 256 B | Filled map | 1,114,128 | 2,170,888 | 1,036,288 | 2,169,312 | 1,114,128 |
| 256 B | Reserve for more entries | 3,342,368 | 6,512,656 | 1,101,824 | 6,510,824 | 3,342,368 |

The filled-map row is shared by the lookup, replace, edit, churn and fill/free
paths. Each complete fill/free trace makes one backing request for Whitefoot,
Rust, Abseil and direct C; C++ unordered makes 3585 requests for its buckets
and nodes. Reserve adds one request in each implementation. The baseline
churn batch makes 265,217 C++ requests while the other four retain one backing
request. All are reclaimed. Native capacity rounding and node/table layout
remain part of these practical outcomes; these bytes do not measure RSS or
prove an allocation-time explanation for elapsed differences.

### Preregistered capacity and memory discriminator

This follow-up is recorded before its measurements, starting from source
`c75520e9d59d74e19ba158e1cae5f394b3a2d874`. The practical comparison above is
retained. Its 4096/3584 point means 4096 materialized Whitefoot slots, but a
request to reserve 4096 entries in each native map. These requests do not
establish equal occupancy or equal memory. No library or compiler change is
part of this control.

An untimed `ecosystem-geometry` target observes filled maps in the accounting
image. It includes the original 3/2 and 64/56 shapes and population 3584 at
requests 3584, 4096, 5120, 6144 and 8192, both value widths and hash series.
Separate fields identify the quantities each API actually exposes:

- Whitefoot's `hash_map_capacity` is the number of materialized probe slots;
  a filled-map diagnostic also checks its public length. These slots can all
  hold entries. A separate filled-map witness reads the direct sparse C
  control's owned capacity and count fields.
- Rust's `HashMap::capacity` is a public lower bound on the entries that fit
  without reallocating. Physical slots and bucket count are left unknown.
- Abseil's `capacity` counts assigned, deleted and empty element slots. Its
  public API does not expose the current guaranteed usable entry capacity;
  the original reserve request supplies only a known lower bound. Its
  compatibility `max_load_factor` result is not treated as a usable-capacity
  formula.
- C++ unordered reports chaining `bucket_count`, `load_factor` and
  `max_load_factor`. A bucket is not an element slot, and the product of
  bucket count and maximum load is reported separately from a public
  capacity accessor. Allocation requests, filled-map live requested bytes,
  cumulative requested bytes and peak bytes are separate observations.

Native observations are compiled only into the accounting image. The
Whitefoot and direct-C capacity diagnostics are separate untimed callers;
the timed trace body has no geometry observer. A missing snapshot, an
incorrect population or implementation identity, inconsistent exposed geometry,
an unmatched allocation, an incorrect setup digest or unreclaimed allocation
must fail. Seven missing/corrupted-observation controls check the new failure
paths; the existing leak control checks reclamation. Header/toolchain
identities are recorded with the outputs; inaccessible native internals are
not reverse-engineered from byte totals and labelled as measurements.

The separate `ecosystem-occupancy-measure` target reuses the existing trace
functions with population fixed at 3584 and requests 4096, 5120, 6144 and
8192. It runs hit, miss, replacement and first-word edit for both payloads,
with aligned hashing, identical seed sequence, `ECO_OCCUPANCY_WORK=1048576`,
two checked warmups and eleven samples in each of two reversed order cohorts.
The second cohort reverses both implementation and capacity order.
All five implementations and all four capacity points are retained. The
3584/3584 geometry witness is not timed: full occupancy creates a distinct
full-table miss scan outside this first bounded discriminator. The original
4096/3584 point is measured afresh in the same sweep image. Its requested
work and complete-trace accounting match the larger-capacity points. Even an
unused diagnostic can change emitted code layout, so this new image supplies
every denominator for the capacity discriminator. Comparing it to the older
practical image is not a causal same-source comparison.

The primary hypothesis is that capacity policy materially contributes to the
large miss cost. For each payload, compare Whitefoot's same-source 4096-slot
and 8192-slot miss traces, with count, keys, hash calculation, seeds and work
fixed. A reduction of at least 10% in both cohorts, with all samples at least
1 ms and cohort ratio spread at most 10%, establishes sensitivity to this
capacity change. Failure of that criterion leaves the proposed contribution
unestablished. This changes occupancy, bucket placement, allocation size and
setup/cleanup together; it cannot assign an isolated percentage to probing
or emitted instructions. Intermediate points and other paths test whether
the effect is specific to misses, without changing the primary criterion.

For a memory comparison, hold each native peer at its original reserve
request of 4096 and choose the Whitefoot sweep point with the smallest
absolute difference in post-fill live requested bytes, separately by payload.
Break a byte-distance tie toward the smaller Whitefoot request. Select these
pairs from geometry before reading timings. A pair is labelled comparable
requested memory only if the larger total is at most 1.10 times the smaller;
otherwise report the mismatch and no matched-memory result. Preserve every
point, the byte ratio, and all native policies. This comparison controls one
resource budget, not table layout, collision behavior or allocation overhead.
Native-default results above remain a separate practical question. A better
controlled point cannot replace an earlier losing or unstable cell.

The new geometry and occupancy outputs use a separate build directory and,
after review, remain beside the existing raw files with their source identity
and qualifications. These modes and their data are retained with this capacity
discriminator, or removed when a maintained successor preserves its evidence.
The untimed geometry and accounting stages have passed; their observations
and the memory-pair selection are recorded below before timing.

Build and run these opt-in stages separately under the repository host guard,
with a frozen baseline `WHITEFOOTC` and the pinned `ABSEIL_PREFIX`:

```sh
for phase in build occupancy-check geometry occupancy-account; do
  perl .github/run-check.pl "map-$phase" \
    make -C research/experiments/container-representation/map-library "ecosystem-$phase" \
      BUILD=.build/geometry WHITEFOOTC="$WHITEFOOTC" ABSEIL_PREFIX="$ABSEIL_PREFIX"
done
```

After recording the closest-byte pairs from the geometry output, run timing:

```sh
perl .github/run-check.pl map-occupancy-measure \
  make -C research/experiments/container-representation/map-library ecosystem-occupancy-measure \
    BUILD=.build/geometry WHITEFOOTC="$WHITEFOOTC" ABSEIL_PREFIX="$ABSEIL_PREFIX"
```

`ECO_GEOMETRY_FILE`, `ECO_GEOMETRY_METADATA`, `ECO_OCCUPANCY_ACCOUNT` and
`ECO_OCCUPANCY_SAMPLES` select distinct output files. The geometry matrix has
140 data rows, occupancy accounting has 160, and occupancy timing has 3,520.
The existing practical sample/accounting paths are not overwritten. The
practical reducer's complete-matrix mode does not accept this separate
capacity-sweep contract; reduce it by payload, request, path, variant and
cohort while keeping all four requests visible.

### Observed capacity geometry and selected memory pairs

The guarded build used the frozen compiler from
`c75520e9d59d74e19ba158e1cae5f394b3a2d874`, whose executable SHA-256 is
`cb918e191bb344733347e0602171d2ec53bd1d201044fdbc5dd7666468eea0a0`.
The control additions above were built in `.build/geometry/`, preserving the
earlier practical images. Apple Clang 21.0.0, Rust 1.98.1
(`48a229ceaefd4985c50990b14116b6d856af0985`, LLVM 22.1.8), libc++ header
version `220106`, and Abseil `20260817.0` are recorded by
`.build/geometry/ecosystem/geometry-environment.txt`. These are observations
of this pinned toolchain, not portable predictions of native allocation.

The build passed in 8.28 seconds. Each image then passed the 18,390 ordinary
oracle traces and 1,440 occupancy traces. All 140 filled-map observations
passed, as did the seven controls that must reject missing or corrupted
observations. The ordinary and occupancy check stages took 1.66 and 0.93
seconds; geometry collection took 0.21 seconds and occupancy accounting
2.58 seconds. The complete guarded validation session took 13.79 seconds.
No occupancy timing has run at this point.

The retained [geometry observations](ecosystem-geometry.csv) contain 140
unique rows, SHA-256
`69e1b3c76be4cdf2857953a705faf6c833253ac6fdba2335b5501dd7b60329e0`.
Both hash series give the same geometry and requested-byte totals. At
population 3584, the exposed capacities are:

| Entry/slot request | WF usable entries and physical slots | Rust usable-entry lower bound | C++ chaining buckets | Abseil physical slots |
|---|---:|---:|---:|---:|
| 3584, geometry only | 3584 | 3584 | 3593 | 4095 |
| 4096 | 4096 | 7168 | 4096 | 8191 |
| 5120 | 5120 | 7168 | 5147 | 8191 |
| 6144 | 6144 | 7168 | 6151 | 8191 |
| 8192 | 8192 | 14336 | 8192 | 16383 |

At the original 4096 request, Whitefoot occupies 0.875 of its physical
slots; Abseil occupies about 0.43755. Rust's 7168-entry guarantee does not
expose a physical-slot denominator. C++ reports 0.875 entries per chaining
bucket, which is a different quantity from open-addressed slot occupancy.
The direct sparse C control has Whitefoot's capacity and requested-byte
totals, but retains its independently written sparse algorithm. The
geometry-only 3584 point fills all Whitefoot slots; it remains excluded from
the timing sweep as preregistered.

The policy difference is also present at the medium request: at 64/56,
Whitefoot has 64 slots, Rust guarantees at least 112 entries, C++ has 64
chaining buckets and Abseil has 127 slots. At 3/2 all four exposed counts are
three, while their meanings and allocation sizes still differ.

The following pairs are selected solely from the filled-map byte totals,
before timing, among the four timed Whitefoot requests. Each native request
remains 4096. The last column is the larger divided by the smaller total;
all six pass the preregistered 1.10 limit for comparable requested memory.

| Payload | Native peer | WF request | WF live requested bytes | Native live requested bytes | Byte ratio |
|---|---|---:|---:|---:|---:|
| 8 bytes | Rust HashMap | 6144 | 147472 | 139272 | 1.058878 |
| 8 bytes | C++ unordered_map | 6144 | 147472 | 147456 | 1.000109 |
| 8 bytes | Abseil flat_hash_map | 6144 | 147472 | 139184 | 1.059547 |
| 256 bytes | Rust HashMap | 8192 | 2228240 | 2170888 | 1.026419 |
| 256 bytes | C++ unordered_map | 4096 | 1114128 | 1036288 | 1.075114 |
| 256 bytes | Abseil flat_hash_map | 8192 | 2228240 | 2169312 | 1.027164 |

These totals include the C++ bucket backing and all 3584 nodes: C++ makes
3585 allocation requests, whereas the other implementations make one.
They do not include allocator metadata or establish equal physical memory,
layout, lookup strategy or allocation cost. In particular, the wide C++
pair keeps Whitefoot at its original 4096 slots; the Rust and Abseil pairs
use 8192 slots. There is no single capacity request that matches every
peer's memory policy.

The retained [occupancy allocation observations](ecosystem-occupancy-accounting.csv)
contain 160 unique rows, SHA-256
`2b6242c6b9d36c85d2a8d588f7b4a3d698aff4336511e5fee02c531190581df3`.
Every row balances requests and releases and ends with zero live bytes.
At `ECO_OCCUPANCY_WORK=1048576`, each row executes one trace with 292
rounds; all implementations and capacities agree on each payload/path
checksum. This validates the proposed timing workload without supplying a
timing result.

### Query dispatch and inlining in the practical image

Read-only inspection of the retained practical `map-timed` image, SHA-256
`0ef00b9de6a541304f1029b0a8c7cb0b69415e6bbb8e971ac1917a32ff3ca54c`,
separates the current generic map from the historical fixed-eight-slot
inlining finding. The scalar and wide `wf_map_cost_library_*_trace` entries
lead to specialized whole-trace bodies whose query paths already inline
lookup, find/probe, hash, equality and observation. A separately emitted
lookup symbol is not evidence of a query call. Put, try-put and rebuild still
have calls elsewhere in these bodies; the native images also retain mutation
helpers. This inspection supplies no measured benefit from another inlining
change.

The workload dispatch placement agrees in the three adapter sources.
`map-library.wf` puts `lookup_path` and the operation branch inside its
`for (index in 0_u64..count)` loop; Rust puts `match path` inside
`for index in 0..count`; C++ puts its `if (path == Hit || ...)` inside
`for (Word index = 0; index < count; ++index)`. All perform reserve before
that key loop. Whitefoot also preserves the excluded historical rehash path
outside this common loop; native adapters reject that path. The source
control flow therefore differs in that extra case, but the native query
dispatch was not manually moved outside the loop.

The aligned scalar machine-code paths show three different resulting loops:

| Implementation | Observed query loop |
|---|---|
| Whitefoot | Workload dispatch remains per key. Salt/collision state and map backing/capacity are reloaded per lookup. One `udiv`/`msub` pair computes the initial bucket, followed by a rolled probe over 24-byte tagged slots; the query key stays in a register. Lookup-result tag/payload/unit stores also remain. |
| Rust | The hit/miss route is outside the key loop. Salt/collision state, control base and bucket mask are loaded before it. Inlined probing compares eight control bytes together, filters by seven hash bits, masks indexes and advances by growing groups. |
| C++ unordered_map | The hit/miss route is outside the key loop, with hash state and bucket metadata loaded before it. Inlined lookup follows nodes, tests cached hashes and keys, and uses a mask for power-of-two bucket counts with a division fallback otherwise. |

The native routing difference is an optimizer outcome from source dispatch
inside the loop. The retained artifacts do not contain optimization remarks
that establish why Whitefoot's branch was not hoisted. Its raw IR has a larger
control-flow graph with loop-carried values and aggregate result stores;
an unswitching cost or ordering explanation remains a hypothesis. The current
query loop does not exhibit the old fixed-capacity failure in which early
full unrolling kept `find` out of its caller. Capacity, probing strategy,
layout, metadata reloads and result stores remain separate possible costs.
Neither this inspection nor the occupancy discriminator assigns a causal
timing percentage to any one of them. The adapter sources were left unchanged.

## Contract fixed before measurement

All candidates own inline keys and values. Put installs the offered
pair and either inserts it, returns the replaced pair, or returns the offered
pair at the capacity ceiling. A replacement remains possible at that ceiling.
Remove returns the owned pair. Lookup borrows its query and visits the stored
value; references and persistent iterators do not escape. No per-payload Box
is required, and the comparison must not add one to a native control.

The first trial uses no cached hash. Rehash recomputes the hash of every live
key and never calls equality or deduplicates entries. The initial runtime
capacity is exact, without rounding. Reserve grows to its explicit target,
does nothing below the current capacity, and refuses above the ceiling.
Same-capacity rehash is a separate operation. Automatic growth occurs only
after a complete insertion probe finds no free slot, doubling the capacity
or saturating at the ceiling, with zero growing to one. This fixes a matched
ordinary algorithm for comparison, not an optimal load-factor policy.

The same deterministic, read-only hash environment and equality body apply
to WF and C. Mixed and deliberately colliding hash distributions are separate
cohorts. Hostile equality belongs to correctness cases; no equality law may
establish a memory bound, discard an owner, or justify a rehash deduplication.

## Distinguishing layout, algorithm and lowering

Compare both admitted WF candidates, their C algorithm controls, and native
sparse/dense floors. The sparse native floor allocates a new table and moves
each live pair directly; it does not inherit a WF permutation plan or force
initialization of an inactive payload. The candidate's temporary planning
backings, slot exchanges and emitted initialization remain measured costs.
The dense control includes a reverse bucket index and repairs it when removal
moves the last entry. Its reserved entry capacity counts in full, not merely
its live population. A compact native index and a copy-enum index are distinct
layouts whenever their strides differ.

The original comparison distinguishes five C combinations, each at scalar and
256-byte values: sparse direct rebuilding; sparse planning plus permutation;
dense direct rebuilding with a tagged index; the same native algorithm with
a compact index; and dense planning followed by a full bucket scan to repair
the reverse indexes. Sparse planning drops its one-byte-per-target used array
before growing the payload backing; its destination array remains live until
the permutation completes. A deleted sparse slot is restored through the
same complete-slot exchange and reconstruction algorithm as the source
candidate. C leaves inactive payload bytes uninitialized; WF's emitted stores
are a separate lowering cost. Both C map owners contain two words. The ceiling
is a compile-time parameter in both languages, with separate ceiling-three
instances for the correctness policy chains.
The source signatures expose different metadata bounds: sparse planning's
`u64` array requires `ceiling <= 2305843009213693951`, while the dense
16-byte index array requires `ceiling <= 1152921504606846975`. The experimental
ceiling 16384 satisfies both; this comparison does not erase that API difference.

Count map owner width, backing headers, cell/entry/index strides, every
reserved capacity, result layouts, allocation requests, requested bytes and
peak simultaneously live bytes. Inspect retained optimized functions for
actual transfers and stores; a missing memcpy intrinsic is not zero transfer.
Native shared-payload results and the original WF product results are not
the same ABI. Normal versus retained also changes visibility and native calling conventions, so
that ratio is not a copy-only attribution.

In retained mode, only the public `new`, `put`, `remove`, `lookup`, `edit`, `reserve`,
`rehash` and `free` operations, plus supplied hash, equality, borrowed observer
and edit/consuming callbacks, are marked `noinline` on both sides. Private probing,
bounded insertion, planning, normalization and permutation helpers remain
ordinarily optimizable. The trace entry is retained in both modes. The WF
adapter must select those exact names and verify the optimized calls before
measurement; a broad prefix-based barrier would not match this C control.

The original WF put has two distinct owned-pair alternatives and therefore
uses 40 bytes for scalar and 536 bytes for record results in the current
target layout. C shares the alternative pair payload, using 24 and 272 bytes.
A separately identified source control that groups the return reason beside
one pair can test this boundary cost; it must not replace the original
candidate's measurements silently.

### Allocation oracle fixed before timing

Let `C` be the initial capacity, `T` the requested growth capacity,
`B(C) = 16 + C * S` the slot/entry backing, `I(C) = 8 + C * J` the dense
index backing, `P(C) = 8 + 8*C` the sparse destination plan, and
`U(C) = 8 + C` its used array. `S` is 24 or 272 bytes for scalar or record;
`J` is 16 for the tagged index and 8 for the compact native index. The later
single-slot extension uses its separately stated 32/280-byte strides. Allocation
headers belonging only to the observer are excluded equally from requested
bytes. Every request has one release and final live bytes must be zero.

| Algorithm | Initial requests / bytes | One growth: additional requests / bytes | Growth peak | One same-capacity rehash: additional requests / bytes | Rehash peak |
|---|---:|---:|---:|---:|---:|
| Sparse direct | 1 / `B(C)` | 1 / `B(T)` | `B(C)+B(T)` | 1 / `B(C)` | `2*B(C)` |
| Sparse planned | 1 / `B(C)` | 3 / `P(T)+U(T)+B(T)` | `B(C)+P(T)+max(U(T),B(T))` | 2 / `P(C)+U(C)` | `B(C)+P(C)+U(C)` |
| Dense, either algorithm | 2 / `B(C)+I(C)` | 2 / `B(T)+I(T)` | `B(C)+I(C)+I(T)+B(T)` | 1 / `I(C)` | `B(C)+2*I(C)` |

Growth means `T > C`; a smaller/equal reserve is a no-op. Repeated same-capacity
rebuilds multiply total requests/bytes, not peak bytes. These formulas are
independent of payload contents and hashing. The adapter must confirm WF's
actual allocation requests against them before the matching claim holds.

## Bounded workload and oracle

The principal sizes are capacities 64 and 4096, with live counts at one half
and seven eighths of capacity. Scalar and 256-byte inline values exercise
hit lookup, absent lookup, replacement, and remove/absence/reinsert churn.
Growth and same-capacity rehash are separate paths. The growth timing cohort
uses one round so repeated no-op reserves do not dilute its label. Each rehash
round removes every even key ID, checks absence, rebuilds with those tombstones
present, checks all surviving and absent keys, and reinserts the removed keys
with their next value version. It never asks rehash to deduplicate. Zero-round
fill/cleanup traces measure construction
and initial insertion together; they are not subtracted from other traces to
claim isolated operation latencies. Collision pressure needs only the small
scalar case, not another full Cartesian matrix.

Lookup reads a payload identity rather than checksumming 256 bytes per hit;
construction and consuming paths still observe every record word. Ordered
operation outcomes have an ordered digest. Final consumption has no promised
iteration order, so its independent content oracle compares an order-neutral
multiset digest and count. Expected lookup identities and consumed contents
are derived from a direct key-ID inventory, not another hash table. Empty, singleton,
irregular capacities, full-table replacement/refusal, tombstones, growth from
zero and repeated rehash are explicit controls.

The current correctness matrix covers nine capacity/population shapes
(`0/0`, `1/0`, `1/1`, `3/2`, `3/3`, `63/55`, `64/32`, `64/56`, `64/64`),
eight paths, three round counts, three seeds and two hash distributions.
Ten C variants give 12,960 executions per mode; adding the four WF variants
gives 18,144. Each mode also runs ten C policy chains. All passed, including
full-table hostile-equality misses. The actual guarded costs were:

| Source shape | Construction | Execution/check | Validated scope |
|---|---:|---:|---|
| original | 31.92 s | 10.29 s | C-only and WF/C matrices; 24 source/observer executions; retained-call checks |
| compact | 14.49 s | 1.03 s | WF/C matrix; retained-call checks |

Four source callers execute with ordinary deallocation and the quarantining
observer, in each of the three CLI configurations. Their allocation counts
are 24/14 for dense and 29/14 for sparse, each identity released exactly once.
The observer's concurrent and three negative controls passed. Optimized IR
retains every selected public operation and callback in retained mode on both
sides. The compact check exercises the full trace oracle, not a new run of
the separately authored source/observer fixtures. These commands used the
existing v0.67 gate-profile compiler and Apple Clang 21; they did not rebuild
Rust. Earlier seven-path evidence remains in commit `30f36dff`.

Every accepted timed trace must match its independent content/outcome oracle
and the per-representation request/release counts, requested bytes, peak bytes
and final zero live bytes. The cost driver's header signature is a local
consistency check, not an exact allocation-identity release ledger: it frees
immediately, so reading the header again after a duplicate release is invalid
and reused addresses do not retain allocation identity. The formal library
caller uses the existing quarantining observer and its negative controls for
that stronger check, together with must-consume and owning-child cleanup.
A timing checksum is not a proof of ownership. Allocation is total under
STOR-8: a capacity refusal is an ordinary map outcome, not a recoverable
allocation failure.

Reuse the Slab/Deque harness's ordinary and retained modes, fresh-allocation
`noalias nonnull` instrumentation, rotated implementation order, reversed
cohorts and eleven samples. Compare paired medians within the same source,
seed, occupancy and boundary treatment; keep raw samples. Do not average
different workloads into a fabricated application distribution or select a
layout from bytes alone. Record unexplained costs, including wide owning
results, before claiming a performance floor or choosing further machinery.

The selected primary matrix has 31 workload cells, chosen to separate costs
without inventing an application distribution:

| Capacity / occupancy / hash | Payloads | Paths | Cells |
|---|---|---|---:|
| 64 / 7/8 / mixed | Scalar and record | All seven original paths | 14 |
| 4096 / 7/8 / mixed | Scalar and record | Hit, miss, grow, rehash, setup/cleanup | 10 |
| 64 / 1/2 / mixed | Scalar and record | Miss and churn | 4 |
| 64 / 7/8 / colliding | Scalar | Miss, churn and rehash | 3 |

The larger backing separates dependent lookup and rebuilding costs; the
half-full pair checks load sensitivity; the collision subset checks long
probes and tombstones. A capacity-dependent reversal, anomalous half-full
behavior, or an unexplained operation-specific gap triggers a targeted
extension, not an automatic Cartesian expansion. Correctness still covers
all nine shapes and both hash kinds. Each cell compares seven implementations
at its payload size. Eleven paired seeds in each of two reversed cohorts and
both boundary modes yield 9,548 primary rows. Mode order is normal then
retained for cohort zero and reversed for cohort one; implementation order
rotates with the sample and reverses between cohorts. The regular paths use
`floor(8192/count)` rounds. Setup uses zero rounds and growth one round; each
repeats `floor(8192/count)` complete traces with successive seeds. Their time
includes fill and cleanup on every repetition. The expected checksum folds
each independent trace with multiplier 257; allocation counts and total
requested bytes scale by repetitions, while peak bytes do not.

`MEASURE_SET=edit` selects four additional cells: capacity 64, both payloads
and occupancies, mixed hash. Its callback increments only the first stored
`u64` and returns that identity; the wide value's other 31 words stay unchanged
and are all checked at cleanup. This is a different operation from replacement,
so their timing ratio is not a pure-copy attribution. The additional cells
yield 1,232 rows with the same paired schedule. `MEASURE_SET=boundary` selects
capacity 64 at 7/8 occupancy, both payloads, mixed-hash replace and churn:
1,232 rows per source shape for the original and separately transformed
compact-result control. The `source_shape` CSV column labels WF original or
compact results; C's shared-payload result stays `c-shared`. Source hashes and
independent build directories identify the actual compiled modules; the label
alone is not an identity check. No production library result shape changes
as part of this control.

## Maintained commands and source candidates

Run heavy commands through the repository's shared guard. Construction and
execution can be reported separately without weakening the combined check:

```sh
make -C research/experiments/container-representation/map-library build-candidates build-costs CLANG=/usr/bin/clang
make -C research/experiments/container-representation/map-library check CLANG=/usr/bin/clang
make -C research/experiments/container-representation/map-library measure CLANG=/usr/bin/clang
make -C research/experiments/container-representation/map-library summarize
make -C research/experiments/container-representation/map-library build-compact check-compact CLANG=/usr/bin/clang
```

`BUILD`, `WHITEFOOTC`, `SOURCES`, `MEASURE_SET` and `SOURCE_SHAPE` are overridable;
use a distinct build directory for each transformed source. `summarize` reads
`SAMPLES` (default `$(BUILD)/measurements-$(MEASURE_SET)-$(SOURCE_SHAPE).csv`)
and writes medians grouped by
mode, reversed cohort, workload, implementation and source shape, retaining
all eleven paired samples in the raw CSV. It reports whole-trace elapsed time
and either time per complete trace for growth/setup or time per item-round
for the other paths, including their setup and cleanup. It does not subtract
baselines or pool the two cohort orders. The source candidates are
`dense-map.wf` and `sparse-map.wf`; their corresponding `*-check.wf` and
`*-owned-check.wf` callers cover source outcomes, hostile equality, wrapping,
contracts and owning-child cleanup. Current exact allocation expectations are
24/14 for the dense callers and 29/14 for the sparse callers. The three CLI
configurations are default, `--no-overlap` and `--par`; default and
`--no-overlap` currently select the same lowering, so these are two distinct
lowerings. The ordinary allocator runs as well as the quarantining observer;
the latter's concurrent and three negative controls are reused without
creating another observer implementation. Once a library representation is
selected, move its implementation and maintained correctness callers to their
existing library/formal-test homes and update these callers; do not maintain
a frozen duplicate library here. The other candidate stays only while it
provides this explicitly selected representation comparison.

The compact overlay is [compact-put.patch](compact-put.patch), not a second
copy of either library. `compact-source` copies the three original source
files into `$(BUILD)/compact-source` and applies that patch with zero fuzz.
`build-compact` and `check-compact` use those generated sources in an independent
`$(BUILD)/compact-build`, reusing the existing compilation, retained-boundary
and oracle targets. Each library changes one enum declaration, five returned
owner constructors and one paired outcome match; the adapter changes four
paired matches per representation. The nested reason match has the same
branch bodies, with renamed binders. Hashing, probing, allocation, mutation,
edit callbacks and the operation trace are unchanged. The overlay exists only
for this result-shape discriminator; remove it when the selected source or a
maintained successor preserves the comparison, citing this checkpoint in Git
instead of retaining duplicate libraries. Applying it at the measured
[c206655d checkpoint](https://github.com/mbbill/Whitefoot/tree/c206655d/research/experiments/container-representation/map-library)
reproduces the compact source hashes below exactly; use that checkout for
the exact original build. It adds no daily gate dependency.
The overlay SHA-256 is
`d63ef0692294ae534127575065b5765125964e8e3789c6605dec5e03d3634e5a`;
at that checkpoint the maintained generation target was executed and all
three generated files compared byte-for-byte with the measured compact
sources. The current extended adapter also contains the third candidate:
the target replays the same result-shape control in that larger module,
including the unchanged single-slot library, rather than claiming the old
adapter or module hashes. The recursive
build/check caller was dry-run checked; it reuses the already verified
recipes, without another construction or timing run for the wiring change.

`measure MEASURE_SET=primary` and `measure MEASURE_SET=edit` keep their output
files separate. For the two-build boundary comparison, run the checked native
binaries in this order inside one guarded command (paths are build outputs
under this experiment):

```sh
original_build=.build
compact_build=.build/compact-build
for cohort in 0 1; do
  if test "$cohort" = 0; then
    modes='normal retained'; shapes='original compact'
  else
    modes='retained normal'; shapes='compact original'
  fi
  for mode in $modes; do
    for shape in $shapes; do
      if test "$shape" = original; then binary_dir=$original_build; else binary_dir=$compact_build; fi
      "$binary_dir/map-costs-$mode" measure "$cohort" boundary "$shape" > "$original_build/boundary-$shape-$cohort-$mode.csv"
    done
  done
done
```

Do not run all original cohorts before all compact cohorts. The output
filename, independent build directory and source hashes jointly preserve
the image identity even for C's `c-shared` rows.

## Measured comparison, 2026-09-22

The host was an Apple M1 Pro with eight cores (six performance, two efficiency)
and 32 GB RAM, arm64 macOS 26.6.2 / Darwin 25.6.0. The native compiler was
Apple Clang 21.0.0 (`clang-2100.3.34.2`), at `-O2` with no LTO. All heavy
commands used the repository's shared guard; construction/checks used
`WF_WORKERS=2`. Timed traces are sequential and do not publish parallel work.
The original first normal/cohort-zero primary command took 1.04 seconds;
the remaining primary, edit and boundary commands together took 4.90 seconds.
Those command durations include oracle/accounting and CSV output outside
each measured interval; they are not operation times.

The first primary command was a bounded cost pilot and is retained as cohort
zero's normal result, not discarded or rerun. It was followed by original
retained/cohort zero, retained/cohort one and normal/cohort one. Edit uses
the same mode/cohort order. Boundary uses original then compact in cohort
zero and compact then original in cohort one, with normal/retained mode
order also reversed. All samples, including the first, remain published.

Each table entry below is the median of eleven **per-sample elapsed-time
ratios**, pairing the same seed, trace sizes and workload. It is not the ratio
of two independently computed medians. `A / B` gives cohorts zero and one
separately. Every trace passed its content/outcome and allocation oracle.
An independent readback checked all 13,244 rows for uniqueness, eleven-sample
groups, cross-implementation/mode/cohort checksums, and exact request/byte/peak
formulas. Observed durations are quantized to 1,000 ns; the shortest is
21,000 ns. A few-percent difference in a short trace does not select a winner.
If such a difference becomes decisive, repeat complete traces in a declared
batch rather than increasing rounds and changing the setup proportion.

### Representation comparison

These ratios are **WF dense / WF sparse**, using the original three-variant
result. Values below one favor dense. Every primary workload is shown; no
application-frequency weighting or overall score is assigned.

| Value bytes | Capacity/live | Hash | Whole trace | Normal A / B | Retained A / B |
|---:|---:|---|---|---:|---:|
| 8 | 64/32 | mixed | churn | 1.667 / 1.585 | 1.264 / 1.276 |
| 8 | 64/32 | mixed | miss | 1.111 / 1.088 | 1.267 / 1.289 |
| 8 | 64/56 | colliding | churn | 1.218 / 1.215 | 1.068 / 1.067 |
| 8 | 64/56 | colliding | miss | 1.416 / 1.430 | 1.091 / 1.095 |
| 8 | 64/56 | colliding | rehash | 1.195 / 1.193 | 1.094 / 1.095 |
| 8 | 64/56 | mixed | churn | 1.534 / 1.542 | 1.172 / 1.138 |
| 8 | 64/56 | mixed | grow | 0.964 / 0.962 | 0.993 / 1.005 |
| 8 | 64/56 | mixed | hit | 1.217 / 1.216 | 1.209 / 1.218 |
| 8 | 64/56 | mixed | miss | 1.233 / 1.233 | 1.107 / 1.149 |
| 8 | 64/56 | mixed | rehash | 1.204 / 1.196 | 1.127 / 1.144 |
| 8 | 64/56 | mixed | replace | 1.194 / 1.194 | 1.126 / 1.125 |
| 8 | 64/56 | mixed | setup-cleanup | 1.013 / 1.000 | 1.045 / 1.040 |
| 8 | 4096/3584 | mixed | grow | 0.838 / 0.844 | 0.881 / 0.881 |
| 8 | 4096/3584 | mixed | hit | 1.164 / 1.185 | 1.110 / 1.124 |
| 8 | 4096/3584 | mixed | miss | 1.088 / 1.079 | 1.100 / 1.096 |
| 8 | 4096/3584 | mixed | rehash | 0.970 / 0.977 | 0.936 / 0.948 |
| 8 | 4096/3584 | mixed | setup-cleanup | 1.015 / 1.007 | 1.016 / 1.023 |
| 256 | 64/32 | mixed | churn | 1.150 / 1.140 | 1.146 / 1.143 |
| 256 | 64/32 | mixed | miss | 1.111 / 1.111 | 1.220 / 1.340 |
| 256 | 64/56 | mixed | churn | 1.186 / 1.178 | 1.147 / 1.142 |
| 256 | 64/56 | mixed | grow | 0.795 / 0.781 | 0.840 / 0.845 |
| 256 | 64/56 | mixed | hit | 1.180 / 1.200 | 1.198 / 1.213 |
| 256 | 64/56 | mixed | miss | 1.185 / 1.162 | 1.100 / 1.115 |
| 256 | 64/56 | mixed | rehash | 0.866 / 0.871 | 0.931 / 0.917 |
| 256 | 64/56 | mixed | replace | 0.980 / 0.985 | 1.013 / 1.018 |
| 256 | 64/56 | mixed | setup-cleanup | 0.883 / 0.881 | 0.951 / 0.950 |
| 256 | 4096/3584 | mixed | grow | 0.702 / 0.704 | 0.743 / 0.745 |
| 256 | 4096/3584 | mixed | hit | 0.916 / 0.919 | 0.956 / 0.960 |
| 256 | 4096/3584 | mixed | miss | 0.944 / 0.944 | 0.954 / 0.951 |
| 256 | 4096/3584 | mixed | rehash | 0.818 / 0.807 | 0.827 / 0.830 |
| 256 | 4096/3584 | mixed | setup-cleanup | 0.778 / 0.781 | 0.842 / 0.850 |

The two orders agree on the substantial split: sparse wins the measured
small-table access/churn cases, while dense wins wide growth and rebuilding.
Large-record lookup changes direction at the larger capacity. Those are
specific measured consumers, not a universal dense-versus-sparse ranking.

The native controls distinguish the wide 4096/3584 rebuilding gap. They use
the same hash, occupancy, outcome and allocation policy; the planned/repaired
controls additionally follow the candidate algorithm, while the direct/tagged
controls expose a cheaper ordinary-native formulation.

| Whole trace | Mode | WF sparse / C planned A / B | C planned / C direct A / B | WF sparse / C direct A / B | WF dense / C repaired A / B | C repaired / C tagged A / B |
|---|---|---:|---:|---:|---:|---:|
| grow | normal | 1.209 / 1.211 | 1.475 / 1.481 | 1.784 / 1.782 | 1.120 / 1.116 | 1.191 / 1.202 |
| grow | retained | 1.043 / 1.051 | 1.704 / 1.678 | 1.777 / 1.769 | 1.104 / 1.102 | 1.295 / 1.296 |
| rehash | normal | 1.108 / 1.115 | 1.288 / 1.295 | 1.427 / 1.450 | 1.061 / 1.057 | 1.132 / 1.140 |
| rehash | retained | 1.010 / 1.022 | 1.409 / 1.329 | 1.396 / 1.385 | 1.035 / 1.038 | 1.158 / 1.158 |

The algorithm control retains much of sparse's gap even in C; it cannot all
be charged to the WF helper boundary. Dense reverse-index repair is also
measurable. These are whole traces including fill, verification and cleanup,
so subtracting columns would not isolate memcpy time. The result meets the
recorded reopening condition for a `Slots<Pair,1>` sparse cell with direct
migration. That additional ordinary source formulation still has to admit,
preserve every owner, and justify its larger cell/double-backing peak. No
third-source timings or library selection are claimed here.

Wide replacement is a separate remaining cost. At 64/56, original WF sparse
is 1.466 / 1.459 times planned C in normal mode and 1.559 / 1.459 with
retained helpers; dense is 1.266 / 1.272 times repaired C and
1.593 / 1.522 respectively. Against the direct/tagged native floors the
corresponding WF ratios are 2.376 / 2.408 and 2.922 / 2.734 for sparse,
2.201 / 2.175 and 2.807 / 2.616 for dense. Algorithmic transfers and the
different result ABI both remain relevant. The compact control below tests
one source formulation; it is not evidence that these costs have all gone.

### Storage and allocation costs

Measured requests agree with the formulas above. At capacity 4096 the
following requested-byte counts exclude only the common observer header.
The growth columns describe one complete setup plus 4096-to-8192 reserve,
not the number of repeated traces in a timing row. Every listed total has
matching releases and zero final live bytes.

| Value bytes | Algorithm | Initial requests / bytes | Growth total requests / bytes | Growth peak bytes |
|---:|---|---:|---:|---:|
| 8 | sparse direct | 1 / 98,320 | 2 / 294,944 | 294,944 |
| 8 | sparse planned (WF and C) | 1 / 98,320 | 4 / 368,688 | 360,488 |
| 8 | dense tagged/repaired (WF and C) | 2 / 163,864 | 4 / 491,568 | 491,568 |
| 256 | sparse direct | 1 / 1,114,128 | 2 / 3,342,368 | 3,342,368 |
| 256 | sparse planned (WF and C) | 1 / 1,114,128 | 4 / 3,416,112 | 3,407,912 |
| 256 | dense tagged/repaired (WF and C) | 2 / 1,179,672 | 4 / 3,538,992 | 3,538,992 |

Both sparse cells and dense entries are 24/272 bytes, but dense additionally
reserves a 16-byte index per capacity unit and an eight-byte index header.
The compact native index is eight bytes per unit; its initial totals are
131,096/1,146,904 bytes. Same-capacity sparse planning needs two temporary
arrays (three requests including construction), with peaks 135,200/1,151,008
bytes; dense rebuild needs one new index (also three including construction),
with peaks 229,408/1,245,216. Fewer temporary requests alone therefore does
not determine the smaller peak.

### Borrowed in-place edit

The separate four-cell edit trace changes only the first word and validates
the complete final record. The dense/sparse ratios are:

| Value bytes | Capacity/live | Normal A / B | Retained A / B |
|---:|---:|---:|---:|
| 8 | 64/32 | 1.188 / 1.214 | 1.220 / 1.231 |
| 8 | 64/56 | 1.151 / 1.158 | 1.239 / 1.224 |
| 256 | 64/32 | 1.176 / 1.172 | 1.393 / 1.385 |
| 256 | 64/56 | 1.205 / 1.200 | 1.281 / 1.323 |

This establishes an ordinary borrowed mutation path without returning the
whole value. It does not establish that arbitrary wide mutation is cheap,
nor that the difference from the replacement workload is all transfer cost.
The latter computes and consumes different contents.

### Compact-result boundary control

Compact uses `Inserted | Returned(kind: ReplacedOrFull, pair: Pair)` and
preserves the original operation sequence. It changes only result declarations,
constructors and corresponding matches. The actual layout is scalar 40 to
24 bytes and wide 536 to 272 bytes; `Option<Pair>` stays 24/272 bytes.
These ratios are **compact WF / original WF**. `C-normalized` is the median
of each sample's WF ratio divided by that sample's matching C-algorithm ratio,
not a division of the displayed aggregate medians.

| Representation / value / path | Normal A / B | Normal C-normalized A / B | Retained A / B | Retained C-normalized A / B |
|---|---:|---:|---:|---:|
| sparse / 8 / replace | 1.000 / 1.000 | 0.986 / 1.000 | 0.898 / 0.923 | 0.898 / 0.927 |
| sparse / 8 / churn | 1.000 / 1.017 | 1.008 / 1.038 | 0.985 / 0.985 | 0.968 / 0.977 |
| dense / 8 / replace | 0.989 / 0.976 | 0.936 / 0.964 | 0.966 / 0.953 | 0.931 / 0.955 |
| dense / 8 / churn | 0.996 / 1.014 | 0.997 / 1.009 | 0.993 / 0.998 | 0.986 / 1.007 |
| sparse / 256 / replace | 0.886 / 0.914 | 0.883 / 0.883 | 0.826 / 0.852 | 0.793 / 0.854 |
| sparse / 256 / churn | 0.897 / 0.909 | 0.904 / 0.879 | 0.919 / 0.916 | 0.918 / 0.913 |
| dense / 256 / replace | 0.898 / 0.922 | 0.902 / 0.913 | 0.830 / 0.852 | 0.792 / 0.857 |
| dense / 256 / churn | 0.905 / 0.924 | 0.924 / 0.920 | 0.950 / 0.949 | 0.952 / 0.952 |

The wide improvement survives both orders, but width alone does not explain
it. The original/compact C controls' optimized modules are byte-identical;
their per-cell paired time ratios span 0.982–1.046, indicating the remaining
run-to-run variation. Small scalar changes are not a basis for a general
performance claim.

The following is the optimized wide **public put** path under both source
shapes. The entry copy occurs before any outcome; different outcome rows
are mutually exclusive and must not be added together.

| Path | Original product result | Compact shared-pair result |
|---|---|---|
| entry snapshot | 1 × 256-byte memcpy | unchanged |
| Inserted | 536-byte result memset | 272-byte result memset |
| Replaced | 536-byte result memset + 1 × 264-byte memcpy | header/tag stores + 2 × 264-byte memcpy |
| Full at ceiling, or after refused reserve | 2 × 264-byte memcpy, including projection; 272-byte prefix memset | same two copies; header/tag stores |
| growth/retry | 264-byte projection + 256-byte retry argument; try-put writes the final result | same transfers |
| caller consumes Replaced/Full | 264-byte Pair projection + 256-byte value projection | unchanged |

Compact's shared projection precedes its inner reason match, introducing the
extra public Replaced copy even while reducing inactive-payload initialization.
Its public put IR frame extent falls from 1584 to 1056 bytes; this is not a
native whole-call-stack bound. Normal optimization retains the same relevant
entry and caller transfers. Private try-put remains a real call in both
modes and shapes without an imposed private `noinline` barrier.
Removing repeated static copy sites by sharing a match arm does not reduce
the dynamic copies on a path. Wide remove's `Option<Pair>` layout and its
optimized function instructions are unchanged between shapes after nominal
type/metadata renumbering; its caller still projects 264 then 256 bytes.
Dense removal also carries its whole-entry swap work. The compact control
therefore establishes a useful source-shape opportunity, not a solved generic
owning-result transfer path or a compiler-layout requirement.

### Sample and build identities

These are successors of the source checkpoint `30f36dff`; the exact measured
input bytes are identified below. Kernel v0.67's existing compiler executable
has SHA-256 `bec227121f4a223906f6f429cec0e4a34a108e8dafac46daa56ffc8a23d19674`.
No compiler implementation changed for this experiment. The linked runtime
comes from base `e6349b80` (backend tree
`ad0f0f663b6daf02cbcfee8efbc6a37ff0d61616`). All twelve runtime object files
are byte-identical between the two builds. C driver SHA-256 is
`6c1996164bfb1e932fd4ed4487de2b1940271d8518cdd17a8f71614e8640cff0`.
The later Makefile edits distinguish output filenames and reproduce the
compact source overlay; they do not change instrumented IR or compiler flags.

| Artifact | Original SHA-256 | Compact SHA-256 |
|---|---|---|
| dense source | `4888f8d2f5cdc8ebd5deb90b920daf41a8b04ba1f13ba70ef347cc2cc97ab62e` | `80c13a5cf4ab0c130a920af2d899ed7d3ac979e8949c49031b292195eb8fbc53` |
| sparse source | `f8bf9f5a4d010f79e388aeb4ea27bc58c500141a4d44ecd996736c47a8aefdb9` | `38f60b5f7d3997a0d82c637aa799d7b4dcfe328fd3dfa68c6a484a10ec5bbd79` |
| trace adapter | `737eb59f8989d4b5a3c1f9f2188387125748c8a990c3ed5e67faecd94406b285` | `7f1ba38e3249ad2c863f1eca8d1cfb8eca4637e849d56b8b1913a0176138420b` |
| raw WF module | `9731b64977e3609cb17769de63f2c421e0c7d8e14c6ecf290135bd600f3192c4` | `ad0d964bf62265bef62617b4d33f9cb7e8e3faf221243a346db47d7bc164ef56` |
| normal optimized WF | `910b76f53b31fb91bb70dbb2fda4977a345d0a104318f42b1b9889e3820bcd67` | `2140143cfad184987147603e6ddd488266b12fe87e52cfef1e81687e060335a6` |
| retained optimized WF | `4f9e46fcf7bb5909365b8780fee39a3453629839c6265bce704b78a3a9e6b6e3` | `17db060dc0ffe60025732bfd160fc8115e8e7cce9296659f613179ee624333ef` |
| normal executable | `1e494f95003ec35217cf1a73462781543e0f11663ff6381994b3e9690115dce7` | `0764aad7137b5592370bdb886c4c17f20f30a92eb31950ff9fe06b9d02882f64` |
| retained executable | `653951fe2f072963b763cd8b9e235384b243c55752f51553081e7bfe99736a2a` | `0ad1600f9c7c6f029d445a4b765eb38ae48c4cfd523ef17b0bdef3b74d1fc8ec` |

Both builds have the same C optimized module hashes:
`7b2cc6f64c3bdbccd7a529ee8d44a5611db8d8fca064eabc9ec10d018bfaf432`
(normal) and
`2011bc70afeced36525939684be33163b73760ffd0356a457a22779c13080b46`
(retained). The matching runtime-object manifest sorts each relative path
under `native/`, with one `SHA256  path` line per object, ending in a newline;
its SHA-256 is
`cfd944afa96c7d0c49c39b28239a1a2baa4a28d87faf7b59cafd62c6827368b5`.

The four archives retain the native driver's original seventeen columns.
They concatenate cohort-zero normal/retained and cohort-one retained/normal,
with one unchanged header. Boundary-original and boundary-compact remain
separate because C's `source_shape=c-shared` does not by itself identify which
linked executable was timed. No synthetic run column was added. Filtering
the header and one `(contract,cohort)` pair recovers that original CSV exactly.

| Raw archive | Data rows | gzip SHA-256 | Uncompressed CSV SHA-256 |
|---|---:|---|---|
| [primary original](measurements-primary-original.csv.gz) | 9,548 | `010db16f338399c04ee83317bbebec749f881039641890967a40c5c76ce7d183` | `9e362c8172f9ab4b4d1376bd7ee0c7cd4d7a3d7f975037d610b7011dced4f234` |
| [edit original](measurements-edit-original.csv.gz) | 1,232 | `291ae62d5d1cf234d859675ea6d68fe8340d1b73e222e6ae5c573664a8504c8f` | `ae74e4b4d8a7dc5cca71259cc54aa521da47bec4d5e3c957a6b1ca1bc93a85be` |
| [boundary original](measurements-boundary-original.csv.gz) | 1,232 | `d62d0b5983f5a7460efc4ba6d0254c622a2b7288fbd3fc29da93a46d9e3e9b3b` | `66e61ed26a843b912d7069b3e976268fc6a18f0ecf28ff36f0fe23006a15c52f` |
| [boundary compact](measurements-boundary-compact.csv.gz) | 1,232 | `55770d7e49908e0da652cc545e10545fe3a74a40d1ee176815e081c96b89d5b4` | `695f38ad600fa4c8ca04bb7e9304330607892e0adbdd0cda03c82df7c7b76dee` |

For example, from this directory the complete primary dense/sparse paired
table can be recomputed without building or timing anything:

```sh
gzip -dc measurements-primary-original.csv.gz | perl -F, -lane '
  next if $. == 1;
  next unless $F[7] =~ /^(?:word|record)-wf-(dense|sparse)$/;
  $v = $1; $g = join q{,}, @F[0..6]; $t{$g}{$F[9]}{$v} = $F[12];
  END { for $g (sort keys %t) {
    die "incomplete group" unless keys(%{$t{$g}}) == 11;
    @r = sort {$a <=> $b} map {$t{$g}{$_}{dense}/$t{$g}{$_}{sparse}} 0..10;
    printf "%s,%.6f\n", $g, $r[5];
  }}'
```

Use the edit archive with the same command for its paired table. The
`summarize` target instead reports individual implementation medians and
whole-trace units; those medians must not be divided and called the paired
statistic above. All archives remain evidence for these exact source shapes,
even if a later library choice supersedes their implementation.

## Direct-migration extension

The first matrix above is frozen evidence from `c206655d`, not a timing of
the later candidate additions. Its wide rebuild costs reopen a direct
migration using one inline `Slots<Pair,1>` per bucket. The source retains
the same key/value, result and growth policies, but allocates one new backing
and visits old materialized buckets in descending order. It takes each old
cell into a local and drains the zero-or-one pair into an empty destination.
Migration hashes each live key once and calls no equality function. A
zero-capacity rehash returns without another allocation.

The extension reuses the existing driver, oracle and `measure` target with
`MEASURE_SET=rebuild`. It selects only scalar/record grow/rehash at capacity
4096, count 3584 and mixed hashing. Growth performs one real reserve per trace;
rehash removes the even key IDs, migrates 1792 live owners, then reinserts the
removed IDs on each round. Timing still includes fill, lookup, reinsertion
and cleanup. Neither a whole-trace ratio nor subtraction of a setup trace
isolates migration latency.

Three additional C controls separate the source choice from its lowering:
descending direct migration of the original enum buckets; the same native
algorithm with single-slot buckets; and the source-shaped take/local/drain
algorithm. Direction and cell shape are compile-time parameters of the
existing native control, not another copied harness. Native direct cleanup
retains its ascending order; source-shaped cleanup, like WF, takes cells in
descending order. That remaining algorithm difference is part of their
whole-trace comparison. All preserve the original public-helper retention
policy. Private functions remain ordinarily optimizable.

The single-slot cells have scalar/record strides 32/280, Pair offset 8 and
deleted-flag offsets 24/272. Actual WF IR stores the flag as `i1`, occupying
one byte; C uses a one-byte `bool`. `B(C)=16+C*S` with these larger strides
therefore gives both implementations' requested bytes. Growth adds one
`B(T)` allocation and peaks at `B(C)+B(T)`; positive-capacity rehash adds one
`B(C)` per round and peaks at `2*B(C)`. Empty rehash adds no request. C does
not initialize inactive Pair bytes, so any WF initialization is a lowering
cost, not a forced condition of the native floor.

The original primary/edit/boundary sets explicitly exclude the new variants
and keep their original implementation order. The rebuild set adds three C
controls and one WF candidate to the existing seven implementations per
payload: four cells, eleven implementations, eleven paired seeds, two modes
and two reversed cohorts. This eleven-implementation image was checked but
not timed; the four-candidate image measured below supersedes that timing plan.
New control correctness is limited to these two paths at `(capacity,count)`
`(0,0)`, `(1,1)`, `(3,2)`, `(3,3)` and `(63,55)`, retaining the existing three
round counts, three seeds and both hash kinds. `slot-check.wf` separately
exercises the full operation chain and owning-child identities in all three
CLI configurations with the quarantining observer.

The extended `build-candidates build-costs` construction passed in 39.62 s
with the same v0.67 compiler and Apple Clang 21. It produced all five source
fixtures and the six-entry trace module; 54 selected public/callback
definitions receive the retained treatment. C-only construction took 2.70 s;
execution/check took 9.57 s. Each mode passed 14,040 C-only and 19,584 WF/C
traces, ten policy chains, and the retained-call checks. The five fixtures
passed all thirty ordinary/observed executions with exact allocation counts
24/14/29/14/18. No rebuild timing was taken on this six-entry image.

Optimized wide rebuild paths distinguish new extent `N`, old materialized
extent `O`, and old live owners `L`. Each single-slot backing requests `16+280*N`
bytes. The following transfers are dynamic path counts; empty old cells
still contribute to `O` but not `L`.

| Implementation | Initialization per new cell | Snapshot per old cell | Transfer per live owner |
|---|---|---|---|
| WF normal | memset 272 B + deleted byte | memcpy 264 B | memcpy 264 B |
| WF retained | same | memcpy 272 B | memcpy 264 B |
| Source-shaped C normal | length 8 B + deleted byte | memcpy 272 B | memmove 264 B |
| Source-shaped C retained | same | memcpy 280 B | memmove 264 B |
| Native descending enum C | tag 4 B | none | memcpy 264 B |
| Native descending single-slot C | length 8 B + deleted byte | none | memcpy 264 B |

The live transfer's IR length is `264*len`; the admitted single-slot owner
has `len=1` on that path. LLVM has already collapsed the source append loop.
There is no bulk copy of the old backing and no heap allocation for staging.
The public reserve/rehash result occupies 8 B, so this boundary does not
carry a wide Put result. Both source and C private rebuild helpers remain
ordinarily optimizable. WF's 272-byte new-cell initialization becomes eight
paired vector stores and one vector store on this AArch64 target. Original
enum `SparseVacant` construction also clears its inactive payload; this
initialization cost is not unique to the single-slot representation.

This inspection uses raw WF module SHA-256
`57c229193e564088aef3cd2315b959d4caea55ae1f3c9d25dbbedb5e4f34eadd`,
normal/retained optimized WF modules
`e217feea80f8f504d716197ccba408f19954f66fc2b80547e541b860e11f71a3` /
`e39573b54bc561392e7410fcbc8775b2673f80c2b68a6c9bb2806a8c39afb107`,
and normal/retained optimized C modules
`e2ede7dd2bfa5b0e68ef401edfb3f0dc973364329b0f2b263e38300f21b51b56` /
`3bf2d18bd2f30dff80a2cbfcac84e993ab82c000ab84f8685b6fd97035eae224`.
The C source is
`438aadb4773e7a9c990a6834da6ab7f77aab56684cdce1b6d11948b05f480523`;
the extended adapter is
`3687be38e4e7d77982c34a76ddc7b8d7013fc1bbfcf1b15db7a2845cd56be001`.
These are construction and inspection identities, not new timing identities.

### Enum buckets with one local staging window

Before choosing the larger buckets, a fourth source candidate retains the
original enum bucket layout and stages only the currently migrating owner
in a local `Slots<Pair,1>`. It descends through the old buckets, hashes a
staged owner, probes occupancy without equality, and exchanges into the
available bucket. The exhaustive result match empties staging on insertion
and restores a returned owner on either other outcome. This uses the ordinary
model without a global occupancy fact authorizing a discard. It keeps the
same 24/272-byte buckets and one fresh backing per nonempty rebuild; there is
no staging allocation. Empty rehash again allocates nothing.

The reviewable source difference is [staged-rebuild.patch](staged-rebuild.patch),
applied by `staged-source` to `sparse-map.wf`. The generated unprefixed source
reuses the corrected sparse callers, with allocation expectations 15/12.
A second generated file changes only the `Sparse`/`sparse_` name prefixes to
`Staged`/`staged_`, allowing both candidates in the same timing image. Public
contracts and the unchanged, uninstantiated planning helpers remain in this
overlay; the latter produce no runtime calls. This overlay is removed when
a selected implementation or maintained successor preserves the comparison,
with its historical bytes retained in Git. It is not a selected library API.

The C source control selects the same local-staging algorithm through a
compile-time rebuild policy in the existing sparse macro. Its other
operations remain shared with the original planned control, and no private
`noinline` barrier is added. Adding this WF/C pair increases the same four
rebuild cells to thirteen implementations and 2288 paired rows; it does not
expand the original workload sets. The fourth candidate still uses the
original three-variant Put result. Its migration exchange can therefore
carry a 536-byte wide result where single-slot append returns unit. The
previous compact-public-put measurement does not establish this migration
path's cost; any recommendation to promote a compact staged candidate needs
its own focused four-cell validation.

The generated unprefixed source is byte-identical to the admitted candidate,
SHA-256 `3c6358a57869e62fb35652aeec552b67d594d356f7d4271b07f3518a0a736911`;
the prefixed source is
`74fb915d7e68569ac4d2ddf7789480775377444064883b8732af2cc8a6d76edd`.
Overlay SHA-256 is
`9d0ca2b0207be964bf0556a4f49a9a7909d55fd052cec60ca3b2d1884f89956d`.
The extended adapter preserves every previous trace byte and has SHA-256
`68266d9373bcb33661cb8d0ae65cbbf717f7ffff5b06ed084774f2fcda48d0b5`.

Construction of the fourth candidate and combined image passed in 31.41 s;
execution/check passed in 13.49 s. The unchanged earlier fixture images and
runtime objects were reused after checking their unchanged recipes, flags
and sources; the two new callers and all changed C/WF trace modules were
rebuilt. Each normal/retained image passed 14,400 C-only and 20,304 WF/C
traces, ten policy chains and retained-call checks. All seven fixtures passed
ordinary and exact-identity observer execution in three CLI configurations:
42 runs, with allocation counts 24/14/29/14/18/15/12. The concurrent and three
negative observer controls also passed. All 68 selected WF public/callback
definitions receive the retained treatment, with the selected calls checked
in both optimized modules.

### Four-candidate rebuild measurements

Exact source/build replay of this image uses checkpoint `5970393e`, before
the actual library was added to the combined module. Later builds retain
the comparison sources but are separate executable identities.

The combined image produced 2,288 samples: thirteen implementations per
payload, four cells, eleven paired seeds, normal/retained modes and two
reversed cohorts. All checksums and allocation counters passed during timing.
Independent CSV validation also checked every request/byte/peak formula,
all sixteen complete groups, and checksum equality across implementations,
modes and cohorts. Samples range from 229 microseconds upward and have an
observed 1-microsecond quantum. These are whole-trace times, not isolated
rehash or transfer times. The host, compiler, Clang flags and runtime are
unchanged from the first comparison.

The four measurement invocations completed within a 1.86-second guarded
interval. That interval subsequently attempted a separate scratch library
admission and stopped on a duplicate documentation statement, so 1.86 s is
an upper bound rather than an isolated measurement wall time; it does not
indicate a failed timing oracle. Construction and correctness execution are
the separate 31.41 s and 13.49 s measurements above.

Each entry below is cohort zero / cohort one, using the median of eleven
per-seed time ratios. The denominator is the original planned sparse WF
implementation **in the same combined executable**. Lower is faster.

| Payload / path / mode | Dense / planned sparse | Single-slot / planned sparse | Staged enum / planned sparse |
|---|---:|---:|---:|
| 8 B / grow / normal | 0.822 / 0.828 | 0.866 / 0.876 | 0.784 / 0.797 |
| 8 B / grow / retained | 0.884 / 0.874 | 0.916 / 0.918 | 0.847 / 0.830 |
| 8 B / rehash / normal | 0.973 / 0.970 | 0.934 / 0.927 | 0.921 / 0.895 |
| 8 B / rehash / retained | 0.966 / 0.961 | 0.946 / 0.946 | 0.939 / 0.934 |
| 256 B / grow / normal | 0.690 / 0.690 | 0.980 / 0.982 | 1.013 / 1.014 |
| 256 B / grow / retained | 0.744 / 0.747 | 0.997 / 0.992 | 1.093 / 1.035 |
| 256 B / rehash / normal | 0.810 / 0.804 | 0.931 / 0.932 | 0.913 / 0.915 |
| 256 B / rehash / retained | 0.824 / 0.826 | 0.931 / 0.921 | 0.931 / 0.928 |

Staged enum improves these scalar traces, but its original wide result
shape does not establish an overall advantage: wide growth is slightly
slower than planned sparse and 38–47% slower than dense; wide rehash remains
12–14% slower than dense. The larger single-slot layout does not resolve
that split. There is no assumed application mix with which to average it
away. Cohort variation also matters: retained wide staged/planned growth
moves from 1.093 to 1.035.

The added native controls distinguish some, but not all, causes. The table
uses the same paired statistic and cohort ordering. `C staged` and `C slot`
use the source-shaped algorithms; `C descending` and `C ascending` directly
migrate enum buckets; `C slot direct` changes only that native layout.

| Payload / path / mode | WF staged / C staged | WF slot / C slot | C staged / C descending | C slot direct / C descending | C descending / C ascending |
|---|---:|---:|---:|---:|---:|
| 8 B / grow / normal | 1.013 / 1.025 | 1.141 / 1.151 | 1.007 / 0.997 | 0.987 / 1.000 | 1.010 / 1.023 |
| 8 B / grow / retained | 0.915 / 0.929 | 1.034 / 1.041 | 1.195 / 1.197 | 0.978 / 0.994 | 0.986 / 0.997 |
| 8 B / rehash / normal | 1.003 / 0.981 | 1.078 / 1.063 | 0.970 / 0.973 | 0.957 / 0.953 | 1.004 / 1.009 |
| 8 B / rehash / retained | 0.966 / 0.966 | 1.025 / 1.013 | 1.063 / 1.043 | 0.971 / 0.954 | 0.987 / 1.012 |
| 256 B / grow / normal | 1.248 / 1.242 | 1.472 / 1.429 | 1.452 / 1.437 | 1.030 / 1.017 | 1.006 / 1.015 |
| 256 B / grow / retained | 1.051 / 1.048 | 1.307 / 1.308 | 1.775 / 1.536 | 1.021 / 0.982 | 1.011 / 1.150 |
| 256 B / rehash / normal | 1.090 / 1.111 | 1.247 / 1.245 | 1.191 / 1.193 | 0.990 / 1.006 | 1.013 / 1.008 |
| 256 B / rehash / retained | 0.999 / 0.994 | 1.146 / 1.159 | 1.313 / 1.324 | 0.990 / 0.979 | 1.011 / 1.021 |

The wide staged source algorithm is already costlier than direct native
migration, and WF retains further source/result-boundary work below. It
would be incorrect to attribute its entire gap to the bucket layout or
to LLVM copying alone. The retained wide-growth native direction control
also varies substantially (1.011 versus 1.150); small direction/layout
differences are not a stable selection ground here.

Allocation totals below are independently checked whole-trace values,
formatted as `requests / requested bytes / peak live bytes`. Growth runs
two complete traces with one growth each; rehash runs one trace with two
rebuilds. Peaking is measured across each complete trace, not summed.

| WF representation / payload | Grow | Rehash |
|---|---:|---:|
| Planned sparse / 8 B | 8 / 737376 / 360488 | 5 / 172080 / 135200 |
| Dense / 8 B | 8 / 983136 / 491568 | 4 / 294952 / 229408 |
| Single-slot / 8 B | 4 / 786496 / 393248 | 3 / 393264 / 262176 |
| Staged enum / 8 B | 4 / 589888 / 294944 | 3 / 294960 / 196640 |
| Planned sparse / 256 B | 8 / 6832224 / 3407912 | 5 / 1187888 / 1151008 |
| Dense / 256 B | 8 / 7077984 / 3538992 | 4 / 1310760 / 1245216 |
| Single-slot / 256 B | 4 / 6881344 / 3440672 | 3 / 3440688 / 2293792 |
| Staged enum / 256 B | 4 / 6684736 / 3342368 | 3 / 3342384 / 2228256 |

Staged enum eliminates the plan allocations and saves one word per bucket
relative to single-slot. Same-capacity rehash nevertheless has a higher
peak than planned sparse because it holds both complete backings at once.
These counters do not replace the separate quarantining observer's exact
allocation-identity release checks.

In the measured wide staged rebuild, both optimized modes first read the
old tag and copy a Pair only for a Filled bucket: unlike single-slot, an
empty/deleted old bucket has no payload snapshot. Each initial live owner
still clears its 272-byte local staging window and transfers 264 bytes into
it. The take/projection then passes key and value directly to an outlined
private exchange; no extra aggregate memcpy survives there. Exchange reads
the destination's old 264 bytes into SSA, writes the offered Pair into the
bucket, and clears a 536-byte Put result on the Inserted path. The caller
only reads the tag on successful insertion. A 264-byte result-to-staging
copy executes only for the Replaced/Full retry; the exchange body itself
does not return Full. Each owner-loop attempt hashes once, with no equality
or per-probe hashing. New enum cells clear 264 inactive payload bytes plus
their tag. These facts hold in normal and retained native output, including
the outlined exchange and its Inserted `bzero` tail call; neither private
helper has an imposed `noinline` attribute.

Source-shaped C inlines exchange and has five common-path transfers on
Inserted: old to pending, pending to offered, destination to swap temporary,
offered to destination, and temporary to offered. These are 256-byte copies
plus scalar key handling in normal mode, and 264-byte copies in retained
mode. A sixth copy site is retry-only. C performs no staging-payload or Put
clear. Counting only memcpy intrinsics, or adding every static copy site,
therefore misstates the dynamic comparison. The earlier single-slot
transfer extents remain unchanged in this combined image. Public
reserve/rehash returns 8 bytes throughout; its boundary is separate from
the private exchange's wide Put result.

The measured C source SHA-256 is
`0159dba272b04375fe217d26268a2020c730822a6b2c908b026d7c0f14cfdf59`;
Makefile SHA-256 is
`ba3d7d918946e29d99e92766d3048741c0a96e1f7238164d7a75c8e5f3ee8436`.
The adapter and staged-source identities are those above. The compiler is
unchanged (`bec227121f4a223906f6f429cec0e4a34a108e8dafac46daa56ffc8a23d19674`).

| Combined measured artifact | SHA-256 |
|---|---|
| Raw WF module | `71808b10a76711f526716a3b1ea6c77a0a96cf60278f83ad61344dc4b4ac956b` |
| Normal optimized WF | `5e3077cf3df6a4abbdff37a6d6a3fb36eeb301024306c671e29481957d5892d5` |
| Retained optimized WF | `78e07f3aef13dc7b9e571b76f7086ac774234a7b928817e122a6363a802c4dbf` |
| Normal optimized C | `7c1fad17e6b4e7fae755d3a941d67af4bef7ad42784b825c83d33f122f8fd420` |
| Retained optimized C | `89f54ea6953feca783e54627d60d2fcf75486b7ddb776228131c47017b73506f` |
| Normal executable | `d57fe63766c183cedc737d7724be2c7fb509416fb020264fb3a4168b196bb666` |
| Retained executable | `fd256599c5ee06068ccadcddb6d2cc40cdae952f185d60afafaeda4c0a6afd60` |

[The rebuild archive](measurements-rebuild-original.csv.gz) keeps the same
seventeen columns and cohort/mode concatenation order as the earlier
archives. Its 2,288 data rows have gzip SHA-256
`18da6db7b08482396a9f81cfb9dffc427851e708aa354017b3ae5e572161ac32`
and uncompressed SHA-256
`b52c2851a368ebac5ef1550bb8510c4a18dacc9ef2a31e10ef4e7349ac8665c1`.
Filtering one `(contract,cohort)` pair recovers each original CSV byte for
byte. Reproduction uses the existing `measure` target with
`MEASURE_SET=rebuild SOURCE_SHAPE=original`; no separate harness is required.
The earlier inline paired-ratio command applies to this archive as written
for dense/planned sparse; change its two variant names for the other
comparisons. These samples retain the original three-variant public/private
result shape. They do not measure a later compact library or an inlined
migration exchange.

## Inline compact library trial

This image bundled the then-current [real library](../../../../lib/containers/hash-map.wf)
directly, with compact public Put results and an exhaustive slot swap/match
inside migration. No research copy stands in for it. The old comparison
sets keep their variant membership and order; the new `library` set selects
four rebuild cells with fourteen implementations and four cap-64,
7/8-occupied replace/churn cells with eight. Its 3,872 samples preserve the
same seeds, trace sizes, oracle, two modes and reversed cohorts. Library
rows say `library-compact`; older WF rows remain `original`, and C rows
remain `c-shared`. All are in the same executable for each mode.

Construction passed in 21.57 s and correctness execution in 13.82 s. Each
mode passed 14,400 C-only and 22,896 WF/C traces plus ten policy chains.
All 48 ordinary/observed fixture executions passed, including 28 exact
allocation identities for the actual library in each of the three CLI
configurations. All 84 retained public/callback definitions and their
selected calls passed inspection. The four timing invocations completed
in 2.71 s. Independent validation checked every sample's allocation formula,
checksum equality, complete groups and exact reconstruction of the four
original CSVs. The shortest sample is 39 microseconds; the observed clock
quantum remains 1 microsecond.

The tables again report cohort-zero / cohort-one medians of eleven paired
per-seed ratios. `C descending` is the native direct enum migration floor.
`C staged` retains the earlier local-staging/exchange algorithm and shared
payload result; it is a source-algorithm control, not a claim that its
instructions match the new inline migration.

| Payload / rebuild / mode | Library / planned WF | Library / dense WF | Library / old staged WF | Library / C descending | Library / C staged |
|---|---:|---:|---:|---:|---:|
| 8 B / grow / normal | 0.792 / 0.800 | 0.960 / 0.960 | 0.997 / 1.003 | 1.013 / 1.023 | 1.020 / 1.013 |
| 8 B / grow / retained | 0.823 / 0.816 | 0.936 / 0.924 | 1.000 / 0.997 | 1.090 / 1.087 | 0.912 / 0.910 |
| 8 B / rehash / normal | 0.912 / 0.905 | 0.929 / 0.931 | 0.994 / 0.980 | 0.950 / 0.938 | 0.974 / 0.978 |
| 8 B / rehash / retained | 0.924 / 0.922 | 0.973 / 0.966 | 0.998 / 0.996 | 1.024 / 1.018 | 0.972 / 0.957 |
| 256 B / grow / normal | 0.900 / 0.906 | 1.282 / 1.298 | 0.883 / 0.892 | 1.602 / 1.619 | 1.094 / 1.096 |
| 256 B / grow / retained | 0.918 / 0.914 | 1.211 / 1.232 | 0.877 / 0.889 | 1.594 / 1.612 | 0.908 / 0.926 |
| 256 B / rehash / normal | 0.845 / 0.838 | 1.051 / 1.053 | 0.922 / 0.920 | 1.232 / 1.227 | 1.026 / 1.018 |
| 256 B / rehash / retained | 0.882 / 0.879 | 1.051 / 1.055 | 0.949 / 0.944 | 1.241 / 1.235 | 0.942 / 0.932 |

| Payload / steady path / mode | Library / planned WF | Library / dense WF | Library / native sparse C | Library / planned sparse C |
|---|---:|---:|---:|---:|
| 8 B / replace / normal | 0.967 / 1.000 | 0.807 / 0.827 | 1.228 / 1.276 | 1.014 / 1.014 |
| 8 B / replace / retained | 0.943 / 0.935 | 0.824 / 0.825 | 1.288 / 1.262 | 0.941 / 0.932 |
| 8 B / churn / normal | 1.009 / 1.009 | 0.657 / 0.650 | 0.981 / 0.990 | 0.931 / 0.899 |
| 8 B / churn / retained | 1.003 / 0.996 | 0.862 / 0.853 | 1.051 / 0.996 | 0.894 / 0.891 |
| 256 B / replace / normal | 0.823 / 0.817 | 0.877 / 0.871 | 2.119 / 2.089 | 1.297 / 1.254 |
| 256 B / replace / retained | 0.857 / 0.857 | 0.832 / 0.830 | 2.353 / 2.327 | 1.227 / 1.239 |
| 256 B / churn / normal | 0.898 / 0.904 | 0.764 / 0.775 | 1.335 / 1.398 | 0.975 / 0.974 |
| 256 B / churn / retained | 0.914 / 0.941 | 0.807 / 0.806 | 1.249 / 1.287 | 0.867 / 0.864 |

The library improves wide rebuild versus the original staged source, but
that comparison changes both public result shape and migration body. It
does not isolate the benefit of inlining exchange. The library remains
21–30% slower than dense for wide growth and about 5% slower for wide rehash;
wide replacement
still costs more than twice the native sparse control. Small scalar
differences remain subject to clock quantization and cohort variation.

The library has exactly the staged enum allocation totals in the earlier
table: four requests across two growth traces, three across the two-rehash
trace, and the same 24/272-byte bucket strides. At capacity 64, replace/churn
request one backing, with requested and peak bytes both 1552 for scalar or
17424 for wide values. Rehash's simultaneous old/new backings still peak at
196640/2228256 bytes versus planned sparse's 135200/1151008. Fewer requests
do not imply a lower peak for that operation.

Optimized wide inline rebuild has only the 272-byte pending local, with no
Put result storage, result clear or private exchange call. It keeps a
272-byte pending clear per live owner, old Pair-to-pending and pending-to-
destination transfers of 264 bytes, an unconditional old-destination read,
and a Filled-only restaging transfer. New cells still clear 264 inactive
payload bytes. Both native modes preserve those clears and destination
reads. Public compact put/try-put/exchange/remove instruction bodies match
the earlier compact control after structural type/attribute expansion and
namespace/instance/metadata-ID/comment normalization; this is instruction
text correspondence, not metadata-graph equality or a whole-growth claim.
The public entry/caller copies therefore remain, including the 264-byte
Pair then 256-byte value projections on replace/remove.

The exact library source is
`86d05b964168b8131eb25b495d1540cfbfffbf547bf926c3e5a062811162ee64`;
adapter `ff3b3e90a98436d09a3c232edf4d09cf94cf6dae890a75b0132b265a20198d05`,
C driver `959092c98d52d457d1fb0e4f8fb39858d2c60d272925fec5c823aa80916f08f2`,
and Makefile `8fe96a36217091a9c8ba85855b2039be9ffd83139ac9545c87ae9fd74759fa91`.
The same v0.67 compiler is used; all twelve runtime objects are byte-identical
to the preceding build. Old candidate sources are unchanged.

| Actual-library image artifact | SHA-256 |
|---|---|
| Raw WF | `8e301269601966236cf25cab5a7f800153e65bef6506eee08d33167bbd20dd9b` |
| Normal optimized WF | `2b52e6af605a46d8e24840994b5bcd98cce5b4c62eeceec81e22b563e5ca1249` |
| Retained optimized WF | `181893a5e631a8d7a33cf8f1395db9e9c3b71ebe3c5341b54cb9733462b39630` |
| Normal optimized C | `b32ded46396d24a7ae7cead506ad3d6109fdfa22b6bf41f9519f5d6eb83a0dc3` |
| Retained optimized C | `ba336022dbda8e839a4748f2bc203d742d7aef63be6b9ba60ddc59cdb4f04b43` |
| Normal executable | `88afe95bee5cb3eb99b9053b4e68582b6fbd76d03093928c8eb37a2b6a39efeb` |
| Retained executable | `54782b5d5d6f161637cb89d48e3915940fc05ef57b189ec002e59e50195e3bb3` |

[The library archive](measurements-library.csv.gz) has 3,872 data rows,
gzip SHA-256 `41507cf657fc9afb7ee735738556337f805ed307ed90629042f822a24e6fbd90`
and uncompressed SHA-256
`9795715a9d2b19d66752f576138b0b57cf6b958d9c0fcd441ddcc1fbe6b209b1`.
Its unchanged seventeen columns and cohort/mode order allow exact recovery
of each input CSV. Reproduce with `MEASURE_SET=library SOURCE_SHAPE=original`
on the existing `measure` target; `original` describes the comparison
sources, while the tested library retains its explicit compact label. The
following controlled comparison supersedes this trial as the selection
evidence; these samples remain evidence for the identified inline source.

### Constant-interface comparison and selected helper body

To separate public result compaction from migration inlining, a control
restores the shared exchange helper while keeping the compact public API
and every source byte outside rebuild unchanged. Its source SHA-256 is
`772da5d8755c902916f3dee7ed8dfc472bf552ac7f4048aaee238034a9f53f0a`.
Construction took 20.70 s and `check-costs` took 1.55 s, again passing 22,896
traces per mode, ten policy chains and retained-call assertions. The original
three-mode ordinary/observed caller had already passed with 28 allocations.
C optimized modules and the runtime objects are byte-identical to the inline
image. No private helper receives a `noinline` attribute.

The two images then ran in ABBA order: inline normal/retained, helper
normal/retained, helper retained/normal, inline retained/normal. All eight
invocations completed in 5.41 s, yielding 7,744 validated samples. Each image
has its own 3,872-row archive because its C rows have the same `c-shared`
label. Independent validation checked all allocation/peak formulas,
cross-image content checksums, complete groups and exact CSV reconstruction.
The minimum times are 39/40 microseconds for inline/helper; both retain the
1-microsecond clock quantum. No samples or tails were discarded.

`I/H` below pairs equal seeds across the two images. `C I/H` uses the native
ascending sparse control in the same samples; the normalized statistic is
the median of `(I library / I C) / (H library / H C)`, not a ratio of medians.
Each cell still gives cohort zero / cohort one.

| Rebuild / mode | Raw inline/helper | C inline/helper | C-normalized inline/helper |
|---|---:|---:|---:|
| 8 B grow / normal | 0.987 / 1.006 | 0.987 / 1.007 | 1.006 / 0.997 |
| 8 B grow / retained | 0.958 / 0.995 | 0.982 / 0.997 | 0.984 / 0.998 |
| 8 B rehash / normal | 1.018 / 1.019 | 1.024 / 1.023 | 1.000 / 1.006 |
| 8 B rehash / retained | 0.998 / 0.996 | 0.994 / 1.005 | 1.006 / 0.999 |
| 256 B grow / normal | 0.941 / 0.981 | 0.922 / 1.008 | 1.045 / 0.971 |
| 256 B grow / retained | 0.973 / 0.984 | 0.984 / 1.034 | 1.010 / 0.979 |
| 256 B rehash / normal | 0.963 / 0.992 | 0.967 / 1.001 | 1.000 / 0.991 |
| 256 B rehash / retained | 0.986 / 1.016 | 0.993 / 1.009 | 0.984 / 0.987 |

The registered independent-inline-benefit criterion is not fully met.
The raw wide-growth medians favor inline, but their control-normalized signs
change between cohorts; the unchanged C control itself varies. Wide-growth
raw per-seed I/H ratios span 0.760–1.373 across the four groups, while helper
whole-trace medians span 868–964 microseconds and individual samples span
863–1182 microseconds. Those ranges include seed-dependent work as well as
execution variation; they are not an estimate of scheduler noise alone.
The result does not justify assigning the earlier combined improvement to
the inline body. The selected first-library body therefore uses the shared
helper; the inline rewrite remains a measured alternative, not a production
optimization selected from this run.

IR explains what changes without predicting its speed. The helper rebuild
adds a 272-byte Put local, an exchange call and an Inserted result clear of
272 bytes; only Returned retries copy 264 bytes from the result to pending.
Inline removes that result path, while keeping fresh-cell/pending clears,
the common owner transfers and destination reads described above. The
helper's native local stack reservation is 672/656 bytes in normal/retained
mode, versus inline's 416/464; exchange reserves another 64 bytes while
called. These are function-local reservations, not whole-program peaks.
Instruction grouping also changes, so even a stable timing difference would
not identify the result clear alone. Public put/try-put/exchange/remove and
the wide trace's own optimized instructions match between these images
after metadata-ID normalization; their called rebuild implementation differs.

The following selected-helper estimates compare implementations within the
same helper image. They keep the original bucket-size/allocation tradeoffs,
and do not imply a universal winner.

| Payload / rebuild / mode | Helper / planned WF | Helper / dense WF | Helper / old staged WF | Helper / native descending C | Helper / staged C |
|---|---:|---:|---:|---:|---:|
| 8 B / grow / normal | 0.802 / 0.792 | 0.955 / 0.948 | 1.000 / 0.994 | 1.019 / 1.013 | 1.013 / 1.017 |
| 8 B / grow / retained | 0.824 / 0.828 | 0.915 / 0.943 | 0.986 / 1.005 | 1.098 / 1.093 | 0.964 / 0.920 |
| 8 B / rehash / normal | 0.882 / 0.899 | 0.922 / 0.928 | 0.985 / 0.985 | 0.947 / 0.939 | 0.967 / 0.977 |
| 8 B / rehash / retained | 0.938 / 0.926 | 0.965 / 0.969 | 0.995 / 0.998 | 1.026 / 1.018 | 0.969 / 0.964 |
| 256 B / grow / normal | 0.926 / 0.934 | 1.341 / 1.303 | 0.913 / 0.905 | 1.689 / 1.663 | 1.131 / 1.140 |
| 256 B / grow / retained | 0.928 / 0.931 | 1.257 / 1.262 | 0.903 / 0.896 | 1.642 / 1.637 | 0.937 / 0.947 |
| 256 B / rehash / normal | 0.848 / 0.866 | 1.073 / 1.066 | 0.949 / 0.946 | 1.242 / 1.255 | 1.048 / 1.046 |
| 256 B / rehash / retained | 0.891 / 0.886 | 1.071 / 1.071 | 0.952 / 0.950 | 1.259 / 1.262 | 0.949 / 0.948 |

| Payload / steady path / mode | Helper / planned WF | Helper / dense WF | Helper / native sparse C | Helper / planned sparse C |
|---|---:|---:|---:|---:|
| 8 B / replace / normal | 0.966 / 1.000 | 0.814 / 0.827 | 1.326 / 1.234 | 1.014 / 1.024 |
| 8 B / replace / retained | 0.898 / 0.952 | 0.833 / 0.820 | 1.295 / 1.300 | 0.952 / 0.958 |
| 8 B / churn / normal | 1.000 / 0.976 | 0.663 / 0.661 | 0.959 / 0.899 | 0.888 / 0.853 |
| 8 B / churn / retained | 0.993 / 0.971 | 0.809 / 0.861 | 1.028 / 1.004 | 0.902 / 0.892 |
| 256 B / replace / normal | 0.849 / 0.849 | 0.901 / 0.895 | 2.090 / 2.108 | 1.253 / 1.282 |
| 256 B / replace / retained | 0.855 / 0.845 | 0.820 / 0.822 | 2.307 / 2.351 | 1.253 / 1.222 |
| 256 B / churn / normal | 0.901 / 0.910 | 0.775 / 0.779 | 1.312 / 1.312 | 0.955 / 0.985 |
| 256 B / churn / retained | 0.910 / 0.916 | 0.806 / 0.775 | 1.254 / 1.336 | 0.862 / 0.864 |

Allocation requests, bytes and peaks are identical between the two compact
images and the earlier staged enum rows. In particular, same-capacity
rehash still retains both backings, and wide growth remains 26–34% slower
than dense in these helper-image traces. The steady-state wide replacement
gap versus native C remains 2.090–2.351, alongside the unchanged public/caller
aggregate transfers. Those costs are retained evidence, not removed by the
source selection.

| Helper image artifact | SHA-256 |
|---|---|
| Raw WF | `1660b310a4eed700bed52feaa0af60a4780616117460f354005549f89c97d7ae` |
| Normal optimized WF | `3caf363513f77b18842b98ede84b837e4d8e92f2548c245b376e36c45fea6eb3` |
| Retained optimized WF | `023205e0bb82df9ebcc0343469ac1e03c21ca8c7f33276d1c87ca9aa38b438e4` |
| Normal executable | `5d92456ccdd7427ed0a237f56fe66943a0b2c4d77d50f68cc4ead7343cb208f6` |
| Retained executable | `52968598a329c9850f17cdc6b89e2c1211c901deb09039264fa066ad92968f5a` |

The inline image's identities are unchanged from its preceding trial. Both
control archives retain the native driver's seventeen columns; each has
3,872 rows and restores its four input CSVs by `(contract,cohort)` filtering.

| ABBA archive | gzip SHA-256 | Uncompressed CSV SHA-256 |
|---|---|---|
| [inline](measurements-library-inline-control.csv.gz) | `29b6dc4a6555363554386db397987853442b0143857225c0c309a19d09b2cb7b` | `5ef61d9c7331dd816b18081aeaa6d60744601046c95a2c8861175faad85e221c` |
| [helper](measurements-library-helper-control.csv.gz) | `2b034302b909debe26c224e3ea8e38d054e36f5d42a95820c8af1196f48304ca` | `147eb5cc3b7cdc7f21597196a268cc47570dcf06b696c09d97e272028c99aa08` |

The selected library now contains the measured helper source directly.
[inline-rebuild.patch](inline-rebuild.patch), SHA-256
`c4bf0557159a5c891bedd141895949749c02e5d5518154178c5730c47bcfb07f`,
reconstructs the rejected inline source without another library copy.
`inline-source` applies it with zero fuzz, `build-inline` builds the same
comparison harness, and `check-inline` runs its normal/retained oracle and
retention checks. The generated source was verified byte-identical to the
measured `86d05b96` source. Keep this overlay only while it provides the
constant-interface comparison; a successor may retire it with this checkpoint
retained in Git. The replay-target-only Makefile revision has SHA-256
`2dc81c3e5d90ed9624c527d33d7d1710c38c30c540915c5246c1461c1bb5d588`;
it does not alter the compiler flags, trace or instrumentation used above.

For clarity, the selected helper's wide-growth ratio to **ascending** native
sparse migration is 1.560–1.676; to **descending** native sparse migration it
is 1.637–1.689. The descending control is the closer migration-direction
floor and is used in the table. Its cleanup remains ascending, while WF and
source-shaped staged C clean up in descending order. The floor also avoids
the source's staging/result work and inactive-payload clearing, so the whole
gap cannot be assigned solely to the compiler or to layout.

## Same-source inactive-storage lowering comparison

The owner rejected this optimization for production after the comparison
and its bounded SSA-construction follow-up failed their selection criteria.
Production retains whole-aggregate destination and empty-window clearing.
The measurements, migrated fixtures and replay inputs remain as negative
evidence; the prospective protocol below retains its original conditions.

This prospective criterion is fixed before implementing or timing the next
compiler candidate. Following the requested rebase, the baseline is merged
main `345e2966a`; this baseline update precedes every timing sample and leaves
the workload and selection criteria below unchanged. The question is
whether omitting initialization of inactive storage improves the maintained
owning map without changing its source contract, active-value semantics or
representation. The earlier source comparisons identify retained stores but
do not establish their share of elapsed time.

### Fixed inputs and attribution boundary

Build A with the baseline compiler and B with only the candidate compiler
change. Both compile byte-identical current `lib/containers/hash-map.wf`,
`map-library.wf`, comparison sources and C driver, using the same target,
Clang flags and runtime objects. Preserve the shared exchange helper,
24/272-byte bucket and compact Put layouts, allocation policy, hash functions,
seeds and complete operation traces. Do not combine this comparison with
`inline-rebuild.patch`, enum overlay, a different result ABI or a separate
aggregate-transfer optimization. Record source, compiler, raw/optimized IR,
native executable and runtime identities; the C optimized modules must agree
between A and B.

Reuse normal and retained modes with the existing public-operation and
callback retention rules; private helpers remain ordinarily optimizable.
Inspect optimized IR and native instructions for initialization stores,
actual payload transfers, retained calls and local stack reservations.
Layout, allocation requests/bytes/peaks and cleanup must remain equal.
Removing a clear can change subsequent optimization: the experiment then
measures the entire compiler change, not the clear's isolated latency.
Neither fewer copy intrinsics nor the normal/retained time difference measures
copy or call time by itself.

### Selected workloads and checks

Use the existing `map-costs.c` trace bodies, independent key-ID/content oracle,
allocation accounting and seventeen-column CSV. Add only an explicit
measurement-set selection in that driver and its existing Makefile caller.
The earlier `library` set omits hit, edit and setup traces, while `primary`
excludes the maintained library; neither alone supplies this comparison.
Every row below uses the mixed hash distribution and seven-eighths occupancy.

| Capacity / live count | Payloads | Paths | Workload cells |
|---|---|---|---:|
| 4096 / 3584 | u64 and 256-byte record | grow, rehash, setup-cleanup | 6 |
| 64 / 56 | u64 and 256-byte record | replace, churn | 4 |
| 64 / 56 | u64 and 256-byte record | hit, edit-first-word | 4 |

All fourteen cells compare the actual WF library with the existing native
ascending sparse control. The four grow/rehash cells additionally include
the existing source-shaped staged C control. This gives 32
cell/implementation combinations. Ascending C supplies the common timing
control; its migration/cleanup order differs from WF, so its entire gap is
not compiler cost. Staged C retains the source algorithm but does not
initialize inactive payloads. No dense or per-bucket-window competition is
reopened by this experiment.

The two preselected primary consumers are the 4096-capacity record grow and
rehash cells. Setup includes filling and cleanup and prices construction as a
whole. Hit and edit test paths whose hot loops do not need the targeted empty
payload initialization; their setup can still benefit. Do not subtract setup
time to claim isolated operation latency. Preserve existing rounds and trace
repetitions, eleven seeds, two reversed cohorts, implementation rotation and
both helper modes. Each image contributes `32 * 11 * 2 * 2 = 1408` samples.

Keep the complete existing correctness matrix, rather than narrow correctness
to the timing cells. It covers empty/singleton/irregular/full capacities,
three round counts, three seeds and two hash distributions. The formal
`hash-map-program.wf` caller already covers zero capacity, unit/unit pairs,
must-consume Box-owning keys and values, hostile equality, capacity refusal,
owned callback results and all operation outcomes. Run its existing three
CLI configurations with ordinary execution and the exact 28-allocation
observer ledger. Zero-size cases are semantic and emitted-code controls,
not new sub-clock-resolution timing workloads. Active tags, descriptor words
and payloads must remain defined through construction, consumption, every
selected variant and ordinary call boundaries.

### Null control, order and selection rule

First execute a null comparison whose two labelled sides A1 and A2 invoke
the exact same baseline executable for each mode. Then execute the real A/B
comparison in the same order. Each comparison has these four groups:

1. First side, cohort zero: normal then retained.
2. Second side, cohort zero: normal then retained.
3. Second side, cohort one: retained then normal.
4. First side, cohort one: retained then normal.

The driver still rotates implementation order for each sample and reverses
it between cohorts. Keep separate image identities and every raw sample,
including tails. The null comparison has 2816 rows and A/B another 2816,
for 5632 initial timing rows. A/A requires no second build. Do not treat
the null halves as independent compiler variants.

For equal seeds and trace sizes, report raw B/A, native-C B/A, and
`(B_WF / B_C) / (A_WF / A_C)`. Take medians of these per-sample ratios,
separately by cell, mode and cohort, rather than ratios of independent
medians. Report staged C separately on the four rebuild cells. Its result
qualifies algorithm cost, not a replacement normalization selected after
seeing which control favors the candidate.

The initial screening margin for each cell and mode is the maximum of:

- 3%, the preselected materiality scale for this bounded experiment;
- the largest absolute deviation from one of either cohort's raw or
  C-normalized A/A median ratio for that cell and mode;
- the timer-quantization envelope. Let `q` be the observed clock quantum and
  `t_min` the shortest participating interval in that cell and mode across
  null and A/B samples. For `t_min > q`, use
  `((t_min + q) / (t_min - q))^2 - 1`, covering the four intervals in a
  normalized ratio; an interval at or below `q` cannot select an improvement.

This screen is not a confidence interval or a universal parity threshold.
At least one preselected primary consumer must improve beyond its margin
in both modes and both cohorts, with raw and C-normalized ratios agreeing
in direction, before claiming a repeatable runtime benefit. No preselected
cell may retain an unexplained regression beyond its own margin. Publish
the before/after WF/C gaps without assigning all residual cost to stores,
layout or helper boundaries. Initialization counts alone cannot select
production behavior.

Any correctness, defined-active-state, layout, allocation or ownership
failure stops performance selection and is reported. If an apparent gain
cannot be distinguished from null/control drift, directions reverse, or a
material regression needs confirmation, permit at most one replay of this
same full protocol with the outer invocation order reversed. Keep its
results separate; do not add workloads, change seeds or pool away a reversal.
Persistent sign changes or null variation comparable to the proposed gain
mean the timing result is inconclusive and further sampling stops. If no
relevant optimized code changes, record that outcome without inferring a
speedup or inventing another consumer to rescue the candidate.

### Other consumers and execution budget

For Slab and Vector, first compare same-source A/B optimized IR and native
instructions and run their complete existing correctness oracles. They test
one-slot storage and existing consumption paths, respectively. If their
relevant optimized bodies are unchanged, do not add timings. If this compiler
change affects those bodies, reuse the affected consumer's complete existing
normal/retained timing matrix and C controls; do not select only favorable
lengths or payloads. Report this additional scope and sample count separately.
The known Slab aggregate-return/layout costs and Vector short-cycle costs
remain independent questions unless this controlled comparison resolves them.

Across this bounded program experiment, including any triggered consumer
checks and the one permitted replay, allow at most 180 seconds for experiment
construction, 60 seconds for correctness execution and 60 seconds for timing.
These are investigation limits, never source-acceptance budgets. Exceeding
one triggers investigation and a report before further heavy commands; it
does not justify dropping cases or overlapping another run. Historical map
construction was about 21 seconds per image, cached cost checks about
1.5 seconds, fixture-inclusive checks about 14 seconds, and the prior
7744-row ABBA timing about 5.4 seconds; these guide cost monitoring rather
than promise current durations. Guard heavy commands, separate construction
from execution, and record Rust compiler construction and the final canonical
gate separately from these program budgets. No new script, experiment
directory, library copy or daily-gate dependency is needed.

### Syntax migration and preparation

Main's v0.68 grammar writes value parameters and results as `name: T`, replacing
the earlier `name: own T` source annotation without changing their value mode.
The existing executable Map, Slab and Vector research inputs, together with
the three Map replay patches, received only that token removal. Their line
counts, bodies, contracts, call shapes and trace sizes are unchanged. Each
migrated file is byte-identical to its earlier contents after replacing
`: own ` with `: `, and the staged, compact and inline replay patches apply
with zero fuzz. The baseline and candidate will consume the same migrated
sources; the old compiler cannot serve as a v0.68 baseline. Earlier dated
measurements and their compiler/source conditions remain unchanged.

The driver's new `inactive` set implements the fourteen cells and thirty-two
cell/implementation combinations above. `make measure-inactive` selects it
through the existing measurement caller and records each mode's minimum
positive monotonic-clock delta across 10,000 consecutive probes. That observed
quantum supplies the preselected timer envelope; it neither changes the
seventeen-column samples nor replaces the unchanged trace and allocation
oracles. All earlier measurement sets retain their selection.

For this invocation, use the largest of the participating image/mode minimum
positive clock deltas as `q`. If the prescribed replay is triggered, reverse
the outer side order to B/cohort 0, A/cohort 0, A/cohort 1, B/cohort 1 (and
the corresponding A2/A1 labels for the null), retaining the cohort-specific
helper order and reporting the replay separately. These operational details
are fixed before collecting any timing sample.

Before the rebase, a baseline using compiler SHA-256
`c57f989b1b0073769d4999266f3353143d758b2f400c26a74e8d260b684afe33`
completed Map construction in 20.01 seconds and the full cached correctness
target in 14.34 seconds. These are obsolete preparation costs, retained in
the investigation's construction/execution accounting; they are not timings
of the program comparison and establish no post-rebase result. No performance
samples were collected from that image.

### Completed comparison: gains with unresolved regressions

The candidate does **not** meet the prospective selection rule. Both primary
wide rebuild consumers improve beyond their screening margins in both helper
modes and cohorts, including the one reversed replay, but normal wide
replacement has a repeatable regression. Slab adds a retained scalar lookup
regression. These results support a bounded rebuild benefit, but the owner
rejected accepting the recorded regressions and retained baseline emission.
No further sampling was performed after the permitted Map replay and the
triggered Slab matrix.

The trial ran on 2026-09-23 UTC on an Apple M1 Pro, eight CPUs, 32 GiB RAM,
macOS 26.6.2 (25G83), using Apple Clang 21.0.0
(`clang-2100.3.34.2`), target `arm64-apple-darwin25.6.0`, and the existing
`-O2` recipes. A uses the compiler built from clean main `345e2966a`; B uses
the compiler built from candidate revision `483f64401`. Both consume the
same migrated WF sources, C drivers, helper-retention transformations and
the exact same twelve runtime object files, in the same link order. The
normal and retained optimized C modules are byte-identical between A and B
for Map, Slab and Vector.

The Map correctness target passed for both images: each helper mode ran
22,896 checked traces and ten C policy chains; all existing ordinary and
observed fixture configurations passed, including the maintained library's
three 28-allocation ledgers and the observer's concurrent/negative controls.
Slab passed 864 executions per mode and image; Vector passed 6,300 per mode
and image. Their existing retained-call checks passed. Every timing sample
also passed its independent content/outcome and allocation oracle. Across
each paired matrix, trace sizes, checksums, requests, requested bytes and
peak bytes match exactly for equal implementation/seed/configuration keys.
No allocation, ownership or correctness failure occurred in these cases.

#### Map timings

The initial A/A plus A/B protocol produced 5,632 samples. The repeatable
normal wide-replacement regression triggered exactly one full replay, with
the predeclared reversed side order, producing another 5,632 samples.
Each of the eight side images has exactly 1,408 rows: 32 implementation/cell
combinations, eleven seeds, two cohorts and two helper modes. All eight clock
probes reported a 1,000 ns minimum positive delta. The shortest participating
interval was 31,000 ns; none was at or below the observed quantum. The
quantization screen is consequently appreciable for the shortest scalar
cells, and it is not replaced by a universal 3% threshold.

The [complete paired summary](measurements-inactive-summary.csv) retains
all fourteen cells, both cohorts, both modes and both protocols separately.
Its entries are medians of eleven equal-seed ratios, never ratios of
independent medians. It includes raw WF B/A, ascending C B/A, the registered
C-normalized B/A, before/after WF/C gaps, null ratios, minimum intervals and
screening margins. Staged C and WF/staged-C ratios remain separate on all
four rebuild cells; staged C is never substituted as the normalizer.

The principal 256-byte results below show **cohort 0 / cohort 1**. Ratios
below one favor B. The displayed values are rounded; the screening used
the unrounded ratios in the summary.

| Path / mode | Initial WF B/A | Initial C B/A | Initial normalized | Replay WF B/A | Replay C B/A | Replay normalized |
|---|---:|---:|---:|---:|---:|---:|
| grow / normal | 0.690 / 0.712 | 0.974 / 0.992 | 0.719 / 0.719 | 0.718 / 0.710 | 1.000 / 0.996 | 0.717 / 0.712 |
| grow / retained | 0.766 / 0.725 | 1.002 / 0.945 | 0.764 / 0.765 | 0.797 / 0.759 | 0.983 / 0.989 | 0.780 / 0.761 |
| rehash / normal | 0.859 / 0.878 | 0.972 / 0.990 | 0.908 / 0.891 | 0.891 / 0.898 | 1.001 / 1.004 | 0.887 / 0.897 |
| rehash / retained | 0.910 / 0.900 | 1.002 / 0.996 | 0.908 / 0.900 | 0.906 / 0.903 | 1.005 / 1.001 | 0.911 / 0.903 |
| replace / normal | 1.091 / 1.092 | 1.000 / 1.008 | 1.088 / 1.075 | 1.093 / 1.090 | 1.012 / 1.000 | 1.075 / 1.090 |
| replace / retained | 0.957 / 0.915 | 0.996 / 0.955 | 0.962 / 0.957 | 0.896 / 0.938 | 0.963 / 1.000 | 0.957 / 0.939 |

The primary screens are 3% except retained wide grow, whose null variation
raises its margin to 5.386% initially and 4.197% on replay. Wide replacement's
normal-mode screen remains 3% in both trials; its regression exceeds that
margin in both cohorts each time. Retained replacement improves, so pooling
the helper modes would hide a real distinction.

Normal scalar churn is another unresolved observation: initial raw B/A is
1.028 / 1.041 and normalized B/A is 1.014 / 1.009, against a 3.965% screen;
replay raw B/A is 1.018 / 1.116 and normalized B/A is 1.023 / 1.058, against
a 3% screen. The replay's second cohort is materially slower by both measures,
but the initial evidence is weaker. Scalar setup also has one replay raw
regression accompanied by C drift (1.046 raw, 1.002 normalized). These remain
visible without extending the sampling or claiming a common causal account.

The residual wide-grow WF/ascending-C gap across the separately retained
cohort medians is 1.189–1.197 in normal mode and 1.265–1.285 retained.
Normal wide replacement changes from 2.078–2.091 in A to 2.259–2.286 in B;
retained replacement changes from 2.310–2.342 to 2.159–2.232. These are
whole-trace gaps. Ascending C differs in migration/cleanup order, and neither
its gap nor the helper-mode difference is an isolated store, copy or call
cost. No setup time was subtracted.

#### Emitted-code attribution and other consumers

The full Map harness's raw IR changes from 371 whole-aggregate zero stores
to zero, while explicit active tags and descriptors remain. The change also
affects subsequent optimization. In particular, normal wide `try_put` changes
from 112 native instructions with a 288-byte local frame and a private
`exchange` call (64-byte frame) to 272 instructions with a 784-byte local
frame and the exchange inlined. Its matched-key branch performs five
256-byte transfers through two temporaries. The public `put` keeps its two
`try_put` sites and 1,136-byte frame. This identifies concrete optimizer
fallout accompanying replacement's slowdown; it does not isolate which
instruction change causes the measured difference. The focused lowering
analysis is recorded with the [compiler investigation](../../../investigations/containers-and-resources/X1-LIBRARY.md).

Slab's relevant optimized bodies change, triggering its complete existing
matrix: both 8-byte and 256-byte payloads, counts 16/256/4096, prefilled lookup,
reuse churn and setup/cleanup, all three implementations, eleven seeds, two
internal cohorts, both modes and both existing outer runs. Each side has
4,752 rows; its A/A plus A/B comparisons retain 19,008 rows in total. The
existing outer run orders are preserved, with the four side groups A/run 0,
B/run 0, B/run 1, A/run 1. There was no additional Slab replay.

The [complete Slab paired summary](../slab-library/measurements-inactive-summary.csv)
retains all four run/cohort groups per cell and mode. Its `screening_margin`
is a descriptive application of the same 3%/null/quantum calculation across
those groups and both C normalizations, using the observed 1,000 ns quantum;
it is not an additional prospectively selected Slab acceptance rule.
Ranges below span the separate group medians, not pooled samples:

| Slab observation | Raw WF B/A | Window-C-normalized B/A | Tagged-C-normalized B/A |
|---|---:|---:|---:|
| retained scalar lookup, count 16 | 1.067–1.099 | 1.098–1.105 | 1.098–1.111 |
| retained scalar lookup, count 256 | 1.051–1.106 | 1.104–1.109 | 1.105–1.107 |
| retained scalar lookup, count 4096 | 1.038–1.079 | 1.045–1.082 | 1.048–1.077 |
| normal wide churn, all three counts | 0.886–0.948 | 0.911–0.922 | 0.906–0.925 |
| retained wide churn, all three counts | 0.905–0.985 | 0.918–0.952 | 0.925–0.951 |
| normal wide setup, all three counts | 0.897–0.929 | 0.915–0.935 | 0.913–0.930 |
| retained scalar setup, all three counts | 0.817–0.883 | 0.820–0.873 | 0.816–0.881 |

Retained scalar lookup is slower in every run/cohort group at all three
lengths, with both unchanged C controls agreeing on direction. Its null
normalized medians range 0.984–1.006 for window C and 0.967–1.004 for tagged C.
This is a separate regression to explain, not evidence of blanket Slab
improvement. Retained wide setup has raw sign changes (0.964–1.081), while
its window-C-normalized medians span 0.986–1.011; its largest raw slowdown
coincides with C drift. Other small/control-sensitive differences remain in
the complete summary and all their raw tails remain in the archive.

Vector passes its complete correctness matrix. In each mode, all 47 inspected
relevant WF native function bodies are unchanged, including branch operands
and positions, despite SSA naming/order differences in optimized IR.
Accordingly no Vector timing was added. The existing Vector short-cycle and
Slab layout/result-ABI questions remain unresolved by this comparison.

#### Retained data, identities and cost accounting

The [Map raw archive](measurements-inactive-raw.tar.gz) contains eight
1,408-row CSVs named `{initial,replay}-{null,ab}-{A,B}.csv`, each retaining the
driver's seventeen columns, plus all clock observations and `identities.sha256`.
In `null` files A and B mean the labels A1 and A2 invoking the identical A
executable; in `ab` files they mean the distinct compiler images. Filtering
by `(contract,cohort)` restores each original invocation. The
[Slab raw archive](../slab-library/measurements-inactive-raw.tar.gz) contains
four 4,752-row `{null,ab}-{A,B}.csv` files in its existing fifteen-column
Makefile format, including `run`; `(run,contract)` restores its invocations.
No row, seed, tail or cohort was discarded. The two summaries have 112 Map
and 144 Slab rows, respectively. The archives contain source, compiler,
raw/optimized IR, executable and shared-runtime SHA-256 identities, including
the untimed Vector comparison in the Map manifest.

| Artifact | SHA-256 |
|---|---|
| A compiler, main `345e2966a` | `cbffd4dd1ae8641ef03790457181188988bf70cc4af1a53c50c1f406307bb7f9` |
| B compiler, candidate `483f64401` | `4d6afc99e1882a70f1ad7caafe9d6127095fffd59488bc05d2ce7b657060f3a4` |
| Map raw archive | `505b583ce5163014d386915228c6268a98ecb7c00a6896c182f4c13634bae83b` |
| Map paired summary | `31af0ac9649f36b7d51871e74ec1a89ccc89b062cad5ea0447988211cf35937b` |
| Slab raw archive | `41f97d4c512b5d2a7713ee428c7f02cd07af5d53079f13dd2fef1351c559e3a1` |
| Slab paired summary | `687578de3f0f5ce5e4c21733a257f6f0464b8a251dfbc5e8f9a643d602ea0218` |

All program construction, correctness and timing commands used the shared
verification guard sequentially. Wall seconds below include the obsolete
pre-rebase preparation, but only the new A/B images contributed samples:

| Stage | Construction | Correctness execution | Timing |
|---|---:|---:|---:|
| Obsolete pre-rebase A | 20.01 | 14.34 | 0 |
| v0.68 A Map | 19.54 | 14.65 | 0 |
| v0.68 B Map | 19.76 | 14.35 | 0 |
| Slab and Vector, A and B | 4.47 | 3.37 | 0 |
| Map initial A/A plus A/B | 0 | 0 | 2.84 |
| Map single reversed replay | 0 | 0 | 2.82 |
| Slab complete A/A plus A/B | 0 | 0 | 18.40 |
| Total / investigation limit | 63.78 / 180 | 46.71 / 60 | 24.06 / 60 |

Rust compiler construction is separate: the clean rebased A compiler took
47.75 seconds; the verified B binary and unit harness took 47.10 and 83.38
seconds. A shared-target freshness error initially retained the A binary when
building B; its identical hash exposed that error before B program construction
or timing, and the package was cleaned and rebuilt before freezing B.
The complete canonical gate belongs to the enclosing compiler change and is
reported separately; these experiment checks do not replace it.

### Rejected SSA construction follow-up and replay

The [bounded follow-up](../../../investigations/containers-and-resources/X1-LIBRARY.md#first-result-and-bounded-constructor-follow-up)
formed destination aggregates from a poison seed after capturing every active
operand. It failed its structural screen before correctness execution or
timing: retained scalar Slab lookup grew to 28 successful-path instructions
against the baseline's 23, and wide Map put expanded to 105 LLVM loads and
102 insertvalue operations. Its reduced replacement-copy count did not
override either failure. The owner rejected this form as well.

[inactive-ssa-construction.patch](inactive-ssa-construction.patch) preserves
the complete two-file change as a zero-context overlay from base
`e31d9422f8ea297a39ab02dc486e0c6e82585f99` to the local prototype
`fa50c0d873a6c1f9c48d4a87b98890fd295bf4fc`. The overlay's SHA-256 is
`01c9718fe897f346670504185be859ea232445f6455237103358d8352e58ae79`.
The original measured diff, including context, has SHA-256
`4e4d04400d0ea0ec7c11e25499490a492505b8827f2b828a5cc9ffd39aa21e1f`;
the format change reconstructs the same two source files byte for byte.
It changes `compiler/src/backend/emitter/operations.rs` and
`compiler/src/backend/emitter/places.rs`; no production or correctness target
applies it. Keep this overlay while it supplies the rejected-form replay;
a successor may retire it with this evidence retained in Git.

To reconstruct its source, start at the named base in a separate worktree,
then run `git apply --unidiff-zero --check` and `git apply --unidiff-zero`
there with the overlay's absolute path. The base already contains the first
candidate's window emission;
applying this overlay to the current production compiler would not recreate
the measured prototype. Build its compiler with the gate profile and two
jobs in that worktree's own target directory under the shared verification
guard. Use the existing Map and Slab Makefile recipes with the same migrated
sources, normal/retained transformations, target, Clang flags and shared A
runtime objects. This recovers the untimed structural discriminator, not a
new performance selection. The recorded frozen compiler identity, exact
screen observations and construction costs remain in the linked investigation.

The raw measurement archives above contain samples, clock observations and
identity manifests, not compiler binaries, generated IR or runtime objects.
Replay therefore also needs the pinned compiler revisions and existing
library/runtime sources. Retained ownership regressions continue to cover
dirty linked destinations, selected variants, partial windows and parallel
argument/result cleanup. Assertions requiring omitted aggregate clears or
descriptor-only stores were retired with the rejected optimization; their
replacement checks require baseline clearing and defined active values.
