# Guard fairness probe

`probe.wf` is firn's guard-fairness probe, copied unchanged from Firn-wf
branch `exp/engine-probe`, `research/experiments/guard-fairness/probe.wf`
(Firn-wf's README there describes the instrument): N spawned contexts each
take one `Shared<State>` K times with `atomic p = &state when p^.out == 0`,
hold it for 4,096 dependent mixing steps, and release it; it prints the
exact p50, p99 and longest acquisition delay over all N × K acquisitions,
the mean hold and the run time.

The current comparison tests an ordered wait queue for oldest-only handoff,
with its cost-attribution prediction registered in
[the investigation](../../investigations/guard-fairness/DESIGN.md#ordered-wait-queue-prediction-registered-before-implementation).
The original acceptance criteria and timing protocol still apply. Run
38140292139 on revision `ba0e7c5e7` rejected the earlier implementation
that scanned for each successor: its N = 50 tail passes, but runtime is
1.61× baseline. The
[results and attribution](../../investigations/guard-fairness/DESIGN.md#oldest-only-handoff-results)
include the twin control and separate diagnostic counters.
No ordered-queue measurement is claimed here.

`build.sh` emits the unchanged probe's LLVM once with `WFC`, then uses
`Makefile` and the maintained `compiler/runtime.mk` native source list to
build three runtime arms from this checkout: `WF_GUARD_MODE=0` baseline,
`=1` rejected age-bounded turns, and `=2` oldest-only handoff with an ordered
wait queue. Every arm uses
the same emitted LLVM, C11, `-O2 -falign-functions=64`, full LTO and LLD,
with the ordinary runtime's POSIX thread and link flags. These are the
optimization settings of the previous driver's `--full-lto` build, applied
through native object rules so no experimental compiler option is needed.
The age arm retains the rejected algorithm in the current source; it is a
same-source reference, rather than a reconstruction of the earlier compiler
binary. `twin` is a byte-for-byte copy of baseline, checked with `cmp`.
Construction records include the revision, compiler and executable hashes,
LLVM hash, clang and linker versions. Timing builds use this original LLVM
unchanged. Optional counter builds retain it and additionally save
`probe-diagnostic.ll`, described below, with its own hash.

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

The workflow's `path_diagnostics` boolean defaults to false: that dispatch
builds and runs only the uninstrumented comparison. With it true, the
workflow also builds separate arms with `WF_GUARD_COUNTERS=1`. After each
oldest, baseline and age timing cell in pass 1, it runs that arm's diagnostic
image at the same N and K. Thus each diagnostic arm runs once at N = 2, 8,
50, between timing executions, rather than in a separate prior sweep.
Diagnostic pilots follow each corresponding arm's two timing pilots.
Removing the diagnostic executions recovers the original timing order in
both passes; diagnostics write their own CSV and never enter the judge.

Runtime stderr records total waiters woken, spurious retries,
lock grabs on refused turns and fairness clock reads. The spurious counter
counts a previously notified guard that reads false and actually parks again,
excluding an early wake that makes park return immediately. It covers guards
holding at least one ordinary object; table-only successful statements have
no completion hook in the bridge to clear their retry attribution. Woken
notifications still include all guard units. The harness prints
each total divided by the probe's completed acquisition count beside the
same run's latency percentiles, and preserves both raw totals and normalized
CSV. The diagnostic schema additionally reports seven oldest-only path
totals: batch notifications at or below T, initial age-qualified selections,
false-condition handoffs, rescans, links examined while passing a
reservation, notifications bypassing handoff because an ordinary hold has
already been granted, and notifications falling back to a batch because
the age clock is missing or backward. The three batch counters count
notifications rather than batches. Link visits include unsuccessful scans
and cross-object continuation; initial oldest searches, position anchors
and registration comparisons are excluded. The ordered queue retains an
anchor for the reserved waiter's position, so its false re-registration
inserts directly after that anchor. New waits and unreserved retries still
search for their insertion position; passing-link counts therefore do not
measure every comparison needed to maintain the queue. Age
selections can occur during a rescan, so those columns are not disjoint.
These path totals are zero in the baseline and age reference. Raw totals
and all event counts divided by successful checkout count are preserved.

These whole-process counts include ordinary startup/teardown and the
probe's other atomic statements; the denominator is successful guarded
checkout, so the reading is amortized event cost rather than individual
checkout tagging. Wake-to-run latency remains omitted because extra
timestamps at every wake and resumption would perturb the attribution run.

`tag-acquire.py` instruments only diagnostic LLVM using the generic
`wf__guard_measure_next_acquire()` runtime observer. The build selects the
second ordinary acquire in `wf_contend`, after `wf_hold_work`: the return
statement's first actual lock attempt. It inserts one marker in the unique
incoming basic block before that acquire's retry loop, leaving the checkout,
retry calls, later lazy-lock calls and final state read untouched. A changed
or ambiguous IR layout fails construction. The original probe source and
original emitted IR are preserved, and run.sh verifies that timing and
diagnostic builds started from byte-identical original IR.

The observer samples returns 1, 65, 129, … separately in each context, with
two monotonic clock reads per sampled acquire. Its interval starts at the
public acquire entry and ends on successful acquisition, retaining the start
across spin, park and resume. This measures actual return-lock wait; the
probe's existing source clock can execute before the lazily acquired lock.
The `guard_return_wait` record retains marked count, valid sample count,
nanosecond sum, maximum and clock-failure count. The harness requires exactly
N × K marks and N × ceil(K/64) valid samples plus clock failures, and prints
the sample mean and raw maximum beside latency percentiles; when all samples
fail, the mean field is empty. Event counts are normalized per successful
checkout, while the sampled nanosecond sum stays raw. These sample
clock reads are separate from the fairness clock-read counter. A sampled
mean is descriptive, not an estimate with a guaranteed error bound.
Diagnostic timings judge nothing; uninstrumented arms alone determine
acceptance. Compare matching diagnostic and timing cells to disclose
instrumentation perturbation before attributing costs.
The final diagnostic integrity check requires all nine arm/N cells to
complete the same work as their matching pass-1 timing cells; missing,
duplicate or unequal checksums fail independently of the research verdict.
The new per-watch test observer is separately guarded by
`WF_GUARD_TEST_HOOKS`, enabled only in native test builds, so counter-free
production builds retain neither its call nor counter bookkeeping.

All builds and runs use CI. The temporary workflow stores raw stdout/stderr,
combined CSV, original and diagnostic LLVM, construction metadata and the paired verdict in
the `guard-fairness-evidence` artifact. It is removed before branch readiness.
Its hosted checks retain all five intended negative-control failures and
run both the expanded handoff case and the private ordered-queue fixture
under normal compilation and ThreadSanitizer.

```sh
gh workflow run tmp-guard-fairness.yml --repo Ming-Research/Whitefoot --ref claude/guard-fairness -f path_diagnostics=true
```

Omit the input (or set it false) for the original timing-only experiment.
Plan approximately three minutes on the 14900K for the optional run,
excluding the hosted build and queue; this is a planning estimate, not a
measurement of the new diagnostic instrumentation.
