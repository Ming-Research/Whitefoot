# Shared-take progress while holding a map entry

## Purpose and scope

This directory owns the investigation of whether a statement already
holding a map entry can be overtaken indefinitely at its later ordinary-object
acquisition. It retains the question, protocol and evidence needed to choose a
phase-2 progress mechanism. Supersede this protocol in place when that mechanism
is selected, retaining its dated results as evidence; remove the temporary
workflow and research sweep runner when the phase-2 hosted comparison is
complete; keep the required native regression and its maintained count checker.
The native witness belongs under `compiler/src/backend/completion/` and joins
runtime checks without a dependency on research inputs.

The owner selected phase-2 option B: count-based escalation to nonparking
handoff claims. Its implementation and proposed arbitration are below. The
specification is unchanged. The phase-1 protocol preceded the first hosted
measurement; phase-2 execution is also CI-only and is performed by the owner.

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

## Phase-1 construction and expected-failure wiring

At the recorded phase-1 revision, `compiler/Makefile` provided an isolated
`shared-take-witness-build` and a separately invoked trial target. Neither
was a dependency of `make check`, `completion-test` or
`completion-test-images`. The current required wiring is described below. Compatible
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

The phase-1 dispatch-only `.github/workflows/tmp-shared-take-witness.yml` ran on
`ubuntu-24.04` and `macos-15`. It built only this image, ran the evidence
reader's boundary and invalid-input controls, then invoked this directory's
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
none was executed locally. Phase 2 uses `--require-bound` to fail on any
count violation and promotes the native witness to required runtime ownership,
while retaining the temporary sweep for the hosted comparison.

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

## Phase-2 alternatives

The owner chose option B below, escalation to handoff arbitration. Option A
was declined because tickets approach strict turns: the guarded-wait record
measured firn engine throughput at 1/7.5–1/47 of control with strict turns,
and putting a spinner behind a grantee unable to run on its driver recreates
the borrowing deadlock. Those are selection grounds, not a measurement of
this spinner-ticket alternative. Option C was declined because a single
arbiter has the widest interface and proof scope and is premature while the
guard-fairness cost sources are still being separated. First-acquisition
resumption remains separate work.

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

## Phase-2 claim arbitration

The proposed implementation uses **K = 1 failed admission confirmed under O's
lock**. An optimistic observation is not the registration point. On the first
confirmed miss, X links its stack-resident claim into O's active-claim queue
in the same critical section. It keeps E and its physical executor, and
polls its claim; it never parks its coroutine. An uncontended take and an
immediately available borrowed hold need no queue entry. K greater than one
failed probe would leave an interval in which a preempted X has no entitlement
and arbitrarily many Y completions could precede the next probe. K = 1 closes
that interval at the confirmed miss, leaving room within the unchanged 64 × N
witness criterion. This is a provisional implementation choice, not an
extension of WAIT-2.

The active queue is FIFO among escalated claims, rather than tickets for all
acquisitions. A queued claimant has no earlier object pending acquisition:
it already holds every earlier target it needs. A claim is an entitlement
to a physical hold, separate from both a parked context's wake and a guard's
reservation to retry. Ordinary acquisitions, including compatible new readers,
are refused while claims wait; current readers drain. On the last physical
release:

1. Give the parked head its existing opportunity, preserving two vain wakes
   before a physical handoff. If the opportunity only wakes it to retry,
   hand the free physical hold directly to the oldest active claim.
2. If the parked opportunity hands a physical hold to a context, retain that
   context's ownership. Mark that it has had its opportunity while active
   claims wait. The next real free release must hand directly to the oldest
   active claim rather than grant another parked hold first.
3. Reset the opportunity marker on an active-claim admission, including
   borrowing. Thus each class gets progress: continuous active claims offer
   the parked queue retries and its bounded handoff, while continuous due
   parked handoffs cannot indefinitely precede an active claim.

Until a handed grantee claims its hold, the oldest active claim may borrow
that hold exclusively. With no pending claim an immediate take may borrow
as before. No later claimant may bypass an older one to borrow. Borrowing
consumes the borrower's active claim and returns the original physical hold
to its original grantee on unlock. Once the grantee claims, only a borrower
already underway may finish before it; that precedence is unchanged. A
borrowed release is not a real free release and does not award the original
hold to another grantee.

There is no reserved guard in this base. The queue and service helpers depend
only on physical ownership and active claims, not guard selection, so a claim
does not wait behind a logical guard reservation. When integrating with the
guard-fairness branch, keep active service ahead of reservation-only admission
and preserve an exempt claimant's borrowing; the reserved guard may need E
before it can retry O. A **physical** handed grantee is different: its actual
hold was granted at its acquisition boundary, after any preceding locks it
needs, and it can finish without awaiting an earlier lock held by X. Queuing
an entry holder behind a reserved guard, or replacing a borrowed grant with
a new unborrowable parked-owner entitlement, would reintroduce the deadlock.

### Progress argument and scope

At registration, only finitely many active claims precede X. New claims join
behind it; ordinary newcomers cannot barge. Each earlier claimant gets at
most one intervening parked physical handoff before its own admission. A
parked handoff retains a finite running borrow if one is underway, and once
its grantee runs its claim excludes subsequent borrowing. A claim either
receives a free hold or, at the queue head, borrows the existing unclaimed
hold. Removing the claim before release-publishing its grant prevents a
stack record from surviving its caller's return. Every earlier active owner
and every claimed grantee obeys the total target order, so retained earlier
entries cannot create a lock cycle with later targets. The queue never
requires a claimant to suspend the driver that must run a parked grantee.

Let q count active claims ahead of X and h count the statements already
entitled to complete on O at registration: current readers or a writer,
including both an underway borrower and its already-handed grantee when
borrowing is in progress. A conservative completion count from registration
through X's admission is h + 2q + 1: h initial completions, q earlier active
claim completions, and at most q + 1 newly handed parked completions. An
already-handed grantee belongs to h, not to the newly handed term. With at
most N competing requests already before X, h + q ≤ N, so this is at most
2N + 1, leaving room under 64N for N ≥ 1. In the primary
write-only witness there is no earlier active claim or initial parked grant:
Y0 holds O until X's confirmed miss and registration, then its one release
hands to X, so only one Y completion precedes X. The broader argument counts
completions on O, not arbitrary code work or elapsed time, and is conditional on
holders and ready grantees getting steps and finishing their blocks.

There is no finite completion bound before X obtains O's internal spin lock
to register: host preemption and that primitive's scheduling are separate
conditions. The witness's initial holder remains held until the confirmed
miss, so this limitation cannot hide a counted pre-registration interval
in its 64N verdict. Counters and deterministic schedules remain test-only;
production admission never depends on a test mode, N, cap or observation.

The same claim applies to a grouped atomic's contended first ordinary object,
because `wf__atomic_group_take` calls `wf__shared_take`. It supplies contention
progress there without changing `keyed_table.c`. It does **not** make an
uncontended grouped loop pass to another ready coroutine on one driver, nor
distinguish a first-object newcomer from an earlier-lock holder for the other
branch's guard reservations. Those two entry-path/scheduling gaps remain
separate; this change does not claim to close them.

### Representation and integration limits

The parked queue becomes a circular tail queue with the same head-retry and
tail-newcomer order. Its saved pointer slot holds the active-claim tail, and
the arbitration marker occupies header padding, preserving the 64-byte
shared-state offset and the public header ABI. Claim records live on the
native stack of nonparking callers, including native test holders outside a
coroutine context. Ownership of the hold is published with release/acquire
ordering before the record may disappear.

The guard-fairness branch independently used the same saved header slot for
its reservation pointer. A later combination must reconcile that layout and
validate both protocols together; this phase does not claim tested binary
or execution compatibility with an unmerged implementation. No `bridge.h`
change is needed here. The grouped scheduling gap and the combined layout
are recorded integration work, rather than hidden by a fallback.

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

The sampled 64 x N bound was rejected on both hosts. On macOS the
counts grew with the cap and X often waited for the competitors to drain;
Linux also produced one violation. These observations support the static
starvation argument, but do not establish an infinite execution, repeated
losses at each available gap, or the absence of every possible N-dependent
bound: host scheduling also contributes to the observed interval. The
uncontended control passed on both. These are the owner's recorded results;
this phase has not retrieved the raw artifacts or run a new measurement.

## Required regression and pending phase-2 evidence

`compiler/Makefile` now includes `shared-take-witness` in `completion-test`
and both candidate and disabled-escalation images in `completion-test-images`.
The maintained `compiler/src/backend/completion/shared_take_check.py` runs
with `--require-bound`, validates raw counts and child exits independently,
and rejects missing evidence or runtime errors. The research sweep imports
that validator; the gate has no research dependency.

The fixed gate inventory contains:

- An uncontended control, which must acquire without a failed probe.
- A scheduled N=1, cap=65 candidate trial, which must acquire after exactly
  one completion. The same fixture pauses X outside the object lock at the
  first failed-probe hook until its claim is granted or 65 Y completions
  finish. Thus the disabled-escalation image must report exactly 65 overtakes
  and exit 2; a crash, timeout or passing mutant is an error. This deliberately
  models host preemption after the failed probe, without modifying production
  admission or release. The phase-1 runtime lacks the claim and fails it.
- Two natural trials each at N=1,2,8 and cap=256, all requiring <=64 x N.
- Native arbitration fixtures checking writer FIFO, initial-reader drain,
  compatible-reader admission veto, alternating parked and spinner handoffs,
  the handed grantee's precedence, and head-only queued borrowing. The prior
  runtime has no claim queue and cannot satisfy these assertions. Handoff
  counters are primed in these narrow fixtures; existing shared-object tests
  retain real parked-wake and repeated borrowing coverage.
- Validator sensitivity cases for the inclusive 64 boundary, 65 violation,
  false child verdicts, runtime errors, missing/duplicate rows, incorrect raw
  subtraction, incomplete Y work and the negative control's intended count.

The dispatch-only workflow retains the repeated sweep on ubuntu-24.04 and
macos-15, now also requiring the bound and running the fixed gate inventory.
It uploads raw counts, exits and construction logs even on failure. No new
`run-check.pl` label is introduced, so existing stage budget rows still apply.
Remove this temporary workflow and the sweep driver when the hosted phase-2
comparison is complete; keep the native regression and maintained checker.

Native compilation, execution, sanitizer results and phase-2 counts are
unverified until the owner runs CI. There is no local runtime measurement.
