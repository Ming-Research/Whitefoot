# Signal delivery

How the runtime receives the host's stop requests and hands them, in order, to a program's stop listener.

### decision stop-requests-queue-without-loss
Stop requests enter a growing FIFO in the runtime pool, and the same queue lock completes a waiting record or removes a timed-out one.
- rejected: an agent proposed a bounded queue that drops requests silently; the owner rejected it, because requests seen between calls must keep their order, and a finite queue would lose that history.

### decision launcher-thread-receives-stop-signals
On POSIX the launcher's original thread receives stop requests while the entry runs on its own thread; on Linux it reads them from a signalfd that every other thread has blocked.
- rejected: an agent proposed a receiver thread created at start; the owner rejected it, because it slowed the width-one stencil by 11 to 28 percent on AMD hosts, and removing it restored the speed (data: `research/investigations/stop-signals/README.md`).
- rejected: an agent proposed keeping the receiver thread but creating it after the entry thread; the owner rejected it, because every program would still carry one more thread, and nothing showed that the thread's place, not the thread, was the cause.
- rejected: an agent proposed a receiver thread created at the first listen; the owner rejected it, because earlier threads would either leave the signals unblocked and defeat signalfd, or block them with nobody applying the default action.

### decision entry-thread-failure-stops-startup
On POSIX a failure to create the entry thread stops startup with a diagnostic, and a native caller with no launcher loop gets ENOTSUP when it opens a listener.
- rejected: an agent proposed running the entry on the launcher's thread when the entry thread cannot be created; the owner rejected it, because no thread would then receive stop requests.
- rejected: an agent proposed creating a substitute receiver thread for a native caller with no launcher loop; the owner rejected it, because a native caller cannot block the signals in its threads that already run.

### decision stop-requests-in-observation-order
Stop requests reach the program in the order the runtime observes them, and Windows holds its close, logoff and shutdown callbacks until their listener closes.
- rejected: an agent proposed giving requests in the order their senders sent them; the owner rejected it, because the host may merge signals before the runtime sees them, so that order cannot be rebuilt.
- rejected: an agent proposed returning from the Windows close, logoff and shutdown callbacks at once; the owner rejected it, because Windows ends the process when they return, before the program can stop.
