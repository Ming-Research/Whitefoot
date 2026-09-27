<!-- Adversarial review of research/investigations/io-model/DESIGN.md revision 1,
     2026-08-25, sol-ultra agent "runtime-attacker". Checked in as evidence behind revision 2;
     paths sanitized to repo-relative and <scratch-root> forms. Findings were
     re-verified by the lead before adoption; §3f's original disposition in the
     runtime and sweep reports was OVERRULED by constitution T3 (see DESIGN.md).
     The body was written in Chinese and translated into English on 2026-09-27;
     the original is in the repository history, for example at commit ad51e05df. -->

# Whitefoot I/O concurrency runtime adversarial audit

Completion is already ready: can every lane still sleep itself to death? Under the state machine as currently written in §4, yes. Here, a lane means a scheduling thread that executes Whitefoot code and owns a deque and an I/O completion queue.

```text
L0 and L1 are both in join(Ti), deque empty

1. Check Ti: PENDING
2. pop / steal: no compute work
3. The kernel posts Ti's CQE
4. reap CQE: Ti = DONE
5. Go straight into io_uring_enter(min_complete = 1)
6. No second CQE after this

Result: both Ti have completed, both continuations can proceed,
but L0 and L1 are both waiting for the next completion, permanently asleep.
```

[DESIGN.md §4](/research/investigations/io-model/DESIGN.md:148) lists only "reap CQ -> park" and does not specify that "once any CQE has been processed, the lane must return to the scheduling loop." This is not a defect in `io_uring`; it is a missing edge in the user-space state machine.

Overall verdict: §4's architectural direction may be kept; the current protocol cannot enter implementation. §5's intent behind the `W=0/W=1` split may be kept, but both "the existing refusal path is sufficient" and "a single lane still executes in source order" are directly refuted by the existing code.

This report is based on `io/model` at `7ef03f8e`; the entire review was read-only: no build, no run, and no modification of the working tree.

## 1. `io_uring` wakeup and permanent sleep

### 1.1 All wakeup sources

| Source | Arrival form | Guarantee that must be established |
|---|---|---|
| Compute frame publication | The CQE corresponding to the eventfd, or a targeted ring message | The deque's release-publish must happen before the notification; the notification means only "rescan," not "one task" |
| I/O completion, error, short completion | The CQE on the ring that submitted the operation | Once the CQE is processed, the join target must be rechecked |
| Cancellation, timeout | The cancel-request CQE plus the terminal CQE of the original operation | The cancel request itself must not release the frame or buffer |
| trap, resource death, process shutdown | Signal or process termination | Hosted targets end the process directly, without depending on scheduler wakeup; bare metal additionally requires quiesce |
| `EINTR`, CQ overflow, ring teardown | Control result | Not executable work; must be handled explicitly and must not be treated as an ordinary empty queue |

### 1.2 `eventfd registered in each ring` gets the direction backwards

The semantics of `io_uring_register_eventfd` are: when a CQE appears on the ring, the kernel increments the eventfd. It cannot turn a user write to the eventfd into a CQE on the ring. The [liburing manual](https://www.man7.org/linux/man-pages/man3/io_uring_register_eventfd.3.html) states this direction explicitly.

For a compute publication to become a CQE, each ring must submit an `IORING_OP_POLL_ADD` that listens on the eventfd. By default this operation is one-shot, and once it completes it must be resubmitted; multishot must also check `IORING_CQE_F_MORE` and rearm once it terminates. [io_uring_enter(2)](https://www.man7.org/linux/man-pages/man2/io_uring_enter.2.html) documents these poll rules.

Taking "registered" literally, the trace is:

```text
publisher: deque.push(F); eventfd_write(E, 1)
parked lane: io_uring_enter(..., min_complete=1)

E increments, but the ring has no CQE; the lane does not wake.
```

In the current structured-compute model, the publisher can usually still execute `F` at its own join, so this trace does not necessarily cause a whole-pool deadlock by itself; it is certain to drop the compute wakeup, and it will become a whole-pool deadlock once an external producer or continuation appears in the future.

### 1.3 eventfd coalescing is not a counting protocol

A single read of an ordinary eventfd takes the whole count and clears it to zero; only `EFD_SEMAPHORE` decrements by one each time. [eventfd(2)](https://man7.org/linux/man-pages/man2/eventfd.2.html) gives this distinction.

So coalescing multiple publishes into a single notification is not a problem in itself, but it can satisfy only the following protocol:

```text
Receive one notification
    -> scan all work sources until quiescence is confirmed
    -> rearm
    -> announce that it is about to sleep
    -> scan once more, last
    -> park
```

The eventfd value must not be read as a frame count, and a single lane must not clear it and then execute only one frame. To wake multiple lanes precisely, either use the targeted-selection approach from the existing `wf__par_idle` together with per-lane notification, or explicitly accept "one wakeup wakes one drain-to-quiescence worker."

### 1.4 The narrow "CQ empty until enter" window is itself safe

If the poll is already armed and the CQE arrives after the last empty check, `io_uring_enter(..., min_complete=1)` will observe the already-present CQE and return; no extra condition variable is needed. [io_uring_enter(2)](https://www.man7.org/linux/man-pages/man2/io_uring_enter.2.html) guarantees that what it waits for is an event available in the CQ.

What is genuinely dangerous is three pieces of user-space state:

1. reaping a CQE and flipping the completion flag, and still parking afterward;
2. a one-shot eventfd poll that has been consumed but not resubmitted;
3. a CQE delivered to A's ring, while the one actually waiting for it is B.

### 1.5 The ring ownership of a stolen frame must be defined independently

The existing compute slot uses `slot->home` to point at the publisher's lane. The slot at [par_runtime.c](/compiler/src/backend/par_runtime.c:183) can be executed by another lane.

If the I/O ring is also selected by the frame's `home`, the following can occur:

```text
A publishes compute frame C, B steals C
B submits I/O inside C, but the SQE goes into A's ring
B joins on its own ring and parks
A receives the CQE, flips B's target flag, then parks again
B never receives its own CQE, and there is no cross-ring notification

target is already DONE, and both A and B are asleep.
```

The submission ring must be taken from "the lane that performs the submission," and the join must park on that same ring; or else the completion handler must deliver the wakeup directed at the actual waiter's ring. The meaning of the compute slot's `home` cannot be reused for this.

### 1.6 "The dependent continuation lives in its own deque" conflicts with no-continuations

In the current design, the dependent continuation should be the not-yet-returned C stack below the join; it should not live in the deque. If an implementation turns completion into a deque continuation:

- it is no longer "no continuations";
- the waiter or a kernel thread becomes a second producer for the Chase-Lev deque;
- the existing single-owner push assumption fails;
- a new cross-thread enqueue-and-wakeup protocol is required.

The rule that "completion writes only the result and the terminal state, and never publishes a continuation to the compute deque" should be listed as an explicit invariant.

## 2. join-as-worker latency, stack, and starvation

### 2.1 Completion is already ready, yet join executes an unrelated task of arbitrary length

The current `wf__par_wait`, after checking the target, first pops its own deque, then steals arbitrarily, and then runs the whole frame to completion. [par_runtime.c](/compiler/src/backend/par_runtime.c:455)

```text
join checks T: not yet complete
T's CQE arrives right after
join steals U
U runs for ten seconds, or never returns
T stays ready the whole time; join cannot proceed
```

I/O magnifies this problem, because a CQE must be reaped by user space before it can flip the flag. §4's idle order places the CQ after compute work, so a persistent compute backlog can keep an I/O completion from ever being reaped.

At minimum the following priority order is needed:

1. drain its own CQ;
2. if the current join target has already completed, return immediately;
3. run compute frames that must also complete within the same structured window;
4. run at most one unrelated steal, then go back to step 1.

Even so, a non-preemptible `U` can still run for an arbitrary length of time. To preserve the source program's liveness, an I/O join must not execute an arbitrary unrelated frame; it may only help with a frame that, within the same window, must already be joined before exit.

### 2.2 A steal inside join recursively grows the lane stack

The actual call chain is:

```text
A at join
  wf__par_execute(B)
    B at join
      wf__par_execute(C)
        C at join
          ...
```

Neither the previous layer's join nor the user frame has exited. The current runtime comment still claims that a stolen call starts from the bottom of the lane stack, [par_runtime.c](/compiler/src/backend/par_runtime.c:287); a later audit in 0079 has already established that this premise is false, [0079-exhaustion-floor.md](/docs/ongoing/0079-exhaustion-floor.md:387).

The current total slot count gives dynamic nesting a coarse upper bound, at most `64 lanes × 64 slots`, so it is not infinite in the mathematical sense; but it is not bounded by the source call graph or a byte stack budget, and any stolen frame can still recurse deeply. Once a lane is near the bottom of its 1 GiB stack, executing just one more small frame is enough to produce `{"resource":"stack"}`.

`--stack-ledger` cannot see this scheduling edge either: it explicitly excludes the runtime translation unit, and it cannot see indirect thunk calls. [stack_ledger.rs](/compiler/src/backend/stack_ledger.rs:22)

The minimal fix is to add a per-lane `help_depth`:

- the outermost join may execute one helper frame;
- a join inside the helper only drains completions or parks; it does not continue to steal;
- the ledger explicitly states the reserve for this one extra scheduler layer.

Without this limit, the design must withdraw arguments such as "a fixed lane stack means stealing only adds headroom," and must treat this resource death as an explicit cost.

### 2.3 Two work sources need a fairness rule

Writing only "one scheduler, two work sources" is not yet a scheduling rule. At minimum it must specify:

- join target completion ranks above other work;
- the CQ must be drained after executing a bounded number of compute frames;
- a CQ flood also must not permanently starve the compute deque;
- each batch of one-shot/multishot completion processing is bounded;
- a non-terminating frame cannot provide a latency guarantee, and this limitation must be stated openly.

## 3. kqueue, waiter, and the blocking disk pool

### 3.1 The buffer loan must extend across threads to the terminal state

Dangerous trace:

```text
lane submits read(buf: &uniq ...)
submit returns
lane leaves the window, and moves, frees, or reuses buf
disk worker later writes to the old address
```

An input buffer needs a cross-thread exclusive loan; an output buffer needs a shared/read loan. The loan must hold from the submission's linearization point until the original operation's terminal completion; during that time:

- the address must not move;
- the memory must not be freed or reused;
- the lane must not read or write the exclusive buffer;
- the frame must not return to the free list;
- the waiter/disk worker must hold target-level safety qualification to access that storage.

This is exactly what io_uring actually requires of a buffer: a read or write buffer must stay valid until completion. [io_uring(7)](https://man7.org/linux/man-pages/man7/io_uring.7.html)

### 3.2 A waiter may exist, but must not block on ordinary I/O or the mailbox

A single waiter is safe only if it always performs short, non-blocking actions:

- kqueue readiness;
- submission dispatch;
- putting completions into the mailbox;
- wake.

Ordinary file operations must be placed into the disk pool. The pool also needs a fixed depth, a queuing cap, a fairness policy, and reserved capacity for cancellation. If the waiter blocks on a full mailbox, and the wake happens only after a successful enqueue, this forms:

```text
mailbox full
waiter blocks on enqueue, not yet woken
all lanes are already parked, and only they can drain the mailbox
```

The safest shape is for the frame to carry its own preallocated completion node, so that the mailbox's capacity covers every permitted in-flight frame and the waiter never waits on memory or queue space.

### 3.3 The minimum memory model for the MPSC mailbox

`db543775` itself is not the lane-count fix; the fix is `39195bca`, among its ancestors. That commit changed all seven concurrent accesses to `wf__par_lane_count` to relaxed atomics, because "writing the same or a harmless value" is still a C data race.

The mailbox needs a stronger guarantee than the lane count:

1. after the producer fully writes result, status, and node, it publishes with a release;
2. the consumer takes it out with an acquire, and only then reads the payload;
3. all concurrent accesses to head, tail, and next follow the chosen MPSC algorithm, with no plain access mixed in;
4. "queue empty" must distinguish genuinely empty from the producer having already exchanged tail but not yet linked next;
5. the enqueue's linearization point precedes the wake;
6. the consumer first announces that it is going to sleep, then rechecks the mailbox with an acquire;
7. each operation has exactly one terminal state, and the frame must not be reused before the consumer confirms it;
8. the cancel CQE and the original operation's CQE must eliminate ABA using a generation or an unreused frame;
9. the waiter, disk workers, and the ISR must never write the same completion field at the same time.

A typical faulty trace is:

```text
P: atomic_exchange(tail, node)
P: pauses, has not yet written prev->next

C: sees head->next == NULL
C: judges it empty and parks

P: writes prev->next
P: if the wake was already sent earlier, or no posted handshake was used, C never wakes
```

The existing condition-variable path uses a `posted` flag and an idle bit specifically to close this window, [par_runtime.c](/compiler/src/backend/par_runtime.c:303). A mailbox cannot just write "MPSC" and assume the same property holds automatically.

## 4. `WF_WORKERS=1` and actual lowering

### 4.1 The current runtime really does make claim return `NULL`

The current parsing code maps every value `< 2` to 0:

```c
if (end == setting || *end != '\0' || requested < 2) {
    return 0;
}
```

And the bootstrap query is:

```c
int wf__par_pool_active(void) {
    return wf__par_requested_lanes() >= 2;
}
```

See [par_runtime.c](/compiler/src/backend/par_runtime.c:611) and [pool query](/compiler/src/backend/par_runtime.c:798).

So under the current `W=1`:

- `wf__par_pool_active()` returns false;
- bootstrap selects the sequential clone;
- if the overlapped clone is forced into selection, `wf__par_claim()` will return `NULL` because there is no lane.

Changing only the query can yield "overlapped clone + refuse every compute claim," but then lane 0 is not prepared, and there is no per-lane ring either. If `wf__par_start` is changed to actually initialize one lane, the existing `wf__par_claim` will obtain a free slot again and no longer self-refuse.

The design must separate three concepts:

- whether the overlapped world is selected;
- how many Whitefoot compute execution lanes there are;
- whether the I/O ring/backend has been initialized.

`wf__par_pool_active` should be renamed, or its contract changed, into a mode selector. `W=1` should mean one program-execution lane, zero stealing workers, and one I/O scheduler endpoint; a compute claim needs an explicit `compute_lanes < 2` refusal.

### 4.2 The `NULL` fallback does not execute at the original statement's position

At the hand-out site, the existing emitter only performs the claim and the conditional branch; when it is `NULL`, the call is deferred to the join after the last member. The control flow formed by [parallel.rs](/compiler/src/backend/emitter/parallel.rs:385) and [join lowering](/compiler/src/backend/emitter/parallel.rs:534) is:

```text
s1: claim == NULL, record "inline later"
s2: execute the last member now
join(s1): only now does s1 execute inline
```

Therefore what [DESIGN §5](/research/investigations/io-model/DESIGN.md:204) says, "program statements execute in source order," is false, even with only one thread.

### 4.3 Three kinds of observable change

1. **The trap record definitely changes.** In the existing test, `left` is the first member and `right` is the last member. Currently both `W=0/1` select the sequential clone, so `left`'s claim record always wins. [trap_latch.rs](/compiler/src/backend/tests/trap_latch.rs:205)  
   If `W=1` selects the overlapped clone and every claim is refused, `right` runs first and traps, and the record becomes `right_index_in_range`. [PAR-1](/spec/kernel-spec.md:2016) currently permits erroneous execution to have the claim selected by the schedule, but `W=1` is no longer a path that reproduces source order; only `W=0` remains.

2. **The published bytes can change.** Suppose the first member contains a failing claim and the last member submits independent output:

   ```text
   W=0: the first member traps, output is never submitted
   W=1 overlapped: output is submitted first, then the first member runs inline and traps
   ```

   The bytes may even be published by already-submitted I/O after the trap record. The reason v0.36 currently promises that erroneous execution has no external effect is that the overlap window expressly forbids system operations, [kernel-spec.md](/spec/kernel-spec.md:2010). The I/O design must re-adjudicate this: the most conservative rule is that any overlap window that might submit external work must be `traps`-free.

3. **The stack resource record can change.** 0079 has measured that for the same recursion, the overlapped clone costs 48 B/level while the sequential clone costs 16 B/level. [0079-exhaustion-floor.md](/docs/ongoing/0079-exhaustion-floor.md:475)  
   Once `W=1` switches to selecting the overlapped clone, a program that previously completed may change to `{"resource":"stack"}`. Under the current specification this is an allowed-to-vary resource condition, but it is indeed a change in the process result and in stderr.

A correct `W=1` should not create a new compute pthread. The existing worker count includes the calling thread itself; lane 0 should reuse the 1 GiB entry stack that `wf__floor_run` already provides. The waiter and the disk pool are TCB threads and must not execute writer code.

## 5. in-flight buffers, floor, and abort

The SIGSEGV/SIGBUS handler in [wf_floor.c](/compiler/src/backend/wf_floor.c:150) selects the sole record and then calls `abort()` directly; other lanes may still be inside `io_uring_enter`, and the kernel or a device may still be holding the buffer.

### Hosted Linux must be guaranteed by target qualification

- closing the ring or exiting the task cancels pending requests;
- a request that has already been handed to hardware and cannot be canceled continues to hold the references it needs;
- the lifetime of the CQ, the callback, and pinned pages must not extend past a destroyed address space;
- user space does not need to drain the CQ inside the signal handler;
- an external write may still complete, and there is no promise of rollback.

Linux's ring shutdown automatically cancels pending requests, but an operation that has already been handed to hardware is usually not cancelable. [io_uring cancellation](https://man7.org/linux/man-pages/man7/io_uring_cancelation.7.html) The current kernel exit path also calls io_uring cancellation before tearing down the address space, [Linux `exit.c`](https://github.com/torvalds/linux/blob/master/kernel/exit.c). This is a qualification of the Linux backend, not a natural law of the language itself.

### Bare metal cannot borrow "process exit reclaims it"

A DMA device may keep writing physical memory after the CPU declares abort. If those pages are immediately handed to another instance, memory corruption is reintroduced. The arbiter must do one of the following:

- stop new doorbells, cancel or reset the device, and wait for ownership handback;
- revoke access with the IOMMU, but respect the device's drain/fence;
- quarantine the descriptor and the buffer until the non-cancelable DMA completes;
- if a whole-machine abort is equivalent to a halt or a hard reset, explicitly state that the memory will not be reused before the reset.

All of these are TCB teardown, not language cleanup, so they do not conflict with [EFF-4](/spec/kernel-spec.md:1429).

### v0.36 does not cover the full problem

[TRAP-1](/spec/kernel-spec.md:2395) only says:

- hosted OS teardown reclaims process-local objects;
- external work that has already started keeps the semantics its own family specifies;
- the current synchronous path needs no pending-operation transfer.

It does not define an asynchronous family, buffer pin/quarantine, bare-metal DMA, or the ordering "submitted, then completing after the record." [PAR-1's what-survives sentence](/spec/kernel-spec.md:2026) even more explicitly depends on the window having no external effect at all, and cannot be generalized directly to I/O.

## 6. The minimal sound cancellation rule for window exit

The minimal rule should be:

> Every normal or recoverable window-exit edge must first observe the terminal state of every operation the window has submitted. A cancel request is not a terminal state. The frame, the capability, and every borrowed buffer stay alive and unreusable until the original operation reaches its terminal state. A trap does not perform language-level cancellation; the target TCB is responsible for quiesce or quarantine.

The corresponding state machine:

```text
CLAIMED
  -> SUBMITTED                 // loan takes effect
  -> CANCEL_REQUESTED          // loan still in effect
  -> COMPLETED(result)         // terminal state of the original operation
     or CANCELLED_BEFORE_EFFECT
  -> CONSUMED
  -> FREE
```

Under io_uring, the cancel request and the original operation each get a CQE, and their order is not guaranteed; neither `-ENOENT` nor `-EALREADY` proves that the original operation has safely disappeared, and some hardware operations cannot be canceled at all. [cancellation manual](https://man7.org/linux/man-pages/man7/io_uring_cancelation.7.html)

So §9's "stop waiting" is not enough. If the window really must exit early, the runtime can only take over ownership of the buffer and keep a hidden reaper; a borrowed stack buffer cannot do this. The first-version choice most compatible with no-continuations is: do not provide early-exit cancellation, and keep the existing lowering's "join every member after the last one, then allow any exit edge." [builder.rs](/compiler/src/lowering/builder.rs:582) already has this structure.

## 7. Which parts can be kept

| Design part | Verdict |
|---|---|
| submit/complete as a common model below the language level | Keep |
| One scheduler observing the compute deque and completion sources | Keep, but it must be written as a complete state machine |
| per-lane ring | Keep, but ring affinity must be taken from the executing lane, and completion must wake the actual waiter |
| Helping execute work at join | Keep with limits: help only within the same window, or set a bounded `help_depth` |
| no writer-visible futures / async / continuation | May be kept in the first version, on condition that every normal exit first joins to a terminal state |
| kqueue waiter + disk pool | Conditionally kept: needs a cross-thread loan, an MPSC memory model, and bounded backpressure |
| `WF_WORKERS=0` sequential anchor | Fully kept |
| `WF_WORKERS=1` enabling I/O overlap | Direction kept |
| `W=1` automatically preserves source order via the existing claim refusal | Does not hold |
| "no write to a world region after trap record" | Does not hold for in-flight I/O; must be changed to family-defined already-started semantics, or trap/I/O overlap must be forbidden |

## 8. Required revisions, in priority order

1. **P0: specify in-flight loans, terminal states, and abort teardown.**  
   Failure trace: a window exits, or a bare-metal instance reclaims the buffer, and DMA subsequently writes into already-reused memory. This item bears directly on Whitefoot's memory-safety guarantee.

2. **P0: write out the complete park/wake state machine, and fix the eventfd direction.**  
   Failure trace: reap the target CQE, set `DONE`, then continue into `io_uring_enter`; every lane waits for a nonexistent second CQE. After any CQE or wake hint has been processed, the lane must always return to the scheduling loop.

3. **P0: adjudicate whether trap and external submission can overlap.**  
   Failure trace: the later I/O is submitted first, and the earlier delayed-compute fallback then traps; `W=0` publishes zero bytes, `W=1` can publish bytes, even completing after the record. The recommendation for the first version is that the complete call closure of every I/O overlap window excludes `traps`.

4. **P0: split apart the W=1 mode, the compute-lane state, and the I/O-backend state.**  
   Failure trace: changing only the bootstrap query makes claim refuse even though the ring does not exist; after lane 0 is initialized, claim succeeds again. The existing emitter also moves a refused first member to after the last member. The query's contract must be renamed and it must explicitly refuse compute hand-out.

5. **P1: limit the scope of the join helper and its stack nesting.**  
   Failure trace: a join deep in the stack executes B, B's join then executes C, and the floor triggers in a program that could otherwise have waited for completion. Add a `help_depth`, or a same-window tag, and bring the scheduler layer into the stack ledger.

6. **P1: state explicit fairness for the CQ and compute frames.**  
   Failure trace: a persistent compute backlog means the lane never reaps a target CQE that has already arrived; in the opposite direction, a CQ flood can likewise starve compute. Specify target-first ordering, bounded batches, and a recheck after every frame.

7. **P1: fix the waiter/mailbox/disk-pool contract.**  
   Failure trace: the producer updates tail but has not yet linked node, and the consumer judges it empty and parks; or the waiter blocks on a full mailbox, and the lane that must wake up to drain it has not yet received the wake. This needs release/acquire, exactly-once terminality, preallocated nodes, reserved cancellation capacity, and per-lane fair dispatch.

Final verdict: §4's "one scheduler, two work sources" and §5's "`W=1` enables the world clock" are worth keeping; the literal wake mechanism, a join that steals arbitrarily, the existing refusal path, the single-lane source-order argument, and the judgment that v0.36 already adequately covers abort cannot be kept. The current design should be treated as an architectural sketch, not a runtime protocol that is safe to implement.
