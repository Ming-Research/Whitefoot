# Waiting functions: design and measurements

Status: direction agreed with the owner on 2026-09-27 in conversation. Nothing
in this file is a design-tree decision yet; each choice it states becomes an
amendment when it is proposed. It serves the concurrent I/O design and is
superseded by the design-tree nodes that record its surviving decisions.

## The question

Compute parallelism in Whitefoot is fork-join over proved independence, on a
runtime whose correctness and speed rest on one assumption: a compute task
never waits. The work-stealing join helps other work on its own stack, so a
task that blocks on a peer strands every join beneath it (see the
`docs/todo.md` entry "A handed-out call may block on a peer while a join waits
beneath it"). Every earlier I/O design either let waiting happen inside
ordinary calls and paid for it in the scheduler (park-on-miss), derived where
waiting happens and changed calling conventions silently (the continuation
branch), or kept waiting in the writer's own loop and could not say where that
loop may run (the batch loop of the 2026-09-13 handover).

The direction selected here separates the two by function kind:

- A function whose signature carries `waits` may call waiting functions;
  every other function may not. Compute code is therefore wait-free by
  construction, and a statement that contains a waiting call is never an
  overlap member, so no handed-out or helping task ever waits.
- A `waits` function is ordinary straight-line code. Its host operations
  suspend it until their completion arrives; the runtime keeps many such
  waiting contexts alive and resumes each one when its own completion
  arrives. Completion order is an input.
- Host effects are ordered only through shared owners: two statements proved
  independent order their state results, not their host effects (owner,
  2026-09-27).

The language design that follows from it is in [Design](#design); the
measurement that decided whether to start is Experiment 1.

The first question is not the language: it is whether the runtime this implies
can be as fast as the fastest hand-written server. If it cannot, the language
work is not worth starting.

## Experiment 1: the waiting runtime shape against the native echo servers

### Design

`research/experiments/io-completion-bench/waiting_echo.c` is the runtime a
`waits` program would compile to, written by hand in C. Each driver thread owns
one io_uring ring with `SINGLE_ISSUER | DEFER_TASKRUN`, one `SO_REUSEPORT`
listener and one accepting context. Each accepted connection gets its own
waiting context: a 64 KiB stack of reserved address space, a 64 KiB receive
buffer, and straight-line code that receives, sends everything it received,
and repeats until the peer finishes. A `wait` fills one submission entry that
names the context and switches to the driver; the driver submits, waits for
completions and switches back into each context its completions name.
Contexts never migrate between threads. Sends are tried once without blocking
before they go to the ring; `--inline-receive` does the same for receives.

The comparison is the bundle's existing TCP protocol (`linux-net-bench.sh`),
unchanged: `uring_echo`, the io_uring reference with multishot accept,
multishot receive into a provided buffer ring and one ring per core, and
`epoll_echo`, measured by the same `netload` in alternating pass order.

### What would distinguish the hypotheses, stated before measuring

The shape differs from `uring_echo` in three ways: one switch into and one out
of a context per wait, a single-shot receive per message instead of a
multishot one, and a per-connection buffer instead of a kernel-provided one.

- The direction is confirmed on this host if the waiting line's median rate is
  at least 0.90 of the better of the two references at 64 and at 1024
  connections with 64-byte messages, and its bytes per second at least 0.90
  of the better reference with 64 KiB messages.
- A line below 0.90 anywhere is attributed before any language work starts. If
  a variant that issues the reference's operations (multishot receive into
  provided buffers) from the same waiting contexts still stays below 0.90,
  the switching itself is the cost and the stackful runtime shape is rejected
  for this workload.
- One connection is reported but not judged: it measures latency of a single
  exchange, which no concurrency design changes.

Host: the development container, 4 online CPUs (Intel Xeon, 2.1 GHz),
Linux 6.18.44, client and servers on the same host. A result here describes
this host; the hosted `io-bench.yml` runner is the second host.

### Result

Measured 2026-09-27 with
`ROOT=… OUT=… NET_LINES="uring epoll waiting" ROUNDS=5 WARMUP=1 sh linux-net-bench.sh measure`,
run twice back to back. Each line is the median of 5 recorded passes. The
right-hand columns are the waiting line's rate divided by the named reference.

| Case | Run | uring rt/s | epoll rt/s | waiting rt/s | waiting / uring | waiting / epoll |
|---|---|---:|---:|---:|---:|---:|
| 1 conn, 64 B | 1 | 33,028 | 30,846 | 31,462 | 0.95 | 1.02 |
| 1 conn, 64 B | 2 | 31,880 | 30,180 | 31,930 | 1.00 | 1.06 |
| 64 conns, 64 B | 1 | 333,403 | 310,180 | 315,952 | 0.95 | 1.02 |
| 64 conns, 64 B | 2 | 344,727 | 300,230 | 328,182 | 0.95 | 1.09 |
| 1024 conns, 64 B | 1 | 374,886 | 346,979 | 359,180 | 0.96 | 1.04 |
| 1024 conns, 64 B | 2 | 380,326 | 344,800 | 362,464 | 0.95 | 1.05 |
| 64 conns, 64 KiB | 1 | 73,478 | 95,286 | 93,040 | 1.27 | 0.98 |
| 64 conns, 64 KiB | 2 | 75,341 | 89,180 | 83,799 | 1.11 | 0.94 |

The criterion is met in both runs. Against the better reference, the waiting
line reaches 0.95 and 0.95 at 64 connections, 0.96 and 0.95 at 1024
connections, and 0.98 and 0.94 of epoll's bytes per second with 64 KiB
messages. The loss against `uring_echo` at small messages is 4–5 percent and
was not attributed further. The three candidate causes named above
(switching, single-shot receive, per-connection buffers) remain unseparated,
and the provided-buffer variant was not needed to reach the bar.

Peak server resident memory, one run each, from `/usr/bin/time -f %M` around
the server during one `netload` run:

| Server | 1024 conns, 64 B | 64 conns, 64 KiB |
|---|---:|---:|
| `uring_echo` | 20,664 KiB | 11,584 KiB |
| `epoll_echo` | 1,960 KiB | 1,832 KiB |
| `waiting_echo` | 11,728 KiB | 7,760 KiB |

At 1024 connections the waiting line holds about 10 KiB per connection above
its base: the touched pages of one stack and one receive buffer. That is below
the io_uring reference, whose provided buffer ring is allocated up front, and
well above the epoll reference, which keeps no buffer for an idle connection.

What this does not establish:
- that a compiler emits code as good as this hand-written C;
- that the result holds on the hosted runner or on hardware with more cores;
- anything about files, or about a context that computes between waits.

## Experiment 2: the compiled context server

### Design

`tests/programs/tcp_contexts.wf` is the Whitefoot server this design makes
possible: the entry accepts, and each accepted connection is served by
`mustpar serve(...)` in a context of its own, which receives into a 16 KiB
inline window and sends back what it received until its peer finishes. It is
compiled by the ordinary compiler with no flag, and it runs every context on
the entry's one thread with one ring (`design/amendments/compiler-waiting-contexts.md`).

The references are the three C servers of Experiment 1 under the same
`linux-net-bench.sh measure` protocol, each run twice: at its default of one
thread per online CPU, and with `--threads 1`, the one-driver shape the
compiled server has.

### What would distinguish the hypotheses, stated before measuring

- The lowering and runtime add no material cost to the shape if, at one
  driver each, the compiled server's median rate is at least 0.90 of
  `waiting_echo --threads 1` at 64 and at 1024 connections with 64-byte
  messages, and its bytes per second at least 0.90 of it with 64 KiB
  messages. Below 0.90, the cost is attributed (the switch, the scan for
  completed records, the emitted receive and send path) before anything else
  is built on it.
- Against the references at their default thread counts, the ratio measures
  what the single driver costs. It decides nothing here; it is the number the
  `docs/todo.md` entry on one driver thread reopens on.
- One connection is reported but not judged, as in Experiment 1.

## Design

Agreed with the owner in conversation on 2026-09-27; the specification text
is kernel-spec v0.74 [WAIT-1, WAIT-2, PAR-4, HOST-1], and each choice below is
proposed to the design tree as an amendment.

### Waiting is a function kind the writer declares

`waits` after a signature's effect row declares a waiting function. A call of
a waiting function is admitted only in the body of another waiting function
[WAIT-1]; the entry may wait and runs in the root context. There is no
call-site `wait` keyword: the signature already says which calls wait, and a
second spelling at every call would repeat it. There is no bridge that lets a
function that does not wait block on a waiting one: that bridge is exactly the
wait inside compute that strands a join (`docs/todo.md`, "A handed-out call
may block on a peer while a join waits beneath it").

Alternatives refused:

- Deriving where waiting happens from the call graph (the continuation
  branch): the writer cannot see which calls change their calling convention,
  and a derived classification makes a callee's body part of its callers'
  meaning.
- Letting any call wait and absorbing it in the scheduler (park-on-miss,
  `PARK-ON-MISS.md`): a task that waits under a helping join strands every
  join beneath it, so the scheduler has to detect and repair what the
  language could have excluded.
- Waiting inside the writer's own batch loop (the 2026-09-13 handover): the
  loop is ordinary code, so nothing states where it may run, and it needs a
  hand-written state machine per protocol.

### A context pauses only where it needs an unfinished result

A waiting call executes as an ordinary call. The runtime implements a host
operation's wait by switching from the calling context's stack to the driver,
which resumes another ready context or polls for completions [WAIT-2]. The
compiler does not transform a waiting function into a state machine.

Alternatives refused:

- Compiling a waiting function into a resumable state machine (the Rust
  `async` model): every local that lives across a wait moves into a
  compiler-built record, and the proofs checked on the written function would
  have to be carried to the transformed one. Experiment 1 shows that the
  stackful shape reaches 0.94 to 0.98 of the best hand-written server, so the
  transform buys no measured speed here.
- A host thread per context: one thread per connection is the design the
  echo references beat; its stack and scheduling cost grow with connections.

### `mustpar` asserts independence and starts contexts

One marker states that a construct proceeds independently of what follows it,
and the checker must prove it [PAR-4]:

1. On a counted loop, the loop's [PAR-2] permission.
2. On the call of a statement whose callee does not wait, [PAR-1] permission
   with the next statement.
3. On the call of an expression statement whose callee waits, a new context:
   every parameter of the callee is a value parameter and its result can be
   dropped, so the started call shares no storage with its starter; every
   context an activation starts completes before the activation leaves.

The first two forms are proof syntax: they grant nothing, are erased before
lowering and leave overlap an implementation liberty, so a program that
states them means what it meant without them. Their use is to make a lost
parallelism a rejection instead of a silent sequential run. The third form is
the only way to start a context.

Alternatives refused:

- A separate `spawn` statement: the condition under which starting a context
  is sound is the same independence the first two forms state, so a second
  keyword would repeat one judgment under two names.
- Starting a context for every independent waiting statement automatically:
  an implementation would have to choose where a waiting call runs, and a
  context outlives the statement that starts it, which is not an
  implementation liberty.
- Unstructured tasks with handles: a handle is a value that must be joined or
  dropped, which needs a new type and a new consuming rule; joining every
  started context at the activation's exit needs neither.
- Reference parameters for a started call: a reference cannot outlive the
  statement under [REF-3], and a started call does.

### Host effects are ordered through state, not through statement order

Two host operations of one context take effect in order exactly when their
footprints overlap with a write [HOST-1]. Independent operations have no host
order, including while one of them has not completed or never completes. A
program that needs two operations ordered passes both through one owner.
This is the owner's ruling, written in Chinese: the order of host effects is
not for user code to decide, it is for the API to decide.

This resolves the `docs/todo.md` entry "Overlap can produce host effects that
no sequential execution produces": that overlap is now permitted behavior.

Alternative refused: preserving sequential host order under overlap. It would
forbid overlapping any two statements that each reach the host, which removes
the concurrency this work exists to provide, or it needs a hidden global order
that no footprint states.

### A shared handle budget

A started context takes its arguments by value, so it cannot borrow its
starter's `HandleFactory`. `factory_share` returns a second factory drawing on
the same budget: an acquisition through either spends a credit of the one
budget and a close through either returns one.

Alternatives refused:

- Splitting the credits between two factories: a fixed partition refuses an
  acquisition while the other factory holds unused credits.
- A reference to the factory in the started call: refused by the value
  parameter condition above.

The accounting stays a plain counter because every context runs on one driver
thread and only a waiting host call writes the counter, and a waiting call
never runs on a compute worker [PAR-1, PAR-2].
