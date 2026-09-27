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

## Limits

- Composition granularity: every build of an edited entry forms, resolves
  and type-checks the whole closure, which grows with the program; only the
  proof analyses are reused per function.
- The ThinLTO planning runs in full at every link, as the design's first step
  selects; at these sizes the whole link is under 150 ms.
- Two runtime workloads, both small loops; one host, one compiler build.
