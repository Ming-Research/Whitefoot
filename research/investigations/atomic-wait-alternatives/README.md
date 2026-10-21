# Acquiring atomic targets with a deadline, and starting work while holding them

## Status and outcome

Design study, 2026-10-13, with a withdrawal model added after the owner's
direction (status board item `firn-req-wait-key-or-flag`). The study was
written against checkout `53809df17f85c5cff8d23a6764c34c820bc70efe`, with
Kernel Specification v0.149; the model follows the runtime as merged into
this branch at v0.151. No rule is changed here; the specification,
checker, lowering and runtime work is status board item
`atomic-until-impl`. Firn's four-driver hang is evidence supplied with the
request, not a run reproduced here. Redis compatibility requirements below
are the requirements supplied by the owner, not an independent Redis audit.

The study compared ways to let a waiting atomic statement give up
([Gap 1 candidates](#gap-1-candidates)) and ways to start a timer while a
statement holds its targets ([Gap 2 candidates](#gap-2-candidates)). It
recommended a cancellation watch (G1-A) and expression `spawn` under a hold
(G2-A). The owner selected neither as written:

1. **Gap 1: a deadline, not a watch.** An atomic statement takes an
   optional `until <Instant>` and a mandatory `else` branch. It either
   takes effect before the deadline or takes the `else` branch with no
   target binding and no target effect. One form ends both waits: for a
   unit another statement holds, and for a false guard. This is G1-A's
   outcome structure and withdrawal obligation with the runtime's existing
   timer as the only trigger. A cancellation watch may later be added as a
   second argument beside the deadline without changing this form. See
   [Selected form](#selected-form-deadline-acquisition).
2. **Gap 2: not pursued.** Firn's busy-script timing no longer needs a
   timer started under the hold: the script stores its start instant in an
   atomic value, and a waiting command computes its deadline as that
   instant plus the threshold. The Gap 2 sections remain as the comparison
   made, not as a proposal in progress.

The single-driver progress question remains a separate decision; see
[Progress and embedded targets](#progress-and-embedded-targets).

## Selected form: deadline acquisition

```wf
// store: &Shared<Store>; deadline: std::time::Instant, computed before.
atomic s = &store^ when ready(s: s) until deadline {
  // admitted before the deadline: every target held, guard true
  return Found(value: s^.value);
} else {
  // the deadline passed first: nothing taken, no binding of s here
  return Busy();
}
```

The form is a proposal for the specification step, uncompiled. Its
semantic commitments, as the owner selected them, are:

- The deadline is an absolute `std::time::Instant`, the same kind as
  `sleep_until` and the deadlines of host reads and writes. The guard reads
  no clock, so the rule that a guard reaches no self-changing host handle
  [SHARE-2] is unchanged.
- The statement has two outcomes: admission (every target held, the guard
  true, the block executed at one point [SHARE-3]) or withdrawal, after
  which the `else` branch runs with no target binding in scope. Which comes
  first when admission and expiry race is an input of the execution
  [WAIT-2]; both are valid. An admitted block is never interrupted, so
  nothing is rolled back.
- A statement without `until` compiles to the same code as today. A
  statement with `until` that needs no wait registers no timer.

Two consequences follow for the specification step; both are proposals:

- **The deadline ends a wait; it does not pre-empt an available
  admission.** A statement whose targets are free and whose guard is true
  is admitted without reading the clock, even when its deadline has
  already passed. Otherwise the no-wait path would sample the clock on
  every execution, against the zero-cost commitment above. C++
  `std::timed_mutex::try_lock_until` with a past time point behaves the
  same way: it acquires an available mutex. Once the statement has waited
  and the timer has fired, it is not admitted; it withdraws.
- **A statement with `until` takes every group before its guard.** Today
  a group the guard does not read may be taken lazily at its first use in
  the block (design node `compiler/waiting-contexts/state-locks`). A block
  that waited for such a group after admission would take effect after its
  deadline, or would need rollback. G1-A's text already required the same
  eager acquisition for its form.

### Withdrawal obligations in today's runtime

A statement can wait in five places today (from
`compiler/src/backend/completion/bridge.c`,
`compiler/src/backend/keyed_table.c` and
`compiler/src/backend/concurrent_map.c` as merged here). Withdrawal must leave each one exactly
as if the statement had never arrived:

| Wait | What the waiter may own when the deadline passes | What withdrawal must do |
|---|---|---|
| First ordinary object: parked on the object's queue | A queue link; a wake (the unlocker popped it and left the object free); a hand-off (after `WF_SHARED_HANDOFF` vain wakes the unlocker made it the holder), which a running statement may be borrowing | Unlink; release a handed-off object as an ordinary quiet unlock, after claiming it back and waiting for a borrower to give it back, as a resuming context does; pass a wake on to the next queued context when the object is free |
| Later ordinary object: a claim on its own stack | A claim link, which refuses parking newcomers, possibly with the claims' turn set (`take_turn`); a granted claim; a borrowed hold (the oldest claim, or a take's first look when no claim waits, may borrow a hold an unlock handed to a parked context) | Unlink the claim (and hand back the reservation slot if its record is the queue tail); when it was the last claim, end the claims' turn, as a grant does, since today a claim leaves only through a grant; release a granted object; give a borrowed hold back to the context it was handed to |
| Guard false: parked on a watch | A watch link; a notification; a reservation that refuses newcomers holding nothing | Unlink the watch; pass a notification on as a retry that wrote nothing does; clear a reservation and wake the queue head when the object is free |
| Map entries: spinning on cell lock bits | Cell locks already taken | Release the taken cells in reverse order; nothing is queued |
| Whole map: ticket lock, then the gate | A ticket in `hold_next` / `hold_serving`, possibly being served | A ticket cannot be abandoned: a withdrawing waiter either keeps spinning until served and then releases at once, or the ticket lock gains a skip mark. This is an implementation choice for `atomic-until-impl`, not modelled below |

In every row the units already taken are released in reverse order, as a
quiet unlock that starts no wake sweep, since the statement wrote nothing.
Timers do not reach these waits today: `wf_driver_expire_wait` returns at
once for a context without a completion record, and shared and watch parks
set none. The implementation therefore needs a record (or an equivalent
link) for a timed atomic park, armed at the first wait.

### Withdrawal model

`withdrawal_model.py` in this directory enumerates every interleaving of a
small configuration and checks the protocol of the table's first three
rows. It models lock units taken in one order. The first unit is taken by
parking. A later unit is taken without parking: when free, by borrowing a
hold handed to a parked context, or by a queued claim. When the last hold
ends, the unlock follows `wf_shared_free_locked`: with the claims' turn set
it grants the oldest claim; otherwise it wakes the first parked context the
reservation admits, or hands it the unit (both choices explored), then
grants the oldest claim if the unit is still free or sets the claims' turn.
The oldest claim may borrow a pending hand-off, which ends the claims'
turn; the borrower's unlock gives the hold back, and a handed context that
resumes while it is borrowed claims it back and waits. A false guard registers a watch, releases quietly and parks; a
write notifies the oldest watch whose test holds, optionally with a
reservation. Each operation on a unit is one step, as it runs under the
unit's lock. The timer firing is a separate step that only marks the timed
context expired and makes it runnable; its links stay until its own
withdrawal step, so every grant, hand-off, wake and notification that
races with the firing is explored.

Configurations: one or two units; the timed statement with and without a
guard; together with a writer that makes the guard true, an untimed
guarded statement, an untimed newcomer, and (with two units) one or two
holders of the second unit alone. One such holder makes the timed statement
wait on a claim while it holds the first unit; with two, one parks behind
the other, so the claims' turn, hand-offs to a parked context and borrowing
of them are reached. One more configuration has the timed statement take
the second unit alone, with a holder of that unit and the newcomer: the
timed statement parks on the second unit, and the newcomer, which takes it
as a later unit, borrows the hand-off made to the timed statement while the
timer fires.

Checked in every reachable state and every final state:

- admission only with every unit held and the guard true, and never after
  the timer fired;
- the `else` branch only after the timer fired, and then no body effect;
- every final state has every context completed, every unit free, and no
  queue, claim or watch link, claims' turn, pending hand-off, borrow,
  notification or reservation left;
- from every reachable state a final state is reachable, so no wake, grant,
  notification or reservation is lost.

A coverage count shows that the races are reached: the timer firing while
the timed statement holds a unit (also after it waited, before it takes the
next unit), waits on a claim (also while the claims
hold the turn), has a hand-off not yet taken up (also while a running
statement borrows it), holds a borrowed unit, owns a notification, owns a
reservation, and has been woken; a count of zero fails the run. Each of
eight deliberately broken protocols must fail at least one configuration:
keeping the units taken, keeping a hand-off not yet taken up, keeping the
claims' turn after withdrawing the last claim, leaving the links, dropping
a wake, keeping a notification, keeping a reservation, and admitting after
expiry.

RESULTS-PLACEHOLDER

What the model does not cover: map entries and whole-map tickets (rows four
and five), lazily taken groups (excluded by the eager rule above), a
notification sweep's cutoff by the writer's clock sample, shared (read)
holds, the count of vain wakes before a hand-off (`WF_SHARED_HANDOFF`; the
model lets every unlock choose either), the spin and yield before parking,
the claims' reservation slot, and more than one timed statement at a
time. It shows the protocol is consistent at this
scale; it is not a proof of the C implementation, which `atomic-until-impl`
tests directly.

## Sources and current semantic boundary

The authority is [the active specification](../../../spec/kernel-spec.md),
particularly GRAM-4, WAIT-1 through WAIT-3, EFF-1 through EFF-5, HOST-1,
SHARE-1 through SHARE-3, PRE-1 and PRE-2. The
[constitution](../../../docs/constitution.md) requires checked safety,
practical compilation, and support for small embedded systems as well as
large systems. No proposal uses SMT, a timeout to decide acceptance, a
writer-visible trap, or a runtime substitute for a missing proof. Runtime
arbitration and scheduling are execution mechanisms, not proof artifacts.

Relevant implementation and rationale read for this study:

- [Waiting contexts](../../../design/compiler/waiting-contexts.md), its
  [state locks](../../../design/compiler/waiting-contexts/state-locks.md),
  [concurrent map](../../../design/compiler/waiting-contexts/concurrent-map.md)
  and [bounded waits](../../../design/compiler/waiting-contexts/bounded-waits.md).
  These explain implementation choices, not extra source-language rules.
- [Atomic checker](../../../compiler/src/semantic/check/control/atomic.rs),
  `check_atomic` and `check_atomic_parts`, and
  [call checking](../../../compiler/src/semantic/check/expressions/calls/user.rs),
  where a waiting signature is rejected at positive atomic depth before
  spawn checking. The diagnostic is `WaitInsideAtomic` under SHARE-2.
  [Atomic lowering](../../../compiler/src/lowering/builder/atomic.rs)
  selects the eager group prefix from map targets and guard reads, and
  distinguishes first-group acquisition from later-group taking.
- [Completion bridge](../../../compiler/src/backend/completion/bridge.c),
  `wf_context_pass`, `WF_CONTEXT_YIELD_PASSES`, `wf_driver_reassign`,
  `WF_HANDOFF_TAU_NS` and the monitor. Immediately answered legal waits yield
  after 64 passes when another context is ready. This is not a poll in a
  computing atomic body. Reassignment currently requires an overdue timer;
  its threshold is 1,000,000 ns. It does not detect an otherwise ready child
  that has never run far enough to register its timer.
- [Cancellation decisions](../../../design/language/system-interface/context-cancellation.md)
  and [shared-object decisions](../../../design/language/waiting/shared-objects.md).
  Both record a previous rejection of cancellation that completes an atomic
  statement without running its body. That rejection must be reopened
  explicitly if this study is selected.
- The prior [atomic-target study](../atomic-targets/DESIGN.md),
  [guard cancellation study](../context-cancellation/guards.md), and
  [driver handoff study](../driver-handoff/DESIGN.md) provide comparison
  grounds, not proof that the new forms already work.

The following distinctions matter to both gaps:

| Rule | Current meaning relevant here |
|---|---|
| SHARE-1 | Shared state belongs to no context; paths into it arise through atomic targets. Handle copies do not prove different object identities. |
| SHARE-2 | An atomic statement is waiting. Its guard and body contain neither waiting calls nor another atomic statement. A guard writes nothing, reaches no self-changing host handle, and has only decision-cell-free callable boundaries. |
| SHARE-3 | Target-state accesses of a statement have one effect point. Shared effects have one total order respecting nonoverlapping source constructs and effects whose calls have already returned. Decision-cell calls inside a body have their own points; they publish only a control verdict, not intermediate target contents. |
| WAIT-1 | `waits` is a callable-boundary property, distinct from the state-effect row. A spawn's callee currently remains a waiting call for SHARE-2. |
| WAIT-2 | Progress is conditional: every context must reach completion or a wait in finitely many steps from every point. This is eventual progress, not a millisecond latency guarantee. |
| WAIT-3 | Spawn arguments are values. An expression spawn joins at activation exit; a bound spawn joins before the first using/leaving statement of its block, or at block end. |
| EFF / HOST-1 | State rows describe accesses through reference parameters, not an inventory of shared objects reachable through value arguments. Disjoint host effects have no order merely because two source calls appear in sequence. |
| PRE-1 | `DecisionCell` is not generic: it holds exactly `Open`, `Written`, or `Killed`. Read and claim are nonwaiting, independently ordered shared effects. |
| PRE-2 | `now` is nonwaiting and writes its clock; `instant_after` is total and saturates at the latest instant. `cancel_fire` waits and updates ordinary shared cancellation state. `sleep_until` waits and returns `Ok(unit)` for expiry or `Err(unit)` for cancellation. A completed outcome wins a later cancellation. |

“Acquire all targets before the guard” describes the semantic obstacle for
the key witness, but is not an exact description of every lowering. The
runtime may acquire ordinary-object groups lazily at first use. Map entry
and key-set groups, guard-read groups, and their earlier groups are acquired
before the guard. Groups use static state-type order, runtime identity order
within one type, and sorted keys inside maps. This is an implementation
deadlock argument, not a source annotation the writer must provide.

## Minimal semantic witnesses

### Gap 1: the alternative must not acquire the unavailable resource

There is one shared resource R and an independent monotone stop event C.
Context H holds R in an atomic body. Context W begins acquiring R. Context P
commits C while H still holds R. Required observation: W returns `Busy`
without reading or changing R and before H releases R. This requires neither
Redis, Lua, nor a map; a single shared scalar suffices. A map key makes the
same requirement in a server's resource domain.

Two versions separate expressibility from progress:

- H does a finite computation that lasts beyond C. Current WAIT-2 can apply,
  but it permits W to remain blocked until H releases. The required
  resource-or-cancel outcome is still missing.
- H never leaves its computing body. The owner reports Firn reproduced the
  hang with four drivers and a busy publication after one second. Current
  WAIT-2's universal premise does not hold, so this alone is not a violation
  of that progress rule. The language still lacks the requested alternative.

This attempted header cannot implement the alternative:

```wf
atomic slot = &data^[key], fired = &stop^ when fired^.fired {
  if fired^.fired { set answer = Busy(); } else { set answer = Found(value: slot^); }
}
```

This is a syntax fragment with `data`, `key`, `stop`, `answer` and constructors
supplied by its surrounding function. In addition to waiting for the key,
its `else` is unreachable: its guard requires the flag to be true. Replacing
the guard by `bor(resource_ready, fired^.fired)` fixes that logical mistake
but still requires acquisition of the resource before observing either
condition. Removing the guard likewise cannot bypass acquisition.

A flag probe before the atomic statement leaves a race: C can fire after the
probe but before acquisition succeeds. A probe of the protected resource
without acquiring it violates SHARE-1; repeating a try operation needs its
own progress and lost-wakeup semantics.

### Gap 2: create a deadline after acquisition but before release

One context queues for R. When it acquires R and starts its body, it samples
the monotonic clock as t. While still holding R it computes for longer than
D. An independent action must become eligible at t + D, including while R
remains held. The queueing interval before t must not contribute to D.

The minimal proposed operation is:

```wf
atomic value = &resource[key] {
  let started = std::time::now(clock: &clock);
  let deadline = std::time::instant_after(instant: started, nanoseconds: 5000000000_u64);
  spawn timer(source: move source, deadline: deadline, stop: move stop);
  set value^ = work(input: value^);
}
```

Here R is one keyed resource in a shared map, so its entry is acquired
before the body; no ordinary-object lazy-acquisition assumption is needed.
This fragment uses ordinary Whitefoot spelling, but is currently rejected
at `spawn`. `now` and arithmetic on `Instant` are not the rejection. Starting
the timer before acquisition can expire during queueing; starting it after
the body misses the interval to observe. A helper called inside the body
cannot hide a waiting spawn or `cancel_fire` from SHARE-2.

Storing t in another ordinary shared target does not publish it immediately:
the statement still holds that target. Reading it outside needs another
atomic statement and waits for release. Existing `DecisionCell` cannot carry
t, and a decision-cell observation cannot be used in an atomic guard.

## Gap 1 candidates

The following are exact proposed spellings, not existing grammar. Every
candidate retains ordinary borrowing, target lifetime, reference escape,
invariant-at-exit and effect-row checks unless explicitly changed. Fragments
use `answer`, `slot`, `ready` and `read_slot` only as surrounding program
names; complete recommended examples appear later.

### G1-A: cancellable acquisition with a separate outcome — basis of the selected form

```wf
atomic slot = &data^[key] cancel &watch {
  set answer = read_slot(slot: slot);
} else {
  set answer = Busy();
}
```

With a guard, the order is targets, `when <expr>`, `cancel &watch`, body,
`else`, cancellation body. `watch` has `std::time::CancelWatch` type; it is
borrowed for the statement. The new `else` is mandatory and means cancellation,
not a false guard. A false guard without cancellation keeps waiting.

**Semantics and effect order.** Evaluate handles, keys and watch once before
acquisition, retaining their required lifetimes. There are two competing
transitions: pending to admitted, and pending to cancelled. Admission needs
all targets acquired and the guard true. A firing completed before the
statement begins wins immediately. A firing completed while admission is
still pending prevents later admission. Concurrent admission and firing may
choose either order, recorded among WAIT-2's execution inputs. Admission is
irrevocable; later firing cannot roll back or interrupt the body.

On cancellation, remove every pending registration/claim and release partial
holds before entering `else`. No target binder is in scope there and no
target-state read, write, move or release from the success body has occurred.
The cancellation selection is a control effect ordered after the firing;
it is not a successful atomic effect on the targets. The ordinary statements
in `else` have their own effects. On success, preserve SHARE-3 for target
accesses. Admission arbitration is not a promise to order unrelated host
effects, and must not be confused with each target access being observable
at a separate point.

Cancellation applies both while acquiring and while a guard is false. It
stops at body admission, **not** at an arbitrary later group acquisition.
For this form, acquire all groups before executing any body operation,
including a clock read, DecisionCell claim or spawn. Ordinary atomic
statements retain lazy acquisition. Otherwise a body could already publish
an effect and then take the cancellation branch when a later group blocks;
that would require rollback which Whitefoot does not have.

**Checker.** Keep the existing group order and target alias merging. Check
the guard and success body under SHARE-2, the cancellation branch outside
the target scope, and the statement as waiting under WAIT-1 and PAR-1/2.
Check each outcome's ownership and normal leaving edges; facts learned from
the guard exist only in the success body. Outcome joins cannot invent
initialization, consume the same value twice, or expose an atomic reference.
No proof that cancellation will eventually happen is required. A pending
statement with `cancel_never` retains ordinary conditional progress.

**Runtime.** One arbitration record per cancellable statement, a watch
lifetime, cancellation observation during every potentially unbounded
acquisition phase, and cleanup of partial holds. An ordinary object waiter,
claims-queue claimant, map cell probe, reader-drain wait, map move, key-set
acquisition and whole-map fairness escalation must all reach cancellation.
It is insufficient to add one check before an existing uninterruptible map
take. The path must detach queue claims, borrowed handoffs and guard
reservations exactly once, preserve the next waiter's turn, and release map
user/publication state only when no cancelled operation can still use it.

The runtime may observe a committed cancellation indication without taking
its shared-state target lock; this is an internal implementation of the
new control outcome, not a public unheld read of `CancelState`. Registration
must close both races: fire just before registration, and fire after a
miss but before parking. Concurrent acquisition and cancellation need one
winner before user code can run. A fired indication means the existing
`cancel_fire` transition committed, not merely that a caller requested it.
The inspected bridge already maintains a protected visible state and an
atomic fired indication in `wf__cancel_fire_held`; their existence supports
this direction but does not supply the new admission/withdrawal protocol.

**Cost and progress.** Users pay registration/arbitration and cancellation
checks, plus eager acquisition of groups this form names. Nonusers should
keep the current call path, frame layout and acquisition operations, with
no new polling or allocation. Sharing implementation helpers is compatible
with compile-time specialization for an absent watch; this is not a
second acceptance path. Bounded per-wait memory and fixed-size atomic
control suffice in principle on weak CPUs; no wide CAS or unbounded thread
creation is a semantic requirement. Actual map cleanup progress is a major
unverified obligation. Single-driver behavior depends on the cancellation
producer running and on the acquisition loop not monopolizing the remaining
executor; see the progress section.

**Why reopen the old rejection?** The earlier choice used a cancellation
state as an ordinary guard target to avoid a new nonexecuting outcome. The
new witness distinguishes those semantics: the other target cannot be
acquired while the holder computes, even after firing has committed. An
extra outcome and its edge rules are now the capability needed, not
redundant syntax for the old guard.

### G1-B: acquire a control group separately and abandon data targets

```wf
atomic slot = &data^[key]
  unless fired = &stop^ when fired^.fired {
  set answer = read_slot(slot: slot);
} else {
  set answer = Busy();
}
```

The proposed `unless` target is a separate control observation. Repeatedly
acquire only that group's states, evaluate its read-only guard, and release
them; a true observation cancels the data acquisition. Successful admission
needs an arbitration rule against that observation. This is more general
than cancellation watches: queue closure, resource withdrawal and composite
predicates could be control conditions.

SHARE-3 must explicitly distinguish each control observation from the data
statement's effect point. Control bindings cannot survive into the success
body; `else` receives at most a copied observation. The checker must forbid
target-dependent control paths and preserve lock order across both groups.
Taking an earlier control lock while holding later data locks can create a
cycle. A safe design drops all tentative data holds before a control read,
or provides a proved disjoint protocol; it cannot simply exempt the control
group from the order. Alias handles can name the same object across groups.
No ordinary row establishes their identity separation.

Runtime costs include two wait sets, repeated control acquisition, partial
release/retry and fairness across both. Nonusers can remain unchanged. Weak
CPUs need only existing locks but may suffer retry traffic. On one driver,
a held control object can block the cancellation observation itself; option
A cannot make that holder release. A nonmonotone predicate can become true
and false between observations, so existing WAIT-2 cannot promise to catch
every pulse. Requiring persistent truth is coherent but weaker than a
latched CancelWatch. Reject as the first mechanism: it adds these observable
snapshot, alias and fairness choices without improving this monotone witness.

### G1-C: select among atomic alternatives

```wf
select {
  atomic slot = &data^[key] { set answer = read_slot(slot: slot); }
  atomic fired = &stop^ when fired^.fired { set answer = Busy(); }
}
```

Only one arm's body runs. Evaluate arm inputs once; try acquisition/guards
without body effects, register all alternatives, and commit one arm. This
would also express wait-for-any-queue, unlike a cancellation-only suffix.
Both ready arms need an explicit nondeterministic or priority policy;
strong fairness for transient readiness is not supplied by WAIT-2.

WAIT-1/2 must classify the whole selection as one wait. SHARE-3 applies to
the selected arm only; losing arms contribute no body effects. The checker
keeps per-arm group order, joins ownership across branches, and prevents
bindings and loans from a losing arm escaping. The runtime must never hold
partial locks for one arm while waiting for another arm in reverse order;
it must retain registrations without such holds. Cross-arm alias merging,
guard reservations, withdrawal and repeated selection fairness are real
obligations, not consequences of sorting each arm alone.

Cost is proportional to registered alternatives and their targets; nonusers
can have zero added work. An embedded implementation can bound storage by
the syntactic arm count, but notification and starvation policy are more
complex than G1-A. On one driver, this still needs a runnable producer;
option A cannot rescue a selection implementation that occupies every spare
spinning on an arm it should have registered. Keep as a later generalization
if independent server requirements need several productive arms. It is not
just syntactic sugar implementable by racing whole existing statements:
both bodies could run before the loser is stopped.

### G1-D: nonblocking try-acquire followed by retry

```wf
try atomic slot = &data^[key] {
  set answer = read_slot(slot: slot);
} else {
  set retry = True();
}
```

`try atomic` would return immediately when the entire target set cannot be
acquired; failure has no target effects. Success must still evaluate a guard
under holds and execute its body once. This adds failed-acquisition outcomes
to WAIT-2's execution inputs and needs a decision whether false guards count
as failure. Keep its public classification waiting, even when answered at
once, so it remains forbidden inside another atomic body and participates
in the existing fairness pass. Otherwise a busy retry loop never reaches a
WAIT-2 wait.

The checker must prohibit partial body execution and keep ordinary target
order/alias rules. Runtime attempts release partial holds and give an honest
failure; a whole-map fallback cannot quietly turn “try” into a wait. Failed
attempts have finite work but no source-level elapsed-time promise.

A separate cancellation probe plus try in a loop still needs a nonblocking
probe API, consumes CPU, and can lose the resource forever to competitors.
Sleeping between retries changes responsiveness; registering for key or
cancel recreates G1-A/G1-C. Nonusers need no extra path; users pay repeated
attempts and cache traffic, especially unattractive on embedded CPUs.
Option A schedules peers but does not establish retry acquisition fairness.
Reject as the sole answer to a wait-for-resource-or-cancel requirement;
retain as a possible separate fail-fast operation if a workload needs it.

### G1-E: holder-published control state with a nonblocking probe

```wf
let stopped = cancel_fired(watch: &watch);
if stopped {
  set answer = Busy();
} else {
  atomic slot = &data^[key] { set answer = read_slot(slot: slot); }
}
```

`cancel_fired` is a proposed function, not PRE-2. A read of a committed
monotone control bit could be independently ordered, like a DecisionCell
snapshot, but it cannot establish a stable ordinary shared-state fact or a
guard dependency. A public probe would require revisiting the recorded
rejection of that function. Reading `DecisionCell` already provides a
nonwaiting control observation, but does not subscribe an atomic acquisition
to it and cannot be placed in a guard.

An entry-only check fails the after-check firing witness. Checking during
acquisition is G1-A's runtime mechanism; checking only in user code cannot
interrupt a blocked `atomic`. If the holder itself polls the clock and
publishes busy, ordinary `cancel_fire` is waiting and forbidden there. A
nonwaiting replacement that updates held cancellation state would break the
stable-state observation required by SHARE-3. Checks and publications have
cost only where called, but polling adds compute and energy costs; scheduling
option A does not fix the semantic race. Reject as a complete solution.

## Gap 2 candidates

Not pursued; see [Status and outcome](#status-and-outcome).

### G2-A: expression spawn inside an atomic body — recommended in the study, not pursued

```wf
atomic slot = &data^[key] {
  let started = std::time::now(clock: &clock);
  let deadline = std::time::instant_after(instant: started, nanoseconds: delay);
  spawn timer(source: move source, deadline: deadline, stop: move stop);
  set slot^ = compute();
}
```

**Meaning.** Starting is a context-creation event, not waiting for the
callee. Argument evaluation/move precedes that event; subsequent parent
statements follow it. The child may begin before the parent releases its
targets. Keep WAIT-3's value-only arguments, waiting callee and droppable
discarded result. Keep the current activation lifetime; this is structured
spawn, not an unjoined detached thread.

Revise WAIT-1/SHARE-2's classification at the **spawn expression statement**,
not the callee's signature. The callee still waits and cannot be called
directly inside the block. The enclosing function remains waiting and the
atomic statement still excludes PAR overlap. Guards remain unable to spawn.
`let result = spawn ...` remains refused inside an atomic body: its first use
or block end is an implicit wait, even when the child usually finishes early.

On normal exit, return, propagation, break and continue, the compiler must
restore target invariants and release any atomic holds before any
activation-exit join. A break/continue that merely leaves the atomic region
does not prematurely join an expression spawn; it still belongs to the
activation. Do not implement a new atomic-block join or move the start to
block exit. Runtime resource refusal remains the existing SCOPE-3 boundary,
not a source-visible trap or a fallback that runs the child inline.

**Effects and proof.** Moved/copied values give the child independent owned
storage. A borrowed atomic binder cannot be a spawn argument. Shared handle
aliases remain legal: a child acquiring a parent's target waits until its
release, and the parent does not join under that hold. Thus no new
parent-holds/parent-joins/child-acquires lock cycle is introduced. Existing
lock ordering still governs the child's own statements. No global
termination proof is claimed: a parent that loops until a child changes
state behind the parent's own hold can still fail to terminate, just as a
pre-existing child can today.

Do not impose “the spawned call reaches no held object” using the callee's
ordinary effect row. A function taking `Shared<T>` by value can have row
`pure` and still execute atomic statements on its state. Different handles
may alias. Such a stronger condition would need a new independently checked
capability/identity summary. It is not needed for the no-join-under-hold
argument, and would refuse safe queue-after-release programs.

The spawn event has no target-state effect. SHARE-3 still gives the outer
statement one point and the child's atomic statements/DecisionCell calls
their own points. Do not declare that the entire enclosing atomic effect is
before every child effect, nor that all child effects are after release.
HOST-1 remains unchanged. In the timer witness the child receives the
completed clock reading as an `Instant` value; the deadline is derived
from that reading, regardless of how late the child is scheduled.

General spawn also permits copying a value computed from a target into the
child, which can publish that copy to an independent object before the
parent releases. Value ownership prevents an escaping target reference;
it does **not** prevent this information flow. Therefore G2-A, like G2-B,
needs an explicit semantic account of intermediate-value publication. The
recommendation permits it as an ordinary transferred value, never as an
unlocked observation of the held target, subject to the effect-order model
below. Existing DecisionCell's verdict-only clause is not a proof of that
account. If the owner requires all such publication to remain impossible,
unrestricted expression spawn is not the right recommendation; an
Instant-only start event would be a materially narrower alternative.

**Mechanism and cost.** Reuse the context argument block, wrapper, arena,
enqueue and activation join list. Audit first-spawn initialization and
displaced-driver enqueue paths: a start inside a hold cannot suspend the
parent, run the child inline, borrow target references, or call the
64-pass fairness suspension path. This reuse is a design expectation, not
verification that all existing bridge entry points already satisfy it.
Users pay ordinary spawn storage/enqueue and eventual join, plus their
timer. Nonusers need no new runtime state or branch. A weak CPU can implement
this with the existing queue primitives; no new atomic width is needed.
An embedded runtime without spare execution capacity still cannot promise
the child runs during endless parent computation. Under option A the ready
child is precisely the new detector's case, subject to its capacity premise.

**Rejected narrower variant.** Allowing only `spawn timer(...)` by name or
recognizing a clock/sleep/publish function shape would make program names
select acceptance. A general ordinary spawn meets the same safety argument
and serves logging, independent work dispatch and resource-release follow-up.

### G2-B: publish a one-shot value to an already started waiter

Proposed interfaces and use, not existing declarations:

```wf
fn once_publish<T: copy>(cell: &Once<T>, value: T) -> accepted: Bool reads(cell);
fn once_wait<T: copy>(cell: &Once<T>, cancel: &std::time::CancelWatch)
  -> result: Result<T, unit> reads(cell), reads(cancel) waits;

atomic slot = &data^[key] {
  let started = std::time::now(clock: &clock);
  let published = once_publish::<std::time::Instant>(cell: &start, value: started);
  set slot^ = compute();
}
```

`Once<T>` would need owning shared handles, creation/share operations and a
single-assignment protocol: the first publish wins, repeated publish returns
false, wait returns the immutable value or cancellation. A consuming unique
publisher is another viable ownership design, but still needs a defined
closed-without-publishing outcome. These declarations deliberately do not
pretend PRE-1 already supplies `DecisionCell<Instant>`.

The publication is a separately ordered shared effect, never a write to an
ordinary atomic target at an intermediate visible point. Its callable
boundary needs the same recursive guard and PAR exclusion treatment as
DecisionCell; otherwise `reads(cell)` would falsely permit speculative
duplication/reordering or a guard with no registered dependency. Payload
copies cannot contain target references; extending the immutable payload
domain to handles requires an explicit ownership/alias design. Even copy
payloads can expose intermediate values computed under a hold, beyond the
current DecisionCell control-verdict promise. That deserves a semantic
decision, not an assumption that renaming the type preserves SHARE-3.

No lock held by a target may be acquired by the publisher; waiting and joins
remain outside the atomic body. Runtime costs are a cell, publication
arbitration, immutable payload, waiter registration and notification, with
zero additions where unused. A weak-CPU implementation must not assume that
arbitrary payload publication is one lock-free machine instruction. A fixed
cell with one publisher can use a small atomic ready state and release/acquire
publication, but full ownership and withdrawal still need design.

The outside context may be parked before publication, but must be scheduled
afterward to register its sleep. Option A is still required in the same
single-driver compute case; an event wake is not a timer already installed
in the heap. Reject as the first answer because it introduces another
publication abstraction and its lifecycle/effect boundary when existing
spawn can transfer the value. Reconsider if independent consumers require
one-shot data delivery without a newly created context.

The [relaxed-fields investigation](../relaxed-fields/README.md)
also records, immediately before its pause section, the owner's rejection
of wait conditions/publication flags on relaxed cells because opposite
wait orders can deadlock. G2-B cannot bypass that objection by changing the
type's name. Its distinguishing proposal is a one-shot publication with no
wait under a hold and explicitly outside waiting, but closed-source behavior
and cycles of outside dependencies still require review against that prior
ground. The recommended G2-A creates no new cell-wait primitive.

### G2-C: an entry-relative timer attached to the atomic statement

```wf
atomic slot = &data^[key]
  timer clock: &clock, after: delay, fire: &source {
  set slot^ = compute();
}
```

The proposed header samples time immediately before body admission, after
acquisition and a true guard, registers t + delay, and arranges cancellation
publication if the body remains active. It must specify what disarms on
each leaving edge and which side wins expiry racing with disarm. A false
guard/retry must not start or restart the timer. The source is explicit;
there is no implicit Redis busy flag.

This closes the ready-but-not-yet-armed child gap: the runtime already has
the deadline when the body starts. It does not make the firing automatically
nonwaiting. PRE-2 says `cancel_fire` is an ordinary held state transition;
the source can have an alias in the atomic targets. An interrupt callback
must neither deadlock acquiring that state nor change its stable observation
behind a held target. A scheduled context could fire normally but then the
design is specialized spawn with a lifecycle policy. Alternatively this
candidate requires a new independent timer-event state and observation API.

Checking must add this distinction, retain group order, and reject or define
source/target aliases without runtime proof traps. SHARE-3 needs the timer
event distinguished from target-state effects; HOST-1 needs explicit clock
and source footprints. Timer descriptors, arm/disarm arbitration and wake
work occur only for users. An embedded implementation can use a hardware
timer to make an event due, but scheduling language code or committing the
shared-state firing still requires execution capacity. Current overdue-timer
handoff helps once armed; option A still matters for runnable cleanup and
other clients. Reject as the first form because it couples atomic syntax to
one time service and disarm policy; reconsider if ordinary spawn's per-script
cost fails the predeclared workload criterion and a general timer capability
can preserve the same semantics.

### G2-D: have the computing context poll its elapsed time

```wf
atomic slot = &data^[key] {
  let started = std::time::now(clock: &clock);
  let deadline = std::time::instant_after(instant: started, nanoseconds: delay);
  loop {
    let current = std::time::now(clock: &clock);
    let due = std::time::instant_reached(deadline: deadline, instant: current);
    if due { publish_busy_now(source: &source); }
    step(slot: slot);
  }
}
```

`publish_busy_now` is hypothetical. Ordinary `cancel_fire` is not usable in
its place. A separate nonwaiting control cell could record busy, with
DecisionCell-like ordering and guard exclusion, but waiters still need G1-A
or selection to interrupt acquisition. Replacing ordinary cancellation by a
raw bit would contradict its coherent shared-state semantics.

The checker can admit `now` in the body, never in a guard. This needs no
waiting child for observation, but binds detection latency to application
poll frequency and every potentially long compute primitive. It adds clock
reads and branches to that path, especially material on weak CPUs. Nonusers
pay nothing. Option A lets other contexts run but cannot shorten time until
the next application poll. Reject as the language's answer to independent
timed work; it is a possible application policy only where bounded polling
work is itself established.

## Progress and embedded targets

Keep three different properties separate:

1. **Safety:** target references never escape; no body rollback, unsound
   unlocked target access, new lock-order cycle or join under a hold.
2. **Event eligibility:** a committed cancel can win pending acquisition;
   timer expiry is derived from the timestamp captured after acquisition.
3. **Scheduling progress:** a publisher, waiter or kill requester eventually
   runs, and cleanup returns. Neither syntax supplies CPU time by itself.

Current WAIT-2 proves conditional eventual progress only. Its premise can
fail while a script computes forever without waiting. The existing monitor
hands a role to a spare only for an overdue registered timer. A child queued
by G2-A, or awakened by G2-B before submitting its sleep, can remain unrun
with no such timer. G1-A can observe a cancellation already committed on
another executor; it cannot cause an unscheduled publisher to commit it.

Take the pending option A exactly as supplied: detect a nonempty ready
queue while the running context has not waited for tau = 1 ms, hand the
driver to a spare, and extend WAIT-2 only for a bounded number of
simultaneously nonwaiting contexts, at most the driver count. This study
does not approve it or convert tau into a 1 ms response guarantee. Monitor
sampling, host scheduling, arbitration and cleanup add latency.

One driver with one computing script and a short publisher fits the intended
case only if the publisher's bounded work and resource waits are handled
as the option's progress model requires. A **second spinning key acquirer**
is a critical counterexample: the existing map path spins without parking.
It can occupy the replacement executor while the timer publisher remains
ready, exhausting the spare even though the source operation is called a
wait. The combined design must either park/release tentative holds on that
new cancellable path or count such runtime spinning against the capacity
bound and admit that Firn's witness remains uncovered. Merely checking a bit
in the map loop does not help if its producer cannot run. This is a blocking
design question for implementation, not evidence against the cancellation
semantics themselves.

Similarly, `cancel_fire` may wait for its ordinary cancellation state if a
different statement holds it. G1-A guarantees withdrawal after a **committed
firing**, not after an arbitrary attempt to fire. Do not promise that busy
publication can overcome a program holding its own cancellation state.

Embedded remains in scope. The language forms need bounded per-operation
records and no instruction-set-specific atomic width; an implementation
may use short internal critical sections to publish queue/control state,
without running source code or waiting for a target inside those sections.
Proofs remain erased. A single CPU with a preemptive host can run a warm
spare without providing a second core. A truly cooperative bare-metal
target with no interrupt-driven context service cannot execute independent
work during an endless computation. A conforming strengthened progress
promise would need a bounded interrupt/preemption mechanism there, or a
separately justified conditional premise that covers that target. “Require
two drivers” or “exclude weak CPUs” is not an acceptable resolution. No
embedded implementation feasibility or energy cost is established here.

## Composition, SCRIPT KILL, and whether one mechanism suffices

A universal resource selection construct could include cancellation and
timer alternatives. It still needs a way to create or arm the timer after
the holder enters its body. Conversely, spawn creates the publisher but
cannot withdraw an existing atomic acquisition. The two gaps concern
different boundaries. Sharing a runtime completion/arbitration facility is
sensible; a single new source construct is not required or justified by
these witnesses.

The recommended composition is:

1. Give each script invocation a fresh busy cancellation source/watch
   generation, an independent timer-disarm source/watch, and a DecisionCell
   for kill versus first write. Do not reuse a fired busy generation.
2. Acquire the script's full key set. Capture t at the chosen first body
   operation and spawn the timer with deadline t + threshold. The timer
   sleeps, then calls ordinary `cancel_fire` if expiry won over disarm.
3. Commands that can be refused use G1-A with the busy watch. A selected
   cancellation outcome means BUSY only for that script generation. Publish
   and retire the active generation through the server's ordinary shared
   metadata protocol; a stale timer must not poison a later script.
4. SCRIPT KILL uses `claim_killed`, independent of acquiring script keys.
   The script polls `decision_cell_read` at its interpreter checkpoints.
   Immediately before the first real write, it calls `claim_written` and
   mutates only if the result is `Written`. A prior `Open` read is not a
   permission to write. If `Killed` won, leave by an ordinary edge, release
   keys, disarm the timer outside the block and join at activation exit.

The existing verdict arbitrates eligibility, not arbitrary interruption or
rollback. After `Written` wins, killing must fail, even if the physical
write follows the claim. PRE-1 explicitly says the verdict is independent
of whether a subsequent write changes a value; the exact Redis command
classification for a no-op needs Firn's existing compatibility decision.
Cancellation of command acquisition does not kill the script. A killed
script stops only at an executed checkpoint; an unbounded nonpolling
primitive is a separate cooperative-kill limitation. Option A lets the kill
requester run but does not inject a checkpoint into that primitive.

An expiry racing with script completion can fire busy after disarm is
requested, since a completed sleep outcome wins. Per-invocation generations
and retiring the invocation prevent that from affecting future scripts.
Neither closing a source/watch nor firing disarm proves the child has
joined. A production server must arrange disarm on every normal/error/kill
path before its activation join; no hidden `cancel_fire` is inserted under
the atomic hold.

## Complete before/after examples

These are complete source programs, not claimed passing compiler runs.
Existing spelling was checked against
[guard cancellation](../../../tests/conformance/cases/share-run-guard-deadline-through-a-cancel-watch.wf),
[spawn and joins](../../../tests/conformance/cases/wait3-pos-spawn-let.wf),
[spawn refusal](../../../tests/conformance/cases/wait3-neg-inside-atomic.wf),
[DecisionCell under atomic](../../../tests/conformance/cases/decision-cell-run-atomic.wf),
and [the cancellation program](../../../tests/programs/cancellation.wf).
Proposed grammar or admission is identified for each program. Counts in
the computation examples are finite demonstration inputs, not measured
durations or a timeout test. Each fenced program is independent.

### Gap 1 before: a cancellation target still waits for the key

This uses current grammar. With no guard it can read an available key or
report an observed cancellation, but a held key prevents either body path.
The pre-fired main is just a complete caller, not a contention reproducer.

```wf
enum Reply {
  Found(value: Option<u8>);
  Busy();
}

fn read_or_busy(data: &Shared<ConcurrentHashMap<u8>>, key: &[u8],
                watch: &std::time::CancelWatch) -> reply: Reply
  reads(data), reads(key), reads(watch) waits {
  let stop = std::time::cancel_state(watch: watch);
  atomic slot = &data^[key], fired = &stop {
    if fired^.fired {
      return Busy();
    } else {
      return Found(value: slot^);
    }
  }
}

fn main() -> status: std::process::ExitStatus pure waits {
  let data = shared_map_new::<u8>(capacity: 1_u64);
  let bytes = array_filled::<u8, 1>(value: 107_u8);
  let key = &bytes[0_u64..1_u64];
  let source = std::time::cancel_source();
  let watch = std::time::cancel_watch(source: &source);
  std::time::cancel_fire(source: &source);
  let reply = read_or_busy(data: &data, key: key, watch: &watch);
  match reply {
    Busy() => { return std::process::exit_status(code: 0_u8); }
    Found(value: ignored) => { return std::process::exit_status(code: 1_u8); }
  }
}
```

### Gap 1 after: cancellation chooses a branch without the key

The sole new syntax is `cancel &watch { ... } else { ... }` after the target
list (here the parameter already is a reference, so `cancel watch`). For
precision, the proposed grammar takes a borrow expression **or a reference
atom** of type `&CancelWatch`, just as an ordinary reference argument does.
No cancellation target is added to the target list.

```wf
enum Reply {
  Found(value: Option<u8>);
  Busy();
}

fn read_or_busy(data: &Shared<ConcurrentHashMap<u8>>, key: &[u8],
                watch: &std::time::CancelWatch) -> reply: Reply
  reads(data), reads(key), reads(watch) waits {
  atomic slot = &data^[key] cancel watch {
    return Found(value: slot^);
  } else {
    return Busy();
  }
}

fn main() -> status: std::process::ExitStatus pure waits {
  let data = shared_map_new::<u8>(capacity: 1_u64);
  let bytes = array_filled::<u8, 1>(value: 107_u8);
  let key = &bytes[0_u64..1_u64];
  let source = std::time::cancel_source();
  let watch = std::time::cancel_watch(source: &source);
  std::time::cancel_fire(source: &source);
  let reply = read_or_busy(data: &data, key: key, watch: &watch);
  match reply {
    Busy() => { return std::process::exit_status(code: 0_u8); }
    Found(value: ignored) => { return std::process::exit_status(code: 1_u8); }
  }
}
```

### Gap 2 before: legal placement starts the clock too early

The timer starts before acquisition. Under contention its five-second
deadline includes queueing time. `cancel_fire` after the block disarms a
still-waiting timer; the expression spawn joins on main's return.

```wf
fn timer(source: std::time::CancelSource, deadline: std::time::Instant,
         stop: std::time::CancelWatch) -> result: unit pure waits {
  let outcome = std::time::sleep_until(deadline: deadline, cancel: &stop);
  match outcome {
    Ok(value: elapsed) => { std::time::cancel_fire(source: &source); }
    Err(error: cancelled) => {}
  }
  return unit;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(cwd: directory, handles: factory, clock: clock, ..) = move inputs;
  let std::fs::Directory(read: directory_read, write: directory_write) = move directory;
  std::fs::close_directory_write(factory: &factory, directory: move directory_write);
  std::fs::close_directory(factory: &factory, directory: move directory_read);
  let data = shared_map_new::<u64>(capacity: 1_u64);
  let bytes = array_filled::<u8, 1>(value: 107_u8);
  let key = &bytes[0_u64..1_u64];
  let source = std::time::cancel_source();
  let busy_watch = std::time::cancel_watch(source: &source);
  let disarm = std::time::cancel_source();
  let stop = std::time::cancel_watch(source: &disarm);
  let started = std::time::now(clock: &clock);
  let deadline = std::time::instant_after(instant: started, nanoseconds: 5000000000_u64);
  spawn timer(source: move source, deadline: deadline, stop: move stop);
  let checksum = 0_u64;
  atomic slot = &data[key] {
    for (i in 0_u64..1000_u64) {
      set checksum = checksum +wrap i;
    }
    set slot^ = Some<u64>(value: checksum);
  }
  std::time::cancel_fire(source: &disarm);
  std::time::close_cancel_watch(watch: move busy_watch);
  return std::process::exit_status(code: 0_u8);
}
```

### Gap 2 after: same spawn syntax, newly admitted inside the body

This is proposed acceptance of current spellings; today's checker rejects
the spawn under SHARE-2. `data` is a local handle, hence `&data[key]`;
`&data^[key]` in the preceding functions dereferences a reference parameter.
The sample keeps a busy watch alive but needs no command client to show
where the deadline originates. The map key is acquired before the timestamp.

```wf
fn timer(source: std::time::CancelSource, deadline: std::time::Instant,
         stop: std::time::CancelWatch) -> result: unit pure waits {
  let outcome = std::time::sleep_until(deadline: deadline, cancel: &stop);
  match outcome {
    Ok(value: elapsed) => { std::time::cancel_fire(source: &source); }
    Err(error: cancelled) => {}
  }
  return unit;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure waits {
  let std::process::Inputs(cwd: directory, handles: factory, clock: clock, ..) = move inputs;
  let std::fs::Directory(read: directory_read, write: directory_write) = move directory;
  std::fs::close_directory_write(factory: &factory, directory: move directory_write);
  std::fs::close_directory(factory: &factory, directory: move directory_read);
  let data = shared_map_new::<u64>(capacity: 1_u64);
  let bytes = array_filled::<u8, 1>(value: 107_u8);
  let key = &bytes[0_u64..1_u64];
  let source = std::time::cancel_source();
  let busy_watch = std::time::cancel_watch(source: &source);
  let disarm = std::time::cancel_source();
  let stop = std::time::cancel_watch(source: &disarm);
  let checksum = 0_u64;
  atomic slot = &data[key] {
    let started = std::time::now(clock: &clock);
    let deadline = std::time::instant_after(instant: started, nanoseconds: 5000000000_u64);
    spawn timer(source: move source, deadline: deadline, stop: move stop);
    for (i in 0_u64..1000_u64) {
      set checksum = checksum +wrap i;
    }
    set slot^ = Some<u64>(value: checksum);
  }
  std::time::cancel_fire(source: &disarm);
  std::time::close_cancel_watch(watch: move busy_watch);
  return std::process::exit_status(code: 0_u8);
}
```

The timestamp is the first explicit body operation, not an assertion that
the physical last-lock instruction and the clock sample are simultaneous.
If the required origin is exact runtime admission even across host
preemption before that sample, select an admission-timestamp facility
instead; this distinction needs the owner's answer.

### Existing DecisionCell protocol for cooperative kill

This complete program uses current syntax and no new language feature. The
deterministic pre-kill checks the first-write refusal path; a concurrent kill
requester would receive another `decision_cell_share` handle and call the
same `request_kill` function. It illustrates the protocol, not a Redis
command dispatcher or a timing claim.

```wf
fn request_kill(cell: &DecisionCell) -> result: DecisionState reads(cell) {
  return claim_killed(cell: cell);
}

fn script(data: &Shared<u64>, cell: &DecisionCell, steps: u64)
  -> result: DecisionState reads(data), reads(cell) waits {
  let killed = Killed();
  let written = Written();
  let verdict = Open();
  atomic value = &data^ {
    let sum = value^;
    for (i in 0_u64..steps) {
      let observed = decision_cell_read(cell: cell);
      if observed == killed {
        set verdict = killed;
        break;
      }
      set sum = sum +wrap i;
    }
    if verdict != killed {
      set verdict = claim_written(cell: cell);
      if verdict == written {
        set value^ = sum;
      }
    }
  }
  return verdict;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let cell = decision_cell_new();
  let control = decision_cell_share(cell: &cell);
  let data = shared_new::<u64>(value: 7_u64);
  let requested = request_kill(cell: &control);
  let observed = script(data: &data, cell: &cell, steps: 1000_u64);
  let killed = Killed();
  if observed != killed {
    return std::process::exit_status(code: 1_u8);
  }
  atomic value = &data {
    if value^ != 7_u64 {
      return std::process::exit_status(code: 2_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
```

## Separating evidence before implementation commitment

These are proposed experiments and acceptance criteria, not results. First
perform a finite event/ownership model and lowering design review; no
production implementation should be selected merely because examples read
well. Then, only on explicit authorization, run the smallest useful pilot
in CI, inspect its duration/spread, and choose the repetitions. Correctness
cases belong in conformance/program/runtime tests when implemented; the
gate must never depend on this research document or its examples.

| Test | Separating construction and pass/fail criterion |
|---|---|
| Cancellation admission model | Enumerate both orders of firing, registration, each partial target acquisition, guard test, admission and withdrawal, with two target groups and aliased handles. Pass only if exactly one outcome occurs, cancelled bodies have zero effects, no binder escapes, and every pending-to-cancelled trace releases all claims/holds. A trace with a lost wake, double completion or residual hold rejects G1-A's protocol. |
| Held-key witness | Hold a key until a test-controlled release event. After a waiter registers, commit cancellation independently. The waiter must report cancellation while the release event is still absent, with zero target body effects. The old guard-target form must fail that ordering observation. Use controlled scheduling/fake events in the model; do not depend on a guessed compute-loop duration. |
| Multi-target withdrawal | Hold an early target tentatively, block on a later target, then cancel. An unrelated acquirer of the early target must subsequently succeed; the cancelled body must not have run. Include same-type alias merging, key sets, whole-map escalation, resize, guard reservations, queue claims and borrowed handoffs. Any stranded next waiter or retired-table access fails. |
| Guard outcome | With a never watch, a false guard stays pending and a persistently true guard progresses under WAIT-2's premise. With a fired watch and false guard, cancellation completes. Under racing firing/admission either declared outcome is allowed, never both, never partial body effects. |
| Start-origin model | Delay acquisition by Q = 100 logical ticks, choose D = 10 ticks, then enter the body at t. Expiry eligibility must be exactly the saturating t + D, never request_time + D or child_run_time + D. Delay the child's first run until t + 20: it must recognize an already-passed deadline when it runs. No event may be labelled expiry before t + D. |
| Join-under-hold refuter | Spawn a child that acquires the very same shared object, then take each parent leaving-edge class. Pass if release precedes join, the child can finish, and target invariants hold. Reject the design if it needs to prohibit an otherwise safe handle alias. Keep bound spawn, direct waiting calls, guard spawn and borrowed target arguments as negative cases with their intended diagnostics. |
| Effect-order litmus | An outer atomic modifies a target and starts a child touching that target and an independent DecisionCell. Check traces against SHARE-3's total order, distinct cell points and HOST-1; child reads of the held target see only the state after release. Separately transfer a copied intermediate value into a child that publishes it to an independent object before parent release, then have another context read both objects. Every admitted trace must have the declared shared-effect order; any counterexample rejects unrestricted G2-A pending a revised semantic design. Add an independent child that completes before release so accidentally moving every spawn to block exit fails. |
| One-driver scheduling | Begin with no timer registered, one computing holder, a ready publisher, and a cancellable key waiter. Under the selected option-A capacity definition, require publisher execution and waiter cancellation before holder release. If the waiter consumes the only spare indefinitely, the combination fails even if each feature passes on four drivers. Also test just outside the stated capacity bound and claim no stronger progress. |
| Kill versus first write | Exercise kill-first and write-first orderings, plus two concurrent claimants. Exactly one terminal verdict survives. Killed-first has zero protected writes; written-first rejects kill. A check-Open-then-write mutant must fail. A nonpolling script primitive is explicitly outside a bounded cooperative-stop claim. |
| Generation and cleanup | Expiry racing with disarm/return may complete either declared outcome, but an old invocation can never fire a new generation; join ends all child access before activation storage is released. All early exits and closed-watch/source lifetime cases must retain valid state. |

A hosted timing trial, if authorized after semantic review, uses one driver
on one CPU and four drivers, with the same instrumented source, base,
candidate and interleaved twin control on the 14900K CI runner. Use a
100 ms threshold and force acquisition queueing longer than that threshold;
start time is instrumented separately. Proposed trial criterion: no busy
publication before recorded start + 100 ms; cancellation and publication
continuations within 50 ms after eligibility on that declared host, and
before test-controlled holder release. The 50 ms is an experimental
acceptance threshold, **not** a proposed universal WAIT-2 bound. If host
noise prevents separating candidates, the result is inconclusive, not a
pass or a reason to weaken the semantics.

For the nonusing path, inspect generated lowering and runtime calls first:
zero added watch allocations, cancellation polls, clock samples or context
starts is the structural criterion. Compare ordinary uncontended atomic
operations and an ordinary server path under identical settings. Proposed
cost veto: more than 2% throughput loss relative to base, reproducibly beyond
the interleaved twin's spread, rejects the zero-cost mechanism claim and
requires redesign. New opt-in functionality must separately report
per-wait memory, timer/context counts and contended latency; no numerical
using-path cost is claimed in advance. Embedded evaluation needs an actual
weak-CPU target and a declared resource/energy budget before it can pass;
an x86 result alone never establishes it.

## Proposed rule changes, confidence and unresolved questions

This section records the study's proposal before the owner's direction. The
selected form replaces `cancel &watch` with `until <Instant>`, so the PRE-2
row becomes a timer registration with no watch, and the WAIT-1/WAIT-3 spawn
row is not pursued. The uncertainty items about map withdrawal (7), grammar
(8) and the ordinary-object boundary of `CancelWatch` (13) carry over, the
last as the boundary of `Instant`.

No rule is changed by this document. If selected, the amendment would need
the following behavior changes together, with conformance and diagnostics:

| Rule family | Before | Proposed after |
|---|---|---|
| GRAM-4 / SHARE-2 | One target body and optional guard. | Optional explicit cancellation watch and mandatory cancellation branch; its bindings/ownership are separate. |
| SHARE-3 / WAIT-2 | A begun atomic waits to take effect; false guard has no alternative completion. | A pending cancellable statement may instead withdraw without a target effect; admission/fire race and conditional progress have explicit meanings. |
| WAIT-1 / WAIT-3 / SHARE-2 | Any call to a waiting function in an atomic body is refused, including spawn. | Expression spawn is a nonwaiting start there; direct calls and bound spawns remain refused; activation joins occur after holds are released. |
| Leaving-edge and fact rules | Atomic exits restore invariants; bound-spawn joins follow block rules. | Explicitly account for release-before-activation-join and separate success/cancellation facts on every edge, without weakening invariant or lifetime obligations. |
| PRE-2 | CancelWatch bounds host waits; cancel_fire is an ordinary waiting state update. | The same watch can also end pending atomic acquisition. Firing, source lifetime, stable shared views and host-wait outcomes retain their meanings. |
| PRE-1 / EFF / HOST-1 | DecisionCell arbitration and ordinary rows/order. | No intended change for the recommended pair; no generic DecisionCell or hidden reachable-object effect is introduced. |
| WAIT-2 progress option A | Universal finite-to-wait premise. | A separate pending owner decision; this study identifies the ready-publisher and spinning-acquirer cases it must cover, but does not select new normative progress text. |

**Gap 1 confidence: 4/5 in the semantic direction, 2/5 in the unbuilt runtime
protocol.** It directly supplies the missing no-target outcome, preserves
ordinary atomic bodies, and serves resource-or-cancel generally. An
unresolvable map withdrawal/fairness problem, unacceptable eager-acquisition
cost, or a demonstrated server requirement for several productive arms
could favor G1-C. Existing code inspection does not prove a cancel-aware map
take can satisfy the one-driver capacity premise.

**Gap 2 confidence: 4/5.** Expression spawn transfers a timestamp using an
existing ownership boundary and needs no new payload-publication primitive.
The crucial new obligation is release-before-join, not handle disjointness.
A valid effect-order counterexample, a start path that cannot be made
nonwaiting under holds, a requirement for an exact admission instant, or
unacceptable per-script context cost could overturn it in favor of an
explicit one-shot or timer capability. Its scheduling usefulness still
depends on the separate progress decision.

The complete uncertainty ledger from this inspection is:

1. **Admission versus semantic effect point.** SHARE-3 specifies one atomic
   point, not a source-visible acquisition-complete event. New cancellation
   arbitration must be defined without accidentally serializing all child
   effects before/after the whole parent. The table's proposed semantics
   need a formal trace argument, including child publication of a transferred
   intermediate target-derived value; none was executed here.
2. **Start means what instant?** The supplied requirement says when script
   execution starts after acquiring keys. Is the first clock read enough,
   or must runtime admission be timestamped even if the host preempts before
   that read? G2-A answers the first, G2-C can answer the second. Lazy ordinary
   object groups also mean “all header objects acquired” is not currently a
   public event; the map key/key-set witness does acquire its keys first.
3. **Effect and join rules for new placement.** WAIT-3 starts a context
   immediately and joins at activation exit, but SHARE-2 currently excludes
   this placement. Existing rules therefore do not themselves settle the
   relative order of an activation-exit join and release on every atomic
   leaving edge. The proposal must state it and audit lowering.
4. **Cancellation race priority.** Current PRE-2 defines host-outcome races,
   not atomic admission. Reusing “first committed outcome wins” is a proposed
   choice; cancellation priority whenever both are ready is another. The
   recommendation fixes completed-firing-before-admission behavior and
   permits either order only for concurrent arbitration.
5. **Cancellation state availability.** PRE-2 clearly makes firing waiting;
   it does not promise a nonblocking source update. No guarantee here extends
   to a source whose state another long atomic holds. Internal committed-bit
   observation and registration must preserve that state model.
6. **Progress capacity accounting.** Option A's “nonwaiting contexts” needs
   a precise interpretation for map acquisition that is a source wait but
   spins on an executor. The holder + waiter + publisher witness separates
   an adequate definition from an insufficient one. Spare exhaustion,
   finite publisher work and host scheduling assumptions remain unproved.
7. **Map cancellation withdrawal.** Claims, reservations, borrowed holds,
   table moves, reader counts and whole-map fallback have interacting
   lifetimes. This study names required obligations; it has not designed or
   verified every lock-level cleanup path. No performance or memory bound
   for that implementation is established.
8. **Exact grammar and branch integration.** `cancel` plus a borrowed watch
   and `else` is a concrete proposal. Its addition to delivery completeness,
   diagnostics, fact merging and ownership needs specification work.
   `else` must never be mistaken for “guard false.” All examples remain
   uncompiled, including otherwise-current before examples.
9. **One-shot alternative.** PRE-1 unambiguously has no payload cell. Its
   proposed copy-payload boundary, guard exclusion, publication ordering,
   close semantics and target support are unresolved design choices, not
   inferred current features.
10. **Embedded progress and cost.** The selected hosted runtime has warm
    spare threads; the language's embedded aim does not by itself provide
    such a mechanism. The progress change must preserve the target range,
    with an explicit implementable premise and resource bound. No weak-CPU
    evidence was collected.
11. **Application protocol details.** This study does not establish Firn's
    active-script generation publication, Redis no-op/write classification,
    kill acknowledgment timing, or checkpoint coverage of long interpreter
    operations. DecisionCell establishes a verdict, not completed cleanup.
    Those are validation inputs for the consuming project, not reasons to
    add Firn-specific language rules.
12. **Existing guidance scope.** The checker's suggestion to put a timer's
    cancellation state in an atomic guard is valid for available targets
    and false readiness guards. It does not repair a target held by the
    long computation. If G1-A is selected, that guidance needs this
    distinction; it is recorded here, not changed in this read-only study.
13. **Ordinary-object boundary.** A core statement referring to the nominal
    `std::time::CancelWatch` needs an explicit account under the constitution's
    ordinary-object rule. Recognition must be by the declared ordinary
    capability/type, never by whether its implementation crosses a host
    boundary. This study proposes reuse of its semantics, not a new hidden
    host-origin exception. Whether that warrants relocating the cancellation
    declarations or a more general callable protocol remains a design
    question; adding a protocol solely to disguise the same primitive would
    not settle it.

No implementation, specification approval, progress-option ruling, merge,
or performance claim follows from writing this study.

## Review evidence

A separate read-only reviewer, configured as `gpt-6.1-sol` at high effort,
reviewed the complete untracked document against the checkout identified
above, with the relevant specification, source, examples and checklist.
Checklist A4 and D2 passed; D1 and V3 were not applicable because neither
constitution nor specification changed. Code, test and workflow checklist
groups were outside this prose-only change. No compiler, test, gate or
measurement was run. Example acceptance and the proposed semantics remain
unverified.

One comparison finding was fixed and re-reviewed: general expression spawn
can transfer intermediate target-derived copies to a child, so it shares
the one-shot alternative's publication question. The recommendation now
states that choice explicitly, distinguishes it from reference safety, and
includes a separating effect-order test. The primary author checked that
argument against SHARE-3 and the revised text. The reviewer reported no
remaining material finding within this scope; that result is not a proof
of the proposed language or runtime mechanisms.
