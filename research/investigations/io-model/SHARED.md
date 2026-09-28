<!-- Serves the shared-state form that a multi-client server such as a Redis
     subset needs (WAITS.md, "Sharing between concurrent activities", which
     deferred it until a program needs one). Design exploration for the
     owner's ruling; its surviving decisions go to the design tree and the
     specification, and this record stays as their grounds. -->

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
how elegant, or at least how not ugly, it can be", written in Chinese). This
record is that design. Nothing in it is decided.

## The idea

**A shared object is a small host that the program runs for itself, and a
transaction is a waiting call whose body cannot wait.**

The language already has everything that sentence needs:

- The program means its sequential execution, and which outstanding operation
  completes first is an input of the execution [WAIT-2]. The order in which
  other contexts' transactions reach the object is one more such input. A
  transaction observes some prefix of that order, exactly as a receive
  observes whatever bytes the peer has sent.
- Only a waiting function may make a waiting call [WAIT-1]. A transaction's
  body is a function-kind parameter without `waits`. It therefore cannot
  start another transaction, cannot wait for the host, and cannot hold the
  object while anything outside it runs. The existing rule gives nested
  transactions, lock-order deadlocks and holding a lock across I/O no
  representation at all; none of them needs a new rule.
- A waiting call never overlaps another statement of its context [PAR-1].
  Transactions of one context therefore take effect in its source order, with
  no new ordering rule.
- References never escape [REF-3]. The body receives `&T` for its duration
  only, so no reference into the shared state, and no proof fact about it,
  survives the transaction. What the caller learns is the owned result.
- The body's effect row is its complete footprint [EFF-1]: the object's state
  and the caller's environment. The runtime therefore knows exactly what a
  transaction touches, and can run it wherever it likes.

## Surface

Prelude additions. The shape follows `factory_share` and the collection
modules' callback functions; no grammar changes.

```wf
opaque nocopy struct Shared<T: drop> {}

fn shared_new<T: drop>(value: T) -> result: Shared<T> pure;
fn shared_share<T: drop>(shared: &Shared<T>) -> result: Shared<T> reads(shared);

fn transact<T: drop, E, R,
            fn body(state: &T, env: &E) -> result: R writes(state), writes(env)>(
    shared: &Shared<T>, env: &E) -> result: R reads(shared), writes(env) waits;

fn inspect<T: drop, E, R,
           fn body(state: &T, env: &E) -> result: R reads(state), writes(env)>(
    shared: &Shared<T>, env: &E) -> result: R reads(shared), writes(env) waits;
```

- `shared_new` moves a value into a new object; `shared_share` returns another
  handle to the same object, to move into a context.
- A handle is `nocopy`. The object is released, and its `T` dropped, when its
  last handle is released [OWN-1]. `T` must have drop in this version.
- `transact` runs `body` with exclusive access to the object's state and the
  caller's `env`. `inspect` is the same with a body whose row only reads the
  state; the runtime may run inspections of one object concurrently with one
  another. The row is what tells them apart.
- `env` is a reference into the calling context's own storage. That is sound
  because the caller is suspended inside the waiting call for the whole
  transaction, so its storage is quiescent even if the body runs on another
  thread.

### What the writer sees: a Redis subset

Proposed syntax, not yet checked; `Bytes` stands for the program's own
byte-string type, and `...` for ordinary parsing and encoding code.

```wf
struct Store {
  keys: HashMap<Bytes, Bytes, 16777216>;
}

struct Request {
  command: Command;
  reply: Bytes;
}

fn execute(store: &Store, request: &Request) -> result: unit writes(store), writes(request) {
  doc "Runs one command against the store and encodes its reply.";
  match request^.command {
    Get(key: key) => {
      match store_get(store: store, key: &key) {  // borrowed lookup
        ...                                        // encode $len\r\n...\r\n into request^.reply
      }
    }
    Set(key: key, value: value) => { ... }
    Del(key: key) => { ... }
  }
  return unit;
}

fn serve(connection: TcpConnection, factory: HandleFactory, keyspace: Shared<Store>) -> result: unit pure waits {
  let request = request_new();
  loop @commands {
    // read and parse one RESP command into request.command: ordinary code
    ...
    transact::<Store, Request, unit, fn execute>(shared: &keyspace, env: &request);
    // send request.reply: ordinary code
    ...
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

There is one line of concurrency in `serve` and two in `main`; everything else
is ordinary sequential code, and `execute` is an ordinary function that a unit
test can call on a local `Store`. `MULTI`/`EXEC` is one transaction whose
request holds several commands.

## Meaning

The rules this would add, stated as they would read in the specification:

1. A transaction's body executes with exclusive access to its object's state.
   The transaction takes effect entirely at one point after its call begins
   and before it returns.
2. Transactions on one object take effect one at a time. The order in which
   transactions of different contexts take effect is an input of the
   execution, exactly as which outstanding operation completes first is
   [WAIT-2]. Transactions of one context take effect in its source order.
3. An inspection takes effect like a transaction that writes nothing.
4. The object's state belongs to no context. It is reached only through a
   body's `state` parameter, so it contributes no path to any footprint
   [PAR-1], and its handles are ordinary values.

Rule 1 is linearizability. When client X receives the reply to `SET` and then
tells client Y, and Y sends `GET`, Y's transaction begins after X's returned,
so it sees the write. No completion stamps are needed. The stronger form
considered in `WAITS.md`, transactions ordered by the host completions that
produced them, would only order transactions that overlap in time, which no
program can observe apart from their results.

**Why the rest of the language is untouched.**
- Inside a body, ownership, windows and proofs work unchanged: the state is
  an ordinary `&T` parameter.
- Outside a body nothing about the state is known, which is true: another
  context may have changed it.
- The sharing rule (`WAITS.md`) is extended by one clause rather than broken:
  an object whose every operation is one atomic transaction admits every
  interleaving by its definition, as a peer admits every answer.

## What the runtime may do

Whether, when and where a transaction runs is unobservable beyond rule 2, and
the body's row names its whole footprint. The runtime may therefore choose
freely among these:

- **A lock.** One word per object and a queue of parked contexts. A
  transaction that finds the object free costs one atomic acquire and one
  release. One that finds it held parks its context and lets the driver run
  others. Because a body never waits, the object is held only for the
  duration of pure computation.
- **Combining.** A transaction that finds the object held enqueues its body,
  environment and result slot. The holder runs the queued bodies before
  releasing, so the state stays in one core's cache and a burst costs one
  handoff rather than one per transaction. This is the known best form for
  one hot structure, and it is possible only because a body cannot wait.
- **A home driver.** The object is pinned to one driver and transactions are
  shipped to it, like an actor. This is better when the state is large and
  contention is constant.
- **Concurrent inspections.** Inspections run concurrently with one another
  under a reader-writer lock or RCU. That is read parallelism Redis itself
  does not have.
- **Optimistic re-execution.** A body has no side effect beyond its row, so
  one that lost a race can simply be run again.

For a Redis subset this is the Redis 6 I/O-thread shape without configuration.
Parsing, encoding and all network I/O run in each connection's context,
spread over the drivers. Only `execute` is serialized on the object.

## Alternatives within B

- **A block form**, `let r = atomic (state in &keyspace) { ... give v; };`,
  would read better at the call site and could reach locals directly. It
  needs new grammar and a footprint judgment for a block. It also puts the
  command's logic inline, where the function form keeps it testable alone.
  Deferred until a program shows the function form's generic arguments to be
  a burden; a per-program wrapper function hides them today.
- **Moving the environment in and out by value** instead of lending it: this
  forbids nothing that the reference forbids, and it costs a move per
  transaction and a result type per environment. Rejected, because the
  suspended caller makes the reference sound.
- **Transactions ordered by host completion** (the second form in
  `WAITS.md`): it adds completion stamps and orders nothing a program can
  observe beyond rule 1. Rejected.
- **Asynchronous messages to the object (actors, channels):** a sender
  that does not wait for the effect cannot know the order its messages take
  effect in. This fails the sharing rule for the same reason channels do.
  Rejected.
- **Atomic fields and lock-free cells:** they expose the interleaving of
  individual reads and writes, which is exactly what the sequential meaning
  excludes. Rejected.

## Extensions it leaves room for

- **Guarded transactions.** `transact_when` takes a `ready(state, env) ->
  Bool` predicate that reads only, and takes effect at a point where `ready`
  holds. This is Hoare's conditional critical region, or Ada's protected
  entry. `BLPOP` waits until a list is non-empty. A pub/sub subscriber waits
  until its inbox is non-empty, so pub/sub needs no second primitive. The
  runtime re-checks guards after each transaction on the object.
- **Several objects at once.** A transaction over two objects that the
  runtime acquires in a fixed order stays deadlock-free. It is for sharded
  keyspaces with multi-key commands.
- **Object invariants.** The body's function-kind parameter can carry
  `requires` and `ensures` [FN-3], so a checked invariant that every
  transaction preserves is expressible, for example that a count field
  equals the number of entries. What form a shared object would declare it
  in is open.
- **`nodrop` state and getting the value back.** `shared_into` would return
  the state when its caller holds the last handle.

## Open questions for the owner

1. Is the function form acceptable, or should the first version already have
   an `atomic` block?
2. Should `inspect` ship in the first version? It is cheap to specify, and it
   is where reads can beat Redis.
3. Should guarded transactions be in the first version, or wait for `BLPOP`
   and pub/sub?
4. Names: `Shared`, `transact`, `inspect`, `shared_share`.

## What would test it

Write the Redis subset (`GET`, `SET`, `DEL`, `INCR`, `MULTI`/`EXEC`, RESP2
over TCP) as `tests/programs/redis_subset.wf`, and check two things:

- Whether it is as short as the sketch above.
- Whether it runs against `redis-benchmark` on the echo bench's host,
  compared with `redis-server` on its default configuration.

Criteria are to be recorded before the measurement, as for Experiments 1 to 6.
