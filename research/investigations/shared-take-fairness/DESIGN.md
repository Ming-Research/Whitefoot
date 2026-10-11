# Shared-take progress while holding a map entry

## Purpose and scope

This directory owns the phase-1 investigation of whether a statement already
holding a map entry can be overtaken indefinitely at its later ordinary-object
acquisition. It retains the question, protocol and evidence needed to choose a
phase-2 progress mechanism. Supersede this protocol in place when that mechanism
is selected, retaining its dated results as evidence; remove the temporary
workflow and runner when the witness becomes a required runtime regression.
The native witness belongs under `compiler/src/backend/completion/` so it can
eventually join runtime checks without a dependency on research inputs.

This phase changes neither the specification nor runtime admission, scheduling,
handoff or borrowing. No phase-2 option below has been selected. No measurements
were available when this protocol was written; execution is CI-only and is
performed by the owner.

## Question and semantic witness

[WAIT-2](../../../spec/kernel-spec.md) promises eventual effect for a begun
unguarded atomic statement while every context continues to reach completion
or a wait in finitely many steps. It states no numeric overtaking bound. Let
map entry E precede ordinary object O in the permitted acquisition order:

```text
X: begin an unguarded atomic statement over E and O
   acquire E; later first use of O attempts O; finish; release O and E
Y: repeatedly begin a finite unguarded atomic statement over O
   acquire O; update O; release O
```

This is a semantic schedule, not a complete Whitefoot source program. X has
one entry throughout its attempt on O. Y never needs E, and no guard changes
truth value. The runtime witness uses the compiler's actual emitted entry
points for these actions; it does not test source parsing or lowering.

At base `89ddad8866e24aacadfb42217a594ad58996cff7`,
`wf__shared_take` in `compiler/src/backend/completion/bridge.c` retries without
registering in the acquisition queue or accumulating missed admissions.
`wf__shared_acquire` does register a parked context and escalates after two
vain wakes. The former can borrow an unclaimed hold handed to a parked
context. Borrowing is necessary when that context is ready on the spinner's
own driver; stopping the driver behind its grantee would deadlock.

There is a repeatable abstract cycle: X probes while Y holds O; Y releases
and reacquires before X's next probe. Both threads take steps, each Y
statement is finite, and each cycle returns the admission state to the same
situation. The spinner's periodic host yield adds no entitlement to O. On
one Y driver with no other ready context, the ordinary acquisition's context
pass does not force X's admission on its other driver either. Repeating the
cycle indefinitely is a static argument for a missing progress mechanism,
not a claim that a finite hosted run observed an infinite execution. Host
scheduling and atomic operations are not assumed to provide lock fairness.

The related record was read from local ref `origin/claude/guard-fairness`,
resolved to `7dab57ae5442c5f1563b989f3c9dace354e2a068`, at
`research/investigations/guard-fairness/DESIGN.md`, especially "Reservation
lifetime and progress obligations" and "Related spinner progress gap".
Its timed guarded-checkout probe contained no `wf__shared_take` call and
does not measure this witness. Its latency and cost thresholds are not reused
here. The existing `shared_object_test.c` checks handed-hold borrowing and
claiming; `cancel_test.c` supplies the precedent for native threads with
runtime contexts and a test translation unit containing the bridge.

## Comparison and criterion, before measurement

The comparison uses the same instrumented runtime in an uncontended control
and contested trials. X holds one real keyed-table entry in both. In a
contested trial N Y contexts repeatedly acquire and release O through
`wf__shared_acquire` and `wf__shared_unlock`; X uses `wf__shared_take`.
Admission, release and handoff are never replaced by a test implementation.
Test-only counters report unsuccessful spinner probes without withholding
X, choosing a lock winner or changing an admission result.

Each context has its own native thread and real runtime driver/context
records, with the fixture manually performing wake commit and ready-queue
retry. This deliberately fixes placement; it tests the acquisition protocol,
not the production driver loop, stealing, coroutine lowering or scheduler
fairness. Y0 first holds O until the counter observes X's first unsuccessful
probe, then all Y loops proceed independently. The initial hold is a setup
handshake; it contributes one completion, is included in K, and prevents a
trial from accidentally measuring no contention. After this handshake no
test barrier controls which contender wins. The control has N = K = 0.

A Y completion is its protected update immediately before unlocking O,
used as the statement's linearization point. Count publication happens while
O is held, so X's snapshot while holding O cannot race a later Y completion
or miss an earlier one. It is not a count of unlock calls that have returned.
The first-failure handshake makes the baseline zero. Every Y still runs all
K statements after X succeeds, permitting an independent final work-total
check. A drain marker reports whether every available Y statement had
completed before X acquired; eventual success after drainage does not pass
an exceeded bound.

The diagnostic bound is **B(N) = 64 × N Y completions**, measured after X's
first failed probe through its successful acquisition. The factor 64 is a
preselected experimental allowance, motivated by the runtime's context-pass
interval, not a bound derived from WAIT-2 or a proposed language requirement.
X must acquire within B(N) in **every valid trial** to call this sample
bounded. One count above B(N) rejects that bound; no averaging or percentile
can erase it. The no-contention control must acquire with zero overtaking
and zero failed probes. Missing observations, a watchdog expiry, a runtime
error or malformed output invalidate evidence and are never an expected
fairness failure.

The full grid uses N = 1, 2, 8 and per-Y finite loop caps K = 256, 4,096,
65,536, five trials per cell on each host. Keep all rows, including zero
overtaking. Increasing K asks whether larger available populations of Y
statements allow longer overtaking; it is not a performance comparison.
Before expansion run two small N = 1, K = 256 pilots and retain their
elapsed times and spread only to decide whether CI can reasonably run the
full grid. A pilot over ten seconds stops expansion as inconclusive. A
per-invocation watchdog protects CI, not the fairness verdict. Do not infer
fairness from elapsed time.

A finite run can reject B(N), and a growing sequence of maxima can support
the static repeatable-cycle argument. Even exhausting every tested K with X
still pending proves only finite overtaking. Conversely, all trials within
B(N) establish only the stated sampled bound and leave the unbounded
schedule argument open. A future claim of bounded waiting needs a mechanism
argument covering all schedules and both borrowers and parked contexts.

Counts include intervals when the host deschedules X. The aggregate failed
probe count establishes that X attempted acquisition, not that it probed
every free interval; a high overtaking count alone cannot distinguish repeated
lost races from host scheduling delay. Instrumentation can also change the
interleaving. These trials measure this instrumented acquisition protocol
under hosted schedules, with no latency or throughput claim.

## Construction, evidence and expected-failure wiring

`compiler/Makefile` provides `shared-take-witness-build` and the separately
invoked `shared-take-witness` (default arguments `1 256 1`, or set
`SHARED_TAKE_WITNESS_ARGS` to N, K and trial). Neither is a dependency of
`make check`, `completion-test` or `completion-test-images`. Compatible
native support objects reuse the ordinary runtime-test construction. The
witness translation unit includes the bridge with
`WF_SHARED_TAKE_WITNESS` defined; a failed-probe counter hook is compiled
only there. There is no bridge-header change, production hook call or change
to the lending, claiming or admission branches.

The native executable prints one CSV header and row per invocation and exits
0 for the bound passing, 2 for a measured bound violation, and 1 for invalid
execution. Its columns are `mode,n,cap,trial,bound,y_at_first_fail,`
`y_at_acquire,overtakes,failed_probes,y_total,drained_before_acquire,result`.
Omitting or corrupting the first-failure observation prevents a contested
trial from producing valid evidence; an incorrect subtraction, total,
verdict or exit code fails the caller's independent count checks.

The dispatch-only `.github/workflows/tmp-shared-take-witness.yml` runs on
`ubuntu-24.04` and `macos-15`. It builds only this image, runs the evidence
reader's boundary and invalid-input controls, then invokes this directory's
`run.py` for the control, pilots and full grid. The runner retains stdout,
stderr, raw child exit codes, per-invocation arguments, pilot elapsed-time
spread, aggregate counts and a verdict; the workflow also uploads the exact
git revision, tool versions and build log. This Python caller is for
subprocess deadlines and structured evidence handling, not an alternative
implementation of the runtime's lock protocol.

**Expected-failure means only exit 2 with a valid row above B(N).** The
runner records such a result as XFAIL and succeeds so the investigation
does not break the gate. An all-pass sample is reported as sampled pass,
not forced to fail because starvation was suspected. Any crash, timeout,
missing row, wrong count or nonzero control result fails the workflow.
`run.py --self-test` includes the exact boundary (64 passes; 65 exceeds for
N = 1), reversed exit codes, runtime errors, missing/duplicate rows, absent
probe observations and corrupted counts. These controls are wired into CI;
none was executed locally. A phase-2 caller can use `--require-bound` to
fail on any count violation, before promoting the native witness to required
runtime ownership with an approved bound and removing this temporary sweep.

The two `run-check.pl` stage labels have provisional Linux/macOS ceilings
in `.github/time-budgets.txt`. These are initial CI resource limits, explicitly
unmeasured, and neither change the count criterion nor classify watchdogs
as fairness evidence. Revisit them using the first hosted sample; this phase
does not raise an existing budget.

## Four progress shapes

| Shape | Phase-1 evidence and limit |
| --- | --- |
| X holds entry E and spins on O while unguarded Y statements reuse O | Primary native witness, with real entry ownership, successful Y completions and failed X probes. |
| A grouped atomic spins on its first ordinary object despite having no earlier hold | Static only: `wf__atomic_group_take` in `keyed_table.c` calls `wf__shared_take` for every ordinary member, including the first. No grouped-atomic trial in this phase. |
| One driver runs a chain of grouped statements without passing to another ready coroutine | Static only: the group path bypasses `wf__shared_acquire`'s context pass; yielding the host thread does not resume another coroutine. The primary witness does not test coroutine scheduling or a single-driver group loop. |
| X holds an entry needed by a reserved guard and then needs that guard's object | Design constraint only: this base lacks the other branch's guard reservations, and the witness has no guards. A later fix must validate this cycle against the reservation implementation. |

## Phase-2 alternatives, not implementations

All alternatives must retain the distinction between a **physical handed
hold** (temporarily borrowable until its grantee claims it) and a **guard
reservation** (permission for a retry, which may first need earlier locks).
None may make an entry holder await a reserved guard that needs that entry.
Keep the existing single-borrower rule, return the hold on unlock, and honor
the grantee's claim so at most the borrower already underway precedes it.
Each option also needs a progress argument between classes; perpetual
priority for new exempt holders can starve guards or parked waiters.

| Option | Borrowing and dependency treatment | Cost, risk and evidence needed |
| --- | --- | --- |
| Ticket or ordered registration for nonparking spinners | Register X's outstanding attempt and order eligible spinners. An unclaimed handed hold remains borrowable; a borrow must discharge the borrower's admission without consuming the grantee's entitlement. Do not put X behind a reservation owner waiting for E. | Admission metadata must be shared with ordinary newcomers, otherwise Y can still bypass X. Ordering behind a grantee unable to run on X's driver recreates the existing deadlock. Requires an explicit arbitration/progress rule between tickets, claims and guard reservations. |
| Join handoff arbitration after T unsuccessful probes or overtaking completions | Keep X running with E held; join an admission queue, not a coroutine park. Preserve lending and claimed-hold precedence. A reserved guard blocked on E cannot occupy an unborrowable head position ahead of X. | Count-based T avoids a wall-time fairness criterion; probe counts and completions have different scheduling meanings. A queue storing only parked contexts cannot represent this without a contract change. Test escalation, retirement on borrowing, and head-of-line dependencies. |
| One admission arbiter with explicit earlier-hold obligations | Represent parked waiters, spinners and guard reservations in one protocol; distinguish runnable acquisition from permission to retry. Temporarily admit necessary earlier-lock holders while preserving the blocked owner's place. Physical lending remains a separate ownership operation under that protocol. | Could give one progress argument across entry points, but merely labelling all spinners exempt provides no bound. Needs dependency information or a finite-priority argument, including chains of earlier objects and map entries; greater interface and proof scope than adding a counter. |
| Make first acquisition resumable, retaining nonparking later acquisitions | Route a group's first acquisition with no earlier hold through the ordinary acquisition and context-pass path. Retain borrowed-hold support and a fair nonparking protocol for X after it takes E; preserve the reservation exemption only where an earlier hold requires it. | Addresses the two grouped/single-driver shapes, but alone leaves the primary entry-holder starvation gap. Requires lowering/group API work and separate source-to-runtime evidence; it complements rather than replaces a spinner fix. |

Releasing E and retrying the statement is not a drop-in alternative: the
block may already have moved values or changed locals before its lazy use
of O, so replay changes the once-executed block contract. Moving all locks
to statement entry changes the recorded lazy-locking decision and needs its
own investigation. Neither is implemented or silently used by this witness.

## Results and remaining evidence

**First hosted run, 2026-10-11** (run 38154989622, revision `9e5e31f72`, the
witness as committed in `2971c435e`; raw rows in the run's artifacts). Five
trials per row, N competitors, cap = each competitor's statement count;
"overtakes" = competitor completions on O between X's first failed probe
and X's acquisition; "drained" = X acquired O only after every competitor
had finished all its capped statements.

| Host | Valid trials | Over B(N) = 64 x N | Drained |
| --- | ---: | ---: | ---: |
| ubuntu-24.04, x86-64, clang 18.1.3 | 47 | 1 | 0 |
| macos-15, arm64, Apple clang 17.0.0 | 47 | 43 | 18 |

| N, cap | Linux max overtakes | macOS min-max overtakes |
| --- | ---: | ---: |
| 1, 256 | 5 | 256-256 |
| 1, 4096 | 5 | 92-4096 |
| 1, 65536 | 5 | 27-65536 |
| 2, 65536 | 1 | 1-131072 |
| 8, 4096 | 2467 | 4096-32768 |
| 8, 65536 | 3 | 708-266982 |

The sampled bound is rejected on both hosts, so the static cycle in
[the question](#question-and-semantic-witness) is observed, not only
argued: on macOS X routinely waited until its competitors ran out of work,
and its overtaking count grew with the cap, which is the shape unbounded
overtaking predicts; on Linux it is rare but present. The uncontended
control passed on both. Finite runs cannot show that the wait is
infinite; they show that today's runtime gives X no bound related to N.
Phase 2 needs one of the alternatives above; the guard reservation it must
stay compatible with is on `claude/guard-fairness` and is not on this base.
