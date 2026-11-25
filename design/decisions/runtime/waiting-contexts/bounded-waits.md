# Bounded waits

How the runtime ends a wait at its deadline or on a cancellation, and how a driver's due work is serviced while its thread computes.

### decision each-driver-keeps-a-deadline-heap
Each driver keeps the deadlines of the contexts waiting on it in a binary heap and bounds every park and poll by the earliest one.
- rejected: an agent proposed one timer thread for the whole program; the owner rejected it, because every expiry would cross to the driver that owns the waiting context, whose own loop already wakes at the earliest instant, and it would leave a computing driver's other parked contexts unserviced.
- rejected: an agent proposed a linked timeout on each ring entry; the owner rejected it, because the readiness and helper routes need the driver's bound anyway, so the ring would carry a second mechanism for the same deadline.

### decision an-idle-driver-borrows-a-computing-driver
An idle driver may borrow a driver whose thread runs a context, for one bounded pass over its due sleeps and finished records; with several drivers, an idle one parks at most 1 ms.
- rejected: an agent proposed a service thread per driver that never runs a context, so that no driver is ever handed over; the owner rejected it, because every wake would then cross threads, the ordinary path might be slower than a handoff, and it is a larger change of the architecture.

### decision a-spare-thread-takes-an-overdue-driver
When a driver stays outside past a due deadline and no idle driver can borrow it, a monitor thread hands its whole role to a spare from one pool of one spare per driver.
- rejected: an agent proposed a spare reserved to each driver; the owner rejected it, because one driver's spare would then sit idle while another driver's second computation waits.
- rejected: an agent proposed a library wait function, such as `yield_now`, that the writer puts in long loops; the owner rejected it, because it fits only in waiting functions, cannot cover pure functions or compiler-generated cleanup, and a missed call stalls silently.
- rejected: an agent proposed a form that runs a pure function on a compute thread, its join being a wait point; the owner rejected it, because it needs WAIT-3 extended and covers only the computation the writer marks, not cleanup.

### decision cancellation-goes-through-the-operations-route
A cancellation goes through the route that holds the operation: a ring cancel, `CancelIoEx`, or a repeated interrupt of the helper, with Windows helper sends and accepts on bounded polls.
- rejected: an agent proposed polling for readiness before each blocking helper call; the owner rejected it, because a standard stream shared with other processes can be ready at the poll and empty at the call, and a poll does not bound a large socket send.

### decision a-bounded-operation-stays-off-the-driver-thread
An operation with a deadline never runs on the driver thread: a socket transfer waits for readiness, and any other goes to the helpers and switches the program to the helper policy.
- rejected: an agent proposed sending only that one operation to a helper that retires after it; the owner rejected it, because that needs a helper pool that shrinks.

### decision a-watched-wait-keeps-its-reason-in-the-record
A watched wait joins its driver's list, and its completion reason is chosen on the owning driver and kept in the record's file-open diagnostic word.
- rejected: an agent proposed enlarging every completion record, or keeping a second clock value for cancellation; the owner rejected it, because file opens take no watch, so the two request kinds never meet and the existing 160-byte record suffices.
