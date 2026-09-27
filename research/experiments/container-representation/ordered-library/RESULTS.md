# Ordered map library costs

This explicit experiment compares the complete boxed-node B-tree in
[`ordered-map.wf`](../../../../lib/std/collections/ordered_map/ordered-map.wf), source-shaped C,
direct C, and a complete native AVL. The prospective criterion is
[Ordered map trial at v0.68](../../../investigations/containers-and-resources/X1-LIBRARY.md#ordered-map-trial-at-v068),
committed before implementation at `5f10d1d3a`. The complete matched matrix
passed on 2026-09-23 at library revision `06f9a6a7c`. Its costs depend on payload
and workload: the boxed B-tree is not a default representation winner.
The [raw samples](measurements.csv), [identities](identities.txt), and
[structural counts](structural-paths.csv) preserve the complete evidence.
Retire or supersede this directory when a maintained comparison replaces this
contract and preserves its evidence. Its Makefile is its explicit caller;
these targets have no daily gate or CI dependency.

## Contract and construction

The payloads are 16-byte scalar pairs and 264-byte inline pairs. Counts 8, 256,
and 4096 cover one leaf and multiple B-tree levels. Initial insertion and each
round visit ranks `(73 * index) % count`; all selected counts are coprime to
73. Keys are `2 * rank + 1`. Work consists of complete build/cleanup, balanced
hit/miss lookup, replacement/edit/remove/reinsert churn at fixed cardinality,
and half-open ranges spanning at most 16 keys. The library and both C trees
share the 8192-entry logical ceiling; replacement is permitted at the ceiling.
Native AVL implements the same ownership outcomes and operation callbacks.

The matched source control now uses the admitted bundled-entry candidate:
one leading zero-or-one link and 15 entries, each holding a pair and its
following zero-or-one link. It preserves the replacement presearch, recursive
owned Pair insertion, bottom-up promotion, seven reverse/take transfers and
three entry swaps when splitting, detached-separator underflow repair,
successor-only internal deletion, and consuming cleanup order. Direct C keeps
a compact two-array node with top-down split/repair and contiguous transfers.
AVL retains one allocation per pair and height-based rotations. The direct
controls therefore compare complete ordinary alternatives. Source C matches
the argument shapes, layout, branches and transfers; the comparison includes
the languages' different inferred alias facts, as qualified below.

| Representation | Scalar node bytes | Wide node bytes |
| --- | ---: | ---: |
| WF / source C bundled B-tree | 504 | 4224 |
| Direct C B-tree | 376 | 4096 |
| Native AVL | 40 | 288 |

These extents passed both `sizeof` assertions and the exact allocation
observer. They exclude allocator bookkeeping.

Every observed payload word contributes to an ordered digest. Every returned
or finally consumed pair contributes to a separate sum, mixed parity and
count, because final cleanup order is unspecified. The independent oracle
uses a sorted flat sequence and binary search; it has no tree shape or
rebalancing code. The maintained owning caller supplies the separate nodrop
ownership and hostile-comparator checks; inline scalar/record digests are not
a claim of exact resource-identity accounting.

Normal and retained modes use the same sources. Retained mode keeps only the
public operations and supplied callbacks out of line in both languages;
private node, payload-content, result-processing and digest helpers remain
optimizable. The native controls are separately specialized for each payload,
as are the WF interface instances. No runtime comparator dispatch or payload
stride branch is introduced. Saved optimized IR supports attribution.
The harness gives WF the same closed helper scope as C's static functions:
only the two measured WF trace bridges stay exported. Its LLVM adaptation
changes helper linkage, allocator observation names/facts, launcher naming and
the stated public `noinline` attributes; it changes no algorithm instructions.
Tree-shape validation and structural counters run only in correctness/audit
commands. No extra bounds assertion is present in a timed C tree helper.

All node allocations pass through the same counter and retain allocator
`noalias nonnull` information. The observer's allocation header is excluded
equally from requested bytes. Each request must equal that representation's
node size; all nodes and requested bytes must be released after every trace.
The WF and source-shaped C allocation count, requested bytes and peak storage
must agree. Direct C and AVL report their own storage. Peak-node utilization
is `count / (15 * peak_nodes)` for B-trees and 1 for AVL; this is a reserved
capacity measure, not a per-node occupancy histogram.

## Reproduction and budgets

The original 1,152-row replay is owned by revision `1811f2d01`, which preserves
the four-path driver, original samples and identities. Run the following
commands from a checkout of that revision. The current driver has the fifth
replacement-only path; its separate reconstruction/check/comparison commands
appear in the candidate section below.

Run construction, correctness and timing separately under the shared guard:

```sh
WHITEFOOT_CHECK_TIMEOUT=120 perl .github/run-check.pl ordered-cost-build \
  make -C research/experiments/container-representation/ordered-library build \
  WHITEFOOTC=/path/to/the/recorded/whitefootc
WHITEFOOT_CHECK_TIMEOUT=40 perl .github/run-check.pl ordered-cost-check \
  make -C research/experiments/container-representation/ordered-library verify
WHITEFOOT_CHECK_TIMEOUT=60 perl .github/run-check.pl ordered-cost-measure \
  make -C research/experiments/container-representation/ordered-library measure-only
make -C research/experiments/container-representation/ordered-library identities \
  WHITEFOOTC=/path/to/the/recorded/whitefootc \
  WHITEFOOTC_LABEL=frozen-v0.68-whitefootc
```

Compiler construction and the canonical repository gate have separate costs.
Investigate an exceeded stage before extending its budget or running it again.
Timing consists of one warm-up (sample 0) and five paired samples (1--5), with
implementation order reversed on alternating samples. Build/cleanup batches
4096 entries across whole traces; other paths run `8192 / count` rounds in a
single complete trace. Report these as amortized whole-trace costs, including
build, full ordered visitation and cleanup, without subtracting setup to claim
isolated operation latency. No additional samples were run for these unchanged
images; a changed candidate needs its own prospective comparison criterion.

## Construction and correctness

The measured compiler is the frozen v0.68 binary with SHA-256
`cbffd4dd1ae8641ef03790457181188988bf70cc4af1a53c50c1f406307bb7f9`.
Native compilation used Apple Clang 21.0.0 (`clang-2100.3.34.2`), `-O2`, on
arm64 Darwin 25.6.0. The identity file records all measured sources, generated
IR and executable hashes. No compiler or specification change was needed.
Published identity labels omit hostnames and local installation/source paths.
The original measured Makefile hash is
`0fd32a941646865a41d702adf41399406c9b838a2ea37da35bd28850b08a5a00`;
the identity file separately records that revision's Makefile after its
metadata-only portable-label recipe change. That change modifies no measured
C/WF source, LLVM instruction, executable or sample; timings were not rerun.

| Stage | Command seconds | Guard seconds | Budget seconds |
| --- | ---: | ---: | ---: |
| Complete matched construction | 5.12 | 5.15 | 120 |
| Initial matched verification | 1.66 | 1.73 | 40 |
| Final C aggregate-move alignment rebuild | 3.74 | 3.80 | 120 |
| Final matched verification | 1.45 | 1.50 | 40 |
| Single complete timing matrix | 1.67 | 1.74 | 60 |

Three earlier cost-caller admission attempts took 0.54, 0.05 and 0.05 seconds
(0.64, 0.11 and 0.11 guarded). They exposed obsolete `own` type modifiers,
enum binder freshness, and an incomplete forwarded interface application;
all were corrected in the caller before the successful construction. The
initial provisional two-array native-only construction/check took 2.66/1.40
seconds and was superseded by the bundled source control. No stage exceeded
its budget. No timing sample was collected before final alignment and checks,
and no sample was added afterward.

Each normal and retained executable passed 264 sorted-oracle configurations
through all four implementations: 1,056 executions per mode. Each also passed
six 8192-entry native audits of ordering, occupancy, equal B-tree leaf depth,
AVL balance/height, cardinality, replacement/refusal at the ceiling, exact
returned payloads, complete removal, and empty-root reuse. Every trace released
all allocations; WF and source C matched the complete allocation-accounting
record. Both timed modes independently checked every sample against the oracle
and checked the same allocation agreement outside each measured interval.

A separate source-control build counts structural branches; timed builds
contain no such counters. It follows the maintained scalar trace: insert
16--151 ascending and 15--8 descending; remove 40; insert 7; remove 112 and 79;
then remove `7 + (67 * step) % 145`, skipping the three already removed keys,
and reuse the empty map. Independent shape validation follows each removal.

| Structural event | Count |
| --- | ---: |
| Root / nonroot split | 2 / 16 |
| Borrow left, leaf / internal | 23 / 1 |
| Borrow right, leaf / internal | 9 / 1 |
| Merge, leaf / internal | 17 / 1 |
| Internal successor replacement | 19 |
| Root contraction onto a child | 2 |

The algorithm has no predecessor replacement; its recorded count is zero.
The contraction counter counts height reductions onto a nonempty child;
the final empty leaf release is checked by empty-root reuse and accounting.
These are observed branches of the final matched source C, supporting the
trace's structural attribution. They are not direct instrumentation of WF.
The [maintained owning caller](../../../../tests/programs/containers/ordered-map-program.wf)
separately verifies every allocation and each payload serial exactly once,
including owned edit results, refusal/retry, and hostile-comparator progress.

## Timings

Values below are medians of the five measured samples, in nanoseconds per
inserted pair for build/cleanup or per round/key step for other paths. A lookup
step contains one hit and one miss; churn contains replacement, edit, removal
and reinsertion. Range steps visit at most 16 pairs. Every value includes its
amortized complete build, final ordered visit and cleanup. These are not
isolated-operation timings. `B`, `L`, `C`, `R` denote those four paths.

**Normal optimization.**

| Pair bytes | Count | Path | WF | Source C | Direct C | AVL C |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 16 | 8 | B | 19.5 | 18.3 | 13.2 | 39.1 |
| 16 | 256 | B | 34.7 | 34.7 | 22.5 | 47.9 |
| 16 | 4096 | B | 81.3 | 87.4 | 70.3 | 74.7 |
| 16 | 8 | L | 6.8 | 10.3 | 5.9 | 4.9 |
| 16 | 256 | L | 22.9 | 29.9 | 16.6 | 13.3 |
| 16 | 4096 | L | 109.7 | 120.6 | 100.7 | 58.6 |
| 16 | 8 | C | 33.4 | 32.1 | 25.4 | 41.9 |
| 16 | 256 | C | 121.1 | 124.9 | 87.9 | 113.0 |
| 16 | 4096 | C | 210.8 | 222.7 | 212.9 | 202.8 |
| 16 | 8 | R | 12.3 | 12.3 | 7.3 | 13.1 |
| 16 | 256 | R | 54.8 | 57.0 | 29.4 | 50.5 |
| 16 | 4096 | R | 113.3 | 115.8 | 90.1 | 131.1 |
| 264 | 8 | B | 103.8 | 86.4 | 56.2 | 74.5 |
| 264 | 256 | B | 197.0 | 161.4 | 100.6 | 87.2 |
| 264 | 4096 | B | 240.7 | 210.4 | 151.9 | 129.2 |
| 264 | 8 | L | 19.4 | 19.8 | 20.0 | 22.6 |
| 264 | 256 | L | 32.8 | 33.9 | 27.3 | 35.8 |
| 264 | 4096 | L | 202.3 | 199.5 | 152.5 | 120.8 |
| 264 | 8 | C | 194.1 | 179.9 | 127.2 | 111.5 |
| 264 | 256 | C | 303.7 | 282.7 | 208.9 | 207.8 |
| 264 | 4096 | C | 543.6 | 497.3 | 352.1 | 317.3 |
| 264 | 8 | R | 72.4 | 71.7 | 71.3 | 70.4 |
| 264 | 256 | R | 272.2 | 268.9 | 263.2 | 259.4 |
| 264 | 4096 | R | 414.7 | 400.6 | 374.1 | 380.6 |

**Retained optimization.**

| Pair bytes | Count | Path | WF | Source C | Direct C | AVL C |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 16 | 8 | B | 25.6 | 31.0 | 21.2 | 41.0 |
| 16 | 256 | B | 53.5 | 61.3 | 38.8 | 51.8 |
| 16 | 4096 | B | 107.9 | 123.0 | 95.2 | 85.0 |
| 16 | 8 | L | 17.1 | 20.5 | 15.9 | 9.4 |
| 16 | 256 | L | 59.4 | 66.9 | 52.2 | 36.7 |
| 16 | 4096 | L | 157.1 | 170.2 | 147.0 | 98.5 |
| 16 | 8 | C | 47.1 | 64.7 | 45.4 | 51.8 |
| 16 | 256 | C | 179.9 | 200.8 | 142.6 | 141.2 |
| 16 | 4096 | C | 290.3 | 321.0 | 299.7 | 244.5 |
| 16 | 8 | R | 21.6 | 21.2 | 16.0 | 26.0 |
| 16 | 256 | R | 95.8 | 94.8 | 74.5 | 113.0 |
| 16 | 4096 | R | 161.5 | 168.5 | 138.8 | 184.0 |
| 264 | 8 | B | 91.8 | 89.6 | 56.6 | 78.6 |
| 264 | 256 | B | 189.0 | 184.1 | 114.3 | 98.6 |
| 264 | 4096 | B | 239.0 | 243.4 | 169.4 | 140.9 |
| 264 | 8 | L | 22.9 | 23.4 | 22.0 | 24.8 |
| 264 | 256 | L | 75.0 | 77.9 | 69.1 | 56.5 |
| 264 | 4096 | L | 236.8 | 241.9 | 195.9 | 164.3 |
| 264 | 8 | C | 208.6 | 184.7 | 128.2 | 120.7 |
| 264 | 256 | C | 345.8 | 347.3 | 242.4 | 261.2 |
| 264 | 4096 | C | 633.8 | 612.5 | 447.9 | 402.7 |
| 264 | 8 | R | 73.1 | 73.2 | 72.5 | 73.2 |
| 264 | 256 | R | 293.0 | 292.5 | 287.6 | 290.5 |
| 264 | 4096 | R | 439.6 | 439.7 | 400.0 | 410.2 |

Within-sample WF/source-C ratios range 0.67--1.07 for normal scalar and
0.95--1.20 for normal wide; retained ranges are 0.73--1.01 and 0.95--1.13.
These are ranges of each cell's median paired ratio, not ratios of separately
selected best runs. The largest relative WF five-sample spread occurs in retained
scalar lookup at count 256 (55.1--71.7 ns/step); its source-control ratio remains
at or below one in every pair. The matrix supports workload-level differences;
one-percent differences do not establish an ordering.

At 4096 wide pairs, normal WF build/cleanup and churn cost 1.85x and 1.71x the
native AVL; retained costs are 1.70x and 1.54x. Ordinary representation and
movement choices therefore dominate the small matched-lowering difference.
At eight scalar pairs, normal WF build/cleanup and churn cost 0.51x and 0.80x
AVL, and scalar range at 4096 costs 0.87x AVL. The complete native AVL is a
useful alternative rather than evidence that every tree should be binary.
No WF AVL was implemented, so its WF cost is not established by this floor.

## Allocations and reserved storage

Counts and occupancy are independent of payload width and mode for these
streams. Build requests below are per complete trace; churn requests cover
all 8192 round/key steps plus construction. Lookup and range preserve build
storage. `WF` also describes source C, which agrees exactly.

| Count | Path | WF requests / peak nodes | Direct requests / peak nodes | AVL requests / peak nodes | WF / direct peak utilization |
| ---: | --- | ---: | ---: | ---: | ---: |
| 8 | Build | 1 / 1 | 1 / 1 | 8 / 8 | 53.3% / 53.3% |
| 256 | Build | 31 / 31 | 31 / 31 | 256 / 256 | 55.1% / 55.1% |
| 4096 | Build | 363 / 363 | 365 / 365 | 4096 / 4096 | 75.2% / 74.8% |
| 8 | Churn | 1 / 1 | 1 / 1 | 8200 / 8 | 53.3% / 53.3% |
| 256 | Churn | 36 / 33 | 93 / 33 | 8448 / 256 | 51.7% / 51.7% |
| 4096 | Churn | 563 / 562 | 366 / 366 | 12288 / 4096 | 48.6% / 74.6% |

Multiply requests or peak nodes by the verified node extent to obtain requested
or peak bytes; both quantities also appear unrounded in every raw sample.
For 4096-pair churn, WF reserves 283,248 scalar / 2,373,888 wide bytes at peak;
direct C reserves 137,616 / 1,499,136; AVL reserves 163,840 / 1,179,648.
Bottom-up repair plus reinsertion changes this stream's WF occupancy from
75.2% at construction to 48.6% peak utilization. Native AVL pays one allocation
per reinsertion but does not reserve vacant pair slots. Fewer allocations alone
does not establish lower reserved storage or lower elapsed cost.

## Transfer and generated-code attribution

No full node is copied on ordinary search/insertion/removal descent. Both
source implementations retain the independent replacement presearch and the
owning pair argument on insertion: 16 or 264 payload bytes travel at each
recursive level. The direct AVL instead borrows its offered pair while
descending. The source split transfers seven upper entries, reverses them with
three swaps, extracts one median and inserts one carried entry; each entry is
32 or 280 bytes, including its 16-byte link. Insert/remove shifts and merge
append stay linear in fanout. Sibling borrowing exchanges complete pairs and
links locally, without relocating unrelated nodes. Cleanup visits each pair
once and each node once, independently of comparison results.

The saved optimized IR exposes two remaining lowering costs, not new runtime
bounds checks. Wide WF `ordered_map_insert_item$instance$118` has 482 static
loads, 498 stores and 28 explicit 280-byte memcpy call sites; source C's private
`record_source_insert_item` has 6 loads, 18 stores and 21 such call sites, plus
bulk node/window transfers. WF expands the node-to-Box construction into many
field transfers, while Clang retains bulk copies. These are static IR counts
over multiple branches, not dynamic bytes moved or an isolated causal timing.

The alias assumptions are not identical. Retained WF lookup/edit/remove
parameters include `noalias` and `dereferenceable` facts for borrowed map,
key and callback-environment pointers. Source C uses ordinary pointers without
`restrict`; its corresponding optimized parameters lack `noalias`. C does
carry `noalias` on aggregate-result storage, and both languages retain fresh
allocation facts. This is a comparison of matched source algorithms with their
ordinary language/compiler facts, not a lowering-only comparison holding all
alias metadata constant. The metadata can affect optimization, but this dataset
does not isolate its timing contribution; it cannot assign the entire WF/C
ratio to copying or alias information alone. The frozen controls were not
rewritten after this read-only audit.

Result representation is also qualified. WF payload enums use a four-byte
tag; source C uses a byte tag. Put/Taken/Promotion keep the same aggregate
extent and pair/entry offset, but Put's reason is at offset four in WF and one
in C. WF `Result<u64,unit>` reserves 24 bytes (`i32`, `i64`, `i8`), whereas the
C Lookup result reserves 16 and omits the unit byte. Thus source C is matched
to the algorithm, node layout and ownership-transfer shapes, not an identical
result ABI. These frozen measurements cannot isolate exact-ABI lowering cost.
This limitation does not differ between the two WF arms in the discriminator
below; their native normalization controls remain unchanged.

Both WF modes also retain one 504/4224-byte memcpy per exhausted node during
cleanup before releasing that Box and recursively consuming its leading link.
Source C spells the same final aggregate move, but Clang removes unused fields
and carries only the link. This copy occurs once per released node, not at each
search level, and does not make cleanup quadratic. A consumer of the leading
subobject should need only its 16 bytes; validating that optimization requires
an isolated compiler/source experiment beyond this matrix.

Retained WF has exactly 29 explicitly marked public operations/callback
instances (18 operations and 11 callbacks); private helpers remain unmarked.
Its optimized IR retains 24 public-operation call sites; each C specialization
retains 19 including correctness-only callers. C control tree helpers contain
zero executable proof-only assertions. Logical-ceiling, vacancy, order and
underflow branches remain algorithm work; allocation extent/identity/host
failure checks belong to the shared observer in every implementation. No
extra bounds guard is charged only to a C retained helper.

**Design suitability.** The complete ordinary boxed implementation satisfies
the ownership and progress criterion and provides a reusable consumer for
lowering work. This matrix does not select it as the default map: direct C and
AVL expose material wide-pair transfer and occupancy costs, while scalar and
range results are mixed. The already recorded single-descent insertion and
borrowed owning-pair carrier are concrete ordinary alternatives; no new proof
mechanism or second WF library follows from these measurements alone.

## Single-descent insertion candidate

**Outcome: the candidate fails the registered no-regression criterion.** Its
replacement-only cost rises materially in both reversed cohorts. Normal
264-byte pairs at count 256 have source-C-normalized ratios 1.6214 and 1.6331
against a 3% band; counts 8 and 4096 also lose. Retained wide replacement loses
at all three counts. Build and some churn paths improve, but the criterion
does not permit those gains to hide replacement regressions. The published
library remains the baseline; no second sampling pass was run.

The prospective [single-descent discriminator](../../../investigations/containers-and-resources/X1-LIBRARY.md#single-descent-insertion-discriminator)
was committed at `881f6c888` before this comparison. The
[candidate patch](insertion-candidate.patch) is the sole additional source
artifact: its Makefile caller extracts the baseline from `1811f2d01` into the
build directory, applies the patch, and checks both full source hashes.
The reconstructed candidate is
`ddc53f3bd12e3682c26ea72f33d94da5da787371afb7461fd7d69d1798aca4ec`;
the baseline remains
`affaee669a09320aafc9ec5badbea6c11fd95623e1ea8987ad9811742c4b70bb`.
Retire or supersede this patch with the candidate claim; it is not a second
maintained library or a gate dependency.

The shared WF/C/sorted-oracle driver adds only replacement of existing keys
below the logical ceiling as path `P`. At step `(round,index)`, its offered
value seed is `seed + count + round * count + index`, with wrapping arithmetic;
it consumes the returned old pair. It uses the same 8192 round/key steps and
full construction/visitation/cleanup accounting as the other operation paths.
Native map algorithms are unchanged, and source C remains explicitly
**baseline-shaped**, not a source translation of the candidate. No C `restrict`
or result-layout revision was folded into this comparison.

Construction used the same frozen compiler and native flags as the baseline.
Both images in both modes passed 330 configurations through all four
implementations: 1,320 executions and six complete native audits per image/mode.
All node accounting matched; every timed checksum also passed. The two images'
C control IR files are byte-identical. Each retained WF image marks the same
29 public operation/callback instances and retains 26 public-operation call
sites; each C specialization retains 20 including audit callers.

| Combined stage | Command seconds | Guard seconds | Registered budget seconds |
| --- | ---: | ---: | ---: |
| Initial baseline/candidate construction | 11.54 | 11.65 | 120 |
| Complete correctness and clock observation | 3.34 | 3.44 | 40 |
| Complete fresh A/A and A/B matrix | 14.90 | 14.97 | 60 |

The sources were frozen before timing: driver SHA-256 starts `cd3c8cca`, WF
trace `779ffc60`, Makefile `39a4103c`, and candidate patch `90c71e4b`. Complete
hashes, both images and their IR are recorded in
[the new identities](insertion-initial-identities.txt). The original
[1,152 rows](measurements.csv) and [identities](identities.txt) are unchanged.
The separate [11,520 rows](insertion-initial.csv) retain every warm-up and sample.

The measured patch representation (`90c71e4b...`) is preserved at `bc83d10ad`.
After measurement, context-only blank lines were removed by regenerating a
zero-context unified diff. The current patch hash is
`1b008bbf35d48080cae616adf05a4f05962d555e76164bf0c3e97a44e0084f6e`;
the existing reconstruction target still produces the exact `ddc53f3b...`
candidate. The historical measured identity file is intentionally unchanged.
This artifact-hygiene correction changes no candidate bytes, images or samples;
no timing was rerun.

A/A uses two fresh processes of the same baseline executable. A/B uses the
rebuilt baseline and candidate. Each comparison has two cohorts; cohort one
reverses arm/mode order and the alternating within-cell implementation order.
Every arm measures its own three native controls, with the same seeds and one
warm-up plus five samples per cell. There are 60 payload/count/path/mode cells,
1,440 rows per complete arm, and eight complete arms. No row from the original
baseline dataset is reused to estimate the candidate's gain.

For each sample, primary normalization is
`(WF_B / sourceC_B) / (WF_A / sourceC_A)`; each displayed cohort value is the
median of its five paired ratios. Direct-C and AVL columns apply the same
formula independently. The cell's symmetric band is the maximum of 3%, the
absolute A/A normalized departure in either cohort, source-C median drift
between arms, and four clock quanta divided by the shortest WF/source-C median
interval. Drift means `abs(median(C_B) / median(C_A) - 1)`, maximized over A/A,
A/B and both cohorts. The [clock observation](insertion-clock.csv) reports
1000 ns for `clock_getres`, the positive-delta grid and the minimum positive
delta across 4096 reads, so the clock term uses 4000 ns. These interpretations
were stated before outcomes were inspected.

Tables show cohort `0 / 1`. Ratios below one favor the candidate. `gain` means
both cohorts beat the lower boundary; `loss` means at least one crosses the
upper boundary; `within` means neither decision is established. Classification
uses unrounded values. `B/L/C/R` retain their earlier meanings; `P` is
replacement-only. No cross-cell average enters the criterion.

**Normal candidate comparison.**

| Pair bytes | Count | Path | Band | A/A source C | A/B source C | A/B direct C | A/B AVL | Result |
| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 16 | 8 | B | 5.56% | 1.0069 / 1.0127 | 0.9494 / 0.9494 | 0.9494 / 0.9487 | 0.9487 / 0.9438 | within |
| 16 | 256 | B | 3.95% | 0.9997 / 0.9930 | 0.8732 / 0.8769 | 0.8750 / 0.8617 | 0.9073 / 0.8760 | gain |
| 16 | 4096 | B | 3.71% | 0.9975 / 1.0025 | 0.6607 / 0.6676 | 0.6591 / 0.6674 | 0.6424 / 0.6625 | gain |
| 16 | 8 | L | 7.02% | 0.9830 / 1.0000 | 0.9828 / 0.9989 | 0.9828 / 0.9928 | 0.9661 / 1.0045 | within |
| 16 | 256 | L | 10.98% | 1.0174 / 1.0056 | 0.9343 / 1.0234 | 0.9913 / 0.9790 | 0.9704 / 0.9721 | within |
| 16 | 4096 | L | 4.59% | 1.0073 / 0.9994 | 0.8849 / 0.8696 | 0.8811 / 0.8678 | 0.8848 / 0.8675 | gain |
| 16 | 8 | C | 3.79% | 0.9812 / 0.9882 | 1.0195 / 1.0339 | 1.0269 / 1.0450 | 1.0234 / 1.0311 | within |
| 16 | 256 | C | 4.33% | 1.0302 / 0.9960 | 1.0226 / 1.1022 | 1.0271 / 1.1023 | 1.0596 / 1.0922 | loss |
| 16 | 4096 | C | 3.00% | 1.0231 / 0.9999 | 0.8447 / 0.8428 | 0.8468 / 0.8528 | 0.8359 / 0.8432 | gain |
| 16 | 8 | R | 4.67% | 1.0090 / 0.9904 | 1.0000 / 1.0000 | 1.0000 / 1.0000 | 1.0000 / 0.9908 | within |
| 16 | 256 | R | 3.50% | 0.9994 / 0.9912 | 0.9589 / 0.9597 | 0.9668 / 0.9803 | 0.9522 / 0.9739 | gain |
| 16 | 4096 | R | 3.29% | 1.0229 / 1.0043 | 0.8856 / 0.8806 | 0.8794 / 0.8842 | 0.8967 / 0.8619 | gain |
| 16 | 8 | P | 10.00% | 0.9783 / 1.0244 | 1.4000 / 1.3750 | 1.4146 / 1.3750 | 1.4146 / 1.3750 | loss |
| 16 | 256 | P | 5.56% | 0.9909 / 0.9810 | 1.4366 / 1.4854 | 1.4637 / 1.5287 | 1.4500 / 1.5541 | loss |
| 16 | 4096 | P | 3.64% | 0.9971 / 0.9937 | 0.8768 / 0.8986 | 0.8857 / 0.8921 | 0.9009 / 0.8939 | gain |
| 264 | 8 | B | 4.22% | 1.0112 / 0.9911 | 0.8802 / 0.8663 | 0.8822 / 0.8635 | 0.8866 / 0.8719 | gain |
| 264 | 256 | B | 4.74% | 1.0089 / 0.9947 | 0.8898 / 0.9009 | 0.8989 / 0.8973 | 0.8954 / 0.8939 | gain |
| 264 | 4096 | B | 5.84% | 1.0073 / 1.0028 | 0.8116 / 0.8114 | 0.8173 / 0.8145 | 0.8153 / 0.8235 | gain |
| 264 | 8 | L | 7.05% | 1.0063 / 0.9945 | 1.0003 / 1.0000 | 1.0010 / 1.0000 | 0.9990 / 1.0000 | within |
| 264 | 256 | L | 12.32% | 0.8768 / 1.0157 | 1.0435 / 0.9804 | 1.0370 / 0.9825 | 1.0414 / 0.9775 | within |
| 264 | 4096 | L | 3.00% | 1.0014 / 0.9961 | 0.8932 / 0.8928 | 0.8995 / 0.8921 | 0.8916 / 0.8857 | gain |
| 264 | 8 | C | 3.00% | 1.0023 / 0.9865 | 1.0100 / 1.0160 | 1.0311 / 1.0187 | 1.0084 / 0.9959 | within |
| 264 | 256 | C | 3.27% | 0.9861 / 1.0032 | 1.0331 / 1.0330 | 1.0412 / 1.0410 | 1.0450 / 1.0250 | loss |
| 264 | 4096 | C | 3.00% | 0.9948 / 1.0014 | 0.9833 / 0.9845 | 0.9946 / 0.9893 | 1.0036 / 0.9778 | within |
| 264 | 8 | R | 4.38% | 1.0017 / 1.0000 | 1.0016 / 0.9966 | 1.0067 / 0.9966 | 1.0257 / 0.9966 | within |
| 264 | 256 | R | 3.00% | 1.0169 / 0.9991 | 1.0103 / 0.9919 | 1.0215 / 1.0057 | 1.0123 / 0.9972 | within |
| 264 | 4096 | R | 4.18% | 0.9887 / 0.9967 | 0.9415 / 0.9474 | 0.9334 / 0.9506 | 0.9327 / 0.9561 | gain |
| 264 | 8 | P | 3.87% | 1.0010 / 1.0002 | 1.5494 / 1.5848 | 1.5573 / 1.5263 | 1.5504 / 1.5446 | loss |
| 264 | 256 | P | 3.00% | 0.9993 / 1.0067 | 1.6214 / 1.6331 | 1.6524 / 1.5928 | 1.6169 / 1.6335 | loss |
| 264 | 4096 | P | 3.42% | 0.9879 / 1.0044 | 1.0955 / 1.1166 | 1.1027 / 1.1034 | 1.1128 / 1.1183 | loss |

**Retained candidate comparison.**

| Pair bytes | Count | Path | Band | A/A source C | A/B source C | A/B direct C | A/B AVL | Result |
| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 16 | 8 | B | 4.71% | 0.9962 / 1.0017 | 0.8616 / 0.8900 | 0.8333 / 0.8880 | 0.8450 / 0.8900 | gain |
| 16 | 256 | B | 3.00% | 1.0181 / 1.0003 | 0.7448 / 0.7167 | 0.7367 / 0.7244 | 0.7252 / 0.7311 | gain |
| 16 | 4096 | B | 3.00% | 1.0071 / 1.0019 | 0.6420 / 0.6503 | 0.6209 / 0.6327 | 0.6287 / 0.6436 | gain |
| 16 | 8 | L | 3.00% | 1.0072 / 1.0072 | 1.0000 / 0.9985 | 1.0053 / 0.9897 | 1.0000 / 1.0024 | within |
| 16 | 256 | L | 6.52% | 1.0496 / 0.9650 | 0.9943 / 0.9896 | 1.0045 / 1.0242 | 0.9866 / 0.9729 | within |
| 16 | 4096 | L | 3.00% | 1.0223 / 0.9938 | 0.8382 / 0.8804 | 0.8581 / 0.8771 | 0.8464 / 0.8802 | gain |
| 16 | 8 | C | 3.00% | 1.0000 / 0.9926 | 0.9411 / 0.9082 | 0.9407 / 0.9409 | 0.9361 / 0.9438 | gain |
| 16 | 256 | C | 3.00% | 0.9904 / 0.9825 | 0.8706 / 0.8740 | 0.8685 / 0.8792 | 0.8655 / 0.8827 | gain |
| 16 | 4096 | C | 3.00% | 1.0200 / 0.9857 | 0.8147 / 0.7913 | 0.7829 / 0.7940 | 0.7876 / 0.7977 | gain |
| 16 | 8 | R | 3.39% | 1.0000 / 1.0000 | 1.0000 / 1.0112 | 0.9992 / 1.0112 | 1.0054 / 1.0009 | within |
| 16 | 256 | R | 3.00% | 1.0013 / 1.0026 | 0.9974 / 0.9885 | 0.9997 / 0.9923 | 0.9995 / 0.9987 | within |
| 16 | 4096 | R | 4.05% | 0.9787 / 1.0001 | 0.8744 / 0.8823 | 0.8852 / 0.8939 | 0.8801 / 0.8856 | gain |
| 16 | 8 | P | 5.41% | 1.0132 / 0.9878 | 1.2146 / 1.1604 | 1.2324 / 1.1622 | 1.1773 / 1.1622 | loss |
| 16 | 256 | P | 6.49% | 1.0070 / 0.9927 | 1.1317 / 1.1161 | 1.0954 / 1.2609 | 1.0916 / 1.0557 | loss |
| 16 | 4096 | P | 6.67% | 1.0262 / 0.9898 | 0.8758 / 0.8663 | 0.8694 / 0.8633 | 0.8759 / 0.8746 | gain |
| 264 | 8 | B | 6.85% | 1.0230 / 1.0144 | 0.9969 / 1.0053 | 1.0183 / 0.9990 | 1.0149 / 1.0022 | within |
| 264 | 256 | B | 4.76% | 1.0110 / 1.0014 | 0.9695 / 0.9626 | 0.9735 / 0.9621 | 0.9783 / 0.9812 | within |
| 264 | 4096 | B | 4.45% | 1.0037 / 1.0031 | 0.8568 / 0.8600 | 0.8581 / 0.8423 | 0.8463 / 0.8626 | gain |
| 264 | 8 | L | 4.69% | 0.9906 / 1.0000 | 1.0159 / 1.0053 | 1.0049 / 1.0053 | 1.0025 / 0.9951 | within |
| 264 | 256 | L | 23.55% | 1.2355 / 1.0141 | 1.0054 / 0.9865 | 0.9764 / 0.9886 | 1.0000 / 0.9962 | within |
| 264 | 4096 | L | 3.00% | 1.0114 / 1.0149 | 0.9288 / 0.9442 | 0.9292 / 0.9115 | 0.9337 / 0.9325 | gain |
| 264 | 8 | C | 3.00% | 0.9901 / 1.0030 | 1.0399 / 1.0503 | 1.0451 / 1.0540 | 1.0473 / 1.0368 | loss |
| 264 | 256 | C | 3.00% | 0.9994 / 0.9997 | 0.9715 / 0.9658 | 0.9634 / 0.9657 | 0.9696 / 0.9599 | within |
| 264 | 4096 | C | 3.00% | 1.0030 / 1.0014 | 0.9513 / 0.9492 | 0.9462 / 0.9495 | 0.9521 / 0.9546 | gain |
| 264 | 8 | R | 4.00% | 0.9917 / 1.0084 | 1.0067 / 0.9739 | 0.9822 / 1.0050 | 1.0183 / 1.0017 | within |
| 264 | 256 | R | 4.42% | 0.9983 / 0.9975 | 0.9987 / 0.9967 | 0.9966 / 0.9950 | 0.9880 / 0.9981 | within |
| 264 | 4096 | R | 3.00% | 1.0033 / 0.9958 | 0.9512 / 0.9620 | 0.9546 / 0.9484 | 0.9454 / 0.9514 | gain |
| 264 | 8 | P | 3.14% | 0.9979 / 0.9935 | 1.1692 / 1.1427 | 1.1684 / 1.1318 | 1.1614 / 1.1597 | loss |
| 264 | 256 | P | 5.18% | 0.9898 / 1.0138 | 1.2127 / 1.1687 | 1.2286 / 1.1554 | 1.1892 / 1.1369 | loss |
| 264 | 4096 | P | 3.00% | 1.0156 / 1.0160 | 1.0848 / 1.0521 | 1.0827 / 1.0464 | 1.0690 / 1.0536 | loss |

The large replacement regressions survive both cohort orders and all three
native normalizations. Some lookup cells have large A/A bands (23.55% for
retained wide/count 256), so those cells alone would not support a small-effect
decision. They do not erase the independent, consistent replacement losses.
The optional complete repeat was reserved for variance or directional
uncertainty; neither prevents rejecting this candidate, so it was not used.
No cell was selectively rerun or removed.

Optimized IR confirms the intended transfer removal. Baseline wide
`ordered_map_insert_link$instance$89` loads all 33 words of the owned Pair at
entry and stages it for recursion; candidate `$instance$91` takes a reference
to the 272-byte one-slot carrier and passes that same pointer recursively.
The Pair is extracted only at the vacant leaf; replacement swaps the resident
pair with the occupied carrier.

The candidate also changes replacement's result path. Its wide matched-hit arm
clears a 288-byte `Option<Entry>` result, and each recursive None unwind clears
another 288-byte result. This appears as `llvm.memset(..., 288)` in both modes.
The baseline's Boolean replacement search has no optional-promotion return
chain. The presence of these transfers supports the predicted cost mechanism;
the measured percentages do not isolate zeroing, recursion, or carrier
materialization from one another. Source-C result ABI and alias differences
also prevent using that control alone as a causal decomposition.

Lookup/range and replacement-only timings still include complete construction.
Consequently, a faster 4096-pair lookup/range trace does not establish a faster
lookup/range helper; the candidate's lower build cost contributes to it. The
small-count replacement paths amortize construction across many rounds and
expose the regression that a combined churn path could conceal.

Current explicit targets, from this revision, are:

```sh
WHITEFOOT_CHECK_TIMEOUT=120 perl .github/run-check.pl ordered-insertion-build \
  make -C research/experiments/container-representation/ordered-library insertion-build \
  WHITEFOOTC=/path/to/the/recorded/whitefootc
WHITEFOOT_CHECK_TIMEOUT=40 perl .github/run-check.pl ordered-insertion-check \
  make -C research/experiments/container-representation/ordered-library insertion-verify
WHITEFOOT_CHECK_TIMEOUT=60 perl .github/run-check.pl ordered-insertion-measure \
  make -C research/experiments/container-representation/ordered-library insertion-measure-only \
  INSERTION_RUN=initial
make -C research/experiments/container-representation/ordered-library insertion-identities \
  WHITEFOOTC=/path/to/the/recorded/whitefootc WHITEFOOTC_LABEL=frozen-v0.68-whitefootc
```

`insertion-build` reconstructs only checked build-directory copies and never
edits `lib/containers/ordered-map.wf`. `insertion-verify` includes the full
five-path oracle, native audits, retained-boundary checks, control-IR equality
and clock record. `insertion-measure-only` writes a separate complete matrix;
`INSERTION_RUN=repeat` is available solely for the registered one-repeat rule.
These are explicit research targets and add no correctness-gate dependency.

**Design suitability.** Borrowing the carrier removes the intended recursive
Pair copies, but this particular combined-search protocol introduces a
material replacement cost. Retain the baseline source under the registered
criterion. The evidence does not reject every one-pass or carrier formulation,
does not select an ordered representation, and does not by itself require a
language or compiler change.

## Borrowed promotion follow-up

The second candidate also fails the registered no-loss criterion. Normal wide
replacement at 256 entries is 1.6885 / 1.6003 times the baseline after source-C
normalization, outside its 3.13% band; at eight entries it is 1.5313 / 1.5620,
outside 3%. Retained wide replacement at eight and 256 entries also loses in
both cohorts. Build and larger-count churn gains cannot compensate for those
losses. No repeat was needed, and no third source candidate was measured. The
published library remains the baseline.

This is the separately registered
[borrowed-promotion discriminator](../../../investigations/containers-and-resources/X1-LIBRARY.md#borrowed-promotion-follow-up),
committed before construction and timing in `8bd165e70`. It keeps the first
candidate's offered Pair carrier and also borrows one promotion Entry slot;
recursive insertion returns unit. The [zero-context patch](promotion-candidate.patch)
reconstructs SHA-256
`5514ce2aecdf2a8074dc8897b0457d6858e7ecf16d2d2fa559dfefb6420f2c8f`
from the checked original baseline. It is built only under `.build/`, with no
edit to the published library. The first candidate's patch, measurements and
identities remain unchanged. The separate [11,520 raw rows](promotion-initial.csv),
[identities](promotion-initial-identities.txt) and [clock observation](promotion-clock.csv)
belong to this comparison; their evidence claim owns their retention.

The five-path WF trace (`779ffc60`) and C driver/controls (`cd3c8cca`) are exactly
the sources measured for candidate 1. All 13 optimized native control IR files
match the earlier recorded hashes and match between the new baseline and
candidate images. Baseline optimized WF IR differs from the earlier rebuilt
baseline only in its ModuleID path, in both modes. The current Makefile hash
is `06a09ed6`; it selects the separate patch and `promotion-*` build/evidence
names, while default targets still reconstruct candidate 1. Full hashes,
compiler identity, toolchain, OS and architecture are recorded in the new
identity file. Both earlier datasets and identity files are byte-for-byte
unchanged; replay of the original four-path dataset remains at `1811f2d01`.

| Stage | Command seconds | Guarded seconds | Registered budget |
| --- | ---: | ---: | ---: |
| Construct both images and optimized IR | 11.38 | 11.41 | 120 |
| Complete checks and clock observation | 3.22 | 3.33 | 40 |
| One complete A/A then A/B matrix | 14.86 | 14.94 | 60 |
| Total | 29.46 | 29.68 | Separate stage limits |

Both images, in both modes, pass 330 oracle configurations, 1,320 executions
and six complete native structural audits per mode. Retained mode still has
29 marked WF definitions, 26 WF public-operation call sites and 20 such C call
sites per control translation unit, including audit callers. All 11,520 rows
are present in 1,920 groups with exactly samples 0–5; checksums agree across
variants and arms, and every WF/source-C allocation count, requested-byte,
peak-node and peak-byte observation agrees. All 53 recorded identities were
checked against the measured files.

The protocol and interpretation are unchanged from the preceding candidate:
60 cells, fresh A/A followed by A/B, two reversed cohorts, independent controls
in every arm, one warm-up and five samples, same seeds/batching. Each cohort
ratio is the median of paired `(WF_B / C_B) / (WF_A / C_A)` ratios. The band is
the maximum of 3%, either cohort's absolute A/A departure, source-C median
arm drift over both comparisons/cohorts, and four clock quanta over the
shortest WF/source-C median interval. Clock resolution and observed grid are
again 1,000 ns. Direct C and AVL remain cross-checks. Tables use unrounded
values for decisions and display cohort `0 / 1`; `B/L/C/R/P` have the preceding
five-path meanings. There is no pooled score or selective rerun.

**Normal borrowed-promotion comparison.**

| Pair bytes | Count | Path | Band | A/A source C | A/B source C | A/B direct C | A/B AVL | Result |
| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 16 | 8 | B | 5.56% | 1.0011 / 0.9898 | 0.9495 / 0.9618 | 0.9336 / 0.9750 | 0.9037 / 0.9664 | within |
| 16 | 256 | B | 4.96% | 1.0000 / 1.0071 | 0.8628 / 0.8817 | 0.8602 / 0.8538 | 0.8621 / 0.8613 | gain |
| 16 | 4096 | B | 3.00% | 0.9915 / 1.0003 | 0.6670 / 0.6848 | 0.6737 / 0.6795 | 0.6725 / 0.6612 | gain |
| 16 | 8 | L | 7.02% | 0.9946 / 1.0169 | 1.0000 / 0.9783 | 1.0000 / 0.9862 | 1.0175 / 0.9955 | within |
| 16 | 256 | L | 10.81% | 1.0074 / 1.0060 | 0.9626 / 0.9075 | 0.9902 / 0.9880 | 1.0014 / 0.9804 | within |
| 16 | 4096 | L | 3.43% | 0.9981 / 0.9969 | 0.8916 / 0.8753 | 0.8866 / 0.8970 | 0.8807 / 0.8995 | gain |
| 16 | 8 | C | 3.00% | 0.9709 / 1.0147 | 1.0154 / 1.0656 | 1.0324 / 1.0462 | 1.0169 / 1.0704 | loss |
| 16 | 256 | C | 4.43% | 1.0087 / 0.9759 | 1.0239 / 1.0607 | 1.0307 / 1.0433 | 1.0360 / 1.0554 | loss |
| 16 | 4096 | C | 3.00% | 1.0007 / 0.9976 | 0.8341 / 0.8465 | 0.8392 / 0.8491 | 0.8367 / 0.8439 | gain |
| 16 | 8 | R | 6.31% | 1.0000 / 1.0004 | 1.0000 / 0.9820 | 0.9836 / 1.0230 | 0.9813 / 1.0155 | within |
| 16 | 256 | R | 7.93% | 1.0104 / 1.0793 | 0.9914 / 0.9848 | 1.0016 / 0.9703 | 0.9792 / 0.9824 | within |
| 16 | 4096 | R | 3.68% | 0.9888 / 1.0055 | 0.8673 / 0.8776 | 0.8797 / 0.8816 | 0.8680 / 0.8818 | gain |
| 16 | 8 | P | 10.00% | 1.0000 / 1.0000 | 1.3423 / 1.3750 | 1.3750 / 1.3750 | 1.3719 / 1.3750 | loss |
| 16 | 256 | P | 4.04% | 1.0081 / 1.0000 | 1.2741 / 1.3454 | 1.3087 / 1.3539 | 1.3750 / 1.3556 | loss |
| 16 | 4096 | P | 3.00% | 0.9986 / 1.0025 | 0.8957 / 0.8932 | 0.8936 / 0.8898 | 0.8964 / 0.8861 | gain |
| 264 | 8 | B | 3.00% | 0.9970 / 1.0030 | 0.9270 / 0.9293 | 0.9250 / 0.9318 | 0.9489 / 0.9359 | gain |
| 264 | 256 | B | 3.00% | 0.9961 / 1.0061 | 0.9176 / 0.9169 | 0.9259 / 0.9237 | 0.9244 / 0.9154 | gain |
| 264 | 4096 | B | 3.29% | 1.0040 / 1.0046 | 0.8133 / 0.8142 | 0.8183 / 0.8133 | 0.8183 / 0.8075 | gain |
| 264 | 8 | L | 7.05% | 1.0130 / 0.9948 | 0.9995 / 1.0000 | 0.9988 / 1.0000 | 0.9861 / 1.0000 | within |
| 264 | 256 | L | 6.64% | 1.0664 / 0.9915 | 0.9788 / 1.0154 | 0.9898 / 1.0100 | 0.9944 / 0.9928 | within |
| 264 | 4096 | L | 6.29% | 0.9805 / 0.9961 | 0.8915 / 0.9074 | 0.8924 / 0.8952 | 0.8882 / 0.9058 | gain |
| 264 | 8 | C | 3.00% | 1.0101 / 1.0045 | 1.0310 / 1.0490 | 1.0390 / 1.0485 | 1.0379 / 1.0497 | loss |
| 264 | 256 | C | 5.53% | 1.0089 / 0.9968 | 1.0156 / 1.0369 | 1.0282 / 1.0516 | 1.0167 / 1.0318 | within |
| 264 | 4096 | C | 3.00% | 0.9955 / 0.9838 | 0.9405 / 0.9362 | 0.9468 / 0.9396 | 0.9487 / 0.9529 | gain |
| 264 | 8 | R | 6.20% | 0.9967 / 1.0047 | 1.0017 / 0.9986 | 0.9983 / 0.9997 | 0.9921 / 0.9932 | within |
| 264 | 256 | R | 3.00% | 0.9964 / 0.9968 | 0.9955 / 1.0037 | 0.9950 / 1.0008 | 0.9968 / 0.9786 | within |
| 264 | 4096 | R | 4.00% | 1.0079 / 0.9916 | 0.9501 / 0.9476 | 0.9592 / 0.9547 | 0.9773 / 0.9444 | gain |
| 264 | 8 | P | 3.00% | 0.9982 / 1.0043 | 1.5313 / 1.5620 | 1.5264 / 1.5379 | 1.5443 / 1.5620 | loss |
| 264 | 256 | P | 3.13% | 1.0064 / 0.9995 | 1.6885 / 1.6003 | 1.7010 / 1.6413 | 1.6773 / 1.6824 | loss |
| 264 | 4096 | P | 3.70% | 1.0189 / 1.0070 | 1.0836 / 1.1031 | 1.0891 / 1.1005 | 1.1045 / 1.1078 | loss |

**Retained borrowed-promotion comparison.**

| Pair bytes | Count | Path | Band | A/A source C | A/B source C | A/B direct C | A/B AVL | Result |
| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 16 | 8 | B | 5.08% | 0.9944 / 1.0000 | 0.8575 / 0.8419 | 0.8600 / 0.8725 | 0.8498 / 0.8576 | gain |
| 16 | 256 | B | 5.63% | 0.9913 / 0.9882 | 0.7143 / 0.7246 | 0.7220 / 0.7411 | 0.7211 / 0.7167 | gain |
| 16 | 4096 | B | 4.40% | 1.0165 / 0.9979 | 0.6479 / 0.6410 | 0.6502 / 0.6451 | 0.6333 / 0.6292 | gain |
| 16 | 8 | L | 5.14% | 1.0060 / 1.0000 | 1.0151 / 1.0011 | 1.0221 / 1.0072 | 1.0280 / 1.0328 | within |
| 16 | 256 | L | 10.58% | 0.9950 / 0.9680 | 1.0549 / 0.9888 | 1.0627 / 0.9194 | 1.0165 / 0.9598 | within |
| 16 | 4096 | L | 4.16% | 0.9932 / 0.9876 | 0.8557 / 0.8468 | 0.8771 / 0.8739 | 0.8688 / 0.8513 | gain |
| 16 | 8 | C | 9.41% | 0.9965 / 0.9960 | 0.9468 / 0.8941 | 0.9289 / 0.9541 | 0.9471 / 0.9424 | within |
| 16 | 256 | C | 6.03% | 0.9796 / 1.0048 | 0.8931 / 0.8789 | 0.8905 / 0.8747 | 0.8836 / 0.8353 | gain |
| 16 | 4096 | C | 3.00% | 1.0028 / 1.0059 | 0.8074 / 0.8013 | 0.8023 / 0.7867 | 0.8075 / 0.7935 | gain |
| 16 | 8 | R | 4.30% | 0.9999 / 1.0000 | 1.0396 / 0.9944 | 0.9944 / 1.0000 | 1.0209 / 1.0000 | within |
| 16 | 256 | R | 3.00% | 0.9961 / 1.0026 | 0.9987 / 0.9872 | 0.9959 / 0.9976 | 0.9922 / 1.0102 | within |
| 16 | 4096 | R | 3.00% | 0.9825 / 0.9964 | 0.8816 / 0.8742 | 0.8829 / 0.8743 | 0.8844 / 0.8728 | gain |
| 16 | 8 | P | 5.41% | 1.0000 / 1.0000 | 1.1775 / 1.1763 | 1.1622 / 1.1757 | 1.1622 / 1.1757 | loss |
| 16 | 256 | P | 5.54% | 0.9601 / 0.9446 | 1.2252 / 1.2173 | 1.1784 / 1.2430 | 1.1524 / 1.1777 | loss |
| 16 | 4096 | P | 3.00% | 1.0131 / 1.0077 | 0.8587 / 0.8938 | 0.8722 / 0.8727 | 0.8696 / 0.8865 | gain |
| 264 | 8 | B | 3.00% | 1.0000 / 1.0243 | 1.0322 / 1.0538 | 1.0213 / 1.0410 | 1.0087 / 1.0409 | loss |
| 264 | 256 | B | 3.00% | 0.9972 / 0.9947 | 0.9664 / 0.9631 | 0.9666 / 0.9704 | 0.9653 / 0.9822 | gain |
| 264 | 4096 | B | 3.00% | 1.0271 / 1.0123 | 0.8442 / 0.8289 | 0.8315 / 0.8544 | 0.8341 / 0.8500 | gain |
| 264 | 8 | L | 3.66% | 1.0000 / 1.0001 | 1.0106 / 1.0005 | 1.0053 / 0.9761 | 1.0053 / 0.9880 | within |
| 264 | 256 | L | 3.00% | 1.0171 / 1.0120 | 0.9355 / 0.9250 | 0.9387 / 1.0200 | 0.9309 / 0.9199 | gain |
| 264 | 4096 | L | 4.98% | 0.9876 / 0.9987 | 0.9425 / 0.9243 | 0.9420 / 0.9175 | 0.9362 / 0.9147 | gain |
| 264 | 8 | C | 5.06% | 0.9892 / 0.9817 | 1.0449 / 1.0572 | 1.0484 / 1.0448 | 1.0401 / 1.0484 | loss |
| 264 | 256 | C | 3.00% | 1.0011 / 0.9951 | 0.9299 / 0.9351 | 0.9337 / 0.9427 | 0.9420 / 0.9443 | gain |
| 264 | 4096 | C | 3.00% | 1.0034 / 0.9878 | 0.9046 / 0.8923 | 0.9002 / 0.8947 | 0.8922 / 0.8909 | gain |
| 264 | 8 | R | 3.70% | 1.0004 / 1.0000 | 0.9950 / 1.0017 | 1.0000 / 1.0016 | 0.9967 / 1.0017 | within |
| 264 | 256 | R | 3.00% | 1.0001 / 1.0000 | 0.9996 / 0.9959 | 1.0109 / 0.9975 | 1.0105 / 1.0004 | within |
| 264 | 4096 | R | 3.00% | 1.0035 / 1.0053 | 0.9540 / 0.9547 | 0.9600 / 0.9530 | 0.9583 / 0.9610 | gain |
| 264 | 8 | P | 3.00% | 1.0000 / 0.9726 | 1.2377 / 1.2129 | 1.2499 / 1.2224 | 1.2507 / 1.2048 | loss |
| 264 | 256 | P | 3.00% | 0.9982 / 0.9870 | 1.1214 / 1.1298 | 1.1332 / 1.1412 | 1.1321 / 1.1204 | loss |
| 264 | 4096 | P | 4.57% | 1.0070 / 0.9971 | 0.9539 / 0.9529 | 0.9538 / 0.9388 | 0.9683 / 0.9444 | gain |

The repeated large replacement losses are sufficient to reject this candidate
without the optional complete repeat. Some smaller effects remain uncertain:
for example, scalar/count-256 lookup bands exceed 10%, and normal small-count
churn losses differ in magnitude between cohorts. No conclusion about those
cells is needed to resolve the candidate. All cells and warm-ups are retained.

Optimized IR confirms the intended private-interface change. In both modes,
wide `ordered_map_insert_link$instance$93` takes the link plus references to
the 272-byte offered slot and 288-byte promotion slot. It has no aggregate
result pointer; recursion passes the same two slot pointers. Its matched-hit
and unchanged-ancestor paths contain no 288-byte result clearing. The scalar
helper likewise loses the 40-byte optional-result clears. Private helpers remain
optimizable under the same public retention policy.

Remaining initialization and owner transfers are visible:

| Path in candidate 2 | Optimized operation in both modes |
| --- | --- |
| Below-ceiling public put | One promotion-slot memset: 40 scalar bytes or 288 wide bytes; offered-slot length is stored after its Pair is placed. |
| Wide offered Pair placement | Normal mode stores 33 words plus the slot length; retained mode copies 264 bytes into the slot plus its length store. |
| Wide replacement match | Three 264-byte memcpy operations implement the Pair swap; no promotion payload is read or written on that match. |
| Unchanged ancestor after recursion | One promotion-length load/comparison selects immediate return; no optional aggregate is returned. |
| Wide replacement public result | One 264-byte memcpy extracts the old Pair into the unchanged returned-owner result, plus result tags and occupancy stores. |
| Vacant wide leaf | The 264-byte Pair transfers to the promotion entry, followed by 16 zeroed bytes for its vacant right link and the occupancy stores. |
| Parent accepting a wide promotion | One 280-byte memcpy stages the taken Entry before `insert_item`; ordinary entry shifting/splitting and node allocation still apply. |

The public wide put retains 73/144 load/store instructions in normal optimized
IR and 23/30 in retained IR, compared with baseline 71/139 and 21/25. The wide
recursive link has 27/20 and 23/20, compared with candidate 1's 26/20 and 22/20:
removing its result writes adds an explicit promotion occupancy path instead.
These are static instruction-site counts across whole functions, not executed
counts or byte totals. Wide splitting still has its 2,240-byte unused suffix
initialization; a new root still has 3,920 unused bytes initialized. Those
allocation paths are unchanged representation costs, not replacement-only work.

The final arm64 prologues also retain material stack frames. These byte counts
include saved registers and explicit stack subtraction, but do not sum nested
frames or measure a high-water mark:

| Wide helper | Baseline normal / retained | Candidate 1 normal / retained | Candidate 2 normal / retained |
| --- | ---: | ---: | ---: |
| Public put | 928 / 1,184 | 960 / 1,456 | 944 / 1,456 |
| Recursive insertion link | 800 / 720 | 352 / 368 | 352 / 368 |

The baseline replacement path uses its Boolean search and does not enter the
listed insertion helper. The borrowed promotion removes aggregate result
clearing but does not reduce the recursive frame relative to candidate 1;
Pair-swap and taken-Entry temporaries remain. Its public boundary still
materializes the carrier and initializes a promotion slot even on replacement.

This follow-up changes the earlier causal inference: removing recursive
optional results did **not** cure the replacement regression. Their clearing
was an observed cost, not an established dominant cause. Remaining slot
initialization, Pair transfers, occupancy branches, stack traffic and inlining
shape are possible contributors; these measurements do not isolate their
elapsed shares. Comparing candidate 1 and candidate 2's percentages across
separate matrices would also not be a direct paired comparison between them.

The source-C control remains baseline-shaped, with the previously documented
noalias and tag/result-ABI differences. No frozen native control was rewritten.
The primary evidence is unchanged-control, WF-versus-WF normalization with
identical public WF result ABIs, not proof that WF and C have identical ABIs.
All five paths still include construction and cleanup, so large-count lookup,
range and replacement gains can include the candidate's faster construction.
This experiment selects neither a default ordered representation nor a compiler
or language rule.

Reconstruct and replay this follow-up with the existing explicit targets:

```sh
WHITEFOOT_CHECK_TIMEOUT=120 perl .github/run-check.pl ordered-promotion-build \
  make -C research/experiments/container-representation/ordered-library insertion-build \
  INSERTION_CANDIDATE=pair-promotion WHITEFOOTC=/path/to/the/recorded/whitefootc
WHITEFOOT_CHECK_TIMEOUT=40 perl .github/run-check.pl ordered-promotion-check \
  make -C research/experiments/container-representation/ordered-library insertion-verify \
  INSERTION_CANDIDATE=pair-promotion
WHITEFOOT_CHECK_TIMEOUT=60 perl .github/run-check.pl ordered-promotion-measure \
  make -C research/experiments/container-representation/ordered-library insertion-measure-only \
  INSERTION_CANDIDATE=pair-promotion INSERTION_RUN=initial
make -C research/experiments/container-representation/ordered-library insertion-identities \
  INSERTION_CANDIDATE=pair-promotion WHITEFOOTC=/path/to/the/recorded/whitefootc \
  WHITEFOOTC_LABEL=frozen-v0.68-whitefootc
```

The build checks both frozen driver hashes and both reconstructed library
hashes. Verification checks the complete oracle, audits, retention and native
control equality, then records the clock. Outputs use `.build/promotion-*`;
omitting `INSERTION_CANDIDATE` keeps the earlier `.build/insertion-*` targets.
A different source or trace requires its own prospective criterion. No research
target is a correctness-gate dependency.

**Design suitability.** The borrowed promotion is expressible in the existing
language and removes the intended recursive result transfers, but the complete
candidate still loses on replacement. Retain the baseline library. Further
source or lowering work requires a newly motivated experiment; this bounded
follow-up does not authorize a third candidate or settle the remaining causal
attribution. No specification rule changed.

## Current Rust and C++ comparison

The explicit `ecosystem-*` targets implement the separately registered
[practical comparison contract](../ECOSYSTEM.md). They rebuild the current
Whitefoot `std::collections::ordered_map`, the unchanged source-shaped C,
direct B-tree C and AVL controls, Rust `BTreeMap`, C++ `std::map`, and pinned
Abseil `btree_map`. No historical timing is reused as a denominator. The active
WF caller now imports the standard module; the insertion and promotion replay
targets above retain their frozen hashes and require the recorded historical
source and compiler checkout. Their patches and published data are unchanged.

The five complete traces and their sorted flat-sequence oracle are unchanged.
They observe 16-byte scalar pairs and 264-byte inline pairs, sorted full
traversal, half-open ranges, replacement and removed owners, and final cleanup.
They retain no references across mutations and impose no physical destruction
order. Rust and C++ use their native entry operations below the 8192-entry
logical ceiling. At that ceiling, an existing key still replaces its value;
an absent key returns the offered value and preserves the map. All keys are
`u64`: retaining an equal stored key is equivalent here, but does not establish
equivalence for distinct comparator-equal owning keys. The wide Rust and C++
records are movable inline values without copying APIs or per-value heap
allocation. Separate boxed-owner audits check transfer and exactly-once
destruction; the timed records do not measure nested-owner performance.

Normal practical images use Clang `-O3`, C++ `-O3 -DNDEBUG`, and Rust
`opt-level=3`, without forced operation barriers. Whole traces cross the C ABI;
internal operations remain ordinarily optimizable. Timed WF and C nodes use
ordinary `malloc`/`free`, and native containers use their ordinary allocator.
Separate `ACCOUNT_ONLY` images observe requested allocation bytes and live
peaks. These are not process RSS or allocator-resident memory. The allocation
image checks that WF and source C still match, and every implementation must
return to zero live allocations after each trace. Native container audits also
cover replacement/refusal at the ceiling, absent removal, empty/inverted
ranges, complete removal, and reuse. Timed rows check that every allocation
counter remains zero; allocation costs are reported only by the separate
accounting image.

Samples use counts 8, 256 and 4096, both payloads, and all five paths. Before
sampling each cell in each cohort, every variant completes one untimed whole
trace and passes the independent oracle and cleanup checks. Sample 0 remains
a recorded warm-up batch; samples 1--5 are paired measurements. Implementation
order rotates each sample, and cohort 1 reverses cohort 0's order. `ECO_SCALE`
defaults to 16 and repeats whole traces: build/cleanup uses
`(4096 / count) * ECO_SCALE` traces, while other paths use `ECO_SCALE` traces
of `8192 / count` rounds. The trace seeds advance within each batch and match
across implementations and cohorts. Report elapsed time per complete trace;
do not subtract setup to claim isolated operation latency. Allocation rows use
one complete trace per cell with seed 101, separately from timing.

From the repository root, run construction, verification, accounting, and
timing as separate guarded commands:

```sh
perl .github/run-check.pl ordered-ecosystem-build \
  make -C research/experiments/container-representation/ordered-library ecosystem-build \
  WHITEFOOTC=/path/to/current/whitefootc ABSEIL_PREFIX=/path/to/pinned/abseil
perl .github/run-check.pl ordered-ecosystem-check \
  make -C research/experiments/container-representation/ordered-library ecosystem-check \
  WHITEFOOTC=/path/to/current/whitefootc ABSEIL_PREFIX=/path/to/pinned/abseil
perl .github/run-check.pl ordered-ecosystem-account \
  make -C research/experiments/container-representation/ordered-library ecosystem-account \
  WHITEFOOTC=/path/to/current/whitefootc ABSEIL_PREFIX=/path/to/pinned/abseil
perl .github/run-check.pl ordered-ecosystem-measure \
  make -C research/experiments/container-representation/ordered-library ecosystem-measure-only \
  ABSEIL_PREFIX=/path/to/pinned/abseil ECO_SCALE=16
make -C research/experiments/container-representation/ordered-library ecosystem-identities \
  WHITEFOOTC=/path/to/current/whitefootc ABSEIL_PREFIX=/path/to/pinned/abseil
```

The check target includes bounded negative controls: a deliberately corrupted
checksum and a simulated unreleased allocation must each exit unsuccessfully
with the matching diagnostic. Raw samples, accounting, clock resolution, and
construction identities are written under `.build/ecosystem/`.

### Verified practical run

The measured sources are revision
[`0c3203aa6111`](https://github.com/mbbill/Whitefoot/tree/0c3203aa6111f14247aa950e3794e83082d4f29c).
The [original timing rows](ecosystem-samples.csv) and
[longer replay](ecosystem-replay-samples.csv) each preserve all 2,520
observations, including sample 0; the
[allocation rows](ecosystem-accounting.csv) preserve 210 separate single-trace
observations. The ordinary and accounting images
each passed 330 independent-oracle configurations through all seven variants
(2,310 executions), six C tree audits, and three native-container audits.
Both negative controls failed with their expected diagnostic and status 1.
The current normal/retained C/WF comparison also passed after its helper
selectors were migrated; generated aggregate-result `.body` helpers remain
outside the retained public boundary. No specification rule changed.

| Separate stage | Wall seconds |
| --- | ---: |
| First successful ecosystem construction | 10.766 |
| Ordinary/accounting correctness and negative controls | 1.786 |
| Allocation observations | 0.355 |
| Both timing cohorts, `ECO_SCALE=16` | 40.087 |
| Both replay cohorts, `ECO_SCALE=64` | 158.153 |

All timing tuples, variant/cohort checksums, sample identities, and summary
medians were checked against the raw rows. The shared reducer's `--complete`
check passes for the replay as a separate work setting. Every timed allocation
field is zero, and all 30 accounting cells match WF to source C exactly. The
observed clock quantum was 1,000 ns. Sample 0 is excluded from the following
medians. The complete replay clears both registered criteria: its shortest
ranked batch is 1.647 ms, and its largest between-cohort ratio spread is
9.5957%, for scalar count-256 hit/miss against Rust. No short or unstable
comparison remains under those criteria.

The table reports the range of the two cohort medians at 4,096 entries. WF
time is microseconds per complete trace, including construction, ordered
visitation, and cleanup. Each ratio is WF elapsed time divided by the named
native library's time; values above 1 mean WF took longer. All timing results
below use `ECO_SCALE=64`; they are not pooled with the original samples.

| Pair bytes | Path | WF µs/trace | WF / Rust `BTreeMap` | WF / C++ `std::map` | WF / Abseil `btree_map` |
| ---: | --- | ---: | ---: | ---: | ---: |
| 16 | Build/cleanup | 319–324 | 1.15–1.16 | 1.12–1.13 | 1.30–1.33 |
| 16 | Hit/miss | 895–913 | 0.90–0.92 | 0.96–0.98 | 1.02–1.04 |
| 16 | Replace/edit/remove/insert | 1,606–1,608 | 0.83–0.83 | 1.15–1.18 | 0.96–0.96 |
| 16 | Range of at most 16 keys | 916–941 | 0.92–0.95 | 0.98–1.00 | 1.18–1.20 |
| 16 | Replace only | 615–625 | 0.96–0.97 | 1.00–1.01 | 1.06–1.09 |
| 264 | Build/cleanup | 904–913 | 1.71–1.76 | 1.69–1.75 | 1.30–1.31 |
| 264 | Hit/miss | 1,678–1,697 | 1.20–1.21 | 1.24–1.26 | 0.96–0.97 |
| 264 | Replace/edit/remove/insert | 4,449–4,572 | 1.53–1.58 | 2.11–2.17 | 1.32–1.34 |
| 264 | Range of at most 16 keys | 3,400–3,496 | 1.09–1.11 | 1.14–1.16 | 0.96–0.96 |
| 264 | Replace only | 1,409–1,422 | 1.24–1.32 | 1.24–1.25 | 0.97–0.99 |

The C controls distinguish useful follow-up questions without establishing a
causal share. At 4,096 entries, scalar WF/source-C ratios range from 0.90 to
0.99 across the five paths; wide ratios range from 1.03 to 1.13. WF/direct-C
ratios are 1.11–1.56 on every wide path, while scalar WF is faster
on churn (WF/direct-C 0.91–0.92) and slower on hit/miss (1.11–1.15) and range
(1.27–1.29). The AVL control is substantially faster on scalar hit/miss
(WF/AVL 1.95–1.97), but slower on scalar range (0.85–0.85) and replacement
(0.86–0.87). Its wide construction, hit/miss and churn ratios are 1.73–1.86.
These are whole-trace comparisons with different layouts, algorithms and
public result ABIs, not isolated lookup or transport costs.

Size changes the ranking. At 256 scalar entries, churn takes WF 1.34–1.37
times Rust and 1.76–1.78 times Abseil, while the 4,096-entry ratios favor WF
or are close. Scalar hit/miss at 8 entries takes WF 1.73–1.75 times Rust,
1.77–1.80 times `std::map`, and 1.63–1.65 times Abseil. At 256 entries those
ratios are 1.66–1.82, 2.13–2.25 and 1.48–1.51, while source C is close to WF
(WF/source-C 0.98–1.02). Wide construction at 256 entries takes WF 2.14–2.16
times Rust, 2.25–2.26 times `std::map`, and 2.03–2.17 times Abseil. The
scalar/wide replacement-only contrast also persists against source C at 256
entries: 0.66–0.67 versus 1.41 despite identical allocation records. This
motivates a controlled emitted-code comparison;
transfers, call visibility and inlining remain hypotheses rather than measured
causes. No practical ratio selects a library or compiler change by itself.

### Requested allocation storage

Peak live requested bytes at 4,096 entries follow below. Construction rows
also represent the hit/miss, range, and replacement-only traces; churn can
change the tree shape. WF and source C match in every field.

| Representation | Scalar build peak | Scalar churn peak | Wide build peak | Wide churn peak |
| --- | ---: | ---: | ---: | ---: |
| WF / source C | 182,952 | 283,248 | 1,533,312 | 2,373,888 |
| Direct C B-tree | 137,240 | 137,616 | 1,495,040 | 1,499,136 |
| C AVL | 163,840 | 163,840 | 1,179,648 | 1,179,648 |
| Rust `BTreeMap` | 115,968 | 115,968 | 1,673,656 | 1,673,656 |
| C++ `std::map` | 196,608 | 196,608 | 1,212,416 | 1,212,416 |
| Abseil `btree_map` | 88,320 | 88,320 | 1,431,216 | 1,453,928 |

WF's peak node count grows from 363 during construction to 562 during
fixed-cardinality churn. That observed growth justifies revisiting the existing
occupancy question, with split/merge and per-node occupancy observations before
choosing a repair or representation change. Allocation count alone does not
predict elapsed time: WF makes 563 requests during the 4,096-entry churn trace,
versus Rust's 619 and `std::map`'s 12,288, while both native libraries complete
wide churn faster. At 256 entries, Rust instead makes 1,181 requests and WF
36; this stream also changes allocation behavior with population.

### Why the longer replay was needed

The initial matrix left seven cells inconclusive under the registered
duration/stability criteria. Scalar count-8 build/cleanup, hit/miss, range,
and replacement contain batches below 1 ms (minima 0.772, 0.411, 0.969 and
0.456 ms). Scalar count-256 hit/miss has 10.45–15.50% ratio spread between
cohorts; its range cell has 12.22% spread against Abseil. Wide count-256
hit/miss had 10.36% spread against direct C. The bounded same-source replay at
`ECO_SCALE=64` resolved these duration/stability flags. It quadruples the
whole-trace batch length and extends its seed sequence, leaving population,
per-trace rounds, executable, and oracle unchanged. The original samples
remain preserved; all final timing comparisons above consistently use the
replay. Use the guarded measure command above with `ECO_SCALE=64` and
`ECO_SAMPLE_FILE=.build/ecosystem/measurements-replay.csv`. From this directory,
validate that file separately:

```sh
perl ../summarize-ecosystem.pl --complete ordered=.build/ecosystem/measurements-replay.csv
```

Combining the two work settings into one statistical input is invalid.
