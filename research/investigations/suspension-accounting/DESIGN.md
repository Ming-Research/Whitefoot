# Context-switch accounting without process-global counters

## Status

Implemented by item `firn-req-shared-max`. The decisions are in
[`design/compiler/waiting-contexts/suspension-accounting.md`](../../../design/compiler/waiting-contexts/suspension-accounting.md)
and the 64-pass rule in
[`design/compiler/waiting-contexts.md`](../../../design/compiler/waiting-contexts.md).
This record holds the evidence that motivated them, the correctness argument
for deadlock detection under per-executor accounting, and what the change's
tests cover. No performance claim is made here for the merged runtime: the
workload that measured the cost is not in this repository (section 1).

## 1. Question and evidence

**Question.** Can a context that gives up its driver at the 64-pass fairness
boundary avoid writing a process-global accounting line, while the runtime
keeps [WAIT-2]'s promise that a ready or expired context proceeds and still
detects a program that can take no further step?

**Where the cost was seen.** A sharded-counter study (branch
`claude/shared-counter-study`, a closed pull request whose runtime part this
change splits out) made every atomic statement on a counter pass the 64-pass
boundary, as ordinary object acquisitions already did. Its workload
(64 contexts adding one to a per-thread shard, 5 s cells, six alternating
blocks, twin-build control valid at every CPU list) lost throughput when the
boundary was applied. Rates are millions of statements per second at 2 / 4 / 8
P-cores on the 14900K:

| Run | Boundary | 2 P | 4 P | 8 P |
|---|---|---|---|---|
| [38413746180](https://github.com/Ming-Research/Whitefoot/actions/runs/38413746180) | not passed by shard statements | 368.7 | 737.9 | 1476.3 |
| [38427370760](https://github.com/Ming-Research/Whitefoot/actions/runs/38427370760) | passed every 64 statements | 277.3 | 496.3 | 948.8 |

Against a C program adding to a per-thread cache line in the same run
(498.4 / 997.1 / 1993.9), the second row is 0.556 / 0.498 / 0.476.

An attribution run ([38441051550](https://github.com/Ming-Research/Whitefoot/actions/runs/38441051550),
same source with the threshold built as 64, 256 and 1,048,576) gave: raising
the threshold to 256 recovered 0.746 / 0.814 / 0.854 of the gap between 64 and
1,048,576 at 2 / 4 / 8 P-cores, and the modeled cost per taken yield was
59 / 127 / 134 ns, 2.3 times larger at 8 than at 2 P-cores. The registered
prediction that the 1,048,576 arm returns to at least 0.70 of the C program
failed (0.643 / 0.643 / 0.637), and the registered falsifier (below 0.60) was
not met, so the attribution of the whole cost to switching stayed unsettled.
Taken-switch counts were not measured, so the growth with cores is a
description, not a proof. Reading the code showed three process-global atomic
read-modify-writes per native resume (an active-invocation count incremented
and decremented, and a change counter incremented), shared by every driver.

**After this change.** On the study branch with this change and a shorter
ticket wait, [38458676162](https://github.com/Ming-Research/Whitefoot/actions/runs/38458676162)
measured 0.589 / 0.591 / 0.601 of the C program at 2 / 4 / 8 P-cores. That
branch also changed the whole-read ticket wait, so the run does not attribute
the recovery to this change alone, and it is not a measurement of the code
merged here.

**A separate defect.** Code inspection found that the old 64-pass check
returned without unwinding when the driver's ready queue was empty, even with
a sleeping context parked on a timer. A sleeper is not in the ready queue
until a reap expires its timer, and a reap happens only when the driver
resumes contexts. A lone busy context could therefore keep a sleeper's timer
unserviced until the deadline monitor or an idle borrower rescued it. A
sleeping whole reader beside a busy writer made only 81 and 76 reads in 5 s
in two cells of the study's eight-core blocks; the cause of those two cells was
not established, and this defect is one plausible contributor.

## 2. Decisions

1. Each physical executor publishes an odd/even invocation sequence on its own
   cache line; the stuck check collects every slot before and after the driver
   scan (section 3).
2. A cold mutex serializes the stuck scan with idle, wake and transfer
   transitions, and logical idleness persists across empty polls (section 4).
3. A fairness suspension is published to the ready queue after unwind without
   the external-wake gate, and the threshold also unwinds for pending host
   waits or ingress (section 5).

Rejected alternatives are recorded with the decisions.

## 3. The invocation sequence and the snapshot argument

**Physical invocation lifetime.** The runtime reserves 128 static slots, one
64-byte line each, indexed by the physical executor's existing identity
(twice the 64-driver limit). Only that executor writes its monotonically
increasing sequence. Even values mean no invocation; odd means the executor
is inside a native resume or has not finished publishing the returned
disposition. Slots exist before threads start, are never reset on rebinding
and never wrap (an exhausted sequence stops the program with a report). Initial
driver `i` uses executor `2i` (root zero); its warm spare uses `2i+1`. An
executor keeps its index when it changes roles or borrows service, so unused
slots stay zero and no changing roster is needed by the scans.

Before publishing OUTSIDE and resuming a frame, the executor stores the next
odd sequence. After native unwind it first adopts the returned context under
retained SERVICE or publishes it through ingress, including the ingress count
and notification, and only then stores the next even sequence. Both stores and
every correctness reader's loads are sequentially consistent. A spare uses its
own slot when it begins a resume; per-driver slots would be unsound because a
replacement and the displaced executor can both execute for one logical
driver. Borrowed service runs no coroutine and uses the `moving` count until
detached contexts are requeued; their later resumes use the borrower's slot.

The retained rotation removes the global active-count increment and decrement
and the invocation's change-counter increment. It keeps the run-queue lock
acquisitions and the service-token CAS. The two sequence stores still need
locked instructions on x86-64; they are writes to the executor's own line,
not free. No disassembly or speedup is claimed.

**Snapshot argument.** Stuck detection runs under the cold mutex (section 4).
It reads the change generation, collects all 128 sequences (rejecting any odd
one), scans every driver-table slot (not bounded by a possibly stale driver
count), checks `moving`, collects the sequences again, and rereads the change
generation. It rejects any changed sequence or generation, any driver not
idle, and any nonzero ingress, host-wait, ready or `moving` count. A driver's
host-wait count is read before its ready count, because service enqueues a
completed wait before dropping `host_waits`.

In the sequentially consistent order, equal even reads of one slot exclude any
store to it between those reads, because the sequence only grows. So no
invocation begins or ends on that executor in that interval, and none is in
flight. Every slot's interval contains the common stretch that starts after
the last first-collect load and ends before the first second-collect load;
the whole driver scan lies in it. Hence no native invocation runs, begins or
ends during the scan. An even value read in the first collect also
synchronizes with the store that published it, which follows the invocation's
queue, host-wait and ingress writes; the scan therefore sees every effect of
every completed invocation. A single collect or a Boolean "active" flag cannot
give this: an invocation can begin and end during the scan, publish a ready
context into a queue the scan already passed, and leave the flag unchanged.

Displaced ingress stays counted until adoption has published its ready,
host-wait or completion result; a consumer may detach the list while adopting
it but cannot retire the count early. Stealing, borrowing and reservation keep
`moving` raised while work or service ownership is between representations,
and the change generation covers a transfer that finishes during the scan.
These obligations exclude a runnable context hidden between a queue, an
invocation, an ingress record and a host-wait record. A successful scan
therefore reports stuck only for an execution with no further step; root
completion is checked separately before a deadlock is reported.

## 4. Persistent deadlock and the cold mutex

Repeated double collection alone has no liveness proof: empty timed polls could
clear and re-announce idleness, and unsuccessful assistance repeatedly changes
`moving`, so each attempted snapshot is invalidated. One native mutex (the
platform primitive of the scheduler's `wf_prim_wait`; its condition variable is
unused) serializes the stuck scan, the wake claim, the searcher end, startup
publication, idle announcements and the bookkeeping of steals, borrows and
reassignments. Logical idleness persists across empty timed polls and ends
before a driver takes actual work, in `wf_run_take` under the mutex when the
driver's idle flag is set. The already-active rotation does not acquire the
mutex. Native engines, parks and notifications run outside it; it is never held
over a clock reading, host operation, coroutine resume or park; its lock order
is before the executor, run and ingress locks; no callback acquires it
recursively. A successful transfer keeps `moving` raised through any unlocked
work and retires it under the mutex.

Assume physical threads receive service and finite runtime operations finish.
Once every unfinished context waits persistently on a false guard or another
context and no host operation remains, all final invocation publications and
successful transfers finish. Empty polling preserves idleness, and a failed
probe cannot overlap and invalidate a locked scan. The last driver to announce
idleness, or a later empty-loop scan, then observes stable even slots and zero
counts, and reports the deadlock. This is eventual detection, not a wall-clock
bound; it does not rely on a fortunate quiet interval between independently
changing idle announcements.

A displaced executor sends one further notification to its driver after its
even store. A replacement can consume the first (ingress) notification, adopt
the disposition, observe the old executor still odd, and park without a host
deadline; the final even publication must cause a fresh scan, and a
notification before the even store cannot guarantee that. Other executors are
joined before wake endpoints are destroyed, and executor zero cleans up only
after its own invocation returns, so the endpoint is live for that
notification. Retained rotation sends nothing extra.

Descriptor-only polling collects executor activity before it checks ingress and
validates that collection afterward, so an active or changed slot keeps the
poll finite. Shutdown requires all physical slots inactive and no `moving` or
ingress work after root completion and monitor shutdown have closed admission;
root completion alone does not authorize destroying a displaced executor's
wake endpoint or an ingress record still being adopted.

## 5. Fair suspension and pending host work

A fairness suspension saves its frame and returns a distinct ready
disposition. It registers no external waker, so it needs no ARMING-to-EARLY
CAS or gate exchange. The retained owner enqueues it after native unwind; a
displaced owner publishes it through ingress. Joins, guards and shared-lock
registrations keep the external-wake gate because their wakers can win before
unwind. No path accesses a context after handing it to a queue or ending its
lifetime.

At the 64-pass boundary a context unwinds when another context is ready on its
driver, when its invocation is displaced, or, new here, when the driver has a
pending host wait or ingress. Only atomic counts are inspected while OUTSIDE,
since the driver-private timer heap cannot be read there. This closes the
local service hole of section 1 without changing the source language or
relying on a monitor timing bound.

## 6. What the tests cover

Everything runs in CI; nothing was run on the author's machine.

- `driver_service_test.c`, which includes the bridge and drives its internals
  deterministically, adds: `fair_yield_boundaries` (alone and nothing pending
  continues; a ready peer, a displaced invocation, a pending host wait and
  pending ingress each unwind, and a fair suspension reaches the run queue only
  on adoption with the wake gate untouched), `solitary_sleep_reap` (one driver,
  no monitor, no second executor: a parked sleep whose timer is due becomes
  ready by the 64th resume of a lone busy context), `invocation_collections`
  (an invocation placed between the two collections, including on the last
  slot, defeats the snapshot), `idle_quiescence` (repeated failed steal and
  borrow probes leave the scan stable, and a searcher claim is retired only by
  its owner), and updated displaced-ingress, shutdown and polling cases that
  read the sequences instead of the old count.
- Each test was also run alone against a variant of `bridge.c` that breaks
  one condition it protects, built by a temporary workflow (removed with the
  branch). Every variant built and then failed one of that test's own checks,
  none a compile error. `fair_yield_boundaries` rejects a 64-pass check that
  ignores a pending host wait or ingress, and a fairness suspension that arms
  the wake gate again; `solitary_sleep_reap` rejects the first of these;
  `invocation_collections` rejects a stuck check that does not validate its
  first collection, a collection that accepts an odd slot, and shutdown
  quiescence that ignores the invocation slots; `idle_quiescence` rejects a
  run-queue take that leaves its driver logically idle, a refused borrow
  probe that counts as a state change, and an end of search that retires a
  wake claim the driver never held; `root_ingress_quiescence` rejects a
  displaced return that sends no final notification.
- `contexts.rs` runs `lone_timer_writer.wf`, a sleeping reader beside one busy
  writer on one driver, on the ring and adapter routes. The deadline monitor
  can rescue the old runtime, so this is end-to-end coverage only; the
  deterministic cases above are what reject the old threshold check.
- The driver-service tests also ran under the address and thread sanitizers
  (the `driver-service-sanitize`, `driver-service-tsan` and native-test
  targets, which `make check` does not run), and `sanitizers.yml` builds the
  corpus programs with this runtime instrumented. Sanitizers cannot prove an
  all-atomic ordering argument, so sections 3 and 4 need review as written.

## 7. Not in this change

The sharded state's ticket wait (spin, then yield the host thread) and the
sharded writes themselves belong to the sharded-field work. The inline atomic
boundary the study added for shard statements is not on the main line, so
this change touches only the shared out-of-line boundary.
