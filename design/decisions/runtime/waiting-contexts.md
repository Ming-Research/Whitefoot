# Waiting contexts

The context runtime: drivers and their placement, how waiting functions and host waits lower, shared objects' locks and claims, helpers, fairness, deadlock reports and decision cells.

### decision contexts-run-on-driver-roles
Contexts run on drivers, each with its own ring, parked contexts and run queue; a start is made ready on its starter's driver, and an idle driver takes about half of another's ready contexts.
- replaced: One driver thread for every context was on main; the owner changed it, because a server then uses one core, while at four drivers the echo server ran 1.71 to 2.13 times its one-driver rate (data: `research/investigations/io-model/WAITS.md`).
- rejected: an agent proposed placing each start round robin on a driver it never leaves; the owner rejected it, because every start and join then woke another thread, so a start-then-join step cost 35.8 us at four drivers against 0.12 at one (data: `research/investigations/io-model/WAITS.md`).
- rejected: an agent proposed running contexts on the compute workers; the owner rejected it, because a compute task never waits [PAR-1, PAR-2], so no suspension may fall between a compute offer and its join.

### decision driver-count-is-fixed-before-the-entry
The runtime selects and keeps its driver count before it calls the entry, probing the root ring without starting threads, and starts exactly that many driver roles on first need.
- rejected: an agent proposed rereading the configuration or the CPU affinity at the first spawn; the owner rejected it, because the invocation already supplies the actual run-wide count through `Inputs` [PRE-2].
- rejected: an agent proposed silently starting fewer drivers when a later role cannot initialize; the owner rejected it, because the program has already read the count from `Inputs`, so the runtime stops with a report instead.

### decision waiting-function-is-an-llvm-coroutine
A waiting function lowers to an LLVM switched-resume coroutine that starts suspended, with its frame in the context's arena, and calls and returns transfer by a resume just before the suspension.
- rejected: an agent proposed returning every finished callee through the driver's ready queue; the owner rejected it, because LLVM 18's coroutine split turns the resume into a tail call, so returns do not grow the native stack.

### decision context-memory-comes-from-host-regions
Contexts and their frame arenas take memory from host regions the driver reserves itself, 64 MiB at a time, carved into power-of-two blocks kept on free lists.
- rejected: an agent proposed the program's allocator; the owner rejected it, because the runtime every build links never calls the program's allocator [STOR-8], so a program that uses no heap needs none.
- rejected: an agent proposed a reservation per context; the owner rejected it, because it adds a kernel mapping per connection, while with regions the compiled server's mapping count stayed at 37 from 1,000 to 19,000 idle connections (data: `research/investigations/io-model/WAITS.md`).

### decision one-completion-record-per-context
A waiting host operation lowers to a start into the running context's one completion record, a suspension while it is pending, and a finish that reads it into the result.
- rejected: an agent proposed a completion record in the frame of every waiting function that calls the host; the owner rejected it, because a context has at most one host operation pending at a time, so one record per context serves every call it makes.

### decision parked-frames-skip-the-ring
A suspended frame registers its context on its pending record without entering the ring, and the driver reaps the ring when no context is ready, after every 64 resumptions and when a run yields.
- rejected: an agent proposed a ring entry per wait and a scan of parked contexts per wake; the owner rejected it, because they held the compiled server to 0.64 and 0.59 of the hand-written one, and removing both raised it to 0.94 and 1.12 (data: `research/investigations/io-model/WAITS.md`).

### decision socket-receive-goes-to-the-ring
With other contexts live and a ring to wait in, a socket receive goes to the ring at once, without first trying the host without waiting.
- rejected: an agent proposed a first nonblocking attempt, as a sequential program makes; the owner rejected it, because that system call finds no data when the peer has not yet answered, and removing it raised the server from 0.80 to 0.90 (data: `research/investigations/io-model/WAITS.md`).

### decision spawn-lowers-to-a-wrapper-and-join
A spawn lowers to a synthesized wrapper whose frame and argument block the new context's arena holds, with a join before every exit of the starting activation and no inline path.
- rejected: an agent proposed reusing the compute lanes for spawns; the owner rejected it, because they refuse an offer by running it on the caller, and running a spawned call inline would wait for it, while [WAIT-3] makes it run beside its starter.

### decision bound-spawn-joins-at-first-use
A bound spawn returns its result into a slot of the starting frame and joins before the first later statement of its block that may leave the block or names the binding, else at the block's end.
- replaced: Joining before every statement whose overlap footprint is refused or not resolved was on main; the owner changed it, because a starter then waited for a context whose guard only that statement would make true, before the point [WAIT-3] fixes.
- rejected: an agent proposed joining at the next statement; the owner rejected it, because that serializes a gather.
- rejected: an agent proposed joining inside the statement that uses the binding, on the path that reaches the use; the owner rejected it, because later code would then have a joined and an unjoined path to merge.

### decision socket-waits-use-readiness-without-a-ring
With no kernel completion ring and other contexts live, a socket receive, send or accept with a never-firing watch waits for readiness, then runs on the driver thread.
- rejected: an agent proposed queuing socket waits on a peer to the helper pool; the owner rejected it, because a server doing so held only as many silent peers as the pool's eight threads, where readiness waits answered sixteen (data: `research/investigations/io-model/WAITS.md`).

### decision shared-object-counts-a-handle-only-when-needed
A shared object is one pool block, a header and then the state; a statement counts a handle of its own only when its guard or block may lose the place it read the handle from.
- replaced: Counting a handle in every atomic statement was on main; the owner changed it, because each statement then wrote a line all drivers share, and without it firn's depth-16 `SET` rose from 1.71-1.80 to 1.89-2.12 million a second (data: `research/investigations/concurrent-map/DESIGN.md`).
- replaced: Omitting the count only for a handle reached through a reference parameter whose row writes nothing below it was on main; the owner changed it, because a handle in a local, a value parameter or a field that the block leaves alone keeps the object just as live, and still wrote that line.
- rejected: an agent proposed allocating shared objects with the program's heap; the owner rejected it, because the runtime every build links never calls the program's allocator [STOR-8].
- rejected: an agent proposed refusing a block that touches its target's handle place; the owner rejected it, because a block may need to move or release the handle it reached the object through.

### decision held-object-spins-then-parks
A statement that finds its object held spins for a bounded time and parks; an unlock wakes the first parked statement to retry, and after two vain wakes hands it the object.
- rejected: an agent proposed a first-come queue that hands the object to its head; the owner rejected it, because the object stays held until a driver resumes that context, and 97 percent of pipelined acquires parked, against 0.6 with retries (data: `research/investigations/io-model/SHARED.md`).
- rejected: an agent proposed retrying after a wake with no bound; the owner rejected it, because a woken statement could miss the object at every retry, while [WAIT-2] promises that a statement whose guard stays true takes effect.
- rejected: an agent proposed bounding how long a statement has waited instead of how often it was woken; the owner rejected it, because a count of vain wakes gives the promise on every schedule, while a time bound adds a threshold that depends on the host's speed.

### decision unparkable-statement-queues-a-claim
A statement that cannot park, since it holds a map's entries or an earlier object, queues a claim at its first miss, and while a claim waits the object is taken only through claims.
- rejected: an agent proposed queuing a claim only after several misses; the owner rejected it, because a claim from the first confirmed miss leaves no interval in which a host that preempts the statement lets any number of newcomers pass it.
- rejected: an agent proposed a ticket for every acquisition, parked or not, taken in order; the owner rejected it, because strict turns cut firn's engine to 1/7.5 to 1/47 of its throughput in the guarded-wait experiment (data: `research/investigations/shared-take-fairness/DESIGN.md`).
- rejected: an agent proposed one arbiter over parked statements, claims and guard reservations; the owner rejected it, because it would replace the guard reservation's arbitration, measured and approved on its own, while claims beside it need one added rule.

### decision claims-alternate-with-parked-statements
An unlock that frees the object grants the oldest claim after the first parked statement has had its wake or hand-off, so claims and parked hand-offs take turns.
- rejected: an agent proposed serving claims before every parked statement; the owner rejected it, because a stream of claims could then starve the parked queue.
- rejected: an agent proposed serving the parked queue first; the owner rejected it, because a stream of due hand-offs could then starve the claims.

### decision only-the-oldest-claim-borrows
While claims wait, only the oldest may borrow a hold an unlock handed to a parked statement that has not yet resumed, and it gives the hold back when it ends.
- rejected: an agent proposed claims that wait for the parked statement; the owner rejected it, because the parked statement may wait on the very driver a claim's owner keeps, so the claim would stop that driver for good.
- rejected: an agent proposed letting any claim borrow such a hold; the owner rejected it, because a later claim or a newcomer that borrowed would overtake the oldest.

### decision false-guard-parks-with-a-watch
A statement whose guard reads false parks with a watch on each unit its guard read, and a write wakes or notifies those watches when it ends; a woken statement evaluates its guard again.
- rejected: an agent proposed a condition the writer signals by name; the owner rejected it, because a guard writes nothing, so only a write can change its answer, and a woken statement reads again the state it takes effect in.

### decision helpers-run-operations-the-ring-does-not-carry
Once a context other than the root starts, or a bounded or cancellable wait reaches the adapter, every operation the ring does not carry runs on the helper pool unless `WF_IO_HELPERS` pins it.
- rejected: an agent proposed handing helpers only the kinds of operation that wait on another program; the owner rejected it, because a pipe's read or write or a connect may wait for another context of the same program, and a driver inside one stops every context it runs.
- rejected: an agent proposed waiting for readiness before every stream operation; the owner rejected it, because a blocking write larger than a pipe's free space still blocks.

### decision helper-pool-grows-past-its-ceiling
Once a context other than the root starts, the pool starts one more helper past its ceiling while every helper is inside a wait that may depend on another context, within records for 256 helpers.
- rejected: an agent proposed a readiness-driven adapter alone; the owner rejected it, because it cannot wait on a standard stream shared with other processes without making it nonblocking under them, so a stream's waits would still need helpers.

### decision stuck-program-stops-with-a-report
A driver that goes idle stops the program with a report when every driver has said it is idle with nothing ready and none of its contexts waits for a host outcome, counted per driver.
- rejected: an agent proposed one shared count of host waits; the owner rejected it, because every park and unpark on every driver would then update it.

### decision deadlock-report-walks-a-roster
A deadlock report walks a roster under one lock that holds the root and each child from its first real guard or join park until before it is reclaimed.
- rejected: an agent proposed registering every child at spawn; the owner rejected it, because a context that never waits cannot appear in an all-stuck report, yet that roster cost 8.4% on one driver's concurrent spawns (data: `research/investigations/deadlock-sites/DESIGN.md`).
- rejected: an agent proposed rebuilding the list of waiters from the scheduler's queues; the owner rejected it, because join groups have no global index and guards can wait on no unit, so those contexts have no root to walk from.
- rejected: an agent proposed per-driver locked rosters as the first cost repair; the owner rejected it, because they keep the same two lock takes per context that never waits and cannot remove the one-driver work.
- rejected: an agent proposed lock-free rosters based only on which driver owns a context; the owner rejected it, because a displaced executor may still spawn while a replacement serves its role, so the driver alone does not give one writer.
- rejected: an agent proposed a list of only parked contexts, linked and unlinked on every park and wake; the owner rejected it, because parks and wakes far outnumber spawns and finishes in an I/O server.
- rejected: an agent proposed reporting only the detecting thread's own wait; the owner rejected it, because the report must name every waiter to explain why the execution cannot go on.
- rejected: an agent proposed one wait-site slot per worker thread; the owner rejected it, because contexts usually outnumber worker threads, and a suspended context can resume on another executor.

### decision diagnostic-fields-in-a-cold-tail
A context's diagnostic fields sit in a cold tail after its scheduler fields; allocation clears only the scheduler fields and the roster marker, and the first registration sets the rest.
- rejected: an agent proposed diagnostic fields ahead of the scheduler fields, all cleared on every spawn; the owner rejected it, because that prefix shifted hot ready-queue fields and lengthened every spawn's clearing, while unregistered fields have no reader.

### decision one-inactivity-predicate
Every deadlock check, the recheck under the roster's lock included, calls one `wf_all_contexts_inactive` predicate.
- rejected: an agent proposed repeating the counter reads in each caller that reports; the owner rejected it, because the report's lifetime and publication need one stable contract, whichever counters the stuck snapshot reads.

### decision decision-cell-is-one-atomic-word
A `DecisionCell` reuses the shared allocation and handle count with a private 32-bit atomic verdict, read by a sequentially consistent load and claimed by one strong compare-and-swap from Open.
- rejected: an agent proposed a verdict lock; the owner rejected it, because the cell is read and claimed while a caller holds shared keys, and one compare-and-swap decides the winner with no lock of shared state.

### decision decision-cell-needs-lock-free-atomics
Building the decision-cell runtime requires an always-lock-free 32-bit atomic word, and it has no locking fallback.
- rejected: an agent proposed a locking fallback for targets without such a word; the owner rejected it, because a fallback could wait while shared keys are held, which would break the nonwaiting operation.