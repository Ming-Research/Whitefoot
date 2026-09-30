# Modular build cost and runtime quality

Measured 2026-09-24 on a 4-core Linux x86-64 container (kernel 6.18), Ubuntu
clang 18.1.3 and LLD 18.1.3, with the gate-profile `whitefootc` of the modular
compilation branch at `ed247115` and the [`run.sh`](run.sh) committed beside
this file. One run of the script; wall times in milliseconds from the
invocation's start to its exit, so each includes process start, input reading
and hashing the compiler's own executable (about 40 ms of every cached
invocation). The machine was shared: single runs vary by about 10%, and a
check without a cache by up to 20%, so treat smaller differences as noise.
Two further runs of the 32-module edit builds are reported beside the table.

The original measurement bundle answers slice 6 of the
[modular compilation design](../../investigations/modular-compilation/DESIGN.md#ordered-implementation-slices-and-completion-evidence)
for the measured implementation: module verdicts that record which
declarations of other interfaces their check reached, function analyses
reused through proof receipts, stable names for every link-visible entity, a
native split of the emitted module into ThinLTO fragments, cached runtime and
program objects, and LLD's ThinLTO object cache. A composition, and so every
build, first requires the verdict of every module in the entry's closure and
then checks the closure as a whole, taking the analyses whose inputs are
unchanged from their receipts; it still forms, resolves and type-checks
every body of the closure.

## Disposition of the module-product prototype

The owner selected the independent runtime SHA optimization and retired this
module-product import implementation from PR #160's mergeable code. The
[complete prototype](https://github.com/mbbill/Whitefoot/tree/e1db29f19addf1274d794560fd067c376b77558d) remains at `e1db29f19addf1274d794560fd067c376b77558d` on
`research/module-product-import-prototype`, including its typed codecs,
current-input and rollback controls, source-coordinate/instance rebinding,
lowering helpers, work counters and dedicated tests. These are historical
implementation evidence, not features of the reduced compiler.

The reduction retains `sha2`, its locked dependencies, offline CI provisioning,
published-vector/padding/specification digest checks, the independent constant
specification hash, and the corrected diagnostic design ground. Main's
existing module verdicts, proof receipts and native object cache remain.
No language or conformance rule changes. The import-specific tests retire
with the mechanism they observe; no test of the retained compiler is removed
or weakened to obtain a passing check.

The [matched hashing comparison](#final-current-main-qualification) measured
+4.6% GrowVector and +14.7% HashMap native entry-edit cost for its recorded
revisions, with higher memory/cache use. The subsequent local trials did not
establish both workloads' approximately 5% target. The latest
[ownership screen](#shared-callable-ownership-trial) also failed its criterion.
This supports declining the measured import representation, not abandoning
incremental compilation or claiming a current-head performance number.

The following product protocols and results describe their named historical
revisions. For reproduction, check out the named revision's compiler, harness
and fixtures together; `run.sh --units` retains that explicit research caller.
Instrumentation, omission and library-work assertions require a compatible
prototype. The reduced compiler deliberately has no module-product counters.
A new representation requires a newly requested experiment with complete
input validation, fresh-result controls and matched hashing costs; no further
trial is scheduled by this record.

## Workloads and modes

- `demo`: the five-module [specimen](../../investigations/modular-compilation/demo/README.md),
  entry `inspect`.
- `chain-8`, `chain-32`: generated chains of 8 and 32 modules, each publishing
  16 functions that call their predecessor's; entry `app`.
- `rng`: [`rng/`](rng/), a 400-million-step xorshift loop in the root module
  calling `pkg::rng::next` across a module boundary.
- `crossing`: a generated chain of 8 modules whose `step` mixes the result of
  the previous module's, called 100 million times from the root module's
  loop, so every step of the hot path lies in another fragment.

Build modes: `none` is a build without `--cache` (one clang invocation);
`image` caches one object for the entry's whole module; `module` and
`function` split that module into ThinLTO fragments per source module or per
function; `full` is `--full-lto`, which optimizes the program and every
runtime unit as one region. Edits: `body-edit` changes one private body (the
specimen's report helper, or the middle chain module's bodies);
`interface-edit` changes a `doc` entry of the first dependency's interface.

## Checking: `--check-modules` over every module and entry

| workload | no cache | cold cache | warm | after body edit | after interface edit |
|---|---:|---:|---:|---:|---:|
| demo (5 modules, 2 entries) | 152 | 236 | 55 | 153 (3 of 7 recomputed) | 112 (1 of 7) |
| chain-8 (9 modules, 1 entry) | 307 | 409 | 69 | 163 (2 of 10) | 106 (1 of 10) |
| chain-32 (33 modules, 1 entry) | 1928 | 2431 | 75 | 531 (2 of 34) | 184 (1 of 34) |

A warm check costs what reading inputs and hashing costs. A body edit
recomputes its module and the compositions that contain it; inside them only
the edited functions are analyzed again (on chain-32, 16 analyses recorded
and 827 taken from receipts). An interface edit that rewords documentation
recomputes only the edited module, where every downstream module was
recomputed before: a verdict's record names the declarations of other
interfaces its check reached, by a digest that leaves `doc` strings out.
Reordering graph rows or dependency lists, and registering a module outside
a check's closure, recompute nothing else. Every cached run printed the
verdicts a run without a cache printed.

## Building one entry

Total wall time; the compiler's report splits it into front end, fragment
split, object compilation and link.

| workload | mode | cold | warm | after body edit | objects recompiled after the edit |
|---|---|---:|---:|---:|---:|
| demo | none | 1375 | | | |
| demo | image | 1562 | 127 | 219 | 1 of 13 |
| demo | module | 1242 | 118 | 214 | 1 of 19 |
| demo | function | 1539 | 145 | 212 | 1 of 28 |
| chain-8 | none | 1485 | | | |
| chain-8 | image | 1734 | 122 | 241 | 1 of 13 |
| chain-8 | module | 1667 | 129 | 276 | 1 of 22 |
| chain-8 | function | 1502 | 156 | 306 | 1 of 24 |
| chain-32 | none | 3158 | | | |
| chain-32 | image | 3552 | 123 | 589 | 1 of 13 |
| chain-32 | module | 3996 | 136 | 609 | 1 of 46 |
| chain-32 | function | 4225 | 139 | 617 | 1 of 48 |

Two further runs of the chain-32 edit builds measured 550, 588, 595 and 650
(function) and 602, 611, 628 and 661 (image).

- The native split costs 0.1 to 0.5 ms in every fragment build; the
  `llvm-extract` split it replaced cost 445 to 460 ms per chain-32 edit.
- After a body edit exactly one object is recompiled in every mode, and the
  rest of the edit's cost is the front end: 416 to 432 ms of a chain-32 edit
  build. An instrumented run of the same step split it into about 67 ms for
  the edited module's check and about 350 ms for the composition, which
  forms, resolves and type-checks the whole closure (54, 99 and 198 ms);
  over the invocation 16 analyses were recorded and 827 taken from receipts.
  Lowering and emitting the entry's reachable code took about 3 ms of it.
- The twelve runtime units dominate a small cold build (0.9 to 1.7 s of
  object compilation); after the first build they are always reused.
- A cold chain-32 build spends about 2.3 s in the front end: 33 module checks
  against their dependencies' interfaces, then the composition, which reuses
  9406 analyses the module checks recorded.
- A warm build of an unchanged entry reuses its emitted module and every
  object, and costs the final link (40 to 60 ms) plus input validation.

## Runtime quality

Five runs of each benchmark per mode; each benchmark's result (its exit
status) is identical in every mode.

| workload | mode | median | min | max |
|---|---|---:|---:|---:|
| rng | none | 746 | 684 | 753 |
| rng | image | 740 | 676 | 767 |
| rng | module | 771 | 737 | 772 |
| rng | function | 755 | 667 | 773 |
| rng | full | 748 | 695 | 763 |
| crossing | none | 507 | 396 | 516 |
| crossing | image | 507 | 488 | 508 |
| crossing | module | 477 | 396 | 548 |
| crossing | function | 508 | 484 | 510 |
| crossing | full | 507 | 422 | 517 |

- `rng::next` is inlined into the loop in every mode: no executable contains
  a call to it (checked with `objdump`).
- The crossing loop of `wf_main` is inlined through all eight modules in every
  mode. In the `module` and `function` builds the copy of that loop the floor
  runtime's thread entry inlines keeps one call to the innermost
  `pkg::s0::step` per iteration, while `none`, `image` and `full` inline all
  eight steps there too: ThinLTO imports along the ten-call chain from the
  runtime's entry, and its import threshold decays with depth. Ten further
  runs of each build agree within noise (means 471 to 492), so the call costs
  nothing measurable here, hidden behind the loop's dependency chain; a
  workload whose inner step is not on such a chain may show it.
- No mode shows a repeatable loss at this noise level against `full`, the
  full link-time optimization of program and runtime together.

## Driver qualification before module build unit measurements

The module build unit follow-up uses the same workload home. Before measuring
its candidate, the driver now reads the compiler exit status directly and
admits no timing sample on failure. Runtime samples retain the program's exit
result and require agreement with the first ordinary build; a signal-style
exit is a failed run. The clock uses the host's monotonic clock, and record
edits use portable `sed` output followed by replacement of the scratch copy.

The actual `measure` and `recheck` helpers were exercised with a successful
compiler control printing two verdict records and a failing control exiting
23. The former produced one timing row, including the expected `recomputed
1 of 2` count; the latter preserved exit 23, printed its diagnostic and emitted
no timing row. The actual runtime loop admitted five rows for repeated exit 7
in the ordinary and fragment modes, and rejected exit 8 against expected 7
and signal-style exit 143 before emitting a row. A scratch-record edit
retained its unchanged line and made only the requested replacement. These
controls qualify the driver's observation, not compiler performance; new
module build unit cost results are still pending. They do not retroactively
supply successful-build evidence for a historical timing line.

## Structural import checkpoint

The initial module-product implementation at `9ad5fe4b02ed00381c32fe792d10e7c7b43b70a3`
was exercised on a scratch copy of the queue specimen, using one persistent
cache across three fresh CLI invocations: `kernel`, then `inspect`, then
`inspect` after renaming its local `stored` binding to `saved`. Every build
used `--cache`, `--report` and ordinary native emission. Each resulting
executable exited zero. The edited entry's emitted LLVM was byte-equal among
the retained build, an independent uncached candidate build and the retained
merged-baseline compiler. The candidate executable's SHA-256 was
`c52d8ca1571736f62ee39a6bf3cc38151103e14e38121a98346eeb2509d7a9e6`.

| Invocation | Body walks | Body imports | Body-less header checks | Executable exit |
|---|---:|---:|---:|---:|
| cold | 21 | 9 | 381 | 0 |
| second-entry | 13 | 7 | 139 | 0 |
| entry-edit | 11 | 9 | 139 | 0 |

These count actual structural checking and import calls; symbolic and ordinary
views count separately. Header checks do not contain implementation bodies.
The edited entry still walked three bodies in `pkg::runtime` and four in
`pkg::runtime::queue`, while `pkg::data` walked none and imported three. This
checkpoint therefore **does not meet the module build unit criterion**: the
first adapter falls back for discovery products and FN-4 queries it cannot yet
import. It has no lowered-fragment retention yet. This single diagnostic trial
selects no timing or memory conclusion; the paired real-consumer qualification
still follows completion of those paths.

## Module product qualification protocol

The implementation comparison uses `run.sh --units BASELINE CANDIDATE`.
Both arguments name already-built immutable compiler executables; building
those compilers is outside the samples. The default is seven rounds, reversing
compiler order on each round. Every compiler/round gets a new scratch source
tree and cache. Each sequence measures cold, unchanged warm, second-entry and
entry-local rename builds. The queue uses its existing two entries; the other
programs are unchanged implementation records behind a public `main` interface
and two small entry wrappers. Generated chains contain 8 or 32 modules with
16 functions each. SHA-256, GrowVector, wfgrep and HashMap come from maintained
programs; HashMap supplies the generic/behavior-heavy consumer. This is a
build experiment, not a new formal fixture or an alternate language parser.

The JSONL conditions identify compiler and maintained-source hashes. Each
sample records native construction/linking from the CLI report, wall time,
Darwin `wait4` peak RSS in bytes, cache bytes, work counters where supported,
LLVM hash and runtime output. The driver reads compiler and program exit
statuses directly. It compares baseline/candidate LLVM bytes and runtime
observations at each step before accepting their paired result. Native-build
RSS includes child resource accounting; `--compiler-only` repeats the sequence
using `--emit-llvm`, separating compiler memory from native tools. The ordinary
mode's additional emitted-module check is warm and is labelled accordingly;
it is not a cold-compiler memory estimate.

Use `--require-reuse` for native qualification of the candidate: the entry-edit
sample must report imported library bodies and lowerings with no unchanged
library walks. The queue's second entry also requests generic instances absent
from its first entry, so that transition can legitimately form new bodies;
its counters are reported separately. The assertion accepts all 49 candidate
entry-edit reports from the indexed-container qualification. Controls that
replace either a body or lowering import with a walk, or remove the corresponding
work observations, each fail the assertion.

For stage attribution, export each revision to a disposable scratch tree,
then run `python3 units.py --instrument TREE` and build that tree's compiler
under the shared verification guard, with a distinct Cargo target directory
for each exported tree. The same exact source boundaries report
source validation/resolution plus dependency-key assembly, formation/checking,
typed lowering and emission. These instrumented binaries are separate from
the primary timing pair. The instrumentation refuses the working repository
and nonunique insertion points; it lives in this experiment's driver and
retires with this retained-product comparison. No timers enter the compiler's
maintained acceptance path. Run the instrumented pair with `--compiler-only --stages`;
the raw stage observations are retained in `stages_ms`. Ordinary timing runs
reject executables containing the stage probe; only `--stages` admits them.
The probe also reports dependency discovery, source-identity setup, body
container setup, cache addressing, file reads and record validation, so a
source-input or lowering-stage difference alone cannot be mistaken for its
cause. Timers accumulate by label and flush when the emission timer ends, so
per-identity instrumentation does not write to stderr inside the measured loop.
The retained-body probe separates decoding, identity mapping, input validation,
staging clone/retirement and payload import; its callable/nominal input buckets
are subsets of validation, not additional work.

A first profiling setup shared one Cargo target directory between exported
trees. Cargo reused its preceding binary; the two executable hashes exposed
that mistake. Those stage samples are discarded. The caller now rejects
distinct input paths with identical executable bytes; a null comparison must
explicitly pass the same compiler path twice. Candidate-only import timings
are nested subsets of driver stages and are never summed into their totals.

Driver controls ran the actual caller against one-shot compiler stand-ins:
the success control completed; compiler exit 23, program exit 5, unequal
program output and unequal LLVM each caused a nonzero experiment exit before
the offending sample was admitted. The stand-ins were temporary and are not
an oracle for compiler behavior. A baseline-against-itself run supplies the
host's null comparison. Measurements and conclusions follow qualification;
the initial real-consumer probes already identified repeated interface digest
requests in dependency-key assembly. Parsing was already memoized; the local
fix removes repeated hashing and cloning, not repeated syntax judgments.

### First complete import cost probe

A single diagnostic trial of `8f9208176c216aec10b3ff6772f28f30b6d46056` (compiler SHA-256
`7c58986d560588e6684142c161ea66eb1dd55070b135f3a01b395a4ed2aa4623`) compared the merged baseline with the
complete body/discovery/lowering importer. Every admitted real-program and
chain sample compared baseline/candidate LLVM bytes and native results; all
executables exited zero. The generated-chain local rename initially also
renamed a named-argument label; the compiler rejected it and the driver stopped
without an edit sample. Correcting that fixture and rerunning both chains
supplied their complete observations below. These are one-run attribution
observations, not seven-pair timing claims or evidence that the cost target
has been met.

Entry-edit front-end milliseconds and actual body work:

| Workload | Baseline ms | Candidate ms | Candidate body walks/imports | Library bodies and lowered functions walked |
|---|---:|---:|---:|---:|
| queue | 49.1 | 62.0 | 1/19 | 0 |
| sha256 | 40.7 | 52.6 | 2/24 | 0 |
| grow-vector | 112.8 | 199.0 | 2/100 | 0 |
| wfgrep | 144.6 | 213.1 | 2/58 | 0 |
| hash-map | 251.2 | 571.2 | 2/301 | 0 |
| chain-8 | 83.8 | 136.6 | 2/262 | 0 |
| chain-32 | 294.7 | 580.7 | 2/1030 | 0 |

Thus avoiding body and lowering walks alone did not make this implementation
cheaper. The valid, separately built stage probe attributes the HashMap entry
edit to 432.0 ms formation/checking versus 166.6 ms in the baseline, with
106.8 ms inside body import and only 1.7 ms in missing-instance formation;
source/input assembly costs 64.3 versus 52.4 ms, and lowering 51.5 versus
3.0 ms. These nested observations are not additive. The 32-module chain's
source/input assembly costs 235.5 versus 121.9 ms. They select the recorded
work-reduction experiment: share module input validation, immutable identity
tables and byte framing, and stop constructing a complete syntax view merely
to enumerate the resolver's existing item keys. The following candidate must
retain the same equality and work-count observations and beat the prior
candidate on the same sources before attributing an improvement to those
changes. The null and seven-pair final comparison remain required.

### Seven-pair qualification and memory result

The final qualification used baseline executable
`a4d8c2cbad74b179485bf3915f71066ad1a0acf9d55401b75dd3c816f2510680` and
the current indexed-container candidate executable
`ca822b1c77380aadf29ceef1c44ca585493865b197b191227e516a40d4e77d7f`.
It ran seven alternating-order pairs for each workload, with a new source tree
and cache per compiler and round. All 392 native samples and their compiler-only
counterparts agreed in LLVM bytes and runtime results; every executable exited
zero. The table reports the median entry-edit wall time, compiler-only peak RSS,
and persistent cache size from the seven samples.

| Workload | Baseline ms / RSS MiB / cache MiB | Candidate ms / RSS MiB / cache MiB |
|---|---:|---:|
| queue | 165.7 / 21.5 / 0.63 | 173.1 / 23.4 / 0.94 |
| SHA-256 | 158.3 / 21.0 / 0.51 | 165.4 / 22.9 / 0.71 |
| GrowVector | 231.0 / 30.1 / 2.40 | 263.8 / 37.2 / 4.81 |
| wfgrep | 258.0 / 34.3 / 3.54 | 274.2 / 39.1 / 5.62 |
| HashMap | 364.0 / 48.2 / 6.20 | 463.6 / 64.0 / 15.99 |
| chain-8 | 195.5 / 25.0 / 0.90 | 207.9 / 28.0 / 1.60 |
| chain-32 | 406.3 / 39.9 / 3.22 | 436.4 / 47.9 / 6.03 |

The same-binary null run varied by at most 3.2% in wall time and 0.7% in
compiler-only RSS across these medians, while the indexed-container candidate
was 4.5–27.4% slower and used 8.8–32.8% more compiler-only RSS. Its entry-edit cache was smaller than the preceding
candidate on HashMap and chain-32, but remains larger than baseline on every
workload. The seven-pair run therefore qualifies the lazy payload indexing and
preserves all equality and work-count observations, but still rejects a claim
of build-cost or memory improvement for this representation. The pending
amendment remains an owner decision with this cost condition; further
optimization is a separate choice, not an implicit acceptance of the measured
regression.

## Current-main audit

The fresh audit compares main `c84c4dd7ab46848f6a5b816fcf57fce32b98158e`
with the merged module-product implementation
`b46f2d79e62d810154d5713506bf9d461b59e178`. The latter passed the complete
canonical `make check`. The queue specimen needed the current `queue^` and
`entry(queue)^` reference-access spelling; both compilers consumed the same
migrated specimen. No language rule changed in this work.

The uninstrumented baseline executable is
`85f00f45534f2b87a147e641b38acd68a40c2fe135170b06a8c1c2233aa5ae12`;
the uninstrumented candidate is
`1209668450623a9fbd20a42abea44ad740e95d9decb5c8a2be7ca5884793fc26`.
Three alternating pairs for queue, GrowVector and HashMap completed all
72 native samples and their LLVM/result comparisons. Entry-edit library body
and lowering work assertions passed. These are diagnostic observations before
optimization, not a replacement for the final seven-pair qualification.

| Workload | Baseline entry-edit ms | Candidate entry-edit ms | Difference |
|---|---:|---:|---:|
| queue | 165.1 | 172.9 | +4.7% |
| GrowVector | 241.3 | 275.6 | +14.2% |
| HashMap | 372.0 | 474.1 | +27.4% |

The current-main result reproduces the cost problem. Three separately
instrumented compiler-only pairs attribute the following entry-edit medians
(milliseconds; nested rows are not additive):

| Stage | GrowVector baseline / candidate | HashMap baseline / candidate | chain-32 baseline / candidate |
|---|---:|---:|---:|
| Source and input assembly | 33.51 / 46.02 | 51.98 / 95.47 | 121.67 / 146.24 |
| Formation and checking | 67.50 / 73.77 | 178.63 / 213.23 | 136.58 / 182.35 |
| Lowering | 1.07 / 17.41 | 3.10 / 48.82 | 0.68 / 9.45 |
| Cache record validation, across phases | 6.97 / 19.36 | 16.20 / 66.68 | 9.70 / 23.77 |
| Body-container loading | — / 8.96 | — / 39.19 | — / 16.53 |
| Lowered-record loading | — / 10.30 | — / 31.08 | — / 2.39 |

The candidate's source-identity setup is 1.12 ms for GrowVector and 2.01 ms
for HashMap; declaration-read discovery is 1.08 and 2.00 ms respectively.
These observations identify record validation/loading as material costs and
do not support attributing the loss primarily to identity-table construction
or declaration discovery. Instrumentation adds observations on every cache
load, so these times select an experiment rather than replacing the native
timing comparison. These observations selected a trial interning repeated
structural names within canonical records, retaining full-input equality and
unchanged reuse coverage.

Previous exploratory
observations from mixed instrumented and uninstrumented executables are not
qualification evidence; the runner now rejects that pairing by default.

### Rejected compact-name trial

Interning structural names within each canonical record passed the focused
identity and driver checks and all 48 native comparison samples, including
the output and library-work assertions. Three alternating pairs against the
pre-change candidate measured GrowVector at 259.0 versus 261.7 ms and HashMap
at 470.1 versus 471.6 ms. Cache sizes fell from 5,021,135 to 4,809,829 bytes and
16,740,022 to 15,948,076 bytes respectively. Fewer stored bytes did not yield a
build-time gain, so the encoding change and its dedicated test were removed.
The existing private encoding remains unchanged.

### Runtime SHA-256 trial

A separate native probe kept the SHA-256 algorithm and tested a fixed
eight-round unrolling, with and without forced inlining. Published vectors and
padding-boundary tests passed, but the scalar-loop gain was small: processing
32 one-million-byte messages took median 152,160 microseconds in the existing
implementation, 147,355 unrolled and 144,922 with inlining in five alternating
observations. This is a kernel screen, not a compiler timing claim, and does
not justify another hand-optimized implementation. This selected a comparison
with a maintained runtime implementation before introducing a dependency.

The same probe with RustCrypto `sha2` 0.11.0, default features disabled,
measures median 152,589 microseconds for the scalar implementation and 14,500
for the library in five alternating observations on this AArch64 host. Digest
equality holds at the tested padding boundaries and one-million-byte input.
The crate's [documented default dispatch](https://docs.rs/sha2/0.11.0/sha2/#backends)
uses available host instructions and otherwise the software implementation.
This screen justifies a whole-compiler trial, not a claim that all hosts gain
equally. The trial changes runtime cache hashing only, retaining the original
constant-evaluation implementation and exact SHA-256 bytes. Its main control
receives the same runtime hashing change.

Three alternating native pairs then compare the original candidate with the
runtime-hash candidate: GrowVector falls from 259.7 to 196.7 ms and HashMap
from 467.9 to 339.5 ms, reductions of 24.3% and 27.4%. Each comparison completes
48 samples, including output equality and entry-edit library-work assertions.
The same optimization applied to main gives a stricter control: GrowVector
measures 176.3 versus 192.7 ms (+9.3%), and HashMap 303.1 versus 343.4 ms
(+13.3%). The general hashing improvement is useful, but these three-pair
diagnostics do not establish the owner's approximately 5% module-product target.
The matched baseline binary is
`fe2ea831d68e6a8e3a0206d34d8884428b7553574e81f065d5edfa990687381a`;
the runtime-hash candidate is
`17602078de46432740ef70e2f58e4f3abf6e399bd99ccd32bbe8bd317240cea0`.

### Rejected invocation-local memo trial

The trial retained lowering semantic names and callable canonical inputs within
their current product adapters, comparing complete signature bytes before each
callable-input reuse. The 97 driver/cache tests passed. Three alternating
native pairs completed 48 samples with matching outputs and unchanged library
work; GrowVector measured 198.4 versus 202.0 ms, and HashMap 337.4 versus
339.3 ms. This did not support a build-time gain, so both memos were removed.

### Remaining import attribution

Six entry-edit observations per container using one instrumented runtime-hash
candidate give these medians in milliseconds. This version accumulates timers
before emitting observations; the earlier per-call logging probes have a
different observer cost and are not a timing comparison with this table.

| Stage | GrowVector | HashMap |
|---|---:|---:|
| Complete body import | 10.98 | 36.28 |
| Record decoding | 0.37 | 1.61 |
| Identity mapping | 2.07 | 8.36 |
| Current-input validation | 3.60 | 13.34 |
| Callable inputs, inside validation | 2.87 | 9.34 |
| Nominal inputs, inside validation | 0.27 | 1.62 |
| Staging clone | 1.25 | 3.08 |
| Retiring previous metadata | 0.73 | 2.07 |
| Typed payload import | 1.81 | 5.00 |

The observations do not justify replacing the staging ownership model for this
cost target. They select a smaller identity-lookup trial, preserving the
ordered discovery/serialization structures and complete input checks.

The runtime digest regression also passed a native test-control exercise using
its unchanged test body and digest wrapper. Returning an incorrect digest for
the short published vector, million-byte vector, a padding-boundary input or
the actual specification made each corresponding assertion fail (exit 101);
the original implementation passed (exit 0).

The identity-lookup trial passed 97 driver/cache tests and all 48 native
causal-comparison samples, including independent process seeds and unchanged
library-work assertions. GrowVector measured 195.9 versus 197.9 ms and
HashMap 345.3 versus 344.3 ms. That is not evidence of a useful gain; the
lookup-map changes were removed. The final candidate retains only the runtime
hashing optimization from these cost trials.

### Final current-main qualification

The final candidate is `52d584d37146ed47bd80011cbe7f10b01eb0a596`, built
with Cargo's `gate` profile. The matched baseline is main
`c84c4dd7ab46848f6a5b816fcf57fce32b98158e` plus only the candidate's
`Cargo.toml`/`Cargo.lock` dependency changes and the runtime `digest` wrapper
in `driver/cache.rs`. It has no module-product changes. This control separates
module-product overhead from a general hashing improvement that main can also
use. Its executable and the candidate match the hashes recorded in the runtime
SHA-256 trial above. A second control uses the unchanged main executable
identified at the start of this audit, measuring the complete PR's effect.

Conditions: macOS 26.6.2 AArch64, Rust 1.98.1 (LLVM 22.1.8), Apple clang
21.0.0 (`clang-2100.3.34.2`), Python 3.14.7. All binaries are uninstrumented.
Seven alternating pairs ran all seven workloads against matched main, then a
same-candidate-path null comparison; both ran in native and compiler-only
modes. The unchanged-main comparison ran both containers in both modes.
Together these completed 1,792 compiler samples (896 native builds and 896
compiler-only invocations). Every paired LLVM and native-result observation
matched; every candidate entry-edit library-work assertion passed. Builds of
compiler executables were outside invocation timing.

The following native medians are matched-main / candidate milliseconds, with
the candidate's relative difference. They include native construction and
linking; they do not measure generated-program execution time.

| Workload | Cold | Unchanged warm | Second entry | Entry edit |
|---|---:|---:|---:|---:|
| queue | 856.2 / 849.5 (-0.8%) | 90.7 / 89.4 (-1.4%) | 173.3 / 168.6 (-2.7%) | 129.8 / 134.4 (+3.5%) |
| sha256 | 838.1 / 846.7 (+1.0%) | 87.8 / 88.2 (+0.4%) | 141.5 / 146.5 (+3.5%) | 123.3 / 127.3 (+3.3%) |
| grow-vector | 1146.7 / 1169.2 (+2.0%) | 88.9 / 87.7 (-1.2%) | 294.2 / 308.2 (+4.8%) | 185.3 / 193.8 (+4.6%) |
| wfgrep | 2095.8 / 2065.8 (-1.4%) | 89.7 / 89.3 (-0.4%) | 397.1 / 397.8 (+0.2%) | 211.7 / 206.2 (-2.6%) |
| hash-map | 1595.0 / 1675.6 (+5.1%) | 86.6 / 87.5 (+1.1%) | 571.2 / 616.7 (+8.0%) | 299.1 / 343.2 (+14.7%) |
| chain-8 | 965.6 / 977.7 (+1.3%) | 85.9 / 86.7 (+0.9%) | 145.9 / 159.5 (+9.3%) | 157.7 / 162.7 (+3.1%) |
| chain-32 | 2322.6 / 2100.2 (-9.6%) | 81.7 / 82.7 (+1.2%) | 267.1 / 303.4 (+13.6%) | 339.3 / 348.0 (+2.6%) |

Entry-edit compiler-only time and peak RSS separate the compiler from native
child tools. Cache size comes from the native sequence after the edit; all
pairs below are matched-main / candidate. MiB means 1,048,576 bytes.

| Workload | Compiler-only ms | Compiler peak RSS MiB | Native cache MiB |
|---|---:|---:|---:|
| queue | 53.0 / 55.6 (+5.0%) | 21.41 / 23.50 | 0.62 / 0.93 |
| sha256 | 45.6 / 48.1 (+5.4%) | 20.83 / 22.84 | 0.51 / 0.71 |
| grow-vector | 103.6 / 116.3 (+12.2%) | 29.44 / 35.33 | 2.40 / 4.79 |
| wfgrep | 129.6 / 127.1 (-1.9%) | 34.33 / 38.25 | 3.54 / 5.61 |
| hash-map | 221.3 / 258.1 (+16.6%) | 48.33 / 62.48 | 6.20 / 15.96 |
| chain-8 | 78.4 / 83.6 (+6.6%) | 25.09 / 28.14 | 0.90 / 1.60 |
| chain-32 | 258.3 / 269.8 (+4.4%) | 39.81 / 48.27 | 3.22 / 6.03 |

The null comparison's entry-edit differences range from -3.8% to +0.6% for
native builds and -0.5% to +1.4% for compiler-only invocations. Across all
steps its largest absolute differences are 5.1% native and 3.6% compiler-only.
These observed differences describe this run's variability, not a statistical
confidence bound. GrowVector's native +4.6% is near the owner's approximate
5% target, but the compiler alone remains +12.2%; HashMap's +14.7% native
and +16.6% compiler-only loss remains material. Second-entry cost also grows
for both dependency chains. These losses must not be averaged away against
chain-32's faster cold construction.

The actual unchanged-main comparison answers the different, user-visible
question of whether that qualified revision slowed an edit build against its
recorded main:

| Workload | Native entry-edit ms | Compiler-only entry-edit ms |
|---|---:|---:|
| grow-vector | 227.5 / 193.8 (-14.8%) | 157.1 / 117.1 (-25.5%) |
| hash-map | 363.4 / 336.3 (-7.5%) | 291.3 / 260.2 (-10.7%) |

That qualified revision was faster than its recorded unchanged main on these
two workloads, but the matched control did not establish the import decision's
condition that importing costs less than the work saved. The owner approved
the independent runtime hashing choice and
diagnostic-ground correction while retaining the Draft and the unresolved
import-cost condition. That ruling changes design status, not these
measurements; it does not adopt the import proposal or waive its criterion.

Cold compiler-only wfgrep peak RSS is 364.61 / 377.62 MiB. Most of that
memory already exists in the baseline; this experiment does not attribute it
or establish a new memory defect. The maintained TODO records profiling it
when larger consumers or concurrent compilation make that footprint limiting.

Reproduce the matched and null sequences with the qualification command above,
`--rounds 7 --require-reuse`, and repeat with `--compiler-only` instead of
`--require-reuse`. For the unchanged-main pair add
`--workloads grow-vector hash-map`. Run them serially under the
shared verification guard. The raw local observations are named
`final-{matched,null,actual-main}-{native,compiler}.jsonl`; the protocol and
compiler/source hashes, rather than those temporary paths, identify the runs.

### Continued import investigation

The owner directed continued investigation under the same approximately 5%
entry-edit target against equally optimized main. A separate counting probe
ran one pair of the same instrumented candidate for each container (16
compiler-only samples). Both labels reported identical counts and LLVM.
Within one checking view, including its speculative import attempts, the
probe counted complete input encodings and exact repetitions:

| Workload | Input records | Repeated records | Encoded bytes | Repeated bytes | Records in staged imports |
|---|---:|---:|---:|---:|---:|
| grow-vector | 4,304 | 3,329 | 1,370,572 | 693,165 | 1,110 |
| hash-map | 20,989 | 18,326 | 6,397,616 | 4,087,029 | 2,503 |

These are operation/byte observations, not timing or proof that a repeated
encoding remains valid after mutable input changes. The probe tracks complete
encoded snapshots; it grants no cache authority. Parser controls distinguish
integer counts from nanosecond stage durations, accumulate repeated labels
and still reject a failed compiler process.

A follow-up memo trial guarded canonical inputs by derived equality of the
entire current typed function signature or nominal, including allocation
effects and fields. Each speculative import kept independent memos. This
avoided the earlier trial's serialization on every guard lookup. It passed
97 driver/cache tests and 48 native samples with equal outputs and unchanged
library-work coverage. In three alternating pairs GrowVector measured
196.5 / 192.7 ms (-1.9%) and HashMap 337.8 / 335.3 ms (-0.7%). That small
gain did not select the additional memo state; the trial was removed.

To avoid treating short-helper instrumentation as recoverable invocation cost,
`/usr/bin/sample PID 1 1 -mayDie -file TRACE` sampled eight entry edits per
compiler, alternating the saved uninstrumented hash-only candidate and the
equally optimized main. Each sample used a fresh HashMap fixture/cache, built
the two entries first and then renamed the second entry's local binding.
All compiler and sampler processes exited successfully. Sampling begins after
process launch, so it does not cover every startup phase, and inlining obscures
some helper boundaries. The traces still reach body-product decoding/identity
handling and lowered-product key construction, alongside proof-receipt work
already present on main. They select a cost hypothesis, not a before/after
compiler timing result. In particular, the earlier per-identity timer totals
include observation work and must not be read as a promised removable budget.

A second local trial deferred the serializer's ordered unique-reference set
until a caller requested it, deriving the same set from its already recorded
identity positions. It passed 97 driver/cache and three semantic-product tests
and 48 native samples with equal results and unchanged library-work coverage.
The three-pair native entry-edit medians were 197.0 / 201.6 ms (+2.3%) for
GrowVector and 344.2 / 351.6 ms (+2.1%) for HashMap. This failed its recorded
criterion (more than 2% HashMap improvement without a GrowVector regression),
so the trial was removed.

### Feature omission controls

Scratch builds from the unchanged candidate omit the body-product adapter,
the lowering adapter, or both. Body omission routes checking directly through
the same proof-receipt store, respecting its existing enable condition;
lowering omission supplies no retained-product adapter to ordinary lowering.
No language rule changes. These are attribution controls that repeat library
work, so they cannot qualify the module-product implementation. Each row below
has three alternating pairs on both containers, measured separately for pure
compilation and native construction: 288 samples in total, all paired LLVM
and native results equal. Compiler construction is excluded.

Entry-edit medians in milliseconds:

| Comparison | GrowVector compiler | HashMap compiler | GrowVector native | HashMap native |
|---|---:|---:|---:|---:|
| Full candidate / omit bodies | 117.5 / 112.1 | 260.9 / 235.9 | 200.7 / 199.9 | 355.5 / 326.6 |
| Full candidate / omit lowerings | 120.5 / 114.1 | 267.0 / 253.5 | 198.9 / 179.5 | 346.7 / 329.2 |
| Matched main / omit both | 105.3 / 102.5 | 221.9 / 219.7 | 182.9 / 188.1 | 305.3 / 306.3 |

Body products add about 25.0 ms and lowering products about 13.5 ms to
HashMap's compiler invocation in their respective pairs. Neither establishes
the recorded two-thirds dominance criterion, so both remain investigation
targets. Omitting both restores approximately the matched-main cost, supporting
the hypothesis that the product adapters explain the regression. The distinct
pair controls vary, and native child-process costs also vary; subtracting rows
does not yield an exact additive phase decomposition. In the earlier final
qualification's entry edits, full candidate and matched main reused the same
proof-analysis counts and all 13 native objects on every workload. That control
does not attribute the product loss to additional proof analysis or codegen.

The three scratch executable hashes (bodies, lowerings, both omitted) are
`80304db45e4061a50f7064e6690f80c6836d8d2969f0cdad0e84b23c9a3188f1`,
`27a35dbe4f5601f7c633298b3a9f6f68bdf5818a71624832eaa418e0f95312a9`, and
`663eba3dcb5b71b4140c671b814532e4ece56c751a4a8c6f2f95b1fd1078e211`.
Reproduce each source change on a separate export of the candidate with
`python3 -B research/experiments/modular-build-cost/units.py --ablate FEATURE TREE`,
where `FEATURE` is `bodies`, `lowerings` or `both`; build the scratch compiler
with its own target directory. Use the ordinary paired runner with
`--rounds 3 --workloads grow-vector hash-map`, once with `--compiler-only`
and once without. Omission controls intentionally cannot satisfy
`--require-reuse`. The tested full candidate and matched-main hashes are the
runtime-SHA controls recorded above.

### Relocated-input and lookup trials

A private input-encoding trial keeps raw typed bytes and the positions/kinds
of their identity references. After resolving the existing complete identity
table, import compares all literal segments and maps every retained reference
to the current ordinal. It still reconstructs and compares every current
input; neither a name alone nor a previously successful validation authorizes
reuse. Its independent wire fixture distinguishes renumbering from changes
to literal segments, reference kinds/positions/targets and missing mappings.
The driver/cache suite and product tests passed. Three alternating native
pairs preserved all outputs and unchanged-library reuse, measuring GrowVector
at 195.7 / 191.1 ms (-2.3%) and HashMap at 344.8 / 336.5 ms (-2.4%). Native
cache bytes fell from 5,021,135 to 4,708,037 and from 16,740,022 to 14,608,974.
This passed the local screen, selecting the larger paired/null comparison
reported below. The trial executable is
`610faff8bf361de5dbd33bf8289940ced6c99a79ee511e01fedfcda263267eed`.

Two subsequent candidates were tested against that saved executable, each
passing the same focused tests and 48 native plus 48 compiler-only samples.
Each preserved output equality and library work counts:

| Additional change | GrowVector native ms | HashMap native ms | GrowVector compiler ms | HashMap compiler ms |
|---|---:|---:|---:|---:|
| Dense ordinal maps | 187.7 / 196.7 (+4.8%) | 335.7 / 330.4 (-1.6%) | 116.9 / 115.7 (-1.1%) | 257.4 / 258.0 (+0.2%) |
| Source-name resolution memo | 195.0 / 200.2 (+2.7%) | 348.0 / 347.1 (-0.3%) | 116.4 / 116.7 (+0.2%) | 257.1 / 257.6 (+0.2%) |

Both failed the recorded native-gain screen and were removed. The dense-map
comparison's compiler RSS changed by -4.1% / -0.5% for the two containers;
the source-name memo changed it by -1.1% / -0.3%. Memory did not select either
trial. These results also caution against assigning the full instrumented
identity-mapping duration to the individual lookup operation.

The extended relocated-input comparison completed seven alternating pairs
against the unchanged candidate (causal), the same trial executable (null)
and equally optimized main (matched), in native and compiler-only modes.
All 672 samples passed their paired output controls and every native trial
entry edit preserved library reuse. The native entry-edit medians were:

| Control / relocated-input trial | GrowVector ms | HashMap ms |
|---|---:|---:|
| Unchanged candidate | 192.1 / 195.2 (+1.6%) | 361.4 / 341.6 (-5.5%) |
| Same trial executable | 194.6 / 196.3 (+0.9%) | 338.4 / 346.3 (+2.3%) |
| Equally optimized main | 188.3 / 195.1 (+3.6%) | 298.9 / 344.6 (+15.3%) |

The corresponding compiler-only medians were:

| Control / relocated-input trial | GrowVector ms | HashMap ms |
|---|---:|---:|
| Unchanged candidate | 120.6 / 119.1 (-1.3%) | 265.1 / 266.4 (+0.5%) |
| Same trial executable | 116.9 / 116.8 (-0.1%) | 254.5 / 257.9 (+1.3%) |
| Equally optimized main | 106.0 / 117.4 (+10.8%) | 219.8 / 256.9 (+16.9%) |

The causal native HashMap pair did improve, but the compiler-only pair did
not reproduce a compiler gain, and GrowVector failed the no-native-loss
criterion. The same-image observations are variability controls, not
confidence bounds. The trial also left HashMap well outside the owner's
approximately 5% matched-main target. Its smaller cache does not satisfy
those cost criteria; the encoding change and its trial-only test were removed.
No compiler optimization from this continuation remains. At that continuation's
end, production source and pending design amendments were unchanged from the
earlier final qualification; its seven-workload results still describe the
compiler implementation. Reproduce the extended trial comparisons with the paired
runner's `--rounds 7 --workloads grow-vector hash-map`, native
`--require-reuse`, and a separate `--compiler-only` sequence. The trial hash
above and the recorded runtime-SHA control hashes identify these observations.

### Grouped lowering-read trial

The storage-only trial keeps the complete lowering key and typed fragment
reader, grouping records into one checksummed module container. Each emitted
function/overlap slot keeps its latest complete key and payload, replacing
that slot's prior version instead of reading its entire edit history. The
driver's indexed body container supplies the common storage operation; source
inputs and lowering authority remain distinct. The
[screen](../../investigations/modular-compilation/DESIGN.md#grouped-lowering-read-screen)
was recorded before implementation and measurement. This tests storage
grouping, not shared semantic input validation.

The candidate executable SHA-256 is
`ed924b8ff103286a99dbb1fb0ee2082b1775648021a7daafe8d3644b432e6e89`;
the control is the qualified runtime-SHA candidate
`17602078de46432740ef70e2f58e4f3abf6e399bd99ccd32bbe8bd317240cea0`.
The native driver/cache suite passed 98 tests, including a new complete-key,
slot replacement, module/compiler separation and damaged-container control.
The existing damaged-product test targeted the new storage family with its
original recomputation assertions preserved. Test execution took 4.55 s;
compiler executable construction took 50.94 s and is excluded below.

Three alternating pairs in each mode completed 96 samples with identical
LLVM/runtime observations. Every candidate native entry edit imported the
unchanged library bodies and lowerings without walking them. Median elapsed
milliseconds, control / grouped trial:

| Step | GrowVector native | HashMap native | GrowVector compiler-only | HashMap compiler-only |
|---|---:|---:|---:|---:|
| Cold | 1288.5 / 1245.2 | 1691.7 / 1728.9 | 350.4 / 343.8 | 696.7 / 676.9 |
| Warm | 96.6 / 92.0 | 93.7 / 97.1 | 11.9 / 12.3 | 13.3 / 12.8 |
| Second entry | 323.4 / 328.9 | 622.7 / 611.6 | 102.1 / 101.7 | 252.5 / 245.7 |
| Entry edit | 212.0 / 196.5 (-7.3%) | 343.2 / 348.8 (+1.6%) | 121.8 / 121.0 (-0.7%) | 267.9 / 269.5 (+0.6%) |

HashMap compiler-only paired changes were +0.6%, +1.2% and -5.8%; only one
round improved, failing the required median gain and paired-direction screen.
Entry-edit median compiler peak RSS changed by -0.2% for GrowVector and +1.1%
for HashMap. Median compiler RSS growth stayed below 5% at each measured step,
but the third cold pair rose 12.1% for GrowVector and 7.0% for HashMap.
Native cold GrowVector median peak RSS rose 9.8% (53.05 / 58.27 MiB), so the
compiler-only observation is not a claim that every process peak improved.
Native entry-edit cache sizes changed from 5,021,135 to 5,020,899 bytes and
16,740,022 to 16,739,273 bytes. Grouping removes record framing but retains
nearly all key/payload bytes.

The screen did not select this candidate. All six compiler/test file changes
were removed, including the trial-only test and storage-family adjustment.
There was no seven-pair qualification or repeated-edit history extension
because the initial consumer criterion failed. This rejects the measured
storage-only implementation, not every possible batching design. The shared
identity/input catalogue was left to the following trials. At the end of
that trial, production source, specification and pending amendments were unchanged.

The raw `grouped-lowering-{native,compiler}.jsonl`, build/test log, saved
executable and `grouped-lowering.patch` are in the same local audit directory
as the other diagnostic trials. The patch SHA-256 is
`6cc458072269560a80158620746fbb304795dd5ae2f3bf9aa43a01d5f2255511`;
it applies to investigation revision
`cb9442d5a6eec464bff2a43f3d7733248726b84b`. Repeat the screen with the paired
runner's `--rounds 3 --workloads grow-vector hash-map`, native
`--require-reuse`, and a separate `--compiler-only` invocation under the
verification guard. Do not treat the omitted history qualification as passed.

### Versioned current-input trial

The [screen](../../investigations/modular-compilation/DESIGN.md#versioned-current-input-catalogue-screen)
shares complete callable/nominal input encodings while private per-entry
revision tokens remain equal. Mutable access replaces the token before
returning a borrow; clones diverge on mutation, and speculative imports use
fresh memos. Every consumer still compares its complete expected bytes.
The candidate SHA-256 is
`483cb02f4661de26ec2be50de5f99702c195a67e8bb348cea67d39a66f473c07`;
the control is the qualified runtime-SHA candidate
`17602078de46432740ef70e2f58e4f3abf6e399bd99ccd32bbe8bd317240cea0`.

Three revision/current-input controls and all 97 driver/cache tests passed;
the latter ran in 4.47 s. Compiler construction took 56.68 s, excluded from
the measurements. Five alternating pairs per mode completed 160 samples
with identical paired LLVM/runtime observations and unchanged-library reuse
on every native entry edit. Median elapsed milliseconds, control / trial:

| Step | GrowVector native | HashMap native | GrowVector compiler-only | HashMap compiler-only |
|---|---:|---:|---:|---:|
| Cold | 1217.0 / 1252.0 | 1767.2 / 1735.5 | 357.6 / 357.9 | 718.7 / 705.4 |
| Warm | 81.0 / 89.4 | 88.9 / 88.3 | 11.7 / 12.3 | 13.0 / 13.2 |
| Second entry | 310.2 / 317.4 | 669.7 / 646.4 | 100.4 / 101.0 | 252.3 / 248.2 |
| Entry edit | 199.8 / 198.0 (-0.9%) | 349.7 / 347.0 (-0.8%) | 122.1 / 121.0 (-0.9%) | 280.2 / 270.0 (-3.6%) |

HashMap native paired changes were -1.6%, +0.1%, +6.6%, -2.8% and -0.4%:
only three improved, failing both the 3% native median gain and four-pair
direction criteria. Its compiler-only pairs were +1.4%, -3.8%, -2.2%, -2.2%
and -2.0%. The first native GrowVector entry pair was a +46.5% outlier;
the full five-pair medians above retain it. These observations do not justify
a claim of stable native gain.

Compiler-only median peak-RSS changes for cold, warm, second entry and entry
edit were -0.4%, +0.8%, +3.1%, -0.2% for GrowVector and +1.3%, +0.3%, +2.0%,
+4.8% for HashMap. Individual pairs exceeded 5%: GrowVector cold +6.6% and
second entry +8.0%; HashMap second entry +5.2% and entry edits +8.3%, +7.6%,
+5.1%. Native HashMap entry RSS rose 2.6% by medians, with one +6.8% pair.
Cache sizes were unchanged at every step. Neither the RSS medians nor the
compiler-only gain rescues the failed native screen.

All four compiler/test file changes were removed. There was no broader
qualification or mutation campaign because the screen failed. Raw
`versioned-input-{native,compiler}.jsonl`, the screen log, saved executable
and `versioned-input.patch` remain in the local audit directory. The patch
SHA-256 is `dd66254018d15fe7a39266548410cf1c0031aace05e32c3dd9bad833089f3332`
and applies to `28fc400b436ac2f42e5ca06f1feba8e5061d7607`.
Reproduce with the paired runner's `--rounds 5 --workloads grow-vector hash-map`,
native `--require-reuse`, and a separate `--compiler-only` invocation under
the verification guard. The shared stored-identity catalogue is a separate
representation experiment; this rejected memo is not part of that trial.

### Shared retained-identity trial

The [screen](../../investigations/modular-compilation/DESIGN.md#shared-retained-identity-catalogue-screen)
interns complete stored identity records in their module source-input
container. Bodies reference slots; one checking view shares typed record
decoding and successful name resolution, while speculative formation keeps
its own resolution map. Old ordinals alone never identify a shared record,
and every consumer still reconstructs and compares all current inputs. The
rejected revision memo and lowering storage changes are absent.

The candidate executable SHA-256 is
`199d0523200d58f0a382d08111ef47df7b258ea3237853c23bd00bd29b07bc97`;
the control is the same qualified runtime-SHA candidate as above. All 99
driver/cache tests passed in 4.38 s, including exact-record deduplication,
equal old-ordinal separation, module isolation, reload, missing-slot and
malformed-container controls. Compiler construction took 55.97 s and is
excluded below. Five alternating pairs per mode completed 160 samples with
identical paired LLVM/runtime observations and unchanged-library body and
lowering reuse on every native entry edit. Median elapsed milliseconds,
control / trial:

| Step | GrowVector native | HashMap native | GrowVector compiler-only | HashMap compiler-only |
|---|---:|---:|---:|---:|
| Cold | 1210.8 / 1231.2 | 1736.8 / 1752.3 | 351.2 / 358.9 | 711.4 / 696.6 |
| Warm | 87.1 / 88.7 | 81.0 / 84.6 | 11.8 / 11.9 | 12.6 / 12.6 |
| Second entry | 309.3 / 317.9 | 634.5 / 633.9 | 99.4 / 101.0 | 248.4 / 244.1 |
| Entry edit | 197.5 / 200.5 (+1.5%) | 351.4 / 346.7 (-1.3%) | 120.5 / 122.1 (+1.3%) | 269.3 / 266.1 (-1.2%) |

HashMap native paired changes were -1.2%, +2.2%, -0.8%, -1.3% and -0.6%;
compiler-only changes were -1.5%, -1.5%, +0.4%, -1.4% and +2.8%. Neither
mode met the 3% median-gain criterion, and only three compiler-only pairs
improved. Native GrowVector cold pairs included +17.5% and +8.3%; its
compiler cold pairs included +5.9%. The medians retain those samples.

Compiler-only median peak-RSS changes for cold, warm, second entry and entry
edit were +0.7%, +0.2%, +0.3%, -0.4% for GrowVector and -5.3%, +0.3%, +7.5%,
+4.4% for HashMap. HashMap second-entry memory therefore also failed the 5%
median ceiling. Individual HashMap second-entry pairs rose 9.3%, 8.3%, 10.0%
and 7.5%; entry pairs rose 11.1%, 5.1% and 5.9%. Native HashMap second-entry
and entry-edit RSS medians rose 5.5% and 5.9%, with pair maxima of 9.6% and
11.3%. A shared decoded catalogue can retain more live memory while storing
fewer bytes on disk; the smaller cache alone is not a latency or memory gain.

Native entry-edit cache sizes fell from 5,021,135 to 4,648,087 bytes (-7.4%)
for GrowVector and from 16,740,022 to 13,670,330 bytes (-18.3%) for HashMap.
Both compiler file changes and the two dedicated tests were removed after
the screen failed. There was no seven-pair qualification, mutation campaign
or repeated-edit history extension. The ordinary current-input checks were
unchanged; passing these controls is not proof of every possible import.

Raw `shared-catalogue-{native,compiler}.jsonl`, the controls/screen logs,
saved executable and `shared-catalogue.patch` remain in the local audit
directory. The patch SHA-256 is
`e8aaabc39971fb4ded980c548864ec2e69be33e9b2c79414f6e4d1d730e5648a`
and applies to `28fc400b436ac2f42e5ca06f1feba8e5061d7607`.
Repeat with `--rounds 5 --workloads grow-vector hash-map`, native
`--require-reuse`, and separately `--compiler-only`, under the verification
guard. No production optimization or tree/specification change survives
these two catalogue screens.

The source review also revisited the separate lowering-key projection
opportunity. `body-import-aggregate.jsonl` already records a three-pair
same-image instrumented observation with executable
`df088685cfdcae8a3d4c31af453092f1cfe4ca5350986e034ab835c63e55f525`.
Its candidate HashMap entry-edit medians were 6.30 ms for complete lowering
key construction, 6.23 ms for lowering cache loads and 19.60 ms for the whole
lowering stage. These are historical nested observations with observer cost,
not a new causal run or additive savings. Inspection confirms erased Proof
payloads and loop invariants in the key, but does not measure their share.
No runtime-input projection is selected; its required guards and reopening
condition remain in the investigation and TODO.

### Integrated-main compiler cost

The qualification protocol was recorded in the integration revision
`7ec0a8b81c01fdac1c510d1b0e95486a6850d7ea` before these observations. Its
source includes main `f5024250fd596b9b41e8e1b69a8366582587d756`, specification
v0.82 and the typed-record compatibility changes. These observations apply
only to that revision pair. The later integration of main
`4459df880b04a7e87f46398969e1382b52637af0` adds parser lookup improvements
and diagnostic repairs and has not received this cost qualification. The baseline exports
that main and receives only the same safe runtime SHA implementation and its
locked dependencies; no product adapter is added to it. The three-file baseline
patch has SHA-256 digest
`743f4ddd25d6199b42879ce1a112c0aff49d1fb2f21fdd38c4fb6dc0b43cffa6`.
The saved executables
have SHA-256 digests:

- SHA-matched main: `83c4f3b6cb68fd90d80438dac528c8a1fb0dd5e461b57de4042fc8ce42d5909f`.
- Integrated candidate: `cb7f60e8f31d012045e0a253ca1f86933dd15bfa8b4aaa655cf8cb7ee246f2f9`.

The host was macOS 26.6.2 arm64. Five alternating pairs per container and
mode ran serially under the shared check guard. Compiler construction and
program execution are excluded from build latency. Entry-edit same-image
controls must have a paired median difference within 3% and fewer than four
of five differences in one direction. The native control failed, including
one repeat after host inspection:

| Control | GrowVector paired median | Positive pairs | HashMap paired median | Positive pairs | Verdict |
|---|---:|---:|---:|---:|---|
| Native same image | +2.98% | 3/5 | -1.89% | 0/5 | Fail: HashMap direction |
| Native repeat | -0.53% | 1/5 | +1.41% | 4/5 | Fail: both directions |
| Compiler-only same image | -0.70% | 2/5 | +0.11% | 3/5 | Pass |

The first native entry-edit pairs ranged from -28.68% to +17.06% for
GrowVector and -17.23% to -0.54% for HashMap. Native medians within 3% alone
do not pass the recorded control. A host snapshot between runs showed active
system and GUI background work; it does not prove the cause of earlier timing
variation. The native comparison with matched main was not run, and there is
no current native cost qualification or approximately 5% target claim. The
repeat's reversed directions do not justify discarding either result or
repeating until a passing window appears.

Compiler-only measurements isolate `--emit-llvm` invocations. Baseline /
candidate medians follow; percentages in this table compare the two medians,
while the entry-edit interpretation below uses the median of paired ratios.
RSS is the measured compiler process's peak, not an untimed warm lookup.

| Workload | Step | Wall ms | Change | Peak RSS MiB | Cache bytes |
|---|---|---:|---:|---:|---:|
| grow-vector | cold | 365.385 / 383.267 | +4.89% | 51.98 / 59.00 | 1,840,980 / 4,343,650 |
| grow-vector | warm | 12.115 / 12.878 | +6.30% | 14.06 / 15.12 | 1,840,980 / 4,343,650 |
| grow-vector | second-entry | 92.023 / 106.022 | +15.21% | 31.53 / 36.33 | 2,132,040 / 4,636,675 |
| grow-vector | entry-edit | 115.742 / 128.859 | +11.33% | 31.66 / 37.56 | 2,442,674 / 4,958,584 |
| hash-map | cold | 677.320 / 766.089 | +13.11% | 74.83 / 95.97 | 4,541,138 / 14,801,390 |
| hash-map | warm | 13.696 / 14.192 | +3.62% | 15.97 / 17.02 | 4,541,138 / 14,801,390 |
| hash-map | second-entry | 216.542 / 256.105 | +18.27% | 50.44 / 64.48 | 5,456,881 / 15,719,098 |
| hash-map | entry-edit | 241.226 / 287.705 | +19.27% | 50.47 / 64.27 | 6,392,272 / 16,665,768 |

Every matched entry-edit pair is slower: GrowVector's paired median is
+11.33%, with a +9.50% to +13.15% range; HashMap's is +19.97%, with a +12.09%
to +27.00% range. The passing compiler-only entry-edit null control supports
investigating this gap. It does not make cold or warm timings precise: one
matched GrowVector cold pair was -46.98%, and one HashMap warm pair was
+185.91%. No cold/warm performance selection follows these observations.
Comparing percentages with the earlier compiler does not attribute a change
to the new language features, because the measurement windows differ.

The four sequences contain 320 measured samples and 160 exact paired LLVM
comparisons. The 80 native pairs also have equal executable outputs, every
execution returning zero; every one of their 40 entry-edit samples has actual
unchanged-library body and lowering reuse with zero such walks. Those work
observations do not depend on a passing timing control. Compiler-only samples
have no native build report and do not independently establish work counts.

Reproduce with the existing paired runner, `--rounds 5 --workloads grow-vector
hash-map`, using the candidate's same path on both sides for null controls.
Add `--compiler-only` for both pure sequences. Keep executables independent
from subsequent builds. Raw local records are `f502-null-native.jsonl`,
`f502-null-native-repeat.jsonl`, `f502-null-pure.jsonl` and
`f502-matched-pure.jsonl`; the source revisions, executable digests and
protocol identify the experiment. The wider module-owned sharing boundary
remains unimplemented and unadopted; current-model adapter omissions are the
next attribution control, not another selected local cache.

### Current-model omission preconditions

The next attribution attempt used the preceding integrated source and the
existing scratch-only `--ablate` transformations. A setup check found that
three scratch exports sharing one Cargo output directory had received the
same executable bytes. No measurement used those mislabeled copies. Rebuilding
the lowering and joint omissions in independent output directories produced
three distinct executables. One native pair per container and
variant then checked exact LLVM and executable results at all four steps.
The enabled adapters still reported actual library reuse and zero library
walks on entry edits; omitted adapters had no report rows. An empty adapter
report does not measure how much ordinary checking/lowering work ran.

| Omitted adapter | Executable SHA-256 | Source patch SHA-256 |
|---|---|---|
| Bodies | `5d3715d75f2eadf16a1a35185d2156ad74b237043bee38cdf5adae8ddef5a3cc` | `daffdad3debe13c9b845138c2f1698a91a4f485a6f85394e41e646babbb3eff2` |
| Lowerings | `ae3fc2435ce62a08bd5d0f527b9c17d2ae5ca950e05bb0ff97d6c447d110f69c` | `91b320b8d0b069858ab57a225cf03f643836037fd261aac64846d9667e8cafcc` |
| Both | `22a5dd10ead0af93e694804a1a6112ad83e2f3605119cbeb3627bb9ca216af70` | `749764bab65b4ab5816c9d94ce21dd49b178b1300f06b128c949c2478ff25cc5` |

The five-pair compiler-only same-image control preceding the timing
comparisons failed the recorded direction criterion for HashMap. GrowVector's
entry-edit paired median was +0.17%, with three positive pairs; HashMap's was
-1.28%, with one positive pair. Their five paired differences were:

| Workload | Round 1 | Round 2 | Round 3 | Round 4 | Round 5 |
|---|---:|---:|---:|---:|---:|
| grow-vector | -0.23% | +1.42% | +0.17% | +3.57% | -2.47% |
| hash-map | +0.26% | -0.91% | -1.28% | -3.62% | -3.39% |

No omission timing comparisons were run,
and no current-model adapter cost or dominance conclusion follows. This later
control does not supply a new null result for the earlier matched-main window,
whose entry-edit control passed; it limits this subsequent attribution window.
There was no retry or new production optimization.

The three output-control sequences contain 48 samples and 24 exact paired
LLVM/native comparisons. The null sequence contains 80 samples and 40 exact
paired LLVM comparisons. The guarded orchestration exited 2 on the null
precondition; all individual compiler/runner commands exited zero. This is
an experiment refusing to admit a timing comparison, not a repository test
failure. The one-shot orchestration was removed after the attempt.

Reproduce the output controls with `--rounds 1 --workloads grow-vector
hash-map` against the full candidate; then run the same candidate path on
both sides with `--rounds 5 --compiler-only` and the same workloads. Only a
passing control admits the three-pair omission comparisons in the recorded
protocol. Raw local records are `f502-omit-{bodies,lowerings,both}-outputs.jsonl`
and `f502-omission-null-pure.jsonl`. Keep each exported scratch compiler's
output directory independent and verify artifact identity before observation.

### Shared callable ownership trial

The [prospective screen](../../investigations/modular-compilation/DESIGN.md#shared-callable-ownership-screen)
tested shared immutable callable entries and their complete input encoding
across speculative forks, with copy-on-write invalidation. It did not replace
the composition's dense handles with module-owned checked bodies. Unlike the
previous version-token memo, it retained both values and encodings across
forks and avoided invalidation on unchanged allocation-bit assignments.
Every body still compared every complete consumed input; nominal inputs,
including formed type invariants, remained freshly constructed.

The unmodified control is integration `b02dff8329d055d72689a3efcde882008a5e7df4`,
which merges main `db3ba937f346e668f870cb634b99cdfe4effc687`. The trial adds
only its five-file compiler patch. Both use the same runtime SHA optimization
and Cargo gate profile. Compiler construction is excluded: the control build
took 45.64 s; the trial test build took 46.75 s and its executable build
33.31 s. One new ownership/isolation test and 74 driver tests passed; the
latter executed in 1.39 s. An extracted copy of the actual ownership type and
test also passed under `rustc --test`; deliberately retaining a stale encoding
or deep-copying a fork's entries each made it fail with exit 101. The source
extraction changed visibility only, not ownership or test logic.

Seven alternating pairs per mode and workload completed 448 samples across
two null and two causal comparisons. All 224 paired LLVM comparisons and
112 paired native runtime comparisons agreed; all 28 candidate-side native
entry edits showed positive library reuse and zero unchanged-library body
and lowering walks. The new effect-size null control passed in both modes:

| Mode | GrowVector paired median | Pairs within 5% | HashMap paired median | Pairs within 5% |
|---|---:|---:|---:|---:|
| Compiler-only | -1.01% | 7/7 | +0.39% | 7/7 |
| Native | -0.86% | 6/7 | +2.45% | 6/7 |

Elapsed medians in milliseconds, control / trial. Entry-change percentages
below are medians of paired ratios, not ratios of these wall-time medians.

| Step | GrowVector native | HashMap native | GrowVector compiler-only | HashMap compiler-only |
|---|---:|---:|---:|---:|
| cold | 1437.12 / 1441.82 | 1985.72 / 1991.92 | 350.17 / 355.44 | 733.48 / 728.87 |
| warm | 95.72 / 94.15 | 94.51 / 95.05 | 12.94 / 12.50 | 13.40 / 13.41 |
| second-entry | 333.19 / 325.05 | 662.07 / 657.60 | 98.04 / 96.11 | 255.68 / 244.28 |
| entry-edit | 198.34 / 203.43 | 354.43 / 341.05 | 114.66 / 115.91 | 265.27 / 262.62 |

All seven causal entry-edit changes, percent:

- grow-vector, pure: +61.89, -2.86, +6.11, -0.58, -1.88, +1.71, -0.62; median -0.58%.
- hash-map, pure: -4.10, +3.05, -1.15, -4.23, -0.13, +0.11, -30.39; median -1.15%.
- grow-vector, native: +4.96, +2.33, +2.57, -2.74, +37.79, +2.66, -5.53; median +2.57%.
- hash-map, native: -4.53, -0.67, +0.29, -6.32, -6.55, -5.58, -8.51; median -5.58%.

HashMap's native gain of 5.58% did not reproduce as the required compiler-only
gain (1.15%, below 3%). GrowVector's native paired median regressed 2.57%,
above the 2% limit. Both independently fail the screen. Retain the large
outliers rather than removing them; a host-process snapshot after the pure
comparison did not establish their cause. The passing preceding null does
not make subsequent noise disappear or establish a population confidence bound.

Compiler-only peak RSS medians in MiB, control / trial:

| Step | GrowVector | HashMap |
|---|---:|---:|
| cold | 57.78 / 59.55 | 95.95 / 97.25 |
| warm | 15.31 / 15.30 | 17.19 / 17.20 |
| second-entry | 37.53 / 37.81 | 65.77 / 67.20 |
| entry-edit | 37.83 / 37.84 | 65.22 / 66.41 |

All per-step compiler RSS medians and cold compiler-time changes stayed within
the screen's 5% limit; those do not rescue the failed latency criteria.
Cache bytes were identical in every causal pair: entry edits ended at
4,958,584 / 16,665,768 bytes in compiler-only mode and 5,120,531 / 16,878,179
bytes in native mode for GrowVector / HashMap. These are cache sizes, not
compiler RSS. Native warm-emit RSS is not used as compiler peak evidence.

Remove all five trial compiler edits, including the trial-only ownership
test, because the unselected representation no longer exists. The integrated
compiler and its existing correctness controls remain unchanged. No new
representation or performance optimization is selected. At the owner's
request, close this investigation round instead of starting another trial.
The additional current-main baseline build was stopped; no fresh matched-main
comparison, history qualification or full module-owned body implementation
was completed. The roughly 5% matched-main native target remains unmet by the
recorded evidence; these causal results must not be relabeled as main overhead.

Reproduce with `units.py --rounds 7 --workloads grow-vector hash-map`, using
`--require-reuse` for native and a separate `--compiler-only` invocation,
under the host-wide guard. Raw `shared-callable-{null,causal}-{pure,native}.jsonl`,
the compiler patch and control logs remain in the local audit directory.
The one-shot orchestration and extracted mutation-test files were removed.
Artifact SHA-256 digests:

- Control executable: `7b425bbb1e1bdf80be16953ef4ffa53d48bdde598581b94d0b6bb7beff65325b`.
- Trial executable: `3eae48dc55568341195f3efcc4cce67a643c0d3bb096e7bf542613f77fc63419`.
- Trial patch: `09b2047ab44976fe7843393c98ef68cb0f332727485dee7a7f3224debe4bbe74`.

## Limits

- The original backend comparison above used two small runtime loops on one
  host. Its link times and runtime conclusions apply to that compiler pair.
- Module-product qualification uses seven workloads on one host. The candidate
  still parses and resolves the selected closure and runs current composition
  judgments. It imports unchanged library structural bodies and lowered
  functions; proof analyses retain their separate input keys. Editing a
  module's own source invalidates its grouped structural-body container, so
  entry-edit reuse does not establish per-body invalidation within that module.
- ThinLTO planning still runs at each link. Module-product timings measure
  compilation and construction, with native output equality checked separately;
  they do not measure the generated programs' runtime performance.

## Parser row lookup cleanup

Measured on 2026-09-30 UTC on macOS 26.6.2, arm64 MacBookPro18,3 (eight
physical and logical CPUs), Rust 1.98.1, with optimized `gate` binaries.
The baseline is `7edc86591e12b810665d8e60f71da013c1a7025f`; the candidate is
`d003cdeee5f818a9ed56304fc39e6f1be0bfb51d`. The candidate adds a generated
first-predicate index for SELECT2 rows and iterates terminal sets through
their set bits. This comparison measures their combined effect, not each
change separately. The
[prospective criterion](../../investigations/library-modules/DESIGN.md#small-parser-cleanup-prospective-criterion)
was committed before this run.

| Check | Baseline median ms | Candidate median ms | Median paired candidate / baseline | Same-image median ratio |
|---|---:|---:|---:|---:|
| One-function module | 10.591 | 9.410 | 0.8762 | 0.9939 |
| Two-module composition | 22.366 | 18.591 | 0.8312 | 0.9977 |
| `wfgrep` | 1408.124 | 1399.948 | 0.9964 | 1.0079 |

The criterion is met: the one-function module's paired median improves by
12.4 percent, above the required 10 percent, and `wfgrep` does not regress.
The composition improves by 16.9 percent. No measurable `wfgrep` benefit is
established. The same-image median ratios are all within three percent;
the maximum absolute same-image pair variation is 17.5 percent for the
module, 3.1 percent for composition and 4.2 percent for `wfgrep`. This is
one shared host and seven pairs, so the result does not establish other
hosts' costs or reproduce the earlier Linux instruction counts.

### Protocol and reproduction inputs

Build each revision with `make -C compiler build`. Construction is excluded
from the check times: the matched incremental Cargo builds took 4.47 seconds
(baseline) and 4.27 seconds (candidate). The measured binary SHA-256 digests
were `3f1868b4e8134f72c1f5718f4b17f9785ac7b56ce344689badb4f90dbfe250c8`
and `85bd1ed6f275ab3ea792d18d7ba75c647a778a1bc4cbba21a15549b4a092d349`,
respectively. Build/profile/platform changes may change those bytes.

Create the following five UTF-8 files under a scratch fixture directory,
each with a final newline; no library module is named:

```text
# modules.wfg
pkg::a: [];
pkg: [pkg::a];

entry main = pkg::main;

# a/module.wfm
public fn one() -> result: u64 pure doc "Returns one.";

# a/body.wf
fn one() -> result: u64 pure {
  return 1_u64;
}

# module.wfm
public fn main() -> result: u64 pure doc "Returns the module result.";

# main.wf
fn main() -> result: u64 pure {
  let value = pkg::a::one();
  return value;
}
```

Invoke each compiler with the same working directory (the candidate checkout)
and these arguments, without a cache or report:

```sh
<compiler> --graph <fixture>/modules.wfg --check-module pkg::a
<compiler> --graph <fixture>/modules.wfg --entry main --check
<compiler> --check tests/programs/wfgrep.wf
```

Run the measurement under `.github/run-check.pl` to serialize it with other
heavy commands. For each workload, warm both sides once, then run seven
pairs, left first on even pairs and right first on odd pairs. Measure an
entire side with a monotonic wall clock: 25 successive subprocesses for
the module and composition, one for `wfgrep`, divided by the repetition
count. Require exit zero and byte-identical stdout/stderr both within a
batch and across its paired sides. First do all workloads with the baseline
on both sides and require every median paired ratio within three percent of
one; only then compare baseline and candidate. The table's paired ratios
are medians of the seven ratios, not ratios of the two medians. The
`wfgrep` regression limit is the larger of three percent and its control's
maximum absolute pair variation (4.2 percent here).

Before timing, both binaries rejected this source with exit one and identical
JSON diagnostics (`--diagnostic-format json --check <source>`):

```whitefoot
fn broken() -> result: unit pure {
  let value = 0_u64
  return unit;
}
```

The [raw samples](parser-lookup-samples.csv) retain 84 timed batches,
representing 1,428 process invocations; warmups and the rejection comparison
are excluded. They serve this checking-cost result in the existing modular
build experiment, and may be removed if a replacement makes this dated
comparison no longer useful. The one-shot timer stays outside the repository;
the protocol and complete inputs above define reproduction.
