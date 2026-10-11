# Guard fairness probe

`probe.wf` is firn's guard-fairness probe, copied unchanged from Firn-wf
branch `exp/engine-probe`, `research/experiments/guard-fairness/probe.wf`
(Firn-wf's README there describes the instrument): N spawned contexts each
take one `Shared<State>` K times with `atomic p = &state when p^.out == 0`,
hold it for 4,096 dependent mixing steps, and release it; it prints the
exact p50, p99 and longest acquisition delay over all N × K acquisitions,
the mean hold and the run time.

The current protocol is the oldest-only handoff experiment registered in
[the investigation](../../investigations/guard-fairness/DESIGN.md#step-b-oldest-only-handoff-registered-before-measurement).
Run 38140292139 on revision `ba0e7c5e7` rejected this candidate under that
rule: its N = 50 tail passes, but runtime is 1.61× baseline. The
[results and attribution](../../investigations/guard-fairness/DESIGN.md#oldest-only-handoff-results)
include the twin control and separate diagnostic counters.

`build.sh` emits the unchanged probe's LLVM once with `WFC`, then uses
`Makefile` and the maintained `compiler/runtime.mk` native source list to
build three runtime arms from this checkout: `WF_GUARD_MODE=0` baseline,
`=1` rejected age-bounded turns, and `=2` oldest-only handoff. Every arm uses
the same emitted LLVM, C11, `-O2 -falign-functions=64`, full LTO and LLD,
with the ordinary runtime's POSIX thread and link flags. These are the
optimization settings of the previous driver's `--full-lto` build, applied
through native object rules so no experimental compiler option is needed.
The age arm retains the rejected algorithm in the current source; it is a
same-source reference, rather than a reconstruction of the earlier compiler
binary. `twin` is a byte-for-byte copy of baseline, checked with `cmp`.
Construction records include the revision, compiler and executable hashes,
LLVM hash, clang and linker versions.

`run.sh` first runs two N = 2, K = 100 pilots per arm, retaining their spread;
a pilot exceeding 10 seconds stops expansion. It then fixes K = 1,000 and
uses `WF_DRIVERS=2`, pinned to CPUs 2 and 4 on the native i9-14900K. Pass 1
uses N = 2, 8, 50 and arm order oldest, baseline, age, twin; pass 2 reverses
both orders. Each child has a 120-second full-run watchdog: a timeout is
missing evidence, never a changed bound or a completed comparison. Pilots
are excluded from the decision. For every pass/N cell, oldest runtime/base
must be at most 1.10; at N = 50 oldest maximum wait must be at most
3,300,000 ns. Twin runtime/base must lie in [0.95, 1.05] in every cell;
a failed control makes the comparison inconclusive. No averaging can hide
a failed cell. `summarize.py` validates completed work and applies these
rules; its sensitivity fixtures run only in the temporary CI workflow.
Rejection or an inconclusive result is a research verdict, rather than a
compiler correctness failure.

The workflow first builds counter arms separately with
`WF_GUARD_COUNTERS=1`, and runs one diagnostic sweep at N = 2, 8, 50 after
its pilots. Runtime stderr records total waiters woken, spurious retries,
lock grabs on refused turns and fairness clock reads. The spurious counter
counts a previously notified guard that reads false and actually parks again,
excluding an early wake that makes park return immediately. It covers guards
holding at least one ordinary object; table-only successful statements have
no completion hook in the bridge to clear their retry attribution. Woken
notifications still include all guard units. The harness prints
each total divided by the probe's completed acquisition count beside the
same run's latency percentiles, and preserves both raw totals and normalized
CSV. These whole-process counts include ordinary startup/teardown and the
probe's other atomic statements; the denominator is successful guarded
checkout, so the reading is amortized event cost rather than individual
checkout tagging. Wake-to-run latency is omitted because extra timestamps
at every wake and resumption would perturb this attribution run. Diagnostic
timings judge nothing; uninstrumented arms alone determine acceptance.
The new per-watch test observer is separately guarded by
`WF_GUARD_TEST_HOOKS`, enabled only in native test builds, so counter-free
production builds retain neither its call nor counter bookkeeping.

All builds and runs use CI. The temporary workflow stores raw stdout/stderr,
combined CSV, source LLVM, construction metadata and the paired verdict in
the `guard-fairness-evidence` artifact. It is removed before branch readiness.

```sh
WFC=<compiler> OUT=<scratch>/gf-counters COUNTERS=1 sh research/experiments/guard-fairness/build.sh
OUT=<scratch>/gf-counters DIAGNOSTIC=1 sh research/experiments/guard-fairness/run.sh
WFC=<compiler> OUT=<scratch>/gf-timing COUNTERS=0 sh research/experiments/guard-fairness/build.sh
OUT=<scratch>/gf-timing sh research/experiments/guard-fairness/run.sh
```
