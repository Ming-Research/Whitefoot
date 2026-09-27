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

For stage attribution, export each revision to a disposable scratch tree,
then run `python3 units.py --instrument TREE` and build that tree's compiler
under the shared verification guard, with a distinct Cargo target directory
for each exported tree. The same exact source boundaries report
source validation/resolution plus dependency-key assembly, formation/checking,
typed lowering and emission. These instrumented binaries are separate from
the primary timing pair. The instrumentation refuses the working repository
and nonunique insertion points; it lives in this experiment's driver and
retires with this retained-product comparison. No timers enter the compiler's
maintained acceptance path. Run the instrumented pair with `--compiler-only`;
the raw stage observations are retained in `stages_ms`.

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

## Limits

- Composition granularity: every build of an edited entry forms, resolves
  and type-checks the whole closure, which grows with the program; only the
  proof analyses are reused per function.
- The ThinLTO planning runs in full at every link, as the design's first step
  selects; at these sizes the whole link is under 150 ms.
- Two runtime workloads, both small loops; one host, one compiler build.
