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

The current Draft implements the provisional complete-root alignment recipe
and ordinary EDIT mask/running-index source forms. The compiler choice remains
[Q206, pending an owner ruling](../../../../design/amendments/independent-local-alignment.md);
this proposed implementation is not a completed HashMap performance target or
an owner-approved compiler selection. The source forms preserve the existing
representation, public contracts and cyclic linear-probe algorithm.

The fresh [actual integrated-candidate comparison](#integrated-candidate-with-the-maintained-caller)
uses the maintained multipath caller and has no qualified before/after gain in
40 cells; all four overall screens remain unqualified. Reserve's recorded bad
clock window invalidates every growth cell, and other drift-invalid cells
remain visible. Overlapping ranges and no separated loss do not prove no
effect or establish nonregression. The earlier outer-caller small-map gains
remain context-specific Q206 grounds, not replicated maintained-caller gains.
The [cleanup-boundary discriminator](#late-cleanup-exposure-native-success-without-qualified-gain)
removes the wide ordered-digest memory pair but does not qualify a timing gain.
Earlier failures and the provisional status of the compiler choice are unchanged.

The new [isolated public EDIT campaign](#isolated-public-edit-hit-targets-pass-misses-remain-open)
qualifies the four hit cells against the selected Rust/C++ peer in one fixed
two-cohort campaign. All four miss cells fail; both large misses also retain
C-attribution drift invalidation. This API window neither supplies a before/after
optimization gain nor replaces the unchanged whole-application performance goal.
Public `hash_map_lookup` already has isolated hit/miss measurement, with its
miss gap still open; `hash_map_find` is private. Separate fresh-insertion and
owned-removal API windows remain missing.
The [EDIT-only two-span follow-up](#edit-only-two-span-probing-rejected)
reduces native body instructions but has no qualified miss gain, three qualified
two-cohort hit losses and zero candidate peer targets; its source stays rejected.
The later [first-probe peel](#first-probe-peel-native-pass-timing-rejection)
passes its narrow native criterion but records no qualified miss gain and a
qualified large-scalar hit loss; its source also remains rejected.
The [fixed-hash sensitivity](#fixed-hash-sensitivity-supplemental-rejection)
likewise supplies no qualified miss gain and does not replace the original
consumer's qualification. [Byte packing](#byte-pack-load-native-only-result)
passes only a native-load screen, with no runtime or map-performance claim.
The subsequent [mirrored-byte sequence](#mirrored-byte-sequence-bounded-checks-rejected-timing)
passes bounded source checks but supplies no qualified miss gain or peer target;
its early native failures and final qualified losses keep the representation unselected.
The [matches-first follow-up](#matches-first-order-native-improvement-timing-rejection)
also fails timing qualification and exposes the conditional hash/equality-law
boundary; it supplies no adoption ground.

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
The initial untimed geometry and accounting stages passed; their observations
and memory-pair selection are recorded below. The subsequent frozen-source
continuation supplies the occupancy timing without changing this criterion.

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

### Initial capacity geometry and selected memory pairs

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

### Frozen-source occupancy continuation

The single registered sweep ran on 2026-09-28, 10:00:16–10:01:08 UTC. It
establishes sensitivity of missing lookup to capacity in both payloads;
it does not select a load policy or explain all remaining native deficits.
All library and harness inputs match
`7abd6bb34746b7b983b83103c68ee429b24859bb`. Construction observed documentation
revision `c8fd35d7b185935bdcef56c50962e480a066a131`; the before/after timing
identity observed `22096b01dbd508d47643a1211a8feb950b26a3f1`, also without a
production-input change. The copied compiler SHA-256 is
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`.
The fresh build directory is
`/private/tmp/whitefoot-map-occupancy-7abd6bb/build`, with
`ABSEIL_PREFIX=/private/tmp/whitefoot-abseil-20260817.0-install`.

The [raw samples](ecosystem-occupancy-samples.csv) contain all 3,520 expected
rows, SHA-256
`5846e907d2656f52a096c65c225d1de04dbca2e8cd483cc128ce9fa1db296dc3`.
The [identity and reduction ledger](ecosystem-occupancy-identities.json)
retains exact commands, direct exits and wall times, source/compiler/toolchain
and object/image hashes, dependency recovery, all memory pairs and numerical
reductions. Its SHA-256 is
`c637466e10102c59629d5847e20325a8dffc0bac86bde0233a5823c4af63ac39`.
These two artifacts belong to this discriminator and remain with it or an
evidence-preserving successor; the older practical samples remain unchanged.

The timed image SHA-256 is
`63a4ba532f15aceb6d37e54c1f0cbf145d31ea85b91084b39e7526a17a51ee32`;
the accounting image is
`87fcd289557208ea247f588c0a93ee3b965e2bd7b7b4b88c1880036fccf56023`.
The 56 recorded repository inputs, copied CLI, 29 construction artifacts,
six recorded toolchain binaries and 718 installed dependency files matched
their frozen hashes before and after timing. Apple Clang 21.0.0, Rust 1.98.1,
libc++ `220106` and Abseil `20260817.0` retain the recorded flags. All ten
fresh ecosystem object/archive files, including both WF objects, match the
retained current-build counterparts byte for byte. Fresh geometry, ordinary
accounting and occupancy accounting match the three existing CSV files
exactly; no duplicate accounting dataset is added.

Dependency recovery preceded construction. The retained Abseil installation
lacked package metadata and most headers. Native CMake configuration of the
exact release, with Release/C++20/tests-off/install-on settings, and its local
install script supplied 373 missing include files and 216 generated `.pc`
files from the original install inventory. The generated `options-pinned.h`
was installed through that native script. Every one of the 129 existing
installed files, including all 94 static libraries, remained byte-identical;
no Abseil library was rebuilt or replaced. Recovered package Cflags, static
libraries and version exactly match the retained configuration before the
existing `-framework CoreFoundation` addition.

The newly fetched official commit archive has SHA-256
`7f4240fe135c0b0dcdd2efa664f1393b1da7e25031e17560515f452182aa0c5e`,
which differs from the historical `db5de644…` construction record. The
fresh tag archive and exact commit archive have identical 1,602 regular-file
payloads, and all 35 surviving installed headers match that source. This
establishes the recovery inputs without validating the absent historical
archive or explaining its different checksum. The ledger retains both fresh
archive identities and this qualification, along with the initial guard
refusal, failed dependency check and failed sandbox network attempt.

Each stage was separate, used the maintained target under the host guard,
and returned status 0 after dependency recovery:

| Target | Wall time, seconds | Observation |
|---|---:|---|
| `ecosystem-build` | 9.451 | Fresh ordinary and accounting images |
| `ecosystem-check` | 1.584 | 18,390 traces per image; checksum, cleanup and eight omitted-reserve controls rejected |
| `ecosystem-occupancy-check` | 0.962 | 1,440 traces per image; 140 geometry observations; seven geometry controls rejected |
| `ecosystem-geometry` | 0.331 | 140 filled-map observations; memory pairs selected before timing |
| `ecosystem-account`, work 262144 | 3.676 | 420 balanced allocation rows, zero final live bytes |
| `ecosystem-occupancy-account`, work 1048576 | 2.521 | 160 balanced allocation rows, zero final live bytes |
| `ecosystem-occupancy-measure`, work 1048576 | 51.879 | One sweep, both cohorts, no rerun |

The measurement has 320 groups of eleven samples, with the required reversed
capacity and implementation order. The existing independent key-ID/content/
outcome oracle checked every warmup and timed trace; all 88 semantic checksum
groups agree across implementations, capacities and cohorts. Sample zero
also agrees with every occupancy-accounting row. Every duration exceeds
1 ms; the minimum is 2.098 ms. Instrumented allocation/geometry ledgers remain
separate observations and do not automatically establish timed allocation
traffic. Different sample seeds change placement; within-group ranges are
not an isolated measure of timing noise.

Each pair below is cohort 0 / cohort 1. The first numeric column reports WF's
4096-slot median in milliseconds; the other columns divide the same payload
and path's larger-capacity WF median by that reference. A dagger marks cohort
ratio spread above 10%, calculated as `100 * (max ratio / min ratio - 1)`;
those comparisons remain descriptive. All four requested capacities are
retained, with population fixed at 3584.

| Payload | Path | WF 4096 median, ms | WF 5120 / 4096 | WF 6144 / 4096 | WF 8192 / 4096 |
|---|---|---:|---:|---:|---:|
| 8 B | `hit` | 12.573 / 11.489 | 0.805 / 0.618 † | 0.576 / 0.456 † | 0.380 / 0.428 † |
| 8 B | `miss` | 34.279 / 34.329 | 0.541 / 0.471 † | 0.447 / 0.417 | 0.361 / 0.358 |
| 8 B | `replace-old-value` | 12.912 / 12.637 | 0.707 / 0.724 | 0.599 / 0.583 | 0.510 / 0.496 |
| 8 B | `edit-first-word` | 12.000 / 11.310 | 0.743 / 0.651 † | 0.573 / 0.570 | 0.465 / 0.475 |
| 256 B | `hit` | 12.262 / 12.083 | 0.746 / 0.718 | 0.506 / 0.597 † | 0.434 / 0.405 |
| 256 B | `miss` | 40.455 / 40.482 | 0.491 / 0.476 | 0.427 / 0.424 | 0.279 / 0.258 |
| 256 B | `replace-old-value` | 75.372 / 76.889 | 1.007 / 0.993 | 0.973 / 0.994 | 0.963 / 0.980 |
| 256 B | `edit-first-word` | 13.186 / 14.142 | 0.763 / 0.645 † | 0.714 / 0.510 † | 0.481 / 0.434 † |

The primary scalar miss medians fall to 12.375 / 12.284 ms, reductions of
63.899% / 64.217%, with 0.888% cohort-ratio spread. Wide miss medians fall to
11.269 / 10.460 ms, reductions of 72.144% / 74.161%, with 7.806% spread.
Both meet the registered reduction, duration and stability criterion. The
change jointly affects occupancy, bucket placement, backing size and complete
setup/cleanup work; these percentages cannot be assigned solely to probing
or to emitted instructions. Nine of the 24 larger-capacity comparisons fail
the spread screen. Wide replacement at 5120 also has mixed median direction;
its 6144/8192 reductions remain below 4% in either cohort.

Fresh geometry selected exactly the six requested-memory pairs above before
timing: scalar WF 6144 for all three native peers; wide WF 8192 for Rust and
Abseil, and WF 4096 for C++. Each native request remains 4096, and all six
byte ratios remain within 1.10. The following table contains every path for
those pairs, again cohort 0 / cohort 1 and with the same dagger qualification.
It compares complete-trace medians, not an isolated lookup or allocation cost.

| Payload | Path | WF / Rust | WF / C++ unordered | WF / Abseil |
|---|---|---:|---:|---:|
| 8 B | `hit` | 2.727 / 1.978 † | 1.494 / 1.122 † | 2.657 / 1.932 † |
| 8 B | `miss` | 5.813 / 5.508 | 3.656 / 2.807 † | 5.999 / 5.555 |
| 8 B | `replace-old-value` | 1.757 / 1.670 | 0.878 / 0.850 | 1.430 / 1.354 |
| 8 B | `edit-first-word` | 2.397 / 2.207 | 1.272 / 1.124 † | 2.374 / 2.182 |
| 256 B | `hit` | 1.677 / 1.544 | 2.234 / 2.222 | 1.849 / 1.704 |
| 256 B | `miss` | 3.972 / 3.697 | 6.922 / 7.548 | 4.140 / 3.721 † |
| 256 B | `replace-old-value` | 2.678 / 2.728 | 2.556 / 2.521 | 2.564 / 2.622 |
| 256 B | `edit-first-word` | 1.775 / 1.676 | 1.764 / 1.846 | 2.017 / 1.919 |

Six requested-memory comparisons fail the spread screen. The scalar C++
replacement comparison has faster WF medians but does not separate every WF
sample below every C++ sample in both cohorts; it is no strict target pass.
The other 23 comparisons have slower WF medians in both cohorts, with the
marked unstable magnitudes left unqualified. These points do not replace any
losing or inconclusive cell in the ordinary practical matrix.

Wide replacement remains a separate cost lead. At the same 8192 slots,
WF/direct-C miss ratios are 1.020 / 0.962 (6.048% spread), while
`replace-old-value` is 2.202 / 2.247 (2.054% spread). At 4096, the respective
ratios are 0.789 / 0.789 (0.021% spread) and 1.920 / 1.964 (2.310% spread).
Thus lowering occupancy greatly reduces misses without removing the wide
replacement deficit. The next layout/probing discriminator and the next
wide-value transport discriminator need separate hypotheses. This sweep
changes no library, compiler, language rule or selected representation.

### Prospective ordinary group-control comparison

The next candidate adds eight packed control bytes per `u64` beside the
existing owning enum slots. It keeps the same physical bucket count, home
remainder, cyclic probe order, public capacity, ceiling, growth trigger,
refusal result and owned-pair result interface. A private ordinary record
holds one inline tail word and a `Slots<Box<Array<u64>>, 1>` metadata owner:
the window is empty through eight buckets, otherwise it owns an array of
`floor(C/8)` complete words. Vacant and Deleted use distinct bytes with the high bit set; filled
bytes hold the hash's high seven bits. Ordinary integer word operations form
candidate masks. Every payload access still matches the actual slot enum;
metadata grants no extraction or owner-discard authority.

This is a pending representation trial, not a selected policy. The control
is the exact HashMap interface and implementation from
`7abd6bb34746b7b983b83103c68ee429b24859bb`, copied into an ordinary local
module. Both arms redirect only the fixture's map aliases to that module and
use compiler SHA-256
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`.
Before candidate attribution, the carrier must reproduce the original
concrete instances and native work after recorded symbol/namespace
normalization. A material carrier difference stops that comparison. No
compiler, result-transport or capacity-policy change belongs to this factor.

Lookup and edit share the candidate's bounded group search. Equality calls
become the matching-fingerprint subsequence of the old cyclic sequence; hash
calls stay once per query or insertion attempt. The existing consistent-key
protocol, including equal hashes for equal keys, remains necessary for map
semantics. Inconsistent callbacks may change results but must preserve
bounded probes, safe access and every owner. Insert remembers the first
reusable bucket while searching for replacement; reuse updates its control.
Remove writes Deleted after the actual slot exchange. Rebuild keeps the
current descending owner migration, hashes each pending attempt and probes
actual enum occupancy without equality, adding only fresh controls and their
updates. Iteration and cleanup visit actual enum owners. Returned owners are
forwarded completely in every exchange outcome.

On the retained arm64 layout, predicted metadata allocation is zero for
`C <= 8`, otherwise `8 + 8*floor(C/8)` bytes: 4,104 bytes at 4096 buckets and
8,200 at 8192. Large construction gains one allocation; rebuild can hold four
allocations across two owning-slot and two control backings. The ordinary
control record predicts a 24-byte field, enlarging the map descriptor from 16
to 40 bytes. These are predictions to check with fresh accounting, not timed
allocation observations. Public function signatures and existing `cells`
extent postconditions stay; external exhaustive destructuring gains `..`,
and enclosing storage needs renewed layout qualification. Dividing by eight
without rounding up proves the fixed `u64` allocation count fits for every
`u64` input; the inline tail holds a partial final group. No smaller ceiling
or additional caller requirement is proposed.
Unused tail lanes are masked before selection and every array access proves
its actual bound independently. Control reads and writes check their own
runtime Array bound: a missing word reads as empty and an out-of-range update
does nothing. These fallback branches do not assert an extent equality
between the two backings; the native screen includes their retained cost.

Before timing, require the unchanged complete ecosystem correctness, oracle,
fault-control, geometry and allocation screens plus ordinary owning/extent
consumers. Add discriminating mask/probe witnesses for capacities 0, 1, 7, 8,
9, 15, 16 and 17, maximal arithmetic boundaries, full tables, wrap, tombstone-before-match, reuse,
constant hashes, ceiling refusal and inconsistent callbacks. Native code must
skip full-slot tag/key loads for nonmatching control lanes; retained per-slot
probing or new hot helper/descriptor work that erases that mechanism refuses
the form before measurement. The primary causal pair has identical physical
capacity and inputs. Requested-memory pairs follow fresh geometry and keep
their own labels. Then retain the complete practical operation/payload/size
matrix, both cohorts, all samples and existing qualification: a repeatable
useful-cell regression rejects the candidate, short or unstable cells stay
inconclusive, and completion still requires every meaningful cell to separate
strictly below the median-slower ordinary Rust/C++ peer. Dense owning entries
are deferred: retaining the capacity floor still reserves their payloads,
while the separate index and swap-last deletion add dependent loads and owner
movement that this trial does not require.

Initial source admission retained three refusals. Direct operation/conversion
returns needed named local values to publish their result bounds, and the
three-term span postcondition needed the already-computed suffix as an
ordinary helper parameter. More materially, the first rounded-up word array
failed OP-9 in its fixed-layout generic schema: at the full `u64` domain,
`ceil(C/8)` exceeds the maximum `u64` element count by one. The owning-slot
allocation does not publish that different stored type's numerical bound.
The full-word array plus inline tail form above replaces that prototype,
preserving the public domain. Its ecosystem source carrier was accepted by
the frozen compiler. The repaired carrier retains all 131 original raw
definitions and 36 types under an explicit symbol bijection; its 106 original
native bodies have identical instructions, callees, constants and CFI. The
separate cold entry preserves the original fixture main and all four ABI
wrappers. Eighteen absent extern declarations are wholly unreferenced; no
implementation body is omitted. The candidate's inlined hit/miss paths skip
enum loads for nonmatching lanes, while metadata bounds/tail branches remain.
Edit retains an out-of-line find and another enum test, and the scalar trace
frame grows from 672 to 784 bytes. These costs remain falsifiers, not a
predicted timing win. The untimed runtime screen below passes; the completed
paired timing rejects the candidate under the registered no-loss criterion.
An independent arithmetic oracle passed 5,439,488 byte and adjacent
byte comparisons, covering every byte value/code in each lane and the
selected boundary codes for every adjacent-byte pattern; omitting the
original high-bit term and using an inexact borrowing mask each failed. This
checks the packed arithmetic, not native compilation or container behavior.

The candidate-only account image uses an explicit representation mode. Its
independent ledger derives `E(C) = 16 + C*S + M(C)` and
`Q(C) = 1 + (C > 8)` from source capacity transitions, with slot stride
`S = 24` or `272` and metadata extent `M` given above. Fixed-capacity traces
require exactly `Q` requests/releases, `E` total bytes and peak, and zero live
bytes. Growth adds the new capacity's requests/bytes and overlaps both
backings; positive-capacity rehash does so each round. Sequential batches
multiply total counts/bytes, preserving peak. Baseline and native controls
keep their existing assertions. Geometry decodes actual payload capacity
separately from combined backing bytes. Wrong metadata extent and omitted
recorded metadata release must each fail the exact ledger.

An additional untimed owning witness fixes capacity schedules
`8 -> 16 -> 17 -> 17` and `9 -> 9`, with 66 independently numbered child
owners across replacement, full-capacity returned-owner retry, tail
remove/reuse, rehash, edit and logical-ceiling refusal. Before execution, its
expected baseline ledger is 72 allocations, 2,976 requested bytes and 1,408
peak bytes; the candidate requires 77, 3,080 and 1,456, respectively. Both
must release every exact allocation identity and each child owner once,
finish with zero live bytes, and reject wrong extent/double/foreign/missing
release controls. Existing owning and indexed consumers retain their 28 and
129 allocation totals in sequential and parallel lowering. This checks
normal-return ownership; [STOR-8] supplies no source-visible heap-exhaustion
refusal or cleanup path to test.

All four ecosystem images passed 18,390 practical and 1,440 occupancy traces,
with 140 geometry, 420 practical-accounting and 160 occupancy-accounting rows
per arm. All previous fault controls and both metadata faults were detected.
The existing owning/indexed consumers preserved 28/129 allocation identities
in both lowerings. The new witness matched the predicted sequential ledgers
above; parallel lowering preserved exact identities, extents, counts, bytes
and zero-live cleanup. Its observed peaks also were 1,408/1,456 bytes, but its
separate observer mode does not assert a fixed parallel lifetime schedule.
Concurrent observation passed; wrong extent/total/peak and double/foreign/
missing release controls each failed. The 143 frozen construction inputs
remained unchanged. These are instrumented allocation observations.

The registered single timing pair uses work 1,048,576 throughout: practical
cohort 0 runs baseline then candidate, cohort 1 candidate then baseline;
the occupancy sweep then uses that same arm-order reversal. Each arm retains
all 9,240 practical and 3,520 occupancy rows, eleven samples and two checked
warmups per group, for 25,520 recorded rows in total. Ranked comparisons need
every sample at least 1 ms and cohort median-ratio spread at most 10%.
Report every frozen peer's inter-image drift; above 10% leaves source
attribution in that cell inconclusive. A qualified range-separated loss in
any useful cell rejects the general representation candidate; overlap,
adverse median-only results, short samples and instability remain unresolved.
No rerun or sample removal selects a preferred result. The full per-cell
slower-standard-peer target remains unchanged.

Fresh candidate geometry selects 5,120 WF buckets against scalar Rust and
Abseil (128,024 versus 139,272/139,184 requested bytes), and 6,144 against
C++ (153,624 versus 147,456). Wide pairs remain 8,192 against Rust/Abseil and
4,096 against C++. All six satisfy the existing 10% requested-memory limit.
They are selected before timing, separately from the primary comparison at
identical physical capacities; the old representation's pair identities are
not reused.

#### Paired outcome and refusal

The complete pair rejects this group-control representation. Of 116 useful
same-capacity cells, 52 have qualified, range-separated regressions and seven
have qualified gains. This result does not select a capacity-dependent
fallback or a narrower operation interface. The production map and compiler
remain unchanged by this trial; the pending amendment proposes refusal.

| Matrix | Qualified gains | Qualified losses | Qualified overlaps | Inconclusive |
|---|---:|---:|---:|---:|
| Practical, 84 cells | 6 | 44 | 25 | 9 |
| Occupancy, 32 cells | 1 | 8 | 6 | 17 |

All 25,520 samples in 2,320 eleven-sample groups passed the explicit matrix,
work and checksum checks, including both arms' 160 seed-101 occupancy-account
outcomes. The shortest recorded sample is 1.399 ms. Thirteen inconclusive
cells exceed only the peer-drift limit, six only the cohort-ratio limit and
seven both; none are short. Before qualification, the raw ranges show 63
losses, ten gains and 43 overlaps. Both medians are adverse in 83 cells.
Unstable or overlapping results are retained rather than counted as wins or
evidence of no loss. The largest unchanged-peer drift is C++'s scalar
8192-bucket miss: 0.9499/1.8492 between images in cohorts 0/1, so that
candidate comparison remains inconclusive.

Representative same-capacity ratios below are candidate/baseline, retaining
both cohorts. Every listed comparison passes duration, cohort and peer-drift
qualification; the archive contains every cell, including the adverse
unqualified ones.

| Matrix / hash | Value bytes | Capacity / count | Path | Cohort 0 | Cohort 1 | Range result |
|---|---:|---:|---|---:|---:|---|
| Practical / aligned | 256 | 4096 / 3584 | miss | 0.7968 | 0.7891 | gain |
| Practical / native | 256 | 4096 / 3584 | miss | 0.8164 | 0.7879 | gain |
| Practical / aligned | 8 | 64 / 56 | fill-free | 0.9333 | 0.9369 | gain |
| Practical / aligned | 8 | 4096 / 3584 | fill-free | 0.9226 | 0.9115 | gain |
| Practical / aligned | 8 | 3 / 2 | edit-first-word | 2.6488 | 2.6596 | loss |
| Practical / aligned | 256 | 64 / 56 | edit-first-word | 1.9143 | 1.8422 | loss |
| Practical / aligned | 8 | 4096 / 3584 | reserve-more-entries | 1.0846 | 1.0841 | loss |
| Occupancy / aligned | 256 | 8192 / 3584 | hit | 2.0135 | 2.0971 | loss |
| Occupancy / aligned | 256 | 8192 / 3584 | edit-first-word | 2.8443 | 2.8158 | loss |
| Occupancy / aligned | 8 | 8192 / 3584 | replace-old-value | 2.6378 | 2.5557 | loss |

The six practical gains are the two wide-miss cells and scalar fill/free at
64 and 4096 buckets in both hash series; the seventh gain is the separate
4096-bucket wide occupancy miss (0.8247/0.7808). Filtering can help that dense
miss workload, but the same representation loses on tiny maps, ordinary
edits, reserve and lower-occupancy hits. Capacity and the owned-result
interface are fixed within each primary pair: these are effects of the
complete control representation and its emitted code, not isolated timings
of a mask, a helper call or descriptor traffic.

The unchanged practical target reducer reports 23 passes, 44 deficits and
17 inconclusive cells for the fresh baseline, versus seven passes, 62
deficits and 15 inconclusive cells for the candidate. Each target still
selects the median-slower Rust/C++ peer separately in each cohort and demands
strict sample separation. The candidate's fresh comparable-memory pairs
have 20 qualified deficits and four unstable raw deficits among their 24
operation/payload/peer comparisons. For example, scalar miss versus Rust at
5120 WF buckets is 4.8349/4.8273, and wide miss versus Rust at 8192 is
2.8681/2.9000, both qualified. Those resource comparisons do not replace the
same-capacity causal pair. The complete memory and capacity-sensitivity
tables preserve all four occupancy paths, both representations and all
three native peers used for memory matching.

The [evidence archive](ecosystem-group-control-evidence.tar.gz) retains the
eight original raw timing CSVs; all 116 primary, 464 peer-drift, 928 peer,
48 memory and 48 capacity-sensitivity rows; both 84-cell target tables;
fresh geometry/accounting; source, arithmetic and ownership oracles; raw and
native carrier qualification; and exact stage commands, exits, costs and
input/image identities. Its SHA-256 is
`d41ce58752d5c57c5f2c7f6709b7c426a714d02e80dc61fbd85ad8bc20d14541`.
Metadata paths use documented portable placeholders, including
`${USER_HOME}`; raw scratch evidence and every measured source/data/native
byte remain unchanged. The archive index distinguishes raw and published
metadata hashes. The [replay patch](ecosystem-group-control-replay.patch),
SHA-256 `f460376036b71ddc35afd77e2ca7cc5e0d32d90ec22524a3733ae704ad55b931`,
is a portable zero-context rendering. Applying it to the exact pinned source
with `git apply --unidiff-zero --directory=lib/std/collections PATCH` reproduced
all three measured candidate files byte for byte; reversing it restored the
exact two-file baseline. The archive preserves the original measured patch
and its distinct SHA-256
`a57a18093694297bd672c70b5cb8269cb5a2677cdb8e40db8e50ad10a7c4709b`.

The eight timing commands returned zero once, with 707.120160 seconds total
guarded wall time, on observed Git head
`6da90dba09db335c0ea9158fa075f3aecc27e309`. Production/harness inputs still
match `7abd6bb34746b7b983b83103c68ee429b24859bb`; both images use the frozen
compiler identified above. All 206 frozen input/generated-artifact entries
rehash unchanged before and after timing. The timed images are
`bdb973e95144ded2c1e5b6666c1c65543f0779d8fbd14328d348dcfac4b19e80`
(baseline) and
`ff414824f79e2bb2c9a9c79f4102e902c8b03067610b8558cde53c3146421974`
(candidate). The controls reuse the preceding occupancy construction,
including its documented authentic Abseil header/package-metadata recovery;
no existing Abseil binary was replaced. Construction, correctness and
instrumented accounting were completed separately before the one timing
pair. Whole-command wall costs are not operation latencies. No sample was
removed and no rescue rerun occurred.

### Prospective two-span cyclic probing

This ordinary-source factor starts from source
`7abd6bb34746b7b983b83103c68ee429b24859bb` and frozen CLI SHA-256
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`.
It replaces the repeated cyclic-index calculation with two consecutive
bounded scans, `[home, capacity)` followed by `[0, home)`. The question is
whether executed unit-stride scans reduce the baseline's per-bucket address
work without changing its buckets, probe sequence or ownership protocol.
The rejected group controls and the separate consuming-projection factor are
not inputs. Preparation and source/native review precede any execution or
timing; no result is selected by this criterion.

The candidate changes find, lookup, edit, try-put and occupancy migration;
remove inherits find. The obsolete private index helper may disappear, with
that removal recorded in the complete function inventory. Hash/equality
order and counts, first-deleted reuse, first-vacant termination, replacement
before refusal, public capacity, exact allocation extents, growth trigger,
ceiling, descending migration and cleanup stay. Empty capacity still reaches
the source hash call where the baseline does. Range arithmetic must admit
the existing full integer domain without a new capacity restriction or a
`home + capacity` intermediate. Workload routing and native peers stay frozen.

The isolated baseline module carrier must first reproduce every original raw
definition, type, attribute and used callee under an explicit bijective name
mapping, and every original optimized native body, including the fixture main
and four trace/geometry ABI bodies. Entry plumbing and wholly unused extern
declarations are inventoried separately; absent implementation bodies are
never normalized away. The source factor then passes a native gate only if
the executed inner scans lose per-bucket wrap selection without new calls,
tag reloads, payload transfers or query outlining. Increased text, frame
traffic, or changed helper placement remains adverse evidence.

Correctness retains the complete practical/occupancy controls, exact baseline
allocation tuples, geometry, owning/refusal witnesses and their fault controls.
A separate small witness covers zero, tiny, irregular, full and tombstone
tables, environments changed between operations, non-reflexive/asymmetric
equality, first-deleted reuse and full negative probes. Hash/equality have
read-only rows: an untimed instrumented image may observe callback entry order,
but no hidden counter mutation is presented as an ordinary callback effect or
as timed work. Expected traces and allocation lifetimes are calculated
independently of candidate output and receive deliberate wrong-order/count
controls. Later timing, if authorized, retains the complete existing matrix,
identical physical capacities, all peers and all adverse/inconclusive cells;
a selected subset or a repaired group-candidate comparison cannot establish
this source factor's benefit.

**Outcome: rejected by the native gate before program execution.** The
five-scan candidate removes the inner wrap selection, but both emitted
remove specializations acquire a call to find and a second bucket-tag load
after its returned index. The wide practical trace also acquires two calls
to remove. Those are registered falsifiers; no correctness image, allocation
observer or timing run followed, and no runtime speedup or regression is
claimed. The library and live design remain unchanged.

The admitted source forms span-local endpoints for `[home, capacity)` and
`[0, home)` without computing `home + capacity`. It changes five functions,
removes only the private probe helper and retains the other 14 bodies and
public module exactly. For positive capacity, concatenating those ranges
gives the original cyclic sequence, and all indices and final increments
stay within the original unsigned domain. The source retains the original
callback sites, enum matches, returned owners, first-deleted selection and
terminal branches. All ten practical/new/retained ownership source graphs
admit. An independent arithmetic oracle checked 2,145 complete small
sequences and 173 boundary positions through `u64` maximum; omission of the
suffix end, omission of prefix zero and reversal of the prefix each failed.
The new callback-order and allocation observer was prepared but not run;
source admission and arithmetic do not establish its runtime observations.

Every admission attempt is retained. Initial graph/trivia/binder authoring
errors were repaired before the owning graph exposed an `INV-1` backedge
rejection in the loop-carried endpoint spelling. Adding even
`invariant home_low: home >= 0_u64` did not admit that form. The final
span-local endpoint spelling needs no such header facts and admits the
unchanged capacity domain. A separate bounded reduction isolates the frozen
diagnostic to a standalone zero-ceiling Map instantiation: otherwise
identical ceilings 8 and 17 admit, as do the small loop controls. The full
library remains in that reduction, so its rule-level cause is unresolved;
the [source-limit finding](../../../../docs/todo.md) records
the `ENT-2`/`INV-1` classification work without calling it a compiler defect.

The isolated baseline reproduced all 131 original raw definitions and 36
types under the explicit symbol bijection, and all 106 original optimized
native bodies, including the fixture main and four trace/geometry ABI
bodies. Three entry-plumbing bodies, the added cold carrier entry and 18
wholly unused extern declarations have separate inventories; no live body
was omitted. Native construction used Apple Clang 21.0.0
(`clang-2100.3.34.2`, `arm64-apple-darwin25.6.0`) with the frozen CLI above;
all six direct commands exited zero and their inputs remained unchanged.
The observed worktree head was
`a1b4c4aaa8b65dd11471786fdf5fabecce6ff86d`; the library and workload inputs
remain the separately verified `7abd6bb` bytes.

The complete baseline/candidate native inventory has 110/109 bodies: 91
unchanged, 18 changed, one removed private helper and none added. Raw
definitions are 135/134, with the same 36 types. Static instruction counts
include returns and exclude alignment and constants; the complete totals
are 8,480/8,524, or 33,920/34,096 unpadded instruction bytes. These are not
linked-image sizes or operation costs. Representative changed bodies are:

| Native body | Instructions, baseline → candidate | Prologue reservation, bytes | Boundary observation |
|---|---:|---:|---|
| lookup, each of two instances | 61 → 77 | 0 → 0 | still inline in both traces |
| edit, each of two instances | 63 → 79 | 0 → 0 | still inline in both traces |
| find, each of two instances | 65 → 79 | 0 → 0 | newly called by emitted remove |
| scalar remove | 70 → 33 | 0 → 32 | new find call and tag reload |
| wide remove | 105 → 114 | 0 → 48 | new find call and tag reload |
| scalar practical trace | 1,216 → 1,361 | 672 → 672 | removal stays inline; calls unchanged |
| wide practical trace | 2,457 → 2,207 | 8,048 → 7,904 | two new remove call sites |

The span selector unrolls into a suffix using a unit-stride index and
multiply-add addressing, then a prefix advancing a bucket pointer by 24 or
272 bytes. The old wrap compare/select/add disappears; the initial
division/remainder, query router, hash/equality and enum decisions remain.
Successful lookup/edit can recompute the bucket address after the spans
join. Rebuild calls and frames remain unchanged. Wide try-put has two
static exchange sites for mutually exclusive replacement and insertion,
not two exchanges per operation. The smaller wide trace/frame follows its
changed helper placement and cannot be counted as a benefit after the gate
failure.

The [frozen evidence](ecosystem-two-span-evidence.tar.gz) retains the criterion,
all admitted and rejected sources, direct exits/costs, before/after identities,
raw IR and assembly, exact carrier comparisons, full function inventory,
arithmetic/fault results, unexecuted semantic observers and invariant
reductions. The original private reduction omitted operand-free returns and
emitted unused load/store fields fixed at zero. Its original bytes are
retained under `original-reductions/`; the corrected reducer includes returns
and removes those invalid fields. No claim uses the zeros. CFI offsets and
actual prologue reservations are kept distinct. The archive index hashes
every entry and maps historical scratch paths to its relative files.

The [zero-context replay](ecosystem-two-span-replay.patch), SHA-256
`032256ea273f449dbc1a402f33840721223d4533fe54f8c6e76712da7c40c0f7`,
changes the pinned source hash
`35b5ed66fdd683f6c9dc8f93d019b7f1b8069cc4a48d020d6de406d04d98c7e9`
to `dbae3eb36ce9451e315ac2ae42791b8fdb057c4423fc27a21ed3da47b1f59b71`.
Forward and reverse replay both reproduced those bytes. In an isolated
checkout of the source pin, apply it with
`git apply --unidiff-zero --directory=lib/std/collections <patch>`.
The native gate rejects this complete five-scan factor. A future query-only
factor would need its own source and criterion; it would not revise this
outcome or supply missing runtime evidence for it.

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

### Consuming-projection storage: broad refusal and strict local result

The strict local-only candidate passes its separately registered native,
correctness and normal/retained mechanism screens: both wide capacity4096/
count3584 replacement targets qualify under both hash series, with no
qualified paired loss in Map or triggered Slab. This supports the next
current-main implementation discriminator; it does not select general compiler
policy or complete the ordinary Rust/C++ target. All observations below use
the frozen compiler lineage, not current-main execution. No language rule,
public ABI, Map source API, inactive initialization or inline policy changes.

The [evidence archive](consumed-projection-local-evidence.tar.gz), SHA-256
`68c44e36bbbf3a012c4262d80ac7e7bdf2ee4468773bcb21c4f2272c72bd321b`,
retains the original prospective section, broad refusal and unexecuted
three-arm protocol, strict registration, native/correctness inventories, all
24 raw timings and their clocks, statuses, complete cells/peers/transitions,
and source/image pins. `INDEX.json` distinguishes original identities from
hashes of the redacted public bytes. All timing samples remain byte-identical;
empty correctness logs are elided with their empty-byte hashes/statuses retained.
Preparation records describe their original then-unexecuted state. There is
one cross-family archive, rather than a second active research document.

The [source replay](consumed-projection-local-replay.patch), SHA-256
`61d80ae3c515014b998b6038f4fc9ef903908ff6f421a0ad75417fa34f040fdf`,
applies with `git apply --unidiff-zero` to
`6da90dba09db335c0ea9158fa075f3aecc27e309` and reproduces all nine frozen
candidate compiler files. The included focused-test fixture repair preserves
the production storage prefix byte for byte; it does not rebuild or change
the timed CLI. The archive also retains the original broad freeze and
local-only delta for stepwise replay. This historical overlay is not a
current-main implementation. Keep these artifacts while they supply this
comparison's evidence and replay; a successor can retire them with history retained.

#### Broad forwarding refuses the native screen

The original prospective factor allowed some synchronous call consumers to
receive an interior field address. Its broad CLI
`f575555aa641f6cbd2aa70f08f3ff5a1969381c13e9811a4e3958715b044270c`
passed exact-image value/ownership controls but fails the no-extra-transfer
native criterion. Retained wide free changes from tag-first access and a
Filled-only 256-byte register copy to a 272-byte complete-parent transfer
before the tag test, including Vacant/Deleted iterations; its frame grows
320→336 bytes. Retained wide rebuild gains an additional 264-byte capture
before exchange and grows 656→928 bytes, while retaining retry restoration.
The complete paired function differences remain in the archive. This is
additional physical representation traffic, not a demonstrated source-level
uninitialized read: active-variant checks and original initialization remain.

Exposing a parent's interior to a callee keeps otherwise removable complete
parent transfers alive in these optimized bodies. A producer-name blacklist
or predicted inlining does not establish the needed native outcome. The broad
form stays refused, and its prospective A/A plus three-arm timing protocol
was never run. The earlier link to an unavailable broad storage amendment is
removed here; no absent amendment or live-tree ruling is inferred.

#### Strict eligibility and independent correctness

The separate local variant keeps the original complete-parent layout, checked
consume-root/active-variant facts, privacy, exposure, distinct-field and
original-CFG fixed-point liveness checks. It adds one conservative condition:
reject forwarding if any `IrOperation::Call` uses any member of the projected
storage group as an argument, including calls that might later inline.
Further local projections can borrow; an aggregate crossing a call keeps its
independent snapshot. The existing storage planner and allocation-root query
own the change; `FieldReuse`, call-entry capture and source ownership remain
unchanged. No second liveness engine, IR or language exception is introduced.

Baseline CLI SHA-256 is
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`;
strict CLI SHA-256 is
`5f36879094dd1cc2531989af7b80ef30282dda70ea0229dec9200d438d264e33`.
Both selected Pair264 projection transfers disappear from the ordinary wide
Replaced path in normal and retained images. The complete Enum272 parent,
return extent/ABI, tag guards, remaining constructor copy and independent
Record256 call captures remain. Map free/rebuild native bodies reproduce
baseline in both modes, removing the broad failure. Wide trace frames change
8,048→6,128 normal and 6,800→5,168 retained; each put changes 1,136→864.
Slab removes three Handle16 projections without adding bulk or expanded
transfers; Vector and PriorityQueue bodies reproduce baseline. The archive
retains all changed functions, outside-body constants, source-block transfers
and native controls. Smaller frames and removed copies do not isolate a
share of elapsed runtime.

CLI construction costs 62.62 s; native continuation has 61 direct zero
statuses and costs 8.21 s, separate from execution. The corrected focused
harness runs 28 storage and 36 owned-place cases. Exact-image correctness
retains 236 direct commands: 56 constructions, 75 positive executions and
105 intentional failure controls, all matching their expected statuses.
All 18 Map accounting/occupancy/geometry ledgers match baseline bytes; owning
ordinary/observed sequential/parallel runs retain their release identities.
Slab, Vector and PriorityQueue oracles, normal/retained checks and wrong-value/
cleanup controls remain independent observations. Source/test admission,
retained-link ordering and focused-fixture authoring failures and repairs are
retained; the successful runs do not erase them. No timing failed or was retried.

#### Complete registered normal and retained measurements

Each stage preserves twelve fixed timings and twelve preceding pinned clocks:
primary Map B0→C0→C1→B1, independent occupancy in the same order, then
Slab B/run0→C/run0→C/run1→B/run1 with both internal cohorts. All eleven
Map/Slab CSV samples 0–10 are ranked; Map's two warmups are unrecorded.
There is no pooling, outlier removal, sample renumbering or favorable-cohort
selection. Per stage, 25,520 Map plus 4,752 Slab rows have exact schemas/keys
and matched non-time work/checksums. Both continuous outer guards exit 0:
719.82 s normal and 828.70 s retained. All 48 direct clock/timing statuses
and 24 nested guard statuses are 0, all clock quantum observations are
1,000 ns, and normal's 197 / retained's 259 pins remain unchanged. The child
`guard_absent=false` records its live inherited outer owner.

The factor screen uses medians of paired same-sample raw ratios and ratios
normalized by each C peer. It requires raw and every C-normalized direction
beyond `M=max(3%, Q, every native raw/normalized drift across all groups)`
in every group. Q uses the shortest participating WF/native interval and
largest clock quantum: `((t+q)/(t-q))^2 - 1`. All relevant intervals must
be at least 1 ms, ratio spread is `max/min - 1` and at most 10%, and native
raw peer drift above 10% leaves attribution unresolved. Raw-only adverse
directions remain visible. Slab retains both window-C and tagged-C
normalizations and all four outer-run/cohort groups. No peer drift is subtracted.

| Mode / matrix | Cells | Qualified gain | Qualified loss | Inconclusive |
|---|---:|---:|---:|---:|
| normal / primary | 84 | 11 | 0 | 73 |
| normal / occupancy | 32 | 4 | 0 | 28 |
| normal / slab | 18 | 0 | 0 | 18 |
| retained / primary | 84 | 7 | 0 | 77 |
| retained / occupancy | 32 | 4 | 0 | 28 |
| retained / slab | 18 | 0 | 0 | 18 |

| Registered wide primary replacement | Paired raw C/B range | Direct-C-normalized range | M |
|---|---:|---:|---:|
| normal, aligned-hash | 0.860354–0.865987 | 0.846527–0.878813 | 0.038846 |
| normal, native-default | 0.862666–0.866379 | 0.863203–0.868352 | 0.040153 |
| retained, aligned-hash | 0.878633–0.890281 | 0.882953–0.895151 | 0.030000 |
| retained, native-default | 0.889938–0.894037 | 0.890574–0.916079 | 0.030000 |

These are 13.36–13.96% raw normal and 10.60–12.14% raw retained gains.
All four independent wide occupancy replacement capacities 4096/5120/6144/
8192 also qualify in both modes. Six non-target scalar paths (hit, miss,
remove-churn, edit, fill and reserve), plus scalar replacement, retain all
42 primary and 16 occupancy negative-control cells per mode, with no qualified
gain/loss. Wide hit/miss/edit has changed enclosing frames and no qualified
factor gain. Normal Slab has seven wide duration-qualified cells, retained
has nine; none exceeds the raw and both-normalization envelopes in all
groups. All nine scalar Slab cells are sub-1ms in both modes.

#### Ordinary peer targets and adverse transitions

The ordinary target compares each arm independently with its median-slower
Rust/standard-C++ peer. It uses complete sample ranges and maintained
duration/cohort qualifications, separate from the paired factor screen.
The unchanged maintained reducer agrees on every primary target status,
reason, selected peer and minimum interval in both arms/modes. Its `--complete`
matrix lacks occupancy, so the independent occupancy reader applies the same
algorithm after frozen complete-key checks. Slab's control driver has no
Rust/C++ peers; no such target is invented.

Counts below are pass/deficit/inconclusive. Retained helper-boundary results
are diagnostics; they do not replace normal public-use target evidence or
represent source application gains. Every individual peer ratio is retained.

| Mode / matrix / arm | Against Rust | Against C++ | Slower-standard target |
|---|---:|---:|---:|
| normal / primary / B | 12/47/25 | 7/46/31 | 18/34/32 |
| normal / primary / C | 15/51/18 | 10/47/27 | 22/36/26 |
| normal / occupancy / B | 0/19/13 | 0/7/25 | 0/7/25 |
| normal / occupancy / C | 0/17/15 | 2/12/18 | 2/11/19 |
| retained / primary / B | 7/73/4 | 8/71/5 | 14/62/8 |
| retained / primary / C | 6/74/4 | 6/72/6 | 11/65/8 |
| retained / occupancy / B | 0/32/0 | 0/25/7 | 0/25/7 |
| retained / occupancy / C | 0/31/1 | 0/28/4 | 0/27/5 |

Normal loses one pass: scalar fill-free at capacity4096/count3584,
native-default, becomes sample-overlap inconclusive. Its paired raw ratios
are 1.009372/0.980732, normalized 1.006899/1.013698, with M=0.106121.
The native-identical control retains an adverse cohort-0 target upper bound
2.544294. Normal also records eight primary and six occupancy new-deficit
flags. None is automatically a qualified paired-factor regression.

Retained loses four primary passes, all native-identical scalar capacity3/
count2 cells. Each is pass→inconclusive; no pass→deficit occurs in either mode.

| Retained lost pass | Hash | Reason | Paired raw cohorts 0 / 1 | C-normalized cohorts 0 / 1 | M |
|---|---|---|---:|---:|---:|
| fill-free | aligned-hash | sample-overlap-or-tie | 1.001388 / 0.992509 | 1.008142 / 1.005252 | 0.046626 |
| miss | native-default | sample-overlap-or-tie | 1.001979 / 0.992772 | 0.996459 / 1.003727 | 0.085172 |
| replace-old-value | native-default | sample-overlap-or-tie | 1.011058 / 1.021112 | 0.997841 / 0.982238 | 0.068799 |
| reserve-more-entries | native-default | selected-peer-unstable;target-cohort-unstable | 1.070489 / 0.987430 | 1.073530 / 0.991505 | 0.047885 |

The retained reserve control has one +7.05% raw cohort and one −1.26%
cohort; its ordinary peer/cohort qualification fails. The retained fill
control preserves its cohort-0 target upper bound 2.096635. Retained also has
eight new deficit flags (four primary/four occupancy), including the small
wide native-default replacement cell, which improves under the paired factor
screen while becoming a separately qualified ordinary-target deficit. These
classifications depend on separate distributions and peers; a deficit flag
is not the definition of a paired-factor loss. All exact keys, reasons, raw/
normalized ratios and peer bounds remain in both complete 116-row transition
tables. Unresolved peer drift remains a limitation, not a rescue.

Per mode the archive supplies 15,136 sample pairs, 304 cohort groups, all
134 cell decisions, 232 ordinary targets and 464 individual Rust/C++
comparisons. The retained reader reuses the normal arithmetic unchanged;
its identity-only diff is recorded. A preliminary retained postflight
assertion wrongly expected empty wrapper stderr and failed on `/usr/bin/time`'s
real/user/sys lines; its corrected check requires exactly those lines,
while all clock/timing stderr remains empty. That reader failure is preserved
and caused no native replay or criterion change.

CSV replay and source/native prerequisites are in the archive's `REPLAY.md`.
The native mechanism and both-mode measurement criterion are met for this
frozen local-only form, with no A/A or isolation of arbitrary temporal,
arm/cohort, layout, allocator or ASLR effects. The broad A/A proposal remains
unexecuted and is not claimed satisfied. Current-main waiting/context
boundaries, grammar/layout adaptation, complete correctness and native
qualification are the next discriminator. No size threshold, timing retry or
final general-policy selection follows this evidence alone.

## Current-library lookup: matched query-only discriminator

The baseline source is `63c6cd6ac82c7c1f9f94b688ccc4cccef87e51cc`, including
main's live-plus-vacated growth policy and the inline Slots representation.
Its frozen compiler SHA-256 is
`a7da03da398bdedc8adc2abccd1cf667ffbac1016e06f8431617fd5f50e8d798`.
The older fixed-eight `find` inlining observation concerns a different source;
it is not evidence that the current runtime-map lookup retains a helper call.

First isolate successful and unsuccessful borrowed lookups, reading one word
from either an 8-byte or 256-byte value. Prepare the maps and verify their
complete contents outside the query clock; consume every query outcome in an
independent digest. Use the same salted mix, odd stored keys, even missing
keys and query order. The first geometry is 64/4096 bucket counts with
32/2048 live entries, below Whitefoot's rebuild threshold. Verify actual
post-fill counts: WF physical slots and C++ chaining buckets equal the target;
Rust usable capacity is 56/3584, with any inferred physical-slot count labelled
as such. Chaining buckets are not flat slots, and representation-dependent
allocated bytes must be reported rather than called equal. Default hashers
and higher-load/ceiling cases remain separate follow-ups.

The first candidate keeps a running wrapped bucket index in public
`hash_map_lookup`, preserving the hash call on an empty map, the exact bounded
probe order, tombstone handling and callback order. Its hypothesis is less
per-probe address reconstruction, not fewer hash computations: retained native
code already hoists the division. Before timing, require actual reached native
code to remove the predicted work without new calls or spills, and require
empty, hit, miss, wrap, tombstone and ownership observations to pass. If it does
not, retain the result and do not use timing noise to select the candidate.

Compare ordinary source compiled by frozen baseline/candidate compilers with
identical harness and peer inputs. Calibrate a common work count before the
comparison so real intervals exceed 1 ms; keep all samples and separate
unsubtracted clock controls. Use two rotated/reversed cohorts and nine samples,
report both peers, complete ranges and cohort ratios. The existing full-range
and at-most-10% cohort-ratio criterion remains the sufficient performance test.
A candidate gain also needs separated before/after ranges in both cohorts and
no unexplained peer drift above 10%. This first discriminator qualifies no
unmeasured mutation API or whole-map workload.

### Running-index native prerequisite

Both fresh scalar and wide query instances pass the native prerequisite.
The first probe drops the `cmp/csel/add` reconstruction before its addressing
`madd`; a continued probe reduces its index/control sequence from seven
instructions to six. Each complete query body shrinks from 244 to 235
instructions and its stack frame from 32 to 16 bytes. Division remains once
per query, bucket strides remain 24/272 bytes, and no hot call or loop spill
is introduced. These counts establish the mechanism, not its timing benefit.
The baseline and candidate object SHA-256 values are respectively
`ffdd2c1c9a4ebf29d216347c6925fd5ad886ad7989722380f2037ec12ae05675`
and `5922a39a8415e19767e5469e5b68ae0428203f0c8ca6cec75c491b46720294a0`.

Common objects link before the varying WF object. The four timed Rust/C++
query bodies have identical addresses and bytes in both linked images,
retain actual lookups and borrowed first-word loads, and have no calls or
constant-pool references. Untimed setup/cleanup relocations do differ as
other constants and stubs move; whole-image identity is not claimed.

The next lookup discriminator is the already-recorded bucket-indexing TODO:
for a positive power-of-two bucket count, use `iand(hash, count - 1)`;
otherwise retain `% count`. This preserves every bucket index, including
arbitrary non-power-of-two capacities. Require accepted ordinary source,
non-power-of-two/wrap/empty correctness, removal of division on the reached
power-of-two native path, and the same paired timing criterion. Compare both
against the unchanged baseline and against the running-index-only arm; do not
attribute their combined gain solely to either component. No table layout,
capacity, hash, growth policy or compiler proof rule changes in this trial.

### Paired lookup results and remaining gap

The [samples](lookup-api-samples.csv) retain the calibration and both paired
campaigns; [evidence](lookup-api-evidence.json) retains exact identities,
commands, accounting, clock observations and the unselected mask patch.
The running-index source is
`c5ba1a5bca2c449df0e5d4457709447fc780dc26`. Each formal arm has 432
observations: eight operation/width/capacity cells, three implementations,
nine samples and two cohorts. Each sample performs 2,097,152 lookups.
The first campaign's minimum intervals are 3.021/3.024 ms for baseline/index;
the second's are 3.013/3.026 ms for index/mask. The 262,144-work pilot is
calibration only. Clock controls are separate and unsubtracted: median of
nine batch means is 20.447 ns per clock pair; individual pairs range from
0 to 35,000 ns.

First campaign, combined-cohort medians in nanoseconds per lookup:

| Value bytes | Buckets | Query | Baseline WF | Running-index WF | Rust | C++ |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 8 | 64 | hit | 2.699 | 2.074 | 2.134 | 1.593 |
| 8 | 64 | miss | 3.433 | 2.850 | 1.586 | 1.569 |
| 8 | 4096 | hit | 2.883 | 2.292 | 2.211 | 1.710 |
| 8 | 4096 | miss | 3.639 | 3.173 | 2.148 | 1.453 |
| 256 | 64 | hit | 2.691 | 2.092 | 2.300 | 1.596 |
| 256 | 64 | miss | 3.375 | 2.833 | 1.575 | 1.556 |
| 256 | 4096 | hit | 3.096 | 2.468 | 2.465 | 1.988 |
| 256 | 4096 | miss | 3.837 | 3.402 | 2.167 | 1.764 |

All eight WF medians improve, by approximately 11–23%, but this is not
complete-range qualification. Only the 256-byte/64-bucket hit meets the
registered peer target in both cohorts. The other hits overlap or trail;
all misses still trail the slower peer. Retain the running-index source and
continue the lookup investigation; no mutation API or complete-map target is
qualified by this panel.

The mask trial removes division on the selected power-of-two path, while
retaining the generic modulo fallback. Each query body grows from 235 to 287
instructions and its frame from 16 to 32 bytes, with one additional entry
save/exit restore pair, but no new hot calls or inner-loop spills. Its native
object SHA-256 is
`63487bbad4747574b34885ab8b3b1e96066ff1164244fab667acbb809af41de5`.
The shared timed peer bodies remain identical in bytes and linked addresses.

Second campaign, combined-cohort WF medians in nanoseconds per lookup:

| Value bytes | Buckets | Query | Running index | With mask |
| ---: | ---: | --- | ---: | ---: |
| 8 | 64 | hit | 2.117 | 1.832 |
| 8 | 64 | miss | 2.965 | 2.649 |
| 8 | 4096 | hit | 2.285 | 2.321 |
| 8 | 4096 | miss | 3.198 | 3.268 |
| 256 | 64 | hit | 2.034 | 1.880 |
| 256 | 64 | miss | 2.890 | 2.547 |
| 256 | 4096 | hit | 2.492 | 2.491 |
| 256 | 4096 | miss | 3.383 | 3.213 |

Masking qualifies both small-table hits against the peers, but neither large
hit nor any miss. Its scalar large-table miss changes from 3.909 ns in cohort
0 to 2.930 ns in cohort 1; this exceeds the stability criterion. Large wide
misses also retain substantial outliers. Therefore the mask fast path is not
selected: fewer division instructions alone do not settle its overall cost.
Keep its ordinary-source acceptance and non-power-of-two regression as useful
evidence, with the exact alternative retained for reproduction.

Filled requested live bytes (allocator overhead excluded) are identical
across WF variants:

| Value bytes | Buckets | WF | Rust | C++ |
| ---: | ---: | ---: | ---: | ---: |
| 8 | 64 | 1536 | 1096 | 1536 |
| 8 | 4096 | 98304 | 69640 | 98304 |
| 256 | 64 | 17408 | 16968 | 9472 |
| 256 | 4096 | 1114112 | 1085448 | 606208 |

WF and Rust request one allocation; C++ requests 33/2049 for its bucket array
and nodes. C++ stores only the live pairs, while the flat representations
reserve value slots for empty buckets too. Every query allocates nothing and
every final ledger is empty. Actual WF slots/C++ buckets are 64/4096; Rust
reports 56/3584 usable entries, and its physical count remains an inference,
not an observed public capacity. These are controlled-hash, half-load results,
not a default-hasher or high-load qualification.

Reproduce the current source with the explicit `ecosystem-lookup-check`,
`ecosystem-lookup-account` and `ecosystem-lookup-measure` Makefile targets;
`ECO_LOOKUP_WORK` defaults to the registered 2,097,152. The measured compiler
and native tools are pinned in the evidence file. The remaining discriminator
is the number and kind of probes on a miss, followed by a matched C linear
probe control before attributing that gap to representation or lowering.

### Matched linear-probe discriminator

Next add the existing direct-C sparse storage to the isolated query driver,
as attribution only, never as the Rust/C++ target denominator. Its 24/272-byte
enum cells, bounded running-index probe, one initial remainder, exact keys,
hash and digest match the retained WF lookup. Its old backing header adds
16 bytes per allocation; report that difference and inspect whether its load
is hoisted out of the query loop. Prepare and cleanup remain outside timing.
Require the same geometry/content/cleanup observations, actual native probes,
and two cohorts of nine samples at 2,097,152 queries before interpretation.
Inspect tag dispatch, hashing, calls and surviving transfers in both native
query bodies. A repeated C advantage identifies a concrete implementation
cost to investigate; similar timings with both behind Rust/C++ motivate a
probing/layout experiment. Neither result proves that the C implementation
is an absolute lower bound on every possible linear-probe implementation.


The retained C attribution campaign uses two cohorts with the same frozen
running-index WF compiler, four peers, nine seeds and 2,097,152 queries.
All 576 observations are in `lookup-api-samples.csv`; source, binaries,
commands, direct exit statuses and allocation ledgers are pinned in
`lookup-api-evidence.json`. Both campaigns returned zero; the shortest
sample is 3.027 ms. C adds the disclosed 16-byte backing header; native
inspection finds its header load outside the query loop and no hot calls.
The C query has 191 instructions and a 16-byte frame, against WF's 235 and
16. Both use an initial remainder and the same bounded linear probes.

| Value bytes | Buckets | Query | WF ns/query | C linear ns/query | Rust ns/query | C++ ns/query |
| ---: | ---: | :--- | ---: | ---: | ---: | ---: |
| 8 | 64 | hit | 2.126 | 1.819 | 2.171 | 1.606 |
| 8 | 64 | miss | 2.861 | 2.294 | 1.593 | 1.569 |
| 8 | 4096 | hit | 2.294 | 2.787 | 2.230 | 1.643 |
| 8 | 4096 | miss | 3.197 | 2.649 | 2.154 | 1.458 |
| 256 | 64 | hit | 2.104 | 1.792 | 2.310 | 1.604 |
| 256 | 64 | miss | 2.876 | 2.337 | 1.610 | 1.569 |
| 256 | 4096 | hit | 2.490 | 2.721 | 2.495 | 2.249 |
| 256 | 4096 | miss | 3.891 | 2.961 | 2.199 | 1.802 |

These are medians over both cohorts, not sufficient target qualifications.
The cohort-wise small-table WF miss penalty over C is 22–27%; C itself
still trails Rust and C++ on misses. Large-table hits make C slower than WF,
so this control is not an absolute performance floor. Its useful distinction
is a same-algorithm miss cost that remains after isolating lookup. WF has
three tag decisions on the vacant path, C two; the extra WF edge aborts for
an invalid enum tag. That is a concrete code-generation hypothesis, not a
claim that this edge explains the entire time difference.

### Initialized enum-domain discriminator

Before measuring the candidate, require source exhaustive matching (ERR-2)
and ordinary typed values at both WF and linked boundaries (SCOPE-3) to map
to the exhaustive IR targets retained by lowering. A complete initialized enum match may then expose
its impossible default as LLVM `unreachable`. Do not infer validity of
inactive payload bytes, unfinished constructors, cleanup inputs or arbitrary
native bytes. Consume the existing checked exhaustiveness rather than repeat
semantic coverage checking in the emitter. Execute all three variants
through borrowed and owned matches, including
linked constructors with dirty inactive owner bytes. Keep cleanup guards.

Compare the frozen running-index compiler with this one change, the same
WF source and four-peer harness. Inspect actual native queries before
measurement, then use two reversed cohorts and the existing sufficient
qualification and stability criteria. Select only a repeatable improvement
without a new unexplained regression; do not select merely because a branch
or instruction disappears. If misses still trail both peers, investigate
ordinary-source grouped control-byte probing rather than attributing that
remaining cost to the foundational storage types without a witness.


The initialized-enum candidate passes 4 resource-enum and 7 payload-enum
focused tests. The final gate test build takes 73.54 s, their guarded
executions 3.85 s and 3.20 s, and the final optimized CLI build 44.17 s.
The fresh paired query fixture differs in raw LLVM only by removing 218
`call void @abort()` instructions from impossible match defaults; no
algorithm or payload access changes. Each harness arm passes 96 timed and
96 accounting cases plus the seven deliberately faulty observations, and
all 32 accounting rows finish with zero live bytes.

Native inspection finds one fewer conditional branch on the vacant probe
path (three to two) in both widths, unchanged modulo, stride, wrap, hash,
key/first-word reads and digest. Nonempty query frames remain 16 bytes with
no hot calls or spills. Static instruction counts increase from 235 to 237
because LLVM duplicates the final digest/return after moving the prologue
behind empty guards; this does not substitute for timing the executed path.
All six Rust/C++/C query bodies retain identical bytes and linked addresses
between the fresh pair. The fixture is therefore ready for the registered
measurement; no timing conclusion is implied by this native inspection.


The two-cohort pair completed with direct exit zero, 576 rows per arm and
minimum sample intervals of 3.135 ms (index control) and 3.018 ms (enum
domain). Carry this as a provisional recommendation under Q202, not a
claim that the registered no-regression condition passed. In
both widths at 64 buckets, every same-seed miss sample is faster in each
cohort (36 of 36 pairs across the two widths). The medians of those paired
candidate/control ratios are 0.915/0.945 for scalar values and 0.947/0.946
for wide values. Corresponding native-peer paired ratios remain near one.
The same-input ratios distinguish the branch effect from different seeds'
probe lengths; they supplement, and do not replace, the registered target
qualification.

| Value bytes | Buckets | Query | Index WF ns/query | Enum-domain WF ns/query |
| ---: | ---: | :--- | ---: | ---: |
| 8 | 64 | hit | 2.110 | 2.091 |
| 8 | 64 | miss | 3.016 | 2.708 |
| 8 | 4096 | hit | 2.293 | 2.331 |
| 8 | 4096 | miss | 3.248 | 3.023 |
| 256 | 64 | hit | 2.102 | 2.070 |
| 256 | 64 | miss | 2.832 | 2.744 |
| 256 | 4096 | hit | 2.486 | 2.494 |
| 256 | 4096 | miss | 3.787 | 3.168 |

The table combines both cohorts. Hits overlap; scalar 4096-bucket hits have
a same-seed median ratio of 1.018 in both cohorts. Wide 64-bucket hits are
slower in all nine same-seed pairs in cohort 1 (median ratio 1.0059), while
seven of nine improve in cohort 0. Overlapping ranges establish neither a
qualified regression nor the required absence of one; these recurrent
adverse observations remain unexplained. The registered selection condition
is therefore unresolved. Q202 recommends retaining the valid tag-domain
fact provisionally for its bounded small-miss gain while accepting that
unresolved hit tradeoff; the owner has not approved that tradeoff, and the
proposal does not relax the all-API performance target. Large-table misses
have substantial spread: the wide
cohort medians change from 4.039 to 3.109 ns and from 3.398 to 3.262 ns, so
the combined large gain is not qualified causal attribution. Only the
64-bucket wide hit meets the registered complete-range slower-peer target;
lookup as a whole remains unqualified and every miss cell still loses.
This selects neither a new enum layout nor a primitive container change.

### Single-backing grouped lookup screen

The next ordinary-source screen groups one `u64` control word with eight
ordinary `HashMapSlot<K, V>` elements in an array, all in one backing. This
differs from the earlier rejected sidecar experiment: probing starts at a
group boundary, without rotating a partial control word. Fingerprints only
select candidates; every payload access still matches the actual enum.
The pre-timing criterion requires the existing independent query and cleanup
oracles, exact allocation accounting and native evidence of control-word
filtering. Only repeatable improvement without separated query regressions
would justify implementing the complete mutation protocol. Arbitrary
capacity tails, mutation and refusal handling are outside this narrow screen.

Both arms use the same frozen enum-domain compiler and the existing
four-peer, half-load, controlled-hash protocol. The candidate passes 96 timed
and 96 accounting cases and all seven fault controls; 32 accounting rows
end with zero live bytes. There is one allocation and no query allocation.
At 64/4096 payload slots, scalar backing bytes rise from 1536/98304 to
1600/102400; wide backing bytes rise from 17408/1114112 to 17472/1118208.
The eight-slot group strides are 200 and 2184 bytes.

Two reversed cohorts retain 1152 observations, with a shortest sample of
3.015 ms. The exact source patch, driver transformation, compiler and image
identities, criterion and accounting are in `lookup-api-evidence.json`;
`lookup-api-samples.csv` retains every row. An initial control run used the
older index compiler by mistake; its 288 rows are preserved separately as
`wrong-cli-calibration` and excluded from these comparisons.

| Value bytes | Buckets | Query | Flat enum-domain ns/query | Grouped ns/query |
| ---: | ---: | :--- | ---: | ---: |
| 8 | 64 | hit | 2.062 | 3.809 |
| 8 | 64 | miss | 2.664 | 3.064 |
| 8 | 4096 | hit | 2.288 | 3.942 |
| 8 | 4096 | miss | 2.982 | 3.283 |
| 256 | 64 | hit | 2.061 | 3.697 |
| 256 | 64 | miss | 2.651 | 2.949 |
| 256 | 4096 | hit | 2.440 | 4.232 |
| 256 | 4096 | miss | 3.159 | 3.234 |

These are combined-cohort medians. Every hit shape has separated adverse
ranges in both cohorts: candidate minimum exceeds control maximum by at
least 1.60, and cohort median ratios range from 1.719 to 1.887. Miss medians
are also worse, with overlapping ranges. Reject this source shape; do not
proceed to its full mutation implementation on this evidence.

The grouped query has fewer static instructions (189 versus 237) but a
longer first-candidate dependency chain. After hashing, the first-group,
first-candidate hit executes 28 instructions through its first value load,
versus 14 for flat WF and 20 for Rust on the inspected paths. This is a
path observation, not an average probe count or a timing attribution.
Grouping adds fingerprint-mask production, lane selection and the resulting
slot address before the actual enum and key checks. Its frame is 80 bytes,
with no hot calls or loop spills. The six native peer query bodies retain
identical bytes and addresses across the two images.

The next bounded discriminator changes only mask production, preserving
group layout, probing and enum checks. An equivalent LLVM vector mask is a
measurement instrument, not an implemented WF capability: first check its
byte-wise equivalence, query oracle and ledger, then measure it against the
original grouped body with the same harness. A loss even with that mask
rules out that replacement as a sufficient repair of this shape. Ordinary-source
byte-array forms are screened separately to distinguish an unexpressible
operation from a missed code-generation opportunity; neither instruction
counts nor SIMD instructions alone select a representation.

The mask-only pair changes one LLVM function. Both versions XOR the control
word with the same repeated fingerprint; the replacement compares each of
the resulting eight bytes with zero and packs 0x80 for equal bytes. In the
original expression, `(byte & 127) + 127` never carries to another byte and
its high bit, combined with the original byte's high bit, detects exactly
nonzero bytes. Thus the replacement is equivalent for every word and code,
without assuming a fingerprint range. An additional 117,664-case oracle
checks byte boundaries, cross-byte patterns and seeded arbitrary inputs.

The floor passes the 96-case query oracle, a separately patched accounting
image's 96 cases, seven fault controls and the same 32-row release ledger.
Two cohorts retain 1152 timing rows; the shortest interval is 3.010 ms.
All eight candidate/control ranges separate in the favorable direction in
both cohorts. Combined-cohort medians are:

| Value bytes | Buckets | Query | Original group ns/query | Vector mask floor ns/query |
| ---: | ---: | :--- | ---: | ---: |
| 8 | 64 | hit | 3.641 | 3.212 |
| 8 | 64 | miss | 3.063 | 2.265 |
| 8 | 4096 | hit | 3.937 | 3.510 |
| 8 | 4096 | miss | 3.277 | 2.640 |
| 256 | 64 | hit | 3.673 | 3.187 |
| 256 | 64 | miss | 2.992 | 2.265 |
| 256 | 4096 | hit | 4.222 | 3.729 |
| 256 | 4096 | miss | 3.226 | 2.735 |

Mask lowering therefore accounts for a meaningful part of this grouped
query's cost: cohort median ratios are 0.866–0.892 on hits and 0.739–0.863
on misses. It does not repair the large hit disadvantage against flat WF.
The flat/group and mask-only pairs are separate campaigns; cross-table
numbers are not a freshly paired flat-versus-floor attribution. The grouped
representation remains unselected, and these floor timings are not ordinary
WF performance.

Three ordinary-source forms instead store controls in `Array<u8, 8>`,
compare its bytes, and assemble eight 0x80/0 results: explicit lanes, a
fixed eight-iteration loop, and a separate ordinary byte-packing helper.
All compile and produce `LDR D` plus `CMEQ.8B`. Their mask packing widens
lanes and reduces them with AND/OR operations, leaving a 201-instruction,
48-byte-frame query rather than the original group's 189 instructions and
80-byte frame. They have no hot calls but were not executed or timed.
The LLVM vector instrument has 184 instructions and a 64-byte frame. It
still loads and XORs the word in scalar registers before transferring it
into and out of SIMD registers, so it is not an ideal SIMD lower bound. This
exposes a code-generation opportunity, not a missing byte-comparison
operation or evidence that primitive container types must change.

### Query-caller normalization discriminator

Current flat WF reconstructs each input key with a shift, add, hit/miss
comparison and selection inside the query loop. The matched C body folds
the invariant offset outside the loop and uses one shifted add. Test only
the ordinary WF caller spelling: compute `first_key` as 1 for hits or 2 for
misses before both loops, then form `index *wrap 2 +wrap first_key`.
Modular associativity preserves every query key for all `u64` indices;
lookup, callbacks, hash, digest, preparation and cleanup stay unchanged.
This is caller-overhead normalization, not a HashMap implementation gain.

Before timing, require identical independent query/cleanup results and
allocation ledgers, one key-construction instruction on the native query
path, no added hot calls or spills, and identical native peer bodies. Pair
with the unchanged flat enum-domain control under the existing two-cohort
protocol. Retain adverse observations and apply the same full-range and
stability criteria; do not attribute its gain to storage or probing.

The normalized source passes both 96-case query checks, seven fault
controls and a 32-row accounting record identical to the flat control's.
Both native queries construct the key with one shifted add. Their frames
remain 16 bytes with no hot calls or loop spills; the complete bodies shrink
from 237 to 196 instructions, including elimination of a duplicated query
path. The six native peer query bodies retain identical bytes and addresses.
WF's internal code layout also changes, so timing attributes the complete
source rewrite, not just three instructions in isolation.

The two-cohort comparison has 1152 observations and a 3.015 ms minimum
interval. Combined-cohort medians, in nanoseconds per lookup:

| Value bytes | Buckets | Query | Previous WF caller | Normalized WF caller | Rust | C++ |
| ---: | ---: | :--- | ---: | ---: | ---: | ---: |
| 8 | 64 | hit | 2.118 | 1.973 | 2.176 | 1.619 |
| 8 | 64 | miss | 2.691 | 2.698 | 1.654 | 1.577 |
| 8 | 4096 | hit | 2.338 | 2.181 | 2.231 | 1.668 |
| 8 | 4096 | miss | 3.058 | 2.745 | 2.141 | 1.508 |
| 256 | 64 | hit | 2.115 | 1.958 | 2.315 | 1.615 |
| 256 | 64 | miss | 2.727 | 2.469 | 1.592 | 1.567 |
| 256 | 4096 | hit | 2.471 | 2.350 | 2.470 | 2.283 |
| 256 | 4096 | miss | 3.235 | 3.023 | 2.173 | 1.783 |

Retain the normalized caller to expose the same invariant key offset that
the native controls already move outside the query loop. This keeps the
input stream and complete observations unchanged; it is not a change to
the container or its target. All four hit medians improve in both cohorts,
but scalar 64-bucket misses are 1.040 times the previous caller in cohort 1,
with overlapping ranges. Do not claim a uniform no-regression result.
The complete-range slower-peer target qualifies three of eight shapes in
both cohorts: scalar 64-bucket hits and wide hits at both capacities.
Scalar 4096-bucket hits qualify only in cohort 1; no miss qualifies.
All four hit medians beat Rust, while misses remain 28–63% slower than Rust.
This is still controlled-hash, half-load lookup evidence, not whole-HashMap
qualification. The exact old fixture revision and both source hashes are
retained so earlier experiments remain reproducible after this normalization.

A following ordinary-source vacant-first match was compiled against an
identical copied-lookup control. LLVM reconstructed the same tag dispatch:
both widths' native query disassemblies were identical to the normalized
caller, including their addresses. The experiment stopped without timing
identical code. No variant tag order or branch-likelihood hint changed.

### Index-linked chaining discriminator

Test one ordinary-source chaining representation because the matched C++
node table remains faster on misses than the flat linear probes. Use
64/4096 head buckets and reserve 64/4096 entry positions, of which 32/2048
are initialized, with the same keys, hash, seeds, payloads and normalized
caller. Heads contain indices into a
separate backing of `{key, value, next}` records. Every stored index is
range-checked before access, and traversal is bounded by the initialized
entry count. This requires two allocations. Reserve the same number of
payload positions as the flat table to avoid crediting a reduced reserve
to the chaining algorithm; disclose both capacities and all requested
bytes rather than calling the representations identical. C++ still
allocates nodes as it inserts them. The screen tests an
algorithm and representation together, not one lowering instruction.

Before timing require the independent query/full-cleanup oracles, exact
geometry, zero query allocations, final-zero release ledger, and native
inspection of the stored-index checks and dependent loads. Pair against
the normalized flat caller with unchanged native peer bodies and the
existing two-cohort protocol. A repeatable benefit without separated query
losses justifies studying the complete ownership and mutation protocol;
it does not select a library replacement. Duplicate replacement, deletion,
slot reuse, growth/rehash and their refusal behavior are outside this
query-only screen. A failure ends this candidate without a language change.

#### Equal-reserve chain outcome

The two reversed-order cohorts retain 1,152 observations in
[the samples](lookup-api-samples.csv); the `equal_reserve_index_chain` record
in [the evidence](lookup-api-evidence.json) contains the exact ordinary source,
driver delta, compiler/object/image identities, allocation ledger and commands.
The initial half-reserve constructor was not timed. All reported chain runs
reserve as many entry positions as head buckets.

Median nanoseconds per query, with the two cohorts shown separately:

| Payload | Buckets / entry reserve | Query | Flat cohort 0 / 1 | Chain cohort 0 / 1 |
|---|---:|---|---:|---:|
| 8 bytes | 64 | hit | 1.914 / 1.934 | 1.844 / 1.839 |
| 8 bytes | 64 | miss | 2.430 / 2.422 | 1.637 / 1.658 |
| 8 bytes | 4096 | hit | 2.147 / 2.133 | 1.951 / 1.976 |
| 8 bytes | 4096 | miss | 2.743 / 2.754 | 1.545 / 1.556 |
| 256 bytes | 64 | hit | 1.895 / 1.940 | 1.832 / 1.857 |
| 256 bytes | 64 | miss | 2.506 / 2.469 | 1.630 / 1.656 |
| 256 bytes | 4096 | hit | 2.326 / 2.323 | 2.270 / 2.233 |
| 256 bytes | 4096 | miss | 3.582 / 3.040 | 1.600 / 1.625 |

All four hit shapes and both 4096-bucket misses separate from the slower
native peer's full sample range in both cohorts. The 64-bucket misses do not:
scalar medians are 1.637/1.658 ns against Rust's 1.624/1.616, and record
medians are 1.630/1.656 against 1.580/1.604. Keep the 4.714 ns record-small-miss
sample. The flat record-large-miss control also drifts between cohorts;
its apparent 47--55 percent reduction must not be treated as a stable
single-effect estimate. This is six range-qualified query shapes under this
hash and half-load setup, not a whole-map performance qualification.

The allocation observer records two requests and two releases, no allocation
while querying and zero final live bytes. Equal reserve costs 2,048/131,072
bytes for scalar payloads versus flat 1,536/98,304 (+33.3 percent), and
17,920/1,146,880 for records versus flat 17,408/1,114,112 (+2.94 percent).
The dense entry prefix alone is initialized; its remaining reserve is not
populated. C++'s node allocations and Rust's control-byte layout remain
different and are reported by the same observer.

The timed and accounting images each pass 96 independent query/full-cleanup
cases; eight deliberately faulty controls reject, including wrong chain
allocation bytes. Native review finds 191 instructions and a 16-byte frame
in each query, no hot calls or loop spills, a checked stored index and a
bounded traversal. Every visited node still reloads the entry backing pointer.
All six native peer query bodies and addresses match the flat paired image.

Proceed to an ordinary-source full-operation prototype, without changing the
library representation yet. Dense swap-removal must repair the incoming link
to the moved last element; a full-table repair scan would make deletion linear
in capacity. The current library's filled-plus-vacated pressure policy also
needs an explicit comparison because this representation has no tombstones.
Replacement, removal/reuse, owning cleanup, ceiling refusal, growth and rebuild
costs remain unmeasured. No allocator-refusal outcome is introduced: allocation
exhaustion follows the current language's terminal resource-failure model.

### Mask retry after caller normalization

Revisit exactly the earlier power-of-two mask with arbitrary-capacity modulo
fallback because normalization removes a duplicated query path. The original
mask screen grew the body/frame to 287 instructions/32 bytes; on the
normalized caller the same source change produces 242/16, against 196/16
for its copied-lookup control. The native power-of-two path bypasses
division and the fallback remains, with no hot calls or loop spills.
The extra per-query selection and larger body still cost work. This changed
native shape warrants a new comparison, not a reversal of the earlier
unselected result by assertion.

Require the same query/cleanup/geometry/accounting checks and peer identity,
then pair against the normalized flat caller under the existing two-cohort
protocol. Retain every adverse cell and arbitrary-capacity coverage. Only
a repeatable improvement without a new separated query loss would justify
carrying the source fast path into the production library for full testing.
No modulus algorithm, capacity policy or primitive type changes in this
screen; do not combine it with chaining when attributing its effect.

#### Normalized-mask retry outcome

The retry retains all 1,152 paired samples and exact source/driver/image pins
under `normalized_mask_retry` in the same evidence. Each candidate and control
passes 98 timed and 98 accounting cases, including capacity-three hit/miss
fallback; the seven candidate fault controls reject and all 32 accounting
rows are byte-identical to the flat first-key control. A common driver adds
those untimed cases to both arms. Peer addresses differ from older images,
so the native identity claim applies only within this new pair.

| Payload | Buckets | Query | Candidate/control median, cohort 0 / 1 |
|---|---:|---|---:|
| 8 bytes | 64 | hit | 0.846 / 0.884 |
| 8 bytes | 64 | miss | 0.914 / 0.750 |
| 8 bytes | 4096 | hit | 0.985 / 0.933 |
| 8 bytes | 4096 | miss | 0.925 / 0.909 |
| 256 bytes | 64 | hit | 0.885 / 0.894 |
| 256 bytes | 64 | miss | 0.911 / 0.875 |
| 256 bytes | 4096 | hit | 0.966 / 0.927 |
| 256 bytes | 4096 | miss | 0.843 / 0.959 |

Only the two 64-bucket hit shapes show separated candidate/control sample
ranges in both cohorts. No shape shows a separated loss. The slower-peer
target still qualifies only three shapes in both cohorts, the same three as
the normalized flat source; no miss qualifies. The 20.212 ns record-small-hit
control outlier and 7.742 ns record-large-miss candidate outlier are retained.
Control medians for scalar-small-miss and record-large-miss drift substantially;
ratios in those cells are attribution leads, not stable improvement estimates.

The selected power-of-two path bypasses division, but its test still executes
per query and the query grows from 196 to 242 instructions. Frames stay at
16 bytes with no hot calls or loop spills. All six native peer query bodies
and addresses match within the new pair. Keep this source fast path as a
measured alternative rather than selecting it while a different representation
is being evaluated: it helps small hits but does not close the miss gap.

### Chain home-index discriminator

Before measuring a combined chain/mask candidate, isolate the home-index
change from the full-operation prototype. The linked chain's empty-head miss
executes unsigned divide, multiply-subtract, head address and head load before
its sentinel/index-bound branch. The repeated entry-base load is absent on
this empty path. Rust and C++ use masking in their selected power-of-two paths.
This motivates one source change: the same exact power-of-two mask and
arbitrary-capacity modulo fallback, used for both placement and lookup, with
the zero-capacity guard preserved. It is a hypothesis about the dependent
address calculation, not a cycle attribution from instruction counts.

Use the frozen equal-reserve chain layout, normalized caller, hash, seeds and
payloads; do not combine the trial with replacement/removal or growth changes.
Require a division-free selected native path, retained nonpower fallback,
unchanged bounds and no new calls or spills before timing. Run capacity-three
hit/miss oracles as well as the full query/cleanup/accounting checks in both
arms of a fresh common driver. Confirm native peer identity within that pair,
then apply the existing two-cohort protocol with every adverse observation
retained. A separated loss defeats a universal improvement claim; any gain
still qualifies only this query shape and does not select the library layout.

#### Chain home-index short-interval outcome

The first pair adds 1,152 observations with unchanged equal-reserve accounting.
Each arm passes 98 timed and 98 accounting oracle cases, including the
capacity-three fallback; the candidate's eight fault controls reject. Native
peer bodies and addresses match within the new pair. The query grows from
191 to 234 instructions with its 16-byte frame unchanged, no calls or loop
spills. The power-of-two route bypasses division but adds conditional work.

| Payload | Buckets | Query | Chain cohort 0 / 1, ns | Masked chain cohort 0 / 1, ns |
|---|---:|---|---:|---:|
| 8 bytes | 64 | hit | 1.987 / 1.924 | 1.643 / 1.670 |
| 8 bytes | 64 | miss | 1.731 / 1.732 | 1.620 / 1.588 |
| 8 bytes | 4096 | hit | 1.997 / 1.957 | 1.883 / 1.935 |
| 8 bytes | 4096 | miss | 1.620 / 1.556 | 1.495 / 1.542 |
| 256 bytes | 64 | hit | 1.888 / 1.902 | 1.642 / 1.629 |
| 256 bytes | 64 | miss | 1.645 / 1.653 | 1.563 / 1.590 |
| 256 bytes | 4096 | hit | 2.285 / 2.242 | 2.189 / 2.114 |
| 256 bytes | 4096 | miss | 1.596 / 1.614 | 1.611 / 1.539 |

Both small-hit shapes and the scalar-small-miss shape separate from the
unmasked chain's sample range in both cohorts. No separated loss occurs,
but the record-large-miss median is adverse in cohort zero (1.009 times the
control). Five shapes separate from the slower native peer in both cohorts;
both small misses still overlap, and the record-large-hit range now overlaps
in cohort one despite a favorable median. Retain record-small-miss candidate
outliers of 2.231 and 2.722 ns. This is not evidence that the mask closes the
whole query target. Exact deltas and raw samples remain in the existing
lookup evidence; the full-operation prototype is a separate source.

#### Longer-interval stability discriminator

The initial chain/mask comparison has overlapping small-miss ranges and loses
one wide-hit range qualification to an adverse sample even though its cohort
medians improve. Retain that complete short-interval campaign. Before another
run, change only the existing driver's work argument from 2,097,152 to
16,777,216 queries per observation: both unchanged linked images, all peers,
the same nine seeds, two reversed-order cohorts and off-clock preparation and
cleanup. This targets intervals near or above 25 ms instead of 3 ms and tests
whether short-interval disturbance explains the inconclusive comparisons.
It changes neither the algorithm nor the query key distribution. The first
requested work count, 33,554,432, was refused by the existing driver before
measurement because its maximum is 16,777,216. Retain that exit-one log as an
instrument-limit observation, not a timing sample. The revised count is fixed
before the valid campaign and preserves both binaries unchanged.

Keep the longer campaign separate, preserve every outlier, and apply the same
full-range comparison and at-most-ten-percent cohort-drift requirement.
Longer intervals do not themselves qualify a cell, and must not be pooled with
the earlier samples to manufacture a passing range. Independently check
checksums and geometry at the larger work count. No simultaneous heavy build
or timing is allowed during the pair. This is a measurement-stability test,
not permission to relax the performance target.

#### Longer-interval outcome

All four valid arms returned zero with 288 samples each; the minimum interval
was 24.082 ms. The 1,152 rows form a separate retained campaign. Median query
time in nanoseconds, cohort zero / cohort one:

| Payload | Buckets | Query | Masked chain WF | Rust | C++ |
|---|---:|---|---:|---:|---:|
| 8 bytes | 64 | hit | 1.609 / 1.602 | 2.145 / 2.140 | 1.608 / 1.585 |
| 8 bytes | 64 | miss | 1.562 / 1.569 | 1.593 / 1.590 | 1.554 / 1.584 |
| 8 bytes | 4096 | hit | 1.829 / 1.808 | 2.273 / 2.223 | 1.640 / 1.658 |
| 8 bytes | 4096 | miss | 1.521 / 1.498 | 2.182 / 2.149 | 1.474 / 1.448 |
| 256 bytes | 64 | hit | 1.665 / 1.649 | 2.369 / 2.343 | 1.625 / 1.625 |
| 256 bytes | 64 | miss | 1.573 / 1.557 | 1.592 / 1.636 | 1.563 / 1.565 |
| 256 bytes | 4096 | hit | 2.080 / 2.181 | 2.457 / 2.541 | 2.122 / 2.196 |
| 256 bytes | 4096 | miss | 1.534 / 1.562 | 2.164 / 2.213 | 1.754 / 1.808 |

Every candidate median beats the slower peer's median in both cohorts, and
candidate/control medians improve in all sixteen cells (ratios 0.819--0.977).
Full-range target separation still holds for only five of eight shapes:
both small misses overlap, and the record-large hit overlaps in cohort one.
The maximum cohort drift of each shape's candidate and both peer medians is
below five percent, so it is range separation, not that drift requirement,
that remains unqualified. A 2.685 ns candidate sample prevents the latter
cell from passing. Only the record-small miss separates from the unmasked
chain in both cohorts; there is no separated loss. Longer intervals have not
resolved every range comparison. No sample is dropped and no target is
relaxed; move to mutation costs before selecting the representation.

### Full-operation index-chain correctness discriminator

The separate `full_operation_index_chain` record in the same evidence retains
ordinary WF operations, additional controls, the exact adaptation of the pinned
maintained program, all build/run commands and identities, and the policy
observations. Replaying that patch and concatenation reproduces the measured
source bytes. The current compiler, library and formal program are unchanged.
This prototype is bounded to a research ceiling of 8192 and has been executed
in one optimized native configuration; it is not a three-mode library release.

The map owns an initialized dense prefix of `{key, value, next}` entries and
integer bucket heads. Replacement exchanges the complete key/value pair.
Removal unlinks the selected entry, takes the final owner and, when needed,
swaps it into the hole, then repairs the old final index by hashing the moved
key and walking its chain with explicit bounds. This adds a hash call on a
nonfinal removal, but avoids a full-capacity repair scan or additional metadata
in each entry. For consistent key behavior it preserves reachability. With a
changed hash environment, repair may find no link; that is an ordinary bounded
exit, not an assertion that the path is impossible. Every subsequent access
still checks its index, and dense iteration and cleanup retain every survivor.
FN-4 requires safety under inconsistent behavior; the library's ordinary map
meaning additionally requires the consistent protocol its interface states.

Scalar, wrap, collision-cycle, borrowed edit, owning-pair, zero-sized,
six-position removal-topology and changed-hash cleanup groups each report
zero failures. The latter preserves and consumes every remaining owning pair
even when lookup cannot reach the moved entry. Four controls demonstrate that
the observations can reject wrong behavior: omitting incoming-link repair
causes 13 topology and five owning failures; omitting the owner swap causes
24 topology and four owning failures; lost and duplicate release-account
observations each cause ten owning and eight changed-hash failures. The last
two controls change the recorded count, never perform duplicate frees.
All four executables return one; the valid observer returns zero.

The current public shape and pressure policy remain incompatible. The adapted
corpus preserves all expected values and branches, but reports zero for the
removed tombstone field. It retains eight failures in the old growth checks:
seven expected tombstone counts (10, 10, 1, 6, 7, 40, 40) are zero, and one
post-insertion capacity is 16 where the old pressure rule expects 32. A
separate replay of all sixteen checkpoints confirms exactly these differences;
every expected population count matches. The observer explicitly requires
those eight differences rather than counting this as a compatibility pass.

This establishes a usable mutation prototype, not the final layout or policy.
Before selecting it, compare isolated replacement and remove/reinsert at fixed
capacity with the flat library and native peers, including complete returned
payload digestion and final cleanup. Keep construction and cleanup outside
timing, use the same hash, keys, seeds and payloads, and exclude automatic
pressure-policy changes by using the no-growth insertion operation. Rehash
and reserve costs follow separately. Their measurements and independent
oracles remain outstanding.


#### Isolated mutation comparison contract

The next comparison fixes capacity at 64 or 4096 buckets and population at
half that value, with the same salted hash, odd keys, nine seeds and 8/256-byte
payloads as the query comparison. It measures two operations separately:
duplicate replacement, and remove followed by reinsertion of that key.
Construction, geometry checks, verification and final cleanup are outside the
clock. Each timed operation constructs a new value and consumes every word of
the returned old value. These are complete owning-operation costs, not bare
insertion latency. Generation persists across warmup and measured batches;
the independent oracle must account for that state rather than accepting a
checksum from a reset population.

WF uses the existing no-growth insertion operation to exclude the prototypes'
different automatic-pressure policy. Rust uses ordinary `insert` and
`remove_entry`; C++ uses `try_emplace`, mapped-value exchange, and extraction
whose node is destroyed before reinsertion. Native node allocation during
churn is part of that container's ordinary cost. Accounting is separate from
timing and must report it, not require every implementation to allocate zero.
All implementations retain the full offered-value construction and returned
payload observation; optimizers may eliminate legitimate redundant copies.
No artificial transfer is added to equalize their generated instructions.

Before timing, independently check batch results, persistent generation,
post-batch hits and misses, geometry, full final contents and release ledgers.
Faulted result, generation, wide-payload and cleanup observations must fail.
Inspect the actual linked batch paths and retained helpers. Run one matched
campaign with two reversed-order cohorts, retain every sample, and apply the
existing range and cohort-stability criteria without pooling query samples.
Report Rust and C++ separately, with the flat WF implementation as the
representation baseline. Fix work at 1,048,576 logical replacements or
remove/reinsert pairs per sample before timing; report a pair as one churn
operation, not as either component's isolated latency. Rehash, reserve and
the final public representation remain outside this comparison.


#### Mutation running-index discriminator

The current lookup alone uses a bounded running bucket index. `find` and
`try_put`, reached by removal and replacement, still derive each bucket from
home and step; the inspected wide native path retains a compare, select and
addition per probe. Apply lookup's ordinary running-index spelling to these
two helpers, preserving the same visited slots, hash/equality call order,
arbitrary capacities, first deleted-slot preference and complete refusal
outcome. This changes no layout or API contract.

Compare that source-only candidate with the frozen baseline through the same
isolated mutation adapters and linked peer objects. Inspect the reached native
probe loops, then require repeatable measured gains with no separated loss
before selecting it. Keep all unchanged and adverse cells. Full correctness
includes wrap at a non-power-of-two capacity, replacement beyond a tombstone,
a complete probe cycle and preservation of a refused owner. Removing native
instructions alone is not the selection criterion.


#### Mutation running-index outcome

The four frozen arms each completed with 288 observations, retained separately
in `mutation-api-samples.csv`; the shortest interval was 2.328 ms. Each sample
uses one full untimed warmup on the same owner, then 1,048,576 timed operations.
The following are nanoseconds per complete replacement or remove/reinsert pair,
cohort zero / cohort one. New-value construction and full old-value digestion
are included; preparation, verification and final cleanup are excluded.

| Payload bytes | Buckets | Operation | Previous WF | Running-index WF | Rust | C++ |
|---:|---:|---|---:|---:|---:|---:|
| 8 | 64 | replace | 2.671 / 2.679 | 2.503 / 2.413 | 2.522 / 2.504 | 2.779 / 2.743 |
| 8 | 64 | churn | 8.894 / 8.452 | 7.906 / 8.231 | 16.278 / 16.270 | 27.994 / 27.997 |
| 8 | 4096 | replace | 3.616 / 3.708 | 3.599 / 3.469 | 2.587 / 2.660 | 4.141 / 4.264 |
| 8 | 4096 | churn | 13.753 / 13.943 | 13.040 / 12.534 | 10.340 / 10.547 | 30.078 / 30.921 |
| 256 | 64 | replace | 29.129 / 29.389 | 27.680 / 28.119 | 22.154 / 22.393 | 19.079 / 19.045 |
| 256 | 64 | churn | 37.863 / 37.658 | 36.701 / 37.154 | 27.450 / 27.288 | 50.461 / 49.797 |
| 256 | 4096 | replace | 32.839 / 32.965 | 31.555 / 31.562 | 27.600 / 27.148 | 23.358 / 23.636 |
| 256 | 4096 | churn | 53.796 / 52.109 | 48.171 / 47.989 | 32.132 / 32.205 | 60.997 / 60.954 |

All sixteen candidate cohort medians improve, with candidate/control ratios
0.889--0.995. The large-record, large-table churn shape has separated sample
ranges in both cohorts, improving by 10.5 and 7.9 percent. In that shape,
the unchanged Rust and C++ peer medians instead rise by 0.7--2.5 percent;
normalizing WF's median ratio by either peer retains an 8.5--12.6 percent
improvement. The other shapes have overlapping before/after ranges; no
separated loss occurs. The largest
between-cohort spread of the paired median ratio is 9.553 percent, within
the registered ten-percent limit. Retain the source change on this bounded
benefit and unchanged operation contract; do not interpret every median
movement as an independently established gain.

All four churn shapes separate from the median-slower native peer in both
cohorts. Scalar replacement medians beat C++, but the full ranges do not
qualify in both cohorts. Wide replacement remains slower than both peers.
Thus four of eight shapes pass the strict target; this does not complete
HashMap optimization. The retained chain prototype improves scalar mutation
medians but has worse wide replacement medians than the flat candidate:
31.465/31.550 ns at 64 buckets and 33.634/33.707 ns at 4096. Its wide small
churn is also slower. Its lookup gains alone therefore do not select it as
the production representation.

Native inspection confirms the intended attribution: the scalar flat batch
shrinks from 205 to 197 instructions and from a 144-byte to a 112-byte frame;
the wide batch changes from 384 to 381 instructions with its 1024-byte frame
unchanged. Probe reconstruction disappears, but the wide `try_put` entry
snapshot, returned-pair transfers and inactive enum clears remain. The latter
are a previously rejected compiler optimization, with its losses retained in
TODO; these observations do not reverse that rejection. All nineteen examined
chain definitions have unchanged normalized instructions and relocations.
Peer loop addresses agree across the final pair, but their call bytes differ
at relocated Rust rehash and C++ allocation/unwind targets; ordinary C++ churn
reaches the relocated allocation stubs. Retain that linked-layout limitation.

The extended maintained HashMap program passes sequential and parallel
lowering, each both normally and with 36 allocations observed released once.
Its new capacity-three sequence observes full refusal, a missing removal,
wrapped removal, replacement beyond a tombstone, reuse, rehash and the exact
returned/final values. The focused gate-profile test took 11.46 seconds,
including 5.59 seconds building its test executable and 5.27 seconds executing;
the separate clean compiler build took 72.11 seconds.

Each of the four measurement/accounting images passes 96 owner traces with
three successive batches of zero, one and two rounds. Twenty deliberately
faulted runs fail with the intended diagnostics, including corruption of the
last wide-payload word that leaves the first-word lookup check unchanged.
The two 32-row allocation tables are byte-identical. Flat WF reserves one
backing of 24/272 bytes per bucket; chain WF reserves two backings totaling
32/280 bytes per bucket. Rust reserves 17/265 bytes per physical bucket plus
eight bytes. C++ reserves eight bytes per head plus 32/280 bytes per live
node. Those are observed allocation requests, excluding allocator overhead.
WF, Rust and C++ replacement allocate nothing during the batch; C++ churn
releases and allocates one node per operation. Every final ledger is empty.
`mutation-api-evidence.json` retains source, compiler and image identities,
commands, exact sources, independent checks and raw accounting observations.


#### Direct-field replacement rejected

The next discriminator was recorded before applying the candidate in the
retained evidence: match the borrowed bucket, swap its key and value fields
with the offered locals, and return the old complete pair. Insertion and
migration keep the exchange helper. The public interface, probes, callback
arguments and order, ownership outcomes and representation are unchanged.
The native test asks whether the complete reached replacement path removes
staging and the exchange call without increasing hot spill traffic; if it
does, apply the same timing contract with the running-index version as control.

Native inspection supports timing but does not establish a gain. On a
nonempty map with the first probed filled key equal, the control's `try_put`
and exchange together execute 135 instructions versus 131 for direct fields.
The offered snapshot and its reload each shrink from 256 to 112 bytes;
total register-save traffic is unchanged at 96 bytes, and the nested call
disappears. However, the nested peak helper stack rises from 352 to 480 bytes,
and the static `try_put` body grows from 126 to 217 instructions. The outer
wide batch still has 381 instructions and a 1024-byte frame, constructing and
observing all 32 words. Counting `try_put` alone would misattribute the
control's work in exchange, so the comparison includes both helpers.

The separate fresh campaign retains every sample in phase
`rejected-direct-field-discriminator` of `mutation-api-samples.csv`, without pooling
with the prior running-index campaign. Two reversed-order cohorts use the
same driver, peers, runtime objects, seeds and work. Values below are ns per
complete operation, cohort zero / one.

| Payload bytes | Buckets | Operation | Running-index control | Direct fields | Rust | C++ |
|---:|---:|---|---:|---:|---:|---:|
| 8 | 64 | replace | 2.376 / 2.443 | 2.360 / 2.341 | 2.501 / 2.500 | 2.785 / 2.740 |
| 8 | 64 | churn | 7.996 / 7.939 | 7.907 / 7.892 | 16.090 / 16.255 | 27.705 / 27.739 |
| 8 | 4096 | replace | 3.378 / 3.386 | 3.217 / 3.208 | 2.594 / 2.620 | 4.306 / 4.420 |
| 8 | 4096 | churn | 14.040 / 12.855 | 17.666 / 16.681 | 10.530 / 10.328 | 29.963 / 30.302 |
| 256 | 64 | replace | 27.484 / 27.590 | 30.813 / 31.012 | 22.119 / 22.373 | 18.939 / 19.074 |
| 256 | 64 | churn | 37.086 / 37.554 | 42.089 / 41.413 | 27.087 / 27.425 | 51.071 / 50.165 |
| 256 | 4096 | replace | 30.853 / 31.147 | 29.260 / 29.428 | 27.224 / 27.683 | 22.943 / 23.548 |
| 256 | 4096 | churn | 47.649 / 47.751 | 54.024 / 54.757 | 31.404 / 31.567 | 59.574 / 60.182 |

All 1152 rows are retained; the shortest interval is 2.327 ms.
Wide replacement at 4096 buckets improves by about five percent with separated
ranges in both cohorts, but wide replacement at 64 buckets regresses by about
twelve percent, also separated. Wide churn regresses by ten to fifteen percent
with separated ranges at both sizes. Scalar large churn medians also worsen,
though its ranges overlap. The candidate fails the registered no-separated-loss
criterion and is not retained. Fewer executed instructions and less snapshot
traffic are insufficient grounds to override these observations.

The candidate passes the strengthened maintained owning program and each
native image's 96 owner traces; ten fault controls fail with the intended
diagnostics, and its 32-row accounting record equals the running-index control.
The existing owning corpus now also checks all 31 inline words of each
256-byte node against its child serial on consumption. Existing observations
already distinguish the returned old equivalent key from the retained offered
key and observe each child release exactly once. The added full-content
observation is retained independently of the rejected optimization.

On the restored running-index implementation, the strengthened fixture passes
in 37.26 seconds including test-executable rebuild, with 27.34 seconds in the
case. Phase timings locate the increase in the two parallel native builds
(11.32 and 11.40 seconds); WF compilation stays below 0.70 seconds per mode,
and each program execution below 0.28 seconds. The sequential native builds
take 1.53 and 0.70 seconds. This is additional test compilation cost, not a
container runtime measurement; the full-content ownership observation remains.

A separate native sensitivity check with the frozen running-index compiler
returns zero for the unchanged strengthened program. Incrementing only word 30
in each constructed node, while leaving every child and key serial unchanged,
returns status one. The new full-content observation therefore detects a
corrupted tail independently of the existing identity and release checks.


#### Wide-owner waiting-frame cost

The loop-constructor fixture at `e39dbc2776421b689fcb8db11ef8dc8b039a17e1`
exposes a separate compilation cost. With the frozen running-index compiler
and ordinary CLI `--par` policy, its LLVM module is 5,122,451 bytes with 254
definitions. `wf_hash_map_test_owned` alone occupies about 4.26 MB, versus
about 64 KB for its sequential clone. Repeated complete waiting-frame address
types contribute to this expansion; this is not evidence that the 31-word
checking loop itself was unrolled. The module SHA-256 is
`d8aebff3ffe05d11f35d363bae8d9f0e09fa59561079c2b665c86502d868ab92`.
The retained phase records locate 11.32 and 11.40 seconds in parallel native
compilation on the local ARM host, with execution below 0.28 seconds per image.

Linux corpus reached the 60-second native-child deadline twice at
`b9ccb67de4d2ab3110418f47670944c5d000970a`. The improved diagnostic at
`e39dbc2776421b689fcb8db11ef8dc8b039a17e1` identifies `/usr/bin/clang -x ir`
with `-O2` as the timed-out command, before program execution. macOS passed.

The current constructor computes the same wrapping base once, fills all 31
words with it and writes indices 1 through 30 explicitly. Word zero is already
`base +wrap 0`; each other word retains `base +wrap index`. This removes an
independent initialization loop from the transfer witness, not its full-content,
identity, cleanup or lowering-mode observations. The original loop remains
available at the revision above and its general compilation cost stays in TODO.

Using the same frozen compiler, the new parallel module is 866,051 bytes
(SHA-256 `80b5f5b1247a16bc382627c7361052fe8b1c78ada03d8977568516103aafe3de`).
Sequential ordinary/observer Clang object builds take 0.572/0.626 seconds;
parallel builds take 0.592/0.617 seconds, with about 0.05 seconds per link.
All four executions exit zero; both observers report exactly 36 allocations,
each released once. Changing only word 30 from `base +wrap 30` to
`base +wrap 31` makes the native program exit one. The positive checks take
5.55 seconds and the independent tail-fault check 1.66 seconds, including
emission, native compilation, linking and execution. The source hash, commands,
phase observations and logs are retained in `mutation-api-evidence.json` under
`wide_owner_fixed_constructor`. Exact-head CI at
`5a6c371142079585c9e0971c91846dc54170c61e` passed all selected Linux and macOS
correctness groups, Linux and Windows native checks, and compute regression;
design readiness stayed skipped for the Draft PR. No timeout, assertion or
runtime overlap policy was weakened.

#### Adjacent aggregate Load-to-Store trial

The bounded compiler trial forwards a same-block, adjacent, single-use aggregate
load into its store while retaining snapshots when its eligibility conditions
fail. Native comparison covers 243 functions: only three standalone `swap`
bodies change, so the whole object is different. The reached wide `try_put`,
exchange and mutation batch retain identical instructions and relocations
(126, 87 and 381 instructions respectively). The intended mutation code
improvement is absent. The patch, preregistered criterion, compiler and image
hashes, commands and native comparison are retained in
`mutation-api-evidence.json` under `adjacent_load_store_trial`; all four compiler
trial files were restored.

The separate phase `rejected-adjacent-load-store-discriminator` retains all
1152 samples in `mutation-api-samples.csv`. None of the 16 matched flat-map
cohort cells has a range-separated gain or loss; unchanged reached code gives
no basis to attribute a mutation benefit to this trial. Both arms pass their
96 owner traces in timed and accounted images, all ten fault controls per arm
fail with their specific diagnostics, and their 32-row ledgers are identical.
The eligibility check passes (0.03 seconds after a reported 1m 12s test build;
73.20 seconds guarded total), and all 30 owning-place regressions pass
(16.56 seconds execution, 16.66 seconds guarded total). The trial is rejected;
these timings select no compiler, library or representation change.

#### Diagnostic entry-forwarding floor

A separate hand-edited LLVM diagnostic removes the 256-byte private input
capture from the reached wide `try_put` and redirects its three uses to incoming
backing. All other raw functions are unchanged. The native body shrinks from
126 to 95 instructions and its frame from 288 to zero bytes; eight paired
load/store entry transfers disappear. The tested first-hit boundary executes
23 fewer instructions. The 381-instruction wide caller and 87-instruction
exchange remain identical after address normalization. The patch, criterion,
raw LLVM pins, images, commands and controls are retained under
`entry_forward_floor_diagnostic` in `mutation-api-evidence.json`.

All 1152 samples are retained separately in phase
`diagnostic-entry-forward-floor`. Wide replacement has separated lower ranges
at both capacities in both cohorts. One wide 64-bucket churn cohort also
separates; no matched flat-map cohort cell has a separated loss. Values below
are median ns per complete wide replacement, cohort zero / one; peers are
from the same diagnostic images.

| Buckets | Running-index control | Diagnostic floor | Rust | C++ |
|---:|---:|---:|---:|---:|
| 64 | 27.503 / 27.733 | 23.910 / 24.293 | 22.305 / 22.612 | 18.914 / 19.072 |
| 4096 | 31.473 / 31.819 | 28.594 / 28.813 | 27.770 / 28.003 | 24.115 / 23.894 |

Both arms pass 96 owner traces in timed and accounted images, all ten fault
controls per arm fail with the intended diagnostics, and their 32-row ledgers
are identical. This is a diagnostic floor, with no production selection:
general result/input aliasing, backing writes, capture before output writes
and scheduling lifetimes remain unproved by the IR patch. The benchmark's
distinct input/output storage and synchronous helper calls do not establish
those premises for arbitrary programs. The measurements motivate a sound
capture optimization; they are not evidence for the separate compiler
prototype currently being tested.


#### Ordinary incoming backing: measured implementation

The actual compiler trial forwards only an unexposed indirect `Own` input whose
complete storage group contains that input and its typed identity CFG carries.
Loads and interior projections remain snapshots. Liveness must establish a
private capture before a hidden-result write; defined synchronous callees need
a finite eager-capture guarantee, while exposed, mixed, result-backed, deferred,
unknown and recursive cases retain the copy. Review subsequently found that
Jump-edge writes into the result root need the same guard. The measured compiler
predates that repair. The final guarded compiler reemits both benchmark LLVM
modules and three native objects byte-identically (3.69 seconds, exit zero),
so the retained timings apply to those final outputs without a new timing run.
Both final focused tests pass (66.33 seconds guarded, a reported 1m 01s build
and 4.04 seconds execution); the final CLI build takes 34.42 seconds. Source
pins, patch and all five identity comparisons are retained. Source acceptance,
ABI and alias attributes are unchanged; the compiler placement choice is
provisional work-branch choice Q203. Its bounded compiler/design review is
complete. The published [360a711 revision](https://github.com/mbbill/Whitefoot/commit/360a711ad33b16806fc43b9242a56e6efe5e7f10)
passed the canonical correctness groups, native and compute checks; Draft
readiness stayed skipped.

The separate `actual-incoming-forward-discriminator` phase retains 1152 samples
in `mutation-api-samples.csv`; it is not pooled with the diagnostic floor. The
wide `try_put` shrinks from 126 to 95 native instructions and its frame from
288 to zero bytes, removing the 256-byte entry capture. Reached exchange and
wide caller remain at 87 and 381 instructions. Wide replacement at both sizes
and wide 64-bucket churn separate in both cohorts; the other ten flat cohort
cells overlap, with no separated loss. Median ns per complete wide replacement
(cohort zero / one), with peers from these actual candidate images, are:

| Buckets | Running-index control | Actual compiler candidate | Rust | C++ |
|---:|---:|---:|---:|---:|
| 64 | 27.520 / 27.595 | 23.729 / 23.797 | 22.100 / 22.025 | 18.832 / 18.863 |
| 4096 | 31.159 / 30.723 | 28.000 / 28.228 | 26.987 / 26.993 | 23.659 / 23.767 |

Both arms pass 96 owner traces in timed and accounted images, all ten fault
controls per arm fail distinctly, and their 32-row ledgers are identical. The
planner retry passes two tests (20.18 seconds construction, 3.85 execution),
the final storage module passes 25 tests (0.05 seconds execution, 0.10 guarded)
and owning-place tests pass 31 (18.06 seconds execution, 18.15 guarded).
Final formatting/lint passes in 4.11 seconds. These include full copied content, retained inputs, nested-owner
cleanup, identical and partially overlapping input/result storage, and independent
tag/tail corruption. The initial fixture parse failure is retained. The published revision above passed the complete gate; the bounded
compiler/design review is complete.

The edge regression uses valid typed IR: unused `AddressOf` operations expose
the two live private alternatives without changing their values or introducing
a source `Load` snapshot. Removing only the Jump-write guard makes the planner
select incoming parameter zero, where it must retain a private copy, and the
independent native observer rejects a wrong result tag with exit 31 instead
of zero. Both focused tests fail as intended (Cargo exit 101; 63.44 seconds
guarded, 1.29 seconds execution). The exact guard-removal patch and log are
retained. This witnesses a general backend alias obligation, not a WF source
bug; the restored guard passes both final focused tests.

The normal whole-Map screen retains all 18480 rows and an identical 420-row
allocation ledger. Of 168 WF cohort cells, 36 separate positively, 130 overlap
and two separate adversely: cohort-zero wide hit at capacity 64/population 56
is 3.38 percent slower under native-default hashing and 4.37 percent under
aligned hashing. The longer hit discriminator retains another 880 rows: all
eight WF cohort cells overlap, and the wide medians differ by less than 0.18
percent. The original adverse observations remain; their direction is not
reproduced, and this does not establish a layout or codegen cause. Neither
screen includes retained callbacks or explicit rehash timing. The provisional
compiler choice has the published revision's successful gate; its bounded
compiler/design review is complete. The overall container performance goal is not established and
the PR remains Draft.

A separate native boundary diagnostic adds exactly two `alwaysinline` tokens
on wide `try_put` and exchange. Native construction exits zero in 2.29 seconds,
but the wide caller grows from 381 to 570 instructions. Its frame shrinks from
1024 to 448 bytes and the old 264-byte result staging disappears, while equivalent
256-byte scalar spill/reload traffic remains. This fails the preregistered
no-relocated-transfer gate. Correctness and timing were not run at that earlier
stage: it is a rejected native-code hypothesis, not a measured performance loss
or a general inline recommendation. The later partial-cost question below does
not revise that verdict. The tiny recipe, criterion, exact patch, pins, direct build
statuses and relevant caller excerpts are retained separately in the same archive
under `rejected-inline-boundary`.

#### Partial savings at the inline boundary

A later preregistered question asks whether the same diagnostic's smaller offered
staging (256 to 120 bytes), removed helper edges and smaller reached stack
(1024 + 64 to 448 bytes) outweigh its retained old-payload spills. This is a
separate frozen-IR cost question; it does not rescue the failed transfer-elimination
hypothesis or select a production inline policy. Both arms pass all 96 owner
cases in timed and accounted images, all ten fault controls per arm fail with
their exact diagnostics, and the 32-row ledgers are identical (0.98 seconds
guarded). The single fixed panel exits zero in 55.35 seconds and retains all
1152 rows separately in the archive; no retry occurred. Median ns per operation
(cohort zero / one), including all eight cells, are:

| Width | Buckets | Operation | Entry-forward control | Inline diagnostic | Rust | C++ |
|---|---:|---|---:|---:|---:|---:|
| word8 | 64 | replace | 2.338/2.340 | 2.357/2.336 | 2.481/2.468 | 2.772/2.740 |
| word8 | 64 | churn | 7.826/8.028 | 7.793/7.947 | 15.967/16.023 | 27.501/27.713 |
| word8 | 4096 | replace | 3.184/3.507 | 3.127/3.411 | 2.583/2.567 | 4.113/4.259 |
| word8 | 4096 | churn | 12.691/15.085 | 14.447/14.593 | 10.384/10.252 | 30.424/29.739 |
| wide256 | 64 | replace | 23.824/23.751 | 23.295/23.222 | 22.141/22.053 | 18.927/18.869 |
| wide256 | 64 | churn | 32.722/32.290 | 34.454/35.175 | 27.106/27.000 | 49.524/49.210 |
| wide256 | 4096 | replace | 28.419/28.044 | 26.124/26.545 | 27.012/26.994 | 23.250/23.401 |
| wide256 | 4096 | churn | 47.568/47.860 | 45.623/46.084 | 31.176/31.286 | 59.895/60.297 |

Only wide 4096-bucket replacement has separated lower ranges in both cohorts.
Wide 64-bucket replacement overlaps, so the required two-size improvement is
absent. Its wide churn medians worsen by 5.29 and 8.94 percent, although ranges
overlap. Scalar 4096-bucket churn changes by +13.84 percent in cohort zero and
-3.26 percent in cohort one: its paired-ratio spread of 17.681 percent exceeds
the registered 10-percent limit. The shortest interval is 2.326 ms; maximum
symmetric unchanged-peer median drift is 8.981 percent (chain control), or
2.875 percent for Rust/C++, within its separate limit. Fourteen flat cohort
cells overlap and none has a separated loss. The diagnostic still fails its
registered criterion; the large replacement observation supplies no qualified
global benefit or independent repeatability. Raw cohorts, criterion, commands,
controls, ledger, script and full reduction remain under `inline-partial-cost`
in the same archive and `inline_partial_cost_diagnostic` in the JSON.

A separate result-only disjointness diagnostic clones wide `try_put` and exchange
for the audited distinct input/result batch allocas, leaving original helpers and
other callers unchanged. All six native construction stages exit zero. Batch,
clone `try_put` and clone exchange remain at 381/95/87 instructions with
1024/0/64-byte frames; exchange has the same normalized sequence and simultaneous
complete old/offered payload lifetimes. `try_put` changes only its refusal copy
schedule to streamed vector pairs, with no successful-replacement improvement.
The registered native gate fails, so correctness and timing are not run. This
rules out that narrow result-only attribute benefit, not all disjointness or
ordering optimizations. Its minimal recipe, criterion, patch, callgraph, pins,
stage statuses and three reached-body excerpts are retained under
`result-only-disjoint` in the same archive.

#### Result-before-commit ordering floors

Two frozen-IR diagnostics change only the result-disjoint exchange clone to
materialize the complete old result before a common new-slot commit. Offered
input remains captured before all writes; original helpers, alias premises and
callers remain unchanged. The first uses one 264-byte old-pair transfer; the
refinement uses a scalar key plus a typed 256-byte record transfer. All six
construction stages pass in each trial (2.17 / 2.27 seconds guarded). The first
standalone exchange calls `memcpy` and has 68 instructions / a 304-byte frame;
the typed version has 77 instructions / a 272-byte frame, with the offered
256-byte stack capture still present. These are standalone bodies, not the
actual replacement path: both trials naturally inline the clones into a
byte-identical 613-instruction batch with a 624-byte frame and no helper `BL`.
The reached path retains old/new interleaving and scalar staging around its
later digest, rather than the required streaming without relocated transfers.
Both native criteria fail; correctness and timing are not run. Recipes, criteria,
patches, callgraphs, pins, direct statuses and compact body excerpts remain under
`ordering-result-before-commit` and `ordering-typed-fields` in the archive.

#### Conditional home mask in the source library

A separate source trial changes only the positive-count home initializations in
`hash_map_find` and `hash_map_try_put`: start with `iand(hash, count - 1)`, falling
back to `hash % count` when the capacity is not a power of two. Hash call order,
running-index probing, wrap, full refusal, tombstones, layout and complete owning
payload consumption stay unchanged. A fresh compiler embeds those candidate
library bytes; the library is then restored byte-exact. Selected 64/4096 native
paths bypass home division, while the generic modulo fallback remains. Word
batch instructions grow from 197 to 205 and wide batch from 381 to 386; frames
remain 112 / 1024 bytes, with no added helper call or stack-access change. Wide
`try_put` grows from 95 to 99 instructions with no frame; exchange remains
87 instructions / 64 bytes. The source/compiler pins identify the explicit
frozen baseline and candidate: the mutable gate CLI is not the restored baseline.

All 13 construction stages pass (15.88 seconds guarded, compiler real time
9.41 seconds). Both arms pass their timed/accounted 96-owner, three-successive-
batch checks and all ten specific fault controls per arm; their 32-row ledgers
are byte-identical. Both also compile and execute the maintained capacity-three
program, retaining its zero/wrap/full/refusal/tombstone and owner-cleanup
observations. Those 30 expected statuses pass in 5.97 seconds guarded. The one
fixed control/candidate/candidate/control panel exits zero in 54.46 seconds,
retaining all 1152 rows without retry. Median ns per operation (cohort zero /
one), with peers from candidate images, are:

| Width | Buckets | Operation | Entry-forward control | Source mask | Rust | C++ |
|---|---:|---|---:|---:|---:|---:|
| word8 | 64 | replace | 2.342/2.342 | 2.737/2.683 | 2.476/2.481 | 2.743/2.743 |
| word8 | 64 | churn | 7.842/7.913 | 6.971/7.059 | 16.068/16.123 | 27.625/27.603 |
| word8 | 4096 | replace | 3.165/3.412 | 3.694/3.824 | 2.593/2.589 | 4.232/4.305 |
| word8 | 4096 | churn | 13.489/13.745 | 12.155/13.029 | 10.159/10.379 | 29.975/30.055 |
| wide256 | 64 | replace | 23.743/23.688 | 20.783/20.854 | 22.011/22.048 | 18.887/19.373 |
| wide256 | 64 | churn | 32.271/33.420 | 27.165/28.429 | 27.091/27.024 | 49.673/49.514 |
| wide256 | 4096 | replace | 28.098/28.329 | 25.645/25.701 | 27.169/27.083 | 23.089/23.387 |
| wide256 | 4096 | churn | 48.777/48.928 | 39.351/39.707 | 31.250/31.212 | 59.078/60.050 |

All four wide cells have separated lower ranges in both cohorts, with median
reductions of 8.73 to 19.32 percent. All eight scalar cohort cells overlap;
scalar replacement medians worsen by 12.07 to 16.86 percent, and those adverse
observations remain. No flat cohort cell has a separated loss. The shortest
interval is 2.326 ms and maximum paired-ratio spread is 5.194 percent, but
unchanged chain word4096 churn median drift is 14.889 / 13.007 percent, exceeding
the registered 10-percent limit (Rust/C++ maximum is 3.746 percent). The campaign
therefore does not pass all instrument controls and does not qualify a production
choice or a global benefit. The source patch, original/candidate source, criteria,
compiler/image pins, raw cohorts, controls, exact ledgers and full reduction are
retained under `mutation-home-mask` in the same archive; the JSON separates this
source trial from the two rejected native ordering floors. The overall container
performance goal remains unestablished.

#### Source-mask final-peer qualification

A separately registered question tests those unchanged source-mask images
against the median-slower ordinary Rust/C++ peer in each cell/cohort. It requires
the candidate's full range below that peer's full range in all 16 conditions
(both peers on a median tie). This is a final-peer goal, distinct from a
no-Whitefoot-before/after-loss criterion; the earlier short-panel drift failure
remains unchanged. The existing source/check/native prerequisites are reused,
without rebuild or rerun. Fixed work increases from 1048576 to 4194304, including
four times the untimed warmup rounds, with the same driver, geometry and ownership.
This is a separate campaign, not pooled short-panel replication. All four direct
exits are zero in 216.67 seconds guarded; all 1152 rows, stable image hashes and
equal non-time fields are retained. Candidate median ns/op, cohort zero / one,
with each native peer reported independently, are:

| Width | Buckets | Operation | Source mask | Rust | C++ | Slower-peer separation c0/c1 |
|---|---:|---|---:|---:|---:|---|
| word8 | 64 | replace | 2.691/2.721 | 2.488/2.481 | 2.748/2.742 | overlap/overlap |
| word8 | 64 | churn | 7.494/6.969 | 16.085/16.126 | 28.904/27.499 | pass/pass |
| word8 | 4096 | replace | 3.722/3.764 | 2.589/2.590 | 4.199/4.321 | overlap/overlap |
| word8 | 4096 | churn | 11.708/11.767 | 10.449/10.222 | 29.466/29.951 | pass/pass |
| wide256 | 64 | replace | 20.910/20.782 | 22.114/22.088 | 18.854/18.857 | overlap/pass |
| wide256 | 64 | churn | 27.199/27.811 | 27.348/27.157 | 49.477/49.611 | pass/pass |
| wide256 | 4096 | replace | 25.439/25.377 | 27.093/27.045 | 22.960/23.511 | pass/pass |
| wide256 | 4096 | churn | 39.404/38.868 | 31.317/31.246 | 59.615/60.126 | pass/pass |

Five of eight cells pass both cohorts, and 11 of 16 cohort conditions pass;
scalar replacement overlaps C++ in both capacities/cohorts, and wide 64-bucket
replacement overlaps Rust in cohort zero. Candidate is also separated slower
than Rust on scalar replacement, C++ on wide replacement and Rust on wide
4096-bucket churn; the chosen slower-peer goal does not erase those comparisons.
Before/after scalar replacement medians worsen by 12.456 / 15.375 percent at
64 buckets and 22.274 / 21.523 percent at 4096, with all eight scalar cohort
ranges overlapping; all eight wide before/after comparisons show separated gains.
Minimum interval is 9.316 ms and maximum paired-ratio spread 7.098 percent,
but unchanged chain scalar4096 churn cohort-zero drift is 13.300 percent,
exceeding the retained 10-percent gate (native-peer maximum 4.751 percent).
Both the full peer goal and instrument criterion fail. No adoption or retry
follows. New criterion, runner, pins, direct metadata, every raw cohort and full
peer/adverse reduction remain under `mask-peer-qualification` in the same archive;
source/native/controls remain under `mutation-home-mask`. The complete goal
remains unmet.

#### Explicit mask fast arm: native stop

One ordinary-source spelling trial uses the existing source-mask candidate as
its causal control. In the two positive-count home blocks, it initializes the
index to zero, then explicitly assigns the mask result for a power of two and
the modulo result otherwise. Hash/equality order, probing, arbitrary capacities,
exchange, owners and public outcomes remain unchanged. The full mutation witness
and maintained capacity-three program emit successfully in both arms. All 15
construction/emission/native stages exit zero in 17.36 seconds guarded, including
a 9.285-second candidate CLI build; production library bytes are restored exactly.

The requested native fallthrough does not result. LLVM evaluates both `AND` and
`UDIV/MSUB`, then selects with `TST/CSEL`, so power-of-two capacities execute the
division again. This occurs in both scalar batch home calculations, wide churn
and the retained wide `find`/`try_put` paths. Scalar batch stays 205 instructions;
wide batch changes 386 to 385, `try_put` stays 99, `find` changes 64 to 63 and
`remove` 107 to 106. Exchange remains 87 instructions. Frames, stack-access
counts and hot calls do not grow, and object code shrinks by 48 bytes, but these
counts do not satisfy the registered division-bypass mechanism. The native gate
fails; runtime correctness, capacity-three native execution, allocator ledgers
and timing are not run. No rescue spelling or selection follows. The criterion,
two-block source patch, exact raw pair, compiler/source/restoration pins, direct
statuses and compact reached native excerpts are retained under
`mask-fallthrough-source` in the same archive. The earlier source-mask timing
verdicts remain unchanged.

#### Descriptor and payload alias metadata

A fact-only LLVM diagnostic gives the reached scalar mutation batch one fresh
entry-declared scope separating its live 80-byte caller owner from separately
allocated bucket backing. Its audited inventory marks 18 owner and 19 payload
accesses, leaving five locals unannotated; it asserts neither owner immutability
nor sibling-field or bucket-entry disjointness. Stripping the additions recovers
the control bytes. Both arms receive the same extra O3 pass and produce
byte-identical native objects; all 191839 linked disassembly lines match except
the filename header. The 205-instruction, 112-byte-frame scalar body retains both
in-loop count/mask selections. The equal extra pass itself changes four scalar
instructions versus the original mask image, so those changes are not metadata
effects. Scope metadata survives reoptimization, but unlocated LICM remarks
cannot explain a particular missed hoist. The native criterion fails: correctness
and timing are not run. This is no native response to one audited instrument,
not a general conclusion about aliasing. Exact premises, classification, recipes,
patch, pins, direct statuses and compact excerpts remain under
`descriptor-alias-native` in the archive.

#### Prepared capacity snapshot cost floor

The subsequent manual LLVM diagnostic uses that same unannotated extra-pass
control and replaces exactly two scalar bucket-count loads with one plain entry
snapshot. This is owner offset zero, distinct from allocation capacity at offset
eight. The prepared owner is live even for zero rounds/count; the 42-access audit
writes only owner offsets 24/32/72, accesses buckets through separate live backing,
and calls only bounded LLVM intrinsics. These local premises do not establish
a general WF contract or select a compiler transport. Native instructions shrink
205 to 201 with the same 112-byte frame and no helper call. Count and count-minus-one
move before both loops, saving one load/subtraction per positive power-of-two
replacement and two per churn. Per-item power-of-two tests/branches and payload
pointer loads remain. New rounds spill/reload occurs once per outer round and the
prologue now runs on zero-work paths. The shared extra pass also changes the wide
body from the source-mask candidate's 386 instructions to 665 in both current
arms (381 belongs to its no-mask baseline);
this collateral cannot be attributed to the snapshot.

All six native stages pass (2.17 seconds guarded). Both arms pass their timed and
accounted 96-owner checks, all ten specific negatives per arm and exact 32-row
ledger equality, including equality to the source-mask ledger: 30 expected
statuses pass in 3.11 seconds. The fixed four-process panel exits zero in 52.58
seconds and retains all 1152 rows with stable image hashes. Every one of the 16
flat cohort cells overlaps; there is no separated gain or loss. Scalar 64-bucket
replacement medians worsen by 1.570 / 3.556 percent, while scalar 4096-bucket
replacement changes by -1.615 / +1.702 percent; all complete ranges overlap. The shortest
interval is 2.328 ms, but scalar 4096-bucket churn's paired-ratio spread is 12.772
percent and unchanged chain scalar 4096-bucket replacement median drift is
14.011 / 16.596 percent, both exceeding the registered 10-percent limits.
Rust/C++ maximum drift is 3.334 percent. The cost floor fails its criterion and
selects no implementation or cause for the earlier scalar observations. Its
criterion, patch, complete access/call proof, recipes, controls, ledgers, raw
cohorts and all peer/adverse reductions remain under `prepared-capacity-snapshot`
in the same archive, separate from the alias negative.

#### Offered input disjointness: native frame removed, timing unqualified

A frozen-IR diagnostic adds exactly two offered-input `noalias` flags to the
existing result-disjoint wide clones. Its causal control is the prior result-only
clone, without source-mask, ordering or inline composition. The audited batch
passes separate 256-byte input and 272-byte result allocas, distinct from its
owner-derived heap cells; originals and other callers remain unchanged. This
local premise is not an ordinary callable-ABI promise: input/result overlap is
still permitted. Exchange falls from 87 to 75 instructions and its 64-byte
vector-save frame disappears through offered streaming. The reached caller stays
381 instructions / 1024 bytes and `try_put` stays 95 / zero; caller input/result
buffers and complete old-result materialization remain. All six native stages
pass in 2.288 seconds guarded. This native mechanism passes its initial criterion.

Both arms subsequently pass all four timed/accounted 96-owner checks, all ten
specific negatives per arm and identical 32-row ledgers: 26 expected statuses
pass in 1.536 seconds. The single fixed panel exits zero in 55.398 seconds,
retaining all 1152 rows and stable image hashes. All 16 flat cohort ranges overlap,
with no separated gain or loss. Scalar 4096-bucket replacement medians worsen
2.241 / 3.637 percent, scalar 4096-bucket churn worsens 4.601 percent in cohort one,
and wide 4096-bucket churn worsens 0.580 percent in cohort zero; these observations
remain inconclusive with overlapping ranges. The shortest interval is 2.328 ms
and maximum paired-ratio spread is 4.853 percent, but unchanged chain scalar
4096-bucket churn drift is 13.236 percent in cohort zero, exceeding the registered
10-percent limit (Rust/C++ maximum is 6.335 percent). The measured criterion fails;
the frame reduction establishes no qualified timing benefit or production alias
variant. Initial native-stage flags, later controls, raw cohorts, all peer/adverse
reductions, criteria, exact header patch and recipes are retained separately
under `offered-input-disjoint` in the archive.

#### Input-disjoint inline composition

A subsequent native-only trial uses that input/result-disjoint noinline candidate
as its causal control and adds exactly two clone `alwaysinline` attributes.
Reversing those edits recovers the control LLVM bytes. All six construction
stages pass in 2.258 seconds guarded, but the actual reached batch becomes
570 instructions / a 448-byte frame with 168 stack-access instructions and no
helper call, versus the current control's 381 / 1024 and 102 stack accesses,
followed by 95/zero and 75/zero-frame helpers. Its normalized instruction stream
is exactly the earlier failed-inline batch: 120-byte offered spills and equivalent
256-byte old-payload scalar spill traffic persists. That older image is explanatory
context, not the causal control or a timing comparison. The 256 bytes are total
old-word stack-store traffic: 32 events across 23 distinct old words, including
nine repeated stores during the post-commit shuffle, not a simultaneously resident
full payload. The native criterion fails, so correctness and timing are not run. No blanket input `noalias` or inline
policy follows from these trials. The minimal recipe, criteria, patch, callgraph,
pins, direct statuses and reached excerpts remain under `input-disjoint-inline`.

#### Replacement continuation placement: gains and churn losses

A post-O3 LLVM floor duplicates the existing complete Returned-value continuation
onto the successful-replacement predecessor, substituting 73 PHI inputs,
renaming 69 definitions and adding seven successor live-outs. Old key, reason,
all 32 payload words, the 131-based Horner digest, mutation ordering and other
predecessors remain. Both arms start at the same optimized stage and receive
identical ordinary extra-O3 native compilation. The causal control is therefore
594 instructions / 448-byte frame / 172 stack accesses, not the earlier one-pass
570 / 448 / 168 image. Candidate becomes 579 / 448 / 148 with no calls. Replacement
old-word stack-store traffic falls from 27 events across 22 words (216 bytes) to
ten events across ten words (80 bytes), without the post-commit reshuffle;
offered traffic changes from 128 to 120 bytes. All six native stages pass in
2.18 seconds guarded. This is an optimized-IR cost floor, not ordinary pre-O3
compiler emission; neither the compiler pipeline nor source workload changes.
The proposed pass-disabling alternative was not run.

The account construction and unchanged full controls pass in 3.35 seconds guarded
(1.702 seconds account construction, 1.458 execution): four timed/accounted
96-owner positives, 20 exact negatives and two byte-identical 32-row ledgers,
also equal to the prior ledger. The single fixed panel exits zero in 54.31 seconds,
retaining all 1152 rows with stable image hashes. Wide replacement improves at
both capacities in both cohorts, while wide churn loses at both capacities in
both cohorts. Median ns per complete operation, cohort zero / one, with native
peers from candidate images, are:

| Width | Buckets | Operation | Matching extra-O3 control | Continuation floor | Rust | C++ |
|---|---:|---|---:|---:|---:|---:|
| wide256 | 64 | replace | 23.932/23.810 | 21.630/21.641 | 22.141/22.112 | 18.862/18.853 |
| wide256 | 4096 | replace | 26.412/26.342 | 25.180/25.163 | 27.042/27.058 | 22.904/23.445 |
| wide256 | 64 | churn | 33.968/33.073 | 41.457/41.572 | 27.040/27.123 | 49.386/49.384 |
| wide256 | 4096 | churn | 42.170/43.513 | 49.383/48.777 | 31.305/31.274 | 59.456/59.487 |

The four wide gains and four wide losses are separated; all eight scalar cohort
cells overlap. Scalar 4096-bucket replacement medians worsen by 10.168 / 4.261
percent, and those overlapping adverse observations remain. Scalar 4096-bucket
churn's paired-ratio spread is 13.954 percent, exceeding the registered 10-percent
limit. Minimum interval is 2.327 ms; maximum unchanged-peer drift is 8.064 percent
(Rust/C++ 6.362 percent), within its separate limit. The complete matched panel
fails its criterion and the floor is rejected without adoption or rerun. The
churn-loss cause is not settled by this record. Criteria, exact optimized
controls/source pins, patch/replay verification, full PHI/live-out inventory,
compact native traces, controls, ledgers and all raw/peer/adverse outcomes are
retained under `replacement-continuation` in the same archive. A general production
mechanism would still need qualification through ordinary pre-O3 emission;
this diagnostic selects none.

#### Generic atomic slot update: admission stop

A separate ordinary-source attempt routes the complete exchange outcome into
an initially empty fixed `Slots<HashMapPut<K, V>, 1>` while updating the old slot
through OP-12. Its minimal rejected call is:

```wf
  let output = slots_new::<HashMapPut<K, V>, 1>();
  set cells^.inner[index] = hash_map_exchange_slot::<K, V>(previous: move cells^.inner[index], key: move key, value: move value, output: &output);
```

The candidate witness rejects at `hash-map.wf:204:7` with
`WIN-3 LinearAssignmentTarget`. The [active specification](../../../../spec/kernel-spec.md)
limits OP-12 to affine/copy targets; unbounded `K`/`V` provide no capability at
the FN-2 symbolic instance, so the slot is linear under PROV-6 and WIN-3 refuses
assignment over it. The fixed result sink does not alter that target class.
This is an existing specified capability limit, not a compiler defect or a
native-performance failure. The prepared criterion's generic eligibility premise
was invalidated at admission; no drop bound was added and no rule was changed.
The candidate CLI builds successfully in 9.414 seconds; the guarded trial exits
one in 11.71 seconds at candidate witness emission. Candidate capacity-three
emission, native construction, correctness and timing are not reached. Original
library bytes are restored and verified. The complete diagnostic, source patch,
criterion, compiler/source pins, statuses and restoration hash remain under
`atomic-slot-admission-stop` in the same archive; no gain is inferred.

#### Tag-directed private capture: native pass, timing unqualified

A typed-IR floor changes only the original wide exchange body in the ordinary
production raw module. Eager offered capture and Filled construction remain;
it reads the old tag, captures the complete old key/Record into private storage
only for Filled, commits the whole 272-byte new slot, then constructs the unchanged
result. Offered/active-old reads precede external writes, and cell commit still
precedes result writes, preserving overlap order without a new alias premise.
There are no clones, inline hints, extra-O3 passes, source or compiler changes.
Empty/Deleted now bypass 17 field-load instructions covering 264 old bytes. Exchange
changes from 87 to 90 instructions with its 64-byte frame unchanged; its valid
0/1/2 routes change from 60/62/72 to 46/48/74 instructions. These counts do not
establish timing causality. Reached wide batch/try_put remain exact normalized
381/95-instruction streams, scalar batch remains exact, and object code grows
12 bytes. The six-stage native gate passes in 2.71 seconds guarded.

The unchanged controls pass in 3.15 seconds guarded: 1.597 seconds account
construction and 1.372 execution, four 96-owner positives, 20 exact negative
diagnostics and two byte-identical 32-row ledgers equal to the prior ledger.
The single fixed panel exits zero in 55.02 seconds and retains all 1152 rows,
stable images and equal non-time fields. Median ns per complete operation,
cohort zero / one, with peers from candidate images, are:

| Width | Buckets | Operation | Ordinary control | Private capture floor | Rust | C++ |
|---|---:|---|---:|---:|---:|---:|
| wide256 | 64 | replace | 23.697/23.737 | 23.764/23.865 | 22.043/22.112 | 18.891/18.930 |
| wide256 | 4096 | replace | 28.254/28.189 | 28.765/29.337 | 27.062/27.061 | 23.284/23.332 |
| wide256 | 64 | churn | 32.617/32.319 | 28.238/28.605 | 27.135/27.092 | 49.354/49.744 |
| wide256 | 4096 | churn | 48.154/47.887 | 44.568/45.209 | 31.234/31.217 | 59.711/59.475 |

Only wide 64-bucket churn has separated gains in both cohorts; the other 14
flat cohort comparisons overlap, with no separated loss. Wide 4096-bucket
replacement medians worsen by 1.809 / 4.073 percent; scalar 4096-bucket replacement
by 7.251 / 5.441 percent. Scalar 4096-bucket churn has overlapping median changes
of +12.629 / -4.387 percent and paired-ratio spread 17.797 percent, exceeding the
registered 10-percent limit. Minimum interval is 2.327 ms; maximum unchanged-peer
drift is 5.905 percent (Rust/C++ 4.754 percent), within its separate limit.
The full criterion fails; no adoption or retry follows. All adverse observations,
criteria, exact patch/replay pins, private-capture write-order premises, direct
controls/ledgers, raw cohorts and native excerpts remain under
`tag-directed-private-capture` in the same archive. This floor does not prove a
general production swap/consume fusion.

#### Borrowed-tag source exposure: native stop

A separate ordinary-source trial borrows the indexed slot and matches its tag,
then performs the original swap and complete consuming match in every arm. It
retains all inner outcomes and changes no rule, bound, interface or compiler.
Both the complete mutation witness and maintained capacity-three program emit
successfully with control and candidate CLIs. All 15 construction/emission/native
stages exit zero in 17.14 seconds guarded, including a 9.364-second candidate CLI
build. Source admission therefore succeeds. Empty/Deleted avoid old key/payload
capture, but wide exchange expands from 87 to 142 instructions, frame 64 to 160
bytes and stack accesses 12 to 27; seven offered words spill 56 bytes. This fails
the registered no-frame-growth/no-added-spill native criterion. Wide batch keeps
381 instructions/102 stack accesses with literal-address relocations, and try_put
keeps its exact 95-instruction normalized body; scalar batch remains 197/16 but
its inlined tag order changes. No raw-body identity is inferred from equal counts.
Correctness and timing are not run. Original source is restored byte-exact;
source patch, accepted emissions/statuses, compiler/source pins, restoration hash
and compact native excerpts remain under `borrowed-tag-source`. No source-only
policy or performance gain is selected.

#### Late Pair-field result construction: native stop

A separate native-only floor changes just the final Filled result transfer in
original production raw LLVM: replace the private 264-byte Pair copy with an
old-key store and a 256-byte Record copy from existing private snapshots. All
eager captures, swap, cell commit and intermediate/final initializers remain;
result writes still follow commit with the ordinary overlap contract. This is
not the earlier result-before-commit/noalias ordering floor. No source, compiler,
attribute, mask or extra optimization pass changes.

All six construction/link/disassembly stages exit zero in 2.57 seconds guarded.
The complete WF object bytes are identical in both arms; the linked disassemblies
differ only in their filename header. Scalar/wide batch remain 197/381
instructions, wide `try_put` 95 and exchange 87 with a 64-byte frame. Exchange
still uses sixteen unscaled vector loads/stores across the old Pair plus one
scalar access, so the requested paired-field transfer never appears. Executable
hashes differ for an unattributed non-code reason; object equality is not an
executable-byte identity claim. The native criterion fails, and runtime
correctness, allocator checks and timing are not run. The pre-build criterion hash,
its identified post-build spelling text, the one-transfer patch, raw
replay inputs, direct statuses and reached excerpts remain under
`late-pair-fields` in the same archive. Exact pre-build criterion bytes were not
available at retention; current text is not claimed byte-identical to that
preregistration. Object identity independently establishes this negative result.
No production mechanism is selected.

#### First-probe peel: partial native mechanism, unqualified peer panel

An ordinary-source trial starts `try_put` with one `hash_map_action` at the home
bucket, returns through the unchanged `hash_map_exchange` on an equal hit,
records the home bucket and stops on Stop, and continues the original bounded
scan after Deleted or Miss. It retains the
conditional power-of-two mask with modulo fallback, ownership, tombstones and
all public outcomes. The first candidate CLI built but source admission stopped
at a `TYPE-6` collision between the new first-advance `next` binding and the
existing loop binding. A mechanical rename to `first_next` admitted both the
mutation witness and maintained capacity-three program; the failed attempt and
repair are retained separately. Production library bytes were restored.

The registered native preflight is **partial/fail**: a first equal bucket still
sets the available-slot sentinel, even though the remaining-count setup and
entry branch into the shared loop disappear. Reached first-hit work shortens
from 21 to 20 instructions in the scalar batch and 18 to 16 in wide `try_put`;
there is no new hot call, spill or frame growth. Static cold code grows. A
**separate**, preregistered performance question therefore tested the unchanged
source-mask control and peeled candidate, without reclassifying the native gate.
Both arms pass 30 direct check stages: four 96-owner positives in total,
20 exact fault refusals, identical allocation ledgers, and maintained native
program execution. The fixed 4,194,304-operation C0/B0/B1/C1 panel exits zero
and retains all 1,152 rows, nine seeds per cell and cohort, with matching
non-time fields and unchanged image pins.

| Width | Buckets | Operation | Source-mask WF median ns/op, cohorts 0/1 | Peeled WF median ns/op, cohorts 0/1 | Peeled vs Rust, cohorts 0/1 | Peeled vs C++, cohorts 0/1 |
|---|---:|---|---:|---:|---|---|
| word8 | 64 | replace | 2.803 / 2.694 | 3.174 / 3.195 | loss / loss | overlap / overlap |
| word8 | 64 | churn | 6.997 / 7.131 | 6.644 / 6.542 | win / win | win / win |
| word8 | 4096 | replace | 3.694 / 3.739 | 3.727 / 3.937 | loss / loss | overlap / overlap |
| word8 | 4096 | churn | 11.766 / 12.009 | 12.573 / 11.290 | loss / overlap | win / win |
| wide256 | 64 | replace | 20.778 / 21.135 | 21.041 / 20.801 | win / win | loss / loss |
| wide256 | 64 | churn | 28.011 / 27.779 | 27.753 / 28.074 | overlap / overlap | win / win |
| wide256 | 4096 | replace | 25.476 / 25.995 | 25.811 / 25.745 | win / win | loss / loss |
| wide256 | 4096 | churn | 39.419 / 39.280 | 43.034 / 42.546 | loss / loss | win / win |

The peeled image wins the strict median-slower Rust/C++ full-range comparison
in six of eight cells in both cohorts; scalar replacement fails both sizes.
There is no separated before/after gain, and wide 4096-bucket churn has a
separated loss in cohort one. Minimum real interval is 9.315 ms, but the
maximum paired WF cohort-ratio spread is 13.656 percent (scalar 4096 churn)
and unchanged-chain median drift is 17.193 percent in cohort-one scalar 4096
churn, each over the registered 10-percent limit. The native-peer drift
maximum is 3.727 percent. The campaign and complete peer goal fail; the
candidate is not adopted. A read-only linked audit found the scalar chain
body's 928 machine-code bytes equal across these images while its address
moves by 420 bytes (modulo 64: 28 to 0). That placement observation does not
establish the cause of the drift. The source attempts, criterion, direct
statuses, exact ledgers, native excerpts, all raw rows and reduction are
retained under `first-probe-peel` in the same archive and indexed by
`mutation-api-evidence.json`.

#### Linked chain placement A/A: no qualified explanation

A separate layout-only A/A diagnostic relinks the same frozen objects with
one 164-byte helper moved ahead of the scalar chain batch. The linked chain
body remains 928 identical machine-code bytes and 232 instructions with a
112-byte frame; its address moves from `0x1000a50dc` to `0x1000a5180`
(modulo 64: 28 to 0). Fifteen function addresses move and all 2,917
disassembled symbol occurrences retain their instructions modulo branch
relocations. The explicit-order control is not globally the prior image:
Rust/C++ placement also differs from that older image, although their code and
addresses match between these A/A arms. The test therefore does not isolate a
cache or predictor cause for the preceding unchanged-chain drift.

All 29 check exits, including 26 broad checks and exact ledger, have the
registered results. The fixed 4M-operation C0/B0/B1/C1 panel exits zero and
retains 1,152 rows. Scalar 4096-bucket chain churn full ranges overlap in both
cohorts: median 7.687 to 7.740 ns/op in cohort zero and 8.363 to 7.827 in
cohort one. Its paired-ratio spread is 7.587 percent and native-peer maximum
drift 5.372 percent, but full-panel flat scalar 4096 churn spread is 19.092
percent, beyond the 10-percent limit. Minimum interval is 9.309 ms. There is
no qualified layout response or established cause of the earlier drift; no
source choice changes. Criteria, exact order files, native comparison, checks,
raw rows and full peer ranges are retained under `chain-layout-aa` in the
same archive and indexed by `mutation-api-evidence.json`.

#### Filled-first source order: native stop

A one-hunk source trial moves the `HashMapFilled` match arm ahead of Vacant and
Deleted in `hash_map_action`, against the frozen source-mask control. The
registered native criterion requires one Filled tag test before the key load
in the reached scalar replacement path, with no added hot call or exchange cost.
The candidate compiler admits the mutation witness and maintained
capacity-three program; production library bytes are restored exactly.
An initial candidate object used `-O2` against a `-O3` control. Its apparent
hot call and 137-versus-205 instruction comparison are invalid setup evidence,
not an arm-order effect. With the exact matched `-O3` commands, LLVM inlines
scalar `try_put` into the batch in both arms (cost 305, threshold 525).
Both linked scalar batches have 205 identical machine instruction words, a
112-byte frame and no calls; their two tag tests remain. Diagnostic-only
inliner remarks leave each object byte-identical to its ordinary build.
The native criterion fails, so no correctness/accounting or timing campaign
follows and no source change is selected. Criterion, patch, commands, pins,
native excerpts and the invalid setup's artifact hashes are retained under
`filled-first-order` in the same archive, indexed by `mutation-api-evidence.json`.

#### Matched C mutation attribution: one slower cell, seven unresolved

An ordinary C flat map was added as a fifth participant to the frozen source-mask
mutation image. It uses the same bounded mask/modulo probe, tombstone/refusal
policy, complete returned old/offered pair, full-word digest and cleanup for
concrete integer word and 32-word payloads. This is a same-contract attribution
control, not a performance ceiling or proof for generic owning values. Rust
and C++ remain separate native target peers; source-mask is not production-selected.
The C source uses ordinary `-O3`, without forced inlining or alias promises.
The final image passes 50 expected stages, two 120-case full-content images,
36 exact faults and a 40-row allocation ledger: the prior 32 rows agree and
C contributes eight exact one-allocation/one-release rows. Reached scalar WF/C
batches have 205/209 instructions and 112/48-byte frames; wide WF batch
calls `try_put`/exchange while C naturally inlines its wider graph. These
whole-body counts do not predict executed-path cost.

The preregistered two-process, five-way panel retains 800 rows: ten seeds at
each cell/cohort, every implementation in each order position twice, and
4,194,304 operations per sample. Every full-content outcome agrees, images
remain fixed, all intervals are at least 9.32 ms, maximum C/WF ratio spread is
9.535%, and maximum native cohort spread is 1.018%. Median ns/op follows;
each entry is cohort 0 / cohort 1. Full ranges and all peer directions are
retained in the archive.

| Width | Buckets | Operation | WF flat | C matched | WF chain | Rust | C++ | C/WF qualified |
|---|---:|---|---:|---:|---:|---:|---:|---|
| word8 | 64 | replace | 2.536 / 2.445 | 2.697 / 2.849 | 2.269 / 2.255 | 2.498 / 2.483 | 2.750 / 2.737 | unresolved |
| word8 | 64 | churn | 7.480 / 7.442 | 7.763 / 8.084 | 5.403 / 5.310 | 16.113 / 16.046 | 27.424 / 27.628 | unresolved |
| word8 | 4096 | replace | 3.246 / 3.320 | 3.663 / 3.590 | 2.757 / 2.653 | 2.593 / 2.593 | 4.027 / 4.059 | unresolved |
| word8 | 4096 | churn | 14.364 / 15.428 | 15.281 / 15.894 | 6.961 / 7.009 | 10.470 / 10.481 | 29.690 / 29.670 | unresolved |
| wide256 | 64 | replace | 20.894 / 20.852 | 21.466 / 21.450 | 31.780 / 31.631 | 22.175 / 22.169 | 18.960 / 18.884 | unresolved |
| wide256 | 64 | churn | 27.791 / 27.779 | 34.054 / 34.077 | 41.019 / 40.684 | 27.118 / 27.095 | 49.396 / 49.583 | higher |
| wide256 | 4096 | replace | 25.466 / 25.359 | 23.368 / 23.482 | 32.769 / 33.741 | 27.070 / 27.165 | 23.266 / 23.503 | unresolved |
| wide256 | 4096 | churn | 39.101 / 38.800 | 40.836 / 41.027 | 48.217 / 48.085 | 31.538 / 31.623 | 60.012 / 59.559 | unresolved |

C has no qualified gain over WF: seven cells have overlapping ranges. At
wide256/64 churn C is separated slower in both cohorts (34.054/34.077
versus WF 27.791/27.779 ns/op). At wide256/4096 replacement C medians
are lower, but cohort-zero C range 23.191–32.734 overlaps WF 25.136–26.730;
the outlier remains and the cell is unresolved. C, WF, Rust and C++
directions are assessed separately; neither C nor WF solves the whole API
target. Criterion, adapter/driver, checks, native excerpts, ledgers, raw rows,
per-peer ranges and reduction are under `matched-c-mutation` in the same
archive. Archived role placeholders carry original executed hashes separately.

#### Scalar byte-store LLVM floor: native gain without runtime selection

A separate raw-LLVM diagnostic replaces all 26 ordinary scalar `store i1`
operations in the frozen source-mask module with zero-extension and `store i8`
at the same destination. It leaves SSA, loads, aggregate layouts, ABI and
control flow unchanged. This one-module floor is not a compiler implementation
or a general qualification of every storage path. Linked flat scalar batch
shrinks 205→197 instructions and its frame 112→96 bytes, with no hot calls;
chain scalar grows 232→234 and chain preparation also changes, while wide
mutation instruction counts, frames, calls and stack-access streams stay unchanged. Both arms pass 36 stages, 96 full-owner cases
per timed/account image, 20 exact faults, identical 32-row ledgers and the
maintained capacity-zero/three program.

The fixed 1,048,576-operation control0/candidate0/candidate1/control1 panel
retains all 1,152 rows, nine seeds per group. Non-time fields and complete
outcomes agree, images stay fixed, minimum interval is 2.326 ms and maximum
WF before/after ratio spread is 6.823%. The table gives control→candidate
median ns/op and full-range direction in cohorts 0 / 1. “Peer” columns give
the candidate versus Rust and C++ separately in cohorts 0 / 1; full peer
ranges and controls are archived.

| WF variant | Width | Buckets | Operation | C0 control→candidate | C1 control→candidate | C0/C1 direction | Rust peer C0/C1 | C++ peer C0/C1 |
|---|---|---:|---|---:|---:|---|---|---|
| flat | word8 | 64 | replace | 2.767→2.958 | 2.742→2.963 | overlap / overlap | loss / loss | overlap / overlap |
| flat | word8 | 64 | churn | 6.898→6.984 | 7.014→7.262 | overlap / overlap | win / win | win / win |
| flat | word8 | 4096 | replace | 3.748→3.781 | 3.997→3.825 | overlap / overlap | loss / loss | overlap / win |
| flat | word8 | 4096 | churn | 12.263→12.141 | 13.180→13.721 | overlap / overlap | loss / loss | win / win |
| flat | wide256 | 64 | replace | 20.741→20.780 | 21.197→20.764 | overlap / overlap | win / win | loss / loss |
| flat | wide256 | 64 | churn | 28.332→27.458 | 28.092→27.909 | overlap / overlap | overlap / overlap | win / win |
| flat | wide256 | 4096 | replace | 25.458→25.520 | 25.448→25.588 | overlap / overlap | win / win | loss / loss |
| flat | wide256 | 4096 | churn | 40.131→43.148 | 39.641→43.562 | overlap / loss | loss / loss | win / win |
| chain | word8 | 64 | replace | 2.332→2.276 | 2.259→2.272 | overlap / overlap | win / win | win / win |
| chain | word8 | 64 | churn | 5.424→5.522 | 5.504→5.601 | overlap / overlap | win / win | win / win |
| chain | word8 | 4096 | replace | 3.080→3.005 | 3.012→2.750 | overlap / overlap | loss / loss | win / win |
| chain | word8 | 4096 | churn | 7.986→7.055 | 7.747→7.221 | overlap / overlap | win / win | win / win |
| chain | wide256 | 64 | replace | 31.658→31.535 | 31.595→31.759 | overlap / overlap | loss / loss | loss / loss |
| chain | wide256 | 64 | churn | 40.752→40.879 | 40.890→40.996 | overlap / overlap | loss / loss | win / win |
| chain | wide256 | 4096 | replace | 33.363→33.321 | 33.298→33.182 | overlap / overlap | loss / loss | loss / loss |
| chain | wide256 | 4096 | churn | 47.581→48.202 | 48.018→47.307 | overlap / overlap | loss / loss | win / win |

No flat scalar cell has a separated gain in both cohorts. Flat wide256/4096
churn is separated slower in cohort one (43.562 versus 39.641 ns/op), with
cohort zero overlapping. C++ wide256/64 replacement drifts 12.427% between
unchanged peer arms in cohort one, beyond the registered 10% limit. No
sample is removed; the criterion fails and no lowering change is selected.
The 26-store patch, maintained-program patch, native deltas, direct checks,
four raw cohort CSVs, peer ranges and reduction are under `byte-store-floor`
in the same archive. This storage-only floor is distinct from any future
coherent compiler prototype.

#### Coherent byte-memory prototype: native stop

A general emitter prototype makes scalar Bool/tag loads and stores use
coherent one-byte memory access; aggregate copies, one-bit SSA, calls and
layouts remain unchanged. Both CLIs compile the
same ordinary source and production Map; control LLVM matches the baseline.
Flat scalar batch falls 197→191 instructions and frame 112→96 bytes, with no
new call/spill. Its reason scratch disappears, the same mechanism as the
separate source-mask store-only floor; source policy prevents subtraction.
Wide mutation counts, frames, calls and stack streams stay unchanged; chain
scalar grows 232→234. Scalar preparation has fewer instructions outside the timer. No
additional timed transfer mechanism appears, so no timing or adoption follows.

The source/linked regression and repaired mixed-copy test pass: an old
mixed i1-store/i8-load fixture retains an O3 alloca, while coherent access
promotes it; plain Bool remains promotable. The initial fixture lacked a
main stub and failed linking; repair passes both wrong-result controls.
The native observation proceeded with the preregistered worker/context
prerequisite unmet, so it is not a completed preflight. Broader transport,
full backend, cross-target and performance checks remain unrun. After
preserving the patch, compiler sources were restored to the published baseline. Criterion, patch/base hashes, minimal
counterexample, statuses and native deltas are under `coherent-byte-native`.
The inspected CLI predates formatting-only edits without a pre-format
source snapshot, so its exact source identity is unproved.

[`entry-forward-evidence.tar.gz`](entry-forward-evidence.tar.gz) serves this
section's reader with scripts, criteria, raw API/whole-Map/long-hit records,
commands, metadata, statuses, source patch and native excerpts, without binaries.
It is retained with this attribution trial and replaced or retired when the
trial is superseded. The incompatible broad CSV schema stays in that one
archive. `mutation-api-evidence.json` under `ordinary_incoming_backing` pins the
images, archive, final source and benchmark identity boundary. Native commands
use the symbolic `LLVM_OBJDUMP` instrument (LLVM22.1.8); the archive labels
normalized text and preserves the original executed byte hashes separately.

## Isolated public reserve: entry headroom baseline

This ordinary-source baseline times reserve separately from preparation, complete
owner inspection and cleanup. It uses frozen production CLI
`c12b2b2a6c9694fd2bc1b7be0004d582ae9628951422def849f4cf6a3ac02f77`,
Apple Clang 21 O3 and Rust 1.98.1 opt-level 3, separate translation units without
LTO. Neither the unselected byte-memory compiler nor a hand-edited IR body is used.
The historical reserve trace remains valid as defined: it includes setup, queries
and cleanup, with one reserve per trace. Its timings do not isolate this API.

Each fresh owner starts with application entry floor S=64 or 4096 and S/2 live
entries. Growth requests T=2S; noop requests T=S. WF accepts physical slots, so
its wrapper computes B(T)=floor(4(T−1)/3)+1; Rust receives additional T−len and
C++ receives total T. The conversion is inside the timer. Untimed checks insert
through the Tth ordinary put without further backing growth and consume every
key and all 32 words of each wide value. Passing raw T slots to WF fails this
stronger check at insertion 97 of 128, despite meeting the raw capacity floor.
This is a matched application guarantee, not identical physical reserve semantics.

Observed growth geometry is WF 85→170 / 5461→10922 physical slots, Rust
112→224 / 7168→14336 usable entries (128→256 / 8192→16384 backing slots), and
C++ 64→128 / 4096→8192 buckets with max_load_factor 1. Each grow requests one
new backing and releases one old backing; noop does neither. New backing bytes
for S=64 / 4096 are WF scalar 4,080 / 262,128 and wide 46,240 / 2,970,784;
Rust scalar 4,360 / 278,536 and wide 67,848 / 4,341,768; C++ bucket arrays
1,024 / 65,536 for either width. C++ separately retains 32/280-byte scalar/wide
nodes. These exact byte checks are grounded in the current arm64 hashbrown NEON
8-byte control group and libc++ node layout, not portable layout promises.
The 96 phase-ledger rows finish at zero live bytes; fixed batch peaks pass 64 MiB.

The timed and account correctness images each pass 288 reserve cases; together
they pass 21 independent fault controls. Existing lookup and whole-trace checks also pass in both images.
Build attempts 1–9 retain formation, binder and proof failures, including an accidental edit
to old WF binders that was restored. Build 10 succeeds. The first accounted
check fails an incorrect Rust +16 control-byte expectation; the pinned aarch64
implementation requires +8, and the corrected exact assertion passes. Measured
source is frozen; the only subsequent adapter edit removes Makefile trailing
whitespace from a diagnostic echo.

The preregistered two cohorts each retain 22,032 rows: 432 real samples, their
snapshot controls and every short batch. Nine samples balance each peer position
three times. S=64 uses 256 contexts and 16,384 fresh grow calls per sample;
S=4096 uses 8 contexts and 256 grow calls. Noop uses 1,048,576 calls. Setup and
cleanup stay outside each interval; raw batch intervals are summed without
subtracting the snapshot control. Minimum real batch durations are 22.542/23.041
µs, not millisecond batches. RAW quantum is 41 ns, and all batch overhead checks
pass against the 1,024 retained empty-clock intervals per cohort.

Both processes return **1 after collecting all rows**, because 19/22 real
aggregate samples fall below the fixed 1 ms minimum (minimum 0.841/0.842 ms).
They affect native-default Rust small scalar/wide noops and C++ large scalar
growth. Large default wide growth also fails peer stability: C++ cohort medians
differ by 23.06%. The two planned cohorts were retained without retry.

Medians below are ns/call, cohort 0 / cohort 1. The linked JSON retains every
peer's complete ranges and minimum duration. “Pass” means WF's full range lies
below the slower peer's full range in both cohorts, with duration and ≤10%
cohort-median spread satisfied; it does not mean faster than both peers.

| Hash series | Value B | S | Operation | WF | Rust | C++ | Qualified target |
|---|---:|---:|---|---:|---:|---:|---|
| Aligned | 8 | 64 | grow | 243.513 / 238.161 | 205.467 / 196.444 | 124.825 / 127.418 | WF slower than both |
| Aligned | 8 | 64 | noop | 1.258 / 1.261 | 0.959 / 0.987 | 2.807 / 2.863 | Pass |
| Aligned | 8 | 4096 | grow | 18866.867 / 18502.758 | 9637.043 / 9644.211 | 4899.574 / 4861.340 | WF slower than both |
| Aligned | 8 | 4096 | noop | 1.358 / 1.319 | 1.038 / 1.009 | 2.980 / 2.871 | Pass |
| Aligned | 256 | 64 | grow | 2017.390 / 2023.981 | 866.696 / 877.838 | 230.642 / 236.417 | WF slower than both |
| Aligned | 256 | 64 | noop | 1.258 / 1.258 | 0.959 / 0.959 | 2.811 / 2.813 | Pass |
| Aligned | 256 | 4096 | grow | 121665.691 / 121511.695 | 64215.008 / 62519.852 | 12677.734 / 12746.586 | WF slower than both |
| Aligned | 256 | 4096 | noop | 1.319 / 1.319 | 1.009 / 1.009 | 2.868 / 2.880 | Pass |
| Default | 8 | 64 | grow | 240.583 / 237.600 | 366.027 / 364.199 | 98.071 / 94.394 | Pass |
| Default | 8 | 64 | noop | 1.289 / 1.258 | 0.948 / 0.948 | 2.270 / 2.187 | Duration unresolved |
| Default | 8 | 4096 | grow | 19773.117 / 19497.230 | 20193.191 / 20154.949 | 3307.125 / 3317.879 | Duration unresolved |
| Default | 8 | 4096 | noop | 1.319 / 1.319 | 1.010 / 1.007 | 2.249 / 2.253 | Pass |
| Default | 256 | 64 | grow | 2022.474 / 2029.617 | 947.029 / 939.847 | 200.246 / 206.604 | WF slower than both |
| Default | 256 | 64 | noop | 1.260 / 1.255 | 0.954 / 0.951 | 2.813 / 2.807 | Duration unresolved |
| Default | 256 | 4096 | grow | 121688.797 / 121689.953 | 83107.273 / 83939.129 | 15307.945 / 12439.297 | Spread unresolved |
| Default | 256 | 4096 | noop | 1.319 / 1.319 | 1.010 / 1.009 | 2.869 / 2.873 | Pass |

Seven of 16 cells meet that target: six noops and default small scalar growth.
All aligned growth cells are separated slower than both peers. Aligned large
wide growth costs 121.666/121.512 µs versus Rust 64.215/62.520 and C++
12.678/12.747 µs. Noop is consistently slower than Rust and faster than C++;
default small scalar growth is faster than Rust but slower than C++. Default
large scalar growth overlaps Rust and is slower than C++, with its duration
failure retained. This single baseline panel does not qualify the entire API,
select an optimization or establish repeated performance.

A separately retained source-only rebuild running-index candidate builds and
emits these concrete adapters, but the maintained generic program fails INV-1
at the new loop backedge. Normal-hash probe work falls 10→8 instructions;
forced-collision work rises scalar 6→8 and wide 7→8. Scalar/wide frames remain
96/688 bytes. Parallel admission, runtime checks and timing were not run;
production library bytes were restored. This rejected source candidate does not
contribute to the baseline table; the separate initialization diagnostic below
retains its own matched control.

[`reserve-api-evidence.json`](reserve-api-evidence.json) pins sources, image,
archive and per-peer reductions. [`reserve-api-evidence.tar.gz`](reserve-api-evidence.tar.gz)
contains all 44,064 raw rows, both direct process/guard results, clock intervals,
pre-build criterion, exact ledgers, fault logs, source snapshots/patch, portable
replay commands and compact allocator/rebuild native evidence. It contains no
compiler binaries or full disassembly dumps. Original versus path-normalized
record hashes are distinguished. These files serve this baseline's reader and
are replaced or retired with the experiment; research remains outside the gate.

### Fresh-Vacant initialization floor: rejected

A raw LLVM diagnostic removes only four aggregate zero stores in
`hash_map_extend`, keeping valid Vacant tags, all exchange/result initialization,
allocation geometry and ownership operations. Ordinary single-pass O3 removes
16/264 inactive payload bytes per scalar/wide bucket without new calls, spills
or frame growth. Tag-loop unrolling changes 22 bodies and grows the object by
1,008 bytes. This bounded diagnostic does not implement a compiler rule or select
a new inactive-byte initialization policy.

All 64 build/check stages match their expected outcomes. Each of the four timed
and account images passes 288 reserve cases; each arm passes 21 fault controls
across its timed/account pair. Existing lookup/whole-trace checks pass and both
96-row ledgers equal the baseline. The fixed control0, candidate0, candidate1,
control1 panel retains 88,128 rows. All four processes return 1 after completing
the panel because the unchanged fast-cell instrument failures remain; the guard
returns 1 after 31.38 s. No retries or discarded cells follow.

The two primary aligned-hash wide growth cells pass their duration, overhead,
cohort-spread and peer-drift gates. Values below are median [minimum, maximum]
µs per reserve, cohort 0 / cohort 1; complete Rust/C++ ranges and every other
cell remain in the prefixed archive reduction.

| Initial entry floor | Control WF | Initialization floor WF | Paired verdict |
|---:|---|---|---|
| 64 | 2.041 [2.023, 2.241] / 2.035 [2.026, 2.088] | 1.924 [1.872, 2.441] / 1.930 [1.866, 2.204] | Overlap both cohorts |
| 4096 | 121.422 [120.981, 123.642] / 121.337 [120.993, 124.682] | 132.814 [131.063, 156.213] / 133.598 [131.991, 163.260] | Slower both cohorts |

Large wide growth loses 9.38/10.11% by median; both primary candidate cells
remain separated slower than Rust and C++ individually. Paired ratio spreads
are 0.625/0.661%, and maximum unchanged-peer drift is 4.876%. Default small
scalar growth improves in both cohorts, but does not answer the primary wide
question. All noop before/after ranges overlap. Thus the primary criterion fails;
no pure store-cycle, cache or allocator cause is inferred from the loss.

`vacant-init-floor/` inside the existing reserve archive retains this diagnostic's
criterion, exact four-line patch, historical build/check scripts, native excerpts,
64-stage qualification, all four raw panels and adverse reductions. `component-index.json`
distinguishes it from the ordinary baseline and pins every payload; all 145 prior
archive entries remain byte-for-byte unchanged. The matched diagnostic control
has identical code sections to the earlier baseline but different UUID/signature
metadata; only the new matched pair is compared. The scripts require regenerated
timed LLVM, omitted baseline objects/binaries,
resolved tool/path roles and the recorded scratch hierarchy; the component's
replay.txt lists those dependencies. The published package has not been replayed.
No production source changes or general zero-omission mechanism are selected.

### Ordinary-source pending owners: native rejection and partial timing benefit

Two source trials keep the public reserve contract, allocation geometry, complete
bucket initialization, hash policy and owner outcomes. They are separate from the
raw initialization floor above; neither changes the compiler or specification.
Production library bytes are restored after each candidate compiler is frozen.

The backing-pending trial keeps the pending Filled owner in the old backing and
uses a borrowed tag/hash before swapping it into an available new bucket.
Maintained normal/parallel admission and all eight build/native stages pass.
However, wide native swap performs three 272-byte libc transfers, including a
full temporary and inactive destination writeback to the old backing. The smaller
400-byte frame does not satisfy the no-replacement-copy/call criterion. This
trial stops before runtime/account checks or timing.

Self-tail v1's loop-only completion fails FN-1 as specified: a loop does not
satisfy the explicit-return requirement ([active specification](../../../../spec/kernel-spec.md)).
V2 performs one bounded scan per invocation and uses ordinary direct self-tail
returns for displacement or a complete scan. Both lower to IR jumps. Optimized
native code retains the full-scan backedges and removes the displaced-Filled
edge using the available-slot tag check; no recursive call remains. Wide
old-owner capture is only partially removed:
256 bytes still stage through the caller's argument area. The original strict
native gate therefore remains **partial/fail**. Other transfers do disappear:
the helper no longer loads 264 inactive destination bytes or clears the 272-byte
Inserted result. Simultaneous caller/helper frames fall 752→336 bytes, without
old-backing writeback. Scalar old-pair staging disappears. No initialization-only
floor or special inline/alias attribute is composed with this source change.

A separately preregistered supplemental question tests those partial savings.
Each of the four timed/account images passes 288 reserve cases, with 21 fault
controls per arm across its image pair; both 96-row ledgers equal the baseline.
Maintained normal and parallel executions pass, each with 36 observed allocations.
The initial observer link fails from a missing include path; that failure and the
corrected unfinished-stage executions remain in the record. No expectation is
weakened. The fixed paired panel retains all 88,128 rows and four final instrument
exit-1 statuses (31.73 s outer guard); all frozen inputs remain unchanged.

Aligned wide growth is the primary question. Values are WF median [minimum,
maximum] µs per reserve, cohort 0 / cohort 1. Both native peers and all other
cells remain in the complete reduction.

| Initial entry floor | Matched control | Self-tail v2 | Supplemental verdict |
|---:|---|---|---|
| 64 | 2.058 [2.046, 2.393] / 2.055 [2.051, 2.113] | 1.708 [1.702, 1.788] / 1.734 [1.697, 1.835] | Qualified gain both cohorts |
| 4096 | 125.554 [122.569, 134.222] / 122.952 [122.286, 139.159] | 102.369 [101.134, 106.955] / 102.544 [101.598, 107.171] | Raw gain; peer stability unresolved |

S64 has separated gains with all gates satisfied. S4096 also has separated raw
gains, but control C++ medians differ by 15.49% across cohorts, exceeding the
unchanged 10% stability limit. The full supplemental criterion fails, and both
candidate primary cells remain separated slower than Rust and C++ individually.
Default wide growth gains in both sizes with its gates satisfied. No separated
WF before/after loss appears; other overlaps and fast-cell duration failures
remain visible. This is a measured partial benefit, not overall qualification,
isolated copy-cycle attribution or production adoption.

The existing archive adds `backing-pending/` and `selftail-pending/`, retaining
source patches, both failed attempts, exact criteria, native excerpts, checks and
complete supplemental samples. Scripts are historical command records requiring
omitted compiler/generated-object dependencies and tool/path reconstruction, not
a standalone replay; no published-package replay is claimed. All prior 222 entry
byte sequences remain: 221 at their original names and the prior outer index at
`component-index-before-owner-source-trials.json`. Only the outer component index
is extended. Original/normalized hashes are explicit. The separate initialization
interaction below has its own matched control and criterion.

### Self-tail and initialization interaction: overall criterion failed

This raw LLVM trial starts from self-tail v2 and deletes only its four fresh-Vacant
aggregate zero stores. Explicit tags, complete owner operations and all four
migration-helper instruction streams remain unchanged; wide caller argument
staging still copies 256 bytes. Fresh bucket writes fall from 20/268 bytes to
4 bytes for scalar/wide slots. Ordinary O3 also unrolls tag writes and changes
22 of 140 bodies; frames, call and stack-access counts stay unchanged. These
collateral changes prevent attribution to removed-store cycles alone.

All 64 build/check stages match expectations: four images each pass 288 reserve
cases, with 21 faults per arm, unchanged lookup/whole-trace checks and identical
96-row ledgers. The fixed paired panel retains all 88,128 rows; every aggregate
matches its batch elapsed-time/call-count sums. All four processes finish their
22,032 rows then exit 1 for the unchanged fast-cell instrument failures; the
outer guard returns 1 after 30.80 s. Input hashes remain fixed. No retry follows.

Primary aligned wide growth, WF median [minimum, maximum] µs per reserve,
cohort 0 / cohort 1:

| Initial entry floor | Self-tail control | Initialization interaction | Verdict |
|---:|---|---|---|
| 64 | 1.719 [1.687, 1.825] / 1.695 [1.691, 1.747] | 1.362 [1.291, 1.542] / 1.357 [1.337, 1.405] | Qualified gain both cohorts |
| 4096 | 103.548 [100.004, 115.007] / 101.813 [100.180, 108.263] | 92.155 [90.410, 116.268] / 90.630 [88.651, 95.951] | Overlap / gain; peer drift unresolved |

Small wide growth improves with all gates satisfied but remains separated slower
than both peers. Large wide growth overlaps in cohort 0 and improves in cohort 1;
paired unchanged-peer drift reaches Rust 15.28% and C++ 21.53%, above the 10%
limit. The Rust candidate-cohort-1 maximum of 352.992 µs is retained, not filtered;
WF/Rust range overlap there does not establish parity. WF remains slower than
C++ in both cohorts. The full primary criterion fails.

Aligned and default small scalar growth also have qualified gains. No separated
WF before/after loss appears, but large-cell instability and the existing fast-cell
duration failures remain. All 16 cells and both peers are retained. The result
neither adopts an initialization policy nor supplies a general inactive-byte
safety proof or an explanation of the earlier initialization floor's regression.
The original self-tail native partial/fail and source campaign limits stand.

`selftail-init-interaction/` in the existing archive retains the exact four-line
patch, criterion, native excerpts, checks, clocks, all raw samples and adverse
reductions. Scripts are historical command records with omitted generated-module,
compiler/object and tool/path dependencies; the published package has not been
replayed. All prior 337 entry byte sequences remain, with the former outer index
at `component-index-before-selftail-init-interaction.json` and the other 336 at
unchanged names. Only the outer index is extended; original/normalized hashes are
separate. No compiler, library or specification change is selected.

### Old-backing owner argument floor: no qualified timing benefit

This raw LLVM floor starts from self-tail v2 with ordinary Vacant initialization,
not the initialization interaction. It changes only the wide ceiling-16384
rebuild: the already selected old-last payload address supplies the unchanged
migration call instead of a private 256-byte argument copy. The exact instance
has a live old backing, a separate fresh destination, a read-only scalar hash
callback and eager callee capture before destination writes. The retained
address audit establishes that bounded premise; it is not a generic lifetime,
alias or owning-payload proof, and adds no ABI attributes or source permissions.

Native removal succeeds: eight caller LDP/STP pairs disappear, the frame falls
336→64 bytes and stack accesses 18→8, without a substitute copy or added call.
All four migration-helper bodies stay identical. One wide whole-trace body also
changes address/register scheduling (2529→2536 instructions), while its 7424-byte
frame, call and stack-access counts remain unchanged; that collateral is retained,
not mistaken for the isolated reserve path. No compiler implementation follows
from this native observation alone.

All 64 build/check stages match expectations: each of four images passes 288
reserve cases; each arm passes 21 faults across its timed/account pair; existing
lookup/whole-trace checks pass and both 96-row ledgers equal the baseline. The
fixed paired panel preserves 88,128 rows, with every aggregate equal to batch
elapsed-time/call-count sums and unchanged input pins. All four processes return
1 after collecting their complete panels because known fast-cell instrument
failures remain; the outer guard returns 1 after 32.99 s. No retry follows.

Aligned wide growth, WF median [minimum, maximum] µs per reserve, cohort 0 /
cohort 1:

| Initial entry floor | Self-tail control | Owner argument floor | Verdict |
|---:|---|---|---|
| 64 | 1.754 [1.740, 1.796] / 1.737 [1.729, 1.869] | 1.781 [1.606, 1.918] / 1.822 [1.661, 1.892] | Overlap both; stability unresolved |
| 4096 | 102.685 [102.328, 106.737] / 112.186 [102.976, 122.059] | 95.018 [93.187, 116.339] / 102.889 [97.598, 116.811] | Overlap both; stability unresolved |

Neither primary cell demonstrates a timing benefit. Both fail stability: C++
paired drift is 10.47% at S64 cohort 1 and 11.34/24.29% at S4096. Small-wide WF
remains separated slower than both peers; large-wide WF overlaps Rust and is
separated slower than C++ in both cohorts. Overlap does not establish parity.
All 16 cells and both peers remain in the reduction: there is no qualified gain
or loss, and the other adverse samples, duration failures and maximum 39.89%
paired peer drift are retained. Fewer transfers and lower large-wide medians do
not meet the preregistered criterion. No generic implementation or adoption is
selected. The current [storage plan](../../../../compiler/src/backend/storage.rs)
keeps source loads and ordinary projections as snapshots. This result does not
justify relaxing that boundary: a general forwarding change would need both
qualified performance evidence and explicit lifetime/alias conditions beyond
the frozen instance audit.

The remaining capture is specifically a consumed `take_back` enum's Filled
payload projection, not an ordinary source load: `ProjectVariant` obtains the
field address and `load_place_result` copies it into a separate 256-byte slot
([emitter](../../../../compiler/src/backend/emitter/places.rs)). The storage
plan's incoming-backing selection covers eligible owned function parameters,
while its ordinary projection/load rule keeps this local field as a snapshot
([storage plan](../../../../compiler/src/backend/storage.rs)); the existing
projection snapshot and incoming-backing tests cover those current boundaries.
Reopen a generic consumed-projection forwarding rule only with discriminating
performance evidence and a proof that the source backing remains live and
unchanged until a synchronous callee captures the field, including negative
cases for exposure, overlapping writes and deferred reads.

`owner-argument-floor/` in the existing archive retains the bounded address audit,
patch, original/changed function excerpts, whole-trace collateral, 64 checks and
all raw panels. Its manifest distinguishes original and retained normalized
hashes. Scripts remain historical command records requiring omitted compiler,
module/object and tool/path dependencies; no standalone or verified published
replay is claimed. All prior 408 byte sequences remain: the former outer index
is at `component-index-before-owner-argument-floor.json`, and the other 407 keep
their original names. Only the outer index is extended.


### Same-image reserve A/A: peer stability blocks qualification

This instrument links two symbol-renamed copies of the same frozen production
WF object with one Rust and one C++ peer. It balances four participant orders
within each sample and reverses them in the second cohort, with four times the
previous aggregate work. WF code addresses remain distinct; duplicated code,
allocator history and sequential cleanup remain possible influences. This is
an instrument check, not a before/after source optimization comparison.

The object audit preserves sections and relocations after name reversal. The
native audit compares 142 defined functions and verifies 414 direct branch
targets; linked PAGE21/PAGEOFF12 immediate targets still trust ordinary linker
semantics. Both timed/account images pass 384 reserve cases each. All 39 check
stages match expectations, including 36 deliberate negative controls, and both
WF ledger projections plus unchanged peer rows equal the production ledger.
Failed authoring attempts and corrected checks remain in the record.

The fixed panel retains all 299,520 rows. Both cohort processes return 0 after
64.65/65.74 s with unchanged input pins; the reducer returns 1. The outer guard
returns 1 after 133.41 s, recorded as a direct session observation because its
stdout was not saved. All 32 WF A/A full-range comparisons overlap. WF cohort
stability, B/A ratio stability, aggregate duration and batch overhead pass;
both cohorts report a 41 ns clock quantum and 42 ns maximum empty interval.

Two unchanged-peer cohort median spreads exceed the preregistered 10% limit:
Rust wide/default S64 growth reaches 10.87%, and C++ wide/aligned S4096 growth
reaches 62.86%. The all-cell criterion therefore fails despite the limited A/A
successes. This instrument is retired from A/B qualification; no retry,
candidate timing, compiler/library change or performance adoption follows.
Both peers, every cell and all adverse observations remain in the reduction.

`paired-image-aa/` in the existing reserve archive retains the criterion,
source/patch, historical commands, compact audits/checks, complete CSVs and
terminal statuses. Its manifest distinguishes original and normalized hashes;
replay requires omitted compiler/build artifacts, tool/path reconstruction and
the original scratch hierarchy. No standalone or verified published replay is
claimed. All 472 prior member byte sequences remain: the old outer index is
at `component-index-before-paired-image-aa.json`, and the other 471 retain their
names. The extended archive has 624 members, including 151 in this component.


### Matched C reserve: native and correctness evidence only

The C attribution control keeps the ordinary reserve contract, flat layout,
hash/probe policy, aggregate-zero Vacant source initialization and complete
owner handling. Both timed/accounted images pass 384 reserve cases; all 41
check commands match their expectations (five positive, 36 diagnostic-specific
rejections). Its 128-row ledger projects exactly to the 96-row production
ledger with either WF or C, preserving both native peers. Separate positive
source-policy witnesses cover both widths' refusal, tombstone and pressure
paths; those witnesses have no injected policy faults.

Ordinary C optimization fuses fresh backing allocation and initialization to
`calloc`; the indirect symbol table resolves the actual call target. Accounted
C instead calls its allocation wrapper then `bzero`. C's O0 aggregate-zero IR
initializes all 24/272 cell bytes, whereas WF typed initialization writes
20/268 bytes, leaving four padding bytes. This supports a bounded padding and
allocation-fusion hypothesis; it does not isolate the cause. The C wide rebuild
still stages 256 bytes through the stack. All observed normal-hash rebuild paths
hoist division outside the collision scan, so repeated division is not an
observed C advantage.

Static scalar/wide rebuild instruction counts and frames are 87/133 and
48/336 bytes for C, versus 133/205 and 96/688 bytes for the original WF control.
These counts include different cold paths and are not runtime measurements.
The retained native report also compares self-tail v2 and preserves its wide
staging limit. No C performance timing ran: the failed paired A/A instrument
remains unqualified, and these findings establish no reserve speedup or peer
target result.

`matched-c-native/` in the existing archive retains source, commands and checks,
compact native excerpts, padding IR and indirect-symbol evidence. Small check
streams are consolidated with individual original/normalized hashes. Omitted
compiler/build artifacts and tool/path reconstruction remain replay dependencies;
no published-package replay is claimed. All 624 previous member byte sequences
remain, with only the old outer index renamed to
`component-index-before-matched-c-native.json`. The extended archive has 654
members, including 29 in this component.


### Zero-preserving padding discriminator: native fusion succeeds

A four-site raw-IR diagnostic preserves every zero tag/key/value byte and
additionally zeros the four padding bytes in each fresh Vacant cell. Ordinary
O3 then changes all four fresh rebuilds from allocation plus per-slot stores
to `calloc(1, capacity * stride)`, for both 24-byte scalar and 272-byte wide
cells. No malloc-to-calloc edit, owner/alias change or new ABI attribute was
made. This bounded comparison identifies the incomplete all-byte zero
representation as a blocker to fusion in these control rebuilds.

Standalone extend functions still retain per-slot initialization loops. Wide
staging, scalar/wide frames (96/688 bytes) and payload-copy sites remain
unchanged. Inlined construction and whole-trace code also change: whole-object
malloc/calloc relocations move 26/0 → 12/18, while memcpy stays at 20. Thus
native success does not isolate a whole-program runtime cost. No execution,
allocation observer or timing ran, and no production compiler change is
selected. Any later runtime check must correctly observe emergent `calloc` and
disclose whether accounting changes the optimization.

`zero-fusion-native/` retains the criterion, exact patch, compile commands and
statuses, compact bodies for all four rebuilds and extends per side, pins and
whole-module collateral. Raw whole modules, objects and full disassembly are
omitted; historical commands need tool/path reconstruction and have no verified
published replay. All 654 previous member byte sequences remain, with the old
outer index at `component-index-before-zero-fusion-native.json`. The extended
archive has 669 members, including 14 in this component.

### Ordinary-source owner migration: partial integration

The library now uses the previously measured self-tail v2 source shape alone:
private owning key/value parameters hold the pending pair, with complete owners
transferred on every retry. The public API, enum buckets, allocation policy,
probe order, fresh initialization and reverse old-slot consumption are unchanged.
Applying the retained v2 patch to the prior library gives identical executable
source after excluding explanatory `doc` lines. The new explanation also keeps
the zero-capacity, callback-return and postcondition limits of the old one.

This selection is partial progress toward the peer target. The earlier
small aligned-wide reserve gain of about 16–17 percent is qualified; the large
aligned-wide raw gain remains unqualified, and both primary sizes remain behind
both peers. The earlier strict native criterion also failed because argument
staging remains. The same-image A/A failure above is not reclassified as a pass,
and no new timing claim follows from this integration. The rejected exchange-only
control remains rejected: this candidate changes pending-owner representation
and control flow together, so its benefit is not attributed to exchange
inlining alone. Raw argument forwarding and padding fusion are not included.

The maintained HashMap program adds three complete wide must-consume pairs,
colliding from the final bucket of a capacity-three map, a full same-capacity
rehash, and growth to five. Equality changes from never equal to always equal
before migration. Borrowed enumeration checks each serial and all inline words
after both migrations; final consumption checks each owner and its single
release. This combines wide ownership with collision/wrap and hostile equality,
which the earlier fixture covered separately. The allocation observer's exact
expectation rises from 36 to 45 for three new backings and six child owners.
The focused corpus test passes ordinary and parallel lowering, with their
accounted executions; final peer qualification and full-PR validation remain
outstanding.

Scratch perturbations of that maintained fixture each remain admitted: omitting
one stored owner, duplicating its serial, or corrupting its last inline word.
Each yields ordinary status 6 and the observer's fixture-failure status 1 in
both lowering modes, while the valid fixture yields zero with 45 allocations.
All 40 command outcomes match their expectations (eight admissions, sixteen
builds and sixteen executions). The retained integration check record includes
exact mutations and input hashes; these controls test the new observation and
do not alter any maintained verdict. Initial command/fixture authoring failures
are retained separately from the successful final stages.

### Complete constructor zeroing: implementation comparison does not qualify

A generic compiler candidate replaces stored struct/enum aggregate zero stores
with an ordinary memset of the target allocation extent, before the unchanged
tag and field writes. It leaves input captures, destination interference and
the ABI unchanged; nested aggregate copies can still copy their own padding.
This candidate remains experimental: the registered runtime criterion fails,
so the following results do not select it for production adoption.

Both compilers emitted the same current self-tail library and benchmark source.
The frozen control CLI is `bf3eafb4bd5dba320c45654cd3eb80727f5dab3647dadc96978452a4d18a4701`;
the candidate is `b2de2a4059e037b32313f88ae7dc023c284656344de01989e0349b261efd215d`.
The library hash is `9805a2515877f5c1243a008268a3ae8a3b2a4a50e5d9843397aeabbddac27074`.
The ordinary three-peer reserve driver retains its seeds, ownership oracle,
headroom contract and separate default/aligned hash series. The prospectively
fixed longer panel uses 65,536/1,024 growth calls at S64/S4096 and 4,194,304
no-op calls, nine samples, and the sequence control0/candidate0/candidate1/control1.
The historical paired A/A instrument is not reused or requalified.

Native inspection confirms fresh backing allocation becomes `calloc`, and also
finds a second consequence: ordinary LLVM inlines wide owner migration. Both
wide rebuild frames shrink from 336 to 64 bytes, with direct old-to-new payload
copies replacing the 256-byte caller stack snapshot. The timed reserve entry
reaches the ceiling-16,384 rebuild; the standalone migration helper's remaining
staging is not on that path. Empty induction loops remain after calloc, including
a final index used by probe termination. Consequently, this comparison measures
the complete compiler change, not an isolated contribution from zero stores.

Accounting rewrites allocator calls after optimization and disables a second
LLVM middle-end pass for both timed and accounted objects. It therefore observes
the emergent calloc rather than preventing it. All four ordinary/accounted
reserve panels pass 288 cases, and all 96-row ledgers equal the prior original
ledger byte for byte. The candidate observer records 3,216 calloc requests and
181,246,720 payload bytes across the checks; the control records none. The 98
final prerequisite stages include 54 diagnostic-specific negative outcomes,
with lookup and whole-trace checks in both images. The two initial observer
rewrite failures remain in the attempt records.

The one timing campaign retains 336,960 rows and all four terminal statuses.
Control0 exits 1: its maximum empty-clock interval is 2,959 ns, which fails the
unchanged one-percent overhead condition for many growth batches. The other
three processes exit 0. No run is retried or filtered. Independently recomputed
batch sums, receipts, cleanup checksums and fixed work counts agree in all
3,456 sample summaries. All eight growth cells are unqualified; all eight
no-op comparisons have overlapping complete ranges. Large wide growth also
has peer or ratio drift, so removing the clock failure alone would not qualify
the complete result.

Aligned-hash, 256-byte payload growth medians below are microseconds per
operation, shown as cohort0 / cohort1. They are raw observations, not qualified
gains or established peer parity:

| Initial entry floor | Control WF | Candidate WF | Candidate Rust | Candidate C++ |
|---|---:|---:|---:|---:|
| 64 | 1.757 / 1.764 | 1.878 / 1.878 | 1.101 / 1.170 | 0.238 / 0.245 |
| 4096 | 104.807 / 103.266 | 85.180 / 92.393 | 83.403 / 88.260 | 12.634 / 15.234 |

Small aligned-wide medians worsen by 6.5–6.9 percent; large medians improve by
10.5–18.7 percent without satisfying the range/stability conditions. Both remain
behind each peer by these medians. Default-policy results and all other cells,
including adverse observations, stay in the full reduction.

Compiler validation passes nine payload tests, 31 owned-place tests and the
HashMap corpus test with both lowerings and its exact 45-allocation ledger.
The new isolated constructor byte observer also passes with the old compiler:
it checks values and adjacent bounds but is not a discriminator on this host
optimizer. The separate fresh-allocation test does distinguish the old code:
its optimized function retains `malloc(count * 48)` and a per-slot 40-byte
memset, failing both the complete-backing-fill and calloc alternatives. Neither
fixture was weakened after that observation. These are focused results, not
full canonical validation of the experimental compiler.

A subsequent native-only discriminator isolates the residual counting loop.
Both sides reoptimize the same candidate module at O3; the experimental side
only removes six per-iteration nonnegativity assumptions in the four fresh
rebuilds. Ordinary LLVM then removes all four loops and derives the live final
index as capacity minus one. The equal-pass control retains a residual loop in
each rebuild, although that extra pass alone already removes its large vector
loop. Only the four edited function bodies differ between the equal-pass arms.
Wide frames remain 64 bytes, with the same direct payload transfers, allocation
calls and division count. This identifies an optimizer obstruction, not its
runtime contribution; no execution or timing ran for the omission diagnostic.
It does not justify removing the fact family globally, which has separately
measured window benefits. A generic replacement needs its own complete target
contract and performance comparison.

The existing reserve archive's `q205-implementation/` component retains the
exact three-file experimental compiler patch, all four CSVs, the criterion and
reducer, and 296 compact command, check and diagnostic records. Its prior 671
member byte sequences remain unchanged; the former index is renamed
`component-index-before-q205-implementation.json`. The archive now has 684
members. Whole compiler binaries, optimized modules and native objects are
omitted, with original/retained hashes and reconstruction dependencies stated;
the package has not been replayed as a standalone build.

### Complete zeroing with bounded append: scalar gains, wide criterion unmet

A second generic candidate retains complete constructor zeroing and publishes
`nuw nsw` on positive-target-stride Slots append increments. OP-10's admitted
`len < cap` and STOR-6's complete target allocation bound imply that `len + 1`
fits signed and unsigned i64. Zero-stride logical lengths retain ordinary
addition over the full u64 domain; Ring arithmetic is unchanged. Both ordinary
and resident appends use the existing target-layout query. The current resident
selector admits only non-unit scalars, so zero-stride resident emission is not
an inhabited path. No assumption is removed and no acceptance rule changes.

The actual CLI's emitted module, after the identical fixture-main rename, and
native object byte-match the raw two-original-increment discriminator. First
O3 removes all four empty fresh-initialization loops while retaining complete
initialization through calloc. The reached scalar rebuild changes from 116 to
111 native instructions; wide migration is inlined with its direct 256-byte
old-to-new transfer and a 64-byte frame, versus the self-tail control's
336-byte frame and retained migration call. This measures the combined zeroing,
inlining and bounded-increment change, not the isolated cost of any one part.

The frozen combined CLI is
`997a315bef76c906464b233ff47a6c26201eb7f2c4324de4981f914ecc98a221`;
the published self-tail control remains
`bf3eafb4bd5dba320c45654cd3eb80727f5dab3647dadc96978452a4d18a4701`.
Both emit the same benchmark and embedded library. The new experiment uses the
original three-peer driver, the preceding comparison's prospective fixed work,
post-O3 calloc-aware observer, unchanged strict gates and one
control0/candidate0/candidate1/control1 sequence. Earlier failed observations
remain unchanged. All 98 prerequisite outcomes pass, including 54 specific
negative diagnostics. Four 96-row allocation ledgers remain byte-identical to
the original ledger; all four 288-case reserve panels pass.

The campaign exits zero in 120.22 seconds; all four processes exit zero. Each
retains 84,240 rows, totaling 336,960. All interval-validity conditions pass,
including a 42 ns maximum empty-clock interval in each process. Independent
reductions agree on all 3,456 aggregate hierarchies, medians, complete ranges,
ratios and verdicts. All four scalar growth cells have separated gains in both
cohorts (about 6–32 percent); both small-wide cells overlap, and both large-wide
cells fail candidate-WF cohort stability and paired-ratio stability. All eight
no-op before/after comparisons overlap. The two required aligned-wide gains
are therefore not established, and this combined candidate remains experimental.

Aligned-hash growth medians are microseconds per call, cohort0 / cohort1:

| Payload / initial floor | Control WF | Candidate WF | Candidate Rust | Candidate C++ | Before/after qualification |
|---|---:|---:|---:|---:|---|
| 8 B / 64 | 0.231 / 0.231 | 0.215 / 0.214 | 0.196 / 0.196 | 0.134 / 0.129 | Gain |
| 8 B / 4096 | 13.844 / 13.525 | 9.461 / 9.579 | 9.666 / 9.683 | 4.936 / 4.881 | Gain |
| 256 B / 64 | 1.684 / 1.686 | 1.749 / 1.742 | 0.868 / 0.842 | 0.233 / 0.242 | Overlap; raw medians worsen 3.9 / 3.3 percent |
| 256 B / 4096 | 99.082 / 98.705 | 74.204 / 86.516 | 63.578 / 62.718 | 12.630 / 13.062 | Unqualified; raw gains 25.1 / 12.3 percent |

For the final peer target, only the two native-default scalar growth cells
qualify against Rust, the slower peer there. No growth cell qualifies against
C++. All eight no-op cells qualify against C++, but none against Rust. The
aligned large-scalar medians near Rust do not establish separated peer ranges.
Default-policy and aligned-hash outcomes remain separate in the full reduction.

Focused compiler checks pass 29 window tests, three length-residency tests and
the HashMap corpus case in both lowerings with its exact 45-allocation ledger.
The zero-stride native witness appends across i64::MAX and up to u64::MAX without
changing the storage anchor or capacity. The initial missing-capacity-invariant
fixture failure is retained with its source repair. A separate read-only review
finds no issue in the three new implementation/test files; neither this review
nor those focused tests replace final whole-PR validation.

#### Remaining reserve work differs by representation

The fresh combined ledger and reached native paths distinguish concrete work;
they do not assign a percentage of elapsed time to each difference. At initial
entry floors 64 / 4096, all peers preserve 32 / 2048 live values and provide room
for 128 / 8192 total entries without another backing growth. Physical geometry
is implementation-specific and independently checked, not assumed identical.

| Wide-value reserve work | WF | Rust | C++ |
|---|---:|---:|---:|
| New physical slots or buckets | 170 / 10922 | 256 / 16384 | 128 / 8192 |
| New allocation bytes | 46240 / 2970784 | 67848 / 4341768 | 1024 / 65536 |
| Live value bytes relocated | 8192 / 524288 | 8192 / 524288 | 0 / 0 |
| Preparation allocation requests | 1 / 1 | 1 / 1 | 33 / 2049 |

WF initializes its complete future inline table with calloc and scans old tags
at 272-byte strides. Rust allocates more bytes but initially writes only
264 / 16392 contiguous control bytes, scans and probes groups of eight control
bytes, uses a power-of-two mask, recomputes the same aligned hash and copies
264 bytes per live entry (key plus value). WF also recomputes the hash, uses
one division per live entry and directly copies the same live value bytes.
Requested allocation extent does not establish actual physical zero-write
traffic or explain Rust's cost advantage by itself.

C++ keeps each value in its pre-existing 280-byte node, uses the cached full
hash and changes bucket/node links. Its new allocation covers only bucket
pointers; future insertions still allocate nodes. Thus C++'s reserve avoids
payload relocation, while Rust's does not. Exact operation-headroom checks,
whole-value digests and cleanup remain the common contract. These differences
make compact initialization and metadata scanning/probing the next attribution
questions for the Rust comparison; they do not select a new layout, capacity
policy, boxed-value API or a previously rejected inactive-payload policy.

The `combined-zero-nsw/` component of the existing reserve evidence archive
retains all four complete CSVs, both reducers, the exact three-file append
patch and source pins, and 307 compact records including the compiler preflight
failure, native prerequisites and peer attribution excerpts. All 684 prior
member byte sequences are preserved; the archive now has 698 members. Compiler
binaries, whole LLVM modules and native objects remain omitted with their
identities and reconstruction limits recorded. This is retained experimental
evidence, not a standalone published replay or an adopted compiler change.

### Fresh tag initialization after bounded append: small-wide gain only

The next diagnostic changes exactly four fresh Vacant constructor sites in the
combined candidate's raw module: remove their complete-object memset calls,
retaining the tag-zero stores and all 31 assumption mentions. Other constructors
are unchanged. The sites belong to ordinary HashMap extension, including its
inlined rebuilds; this is not a production compiler or a benchmark-selected
lowering rule. The changed optimizer context justifies this discriminator:
unlike the earlier rejected fresh-Vacant and self-tail floors, the combined
module has bounded append increments and already inlines wide migration without
payload staging. Earlier rejected results remain in force.

An equal-pipeline native control reproduces the combined candidate object byte
for byte. The modified arm replaces calloc with malloc plus strided tag-zero
loops in all four rebuilds. Wide frames remain 64 bytes, with direct 256-byte
payload transfers and no stack staging. Hash mixing, one division per live
entry, scalar strided old-tag scanning, linear probing and capacities remain
unchanged. The reached wide body grows from 142 to 173 instructions as tag
initialization reappears. Thus the experiment changes allocation choice,
initialization and instruction shape together; it cannot assign a pure
payload-zero cost or supply Rust's compact metadata layout.

All 96 prerequisite outcomes pass, including 54 diagnostic-specific negatives.
Both timed and accounted arms pass all 288 reserve cases, lookup and whole-trace
checks. Four 96-row ledgers equal the original baseline byte for byte. The
post-O3 observer sees 3,216 calloc requests / 181,246,720 payload bytes in the
combined control and zero in the fresh-tag arm; reversed observation expectations
reject. The final objects also byte-match the preceding native-only artifacts.
These observations establish this diagnostic's tested outcomes, not a general
proof that arbitrary undefined inactive LLVM fields can be transported safely.

The preregistered fixed four-process campaign exits zero in 119.21 seconds;
every process exits zero. All 336,960 rows are retained, and independent reducers
agree on all 3,456 aggregate hierarchies and all sixteen cell verdicts. Interval
validity passes everywhere. Small scalar and wide growth give four qualified
gains; large scalar growth gives two qualified losses of about 12–14 percent.
Both large-wide cells fail control-WF cohort and paired-ratio stability. All
eight no-op comparisons overlap. No retry, row filtering or criterion change
follows. The primary two-size wide criterion fails.

Aligned-hash wide growth medians, microseconds per call, cohort0 / cohort1:

| Initial floor | Combined control WF | Fresh-tag WF | Same-image Rust | Same-image C++ | Qualification |
|---|---:|---:|---:|---:|---|
| 64 | 1.742 / 1.737 | 1.021 / 1.013 | 0.861 / 0.855 | 0.230 / 0.235 | Gain of 41.4 / 41.7 percent; still slower than both peers |
| 4096 | 75.514 / 86.646 | 69.852 / 70.576 | 63.260 / 62.090 | 12.574 / 12.598 | Raw gains 7.5 / 18.5 percent; unqualified |

Only the two native-default scalar growth cells qualify against Rust, their
slower peer; no growth cell qualifies against C++. All eight no-op cells qualify
against C++, none against Rust. No wide target is met. This keeps generic
inactive-payload omission unselected, including the independently observed
large-scalar loss. A future source or compiler candidate needs a general
lowering argument, complete-operation evidence and the existing active-value,
ownership, alias and transport controls; this four-site diagnostic is not that
implementation. The remaining compact-metadata/scan question stays distinct
from allocation extent and from the already removed payload staging.

The existing archive's `fresh-tag-runtime/` component retains the four raw CSVs,
both reducers, exact four-site patch, criteria and 262 compact native/runtime
records. All 698 prior member byte sequences are preserved; there are now 712
members. Omitted whole modules, native objects and compiler images are identified
with their reconstruction dependencies. No standalone replay is claimed.

### Current-policy dense storage: wide reserve gains and scalar losses

This ordinary-source experiment separates compact index storage from the flat
enum table. A dense initialized prefix owns complete key/value pairs and a
reverse bucket index; a separate array holds Vacant, Deleted or a dense index.
It preserves the current filled-plus-vacated pressure rule, tombstone reuse,
ceiling refusal, returned pairs and cleanup. The new representation is an
experimental package, not a production selection or an interface-identical map:
public `cells` and `length` become `entries` and `indexes`, while `vacated` stays.

There is a real source-domain limit. Allocating the fixed-layout `DenseIndex`
array with only `capacity <= ceiling` fails the OP-9 schema judgment under ENT-1:
its stride ceiling is 16 and the symbolic ceiling has no upper bound. The
original generic bucket allocation has genuinely unresolved K/V layout; that
does not defer this unrelated fixed-layout obligation. Both experimental arms
therefore add `ceiling <= 16384`, the largest actual harness instantiation,
instead of writing an ABI-limit constant or using a phantom type parameter.
This bounds the experiment, not the production API. The reverse-bucket word
also changes the concrete allocation domain for small element types. The
unrestricted rejected source and exact bounded-interface patch are retained.

The copied flat control's package transport preserves 93 layouts, 158 LLVM
function definitions and 140 native instruction streams after explicit
name-only correspondence. Adding the shared ceiling bound leaves its emitted
LLVM byte-identical. Both arms use the frozen combined compiler from the
preceding experiment, not an additional compiler change. An unnecessary loop
invariant failed in the maintained owning instance; traversing the actual
planned-array length admits without it. That rejection and source change are
retained, without inferring a compiler defect from the failed proof alone.

The first dense removal swaps two stored entries before taking the tail.
Taking the tail first and exchanging with that local owner only for a nonfinal
removal reduces the wide native frame from 624 to 528 bytes. The three external
272-byte copy/move calls disappear, but path-dependent inline 256-byte transfers
remain. This is a native-code observation, not a measured removal speedup.
The selected experimental sibling passes the maintained owning program in both
lowering modes and the existing 18,390 complete-operation, 96 lookup and 288
reserve cases. Accounted and timed images pass those same operation panels;
all 50 reserve/lookup fault controls reject at their intended diagnostics.
Its maintained program's total allocation count was not independently derived;
the production fixture and its 45-allocation expectation are unchanged.

For each capacity C, reserved backing bytes are `40*C+8` for scalar values and
`288*C+8` for wide values, versus the flat map's `24*C` and `272*C`. Each positive
constructor has two allocations. Growth in the measured half-full windows
allocates new metadata and entry storage, copies the live prefix, and releases
both old blocks; full-prefix growth may instead use realloc. The post-O3
observer now accounts for malloc, calloc, realloc and free. Its success,
relocation, failure-preserves-owner and interception fault tests pass. Its
realloc peak is known live payload storage, not allocator-internal transient
space. All 32 WF reserve-ledger rows match the prospective source formulas;
the 64 peer rows and every semantic/headroom/checksum field are unchanged.

Native rebuild initializes only the 16-byte-per-bucket metadata, rather than
future payload capacity. It still hashes and divides, probes scalar tags,
copies 272 bytes per live wide entry including its reverse index, and scans
the new metadata to repair those indexes. Lookup retains a dependent index
load and an element-bound check. Wide insertion/replacement staging remains.

The one preregistered four-process reserve campaign exits zero in 113.24 seconds.
All 336,960 rows are retained; independent reductions agree on all 3,456
aggregate hierarchies and sixteen verdicts. Instrument conditions pass in all
cells. Two small-wide growth cells improve, four scalar-growth cells regress,
and eight no-op comparisons overlap. Both large-wide comparisons fail flat
control cohort stability and before/after ratio stability. There is no retry,
sample filtering or criterion change. The primary criterion fails.

Aligned-hash medians are microseconds per call, cohort0 / cohort1:

| Value / initial floor | Flat control WF | Dense WF | Same-image Rust | Same-image C++ | Before/after qualification |
|---|---:|---:|---:|---:|---|
| 8 B / 64 | 0.215 / 0.213 | 0.378 / 0.379 | 0.196 / 0.196 | 0.124 / 0.131 | Loss, about 76–78 percent |
| 8 B / 4096 | 12.432 / 12.388 | 27.066 / 27.072 | 9.686 / 9.667 | 4.954 / 4.910 | Loss, about 118–119 percent |
| 256 B / 64 | 1.747 / 1.721 | 0.622 / 0.623 | 0.914 / 0.904 | 0.240 / 0.240 | Gain, about 64 percent; below Rust, above C++ |
| 256 B / 4096 | 75.073 / 86.740 | 37.087 / 37.055 | 66.599 / 62.754 | 13.871 / 13.600 | Unqualified; raw gain about 51–57 percent |

The native-default series remains separately recorded. The data support neither
a universal dense replacement nor completion of the HashMap target. Small-wide
reserve meets the slower-peer target; the large-wide before/after claim remains
unqualified, and scalar growth loses substantially. Lookup and mutation have
correctness and native evidence here, not a new complete performance matrix.
Raw inherited `whitefoot-bundled-std` labels identify a harness slot; the source
provenance for these two arms is the explicitly bounded copied-control and dense
packages above. Compiler, library, API and representation questions remain open.

The `current-dense-policy/` component of [reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz),
indexed by [reserve-api-evidence.json](reserve-api-evidence.json), retains the
four raw timing CSVs, criteria, both reducers, exact experimental packages,
rejected sources, native excerpts and correctness/accounting observations.
All 712 prior archive members retain their bytes (the previous component index
has a historical name). The new archive has 726 members; an independent check
verified its 725 indexed payload hashes, twelve component payload hashes,
180 embedded records against the producer originals, and 409 observation
streams. Pinned compiler binaries, generated objects and full LLVM dumps are
omitted; the retained replay limits describe the dependencies and reconstruction
needed, without claiming standalone replay.

### Geometry observation across automatic growth

The existing geometry checker assumed setup retained the requested physical
capacity and one allocation. Under the current pressure rule, creating 64 slots
and inserting 56 entries grows to 128 slots. The old control consequently fails
`exposed capacity geometry`. It also reconstructed filled live bytes from the
complete trace's peak, which is incorrect when two backings coexist in growth.

The corrected checker holds an ordinary lookup owner, reads its actual capacity
and allocation ledger before cleanup, and checks its full-content cleanup
against the independent oracle. Its complete allocation totals must match the
original setup trace. A separate model derives expected events from the
existing pressure rule; modeled values do not replace observations. For the
64/56 scalar case, actual filled live bytes are 3,072 and peak bytes 4,608;
the wide case gives 34,816 and 52,224. Both expose 128 physical slots.

The old failure is retained. The corrected control passes 140 geometry rows
and all seven existing negative controls. Two new controls reproduce the exact
wrong assumptions: replacing filled live bytes with peak, and replacing actual
capacity with the original request. Both reject at their intended diagnostics
and run through `ecosystem-occupancy-check`. The focused repair check has fourteen
expected stage outcomes and exits zero in 1.09 seconds. An independent read-only
review finds no issue within this bounded repair. This changes geometry
observation, not the isolated reserve workload, its frozen timing images,
library behavior, specification or recorded storage decision.

### Edit running index: native improvement without qualified timing gain

`hash_map_edit` still reconstructed each bucket from the home bucket and the
probe count. The ordinary-source candidate uses the bounded running index
already used by lookup/find/try_put. Hashing remains before the empty check;
all nonempty visited buckets, equality/callback order, complete-cycle refusal,
public fields and contracts stay the same. There is no dense-layout ceiling
restriction. Explicit identical source packages avoid relying on changes to
an on-disk standard library embedded in a frozen compiler.

The initial combined-compiler native screen and the published self-tail
compiler both pass the existing 18,390 complete-operation traces, 96 lookup
cases and 288 reserve cases. The reached scalar/wide edit paths lose the
first-probe compare/select/add reconstruction; continuation advances a wrapped
index. Native frames do not grow, the update stays a direct load/add/store,
and no assumptions or ownership transfers are removed. This is a native
prerequisite, not a measured speedup. The additional source witness checks a
capacity-three wrap past a deleted home bucket, absent full-cycle probes,
empty results, exact callback count and complete consumed key/value sums.
Both ordinary witness executions exit zero. Three independently perturbed
expectations (edited result, callback count and cleanup value sum) each compile
and exit seven, demonstrating that those observations detect wrong results.

Only the published self-tail compiler is used for the primary timing campaign.
The existing complete EDIT trace is selected at initial capacity/live count
64/56 and 4096/3584, with both payload sizes, both hash series and all five
implementations. Each sample requests 2,097,152 edits, rounded down by the live
count, and includes preparation and full cleanup. These are complete-trace
nanoseconds amortized per edit, not isolated API latency. Eleven samples per
cell follow two warmups, with rotating peer order. The one four-process
control0/candidate0/candidate1/control1 campaign retains all 1,760 rows;
all exits are zero and the shortest interval is 3.804 ms.

Aligned-hash medians are cohort zero / cohort one:

| Payload / initial capacity | Control WF | Candidate WF | Candidate-image Rust | Candidate-image C++ |
|---|---:|---:|---:|---:|
| 8 B / 64 | 3.262 / 3.120 | 3.039 / 3.037 | 2.354 / 2.372 | 2.379 / 2.393 |
| 8 B / 4096 | 4.624 / 4.915 | 4.297 / 4.502 | 2.474 / 2.484 | 5.110 / 5.050 |
| 256 B / 64 | 3.232 / 3.231 | 3.054 / 3.190 | 2.788 / 2.792 | 2.374 / 2.372 |
| 256 B / 4096 | 5.651 / 5.729 | 5.101 / 5.123 | 3.180 / 3.176 | 6.022 / 5.994 |

Every candidate median improves, but no before/after cell separates its full
sample ranges in both cohorts. There is no separated loss; within-arm and
unchanged-peer drift checks pass. The prospective source-selection criterion
therefore fails, and the source patch is not adopted. All four aligned cells
also fail the strict slower-peer range target. The four default-hash cells
pass that peer comparison under their different hash protocols; they do not
replace the aligned result. No repeat campaign, sample filtering or criterion
relaxation follows. Root independently checked 480 min/median/max fields,
88 cross-arm/peer checksums and all eight per-series verdicts against the raw
rows. Exact source, prospective criteria, raw rows and replay limits belong to
the `edit-running-index/` component of [reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz).

A read-only follow-up checks why the range test remains inconclusive. Each
sample uses seed `101 + sample`; there is one observation per seed, arm and
cohort. In all sixteen cell/cohort comparisons the candidate maximum and
control minimum come from different seeds. Same-seed candidate values are
lower in 167 of 176 pairs, but nine are not; aligned scalar/4096 cohort zero
has ratios 1.1734 at seed 104 and 0.8092 at seed 110. Repeated control
observations of the same seed also differ across cohorts. The data cannot
partition input effects from execution/order variation. This diagnostic does
not change the registered verdict. A future protocol investigation could
repeat identical inputs with balanced arm order while retaining all workloads,
peers and the strict target; it must state its criterion before measuring.

### Independent local alignment: native reduction without qualified EDIT gain

An isolated compiler prototype relaxes the equal-natural-alignment requirement
for complete independent local roots when every extent is a multiple of their
maximum natural alignment and the existing frame needs no padding. Stronger
requested alignment, zero roots and padding cases retain the old fallback.
The exact two-file patch and its two new extent-boundary tests are experimental;
this result does not select broader eligibility. Both compiler arms use the published original
Map source, not the rejected running-index source.

The reached successful wide EDIT path loses three private `Result<u64,unit>`
stores: tag, returned value and inactive unit payload. The value update remains
a direct load/add/store and probing is unchanged. Complete enclosing trace
bodies shrink from 1,536 to 1,354 scalar instructions and 2,529 to 1,810 wide
instructions; frames shrink from 720 to 224 bytes and 7,424 to 1,984 bytes.
Those whole-function reductions include setup, cleanup and other operation
arms; they are not per-edit savings. Raw modules match after removing only the
local allocation/frame-address recipe, with nominal types and raw assumptions
unchanged. Ten focused target tests pass; restoring the old selector makes
both new eligibility tests fail. All 22 correctness stages exit zero, including
the maintained ordinary/parallel allocation observations of 45 releases each.
The initial scratch expectation of 36 is retained as a failed authoring attempt.

One fixed four-process primary campaign retains 1,760 complete EDIT rows;
all processes exit zero and the shortest interval is 3.805 ms.

Aligned-hash medians are complete-trace nanoseconds amortized per edit,
including setup and cleanup; each pair is cohort zero / cohort one.
Rust and C++ values come from the candidate-image panels.

| Payload / initial capacity | Control WF | Candidate WF | Rust | C++ |
|---|---:|---:|---:|---:|
| 8 B / 64 | 3.091 / 3.250 | 3.036 / 2.962 | 2.365 / 2.354 | 2.392 / 2.381 |
| 8 B / 4096 | 4.656 / 4.922 | 4.433 / 4.439 | 2.453 / 2.488 | 5.153 / 5.085 |
| 256 B / 64 | 3.224 / 3.274 | 3.126 / 3.178 | 2.782 / 2.787 | 2.376 / 2.371 |
| 256 B / 4096 | 5.699 / 5.671 | 5.116 / 5.341 | 3.166 / 3.211 | 6.708 / 6.016 |

The separate native-default panel is retained in `independent-align/records.json`
(record `runtime/edit-runtime-reduction.json`) in the linked evidence archive;
its different hash protocol does not replace this aligned comparison.

Every WF median improves, but no cell separates the full before/after ranges in both cohorts.
The registered primary criterion fails. Additional C-direct and C++ drift
failures remain in the record. All aligned strict peer targets fail; default
hash results are separate. Conditional lookup, mutation and reserve regression
images and timing were not prepared or run. No compiler change is adopted.

Independent reduction checks all 480 statistical fields, eight cell verdicts,
1,672 same-input checksum comparisons and 34 runtime pins. Same-seed WF values
improve in 79 of 88 aligned pairs and 76 of 88 default pairs, with 19 of 88
cell/seed directions reversing between cohorts. These sequential observations
cannot partition input effects from execution/order variation and do not replace
the failed criterion. The `independent-align/` component of
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains the exact
patch, counterfactual failures, native excerpts, criteria, raw rows and reducers.

### EDIT home mask: native division bypass without qualified timing gain

A separate ordinary-source candidate changes only `hash_map_edit` home-bucket
selection: power-of-two capacities use a mask, with modulo retained otherwise.
It uses the same published compiler on both arms and composes neither the
running-index nor independent-alignment prototype. Earlier find/try_put masking
is a separate result. The reached power-of-two EDIT path bypasses division;
non-power-of-two capacity, probing, direct payload update and private Result
stores remain. Enclosing scalar and wide bodies each gain five static
instructions, with frames, stack accesses and call counts unchanged. This is
not five additional instructions on every dynamic path.

All ten native stages and all thirty correctness stages have their expected
outcomes. Both arms pass the complete trace, lookup and reserve panels and the
capacity-three/empty/full-cycle/callback witness; wrong result, callback and
cleanup expectations each exit seven. One four-process campaign retains all
1,760 rows with zero exits and a 3.788 ms minimum interval.

Aligned-hash medians are complete-trace nanoseconds amortized per edit,
including setup and cleanup; each pair is cohort zero / cohort one.
Rust and C++ values come from the candidate-image panels.

| Payload / initial capacity | Control WF | Candidate WF | Rust | C++ |
|---|---:|---:|---:|---:|
| 8 B / 64 | 3.113 / 3.093 | 3.116 / 3.108 | 2.391 / 2.371 | 2.369 / 2.396 |
| 8 B / 4096 | 4.632 / 4.794 | 4.548 / 4.485 | 2.517 / 2.492 | 4.973 / 4.955 |
| 256 B / 64 | 3.260 / 3.293 | 3.375 / 3.383 | 2.786 / 2.790 | 2.393 / 2.386 |
| 256 B / 4096 | 5.670 / 5.636 | 5.246 / 5.439 | 3.169 / 3.255 | 6.936 / 6.373 |

The separate native-default panel is retained in `edit-home-mask/records.json`
(record `runtime/edit-runtime-reduction.json`) in the linked evidence archive;
its different hash protocol does not replace this aligned comparison.

No cell has separated gain or loss ranges in both cohorts. At aligned capacity 64, scalar medians
worsen by about 0.11/0.49 percent and wide medians by 3.51/2.74 percent. Additional
C-direct cohort drift and C++ cross-arm drift also fail. All aligned strict
peer targets fail; default targets pass separately. The primary criterion
fails, with no retry or source adoption.

Independent checks reproduce 480 statistics, all eight cell verdicts, 1,672
same-input checksum comparisons and 99 input pins. The `edit-home-mask/`
component retains sources, fault controls, small native excerpts, criteria,
all raw samples and reductions. Both new components preserve the earlier 740
member byte sequences, with the previous index under an explicit historical
name. Binaries, objects and full LLVM/disassembly dumps remain omitted; recorded
commands need the pinned dependencies and scratch/toolchain reconstruction
specified in the retained replay limits.

### Outer EDIT dispatch: qualified caller-form gains on large maps

This attribution trial changes the ordinary caller, with the published compiler
and Map source byte-identical in both arms. A single outer `path == 7` choice
selects a dedicated EDIT round/key loop. Setup and cleanup are byte-identical;
the other operation loop is identical modulo whitespace and removal of its
now-unreachable EDIT arm. The dedicated loop copies the original key generation,
edit call and result consumption in the same order and nesting. It composes
none of the running-index, home-mask or independent-alignment candidates.

The reached loop loses repeated operation dispatch and hoists environment,
count and backing loads. Hashing and one division/remainder pair per key remain,
as do the digest load/store and three private Result stores. No repeated call
or payload copy is added to that loop. Whole-trace scalar instructions increase
from 1,536 to 1,617 and its frame from 720 to 768 bytes; wide instructions decrease
from 2,529 to 2,481 and its frame from 7,424 to 7,296 bytes. These counts include
other operation arms, setup and cleanup. An initial mechanical brace deletion
fails admission and is retained; the corrected candidate resumes only its
unfinished build stages. All thirty correctness stages have their expected
outcomes, including both full-operation panels and the six result/callback/
cleanup fault executions.

One fixed four-process campaign retains 1,760 rows with all exits zero and a
3.769 ms shortest interval. Aligned-hash medians below are complete-trace
nanoseconds amortized per edit, including setup and cleanup. Pairs are cohort
zero / cohort one; both peer columns come from candidate-image panels.

| Payload / initial capacity | Control WF | Candidate WF | Rust | C++ |
|---|---:|---:|---:|---:|
| 8 B / 64 | 3.102 / 3.151 | 3.043 / 3.084 | 2.360 / 2.368 | 2.388 / 2.389 |
| 8 B / 4096 | 4.696 / 4.816 | 3.307 / 3.273 | 2.459 / 2.461 | 5.156 / 5.083 |
| 256 B / 64 | 3.256 / 3.235 | 3.103 / 3.042 | 2.845 / 2.786 | 2.374 / 2.374 |
| 256 B / 4096 | 5.802 / 5.670 | 3.610 / 3.639 | 3.167 / 3.177 | 5.815 / 6.027 |

All four capacity-4096 cells, across both payloads and both hash series, separate
gains in both cohorts under the registered caller-form criterion. Median
improvements are about 30–38 percent. All within-arm cohort and ordinary-peer
cross-arm drift checks pass; no cell separates a loss. The four capacity-64
cells overlap and remain unqualified despite lower medians. Both large aligned
cells pass the strict slower-peer target, while both small aligned cells fail.
The candidate remains slower than Rust in both large aligned median comparisons;
passing the slower C++ target does not mean beating both peers. Native-default
passes its separate peer target in all four cells under different hash protocols;
its complete panel remains in the retained reduction.

Independent checks reproduce all 480 statistical fields and eight cell verdicts,
1,672 same-input checksum comparisons and 100 runtime input pins. The gain belongs
to the complete caller rewrite and resulting load hoisting, layout, scheduling
and register changes; it cannot be subtracted as the elapsed share of repeated
path tests alone. This selects no library optimization or benchmark replacement
and does not reclassify the original multipath verdict or earlier failed trials.
The `edit-batch-caller/` component of
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains exact source
packages and patch, the failed attempt, native excerpts, complete correctness
records, criteria, raw rows, reducers and replay limits. All 740 previously
published member byte sequences remain preserved.

### Result materialization and alignment in the outer EDIT caller

A native-only counterfactual replaces the dedicated caller's Result helper with
its identical ordinary match. All ten build/admission stages pass, but the
required mechanism fails: the three private Result stores and digest load/store
remain in both payload cases. Raw lowering still materializes the returned
aggregate before matching. Avoiding the helper alone is insufficient. No new
correctness or timing campaign follows that negative native gate, and no generic
lowering change is selected from it.

A separate interaction comparison keeps the outer-dispatch caller, original
Result helper and published library byte-identical in both arms. Only the
frozen independent-alignment compiler differs. The reached successful EDIT path
loses the three Result stores and the digest load/store pair; key stack storage
also disappears and wide hash constants remain in registers. Hashing, division,
probe order and callback observations remain. Whole enclosing scalar/wide
frames shrink from 768/7,296 to 224/2,240 bytes; these include other operation
arms, setup and cleanup. Ten native/admission stages pass. Both arms pass
18,390 complete traces, 96 lookup cases and 288 reserve cases; the thirty
correctness stages comprise twenty-four zero exits and six intended fault exits
of seven. No direct-match or home-mask source is composed.

One fixed four-process campaign retains all 1,760 rows with zero exits and a
3.782 ms shortest interval. Aligned-hash complete-trace medians, amortized per
edit including setup and cleanup, are cohort zero / cohort one. Rust and C++
columns come from candidate-image panels.

| Payload / initial capacity | Control WF | Candidate WF | Rust | C++ |
|---|---:|---:|---:|---:|
| 8 B / 64 | 3.047 / 3.042 | 2.675 / 2.668 | 2.371 / 2.352 | 2.397 / 2.372 |
| 8 B / 4096 | 3.307 / 3.332 | 3.066 / 3.036 | 2.466 / 2.481 | 5.443 / 5.423 |
| 256 B / 64 | 3.044 / 3.108 | 2.671 / 2.676 | 2.784 / 2.785 | 2.381 / 2.393 |
| 256 B / 4096 | 3.662 / 3.666 | 3.309 / 3.256 | 3.168 / 3.218 | 5.818 / 5.895 |

Five cells qualify under the registered marginal-gain criterion, with no
separated loss: both small aligned cells, wide large aligned, and both small
default-hash cells. Small aligned medians improve about 12–14 percent, but both
still fail the strict peer-range target. Aligned scalar/4096 separates WF gain
ranges yet is invalid because candidate C-direct cohort drift is 11.1495 percent.
That failure is retained. Only aligned wide/4096 passes the full strict peer
target. Both large default-hash cells overlap; default peer targets pass
separately under their different hash protocol and do not replace aligned
qualification. The complete default panel remains in the retained reduction.

Independent checks reproduce 480 statistics, all eight cell verdicts, 1,672
same-input checksum comparisons and 107 input pins. This supports a compiler
benefit in the changed caller context, not a per-store elapsed cost or a
retroactive factorial comparison. The earlier multipath alignment campaign
remains failed and the caller-only result remains separate. No compiler/library
adoption or broader performance target is selected. The `alignment-interaction/`
component of [reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains
both criteria, exact patches/source provenance, the direct-match failure,
compact native excerpts, attribution notes, complete correctness records, raw
rows and reducers. All 789 previously published member byte sequences remain
preserved; omitted generated artifacts and reconstruction limits are explicit.

### Conditional API screen: no separated loss, incomplete qualification

The independent-alignment compiler's conditional non-EDIT screen keeps the
published library and required complete outcomes unchanged. All 28 preparation
and correctness stages have their expected exits. Mutation images check three
successive batches and full cleanup, with wrong-batch, stale-generation,
corrupt-last-word and cleanup controls. Reserve images check ordinary reserve
and the clock controls. All twelve fixed timing processes exit zero; there is
no repeat campaign. The 339,264 retained rows give:

| Family | Rows | Valid cells | Invalid cells | Before/after result |
|---|---:|---:|---:|---|
| Lookup | 1,152 | 5 | 3 | All eight ranges overlap |
| Mutation | 1,152 | 7 | 1 | All eight ranges overlap |
| Reserve | 336,960 | 16 | 0 | Two large-scalar growth gains; fourteen overlaps |

Lookup invalidates scalar/4096 miss, wide/4096 hit and wide/4096 miss through
WF and/or C-direct cohort drift; mutation invalidates scalar/4096 churn through
candidate WF drift. The overall screen is therefore **unqualified**. No
separated loss is detected, which is not proof of nonregression. Reserve passes
its complete interval/clock/cohort checks, but all four aligned growth cells
still fail the strict slower-peer target. Full WF, Rust and C++ statistics
remain in the family reductions. Minimum real intervals are 3.023 ms lookup,
2.328 ms mutation and 3.383212 ms reserve; every reserve instrumentation flag
passes. Independent raw-data audit reproduces the reductions and all 3,456
reserve sample hierarchies with no discrepancy.

A read-only lookup note finds 196 instructions in each word/wide query body,
identical after normalizing absolute addresses in existing object disassembly.
The inlined query already uses a register digest without the EDIT Result-store
pattern. This does not establish linked-image equality, equal runtime cost or
the cause of drift, and cannot qualify the invalid cells. The
`alignment-regression/` component of
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains that limitation,
criteria, source/drivers, fault results, pins, all raw rows and the reducer.

### Marginal home mask with the alignment compiler

This separate source comparison holds the outer-dispatch caller and frozen
alignment compiler fixed. Only EDIT home selection changes to a power-of-two
mask with modulo fallback. Native code still tests power-of-two capacity per
key; it does not hoist that test. Hashing, bounded probing, callback and register
digest remain, without private Result stores or a digest memory pair. The wide
whole-trace frame grows by 16 bytes. These collateral changes prevent assigning
the measured difference solely to division latency. The native and correctness
screens pass their expected outcomes before one four-process campaign; all
1,760 rows are retained, all exits are zero, and the minimum interval is 3.819 ms.

Aligned-hash medians are complete-trace nanoseconds amortized per edit,
including setup and cleanup; pairs are cohort zero / cohort one. Both peers
come from candidate-image panels.

| Payload / initial capacity | Control WF | Candidate WF | Rust | C++ |
|---|---:|---:|---:|---:|
| 8 B / 64 | 2.693 / 2.677 | 2.351 / 2.354 | 2.377 / 2.378 | 2.404 / 2.390 |
| 8 B / 4096 | 3.082 / 3.080 | 3.013 / 3.005 | 2.476 / 2.488 | 5.186 / 5.069 |
| 256 B / 64 | 2.671 / 2.681 | 2.367 / 2.359 | 2.807 / 2.812 | 2.377 / 2.380 |
| 256 B / 4096 | 3.338 / 3.275 | 3.342 / 3.351 | 3.181 / 3.203 | 5.986 / 7.012 |

All four small-map cells, across both payloads and hash series, qualify gains.
All four large-map ranges overlap. Aligned large-wide is additionally invalid:
candidate C++ cohort drift is 17.1394 percent and cross-arm drift is 16.0758
percent, with adverse raw WF median changes retained. No cell separates a loss.
The aligned strict peer target fails for small scalar, passes for small wide
and large scalar, and is invalid for large wide. Lower medians alone do not
meet the range target. Native-default passes its separate peer comparisons,
under different hash protocols; full panels remain in the reduction.

Independent checks reproduce 480 statistical fields, eight cell verdicts,
1,672 same-input checksum comparisons and 103 input pins. This is a marginal
source/code-generation gain in the characterized experimental caller/compiler
context. It selects no adoption, does not revise the earlier original-context
mask failure, and does not cure the unqualified API regression screen. The
`alignment-home-mask/` component retains the exact source patch, native and
correctness evidence, raw rows, criteria and reducers. All 808 previously
published member byte sequences remain preserved; full generated modules,
binaries and disassembly are omitted with pins and replay limits.

### Running index in the combined experimental context

The next marginal source comparison keeps the outer-dispatch caller,
independent-alignment compiler and EDIT home mask fixed. A bounded running
index replaces EDIT's repeated home-plus-step reconstruction. The first hit
skips the wrap continuation; later probes preserve the same bounded order.
Native code removes the first-probe compare/select/add sequence without adding
repeated calls, stack accesses, payload copies or Result storage. Frames and
whole-trace call counts stay unchanged. Native and complete correctness checks
pass their expected outcomes before one fixed four-process campaign, with
1,760 retained rows, zero exits and a 3.796 ms minimum interval.

Aligned-hash complete-trace median nanoseconds per edit include setup and
cleanup; pairs are cohort zero / cohort one. Both peers are from candidate
panels.

| Payload / initial capacity | Control WF | Candidate WF | Rust | C++ |
|---|---:|---:|---:|---:|
| 8 B / 64 | 2.361 / 2.356 | 1.896 / 1.911 | 2.355 / 2.350 | 2.383 / 2.390 |
| 8 B / 4096 | 3.826 / 2.998 | 3.031 / 2.899 | 2.492 / 2.455 | 5.194 / 5.069 |
| 256 B / 64 | 2.355 / 2.360 | 1.900 / 1.903 | 2.790 / 2.796 | 2.405 / 2.370 |
| 256 B / 4096 | 3.296 / 3.321 | 3.604 / 3.003 | 3.184 / 3.178 | 6.008 / 5.912 |

Both small aligned cells qualify marginal gains of about 19 percent and pass
the unchanged strict slower-peer range target. Small default scalar also
qualifies; small default wide separates a gain only in one cohort and remains
unqualified. All large-map before/after ranges overlap. Both large aligned
cells are invalid: scalar control WF cohort drift is 27.6169 percent and its
ratio spread is 22.0544 percent, with additional C-direct drift; wide candidate
WF cohort drift is 20.0095 percent and ratio spread is 20.9300 percent. The wide
raw ratio reverses from an adverse 1.0936 to 0.9044 between cohorts. Raw peer
ranges do not override those invalidations. Default peer targets pass separately
under different hash protocols. No cell separates a loss in both cohorts.

A separate native-only `unswitch-threshold=1000` counterfactual is rejected:
it hoists the capacity test, but scalar/wide trace growth exceeds the registered
25-percent cap and the wide hot loop gains salt reload/hash-constant work.
It runs no correctness or timing campaign, and the source comparison above uses
ordinary O3 without that override. Independent reduction reproduces 480
statistical fields, all eight verdicts, 1,672 same-input checksum comparisons
and 103 runtime pins. Neither this context-specific gain nor the rejected
threshold trial assigns elapsed cost to individual instructions. No adoption
follows; both large aligned cells and the conditional API screen remain
unqualified, and the earlier original-context running-index failure is unchanged.
The `alignment-running-index/` component of
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains exact source,
criteria, compact native/fault evidence, all raw rows and reducers, plus the
threshold rejection and explicit replay limits.

### Repeated-seed diagnostic: unresolved execution variation

A driver-only diagnostic repeats identical seeds in opposite orders within
one process and linked image. Its four raw CSVs retain 3,520 rows and 1,760
paired observations; all outcomes and coverage checks pass, and the minimum
interval is 3.782 ms. Two of 160 within-process group median spreads exceed
10 percent. Individual same-seed ratios are mixed: controlling the seed does
not identify temporal, allocator, cache, address or branch effects. Reversing
order also changes execution history, and recorded link-symbol addresses do
not establish runtime ASLR addresses. No earlier qualification or adoption
verdict changes.

The `repeated-seed-diagnostic/` component in
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains the exact
driver patch, prospective criterion, raw rows, reducers, pins and statuses,
including the first wrapper assertion failure and repair. That wrapper
expected the earlier complete-driver count; the diagnostic driver correctly
checks 2,714 traces. No real sample, source criterion or expected semantic
outcome was changed to repair the wrapper. Replay needs the pinned parent
inputs and omitted build dependencies described in the component.

### Retrospective EDIT peer selection: unchanged verdicts

The seven historical EDIT reducers selected one median-slower Rust/C++ peer
across pooled cohorts. The maintained rule selects independently within each
cohort; ties select both peers and use their less favorable minimum. Replaying
all seven archived reductions reproduces their saved outputs before correcting
only this selection and its dependent range checks. Across 12,320 raw rows and
56 cells, one choice changes: original `edit-home-mask`, aligned hashing,
8-byte payload and capacity 64, cohort 1 selects C++ instead of Rust. Its range
check still fails. All historical range, target and source-retention verdicts
remain unchanged.

Six focused fixtures agree with the unchanged maintained Perl target function.
The crossed-peer false-pass and false-fail fixtures satisfy duration and all
cohort-stability gates, isolating the selector defect; the other fixtures cover
ties, exact range equality and refusal of a sub-1-ms selected comparison.
Initial fixture and archive-replay authoring failures and their repairs are
retained in `peer-selection-correction/` in the same archive.

The separately labeled canonical audit does not replace the historical,
stricter all-implementation drift gate. In particular, its target passes for
aligned scalar 4096 in the alignment-interaction and running-index campaigns
do not requalify those historically invalid cells. The historical strict-target
Boolean also omitted an explicit duration guard, although the source-retention
criterion checked the global minimum. All actual intervals are at least
3.769 ms, so that omission changes none of these observations. Future consumers
must check duration explicitly rather than treat that Boolean alone as a
qualified target. Exact sources, corrected outputs, canonical audits and
archive-only replay provenance are retained with original and normalized
hashes; original frozen records remain byte-identical.

### Integrated candidate with the maintained caller

The actual integrated CLI and embedded library are compared with the pinned
prior production CLI/library using byte-identical maintained multipath source.
This is the original caller context, separate from the outer-dispatch and
cleanup experiments. The 52 preparation/native/oracle/fault stages have their
expected exits: 40 zero and 12 intentional semantic/clock refusals. Complete
outcomes, ownership and cleanup checks pass; this does not qualify timing.

Four fixed panels retain 341,024 rows: 1,152 lookup, 1,152 replacement/churn,
336,960 reserve and 1,760 complete EDIT-trace rows. No cell has a qualified
before/after gain or two-cohort separated loss. All four overall screens are
unqualified. Lookup has six valid overlaps and two invalid cells; mutation
has five valid overlaps and three invalid; reserve has eight valid no-op
overlaps and eight invalid growth cells; EDIT has seven valid comparisons
and one invalid, with no gain separated in both cohorts.

Medians below are ns/operation, cohort 0 / cohort 1. Lookup measures its query
window; replacement/churn includes the complete offered/returned ownership
boundary; reserve measures the ordinary call; EDIT amortizes its entire
setup/edit/cleanup trace. These windows are not interchangeable. Rust and C++
are the candidate-panel observations. Aligned hashing:

| Window / payload / capacity | Control WF | Candidate WF | Rust | C++ | Qualification |
|---|---:|---:|---:|---:|---|
| lookup hit / 8 B / 64 | 1.898 / 1.980 | 1.909 / 1.960 | 2.129 / 2.131 | 1.603 / 1.604 | overlap |
| lookup miss / 8 B / 64 | 2.467 / 2.427 | 2.436 / 2.471 | 1.626 / 1.574 | 1.547 / 1.585 | overlap |
| lookup hit / 8 B / 4096 | 2.343 / 2.145 | 2.151 / 2.161 | 2.219 / 2.223 | 1.618 / 1.627 | overlap |
| lookup miss / 8 B / 4096 | 2.983 / 2.965 | 2.750 / 2.736 | 2.142 / 2.143 | 1.458 / 1.450 | overlap |
| lookup hit / 256 B / 64 | 1.969 / 1.897 | 1.918 / 1.969 | 2.307 / 2.381 | 1.614 / 1.624 | overlap |
| lookup miss / 256 B / 64 | 2.469 / 2.458 | 2.548 / 2.565 | 1.659 / 1.748 | 1.565 / 1.593 | invalid |
| lookup hit / 256 B / 4096 | 2.317 / 2.357 | 2.343 / 2.301 | 2.467 / 2.470 | 1.961 / 2.038 | overlap |
| lookup miss / 256 B / 4096 | 2.885 / 2.935 | 4.007 / 2.946 | 2.226 / 2.224 | 1.743 / 1.762 | invalid |
| mutation churn / 8 B / 64 | 8.063 / 7.924 | 7.994 / 8.058 | 16.212 / 16.216 | 27.876 / 27.935 | invalid |
| mutation replace / 8 B / 64 | 2.439 / 2.344 | 2.379 / 2.381 | 2.489 / 2.487 | 2.748 / 2.755 | overlap |
| mutation churn / 8 B / 4096 | 16.592 / 17.103 | 16.105 / 18.554 | 10.671 / 10.309 | 30.911 / 30.031 | invalid |
| mutation replace / 8 B / 4096 | 3.584 / 3.298 | 3.293 / 3.538 | 2.610 / 2.610 | 4.039 / 4.033 | invalid |
| mutation churn / 256 B / 64 | 34.223 / 32.946 | 34.128 / 32.745 | 27.636 / 27.061 | 50.477 / 49.301 | overlap |
| mutation replace / 256 B / 64 | 24.011 / 23.926 | 24.395 / 23.847 | 22.560 / 22.061 | 19.151 / 18.918 | overlap |
| mutation churn / 256 B / 4096 | 49.444 / 49.668 | 48.467 / 47.999 | 32.047 / 31.419 | 60.181 / 59.079 | overlap |
| mutation replace / 256 B / 4096 | 29.322 / 28.316 | 29.057 / 28.269 | 28.209 / 27.178 | 23.517 / 23.301 | overlap |
| reserve grow / 8 B / 64 | 232.243 / 236.157 | 234.107 / 238.927 | 201.184 / 205.632 | 130.248 / 136.387 | invalid |
| reserve noop / 8 B / 64 | 1.258 / 1.280 | 1.262 / 1.283 | 0.965 / 0.984 | 2.824 / 2.856 | overlap |
| reserve grow / 8 B / 4096 | 14011.103 / 14306.800 | 13093.585 / 13524.578 | 9847.493 / 9961.752 | 5085.121 / 5210.245 | invalid |
| reserve noop / 8 B / 4096 | 1.325 / 1.342 | 1.339 / 1.337 | 1.022 / 1.018 | 2.959 / 2.907 | overlap |
| reserve grow / 256 B / 64 | 1735.534 / 1745.191 | 1879.921 / 1978.442 | 1228.175 / 1255.442 | 260.014 / 253.981 | invalid |
| reserve noop / 256 B / 64 | 1.258 / 1.280 | 1.261 / 1.268 | 0.965 / 0.974 | 2.864 / 2.835 | overlap |
| reserve grow / 256 B / 4096 | 104128.375 / 101664.830 | 98794.435 / 99749.633 | 67887.452 / 65902.431 | 13153.411 / 12976.481 | invalid |
| reserve noop / 256 B / 4096 | 1.319 / 1.347 | 1.342 / 1.329 | 1.026 / 1.016 | 2.936 / 2.900 | overlap |
| EDIT trace / 8 B / 64 | 3.213 / 3.149 | 2.965 / 2.956 | 2.379 / 2.382 | 2.380 / 2.378 | overlap |
| EDIT trace / 8 B / 4096 | 4.783 / 4.947 | 4.228 / 4.235 | 2.518 / 2.510 | 5.166 / 5.146 | invalid |
| EDIT trace / 256 B / 64 | 3.279 / 3.285 | 3.326 / 3.265 | 2.817 / 2.820 | 2.424 / 2.388 | overlap |
| EDIT trace / 256 B / 4096 | 5.876 / 5.740 | 4.973 / 5.000 | 3.286 / 3.287 | 6.277 / 5.941 | overlap |

Native default hashing remains separate:

| Window / payload / capacity | Control WF | Candidate WF | Rust | C++ | Qualification |
|---|---:|---:|---:|---:|---|
| reserve grow / 8 B / 64 | 232.817 / 239.337 | 237.989 / 237.303 | 374.474 / 370.115 | 101.762 / 98.320 | invalid |
| reserve noop / 8 B / 64 | 1.270 / 1.288 | 1.279 / 1.270 | 0.978 / 0.963 | 2.239 / 2.198 | overlap |
| reserve grow / 8 B / 4096 | 13601.275 / 15091.069 | 13816.894 / 12733.364 | 20691.938 / 20507.605 | 3541.015 / 3371.338 | invalid |
| reserve noop / 8 B / 4096 | 1.322 / 1.371 | 1.344 / 1.323 | 1.018 / 1.012 | 2.287 / 2.256 | overlap |
| reserve grow / 256 B / 64 | 1708.095 / 1912.925 | 2057.418 / 1719.772 | 1230.152 / 985.715 | 227.933 / 199.353 | invalid |
| reserve noop / 256 B / 64 | 1.299 / 1.328 | 1.276 / 1.265 | 0.973 / 0.955 | 2.875 / 2.839 | overlap |
| reserve grow / 256 B / 4096 | 100545.495 / 108851.121 | 107546.017 / 101299.650 | 94981.444 / 91828.569 | 15661.701 / 12911.289 | invalid |
| reserve noop / 256 B / 4096 | 1.325 / 1.345 | 1.329 / 1.328 | 1.027 / 1.024 | 2.906 / 2.954 | overlap |
| EDIT trace / 8 B / 64 | 3.237 / 3.192 | 2.977 / 2.981 | 8.805 / 8.775 | 2.110 / 2.118 | overlap |
| EDIT trace / 8 B / 4096 | 4.738 / 4.775 | 4.292 / 4.238 | 8.921 / 8.906 | 1.862 / 1.846 | overlap |
| EDIT trace / 256 B / 64 | 3.319 / 3.418 | 3.363 / 3.286 | 9.349 / 9.129 | 2.135 / 2.068 | overlap |
| EDIT trace / 256 B / 4096 | 5.693 / 5.800 | 4.830 / 4.929 | 9.851 / 9.666 | 2.414 / 2.424 | overlap |

Reserve candidate 0 exits 1: its measured maximum empty interval is 9,500 ns,
versus 42 ns in the other processes, with 41 ns clock quantum throughout.
That maximum violates the one-percent real-batch criterion for 41,029 batches
and invalidates all growth cells. All rows, bad instrument flags and additional
drift failures remain; aggregate duration cannot repair them. Lookup's invalid
cells are both wide misses; mutation's are scalar churn at both sizes and
large scalar replacement. EDIT's large aligned scalar fails C++ A/B drift
at 11.26495 percent. There was no retry. A prior moving-source preflight failed
before any timing process; rebinding the control source to its already pinned
snapshot repaired provenance without changing images or criteria.

The strict peer target passes only wide-small lookup hit, wide-small churn,
the eight reserve no-ops, large aligned wide EDIT and the four separately
reported default EDIT cells. All other target cells remain false. Independent
readers reproduce the frozen statistics and verdicts, including 3,456 reserve
hierarchies. The `matched-production/` component of
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains every raw row,
criterion, reduction, failure and compact native/check record, with explicit
omitted-build replay limits.

Native observations explain remaining work without assigning elapsed shares.
Integrated EDIT removes three Result stores, uses mask/fallback home selection
and a running index, while both widths retain per-key dispatch. Scalar digest
is register-carried; wide digest still has an ordered-field load/store pair.
The linked lookup bodies each have 196 instructions and match after address
normalization; this is not equality of elapsed cost. The ordinary Rust/C++
linked EDIT paths place operation selection outside their key loops despite
the original source callers placing it inside. Whole-frame reductions and
prior outer-caller gains do not establish maintained-caller performance.

### Late cleanup exposure: native success without qualified gain

The wide digest remains address-exposed because terminal cleanup receives the
same complete value by reference. Its ordered-field memory operations already
exist before register allocation; calling them register-pressure spills is
unsupported. The single source discriminator reconstructs all four Copy integer
digest fields immediately before the same cleanup call and observes the final
post-call value. Timed loops, callbacks, map representation and compiler remain
unchanged. This is a caller-level diagnostic, not a general compiler lifetime
transform or library/API change.

Native compilation removes the wide ordered-field load/store pair and retains
one terminal materialization. The wide frame falls from 1,984 to 1,968 bytes;
scalar frame remains 224 bytes. No new hot call, copy or spill appears, but
wide register assignment, branch layout and dispatch promotion also change,
so the experiment cannot isolate the elapsed cost of two instructions. Ten
native stages exit zero; all 26 correctness stages have expected outcomes
(23 zero and three deliberate result/callback/cleanup refusals).

Aligned medians are ns/edit for the complete trace, cohort 0 / cohort 1:

| Window / payload / capacity | Control WF | Candidate WF | Rust | C++ | Qualification |
|---|---:|---:|---:|---:|---|
| EDIT trace / 8 B / 64 | 2.972 / 2.974 | 2.939 / 2.931 | 2.368 / 2.382 | 2.394 / 2.372 | overlap |
| EDIT trace / 8 B / 4096 | 4.148 / 4.132 | 4.213 / 4.083 | 2.474 / 2.478 | 5.094 / 5.023 | overlap |
| EDIT trace / 256 B / 64 | 3.284 / 3.268 | 3.252 / 3.269 | 2.783 / 2.795 | 2.395 / 2.396 | overlap |
| EDIT trace / 256 B / 4096 | 4.764 / 4.900 | 4.873 / 4.963 | 3.183 / 3.188 | 5.984 / 6.023 | invalid |

All 16 before/after cohort ranges overlap, including both aligned wide cells;
there is no qualified gain or separated loss. Large aligned wide also fails
C++ stability (15.3773 percent control-cohort drift and 14.0675 percent
cohort-0 A/B drift). All four processes exit zero, retaining 1,760 rows with
minimum interval 3.776 ms. Default-series observations remain separate in the
complete retained reduction. The failed criterion selects no source change,
compiler transform or API optimization, and no sample was retried.

The `cleanup-digest/` archive component retains the exact source pair and patch,
pretrial rationale, native and timing criteria, compact native excerpts,
checks/faults, raw rows, reducers and pins. Earlier native-stage statements
that timing/checks had not yet run describe that prerequisite stage; the
separate terminal outcomes record the subsequent fixed campaign. The native
result supplies a concrete late-address-exposure mechanism to investigate,
not a general solution or performance qualification.

### Isolated public EDIT: hit targets pass, misses remain open

The explicit research consumer calls ordinary public `hash_map_edit`, Rust
`get_mut`, C++ find/update and the existing sparse C attribution control. Each
sample has one exported mutable batch, a fresh owner and a local ordered digest
returned by value. Setup, capacity checks, complete 32-word cleanup and exact
release checks are outside timing. Hits increment the payload's first word
once and observe the updated value; misses observe absence. The library,
compiler and public API are unchanged. This integer-fixture instrument does
not establish generic owning behavior or replace the application workload.

The single fixed campaign uses salted mix64 hashing, physical targets 64/4096,
counts 32/2048, 12 seeds, two warmups, 4,194,304 edits per sample and two reversed
cohorts with balanced participant positions. It retains 768 raw rows. All four
hit cells qualify under the registered per-cohort median-slower Rust/C++ target;
small hits select Rust, large hits select C++. This does not mean every WF range
beats both peers: scalar-small hit overlaps C++ in cohort 0, and large hit
ranges can overlap Rust. All four miss cells fail. No samples were filtered,
retried or adapted, and this one campaign does not establish repeatability.

Observed min / median / max ns per edit, rounded to six decimals; every adverse
sample remains, including the large-wide C++ hit maximum of 10.355274 ns:

| Payload / physical target / path | Cohort | WF min / median / max | Rust min / median / max | C++ min / median / max |
|---|---:|---:|---:|---:|
| 8 B / 64 / hit | 0 | 1.822501 / 1.891116 / 2.188504 | 2.235125 / 2.308697 / 2.456407 | 2.104670 / 2.151325 / 2.309154 |
| 8 B / 64 / hit | 1 | 1.780679 / 1.858403 / 1.921932 | 2.205094 / 2.260864 / 2.316952 | 2.075533 / 2.135898 / 2.211988 |
| 8 B / 64 / miss | 0 | 2.209206 / 2.632161 / 3.089617 | 1.967838 / 2.273867 / 2.770612 | 1.701226 / 1.785646 / 1.983931 |
| 8 B / 64 / miss | 1 | 2.131462 / 2.442638 / 3.086597 | 1.957486 / 2.232109 / 2.472639 | 1.642744 / 1.704097 / 1.766791 |
| 8 B / 4096 / hit | 0 | 2.145419 / 2.209018 / 2.351075 | 2.382249 / 2.403990 / 2.604753 | 2.793700 / 3.020356 / 3.119737 |
| 8 B / 4096 / hit | 1 | 2.160708 / 2.287085 / 2.527813 | 2.379010 / 2.464881 / 2.635350 | 2.917588 / 3.155544 / 3.486802 |
| 8 B / 4096 / miss | 0 | 2.526194 / 3.213833 / 5.443404 | 2.530942 / 2.600883 / 2.965907 | 1.565655 / 1.654416 / 1.980215 |
| 8 B / 4096 / miss | 1 | 2.536635 / 3.298178 / 4.650603 | 2.516508 / 2.591610 / 2.664536 | 1.595527 / 1.687765 / 1.883914 |
| 256 B / 64 / hit | 0 | 1.787255 / 1.862407 / 1.922170 | 2.499630 / 2.540544 / 2.612015 | 2.086788 / 2.142732 / 2.249837 |
| 256 B / 64 / hit | 1 | 1.785765 / 1.834154 / 1.932770 | 2.496322 / 2.528608 / 2.568543 | 2.063314 / 2.135088 / 2.196848 |
| 256 B / 64 / miss | 0 | 2.143592 / 2.437890 / 3.014873 | 1.979083 / 2.235840 / 2.462725 | 1.638700 / 1.719733 / 1.760274 |
| 256 B / 64 / miss | 1 | 2.158264 / 2.452915 / 2.991498 | 1.971404 / 2.213349 / 2.451877 | 1.648903 / 1.712963 / 1.796613 |
| 256 B / 4096 / hit | 0 | 2.288997 / 2.454296 / 2.790550 | 2.767771 / 2.849941 / 3.114253 | 3.614495 / 3.938322 / 10.355274 |
| 256 B / 4096 / hit | 1 | 2.284209 / 2.372752 / 2.505590 | 2.741933 / 2.778471 / 2.914747 | 3.535668 / 3.689140 / 3.891061 |
| 256 B / 4096 / miss | 0 | 2.659808 / 3.491720 / 6.074230 | 2.519240 / 2.582337 / 2.721896 | 1.968702 / 2.136628 / 2.536386 |
| 256 B / 4096 / miss | 1 | 2.524137 / 3.339122 / 5.743593 | 2.495816 / 2.580548 / 2.716362 | 1.971424 / 2.017046 / 2.160579 |

The unchanged all-participant cohort-stability gate remains part of
qualification. C is outside the Rust/C++ target denominator but inside this
gate; the large miss cells are invalid solely because C drifts above ten
percent. Their peer comparisons are already adverse, so dropping C would not
turn the miss ranges into passes. Cohort median drift and target verdicts:

| Payload / physical target / path | WF drift | Rust drift | C++ drift | C attribution drift | Target |
|---|---:|---:|---:|---:|---|
| 8 B / 64 / hit | 1.760% | 2.116% | 0.722% | 5.971% | pass |
| 8 B / 64 / miss | 7.759% | 1.871% | 4.785% | 7.052% | fail |
| 8 B / 4096 / hit | 3.534% | 2.533% | 4.476% | 1.651% | pass |
| 8 B / 4096 / miss | 2.624% | 0.358% | 2.016% | 10.838% | fail; drift-invalid |
| 256 B / 64 / hit | 1.540% | 0.472% | 0.358% | 0.734% | pass |
| 256 B / 64 / miss | 0.616% | 1.016% | 0.395% | 5.148% | fail |
| 256 B / 4096 / hit | 3.437% | 2.572% | 6.754% | 5.284% | pass |
| 256 B / 4096 / miss | 4.570% | 0.069% | 5.929% | 11.184% | fail; drift-invalid |

Both timing processes exit zero. Clock quantum is 41 ns, maximum empty
intervals are 84/83 ns, and the shortest real interval is 6.566833 ms; every
row passes duration and clock-overhead checks without subtraction. All 275
prerequisite pins remain unchanged. For all 96 width/capacity/path/seed groups,
receipt and complete cleanup agree across the four participants and both
cohorts. The 32 accounting rows balance requests/releases and finish at zero
live bytes; the entire allocation ledger is unchanged during each EDIT batch.
The real allocate/free falsifier checks request/release counts, so unchanged
net live bytes alone cannot conceal an allocation.

Prerequisites retain 288 persistent two-batch EDIT cases per image, the
existing lookup/reserve/whole-trace checks, and all 78 new semantic/clock
refusals. Three failed build-authoring attempts and the successful repair are
recorded; pre-repair source snapshots were not saved. The first reducer used
the wrong historical C participant label and failed before producing a result;
using the actual CSV label repaired it without rerunning data or changing a
threshold. Exact final sources, patch, criterion, raw/account rows, reducer,
logs, pin identities and compact native functions are in `isolated-edit/` in
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz). Original and
path-normalized hashes distinguish retained bytes; omitted toolchain/build
inputs and replay limits are explicit.

Reached WF hit loops update one word and carry the digest in registers, with
no per-key foreign call, Result materialization, digest reload/store, operation
dispatch, full-value copy, allocation or cleanup. Both complete batch bodies
have 248 instructions and 16-byte callee-save frames; these counts include
collision/empty paths, not just measured hit work. Power-of-two selection
remains inside the key loop, with arbitrary-capacity division fallback.
WF/C physical cells, Rust usable capacities and C++ buckets/nodes are checked
separately rather than claimed equal layouts.

Miss probing is now an exposed performance gap in this API consumer; it is
not evidence that a particular probe representation is selected. The roughly
1.9-ns hit API observation cannot be compared with the earlier roughly 3-ns
complete trace as an optimization: the timing boundaries and workloads differ.
No library/source optimization follows from this instrument, no historical
failure is reclassified, and the overall application target remains open.

### EDIT-only two-span probing: rejected

The isolated miss gap supplied a new, narrow discriminator after the
[historical five-function two-span trial](#prospective-two-span-cyclic-probing):
change only EDIT's cyclic continuation to `[home, count)` then `[0, home)` under
the integrated compiler and isolated API consumer. Hash-before-empty, arbitrary
capacity, equality order, tombstones, first update and complete callback/absence
outcomes stay unchanged. This does not reinterpret the earlier rejection.

The first source carrier was rejected for duplicate nominal declarations;
the next failed exact raw-module identity because graph qualification and
reachability differed. The retained carrier qualification then established
identical normalized reached EDIT bodies and callable boundaries for the
unchanged control, not whole-module identity. Both final control bodies match
the frozen 248-instruction baseline, and linked arm bodies match their object
streams. All 22 final check stages pass, including the maintained normal and
parallel HashMap witness against each exact local candidate module. The 32 EDIT
and 96 reserve allocator rows are byte-identical between arms.

Candidate bodies shrink from 248 to 182 instructions, while their frames grow
from 16 to 32 bytes. Wrap selection and the independent probe budget disappear;
no new inner calls, stack traffic or payload/digest copies appear. The collision
mode test re-enters each key iteration, and successful probes branch to a shared
update tail that recomputes the payload address. Eleven of 150 emitted functions
change; total decoded instructions rise from 11,305 to 11,373. These are observed
countervailing code-generation effects, not isolated causes of elapsed losses.

The fixed control-0/candidate-0/candidate-1/control-1 campaign retains all 1,536
rows, four direct zero exits, unchanged pins and matching complete outcomes.
Medians are ns/edit, cohort 0 / cohort 1; Rust/C++ columns are the candidate
panel. Complete min/max ranges and all C attribution observations remain in the
retained reduction:

| Payload / physical target / path | Control WF | Candidate WF | Rust | C++ | Before/after ranges, cohorts 0 / 1 |
|---|---:|---:|---:|---:|---|
| 8 B / 64 / hit | 1.851931 / 1.856759 | 2.323995 / 2.351547 | 2.268235 / 2.282987 | 2.136851 / 2.142057 | loss / loss |
| 8 B / 64 / miss | 2.470046 / 2.466152 | 2.772406 / 2.720555 | 2.215793 / 2.197405 | 1.717001 / 1.727243 | overlap / overlap |
| 8 B / 4096 / hit | 2.184664 / 2.147540 | 3.267194 / 3.441970 | 2.414862 / 2.511924 | 2.993912 / 3.135418 | loss / loss |
| 8 B / 4096 / miss | 3.283674 / 2.846251 | 4.249821 / 4.774173 | 2.627869 / 2.599438 | 1.738687 / 1.717508 | overlap / overlap (drift-invalid) |
| 256 B / 64 / hit | 1.884803 / 1.915574 | 2.362609 / 2.449150 | 2.539406 / 2.522697 | 2.206033 / 2.143184 | overlap / loss |
| 256 B / 64 / miss | 2.475326 / 2.662743 | 2.761414 / 2.788703 | 2.222304 / 2.218256 | 1.727412 / 1.715566 | overlap / overlap (drift-invalid) |
| 256 B / 4096 / hit | 2.402787 / 2.361283 | 3.405248 / 3.270944 | 2.952451 / 2.769147 | 3.860300 / 3.739009 | loss / loss |
| 256 B / 4096 / miss | 3.567537 / 3.639589 | 5.927468 / 5.563766 | 2.755125 / 2.565811 | 2.160837 / 2.065013 | overlap / overlap (drift-invalid) |

There are zero qualified miss gains: all eight miss-cohort ranges overlap and
all candidate miss medians are worse. Three hit cells have separated losses
in both cohorts and pass the instrument/stability conditions. Wide-small hit
has only a cohort-1 separated loss; cohort 0 overlaps. Thus four hit cells lose
in at least one cohort, not four qualified two-cohort losses. Candidate raw and
qualified selected-peer targets both fail all eight cells. Scalar-large miss,
wide-small miss and wide-large miss also retain drift/stability invalidation;
C remains inside the registered gates. The 18.916557-ns candidate scalar-large
hit and 14.573614-ns Rust wide-large miss outliers remain unfiltered.

The candidate is rejected, with no source adoption, retry or relaxed threshold.
Fewer continuation instructions did not qualify a gain in this consumer; no
broader conclusion about all two-span designs follows. The
`isolated-edit-two-span/` component of
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains prospective
criteria, exact both-arm sources and patch, failed carrier attempts, normal/
parallel checks, compact native/linked evidence, allocator parity, all raw rows,
reducers and pin identities. Replay needs the pinned integrated compiler and
baseline driver/peer/runtime dependencies; full binaries/IR are omitted.

### First-probe controls: rejected at the native gate

This native-only discriminator preserves the direct first enum-slot probe before
reading compact fingerprint controls. It replaces optional control ownership
with an inline `Box<Slots<u64>>` descriptor and tail word, and carries one checked
extent into continuation. The final source admits under the unchanged compiler;
all six emission/native-inspection stages exit zero. Earlier formatting,
direct-return postcondition and private-constructor allocation-bound failures
remain recorded. The direct-return limitation already has an existing TODO;
this trial changes no compiler or public capacity/refusal rule.

Continuation achieves one complete-group extent check followed by direct word
loads, without optional-owner/header indirection or repeated storage-length
fallback. Tail classification and actual enum validation before payload access
remain. The first-hit payload/digest path stays register-only and reads no
metadata. Nevertheless the candidate newly stores the round count and reloads
it each outer round (`5a7c`/`5a88` scalar, `62f4`/`6300` wide), violating the
prospective no-new-spill gate. This is not a per-key digest spill. Both hash mix
constants also move from outside the loops into every normal-hash key path.
An unchanged first-probe source shape therefore does not establish unchanged
first-hit native cost.

| Complete EDIT body, either width | Control | Candidate |
|---|---:|---:|
| Decoded instructions | 248 | 542 |
| Frame bytes | 16 | 112 |
| Calls | 0 | 0 |

The experimental descriptor grows from 40 to 72 bytes. Control backing adds
`8 * floor(capacity / 8)` heap bytes and one allocation whenever capacity is at
least eight, including capacity eight; setup initializes full words and the
tail. Whole-object decoded instruction bytes grow from 45,220 to 54,832;
these are not linked-image sizes. Fingerprint filtering also skips some equality
callbacks under the consistent-key protocol. Ownership, callback, full-capacity
and hostile-protocol execution witnesses have not run for this representation.

The native failure stops the trial: no linked correctness qualification,
timing, measured slowdown, rescue variant or representation adoption follows.
The `first-probe-controls/` component of
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains both source
graphs and patch, prospective criterion, all failed attempts, direct statuses,
compact object functions, counts and original/normalized pins. Full modules and
binaries are omitted with explicit historical replay limits. The achieved extent
simplification is retained as evidence; it does not override the failed gate.

### First-probe peel: native pass, timing rejection

This EDIT-only source trial isolates first-empty enum dispatch, without the
rejected controls representation. Both baseline native bodies reproduce the
unchanged carrier. Peeling the first probe removes two tag compares and one
conditional branch, adds an unconditional branch, and preserves bounded
`count - 1` continuation. The narrow native gate passes, but complete bodies
grow 248→276 instructions and frames 16→32 bytes; hash-mode testing moves into
each key iteration and first-hit address calculation is repeated. These effects
are disclosed, not assigned elapsed shares.

Ordinary qualification is 22 full-check stages plus 12 capacity-one normal/
parallel stages, all zero; raw LLVM is compiled unchanged. The supplement fills
a capacity-one coverage gap with vacant/unequal/tombstone misses, hit/update,
removal, exact update-callback count and complete consumed values. Exactly-one
equality on the unequal case is structural source/native evidence, not a runtime
counter claim. Earlier modified-IR equality instrumentation is retained as
discarded and supplies no ordinary qualification. The 32 EDIT and 96 reserve
accounting rows remain identical between arms.

One fixed four-process campaign retains 1,536 rows, unchanged pins and matching
complete outcomes. Medians are ns/edit, cohort 0 / cohort 1; Rust/C++ are the
candidate-panel observations. Complete ranges and stability calculations are
retained in `first-probe-peel/` in
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz).

| Payload / physical target / path | Control WF | Candidate WF | Rust | C++ | Before/after ranges, cohorts 0 / 1 |
|---|---:|---:|---:|---:|---|
| 8 B / 64 / hit | 1.858617 / 1.866803 | 2.054622 / 2.052069 | 2.302527 / 2.276346 | 2.162193 / 2.143507 | overlap / overlap |
| 8 B / 64 / miss | 2.468020 / 2.526497 | 2.506296 / 2.616430 | 2.292285 / 2.199074 | 1.732340 / 1.729056 | overlap / overlap |
| 8 B / 4096 / hit | 2.202834 / 2.197762 | 2.880832 / 2.923906 | 2.431884 / 2.424563 | 3.006160 / 3.081366 | loss / loss |
| 8 B / 4096 / miss | 3.283158 / 3.285800 | 3.867725 / 3.548553 | 2.612496 / 2.725090 | 1.655718 / 1.768226 | overlap / overlap (invalid) |
| 256 B / 64 / hit | 1.839027 / 1.928712 | 2.072886 / 2.247085 | 2.562627 / 2.559801 | 2.146607 / 2.168268 | loss / overlap |
| 256 B / 64 / miss | 2.495895 / 2.636229 | 2.489597 / 2.458453 | 2.295112 / 2.229060 | 1.728431 / 1.726573 | overlap / overlap (invalid) |
| 256 B / 4096 / hit | 2.396683 / 2.472326 | 2.902965 / 2.712195 | 2.829711 / 2.799630 | 3.900697 / 3.801907 | loss / loss (invalid) |
| 256 B / 4096 / miss | 3.576716 / 3.352419 | 2.909541 / 2.566030 | 2.662808 / 2.577449 | 2.213289 / 2.052550 | overlap / overlap (invalid) |

Every miss range overlaps in both cohorts: zero qualified miss gains. Only
scalar-large hit has an instrument-qualified two-cohort loss. Wide-large hit
also loses both raw ranges but is invalid at 10.412 percent paired-WF ratio
spread; wide-small hit loses only cohort 0, while scalar-small hit overlaps.
Candidate raw peer targets pass 1/8 cells, but paired-qualified targets pass
0/8. Large scalar miss and both wide misses retain drift failures, including
C attribution. Wide-large miss median reductions of 18.653/23.457 percent do
not overcome overlapping ranges and invalidity. The 17.930945-ns Rust
scalar-large miss outlier remains. All clock/duration gates pass (minimum
6.583625 ms); those checks do not repair stability failures.

The fixed criterion rejects the source, with no retry, rescue or adoption.
Exact graphs/patches, the ordinary capacity-one supplement, discarded-instrument
history, native excerpts, statuses, ledgers, raw rows and reducers are retained
with original/normalized pins and omitted-build replay limits.

### Fixed-hash sensitivity: supplemental rejection

Separate fixed salted-mix64 owner families remove collision mode in WF, Rust,
C++ and C while preserving the original consumer families. Both map source
arms remain the previously tested original/peeled implementations. Fixed WF
bodies are 169/203 instructions with 16/32-byte frames, versus original
248/276; no new hot spills or calls appear. This is not pure branch-cost
attribution: consumer-owner sizes change WF 72→64, Rust 64→56, C++ 72→64 and
C 40→32 bytes, and control hoists capacity dispatch while the peeled arm keeps
it per key. C++ hasher traits/signature remain unchanged.

All 15 ordinary check stages exit zero. The 32 EDIT and 96 reserve accounting
rows match both arms and the original control byte-for-byte. Prior ordinary
capacity-one witnesses were not rerun because map bytes are unchanged; no
discarded instrumentation is used. The fixed four-process campaign retains
1,536 rows, unchanged 47 pins, matching complete outcomes and minimum interval
6.421792 ms. Medians below are ns/edit, cohort 0 / cohort 1; Rust/C++ are the
peeled-panel observations, with complete ranges retained in the archive.

| Payload / physical target / path | Control WF | Peeled WF | Rust | C++ | Before/after ranges, cohorts 0 / 1 |
|---|---:|---:|---:|---:|---|
| 8 B / 64 / hit | 1.825934 / 1.830861 | 1.894891 / 1.861076 | 2.259696 / 2.250249 | 1.832207 / 1.824121 | overlap / overlap |
| 8 B / 64 / miss | 2.417927 / 2.398168 | 2.392496 / 2.400756 | 1.695936 / 1.689807 | 1.691192 / 1.688336 | overlap / overlap |
| 8 B / 4096 / hit | 1.967256 / 1.984914 | 2.172565 / 2.155681 | 2.380898 / 2.354965 | 2.667844 / 2.579585 | loss / loss |
| 8 B / 4096 / miss | 3.082454 / 2.689600 | 2.353927 / 2.398819 | 2.265910 / 2.276113 | 1.587992 / 1.629556 | overlap / overlap (invalid) |
| 256 B / 64 / hit | 1.874998 / 1.835719 | 1.870990 / 1.861567 | 2.420475 / 2.438987 | 1.814614 / 1.809831 | overlap / overlap |
| 256 B / 64 / miss | 2.478326 / 2.514501 | 2.591625 / 2.419919 | 1.710450 / 1.693209 | 1.696552 / 1.687268 | overlap / overlap |
| 256 B / 4096 / hit | 2.217700 / 2.204155 | 2.391746 / 2.355864 | 2.748539 / 2.718826 | 3.426522 / 3.364414 | overlap / loss |
| 256 B / 4096 / miss | 4.715760 / 5.140781 | 2.488693 / 2.475873 | 2.254441 / 2.268950 | 2.018680 / 2.578497 | gain / overlap (invalid) |

There are zero qualified miss gains. Scalar-large hit loses both cohorts;
wide-large hit loses cohort 1 and overlaps cohort 0. Both large misses are
invalid: scalar includes control WF/C drift and paired-WF ratio spread,
while wide includes control C, candidate C++ and interarm peer drift. Wide-large
miss's cohort-0 raw gain does not qualify; cohort 1 overlaps. All outliers
remain, including candidate WF 10.216047 ns and Rust 14.756958 ns in wide-large
miss. Supplemental candidate peer targets pass 3/8 cells, all hits; no miss
passes. These new-consumer observations do not substitute for original-image
qualification or establish application performance. No adoption or retry follows.

The `fixed-hash-sensitivity/` archive component retains exact coherent owner/
driver patches, source graphs/audit, authoring failures, native/ordinary checks,
ledgers, every raw row, reducers and dependency pins. Earlier native-stage
no-execution wording precedes the separately recorded checks and timing.

### Byte-pack load: native-only result

A separate arbitrary-byte consumer packs eight checked bytes numerically before
scalar equality masking. The guarded wrapper's optimized LLVM has one `i64`
load with alignment 1; the bounded helper still has eight alignment-1 `i8`
loads which ARM64 instruction selection combines. Both native bodies use one
payload load, no calls or stack: helper 16 instructions, wrapper 22. The C
control has 25 instructions with an explicit zero-mask guard. Static counts
are not timing or runtime correctness evidence.

The initial preparation failed to retain the wrapper because `main` did not
reach it; that attempt is preserved. The repaired arbitrary-input external
wrapper remains defined, and all eight native command stages exit zero.
`byte-pack-load/` retains source, criterion, raw/optimized/native excerpts,
failed preparation and pins. This establishes a compact lowering for this
consumer only: no map metadata maintenance, ownership, first-hit pressure,
allocation cost, runtime correctness or speed is qualified. No map/compiler
change or representation selection follows. Both components are in
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz), with normalized/original
hashes and explicit omitted-build replay limits; no unfinished map prototype
is included.

### Mirrored-byte sequence: bounded checks, rejected timing

Four source stages retain a fused metadata load while changing first-probe,
home-group, empty-stop and progress-loop handling. Native bodies count
380→288→312→300 instructions, all with 112-byte frames and zero calls, versus
flat control's 248 instructions/16-byte frame. The first two strict native
gates fail. Empty-stop removes the vacant enum read; progress loops remove
redundant counters while preserving actual guards. The later prospective
frequency-based diagnostic is separate, not a retroactive pass: frame cost
is per batch, a saved round limit reloads once per round, and per-key hash
constant rematerialization remains. Static reductions do not measure those costs.

The final ordinary checks have 23 full-check, 12 boundary and 6 early-empty/
refusal stages, all zero. A source fault omitting mirrored-suffix writes emits
and links successfully, then exits 32 as expected. These witnesses cover
selected ownership, callback, wrap, tombstone and growth cases, not generic
correctness. Metadata adds a `C+7`-byte allocation at supported positive
capacity; map/consumer-owner sizes grow 40→64/72→96 bytes. The 32 EDIT and
96 reserve ledger rows match the independent allocation formulas; non-WF
rows remain identical.

The fixed four-process campaign retains all 1,536 rows. Medians are ns/edit,
cohort 0 / cohort 1; Rust/C++ columns come from the mirrored panels. Complete
ranges, drift gates and all original observations are retained in the archive.

| Payload / physical target / path | Flat WF | Mirrored WF | Rust | C++ | Raw before/after ranges, cohorts 0 / 1 |
|---|---:|---:|---:|---:|---|
| 8 B / 64 / hit | 1.870950 / 1.856168 | 5.160684 / 5.354052 | 2.270525 / 2.297078 | 2.132341 / 2.162233 | loss / loss (invalid) |
| 8 B / 64 / miss | 2.489383 / 2.474442 | 3.338123 / 3.353010 | 2.207701 / 2.210170 | 1.720970 / 1.728063 | loss / loss |
| 8 B / 4096 / hit | 2.194608 / 2.173598 | 4.970774 / 4.944796 | 2.411430 / 2.414013 | 3.080924 / 3.076409 | loss / loss |
| 8 B / 4096 / miss | 3.205106 / 2.995297 | 3.740370 / 3.725355 | 2.648458 / 2.582863 | 1.742030 / 1.674707 | overlap / overlap (invalid) |
| 256 B / 64 / hit | 1.879046 / 1.869609 | 5.233988 / 5.187407 | 2.567276 / 2.540116 | 2.163058 / 2.136002 | loss / loss |
| 256 B / 64 / miss | 2.501791 / 2.498334 | 3.405486 / 3.364960 | 2.414977 / 2.300585 | 1.755908 / 1.735201 | loss / loss |
| 256 B / 4096 / hit | 2.390251 / 2.417028 | 5.408322 / 5.295197 | 2.896592 / 2.835100 | 3.857305 / 3.767138 | loss / loss |
| 256 B / 4096 / miss | 3.536309 / 3.166124 | 3.723482 / 3.718148 | 2.614945 / 2.591809 | 2.045631 / 2.049997 | overlap / overlap (invalid) |

There are zero qualified miss gains and zero of eight peer targets. All four
hit cells and both small misses have raw separated losses in both cohorts;
scalar-small hit is invalid from C cohort/interarm drift, leaving three
qualified hit losses and two qualified small-miss losses. Both large misses
overlap and are invalid: scalar has C cohort drift; wide has flat-WF cohort
and paired-WF ratio drift. No retry, adoption or application-level conclusion
follows.

The `mirrored-byte-sequence/` component in
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) preserves sources,
criteria, authoring failures, checks, compact native excerpts, raw rows,
reducers and original/normalized pins. It also retains the initial criterion's
Deleted=255 versus actual source=254 discrepancy and corrects the first
native report's `+0x40` field attribution to environment salt, not metadata.
Replay requires the pinned omitted compiler/runtime/toolchain dependencies;
no binaries, full IR or unfinished follow-up are included.

### Matches-first order: native improvement, timing rejection

Moving candidate matches before the empty test passes the prospective native
order criterion against the previous mirrored progress-loop source. Ready-hash
first-hit work falls 52→40 instructions and no-match first-empty work 31→28;
an earlier empty followed by one false fingerprint grows 40→53. Bodies fall
300→297 instructions, retaining 112-byte frames, zero calls and per-key hash
constant materialization. These are source-order effects, not a generic
compiler speculation rule. The timed baseline is original flat probing, so
the campaign does not isolate the runtime contribution of empty-check order.

Ordinary dictionary results require stable, consistent hash/equality laws and
returning callbacks. A retained ordinary counterexample stores key 2/hash 2/
value 20, then queries key 0/hash 0 with always-true equality: flat returns
`Err` with final value 20; matches-first returns `Ok` with final value 21.
Both safely consume the stored key once. Extra equality calls may allocate
or diverge; no universal callback-order or termination equivalence is claimed.

The candidate has 12 full-check, 6 boundary and 3 lawful early-empty stages,
all zero, plus 9 expected hostile/fault stages including mirror-write omission
exit 32. Original-flat images/checks are reused byte-identically. The 32 EDIT
and 96 reserve accounting rows match the predicted extra metadata allocation;
non-WF rows remain identical. These are bounded witnesses, not general proof.

All four fixed timing processes exit zero; 1,536 rows, complete same-seed
outcomes and 696 unchanged input pins are retained. There are zero qualified
miss gains and zero of eight raw or qualified Rust/C++ peer targets. Scalar
64/4096 and wide 4096 hits have qualified separated losses in both cohorts;
wide 64 hit overlaps/loses, and scalar 64 miss loses/overlaps. Remaining
miss ranges overlap. Both large misses are invalid from control-WF and C
cohort/interarm drift and paired-WF ratio drift; they establish no improvement.
No filtering, retry, adoption or production change follows.

The `mirrored-byte-matches-first/` component in
[reserve-api-evidence.tar.gz](reserve-api-evidence.tar.gz) retains exact sources,
criteria, native paths, ordinary counterexample/fault outcomes, all raw rows,
complete WF/Rust/C++/C ranges, reducers and original/normalized dependency pins.
Native-stage no-execution statements precede the later authorized checks and
timing. Pinned compiler/runtime/toolchain binaries and full generated modules
remain omitted; this is not a standalone replay bundle.
