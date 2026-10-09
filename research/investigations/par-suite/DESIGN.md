# Stage-one baseline for automatic parallel lowering

## Question and prospective comparison

What does today's ordinary `--par` add to or save against the same source
compiled sequentially, across work size, decomposition, site frequency,
cost spread and intended bottleneck? The owner-selected first stage is the
[unified suite](../../experiments/par-suite/README.md), before changing the
scheduler or its constants. This serves the constitution's requirement to
test useful performance claims without weakening safety; it changes no
language rule or compiler design decision.

The supplied plan's H1 asks for `T_W <= (1+epsilon)*T_seq+d` at every worker
count above one, with proposed `epsilon=0.02` and a per-host startup allowance
fixed before measurement. A cell beyond that bound by more than its identical
image twin's spread, repeated once, refutes the measured direction. The suite
implements conditional cell observations and retains CPU cost alongside wall
time. The initial CI command deliberately names `d=0` as uncalibrated; it
cannot settle the final startup-inclusive criterion. A measured allowance
with a recorded calibration identity must precede a qualified campaign.

H2 asks for at least 0.8 of available parallelism on generated workloads and
0.8 of a tuned same-algorithm reference speedup on real programs. Stage one
does not implement its references or work/span oracle and produces no H2
pass. A fast wall result alone is not that evidence.

## What the first CI run establishes

The frozen sparse grid has 216 cells, split equally before measurement. A
small visible pilot precedes the full baseline. The first run can establish
native construction, actual checksum agreement, admitted/actualized source
shapes from retained ledgers, process wall and CPU costs, twin variability,
and which cells warrant the one separately retained rerun. The compiler and
runtime are from the same revision; timing excludes compiler construction.
Held-out measurements are for verdicts only, with two real programs also
reserved. Their results must not select later constants.

The comparison reuses the ordinary runtime, the formal entry-world adapter,
and the complete maintained paired instrument for its five kernels. The new
process timer is necessary because the maintained instrument intentionally
excludes startup/shutdown, while H1 charges per-process startup. The two
interval types and verdicts remain explicitly separate. A failed formal
null control stops that comparison; missing evidence cannot become a pass.

## Limits and rejection of overclaims

This first run cannot prove the universal H1 bound, the behavior when a work
estimate is wrong by arbitrary factors, H2, non-Linux targets, contention with
unrelated processes, I/O composition, or downstream application behavior.
Nor does a named memory/allocation cell prove its native bottleneck: inspect
emission and eventually profile it. Integer clipping means requested cost
spread is an upper bound, and tiny skewed inputs coincide. A log-scale work
parameter is not a nanosecond calibration. The one-hour budget and the
sub-second real-program inclusion candidates remain unmeasured assumptions;
timeouts stop with incomplete evidence and retain logs.

The references and external-repository hooks are the next coverage steps
listed in the suite README. They do not justify replacing missing evidence
with an easier workload or changing language acceptance. No runtime policy,
specification rule, conformance verdict, or design-tree choice is amended.

## Implementation evidence, 2026-10-09

All 216 generated sources (`p000` through `p215`) passed `whitefootc --check`
with the owner-authorized older main binary, SHA-256
`8fb95f62238a45b4d014a70e0358f51ad32c1f1ee1877892e7a2260dbd19dc27`.
The checked manifest's SHA-256 was
`c76d33708a51ee5e8c65e30b67f7c1c320fb2640d43659651870f7cca8686624`.
This establishes source admission only; it does not establish parallel
permission, native emission, checksums or performance. No Rust build, cargo,
test suite, native program or timing ran on the development machine.

Early generator drafts failed canonical syntax, argument-expression and
effect-row requirements; these were source-generation errors, corrected
before checking the complete grid. An initial memory loop used a conditional
minimum as its bound, whose relation to the slice length the older checker
did not retain. The delivered version uses the task-requested explicit
index guard; that proof-transport limitation is not evidence of a main
regression and should be re-examined on current main before any compiler
conclusion. No final generated source was rejected by the old compiler.

The native instrument controls and all CI campaigns are unrun at this
handoff. The change remains uncommitted, as requested; approval and any
subsequent measurements belong to the owner/coordinator's next step.

A separate read-only Codex reviewer inspected the full uncommitted change
against the repository checklist and relevant design ancestors. Native
behavior remained unverified. Its measurement-integrity finding was fixed:
an existing formal-kernel verdict file is insufficient when the maintained
instrument exits with a refusal; the suite now requires a completed verdict
and its matching success/performance-failure status. A synthetic CI control
covers refused, missing and contradictory verdicts. The standalone summary
also now states allowance provenance so its exploratory zero-startup result
cannot lose that qualification when copied into CI's job summary.
The reviewer inspected those fixes separately and reported no remaining
findings in the changed regions; the new CI controls remain unexecuted.

## Baseline, 2026-10-09

Two runs of the `par-suite` job of `compute-bench.yml`, each building the
compiler and runtime of its own revision, 3 interleaved rounds per cell and
worker count, `d=0` (uncalibrated, so every cell's formal verdict is
inconclusive and the ratios below are exploratory):

- hosted, [37917975092](https://github.com/Ming-Research/Whitefoot/actions/runs/37917975092),
  4-vCPU AMD EPYC 7763, branch at d392af91c's main plus this suite, worker
  counts 1, 2, 4, 8, 16 and default (4);
- i9-14900K, [37990650681](https://github.com/Ming-Research/Whitefoot/actions/runs/37990650681),
  32 logical CPUs, branch at fc594f767 (main 184a4c3ef plus this suite),
  worker counts 1, 2, 4, 8, 16, 32 and default (32).

The ratio is the median `--par` wall time over the sequential image's. Cells
by ratio on the 14900K, visible and held-out split counted apart:

| workers | split | cells | > 2 | 1.1 to 2 | 0.9 to 1.1 | < 0.9 |
|---|---|---|---|---|---|---|
| 4 | visible | 118 | 5 | 32 | 65 | 16 |
| 4 | held-out | 110 | 5 | 33 | 56 | 16 |
| 8 | visible | 118 | 6 | 35 | 62 | 15 |
| 8 | held-out | 110 | 10 | 28 | 56 | 16 |
| 32 (default) | visible | 118 | 40 | 4 | 64 | 10 |
| 32 (default) | held-out | 110 | 41 | 10 | 50 | 9 |

Visible cells at the default 32 workers, by family (minimum, median,
maximum ratio):

| family | cells | min | median | max |
|---|---|---|---|---|
| work | 79 | 0.34 | 1.01 | 9.45 |
| bound | 13 | 0.77 | 1.11 | 8.64 |
| size | 10 | 0.98 | 1.04 | 15.54 |
| hot, 10 calls | 3 | 0.97 | 8.09 | 8.98 |
| hot, 10,000 calls | 1 | 39.14 | 39.14 | 39.14 |
| hot, 10,000,000 calls | 2 | 1.00 | 248 | 248 |
| real programs | 10 | 0.93 | 1.01 | 9.05 |

The visible real programs most slowed: `merge_sort` 9.05, `range_split` 7.16,
`radix_scatter` 4.86, `sha256_abc` 1.35; the other six within 0.93 to 1.01.

The hot family is a four-leaf recursive split called from a dependent
sequential loop; every call hands its leaves out, so its cost grows with the
call count and with the worker count (the hosted 4-vCPU run measured a
median of 40 at 10,000,000 calls, the 14900K 248 at 32 workers). On both hosts
today's `--par` is slower than one thread in far more cells than it is
faster, which fails H1 as stated above at every worker count measured.

Held-out discipline: the held-out cells are reported only in the counts
above. One working listing of the 14900K real programs printed all twelve,
so the ratios of the two held-out ones were seen (both between 0.95 and
1.01); no constant or design choice has been taken from them.
