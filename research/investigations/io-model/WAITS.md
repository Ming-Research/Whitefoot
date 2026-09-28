# Waiting functions: design and measurements

Status: direction agreed with the owner on 2026-09-27 in conversation, and
revised with the owner on 2026-09-28: waiting functions compile to resumable
frames instead of running on stacks of their own ([A waiting function is a
resumable frame](#a-waiting-function-is-a-resumable-frame)), and a program
means its sequential execution ([The program means its sequential
execution](#the-program-means-its-sequential-execution)); Experiment 3 found
the frame server at the stackful server's throughput with a mapping count
that does not grow with its connections. The owner approved its decisions on
2026-09-28, and they are now the design-tree nodes `language/waiting`,
`language/parallelism`, `language/system-interface`,
`language/system-interface/handle-factory` and `compiler/waiting-contexts`
(`design/log.md`); this file keeps their grounds and measurements and is
superseded by those nodes wherever the two differ.

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
`mustpar serve(...)` in a context of its own, which receives into an inline
window and sends back what it received until its peer finishes; the first run
used 16 KiB and every later one 64 KiB, the size `waiting_echo` uses. It is
compiled by the ordinary compiler with no flag, and it runs every context on
the entry's one thread with one ring (`design/compiler/waiting-contexts.md`).

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

### Result

Measured 2026-09-27 on the same host as Experiment 1 with
`linux-net-bench.sh measure`, `ROUNDS=5 WARMUP=1`. The one-driver runs put
wrappers named after the three references in `$OUT` that start each with
`--threads 1`; the default runs use the references as built. Each number is
the median of five recorded passes; the ratio is the compiled server's rate
over `waiting_echo`'s in the same run.

The first run found the compiled server well below the bar, and each cost was
removed only after it was attributed:

| Compiled server | 64 conns | 1024 conns | 64 KiB, bytes/s |
|---|---:|---:|---:|
| First build, 16 KiB echo window | 0.64 | 0.59 | 0.47 |
| 64 KiB window, a join parks without entering the ring | 0.81 | 0.68 | 1.21 |
| and a record published on the context thread wakes its context by address | 0.94 | 1.12 | 1.14 |

Those three rows are two-line runs (`waiting wf`, three recorded passes).
The window size is the program's, not the runtime's: `waiting_echo` receives
into 64 KiB, and the compiled server now does too. The third row's
`waiting_echo` medians were the lowest of any run, and the full four-line
protocol then placed the same build at 0.77 and 0.85 at 64 connections.

A strace count of 64,000 round trips at 64 connections showed the one
remaining difference in system calls: the compiled server tried every receive
once without waiting before it went to the ring, 64,064 `recvfrom` calls
against none, while `waiting_echo` receives only through the ring. One run
measured both builds of the compiled server beside `waiting_echo`:

| Compiled server | 64 conns | 1024 conns | 64 KiB, bytes/s |
|---|---:|---:|---:|
| receive tried once without waiting | 0.80 | 0.86 | 0.95 |
| receive straight to the ring with other contexts live | 0.90 | 0.94 | 1.00 |

The final build, two runs of the full protocol at one driver each:

| Case | Run | uring rt/s | epoll rt/s | waiting rt/s | compiled rt/s | compiled / waiting |
|---|---|---:|---:|---:|---:|---:|
| 1 conn, 64 B | 1 | 32,874 | 32,291 | 30,979 | 29,994 | 0.97 |
| 1 conn, 64 B | 2 | 31,084 | 31,552 | 31,973 | 29,705 | 0.93 |
| 64 conns, 64 B | 1 | 120,421 | 115,674 | 170,487 | 150,163 | 0.88 |
| 64 conns, 64 B | 2 | 123,315 | 115,755 | 179,425 | 157,872 | 0.88 |
| 1024 conns, 64 B | 1 | 130,264 | 110,721 | 115,424 | 121,578 | 1.05 |
| 1024 conns, 64 B | 2 | 127,325 | 117,721 | 130,842 | 105,127 | 0.80 |
| 64 conns, 64 KiB | 1 | 28,858 | 41,595 | 42,153 | 35,864 | 0.85 |
| 64 conns, 64 KiB | 2 | 28,196 | 43,434 | 34,639 | 38,087 | 1.10 |

The 64 KiB rows are round trips per second; bytes per second are the same
ratios. The criterion is not met cleanly. At 64 connections the compiled
server holds 0.88 of `waiting_echo` in both runs, and the remaining 12
percent is not attributed; the candidates are the park path, which waits in
`epoll_wait` and then enters the ring where the hand-written driver makes one
entry, the locks the ring's submit and reap take on every pass, and the
emitted receive and send path. At 1024 connections and with 64 KiB messages
the reference itself moved by 13 and 22 percent between the two runs, so
those two ratios bracket the bar rather than settle it. At one thread each,
the compiled server is ahead of the `uring_echo` and `epoll_echo` references at
64 connections and between 0.83 and 1.10 of them at 1024.

At the references' default of one thread per CPU (four here), one run:

| Case | uring rt/s | epoll rt/s | waiting rt/s | compiled rt/s | compiled / best |
|---|---:|---:|---:|---:|---:|
| 64 conns, 64 B | 333,925 | 325,766 | 315,439 | 169,909 | 0.51 |
| 1024 conns, 64 B | 367,117 | 346,668 | 346,348 | 110,468 | 0.30 |
| 64 conns, 64 KiB | 76,782 | 96,340 | 87,311 | 39,450 | 0.41 |

That is the cost of the single driver thread, which the `docs/todo.md` entry
"Every waiting context runs on the one thread that runs the entry" now names.

What this does not establish:
- that the unattributed 12 percent at 64 connections is gone;
- anything on the helper route or on another host;
- anything about a context that computes between waits.

### The readiness route on a host without a ring

With no kernel completion ring, a socket operation goes to the helper pool,
whose threads block in the host call; `WF_BRIDGE_MAX_HELPERS` caps that pool
at eight. One check on 2026-09-27, at revision `a06d6cf1`, built
`tcp_contexts.wf` twice: once as shipped, and once against a runtime whose
`wf_bridge_waits_for_readiness` always answered no, so that every socket wait
went to the helpers. Each build served N peers that speak in the reverse of
their acceptance order, with `WF_IO_NO_NATIVE_RING=1` and five seconds for each
answer. A server that cannot hold every silent peer at once never answers the
last one.

| Peers | Helpers only | Readiness waits |
|---:|---|---|
| 4 | all answered | all answered |
| 8 | all answered | all answered |
| 9 | peer 8 not answered | all answered |
| 16 | peer 15 not answered | all answered |

The helper-only runtime holds exactly as many silent peers as the pool has
threads. The shipped route waits for readiness on the contexts' own thread and
holds all of them. With four peers the two runtimes do not differ, so a check
of this route needs more peers than helpers.

## Experiment 3: the resumable-frame server

### Design

The same `tests/programs/tcp_contexts.wf`, unchanged, compiled once by the
stackful compiler of Experiment 2 (the parent of the first commit that lowers
waiting functions to frames) and once by the frame compiler ([A waiting
function is a resumable frame](#a-waiting-function-is-a-resumable-frame)),
both with no flag. The references are `waiting_echo --threads 1` and the two
C servers of Experiment 1 under the same `linux-net-bench.sh measure`
protocol, run twice.

Memory is measured on the same program with idle connections: a client opens
N connections, sends nothing, and the server's resident set and mapping count
are read from `/proc` once every connection has been accepted. The
development host's descriptor limit is 20,000 and cannot be raised inside its
container, which bounds N below both servers' ceilings, so N is 1,000, 5,000
and 19,000, and the ceiling is judged by what produces it: the number of
mappings each connection adds. Revised before any measurement, when the limit
was found; the earlier text asked for N up to 100,000.

### What would distinguish the hypotheses, stated before measuring

- The frame costs no throughput if, in both runs, the frame server's median
  rate is at least 0.83 of `waiting_echo --threads 1` at 64 connections with
  64-byte messages, that is within 0.05 of the stackful server's 0.88 in
  Experiment 2, and within the stackful server's range at 1024 connections
  and with 64 KiB messages (0.80 to 1.10), where the reference itself moved
  13 and 22 percent between runs. Below that, the cost is attributed before
  any later revision is built on it.
- The frame removes the stackful ceiling if the frame server's mapping count
  does not grow with N while the stackful server's grows by two per
  connection, the growth that stops it near 32,000 connections under the
  default mapping limit, and the frame server's resident memory per idle
  connection, measured in the same run, is below the stackful server's.
  Holding 100,000 connections is left to a host whose descriptor limit
  allows it.
- One connection is reported but not judged, as in Experiments 1 and 2.

### Result

Measured 2026-09-28 with `linux-net-bench.sh measure`, `ROUNDS=5 WARMUP=1`,
every line at one driver thread as in Experiment 2. The frame server is the
release compiler at the commit that fixes the runtime probe (`0d2b2e753`), the
stackful server the compiler at `4c0d413e1`. Each rate is the median of five
recorded passes; the ratios are over `waiting_echo`'s rate in the same run.
Every line, the references included, ran slower than in Experiment 2, so only
ratios within a run compare.

| Case | Run | uring rt/s | epoll rt/s | waiting rt/s | stackful rt/s | frames rt/s | stackful / waiting | frames / waiting |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 1 conn, 64 B | 1 | 17,177 | 19,157 | 18,314 | 17,360 | 17,848 | 0.95 | 0.97 |
| 1 conn, 64 B | 2 | 17,726 | 17,523 | 18,741 | 17,399 | 16,662 | 0.93 | 0.89 |
| 64 conns, 64 B | 1 | 91,627 | 88,247 | 138,316 | 116,437 | 115,673 | 0.84 | 0.84 |
| 64 conns, 64 B | 2 | 92,782 | 84,421 | 122,004 | 118,736 | 113,959 | 0.97 | 0.93 |
| 1024 conns, 64 B | 1 | 81,813 | 77,460 | 93,077 | 81,372 | 89,551 | 0.87 | 0.96 |
| 1024 conns, 64 B | 2 | 77,544 | 80,991 | 102,244 | 89,803 | 97,539 | 0.88 | 0.95 |
| 64 conns, 64 KiB | 1 | 20,801 | 29,541 | 30,096 | 29,292 | 30,694 | 0.97 | 1.02 |
| 64 conns, 64 KiB | 2 | 22,840 | 28,800 | 32,913 | 28,183 | 31,674 | 0.86 | 0.96 |

The throughput criterion is met: at 64 connections the frame server holds
0.84 and 0.93 of `waiting_echo`, both at least 0.83, and 0.96 and 0.95 at
1024 connections and 1.02 and 0.96 with 64 KiB messages, inside 0.80 to 1.10.
The first run's 0.84 sits near the bar because `waiting_echo` itself ran 13
percent faster in that run than in the second, and the stackful server
measured beside it held the same 0.84; the frame and stackful servers are
within 0.04 of each other at 64 connections in both runs, and the frame
server is ahead at 1024 connections and with 64 KiB messages. At one thread
each, the frame server is ahead of `uring_echo` and `epoll_echo` on every
case with more than one connection; the stackful server trails one of them
at 1024 connections in the first run and with 64 KiB messages in both.

Idle connections, one run of `linux-net-bench.sh memory`:

| N | frames RSS/conn | stackful RSS/conn | frames mappings | stackful mappings |
|---:|---:|---:|---:|---:|
| 1,000 | 70.03 KiB | 68.03 KiB | 37 | 2,036 |
| 5,000 | 70.01 KiB | 68.01 KiB | 37 | 10,036 |
| 19,000 | 70.01 KiB | 68.00 KiB | 37 | 38,036 |

The mapping half of the ceiling criterion is met: the frame server's mapping
count is the same at every N, and the stackful server adds two mappings per
connection, 38,036 at 19,000 connections, which is 58 percent of the host's
`vm.max_map_count` of 65,530. The resident half is not met: the frame server
holds 70.0 KiB per idle connection, 2.0 KiB more than the stackful server's
68.0 at every N. Both hold the program's 64 KiB echo window per connection,
the stackful server on its stack and the frame server in the frame of
`serve`, so the window decides nearly all of it and page rounding decides the
rest. The frame server carves three pool blocks per connection in a row: the
context record, 536 bytes in a 1 KiB block; the first arena chunk, 1 KiB,
holding the argument block and the wrapper's frame; and a chunk whose first
67,376 bytes are `serve`'s frame. That touches 69,456 bytes, 16.96 pages,
and the connections' blocks follow each other every 130 KiB, so every other
connection starts half way into a page and touches 18 pages instead of 17:
17.5 pages, 70 KiB, on average. The stackful server's window and frames fit
in 17 pages. No layout of this program's per-connection state goes below 17
pages, so for this program frames can at best equal the stack's resident
memory, and the criterion as stated could not have been met by any runtime
change; what frames remove here is the two mappings per connection.

The first memory run never finished. `idleload` waited for the server to hold
N more descriptors than before, but a server started for exactly N
connections closes its listener after its last accept, so both servers
stayed one short forever. The tool now counts the server's descriptors that
the kernel's TCP table lists as established on the server's port.

What this does not establish:
- that frames cost less resident memory than stacks for a context whose
  state is small, where a stack still touches at least one 4 KiB page and a
  frame only its own bytes; this program's per-connection state is its
  64 KiB window;
- holding more than 19,000 connections, which the host's descriptor limit
  forbids;
- anything on the helper route, on another host, or at more than one driver
  thread.

## Design

Agreed with the owner in conversation on 2026-09-27; the specification text
is kernel-spec v0.77 [WAIT-1, WAIT-2, PAR-4, HOST-1], and the owner approved
the design-tree nodes that record each choice below on 2026-09-28.

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

### A waiting function is a resumable frame

Revised with the owner on 2026-09-28; it replaces the stackful shape of
Experiments 1 and 2, whose measurements stay here as the evidence that shape
produced.

Each waiting function compiles to a resumable frame. An activation's frame is
allocated when it is entered, from the arena of the context that runs it, and
released when its caller has read its result, so a context's frames are
allocated and released last in, first out. The arenas and the contexts
themselves take their memory from host regions the driver reserves, never
from the program's allocator: the runtime every build links does not call it
[STOR-8], and one 64 MiB region serves thousands of contexts with one kernel
mapping. The values that live across a
suspension point are kept in the frame; everything else stays where ordinary
code keeps it. A host operation that has not completed suspends the frame and
returns to the driver; its completion makes the context ready, and the driver
resumes the frame that suspended. A context [WAIT-2] is one chain of such
frames. The root context runs the entry, and each context a `mustpar` start
creates [PAR-4] runs its wrapper; one driver thread resumes every context, and
compute tasks run on the compute workers and never wait [PAR-1, PAR-2].

The frame is LLVM's switched-resume coroutine: the frame starts suspended, a
caller transfers into its callee and a finishing callee transfers back to its
caller by resuming the target immediately before its own suspension, which
LLVM 18's coroutine split turns into a tail call, so a chain of calls that
return to their callers does not grow the native stack. LLVM 18 has no
symmetric-transfer intrinsic (`llvm.coro.await.suspend.handle` links as an
undefined symbol); the resume-before-suspend form is what its split
recognizes. A design probe ran twenty million such call round trips inside
an 8 MiB stack at `-O0` and `-O2`; a compiler test pins the same property on
emitted code.

Why the stackful shape is replaced:

- It has a concurrency ceiling that a frame does not. Each context is a
  reservation of its own with a guard page below it, two kernel memory
  mappings, and the kernel's default limit on mappings per process
  (`vm.max_map_count`, 65,530 on the development host) therefore bounds a
  process to about 32,000 contexts. A frame is heap storage the size of the
  state that lives across a wait. Experiment 3 measures both.
- A compute join inside a waiting function holds the driver's stack, so no
  other context runs until the join finishes. A frame can suspend at such a
  join; that is the next revision's work, not this one's.
- The reasons recorded against resumable frames do not hold. Proofs are
  erased before lowering, so a frame is a lowering of the checked function
  like any other and carries no proof. The continuation branch was refused
  because it derived which functions suspend from the call graph; here the
  writer declares the kind [WAIT-1], so the calling convention follows the
  written signature and a leaf that begins to wait is a rejection in every
  caller that does not declare `waits`.

Alternatives refused:

- Stacks of their own for waiting contexts (Experiments 1 and 2): the mapping
  ceiling and the resident stack above, and a compute join stops every
  context.
- Returning every finished callee through the driver's ready queue instead of
  a tail transfer to its caller: a queue push and pop per return that the
  split's tail call makes unnecessary.
- The continuation branch's two-thread coordinator (Experiment 39 of
  `SCHEDULER-FINDINGS.md`): 2.6 to 7.1 process switches per round trip made
  it uncompetitive. Here the driver that reaps completions resumes the frames
  on its own thread, as the stackful runtime of Experiment 2 did.
- A host thread per context: one thread per connection is the design the
  echo references beat; its stack and scheduling cost grow with connections.

### `mustpar` asserts independence

This is the kernel-spec v0.77 text, which carries the revision agreed in [The
program means its sequential execution](#the-program-means-its-sequential-execution).

One marker states that a construct proceeds independently of what follows it,
and the checker must prove it [PAR-4]:

1. On a counted loop, the loop's [PAR-2] permission.
2. On the call of a statement whose callee does not wait, [PAR-1] permission
   with the next statement.
3. On the call of an expression statement whose callee waits, [WAIT-2]'s
   permission to execute the call alongside the statements after it: every
   parameter of the callee is a value parameter and its result can be
   dropped, so the call shares no storage with those statements, and it
   completes before the activation leaves.

All three forms are proof syntax: they grant nothing, are erased before
lowering and leave overlap an implementation liberty, so a program that
states them means what it meant without them. Their use is to make a lost
parallelism a rejection instead of a silent sequential run. The compiler's
policy is that every call marked in the third form runs as a context of its
own ([What the first version keeps open](#what-the-first-version-keeps-open));
that is a property of this compiler, not of the language.

Alternatives refused:

- A separate `spawn` statement: the condition under which running a call as
  a context is sound is the same independence the first two forms state, so
  a second keyword would repeat one judgment under two names.
- Unstructured tasks with handles: a handle is a value that must be joined or
  dropped, which needs a new type and a new consuming rule; joining every
  context at the activation's exit needs neither.
- Reference parameters for a call run as a context: a reference cannot
  outlive the statement under [REF-3], and such a call does.

Running a context for an unmarked independent waiting call was refused in the
v0.76 text, because a context outlived its statement and so changed what the
program did. Under the sequential meaning it changes only when the call's
host effects happen, which [HOST-1] already leaves open, so v0.77 permits it;
this compiler still runs only marked calls as contexts.

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

A call run as a context takes its arguments by value, so it cannot borrow its
starter's `HandleFactory`. `factory_share` returns a second factory drawing on
the same budget: an acquisition through either spends a credit of the one
budget and a close through either returns one.

Alternatives refused:

- Splitting the credits between two factories: a fixed partition refuses an
  acquisition while the other factory holds unused credits.
- A reference to the factory in the marked call: refused by the value
  parameter condition above.

The accounting stays a plain counter because every context runs on one driver
thread and only a waiting host call writes the counter, and a waiting call
never runs on a compute worker [PAR-1, PAR-2].

The split first proposed to replace this budget does not serve the server
this work measures, and the owner kept the shared budget under the sharing
rule as stated below
([Sharing between concurrent activities](#sharing-between-concurrent-activities)).

### The program means its sequential execution

Agreed with the owner on 2026-09-28. The meaning of a program is the meaning
of its sequential execution. Every concurrency the implementation adds, an
overlapped statement, a started context, where a context runs and on which
thread a compute task runs, is derived from proved independence and is an
implementation liberty of the same kind as marking a pointer `noalias`: one
path-disjointness judgment [OWN-7, EFF-5] authorizes all of them. The writer
reasons sequentially, and the concurrency obligation moves from every program
into one trusted runtime.

Consequences, the first two carried by kernel-spec v0.77:

- `mustpar` on a waiting call becomes an assertion like its other two forms.
  It states that the call is independent of every statement that follows it
  in the activation, is a rejection when that is not proved, and guarantees
  no overlap. The compiler's policy, recorded as a compiler decision rather
  than promised by the language, is that every marked waiting call runs as a
  context of its own and its starter waits for it at the activation's exit.
- [WAIT-2]'s progress guarantee for contexts is removed, since a sequential
  execution of the same program is a conforming one.
- Later, carried by kernel-spec v0.78: a marked waiting call may bind its
  result, and the starter joins it where the result is first used ([A bound
  context is joined where its result is first used](#a-bound-context-is-joined-where-its-result-is-first-used)).

Refused: a separate keyword whose meaning is that a context must start. The
only difference it would make is a promise of progress, which the sequential
reading does not need and which no single-implementation research compiler
has to write into the language.

### A bound context is joined where its result is first used

Directed by the owner on 2026-09-28 ("do `let a = mustpar f(…)`, joined where
the result is first used"); kernel-spec v0.78 [WAIT-2, PAR-4]. A waiting call
in a `let` right-hand side whose callee takes only value parameters may run
alongside the statements after it, and it completes before its binding is
next read, written or released and before the activation leaves. `mustpar`
asserts that permission, as it does for an expression statement, and this
compiler runs every marked one as a context of its own.

Where the join stands is the compiler's choice under that rule. It is placed
before the first later statement of the `let`'s block whose [PAR-1]
footprint reaches the binding, or that the footprint judgment refuses
because it may leave the block (`return`, `give`, `break`, error
propagation) or has a form the judgment does not compute (a loop, a match
that is not rooted in a call), and otherwise at the block's end. The
footprints are the ones overlap permission already relies on, and they fail
closed, so the join precedes every use, the release at the block's end
included, and a context started in a loop body is joined before the next
iteration reuses its result slot. A statement that waits does not end the
run: two marked fetches both proceed until the statement that combines
their results. The context writes its result into a slot of the starting
frame, which outlives the context because the join precedes every exit.

Evidence: `two_bound_fetches_proceed_together_on_both_routes`
(`compiler/tests/programs/network.rs`) runs `tcp_gather.wf` against two
servers, the first of which answers only after the second has received its
request. It passes with the plan and fails, after 20 seconds, when every
bound context is joined at the statement after its `let`, which is the
sequential order.

Alternatives refused:

- Joining at the statement after the `let`: it loses the concurrency this
  form exists for whenever independent work stands between the call and its
  use, as it does in the gather above.
- Joining inside the statement that first uses the binding, on the path that
  reaches the use: a use in one arm of a match would define the binding on
  that path only, and every later join would have to merge a joined and an
  unjoined path. Joining before the whole statement costs the concurrency of
  that statement's other arms and keeps one definition.
- Joining only at the block's end: a use before the end would read a result
  that has not arrived.

What this does not do yet: a loop or a non-call match between the `let` and
its use ends the run early, because the footprint judgment refuses those
forms; a footprint for them would let the call proceed across them.

### Sharing between concurrent activities

A host interface may split one resource into separately held parts only when
operations through different parts commute under every observation the
interface defines, so that reordering them changes no observation, except
through an outcome the interface already lets the host produce at any call,
which a program must handle in every order anyway. Two files
written separately commute; two writers of one standard output do not,
because the byte order is observed, so standard output has one owner. The
two ends of a channel do not commute, because what a receive returns and
whether it waits depend on the sends, and a channel whose ends are separate
values is therefore not admitted. External systems are outside this rule:
the specification does not define how a peer or a database answers, and a
program is correct for every answer.

The rule was first stated without that clause, and `factory_share` failed it:
two factories drawing on one budget let one acquisition's refusal depend on
what the other factory holds, so its result depends on the order of
operations in two contexts.

A split that gives each part a fixed share of the credits was proposed to
replace it, and it does not serve the server this work measures. The accept
loop acquires each connection through the listener's factory, so the credit
leaves the listener's budget; `serve` closes the connection through the part
it was given, so the credit returns to that part; and the part is dropped
when `serve` finishes. Every connection therefore moves one credit out of the
listener's budget for good, and the listener refuses every acquisition after
as many connections as its budget held. Returning a dropped part's credits to
the budget it came from restores the order dependence the split removes,
because the listener's refusal then depends on when the other contexts
finish. Returning them through the part's owner needs the starter to join
the context and take the part back, which an accept loop that never ends
cannot do.

Two forms remained, and the owner chose the first on 2026-09-28:

- Keep one shared budget and state the sharing rule over what the interface
  lets a program observe: parts may interact through an outcome the
  interface already lets the host produce at any call. The interface already
  lets the host refuse an acquisition the budget would fund, so a program
  handles a refusal at every acquisition in every order, and a refusal caused
  by another context's acquisitions becomes one more input of the execution,
  like which of two operations completes first [WAIT-2]. Standard output and
  a channel still fail the rule, because byte order and what a receive
  returns are not outcomes a program must handle in every order.
- Keep the rule as stated and split the budget, and give a context a way to
  hand its part back to its starter. That needs the starter to join the
  context and receive the part, which is the later `let a = mustpar f(…)`
  form, and an accept loop that never ends still cannot join its contexts
  before it runs out.

The clause in the rule's first paragraph is that choice, and `factory_share`
stays as kernel-spec v0.77 states it.

Mutable state shared by several concurrent activities has no admitted form in
this revision. A program keeps such state behind an external system, or in
one owner that processes requests in the order the host delivers them (the
Redis shape). Two later forms were considered and deferred until a program
needs one: an explicit shared object whose operations are whole atomic
transactions in an unspecified order, and one whose transactions take effect
in the order of the host completions that produced them. Both revise
[CAP-1].

### What the first version keeps open

The first version is one driver thread, frames, `mustpar` starts and the
sharing rule above. Every later need considered with the owner is an
addition to it, provided the first version keeps these properties:

1. Every marked waiting call runs as a context of its own. A later form that
   orders transactions by host completion depends on which calls are
   contexts, so this is a compiler constraint and not an optimization choice.
2. Every completion reaches a context through one entry per driver, where a
   completion stamp can later be added.
3. The frame layout and the context header are private to the runtime.
4. A context's frames are released only after every operation it has in
   flight has completed or been cancelled and reaped.
5. Join bookkeeping is dynamic: a loop may start any number of contexts.
6. Every split host interface satisfies the sharing rule, and no program can
   observe which calls ran as contexts.
7. The footprint classification of [PAR-1] can take a new class, the one a
   shared object would need.

| Later need | Form | First version changes |
|---|---|---|
| Shared mutable state | external system; one owner in host order; a shared object | none; a shared object adds a footprint class and revises [CAP-1] |
| Shutdown | an external signal as host input; pending operations complete as cancelled | none, given property 4 |
| Select and timeouts | a host operation over several operations, and operations with a deadline | none |
| Logging | each context writes its own output, or a record sink whose observation is a set of records | none |
| Several driver threads | placement at the start of a context | none |
