# Compute scheduler

The runtime that runs `--par` compute tasks: worker deques, joins, worker stacks, idle lanes and the splitter's grain.

### decision workers-own-offer-deques
Each worker owns a fixed-capacity offer deque, idle threads steal the oldest task, and a join first takes back its own offer to run it inline.
- replaced: A shared array of lanes, scanned with a compare-and-swap per lane and a mutex and condition handshake, was on main; the owner changed it, because every offer paid the scan and the handshake whether granted or refused, up to 48.6 times slower than sequential at fine grain.

### decision a-full-deque-runs-the-offer-inline
A full deque refuses an offer and the call runs inline, so the deque's capacity is a grain floor on the deepest pending fork.
- rejected: an agent proposed a heartbeat gate that limits promotion by the local work since the last one; the owner rejected it, because it cannot lift the coarse-grain ceiling or free a caller waiting on the half it handed out, and promoting the oldest fork needs a deque anyway.

### decision a-waiting-join-works
A waiting join drains its own deque and then steals, and parks only when no work exists anywhere.
- rejected: an agent proposed a join that sleeps on a condition variable; the owner rejected it, because a sleeping joiner wastes the thread the program already has.

### decision compute-tasks-run-on-native-stacks
Compute tasks run on persistent native threads and ordinary stacks; a join on a stolen target helps other compute work first, and a native engine wait does not help.
- replaced: A park-on-miss scheduler, resuming a joining stack on the worker that finished its target, was on main; the owner changed it, because it measured slower and needed a managed-stack pool and its own schedule model (data: `research/investigations/compute-runtime/PRIOR-BUNDLE.md`).
- rejected: an agent proposed that a join check for a finished target before it helps; the owner rejected it, because its gain on finished targets says nothing of real workloads, and the cost control failed before any timing (data: `research/investigations/compute-model/DESIGN.md`).

### decision runtime-checks-use-deque-stress-and-tsan
The runtime's concurrency is checked by a deque stress test on native threads and ThreadSanitizer, with separate startup, join, completion and exhaustion tests.
- replaced: An exhaustive enumerator of schedules was on main; the owner changed it, because it modeled the managed-stack machine that the runtime no longer is.

### decision worker-and-entry-stacks-reserve-one-gibibyte
Worker and entry stacks each reserve one gibibyte, a size the runtime owns and exports across the language boundary, committed only as used.
- replaced: Sizing worker stacks from the environment's limit was on main; the owner changed it, because a worker's stack was then two orders of magnitude shorter than the entry's, so a deep recursion's survival depended on which thread stole it.
- rejected: an agent proposed a worker stack sized as the larger of the environment's limit and a fixed floor; the owner rejected it, because it brought back the environment's say over the workers and left a deep recursion's survival to which thread stole it.

### decision idle-lanes-spin-for-a-time-window
An idle lane spins for 1,000 microseconds only when the lanes fit the CPUs it may use and all CPUs are of one performance level; otherwise 1,024 rounds and 16 yields.
- rejected: an agent proposed a larger fixed spin count; the owner rejected it, because each larger bound gained 1 to 8 percent where the pool fits and slowed three of four kernels 16 to 570 percent at 8 lanes on 4 CPUs (data: `research/investigations/compute-runtime/RESULTS.md`).
- rejected: an agent proposed the time window on every host; the owner rejected it, because the idle-window comparisons tell wake latency apart from contention on too few or unequal cores (data: `research/investigations/compute-runtime/RESULTS.md`).
- rejected: an agent proposed changing the wake policy for the sparse-cadence loss on hosted SMT runners; the owner rejected it, because the loss did not repeat across machines that looked the same, so no runtime change was shown at fault (data: `research/investigations/compute-runtime/RESULTS.md`).

### decision the-map-splitter-keeps-its-grain
The map splitter keeps a work unit of 150,000 and 16 chunks per lane, both constants a build may override.
- rejected: an agent proposed a global work unit of 10,000; the owner rejected it, because one static weight prices wide and narrow work alike, and its gains on prefix and histogram cost the narrow stencil and high-diameter pull control (data: `research/investigations/compute-model/DESIGN.md`).

### decision a-group-joins-the-newest-call-first
A group's published calls are joined newest first.
- replaced: Joining in publish order was on main; the owner changed it, because on a Chase-Lev deque a join taken newest first finds its target at the owner's end or already stolen, never under a newer entry, and join order cannot be observed.
