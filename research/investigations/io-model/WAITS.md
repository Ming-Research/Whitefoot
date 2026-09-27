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

- A function whose signature carries `waits` may contain `wait` calls; every
  other function may not, and may not call a `waits` function. Compute code is
  therefore wait-free by construction, and a statement that contains a `wait`
  is never an overlap member, so no handed-out or helping task ever waits.
- A `waits` function is ordinary straight-line code. Its host operations are
  `wait` calls that suspend it until their completion arrives; the runtime
  keeps many such waiting contexts alive and resumes each one when its own
  completion arrives. Completion order is an input.
- Host effects are ordered only through shared owners: two statements proved
  independent order their state results, not their host effects (owner,
  2026-09-27).

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
