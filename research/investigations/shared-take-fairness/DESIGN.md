# Shared-take progress while holding a map entry

## Purpose and scope

This directory owns the investigation of whether a statement already
holding a map entry, or an earlier object, can be overtaken indefinitely at
its later ordinary-object acquisition, and the claim mechanism that bounds
it. It retains the question, protocol and evidence that chose that
mechanism. Supersede this protocol in place as the mechanism changes,
retaining dated results as evidence. The three temporary workflows (the
hosted count sweep, the mutant run and the 14900K cost comparison) and the
sweep driver `run.py` are retired; they are cited at revision `f4299910e`
and their runs are the evidence under
[Results and remaining evidence](#results-and-remaining-evidence). The
required native regression, its maintained count checker and the cost
comparison's bundle (`research/experiments/shared-take-overhead/`) remain as
the record of the procedure. The native witness belongs under `compiler/src/backend/completion/`
and joins runtime checks without a dependency on research inputs.

The owner selected option B: escalation to nonparking handoff claims after
K = 1 confirmed miss. Phase 2 first implemented it on an old main
(`d8bece19a`, branch `claude/shared-take-fair`), whose header slot for the
claim tail collided with the guard-fairness branch's reservation pointer.
The current implementation, below, ports it onto the guard-fairness
implementation (Whitefoot PR #377, base `96520e70e` on branch
`claude/guard-fairness`) and reconciles the two protocols. The
specification is unchanged; the owner ruled that none is needed. Every
build, test and measurement here is CI-only and performed by the owner.

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

The phase-1 dispatch-only `.github/workflows/tmp-shared-take-witness.yml`
(retired; read at revision `f4299910e`) ran on
`ubuntu-24.04` and `macos-15`. It built only this image, ran the evidence
reader's boundary and invalid-input controls, then invoked this directory's
`run.py` (retired, same revision) for the control, pilots and full grid. The runner retains stdout,
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
`run.py --self-test` included the exact boundary (64 passes; 65 exceeds for
N = 1), reversed exit codes, runtime errors, missing/duplicate rows, absent
probe observations and corrupted counts; the workflow ran it in CI and none
was executed locally. Phase 2 used `--require-bound` to fail on any
count violation and promoted the native witness to required runtime ownership,
keeping the temporary sweep for the hosted comparison.

The two `run-check.pl` stage labels, `shared-take-witness-build` and
`shared-take-witness-counts`, had provisional ceilings in `.github/time-budgets.txt`
(the build row `60 90 -`, the counts row `180 180 -`), initial CI resource
limits and not measured budgets; they changed neither the count criterion nor
the classification of watchdogs as fairness evidence. The rows are retired
with the workflow. In the claim-arbitration sweep the stages took 8.71 s and
6.52 s on `ubuntu-24.04` (against a read Linux ceiling of 90 s and 180 s)
and 2.53 s and 9.72 s on `macos-15`; the macOS job's budget verdict still
failed, because the three-column rows did not match the budget table's
current columns, which name Linux CPU classes, macOS and Windows ("needs
seconds or - for macos"). That failure was a malformed table and not an
overrun.

## Four progress shapes

| Shape | Evidence and limit |
| --- | --- |
| X holds entry E and spins on O while unguarded Y statements reuse O | Primary native witness, with real entry ownership, successful Y completions and failed X probes. |
| A grouped atomic spins on its first ordinary object despite having no earlier hold | Addressed by the guard-fairness base, not by claims: `wf__atomic_group_acquire` in `keyed_table.c` acquires a statement's first group's first ordinary object through `wf__shared_acquire`, which parks; later members and every later group use `wf__shared_take` as earlier-lock holders, and claims now cover them. A first group whose first target is a map's entries takes them without parking, as before. Not separately witnessed here. |
| One driver runs a chain of grouped statements without passing to another ready coroutine | Addressed for object-first groups by the same first-group acquire, which passes the driver as every `wf__shared_acquire` does; a map-first group still bypasses the pass. The primary witness does not test coroutine scheduling or a single-driver group loop. |
| X holds an entry needed by a reserved guard and then needs that guard's object | Integrated with the guard reservations below. The `reservation` arbitration fixture installs a reservation for a context that has not retried, as a notification installs it, and requires the next free unlock to grant X's claim; it does not run a real guard whose retry blocks on E. |

## Alternatives

The owner chose option B below, escalation to handoff arbitration. Option A
was declined because tickets approach strict turns: the guarded-wait record
measured firn engine throughput at 1/7.5–1/47 of control with strict turns,
and putting a spinner behind a grantee unable to run on its driver recreates
the borrowing deadlock. Those are selection grounds, not a measurement of
this spinner-ticket alternative. Option C was declined because a single
arbiter would replace the guard reservation's arbitration, which was measured
and approved on its own, and would have to prove all three kinds of waiter
together; claims beside it need one added rule, that a reservation never
delays a claim. First-acquisition
resumption arrived separately, as the guard-fairness base's first-group
acquire (see the progress shapes above).

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

## Claim arbitration

The implementation uses **K = 1 failed admission confirmed under O's lock**.
An optimistic observation is not the registration point: until its claim is
queued, every look of `wf__shared_take` takes the lock. On the first
confirmed miss, X links its stack-resident claim into O's claim queue in the
same critical section. It keeps E and its physical executor, and polls its
claim; it never parks its coroutine. An uncontended take and an immediately
available borrowed hold, with no claim waiting, need no queue entry. K
greater than one would leave an interval in which a preempted X has no
entitlement and arbitrarily many Y completions could precede the next
probe; K = 1 closes that interval at the confirmed miss, leaving room within
the unchanged 64 × N witness criterion. K is a provisional implementation
choice, not an extension of WAIT-2, and no workload has compared a larger
value.

The queue is FIFO among escalated claims, rather than tickets for all
acquisitions. A claimant has no earlier object pending acquisition: it
already holds every earlier target it needs. A claim is an entitlement to a
physical hold, separate from both a parked context's wake and a guard's
reservation to retry. While any claim waits, every other acquisition of O is
refused: a parking acquisition (`wf__shared_acquire`), including a
compatible reader beside current readers, the reservation's owner and the
cancellation-state update; and a newcomer take, which queues its own claim
behind the others instead of holding or borrowing. Current readers drain.
On the last physical release (`wf_shared_free_locked`):

1. If a parked statement was handed O while claims waited (the object's
   `take_turn` marker), grant the oldest claim and wake no one.
2. Otherwise give the parked head its existing opportunity, preserving two
   vain wakes before a physical handoff (`wf_shared_wake_locked`, which under
   a live reservation picks the first parked statement the reservation
   admits, and none when it admits none). If that leaves O free, grant the
   oldest claim; if it handed O to a parked statement while claims wait, set
   the marker.
3. Every claim admission, borrowing included, clears the marker. Thus each
   class gets progress: continuous claims still give the parked queue its
   retries and its bounded handoff, while continuous due parked handoffs
   cannot indefinitely precede a claim.

Readers among the claims are granted one at a time, each when O is free; a
reader claim does not admit the reader claims behind it. That serializes
them, which is correct and conservative.

Until a handed grantee claims its hold, the oldest claim may borrow that
hold exclusively, and a take may borrow it at once only when no claim waits.
No later claimant may bypass an older one to borrow. Borrowing consumes the
borrower's claim and returns the original physical hold to its original
grantee on unlock. Once the grantee claims, only a borrower already under
way may finish before it; that precedence is unchanged. A borrowed release
is not a real free release and grants nothing.

A grant publishes `granted` with release ordering after the claim has left
the queue, so the record may disappear the moment its owner sees it. The
owner then retakes O's lock once to count its hold (`wf_shared_took_locked`),
since the giver cannot write the running context's hold count; this costs
one lock round trip on the contended path only.

### Integration with guard reservations

The guard-fairness base adds a reservation (the context a write's
notification went to; refusing newcomers that hold nothing while it is
older than 1 ms), notify-the-oldest-satisfied-waiter, rescans, and
read-only releases that wake nobody. Its rules already exempted
`wf__shared_take` from the reservation, because the reserved retry may need
an entry or earlier object the taker holds. The claims keep and extend that
exemption:

- **A claim never queues behind a reservation.** Claim service depends only
  on physical ownership and on the queue. When the parked queue holds only
  statements the reservation refuses, the wake chooses none and the free
  object goes to the oldest claim. A reservation whose owner needs the entry
  X holds therefore cannot stall X.
- **A claim refuses the reservation's owner.** The owner refused by a claim
  holds nothing the claims need (a claimant already holds every earlier
  target, and the owner tries O only after its own earlier targets), so it
  parks, freeing its driver; the reservation's wakes reach it first, and it
  is handed O after at most `WF_SHARED_HANDOFF` vain wakes, as without
  claims. An owner that itself holds an earlier target takes O through
  `wf__shared_take` and is an ordinary claimant.
- **A grant consumes no notification.** It happens inside the releasing
  statement's `wf_shared_end_hold_locked`, after the releaser's own
  reservation has passed on (`wf_guard_release_locked`) and before its
  write's sweep. The notified watch therefore retries after the claimant's
  hold, and is refused or parks like any other statement. The claimant's
  own release runs the ordinary path: a write sweeps when no reservation is
  live, and otherwise marks the live owner's rescan; a claimant that owns
  O's reservation (a notified retry holding an earlier target) releases it
  at its own unlock, as every owner does. A reservation installed while a
  claimant holds O still refuses newcomers, since a grant is not a handed
  parked hold (`wf_wake_reserves` checks only `granted`).
- **Borrowing ignores the reservation**, as before: lending depends only on
  the handed hold.

There remains no reserved-guard deadlock with entries: the reserved retry
needs E before O, X holds E and needs only O, and nothing in O's admission
makes X wait for the retry.

### Progress argument and scope

At registration, only finitely many claims precede X. New claims join
behind it; ordinary newcomers cannot barge. Each earlier claimant gets at
most one intervening parked physical handoff before its own admission. A
parked handoff retains a finite running borrow if one is under way, and once
its grantee runs its claim excludes subsequent borrowing. A claim either
receives a free hold or, at the queue head, borrows the existing unclaimed
hold. Removing the claim before release-publishing its grant prevents a
stack record from surviving its caller's return. Every earlier claimant and
every claimed grantee obeys the total target order, so retained earlier
entries cannot create a lock cycle with later targets. The queue never
requires a claimant to suspend the driver that must run a parked grantee.

Let q count claims ahead of X and h count the statements already entitled
to complete on O at registration: current readers or a writer, including
both an under-way borrower and its already-handed grantee when borrowing is
in progress. A conservative completion count from registration through X's
admission is h + 2q + 1: h initial completions, q earlier claim
completions, and at most q + 1 newly handed parked completions. An
already-handed grantee belongs to h, not to the newly handed term. With at
most N competing requests already before X, h + q ≤ N, so this is at most
2N + 1, leaving room under 64N for N ≥ 1. In the primary write-only witness
there is no earlier claim or initial parked grant: Y0 holds O until X's
confirmed miss and registration, then its one release grants X, so only one
Y completion precedes X. The argument counts completions on O, not code
work or elapsed time, and is conditional on holders and ready grantees
getting steps and finishing their blocks. Vain wakes of parked statements
are not completions; between a parked statement's wake and its retry any
number of claims may be granted, as any number of newcomers could overtake
it before, and its two-vain-wake handoff still bounds it.

There is no finite completion bound before X obtains O's internal spin lock
to register: host preemption and that primitive's scheduling are separate
conditions. The witness's initial holder remains held until the confirmed
miss, so this limitation cannot hide a counted pre-registration interval in
its 64N verdict. Counters and deterministic schedules remain test-only;
production admission never depends on a test mode, N, cap or observation.

### Representation

The guard-fairness base fills the 64-byte header before
`WF_SHARED_STATE_OFFSET`: handle count (8), lock, `borrowed`, `claimed` and
one padding byte, the watch count (4), holder count (8), reservation (8),
the parked queue's tail (8, the queue being circular for this reason),
watch list (8), handed context (8) and pool size (8). The phase-2 commit
had freed the same slot from the parked queue's head for its claim tail, so
the two cannot both have a slot of their own without growing the header,
which would move every object's state and the cancellation state's address
that emitted code and `wf_cancel` depend on.

The chosen layout keeps the header at 64 bytes and the public header ABI
unchanged:

- The reservation slot becomes `arbiter`. With its low bit clear it is the
  reservation (or none); with it set it is the tail of the claim queue, and
  the tail claim record keeps the reservation. Linking a new tail moves the
  reservation into it; granting the last claim moves it back to the slot.
  Contexts and claim records are at least 8-byte aligned, which a static
  assertion checks. Every access goes through `wf_shared_reservation`,
  `wf_shared_set_reservation`, `wf_shared_claims` and `wf_shared_set_claims`,
  under the object's lock; no hint reads the slot outside it. An ordinary
  admission reads one slot for both the claim and turn checks.
- The claim/parked turn marker (`take_turn`) occupies the padding byte.
- Claim records live on the native stack of nonparking callers, including
  native test holders outside a coroutine context; the queue is circular
  through their `next` fields.

Rejected layouts: growing the header (moves the state address the emitted
code and the cancellation state rely on); a global side table of claim
queues keyed by object (a second lock and a hash on every contended release,
and a global structure for per-object state); narrowing the handle or
holder count to make room (changes the overflow behavior of a count every
statement updates); indexing claims by physical executor in a byte (the
claim tail fits in the padding byte only with at most 255 executors, and
native test threads and displaced invocations would need unique executor
indices, an implicit new contract on every take caller).

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

### Claim-arbitration design, revision `f4299910e`

The three runs below measured the claim-arbitration implementation at
revision `f4299910e`. The workflows that produced them are retired and read
at that revision: `.github/workflows/tmp-shared-take-witness.yml`,
`.github/workflows/tmp-shared-take-mutants.yml`,
`.github/workflows/tmp-shared-take-overhead.yml`, the sweep driver
`research/investigations/shared-take-fairness/run.py` and the two stage rows
in `.github/time-budgets.txt`.

**Hosted count sweep** (run 38382784632, 2026-10-13; the workflow built the
witness with `make -C compiler shared-take-witness-build`, ran the verdict
controls `run.py --self-test`, the fixed gate inventory
`make -C compiler shared-take-witness`, then `run.py --require-bound` for
the control, two N = 1, cap 256 pilots and the full grid of five trials per
cell). `ubuntu-24.04` (x86-64, clang 18.1.3, Python 3.12.3) and `macos-15`
(arm64, Apple clang 17.0.0, Python 3.14.7) each ran 48 trials: the control,
two pilots and 45 grid trials.

| N | B(N) = 64 x N | Trials per host | Max overtakes, Linux | Max overtakes, macOS | Over B(N) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 64 | 17 (2 pilots, 15 grid) | 1 | 1 | 0 |
| 2 | 128 | 15 | 1 | 1 | 0 |
| 8 | 512 | 15 | 1 | 1 | 0 |
| control | 0 | 1 | 0 | 0 | 0 |

Every trial had one overtake, against 43 of 47 macOS trials above the bound
in the first hosted run of the old design. Failed probes ranged over 20-256
on Linux and 256-1,969 on macOS, so X attempted acquisition repeatedly in
every contested trial on both hosts. Both hosts printed the verdict
"SAMPLED PASS: every valid trial met B(N)=64*N; unbounded schedules remain
open." The fixed gate inventory also passed on both: the four arbitration
cases, the scheduled N = 1, cap 65 trial (1 overtake against bound 64) and
the disabled-escalation image, which gave 65 overtakes against bound 64 and
was reported `bound_exceeded` as the negative control requires. The
witness stages exited 0 on both hosts. The macOS job was marked failed only
by its budget verdict (a malformed budget table, described under Phase-1
construction), not by any count or exit. Against the registered criterion,
parts 1 and 2 hold on the sampled schedules.

**Mutant run** (run 38378677089, push to the branch at `f4299910e`, success):
5 controls passed unmutated (the four arbitration cases and the scheduled
N = 1, cap 65 trial) and 14 mutants ran, 15 case checks in all (the no-claim
mutant is checked by two cases); each case failed with its own named message
and none failed for another reason. The table in
[Required regression and pending evidence](#required-regression-and-pending-evidence)
lists each mutant, its case and message.

**Uncontended cost on the 14900K** (run 38381067726, mode `main`, 6 blocks,
base `5dd90db14` (main at the merge of the guard-fairness work) against
candidate `f4299910e`; Intel Core i9-14900K, Linux 7.0.0, governor
`performance`, clang 22.1.8). The run's dispatch input named base
`5dd90db14` and not the base `96520e70e` registered in part 3; the
`compiler/` and `lib/` difference between that base and the candidate is
five files (`compiler/Makefile`, `bridge.c`, `cancel_test.c`,
`shared_take_check.py`, `shared_take_witness.c`), the claims' change alone.
Both programs' emitted LLVM was identical between base and candidate, so the
difference is in the runtime. Each candidate-to-base time ratio t and
twin-to-base ratio w (registered limits: t at most 1.02, w within
0.99-1.01) was:

| Program | Drivers | t | Overhead | w | ns per statement (base) | Reading |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| take | 1 | 1.0004 | +0.04% | 1.0000 | 44.77 | pass |
| take | 2 | 1.0019 | +0.19% | 0.9990 | 45.67 | pass |
| take | 4 | 1.0046 | +0.46% | 1.0003 | 45.29 | pass |
| take | 8 | 1.0024 | +0.24% | 1.0018 | 45.28 | pass |
| own | 1 | 1.0073 | +0.73% | 1.0030 | 21.70 | pass |
| own | 2 | 1.0077 | +0.77% | 0.9999 | 21.68 | pass |
| own | 4 | 1.0077 | +0.77% | 1.0000 | 21.67 | pass |
| own | 8 | 1.0082 | +0.82% | 0.9998 | 21.74 | pass |

The registered verdict, the take program at one driver, is pass: +0.04%
against the 2% limit, with the twin at 1.0000. The every-cell extension is
pass: all eight cells are within 0.82% of the base and every twin ratio lies
within 0.99-1.01, so no cell is inconclusive. Part 3 holds for this
compiler pair, one machine and one workload shape; the scope limits of the
[Uncontended-cost procedure](#uncontended-cost-procedure) apply.

**Still open.** All trials within B(N) establish only the stated sampled
bound; the argument for every schedule, both borrowers and parked contexts,
remains the static progress argument (see Progress argument and scope), so
unbounded schedules remain open. The sweep measured the protocol under hosted
schedules and not latency or throughput. The contended path, the claim
queue's cost when claims wait and guard reservations are unmeasured.
Sanitizer results for the concurrent map and the runtime come from the
gate's `map-sanitizers.yml` and runtime jobs and not from these runs.

## Registered criterion

Registered before the integrated implementation's first hosted or 14900K
run. All three parts must hold; any one failing rejects the integrated
implementation as it stands.

1. **Bound.** On `ubuntu-24.04` and `macos-15`, the dispatch-only sweep
   (`tmp-shared-take-witness.yml`, `run.py --require-bound`, both retired and
   cited at revision `f4299910e`) runs the
   uncontended control, two N = 1, cap 256 pilots and the full grid of
   N = 1, 2, 8 by cap 256, 4,096, 65,536, five trials per cell. Every valid
   trial must have at most 64 × N overtakes. One count above 64 × N rejects;
   no averaging or percentile can erase it. Invalid runs (watchdog, crash,
   malformed rows) are reported as invalid, never as a pass. The comparison
   is the first hosted run above, where 43 of 47 macOS trials exceeded the
   bound.
2. **Negative control.** The disabled-escalation build
   (`-DWF_SHARED_TAKE_TEST_NO_ESCALATION`) in the scheduled N = 1, cap 65
   trial must report exactly 65 overtakes and exit 2 (bound exceeded); the
   claim build in the same trial must report exactly 1. Any other count,
   exit or a passing negative control rejects, because then the fixture
   would not separate a missing claim from a working one.
3. **Uncontended cost, provisional.** On the 14900K, through CI, an
   interleaved same-source comparison of base `96520e70e` and this branch's
   head, with a byte-identical twin of the base as noise control, on a
   single-driver program whose atomic statements each hold one map entry and
   then take one ordinary object, so every take is admitted at its first
   look. The head's median run time must be at most 1.02 times the base's,
   with the twin within 0.99 to 1.01 of the base; a twin outside that range
   makes the run inconclusive, not a pass. The program and runner are
   recorded under [Uncontended-cost procedure](#uncontended-cost-procedure),
   written before any run, without changing these thresholds. The 2 percent
   threshold is the owner's provisional choice, not a derived cost bound,
   and is reopened if a workload shows the uncontended path.

The arbitration fixtures and the mutant run (below) are correctness
evidence, separate from this criterion.

## Uncontended-cost procedure

Written 2026-10-12, before any run, for part 3 of the registered
criterion. The bundle is `research/experiments/shared-take-overhead/`
(`run.sh` documents its interface) and the dispatch-only workflow was
`.github/workflows/tmp-shared-take-overhead.yml` on the 14900K (retired; read
at revision `f4299910e`). The workflow checked the host's tools and that
the host was idle, ran `summarize.py --self-test`, built the whitefootc
compiler at the base and at the candidate with `cargo build --profile gate`,
built the images with `run.sh --build-only`, rechecked idleness, ran the
cells with `run.sh --run` and printed the summary; its inputs were `mode`
(`pilot` or `main`), `blocks` (default 6) and `base`.

**What the change adds to an uncontended statement.** Read from the diff
of `bridge.c` between the base and the candidate: `wf__shared_take` now
initializes a stack claim record and checks the claim slot twice under the
object's lock, and no longer reads the object's state before taking that
lock; the ordinary admission (`wf_shared_admits_context`, every `wf__shared_acquire`)
first checks the claim slot; the free release (`wf_shared_free_locked`)
checks it again; and every reservation read decodes the slot's low bit. The
uncontended take took the object's lock once before and takes it once now.
These are a few loads, stores and branches on lines the statement already
touches, so the expectation, stated here as exploratory and not as a
prediction the verdict rests on, is a candidate-to-base time ratio below
1.01.

**Revisions.** Base `96520e70e3ea9f9000887d1dcf3c453d77f05fe9`, the
registered base and the parent of the claims' first commit
(`2c92dc5fa`). The base branch has since advanced to `2b7c9e5c1`, which
changes only design-tree prose, so its compiler and standard library are
the same bytes; the registered base is kept. The candidate is the
workflow's checked-out revision, whose `compiler/` and `lib/` equal this
branch's head when the procedure was written (`70964a385`); the run
records both revisions and the `compiler/` and `lib/` diff between them.
The runtime is compiled into the compiler (`include_str!` of `bridge.c`),
so the candidate compiler links the candidate runtime into every image.

**Programs.** Two entries of one package, each built from the same source
by both compilers:

- `take` (`take.wf`), the registered program: each context makes its own
  concurrent map and its own `Shared<u64>`; each atomic statement names the
  map's one entry and then the object, so it holds the entry when it takes
  the object and finds the object free at its first look; no claim ever
  queues.
- `own` (`own.wf`): each context's statements name its own `Shared<u64>`
  alone, through the ordinary acquisition, which the change also reaches.

Each context runs statements in batches of 1,024, reading the clock once
per batch, until a deadline every context receives from one gate opened
after all are spawned; it then checks in one more statement that its entry
and object both hold its statement count, and the program exits 3 when any
context's do not. It prints the summed statement count and the elapsed
nanoseconds; the rate is their quotient. One context runs per driver.

**Images and controls.** Per program: `base` and `candidate`, and `twin`,
a byte-for-byte copy of `base` (as the guard-fairness probe's twin was)
checked with `cmp`, which runs in its own rotation slot and so measures
run-to-run and position noise. The build refuses to proceed when a
candidate image is byte-identical to its base (the change would not reach
the program) or when the take program's emitted LLVM calls neither
`wf__shared_take` nor a group acquisition (the program would not reach the
take). It records image hashes, the call-site counts of both compilers'
LLVM, and whether the two compilers emit the same LLVM.

**Cells.** Six arms (two programs by three images) on CPU lists `0`,
`0,2`, `0,2,4,6` and `0,2,4,6,8,10,12,14`, one hyperthread of each of 1, 2,
4 and 8 P-cores, with `WF_DRIVERS` 1, 2, 4 and 8. Each cell lasts 3 s.
Within a block every list runs the six arms once; each block starts the
arm order one position later, so the main run's six blocks put every arm in
every position once. The host must be under 5 percent busy over 5 s before
the compilers are built and again before the cells. A cell whose exit
status is not 0, whose driver or context count differs from the list's, or
whose objects ended wrong leaves no row and fails the run. The pilot, one
block at the 8-P-core list, decides nothing; it shows that the images run,
how long a cell takes and how far the twin strays, and the main run follows
it.

**Measure and verdict.** At each cell, over blocks, t = median rate of base
/ median rate of candidate (the candidate's time per statement over the
base's) and w = median rate of base / median rate of twin. A cell with w
outside [0.99, 1.01] is inconclusive; otherwise it passes when t <= 1.02 and
rejects when t > 1.02.

- The registered verdict is the take program's one-driver cell, exactly
  part 3 above.
- The every-cell extension, registered here, applies the same rule to all
  eight cells: it rejects when any cell rejects, passes when every cell
  passes, and is otherwise inconclusive. It covers the ordinary acquisition,
  which the claim check also enters, and shows whether the per-statement
  cost moves with the driver count; with every object private to its
  context, it measures no contention.
- Beside each cell the summary prints whether t's distance from 1.02 is
  larger than e = |w - 1|. That reading is descriptive and changes no
  verdict.

The falsifier is a valid cell (w within [0.99, 1.01]) with t > 1.02: at the
take program's one-driver cell it rejects the claims as they stand on
part 3; at any other cell it rejects the extension. The instrument can see
a change of that size only when the twin stays within 1 percent, which is
why a twin outside that range decides nothing rather than passing.
`summarize.py --self-test`, run in the workflow before the summary, checks
the rule on fixtures: equality, t exactly 1.02 and just above it, w exactly
0.99, w outside its range, a rejection outside the registered cell, a
rejection beside an inconclusive cell, medians against one slow block, a
pilot without the registered cell, and missing, duplicate, uneven, zero-rate,
wrong-count and misnamed rows, which exit 1.

This compares one compiler build pair on one machine and one workload shape;
it says nothing about the contended path, the claim queue's cost when
claims wait, guard reservations or other hosts.

## Required regression and pending evidence

`compiler/Makefile` includes `shared-take-witness` in `completion-test` and
both the claim and disabled-escalation images in `completion-test-images`.
The maintained `compiler/src/backend/completion/shared_take_check.py` runs
with `--require-bound`, validates raw counts and child exits independently,
and rejects missing evidence or runtime errors. The research sweep imports
that validator; the gate has no research dependency.

The fixed gate inventory contains:

- The arbitration fixtures (`shared-take-witness --arbitration`), four cases,
  each failing with a message naming the rule it guards:
  - `order`: a miss queues a claim; claims queue in arrival order behind
    readers already holding O; a reader arriving then parks; the parked
    head's due handoff precedes the oldest claim; then the oldest claim
    precedes the next due parked handoff, and so on in turn.
  - `newcomer`: a newcomer take with a claim waiting queues behind it,
    neither borrowing a lendable handed hold nor reading beside readers.
  - `borrow`: a take with no claim waiting borrows a handed hold; while that
    borrow is under way claims queue; after it, only the oldest claim
    borrows, then the next, and the parked grantee keeps its hold.
  - `reservation`: with a reservation installed for a context that has not
    retried and a newcomer parked behind it, the claim's queue keeps the
    reservation, the next free unlock grants the claim and wakes no one, and
    the last grant hands the reservation back to the slot.
  Handoff counters and the reservation are primed in these narrow fixtures;
  `shared_object_test.c` keeps real parked-wake, borrowing and notification
  coverage.
- An uncontended control, which must acquire without a failed probe.
- A scheduled N=1, cap=65 trial, which must acquire after exactly one
  completion. The fixture pauses X outside the object lock at the first
  failed-probe hook until its claim is granted or 65 Y completions finish,
  so the disabled-escalation image must report exactly 65 overtakes and exit
  2; a crash, timeout or passing mutant is an error. This models host
  preemption after the failed probe without modifying production admission
  or release.
- Two natural trials each at N=1,2,8 and cap=256, all requiring <=64 x N.
- Validator sensitivity cases for the inclusive 64 boundary, 65 violation,
  false child verdicts, runtime errors, missing/duplicate rows, incorrect raw
  subtraction, incomplete Y work and the negative control's intended count.

The retired `tmp-shared-take-witness.yml` (read at revision `f4299910e`)
ran the repeated sweep on `ubuntu-24.04` and `macos-15` (part 1 of the
criterion) and the fixed gate inventory, and uploaded raw counts, exits and
construction logs even on failure. The retired `tmp-shared-take-mutants.yml`
(same revision) applied one source mutant at a time to `bridge.c` with
`perl`, rebuilt the witness with `make -C compiler` on `ubuntu-24.04` and
required the named case to fail with its own message, after every case
passed unmutated. Its results are under
[Results and remaining evidence](#claim-arbitration-design-revision-f4299910e):

| Mutant | Rule it breaks | Case and message |
| --- | --- | --- |
| no-claim | a miss queues a claim (K never reached) | `order`: "a take that missed queued no claim"; scheduled trial: `bound_exceeded` |
| lifo | claims queue in arrival order | `order`: "claims did not queue in arrival order" |
| newcomer-borrow | a newcomer take does not borrow ahead of a claim | `newcomer`: "a newcomer take borrowed ahead of a waiting claim" |
| newcomer-read | a newcomer take does not read ahead of a claim | `newcomer`: "a newcomer take read ahead of a waiting claim" |
| reader | a claim refuses a reader that parks | `order`: "a reader overtook a waiting claim" |
| no-alternation | a parked handoff yields the next free unlock to a claim | `order`: "a parked context was handed the object again while a claim waited" |
| claims-first | the parked head gets its due handoff first | `order`: "a claim was granted ahead of the parked head's due hand-off" |
| grant-keeps-turn | a grant gives the next turn to the parked head | `order`: "claims took the object twice in a row while a parked context was due" |
| no-turn-after-handoff | a hand-off while claims wait gives the claims the next turn | `order`: "a parked context was handed the object again while a claim waited" |
| no-borrow | a take borrows a handed hold | `borrow`: "a take did not borrow the hold handed to a parked context" |
| no-claim-borrow | the oldest claim borrows a handed hold | `borrow`: "the oldest claim did not borrow the hold handed to a parked context" |
| any-claim-borrows | only the oldest claim borrows | `borrow`: "a later claim borrowed ahead of the oldest" |
| behind-reservation | a claim never waits behind a reservation | `reservation`: "a claim waited behind a guard's reservation" |
| lost-reservation | the claim queue keeps the reservation | `reservation`: "a claim's queue lost the object's reservation" |

The writer-acquire refusal by a waiting claim has no mutant of its own: an
unlock that frees O grants the oldest claim in the same critical section, so
a writer never finds O free while a claim waits, and only a reader (the
`reader` mutant) can observe the refusal.

The evidence above came from CI on revision `f4299910e`: the sweep, the
mutant run and the 14900K comparison, each with the retired machinery cited
there, and no local runtime measurement. The native regression and
maintained checker stay in the gate. Verified: the sampled bound and the
negative control on both hosts, each mutant failing its named case, and the
uncontended cost against its limit. Open: the unbounded-schedule argument,
the contended and guard-reservation costs, and any host or compiler pair
beyond those measured; sanitizer results come from the gate's map and
runtime jobs on the revision to merge.
