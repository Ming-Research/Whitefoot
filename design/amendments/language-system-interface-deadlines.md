Node: language/system-interface/deadlines

Decision: Each host operation that may wait on another party, `read_next`, `write_once`, `tcp_accept`, `tcp_connect`, `receive_next` and `send_once`, takes a last parameter `deadline: Option<Instant>`, whose passing is the `IoError` variant `DeadlinePassed()`, produced only once the monotonic clock has reached the deadline and only when the operation transferred nothing, an outcome the host produces as the deadline is reached being reported instead, while file operations take no deadline, because one optional parameter bounds a wait without a second copy of each operation or a construct that races two waits, and the program's own bound is not a host's timeout (owner's rulings Q24 and Q29), instead of a deadline-taking copy of each operation, a race between a timer and a pending operation, or `TimedOut` with a reserved origin ([design](../../../research/investigations/io-model/TIME-AND-FILES.md#deadlines-on-the-operations-that-wait-on-a-peer)).

Rejected:
- A second, deadline-taking copy of each waiting operation: rejected by the owner because it doubles the I/O surface for what one optional parameter states.
- A race between a timer and a pending operation whose winner cancels the other: rejected because the language has no construct that races two waits and one needs cancellation semantics of its own; the deadline parameter is the bounded cancellation expressible without it.
- Reporting a passed deadline as `TimedOut` with an origin reserved for deadlines: rejected because it makes the program's own bound and a host's timeout one variant told apart by a number.
- A deadline on file operations: rejected because they wait on the host's storage, not on a party that may never answer.
