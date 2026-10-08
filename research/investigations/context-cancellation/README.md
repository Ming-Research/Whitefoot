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

## Proposal

A, with its shape (one bound parameter or a separate watch, and the calls
that take it) designed and measured before it reaches the specification.
Its validation, stated before implementing: firn's stop with no polling
deadline on its receives ends every context within one driver round of
the cancellation; a wait that completed before the cancellation keeps its
outcome; the throughput of firn at pipeline depth 1 loses nothing against
the build without the watch beyond the twin's spread on the measuring
machine.

The proposal would be rejected if a cancelled wait on some host route can
neither be cancelled nor completed without transferring data the program
does not observe.

## Status

Proposed to the owner as Firn ledger Q223.
