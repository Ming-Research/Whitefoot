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
