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

This bundle answers slice 6 of the
[modular compilation design](../../investigations/modular-compilation/DESIGN.md#ordered-implementation-slices-and-completion-evidence)
for the implementation that exists: module verdicts that record which
declarations of other interfaces their check reached, function analyses
reused through proof receipts, stable names for every link-visible entity, a
native split of the emitted module into ThinLTO fragments, cached runtime and
program objects, and LLD's ThinLTO object cache. A composition, and so every
build, first requires the verdict of every module in the entry's closure and
then checks the closure as a whole, taking the analyses whose inputs are
unchanged from their receipts; it still forms, resolves and type-checks
every body of the closure.

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
question of whether this PR currently slows an edit build:

| Workload | Native entry-edit ms | Compiler-only entry-edit ms |
|---|---:|---:|
| grow-vector | 227.5 / 193.8 (-14.8%) | 157.1 / 117.1 (-25.5%) |
| hash-map | 363.4 / 336.3 (-7.5%) | 291.3 / 260.2 (-10.7%) |

The complete PR is faster than unchanged main on these two workloads, but
the matched control shows that the general hashing gain does not establish
the module-import amendment's condition that importing costs less than the
work saved. Keep that design finding open for the owner; do not infer approval
or revise its condition from the overall speedup.

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
