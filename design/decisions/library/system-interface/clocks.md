# Clocks

The clocks the entry receives, the instants they read, and the local time offset.

### decision entry-gets-monotonic-and-wall-clocks
The entry receives a `Clock` for monotonic time and a `WallClock` for calendar time in Inputs, and a monotonic reading is an `Instant` of `std::time` that only host functions form.
- rejected: an agent proposed one `Clock` with both a monotonic and a calendar read; the owner rejected it, because every function that bounds a wait would then also reach the settable calendar.
- rejected: an agent proposed reaching a clock through the handle factory; the owner rejected it, because the factory is the budget of host handles, and a clock is a capability of the invocation that the entry receives like its others.
- rejected: an agent proposed a public integer instant; the owner rejected it, because it is forgeable, which turns `sleep_until` and every deadline into access to time without a clock.
- rejected: an agent proposed a `nocopy` `Instant` moved into every deadline; the owner rejected it, because a deadline computed once is passed to many calls.

### decision instant-operations-are-total
`now` writes its clock, and every `Instant` operation is total: `instant_after` saturates at the latest instant and `nanoseconds_from` answers zero for a pair out of order.
- rejected: an agent proposed a clock read that only reads its clock and promises no order; the owner rejected it, because two adjacent reads may then overlap, so a read could return an instant before the previous one.
- rejected: an agent proposed a subtraction that requires its operands in order; the owner rejected it, because no fact source proves the clock's order, so the caller would hold an obligation no proof discharges.

### decision sleep-and-deadlines-take-instants
`sleep_until` and every deadline take an `Instant` and no clock.
- rejected: an agent proposed a clock parameter on each waiting operation; the owner rejected it, because an `Instant` exists only as a reading of a clock the program already holds, so waiting for one observes nothing a clock read has not.

### decision local-offset-read-through-wall-clock
`local_offset_seconds` reads the wall clock and returns the local offset from UTC that the host C library's `localtime` gives at a second, or `None`, with the offset under a day on `Some`.
- rejected: an agent proposed a nullary or pure local offset function; the owner rejected it, because the zone rule can change between reads, and a read absent from the parameters hides a state channel no effect row states.
- rejected: an agent proposed `GetTimeZoneInformationForYear` and `SystemTimeToTzSpecificLocalTimeEx` on Windows; the owner rejected it, because they ignore `TZ`, so a program would disagree with the C runtime's `localtime` that the offset matches.