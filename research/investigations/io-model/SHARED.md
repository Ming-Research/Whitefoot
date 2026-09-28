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
  Ada's protected entries.
- Whether the statement reads or writes the object follows from its block's
  footprint: a block that writes no path rooted at `s` is a read, and reads
  of one object may run concurrently. Nothing marks it.

### What the writer sees: a Redis subset

Proposed syntax, not yet checked. `Bytes` stands for the program's own
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
   - The statement reads that place when it begins and again when it ends,
     so moving or writing the handle inside the block is a use of a dead or
     changed place at the end.
   - The binding's validity ends with the block [REF-2].
   - An atomic statement is admitted only in the body of a waiting function
     and counts as a waiting call for [WAIT-1], [PAR-1] and [PAR-2].
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
directly, it infers read or write from the footprint the checker already
computes, and the guard is one optional clause. Its cost is grammar and a
footprint judgment for one block, the same union the checker already forms
for the arms of an `if`.

Considered and refused:

- **An implicit scope**, where `keyspace^.x` in ordinary code acquires the
  object for the life of the reference. It was refused because the atomicity
  boundary would be invisible: whether two adjacent statements form one
  transaction could not be read from the source, and that is the root of
  read-modify-write races.
- **Asynchronous messages to the object (actors, channels).** They were
  refused because a sender that does not wait for the effect cannot know the
  order its messages take effect in. This fails the sharing rule, as channels
  do.
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

## Remaining questions

1. Spelling: `atomic s = &h { }`. This reuses `&` in a position where `h` is
   a handle and `s` names the state behind it. The alternative is
   `atomic h as s { }`.
2. Several objects in one statement, above.
3. `nodrop` state and taking the value back (`shared_into`, which returns the
   state when its caller holds the last handle).
4. An invariant the object declares and every block preserves.

## What would test it

Write the Redis subset (`GET`, `SET`, `DEL`, `INCR`, `MULTI`/`EXEC`, RESP2
over TCP) as `tests/programs/redis_subset.wf`, and check two things:

- Whether it is as short as the sketch above.
- Whether it runs against `redis-benchmark` on the echo bench's host,
  compared with `redis-server` on its default configuration.

Criteria are to be recorded before the measurement, as for Experiments 1 to 6.
