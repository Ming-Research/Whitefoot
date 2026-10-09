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
