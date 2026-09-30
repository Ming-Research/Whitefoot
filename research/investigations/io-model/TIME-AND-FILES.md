<!-- Serves the clock, timed waits and file writes that a Redis subset with
     key expiry and an append-only file needs. Its surviving decisions go to
     the design tree and the specification; this record stays as their
     grounds and holds Experiment 8. -->

# Time, deadlines and file writes

## The question

The Redis subset of Experiment 7 (`SHARED.md`) serves `SET`, `GET`, `DEL`
and `INCR` over one shared keyspace. Redis's next two features, key expiry
and the append-only file, need three things no program can write today:

- **a clock**, to stamp when a key expires and to ask whether it has;
- **a timed wait**, to wake a context that expires keys ten times a second
  and one that hands the file to the disk once a second, and to give up on a
  peer that has sent nothing for too long; and
- **a file write**, to append each change to a file and to ask the host to
  keep it.

The host modules [PRE-2] read files and directories, and read and write
standard streams and TCP connections; nothing in them observes time, bounds
a wait or writes a file.

## The rulings this design starts from

The owner ruled four questions on 2026-09-30, in Chinese: Q26 directly,
and Q23 to Q25 by approving the recommendations below ("all agreed"):

- **Q23 B.** The clock reaches a program as a field of `Inputs`, as every
  other host capability the entry receives does, and not through the handle
  factory or ambient access.
- **Q24 A.** An operation that waits on a peer takes an optional deadline,
  `deadline: Option<Instant>`, whose `Instant` is opaque; a separate
  `sleep_until` waits for an instant alone; file operations take no deadline.
  The owner refused a second, deadline-taking copy of each operation ("B
  doubling the I/O is plainly wrong. What about Option?").
- **Q25 D.** `Inputs.cwd` becomes a directory the program may both read and
  write, which it may weaken to read-only; every operation that writes needs
  write authority.
- **Q26 A.** A sync promises only that the bytes written before it have been
  handed to the host's durability mechanism; the specification says nothing
  about what survives a crash.

Four smaller choices inside those rulings followed, each a card in
[the choices the owner ruled](#choices-the-owner-ruled); the owner approved
all four as recommended on 2026-09-30, and the surface below follows them.

## Surface

### `std::time`

```
public opaque nocopy struct Clock {
}

public opaque nocopy struct WallClock {
}

public opaque struct Instant {
  ticks: u64;
}

public fn clock_share(clock: &Clock) -> result: Clock reads(clock) doc "Returns a clock that reads the same monotonic time as clock.";

public fn wall_clock_share(clock: &WallClock) -> result: WallClock reads(clock) doc "Returns a wall clock that reads the same calendar time as clock.";

public fn now(clock: &Clock) -> result: Instant writes(clock) doc "Returns the current instant of the monotonic clock, which is not before any instant an earlier read through clock returned.";

public fn instant_after(instant: Instant, nanoseconds: u64) -> result: Instant pure doc "Returns the instant nanoseconds after instant, or the latest instant when that lies beyond it.";

public fn nanoseconds_from(earlier: Instant, later: Instant) -> result: u64 pure doc "Returns the nanoseconds from earlier to later, or zero when later is not after earlier.";

public fn instant_reached(deadline: Instant, instant: Instant) -> result: Bool pure doc "Returns whether instant is at or after deadline.";

public fn sleep_until(deadline: Instant) -> result: unit waits doc "Completes once the monotonic clock has reached deadline.";

public fn unix_nanoseconds(clock: &WallClock) -> result: i64 reads(clock) doc "Returns the calendar time as nanoseconds since 1970-01-01T00:00:00Z, which the host may move in either direction between reads.";
```

**Why each operation has the row it has.**

- `now` writes its clock. Two host operations of one context are ordered
  only when their footprints overlap with a write [HOST-1], and two adjacent
  statements that only read one place may overlap [PAR-1]. With `reads`, two
  successive `now` calls through one clock would have no order, and the
  second could return the earlier instant. The write states the one thing a
  monotonic clock promises its reader: each read is not before the last one
  through the same clock. The clock's state is the latest instant it has
  reported, and a read advances it. Reads through two clocks that
  `clock_share` relates are not ordered by this. A program that needs such an
  order passes both reads through one clock, which is exactly what HOST-1
  asks of every other host state.
- `unix_nanoseconds` reads its clock. The calendar time promises no order
  between reads, since the host may set it back, so a write would order
  nothing the reader can rely on.
- The three `Instant` functions are total and `pure`. The checker has no
  fact source for the clock's monotonicity, so an operation with a domain
  (a subtraction that requires `earlier <= later`) would leave the caller an
  obligation no proof can discharge. `instant_after` saturates at the latest
  instant, which lies more than 580 years after the clock's origin, and
  `nanoseconds_from` answers zero for a pair out of order. Both are named for
  what they return in every case.
- `sleep_until` takes no clock. An `Instant` is formed only by `now` and
  `instant_after` from one that `now` formed, so a program that holds one has
  already read a clock; the wait observes nothing a clock read has not. The
  deadline parameters below take none for the same reason. A context runs its
  statements one at a time and never overlaps a waiting statement with any
  other [WAIT-2, PAR-1], so a `now` executed after `sleep_until(d)` returns
  an instant at or after `d`.
- `sleep_until` returns `unit`. A timer the host cannot arm is a host
  resource exhausted, which may stop execution [SCOPE-3]; a sleep has no
  source-visible failure.

**Why `Instant` has a field.** `Instant` must be copyable, since a program
computes a deadline once and passes it to many calls, and unforgeable, since
an integer the program can write would let `sleep_until` wait for any time at
all without a clock. An opaque struct is unforgeable [TYPE-2], and a struct
is copy when its fields are [PROV-6]. A host handle, by the declaration-home
decision, is a fieldless opaque struct declared `nocopy` or `nodrop`, so an
`Instant` cannot be one. Its one private field states its representation and
gives it the copy capability through the ordinary structural rule: a
monotonic count of nanoseconds from an origin the host chooses. No module
reads the field, since `std::time` has no implementation record; the host
functions form and read it. The one specification change this needs is that
a host function forms every opaque struct a host module declares, not only
its handles [TYPE-2].

### Deadlines on the operations that wait on a peer

Each operation that may wait on another party gains a last parameter,
`deadline: Option<Instant>`:

| Module | Operation | Waits on |
|---|---|---|
| `std::io` | `read_next` | the writer of an input stream |
| `std::io` | `write_once` | the reader of an output stream |
| `std::net` | `tcp_accept` | a connecting client |
| `std::net` | `tcp_connect` | the listening server |
| `std::net` | `receive_next` | the peer's sending |
| `std::net` | `send_once` | the peer's receiving |

`IoError` gains one variant, `DeadlinePassed()`, which is the outcome of an
operation whose deadline the monotonic clock reached before the operation
produced any other outcome. The meaning, stated once for all six:

- `None` leaves the operation as it is today.
- With `Some(d)`, the operation produces `DeadlinePassed` only once the
  monotonic clock has reached `d`, and then it has transferred nothing: no
  byte was read, written, received or sent, and no connection was accepted
  or opened.
- An operation whose own outcome the host produces while the deadline is
  being reached reports that outcome, bytes or connection included; the
  deadline never discards a completed transfer.
- A deadline already reached when the call begins produces `DeadlinePassed`
  unless the operation completes without waiting, as a receive whose bytes
  have already arrived does. The specification promises only the first two
  points; which of the two outcomes a race produces is an input of the
  execution [WAIT-2], like which of two operations completes first.

WAIT-2's progress clause needs no change: an outcome produced at a deadline
is a host outcome that has been produced, and the context waiting for it
takes its next step.

File operations take no deadline. They wait on the host's storage, not on a
party that may never answer (Q24 A).

**That a deadline outcome is an `IoError` variant** is card 3; the
refused alternative is `TimedOut` with a reserved origin.

### Writable directories and append-only files

`std::fs` gains:

```
public opaque nodrop struct DirectoryWrite {
}

public struct Directory {
  public read: DirectoryRead;
  public write: DirectoryWrite;
}

public opaque nodrop struct WriteFile {
}

public fn open_append(factory: &HandleFactory, root: &DirectoryWrite, name: &[u8], start: u64, end: u64) -> result: Result<WriteFile, IoError> reads(root), reads(name), writes(factory) waits contract {
  requires start <= end;
  requires end <= name^.len;
} doc "Opens the file that the bytes of name from start to end name below root for appending, creating it empty when no entry has that name.";

public fn append_once(factory: &HandleFactory, file: &WriteFile, source: &[u8], start: u64, end: u64) -> result: Result<u64, IoError> reads(source), writes(factory), writes(file) waits contract {
  requires start <= end;
  requires end <= source^.len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Appends bytes of source from start toward end to the end of file with one host write; Ok carries the index after the last byte appended.";

public fn sync_file(factory: &HandleFactory, file: &WriteFile) -> result: Result<unit, IoError> writes(factory), writes(file) waits doc "Hands every byte appended to file before this call to the host's durability mechanism; Ok reports that the host accepted them.";

public fn close_write(factory: &HandleFactory, file: WriteFile) -> result: Result<unit, IoError> writes(factory) waits doc "Closes file.";

public fn close_directory_write(factory: &HandleFactory, directory: DirectoryWrite) -> result: Result<unit, IoError> writes(factory) waits doc "Closes the write authority over a directory.";
```

- **Two halves, one struct.** `Directory` is an ordinary struct of two
  separately closable owners, as `TcpConnection` is, for the reason the
  system-interface decision gives for it: well-typed code may pair halves
  from different directories, and closing stays correct for any pair because
  each half closes alone. The read operations keep taking `&DirectoryRead`
  and so take `&cwd.read`; the write operations take `&DirectoryWrite`. To
  weaken the directory to read-only, a program closes `cwd.write` and passes
  `cwd.read` on; a function that receives only a `DirectoryRead` cannot
  write below it. Each half is one host handle and one credit of the
  factory's budget.
- **Appending only.** `open_append` opens for appending and creates a
  missing file; `append_once` is one host write at the end of the file, a
  single attempt as every transfer is (the single-attempt decision), so a
  caller loops for a whole buffer as `send_all` does. Two contexts that each
  open the same file append whole host writes in an order that is an input
  of the execution.
- **`sync_file`** carries the Q26 A promise and nothing more. It orders
  after every append through the same file, since both write it [HOST-1].
- **Not in this version**: writing at an offset, truncating, renaming,
  removing, creating a directory, a create rule other than create-if-missing,
  descending into a subdirectory for writing, and syncing a directory. The
  append-only file needs none of them until it is rewritten, which is Redis's
  `BGREWRITEAOF`: a new file, synced, renamed over the old, and the directory
  synced. Each is recorded in `docs/todo.md` with that reopening condition.

### `Inputs`

```
public struct Inputs {
  public args: Args;
  public cwd: Directory;
  public stdout: OutputStream;
  public stderr: OutputStream;
  public handles: HandleFactory;
  public stdin: InputStream;
  public clock: Clock;
  public wall_clock: WallClock;
}
```

`cwd` holds both halves of the working directory (card 4); `clock` and
`wall_clock` are the two clocks of card 2. The runtime supplies the write half on every
host; a host that refuses the working directory to writing still hands a
write half, and every write through it reports that host's refusal.

### Modules

`std::time` is the sixth host module. Its graph row has no dependency, and
the modules that name `Instant` gain one:

```
pkg::io: [pkg::time];
pkg::text: [];
pkg::time: [];
pkg::fs: [pkg::io, pkg::text];
pkg::net: [pkg::io, pkg::time];
pkg::process: [pkg::io, pkg::text, pkg::fs, pkg::time];
```

The graph stays acyclic: `time` depends on nothing, `io` on `time`, and every
other row on modules above it.

## Choices the owner ruled

The owner approved each card below as recommended on 2026-09-30 ("Q27
agreed. Q28 agreed, Q29 agreed, Q30 agreed", written in Chinese).

**Card 1 (Q27): how is `Instant` declared?** Recommended: an opaque struct
with one private `u64` field, as above; copy through its field, formed only by
host functions. The alternatives are a fieldless `opaque` struct left copy,
which contradicts the declaration-home decision that every host handle is
`nocopy` or `nodrop`; a `nocopy` handle, which makes every deadline a move and
leaves a program copying one through a host function; and a public `u64`,
which is forgeable and turns `sleep_until` and every deadline into ambient
access to time. Confidence 4/5.

**Card 2 (Q28): one clock or two?** Recommended: two capabilities, `Clock`
for monotonic time and `WallClock` for calendar time, as two `Inputs` fields.
Monotonic time measures intervals and bounds waits; calendar time names a
moment another process or a later run of this one can read. They differ in
what they promise (the calendar may move back) and in what a function that
uses one depends on: a function given only a `Clock` provably does not depend
on a setting an administrator can change. The capability dossier reached the
same separation ("wall and monotonic time remain distinct",
`system-capability-architecture/DOSSIER.md` §8). The Redis subset needs
both, and each of its functions needs only one: expiry runs on the monotonic
clock, and the append-only file records absolute calendar times so that a
later run can replay them. The alternative, one `Clock` with both reads, is
one field fewer and loses that separation. Confidence 3/5: the benefit is
real but small, and no current program would be wrong under one clock.

**Card 3 (Q29): how is a passed deadline reported?** Recommended: a new
variant `IoError::DeadlinePassed()`. A passed deadline is the program's own
bound, not a host failure; `TimedOut` already reports a host's timeout, such as
a connection attempt the peer never answered, and a program may well treat the
two differently (retry the host's, give up on its own). The alternative
reuses `TimedOut` with an origin value reserved for deadlines, which adds no
variant but makes the distinction a number to compare. The outcome-typing
decision prefers operation-specific outcomes; a variant an operation without
a deadline never produces is the cost, shared with every `IoError` variant a
given operation never produces. Confidence 3/5.

**Card 4 (Q30): what shape has the writable directory?** Recommended: the
struct of two halves above. The alternative is one `Directory` handle with both
authorities and a consuming `directory_read_only` that weakens it; the read
operations would then need a second form for a `Directory`, or every program
would weaken before reading, and the directory would need a two-count close
hidden behind one handle. Confidence 4/5.

## Departures from the capability dossier

`system-capability-architecture/DOSSIER.md` is a draft that no ruling adopted.
It framed a timeout as a race between a timer and a pending operation whose
winning branch asks for cancellation (§7.4), and a clock read as a shared
observation that advances no cursor (§6.2). This design departs from both,
for these reasons:

- The language has no construct that races two waits, and adding one needs a
  cancellation semantics of its own. A deadline parameter is the one bounded
  cancellation expressible without it, and the owner chose it (Q24). A race
  construct, if one comes, subsumes the parameter without contradicting it.
- The dossier predates HOST-1 and PAR-1's read overlap. Under them a read
  that writes nothing is unordered with the next read, so a monotonic clock
  whose reads promise an order must state that order as a write.

## Runtime plan

### One timer queue per driver

A driver already parks its contexts on its own ring, and only its own thread
touches its parked contexts (`bridge.c`, `wf_driver`). Each driver gains a
binary heap of the deadlines of the contexts parked on it, keyed by the
monotonic instant. Parking with a deadline inserts; completion before the
deadline removes. Every place the driver waits for the host, whether it parks
on the ring (`wf_linux_io_uring_park`), polls for readiness
(`wf_context_poll`) or sleeps on its wake (`wf_completion_park_if_unchanged`),
bounds that wait by the earliest deadline instead of waiting without limit.
A driver that is running contexts looks at the heap's head when it reaps
host completions, which it does every few resumes.

When the head's instant has passed, the driver ends that context's wait:

- A `sleep_until` context has no host operation; the driver completes its
  record and makes it ready.
- A context with an operation asks the operation's route to cancel it, and
  the operation completes through its ordinary completion path with either
  its own outcome or `DeadlinePassed`.

The contexts counted as waiting on the host (`host_waits`) include those
waiting on a deadline, so a program whose only pending wait is a sleep is not
taken for one that can take no step. `wait_host.c` builds its own wait's end
from `CLOCK_REALTIME`, which the calendar can move; a bounded park builds it
from the monotonic clock.

### Cancelling on each route

| Route | Operations | How a deadline ends it | Why nothing is lost |
|---|---|---|---|
| Linux ring | read, accept, connect, receive, send | `IORING_OP_ASYNC_CANCEL` for the record's key | a cancelled request completes with `-ECANCELED` and transfers nothing; one that finished first completes with its result |
| readiness, no ring | receive, send, accept | the driver stops polling the descriptor and completes the record | the transfer is a nonblocking call made only after readiness; a record completed by the deadline made none |
| helper thread, POSIX | stream read and write, connect without a ring | a signal, installed without restart, sent to the helper until it acknowledges | an interrupted blocking call returns `EINTR` before transferring; one that transferred returns its count |
| helper thread, Windows | accept, stream read and write | `CancelSynchronousIo` on the helper, repeated until it acknowledges | an aborted call reports `ERROR_OPERATION_ABORTED` with no transfer |
| IOCP, Windows | connect, receive, send | `CancelIoEx` for the record's overlapped | as the ring |

A readiness wait alone is not enough for a stream: standard input may be
shared with other processes, so it can be readable at the poll and empty at
the read, which then blocks past the deadline. The helper's interruption
covers that case.

### File writes on each host

`open_append` opens with `O_WRONLY | O_APPEND | O_CREAT | O_CLOEXEC` below the
write half's descriptor (`openat`), and on Windows with `FILE_APPEND_DATA` and
`OPEN_ALWAYS`. `append_once` is one `write`, and `WriteFile` on Windows. The
write half of the working directory is its own descriptor, opened as the read
half is. `sync_file` is `fdatasync` on Linux, `fcntl(F_FULLFSYNC)` on macOS,
whose `fsync` hands bytes only to the drive's cache, and `FlushFileBuffers`
on Windows; each is the host's own interface for handing written bytes to
its durability mechanism, which is all Q26 A promises. The regular file
never waits on a peer, so these run on the helpers, as file reads do.

## What is left out, and when it returns

- **Bounding an atomic statement's guard by a deadline.** A context waiting
  for a guard [SHARE-3] cannot also wake at a time. The append-only file's
  writer below polls on a short period instead of waiting for work. Reopen
  when a program must wake on the earlier of work arriving and a time
  passing without polling.
- **The file operations listed above**, with the rewrite of the append-only
  file as their reopening condition.
- **A clock whose reads can be replaced for testing.** No current test needs
  one.

## Experiment 8: expiry and persistence in the Redis subset

### Design

The subset gains the commands that set and read a key's expiry, `EXPIRE`,
`PEXPIRE`, `TTL`, `PTTL` and `PERSIST`, and the `EX` and `PX` options of
`SET`. As Redis does, it expires a key in two ways: a command that finds an
expired key treats it as absent and removes it, and a context of its own
wakes ten times a second and removes expired keys it samples. The expiry is
a monotonic `Instant`; each read of the clock is one `now`.

With a third argument naming a file, the subset appends every command that
changes the keyspace to that file in Redis's own format, as `appendonly yes`
with `appendfsync everysec` does:

- The statement that changes a key also appends the command's bytes to a
  buffer in the same shared object, so the file holds the changes in the
  order the keyspace took them.
- A writer context moves the buffer out every 10 milliseconds, appends it to
  the file and syncs the file when a second has passed since the last sync.
- An expiry is written as `PEXPIREAT` with an absolute calendar time in
  milliseconds, as Redis 7 writes it, and replay converts it back into a
  monotonic `Instant` from the two clocks' current readings.
- At start, the subset replays the file before it listens.

A reply may leave before its change reaches the file. `everysec` already
promises no more than a second of changes, so that fits the promise it
imitates; Redis writes the buffer before it replies, and the measurement
below states the difference.

A fourth argument, an idle limit in seconds, closes a connection that sends
nothing for that long, as Redis's `timeout` does; it is the program's use of
a deadline on `receive_next`.

### What would distinguish the hypotheses, stated before measuring

The reference is `redis-server` 7.0.15 on the host and with the pinning of
Experiment 7, in two configurations: persistence off, as there, and
`--appendonly yes --appendfsync everysec` with the file on the same
file system as the subset's. `redis-bench.sh` gains the persistent lines.

- **Correct.** Every run completes with no error reply, the shared-counter
  check of Experiment 7 holds, and a check sequence answers as Redis does:
  a key set with `PX 100` answers a positive `PTTL`, then reads as absent
  after 200 milliseconds; `PERSIST` removes an expiry; `TTL` answers -1 for a
  key without one and -2 for a missing key. After the subset is stopped and
  restarted on its file, every key that was set and not expired holds its
  value, and every key whose expiry passed while it was stopped is absent.
  An idle connection with a one-second limit is closed within two seconds.
- **Expiry costs little.** With persistence off, the subset's `SET` and `GET`
  rates without pipelining are at least 0.9 times Experiment 7's second run
  on this host, measured in the same session with the build before this
  change. A command now reads the clock and checks an expiry.
- **Persistence keeps up.** With `everysec` on both, the subset on two
  drivers reaches at least the reference's rate for `SET` without
  pipelining, and at least half its rate with 16 per pipeline, the same two
  bars Experiment 7 set without persistence.
- **Active expiry removes keys.** After 100,000 keys set with `PX 1000` and
  no further reads, `DBSIZE` falls below 1 percent of them within ten seconds
  on both servers. The subset gains `DBSIZE` for this check.

A criterion that fails is attributed with a profile before any conclusion is
drawn from it.
