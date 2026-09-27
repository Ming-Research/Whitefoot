# Experiments Index

Current experiment bundles are self-contained: sources, a Makefile or run
script, and a RESULTS.md with measured numbers and honest caveats.
Binaries/corpora are regenerable and gitignored. Some retained historical
bundles below still name the retired democ toolchain; their RESULTS and source
evidence remain useful, but their old runner is not replayable from HEAD and is
not a current compiler gate. Historical chronology and decisions are indexed
by `../../archive/governance/decision-log.md`; current design decisions live in
`../../design/`, research questions in [ideas](../../docs/ideas.md), and known
compiler defects in [todo](../../docs/todo.md). Research notes do not grant or
withhold branch permission.

These bundles follow the [research boundary](../README.md): execution is
explicitly requested, and useful daily regression checks are extracted into
formal test ownership with their required inputs and oracles. Existing caller
descriptions do not grant an exception to that boundary.

## Language design models

- [access-state/](access-state/RESULTS.md) — symbolic access/state checking of
  an earlier alias/state fragment, compared with reusable physical storage. A
  language-design model, not a WF source compiler or a soundness proof; it is
  superseded by the access-effects investigation's candidate x1 and kept as an
  experiment record. Its self-checks run manually with
  `make -C research/experiments/access-state check`.

## Current flagship experiment evidence

The owner's ruling for the flagship: ripgrep is the umbrella target with a
fair two-times end-to-end objective, performance comes first in that loop,
and a missing performance capability stops downstream expansion until its
owning layer is fixed rather than being written around, so that the target
exposes general language defects and every win is attributable to generated
code. SQLite as the umbrella target and shipping the finished tool as the
completion criterion were refused.

- `park-on-miss-switch-cost/` — the first §12 measurement of the park-on-miss
  design: one hand-written stack switch against a condition-variable
  park-and-wake on the same host. Measured 2026-09-04 (Darwin arm64): 9.8–10.4
  ns per switch, 872–934 ns per park-and-wake, 84–95×; `swapcontext` 345–355
  ns. The design's bar (a switch well under one park-and-wake) is met. The
  park-on-miss scheduler was retired on 2026-09-10 in favor of current-stack
  execution
  ([`compute-runtime/PRIOR-BUNDLE.md`](../investigations/compute-runtime/PRIOR-BUNDLE.md)),
  so this is a record of the retired design's bar, not of the shipped runtime.
- `wfgrep-baseline/` — the PERF-1 zero-change baseline of the frozen
  sequential wfgrep against the pinned system `grep -h -F` (BSD grep 2.6.0
  on the macOS host), preregistered
  with null-comparison precision gates per the RG-BASE lesson. Measured:
  0.647/0.656 (large/no-match scan), 0.605 (many small files), 1.105
  (match-dense win), smaller process floor. Attributed: the dominant
  many-files loss to the host's per-open cost for unsigned binaries (C
  control; not a Whitefoot layer); the compute loss to the scalar
  double-walk shape, with the literal matcher above the newline scan and
  the retained per-byte traps a bounded ~18%-ceiling secondary term.
  The run is closed and its driver is not replayable from HEAD: the subject
  was the `tests/programs/wfgrep.wf` of 2026-08-06, which became a recursive
  search printing `PATH:LINE:TEXT` lines on 2026-08-18; `MANIFEST.txt`
  remains the pinned corpus and output identity.
- `wfgrep-double-walk/` — the follow-on slice on the baseline's attributed
  compute cause: three legal source shapes paired against a fresh B0 under
  the inherited corpus and pins. Credited: S2, the fused single-pass
  scan+match, 1.150/1.145 on `large`/`nomatch` with byte-identical behavior,
  landed as `tests/programs/wfgrep.wf` on 2026-08-06; S3, the word-at-a-time
  newline scan, is the witness that no legal shape widens the serial
  per-byte step under that lowering. The run is closed and B0 cannot be
  rebuilt from HEAD. RESULTS.md records that on 2026-09-03 the bundle's own
  verify phase matched the three shape sources' outputs against the inherited
  manifest byte for byte; the sources were last respelled on 2026-09-13, do
  not parse under the active specification, and are not run by the root
  `make check`.
- `wide-scan-lowering/` — the lowering answer to the double-walk's latency
  floor: the same landed `wfgrep.wf` bytes compiled by the base-revision
  compiler and by the candidate carrying the check-aware wide probe.
  Credited route (b): a material 1.43x on both scan cases with every
  then-required runtime trap preserved observably (runtime traps were retired
  in v0.40), and the confirm rerun moves wfgrep past the macOS system grep
  (BSD grep 2.6.0) on every compute-bound case: directional wins on the two
  scan cases and a material win on the match-dense case. Closed and not
  replayable from HEAD: the subject program changed on 2026-08-18 and
  `base` needs a pinned base-revision compiler worktree.
- `ripgrep/` — RG-BASE preregistration for the owner-selected 2x ripgrep
  flagship. It freezes the Apple M4 target, pinned official/native ripgrep
  comparators, two real source trees, one large-text corpus, nine equal-weight
  end-to-end cases, correctness oracles, statistics, and the future 2x rule
  before comparative timing. Attempt 1 (2026-08-05), the comparator-selection
  run between the official and native ripgrep builds, was inconclusive: every
  correctness oracle passed, but no case met the 3% precision gate, so no
  comparator was selected, no Whitefoot timing has run, and there is no
  Whitefoot-versus-ripgrep result.
- `compute-bench/` — the compute scoreboard: for each of four kernels
  (adaptive-Simpson recursion, UTF-8 record batches, a flat FIR map, a skewed
  Mandelbrot map), at each width, is the Whitefoot program built by this
  tree's `whitefootc` with plain `--par` the fastest thing in the row? One
  uniform harness, one scheduler boundary, bit-for-bit equality against an
  independent oracle per call, and native references built on oneTBB,
  ParlayLib, Rayon, a static pthread pool and a serial loop at fixed grain
  policies. Nothing in it fails on a ratio, a spread or an elapsed time; its
  compile-only `programs-check` is an optional manual check, and the root
  `make check` does not run the bundle. The dated tables are recorded in
  [`compute-runtime/RESULTS.md`](../investigations/compute-runtime/RESULTS.md).
  In the first four — a baseline at compiler `33ed2c00` and the merged tree at
  `11d1e4a2` on a four-CPU local Linux host, and the first hosted run,
  `34574271919` at `5dd1eb7b`, one section per leg — which kernel's
  plain-`--par` program is the fastest form in its block differs by host: FIR
  alone on the local host, none at W=4 on the `ubuntu-24.04` runner, and
  records, FIR and Mandelbrot at W=2 on the three-CPU `macos-14` runner. The
  native references are built `-O3` with vectorization disabled while the WF
  module may vectorize, so, as the bundle's README states, these comparisons do
  not establish competitiveness against optimized native implementations; the
  bounded 2026-09-22 control that permits native vectorization on two fixtures
  is retained beside the bundle.

## Completed current-compiler bounded research

- `differential-fuzz/` — the mechanical source of programs nobody wrote, for the
  property [PAR-1] and [PAR-2] state: under a permitted overlap the observables
  equal the source-order ones, and whether an overlap happened is not
  observable. A seeded generator writes accepted command programs that do real
  I/O and control flow from the [GRAM-4]/[GRAM-5] fence under a typing and
  ownership environment; the oracle compiles each three ways, establishes that
  the program agrees with itself, and then requires the overlapping builds to
  publish the same stdout, stderr, and exit status across
  `WF_WORKERS` x `WF_IO_HELPERS`, some of them with stdout on a FIFO whose reader
  is delayed. [`differential-fuzz/RESULTS.md`](differential-fuzz/RESULTS.md)
  records a local smoke run taken during the amendment that retired PAR-3
  staged overlap: 23 accepted programs, 483 captured executions, 7 permitted
  [PAR-1] pairs and 6 permitted [PAR-2] loops, zero divergences, zero unstable
  references. A first full campaign ran on 2026-08-28, while PAR-3 still
  existed; its results are not recorded in this bundle, and no full campaign
  has been recorded since. Not a gate and not reachable from `make check`.
- `blind-writer/` — the standing corpus of what unguided writers write, one
  dated directory per trial. The 2026-08-28 trial gave a writer with no prior
  Whitefoot exposure the v0.38 spec, `docs/patterns.md`, the gate binary and
  `tests/programs/`, and asked for five ordinary I/O utilities. The trial
  describes the writer as a senior systems programmer, but its report states
  that its timings are "model time, not human time": the writer was an AI
  model, and which model was not recorded. All five programs compile with zero
  `claim` statements (v0.38's explicit proof statement); four are correct
  against their references, and the stdin-to-stdout filter was not writable as
  specified because v0.38 had no standard input, so that program copies a
  named file instead. None of the five received a staged I/O overlap: every
  `PAR stage` verdict for programs p1–p5 in the trial's `ledger/` is denied,
  while three of the trial's probe programs show the permitted form. That
  finding concerns PAR-3 staged overlap, which the C2 amendment later removed (see
  `io-completion-bench/` below). The writer's findings are summarized in
  [`REPORT.md`](blind-writer/2026-08-28/REPORT.md) §8. It is removed when the
  language stops changing.
- `park-on-miss-measurements/` — the rest of the §12 measurements and the four
  choices the plan added on 2026-09-05, each alternative built behind a
  compile-time `-D` in the scheduler core and measured against the shipped form
  with the io-completion-bench runner's discipline. Measured 2026-09-05 (Linux,
  four cores, clang 18): the then-shipped park and publish is 4.40 µs at best
  and 6.23 µs at the median, against this host's own 16.2 µs condition-variable
  park-and-wake and the design's quoted 2.2 µs; the lane slot count cannot be
  separated between 4 and 64; the pool stops refusing at twelve stacks at four
  workers and twenty at eight, and a refusal costs no measurable wall time; the
  record's growth is exactly 32 bytes per outstanding operation a frame holds;
  every hand-out entry in `tests/programs` is bounded by 80 bytes. Nothing is
  chosen here. The result that decides the rest is that five of the six
  behavioural variants cannot be measured: four are rejected by the §11
  enumerator, and `WF_SCHED_WEAK_ORDERS` passes the enumerator and then hangs
  `par_layout` deterministically at two workers and above, which is the
  sequentially consistent blind spot the plan's deferred GenMC note names,
  witnessed. Slice 4b acted on that record: the six behavioural switches are
  deleted from the core, `WF_SCHED_LANE_SLOTS` stays the `#if !defined`
  override of `core.h` it was before the sweep, and the bundle keeps its
  tables and the sections it can still reproduce. The park-on-miss scheduler
  these numbers measured was retired on 2026-09-10 in favor of current-stack
  execution
  ([`compute-runtime/PRIOR-BUNDLE.md`](../investigations/compute-runtime/PRIOR-BUNDLE.md)).
  It goes when §12 item 1 and the chain bar are answered or retired.
- `io-completion-bench/` — program-level measurement of ordinary linked I/O
  calls against native C controls (a direct loop, a thread pool and, on Linux,
  a raw io_uring pipeline) and the same Whitefoot source built with and without
  `--no-overlap`, every line publishing the same checked bytes. The current
  status is [`C2-RESULTS.md`](io-completion-bench/C2-RESULTS.md): the C2
  amendment removed PAR-3 staged overlap, the default and `--no-overlap`
  builds emit byte-identical LLVM, and every open and read uses one exclusive
  `HandleFactory`, so the calls run sequentially. On the 8192-file traversal
  on Linux CI the default Whitefoot build measures 1.36–1.45 times the direct
  C median and 4.60–4.89 times the four-thread pool; on Linux warm 4 KiB
  positioned reads it measures 1.85–1.95 times direct C; the TCP harness
  recorded no Whitefoot sample. The earlier tables in
  [`io-model/RESULTS.md`](../investigations/io-model/RESULTS.md) measured the
  removed staged-overlap mechanism, and that file itself retired their headline
  reading before C2: the roughly two-times overlap gain was a macOS reading on
  a host whose `openat` cost 116 µs, an ordinary macOS host measured the
  completion build 1.20 times slower than its own sequential build (narrowed
  to 1.02), and Linux hardware did not reproduce the container ratio. The 68
  scheduler experiments this bench later carried — the TCP packet-policy tail,
  the client-width reversal, the storage and allocator negatives, the native
  and Go references and the continuation lowering — are digested in
  [`io-model/SCHEDULER-FINDINGS.md`](../investigations/io-model/SCHEDULER-FINDINGS.md),
  and the prior compute bundle's grain panel, recursion-frontier evidence and
  two-runtime comparison in
  [`compute-runtime/PRIOR-BUNDLE.md`](../investigations/compute-runtime/PRIOR-BUNDLE.md).
- `buffer-initialization-cost/` — the dossier §9.1 initialization-cost row,
  whose control §9.1 requires to be an *uninitialized* native read loop. A
  Whitefoot drain over a language-initialized reused buffer measures at
  practical parity with the uninitialized C control (1.0014 [0.9982, 1.0083]),
  and the same-source `calloc`/`malloc` ablation is likewise parity. The
  decisive figure is direct: initializing one 4096-byte page costs 28.76 ns,
  which is 612x below 1% of this program's 1.76 ms empty-input process floor,
  so no input size makes it material. Dossier §11's stop condition did not
  fire.
- [modular-build-cost/](modular-build-cost/RESULTS.md) — slice 6 of the
  [modular compilation design](../investigations/modular-compilation/DESIGN.md):
  check and build cost of module programs across `whitefootc --cache` and its
  ThinLTO link fragments, per edit kind, on the specimen and generated module
  chains, and the runtime of one cross-module loop per build mode. Warm checks
  and builds cost input validation and the final link; a body edit recompiles
  one object in every mode, with the whole-composition front end and the
  fragment split as the remaining edit costs; no build mode shows a repeatable
  runtime loss at the measured noise level.
- [bounds-check-spellings/](bounds-check-spellings/README.md) — the root
  README's first example: eleven Rust spellings of one in-place loop, compiled
  with rustc 1.98.1 at `-C opt-level=2` and `3` for x86-64, against the
  Whitefoot function with one checked invariant at clang `-O2`. Seven safe
  spellings keep a compare in the loop, leading to a bounds panic, an
  assertion panic, a `copy_within` panic or a skipped store; the two `unsafe`
  spellings, `Vec::retain` on an owned `Vec` and collecting into a second
  buffer keep none, and neither does the Whitefoot loop, which without its
  invariant is rejected with the residual `kept < deref(buf).len`. Code
  generation only, no timing.
- [par-quicksort/](par-quicksort/README.md) — the root README's second
  example: whole-process wall time of a quicksort over 2,000,000 `u64` values
  built without and with `--par`, best of seven under the host lock on a
  four-processor x86-64 host, 0.178 s sequentially and 0.069 s on four
  workers. One input, one size, one host.

## Frozen v0.17 floor studies

Two single-buffer floor measurements of the v0.17 language, complete and
closed. Their kernels no longer parse under the active specification (a
function's result binding is now named, and the `traps` effect was retired
in v0.40), so their drivers were removed on 2026-09-03 and the bundles are
records: sources, controls, harness, runner, raw evidence, and the freeze
commit each RESULTS.md names.

- `literal-line-floor/` — the then-active v0.17 language expresses an exact
  runtime-needle literal line matcher, but its helper-shaped scalar lowering is
  directionally about 5% behind same-Clang C: C/Whitefoot is 0.9535
  [0.9223, 0.9609], below the preregistered material-loss threshold. Pinned
  Rust `memmem` is descriptively 7.33x the same-toolchain scalar control with
  the expected NEON packed-pair mechanism, but strict primary parity was not
  met, so this is not promoted to a language, ripgrep, end-to-end, or 2x claim.
- `wfgrep-scan-floor/` — the then-active v0.17 language and ordinary
  compiler match same-Clang C on two safe single-buffer scanner shapes. The
  width-16
  Boolean-dataflow full pass measures 0.9993x C [0.9969, 1.0023], and four
  scalar early exits measure 1.0008x C [0.9981, 1.0088]. LLVM removes the
  guard-dominated bounds traps in both. This validates a narrow language floor,
  not an algorithmic, end-to-end, or 2x-ripgrep claim.

## Historical fact-channel benchmarks

These results used the now-archived democ implementation. They remain measured
evidence rather than current-compiler benchmark requirements. Historical
benchmark runners are not active compiler gates. Self-tests of the completed
frequency and model-trajectory instruments remain available through
`make historical-tool-tests`. Useful current-compiler observations and
independent oracles require a formal test home outside this directory.

- `effect-attrs-channel/` — channel 2: effect rows -> LLVM fn attributes.
  O(n)->O(1) at opaque boundaries; ties fat LTO at per-file build cost.
- `scoped-alias-channel/` — channel 1: ownership provenance -> alias.scope.
  Short-trip wins, 17x code size vs Rust's guard-versioned loops; parity at
  long trips (Rust recovers via runtime checks).
- `checked-law-channel/` — historical channel 3: the retired law table licensed
  reassociation, measuring 3.3x over the obvious fold. D7 removes that syntax;
  its two WF kernels remain dated evidence and are not active test targets.
- `frequency-study/` — completed one-time directional scan of popular Rust
  sources/applications, a source and optimized-IR survey rather than a democ
  measurement. Its manual audit found 0 plausible advantages for the
  then-current Whitefoot among 31 high-signal library records; it points the
  next real port at relational bounds proofs.

## Paused expressiveness evidence

- `data-layout-owning-sequence/` — historical E0.1 research and rejected
  isolated-prototype evidence. D11 leaves all work paused before bounded G0-Core
  and a later exact dense-family Lock A; neither is currently selected. The
  evidence separates fixed SoA/AoS
  layout from initialized-prefix ownership and growth, protects the compiler's
  current SoA as the zero-tax baseline, and forbids feature-flagged dual
  semantics.

## Port studies (real programs; historical D9-era evidence)
- `zlib-core-kernels/` — deferred RFC 1951 kernel handoff. Ordinary scalar
  lowering is not competitive for short-period match overlap and trails the
  pinned all-literal Huffman projection; bounds-check elision alone does not
  close either gap. Two unchanged-source stage-0 prototypes recover isolated
  performance through periodic expansion and a guarded six-symbol bit window;
  the periodic prototype calls hand-written ARM NEON C after structural
  matching, and the guarded one accepts one alpha-normalized AST digest.
  The directory preserves corrected raw results, compiler patches, LLVM and
  ARM64 snapshots, candidate writer patterns, proof obligations, and production
  pickup gates. These are feasibility results, not complete proofs or a
  whole-inflate claim.
- `default-floor/` — historical D9a protocol: a fixed low-tier model's first
  correctness-green Whitefoot artifact versus an exact unmodified shipped Rust
  library. Two separately preregistered results are complete: Terra Whitefoot beats
  `percent-encoding` 2.3.2 `percent_decode` by 1.653x [1.631, 1.667] and
  one-shot `utf8parse` 0.2.2 by 1.098x [1.085, 1.145]. Neither result is a
  proof-elision win, and current W1 does not use a model score as a gate; see
  the aggregate claim boundary in
  `default-floor/RESULTS.md` and the two target-specific reports beneath it.
- `port-study/binary-trees/` — floor-raising result: the v0 language's
  no-reborrow rule steered the port to the fast bottom-up shape (RESULTS.md
  corrects the earlier "only expressible shape" reading, and the current
  language has recursive `Box` trees, as in
  [`recursive_tree.wf`](../../tests/programs/recursive_tree.wf)); ~11%
  checked-semantics tax vs identical-shape Rust.
- `port-study/wc/` — full-counts 0.27s vs GNU 0.48 / uutils-Rust 0.56 on a
  426MB corpus (regenerate: see RESULTS); -l honest gap vs memchr/bytecount.
- `port-study/wc-chunk-summary/` — ordered-monoid parallel wc. NEGATIVE
  result for channel attribution (Rust expresses the same algebra); reached
  C/Rust parity after the OWN-1 Bool-copy amendment (220->134ms).
- `port-study/base64/` — first const-array consumer. PROOF-1 discharged 15/27
  bounds sites (kernel 2.50 -> 2.93 GB/s); PROOF-2 then proved all 27, and the
  kernel measures 4.23 GB/s against 2.48 GB/s for the same source without
  proof facts (1.71x). On 384 MB the CLI takes 0.16 s against BSD 0.21 s and
  GNU/uutils 0.36 s, and the controlled Rust adversary puts the scalar kernel
  at practical parity with expert safe Rust.

## Preserved code-generation fixtures

- `codegen-vs-rust-c/` — the splitmix scalar-backend-parity evidence. Its old
  democ runner is dormant and has no current compiler adapter. Making it a
  maintained test would require a Rust harness that binds the fixture to the
  current compiler.

## Earlier corpus-era studies
Moved to `../../archive/experiments/` (scatter residual, guarded-plan
measurements). Their durable conclusions and current dispositions are
summarized in `../archive-promotion-audit.md`; the old protocols add no current
workflow requirements.
