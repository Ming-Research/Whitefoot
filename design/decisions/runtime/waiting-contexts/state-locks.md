# State locks

How an atomic statement locks, releases, watches and wakes the shared objects it names: the lock units, their order, guard watches and wake tests, reservations, and the shared object's header.

### decision each-shared-object-is-one-lock-unit
Each shared object is one lock unit: a map object is held by entries or whole, and any other object by its one lock.
- replaced: Lock units derived from a state's declaration, one for each keyed-table field and one for the other fields, was on main; the owner changed it, because a map is now an object of its own, so a statement's targets are its holds.
- replaced: A pending hold on a local table was on main; the owner changed it, because [TYPE-9] admits maps only as the states of shared objects.

### decision atomic-locks-follow-type-then-identity-order
A statement takes its locks in one order: types by a static total order, one type's objects by identity, a map's entries in key order; a group is taken at its first use or before the guard.
- rejected: an agent proposed ordering by identity alone, with every target taken when the statement begins; the owner rejected it, because that holds the log through every logged write's whole block.
- rejected: an agent proposed taking a table with header entries at its first use in the block; the owner rejected it, because a block that wrote or released a key's bytes before that use had the take read what it wrote or released, which an adversarial program met as a fault.
- replaced: Grant-based acquisition, through header grants, was on main; the owner changed it, because several objects in one statement need one global order of types and identities.
- replaced: Running a twin of the block before the hold, to collect the keys it reaches, was on main; the owner changed it, because a statement now names its entries in its header, so its keys are values when it begins.

### decision a-statement-releases-its-objects-when-its-block-ends
A statement releases every object it took when its block ends by any edge, so every taking comes before every release (two-phase locking).
- rejected: an agent proposed releasing an object once the block no longer uses it; the owner rejected it, because no workload's holds yet show that the earlier release matters.

### decision a-false-guard-registers-its-watch-before-release
A statement whose guard reads false registers a watch on each object the guard read while it still holds it, then releases every object and parks; a map entry's write wakes that map's watches.
- rejected: an agent proposed a list of watchers for each entry; the owner rejected it, because the list must survive the table's moves to larger tables, and no program's blocking pops yet show the cost.
- replaced: Waking every watch of a written object was on main; the owner changed it, because at 50 waiters it woke 29.9 watches per take, 29.0 of them to find their guard still false (data: `research/investigations/guard-fairness/DESIGN.md`).

### decision lowering-derives-a-wake-test-from-the-guard
Lowering derives for each object target a wake test that the guard implies: at most four comparisons over that object's scalars, every other part of the guard counted as true.
- rejected: an agent proposed freezing another target's comparisons at their values at registration; the owner rejected it, because a guard `band(a, b)` registered with both false would then miss the write that sets b and the write that sets a, and its wake would be lost.

### decision a-helper-guard-compiles-to-a-wake-callback
When no wake test can be derived from a guard with a helper call, a guard that passes the audit compiles to a nonwaiting function the writer calls under its hold to decide the wake.
- rejected: an agent proposed replacing the derived tests with callbacks for every guard; the owner rejected it, because that is a cold call per watch under the lock, about 2.5 to 7.5 us per write at 50 waiters, shares nothing between equal guards, and cannot read an object the writer does not hold.
- rejected: an agent proposed filtering helper guards by the fields they read; the owner rejected it, because an append changes a field that the writer and every reply guard read while only some waiters become ready, and the runtime's write flag names the whole object.

### decision a-write-notifies-the-oldest-watch-whose-test-holds
A write to an ordinary object evaluates each distinct wake test once and notifies only the oldest watch whose test holds, by first registration and then context identity.
- rejected: an agent proposed waking every watch whose test holds; the owner rejected it, because at 50 waiters that still woke 26 watches per take and ran at 0.91 to 0.92 of the baseline's time, against 0.69 to 0.70 for one notification (data: `research/investigations/guard-fairness/DESIGN.md`).
- rejected: an agent proposed a hand-off to the oldest waiter only, with every watch below the age threshold woken; the owner rejected it, because at 50 waiters it took 1.61 to 1.63 times the baseline's time, against a limit of 1.10 (data: `research/investigations/guard-fairness/DESIGN.md`).

### decision a-long-waiting-watch-reserves-the-next-attempt
A notified watch that has waited longer than `WF_GUARD_TURN_AGE_NS` (1 ms) reserves the object's next attempt; a younger watch's notification lets newcomers overtake it.
- replaced: Newcomers overtaking a woken watch freely was on main; the owner changed it, because at 50 waiters the longest wait was 524 to 534 ms, and the reservation brought it to 1.31 to 1.34 ms (data: `research/investigations/guard-fairness/DESIGN.md`).
- rejected: an agent proposed a strict turn for every woken watch; the owner rejected it, because firn's script engine lost 7.5 times its throughput at 8 connections and 47 times at 50, its p99 rising from 1.6 to 52 ms (data: `research/investigations/guard-fairness/DESIGN.md`).
- rejected: an agent proposed a turn for every woken watch past 1 ms of waiting; the owner rejected it, because at 50 waiters the longest wait was 40 to 48 ms against a 3.3 ms bound, and the run took 10.3 to 10.5 times the baseline's time (data: `research/investigations/guard-fairness/DESIGN.md`).

### decision a-reservation-refuses-only-statements-holding-nothing
A live reservation refuses only a statement that holds no object, and admits a holder of an earlier object, a holder of a table's entries and a cancellation firing's update.
- rejected: an agent proposed refusing every statement but the reservation's owner; the owner rejected it, because the reserved retry may need an object or entry such a holder keeps, so each would wait for the other, and a firing is the write that makes guards true.

### decision claims-never-wait-behind-a-reservation
A live reservation never delays a claim; a waiting claim refuses the reservation's owner, which parks and is handed the object after at most `WF_SHARED_HANDOFF` vain wakes.
- rejected: an agent proposed claims that wait for the reservation's retry; the owner rejected it, because a claim's owner holds a table's entries or an earlier object that the reserved retry may need, so each would wait for the other.

### decision a-second-notification-leaves-the-first-in-place
A retry over several objects that is notified for a second object while it owns a first object's notification keeps the first, and the second passes on to that object's next watch.
- rejected: an agent proposed moving the context's notification to the second object; the owner rejected it, because the owner ends its notification only by releasing the first object, and that release would then have no notification to end.

### decision the-queue-advances-between-sweeps
Once a sweep has passed through every watch it reaches, the object's ordinary acquisition queue advances before another sweep begins.
- rejected: an agent proposed letting sweeps follow one another while their reservations refuse the queue; the owner rejected it, because a statement without a guard, or one whose guard stays true, must eventually proceed [WAIT-2].

### decision watch-links-sit-in-one-bucket-per-test
An ordinary object's watch links sit in buckets, one per distinct wake test, each a ring in age order; each link names its test, which its frame keeps.
- rejected: an agent proposed one list of every watch in age order; the owner rejected it, because a write would then evaluate every watch's test.
- rejected: an agent proposed a copy of each test in its link; the owner rejected it, because that would add the test's 104 bytes to every link.

### decision the-acquisition-queue-keeps-only-its-tail
A shared object's acquisition queue stores only its tail, whose next link is its head.
- replaced: A separate head pointer beside the tail was on main; the owner changed it, because one pointer leaves room for the reservation in the fixed shared header, and insertion and removal stay constant-time.

### decision claims-queue-in-the-claimants-stack-records
The claims' circular queue lives in the claimants' stack records, and its tail shares the header's one reservation slot, told apart by the address's low bit.
- rejected: an agent proposed growing the header and moving every object's state; the owner rejected it, because the header already fills the 64 bytes before the state, on which emitted code and the cancellation state's address depend.
- rejected: an agent proposed a global table of claims keyed by object, behind a lock of its own; the owner rejected it, because the claims and the reservation are read only under the object's lock, so one tag test is all the shared slot adds to an admission.

### decision an-entry-the-guard-reads-is-locked-alone
An entry the statement's guard reads is locked alone, never read beside other readers.
- rejected: an agent proposed registering the watch before the guard's first evaluation; the owner rejected it, because every statement with a guard would pay for it, whether it waits or not.

### decision a-map-swap-exchanges-entries-in-place
A `swap` of two maps exchanges their entries and leaves each map object where it is, so a handle's object stays the map it named.
- rejected: an agent proposed exchanging the maps by address, storing the new table's address in place of the old; the owner rejected it, because a statement that read a handle orders and waits by that object's identity, and would then lock entries of a map no handle names.
- replaced: Writing a map value over a table was on main; the owner changed it, because [TYPE-9] now admits no such expression, so a map swap is the exchange.

### decision a-whole-hold-reads-an-absent-key-as-shared-none
Under a whole hold, a read of an absent key reads a shared `None` and allocates nothing; a write makes the key's cell, and a cell left `None` is given back at release.
- rejected: an agent proposed making a cell on every read; the owner rejected it, because a block that probes many keys and writes few would fill the table with empty cells.
- rejected: an agent proposed keeping empty cells until the table is released; the owner rejected it, because a block that probes many keys and writes few would fill the table with empty cells.

### decision a-map-borrow-materializes-only-where-written
A map borrow makes its cell only when its root may be written in the context running it; otherwise it reads the stable index and an absent key's shared `None`.
- rejected: an agent proposed always making the cell on a borrow; the owner rejected it, because readers under a read-only whole-map hold may run at once, so it would change shared storage without exclusion.
- rejected: an agent proposed making table hold descriptors thread-safe for concurrent readers; the owner rejected it, because a read needs only the stable index and the immutable `None`, so the synchronization would cost an operation that needs none.

### decision lazy-type-groups-keep-a-taken-word
A lazily taken type group keeps its taken state in one naturally aligned word; eager groups and direct single-entry targets need no taken flag.
- rejected: an agent proposed one Boolean frame slot for each target; the owner rejected it, because under the then rule of uniform alignment, a one-bit address forced the function frame into one aggregate and kept entry copies in LLVM (data: `research/investigations/shared-state/DESIGN.md`).

### decision cancellation-state-is-one-ordinary-shared-unit
Cancellation sources, watches and `SharedRead` views share one ordinary shared unit whose `Bool` it protects, with an atomic mirror for host waits published while it is held.
- rejected: an agent proposed registering host waits under the cancellation unit; the owner rejected it, because guards need stable state under the lock, while host waits, found when the source fires, must stay outside that lock.

### decision the-lock-ledger-reports-when-groups-are-taken
`whitefootc --lock-ledger` reports each atomic statement's ordered target groups and whether each is taken before the guard, on every body path, on some, or never, from lowering's own requests.
- rejected: an agent proposed a warning when a target is used early; the owner rejected it, because early use is often intended.
- rejected: an agent proposed a separate source analysis; the owner rejected it, because it could disagree with the groups the code actually takes.