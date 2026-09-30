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

### Native attribution and unmeasured candidates

Read-only inspection of the measured arm64 `-O3` image
`.build/ecosystem/ordered-ecosystem-timed`, SHA-256
`8b7172e6bb10c5d93739efd5430a8e71de3edde76f594e486e49fdd2d35852d3`,
confirms the following surviving work at revision `0c3203aa6111` above.
These static observations do not quantify elapsed-time shares. The candidates
remain unmeasured and unselected; no production or design-tree change follows
from this inspection.

- **Drained-node consumption:** scalar and wide `ordered_map_free_link` copy
  504 and 4,224 bytes respectively before freeing the node, then recurse
  through the copied leading link. The wide helper reserves 4,336 stack bytes,
  including saved registers, and probes its frame. First test an ordinary
  library reformulation that detaches the leading link before consuming the
  now-empty node, preserving owner consumption and release order. Its predicted
  effect is removal of the whole-node temporary and copy without changing
  representation or allocation counts. A surviving copy/frame or no qualified
  whole-trace gain falsifies that benefit. General consumed-field lowering is
  a separate fallback, requiring compiler regression cases outside research.
- **Contiguous entry shifting:** wide `ordered_map_insert_item` retains a loop
  calling `memcpy` for each 280-byte entry shifted by non-full insertion.
  The general `emit_run_shift` implementation in
  `compiler/src/backend/emitter/runs.rs` emits this per-slot walk for both
  `Slots` and `Ring`. A bounded compiler candidate lowers contiguous `Slots`
  shifts to one overlapping bulk move while keeping the existing ring path.
  The falsifiers are a surviving per-element loop, no qualified gain, or a
  scalar/small-shift regression. Check front/interior/end and empty shifts,
  inline/runtime capacity, captured arguments, zero-sized elements and
  move-only owner order; preserve the admitted operation's descriptor updates.
- **Scalar query search:** `ordered_map_lookup_link` still scans 32-byte
  entries linearly, compares the selected key again, and recurses through
  ordinary calls with a 16-byte frame. Source C also retains linear recursive
  search. At 256 entries WF/source-C is 0.98–1.02 while WF/Rust is 1.66–1.82;
  this does not isolate a WF lowering loss. Direct C's query traversal is
  inlined and iterative, but its layout also differs, so subtracting its time
  cannot price recursion. Test lower-bound binary search under the same node
  layout, or separately retain the selected comparison result. Fewer
  comparisons without a qualified gain, incorrect hit/miss or range results,
  or regressions at other populations falsify the candidate. Keep existing
  hostile-comparator owner-safety coverage; ordering laws must not become
  safety assumptions.

The [single-descent candidate](#single-descent-insertion-candidate) and
[borrowed-promotion follow-up](#borrowed-promotion-follow-up) remain rejected.
At 256 wide pairs their normalized replacement regressions were
1.6214/1.6331 and 1.6885/1.6003 respectively. Removing recursive aggregate
result clearing did not cure the loss. Current native insertion still stages
an owned wide Pair through recursion, but that observation does not justify
reviving either patch. A later insertion-only carrier experiment would need
new evidence that the Boolean replacement fast path acquires no carrier
initialization or extra frame cost, followed by the complete replacement
matrix. The [recorded representation grounds](../../../../design/language/data-model/ordered-map-storage.md)
also remain in force: observe split/merge and occupancy before choosing a
different repair policy or node layout.

Compiler trials must use identical WF source before and after the lowering
change; library trials must hold callers, inputs, compiler, flags and harness
fixed. Inspect final native code and run the arbitrary-owner, ceiling
replacement/refusal, sorted/range traversal and complete-release observations
before timing. Compare both payloads, all five paths, all three populations
and both cohorts under the [registered target criterion](../ECOSYSTEM.md#optimization-criterion).
Preserve allocation observations and the original baseline; a local copy or
comparison-count reduction alone does not complete a cell's performance target.

### Prospective drained-node cleanup source trial

This criterion precedes construction and measurement; no result or production
change is selected. Freeze source at `7abd6bb34746b7b983b83103c68ee429b24859bb`
and compiler SHA-256
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`.
First reproduce the drained-node copy in fresh ordinary std native code:
the older measured image above is a lead, not evidence about this compiler.

The sole library change detaches the leading link after the unchanged entry
loop, then consumes the drained node with both member windows empty:

```whitefoot
let leading = slots_new::<Box<OrderedNode<K, V>>, 1>();
append(destination: &leading, source: &node.inner.leading);
let OrderedNode(leading: empty_leading, entries: empty_entries) = move node.inner;
free_empty(window: move empty_leading);
free_empty(window: move empty_entries);
ordered_map_free_link::<K, V, F, fn consume>(link: move leading, env: env);
```

The new capacity-one link fits the zero-or-one child and receives its owner;
the old link becomes empty. Entry callbacks and following-child recursion
retain their order, and parent release still precedes leading-child recursion.
No representation, allocation policy, public contract, proof or lowering changes.
Predict removal of the 504/4,224-byte drained-node copies and the large wide
cleanup frame. A surviving copy/frame, changed owner/order, or unchanged native
code refuses this candidate before correctness execution or timing.

`LIBRARY_SOURCE` is only a Make dependency/identity: the frozen compiler embeds
std sources. Use identical ordinary `pkg::collections::ordered_map` carriers
with the unchanged interface and callers, remapping only the import prefix.
Before candidate attribution, require fresh pkg-baseline/std native equivalence
for both trace bridges and every reachable map helper; normalize only stable
symbol names, function ordering and relocation addresses. A material code
difference refuses the carrier. Reuse verified identical native-peer objects.

After the code screen, run the paired maintained
[owner program](../../../../tests/programs/containers/ordered-map-program.wf)
in both lowering modes with ordinary release and the existing
[103-allocation observer](../../../../tests/programs/containers/container-allocation-observer.c).
Keep its nested/nodrop, replacement/refusal, hostile-comparator and structural
mutation coverage. Add an independent 16-key cleanup chronology witness:
ascending insertion gives callbacks `7,15..8,6..0`, releasing the right node,
then the parent between callbacks 8 and 6, then the left node. Distinct callback
and release-order mutations must fail it. Existing unordered ledgers do not
establish that chronology. Run existing normal/retained and structural checks,
plus ecosystem timed/accounting oracles, checksum/cleanup fault controls and
the complete allocation matrix; require paired allocation records to match.

Then compare all 30 complete-trace cells at `ECO_SCALE=64`, with unchanged
callers, compiler, flags, peers and five retained samples per cohort. Preserve
both arms and every sample. A strict gain/loss requires disjoint paired ranges
in the same direction in both cohorts, all paired samples at least 1 ms and
cohort median-ratio spread at most 10%; overlap remains inconclusive. Selection
requires a qualified useful-cell gain and no strict qualified useful-cell loss.
The separate [standard-peer target](../ECOSYSTEM.md#optimization-criterion)
still applies per cell; a cleanup gain alone does not complete that target.

#### Native and correctness screen

The criterion was published at `262c662a46ffb06e7dc62a482ff02bfbb214e2e1`
before this screen. The frozen compiler reproduced both old cleanup copies.
Fresh std and pkg-baseline objects have identical bytes, addresses and sizes
in all five sections; all 29 functions, symbols and relocations match after
only the `std.collections` to `collections` prefix substitution. Between the
two pkg arms, only the scalar and wide cleanup helpers change; the other 27
function bodies and their relocations match after relative-address comparison.

| Cleanup specialization | Drained-node copy, before → after | Frame, before → after |
|---|---:|---:|
| Scalar | 504 → 0 bytes | 624 → 128 bytes |
| Wide | 4,224 → 0 bytes | 4,336 → 144 bytes |

The wide cleanup stack probe disappears. A conditional copy of the zero-or-one
leading pointer remains (at most 8 bytes), followed by parent release and then
leading recursion. Inspection of the subsequently linked `-O3` images confirms
the same code change. All 48 reused native-peer/runtime artifacts retained
their verified hashes; their 59 source/header/IR/build inputs match the frozen
source and recorded construction settings.

Both arms pass the maintained owner program and exact 103-allocation observer
in sequential and parallel lowering. A separate 16-key chronology fixture and
combined payload/node release observer pass in both modes. The callback-order
mutation fails both ordinary and observed executions. The release-order
mutation preserves the ordinary callback result but fails the release observer,
demonstrating the additional observation. The selected rewrite below retains
this fixture and observer in formal program tests.

Each arm's timed and accounting ecosystem images pass 330 configurations,
2,310 executions, six C tree audits and three native audits; checksum and
cleanup faults produce their expected failures. Normal and retained checks
each pass 330 configurations, 1,320 executions and six native audits. All ten
required structural paths are nonzero; the unchanged successor-only source
algorithm reports predecessor count zero. Paired structural records and all
210 allocation rows (30 cells, seven variants) are byte-identical. The complete
correctness execution took 13.43 seconds, separate from construction.

Sources, commands, phase logs, raw native code, identities and the two distinct
chronology faults are retained under
`/private/tmp/whitefoot-ordered-cleanup-source-ux6gm4u5`; the entry records are
`IDENTITIES.json`, `native-screen.json` and `correctness-results.json`.
The linked timed-image SHA-256 values are baseline
`d380670480fc374d99c5a82551374e6cd1c55c36c09c828300466f49822ea1be`
and candidate
`f6e060ebb289d9a7484a5d95106c6e4333d6b0d860cce3982b2f1855ad27de99`.
This native/correctness screen preceded the separately authorized timing
pair below; it established no runtime benefit on its own.


#### Paired timing selects the cleanup rewrite

The single preregistered pair uses the frozen images above at `ECO_SCALE=64`
in B0/C0/C1/B1 order. The [baseline](ecosystem-drained-cleanup-baseline-samples.csv)
and [candidate](ecosystem-drained-cleanup-candidate-samples.csv) each retain all
2,520 rows: 30 cells, seven implementations, two cohorts, warmup sample 0 and
five measured samples. No rebuilding occurred. Cohort execution took
77.547 / 77.248 / 77.258 / 77.296 seconds (309.349 seconds total); the complete
guarded command took 309.68 seconds. All four direct statuses are zero, and
all 121 recorded correctness/reuse/compiler artifact hashes match afterward.
The clocks report and observe a 1,000 ns quantum in both arms.

Two cells have qualified gains, none has a qualified loss, and 28 overlap.
Wide build/cleanup at 8 entries improves 7.85% / 6.91%; at 256 entries it
improves 4.49% / 6.35%. All 30 paired comparisons qualify: the shortest WF
sample is 1.819 ms and the largest cohort-ratio spread is 4.9013%. This meets
the recorded source-change selection rule. The exact measured library patch
is selected and integrated with the independent owning-value chronology
fixture and release observer in the maintained container program tests.
The maintained `programs::containers::ordered_map_` filter subsequently passes
both the existing owner/oracle test and the new callback/parent-release-order
test: two executed, none failed or ignored, 5.206596 seconds. The integrated
source hashes were unchanged across the combined compiler construction and
execution. That integration check used CLI SHA-256
`123e00bd26a82c05493657b6a2c61317354a190ca430a9c0ad6af0c8c51761be`;
it is separate from the frozen compiler and images used for the timing pair.
Direct statuses and test names are retained in
`/private/tmp/whitefoot-consumed-projection-verification/ordered-map-execution.json`
and its companion `.stdout`.
The maintained TODO's stale claim that exhausted-node cleanup still needs
copy removal is corrected; unresolved node construction/materialization remains
tracked separately. This is a routine current-guidance fix under the same
selected cleanup behavior. The ecosystem overview's current summary and latest-image
row now name this selected result; its frozen baseline and pre-optimization
attribution remain historical.

The table retains every cell. C/B divides complete-trace medians, candidate
by baseline; G is a qualified separated-range gain and I is overlapping,
inconclusive evidence. The final columns classify the candidate against Rust,
C++, and the median-slower standard target: P pass, D deficit, I inconclusive.
No mean across cells or pure-operation latency is inferred.

| Pair bytes | Trace | Count | C/B, cohorts 0 / 1 | Pair | Rust | C++ | Target |
|---:|---|---:|---:|:---:|:---:|:---:|:---:|
| 16 | build/cleanup | 8 | 0.958027 / 0.953580 | I | D | P | P |
| 16 | build/cleanup | 256 | 0.998861 / 1.015372 | I | D | P | P |
| 16 | build/cleanup | 4096 | 0.984217 / 0.995554 | I | D | D | D |
| 16 | hit/miss | 8 | 0.999817 / 0.982485 | I | D | D | D |
| 16 | hit/miss | 256 | 0.998030 / 1.025489 | I | D | D | D |
| 16 | hit/miss | 4096 | 0.994887 / 0.993855 | I | P | I | P |
| 16 | churn | 8 | 1.000229 / 1.001983 | I | P | P | P |
| 16 | churn | 256 | 1.001078 / 1.000712 | I | D | D | D |
| 16 | churn | 4096 | 0.992433 / 1.000719 | I | P | D | P |
| 16 | range-16 | 8 | 1.003154 / 0.977706 | I | P | D | P |
| 16 | range-16 | 256 | 0.970454 / 1.018018 | I | D | D | D |
| 16 | range-16 | 4096 | 0.988873 / 0.997029 | I | P | I | P |
| 16 | replace-only | 8 | 1.000000 / 0.998902 | I | P | P | P |
| 16 | replace-only | 256 | 0.983336 / 0.987582 | I | D | D | D |
| 16 | replace-only | 4096 | 1.003437 / 0.995309 | I | I | I | I |
| 264 | build/cleanup | 8 | 0.921456 / 0.930860 | G | D | D | D |
| 264 | build/cleanup | 256 | 0.955050 / 0.936522 | G | D | D | D |
| 264 | build/cleanup | 4096 | 0.981144 / 0.974009 | I | D | D | D |
| 264 | hit/miss | 8 | 1.026813 / 1.006088 | I | I | D | I |
| 264 | hit/miss | 256 | 0.982536 / 1.014797 | I | D | D | D |
| 264 | hit/miss | 4096 | 0.984719 / 0.986021 | I | D | D | D |
| 264 | churn | 8 | 1.003183 / 1.000967 | I | D | D | D |
| 264 | churn | 256 | 0.998394 / 0.996758 | I | D | D | D |
| 264 | churn | 4096 | 0.983441 / 0.987096 | I | D | D | D |
| 264 | range-16 | 8 | 0.996717 / 0.996882 | I | I | D | I |
| 264 | range-16 | 256 | 0.991594 / 0.982827 | I | D | D | D |
| 264 | range-16 | 4096 | 0.973370 / 1.001603 | I | D | D | D |
| 264 | replace-only | 8 | 0.996129 / 1.001229 | I | D | D | D |
| 264 | replace-only | 256 | 0.989364 / 0.991324 | I | D | D | D |
| 264 | replace-only | 4096 | 0.968718 / 0.983795 | I | D | D | D |

Four cells have adverse medians in both cohorts while their ranges overlap:
scalar churn at 8 and 256 entries, wide hit/miss at 8, and wide churn at 8.
The largest is wide hit/miss at 8 (2.68% / 0.61% higher medians). Across all
30 cells, 16 have lower medians in both cohorts, four have higher medians in
both, and ten have mixed directions or a tie. These descriptive directions do
not replace the registered range discriminator.

| Comparison | Baseline P / D / I | Candidate P / D / I |
|---|---:|---:|
| Rust `BTreeMap` | 6 / 21 / 3 | 6 / 21 / 3 |
| C++ `std::map` | 5 / 22 / 3 | 4 / 23 / 3 |
| Slower-standard target | 8 / 19 / 3 | 8 / 19 / 3 |
| Abseil `btree_map` reference | 3 / 20 / 7 | 4 / 22 / 4 |

The target total is unchanged: scalar range-16 at 8 changes I to P, while
scalar replace-only at 4,096 changes P to I. Candidate wide churn at 4,096
still takes 1.5324 / 1.5246 times Rust and 2.1874 / 2.1839 times C++; removing
the cleanup copy does not complete the 30-cell standard-peer target.
Every ranked implementation sample exceeds 1 ms (minimum 1.645 ms). All
standard-peer comparisons qualify; baseline scalar range-16 at 256 against
Abseil is unqualified at 17.9113% cohort spread and remains I. Candidate versus
baseline Rust/C++ median drift reaches 7.1484%; all 180 unchanged native/control
pair comparisons remain inconclusive. Raw variation and the C++ pass-count decrease are
retained rather than interpreted as additional source-change gains or losses.

The raw baseline/candidate SHA-256 values are
`3534e2e92b7b1cfeef71983e21036df6eedc01ac9a9e2731ecc0acc92546e236`
and `2afa940dd05abac9156d3bb7c7fdaa872f4861dd929353a073820339d100454b`.
In the same scratch evidence directory, `timing-plan.json` freezes runner and
reducer hashes before timing; `timing/execution.json` records commands,
statuses, durations and pre/post identities. `timing/pair-comparisons.csv`
retains both cohort bounds for all seven paired implementations, and
`timing/peer-comparisons.csv` retains every per-peer classification. Replay
native summaries and the target from either committed raw file with the
maintained reducer:

```sh
perl ../summarize-ecosystem.pl --complete ordered=ecosystem-drained-cleanup-candidate-samples.csv
perl ../summarize-ecosystem.pl --targets ordered=ecosystem-drained-cleanup-candidate-samples.csv
```


### Prospective aggregate-opening-only Slots trial

This is a new isolated compiler factor, not selection of the
[refused blanket Slots trial](../vector-library/RESULTS.md#slots-final-code-and-paired-timing-regression-prevents-selection).
That trial changed both shift directions for every Slots element and lost
13.9–14.5% on small scalar reuse. It also added wide removal spills. The new
candidate selects only `open && Slots && is_stored_aggregate`, using the
backend's existing representation predicate: scalar and Box-pointer elements,
all closing/removal shifts and all Ring shifts keep their present walk.
This addresses those two exposure paths without assuming that aggregate
insertion's call setup is free. No size threshold or container-name dispatch
is introduced. The proposal remains in the
[pending amendment](../../../../design/amendments/contiguous-slots-shifts.md).

Freeze the isolated compiler at `7abd6bb34746b7b983b83103c68ee429b24859bb`
plus this factor; both arms use the exact selected cleanup source from
`846275596c74e1df011ab8f99b1ed61d9576e359`, SHA-256
`1f70037928f71fbf69475ec596b56cb52904540482b823e9d2bdd549c6b51493`.
The baseline CLI is
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`.
That CLI embeds the older std cleanup: use the same explicit ordinary pkg
carrier and import graph for both Ordered arms. First reproduce the complete
selected-cleanup pkg module and native bodies from the preceding trial;
record compiler/source/flags/object hashes before changing the lowering.
Vector keeps its unchanged current source. Other pending compiler factors
are excluded. Scratch preparation is under
`/private/tmp/whitefoot-slots-aggregate-opening-ks7ia9u0`; no construction,
execution or timing result is asserted here.

For eligible insertion, predict one overlap-safe move of `[index, len)` by
one target stride, replacing the final per-entry 280-byte copies in wide
Ordered insertion. Keep the insertion placement, one length increment,
callback order, allocations, owner outcomes and selected cleanup unchanged.
Zero-byte elements normalize address/extent operands only, retaining logical
indices and lengths. The existing byte-transfer helper carries target padding
without typed padding loads. Split-node materialization is a separate factor.
The scratch prototype also corrects the stale IR comment claiming that every
Slots shift is one memmove; OP-10 is not a native-call performance guarantee.

Before execution, require the predicted change in final optimized bodies,
including the ordinary timed caller; raw IR alone is insufficient. Compare
unaltered scalar Vector bodies and all closed/removal/Ring controls in raw IR
and native instructions/relocations, normalizing only stable symbol spelling
and function-relative addresses. A changed unaffected body, surviving eligible
per-entry transfer, or changed cleanup body refuses this factor's code screen.
Pass fixed/runtime shape checks, padded owning records, empty endpoint shifts,
huge zero-stride logical counts with address facts emitted and withheld, and
independent omitted-transfer/omitted-length faults. Then pass the maintained
[container owner and chronology tests](../../../../compiler/tests/programs/containers.rs),
normal/retained/structural Ordered checks, both families' complete ecosystem
oracles and checksum/cleanup faults, and byte-equal allocation accounting.
Reuse verified peer/runtime objects; record build and execution time separately.

Only after these screens, run one full paired Vector 42-cell and Ordered
30-cell comparison with unchanged peers and inputs. Vector runs baseline then
candidate, each `measure 1048576 7` invocation emitting both internal cohorts;
Ordered uses B0/C0/C1/B1 at `ECO_SCALE=64`. Preserve their retained warmups and
all samples; the six Vector suffix-zero controls stay unranked. Use the
existing paired range discriminator: disjoint ranges in the same direction
in both cohorts, samples at least 1 ms and cohort median-ratio spread at most
10%. Selection needs a qualified useful-cell gain and no strict qualified
useful-cell loss across either matrix. Overlap remains inconclusive; retain
all adverse medians and native-peer drift. Evaluate the separate
[slower-standard target](../ECOSYSTEM.md#optimization-criterion) for every
meaningful cell. No source-language rule or live-tree decision changes here.


#### Aggregate-opening native screen

The native screen passes. The subsequent correctness screen is recorded below.
The frozen CLI admits all 17 authored fixtures, the unchanged Vector fixture,
and the selected Ordered pkg graph. The first attempt rejected the twelve
shape fixtures' one-line Record declaration with FORM-2; only canonical
newlines were repaired, with original sources and failed logs retained.
Before candidate construction, fresh selected-cleanup Ordered raw LLVM,
timed LLVM and the complete O3 object reproduce the preceding trial's hashes
exactly (`e46d6598…`, `098af692…`, `a531a253…` respectively).

The isolated candidate CLI SHA-256 is
`ff7def76e016e25082e9bd73561fa1422f1eeab05ec27bf48a5be0a9ac985312`;
the 2,394-file source-freeze manifest is
`c077f73ad571c8c505a4c53398a201f7437120756b858241029e4cd468442f9a`.
One gate/jobs2 CLI build takes 61.290 seconds; tool identity, emission and
native construction take 10.905 seconds separately. The outer guarded stage
is 72.91 seconds, exit zero. All direct construction statuses are zero; no
unit harness, native peers or runtime were rebuilt, and no generated program
was executed. Source and fixture hashes remain unchanged across construction.

Across 19 paired modules, nine raw function bodies change (eight distinct
`insert_at` symbols, with one specialization used in two fixtures). Each has
one stride-range move and no opening loop. All 28 closing-body occurrences
are byte-equal in raw IR; all eighteen retained native closing kernels match
in instruction/relocation records. Eight generated aggregate-body helpers
are absorbed into those unchanged wrappers; two Ordered closing instances
inline into callers that also perform eligible opening shifts. Twelve
complete scalar, Box-pointer and Ring control modules have
byte-identical raw LLVM and native objects. Vector's thirty-function scalar
call closure and both selected Ordered cleanup helpers remain unchanged.
The zero-stride candidate also has an identical final object.

| Native body | Instructions, baseline → candidate | Frame bytes, baseline → candidate |
|---|---:|---:|
| Ordered wide insert-item | 1,476 → 1,418 | 9,056 → 9,056 |
| Vector wide insert-at | 68 → 52 | 272 → 304 |
| Vector wide grow-vector-insert | 78 → 58 | 304 → 304 |
| Vector wide ordinary work | 441 → 407 | 688 → 720 |

Ordered's nonfull wide insertion now performs one `memmove` of
`280 * (len - index)`, then the same length increment and 280-byte incoming
placement. Vector's wide opening uses `256 * (len - index)`. The extra 32
frame bytes in its ordinary caller and standalone insert remain a competing
cost; no runtime benefit follows from the instruction counts alone.

Nine Ordered native bodies change: wide insert-item, scalar insert-link,
both remove-link, both repair-child, both repair-separator, and scalar take-min.
The rebalance/remove helpers contain opening insertions through borrow-left,
separator reinstallation or finish-remove; their actual closing kernels stay
unchanged. Scalar remove-link's frame also rises from 160 to 176 bytes. The
other twenty Ordered native bodies and fifty-six Vector bodies are unchanged.
Actual Mach-O instruction words and per-instruction relocation targets agree
with the separate assembly inventory; constant/literal bytes match in every
module. Only unrelocated direct branch destinations are normalized to their
function and offset in the object comparison.

The scratch evidence directory above retains `native-screen.json`,
`baseline-reproduction.json`, `function-inventory.json`,
`object-function-inventory.json`, all raw/optimized LLVM, assembly, objects,
and direct stage logs. The complete changed-body inventory includes each
rebalance helper's source-call path to its eligible opening. The registered
owner/chronology/fault/accounting checks remain prerequisites to any full
Vector/Ordered timing pair.


#### Aggregate-opening correctness screen

The frozen candidate passes all six focused tests: the three new opening-shift
checks, maintained Vector ownership, and Ordered ownership and cleanup order.
One gate/jobs2 lib/corpus construction takes 103.359 seconds; focused execution
takes 15.226 seconds. All 2,394 frozen source files remain unchanged.

The full paired continuation reuses native peers/runtime whose 63 source
identities match the chosen revision. Its baseline Vector raw LLVM, timed LLVM
and object reproduce the retained image; both families' accounting transforms
and Ordered's normal/retained LLVM also reproduce the previous baselines.
There is no compiler or peer rebuild in this continuation: construction takes
15.776 seconds and execution 14.793 seconds, with guarded exit zero in 31.52
seconds. All 52 construction commands pass; the 63 execution commands have 41
expected successes and 22 expected fault failures.

| Contract, per image in each arm | Vector configurations / executions | Ordered configurations / executions |
|---|---:|---:|
| Ecosystem timed and accounting | 1,260 / 8,820 | 330 / 2,310 |
| Normal and retained O2 | 1,260 / 6,300 | 330 / 1,320 |

Both allocation ledgers are byte-equal across arms and to their retained
baselines: Vector has 294 rows and Ordered has 210. Retained WF/C call audits
pass; all ten required C structural paths remain observed. The 32 paired
owner/chronology commands preserve 103 allocations and the 16-callback/three-node
release sequence in ordinary and parallel lowering. Callback-order faults
fail both source and native observers; release-order faults preserve the
ordinary source result and fail the independent native release observer.
The formal chronology observer differs from the earlier scratch observer only
in its opening three comment lines; all following bytes are identical.

`correctness-tests/{build,execution,summary}.json` and
`continuation/{stages,result,images,native-audits,artifacts}.json` retain direct
statuses, outputs and identities in the existing scratch evidence directory.
The continuation preserves 316 artifact hashes and 48 image hashes; all frozen
inputs remain unchanged. The extra Vector 32-byte frames and Ordered scalar
16-byte frame remain competing costs in the following complete pair;
correctness alone does not select the compiler factor.


The exact timing continuation was frozen before execution in `timing/plan.json`
and `timing/inputs.json`: Vector baseline then candidate, each
`measure 1048576 7`; then Ordered baseline cohort 0, candidate cohort 0,
candidate cohort 1, baseline cohort 1, each `measure <cohort> 64`. Vector's
existing warmup is unrecorded and all sample IDs 0–6 remain ranked; Ordered's
recorded sample 0 alone is excluded from ranking. The six commands retain
4,116 rows per Vector arm and 2,520 per Ordered arm. The estimate was 470–540
seconds with zero construction and a 130-second per-command investigation
limit. All six commands pass without an overrun: Vector takes 80.654/80.357
seconds and Ordered B0/C0/C1/B1 takes 77.220/77.349/77.716/77.320 seconds.
Total timing execution is 470.617 seconds; its stage performs no construction.
The selection criterion and all 72 cells above remain unchanged.


#### Aggregate-opening full pair refuses selection

The factor is refused: three qualified useful gains and two qualified useful
losses fail the prospective no-loss criterion. Ordered scalar count-8
construction/cleanup regresses 2.92% / 8.99%, and its churn trace regresses
5.04% / 5.49%. Wide construction at 256/4096 and wide count-8 range qualify as
gains. These are complete traces including construction and cleanup.
No production compiler change or timing rerun follows this result.

All 13,272 raw rows are retained byte-for-byte: Vector
[baseline](../vector-library/ecosystem-aggregate-opening-baseline-samples.csv) /
[candidate](../vector-library/ecosystem-aggregate-opening-candidate-samples.csv),
Ordered [baseline](ecosystem-aggregate-opening-baseline-samples.csv) /
[candidate](ecosystem-aggregate-opening-candidate-samples.csv).
The [72-cell comparison](ecosystem-aggregate-opening-paired.csv) retains both
arms' medians and ranges, qualification failures and adverse medians.
The [replay patch](aggregate-opening-candidate.patch) and
[identities/commands](aggregate-opening-identities.json) pin compiler, source,
ordinary pkg carrier, peers/runtime, images, direct statuses and reduction.
The measured patch `413b5f92...` remains unchanged in retained scratch. The
public replay `bdc1e426...` uses zero-context diff formatting to remove
whitespace-only context lines; `git apply --unidiff-zero` reproduces all
four measured compiler files exactly and reverses to the pinned revision.
The independent root reduction agrees on every paired classification and all
288 standard-peer and 432 native-control comparisons.

Every cell below reports candidate/baseline median ratios for cohorts 0 / 1.
G/L are qualified separated gain/loss; O has overlapping ranges and remains
inconclusive for a stable gain; I fails qualification; U remains an unranked
suffix-zero control. Vector has 34 useful overlaps and two unstable useful
cells: wide suffix-1 at 16/4096 has 18.43%/34.05% cohort-ratio spread. Its three
scalar U cells include sub-millisecond samples. Ordered has three gains, two
losses and 25 overlaps; every cell meets duration and stability requirements.

Vector element widths:

| Bytes | Path | Count 16 | Count 256 | Count 4096 |
|---:|---|---:|---:|---:|
| 8 | reserved | 0.999 / 1.001 O | 0.967 / 0.996 O | 1.011 / 1.012 O |
| 8 | growth | 1.003 / 1.011 O | 0.998 / 0.995 O | 0.982 / 1.002 O |
| 8 | reuse | 1.012 / 0.997 O | 1.000 / 1.040 O | 1.010 / 0.999 O |
| 8 | suffix-0 | 0.995 / 0.994 U | 1.003 / 1.043 U | 0.999 / 1.032 U |
| 8 | suffix-1 | 0.988 / 1.014 O | 0.996 / 0.997 O | 0.991 / 1.001 O |
| 8 | suffix-2 | 1.000 / 1.001 O | 1.010 / 1.003 O | 0.977 / 0.999 O |
| 8 | suffix-3 | 1.001 / 0.993 O | 0.999 / 0.996 O | 1.000 / 0.967 O |
| 256 | reserved | 0.974 / 1.004 O | 1.000 / 1.001 O | 1.002 / 0.999 O |
| 256 | growth | 1.004 / 1.008 O | 0.997 / 1.008 O | 0.990 / 0.993 O |
| 256 | reuse | 1.003 / 0.999 O | 0.999 / 0.997 O | 0.999 / 1.001 O |
| 256 | suffix-0 | 1.000 / 1.004 U | 1.003 / 1.000 U | 0.998 / 0.991 U |
| 256 | suffix-1 | 0.843 / 0.998 I | 0.882 / 0.928 O | 1.143 / 0.852 I |
| 256 | suffix-2 | 1.002 / 0.999 O | 1.008 / 1.003 O | 1.003 / 1.001 O |
| 256 | suffix-3 | 0.998 / 1.002 O | 1.000 / 1.003 O | 1.002 / 0.998 O |

Ordered pair widths:

| Bytes | Path | Count 8 | Count 256 | Count 4096 |
|---:|---|---:|---:|---:|
| 16 | build-cleanup | 1.029 / 1.090 L | 1.026 / 1.058 O | 1.020 / 1.028 O |
| 16 | hit-miss | 1.059 / 1.011 O | 0.964 / 1.026 O | 1.016 / 1.018 O |
| 16 | range-16 | 1.005 / 1.038 O | 0.993 / 0.999 O | 1.003 / 1.007 O |
| 16 | replace-only | 0.999 / 1.006 O | 1.004 / 1.042 O | 1.014 / 1.014 O |
| 16 | replace-edit-remove-insert | 1.050 / 1.055 L | 1.008 / 0.991 O | 1.032 / 1.035 O |
| 264 | build-cleanup | 1.002 / 0.993 O | 0.950 / 0.939 G | 0.943 / 0.953 G |
| 264 | hit-miss | 1.008 / 0.991 O | 0.994 / 1.005 O | 0.959 / 0.972 O |
| 264 | range-16 | 0.979 / 0.991 G | 1.003 / 0.995 O | 0.983 / 0.985 O |
| 264 | replace-only | 1.012 / 1.003 O | 0.992 / 0.994 O | 0.979 / 0.968 O |
| 264 | replace-edit-remove-insert | 0.991 / 0.997 O | 1.023 / 1.016 O | 0.986 / 0.998 O |

Useful standard-peer and slower-standard target counts are pass/deficit/
inconclusive; Vector's six unranked controls are additional in each arm.

| Family / arm | Against Rust | Against C++ | Slower-standard target |
|---|---:|---:|---:|
| Vector baseline | 0/17/19 | 16/4/16 | 16/4/16 |
| Vector candidate | 0/16/20 | 16/3/17 | 16/3/17 |
| Ordered baseline | 7/22/1 | 6/22/2 | 9/19/2 |
| Ordered candidate | 6/22/2 | 3/24/3 | 7/20/3 |

The identity record retains every native control's cohort-median drift range
and qualification counts; the raw files support the unchanged complete reducer.
Five useful native comparisons also separate: C++ Vector scalar growth/4096
and C++ Ordered scalar churn/256 and /4096 improve, while C++ Ordered wide
churn/256 and Rust Ordered wide replacement/256 worsen. All six native peers
overlap in each of the two scalar WF loss cells. No causal timing share is
assigned to fewer transfer instructions or changed frames. The latest retained
production image remains the selected cleanup; fresh baseline peer counts here
do not identify an implementation change.

Reproduce standard comparisons with `../summarize-ecosystem.pl --complete`
and `--targets`, passing `vector=<raw-path> ordered=<raw-path>` for each arm.
The paired formula is recorded above and in the identity record. Native replay
starts from the pinned source revision plus the selected cleanup/test files and
published compiler patch applied with `git apply --unidiff-zero`; use the
recorded ordinary pkg graph, unchanged
fixture, O3 flags and exact peer/runtime link order. Require the published
baseline raw/timed/object hashes to reproduce before attributing another image.
The detailed 288-row peer and 432-row drift tables, failed authoring attempt,
all native artifacts and direct logs remain in the retained scratch directory.

#### Empty-suffix calls exposed by the refused pair

The scalar pair is 16 bytes, but its shifted `OrderedEntry` is 32 bytes after
adding the child-link window; the wide entry is 280 bytes. The baseline scalar
opening uses an inline `ldp`/`stp` register-copy loop behind `len > index`.
The candidate calls `memmove(32 * (len - index))` unconditionally. At count 8,
`(index * 73) % 8 == index`, so initial construction appends all keys: seven
zero-byte shift calls follow root creation, with no split or removal.
Its insert-link frame actually shrinks 1,136→1,120 bytes; the remove frame
cannot explain this build-path loss.

Count-8 churn stays in one leaf. Each round reinserts at positions 0–7, moving
224, 192, …, 0 bytes. At 1,024 rounds and 64 traces, a measured sample has
65,984 zero-byte opening calls, 458,752 positive-span opening calls and 524,288
leaf remove-link entries. These are source/native path counts, not sampled
call counts or elapsed-cost attribution. The closing loop stays unchanged,
but the function's new child-replacement opening path alters register allocation:
its leaf path gains two stores and one reload, and its frame grows 160→176
bytes. A guard may alter that allocation; neither disappearance nor a runtime
benefit is assumed. Small positive-span calls remain a separate competing cost.

#### Prospective guarded aggregate-opening trial

This criterion was recorded before construction and timing. The completed
refusal is recorded below; the original criterion is retained in the evidence
archive. The registered isolated factor addresses the empty-transfer mechanism above. It
keeps exactly `open && Slots && is_stored_aggregate`, adds the original
`len > index` zero-trip guard around the stride-span move, and statically
omits RunShift's address/extent/copy emission when the existing selected-target
layout helper establishes zero stride. Both paths join before the unchanged
single length increment. Incoming-value capture and RunInsert placement,
including their existing zero-sized copies, stay unchanged. No byte threshold,
container/count dispatch, public representation or language rule changes.

Freeze the same `7abd6bb34746b7b983b83103c68ee429b24859bb` compiler plus
selected cleanup, with a separate guarded patch over the retained unguarded
factor. The ordinary Ordered pkg carrier and all 23 fixture/carrier input files
are byte-identical to the preceding screen. Preserve its adverse raw samples
and candidate image. Scratch preparation is under
`/private/tmp/whitefoot-slots-guarded-opening-8ac7bw5m`; `IDENTITIES.json`,
`source-freeze.json` and both the full candidate and incremental delta patches
pin the proposal before construction. Those preparation records asserted no
native or execution result; subsequent observations are recorded below.

Before execution, require the selected-cleanup baseline module/object to
reproduce. Empty-suffix paths in fixed/runtime wrappers and both Ordered leaf
insertions must bypass shift transfers in optimized native control flow.
Zero-stride raw shift blocks must contain no transfer or shift address/extent,
while the final bodies preserve logical length. Positive wide openings must
retain one overlap-safe `stride * (len - index)` move with no per-entry loop.
Compare all prior scalar/Box-pointer/Ring controls, true closing kernels and
selected cleanup bodies; inventory every changed caller, including executed
scalar insert/remove frames and spills. The previous +32-byte Vector frames
and +16-byte Ordered frame remain competing costs, not promised removals.
Any surviving empty shift call, restored wide per-entry copy, changed owner or
length outcome, or altered unaffected body refuses this code screen.

Then require the existing padded owning, endpoint and huge zero-stride tests
with both address-fact choices, alongside normal/parallel lowering. A native
shift observer must reject an independent guard-bypass fault that calls it for
zero bytes; omitted positive transfer and omitted length-update faults must
still fail their existing observers. Preserve maintained Vector/Ordered owner
and chronology tests, both complete ecosystem oracles, normal/retained/structural
checks, checksum/cleanup faults and byte-equal allocation accounting. Reuse
verified native peers/runtime; record construction and execution separately.

Only after those screens, use the original full Vector 42-cell and Ordered
30-cell pair, cohort order, warmup handling, duration/stability qualifications
and disjoint-range rule. Selection still requires at least one qualified useful
gain and zero qualified useful losses across both matrices. Report every prior
gain/loss cell, all adverse medians and both peer/target results. Overlap remains
inconclusive. A remaining churn loss may expose small positive-transfer or
caller-layout cost; it does not authorize a tuned size threshold or another
timing run. This proposal remains pending in the existing amendment.


#### Guarded aggregate opening: full matrix still refuses selection

The guarded candidate is refused under the unchanged full-matrix criterion:
Ordered has three qualified useful losses despite five gains, and neither
Vector replicate has a qualified useful gain. Removing empty-suffix shift
calls and zero-stride shift work passes the native screen, but does not
establish acceptable whole-program cost. The candidate stays archived research
source; no production lowering, representation or language rule is selected.
The existing amendment records this proposed disposition without an owner ruling.

The [evidence archive](guarded-aggregate-opening-evidence.tar.gz) retains all
eight raw CSVs, statuses, original registration, input/image hashes, complete
cell and peer tables, native inventories, correctness observations and both
original and repaired readers. Its SHA-256 is
`de7769b3caada3e0ba019afc9b725e374ccec95cb62e3da037cd5f3f60300b19`.
`INDEX.json` distinguishes original identities from hashes of redacted public
records; raw CSV bytes are unchanged. Preparation records retain their original
then-unexecuted flags. The [candidate replay](guarded-aggregate-opening-candidate.patch),
SHA-256 `1da31b073190daa51208d989af1496f2e539d7c428db13f0b724ab4b5c80104f`,
applies with `git apply --unidiff-zero` to the pinned
`7abd6bb34746b7b983b83103c68ee429b24859bb` source plus the previously recorded
selected cleanup/test overlay. An isolated replay matches all four measured
candidate compiler files byte for byte; the patch is not production code.

The frozen baseline CLI is
`5753999f224f89a9a99e0399b76bfd7b92cd53d5f22b0d204df4f2ec3ffbc20b`;
the guarded CLI is
`68ecfbbe1f7a29e0320199a6cc1a1d2d893fc68a0ac21cffd1be3412ac116517`.
Both families retain unchanged sources, native peers, work and link order.
This is evidence for that compiler lineage, not a current-main comparison.

The screen reproduces the selected-cleanup baseline module/object. Guarded
positive aggregate openings retain one complete-stride overlap-safe transfer;
empty suffixes bypass that transfer and zero-stride shifts emit no transfer or
shift address/extent. Closing paths, scalar/Box/Ring controls and selected
cleanup bodies remain unchanged. The complete inventories retain all changed
callers and competing costs. In particular, scalar Ordered insertion now
captures an additional 32-byte returned-entry snapshot before the zero-trip
guard, and its frame grows 1,136→1,168 bytes. Small positive spans still call
`memmove(32 * (len - index))`, while the baseline uses inline register copies.
Scalar removal retains a 176-byte frame versus 160 bytes in the baseline,
although its guarded leaf path removes the prior early stores/reload.
Vector insert/grow-insert/work frames are 272→304, 304→320 and 688→704 bytes.
These are native observations, not elapsed-cost allocations.

Construction and correctness are separate from timing. CLI construction takes
62.13 s and other native-screen construction 11.14 s, with 270 direct zero
statuses. The library test build takes 103.02 s; its three focused native
control tests pass. The later 52 native construction and 67 execution commands
match all expected statuses: 97 zero and 22 intentional failure exits.
Normal/retained/structural observations, both full ecosystem oracles, owning
and chronology observations, and byte-equal 294-row Vector / 210-row Ordered
accounting ledgers pass. Full source/fixture and image identities remain pinned.
Passing correctness does not remove the adverse timing result.

One continuous outer guard covers all eight fixed commands: Vector baseline
first→candidate first→candidate second→baseline second, then Ordered baseline
cohort 0→candidate 0→candidate 1→baseline 1. All direct exits are 0, with empty
measurement stderr and no per-command overrun. Native execution totals
637.406 s; the outer guard exits 0 in 638.49 s. All 21,504 rows have exact
schemas/keys and matched non-time work/checksums; all 656 input pins and 2,394
source pins are unchanged. Exact-key checks occur after the complete run.
No timing is repeated, pooled, renumbered, removed or normalized by peer drift.

Vector ranks every recorded sample 0–6, retaining six unranked suffix-zero
controls. The two invocation pairs remain separate: a useful gain must hold
in both and a qualified loss in either vetoes selection. Ordered retains
sample 0 as the registered warmup and ranks samples 1–5. Qualification still
requires every paired WF sample at least 1 ms, at most 10% cohort median-ratio
spread (`max/min - 1`), and disjoint ranges in the same direction in both
cohorts. Overlap remains inconclusive evidence of a difference.

| Useful matrix | Gain | Loss | Overlap | Duration/spread inconclusive |
|---|---:|---:|---:|---:|
| Vector first pair | 0 | 0 | 34 | 2 |
| Vector second pair | 0 | 0 | 33 | 3 |
| Vector both-pair disposition | 0 | 0 | 33 | 3 |
| Ordered | 5 | 3 | 21 | 1 |

Every Ordered cell follows. Entries are candidate/baseline cohort median
ratios, followed by G (qualified gain), L (qualified loss), O (overlap) or
I (duration/spread inconclusive). The archive retains exact ranges, medians,
minimum durations, qualification reasons and all Vector cells/replicates.

| Pair bytes | Path | Count 8 | Count 256 | Count 4096 |
|---:|---|---:|---:|---:|
| 16 | build-cleanup | 0.9623 / 1.0235 O | 1.1035 / 1.0615 L | 1.0303 / 1.0294 O |
| 16 | hit-miss | 0.9102 / 0.9328 G | 0.9280 / 0.9592 G | 0.9804 / 0.9987 O |
| 16 | range-16 | 1.0143 / 1.0112 O | 1.0268 / 0.9800 O | 0.9751 / 1.0000 O |
| 16 | replace-only | 0.9188 / 0.9941 O | 0.9784 / 0.9753 O | 0.9908 / 1.0119 O |
| 16 | replace-edit-remove-insert | 1.0721 / 1.0646 L | 1.0297 / 1.0766 L | 0.9986 / 1.0230 O |
| 264 | build-cleanup | 1.0016 / 1.0100 O | 0.9439 / 0.9442 G | 0.9245 / 0.9372 G |
| 264 | hit-miss | 0.9995 / 1.0305 O | 1.0662 / 1.3265 I | 0.9700 / 0.9391 G |
| 264 | range-16 | 1.0038 / 0.9956 O | 0.9869 / 0.9882 O | 1.0169 / 0.9906 O |
| 264 | replace-only | 1.0083 / 0.9991 O | 0.9903 / 0.9914 O | 1.0044 / 0.9587 O |
| 264 | replace-edit-remove-insert | 0.9911 / 0.9940 O | 1.0398 / 0.9845 O | 1.0048 / 0.9677 O |

The scalar build-cleanup/256 loss is +10.35%/+6.15%; scalar
replace-edit-remove-insert/8 is +7.21%/+6.46%; and the same path/256 is
+2.97%/+7.66%, from raw cohort medians. Their WF ranges are respectively
8.573–9.085→9.416–9.937 / 8.648–8.892→9.134–9.486 ms;
13.073–13.507→14.013–14.623 / 13.318–13.749→14.131–14.587 ms; and
43.360–44.043→44.808–45.104 / 43.455–43.727→46.058–47.996 ms.
All six native peers overlap at build-cleanup/256 and churn/8; at churn/256,
five overlap and C++ separates favorably while WF regresses. The earlier
unguarded build-cleanup/8 loss becomes overlap, while churn/8 remains a loss;
the guarded result adds build-cleanup/256 and churn/256 losses. Prior wide
build gains at 256/4096 remain gains; the prior wide range-16/8 gain overlaps.

Three of the five gains are hit-miss cells. Both lookup bodies and both
Ordered trace bodies compare unchanged under the recorded object-body
normalization. Setup, called insertions, code placement and whole-trace effects
remain possible contributors. The five gains do not measure isolated
`memmove` cost. Native drift is retained independently: useful Ordered cohort
median ratios span source-C 0.944–1.046, direct-C 0.941–1.035, AVL-C
0.954–1.039, Rust 0.945–1.042, C++ 0.859–1.175 and Abseil 0.924–1.080.
C++ scalar churn/256 separates favorably; direct-C wide build/8 and C++ wide
churn/4096 separate adversely. None is subtracted from WF results. The full
684-row native drift and 588-row same-image Vector drift tables remain in
the archive, including unfavorable or unstable controls.

Useful standard comparisons and slower-standard target counts are
pass/deficit/inconclusive; six additional Vector controls remain unranked.

| Invocation | Against Rust | Against C++ | Slower-standard target |
|---|---:|---:|---:|
| Vector baseline first | 0/17/19 | 16/2/18 | 16/2/18 |
| Vector candidate first | 0/17/19 | 18/4/14 | 18/4/14 |
| Vector baseline second | 0/17/19 | 18/5/13 | 18/5/13 |
| Vector candidate second | 0/14/22 | 19/2/15 | 19/2/15 |
| Ordered baseline | 6/21/3 | 5/22/3 | 7/19/4 |
| Ordered candidate | 7/19/4 | 4/21/5 | 8/17/5 |

The original analysis reader incorrectly required empty stderr from the
maintained `--targets` reducer, which emits two totals lines. Its preserved
replacement accepts only those exact lines, with counts derived independently
from all returned target rows, and rejects unexpected warnings, wrong counts
and wrong family names. Twelve sensitive checks retain the expected successes
and failures; raw files, statuses, matrix and selection criteria are unchanged.
The repaired reader hashes to
`6b9eb370c83269b6f24d76e26c1e6ad345552d6901ad65b52e206d065ac9c071`;
the unmodified maintained reducer hashes to
`f9669548af494ccd27285c78b04fcc0e0ddffaa88a574e5733e593dbd70dfd59`.

CSV replay follows the archive's `REPLAY.md`: set only the copied plan's
reducer path to its extracted frozen reducer and run `analyze-complete.py`.
That replay exits 0 and reproduces all five complete comparison CSVs byte for
byte. Both peers, all useful cells, unranked controls and adverse replicates
remain visible. The no-loss criterion refuses the guarded policy and the
slower-standard target remains incomplete; no threshold tuning or additional
timing follows this refusal.
