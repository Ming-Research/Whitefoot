# Stop signals

## The question

A Whitefoot program cannot learn that its host asks it to stop. A service
manager stops a server with SIGTERM, a terminal with SIGINT, and Windows
with a console control event; the host's default applies, since no part
of the standard library intercepts them. On POSIX, the signal's default
action ends the program. firn, the Redis-compatible server
written in Whitefoot, loses the changes its append-only file's writer has
not yet appended, up to one 10-millisecond cycle, and the bytes not yet
synced. Redis 7.0.15 instead treats SIGTERM and SIGINT as a request to shut
down (`src/server.c`, `sigShutdownHandler` setting `shutdown_asap`, which
`serverCron` turns into `prepareForShutdown`): it stops accepting, flushes
and syncs its append-only file, and exits. firn already stops this way on
its `SHUTDOWN` command (Firn-wf `design/firn/orderly-stop.md`); it lacks only
the signal.

The question is how a program receives a host's request to stop, and what
happens to the request while the program does not ask for it.

## What a program needs

- To wait for the next stop request in a context of its own, as it waits
  for a connection or a byte, and then start its own orderly stop.
- To tell an interrupt (SIGINT, Ctrl-C) from a termination request
  (SIGTERM, a console close or a system shutdown), as Redis logs them
  apart; both stop Redis the same way.
- A program that never asks to be told keeps the host's default; on POSIX,
  the signal's default action ends it, as today.
- A second request while the first is being handled: Redis exits at once on
  a second SIGINT during a shutdown (`sigShutdownHandler`, "You insist...");
  a program can do the same by closing the listener after the first request,
  restoring the host's default, or keep it open and wait again to decide.

## Constraints from earlier decisions

- Host access is a parameter the function receives, never ambient
  (`design/language/system-interface.md`, the rejected "ambient mutable
  host access"); a capability of the invocation reaches the program through
  `Inputs`, as the clocks do (`system-interface/clocks.md`).
- An owner of a native resource is linear and closes through an explicit
  consuming function (`system-interface.md`).
- Waiting is declared (`waits`), and a waiting host call completes once the
  host has produced its outcome, an input of the execution [WAIT-2]; a
  bounded wait takes `deadline: Option<Instant>` [PRE-2].

## Hosts

- **Linux.** `signalfd` on a mask of SIGTERM and SIGINT blocked in every
  thread delivers them as reads of a descriptor, which the ring can wait on
  like any other read; unblocking the mask and closing the descriptor
  restores the default.
- **macOS.** `kqueue` with `EVFILT_SIGNAL` reports a signal's arrival while
  its disposition is set to ignore, so the default no longer ends the
  process; restoring `SIG_DFL` restores it.
- **Windows.** `SetConsoleCtrlHandler` receives `CTRL_C_EVENT` (an
  interrupt) and `CTRL_BREAK_EVENT`, `CTRL_CLOSE_EVENT`, `CTRL_SHUTDOWN_EVENT`
  (terminations) on a thread of its own; the handler posts the event to the
  completion port a waiting context parks on, and removing the handler
  restores the default. For `CTRL_C_EVENT` and `CTRL_BREAK_EVENT`, returning
  TRUE lets the process continue. For a close, logoff or shutdown event the
  system ends the process as soon as the handler returns, and after its
  grace period in any case
  ([HandlerRoutine](https://learn.microsoft.com/en-us/windows/console/handlerroutine)),
  so the runtime's handler returns only when the listener closes or the
  process ends: the program's stop then runs within the grace period.

## Candidates

- **A. A capability in `Inputs` and a waiting listener.** `Inputs` gains
  `stops: StopSignals`, an opaque capability like `Clock`. `stop_listen`
  opens a linear `StopListener` from it; while a listener is open the
  runtime intercepts the requests, and `stop_next(factory, listener,
  deadline)` waits for the next one and returns `Interrupt` or `Terminate`.
  Closing the listener restores the default. A program that never opens a
  listener behaves as today.
- **B. Requests mapped onto shared state.** The runtime sets a flag in a
  shared object the program names, and the program polls it. This needs no
  waiting call, but every context that should react must poll, which is the
  stopgap firn already uses for `SHUTDOWN` and which Firn-wf's `docs/todo.md`
  records as a gap; it also needs a runtime write into program state.
- **C. Requests as an ending input stream.** The runtime closes stdin or
  another stream on a request. This hides a stop request behind an
  unrelated resource and cannot tell an interrupt from a termination.
- **D. A handler function the runtime calls.** The runtime calls a program
  function on a request. It needs a context the program did not start and
  code that runs at an arbitrary point, which the waiting model excludes.

## Proposal

A. Its parts:
- `Inputs.stops: StopSignals` (opaque, `nocopy`, drop empty), a
  capability of the invocation that grants nothing until a listener opens.
- `stop_listen(factory, stops: &StopSignals) -> Result<StopListener,
  IoError>` spends a handle credit [PRE-2] and starts interception; a
  second listener while one is open is refused, since each request is
  delivered once.
- `stop_next(factory, listener: &StopListener, deadline:
  Option<Instant>) -> Result<StopKind, IoError> waits` returns the next
  request, `Interrupt()` or `Terminate()`, in the order the runtime
  observed them; requests observed while no context waits are kept. The
  hosts merge requests that arrive before the runtime observes them: POSIX
  keeps one pending instance of a standard signal (`signal(7)`), and a
  kqueue `EVFILT_SIGNAL` event carries a count, not the interleaving of two
  signals. A program therefore learns that a stop was requested, and of
  which kind, not how many times.
- `close_stop_listener(factory, listener: StopListener)` restores the
  default and returns the credit.

Validation, stated before implementing:
- a program test per host that opens a listener, receives a termination
  sent by the test harness, writes a byte to stdout and exits 0, and the
  same program without a listener ended by the request with the host's
  default status;
- a request sent before the program waits is delivered, and an interrupt
  sent after a termination's delivery is delivered after it;
- an interrupt and a termination are told apart where the host can send
  both (POSIX; Windows `CTRL_C_EVENT` beside `CTRL_BREAK_EVENT`);
- firn's `SIGTERM` stops it as `SHUTDOWN` does, every acknowledged write
  replayed after the restart.

The proposal would be rejected if a host cannot deliver a request to a
waiting context without running program code in a signal handler, or
cannot restore its default when the listener closes.

## Startup cost on AMD hosts

compute-regression compares this branch's images with its merge base's on a
hosted runner. On three runs whose host was AMD (EPYC 7763 twice, EPYC 9V74
once) stencil was slower at two or three widths (wall ratio 0.85 to 0.93,
every pair lower), while each host's identical-image and placement controls
passed; the runs on Intel Xeon hosts and the interleaved comparison on the
i9-14900K (Whitefoot run 37748947950: 0.994, 1.004, 1.011) showed no
slowdown. On Linux the branch starts a receiver thread in every program at
launch, before the entry, whether or not the program ever listens.

Question, stated before measuring: is the receiver thread the cause? The
comparison builds three image sets from one compiler pair: the merge base
(B), this branch (S), and this branch with `wf__stop_initialize` returning
before it starts the receiver (N); it runs `tests/performance/compare.sh`
for B against S and S against N on several hosted jobs and keeps the jobs
whose host is AMD. If the thread is the cause, B against S fails as before
and S against N shows N faster by about as much; if S against N shows no
difference while B against S still fails, the thread is not the cause and
the next suspect is the placement of data the launch allocates.

Result (Whitefoot run 37761168172, a temporary workflow on branch
`claude/stop-probe` at 624f496c6, six hosted jobs, all of which landed on AMD
hosts: EPYC 7763 four times, EPYC 9V74 twice; each job's identical-image
control passed). stencil's wall ratios, width 1 / 2 / 4:

| job | host | B to S | S to N | B to N |
|---|---|---|---|---|
| 1 | 7763 | 0.79 / 0.99 / 0.98 (suspect) | 1.28 / 1.09 / 1.04 | 1.00 / 0.97 / 0.96 |
| 2 | 9V74 | 0.82 / 0.92 / 0.97 (fail) | 1.26 / 1.10 / 1.11 | 1.01 / 1.00 / 1.01 |
| 3 | 9V74 | 0.79 / 0.82 / 0.90 (fail) | 1.26 / 1.21 / 1.04 | 0.98 / 0.99 / 1.01 |
| 4 | 7763 | 0.89 / 0.96 / 0.96 (fail) | 1.40 / 1.26 / 1.04 | 1.00 / 0.99 / 0.99 |
| 5 | 7763 | 0.81 / 0.92 / 1.04 (fail) | 1.06 / 1.08 / 1.10 | 1.03 / 1.03 / 0.98 |
| 6 | 7763 | 0.72 / 0.91 / 0.95 (fail) | 1.10 / 1.08 / 1.01 | 0.99 / 1.01 / 0.99 |

S to N recovers what B to S loses in every job, and B to N passes in every
job, so the receiver thread is the cause on these hosts. (Job 6's S to N
verdict failed on records at widths 2 and 4, 0.94 and 0.97, which no other
job shows.) The thread itself is idle in `poll`; it is created before the
floor creates the entry thread with its declared stack, so the entry's stack
and what follows it are mapped at other addresses than on the merge base.
Whether the cost is that placement or the thread's existence is not
separated here; a remedy that starts no thread before the entry removes
both.

## Status

Adopted by the owner: proposal A, with requests returned in runtime observation order and requests the host merged before observation counted once, and the Windows close, logoff and shutdown handlers held until their listener closes; specified in PRE-2, whose approval [`spec/log.md`](../../../spec/log.md) records.
