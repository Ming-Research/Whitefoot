# Streams and TCP under T4

Status: AMENDED. Slice 1 of §7 landed as specification v0.46 on
`io/t4-resource-relations` on 2026-09-05: the two renames, `command.stdin` and
`read_next`, and the types and operations of §4 are in `spec/kernel-spec.md`
under [SYS-15] through [SYS-18], with the merge-time record in
`archive/APPROVALS.md` and the derivation rows in
`spec/derivation/derivation-ledger.md`. Slice 2 landed on the same branch on
2026-09-05: every TCP operation lowers and runs on POSIX, on both the ring and
the adapter routes §5 names. Slice 3 landed on 2026-09-06: every TCP operation
runs on Windows too, on the completion port and on the adapter. Slice 4, the
control test, and slice 5, the batch record `archive/done/0108-streams-and-tcp.md`,
landed on 2026-09-06 and close the batch; the owner decisions at the end are
the ones the amendment implements. Where this document and the specification
differ, the specification is the language.

## 1. Why the network first

Concurrency's real demand is the network: reads are fast, writes slower, a
peer across a socket slower still, and the programs that hold thousands of
operations in flight hold them on connections. The owner ruled on 2026-09-05
that the first API that waits is therefore the network rather than standard
input alone, because a loopback gives the scheduler a real wait with a
controllable peer, and because a socket is where the park-on-miss design
(`PARK-ON-MISS.md`) is meant to pay for itself.

Standard input comes along as the first instance of the same stream design:
one readable byte stream, one operation, the same runtime path.

## 2. The T4 test, applied

Constitution T4: every finite resource a system operation consumes is an
owned value in the operation's signature, drawn from a factory whose capacity
is fixed at start and never larger than what the target provides; an
operation holding its resource cannot fail for want of it; what the mapping
cannot cover is honest target exhaustion, and that only is the typed error.

The finite resources a TCP program consumes, and where each sits on the API:

| resource | owner on the API | exhaustion outcome |
|---|---|---|
| a native descriptor, for a listener and for every connection | one credit of the entry's descriptor factory (today's `FileFactory`, renamed below), consumed by listen, accept and connect, handed back on failure, returned by the explicit closes | the reserve says `ResourceExhausted` in source order; the opens never |
| a local port | the `SocketAddress` value the program binds; two binds of one port are the program's own source-order conflict | `AddressInUse` is the host's answer to the second bind, the program's own outcome |
| an ephemeral port for connect | none on the API: the target's pool, outside the program | `AddressUnavailable`, honest target exhaustion |
| the accept queue | none on the API: a kernel queue the peer fills; the program observes it only through accept's outcomes | none; a full backlog refuses the peer, never the program |
| socket buffers | none on the API: send and receive report partial progress exactly as `write_once` and `read_at` do | none |

Overlap cannot invent an outcome the sequential program never produces: two
accepts on one listener each hold their own permit and take whichever
connection the kernel hands each; two connects each hold their own permit;
the two directions of one connection are two owners and overlap under
[PAR-1] like any two places. No scheduler ledger, no award, no retry.

## 3. Types

A connection is two owners from the start: the owner asked whether full
duplex could be added later without an architectural change, and the honest
answer is that a later `split` would need a second `receive` and `send`
signature over half types, because a system operation has one signature. So
the halves are the design, not a later refinement: `accept` and `connect`
return a receive half and a send half, each an ordinary owner with its own
loan, and full duplex inside one connection is two places overlapping under
[PAR-1] with nothing added. The descriptor credit belongs to the pair: the
explicit close takes both halves and returns the permit; derived release of
one half is the host's half-close of that direction (`shutdown`), and the
release of the second half closes the descriptor and spends the credit,
exactly as derived release of a `ReadFile` does. The runtime keeps the
"both halves gone" fact for itself; the checker sees only the pair close,
which is the relation T4 asks for.

Types beside the existing ones, and two renamed:

- `InputStream`: a readable byte stream with an implicit position. The entry
  input `command.stdin` at ordinal 5 supplies one as `own InputStream`, only
  when the entry selects it.
- `OutputStream`: the existing `Output`, renamed; `command.stdout` and
  `command.stderr` supply it as today.
- `SocketAddress`: an immutable value, an IPv4 or IPv6 address and a port,
  constructed by `socket_address_v4(a, b, c, d, port)` and
  `socket_address_v6(...)` with no host call. No name resolution in this
  batch.
- `TcpListener`: a state resource with one live state, one credit.
- `TcpConnection`: a system-declared struct of two fields, `receive: TcpReceive` and `send: TcpSend`; each half is a state
  resource with one live state; the pair holds one credit.
- The descriptor factory and its permit, renamed from `FileFactory` and
  `FilePermit` because sockets draw from the same table (decision 3 below).

## 4. Operations

Written with the working names `HandleFactory`, `HandlePermit`,
`command.handles` and `reserve_handle`; the owner picks the spelling.

```
fn read_next(input: &uniq InputStream, destination: &uniq buffer<u8>, start: own u64, end: own u64) -> result: own ReadOutcome reads(input, destination), writes(input, destination);
fn write_once(output: &uniq OutputStream, source: &buffer<u8>, start: own u64, end: own u64) -> result: own Result<u64, IoError> reads(output, source), writes(output);

fn socket_address_v4(a: own u8, b: own u8, c: own u8, d: own u8, port: own u16) -> result: own SocketAddress pure;

fn tcp_listen(permit: own HandlePermit, address: &SocketAddress) -> result: own ListenOutcome reads(permit, address), writes(permit);
fn tcp_accept(permit: own HandlePermit, listener: &TcpListener) -> result: own AcceptOutcome reads(permit, listener), writes(permit);
fn tcp_connect(permit: own HandlePermit, address: &SocketAddress) -> result: own ConnectOutcome reads(permit, address), writes(permit);
fn receive_next(receive: &uniq TcpReceive, destination: &uniq buffer<u8>, start: own u64, end: own u64) -> result: own ReadOutcome reads(receive, destination), writes(receive, destination);
fn send_once(send: &uniq TcpSend, source: &buffer<u8>, start: own u64, end: own u64) -> result: own Result<u64, IoError> reads(send, source), writes(send);
fn close_connection(connection: own TcpConnection) -> result: own HandlePermit reads(connection), writes(connection);
fn close_listener(listener: own TcpListener) -> result: own HandlePermit reads(listener), writes(listener);

enum ListenOutcome { Listening(listener: TcpListener); ListenFailed(error: IoError, permit: HandlePermit); }
enum AcceptOutcome { Accepted(connection: TcpConnection, peer: SocketAddress); AcceptFailed(error: IoError, permit: HandlePermit); }
enum ConnectOutcome { Connected(connection: TcpConnection); ConnectFailed(error: IoError, permit: HandlePermit); }
```

The pairing is on the API, by the owner's decision of 2026-09-05: the
connection is a system-declared struct with two fields,

```
struct TcpConnection { receive: TcpReceive; send: TcpSend; }
```

so `accept` and `connect` return one `TcpConnection`, the program borrows
`&uniq connection.receive` and `&uniq connection.send` as two places (an
effect path names a struct field, [EFF-1]; two exclusive loans on disjoint
fields coexist under [OWN-5]), and the close takes the whole value:

```
fn close_connection(connection: own TcpConnection) -> result: own HandlePermit reads(connection), writes(connection);
enum AcceptOutcome { Accepted(connection: TcpConnection, peer: SocketAddress); AcceptFailed(error: IoError, permit: HandlePermit); }
enum ConnectOutcome { Connected(connection: TcpConnection); ConnectFailed(error: IoError, permit: HandlePermit); }
```

Moving one half out of the record is the language's own partial move,
which kills the whole binding ([OWN] rules: "partial moves kill the whole
binding"), so a broken pair can never reach the explicit close; the moved
half lives on as an ordinary owner, the remainder is derived-released, and
the credit is spent, exactly as derived release spends it for a file. No
`split` or `join` operation exists. A system-declared struct is a new
category of system nominal beside the opaque and enum ones, and the
amendment's META-5 delta declares it.

The forms follow what the specification already has: the open outcomes hand
the permit back on failure exactly as `FileOpenOutcome` does; the explicit
closes return the credit exactly as `close_read` does; derived release
closes and spends the credit exactly as for `ReadFile`; `read_next` and
`receive_next` carry `read_at`'s range and outcome rules ([SYS-8]: empty
range makes no host transfer, one progress-producing attempt,
`ReadBytes(next)` only for `next > start`, `ReadEnd` with the buffer
unchanged); `send_once` carries `write_once`'s (`Ok(next)` means the local
facility accepted the prefix, `WriteZero` for a zero-length host write,
`BrokenPipe` through the same signal normalization). The names follow the
stream pair: `read_next` and `receive_next` advance a position, `write_once`
and `send_once` make one attempt. Target contract: `socket_address_v4`
never-suspends; everything else may-suspend, terminal, one-shot, with the
same milestones as the opens. `tcp_accept` borrows the listener `&`, so
several accepts may overlap through shared loans of one listener with their
own permits, which is how a server takes connections concurrently.
`IoError` already carries `ConnectionRefused`, `ConnectionReset`,
`ConnectionAborted`, `NotConnected`, `AddressInUse`, `AddressUnavailable`,
`TimedOut`, `BrokenPipe`.

The operation names are flat, as every system operation is today
(`open_read`, `close_read`, `read_at`). The owner noted they are not uniform
and may later move into namespaces; that is a specification-wide spelling
change for every operation and is not part of this batch.

## 5. Runtime

One lowering, as everywhere: submit, then join, through the frame's record.
The request kinds are added to `wf_file_request` (listen, accept, connect,
receive, send, stream read, half-close) and the routes are:

- Linux: `IORING_OP_ACCEPT`, `IORING_OP_CONNECT`, `IORING_OP_RECV`,
  `IORING_OP_SEND`, and `IORING_OP_READ` at offset -1 for the stream, all on
  the ring, so a wait is a real ring wait for the first time; `listen` and
  `bind` are immediate host calls the adapter runs.

  **Landed for POSIX, 2026-09-05 (slice 2).** The six kinds are
  `WF_FILE_SOCKET_LISTEN`, `_ACCEPT`, `_CONNECT`, `_RECEIVE`, `_SEND` and
  `_SHUTDOWN` in `compiler/src/backend/completion/contract.h`, submitted
  through the six `wf__completion_socket_*_submit` entries and the accept's own
  join in `bridge.c` and `bridge.h`. `linux_io_uring.c` carries accept,
  connect, receive and send on the ring, found by the record's own address
  exactly as the reads are; a connect's socket and its native address record
  are made in that adapter's submit, because neither can wait and a second ring
  round trip would cost more than the operation it wraps. `file_posix.c`
  executes every kind against the host — `socket`, `bind`, `listen`, `accept4`,
  `connect`, `recv`, `send`, `shutdown`, `close`, IPv4 and IPv6,
  `SOCK_CLOEXEC`, and no `SO_REUSEADDR`, because [SYS-17] already fixes what a
  second bind of one port means. The two conversions between the emitted
  `SocketAddress` value and the host's own address record are `static inline`
  in `file_posix.h`, so both POSIX engines state one contract. The pair's
  two-count is `wf_file_connection_release` in `file_adapter.c`: the first
  release of a direction half-closes it, the second releases the target's
  object, and which comes first is the program's own release order.
  `native_adapter_probe.c` runs a loopback connect, accept, send and receive on
  the ring; `harness.c` runs the whole lifecycle and the pair's accounting at
  the bridge's own ABI. The completion queue's size is the caller's, through
  `IORING_SETUP_CQSIZE`, 2048 for the bridge, and a completion the kernel
  holds past it on its overflow list is moved into the queue by the reaper's
  own `io_uring_enter` whenever the kernel raises `IORING_SQ_CQ_OVERFLOW`: a
  program whose every callee is parked on such a completion submits nothing,
  so no submission's enter would ever flush it, and the descriptor's readiness
  alone would wake a sleeper to reap nothing. The control test found exactly
  that at 129 connections against a 128-entry queue, and the probe's overflow
  case pins the flush. On the adapter route the same test shows the helper
  bound as a window: `wf__completion_window` caps a staged loop at
  `WF_BRIDGE_MAX_HELPERS` when no ring is the engine, so a server on that route
  keeps eight connections open at once and issues its ninth accept only when
  one of them ends, which a peer that closes everything at the end never lets
  happen. That is the ring-less host's limit until the readiness-driven
  adapter below exists.
- Windows: `ConnectEx`, `WSARecv`, `WSASend` on the completion port; the
  standard input handle through the adapter (a console handle has no
  overlapped form).

  **Landed for Windows, 2026-09-06 (slice 3).** `file_windows.c` executes all
  six kinds against Winsock — `WSASocketW` with `WSA_FLAG_OVERLAPPED` and
  `WSA_FLAG_NO_HANDLE_INHERIT`, `bind`, `listen(SOMAXCONN)`, `accept`,
  `connect`, `recv`, `send`, `shutdown`, `closesocket`, IPv4 and IPv6 — and
  `windows_iocp.c` carries the connect, the receive and the send on the
  completion port with `ConnectEx`, `WSARecv` and `WSASend`, each on the
  record's own `OVERLAPPED`, the connect's socket created and bound to its
  family's wildcard address in the submitting call exactly as the ring's
  connect creates its socket at submit. `windows_runtime.c` holds the socket
  half of the Windows leaf: `WSAStartup` behind an `INIT_ONCE`, the socket
  open and close, the new `WF_WINDOWS_DESCRIPTOR_CLASS_SOCKET` row of the
  descriptor ledger, and the one normalization that makes a `WSAGetLastError`
  code and the port's own Win32 code for one condition answer one [SYS-7]
  class. The address vocabulary is shared, not twinned: the conversions,
  the peer publish, the backlog and the send flags moved out of `file_posix.h`
  into `socket_address.h`, which every engine on every platform includes.

  `AcceptEx` is **not** used, and the reason is a measurement: its address
  pair is `2 * (sizeof(sockaddr_in6) + 16)` = 88 bytes of caller storage that
  must live until the operation completes, the completion record is exactly
  160 bytes on this platform with the accept's union arm at the 40-byte
  ceiling `contract.h` asserts and the ring-state block at 40 bytes fully
  occupied by the `OVERLAPPED` and its handle, and the record may not grow.
  So the accept is the shared file adapter's blocking `accept` on a helper
  thread, which is the same class of fact as the Linux ring's refusal of a
  listen: it selects an engine, not a qualification.
- Darwin: the shared file adapter's helpers make the blocking calls until a
  kqueue route exists; this is the route `WF_IO_NO_NATIVE_RING` runs on Linux
  and is not a second lowering.

  **Peer-bound requests are a helper's, 2026-09-06.** On that route a socket
  wait is a blocking host call on some thread, so *which* thread makes it is
  the whole of the route's concurrency. A request is peer-bound when its kind
  is accept, receive, connect or send: each waits on another program, so no
  measurement of this adapter's own host calls sees the wait coming and nothing
  this runtime does shortens it. Listen and shutdown act on this program's own
  socket and are not. `wf_file_request_is_peer_bound` in `file_adapter.c` is
  that switch, and three rules read it.

  Growth: a peer-bound submission starts a helper whenever `helper_count <
  helper_cap`, taking neither of the two terms an ordinary submission takes.
  The measured verdict and the queue depth are both about a wait that has
  already happened; the kind is about the one that is about to. The depth term
  in particular would defeat the rule outright, because the second concurrent
  accept finds one helper held and one entry queued and would be refused, on a
  pool whose one helper is inside the first accept.

  Progress: while `helper_cap` is above zero, a scheduler thread's pass
  (`wf_file_adapter_progress`) takes the first queued request that is *not*
  peer-bound and leaves the rest, oldest first. With `helper_cap` at zero —
  `WF_IO_HELPERS=0`, an explicit policy that makes the waiting thread the
  queue's own engine — it takes anything, because nothing else could run it.
  For the same reason the joining thread's claim of its own record
  (`wf_bridge_run_own`) is unchanged for a thread waiting in place, which has
  no stack to park and nothing else to run, and is withheld for a peer-bound
  record on a pool stack once a helper exists: there the join parks the stack
  and the worker goes on to other work, so a host call made there holds a
  worker and every continuation it was carrying. That was the defect — three
  workers of `tests/programs/tcp_fanout.wf` each inside a receive from a silent
  peer, and no thread left to accept the fourth connection, which on a
  three-core runner failed about half of all runs.

  Measurement: a peer-bound execution is not sampled into `mean_execute_ns`. A
  receive that waits seconds for a quiet peer measures the peer, not the host,
  and letting one in would make the verdict LONG for every file operation after
  it — and that verdict decides whether an ordinary read is queued or executed
  where it was stated.

  The bound this leaves is explicit and is not worked around: adapter-route
  socket concurrency is `WF_BRIDGE_MAX_HELPERS`, and a program needing more
  peer waits in flight than that waits. (Once contexts run, the pool now grows
  past it while every helper waits on a peer: `CONCURRENCY-MODEL.md`, section
  10.6.) There is no fallback that puts a
  peer-bound request back on a scheduler thread when the pool is at its cap,
  because that is the defect returning at exactly the moment the program is
  widest. **Open design item:** the bound is a property of an engine built out
  of blocking calls and threads to make them on. The ring-less host's proper
  engine is a readiness-driven adapter — one `poll`, `kqueue` or `WSAPoll` over
  the descriptors of every queued request, made inside the park a thread with
  nothing to run already enters — which waits for any number of peers on one
  thread with no host call per peer. That is the next step for Darwin and for
  `WF_IO_NO_NATIVE_RING`, and it is not the kind rule above.

Each new kind is one more case in the two host leaves (`file_posix.c`,
`file_windows.c`) and the two rings; nothing in the core or the bridge's
routing changes shape. The two halves of a connection share one descriptor
in the runtime with a two-count the releases decrement.

## 6. The control test, and the bar

Loopback is a controlled peer. The test harness (Rust, in `tests/programs`)
plays the other side with `std::net` and can add any delay it wants, so a
Whitefoot server is measured against a known client and a Whitefoot client
against a known server.

The owner's bar: the reference is the fastest existing solution, any
language, any library, because the target is first place. On a Linux
loopback that is a hand-written C server on io_uring using what the kernel
offers for exactly this shape: multishot accept, multishot receive into a
provided buffer ring, `SQPOLL`, one ring per core with `SO_REUSEPORT`, no
copies the protocol does not need. That is the shape the fastest published
io_uring servers take, and it is written into
`research/experiments/io-completion-bench/` beside the existing C read
baselines so the numbers are produced by one runner on one host. An epoll
server of the same design is the second reference, since it is what most
deployed servers still are. Kernel-bypass stacks are not a reference on a
loopback: they replace the kernel's TCP and measure something else. The
measures: connections accepted per second, request-response round trips per
second at 1, 64, 1024 and 8192 connections in flight, bytes per second at a
large payload, and the latency distribution of one request under load;
Whitefoot's number is reported as a ratio to the io_uring reference, and the
gap is the batch's result, not something to hide.

The program shapes the checker admits today bound the connections in
flight: a fixed-trip loop over accepts staged under [PAR-3], or an overlap
group of accepts. Widening those shapes to a real server loop is the
language work this batch exposes, and it is the point of doing the network
now rather than later. Slice 2 found where the widening starts: [PAR-3]
stages `tcp_fanout.wf`'s accept loop as written, but the lowering handed out
only a system operation at the staged point, so a may-suspend user call there
ran on the loop's own stack and the peers were served in turn.

That hand-out form landed on 2026-09-06 and is no longer the missing piece.
A staged step whose call is a may-suspend user call is offered a lane frame at
the staged point and retired by `wf__par_join` in the exact drain, so the
callee runs on a pool stack and parks on its own I/O without holding the loop;
the frame's address is what the pipeline slot holds for the iteration, exactly
as a record's address is what it holds for a system operation. `tcp_fanout.wf`
now keeps four accepts in flight in a `--par` build, and
`four_peers_are_served_at_once_under_par_on_both_routes` is the case that says
so: four peers connect before any of them speaks and the last to connect is
answered first, on both routes, which a server that takes the connections one
at a time cannot do. The bounds above it are the ones this section named — the
runtime's window through `wf__completion_window` under a compiler ceiling of
`WF_SCHED_LANE_SLOTS`, and the stack pool, since a parked callee holds a pool
stack — and a loop stopped by data a remainder produced is still the one shape
the rule itself does not stage. For the control test those bounds are set to
its own numbers: the lane holds 1024 slots and the bridge's default window is
1024, so a fixed-trip accept loop naming 1024 connections keeps every one of
them in flight, and `WF_STACKS` may name up to 2048 stacks, which the
protocol sets to 1100 so 1024 parked callees have a stack each beside the
entry thread's and the workers'. The server the protocol measures is
`research/experiments/io-completion-bench/programs/tcp_echo_server.wf`: the
connection count arrives as an argument and bounds the staged `for`, which
[PAR-3] stages as it does the literal, and each callee's own receive-and-send
loop is that connection's sequential loop. 8192 connections in flight is
outside these shapes, by the stack pool before anything else, and the table
says so. What the control test needs beyond that is the language work rather
than the lowering: a server loop whose trip count is not fixed.

## 7. Slices

1. Amendment v0.46: the two renames, `command.stdin` and `read_next`, the
   types and operations of §4, conformance cases, corpus programs. Landed
   2026-09-05. `read_next` is end to end on POSIX and Windows; the TCP
   operations are declared, checked, lowered and emitted, and a submission is
   refused at target qualification until slice 2 supplies its routes.
2. POSIX runtime: adapter route for every kind, Linux ring route for accept,
   connect, receive, send and stream read; loopback tests in `tests/programs`.
   Landed 2026-09-05; §5 names the routes and the units that carry them.
3. Windows: the completion-port route; the io-hosts job proves it. Landed
   2026-09-06; §5's Windows entry names the units and states why the accept
   is the adapter's.
4. The control benchmark against the io_uring and epoll references, in
   io-completion-bench.
5. Batch record.

## 8. Decisions for the owner

Settled on 2026-09-05: the network first, with standard input as the first
stream; `InputStream` and `OutputStream`; TCP only, address literals only,
no UDP and no name resolution; the backlog is target-fixed; full duplex is
designed in from the start as two halves (§3); the reference is the fastest
existing solution regardless of language.

Settled in the second round, 2026-09-05: `HandleFactory`, `HandlePermit`,
`command.handles`, `reserve_handle`, renaming the v0.45 surface across the
conformance corpus and the programs in the same amendment; the connection
is the two-field struct of §4, so the pairing is on the API and no split or
join exists; the operation names of §4; the reference of §6. Nothing is
open; slice 1 starts.
