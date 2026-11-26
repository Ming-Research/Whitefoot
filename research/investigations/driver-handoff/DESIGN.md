# Driver handoff for waiting contexts

## Status and question

**Draft proposal; no implementation or latency guarantee is established.** It was written as a read-only design study by a delegated model (Codex gpt-6.1-sol) and is kept as the investigation's starting record; the sequencing and the choices it leaves open go to the owner before any implementation.

This study inspected `claude/ctx-handoff` at `6bade699860d534f622d670ec5476fada4fbbfa7`, against the supplied base `8ecd485b81b44dbbcfc95860d06c7c12708694aa`. No files were changed, and no compiler, build or test was run.

The owner selected runtime driver handoff and idle-driver assistance in card `firn-q-ctx-starve`. This draft proposes their protocol. The monitor, thread inventory, ownership representation and numerical bounds below remain recommendations.

**Question:** can a waiting context receive timely service while another context computes continuously, including with one logical driver on one CPU, without materially increasing ordinary I/O cost?

The proposed answer is to keep each logical driver object stable and transfer its service ownership between physical threads. The computing context continues on its existing native stack until a legal suspension boundary.

Two issues must be settled before promising the latency bound:

- Linux `COOP_TASKRUN` may leave completion work dependent on the displaced submitting thread.
- A finite executor pool cannot provide an unconditional bound for arbitrarily many simultaneously computing contexts.

## 1. Current architecture and ownership

Paths below are relative to the repository root. Completion source filenames denote files under `compiler/src/backend/completion/`.

| Fact and consequence | Source |
|---|---|
| A context contains its resume frame, logical driver pointer, group, pending-record pointer, timer and cancellation links, and one operation block. These objects must remain valid throughout a native resume and any pending host operation. | `bridge.c:1264`, `bridge.c:1280`, `bridge.c:1315`, `bridge.c:1323` |
| A driver contains its ready queue, parked-context list and record hash, readiness-polling state, deadline heap, cancellation list, host-wait count, allocation pool, wake runtime and, on Linux, ring adapter. Handoff therefore covers considerably more than the ring. | `bridge.c:1458` |
| Ready-queue mutation uses `run_lock`; `run_count` is atomic. Other threads already enqueue contexts and steal ready contexts. | `bridge.c:1556`, `bridge.c:1573`, `bridge.c:1679` |
| Parked lists, `by_record`, readiness lists, timer heap and cancellation links rely on the driver thread’s exclusive access rather than a common state lock. Concurrently running their existing mutation paths would race. | `bridge.c:1804`, `bridge.c:1816`, `bridge.c:1842`, `bridge.c:1942`, `bridge.c:3211` |
| `wf_context_drive` resumes a coroutine directly on its native thread. It regains control when that resume returns; its normal busy-loop reap interval is 64 resumptions. Continuous computation prevents those reaps. | `bridge.c:3236`, `bridge.c:3257`, `bridge.c:3273` |
| `wf_driver_self` and `wf_bridge_thread_adapter` are thread-local. The latter selects the ring used by ordinary ring submission and progress. Retaining these bindings after handoff would incorrectly imply service ownership. | `bridge.c:348`, `bridge.c:350`, `bridge.c:1523` |
| `wf_context_ready` suppresses a target wake when the target equals `wf_driver_self`; it wakes another driver only when the local ready count exceeds one. A displaced thread retaining this pointer could strand a single newly ready context. | `bridge.c:1631` |
| Foreign notification also excludes `wf_driver_self`. Clearing or validating service TLS is required for both wake paths. | `bridge.c:1988` |
| Completion publication stores DONE before notifying. The completion publisher must not access the record afterward because its consumer may release the containing frame. | `bridge.c:872` |
| The local completion shortcut currently looks through `by_record` before recognizing the running context’s own operation block. Handoff requires reversing that order and restricting hash lookup to service ownership. | `bridge.c:2052` |
| Host `.start` uses the running context’s operation block. Watched dispatch additionally modifies the driver’s cancellation list, relying explicitly on no driver scan occurring before suspension. That assumption becomes false after handoff. | `bridge.c:2192`, `bridge.c:3902`, `bridge.c:4006` |
| Linux SQ/CQ mappings are shared mappings. Submission and completion have separate mutexes; CQE `user_data` identifies the original completion record. Moving service ownership does not require moving or recreating an operation. | `linux_io_uring.h:81`, `linux_io_uring.h:112`, `linux_io_uring.c:258`, `linux_io_uring.c:623`, `linux_io_uring.c:743`, `linux_io_uring.c:1056` |
| SQEs may be staged without a doorbell. Progress kicks them before reaping. The replacement must flush the same adapter, including staged submissions made by the displaced context. | `linux_io_uring.c:762`, `linux_io_uring.c:1055` |
| Ring cancellation addresses the original record on the original ring. The bridge currently obtains that ring through TLS, which is unsuitable when assisting a different driver. | `linux_io_uring.c:994`, `bridge.c:4654` |
| Helper operations are protected by the adapter queue lock and tracked as queued or executing. Helpers publish terminal records independently of driver service. They need no reassignment during handoff. | `file_adapter.c:461`, `file_adapter.c:474`, `file_adapter.c:506`, `file_adapter.c:930` |
| Generic bridge progress can execute a queued blocking open or close when no helper exists. Readiness service can execute accept; ring open classification can perform `fstat` and `close`. These paths cannot simply be called by a supposedly bounded monitor or borrower. | `bridge.c:900`, `bridge.c:1929`, `linux_io_uring.c:833` |
| macOS uses the non-ring readiness/helper routes. Windows uses the global completion port and helper routes; readiness support is absent there. Non-Linux driver startup currently creates no additional logical drivers. | `bridge.c:519`, `bridge.c:744`, `bridge.c:3475`, `file_posix.c:689`, `file_windows.c:975` |
| Timer expiry completes sleep locally but requests engine cancellation for general I/O, retrying when necessary. Expiry service and terminal completion are distinct events. | `bridge.c:3163`, `bridge.c:3172`, `bridge.c:3205` |
| Stop handling has its own lifecycle and queue locks. On POSIX the original launcher receives signals, while a separate entry thread executes Whitefoot. The receiver can use ordinary completion joins, including the root ring. Handoff must preserve this arrangement. | `stop_signals.c:14`, `stop_signals.c:168`, `stop_signals.c:250`, `compiler/src/backend/wf_floor.c:330` |
| Each thread executing Whitefoot needs floor attachment, including its stack bounds and alternate signal stack. Spares need their own attachment rather than a copied attachment. | `compiler/src/backend/wf_floor.c:239`, `compiler/src/backend/wf_floor_windows.c:150` |
| Compute offers, joins and releases belong to the offering physical thread. Compute-worker lane TLS must stay on that thread throughout an outstanding offer. | `compiler/src/backend/sched/core.c:1`, `compiler/src/backend/sched/core.c:1120` |
| Lane-zero attachment is granted once through a process-wide `taken` flag. A context resuming on a different executor may subsequently fail attachment and execute `--par` work on the caller. Correctness and unchanged parallel performance are separate questions. | `compiler/src/backend/sched/core.c:1276`, `compiler/src/backend/sched/core.c:1292` |
| Concurrent-map users are numbered by logical driver today, while each user requires exclusive use by one physical thread. Old and replacement executors sharing a driver number would violate that contract. | `bridge.c:2748`, `compiler/src/backend/keyed_table.c:40`, `compiler/src/backend/concurrent_map.h:46` |
| Stuck detection assumes that idle logical drivers imply no context is computing. After handoff a displaced executor can still compute while every replacement driver is idle. The existing detector could falsely stop that execution. | `bridge.c:3120` |
| Ready stealing excludes the root context, and root completion returns through the original driver loop into cleanup. Servicing the witness on a spare requires separating root-coroutine migration from launcher cleanup affinity. | `bridge.c:1711`, `bridge.c:3282`, `bridge.c:3495` |

The current design records these ownership assumptions in:

- `design/compiler/waiting-contexts.md:1`: drivers, placement and migration at suspension.
- `design/compiler/waiting-contexts.md:7`: one operation block and start/wait/finish.
- `design/compiler/waiting-contexts.md:9`: local publication and periodic reaping.
- `design/compiler/waiting-contexts/bounded-waits.md:1`: an unlocked heap owned by one driver thread.
- `design/compiler/waiting-contexts/bounded-waits.md:9`: driver-owned watched waits.
- `design/compiler/waiting-contexts/concurrent-map.md:9`: driver-numbered map users.

The proposal replaces **thread identity as service ownership**. It preserves stable operation identities, suspension-only context migration, shared-state locking and current-stack compute execution.

## 2. Handoff protocol

### 2.1 The stable driver role

A logical driver remains one stable `wf_driver` object for its entire lifetime. Its role comprises:

- Its wake runtime and Linux adapter, including descriptors, mappings and engine locks.
- Ready queue and queue lock.
- Parked-context list, record hash and foreign-publication cursor.
- Readiness lists and scratch storage.
- Deadline heap and cancellation list.
- Service counters and idle/search bookkeeping.
- Allocation pool and its existing synchronization.

The replacement receives authority to service this object. It does not copy its mutexes, mappings, queues or coroutine frames.

The displaced computing context retains:

- Its native call stack and current coroutine invocation.
- Its context, arena, operation block and retained arguments.
- Any shared-state or keyed-map holdings.
- Its physical compute-lane attachment and outstanding offers.
- A stable **submission-role reference** identifying the driver whose adapter holds its pending operation.

Driver lifetime must cover displaced invocations, committed handbacks, monitor probes, notifications and engine operations.

### 2.2 Ownership token and quiescent point

Introduce one atomic, CAS-comparable ownership token containing an epoch, physical executor identity and phase.

| Phase | Permitted activity |
|---|---|
| `SERVICE` | The named executor may access driver-private service state. |
| `OUTSIDE` | The named executor is running a context; nobody owns private service access. Engine submission and externally locked publication remain permitted. |
| `PROBE` | The monitor exclusively inspects private service state. It executes no coroutine. |
| `BORROWED_SERVICE` | An idle driver performs a bounded assistance pass. It executes no coroutine while borrowing. |
| `RESERVED` | Ownership has been assigned to a replacement that has not yet entered service. |
| `PARKED` | The owner is sleeping in the driver’s service wait; work publication wakes it. |
| `STOPPED` | Admission and service have ended; teardown follows lifetime checks. |

Before invoking `wf__coro_resume`, the executor:

1. Accounts for an active native invocation.
2. Publishes the driver’s minimum deadline and necessary detection hints.
3. Clears service TLS, retaining separate submission binding through the context.
4. Release-publishes `OUTSIDE`.
5. Calls the coroutine.

The `OUTSIDE` publication is the handoff’s quiescent point. It promises that the invocation will not touch private driver fields until it reacquires service ownership.

Use a fresh epoch for each departure and ownership reassignment. Epochs must not silently wrap into a still-observable token. The representation and exhaustion policy require explicit implementation treatment; independent atomic fields are insufficient.

On return from `wf__coro_resume`, the original executor attempts an acquire CAS of its **exact cached token** from `OUTSIDE` to `SERVICE`.

- Success permits ordinary service and adoption of the returned context’s disposition.
- `PROBE` or a temporary borrow requires waiting or yielding until that short pass releases ownership.
- A different epoch or assigned executor means the role was lost. The thread commits the context’s handback and leaves the driver loop.

A stale pointer or matching logical driver index never grants service authority.

### 2.3 Detection and transfer

Recommend one process-wide monitor with a provisional period `M = 1 ms` and starvation age `τ = 1 ms`.

Atomic hints identify candidates:

- Ready-queue or committed-ingress work.
- Foreign completion publication.
- Cancellation requests.
- Published earliest deadline.
- Observable ring readiness.

Hints do not authorize reading the private heap, parked list or record hash.

For an aged `OUTSIDE` candidate, the monitor:

1. Reserves an already initialized spare.
2. Acquires `OUTSIDE → PROBE` with CAS.
3. Checks the current departure epoch, deadline and actual service need.
4. Restores `OUTSIDE` if no qualifying work exists.
5. Otherwise release-publishes `RESERVED` for the replacement with a new epoch and wakes it.

The replacement acquires `RESERVED → SERVICE`, installs service TLS and services the same role. No acknowledgement from the computing thread is needed.

The monitor can inspect DONE records and perform nonblocking readiness inspection. It must not call `wf_context_poll`, execute host requests, resume contexts or wait indefinitely for an engine lock. A failed try-lock defers the probe and contributes to the latency qualification.

For a deadline, its actual instant supplies the age. For readiness without an existing timestamp, the monitor records first observation. This avoids adding a clock read to every publication but can add one sampling period: conservative detection is `τ + 2M`, versus `τ + M` for an already published deadline.

Scan cost must be bounded and measured. A large parked list cannot be hidden inside the nominal 1 ms period.

### 2.4 Submitting after ownership loss

The computing thread can submit before its next suspension. Therefore `.start` must remain safe throughout `OUTSIDE`, including after transfer.

Refactor `.start` to:

- Modify only the running context’s private operation and wait-intent state.
- Submit through the context’s stable submission-role adapter using existing engine locks.
- Use helper, stop or other adapter synchronization already independent of driver ownership.
- Avoid the driver’s heap, cancellation links, parked lists and record hash.

Watched dispatch is the concrete exception requiring change. Store its source and deadline as a context-private intent; install driver-owned cancellation and timer links during post-unwind adoption. Preserve the initial fired-watch check before dispatch. Adoption checks DONE first, then registers and rereads firing/deadline state so a firing before registration is not lost.

Completion publication must recognize the running context’s own operation address **before** considering a local record-hash lookup. Only a validated `SERVICE` scope may use `by_record`; all other publishers use the foreign-publication path.

Split TLS responsibilities:

- Service TLS identifies authority to inspect a driver.
- Submission binding identifies the adapter for the running context.

The ready and foreign-notification “same driver” shortcuts must require validated service ownership. A displaced context spawning one child or waking one join waiter must wake the replacement, even though both refer to the same logical driver.

### 2.5 Suspension and handback

At existing legal wait boundaries, perform an acquire check of the cached ownership epoch. A displaced context must suspend and hand back even when the operation was answered immediately.

These boundaries include host `context_wait`, fairness passes, joins, guard parking and ordinary shared-acquisition retries. Do not introduce suspension inside an atomic block, while keyed entries are held, or between a compute offer and its join. The spin-only `wf__shared_take` path remains non-suspending.

Use three handback dispositions:

- `WAIT`: the context has a pending host, shared, guard or join wait.
- `READY`: the context may resume.
- `COMPLETE`: the context has finished and needs group, arena and lifetime completion.

**Commit only after the native coroutine resume has returned.** Marking `parked_away` inside the coroutine is not proof that its native invocation has unwound.

A displaced executor commits through a driver ingress queue protected by a lock. Give ingress a separate linkage field rather than reusing a ready, parked or shared-wait link. Once committed, the publisher must not access the context again.

The current owner adopts the intent:

- For host WAIT, inspect DONE before parking; otherwise install hash/list, host-wait count, deadline and cancellation registration, then recheck relevant conditions.
- For READY, enqueue exactly once.
- For COMPLETE, perform the existing context/group completion and release.
- Remove a terminal operation’s registration before allowing its context to migrate to another logical driver.

The normal owner can adopt its returned intent directly after reacquiring `SERVICE`, avoiding an ingress lock on that path.

External join, shared and guard wakes also need an unwind gate:

| Gate state | Meaning |
|---|---|
| `ARMING` | Wait registration exists, but native unwind/adoption is incomplete. |
| `ARMING_WOKEN` | A wake occurred; it must not yet enqueue or resume the context. |
| `PARKED` | Adoption is complete and waking may enqueue it. |
| `QUEUED` | Exactly one wake has claimed enqueueing. |

A waker changes `ARMING → ARMING_WOKEN`, or claims `PARKED → QUEUED`. The adopting owner changes `ARMING → PARKED`, or `ARMING_WOKEN → QUEUED`. Registration cleanup and queue publication must follow those transitions so no context resumes or is destroyed before unwind.

### 2.6 In-flight operations and cancellation

**Linux ring operations stay on their original ring.** Submitted and staged SQEs retain their record addresses. The replacement flushes, reaps and cancels through that adapter. Never cancel and resubmit merely to move an operation to a different ring.

The displaced thread may continue submitting its own operation under `submit_lock`; the replacement reaps under `completion_lock`. Transfer of private driver ownership does not replace these locks. Preserve DONE as the publisher’s last record access.

**Helpers keep their queued or executing operation.** Their terminal publication is harvested by the replacement. A completion occurring before handback is observed during adoption; a later completion follows the normal foreign-publication and wake path.

**Cancellation follows the victim operation’s adapter.** Assistance must pass that adapter explicitly rather than use the assisting thread’s ring TLS. Preserve:

- A host outcome produced first wins.
- Cancelled/deadline outcomes transfer nothing.
- Engine retry and helper interruption rules.
- Record/buffer lifetime until the engine has finished.

A 25 ms service target does not imply that every cancelled native operation terminalizes within 25 ms.

### 2.7 Idle-driver assistance

Implement D as a bounded **try-borrow** pass:

1. An idle executor retains ownership of its own role.
2. It tries to acquire another role in `OUTSIDE`; it never waits to borrow.
3. It binds the victim’s service adapter and performs bounded service work.
4. It detaches eligible terminal-ready contexts.
5. It restores the victim’s original `OUTSIDE` token.
6. It queues the detached contexts on its own role.

It executes no coroutine while borrowing. Pending operations, timers and cancellation registrations remain with the victim role.

Already ready contexts can continue to be stolen under `run_lock`. Wake an idle driver for one eligible ready context when the source owner is computing; the existing `run_count > 1` condition is insufficient.

Introduce a nonblocking service-only primitive for monitor/borrow use. It must not blindly call generic bridge progress, which can execute blocking host work. Potentially blocking open classification and readiness execution must be deferred to an appropriate execution route, or their delay must be included explicitly in the qualified service bound.

D reduces unnecessary spare use with several drivers. It cannot solve the one-driver witness alone.

### 2.8 Root completion, stuck detection and shutdown

A suspended root coroutine must be eligible to resume on a replacement or assisting executor. Keep affinity only for the launcher’s return and cleanup.

The original entry’s root-run wrapper becomes a coordinator waiting for root completion. A spare publishes root completion; it does not run `wf_drivers_end` merely because it owns logical driver zero.

Extend stuck detection to include:

- Active native invocations, including displaced computation.
- Committed ingress and contexts still arming waits.
- Transfers, probes, borrows and reserved replacements.

Publish WAIT/READY/COMPLETE before the executor's even sequence store. Collect the executor sequences before role counts in the coherent double collect; transfer and idle transitions must participate in its change/moving accounting. Otherwise all logical roles can appear idle while displaced computation will later fire a guard.

The first implementation used a global active-invocation counter. It was replaced by per-executor odd/even sequences with a double collect over all of them; the collection protocol and its argument are in [suspension accounting](../suspension-accounting/DESIGN.md#3-the-invocation-sequence-and-the-snapshot-argument).

Shutdown must:

1. Stop new admission and monitor reservations.
2. Resolve probes, borrows and assignments.
3. Wait for root completion, context/group completion, ingress adoption and native invocations.
4. Stop executors and helpers while preserving required completion service.
5. Drain existing notification lifetime guards and new monitor/executor references.
6. Destroy rings only after their existing in-flight/SQ/CQ conditions permit it.
7. Destroy wake runtimes and driver storage after the last possible publisher.

Preserve stop receiver affinity, lifecycle acknowledgements, signal masks and console-handler generation protection. Do not hold service ownership across an unbounded stop-control acknowledgement.

## 3. Spare threads and the conditional bound

### Inventory

Recommend an initial pool of `D` warm spares for `D` logical drivers:

- At most `P = 2D` context-executing physical threads.
- One monitor, which never executes Whitefoot contexts.
- Existing compute workers, helpers and stop receiver remain separate.

Create and floor-attach the reserve before the first context dispatch that can require independent service. Park unused spares; do not spin them. Reuse displaced executors after handback rather than creating a new thread on each transfer.

With the existing driver ceiling, `P ≤ 128`. This requires:

- Distinct physical executor IDs for concurrent-map users.
- A corresponding map-user ceiling and static assertions.
- Updated heap-counter inventory for entry, executors, compute workers and any allocating observer.
- No post-startup monitor allocation that silently exceeds that inventory.

Map identity must follow the physical executor while it holds map state. It must not follow the transferred logical role. The current map embeds every user slot, so increasing the ceiling enlarges every map and its construction work (`concurrent_map.c:246`, `concurrent_map.c:2761`).

**Limit:** repeated handoffs can occupy every executor with computation. A finite pool cannot guarantee bounded continuation latency for an unlimited number of such contexts. Reserve availability and the permitted simultaneous-computation count must be explicit profile conditions. Exhaustion must not drop an operation or falsify acceptance; actual resource failure remains a `SCOPE-3` condition.

### One CPU

With one CPU, the OS time-slices the computing executor, monitor and replacement. Whitefoot does not interrupt or migrate the computing context’s native stack.

Use the provisional bound:

\[
L_{\text{service}} = \tau + 2M + J + H + Q
\]

where:

- `τ`: starvation-age threshold.
- `M`: monitor period.
- `J`: total scheduling delay across detection and replacement wake.
- `H`: probe, assignment and bounded service work.
- `Q`: eligible service/continuation work ahead of the tested context.

For a continuation whose outcome still requires engine delivery:

\[
L_{\text{resume}} = L_{\text{service}} + K
\]

`K` includes required host notification or cancellation terminalization. For an expired sleep timer and an already terminal, visible completion, `K = 0`.

**Initial falsifier:** `τ = 1 ms`, `M = 1 ms`, `J ≤ 16 ms`, and `H + Q ≤ 6 ms` give a **25 ms** service/resume target for the sleep witness.

These are assumptions to qualify and measure. Ordinary OS scheduling supplies no universal maximum slice, and `J` covers multiple scheduling opportunities. The target is conditional on scheduler service, a warm reserve, bounded backlog, bounded lock/service delay and timely kernel notification.

## 4. Normal-path costs

The proposal adds synchronization even when no handoff occurs.

| Path | Proposed added cost |
|---|---|
| Every native coroutine resume segment | Release publication of OUTSIDE and acquire CAS on return; an odd/even store pair on the executor's own invocation sequence; publication of deadline/detection hints. |
| Host `.start` | No ownership CAS. Use context-private intent and stable adapter binding; retain existing engine synchronization. |
| Host wait, pass or acquisition retry | One acquire ownership check at an existing legal boundary. |
| Cross-thread shared/guard/join wake | Atomic unwind-gate transition preventing premature enqueueing. |
| Ordinary service reap | No repeated ownership check inside an already acquired SERVICE scope. |
| Detached handback | Ingress lock, publication and replacement notification. |
| Detection | Monitor wake and scan; clock reads concentrated there and at departure rather than every submit. |
| Runtime startup | Spare stacks, floor attachment and monitor startup. |
| Concurrent-map construction | Larger user inventory and initialization; cost exists even without starvation. |

Keep token checks at scope boundaries. Do not add them to every SQE, CQE, heap comparison or local hash lookup. Preserve direct adoption and local completion lookup under SERVICE.

The initial global active counter added two atomic RMWs per resume segment, which could be significant for short I/O continuations. Per-executor invocation sequences replaced it, keeping those writes on the executor's own line ([suspension accounting](../suspension-accounting/DESIGN.md)).

The monitor should scan only candidate roles and batch its work. Redundant hints can tolerate false positives; private-state inspection cannot tolerate data races.

“Ordinary I/O unchanged” is an experiment criterion, not an established consequence.

## 5. Alternatives and recommendation

| Alternative | Benefit | Cost or unresolved issue | Recommendation |
|---|---|---|---|
| Periodic monitor | Portable coverage of timers, ready queues and foreign completions while a context computes. | Periodic CPU cost; scan and scheduling delay. | Initial detector. |
| Deadline-armed watchdog | Fewer wakes when deadlines are distant or absent. | Does not alone detect ready work; requires rearming and wake-race handling. A `timerfd` serviced by the blocked driver provides no independent service. | Compare after the monitor baseline. |
| Whole-role reassignment | Services heap, ring, cancellations and queue together; works with one logical driver. | Requires separation of native execution and service ownership. | Primary A protocol. |
| Move only due/ready work | Uses idle capacity and avoids replacing a role unnecessarily. | Pending ring operations remain attached; arbitrary heap/list access needs ownership. One-driver case still needs a spare. | Bounded try-borrow D alongside A. |
| `--par` demand safepoint | Potentially detects demand with fewer periodic probes and lower delay during instrumented compute. | Work units are not wall time; cannot cover uninstrumented computation or native calls. | Optional supplement. |
| Dedicated service threads that never execute contexts | Service remains available without revocation. | Persistent separate executor architecture and queue crossings; still needs a continuation-executor capacity policy. | Useful comparison if handoff bookkeeping exceeds the cost criterion. |

The existing `150000` constant is a compute splitting grain, not an implemented demand safepoint (`compiler/src/backend/sched/entry.c:139`).

A safepoint proposal needs an atomic word associated with each physical executor. The monitor cannot safely write another thread’s plain TLS word. A poll can notice demand and notify or initiate service reassignment; it must not suspend inside an outstanding compute offer/join or keyed holding. Its work-unit interval alone establishes no time bound.

### Linux task-work choice

Current ring setup uses `COOP_TASKRUN` on the premise that the submitting scheduler thread enters the kernel within a bounded spin (`linux_io_uring.c:206`). That premise fails when it computes for seconds.

Shared SQ/CQ memory establishes visibility, not execution of deferred kernel work. Linux documents cooperative task work in relation to the submitting task’s kernel transitions; multi-thread waiting requires care. [Linux UAPI header](https://github.com/torvalds/linux/blob/master/include/uapi/linux/io_uring.h), [io_uring setup documentation](https://man7.org/linux/man-pages/man2/io_uring_setup.2.html).

Current nonblocking progress does not issue `GETEVENTS` simply because CQ is empty (`linux_io_uring.c:1055`).

Recommend a first correctness candidate without `COOP_TASKRUN` on handoff-capable rings, with a separate comparison retaining it. Retention needs a demonstrated mechanism that exposes completions despite the old submitter remaining in computation—for example, a qualified foreign-enter path or a carefully designed kernel-transition notification to that submitter.

Disabling the flag may materially harm ordinary I/O performance; the existing source records why it was selected. If that candidate fails the cost criterion, the result rejects the proposed combination of guarantees and costs and requires a further design choice.

## 6. Risks and unknowns

| Status | Risk | Required disposition |
|---|---|---|
| **Unverified, correctness-critical** | Coroutine unwind, wake and frame-release ordering. | Deterministic transfer/wake races plus emitted-coroutine tests. |
| **Unverified, correctness-critical** | Token atomicity, supported-target implementation and epoch exhaustion. | One atomic representation with explicit portability and exhaustion treatment. |
| **Unverified, correctness-critical** | Stuck detection during displaced computation and ingress publication. | Coherent collection argument and adversarial tests; no zero-work interval. |
| **Unverified, latency-critical** | Foreign Linux progress with cooperative task work. | Old submitter held in compute while peer completes an operation; inspect publication and foreign progress. |
| **Unverified, latency-critical** | Monitor scan, locks, syscalls and service work fit `H`. | Measure each component; introduce bounded service-only paths where current code can block. |
| **Known limit** | All bounded executors can become occupied with computation. | Conditional profile and explicit exhaustion behavior; no unlimited-context claim. |
| **Unmeasured cost** | Token transitions, invocation sequences and wake gates affect short I/O continuations. | Same-source I/O control and profiling. |
| **Unmeasured cost** | Enlarged map-user inventory increases map size and construction work. | Map construction/holding and MemoryMeter comparison. |
| **Unmeasured consequence** | Migration can lose future lane-zero attachment and reduce `--par` parallelism. | Measure post-handoff compute. Lane leasing is a separate material choice. |
| **Unverified, platform-specific** | Non-ring polling, Windows helper latency and stop lifecycle races. | Real platform routes, not Linux-only mocks. |
| **Unverified, specification** | Exact profile conditions and numerical promise. | Owner selection after evidence; no approval claimed here. |

This work must update the recorded driver-thread ownership, heap ownership, publication and map-identity decisions together. The earlier rejection of a timer thread rested on drivers servicing their own earliest deadlines; starvation supplies new grounds for reconsidering independent detection.

## 7. Falsifiable experiment plan

All execution below is proposed future CI work. Precise timing runs belong on the `14900k` CI runner after checking availability and notifying the coordinator. Start with the smallest useful timed sample, then choose scale from its spread.

Record source and compiler revisions, kernel and ring flags, CPU affinity, driver/worker/helper settings, workload, raw observations and failures.

### 7.1 Starvation witness

Use the same `research/experiments/context-starvation/witness.wf` source for base, independent base twin and candidate.

Configuration:

- `WF_DRIVERS=1`, `WF_WORKERS=1`.
- One allowed CPU through `taskset`.
- No `--par`, so success cannot depend on the optional detector.
- Calibrate approximately two seconds of computation, then retain the same iteration count across comparison arms.
- Retain the zero-computation control.

The witness’s existing first-byte timing includes launch, observer and output costs. Add internal experiment observation of the armed deadline and first continuation step; do not equate first-byte timing with scheduler lateness.

Begin with three samples per arm. If the spread permits discrimination, run approximately thirty interleaved trials per arm, retaining every row.

**Reject the latency proposal if**, while its measured scheduling/backlog/reserve conditions hold, any witness continuation starts more than **25 ms after the 100 ms deadline**.

The external timer marker should therefore appear within approximately **135 ms of launch** when launch/observation overhead is independently at most 10 ms. The internal 25 ms criterion is authoritative.

Also require:

- The approximately two-second computation continues to completion after the timer continuation.
- Its result agrees with an independent recurrence oracle.
- The base witness fails the latency criterion.
- The zero-computation control remains near its timer deadline.
- Trace evidence confirms actual executor and role assignments.

The existing two-driver measurements show the observed delay, but sampled thread counts alone do not prove that a second logical driver had started or isolate every cause.

Additional cases:

1. Two logical drivers, one idle: D alone services due/ready work.
2. Every ordinary driver computes: A uses reserves.
3. A socket operation submitted by the displaced thread receives peer data while that thread keeps computing.
4. Helper and non-ring completions arrive before, during and after handback.
5. Several successive transfers exhaust the reserve: behavior stays correct and the conditional limit is visible.
6. Root and nested contexts own the waiting operation.

### 7.2 No-starvation control

Use the existing TCP echo benchmark:

- `research/experiments/io-completion-bench/linux-net-bench.sh`.
- `.github/workflows/io-bench.yml`.

Its existing `wf1` arm fixes one driver; cases include 64 and 1024 connections with 64-byte messages (`linux-net-bench.sh:48`, `linux-net-bench.sh:53`, `linux-net-bench.sh:65`).

Compare same-source base, base twin and candidate under identical warmed conditions, with interleaved order. Run one-CPU and ordinary-width configurations. Confirm zero handoffs in the no-starvation arm and record native/helper routing, enters, parks and monitor CPU use.

Provisional rejection thresholds:

- More than **3% regression** in throughput or CPU per request.
- More than **5% regression** in p99 latency.
- A base/twin spread exceeding those margins makes the comparison inconclusive.

Compare ring flags separately so flag changes are not misattributed to ownership bookkeeping. Also measure map construction, held memory and compute performance after migration, including later lane attachment.

### 7.3 Correctness tests

Add deterministic C tests under `compiler/src/backend/completion/`, using barrier-controlled interleavings. Existing private-bridge cancellation tests and handwritten shared-context frames provide suitable patterns (`cancel_test.c:1`, `shared_object_test.c:1`).

| Test | Required observation |
|---|---|
| Return versus monitor CAS | Exactly one service owner; losing executor performs no private mutation. |
| Monitor versus idle borrower | One ownership claim wins; loser neither waits indefinitely nor services without authority. |
| Repeated transfers | Stale epochs cannot reacquire a later invocation’s role. |
| Completion before/during/after adoption | Exactly one READY transition; no lost wake or freed-record access. |
| Staged SQEs and pending CQEs | Same ring and record remain usable; flush, rearm and cancellation continue correctly. |
| Displaced submission | Old executor submits through the original adapter while replacement services it. |
| Single detached publication | Replacement parks; old executor publishes exactly one child or join wake; wake epoch advances and it runs. |
| Watch/deadline registration races | Firing before registration, during adoption and after parking is observed with the specified outcome race. |
| Cancellation and transfer | Host outcome wins when already produced; cancelled outcome transfers nothing; engine releases buffers before DONE. |
| Wake before native unwind | Guard, shared and join wakers cannot resume or destroy the frame before commit. |
| COMPLETE handback | Group wake, live count and arena release occur once; shutdown waits for the publishing invocation. |
| False stuck state | Every role becomes idle while a displaced context computes and later fires a guard; no stuck report. |
| Root takeover | Root resumes on a spare; launcher cleanup and root destruction occur once. |
| Map identity | Old and replacement executors use distinct users during concurrent holds, resize and reclamation. |
| Compute interaction | Outstanding offers remain on their physical lane; handoff occurs only at legal suspension boundaries. |
| Stop lifecycle | Open/close, receiver acknowledgement, pending stop wait and shutdown survive ownership changes. |
| Reserve failure | No dropped wait, duplicate completion or unsupported latency claim. |

Wire these tests into completion sanitizer targets explicitly. Current sanitizer targets do not automatically cover a new handoff probe (`compiler/Makefile:527`, `compiler/Makefile:543`).

Use ASan/UBSan and TSan where supported, plus real Linux native-ring, macOS and Windows routes. Map runtime changes also require the project’s map sanitizer workflow. Sanitizers check important memory and synchronization failures; green results do not establish liveness or kernel task-work progress.

### Acceptance evidence

A successful result needs all three:

1. The starvation witness meets the qualified bound.
2. Ordinary I/O meets the predeclared cost criterion.
3. Transfer-race tests and applicable sanitizer/platform checks pass.

Failure in one dimension rejects that version of the proposal. It does not justify weakening a conformance verdict or hiding the missing behavior.

## 8. Proposed WAIT-2 wording and affected rules

### Proposed sentence

> Under an execution environment that meets an implementation’s declared latency profile—including its scheduling and host-notification bounds, execution-resource limits and bound on eligible work ahead—each context waiting for a reached sleep deadline or a host outcome that has been produced takes its next step within that profile’s stated monotonic-time bound, independently of whether another context reaches a wait or completes.

The profile must publish concrete conditions and bounds, rather than assume that the promised continuation already happened on time.

For the proposed one-driver, one-CPU profile:

- Sleep and already terminal visible outcomes target `B = 25 ms`.
- The profile permits the displaced computing context only while sufficient initialized execution capacity remains.
- Scheduling and service/backlog conditions are those used in Section 3.
- General I/O includes its required delivery/terminalization delay `K`; a 25 ms cancellation-service target is not a 25 ms return guarantee.
- The bound starts at the specified deadline or outcome production, rather than when the runtime eventually notices it.

In particular, do not redefine PRE-2 outcome production as CQE consumption. Backend delay belongs in the qualified bound.

### Rule-by-rule effect

| Rule | Current behavior | Proposed effect |
|---|---|---|
| **WAIT-2 progress**, `spec/kernel-spec.md:2252` | Eventual progress is conditional on every context reaching completion or a wait in finitely many steps. No numerical latency is promised. | Add the qualified latency property for reached sleep deadlines and produced host outcomes, including when another context keeps computing. Retain existing eventual guarantees. |
| **WAIT-2 context ordering and placement**, `spec/kernel-spec.md:2247`, `spec/kernel-spec.md:2251` | Each context follows its construct order; execution location and concurrency are unobservable. | Preserve these rules. Moving service ownership does not move a running native invocation or alter context order. |
| **PRE-2 deadlines and cancellation**, `spec/kernel-spec.md:2557`, `spec/kernel-spec.md:2559` | Monotonic deadlines, no-transfer stopped outcomes, and host-outcome precedence are specified. | Preserve outcome selection and production rules. Qualify delivery latency separately; do not reinterpret cancellation service as terminal readiness. |
| **HOST-1**, `spec/kernel-spec.md:2241` | Host order follows shared footprints. | No new host ordering from handoff or driver assignment. |
| **SHARE-3**, `spec/kernel-spec.md:2311` | Atomic statements take effect at one point. | Preserve state locks and cancellation ordering. No suspension inside held atomic state. |
| **WAIT-1 / WAIT-3** | Waiting boundaries and structured context joins define execution. | Preserve join placement and lifetime; handback must not complete a join early. |
| **PAR-1 / PAR-2**, `spec/kernel-spec.md:2150`, `spec/kernel-spec.md:2174` | Overlap is permitted, not required; waiting calls prevent overlap. | Preserve current-stack offers and joins. Safepoints request service; they grant no new overlap permission. |
| **SCOPE-3**, `spec/kernel-spec.md:17` | Safety is conditional on the declared TCB; resource/OS conditions are outside source outcomes. | State the external conditions of the latency profile explicitly. Resource limits do not conceal a runtime correctness defect. |

The owner’s selection establishes A and D as the investigation direction. The precise WAIT-2 sentence, profile conditions and numerical promise remain proposed until the experiment and owner review settle them.

A later approved amendment must update the specification, its approval record and derived conformance evidence together. This read-only study makes none of those changes.

### Review and evidence limits

A separate read-only review examined the protocol against the supplied base and witness revision. Its findings on stale TLS wakes, false stuck detection, physical thread inventory, blocking borrowed service, lane attachment and cancellation latency are incorporated above.

The final manuscript, implementation, coroutine unwind behavior, latency and costs remain unverified.

## Result: waking an idle driver for a lone ready context (D1) does not help

Change (78d6b3f33): `wf_context_ready` also woke one idle driver when a
context became ready on a driver that was running a context, not only when
the run queue held more than one. Witness run
[ctx-starvation 38038660775](https://github.com/Ming-Research/Whitefoot/actions/runs/38038660775)
(hosted ubuntu-24.04, 455,604,710 iterations, about 1.99 s of computation,
three passes each): with two drivers on two CPUs (arm `d`, `taskset -c 0,1`)
the 100 ms timer still fired at 1.9876 to 1.9877 s, the same as one driver on
one CPU (arm `a`, 1.9872 to 1.9880 s) and two drivers on one CPU (arm `b`);
the zero-computation controls fired at 0.1026 to 0.1030 s. Reading: the root
context suspends on its timer microseconds after the spawn, and its own driver
pops the ready child at once, before the woken driver returns from its park
(about 10 us or more); the timer's deadline then sits in that driver's private
heap, which no other driver may read. So moving ready contexts cannot help
once the starter's driver has resumed the child; the deadline itself must be
serviced by someone else, which needs the ownership protocol of section 2
(try-borrow for D, reassignment for A). The change is reverted: it added
wakes with no measured benefit.

## Result: idle drivers borrowing a computing driver's due timers (D)

Change: c5d8308d0 (service-ownership token: SERVICE while a driver services
its own state, OUTSIDE while its thread runs a context; an idle driver may
borrow an OUTSIDE driver for one bounded pass that completes due sleeps and
detaches already-published terminal records; idle parking capped at 1 ms
with several drivers) and 89c928258 (notify a driver started before the
count reached two, so it cannot park unbounded; per-driver counters printed
with `WF_SCHED_REPORT=2`). Witness run
[compute-bench 38045645745](https://github.com/Ming-Research/Whitefoot/actions/runs/38045645745)
(experiment `ctx-starvation`, hosted ubuntu-24.04, 455,702,991 iterations,
about 2.0 s of computation, three passes per arm):

| Arm | Drivers | CPUs | 100 ms timer fired at | Counters (driver 1 unless noted) |
|---|---|---|---|---|
| a | 1 | 1 | 1.988 to 1.994 s | no borrower exists |
| b | 2 | 1 | 0.1029 s | 3,779 borrows, 1 borrowed sleep |
| c (no computation) | 1 | 1 | 0.1027 to 0.1028 s | none |
| d | 2 | 2 | 0.1025 to 0.1028 s | 3,729 borrows, 1 borrowed sleep; driver 0 stole 1 context |
| e (no computation) | 2 | 2 | 0.1031 to 0.1035 s | none |

The timer is serviced by a borrow in both two-driver arms (`borrowed_sleeps=1`),
so the result is attributed to D, not to the computation being stolen; with
one CPU the kernel time-slices the borrower against the computing thread and
the timer still fires within 1 ms of its deadline. The earlier 1.99 s in arm
b (c5d8308d0 alone) was the startup race 89c928258 fixes. The cost side is
visible in the counters: an idle driver capped at 1 ms parks borrows about
1,900 times a second while another computes; whether that and the per-resume
token operations stay inside the owner's cost criterion (3 percent
throughput, 5 percent p99 on ordinary I/O) needs the 14900K comparison. One
driver on one CPU (arm a) still waits the whole computation, which is the
whole-role reassignment of step 2.

## Result: whole-role reassignment for one driver (A, first increment)

Change: 950d5bfd7 on `claude/ctx-handoff-2` (stacked on step 1): a monitor
thread that runs no contexts claims a driver that has stayed outside past a
due deadline with no idle driver to borrow it, and hands its role to a warm
spare executor (one per driver, at most two executors per driver); the
displaced thread keeps computing and, at its next return, commits its context
through a driver ingress queue and joins the spare pool. Map users follow
physical executors (inventory doubled); the ring is created without
`IORING_SETUP_COOP_TASKRUN` so a replacement can reap it. Witness run
[compute-bench 38049310155](https://github.com/Ming-Research/Whitefoot/actions/runs/38049310155)
(hosted ubuntu-24.04, about 2.0 s of computation, three passes):

| Arm | Drivers | CPUs | 100 ms timer fired at | Counters |
|---|---|---|---|---|
| a | 1 | 1 | 0.1037 to 0.1064 s | 1 reassignment, 1 ingress commit |
| b | 2 | 1 | 0.1027 to 0.1028 s | driver 1: 3,745 borrows, 1 borrowed sleep |
| d | 2 | 2 | 0.1025 to 0.1031 s | borrow, as in step 1 |
| c, e (no computation) | 1, 2 | 1, 2 | 0.1027 to 0.1033 s | none |

One driver on one CPU now meets the owner's 25 ms bound by the external
marker (at most 6.4 ms after the deadline), through reassignment rather than
borrowing. Still open: the cost criterion (the per-resume counters, the
monitor, the doubled map-user inventory and the changed ring flags all add
cost; 14900K comparison pending), detection of overdue readiness and
completions (this increment detects deadlines only), behaviour when the spare
reserve is exhausted, and an internal-timestamp measurement of the latency.

## Result: ordinary I/O cost of steps 1 and 2 (first sample)

Comparison: firn built from base fe5589ec5 (the merge base with main) and
from 8e2b14147 (step 1 and step 2), the latter twice (`handoff`,
`handoff-twin`) as the noise control, interleaved, two passes of five
seconds per test (`io-bench.yml` firn comparison, [run 38057973380](https://github.com/Ming-Research/Whitefoot/actions/runs/38057973380),
native i9-14900K, 14:01–14:11 UTC; servers on CPU 2, or 2 and 4, clients on
the remaining performance cores, as `redis-bench.sh` now places them).
Median requests per second and p99 (ms) over the two passes:

| Server CPUs | Test, depth | base rps | handoff ÷ base | twin ÷ base | p99 base / handoff / twin |
|---|---|---:|---:|---:|---|
| 1 | get 1 | 485,895 | 0.997 | 0.990 | 0.079 / 0.079 / 0.079 |
| 1 | get 16 | 4,638,218 | 0.990 | 0.996 | 0.127 / 0.131 / 0.135 |
| 1 | mset 1 | 365,235 | 1.006 | 1.005 | 0.103 / 0.103 / 0.103 |
| 1 | mset 16 | 1,253,086 | 1.009 | 1.011 | 0.487 / 0.479 / 0.479 |
| 1 | set 1 | 481,736 | 0.993 | 0.998 | 0.079 / 0.079 / 0.079 |
| 1 | set 16 | 4,558,105 | 1.001 | 1.001 | 0.135 / 0.135 / 0.135 |
| 2 | get 1 | 908,381 | 0.983 | 0.983 | 0.039 / 0.047 / 0.039 |
| 2 | get 16 | 8,804,187 | 0.998 | 0.998 | 0.071 / 0.067 / 0.075 |
| 2 | mset 1 | 694,143 | 0.987 | 0.988 | 0.055 / 0.055 / 0.055 |
| 2 | mset 16 | 2,437,364 | 1.001 | 1.012 | 0.203 / 0.315 / 0.295 |
| 2 | set 1 | 901,842 | 0.985 | 0.985 | 0.043 / 0.039 / 0.043 |
| 2 | set 16 | 8,580,276 | 0.988 | 0.995 | 0.075 / 0.079 / 0.071 |

Throughput meets the 3 percent criterion in every cell; with two drivers
at depth 1 the handoff build and its twin are both about 1.5 percent slower
(server CPU per request about 1.6 percent higher), consistently across both
passes, so that cost is real and inside the bound. p99 is reported in steps
of about 8 µs and equal within a step in ten cells. mset at depth 16 on two
CPUs is undecided: base's two passes gave 0.143 and 0.263 ms, the handoff
image's four (both names) 0.247 to 0.343 ms, so two passes cannot separate
a p99 increase from that cell's spread; it is rerun with more passes before
the 5 percent p99 criterion is judged.

The undecided cell was rerun alone ([run 38061064855](https://github.com/Ming-Research/Whitefoot/actions/runs/38061064855),
14:48–14:51 UTC, mset at depth 16, servers on CPUs 2 and 4, six interleaved
passes of five seconds). p99 (ms) per pass: base 0.263, 0.303, 0.255,
0.335, 0.215, 0.327; handoff 0.287, 0.279, 0.295, 0.255, 0.311, 0.287;
twin 0.223, 0.327, 0.311, 0.295, 0.287, 0.343. The paired per-pass ratio
handoff ÷ base has median 1.006 (95 percent bootstrap interval 0.82 to
1.30, the experiment's seed), and the noise control, twin ÷ handoff (one
image under two names), 1.106 (0.85 to 1.18). So this cell shows no p99
increase, but at five seconds a pass its p99 moves by about 20 percent
between identical images, and the instrument cannot resolve the 5 percent
criterion here; the other eleven cells agree within one reporting step.
Throughput in the rerun: medians 2,442,338 (base), 2,435,532 (handoff),
2,439,867 (twin) requests per second.


## Result: a long run beside other contexts yields, then loses its driver (item 159)

Change: the monitor no longer hands a driver over only past a due deadline
with no idle driver to borrow it. A context run that has lasted 1 ms while its
driver holds another context (a host wait of any route, a ready context or an
ingress commit, the three counts `wf_driver_alone` reads) is asked to yield:
the monitor moves the driver's service word from OUTSIDE to YIELD, which every
waiting boundary already loads (`wf_context_pass`), and the run suspends there
for fairness. A run still going one monitor period (1 ms) after the request
loses the whole role to a spare, as before. A driver that takes back a run
that yielded reaps (timers, cancelled waits, ring, helpers, readiness) before
it resumes anyone. `wf_context_pass` lost its per-context count and the three
shared loads it made every 64th pass. The owner's questions `handoff_trigger`,
`yield_check` and `progress_wording` on item 159 chose these, by their
recommended options, and the WAIT-2 amendment of 2026-11-26 states the
bounded promise.

What main missed, as seen in fast runs of the witnesses in
`compiler/tests/programs/busy_driver.rs` on main with the witnesses and no
runtime change: run
[45799561898](https://github.com/Ming-Research/Whitefoot/actions/runs/45799561898)
on a24ab8dbd (main 85bd04026 with the first witnesses) and run
[45805938592](https://github.com/Ming-Research/Whitefoot/actions/runs/45805938592)
on dc55a6e07 (main 469944ea4 with the timed witnesses). A status is what the
witness measures inside the program, in milliseconds, 250 for 250 ms or
more: the read's duration, its byte arriving at 100 ms; the bounded read's
duration, its deadline at 50 ms; the time from the firing to the end of the
watched read; and the time from a context becoming ready to its first step.
254 is a read that returned the byte sent after the computation instead of
ending at its deadline or firing.

| Witness (beside a 2 to 3 s computation on the same driver) | io_uring, 1 driver | io_uring, 2 drivers | no ring, 1 driver |
|---|---|---|---|
| read whose byte arrives after 100 ms | 250 | 250 (driver 1 borrowed 5,654 times, 0 borrowed terminals) | 250 |
| read with a 50 ms deadline | 51 (1 reassignment) | 254 (3,656 borrows, 0 borrowed sleeps, 0 reassignments) | 51 to 53 (1 reassignment) |
| read ended by a watch another context fires | 254 | 254 | 254 |
| ready context woken by a write, computations on every driver | 250 | 250 in each of 128 attempts: driver 1 had stolen a computing context and no role was reassigned, so the waiting context was held until the computations ended | — |

The TCP receive beside a computation inside an atomic statement's block
(`programs::busy_ring_receive`, from the ring-assist branch) did not release
the computation within 100 ms in run 45805938592. A borrow serves only due
sleeps and published records, so the deadline beside an idle driver (the
second row) and every completion that a reap must find stayed behind the
computation; only a due deadline with no idle driver was handed over.

With the change, run
[45805940735](https://github.com/Ming-Research/Whitefoot/actions/runs/45805940735)
on da2464c8f passed the four busy-driver witnesses in every configuration
(64 placed runs each for the read and the ready context, 10 for the deadline
and the cancellation, each within its limit: two handoff cycles of 2 ms and
40 ms for a shared runner after the byte's arrival or the context becoming
ready, 100 ms after the deadline or the firing) and the busy-ring receive
in its 16 runs.

### Plan for the 14900K measurements (written before measuring)

Each comparison builds base (the main revision the change was last merged
with) and candidate (this change) and runs them interleaved on the owner's
i9-14900K; the fixed-cost and Firn comparisons also run base under a second
name as the noise control. The record states the host's load beside each
result (the idle check before the runs and each fixed-cost cell's share of
busy CPUs), and a cell that cannot be decided is reported as undecided, not
widened. The scripts are `research/experiments/handoff-yield-check/` and
`research/experiments/io-completion-bench/redis-bench.sh compare`.

1. Ready context at one and two drivers, internal timestamps: one handoff
   cycle is fixed at 2 ms (the 1 ms threshold plus one monitor period), so
   every attempt's ready context must start within 4 ms of becoming ready, on
   any placement. At two drivers, attempts go on until 64 show a
   reassignment (at most 128), so that the handoff at the bound's edge is
   seen 64 times; every attempt counts against the bound. Any attempt over
   4 ms rejects the claim that a ready context starts within two handoff
   cycles. (Amended on review before any result was read: it first counted
   only the attempts that showed a reassignment, which leaves out exactly
   the attempts in which no handoff happened.)
2. The polling shape of `tests/programs/lone_timer_writer.wf` (a context
   looping on atomic statements beside 1 ms sleeps and a read whose byte
   arrives), 64 runs each in io_uring at one and two drivers and no ring at
   one: the lateness of the sleeps and of the read, base (yield every 64
   answered waits) against candidate (yield at the monitor's request).
   The candidate is expected to be later by about the threshold plus a
   monitor period (2 to 3 ms) where base yielded within 64 statements; that
   is the trade the owner took in `yield_check`, recorded, not judged.
3. The fixed cost of an atomic statement: one context incrementing a shared
   counter in an unguarded statement, against a C loop that increments a
   plain counter and one that takes and releases a spin lock around it, base
   and candidate. The candidate's time per statement above the base twin's
   spread rejects the claim that the check became cheaper.
4. Firn GET, SET and MSET at one and two server CPUs and pipeline depths 16
   and 1 through `redis-bench.sh compare` (four interleaved passes of five
   seconds, the same mode `io-bench.yml`'s firn comparison runs), with
   `WF_SCHED_REPORT=2` yield-request and reassignment counts. In each of the
   12 cells, the base-twin interval is the range of the per-pass ratios
   twin ÷ base of requests per second, and the candidate's ratio is the
   median of its per-pass ratios candidate ÷ base. A candidate ratio below
   the interval's low end rejects the claim that ordinary I/O pays nothing
   for the change in that cell; a cell whose interval is wider than 10
   percent is reported as undecided. A nonzero yield-request count under these workloads, whose
   commands run for microseconds, would show that the trigger fires where no
   context computes for long, and is reported to the owner.

### First 14900K run: the host was not idle

Run [45812772918](https://github.com/Ming-Research/Whitefoot/actions/runs/45812772918),
2026-11-26 11:43 to 12:17 UTC, base 0b8b4ad34 against candidate 58efaa8ba,
whose runtime is this change's (later commits change only comments in
`bridge.h` and `frames.rs` and the latency script's summary). The data are in
`measured-2026-11-26/`. Other projects' and works' compiler builds were running
on the host during the whole run. The idle check before the runs found load
averages of 32, 45 and 43 on 32 logical CPUs and 98 percent of all CPUs busy
(`host-load.txt`); the script warned and went on. Most fixed-cost blocks ran
with 50 to 100 percent of the host busy (`fixed-cost.tsv`, column
`host_busy`), and Firn's base reached 273,028 requests a second at one CPU
and depth 1 against 485,895 on the idle host of run 38057973380 above. In
addition, fast run 45815451489 of this work ran its tests on the same host
from 11:59:20 to 12:01:43, during the one-CPU Firn passes 2 (base) and 3
(base and candidate) (`firn-passes.csv`: the times of each pass's first and
last result). So the run tests the criteria above on a loaded host only.
Read by those criteria:

1. Ready context (`latency.tsv`, shape `ready`). Base: 250 in each of its
   64 attempts at one driver and 128 at two, none with a reassignment.
   Candidate: every attempt showed a reassignment. The statuses were:

   | Drivers | Attempts | Status 1 to 3 | Over 3 | Largest | Median |
   |---|---|---|---|---|---|
   | 1 | 64 | 55 | 9 | 10 | 2 |
   | 2 | 64 | 56 | 8 | 14 | 2 |

   The criterion rejects the bound on this run: 17 attempts started more
   than 4 ms after becoming ready. The run cannot tell whether the bound
   holds when the monitor and the spare are not competing with other work
   for CPUs. It has to be measured again on an idle host.
2. Polling shape (`latency.tsv`, shape `poll`), lateness in milliseconds of
   the 16 sleeps and the read, 64 runs per side:

   | Route, drivers | Base median | Base over 3 | Candidate median | Candidate over 3 | Candidate largest | Candidate yield requests |
   |---|---|---|---|---|---|---|
   | io_uring, 1 | 0 | 2 | 1 | 11 | 19 | 1,724 |
   | no ring, 1 | 0 | 1 | 1 | 8 | 6 | 1,733 |
   | io_uring, 2 | 0 | 0 | 0 | 0 | 0 | 1 |

   On poll_contexts.wf's shape (shape `turn`), the median wall time of a
   whole run was 985 µs for base and 1,997 µs for the candidate, with one
   yield request a run: the waiting reader's turn now comes at the
   monitor's request, about 1 ms later, as `yield_check` traded.
3. Fixed cost of an atomic statement (`fixed-cost.tsv`; `c-reference.txt`:
   the C loop takes 1.491 ns per plain increment and 5.685 ns with a spin
   lock around it, medians of six blocks). The medians over all six blocks
   are, in ns per statement: `own` on one CPU, base 17.16, candidate 16.40,
   twin 16.06; on two CPUs, 7.96, 7.95 and 8.09; `take` on one CPU 33.28,
   33.61 and 31.43; on two CPUs 16.55, 22.76 and 19.42. The twin's blocks
   alone spread from 13.78 to 21.38 ns (`own`, one CPU) and from 15.13 to
   27.19 ns (`take`, two CPUs). By the criterion, the claim is not rejected,
   but these blocks cannot show a change that small. Blocks with less than
   30 percent of the host busy were selected after reading the data, so
   this reading is exploratory. In them, `own` on one CPU took 13.74 and
   13.78 ns (candidate) and 13.78 and 13.80 ns (twin), with no base block
   that quiet. On two CPUs it took 6.83 and 6.87 (candidate), 6.85 and 6.88
   (twin), and 7.09 and 7.25 (base). `take` on one CPU took 31.38, 31.44
   and 33.45 (candidate), 31.26 to 31.39 (twin) and 31.21 to 31.25 (base).
   On two CPUs it took 15.16 and 15.24 (candidate), 15.13 (twin) and 15.24
   (base). The base and the twin are one image, yet they differ by up to 4
   percent in these blocks (`own` on two CPUs). So even the quiet blocks do
   not measure the removed count.
4. Firn (`firn-compare.csv`). With every pass, all 12 cells are undecided:
   each base-twin interval is wider than 10 percent, from [0.683, 0.912]
   (set, depth 1, one CPU) to [0.893, 6.197] (set, depth 16, two CPUs).
   Without the passes the fast run overlapped (one CPU, passes 2 and 3),
   two passes remain in each one-CPU cell:

   | Test, depth | Twin ÷ base | Candidate ÷ base, median (per pass) | Reading |
   |---|---|---|---|
   | set, 16 | [0.988, 1.026] | 0.792 (0.783, 0.801) | below |
   | set, 1 | [0.890, 0.912] | 0.662 (0.796, 0.528) | below |
   | get, 16 | [0.839, 0.870] | 0.689 (0.919, 0.459) | below |
   | get, 1 | [0.925, 1.191] | 0.799 (1.056, 0.543) | undecided |
   | mset, 16 | [0.833, 1.725] | 1.441 (0.939, 1.943) | undecided |
   | mset, 1 | [0.761, 3.570] | 1.457 (1.134, 1.779) | undecided |

   The two-CPU cells stay undecided. The passes that remain were as loaded
   as the ones taken out, and a candidate pass varies by a factor of two
   (0.459 to 0.919 in get at depth 16). So three cells fall below their
   interval, but two passes on a loaded host cannot separate the change
   from the other work. There is also a way for the change itself to cost
   throughput only on a loaded host. If the host deschedules a Firn driver
   thread for more than 2 ms in the middle of a run, the monitor sees a long
   run, asks it to yield and then hands the role to a spare. With one
   server CPU, that spare shares the CPU. The yield-request and
   reassignment counts would show whether this happened, but they were not
   recorded: `WF_SCHED_REPORT=2` prints them only as the program exits.
   Firn never reads its stop requests, and `redis-bench.sh` ends it with a
   signal, so `compare-sched.txt` stayed empty.

Acceptance 10 and 12 of item 159 are therefore not shown, and 11 lacks the
Firn counts. All three need a run on an idle host and a way to read the
counts from a Firn that is stopped.

### The runtime test fails on each removed condition

`driver_service_test.c` was built and run on mutants of `bridge.c`, each
removing one condition the change rests on, by a temporary workflow on a
hosted runner (runs
[45813884231](https://github.com/Ming-Research/Whitefoot/actions/runs/45813884231)
and [45815096128](https://github.com/Ming-Research/Whitefoot/actions/runs/45815096128),
the second for the pass mutant, whose first form did not compile). The
unmutated test passed in both; every mutant failed:

| Mutant | Failing check |
|---|---|
| the monitor ignores that the role is alone | `driver_service_test.c:1037`, an alone run is asked to yield |
| the monitor never asks to yield | `:1062`, no request after 1 ms beside a sleeper |
| a pass ignores a yield request | `:259`, the requested pass does not suspend |
| no reap after a yield | `:690`, the due sleep is still pending after the yield |
| handoff without waiting a monitor period | `:1066`, handed over before the period ended |
| the handoff claims the current word, not the one it saw | `:980`, a borrowed role is claimed |
| a return ignores a YIELD word | the test's 30 s guard, in the yield-service phase: the run never gets its driver back |
| handoff when the role became alone | `:1016`, the alone role is handed over |
| a borrow refuses a YIELD word | `:649`, a requested role cannot be borrowed |
