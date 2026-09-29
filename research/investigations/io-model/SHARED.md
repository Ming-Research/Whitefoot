<!-- Serves the shared-state form that a multi-client server such as a Redis
     subset needs (WAITS.md, "Sharing between concurrent activities", which
     deferred it until a program needs one). Its surviving decisions go to the
     design tree and the specification; this record stays as their grounds. -->

# Shared objects: one piece of state, many contexts

## The question

A server whose connections read and change one piece of state, the Redis
shape, cannot be written today. A context [WAIT-2] takes only value
parameters, so two connection contexts cannot reach one map. A single context
that owns the map can wait on only one connection at a time. The waiting
design deferred the form this needs until a program needs one, and it named
two candidates: an explicit shared object whose operations are whole atomic
transactions, and a waiting operation over several connections that lets one
owner serve them all (`WAITS.md`, "Sharing between concurrent activities" and
"What the first version keeps open").

On 2026-09-28 the owner chose to design the first ("I want to design B and see
how elegant, or at least how not ugly, it can be", written in Chinese). The
first draft of this record used a transaction function with a callback. The
owner judged that API not general enough, and on the same day accepted the
statement form below ("this is much cleaner, go with it", written in Chinese).

## The idea

**A shared object is interior mutability under a checked, lexically scoped
lock: state that several contexts hold is changed only inside an `atomic`
statement, and an `atomic` statement is a waiting construct whose block
cannot wait.**

The language already carries most of the meaning:

- The program means its sequential execution, and which outstanding operation
  completes first is an input of the execution [WAIT-2]. The order in which
  other contexts' atomic statements reach the object is one more such input.
  An atomic statement observes some prefix of that order, as a receive
  observes whatever bytes the peer has sent.
- Only a waiting function may contain a waiting construct [WAIT-1]. An
  `atomic` statement is one, and its block may contain none. Nested atomic
  statements, waiting for the host while the object is held, and lock-order
  deadlock therefore have no representation.
- A waiting construct never overlaps another statement of its context
  [PAR-1]. One context's atomic statements therefore take effect in its
  source order.
- The binding is a reference variable whose root ends with the block, so
  every reference derived from it is invalid after the block [REF-2]. No
  reference into the shared state, and no proof fact about it, leaves the
  block. What a context learns is the owned data it copies out.

## Surface

```wf
opaque nocopy struct Shared<T: drop> {}

fn shared_new<T: drop>(value: T) -> result: Shared<T> pure;
fn shared_share<T: drop>(shared: &Shared<T>) -> result: Shared<T> reads(shared);
```

```text
atomic_stmt := "atomic" IDENT "=" "&" place ("when" expr)? block
```

- `shared_new` moves a value into a new object. `shared_share` returns another
  handle to the same object, to move into a context. A handle is `nocopy`, and
  the object, with its `T`, is released when its last handle is released
  [OWN-1]. `T` must have drop in this version.
- `atomic s = &h { ... }` names the object's state `s`, a reference variable of
  kind `&T`, for the block. `h` is a place holding a handle, for example a
  local or `param^` through a reference to one.
- The block is ordinary code. It may call any function that does not wait,
  read and write the enclosing function's locals directly, and leave by
  `return`, `break` or error propagation; every exit releases the object.
- `when guard` makes the statement wait until the guard holds. The guard is a
  `Bool` expression that may read `s` and locals and writes nothing. This is
  a conditional critical region, as in Hoare's and Brinch Hansen's work or
  Ada's protected entries. Inside the block the guard is a proved fact, as an
  `if` condition is in its true branch, so `when items^.len > 0_u64` is what
  lets the block call `take_front`.
- Nothing marks whether a statement only reads. This version acquires every
  statement exclusively; running statements that only read at the same time
  is a liberty the meaning leaves to the runtime (below).

### What the writer sees: a Redis subset

The sketch as first proposed; `tests/programs/redis_subset.wf` is the checked program Experiment 7 wrote from it. `Bytes` stands for the program's own
byte-string type, and `...` for ordinary parsing and encoding code.

```wf
struct Store {
  keys: HashMap<Bytes, Bytes, 16777216>;
}

fn execute(store: &Store, command: &Command, reply: &Bytes) -> result: unit reads(command), writes(store), writes(reply) {
  doc "Runs one command against the store and encodes its reply.";
  ...
}

fn serve(connection: TcpConnection, factory: HandleFactory, keyspace: Shared<Store>) -> result: unit pure waits {
  let reply = bytes_new();
  loop @commands {
    let command = ...;              // read and parse one RESP command
    atomic store = &keyspace {
      execute(store: store, command: &command, reply: &reply);
    }
    ...                             // send the reply and clear it
  }
  ...
}

fn main(inputs: Inputs) -> status: ExitStatus pure waits {
  ...
  let keyspace = shared_new::<Store>(value: store_new());
  loop @accepting {
    match tcp_accept(factory: &handles, listener: &listener) {
      Ok(value: next) => {
        let AcceptedConnection(connection: connection, peer: unused_peer) = move next;
        let factory = factory_share(factory: &handles);
        let handle = shared_share::<Store>(shared: &keyspace);
        mustpar serve(connection: move connection, factory: move factory, keyspace: move handle);
      }
      Err(error: problem) => {
        break @accepting;
      }
    }
  }
  ...
}
```

Small operations need no helper:

```wf
atomic hits = &counter {
  set hits^ = hits^ +wrap 1;
}

atomic queue = &jobs when queue_nonempty(queue: queue) {
  set next = queue_pop(queue: queue);
}
```

`MULTI`/`EXEC` is one atomic statement over several commands. `BLPOP` and a
pub/sub subscriber are atomic statements with a guard: until the list or the
inbox is non-empty.

## Meaning

The rules this adds, as they would read in the specification:

1. **Shared objects.** A `Shared<T>` value is a handle to a shared object
   holding one `T`. The object's state is storage of no binding and belongs
   to no context; it is reached only through the binding of an atomic
   statement. The object is released, and its `T` with it, when its last
   handle is released.
2. **Form.**
   - An atomic statement's place resolves to a live handle.
   - The statement reads that place when it begins. The object stays live
     until the statement completes, whatever the block does with the place:
     the statement holds a handle of its own, so a block may move or replace
     the handle it was reached through.
   - The binding's validity ends with the block [REF-2].
   - An atomic statement is admitted only in the body of a waiting function
     and counts as a waiting call for [WAIT-1], [PAR-1], [PAR-2] and [PAR-4].
   - Its block and guard contain no waiting call and no atomic statement.
   - Its guard writes nothing.
3. **Meaning.**
   - The block executes with exclusive access to the object's state, and the
     statement takes effect entirely at one point after it begins and before
     it completes.
   - With a guard, that point is one at which the guard is true.
   - Atomic statements on one object take effect one at a time. The order in
     which those of different contexts take effect is an input of the
     execution [WAIT-2], and those of one context take effect in its source
     order.
   - A statement whose guard never becomes true does not complete, as a host
     operation that never completes does not.
   - While a statement waits for its guard, the calls around it that could
     run as contexts do, and their starters go on; while every context keeps
     reaching its end or a wait for something not yet there, a statement
     whose guard stays true takes effect (the owner's ruling, below).

Rule 3's single point is linearizability. When client X receives the reply to
`SET` and then tells client Y, and Y sends `GET`, Y's statement begins after
X's completed, so it sees the write. No completion stamps are needed.

**The sharing rule** (`WAITS.md`) gains one clause: an object whose state is
changed only by atomic statements admits every order of them by its
definition, as a peer admits every answer.

**Why the rest of the language is untouched.**
- Inside the block, ownership, windows and proofs work unchanged, because the
  binding is an ordinary reference root.
- Outside the block nothing about the state is known, which is true: another
  context may have changed it.
- Effect rows are unchanged. A function that uses a shared object declares
  `waits`, and like a host resource the object's state is no path of any row.

## Why a statement and not a function

The first draft had `transact<T, E, R, fn body(state: &T, env: &E) -> R>(shared,
env)`, plus `inspect` for reads and `transact_when` for guards. The language
has no closures [FN-5], so every transaction needed a top-level function and
an environment struct holding the locals it used. Read, guarded and
multi-object transactions each needed another function. The owner judged it
not general.

The statement is the lexical scope of a lock guard, checked. It reaches locals
directly, and the guard is one optional clause. Its cost is grammar and one
more block-bearing statement, whose binding the checker treats as it treats a
reference parameter.

Considered and refused:

- **An implicit scope**, where `keyspace^.x` in ordinary code acquires the
  object for the life of the reference. It was refused because the atomicity
  boundary would be invisible: whether two adjacent statements form one
  transaction could not be read from the source, and that is the root of
  read-modify-write races.
- **Asynchronous messages to the object (actors, channels).** A sender that
  does not wait for the effect cannot know the order its messages take effect
  in. The first draft of this record also refused channels by the sharing
  rule, but that reason no longer separates them: what an atomic statement
  reads depends on which other statements took effect first exactly as what
  a receive gets depends on the sends, and this design admits that order as
  an input. A bounded channel whose send waits is a `Shared<Ring<T, n>>` with
  one guarded statement to put and one to take, so it needs no construct of
  its own, while an owner context that serves requests would pass every
  change through one context and two messages. The refusal rests on that
  expressiveness and cost; a channel library over this form is left until a
  program needs one (the owner's ruling of 2026-09-29).
- **Atomic fields and lock-free cells.** They were refused because they expose
  the interleaving of individual reads and writes, which the sequential
  meaning excludes.
- **Ordering transactions by host completion** (the second form in
  `WAITS.md`). It was refused because it adds completion stamps and orders
  nothing a program can observe beyond rule 3's single point.
- **Several objects in one statement** (`atomic a = &h1, b = &h2`), deferred.
  Two handles may name one object, and the checker would take `a` and `b` as
  disjoint roots, so two writable references could reach one state. A form
  whose handles are known distinct, or a runtime rule for the aliasing case
  with a sound static meaning, is needed first.

## What the runtime may do

Whether, when and on which thread a block runs is unobservable beyond rule 3.
The block cannot wait, and the checker knows its footprint. The runtime may
therefore choose freely among these:

- **A lock word and a queue of parked contexts.** An uncontended statement
  costs one atomic acquire and one release. A contended one parks its
  context, and the driver runs others; no host thread sleeps. On one driver a
  statement never finds its object held, because a holder cannot suspend
  inside the block.
- **Reader concurrency.** Statements whose blocks only read run concurrently
  under a reader-writer lock or RCU. That is read parallelism Redis itself
  does not have.
- **Combining or a home driver.** The compiler can lift a block into a
  function whose environment is the locals it uses. The waiting context is
  suspended and its frame quiescent, so the holder, or the driver the object
  lives on, can run queued blocks for their contexts. The state then stays in
  one core's cache.
- **Guards.** A guarded statement that finds its guard false releases the
  object and parks until a statement that writes the object completes, then
  re-checks.

For a Redis subset this is the Redis 6 I/O-thread shape without
configuration. Parsing, encoding and all network I/O run in each connection's
context, spread over the drivers. Only the block is serialized on the object.

## The first version

The owner accepted the statement form on 2026-09-28, and specification v0.80
states it as [SHARE-1] to [SHARE-3], with [SET-1] admitting a write rooted in
an object's state. The implementation:

- **Checker.** The binding is a reference variable anchored at itself, as a
  reference parameter is, so paths through it reach no effect row and no
  caller [EFF-1]. A waiting call or an atomic statement inside the guard or
  block is refused with SHARE-2, and an atomic statement outside a waiting
  function with WAIT-1. The guard's footprint writes nothing when no call in
  it has a row that writes or moves an argument. Permission [PAR-1] refuses
  the statement as a waiting construct.
- **Proofs.** The binding names a state no earlier fact describes. The guard
  enters the block as the true arm of a Bool condition does, and every fact
  that names the binding ends with the block.
- **Lowering.** The statement loads the handle, counts a handle of its own,
  and acquires the object; a guard that reads false watches the object and
  acquires it again. Every edge leaving the block (its end, `return`,
  `break`, `give`, error propagation) unlocks the object, then releases the
  statement's handle, before any join of the activation's contexts. The
  handle's release drops the state and frees the object when it was the last.
- **Runtime** (`completion/bridge.c`). An object is a header (handle count,
  spin lock, holder count, a queue of parked contexts, the contexts watching
  for a write) followed by the state. An uncontended statement takes the
  lock word twice and parks nothing. One that finds the object held spins
  for a bounded time, because a holder's block cannot wait and so its holder
  is running, then parks; an unlock wakes the first parked statement to try
  again, and one that misses again keeps its place at the head. The first
  version handed the object to the parked context at the queue's head
  instead, which Experiment 7 found to make almost every statement park on
  two drivers. After the progress ruling, the unlock after two vain wakes
  hands the object over, so a parked statement gets the object after at most
  two vain wakes (below).
- **A fix along the way.** A context parked where another driver can make it
  ready (a group join, and now an object) could be resumed, finished and
  released by another driver before the driver that ran it read its frame
  again to ask whether it had finished. Such a park now tells the driver not
  to read the context afterwards. The window was narrow for joins; objects
  make it common.

Evidence on this host. `tests/programs/shared_objects.wf` has 16 contexts
each add one 20,000 times to one `Shared<u64>`, and four producers and four
consumers pass 20,000 values through a guarded `Shared<Ring<u64, 8>>`; its
program test runs it three times each on one driver and on four. By hand, and
not kept, the same two workloads ran three times each on 1, 2, 4 and 8 drivers
and always reached their sums, and with the acquire made to succeed without
the lock every 8-driver run of the counter failed. The consumer without its
guard is refused (FN-8, `take_front`'s requirement).

ThreadSanitizer finds no race in the program on 2, 4 and 8 drivers when both
the runtime and the emitted module are instrumented. The emitted module needs
the `sanitize_thread` attribute added to its functions: clang instruments only
functions that carry it, so a module compiled from IR without it checks the
runtime alone. With the lock removed, the instrumented build reports the
write-write race on the counter's state, so the check can fail. The runs were
made by hand: `whitefootc --emit-llvm` for the module, `sanitize_thread` added
to each of its attribute groups, and the module compiled with the runtime's
units (`wf_floor.c`, `ordinary_values`, `sched/`, `completion/`) under
`clang -fsanitize=thread`, first on the handoff lock and again on the lock
that spins and retries.

## Experiment 7: a Redis subset

### Design

`tests/programs/redis_subset.wf` serves `PING`, `SET`, `GET`, `DEL` and
`INCR` in RESP2 over TCP, pipelined requests included. Each connection runs
in its own context, reads into its own buffer, answers every complete command
the buffer holds with one send, and keeps an incomplete one for the next
read. The keyspace is one `Shared<HashMap<Box<Slots<u8>>, Box<Slots<u8>>, c>>`
from the standard library, and each command is one atomic statement over it;
parsing, encoding and the socket calls run outside the statement.

### What would distinguish the hypotheses, stated before measuring

`redis-server` 7.0.15 with persistence off (`--save "" --appendonly no`) is
the reference; both servers run pinned to CPUs 0 and 1 and `redis-benchmark`
to CPUs 2 and 3 with `--threads 2`, 50 clients, 16-byte values and keys drawn
from 100,000, one million requests per test, the two servers interleaved and
each measured twice:

- **Correct.** Every run completes with no error reply, and after
  `redis-benchmark -t incr -n 100000 -c 50` the shared counter reads 100,000
  on both servers.
- **The object does not serialize the server.** Without pipelining the subset
  on two drivers reaches at least the reference's rate for `SET` and `GET`:
  one request per system call is mostly socket work, which the subset spreads
  over both drivers while the reference runs it on one thread.
- **The drivers are used.** Without pipelining the subset on two drivers
  reaches at least 1.3 times its own rate on one driver.
- **A command costs no more than twice the reference's.** With 16 requests
  per pipeline, where the command itself dominates, the subset on two
  drivers reaches at least half the reference's rate for `SET` and `GET`.

A criterion that fails is attributed with a profile of the subset before any
conclusion is drawn from it.

### The first run

`redis-bench.sh` at `c40b738f5`, requests per second, two passes:

| Line | `SET` | `GET` | `SET`, 16 per pipeline | `GET`, 16 per pipeline |
|---|---|---|---|---|
| redis-server | 105,219 / 105,164 | 105,230 / 124,938 | 665,779 / 665,779 | 666,223 / 666,223 |
| subset, 2 drivers | 142,796 / 166,611 | 142,694 / 148,104 | 443,656 / 443,656 | 332,668 / 307,409 |
| subset, 1 driver | 99,980 / 108,085 | 102,533 / 102,543 | 665,779 / 665,779 | 499,750 / 443,656 |

(`redis-benchmark` computes a rate from whole milliseconds, so one million
requests in 1,502 ms prints 665,779 for any line that takes that long.)

- **Correct: met.** Every line passed the correctness pass: 100,000
  increments from 50 clients read back 100,000, and `SET`, `GET`, `INCR` and
  `DEL` answered as Redis does.
- **The object does not serialize the server: met.** Without pipelining the
  subset on two drivers reached 1.36 and 1.58 times the reference for `SET`
  and 1.36 and 1.19 times for `GET`.
- **The drivers are used: met.** Without pipelining two drivers reached 1.43
  and 1.54 times one driver for `SET` and 1.39 and 1.44 times for `GET`.
- **A command costs no more than twice the reference's: not met for `GET`.**
  With 16 per pipeline two drivers reached 0.67 of the reference for `SET`
  but 0.50 and 0.46 for `GET`. One driver did better than two: it matched
  the reference for `SET` and reached 0.75 and 0.67 for `GET`.

### Attribution

No profiler is installed on this host, so the attribution is by two
comparisons, each run on the same pinning.

**Two drivers lose to one because of the object.** With 16 per pipeline,
`PING`, which takes no atomic statement, ran at the client's ceiling of about
two million requests per second on one driver and on two, while `SET` fell
from 799,361 and 726,744 on one driver to 469,704 and 469,814 on two. A build
of the same subset whose runtime counts, per driver, the acquires and the
parks in `wf__shared_acquire` and prints them when the server is stopped (a
scratch patch, not kept; its output is in `redis-samples.csv`) found that on one
driver no acquire parks, as the design says, and on two drivers 786,734 of a
million `SET` acquires parked without pipelining and 964,457 with 16 per
pipeline. The first-come queue hands the object straight to the context at
its head, which is parked: the object is then held by a context no driver is
running until one resumes it, and every statement that arrives meanwhile
parks behind it. A block cannot wait, so a holder is always running and holds
the object for the block's compute alone; parking is the wrong answer to a
held object whose holder will finish in a fraction of a microsecond. This is
the lock convoy the queue's fairness produces, a property of the runtime and
not of the statement.

**`GET` costs more than `SET` because of the program.** `run_get` copies a
value into the reply by walking all 1,024 positions of its buffer, whatever
the value's length, where `SET` copies the value's own 16 bytes.

### The acquire without a convoy, stated before measuring

The runtime is changed so that a statement that finds its object held spins
for a bounded time before it parks, and an unlock wakes the first parked
statement to try again instead of handing it the object; a woken statement
that loses again keeps its place at the queue's head. The emitted acquire
therefore retries after it resumes. The program is changed so that `GET`
copies only the value's own bytes. Each change is judged on its own, against
the first run's build with only that change undone:

- **The convoy is gone** if, with 16 per pipeline, two drivers reach at
  least one driver's `SET` rate and fewer than one in ten of their acquires
  park.
- **The copy was the `GET` gap** if, with 16 per pipeline on one driver,
  `GET` reaches at least 0.9 of `SET`.

### The acquire without a convoy: results

Both comparisons ran on the counting build, the changed and unchanged
variants interleaved, requests per second; the raw output of every run in this
experiment is `research/experiments/io-completion-bench/redis-samples.csv`.

- **The convoy is gone: met.** On the first run's program, with 16 per
  pipeline, two drivers reached 999,001 `SET`s per second in both passes
  against 499,002 and 499,500 with the first-come handoff, while one driver
  reached 665,779 and 570,776; 5,736 and 6,241 of about a million acquires
  parked, against 969,192 and 972,752. Without pipelining two drivers went
  from 159,949 to 153,775 and 173,822, with 6,159 and 5,158 parks against
  755,665 and 561,330.
- **The copy was the `GET` gap: met.** On the new runtime, one driver, 16
  per pipeline, `GET` went from 444,247, 443,853 and 443,853 to 666,223,
  665,779 and 799,361 against `SET`s of 570,776, 666,223 and 665,336.

### The second run

`redis-bench.sh` at `2d5c41250`, requests per second, two passes:

| Line | `SET` | `GET` | `SET`, 16 per pipeline | `GET`, 16 per pipeline |
|---|---|---|---|---|
| redis-server | 111,049 / 108,050 | 114,181 / 105,053 | 570,451 / 570,451 | 664,894 / 571,102 |
| subset, 2 drivers | 153,775 / 166,583 | 159,949 / 153,799 | 799,361 / 799,361 | 998,004 / 999,001 |
| subset, 1 driver | 99,980 / 108,085 | 102,543 / 102,270 | 664,452 / 570,776 | 665,779 / 666,223 |

The host: 4 CPUs (Intel Xeon at 2.80 GHz), Linux 6.18, redis-server and
redis-benchmark 7.0.15, clang 18.1.3.

Every criterion is met. The correctness pass held on every line. Without
pipelining two drivers reached 1.38 and 1.54 times the reference for `SET`
and 1.40 and 1.46 for `GET`, and 1.54 and 1.54 times one driver for `SET`
and 1.56 and 1.50 for `GET`. With 16 per pipeline two drivers reached 1.40
times the reference for `SET` and 1.50 and 1.75 for `GET`. The reference
moved between runs: its pipelined `SET` rate was 665,779 in the first run and
570,451 in the second, and against the first run's figure the second run's
subset would be 1.20 times it.

The reference runs its protocol work on one thread. Not a criterion, but the
comparison the result invites: `redis-server` with `--io-threads 2
--io-threads-do-reads yes` on the same two CPUs, two passes interleaved with
the subset on two drivers, reached 128,999 and 142,796 `SET`s and 142,816
and 148,082 `GET`s without pipelining against the subset's 159,949 and
166,611 and 153,775 and 166,611, and 665,779 and 570,451 `SET`s and 665,779
and 666,223 `GET`s with 16 per pipeline against 799,361, 798,722, 999,001 and
999,001.

What this shows, and what it does not. A server written in the language, its
keyspace one shared object changed only in atomic statements, keeps up with
Redis on this host for these commands, and the one object does not keep it
from using two cores. It does not show the subset is as fast as Redis
per command in general: Redis carries its full command table, expiry,
encoding choices and statistics, which the subset does not, and the rates
here are near what two client threads on this host issue, so the ratios may
understate either server. The subset is 830 lines, most of them RESP parsing
and encoding that a library would hold.


## Progress while a guard waits

**The question.** Executing every call in order was a conforming execution
[WAIT-2], and in it a consumer started before its producer waits for good on
a guard only the producer's later statement makes true, while this compiler's
drivers run the producer and the consumer completes. The completion review
raised it, and the owner asked whether one thread with a reader before a
writer would then deadlock. On 2026-09-29 the owner approved every decision
card and the specification revisions, taking for this card the revised
recommendation: the language promises that progress ("all decisions
approved, the spec revisions approved too", written in Chinese).

**The rule** (specification v0.80). While a statement waits for its guard,
each call whose execution contains it and that [WAIT-2] permits to run
alongside the statements after it executes as a context, and the starter
waits for the call only before or within a statement that holds a point at
which [WAIT-2] requires the call to have completed. While every context keeps
reaching its completion or a wait for a false guard, an unfinished context or
a host operation whose outcome has not arrived, every context that waits for
nothing proceeds, and a statement with no guard, or whose guard is true from
some point on, takes effect [SHARE-3]. [WAIT-2] keeps an in-order
implementation conforming on every execution in which no guard waits.

Both limits follow the runtime. The drivers do not preempt, and a waiting
call that the host or the object answers at once does not suspend, so a
context that loops on such calls, or computes forever, holds back the others
on its driver; the premise names only the waits that suspend. A `let`-bound
call is joined before the whole statement that uses its result or leaves the
block, not inside it on the path that does (`WAITS.md`, "A bound context is
joined where its result is first used"), so an atomic statement inside that
statement, ahead of the use, does not run first. Both limits are recorded in
`docs/todo.md`. A statement whose guard only its own context's later statement makes
true, or two contexts each waiting for the other's write, still wait for
good: the promise covers progress other contexts can make, not a cycle.

**What the compiler had to change.** Three places kept that promise only by
accident, and each change was checked by making it fail once:

- *Which calls start.* Only a call marked `mustpar` started a context, and
  the marker is erased proof syntax that adds no permission [PAR-4]. The
  checker now records every unmarked waiting call the permission covers, and
  once all bodies are checked, a fixed point finds the functions that may
  reach an atomic statement with a guard; each recorded call to one of them
  starts a context as a marked call does. A call that reaches no guard still
  runs in order, so a program without guards starts no context it did not
  mark: `redis_subset.wf` and `shared_objects.wf` start exactly one context
  per `mustpar` in their emitted modules. With the pass disabled,
  `share-pos-guard-progress-without-marker` stops with "every context waits
  for a context that is not waiting for the host".
- *Where a bound start joins.* A `let`-bound context was joined before the
  first later statement whose overlap footprint reached the binding or was
  refused or not resolved, and loops, matches and atomic statements are
  refused forms, so a starter waited for its context before the very
  statement that would make the context's guard true. The join now precedes
  the first statement that may leave the block or that names the binding,
  looking into compound statements; every read, write, release and
  reference formation of the binding names it. Under the old plan
  `share-pos-guard-progress-bound-result` stopped the same way, and a unit
  test pins that a read through a `Box`, whose footprint is not resolved, no
  longer forces the join.
- *How often a statement is woken in vain.* A woken statement that missed
  the object parked at the head again with no bound. The unlock after two vain
  wakes now hands it the object. These runs were made by hand, and the
  counting build is not kept: a copy of the runtime that wrote one byte per
  handoff to standard error. On `shared_objects.wf` it saw 0, 3, 18 and 26
  handoffs on 1, 2, 4 and 8 drivers, every sum correct; under
  ThreadSanitizer, with runtime and module instrumented as above, 53, 639
  and 1,763 handoffs on 2, 4 and 8 drivers and no report. The uncounted
  program ran 40 times each on 2 and 8 drivers without a wrong sum. No test
  forces a handoff deterministically (`docs/todo.md`). One round of
  `redis-bench.sh` on the handoff runtime passed the correctness pass and
  gave two drivers 1.46 and 1.33 times the reference for `SET` and `GET`
  without pipelining, 1.42 and 1.44 times one driver, and 1.40 and 1.40 the
  reference with 16 per pipeline, which meets every criterion; `GET` falls a
  little below the second run's range, which one round does not attribute.
  That round was built before the two checker changes, which add no start
  and no bound join to that program.

## Remaining questions

1. Spelling: `atomic s = &h { }`. This reuses `&` in a position where `h` is
   a handle and `s` names the state behind it. The alternative is
   `atomic h as s { }`.
2. Several objects in one statement, above.
3. Reader concurrency. Whether statements that only read should share the
   object is a runtime choice to measure on a workload where readers
   contend; the runtime's entries take a read request, but lowering makes
   none, so that path runs in no program.
4. `nodrop` state and taking the value back (`shared_into`, which returns the
   state when its caller holds the last handle).
5. An invariant the object declares and every block preserves.

The owner's rulings of 2026-09-29 settled two earlier questions: [ENT-3]'s
S1 source now names an atomic statement's guard, and progress while a guard
waits is the section above.

## What would test it

Experiment 7 above is the test this record proposed: the Redis subset, run
against `redis-benchmark` beside `redis-server`. It left out `MULTI`/`EXEC`,
which a connection can serve as one atomic statement over the commands it
queued, and `BLPOP`, a guarded statement; both remain to write. The subset is
longer than the sketch because it parses and encodes RESP itself.
