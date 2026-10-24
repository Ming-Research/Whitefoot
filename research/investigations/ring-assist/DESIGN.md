# Ring assist: serving a computing driver's completions

## Status and question

**The owner ruled for candidate (c), extending the monitor's whole-role
reassignment. Sub-step 1 of section 5 is implemented on the work branch with
the tests of section 7; sub-step 2 is not, and its gap stays recorded in
section 1.4. The branch's hosted runs pass the native cases and the program
witnesses, and the quick workflow's nine runtime mutants and its witness
mutant (the base's deadline-only trigger) each fail the case written for
them. The latency arms of section 6.1 ran as the program witnesses at 1, 2 and
4 CPUs, 30 invocations each, and every echo stayed inside 100 ms; "What ran"
in 6.1 lists the differences from the registered apparatus. The 14900K cost
comparison of section 6.2 runs on the head that merges, and this record
states no ordinary-I/O result. Nothing here promises a latency.** The study itself was read-only, of `main` at
`51c5f4263` (2026-10-12): no code changed, and no compiler, build or test
was run to write it. The runtime evidence comes from two places: the
earlier driver-handoff measurements
([driver-handoff](../driver-handoff/DESIGN.md)) and one Firn witness run
(section 1.2). Line numbers name files under
`compiler/src/backend/completion/` unless the path says otherwise.

**Question.** Firn runs with two drivers. One context computes for seconds
without waiting, and the other driver is idle. Can the I/O completions that
belong to the computing driver still be served within 100 ms, without making
ordinary I/O slower?

**Proposed answer.** Yes, by extending the existing whole-role reassignment.
The monitor hands a driver's role to a warm spare thread when one context run
on that driver stays outside service for one monitor period while its ring
holds completions or staged submissions. It does this whether or not another
driver is idle (section 5). Ordinary I/O pays nothing new on the owner's
path. Section 6 registers the experiment that could reject this answer.

## 1. The observation and its cause

### 1.1 What Firn reported

Firn ran on CI with `WF_DRIVERS=2`. A 13 s write script ran on one
connection. A `PING` sent on a second connection 100 ms later was answered at
106 ms. A `SCRIPT LOAD` on a third connection was answered at 13.017 s, 30 µs
before the script's own reply, although it takes nothing the script holds.
The scheduler report (`WF_SCHED_REPORT=2`) showed the idle driver borrowing
683 times, with `borrowed_terminals=0`.

### 1.2 Firn's witness: both accept orders stall

Firn then wrote a minimal witness: `research/experiments/ring-assist/ring.wf`
on Firn-wf branch `claude/witness-ring`, run 38234440511 on hosted
ubuntu-24.04 (4 CPUs, AMD EPYC 9V45), with release `wf-exp-45c544b24638` and
`WF_DRIVERS=2`. The program works as follows:

- The root context accepts exactly two connections in a loop. It spawns
  `serve` for each one (`spawn serve(connection: move connection, ...)`).
- A connection whose first line is `go N` computes N steps of a recurrence
  with no wait. Any other connection echoes its lines.
- The client sends `go` on one socket and a line on the other 100 ms later.
  Odd runs send `go` on the first-accepted socket and even runs on the
  second.

In 10 of 10 runs the echo arrived 1,846 to 1,938 ms after its ping, about
100 ms before the compute reply (2,016 to 2,038 ms). The accept order made no
difference. Each run's report has the same shape:

| Counter | Driver 0 | Driver 1 |
|---|---|---|
| `borrow_attempts` | 196 to 198 | 3,871 to 4,005 |
| `borrows` | 0 | 3,683 to 3,817 |
| `borrowed_terminals` | 0 | 0 or 1 (five runs each, not tied to accept order) |
| `stolen_contexts` | 0 | 0 |
| `reassignments`, `ingress_commits`, `reserve_misses` | 0 | 0 |

**Why both orders stall: both connection contexts live on driver 0.** The
counters show this directly, and the code says why:

1. The root context runs on driver 0 (`wf__context_root_begin`, bridge.c:4172).
   A spawn makes the new context ready on its starter's driver:
   `context->driver = wf_driver_self` and then `wf_context_ready`
   (bridge.c:2638-2643). `wf_context_ready` wakes another driver only when
   the starter's run queue then holds more than one context
   (bridge.c:1855-1857). After each spawn the root suspends on its next accept
   or on its join. Driver 0 then pops the single ready child itself, as the
   driver-handoff D1 result also found
   ([D1](../driver-handoff/DESIGN.md#result-waking-an-idle-driver-for-a-lone-ready-context-d1-does-not-help)).
2. A context submits to the ring of the driver that runs it
   (`wf_bridge_linux_current`, bridge.c:356-360, bound per thread in
   `wf_executor_bind`, bridge.c:3935-3941). So both `serve` contexts submit
   their first receive to driver 0's ring and park on driver 0.
3. A parked context never changes driver. `wf_driver_steal` moves only ready
   contexts, which have no operation in flight (bridge.c:1902-1972), and the
   placement decision says so too ([waiting-contexts](../../../design/compiler/waiting-contexts.md),
   first decision). Driver 1 never had a ready context to take:
   `stolen_contexts=0` on both drivers in all ten runs.
4. Driver 1 had nothing of its own. Driver 0's 196 to 198 failed borrow
   attempts came from the setup period, when driver 1 was parked and never
   outside service. During the computation, driver 1 borrowed driver 0 about
   1,900 times a second, which matches the 1 ms idle park
   (bridge.c:3631-3637) and the earlier witness's rate. But a borrow cannot
   reap a ring (1.3), so the echo's completion stayed in driver 0's
   completion queue.

So the accept order only chooses which of two driver-0 contexts computes.
The other one's receive completes in driver 0's ring either way. A server
with one accepting context therefore puts every connection on one driver
until a burst of ready work wakes a peer. The case where "the echo lands on
the computing driver" is the normal case for such a program, not an unlucky
placement. Firn's 106 ms `PING` fits this reading: in a longer-lived server,
that connection's context had likely been stolen while ready at some earlier
moment. This is inferred; the report cannot show where each context ran.

### 1.3 The completion path, with Firn's reading confirmed and corrected

A ring completion reaches its context only through the thread that holds the
driver's service token:

- `wf_bridge_progress` (bridge.c:929-938) calls `wf_bridge_ring_progress`
  (bridge.c:412-436). That reaps `wf_bridge_linux_current()`, the calling
  thread's bound ring. The driver loop calls it only when nothing is ready
  (bridge.c:3772), every 64 resumptions (`WF_DRIVER_REAP_RUNS`,
  bridge.c:3655 and 3726) and after a ring park (bridge.c:3830). A context
  that computes for 13 s returns to none of these.
- The reaped completion publishes its record (`wf_linux_complete_record` to
  `wf_completion_record_complete`, linux_io_uring.c:968). The waiter is found
  through `wf_driver_service->by_record` (bridge.c:2273-2297) and woken on
  that driver.

Firn's three claims, checked against the code:

1. *"Only the owning driver reaps its io_uring."* **Confirmed, with a
   correction.** Only the thread holding the role's service token reaps:
   the owner, or a replacement after reassignment
   (`driver_service_native_test.c` shows a replacement reaping the original
   ring). Non-driver threads that join on the process ring also reap it
   (`wf_bridge_park`, bridge.c:965-967). The ring itself does not limit who
   may reap. It is created with `IORING_SETUP_CQSIZE` alone
   (linux_io_uring.c:228-232), with no single-issuer or task-run flag, and
   its submission and completion queues each have their own mutex
   (linux_io_uring.c:781 and 1093-1094).
2. *"`wf_driver_service_only` handles only sleeps and already-published
   records."* **Confirmed** (bridge.c:3532-3587). It completes due or
   cancelled timer-route waits and detaches records that are already done. By
   design it enters no ring, takes no engine lock and makes no cancellation
   call (comment at bridge.c:3532-3541).
3. *"`wf_driver_reassign` covers only overdue timers."* **Confirmed, and
   narrower than stated.** It covers any overdue deadline in the heap, not
   only sleeps. But it gives up whenever any driver is idle:
   `atomic_load(&wf_drivers_idle)` returns 0 at bridge.c:3868, because "idle
   peers get first chance to borrow" (bridge.c:3853-3856). With an idle
   peer, therefore, nothing serves work that a borrow cannot do.

**Confirmed cause.** With two drivers, a context computing on driver 0, and
driver 1 idle, a completion in driver 0's ring has no server:

- driver 0's thread is inside the computation;
- driver 1's borrow may not reap a ring;
- the monitor declines to reassign because driver 1 is idle, and it reacts
  only to deadlines in any case.

The completion waits until the computation's next suspension.

### 1.4 Two more gaps of the same kind (found by reading, not yet observed)

Both follow from the same split of work. Each is to be confirmed by the
deterministic tests in section 7.

- **Staged submissions.** The doorbell is deferred: a submit stages its SQE
  and does not enter the kernel (linux_io_uring.c:791-815). The kernel learns
  of it at the next `wf_bridge_progress` or flush. Suppose a context submits
  and suspends, and the next context on the same driver computes for
  seconds. The driver loop runs the next ready context before any reap
  (bridge.c:3760-3764). The first operation then sits unsubmitted for the
  whole computation.
- **Deadlines and cancellations of non-sleep operations.** Take a receive
  with a deadline, parked on a computing driver while another driver idles.
  The borrow skips it, because only timer-route records qualify
  (bridge.c:3554). Reassignment declines because a peer is idle
  (bridge.c:3868). So `DeadlinePassed` arrives only when the computation
  ends. The same holds for a fired cancel watch on a ring or helper
  operation: `wf_driver_find_cancelled` runs only on the owner
  (bridge.c:3509-3521).

### 1.5 What the specification promises

[WAIT-2] promises that a context waiting for a host outcome that has been
produced *eventually* takes its next step. This holds while every context
reaches its completion or a wait in finitely many steps. A 13 s computation
reaches its wait, so the observed behavior conforms. [PRE-2] produces
`DeadlinePassed` when the deadline is reached and states no delivery bound.
The 100 ms target is therefore an implementation goal. Nothing below changes
the specification, and the earlier proposal for a latency profile in [WAIT-2]
([driver-handoff §8](../driver-handoff/DESIGN.md#8-proposed-wait-2-wording-and-affected-rules))
stays unadopted. Whether to promise a bound in the language is a separate
owner decision (section 8).

## 2. Constraints every candidate inherits

**Ring ownership on Linux.** Any thread may submit or reap a driver's ring
under its two mutexes. The owner already relies on this: a replacement reaps
while the displaced thread may still submit
([bounded-waits](../../../design/compiler/waiting-contexts/bounded-waits.md),
third decision). Completions also reach the queue while the submitting thread
computes, because the ring has no `IORING_SETUP_COOP_TASKRUN`
([completion-runtime](../../../design/compiler/completion-runtime.md),
decision on `COOP_TASKRUN`). `driver_service_native_test.c` observes this: it
keeps the submitter spinning in user space and waits for the real CQE of a
pipe read.

Two flags would break every candidate below:

- `IORING_SETUP_SINGLE_ISSUER`: the kernel refuses submission from any task
  other than the ring's issuer, so a replacement's kick would fail.
- `IORING_SETUP_DEFER_TASKRUN`, which requires single issuer: completion work
  then runs only when the issuer itself enters the ring to wait, so a
  computing issuer would hold back its own completions.

[WAITS.md, Experiment 4](../io-model/WAITS.md#experiment-4-attributing-the-gap-at-64-connections)
attributes part of the one-driver gap to the reference server to those modes.
So adopting them later would reopen this study. Each candidate here commits
further to a ring that any thread may enter.

**Service token.** One atomic word holds an epoch, the physical executor and a
phase (bridge.c:1488-1499):

- the owner holds `SERVICE`;
- it publishes `OUTSIDE` with a release store before resuming a context
  (`wf_driver_leave_service`, bridge.c:1711-1719);
- a borrower takes `BORROWED_SERVICE` and the monitor takes `PROBE`, each by
  an acquire CAS of the exact `OUTSIDE` word;
- a reassignment publishes `RESERVED` with a new epoch, and the spare takes
  `SERVICE` from it (bridge.c:3943-3955).

The owner returning from a context spins and yields while a borrow or probe
holds its token (bridge.c:1728-1740). So every pass taken under the token
lengthens the owner's return whenever the two collide.

**Hosts.** Only Linux runs more than one driver. `wf_driver_select_count`
returns 1 on every other host (bridge.c:4108), and [PRE-2] uses one driver on
a host without a kernel ring. macOS has no completion ring: sockets wait for
readiness through `poll` on the driver thread (file_posix.c:696), and other
operations run on helper threads. kqueue appears only in stop-signal handling
(stop_signals.c:214), so the question's macOS kqueue premise does not apply.
Windows has one completion port per process and one driver. Its port wait
publishes the packet it dequeues inline and therefore keeps `SERVICE`
(bridge.c:3818-3829 are compiled out on Windows).

So "another, idle driver" exists only on Linux. On macOS and Windows a
computing context holds back I/O completions too, but the only possible
server there is a spare thread. Section 8 records that case.

**Prior objection: several reapers on one ring.** Several idle threads once
reaped one shared ring. Every one of them took the submission lock to kick
and the completion lock to read a single entry. Sixty-four ping-ponging
connections then made 720 thousand futex calls a run (comment at
bridge.c:79-87, [RESULTS.md §6](../io-model/RESULTS.md)). Any design that lets
several threads reap a ring routinely meets this cost again.

## 3. Candidates

### (a) The borrowing idle driver also reaps the busy ring

**Mechanism.**

1. In `wf_driver_assist`, while holding `BORROWED_SERVICE`, the idle driver
   kicks and reaps the victim's adapter, up to `WF_BRIDGE_REAP_BUDGET`
   completions.
2. Each record this publishes wakes its parked context through
   `victim->by_record`.
3. The borrower detaches that context and, after giving the token back,
   queues it on its own run queue.

The borrow already migrates detached contexts this way (bridge.c:3608-3616).

**Correctness.**

- *Who touches the queues.* The victim's SQ and CQ, under their mutexes.
  The displaced owner never reaps while outside service. Non-driver joiners
  on the root ring may reap concurrently, and the completion mutex orders
  them.
- *Publication must change.* `wf_context_record_published` looks only at
  `wf_driver_service->by_record`, which is the borrower's own driver
  (bridge.c:2280-2285). Without a change, every reaped record would take the
  foreign path: a counter, an eventfd write to every other driver
  (bridge.c:2209-2224), and a scan of all parked contexts on the next
  harvest. So the borrower needs a "borrowed view": a thread-local that
  points lookup at the victim and collects into the detach list instead of
  calling `wf_context_ready`. This is new state on the most sensitive path in
  the runtime.
- *Memory ordering of migration.* Unchanged from today's borrow. The owner's
  release of `OUTSIDE` pairs with the borrower's acquire CAS. The frame was
  last written before that release. The run-queue spin lock publishes the
  context to the borrower's next `wf_run_take`. CQE contents pair through the
  kernel's tail and `record->issued` (linux_io_uring.c:975-990).
- *Reaping inside the borrow means host calls under the token:* the kick and
  overflow `io_uring_enter`, the readiness re-arm with its own kick
  (linux_io_uring.c:1013-1021), `fstat` and `close` for opens
  (linux_io_uring.c:893), and `setsockopt` for accepts (linux_io_uring.c:951).
  The node that sets the borrow subset excludes these so that a borrow stays
  short ([bounded-waits](../../../design/compiler/waiting-contexts/bounded-waits.md),
  second decision).
- *Cancellation and deadlines.* The gaps in 1.4 remain unless the borrow
  also cancels. `wf_bridge_cancel` cancels through the calling thread's ring
  (bridge.c:5351), so a borrower would cancel on its own ring, the wrong one.
  It would need the victim's adapter passed in explicitly.
- *macOS and Windows.* Not applicable: one driver, no peer.

**Latency bound.** The idle park lasts at most 1 ms (bridge.c:3631-3637). A
staleness gate (below) adds one more pass. So the bound is about 2 ms plus
scheduling delay, while some driver is idle. **With every driver busy there
is no bound**, so the reassignment of (c) is needed anyway.

**Fast-path cost.** Without a gate, every borrow pass would reap whatever an
outside owner had pending, even during microsecond context runs. Contexts
would then move from busy drivers to idle ones in ordinary I/O. That is a
placement change, and it means two threads reaping one ring, the prior
objection above. A gate is required: reap only an `OUTSIDE` token seen
unchanged on two passes at least 1 ms apart. With the gate, the owner pays
nothing until a collision. A collision lengthens the owner's return by one
budgeted reap, up to 64 publications plus syscalls.

**Interaction.** It widens the borrow's subset. Reassignment would still be
needed when no driver is idle, so ring work would get two servers, each with
its own protocol.

**Design decisions changed.**

- [bounded-waits](../../../design/compiler/waiting-contexts/bounded-waits.md),
  second decision: the borrow subset gains ring entry, engine locks, host
  calls and cancellation through the victim's ring.
- [waiting-contexts](../../../design/compiler/waiting-contexts.md), first
  decision ("woken ... by an idle driver that has borrowed that driver for a
  due sleep or a record already published"): widened.
- [waiting-contexts](../../../design/compiler/waiting-contexts.md), the
  decision on waking by the record's address: extended to the borrowed view.

### (b) Completions reaped by a thread other than the submitting driver

**(b1) A shared reaper thread** waits on every ring's CQ readiness. The
adapter already polls the ring descriptor as `WF_LINUX_WAIT_CQ`. The reaper
reaps every completion and hands woken contexts to drivers.

- *Correctness.* It reaps under the completion mutex, which is safe. But it
  holds no service token, so it cannot touch a driver's parked list or record
  hash. Every record then takes the foreign path (bridge.c:2295-2296), and its
  context becomes ready only at that driver's next harvest. If that driver is
  computing, the reaper must push the context into some driver's queue and
  wake it, which needs a borrow anyway. With every driver busy it needs a
  spare thread, which is (c).
- *Fast-path cost.* Every completion crosses threads. That means a wake per
  batch, cache misses, and the foreign scan. It also gives up local wakes by
  record address, the change that took the compiled server from 0.64 to 0.94
  of the hand-written shape at 64 connections
  ([waiting-contexts](../../../design/compiler/waiting-contexts.md), sixth
  decision). This would be a regression on every I/O operation.
- *Latency.* Immediate reaping, but no bound on running the woken context
  while every driver is busy.
- *macOS and Windows.* On Windows the port is designed for many consumers,
  but there is only one driver. On macOS there is no ring to reap.

**(b2) A ring per driver, reaped by any idle driver under a lock.** The
locks already exist, so this is (a) without the token. The idle driver can
publish records but cannot wake their contexts, so they wait for the
foreign-path harvest by a busy owner, or for a borrower's scan of 64
parked contexts at a time (bridge.c:3545-3585). With 1,024 parked
connections that is up to 16 borrows. It inherits (a)'s costs and adds the
foreign path's.

**Design decisions changed.** (b1) replaces the local-publication decision
and the first decision of
[waiting-contexts](../../../design/compiler/waiting-contexts.md). That
contradicts measured grounds, so it is a strong prior objection. (b2) changes
the same nodes as (a).

### (c) A bounded service handoff when completions are pending

**Mechanism.** Give `wf_driver_reassign` a second trigger beside the overdue
deadline: the driver's ring has pending work, and one context run on it has
stayed outside service for at least one monitor period. Under the prior
study's names this is protocol A, the
[step-2 reassignment](../driver-handoff/DESIGN.md#result-whole-role-reassignment-for-one-driver-a-first-increment),
whose result already says "detection of overdue readiness and completions
(this increment detects deadlines only)" is still open.

- *The pending-work probe* reads shared ring words with acquire loads, with
  no lock and no syscall. Work is pending when the CQ head differs from its
  tail (completions to reap), when the kernel's overflow flag is set, or
  when the SQ tail differs from the kernel's SQ head (staged entries never
  kicked). The monitor already wakes every 1 ms (`WF_MONITOR_PERIOD_MS`,
  bridge.c:3858).
- *Staleness* is judged by the exact token. The monitor keeps a private
  record per driver: the `OUTSIDE` word it saw with pending work, and when it
  first saw it. A later scan that finds the same word, still pending, at
  least `WF_HANDOFF_TAU_NS` (1 ms) after the first sighting claims it. It
  claims by CAS from that exact word to `PROBE`, rechecks the probe, and
  reserves a spare as the deadline path does. The same word means the same
  context run: every run publishes a new epoch at departure.
- *No idle-peer deferral for this trigger.* A borrow cannot do ring work, so
  deferring to an idle peer would bring back exactly the observed failure.
  The deadline trigger keeps its deferral where a borrow can actually do the
  work (section 5, sub-step 2).

**Correctness.**

- *Who touches the queues.* The replacement reaps and kicks the role's own
  ring, bound by `wf_executor_bind` (bridge.c:3938-3940). The displaced
  thread may still submit to that ring under its submission mutex. This is
  the arrangement the owner already approved and
  `driver_service_native_test.c` exercises. The monitor only reads ring
  words; a stale read can at worst cause one needless probe.
- *No context migrates between drivers.* The role moves between threads.
  The ordering chain already exists:
  1. the owner's release of `OUTSIDE`;
  2. the monitor's acquire CAS to `PROBE`;
  3. the monitor's release of `RESERVED`;
  4. the spare's acquire CAS to `SERVICE` (bridge.c:3943-3955).

  So the parked list, record hash and heap the owner wrote are visible to
  the replacement. The displaced context's frame returns through the ingress
  queue, under a spin lock and a sequentially consistent count
  (bridge.c:3691-3721).
- *Cancellation and deadlines.* The replacement runs the full
  `wf_driver_expire`, and `wf_bridge_cancel` reaches the right ring through
  the rebound thread adapter. So once the role has moved, 1.4's deadline gap
  is closed for that role. Triggering on such a deadline when no completion
  is pending is sub-step 2 in section 5.
- *macOS and Windows.* The ring probe is Linux-only. Reassignment exists on
  every host (`wf_handoff_begin`, bridge.c:4016-4032), but neither host has a
  probe that needs no syscall. On Windows a port cannot be inspected without
  dequeuing a packet. On macOS, readiness needs a `poll`. Records published
  by helpers can be detected on any host under `PROBE`, by comparing
  `wf_context_foreign_publications` with the driver's `foreign_seen`
  (bridge.c:2304-2315). Section 8 keeps these as follow-up work.

**Latency bound.** Let t be the moment the completion is posted. The first
scan that sees it comes no later than t + M. The claim comes no later than
that sighting + τ + M, so ≤ t + τ + 2M = t + 3 ms with M = τ = 1 ms. Then add:

- J: the spare's wake and the OS scheduler;
- H: the replacement's first loop, which reaps at most 64 completions;
- Q: contexts queued ahead on the role.

The same mechanism fired a deadline 3.7 to 6.4 ms late on one hosted CPU
([step 2 result](../driver-handoff/DESIGN.md#result-whole-role-reassignment-for-one-driver-a-first-increment)).

The bound has two limits:

- **Spares.** It holds while a spare is free. There is one spare per driver,
  shared (bridge.c:4018), so it covers as many simultaneous long
  computations as there are drivers. A miss counts `reserve_misses` and
  delivers late.
- **Medium runs.** A run is claimed for sure only if it lasts at least
  τ + M = 2 ms. Shorter runs are covered by the existing reap every 64
  resumptions (bridge.c:3726). So 64 back-to-back runs of just under 2 ms
  each, with a completion pending throughout, can delay it by up to about
  128 ms. Section 8 records this case and why it is not closed here.

**Fast-path cost.** None on the owner's path: the monitor already wakes every
millisecond. The monitor adds about four shared-word loads per driver per
scan. The CQ tail and head lines are written by the kernel and the owner, so
this is at most about one coherence miss per driver per millisecond.
Ordinary I/O context runs last microseconds, so a token is expected not
to survive two scans and the trigger is expected not to fire. This is not yet
measured; section 6.2 checks it with the cost comparison.

**Interaction.**

- Borrowing stays as it is, for sleeps and published records.
- A borrower and the monitor race for one `OUTSIDE` word by CAS, and the
  loser retries on its next pass (`reassignment_races` already tests this
  for deadlines).
- After a handoff the role is in `SERVICE` or `PARKED` on the replacement,
  so borrowers leave it alone.
- The deadline trigger and the new trigger share the reservation, the
  counters, the ingress handback and the stuck-detector accounting
  (`wf_drivers_moving`).

**Design decisions changed.**

- [bounded-waits](../../../design/compiler/waiting-contexts/bounded-waits.md),
  third decision (the monitor): gains the ring trigger, and the idle-peer
  deferral is narrowed to work a borrow can do.
- The second decision there (borrowing): its clause "ring completions that
  need the submitting thread stay with the owner" rests on a premise the
  `COOP_TASKRUN` decision has since removed. It is reworded so that ring work
  stays with whichever thread holds the role's service, the owner or a
  replacement.
- [completion-runtime](../../../design/compiler/completion-runtime.md) and
  [waiting-contexts](../../../design/compiler/waiting-contexts.md) are
  unchanged.

### Other designs considered

- **The idle driver's thread adopts the stale role.** The idle driver's own
  role would be left without a service holder, so its own completions would
  need the same handoff. The parked spare already offers the same thread
  wake at the same cost. Rejected.
- **Spread parked contexts at spawn** (start on an idle driver). This is the
  round-robin placement already rejected for cost (a start-then-join step
  took 35.8 µs against 0.12 µs,
  [waiting-contexts](../../../design/compiler/waiting-contexts.md)). And it
  does not help: the computing context can still share a driver with a
  waiting one. Rejected.
- **Register an eventfd on each ring** so that a completion wakes the monitor
  at once. This would cut detection from about 3 ms to near zero, but the
  kernel would signal the eventfd on every completion of every ring, a cost
  on the fast path for latency the 100 ms target does not need. Kept as the
  refinement to try if detection time ever matters.
- **Safepoints or preemption inside the computation.** A computation has no
  suspension point, a polled safepoint costs a branch per loop back-edge in
  compute code, and it cannot cover native calls
  ([driver-handoff §5](../driver-handoff/DESIGN.md#5-alternatives-and-recommendation)).
  Rejected as the primary mechanism.
- **Having the program yield or move its computation** (for example, Firn
  running scripts differently). This routes around a runtime defect and does
  not fix it. Rejected under the project's rule.

## 4. Comparison

| | (a) reap under borrow | (b1) shared reaper | (b2) idle driver reaps, no token | (c) handoff on pending ring work |
|---|---|---|---|---|
| Serves the observed case (one driver computes, one idle) | yes | only with a borrow added | slowly (foreign path, 64 parked per borrow) | yes |
| Serves the case where every driver computes | no | no | no | yes, while spares last |
| Staged SQEs, ring-operation deadlines | needs kick and cancel through the victim's ring under borrow | kick yes, deadlines no | kick yes, deadlines no | yes (deadlines via sub-step 2) |
| Bound | about 2 ms + J, idle peer required | none when all busy | tens of ms at 1,024 parked | τ + 2M + J + H + Q ≈ 3 ms + J; 64-run edge about 128 ms |
| Owner fast-path cost | none if gated; collisions extend the owner's return | every completion crosses threads | foreign path on every assisted record | none |
| New mechanism | borrowed view in publication, victim-adapter cancel, gated reap | a new thread and queue crossings | none, but slow | a pending probe and a staleness record in the monitor |
| macOS and Windows | not applicable | not applicable | not applicable | structure applies; probes are follow-up work |

## 5. Recommendation

**Adopt (c), the handoff on pending ring work, as defined above, in two
sub-steps.**

**Decisive reason.** The case where every driver is computing needs whole-role
reassignment whatever else is built, because only a spare thread can serve a
role then. Reassignment already serves the whole role: reaping, kicking,
cancelling, deadlines and ingress. It is owner-approved, measured within the
cost criterion and covered by deterministic tests. Once its trigger
recognizes ring work and stops deferring to idle peers for it, it also serves
the observed idle-peer case. (a) would add a second server for the same work,
with new invariants on the publication path and longer borrows, and would
still leave the all-busy case to (c). (b) puts a cost on every completion.

**Sub-step 1 (closes the observed failure):**

- the ring-pending probe;
- token-exact staleness of one monitor period;
- reassignment with no idle-peer deferral when ring work is pending;
- a separate counter, `ring_reassignments`, for attribution.

**Sub-step 2 (closes 1.4's deadline gap):** while peers are idle, the monitor
still probes a role whose deadline hint is overdue. Under `PROBE` it hands
the role off when the overdue heap minimum is not a timer-route sleep, or
when a cancellation is pending, since those are work a borrow cannot do. To
stop repeated probes after a borrow completes a sleep, the borrower refreshes
`deadline_hint` before releasing the token. This sub-step is recommended
because the gap is of the same kind, but it was found by reading, and its
first test decides whether it is real.

**Choices inside the recommendation, with their reasons.**

- *Staleness is one uninterrupted run (the exact token), not time since the
  last reap.* A loaded server reaps only every 64 runs. At depth 16, 64 runs
  of 20 to 50 µs take 1.3 to 3 ms, and completions are nearly always pending.
  A rule based on time since the last reap would fire constantly in ordinary
  I/O. The cost of this choice is the 64-run edge in section 3.
- *τ equals the existing `WF_HANDOFF_TAU_NS` (1 ms)*, so one constant governs
  both triggers. This is provisional: reopen it if `ring_reassignments` shows
  in an ordinary workload's profile.
- *No change to the ring flags, the specification or the borrow subset.*

**Design-tree changes to propose on the work branch.** These are proposals
until the owner approves them on their card.

- [bounded-waits](../../../design/compiler/waiting-contexts/bounded-waits.md),
  third decision:
  - add "or stays outside in one context run for a monitor period while its
    ring holds completions, an overflow or staged submissions, whether or not
    a driver is idle";
  - with sub-step 2, add "or past a due deadline whose wait is not a sleep, or
    with a pending cancellation";
  - the grounds: the Firn witness (echo at 1,846 to 1,938 ms with
    `stolen_contexts=0`, 1.2), the cost result and the new latency result.
- The same node, second decision: reword the ring-completion clause as in
  section 3 (c).
- The same node's `Rejected:` list gains:
  - reaping under borrow, because it adds a second server for ring work with
    host calls under the token and a borrowed publication view, and still
    needs reassignment when every driver computes;
  - a shared reaper, because every completion would then cross threads and
    lose the local wake by record address;
  - time-since-reap staleness, because a loaded server's reap gap exceeds
    1 ms in ordinary I/O.

## 6. Registered experiment (written before any measurement)

### 6.1 Latency

**Witness.** A new Whitefoot-side witness, `research/experiments/ring-assist/`
(witness, client and `README.md` with this prediction), built from the same
source for every arm. It mirrors Firn's witness: two accepted connections,
one context each, a `go N` line that computes about 2 s with no wait, and an
echo. The client measures send-to-echo time on its monotonic clock and checks
the compute reply against its own recurrence oracle.

**Two placement arms**, because Firn's accept-order alternation does not vary
placement (1.2):

- *Same driver.* As Firn's witness: both contexts park on driver 0.
- *Split.* After spawning the echo connection's context, the root computes
  about 5 ms before its next wait. The child stays ready on driver 0 long
  enough for the idle driver's 1 ms loop to steal it (bridge.c:3776). The
  echo's receive then sits on driver 1's ring.

Each run's placement is read from its report, not assumed:
`stolen_contexts ≥ 1` on driver 1 means split; `ring_reassignments ≥ 1` means
the same-driver case was served by the new trigger.

Amendment, after the first hosted mutants run and before the registered
experiment of this section ran: the two clauses overlap, since a
handoff makes the echo's context ready on driver 0's role and driver 1 may
then steal it. A run with both counts is classed same-driver, served by the
trigger: split means `stolen_contexts ≥ 1` on driver 1 and
`ring_reassignments = 0` on driver 0. The formal test's first hosted run (4 CPUs), which
the amendment observed, placed all three same-driver attempts as split under
the original rule, each with its echo inside the bound; that test is not this
experiment, and the latency criterion below is unchanged.

**Configurations.** Hosted ubuntu-24.04, with one confirming run on the
14900K. Arms: `WF_DRIVERS=2` on 2 CPUs; `WF_DRIVERS=2` on 1 CPU; a
zero-step control; and the base compiler with the same arms. Start with 3
runs per arm, then 30 interleaved runs per arm, keeping every row. A
temporary workflow on the branch runs it and is removed before the pull
request is marked ready.

**Latency criterion.** With the candidate, in every run with
`reserve_misses=0`, the echo arrives within 100 ms of its send, in both
placement arms and at both CPU counts. The compute reply arrives after the
echo, with the oracle's value.

**Controls.**

- The base must show same-driver echoes after 1 s or more. Otherwise the
  witness does not detect the fault and the run is inconclusive.
- In the candidate, same-driver runs must show `ring_reassignments ≥ 1`.
  If they are fast without it, the attribution has failed: the run is
  inconclusive and the cause must be found.
- Split runs must be fast in both base and candidate.

### What ran

The registered witness `research/experiments/ring-assist/` was not built. The
arms ran as the program witnesses of section 7, `ring_assist.rs` and
`ring_assist.wf`, which keep the registered shape: one source for both
placement arms, a recurrence the client checks against its own oracle, the
send-to-echo time on the client's monotonic clock, and each run's placement
read from the drivers' report lines. The criterion (every echo inside 100 ms
in both arms at each CPU count, the same-driver echoes credited to the ring
trigger) is unchanged. The apparatus differs from the registered plan:

- The computation is 1.5 billion steps first, about 0.2 s on the hosted
  runners, not about 2 s. A run whose computation answers before an echo that
  stayed inside the bound decides nothing and is repeated with twice the
  steps, up to four doublings.
- The base arm is the same files under the deadline-only trigger (the work
  branch with its ring trigger switched off), not the base compiler, which
  lacks the `ring_reassignments` field the placement reads. The control
  "same-driver echoes after 1 s or more" therefore reads "after the
  computation ends": 134.8 ms, over the bound, in the quick workflow run
  `40052611550`. The busy-ring receive test reads no new field, so it also ran
  on main itself, where the receive did not release the computation within
  100 ms (the same run, job `main-busy`; and `d1a3efcba`, section 9).
- Configurations: hosted ubuntu-24.04 with the program pinned by `taskset` to
  1, 2 and 4 CPUs and `WF_DRIVERS=2`, 30 invocations each, not 3 runs then 30
  interleaved with base runs. No zero-step control and no 14900K latency run
  were made.
- The idle driver may take the echo's context before it parks, which makes
  that run split. An arm therefore takes up to eight runs to show its
  placement, and the echo of every run, shown or not, has to be inside the
  bound. An earlier run with a cap of three had one 2-CPU invocation in ten
  whose three runs were all split, with echoes of 76 to 105 µs.

Result on head `079390bd6` (quick workflow run `40052611550`, job
`latency-arms`): all 30 invocations passed at each CPU count. At 1 CPU, 30
same-driver and 30 split runs, every one showing its arm's placement. At 2
CPUs, 40 same-driver runs, of which 30 showed the placement and 10 were
split, and 30 split runs. At 4 CPUs, 30 and 30. The longest echo of any run
was 4.0 ms (split arm, 1 CPU), against the 100 ms bound. Each invocation's
same-driver run that showed the placement had `ring_reassignments` of at least
1 on the computing driver, which the case asserts.

### 6.2 No regression on ordinary I/O

**Harness.** The `io-bench.yml` firn comparison, dispatched with:

- `compare_firn=true` and `compare_runner=14900k`;
- `compare_revisions="base=<merge base> base-twin=<merge base> ring=<head>"`;
- `compare_cpus="1 2"`, `compare_tests="mset set get"`,
  `compare_depths="16 1"`;
- `compare_passes=2` for the probe, then 6 for any cell whose twin spread is
  too wide, and `compare_seconds=5`.

The images are measured interleaved, and `base-twin` measures the base image
under a second name as the noise control. The 14900K is checked to be idle,
and the coordinator is told first.

**Thresholds (provisional; they are the criterion the owner accepted for
driver handoff on 2026-10-10):**

- in every cell, median throughput ring ÷ base ≥ 0.97;
- p99 ring ÷ base ≤ 1.05 wherever the twin's p99 spread (base-twin ÷ base)
  stays within 5 percent;
- a cell whose twin spread exceeds a threshold is inconclusive and is rerun
  alone with six passes, as the handoff record did for mset at depth 16 on two
  CPUs.

**Counter check.** Each server's `WF_SCHED_REPORT=2` lines are kept (the
harness needs a small change to export the setting and keep the report
lines from `server.log`). `ring_reassignments` should be 0. A nonzero count
is reported with its rate and attributed in the profile.

Amendment, before the comparison on the head that merges: the counter check
cannot be run. The Firn server handles no signal and has no `SHUTDOWN`
command, so the harness stops it with `SIGTERM` and the runtime, which prints
its report only at a normal exit, prints nothing. The criteria below are
unchanged and the comparison reports throughput, with no counter. The unit
cases cover the trigger's silence only in part: an idle ring reads as not
pending, a first sighting of ring work hands nothing over, and a new context
run starts a new sighting.

### 6.3 What rejects the recommendation

- Any candidate latency run with `reserve_misses=0` and the computation still
  running has an echo later than 100 ms.
- Any comparison cell has throughput ring ÷ base below 0.97, or a p99 ratio
  above 1.05 that the twin's spread resolves.
- `ring_reassignments` fires in ordinary comparison runs at a rate whose cost
  appears in the profile. Firn's comparison cannot print the counter (the
  amendment in 6.2), so only the throughput and p99 bullets above can reject
  on the comparison; the count stays checkable where a program exits normally.

Rejection reopens (a) for the idle-peer case, or a different staleness rule.
It never weakens the criterion.

**Downstream check, not a Whitefoot gate.** Once 6.1 and 6.2 pass, publish an
experiment compiler release of the branch head (`compiler-release.yml`). Firn
then reruns its witness and its two engine-pool tests 10 times in a row. Its
acceptance belongs to Firn's session.

## 7. Implementation plan

**Functions.**

- `linux_io_uring.c` and `.h`: add `wf_linux_io_uring_pending(const adapter*)`.
  It returns nonzero when the CQ head differs from its tail, when
  `IORING_SQ_CQ_OVERFLOW` is set, or when the SQ tail differs from the
  kernel's SQ head. It uses acquire loads only, takes no lock and makes no
  syscall, and may be called from any thread as a hint.
- `bridge.c`:
  - `wf_driver_ring_pending(driver)`: the root role uses the process ring and
    every other role its own ring; it answers 0 without a ready ring and on
    other hosts.
  - A monitor-private staleness record per driver index (token, first
    sighting), written only by the monitor thread.
  - `wf_monitor_scan`: maintain the staleness record; call the ring path once
    a token is stale.
  - `wf_driver_reassign`: split into a deadline path, unchanged except for
    sub-step 2, and a ring path. The ring path takes the stale token, makes an
    exact-token CAS to `PROBE`, rechecks `wf_driver_ring_pending`, and
    reserves and notifies a spare as today. It does not consult
    `wf_drivers_idle`.
  - `wf_driver_statistics` and `wf_driver_report`: add `ring_reassignments`,
    appended at the end of the line, because
    `research/experiments/context-starvation/run.sh:95` reads the fields
    before it by regular expression. The exact-string check at
    `driver_service_test.c:248` gains the field.
  - Sub-step 2: under `PROBE` in the deadline path, an overdue heap minimum
    that is not a timer-route record, or a set `cancel_pending`, qualifies
    even while peers are idle. `wf_driver_assist` refreshes the victim's
    `deadline_hint` after `wf_driver_service_only`.
- `bridge.c` is already on `.github/oversized-sources.txt`, so the growth
  needs no new entry.

**Tests.** Each test fails on the base revision, which is the first mutant.
Every other mutant is demonstrated once by a temporary commit on the branch,
run in CI and then reverted. Its run is cited in the pull request, and no
production hook is added. The native tests use the existing
`driver_service_native_test.c` fixture: a pipe read submitted on the root
ring and an owner held outside service in user space.

| Test | Guards | Mutant that must fail it |
|---|---|---|
| T1 (native): completion pending, idle peer present (`wf_drivers_idle=1`), no deadline. The first scan does not reassign; a scan τ later does; the replacement reaps and the reader resumes on executor 1. | the ring trigger | M1: deadline-only trigger (base). M2: the ring path keeps the idle-peer check. M3: reassign at first sighting. |
| T2 (native): between the two scans the owner returns and leaves again (new epoch), so the second scan must not reassign; a third scan τ after the new sighting does. | token-exact staleness | M4: staleness keyed by phase rather than the exact token. |
| T3 (native): submit without a flush (staged SQE), write to the pipe. The CQ stays empty; the trigger fires on the SQ; the replacement's progress kicks and reaps. | staged submissions | M5: the probe ignores the SQ. |
| T4 (Linux unit, synthetic adapter words): the probe answers 0 when all three conditions are clear, and 1 for each alone. | the probe | M6: overflow ignored; M5 again. |
| T5 (native): a borrower holds `BORROWED_SERVICE`, so the ring path returns 0 and the token is unchanged; after release it reserves. A stale token (owner returned) returns 0. | exclusive claim | M7: the ring path reserves from the hint without the exact CAS. |
| T6: no spare free, so the ring path returns 0, `reserve_misses` rises by 1, the token is back to `OUTSIDE` and `wf_drivers_moving` is restored. | the miss path | M8: `PROBE` or the moving count leaked on a miss. |
| T7 (sub-step 2, `driver_service_test.c`): an overdue non-sleep wait with an idle peer is reassigned; an overdue sleep with an idle peer is not (the borrow serves it). | narrowed deferral | M9: deferral for every deadline (base). M10: no deferral at all. |
| T8 (sub-step 2): after a borrow completes the only due sleep, the victim's `deadline_hint` no longer reads as due. | probe churn | M11: the hint is not refreshed. |

Mutants M1 to M8 were run as listed. A different M9 was run in place of the
planned one: the claim compares against the `OUTSIDE` word read afresh, not
the sighted one, and T5's stale-word case fails it. The planned M9 to M11 and
T7 and T8 belong to sub-step 2, which is not implemented.

The existing `reassignment_races` case (no reassignment with an idle peer for
a timer sleep) must keep passing unchanged. These tests join the existing
`driver-service-test` and `driver-service-native-test` targets and their
sanitizer and TSan variants (compiler/Makefile:565-625). All of it runs in
CI, and nothing runs on the Mac.

**Order of work.**

1. Sub-step 1, with T1 to T6.
2. The latency experiment 6.1.
3. The cost comparison 6.2.
4. Sub-step 2, with T7 and T8, measured again only if it changes a fast path
   (it touches borrow passes only).
5. The completion review, then the design-tree card.

## 8. Open questions and what could overturn this

- **The 683 borrows in Firn's server report** are about 52 a second. The
  witness's driver 1 borrowed about 1,900 a second. Firn's server may have
  kept its idle driver busy or descheduled. The CI host's scheduler delay
  enters J for every candidate. The latency arm on 1 CPU measures that
  directly.
- **`borrowed_terminals=1` in five of the ten witness runs** is unexplained.
  It is not tied to accept order. A record published by a helper during the
  computation, for example a standard-stream write, would explain it. It does
  not affect the stall.
- **The 64-run edge**, up to about 128 ms for runs just under 2 ms each, is
  left open. Closing it needs a rule that sees "no reap for a long time"
  without firing under ordinary load: for example, CQ-head staleness with a
  much larger threshold, about 25 ms, which a loaded server reaching 64 runs
  per reap does not hit. Reopen it if a workload shows it.
- **Spare exhaustion.** With D drivers, at most D displaced computations are
  covered. More concurrent long computations meet `reserve_misses`, and the
  bound fails visibly rather than silently.
- **Hosts with one driver (macOS, Windows)** have no idle peer, and only
  deadlines trigger a handoff there today. A computing context therefore
  holds back helper publications, readiness polls (macOS) and port packets
  (Windows). The helper-publication trigger works on every host under
  `PROBE`. Readiness and port probes need a syscall the monitor does not
  make. This is a follow-up item, outside this question.
- **Language promise.** Whether [WAIT-2] should state a latency profile is an
  owner decision this study does not take. Its recommendation is to keep the
  bound an implementation goal until the experiment and Firn's acceptance
  have run.
- **Ring flags.** Every candidate requires a ring that any thread may enter.
  A later move to single issuer with deferred task running would reopen this
  study.
- **Kernel posting while the submitter computes** is observed for a pipe read
  (`driver_service_native_test.c`). For a socket receive it is expected to
  work the same way, through signalled task work, but has not yet been
  established by the program witness: its former zero-steal classification
  did not directly observe the receive's ring or the computing executor.

## 9. Busy-receive witness audit (2026-10-13)

A reported main-only run of `busy_ring_receive.wf` passed all sixteen
invocations, including invocations classified as same-driver. The audit read
code only, before the replacement observation was built or run, to find what
could make the witness pass on main; the result at the end shows the reported
revision was not main. The audit's comparison used main
`5b7838b328632999712aacf8313717b6dca1ade6` and branch
`f9f7dc5ee2faaf74a435a6ca05ccde0b32fbb5db`.

The computation already has the required uninterrupted shape. Its one
atomic statement encloses the loop. Lowering initializes a lazy taken flag
outside that loop and skips acquisition after the first take; the backedge
is an ordinary jump. `decision_cell_read` calls a sequentially consistent
load, not a waiting operation. The 64-pass cooperative boundary occurs
before acquisition, not on each decision-cell read. Replacing this loop
with another computation would not explain the reported pass. See the
[lazy acquisition flag](../../../compiler/src/lowering/builder/atomic.rs),
[loop backedge](../../../compiler/src/lowering/builder/loops.rs), and
[main's decision load](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L3826)
and [cooperative boundary](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L2031).

Zero `stolen_contexts` is not a general placement proof. The idle driver's
service-only borrow can detach a published terminal record or a due sleep
and resume its context on the borrowing driver without incrementing that
counter. However, the direct arm has no identified post-spawn operation
that uses this escape before its receive. This is a hole in the former
evidence, **not a demonstrated explanation of the reported main pass**.
The migration is in
[main's borrow path](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L4691).

With a native ring, this valid nonempty TCP receive routes to the current
executor's ring before readiness or helper fallback. Accept provenance does
not select the later receive's ring. Main's idle peers service only runtime
records already published or due sleeps; they do not reap another ring.
Its monitor only hands off for a due deadline, and the witness supplies
none. Spare threads and the monitor park on private wake endpoints. Ring
setup does not enable SQPOLL. The audit therefore predicted that a pending
receive on the computing executor's ring would not complete while that
executor remained in this loop.
The submission may still be staged: main can select the ready child before
flushing its deferred doorbell. Both staged submissions and unreaped CQEs
are shapes the branch's ring trigger covers. The relevant main paths are
[ring selection](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L354),
[dispatch](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L5755),
[borrow service](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L4628),
[deadline handoff](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L4950),
[spare parking](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L5046),
[ready-child selection](https://github.com/Ming-Research/Whitefoot/blob/5b7838b328632999712aacf8313717b6dca1ade6/compiler/src/backend/completion/bridge.c#L4845), and
[ring setup and deferred submission](../../../compiler/src/backend/completion/linux_io_uring.c).

The replacement harness observed ordinary calls with link-time wrappers. It
recorded the receive's actual adapter and the physical thread whose context
wait returned 1, then recorded the thread claiming the computation's entry
cell after its first shared-state read. It sent the peer's byte only after both
records existed. Equal thread IDs, with the wait recorded first,
establish the required schedule. Different IDs in the direct arm are left
unclassified because a role handoff can also change physical threads. The
explicit split arm retains its independent nonwaiting entry spin. See the
[observer and harness](../../../compiler/tests/programs/busy_ring_receive.rs)
and [source witness](../../../tests/programs/busy_ring_receive.wf).

The wrappers do not submit, flush, reap, move a context or release the
computation. The placement writes precede the measured peer send and occur
outside the spinning loop. The audit predicted that a directly observed
same-thread run on main would miss the receive bound because only that
receive can release the computation. On this branch, the monitor sees pending
ring work in the same OUTSIDE epoch, reserves a spare, and binds it to the entire driver role;
ordinary service then flushes/reaps the receive. The 100 ms bound is a test
criterion, not an unconditional host scheduling guarantee. See the branch's
[ring-watch trigger](https://github.com/Ming-Research/Whitefoot/blob/f9f7dc5ee2faaf74a435a6ca05ccde0b32fbb5db/compiler/src/backend/completion/bridge.c#L5050)
and [whole-role reservation](https://github.com/Ming-Research/Whitefoot/blob/f9f7dc5ee2faaf74a435a6ca05ccde0b32fbb5db/compiler/src/backend/completion/bridge.c#L5004).

**Result.** The audit's prediction holds. The run reported as main's was of
`98ea9f486`, which contains the implementation commit `7a640099e`
(`git merge-base --is-ancestor 7a640099e 98ea9f486` succeeds), in the native
Linux step of io-hosts run `38433098848` with `WF_REQUIRE_LINUX_IO_URING=1`,
so its pass was the branch's. The same witness files on a revision without the
implementation, `d1a3efcba` (main at `3adedae9b` plus the witness), failed in
the gate's corpus job, run `38437669835`: the direct arm's first invocation,
with the wait and the computation recorded on one thread, reported "the
receive did not release the atomic computation within 100ms", and the
Firn-shaped same-driver echo case of `ring_assist.rs` failed in the same run.
The timestamped diagnostics added to chase the reported pass were removed once
the cause was known, so the observer in the test is again the placement
observer alone; the verdict logic is the same as in the run that failed on
main. On the branch the test passes, and with only the deadline trigger kept
it fails with that message.

## 10. The busy-receive witness under ThreadSanitizer

The first full check of the finished branch failed one job, the
thread-sanitized programs part of the sanitizers workflow (run `40054875044`,
head `c777467fe`): the direct arm of `busy_ring_receive.rs` reported that the
receive had not released the computation within 100 ms, at the fifth of its
sixteen invocations (run 4); the test ends at its first failure, so the other
eleven did not run. The ring trigger, not the sanitizer's speed, was the first
suspect, so the failure was measured before anything changed.

**Measurement.** The thread-sanitized programs part was repeated in parallel
jobs on the hosted runner, each job a whole corpus run on 4 CPUs.

| Runs (head) | Witness exit sleep | Jobs with a late exit | Late exits after the send |
|---|---|---|---|
| `40055683756` (`0cd64a216`, bound 1 s) | default | 1 of 3 | one over 1 s, after 7 invocations of 13.0 ms to 28.1 ms; the 32 invocations of the other two jobs took 6.9 ms to 42.0 ms |
| `40056461636` (`58e8c9f78`) | default | 6 of 8 | 1.011 s to 1.023 s, each recovering by itself; four split-arm and two direct-arm invocations |
| `40057162081` (`ec33293ec`) | default | 4 of 6 | 1.008 s to 1.028 s, one per job (the job's test ended at it) |
| `40057162081` (`ec33293ec`) | `atexit_sleep_ms=0` | 0 of 6 | none: 96 invocations, longest 30.6 ms, mean 19.6 ms |

Every late exit came 1.0 s plus an ordinary exit time (10 ms to 28 ms) after
the send, in both placement arms, and every one recovered: the release had
happened within the usual tens of milliseconds, and the exit was held back by
one second. At the moment of the stall the process had two threads, both in
`nanosleep` (read from `/proc/<pid>/task/*/syscall`, four jobs): the main
thread and one other.

**Cause.** ThreadSanitizer's `atexit_sleep_ms` defaults to 1000 and is applied
when the main thread finalizes while the thread count exceeds one
(`compiler-rt/lib/tsan/rtl/tsan_rtl.cpp`, `Finalize`; `tsan_flags.inc`). The
witness observes the program's exit as the only sign that the computation was
released, so the sleep lengthens the measured interval by one second in the
invocations where a thread still lives when the main thread finalizes. The
first full-check failure at 100 ms is, by the control above, the same event:
its bound was 100 ms, so whether that exit would have recovered after about a
second was not observed. Which runtime thread is still alive at that moment
was not identified; the control and treatment rows above, which differ only in
this option, show that the option decides the outcome. The runtime's handoff is not implicated: nothing in the data shows a
release later than the ordinary tens of milliseconds.

**Result in the tests.** `busy_ring_receive.rs` starts its program with
`atexit_sleep_ms=0` appended to `TSAN_OPTIONS` under ThreadSanitizer and keeps
the 100 ms bound in every build. Races are still reported, and the other
programs of the sanitized corpus keep the default. `ring_assist.rs` measures
the echo's arrival, not an exit, and its sanitized echoes took 1.4 ms to
2.3 ms in the loaded jobs of the first run above, so it too keeps 100 ms.
