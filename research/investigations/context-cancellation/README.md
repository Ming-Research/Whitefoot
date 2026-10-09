# Ending another context's wait

## The question

A context that waits for a host operation, a deadline or a guarded atomic
statement can be ended only by that operation's outcome, its deadline or
the guard becoming true. No context can end another's wait. A program that
must stop in order, such as a server on `SHUTDOWN` or a termination
request, needs every serving context to leave its receive, send, accept or
sleep, close its handles and return, so that the entry's joins complete
[WAIT-3].

firn does this today by polling: every socket wait carries a deadline of at
most a second, and each context reads a shared stop request when the
deadline passes (Firn-wf `design/firn/orderly-stop.md`). Measured on the
i9-14900K at pipeline depth 1, the receive deadline costs at most 1.25%
(Firn-wf `research/investigations/orderly-stop`). The owner called it a
stopgap: polling reaches only waits that take a deadline and contexts that
come back to their check, so it cannot end a guarded atomic statement
waiting for a state that never comes, or a host operation without a
deadline, and it delays every stop by up to the polling period.

The question is how one context ends the waits of others, and what the
ended waits observe.

## Earlier positions

- The owner chose a deadline parameter as the one bounded cancellation
  (`research/investigations/io-model/TIME-AND-FILES.md`, Q24), noting that
  a construct racing two waits needs a cancellation semantics of its own
  and, if one comes, subsumes the parameter without contradicting it.
- Cancellation is available only when an API returns or accepts ordinary
  owned cancellation state, and the target defines the race between
  cancellation and completion
  (`research/investigations/io-model/FIRST-PRINCIPLES.md` §17.5).
- A deadline ends a wait through the runtime's per-driver deadline heap,
  which asks the operation's route to cancel it (`IORING_OP_ASYNC_CANCEL`,
  `CancelIoEx`, a helper thread's cancellation) and completes it with its
  own outcome or with `DeadlinePassed`, transferring nothing
  (`compiler/src/backend/completion/bridge.c`, the deadlines section). A
  second way to end a wait can reuse that path.

## Candidates

- **A. Owned cancellation state accepted by waiting calls.** A shared
  cancellation object, fired through a source handle and observed through
  watch handles that contexts receive like any other argument. A waiting
  host call that takes a watch ends with `Cancelled`, transferring nothing,
  when the watch's object has fired before or during the wait; an atomic
  statement's guard can read whether it has fired, so a statement waiting
  for a state also wakes. It follows §17.5 and reuses the deadline path.
  Open: whether the watch joins the deadline in one bound parameter or is
  a second parameter, and which calls take it.
- **B. Cancel scopes on spawns.** A scope's cancellation ends every wait of
  every context started within it, as Trio's cancel scopes do. Every
  waiting call may then end with `Cancelled` whether or not it names the
  scope, so every waiting function's outcome gains that case, and which
  waits a cancellation reaches depends on where they were spawned rather
  than on what they were given.
- **C. A construct that races waits.** A statement waits for the first of
  several waits and cancels the rest. It needs the cancellation semantics
  of its own that the deadline decision named, and a stop then needs every
  wait to race a stop wait.
- **D. Closing a resource from another context.** A closer handle derived
  from a connection or listener ends the waits on it, as closing a socket
  does in other systems. It reaches only network waits, not sleeps,
  atomic statements or file operations.
- **E. Polling, the stopgap.** No change; the cost and limits above stay.

## Direction

The owner selected A (Firn ledger Q223) and asked that its shape weigh each
alternative's costs and benefits and keep performance in view before it is
chosen. The sections below record that comparison; the Decision section records the selection.

## The mechanism under every shape

Two parts of the runtime already do most of what A needs
(`compiler/src/backend/completion/bridge.c`):

- **Guard watches.** A statement whose guard reads false registers a watch
  on every unit its guard read, then parks; a statement that writes a unit
  wakes every watch on it. If the cancellation state is a shared object, a
  guard that reads it is woken by the cancellation with no new machinery.
- **Ending a wait at its deadline.** The driver marks the record fired and
  asks the operation's route to give up (`wf_driver_expire`); the operation
  completes with its own outcome or with nothing transferred.

What is new is finding the host waits to end when a cancellation fires.
Two designs:

- **Register at park.** A wait that names a watch puts its record on the
  cancellation object's list, under a lock, and takes it off when it
  completes. Firing walks the list. Every parked wait pays a lock and two
  list writes, and the lock is shared by every driver.
- **Find at fire.** A wait that names a watch stores the watch in its record
  and links the record into its own driver's list of watched waits, which
  only that driver touches, then reads whether the watch has fired, so a
  wait that parks after a firing ends at once. Firing marks the object
  fired and wakes every driver; each walks its own list and ends the
  matching waits through the deadline path. A parked wait pays two pointer
  writes and one atomic read; the cost moves to the firing, which is rare.

Find at fire is the one to build: a server parks a wait per request at
pipeline depth 1, and the firing happens once, at the stop. Today's polling
pays more on that path: a clock reading, a deadline computation, and a
timer-heap insertion and removal per wait.

## Shapes

Each shape below says how a wait names its cancellation, what the writer
writes, and what it costs.

- **S1. A watch parameter on each waiting host function.** `receive_next(...,
  deadline, cancel: &CancelWatch)`; a never-firing watch,
  `cancel_never()`, for a wait that must not be cancelled.
  - For: each wait shows whether a stop can end it; a wait that must finish,
    such as the append-only file's last flush, passes the never-firing
    watch; nothing is implicit.
  - Against: every waiting host function gains a parameter; a function that
    waits on a caller's behalf must take the watch to pass it on, so the
    watch is threaded through each layer between the decision to stop and
    the wait.
- **S2. One bound value replacing the deadline.** `until: Until`, a copy
  value of a deadline and a watch, built by `until_deadline(d)`,
  `until_cancel(w)` or both; the watch is then a copy value naming a
  cancellation object.
  - For: one parameter states every reason a wait may end early, so the
    deadline and the cancellation share one rule, one outcome type and one
    runtime path; existing deadline uses change spelling, not shape.
  - Against: a copy value naming a runtime object needs a rule for an object
    released while copies remain (they would then never fire); the outcome
    must tell the reasons apart, `DeadlinePassed` or `Cancelled`.
- **S3. A watch set on the context.** `cancel_scope(watch)` makes every later
  wait of the context end on that watch.
  - For: no waiting signature changes.
  - Against: ambient state that no parameter shows, which
    `design/language/system-interface.md` refuses for host access; a wait
    that must not be cancelled needs an escape; whether spawned contexts
    inherit the watch is a further rule.
- **S4. A watch bound into a handle.** A receive half, listener or similar
  handle is made cancellable once, and every wait through it honors the
  watch.
  - For: set once per resource, no per-call argument.
  - Against: sleeps and atomic statements have no handle; one handle used by
    a wait that may be cancelled and one that may not needs two bindings.

Atomic statements are the same under every shape: the cancellation state is
a shared object a guard reads as one of its targets, so
`atomic s = &state, c = &stop when bor(s^.ready, c^.fired)` wakes on either.

## What to measure before choosing

The mechanism's cost is mostly independent of the shape, so one prototype
of find at fire, behind S1 or S2, answers the performance question for all.
The rule, stated before measuring: on firn's redis-bench `set` and `get` at
pipeline depths 1 and 16, on one and two server CPUs, interleaved with a
twin of each build, the build whose receives name a never-firing watch and
carry no deadline loses no more than the twin's spread against the same
build with neither; and it is not slower than today's polling build beyond
that spread. A stop under load ends every context within one driver round,
measured from the firing to the last join.

The shape is then chosen on the writer's terms above, with the prototype's
spelling in firn as the worked example, and comes to the owner as a card.

## Results

The prototype is Whitefoot branch `exp/cancel-proto` (find at fire behind
S1: `CancelSource`, `CancelWatch`, `cancel_never` in `std::time`,
`receive_next_until` and `tcp_accept_until` in `std::net`, a fired watch
ending the wait as `DeadlinePassed`), released as `wf-exp-f5602b2240e4`.
Four firn builds on that one compiler, Firn-wf branches `exp/cancel-firn-*`:
`poll`, today's firn (one-second receive and accept deadlines, a 100 ms
signal wait); `watch`, receives and the accept loop name a watch the stop
signal fires, with no deadline unless an idle limit is set, and the signal
wait has none; `never`, receives name `cancel_never()`; `none`, receives
carry neither.

Throughput (Firn-wf run
[37873234213](https://github.com/Ming-Research/Firn-wf/actions/runs/37873234213),
i9-14900K, redis-benchmark `set` and `get` at depths 16 and 1 on one and two
server CPUs, 4 interleaved passes of 10 s, `none` measured twice as the
noise control). The twin differed from `none` by 0 to 3.2%.

- `watch` against `poll`: 0.984 to 1.022 in all eight cells, inside the
  noise. The second criterion holds.
- `never` against `none`: inside the noise in seven cells; at one CPU,
  depth 16, `get` it is 0.950. In that cell `poll`, `watch` and `never`
  all ran at 1.83 to 1.93 million requests a second and `none` alone at
  1.98 to 2.01 million, so the gap separates a receive carrying any bound
  from one carrying none; today's polling already pays it. The first
  criterion fails in that cell, for that reason.

Stopping (Firn-wf run
[37876555665](https://github.com/Ming-Research/Firn-wf/actions/runs/37876555665),
i9-14900K, 10 trials per line, 50 client connections confirmed connected
before each SIGTERM, busy with redis-benchmark `set` or idle). Time from
SIGTERM to the process's exit:

| build | busy | idle |
|---|---|---|
| poll | 1,045 to 1,098 ms, median 1,046 | 1,053 to 1,058 ms, median 1,057 |
| watch | 45 to 97 ms, median 45 | 50 to 53 ms, median 51 |

The watch build stops about twenty times sooner. Its remaining 45 to 50 ms
match the expiry context's 100 ms sleep, which the prototype leaves
unwatched; a `sleep_until` that names a watch would remove it. The first
measurement (run 37872419934) is not used: its idle clients were not
confirmed connected and both builds then waited on a 100 ms signal poll.

## Decision

The owner chose S1 on board card `firn-cancel-shape`: an explicit watch on
all host waits that already take a deadline, and on `sleep_until`, with
`cancel_never` for a must-finish wait. The implementation reuses find at
fire, retains explicit source/watch ownership and distinguishes `Cancelled`
from `DeadlinePassed`. File-system operations remain outside this change;
no cancellation query or atomic-guard integration is added. The
[decision node](../../../design/language/system-interface/context-cancellation.md)
records the shape, mechanism and rejected alternatives. The Results above
support parity with polling on the measured firn workload, not zero cost
against unbounded receives in every cell.
