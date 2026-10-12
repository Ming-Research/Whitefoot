# Ring assist: serving a computing driver's completions

## Status and question

**This is a draft proposal. The owner ruled for candidate (c), extending
the monitor's whole-role reassignment, and sub-step 1 of section 5 is
implemented on the work branch with the tests of section 7. The hosted
gate, `io-hosts` and the mutants workflow pass. The formal program test's
hosted runs (4 CPUs) kept every echo inside 100 ms. The registered
experiment of section 6.1 (1 and 2 CPUs, base controls, 3 then 30 runs per
arm) and the 14900K cost comparison of section 6.2 have not run, so the
latency and ordinary-I/O results are still unmeasured and nothing here
promises a latency.** The study itself was read-only, of `main` at
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
measured; section 6.2 checks it with a counter and the cost comparison.

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

### 6.3 What rejects the recommendation

- Any candidate latency run with `reserve_misses=0` and the computation still
  running has an echo later than 100 ms.
- Any comparison cell has throughput ring ÷ base below 0.97, or a p99 ratio
  above 1.05 that the twin's spread resolves.
- `ring_reassignments` fires in ordinary comparison runs at a rate whose cost
  appears in the profile.

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
  observed. The witness's same-driver arm observes it.
