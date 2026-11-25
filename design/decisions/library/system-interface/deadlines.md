# Deadlines

How a host operation that waits on another party is bounded in time.

### decision peer-waits-take-optional-deadline
Each host operation that may wait on another party takes `deadline: Option<Instant>` before its cancellation watch, and a passed deadline with nothing transferred is `IoError::DeadlinePassed()`.
- rejected: an agent proposed a second, deadline-taking copy of each waiting operation; the owner rejected it, because it doubles the I/O surface for what one optional parameter states.
- rejected: an agent proposed a race between a timer and a pending operation whose winner cancels the other; the owner rejected it, because the language has no construct that races two waits, and one would need cancellation semantics of its own.
- rejected: an agent proposed reporting a passed deadline as `TimedOut` with an origin kept for deadlines; the owner rejected it, because it makes the program's own bound and a host's timeout one variant told apart by a number.
- rejected: an agent proposed a deadline on file operations; the owner rejected it, because they wait on the host's storage, not on a party that may never answer.