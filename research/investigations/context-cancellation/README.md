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
chosen. The sections below are that work; nothing is chosen yet.

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

## S1 performance prototype

This uncommitted `exp/cancel-proto` implementation is a measurement instrument,
not a selection of S1 or a proposed merge. The question and rejection criterion
above still stand. The source base is
`dcca95c1c8fb6c9f466d676a1d7d63b30f1b5285`; the working-tree changes are also
required to reproduce this prototype. No build, test, or measurement was run
locally, as the prototype request requires CI to build it.

### Interface and lifetime

The actual compiler inputs are `lib/std/time/module.wfm` and
`lib/std/net/module.wfm`, embedded by `compiler/src/library.rs`; PRE-2 carries
identical copies. Only prototype declarations and the needed alias were added
there. Existing specification rules, title, and archives were left unchanged
at the owner's explicit request. Consequently this is not a specification
amendment ready for the ordinary archive/version gate.

`std::time::CancelSource` and `std::time::CancelWatch` are opaque `nodrop`
handles. `cancel_source()` creates unfired state; `cancel_share(source: &source)`
and `cancel_watch(source: &source)` retain it and return separate owned handles
that can move into spawned contexts, following the `clock_share` calling
pattern. `cancel_fire(source: &source)` is idempotent. Close each handle with
`close_cancel_source(source: move source)` or
`close_cancel_watch(watch: move watch)`. Closing does not fire; the last close
reclaims the state. A pending call borrows its watch, so it needs no per-wait
reference-count operation. The never watch is a null state and closes without
reclaiming anything. This explicit lifetime spelling is provisional.

The two additions keep the originals' range contracts and outcome types:

```wf
public fn receive_next_until(receive: &TcpReceive, destination: &[u8], start: u64, end: u64, deadline: Option<Instant>, cancel: &CancelWatch) -> result: Result<u64, ReadStop> reads(cancel), writes(receive), writes(destination) waits contract {
  requires start <= end;
  requires end <= destination^.len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Prototype: cancellation uses DeadlinePassed, with nothing transferred, when it wins the race with host completion.";
public fn tcp_accept_until(factory: &HandleFactory, listener: &TcpListener, deadline: Option<Instant>, cancel: &CancelWatch) -> result: Result<AcceptedConnection, IoError> reads(cancel), writes(factory), writes(listener) waits doc "Prototype: cancellation uses DeadlinePassed, with no connection accepted, when it wins the race with host completion.";
```

These are signature excerpts using their modules' aliases, not standalone
modules. The interface records own the complete declarations and documentation.
A fired watch deliberately returns `DeadlinePassed` without a clock deadline
having elapsed; that is this prototype's shortcut, not a change to ordinary
`receive_next` or `tcp_accept`. It deliberately departs from the unchanged
PRE-2 prose that permits `DeadlinePassed` only after clock expiry; no language
conformance claim is made. A host outcome racing the cancellation can
still win, as on the existing deadline path.

### Driver mechanism and costs to measure

A real watch registers on its driver's intrusive list in the context record
before dispatch and reads `fired` after linking. The context holds the pending
native record, so the native completion record's reservation stays unchanged.
The submitting driver cannot scan its list until the start and park have
returned to its loop. A pre-fired watch completes without host submission;
a fire between the read and park is found by that driver's pending scan.

Firing atomically sets `fired`, then, under the existing driver-notification
lifetime protocol, marks every driver's atomic `cancel_pending` flag and
raises its wake epoch. The pending-flag mark is an acquire/release exchange,
so concurrent firings of different sources preserve each other's publication
when their notifications coalesce. Each driver scans only its own list, unlinks fired
entries, and calls the route action extracted from `wf_driver_expire`.
An in-flight host operation still owns its one terminal publication; helper
cancellation retries use the existing timer path. Normal completion removes
registration before making the context ready, so another driver cannot steal
a context that is still linked to the old driver. The loop checks cancellation
again after capturing the wake epoch, closing the notification-before-park
window. The list and links are never touched by the firing thread.

A watch without a clock deadline uses a sentinel in the context's existing
`timer_slot`, allowing the existing no-bookkeeping cleanup return to stay
unchanged for ordinary unbounded waits. A real watch also saves the original
optional clock deadline. Its native record uses a far-future internal bound
when that deadline is absent, so helper routing stays interruptible without
inserting a clock timer. The source is only one-shot; no reset, guard access,
or watched send/sleep operation is added.

With `cancel_never()` and `None<Instant>()`, the new start checks the watch's
null pointer once, then delegates to the original start. It performs no list
registration, watch atomic read, retain/release, or timer insertion. Its
finish delegates to the original finish. This is a source-path statement,
not measured machine-code cost: every driver also has a pending-flag check
at reap and before park, contexts have additional list fields, and the
completion-before-park race has an additional cleanup call. Those common
costs belong in the whole-build comparison. A never watch with a real clock
deadline still pays deadline bookkeeping.

For a real watch, Linux/Windows native completion routing remains first.
Without a native route the prototype uses the existing interruptible helpers;
a condition-variable wake cannot interrupt the existing descriptor-only
readiness poll. This may cost a helper per blocked operation. Windows helper
accept retains its existing bounded polling, so the one-driver-round stop
criterion is not established for that route. This fallback cost is recorded
here for qualification, not hidden by a new polling stop mechanism. The
existing bridge-split TODO in `docs/todo.md` remains deferred; splitting the
runtime is outside this measurement prototype.

Measure the explicitly never watch and a real watch kept unfired as distinct
configurations: only the latter exercises registration and can later stop the
server. Neither configuration has been measured. No final shape is selected.

### Runtime evidence prepared for CI

`compiler/src/backend/completion/cancel_test.c` is a runtime-level C probe in
the style of the existing shared-object tests, rather than a full WF network
program. It includes the private bridge and ordinary library bodies to stage
specific interleavings without production test hooks. A separate pthread
stands for the firing driver's context; real socketpair receives use a peer
that stays open and sends nothing during cancellation. A real loopback TCP
listener covers watched accept and factory-credit return.

The probe checks cross-driver notification, a fire before native sleep, a
fire after registration but before context park, pre-fired watches, repeated
fire, source/watch independent lifetimes, normal watched completion, clock
expiry, empty completion before park, and a never watch receiving a byte the
pre-fired watch left untouched. It checks exactly one ready-queue insertion,
empty watch/timer bookkeeping after completion, and unchanged destination
bytes on `DeadlinePassed`. Its watchdog fails a hang; it is never the waited
operation's deadline. The early-cancellation publication-count assertion
checks that the statistics also count that completion once.

The existing `completion-test` and `completion-test-images` targets include
this probe. The focused CI command is `make -C compiler cancel-test`; it runs
the available native route and the forced helper route with helpers initially
pinned at zero. No new timed CI stage is added. All compilation, LLVM linking,
WF acceptance, native/helper execution, sanitizer results, Windows
qualification, and performance remain unverified. No test mutation or
before/after execution was possible under the no-local-tests instruction.

Read-only review found and locally repaired a probe teardown race (drain
notifiers before destroying its extra driver) and double-counted immediate
cancellation statistics (let the common completion publisher count it once).
Neither repair selects a design. The existing dispatcher-order inspection
case follows the renamed common dispatcher, keeping its native-before-helper
assertion unchanged. Review also corrected the illustrative unit return and
lifted constructor calls out of call arguments to follow WF's flat-argument
grammar. Local inspection changed pending-flag marks to acquire/release RMWs
so coalesced concurrent firings publish every source to the scan. There is no
approval or merge claim.

Read-only review completed on 2026-10-08 at 21:27 UTC against the base above
and this uncommitted diff, including the untracked C probe and this appendix,
by a separate agent on the inherited model. It covered the repository,
documentation, code, test, validation, design-fit and correspondence checklist
groups; constitution changes and design-node form were not applicable. The
fixed findings and the coalesced-firing repair were re-inspected with no open
findings. Its checks were file reads and Git inspection only. Whitespace
inspection with `git diff --check` also passed; it is not execution evidence.

### Firn spelling

These helpers illustrate the ownership needed around an accept. They use the
usual aliases below and are source examples awaiting CI compilation:

```wf
alias HandleFactory = std::io::HandleFactory;
alias TcpListener = std::net::TcpListener;
alias AcceptedConnection = std::net::AcceptedConnection;
alias IoError = std::io::IoError;
alias Instant = std::time::Instant;
alias CancelSource = std::time::CancelSource;
alias CancelWatch = std::time::CancelWatch;

fn accept_one(factory: HandleFactory, listener: TcpListener, watch: CancelWatch) -> result: Result<AcceptedConnection, IoError> pure waits {
  let unbounded = None<Instant>();
  let accepted = std::net::tcp_accept_until(factory: &factory, listener: &listener, deadline: unbounded, cancel: &watch);
  std::time::close_cancel_watch(watch: move watch);
  std::net::close_listener(factory: &factory, listener: move listener);
  return move accepted;
}

fn fire_stop(source: CancelSource) -> result: unit pure waits {
  std::time::cancel_fire(source: &source);
  std::time::close_cancel_source(source: move source);
  return unit;
}
```

Ten-line caller fragment inside a waiting function returning
`Result<AcceptedConnection, IoError>`, with owned `factory` and `listener`:

```wf
let source = std::time::cancel_source();
let watch = std::time::cancel_watch(source: &source);
let stopper = std::time::cancel_share(source: &source);
let accepting_factory = std::io::factory_share(factory: &factory);
let accepting = spawn accept_one(factory: move accepting_factory, listener: move listener, watch: move watch);
let firing = spawn fire_stop(source: move stopper);
std::time::close_cancel_source(source: move source);
let fired = firing;
let outcome = move accepting;
return move outcome;
```

For receives, first bind `let unbounded = None<Instant>();`, then pass the
same watch to
`std::net::receive_next_until(receive: &receive, destination: &bytes[0_u64..1_u64], start: 0_u64, end: 1_u64, deadline: unbounded, cancel: &watch)`;
the `ReadStop::ReadFailed` payload holds `IoError::DeadlinePassed` when
cancellation wins. For a wait that must ignore firing, create
`let never = std::time::cancel_never();`, pass `cancel: &never`, then consume
it with `std::time::close_cancel_watch(watch: move never)` after the wait.
