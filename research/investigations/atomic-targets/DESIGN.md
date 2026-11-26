# The targets of an atomic statement, designed as one form

## Status

A design study, written 2026-10-13 before any implementation. Nothing in it
was compiled or run. It reads the active specification `spec/kernel-spec.md`
(title v0.138) and the design tree at Whitefoot `93e97efdc` (main); specification
rules are cited by rule ID, without line numbers. Measured results (with their
run, measured elsewhere and cited) and estimates (unmeasured judgment) are
marked as such; every other claim is a deduction from cited rules or a reading
of cited sources unless marked.

It answers the owner's direction on the Firn card that asked for a
conditional target (2026-10-13, quoted on the board item "the atomic statement has one designed way to write which objects it holds" (translated)):

> The forms after `atomic` keep getting more complicated; Whitefoot needs to design the shape of what follows `atomic` properly. We cannot keep adding one form at a time. (translated)

So this is not a study of one addition. It lays out every target form the
statement has or is about to get, states the questions the header must
answer, gives three genuinely different ways to answer them (and a fourth,
data-shaped one), writes Firn's programs under each, and recommends one. The
owner decides on a card; the specification change follows the ruling.

The pending additions it must fit, and the records they live in:

- the sharded state `Shards<T>` with the `any_shard()` selector and a whole
  target (owner-approved specification draft on Whitefoot PR #385, branch
  `claude/shared-counter-study`, `research/investigations/shared-counters/DESIGN.md`
  there; at the time of writing (main `93e97efdc`, v0.138) its draft is written
  against v0.136 and needs rebasing; the shards specification has since been
  approved and is on its pull request at a later version);
- the atomic value (owner chose option A on 2026-10-13, card "should there be an 'atomic value' type?" (translated)):
  a shared 64-bit unsigned value used only as a target, at most one operation
  per statement per path, performed at the statement's point without a lock
  on targets with a hardware atomic add, plus the rule "a statement that
  completes before another begins is ordered first"
  (`research/investigations/sequence-take/DESIGN.md` on branch
  `claude/sequence-take`, candidate (V));
- Firn's request for a target held only when a run-time condition holds
  (board item above; Firn-wf PR #65 is the two-statement form);
- the backlog item "an atomic statement waiting on its guard can set a deadline" (translated) (a guard wait with a deadline);
- the handle-count rule of Whitefoot PR #409 (branch `claude/skip-handle-count`,
  not yet on main): a statement counts its handle on the object only when its
  guard or block may write, move or release the place it read the handle from.

## 0. Summary

**The finding.** Every target form the statement has today is decided by the
*place* after `&`, never by a word in the statement: the handle's type says
whether the state is written or read-only, the state's type says what kind of
state it is, and one optional index atom's type says how much of it is held
(an entry under bytes, the entries under a key set, one shard under the shard
selector, or the whole with no index). Both pending state kinds, shards and
the atomic value, fit that rule without a new word. What the statement itself
adds is only: the binding names, and how it waits (`when`). So the header has
four questions, and a coherent design answers each in exactly one place:

| question | answered by | today | pending |
|---|---|---|---|
| what is held | the place: handle type, state type, one index atom | whole, entry, entries, whole map | one shard, all shards, the atomic value |
| whether it is held on this execution | today the executed path: a group is taken at its first use, with every earlier untaken group (compiler behaviour that [SHARE-3] permits, not a guarantee); option A below would add a per-target condition | held on the paths that reach a use; entry, key-set and guard groups and every earlier group before the guard | Firn's "only when AOF is on", which the first-use rule already gives when every use is under the condition |
| how the statement waits | the statement's wait clause | `when` guard | a deadline on the wait |
| what the block may do with it | the state type and the handle type | read-only through `SharedRead`; writes decide a read hold | one operation per path on an atomic value; guards read no shard |

**Two questions, kept apart.** Firn's immediate need, one statement per
command that holds `meta` only when logging or timed, is met by today's
compiler: keep `m = &store^.meta` unconditional and put every use of `m`
under `if held { }` (option C's form, section 5.3; measured as Q0, section
8.2). Firn's slow form used `m` on the block's first line, so it locked `meta`
on every execution. The options below answer a different question: whether
the language should also *guarantee* conditional holding and *refuse* a use
outside the condition, and let a target of an earlier-ordered type be held
conditionally. No current program needs either, so the recommended first step
is no language change; option A is the design to adopt when one does.

**The options** (section 5), each a different place to put "whether it is
held":

- **A. A per-target condition in the header**, `m = &store^.meta if held`:
  the held set is decided from atoms read when the statement begins; the
  binding is in scope only inside `if held { }` blocks of the atomic block.
  No new fact channel, no change to the lock order, zero cost when the
  condition is false.
- **B. Hold extension inside the block**, `hold m = &store^.meta { }` under
  ordinary control flow: the header stays as it is; the hold's type must
  follow every header type in the lock order, checked statically; scoping
  gives the proof. Can also hold an object found inside held state.
- **C. No syntax: elision by rule**, specifying that an implementation holds
  a target from its first use, so Firn restructures its block. Honest about
  its limits: the writer cannot ask for it, the checker cannot refuse a stray
  use, and it fails for a conditional target whose type precedes a used one.
- **D. An optional target shaped as data**, binding `&Option<T>` matched in
  the block: reuses payload-selection facts, but needs a reference
  representation no other binding has and nests `Option` for entries.

**Recommendation (section 7), if the language is to guarantee conditional holding: A**, with the wait clause extended to
`when g until d { } else { }` for the deadline when that work is scheduled,
and the shard and atomic-value forms adopted as index-atom and state-type
rows of the one table. Reasons: it is the only option whose held set is fixed
before the statement takes any lock, so the lock-order argument of the
design tree is untouched and any type may be conditional; it needs no new
proof machinery (a scoping rule, as `match` arm binders are scoped); a target
not held costs no shared write; and every existing program is unchanged.
What would overturn it: a program that must decide to hold an object from
what it finds inside the held state (B's unique power), or the owner
preferring the proof of use to come from facts rather than from scope
(section 5.1, A-fact).

**Check plan (section 8):** conformance cases for each rule, a program test
with Firn's shape, and two measurements on the 14900K's 8 P-cores: first,
before any language change, Firn's SET rewritten as one statement under
today's compiler, which already takes a later-ordered target at its first
use (Q0, since measured: section 8.2); then, after implementation, Firn's 40 sites as one statement each
with AOF off, within twin noise of 21.4 M/s.

## 1. The current target forms and their rules

**Source-read** from [SHARE-1], [SHARE-2], [SHARE-3], [OP-4] and the
`atomic_stmt` grammar production: `atomic_stmt := "atomic" IDENT "=" "&" place ("," IDENT "=" "&" place)* ("when" expr)? "{" stmt* "}"`.

| target place | index atom | binding | what the statement holds |
|---|---|---|---|
| handle place of type `Shared<T>` | none | `&T`, the state | the object, exclusively |
| handle place of type `SharedRead<T>` | none | `&T`, every path through it read-only | the object; the read-only authority follows aliases and call rows |
| `Shared<ConcurrentHashMap<V>>` or `SharedRead<…>` | an atom of type `&[u8]` | `&Option<V>`, the entry under those bytes | that entry; shared with other non-writing statements when the block writes nothing through it (keyed-tables, decision 6; lowering, not source) |
| the same | a place of type `KeySet`, not moved | `&Entries<V>`, element at each index the entry under the set's key at that index, `len` the set's | those entries, locked together in one order |
| the same | none | `&ConcurrentHashMap<V>`, indexable in the block by bytes and by key set | the whole map |

Rules that hold across the table, each cited where the design below keeps or
changes it:

- **R-names.** Each target is `IDENT = &place`; the guard and block together
  name every binding, or the statement is refused ([SHARE-2]: "the guard and the block together name every
  binding"; shared-objects, "unnamed target refused").
- **R-read-at-begin.** Handle places and index atoms are read when the
  statement begins, before it holds anything, and form no path through a
  binding of the statement ([SHARE-2]).
- **R-overlap.** Two targets with overlapping handle places each have an
  index step, so several entry or key-set targets may name one handle place
  while a whole target stands alone on it ([SHARE-2]). Two handles may name one
  object; the statement holds it once; the checker compares the states of
  targets whose types unify as paths from one root ([SHARE-2]).
- **R-live.** Each object stays live until the statement completes whatever
  the block does with the handle place ([SHARE-2]); with PR #409 the handle is
  counted only when the block may lose the place.
- **R-scope.** Bindings are in scope in the guard and the block and leave
  scope when the block ends by any edge ([SHARE-2]).
- **R-waits.** The statement counts as a waiting call; its guard and block
  contain no waiting call and no atomic statement ([SHARE-2]).
- **R-guard.** The guard has an `if` condition's judgment, writes no path
  ([SHARE-2]), and its goals are established at the block's entry
  ([ENT-3.S1]).
- **R-point.** The statement takes effect at one point after it begins and
  before it completes, with exclusive access to its targets' states, in one
  order with every other statement; an implementation may hold less wherever
  outcomes are unchanged ([SHARE-3]).
- **R-order** (design tree, state-locks second decision): every lock follows
  one order, types by a static total order and objects of one type by
  identity at run time; a type's group is taken at the first use of any of
  its targets on the path, together with every earlier group not yet taken;
  entry groups and every group the guard reads are taken before the guard;
  everything is released when the block ends.
- **R-invariants.** A target of a struct with type invariants establishes
  them at the block's entry and owes them at every leaving edge ([TYPE-11]).

Pending rows, as their records draft them, and what they add to the rules:

| target place | index atom | binding | holds | record |
|---|---|---|---|---|
| `Shared<Shards<T>>` or `SharedRead<…>` | a place of type `AnyShard` | `&T`, one shard the implementation selects | that shard | PR #385 |
| the same | none | `&Shards<T>`, indexable, `len` at least one | every shard | PR #385 |
| `Shared<Atomic<u64>>` or `SharedRead<…>` | none | `&Atomic<u64>` | nothing locked: one hardware step at the point | sequence-take (V) |

- Shards add: a shard target stands alone on its handle place, like a whole
  target; a guard reads no path starting at a shards state; a shard path is
  not exact.
- The atomic value adds: at most one operation per path, counting every
  atomic-value binding of the statement together; none in a loop; the binding
  appears only as the `cell` argument of its operations, never in the guard;
  the operation is performed after the statement's last acquisition; and
  [SHARE-3] gains the real-time sentence S6.

The owner's remark is right about the growth: each row brought a sentence to
SHARE-2 about what may stand beside what, and Firn's condition would bring
another. The design below replaces those sentences with one rule each for
"what", "whether", "how it waits" and "what the block may do".

## 2. What the pending forms require

### 2.1 Firn's conditional hold (the witness)

**Source-read** from the board item and Firn-wf branch `claude/write-meta-free`
(`firn/commands/strings.wf`, Firn's SET). Firn's SET must hold the global write
state `meta` only when the append-only file is on or the command carries an
expiry option; otherwise only the key's entry. Today each of 40 commands is
two statements around one body:

```
let held = bor(logged, timed);
if held {
  atomic slot = &store^.keys[key], m = &store^.meta {
    if aof_refuse(meta: m, client: client) {
      return unit;
    }
    let sequence = pkg::store::sequence_new_key(meta: m, loading: time^.loading);
    let log_budget = pkg::store::log_budget_begin(meta: m, loading: time^.loading);
    set work = set_body(sequence: &sequence, journal: &journal, access_key: key, slot: slot, client: client, value: move value, flags: flags, at: at);
    ...
    pkg::store::sequence_commit(meta: m, sequence: &sequence, budget: &log_budget);
  }
} else {
  atomic slot = &store^.keys[key] {
    let sequence = pkg::store::sequence_inert();
    set work = set_body(sequence: &sequence, journal: &journal, access_key: key, slot: slot, client: client, value: move value, flags: flags, at: at);
  }
}
```

**Measured** (Firn, 14900K, 8 cores, cited from the board item): SET with the
global lock on every write 1.23 M/s; without it 21.4 M/s, about 17 times.

Requirements, each a check the design must pass:

- **F1, one statement, one body.** Each of the 40 sites is one atomic
  statement whose body appears once.
- **F2, refused where unknown.** A use of the conditional binding at a point
  where the condition is not known true is refused by the checker.
- **F3, decided before begin.** Whether the object is held is fixed from
  values the statement reads when it begins (Firn's own observation, and
  [SHARE-3]'s one point: a hold decided inside the block would be an
  acquisition after the block has read state, which R-order permits only for
  types later in the order, section 5.3).
- **F4, no cost when not held.** On an execution whose condition is false,
  the statement performs no shared write and takes no lock for that target:
  no lock word, no handle count, no watch. Firn's acceptance: 8-core SET with
  AOF off within twin noise of 21.4 M/s.
- **F5, the condition is any Bool.** Firn's condition `bor(logged, timed)`
  mixes a field of the handle's holder and a command flag; nothing about the
  condition is known at compile time.

### 2.2 The atomic value (owner's option A)

- **V1.** A whole target on a `Shared<Atomic<u64>>` handle; the binding is
  the `cell` argument of `atomic_load`, `atomic_add` or `atomic_raise` and
  nothing else.
- **V2.** At most one operation per path, all atomic-value bindings counted
  together, none in a loop; the reason is a deduction in the sequence-take
  study (section 5.6): two single steps in one statement admit an
  interleaving no one order explains.
- **V3.** The operation is performed after the statement's last acquisition
  (sequence-take, section 3.3, "after the last lock"), so a design that lets
  acquisitions happen later in the block must keep the operation after all of
  them on every path.
- **V4.** Firn's AOF-off path performs no operation and touches no shared
  line (the owner's explicit requirement on the card: no cost on every
  statement). With a conditional target this becomes: the atomic value is a
  conditional target, or the operation sits under `if logging` as the study
  wrote it; both must work.
- **V5.** A statement whose only targets are atomic values takes no lock and
  counts no handle.
- **V6.** Guards read no atomic value (deferred until a program waits on
  one).

### 2.3 Shards (owner-approved draft)

- **S1.** The shard selector is an index atom of type `AnyShard`; the shard a
  target names is an input of the execution.
- **S2.** A shard target stands alone on its handle place; a guard reads no
  path starting at a shards state.
- **S3.** A one-shard statement must not write the object's header (the
  counter study's "lean shard path", section 5.4 there), which is the same
  obligation as V5 and PR #409: the statement writes nothing shared beyond
  its own shard.

### 2.4 Guard waits with a deadline (backlog)

- **D1.** A statement waiting on a guard can also be woken by a time, after
  which it completes without executing its block and the context does
  something else; Firn's AOF writer would wait for "bytes pending or
  stopping" with a one-second deadline instead of sleeping 10 ms in a loop.
- **D2.** The guard still reads only held state: the deadline is an instant
  computed before the statement, not a clock read in the guard (the rule
  that guards read no self-changing host state; a clock is such state, and
  the cancellation state is readable in a guard only because its change is
  itself an atomic transition, context-cancellation, fifth and sixth decisions).
- **D3.** [SHARE-3]'s one point must still describe a timed-out statement:
  it observed its guard false at some point after the deadline, which is an
  observation of held state like any read-only statement's.
- **D4.** The runtime already keeps a per-driver deadline heap for sleeps
  and host operations (bounded-waits, first decision); a parked guard wait
  with a deadline is one more entry in it.

### 2.5 Read-only and the handle count

- **H1.** Read-only authority comes from the handle type `SharedRead<T>` and
  follows aliases and call rows; nothing in the statement marks it.
- **H2.** Whether an entry is held shared comes from the block's write
  footprint; nothing in the source marks it (keyed-tables, decision 6).
- **H3.** The handle count is paid only when the block may lose the handle
  place (PR #409). A target not held on this execution counts nothing.

## 3. What other languages do, and what fails

For each: what composes, what fails, and what Whitefoot's static checking
does that it cannot. "Composes" means: can two independently written pieces
of code that each take locks be combined without reading both.

### 3.1 Go: `sync.Mutex`, `sync.RWMutex`, `sync/atomic`

A mutex is a value with `Lock` and `Unlock`; nothing associates it with the
data it protects, and nothing checks that data is touched only under it
([pkg.go.dev/sync](https://pkg.go.dev/sync)). Races are found at run time by
the race detector ([go.dev/doc/articles/race_detector](https://go.dev/doc/articles/race_detector)),
and a deadlock of every goroutine is a run-time fatal error. `go vet`'s
`copylocks` catches a copied mutex, nothing more. `sync/atomic` operations are
unrelated to any mutex: a program that updates a counter atomically in one
place and under a mutex in another is accepted. Timeouts come from `select`
with `time.After`, which composes with channels but not with mutexes: there is
no "lock with timeout". A conditional lock is `if cond { mu.Lock(); defer mu.Unlock() }`,
and the compiler cannot see which branches hold it.

Fails: forgotten lock (race), wrong lock order (deadlock at run time), a
reader-writer lock that a writer waits on behind readers, `defer` unlock in a
loop (held too long). Composes: nothing statically.

### 3.2 Rust: `Mutex<T>`, `RwLock<T>`, atomics

`Mutex<T>` owns its data, so the data is reachable only through a `MutexGuard`
whose lifetime ends the hold ([std::sync::Mutex](https://doc.rust-lang.org/std/sync/struct.Mutex.html)).
That is the association Go lacks, and it is the same idea as Whitefoot's
state reached only through a target's binding. But the standard library's
documentation says plainly that locking the same mutex twice on one thread
"might" deadlock and that the lock order between mutexes is the program's
problem; the Nomicon states that Rust prevents data races but not deadlocks
([nomicon/races](https://doc.rust-lang.org/nomicon/races.html)). The common
answer is a run-time detector (`parking_lot`'s `deadlock_detection` feature)
or a convention. A guard held across an `.await` is a well-known mistake that
a lint, not the type system, flags. Poisoning adds a failure mode of its own:
a panic while holding leaves the mutex poisoned, and every later lock gets an
error. Atomics with memory orderings are separate from mutexes, and a counter
updated atomically here and under a lock there is accepted. A conditional
lock is spelled with an `Option<MutexGuard>`:

```rust
let guard = if cond { Some(meta.lock().unwrap()) } else { None };
if let Some(m) = guard.as_deref_mut() { ... }
```

which is exactly option D of section 5.4: the condition becomes the shape of
a value, and the proof of use is a `match`. The lock order of the conditional
guard against the other locks is unchecked.

Fails: deadlock (two locks, two orders), poisoning, holding across waits.
Composes: data with its lock; nothing about order.

### 3.3 C++: `std::scoped_lock`, `std::lock`, Clang's thread-safety analysis

`std::lock(m1, m2, ...)` and `std::scoped_lock` acquire several mutexes with a
deadlock-avoidance algorithm, try-and-back-off, which the standard specifies
only as "avoids deadlock" ([cppreference std::lock](https://en.cppreference.com/w/cpp/thread/lock),
[std::scoped_lock](https://en.cppreference.com/w/cpp/thread/scoped_lock)).
It composes several locks taken at once without a global order, at the cost of
a loop of try-locks under contention and no guarantee when a third lock is
taken later in the critical section, which is the lazy-acquisition case of
Whitefoot's R-order. A conditional lock is `std::unique_lock` with
`std::defer_lock`, then `lock()` under an `if`.

Clang's static thread-safety analysis ([clang.llvm.org/docs/ThreadSafetyAnalysis](https://clang.llvm.org/docs/ThreadSafetyAnalysis.html))
is the closest thing in a mainstream language to Whitefoot's checker: data is
annotated `GUARDED_BY(mu)`, functions `REQUIRES(mu)`, and a use outside the
hold is an error; `ACQUIRED_BEFORE` declares an order. Its documentation lists
its limits, and the first is exactly Firn's case: "No conditionally held
locks" — the analysis does not track locks held on some paths and not on
others. It also only knows the locks it is told about, declared per object by
the programmer, not derived from what a region of code touches.

Fails: dynamic avoidance costs a loop; the later-taken third lock; the static
analysis refuses conditional holds. Composes: `scoped_lock` over N locks
dynamically; annotations within what is annotated.

### 3.4 Java: `synchronized`, `Object.wait(timeout)`, `StampedLock`

A `synchronized` block is a reentrant monitor on any object; reentrancy hides
lock-order mistakes until two threads enter in opposite orders. `wait` and
`notify` on the monitor give a guard wait with a deadline, `wait(long)`, with
spurious wakeups the program must loop over ([JLS §17.2](https://docs.oracle.com/javase/specs/jls/se21/html/jls-17.html#jls-17.2)).
`StampedLock` adds optimistic reads validated afterwards and is not reentrant
and has no conditions, a pair of properties its documentation warns about
([StampedLock](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/concurrent/locks/StampedLock.html)).
Nothing associates data with its monitor.

What Whitefoot takes from it: the timed wait is the shape of D1, but
Whitefoot's guard is evaluated under the hold, so a spurious wake costs time
and never correctness ([SHARE-3], "how many times an implementation evaluates
a guard is not observable"), and there is no `notify` to forget.

### 3.5 Software transactional memory: Haskell STM, Clojure refs

Haskell's STM ([Harris, Marlow, Peyton Jones, Herlihy, "Composable Memory
Transactions", PPoPP 2005](https://www.microsoft.com/en-us/research/publication/composable-memory-transactions/))
is the design closest in meaning to Whitefoot's statement: a transaction runs
with the whole read set at one point, `retry` blocks until some variable the
transaction read changes (exactly the guard watch of the state-locks fifth
decision), and `orElse` composes alternatives. Its failures are the known
ones: a transaction cannot perform I/O (it may be re-run), long transactions
starve under short ones, every read and write is logged, and the type system
alone separates `STM` from `IO`. Clojure's refs (`dosync`, `ensure`,
`commute`) are the same model with a run-time `io!` guard against side
effects inside a transaction and `ensure` to prevent write skew
([clojure.org/reference/refs](https://clojure.org/reference/refs)).

Whitefoot chose a block that runs once, holding what it names, and refused
optimistic retry because a block moves values and writes locals
(shared-objects, rejected "transactions that read optimistically and retry").
What it keeps from STM is the guard as `retry` on the read set. What it does
not have is `orElse`: two alternatives in one point. The deadline of section
2.4 is a bounded `orElse` with a time.

### 3.6 Ada protected objects and timed entry calls

Ada's protected object is the nearest kin of the statement ([ARM 9.5.1](http://www.ada-auth.org/standards/22rm/html/RM-9-5-1.html),
[9.5.3](http://www.ada-auth.org/standards/22rm/html/RM-9-5-3.html)): a
protected action runs with exclusive access to the object (the "eggshell"),
entry barriers are Boolean guards re-evaluated when a protected action ends,
and a potentially blocking operation inside a protected action is a bounded
error, which is Whitefoot's "no waiting call in the block" made into a rule
a compiler can check (R-waits). A barrier may not read the entry's parameters
(the program `requeue`s instead); Whitefoot's guard may read locals, which
removes the requeue. Timed entry calls, `select ... or delay`, are D1 with a
deadline ([ARM 9.7.2](http://www.ada-auth.org/standards/22rm/html/RM-9-7-2.html)).
Ceiling locking (Annex D.3) bounds priority inversion, which Whitefoot's
runtime has no equivalent of: a context parked behind a holder waits for the
holder's block to end, and the holder never suspends (concurrent-map, fifth
decision), so the wait is bounded by one block rather than by a priority.
Ada holds one object per action; two objects need nested calls and a
programmer-kept order.

### 3.7 Pony reference capabilities

Pony has no locks: an actor owns its state, reference capabilities (`iso`,
`trn`, `ref`, `val`, `box`, `tag`) prove at compile time that no two actors
can write one object, and messages are the only sharing (Clebsch, Drossopoulou, Blessing, McNeil,
"Deny Capabilities for Safe, Fast Actors", AGERE 2015;
[tutorial](https://tutorial.ponylang.io/reference-capabilities/)). Data-race
freedom and deadlock freedom are theorems. What it cannot say is "these two
objects change together": a transfer between two actors' states is a protocol
of messages, which is the alternative Whitefoot refused (shared-objects,
rejected "shards each owned by one context and reached by messages": two
hand-offs per command and a protocol for two keys). Whitefoot's handle is a
`tag`-like capability outside the block and a `ref` inside it, which is what
the statement's binding scope expresses.

### 3.8 Cilk reducers and Erlang

Cilk++ hyperobjects ([Frigo, Halpern, Leiserson, Lewin-Berlin, SPAA 2009](https://doi.org/10.1145/1583991.1584017))
give every strand its own view of a reducer, combined at joins: the sharded
state for a count, with the failure that a non-associative combine makes the
result depend on the schedule. Whitefoot's shards never combine: a whole
target reads all shards at one point, and the total is the program's
arithmetic (PR #385 draft, "no implementation step combines or adjusts
shards"). Erlang's `receive ... after T` ([reference manual](https://www.erlang.org/doc/reference_manual/expressions#receive))
is D1 in a message-passing world; its failures are an unbounded mailbox and
a cycle of synchronous calls between processes, which no type prevents.

### 3.9 Linux `lockdep`

The kernel checks lock order at run time by recording every order of classes
it has seen and reporting the first cycle ([lockdep design](https://docs.kernel.org/locking/lockdep-design.html)).
That is the dynamic form of R-order. Whitefoot needs no record because the
order is fixed by types and identities, and because no block contains a
statement (R-waits): a cycle has nowhere to form.

### 3.10 What Whitefoot's checking does that none of them can

| property | Go | Rust | C++ (+Clang TSA) | Java | STM | Ada | Whitefoot |
|---|---|---|---|---|---|---|---|
| data reachable only under its hold | no | yes (`Mutex<T>`) | annotated | no | yes | yes | yes: state has no path but through a target |
| every held object named at one place | no | no | partly | no | n/a | one object | yes: the header |
| no nested acquisition, hence no cycles | no | no | no | no | n/a | no (nested calls) | yes: R-waits refuses a statement in a block |
| several objects at one point | manual | manual | `scoped_lock` (dynamic) | nested | yes | no | yes, in one static order |
| conditional hold checked | no | `Option<Guard>` (order unchecked) | refused by TSA | no | n/a | no | this study |
| guard as a condition the block may use as a fact | no | no | no | `wait` loop | `retry` | barrier | yes ([ENT-3.S1]) |
| read-only hold by authority, not by lock kind | `RLock` | `RwLock` read guard | annotated | no | n/a | functions vs procedures | `SharedRead` follows aliases |
| timed wait | `select` | no | `try_lock_for` | `wait(long)` | no | `or delay` | section 2.4 |
| one operation per point on a lock-free value | no | atomics unrelated | atomics unrelated | atomics unrelated | n/a | no | the atomic value's count |

The last two rows are the pending ones; the others are what any new form
must keep.

## 4. The design principle: four questions, each answered once

**Deduction** from section 1's table. Today every difference between target
forms is already carried by the place after `&`:

1. **Which object and with what authority**: the handle place's type,
   `Shared<T>` or `SharedRead<T>`.
2. **What kind of state**: the type argument, an ordinary `T`, a
   `ConcurrentHashMap<V>`, and, pending, a `Shards<T>` or an `Atomic<u64>`.
3. **How much of it**: at most one index step, and the *type of its atom*
   selects the extent: `&[u8]` one entry, `KeySet` several entries,
   `AnyShard` one shard, no index the whole.

The statement adds the binding name and the wait clause. So the one grammar
that already describes every form is:

```
atomic_stmt := "atomic" target ("," target)* wait? "{" stmt* "}"
target      := IDENT "=" "&" place
wait        := "when" expr
```

and the single rule for the place is a table keyed by (state type, index atom
type). What is missing, and what each option below adds in a different
place, is the fourth question:

4. **Whether the object is held on this execution.**

Three further rules follow the principle and change in no option:

- **What the block may do** is a property of the state type and the handle
  type, never of the statement: read-only through `SharedRead`; a shared
  entry hold when the block writes nothing through the entry; one operation
  per path on an atomic value; a guard reads no shard. A per-target modifier
  such as `read` would say in a second place what the handle type and the
  footprint already say, and is refused on that ground (two spellings of
  one fact; the checker already computes the footprint).
- **The lock order is the implementation's**, never spelled (shared-objects,
  rejected "lock levels declared in shared-store types"); whichever option
  is chosen must keep every acquisition inside the order without the writer
  naming it.
- **The one point** [SHARE-3] stays the whole meaning: every option must make
  "held on this execution" a function of values the statement reads when it
  begins, or else prove that a later acquisition cannot form a cycle.

## 5. Options

Each option states its grammar, its checking rules (how the use of a
conditional target is proved, how the order and freedom from deadlock are
kept, how the atomic value's one operation is counted, how guards interact),
its lowering with the cost when a conditional target is not held, how every
existing and pending form is written, migration, and failure modes.

### 5.1 Option A: a per-target condition in the header

**Grammar.**

```
atomic_stmt := "atomic" target ("," target)* wait? "{" stmt* "}" ("else" "{" stmt* "}")?
target      := IDENT "=" "&" place ("if" IDENT)?
wait        := "when" expr ("until" atom)?
```

The `else` block is admitted only with `until`, and `until` only with `when`.
Firn's sketch is this grammar's `if`; the deadline is section 2.4's.

**Meaning.** A target with `if c` is a *conditional target* and `c` its
condition. When the statement begins it reads, in written order, every handle
place, index atom and condition. Its *held targets* on that execution are the
unconditional targets and the conditional targets whose condition read
`True()`. [SHARE-3] then reads: the guard and block execute with exclusive
access to the states its *held* targets name. Nothing else about the point,
the order or progress changes.

**Checking rules** (each would be one sentence of SHARE-2):

- **A1, the condition.** `c` is an IDENT naming an ordinary `let` binding of
  the enclosing function body, of exact mode `own Bool`, in scope at the
  statement, that no `set_stmt` in the function body rebinds, reachable or
  not. (The same "no `set` rebinds it" condition that makes a path exact
  ([ENT-2]); it makes `c`'s value the same at the statement's beginning and at
  every `if c` inside the block.) Any other condition expression is a hard
  error citing SHARE-2 at that IDENT, with a repair naming the `let` to
  introduce.
- **A2, scope of the binding.** The binding of a conditional target is in
  scope exactly in the then-blocks of `if_stmt`s and `value_if`s inside the
  atomic block whose condition `expr` is the bare IDENT `c` (the same
  binding), at any nesting depth, and nowhere else: not in the guard, not in
  an `else`, not under `bnot(c)`, not under `band(c, x)`. Every other
  occurrence of the IDENT is a hard error citing SHARE-2 at that `place`,
  with the repair "enclose in `if c { }`". This is a scoping rule in the
  manner of a `match` arm's `fieldbind` binders: a name visible in one
  lexical region, decided by syntax alone, with no fact channel. Call it
  **A-lex**.
- **A3, named.** R-names holds for a conditional target over its scope: a
  conditional binding named nowhere in its then-blocks is refused as an
  unused target.
- **A4, overlap.** R-overlap is unchanged; a conditional target and an
  unconditional one may name one object, which the statement holds once
  when both are held. A conditional whole target still stands alone on its
  handle place.
- **A5, atomic value.** V1 to V6 unchanged, with V2 counting conditional
  atomic-value bindings together with the others: at most one operation on
  any path, inside or outside an `if c`.
- **A6, guard.** The guard names no conditional binding (A2). With `until
  d`, `d` is an atom of type `std::time::Instant` read when the statement
  begins; the statement takes effect either at a point where its guard is
  true, executing its block, or at a point after the deadline is reached
  where its guard is false, executing no block and then its `else` block in
  the ordinary context with nothing held. Guard goals are established at the
  block's entry as today ([ENT-3.S1]) and nothing is established at the
  `else` block's entry, since the states are no longer held there. A
  statement with a deadline completes; without one, [WAIT-2]'s progress
  sentence applies as today.
- **A7, type invariants.** [TYPE-11] establishes a conditional target's
  invariants at the atomic block's entry and owes them at every leaving edge,
  as for an unconditional target. On a path where `c` is false no write
  through the binding exists, so the owed invariant is the established one
  and discharges by the ordinary flow; on a path where `c` is true the rule
  is today's. (To verify in implementation: that establishing a fact over an
  out-of-scope binding's path is harmless to the fact machinery; otherwise
  establish it at each `if c` then-block's entry instead.)
- **A8, footprint.** The statement's footprint gains the condition atoms,
  read.

**Why A-lex is enough, and when A-fact would be wanted.** Firn's sites test
the condition with `if held` already (section 2.1), so A-lex serves them
verbatim. The semantic alternative, **A-fact**, admits a use wherever the
fact state proves `c` true, so that `if band(held, stored) { ... m ... }`
would pass by signed decomposition ([ENT-3]). It needs a Bool datum
to be a goal, which today it is only when its initializer is an operation row
([ENT-3], the goal-origin definitions): `logged = store^.logging` is a field read and would establish
nothing, so Firn's `if logged { ... m ... }` with `m = &meta if logged` would
be refused under A-fact unless a new "Bool datum truth" fact is added. A-lex
is the smaller, deterministic rule; A-fact can replace it later without
changing any program A-lex accepts.

**Lock order and deadlock freedom.** **Deduction.** The held set is fixed
before the first acquisition. R-order takes the held targets' groups in the
static type order, lazily by first use, each group with every earlier untaken
group; a conditional target not held is simply absent from the set. Every
wait is therefore still for a lock later in the order than every lock held,
and no cycle forms. No order rule enters the source. This is the property
that B and C lack: because the set is decided at begin, a conditional target
may be of *any* type, earlier or later than the others.

**Lowering.** The frame already keeps a taken word for lazy groups
(state-locks, tenth decision). A conditional target's group is marked
"absent" in that word when `c` read false, before the first acquisition; a
group marked absent is skipped by the taking sequence, by the release
sequence and by the handle-count sequence. On an execution whose condition is
false the statement therefore performs, for that target: one local read of
`c`, one local store to the taken word, and nothing else: **no shared write,
no lock word, no handle count, no watch** (F4). When `c` is true the lowering
is today's. For Firn, with `Meta` a source nominal ordered after the prelude
map type, the entry is taken before the guard and `meta` at its first use
inside `if held`, exactly as the two-statement form's then-branch does today.
With the `until` clause: a guard that reads false registers its watches and
parks as today (state-locks, fifth decision), with the deadline entered in
the driver's heap (bounded-waits, first decision); when the heap wakes it, it
re-takes its targets, evaluates the guard once more under the hold, and
either takes effect normally or, the guard being false after the deadline,
releases and continues at the `else` block.

**How every form is written under A.** The table of section 1 is unchanged;
the two pending rows join it; the condition and the deadline are orthogonal
to every row:

| need | written |
|---|---|
| whole object, exclusive | `s = &h` |
| whole object, read-only | `s = &hr` with `hr: SharedRead<T>` |
| one entry | `e = &h[k]` |
| entries under a set | `es = &h[ks]` |
| whole map | `t = &h` |
| one shard | `p = &h[mine]` with `mine: AnyShard` |
| all shards | `all = &h` with `h: Shared<Shards<T>>` |
| atomic value | `s = &seq` with `seq: Shared<Atomic<u64>>`; one operation in the block |
| held only when `held` | `m = &h if held`, used inside `if held { }` |
| Firn's AOF sequence, off at no cost | `s = &seq if logging` and `atomic_add` inside `if logging { }`, or `s = &seq` with the operation inside `if logging { }` as the sequence-take study wrote it; both perform nothing when off |
| guard wait | `when g` |
| guard wait with a deadline | `when g until d { } else { }` |

**Migration.** None for existing programs: the grammar is a superset and
every rule for unconditional targets is unchanged. Firn's 40 sites follow one
mechanical pattern (section 6.1): drop the outer `if`/`else`, add `if held`
to the `meta` target, wrap each use of `m` in `if held { }`, and replace the
two `sequence` initializers by one `value_if`.

**Failure modes.**

- A use of `m` outside `if held` is refused (F2): the writer cannot forget
  the condition.
- A condition computed from shared state read in an earlier statement may be
  stale when this statement begins; that is the program's logic, and today's
  two-statement form has the same property. The design does not try to make
  the condition a fact about the held state; a condition that must hold at
  the point is a guard.
- A writer who wants `m` under `if bor(held, other)` must restructure: A-lex
  admits only the bare IDENT. The repair names it.
- Two conditional targets whose conditions are different IDENTs for one
  handle place: refused by R-overlap for whole targets; admitted for entries,
  each held when its own condition holds.
- The owner asked that no statement pay for a feature it does not use: a
  statement without conditional targets lowers as today, byte for byte,
  since the "absent" mark is set only by a conditional target.

### 5.2 Option B: hold extension inside the block

**Grammar.** The header is today's; a new statement is admitted only inside
an atomic block or a hold block:

```
hold_stmt := "hold" IDENT "=" "&" place "{" stmt* "}"
```

**Meaning.** A `hold` extends the enclosing statement's held targets with its
target from the hold's beginning until the enclosing atomic statement
completes (two-phase: nothing is released before the block ends). The hold's
binding is in scope in its block. Whether to hold is written as ordinary
control flow around the `hold`: `if held { hold m = &store^.meta { ... } }`.

**Checking rules.**

- **B1, order.** The hold's state type must be strictly later, in the static
  type order of R-order, than the state type of every target of the
  enclosing atomic statement and of every enclosing hold. Two targets of one
  type cannot be ordered statically (identity order is at run time), so a
  hold of a type already held is refused. The repair: move the target to the
  header. This is the one place where the implementation's order becomes
  visible to the writer, as a refusal.
- **B2, place.** The hold's handle place and index atom are read when the
  hold begins and may be paths through an enclosing binding (a handle found
  inside held state), since B1 already orders the new object after every
  held one.
- **B3, no guard, no wait.** A hold has no guard: a guard that reads false
  would have to release everything and re-run a block that has already
  executed part of its effect, which the once-run block forbids.
- **B4, atomic value.** V3 ("after the last acquisition") becomes a static
  check: on every path, no `hold` begins after an atomic-value operation.
- **B5, scope.** The binding leaves scope when the hold block ends; the
  object stays held until the statement completes.

**Deadlock freedom.** **Deduction.** Every acquisition of a hold is of a type
later than every type held, by B1, and groups of one type are taken together
in the header; so every wait is for a later lock. The property holds without
reading any other function.

**Lowering.** A hold is the lazy group taking of today, made explicit; the
taken word records it; release is at the statement's end. Cost when not
reached: none. Handle count: as PR #409, per hold.

**How the forms are written.** The header rows of section 1 and the pending
rows unchanged. Firn's condition: `if held { hold m = &store^.meta { ... } }`;
a conditional atomic value: `if logging { hold s = &seq { ... atomic_add ... } }`.
The deadline: as in A, on the header's `when`.

**Migration.** None; additive.

**Failure modes.**

- B1 refuses a conditional target whose type precedes a header target's: a
  conditional `Shared<u64>` beside a map entry (primitives order first) cannot
  be held by a `hold`; the writer must put it in the header and pay its lock
  on every execution, or split the statement. Firn's `Meta` happens to be
  later than the map; a future Firn state that is a prelude type would not
  be.
- The writer sees the type order as a refusal whose reason ("an
  implementation detail") the design tree deliberately keeps out of the
  source.
- A body that uses `m` before and after the hold must enter the hold once
  and keep the whole tail inside it, which reintroduces nesting around the
  common body (Firn's `set_body` call would sit inside the hold in one branch
  and outside it in another, or the hold would have to begin before it).
- Its unique power is also its risk: holding an object found in held state
  makes the held set depend on data, so two statements on one key can hold
  different second objects, which is sound by B1 but harder to read.

### 5.3 Option C: no syntax, elision by rule

**Grammar.** Unchanged.

**Meaning.** Today's lowering already takes a later-ordered group at its first
use on the path (state-locks, second decision: "a target reached only on a
conditional tail, firn's log after a key's entry, is taken only on the path
that reaches it"). Option C makes that a language guarantee: SHARE-3 would
say that a statement holds a target from the first point on its path that
names the target's binding, when the target's state type follows, in the
implementation's order, every state type the path has used before.

**Checking rules.** None new. Firn writes `m = &store^.meta` unconditionally
and keeps every use under `if held`.

**Deadlock freedom.** As today.

**Lowering.** As today; cost when the tail is not reached: none.

**Why it is not the design, stated honestly.**

- It fails F2: nothing refuses a use of `m` outside `if held`; a stray
  `aof_refuse(meta: m)` at the top of the block, which is exactly what Firn's
  one-statement form had, silently takes the lock on every execution and
  costs the 17 times of section 2.1 again. The property Firn needs is a
  contract, and C gives an optimization.
- To state the guarantee, the specification would have to name the lock
  order or at least "the implementation's order", which the design tree keeps
  out of the language; without stating it, the guarantee cannot be written
  as a rule a conformance case tests.
- It has B's type-order hole: a conditional target of an earlier type is
  taken with the first later group whether or not the path reaches it.
- What it is good for: **Q0** of section 8. Because C is today's behavior,
  Firn's SET rewritten as one statement under today's compiler is a
  measurement that costs no language change and tells whether A's lowering
  will meet 21.4 M/s before A is built.

### 5.4 Option D: an optional target shaped as data

**Grammar.** `target := IDENT "=" "&" place ("if" IDENT)?` as in A, but the
binding's type differs.

**Meaning.** The binding of a conditional whole target is `&Option<T>`: `Some`
with the state as its payload when held, the shared immutable `None` when not
(as an absent map entry reads today, state-locks eighth decision). The block
matches:

```
atomic slot = &store^.keys[key], m = &store^.meta if held {
  match m^ {
    Some(value: meta) => {
      ...
    }
    None() => {
    }
  }
}
```

**Checking rules.** No new scoping rule and no new fact: payload selection
needs a current [ENT-3.S15] fact, so a use of the state where `m^` is not
known `Some` is refused by today's rule (F2 by existing machinery). A write of
`m^` whole is refused (the object cannot be made to appear or vanish). For an
entry target the binding becomes `&Option<Option<V>>`; for a key-set target
`&Option<Entries<V>>`; for an atomic value `&Option<Atomic<u64>>`, whose
operation sits inside the `Some` arm, which keeps V2's count.

**Lowering.** The held set is decided at begin as in A. But a reference of
kind `&Option<T>` whose payload is a shared object's state has no cell to
point at: the state is stored as a `T`, not as an `Option<T>`, so the binding
needs a representation of its own (a tag and a pointer, as `Entries<V>` has
its own 24-byte representation, [OP-9]), and payload selection through it
must yield the state's address. Such a binding cannot be passed to a callee
expecting an ordinary `&Option<T>`, so it needs either a prelude opaque type
(`Held<T>` with the one operation "select") or a rule refusing its passing.

**Failure modes.** The condition's proof is by `match`, which Firn's bodies
do not have (they test `if held`), so every site gains a match and its empty
`None` arm; nested `Option`s for entries read badly; the special
representation is a compiler cost for a convenience that A-lex gives with a
scoping rule. It is the Rust idiom of section 3.2 made static, and it shows
that A and D differ only in where the proof comes from: syntax (A-lex) or
data (D).

### 5.5 Variants considered and set aside

- **A condition on the statement rather than the target** (`atomic ... with
  held: m = &store^.meta`): the same as A with a different spelling; A's
  per-target `if` lets two targets carry two conditions.
- **A runtime set of handles as one target** (`hs = &h[set]` with a set of
  `Shared<T>` built before the statement, binding indexable like
  `Entries<V>`): a general "hold these N objects of one type", of which a
  conditional target is the set of zero or one. It is section 1's key-set
  row generalized from entries to objects, sound by R-order's identity
  sorting. No current program needs N objects of one type chosen at run time
  (a transfer between two accounts is two targets today); recorded as a
  possible later row, not an option for Firn, whose condition would become
  `if hs^.len == 1_u64 { ... hs^[0_u64] ... }`.
- **Making `held` a fact about the held state** (a guard-like condition
  evaluated under the hold that decides what else to hold): refused, because
  it is an acquisition decided after the block has read state, which R-order
  admits only for later types (that is option B), and because a condition
  the writer wants to hold at the point is already a guard.

## 6. Before and after, as complete programs

Every program below is written in the accepted grammar as the conformance
cases spell it (call arguments, operands and conversion operands are atoms;
nested calls are bound by `let`; no comments), reduced to the shape that
shows the form. None was compiled: the "after" programs use syntax and
prelude rows that do not exist yet (`if` on a target, `until`/`else`,
`Shards`, `Atomic`), and the "before" programs are sketches checked by
reading against the grammar, not by the compiler. Section 8 registers the
cases that will compile them.

### 6.1 Firn's SET: hold the write state only when logging or timed

The reduced keyspace: a map of entries, a write state `Meta` with a commit
sequence and two counters standing in for the log and the expiry queue, and
the `logging` flag beside the handles, as Firn keeps it.

**Before** (Firn-wf PR #65: two statements around one body).

```whitefoot
const names: Array<u8, 1> =[107_u8];

struct Entry {
  value: u64;
  expires: u64;
}

struct Meta {
  commit_seq: u64;
  appended: u64;
  queued: u64;
}

struct Keyspace {
  keys: Shared<ConcurrentHashMap<Entry>>;
  meta: Shared<Meta>;
  logging: Bool;
}

fn set_body(slot: &Option<Entry>, value: u64, at: u64, timed: Bool, only_fresh: Bool) -> stored: Bool writes(slot) {
  let found = False();
  match slot^ {
    Some(value: unused) => {
      set found = True();
    }
    None() => {
    }
  }
  let blocked = band(only_fresh, found);
  if blocked {
    return False();
  }
  let expires = 0_u64;
  if timed {
    set expires = at;
  }
  let entry = Entry(value: value, expires: expires);
  set slot^ = Some<Entry>(value: entry);
  return True();
}

fn set_key(store: &Keyspace, key: &[u8], value: u64, at: u64, timed: Bool, only_fresh: Bool) -> stored: Bool reads(store), reads(key) waits {
  let logged = store^.logging;
  let held = bor(logged, timed);
  let stored = False();
  if held {
    atomic slot = &store^.keys[key], m = &store^.meta {
      let sequence = m^.commit_seq;
      set stored = set_body(slot: slot, value: value, at: at, timed: timed, only_fresh: only_fresh);
      if stored {
        if timed {
          set m^.queued = m^.queued +wrap 1_u64;
        }
        if logged {
          set m^.appended = m^.appended +wrap 1_u64;
        }
      }
      set m^.commit_seq = sequence +wrap 1_u64;
    }
  } else {
    atomic slot = &store^.keys[key] {
      set stored = set_body(slot: slot, value: value, at: at, timed: timed, only_fresh: only_fresh);
    }
  }
  return stored;
}

fn keyspace_share(store: &Keyspace) -> made: Keyspace reads(store) {
  let keys = shared_share::<ConcurrentHashMap<Entry>>(shared: &store^.keys);
  let meta = shared_share::<Meta>(shared: &store^.meta);
  return Keyspace(keys: move keys, meta: move meta, logging: store^.logging);
}

fn writer(store: Keyspace, rounds: u64) -> stored: u64 pure waits {
  let key = &names[0_u64..1_u64];
  let fresh = False();
  let count = 0_u64;
  for @round (i in 0_u64..rounds) {
    let timed = i == 0_u64;
    let done = set_key(store: &store, key: key, value: i, at: 5_u64, timed: timed, only_fresh: fresh);
    if done {
      set count = count +wrap 1_u64;
    }
  }
  return count;
}

fn run_writers(store: &Keyspace) -> result: unit reads(store) waits {
  for @start (index in 0_u64..4_u64) {
    let own = keyspace_share(store: store);
    spawn writer(store: move own, rounds: 8_u64);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let keys = shared_map_new::<Entry>(capacity: 8_u64);
  let zero = Meta(commit_seq: 0_u64, appended: 0_u64, queued: 0_u64);
  let meta = shared_new::<Meta>(value: zero);
  let off = False();
  let store = Keyspace(keys: move keys, meta: move meta, logging: off);
  run_writers(store: &store);
  let commits = 0_u64;
  let queued = 0_u64;
  atomic m = &store.meta {
    set commits = m^.commit_seq;
    set queued = m^.queued;
  }
  if commits != queued {
    return std::process::exit_status(code: 1_u8);
  }
  let code = cvt.wrap::<u64, u8>(commits);
  return std::process::exit_status(code: code);
}
```

With logging off, each writer's first round is timed and holds `meta`; the
other seven rounds hold only the entry. The program exits with 4: four
commits, four queued expiries. The expected outcome is the same for every
form below; the forms differ only in `set_key`.

**After, option A** (one statement, one body; `m` in scope only inside
`if held`):

```whitefoot
fn set_key(store: &Keyspace, key: &[u8], value: u64, at: u64, timed: Bool, only_fresh: Bool) -> stored: Bool reads(store), reads(key) waits {
  let logged = store^.logging;
  let held = bor(logged, timed);
  let stored = False();
  atomic slot = &store^.keys[key], m = &store^.meta if held {
    let sequence = if held {
      give m^.commit_seq;
    } else {
      give 0_u64;
    }
    set stored = set_body(slot: slot, value: value, at: at, timed: timed, only_fresh: only_fresh);
    if held {
      if stored {
        if timed {
          set m^.queued = m^.queued +wrap 1_u64;
        }
        if logged {
          set m^.appended = m^.appended +wrap 1_u64;
        }
      }
      set m^.commit_seq = sequence +wrap 1_u64;
    }
  }
  return stored;
}
```

A use of `m` outside the two `if held` blocks, such as `let sequence =
m^.commit_seq;` at the top, is refused by A2 with the repair "enclose in
`if held { }`". On a round where `held` is false the statement takes the
entry's lock only, exactly what the `else` branch of the "before" takes.

**After, option B** (header unchanged; a `hold` under `if held`). The
sequence is read before `set_body` and written after it, so the hold must
enclose the body, and the body's call appears in both branches:

```whitefoot
fn set_key(store: &Keyspace, key: &[u8], value: u64, at: u64, timed: Bool, only_fresh: Bool) -> stored: Bool reads(store), reads(key) waits {
  let logged = store^.logging;
  let held = bor(logged, timed);
  let stored = False();
  atomic slot = &store^.keys[key] {
    if held {
      hold m = &store^.meta {
        let sequence = m^.commit_seq;
        set stored = set_body(slot: slot, value: value, at: at, timed: timed, only_fresh: only_fresh);
        if stored {
          if timed {
            set m^.queued = m^.queued +wrap 1_u64;
          }
          if logged {
            set m^.appended = m^.appended +wrap 1_u64;
          }
        }
        set m^.commit_seq = sequence +wrap 1_u64;
      }
    } else {
      set stored = set_body(slot: slot, value: value, at: at, timed: timed, only_fresh: only_fresh);
    }
  }
  return stored;
}
```

The `hold` is admitted because `Meta`, a source nominal, follows
`ConcurrentHashMap<Entry>`, a prelude nominal, in the type order (B1). Firn's
real body (section 2.1) reads `m` first (`aof_refuse`) and last
(`sequence_commit`), so under B the hold encloses the whole body and the
`else` branch repeats `set_body` with an inert sequence: one body in the
sense of one function, two call sites, which is the shell Firn wanted gone.

**Under option C** `set_key` is the A form with `m = &store^.meta`
unconditional. It compiles today; whether it runs at the "before" form's
speed was Q0 of section 8, measured in 8.2 as within the twin's spread.

### 6.2 The sequence take: a number at the statement's point, nothing when logging is off

**Before** (today: an exclusive `Shared<u64>` beside the entry). Note what
R-order does here: `u64` is a primitive, ordered before the prelude map
type, so the sequence's lock is taken together with the entry's before the
guard on *every* round, logging on or off, although `s` is named only under
`if logging`. This is the type-order hole of options B and C shown on a real
program.

```whitefoot
const names: Array<u8, 1> =[107_u8];

fn write(table: Shared<ConcurrentHashMap<u64>>, seq: Shared<u64>, rounds: u64, logging: Bool) -> last: u64 pure waits {
  let key = &names[0_u64..1_u64];
  let last = 0_u64;
  for @round (i in 0_u64..rounds) {
    atomic slot = &table[key], s = &seq {
      set slot^ = Some<u64>(value: i);
      if logging {
        set last = s^;
        set s^ = s^ +wrap 1_u64;
      }
    }
  }
  return last;
}

fn write_four(table: &Shared<ConcurrentHashMap<u64>>, seq: &Shared<u64>) -> result: unit reads(table), reads(seq) waits {
  let on = True();
  for @start (index in 0_u64..4_u64) {
    let own_table = shared_share::<ConcurrentHashMap<u64>>(shared: table);
    let own_seq = shared_share::<u64>(shared: seq);
    spawn write(table: move own_table, seq: move own_seq, rounds: 8_u64, logging: on);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = shared_map_new::<u64>(capacity: 1_u64);
  let seq = shared_new::<u64>(value: 0_u64);
  write_four(table: &table, seq: &seq);
  let taken = 0_u64;
  atomic s = &seq {
    set taken = s^;
  }
  let code = cvt.wrap::<u64, u8>(taken);
  return std::process::exit_status(code: code);
}
```

**After** (the owner's atomic value as a whole target, conditional under A so
that a round with logging off neither holds nor touches it; the operation is
one hardware add at the statement's point, after the entry is held):

```whitefoot
const names: Array<u8, 1> =[107_u8];

fn write(table: Shared<ConcurrentHashMap<u64>>, seq: Shared<Atomic<u64>>, rounds: u64, logging: Bool) -> last: Option<u64> pure waits {
  let key = &names[0_u64..1_u64];
  let last = None<u64>();
  for @round (i in 0_u64..rounds) {
    atomic slot = &table[key], s = &seq if logging {
      set slot^ = Some<u64>(value: i);
      if logging {
        set last = atomic_add(cell: s, value: 1_u64);
      }
    }
  }
  return last;
}

fn write_four(table: &Shared<ConcurrentHashMap<u64>>, seq: &Shared<Atomic<u64>>) -> result: unit reads(table), reads(seq) waits {
  let on = True();
  for @start (index in 0_u64..4_u64) {
    let own_table = shared_share::<ConcurrentHashMap<u64>>(shared: table);
    let own_seq = shared_share::<Atomic<u64>>(shared: seq);
    spawn write(table: move own_table, seq: move own_seq, rounds: 8_u64, logging: on);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = shared_map_new::<u64>(capacity: 1_u64);
  let seq = shared_atomic_new(value: 0_u64);
  write_four(table: &table, seq: &seq);
  let taken = 0_u64;
  atomic s = &seq {
    set taken = atomic_load(cell: s);
  }
  let code = cvt.wrap::<u64, u8>(taken);
  return std::process::exit_status(code: code);
}
```

Both exit with 32. Under the sequence-take study's own form, `s = &seq`
without `if logging`, the program is the same except the header: an atomic
value held unconditionally still performs no step on a path with no
operation. The condition adds nothing for an atomic value except making the
header say what the block does; for an ordinary object it is what removes
the lock.

### 6.3 A shard update and a total

**Before** (one shared object; every context's update waits on one lock, the
counter study's `w_shared` row, 9.6 falling to 0.7 million a second from one
to eight cores, run 38304337014 cited there):

```whitefoot
struct Stats {
  commands: u64;
  peak: u64;
}

fn after_command(stats: Shared<Stats>, rounds: u64) -> result: unit pure waits {
  for @round (i in 0_u64..rounds) {
    atomic s = &stats {
      set s^.commands = s^.commands +wrap 1_u64;
      set s^.peak = imax(s^.peak, i);
    }
  }
  return unit;
}

fn run_four(stats: &Shared<Stats>) -> result: unit reads(stats) waits {
  for @start (index in 0_u64..4_u64) {
    let own = shared_share::<Stats>(shared: stats);
    spawn after_command(stats: move own, rounds: 8_u64);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let zero = Stats(commands: 0_u64, peak: 0_u64);
  let stats = shared_new::<Stats>(value: zero);
  run_four(stats: &stats);
  let total = 0_u64;
  atomic s = &stats {
    set total = s^.commands;
  }
  let code = cvt.wrap::<u64, u8>(total);
  return std::process::exit_status(code: code);
}
```

**After** (PR #385's draft: the shard selector is one more index-atom row;
the whole target reads every shard at one point; nothing in the statement
changes but the place):

```whitefoot
struct Stats {
  commands: u64;
  peak: u64;
}

fn no_stats() -> made: Stats pure {
  return Stats(commands: 0_u64, peak: 0_u64);
}

fn after_command(stats: Shared<Shards<Stats>>, rounds: u64) -> result: unit pure waits {
  let mine = any_shard();
  for @round (i in 0_u64..rounds) {
    atomic s = &stats[mine] {
      set s^.commands = s^.commands +wrap 1_u64;
      set s^.peak = imax(s^.peak, i);
    }
  }
  return unit;
}

fn run_four(stats: &Shared<Shards<Stats>>) -> result: unit reads(stats) waits {
  for @start (index in 0_u64..4_u64) {
    let own = shared_share::<Shards<Stats>>(shared: stats);
    spawn after_command(stats: move own, rounds: 8_u64);
  }
  return unit;
}

fn stats_total(stats: &Shared<Shards<Stats>>) -> total: Stats reads(stats) waits {
  let sum = Stats(commands: 0_u64, peak: 0_u64);
  atomic all = &stats^ {
    for @shard (i in 0_u64..all^.len) {
      set sum.commands = sum.commands +wrap all^[i].commands;
      set sum.peak = imax(sum.peak, all^[i].peak);
    }
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let stats = shared_shards_new::<Stats, fn no_stats>();
  run_four(stats: &stats);
  let sum = stats_total(stats: &stats);
  let code = cvt.wrap::<u64, u8>(sum.commands);
  return std::process::exit_status(code: code);
}
```

Both exit with 32. A shard target beside a conditional target, as Firn's
write with per-driver AOF parts would have it, is
`slot = &keys^[k], p = &parts^[mine] if logging, s = &seq if logging`: three
rows of the one table, two of them conditional on one atom.

### 6.4 A guard wait with a deadline: the append-only writer

**Before** (the board item's polling form: sleep 10 ms, then look). The
producer appends a byte three times, 20 ms apart, then sets `stopping`.

```whitefoot
alias ExitStatus = std::process::ExitStatus;
alias Inputs = std::process::Inputs;
alias Clock = std::time::Clock;

struct Pending {
  bytes: Box<Slots<u8>>;
  stopping: Bool;
}

fn produce(pending: Shared<Pending>, clock: Clock) -> result: unit pure waits {
  for @round (i in 0_u64..3_u64) {
    let started = std::time::now(clock: &clock);
    let wake = std::time::instant_after(instant: started, nanoseconds: 20000000_u64);
    let never = std::time::cancel_never();
    std::time::sleep_until(deadline: wake, cancel: &never);
    std::time::close_cancel_watch(watch: move never);
    atomic current = &pending {
      if current^.bytes.inner.len < current^.bytes.inner.cap {
        place_back(window: &current^.bytes.inner, value: 7_u8);
      }
    }
  }
  atomic current = &pending {
    set current^.stopping = True();
  }
  return unit;
}

fn write_log(pending: Shared<Pending>, clock: Clock) -> drained: u64 pure waits {
  let drained = 0_u64;
  loop @writing {
    let started = std::time::now(clock: &clock);
    let wake = std::time::instant_after(instant: started, nanoseconds: 10000000_u64);
    let never = std::time::cancel_never();
    std::time::sleep_until(deadline: wake, cancel: &never);
    std::time::close_cancel_watch(watch: move never);
    let stop = False();
    let fresh = box_slots_new::<u8>(capacity: 64_u64);
    atomic current = &pending {
      set stop = current^.stopping;
      swap(first: &current^.bytes, second: &fresh);
    }
    set drained = drained +wrap fresh.inner.len;
    if stop {
      break @writing;
    }
  }
  return drained;
}

fn main(inputs: Inputs) -> status: ExitStatus pure waits {
  let Inputs(args: unused_args, drivers: unused_driver_count, cwd: cwd_directory, stdout: unused_stdout, stderr: unused_stderr, handles: handles, stdin: unused_stdin, clock: clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: unused_host_paths) = move inputs;
  let std::fs::Directory(read: cwd, write: cwd_write) = move cwd_directory;
  std::fs::close_directory_write(factory: &handles, directory: move cwd_write);
  std::fs::close_directory(factory: &handles, directory: move cwd);
  let empty = box_slots_new::<u8>(capacity: 64_u64);
  let running = False();
  let state = Pending(bytes: move empty, stopping: running);
  let pending = shared_new::<Pending>(value: move state);
  let producer_handle = shared_share::<Pending>(shared: &pending);
  let producer_clock = std::time::clock_share(clock: &clock);
  spawn produce(pending: move producer_handle, clock: move producer_clock);
  let drained = write_log(pending: move pending, clock: move clock);
  let code = cvt.wrap::<u64, u8>(drained);
  return std::process::exit_status(code: code);
}
```

The writer wakes about a hundred times a second while idle and sees a byte
up to 10 ms after it was appended. Exit code 3.

**After** (the wait clause `when g until d { } else { }` of option A: the
guard is woken by the producer's write, the deadline by the driver's timer
heap; the guard is a call because its two conditions are not one atom):

```whitefoot
alias ExitStatus = std::process::ExitStatus;
alias Inputs = std::process::Inputs;
alias Clock = std::time::Clock;

struct Pending {
  bytes: Box<Slots<u8>>;
  stopping: Bool;
}

fn ready(pending: &Pending) -> result: Bool reads(pending) {
  let filled = pending^.bytes.inner.len > 0_u64;
  return bor(filled, pending^.stopping);
}

fn produce(pending: Shared<Pending>, clock: Clock) -> result: unit pure waits {
  for @round (i in 0_u64..3_u64) {
    let started = std::time::now(clock: &clock);
    let wake = std::time::instant_after(instant: started, nanoseconds: 20000000_u64);
    let never = std::time::cancel_never();
    std::time::sleep_until(deadline: wake, cancel: &never);
    std::time::close_cancel_watch(watch: move never);
    atomic current = &pending {
      if current^.bytes.inner.len < current^.bytes.inner.cap {
        place_back(window: &current^.bytes.inner, value: 7_u8);
      }
    }
  }
  atomic current = &pending {
    set current^.stopping = True();
  }
  return unit;
}

fn write_log(pending: Shared<Pending>, clock: Clock) -> drained: u64 pure waits {
  let drained = 0_u64;
  let last_sync = std::time::now(clock: &clock);
  loop @writing {
    let limit = std::time::instant_after(instant: last_sync, nanoseconds: 1000000000_u64);
    let stop = False();
    let timed_out = False();
    let fresh = box_slots_new::<u8>(capacity: 64_u64);
    atomic current = &pending when ready(pending: current) until limit {
      set stop = current^.stopping;
      swap(first: &current^.bytes, second: &fresh);
    } else {
      set timed_out = True();
    }
    set drained = drained +wrap fresh.inner.len;
    if timed_out {
      set last_sync = std::time::now(clock: &clock);
    }
    if stop {
      break @writing;
    }
  }
  return drained;
}

fn main(inputs: Inputs) -> status: ExitStatus pure waits {
  let Inputs(args: unused_args, drivers: unused_driver_count, cwd: cwd_directory, stdout: unused_stdout, stderr: unused_stderr, handles: handles, stdin: unused_stdin, clock: clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: unused_host_paths) = move inputs;
  let std::fs::Directory(read: cwd, write: cwd_write) = move cwd_directory;
  std::fs::close_directory_write(factory: &handles, directory: move cwd_write);
  std::fs::close_directory(factory: &handles, directory: move cwd);
  let empty = box_slots_new::<u8>(capacity: 64_u64);
  let running = False();
  let state = Pending(bytes: move empty, stopping: running);
  let pending = shared_new::<Pending>(value: move state);
  let producer_handle = shared_share::<Pending>(shared: &pending);
  let producer_clock = std::time::clock_share(clock: &clock);
  spawn produce(pending: move producer_handle, clock: move producer_clock);
  let drained = write_log(pending: move pending, clock: move clock);
  let code = cvt.wrap::<u64, u8>(drained);
  return std::process::exit_status(code: code);
}
```

The writer wakes three times for bytes and once for the stop; idle, it would
wake once a second. Exit code 3. The `else` block runs with nothing held, so
it may not name `current`; A6 refuses it there.

## 7. Comparison and recommendation

| criterion | A: condition in header | B: hold in block | C: elision by rule | D: optional by data |
|---|---|---|---|---|
| F1 one statement, one body | yes | body inside the hold; Firn's shape repeats a call | yes | yes, with a match per site |
| F2 use refused where unknown | yes, by scope (A2) | yes, by scope | no | yes, by [ENT-3.S15] |
| F3 held set fixed at begin | yes | no: fixed at the hold, sound by B1 | no (first use) | yes |
| F4 zero cost when not held | yes | yes | yes when the order allows | yes |
| any type conditional | yes | only later types (B1) | only later types | yes |
| lock order stays out of the source | yes | visible as B1's refusal | must be stated to be a rule | yes |
| new checker machinery | one scoping rule | one static type-order check | none | a reference representation |
| atomic value's one operation | unchanged count | plus "no hold after the operation" | unchanged | unchanged |
| guard with a deadline | `until`/`else` on the wait clause | same | same | same |
| migration of existing programs | none | none | none | none |
| unique power | — | hold an object found in held state | — | — |
| what Firn writes | `if held` to the target, `if held { }` around uses | `if held { hold ... }` | restructure, no guarantee | `match m^` |

**Recommendation.** First, no language change: Firn writes one statement per
command under today's first-use rule, confirmed by Q0. When a program needs
the guarantee, a refusal of stray uses, or a conditional target of an
earlier-ordered type, adopt **A**, with these parts:

1. The target grammar `IDENT = &place (if IDENT)?`, rules A1 to A8, A-lex
   for the scope of a conditional binding.
2. The one table of (state type, index atom type) for what is held, which
   absorbs PR #385's shard rows and the atomic value row as they are
   drafted, with their own rules (S1 to S3, V1 to V6) unchanged.
3. The wait clause `when expr (until atom)?` with the `else` block, specified
   now so the grammar has its place, implemented when the backlog item
   "an atomic statement waiting on its guard can set a deadline" is scheduled (P2 today).
4. SHARE-2's "what may stand beside what" sentences replaced by R-overlap
   stated once over the table: targets on overlapping handle places each
   have an index step of an entry or key-set kind.

Decisive reasons. A is the only option whose held set is a function of
values read before the first acquisition, which is what keeps the design
tree's lock-order argument untouched and lets a target of any type be
conditional; section 6.2 shows that B and C fail on the simplest case, a
`u64` beside an entry. It adds one scoping rule and no fact channel, which is
the smallest checker change that gives F2. A target not held costs no shared
write, so Firn's AOF-off path is byte for byte the two-statement form's else
branch. And it reads as Firn's own sketch, which the owner saw.

**Confidence 3/5.** The order argument is a deduction from R-order and
two-phase locking (strong). The cost claim rests on today's lazy taking and
PR #409. Lazy taking is implemented; PR #409 is an open pull request not
on main and not in the measured compiler. Q0 measured the one-statement
form without PR #409: per-round ratio median 0.9965 against the
two-statement form, within the twin's spread, with a cost below 1% not
excluded (section 8.2). What could overturn the recommendation: a
program that needs to hold an object it finds inside held state (then B is
wanted, and A and B compose: B's `hold` could be added later under A's
header without conflict); the owner preferring proof by facts over proof by
scope (then A-fact replaces A2, same grammar); Q0 or the acceptance
measurement showing a conditional target's lowering costs more than the
two-statement form, which the lowering sketch gives no reason to expect.

## 8. Registered check plan (written before any measurement)

### 8.1 Conformance cases

Each case fails for the rule it guards and for no earlier error. Positive
cases run and exit with the stated code.

Conditional targets (SHARE-2, A1 to A8):

- `share-pos-conditional-target-held`: section 6.1's A form as written,
  logging off and one timed round per writer; exits 4.
- `share-pos-conditional-target-always-held`: the same with logging on and
  the final check comparing the commit count with 32; exits 32.
- `share-pos-conditional-target-skipped`: logging off and no timed round;
  exits 0, and `meta` is never written.
- `share-pos-conditional-two-targets-one-atom`: entry, shard and atomic
  value under one condition (section 6.3's last paragraph).
- `share-pos-conditional-and-unconditional-one-object`: two handles that may
  name one object, one conditional; held once when both are held.
- `share-neg-conditional-use-outside-if`: `m^.commit_seq` at the block's
  top; refused citing SHARE-2 at that `place`.
- `share-neg-conditional-use-in-else`: a use in the `else` of `if held`.
- `share-neg-conditional-use-under-band`: a use under `if band(held, x)`.
- `share-neg-conditional-use-in-guard`: `when m^.x > 0_u64` on a conditional
  binding.
- `share-neg-conditional-condition-not-let`: `if store^.logging` as the
  condition (a place, not a let IDENT).
- `share-neg-conditional-condition-rebound`: a `set held = ...` elsewhere in
  the function.
- `share-neg-conditional-condition-not-bool`: a `u64` binding as condition.
- `share-neg-conditional-whole-twice`: two conditional whole targets on one
  handle place (R-overlap).
- `share-neg-conditional-unused`: a conditional binding named nowhere.
- `share-neg-conditional-atomic-two-operations`: an operation under `if a`
  and another under `if b` on two conditional atomic values; refused by V2.

Deadlines (SHARE-2, SHARE-3, A6), when that work is scheduled:

- `share-pos-guard-deadline-fires`: a guard that never becomes true, a
  deadline of 50 ms; the `else` block runs and the program exits with its
  marker.
- `share-pos-guard-deadline-not-needed`: section 6.4's after program; exits
  3.
- `share-pos-guard-deadline-true-after-deadline`: a guard made true after
  the deadline is reached but before the statement re-evaluates; the block
  runs (the deadline is a lower bound for giving up, not an upper bound on
  taking effect).
- `share-neg-deadline-without-guard`, `share-neg-else-without-deadline`,
  `share-neg-else-names-binding`, `share-neg-deadline-not-instant`.

Program test: `tests/programs/conditional_hold.wf`, section 6.1's A form
with 16 contexts, 100000 rounds and a mix of timed rounds, checking at the
end that `commit_seq` equals the number of timed stores and that every
writer's key holds its last value.

The shard and atomic-value cases are those their records already register
(PR #385, section 10.3 there; sequence-take, section 9 there); this design
changes none of them.

### 8.2 Measurements

All on the 14900K's 8 P-cores, through CI, with a twin control, following the
counter study's procedure (section 8.3 there). Each arm is Firn's SET
benchmark with AOF off, 8 cores, as the board item measured 21.4 M/s.

- **Q0, before any language change.** Firn's SET as one statement under
  today's compiler: `m = &store^.meta` unconditional, every use of `m` under
  `if held`, which today's lowering takes at first use because `Meta`
  follows the map type in the order. Prediction (**estimate**): within the
  twin's spread of the two-statement form's 21.4 M/s, because the AOF-off
  path then takes the same one lock and, with PR #409, counts no handle. If
  it is, A's lowering is today's lowering plus a mark in the taken word, and
  its acceptance carries no cost risk. If it is below, the attribution
  (perf c2c on the object's line, as the counter study did) says what the
  single statement still writes, and that is the first thing A's lowering
  must remove.

  **Q0 result (measured, Firn's run, 2026-10-13).** Firn ran it on the
  14900K: [Firn-wf run 38367914126](https://github.com/Ming-Research/Firn-wf/actions/runs/38367914126),
  compiler release `wf-d75f318d5631` (without PR #409), 8 P-cores (CPUs 0, 2,
  ..., 14), client on CPUs 16-31, AOF off, pipeline depth 16, 6 interleaved
  rounds of 10 s. The one-statement SET against the two-statement SET had a
  per-round ratio median of 0.9965 (range 0.984-1.001); the identical twin
  had 1.001 (0.994-1.025); GET, which does not run this statement, 1.006.
  The registered prediction holds: the single statement is within the
  twin's spread. A cost below 1% is not excluded (the 6-round mean, 0.994,
  sits at the twin's lower edge). So Firn needs no language change for
  speed: it writes one statement per site, and option A is adopted only
  when a program needs the checker-enforced guarantee (section 7).
- **Acceptance, after implementation.** Firn's 40 sites as one statement each
  under A; SET with AOF off within twin noise of 21.4 M/s (Firn's stated
  completion condition); SET with AOF on at the two-statement form's rate
  for AOF on (unchanged path); Firn's gate green. The wrong arm that must be
  detected: a build whose conditional target is taken when its condition is
  false, which the `share-pos-conditional-target-skipped` case catches by a
  write that would then be visible, and which the measurement catches as the
  1.23 M/s collapse.

## 9. Points where the choice is open or the record is unsure

1. **A-lex or A-fact** (section 5.1). The study recommends the scoping rule;
   the owner may prefer uses admitted wherever the checker can prove the
   condition. A-fact needs a Bool datum to be a goal, which [ENT-3] gives
   only to operation rows today.
2. **The keyword.** `if` on a target reads as Firn wrote it; `when` is taken
   by the guard; `with` would also serve. A spelling choice for the owner.
3. **The deadline's spelling and layering.** `until atom` with an `else`
   block is the study's form. Two things are open: whether SHARE-2 may name
   `std::time::Instant`, a host-module type (precedent: [PROG-3] names
   `std::process::Inputs`), or whether the deadline should be a
   `CancelWatch`, which a timer context fires, making the deadline a kind of
   cancellation and needing no clock type in the kernel but one context per
   timed wait; and whether to specify the clause now or only when the
   backlog item is scheduled. The study specifies the grammar slot now and
   leaves the implementation to that item.
4. **Option B later.** Holding an object found inside held state has no
   witness today (`share-pos-map-handle-in-an-entry.wf` uses two statements
   and does not need one). If one appears, B composes with A; a backlog
   item, not a card.
5. **TYPE-11 for conditional targets** (A7): whether establishing an
   invariant over an out-of-scope binding's path at the block's entry is
   harmless in the implementation, or must move to each `if c` then-block's
   entry. To be settled in implementation; the specification can state
   either.
6. **The real-time sentence S6** of the sequence-take study is still needed
   once numbers exist; this design does not change it.
7. **Whole-object read holds.** The lowering's object lock carries no read
   flag (counter study, section 2.2), so a `SharedRead<T>` whole target is
   held exclusively today. H1 and H2 say read-only is authority and the
   footprint decides the hold; closing that compiler gap needs no language
   change and is not part of this design.
8. **The specification's "possibly one object" comparison** (R-overlap)
   treats a conditional and an unconditional target that may name one
   object as held once; this is sound, but the entry-merging lowering
   (concurrent-map, decision on merged holds) must learn to merge a
   conditional group only when it is held. An implementation detail, noted
   so it is not missed.

## 10. Found along the way

- **The board item's sketch guard `bor(current^.bytes.inner.len > 0_u64,
  current^.stopping)` is not admitted**: call arguments are atoms, and a
  comparison is not one. The guard must be a call to a helper (section 6.4)
  or the condition must be two atoms. Disposition: the example in this
  record is written in the accepted grammar; the board item's example should
  be corrected when that item is worked.
- **Firn's single-statement form runs at speed today.** Today's
  lowering takes `Meta` at its first use; Firn's measured collapse to 1.23
  M/s had `aof_refuse(meta: m)` at the top of the block, an unconditional
  first use. Q0 measured the one-statement form with every use of `m` under
  `if held`: per-round ratio median 0.9965 against the two-statement form
  (range 0.984-1.001; twin 1.001), within the twin's spread, compiler
  without PR #409, a cost below 1% not excluded (section 8.2). Disposition:
  measured; Firn needs no language change for speed.
- **`u64` beside an entry is held on every execution today** (section 6.2),
  because primitives precede prelude nominals in the order and a group is
  taken with every earlier untaken group. This is correct and invisible, but
  it means "the lowering takes a target only on the path that reaches it"
  (state-locks, second decision) holds only for later-ordered types.
  Disposition: stated here; A removes the dependence for conditional
  targets.
- **The Shards draft was against v0.136** while main was at v0.138 at the
  time of writing (main `93e97efdc`; two amendments landed since); its
  rebase must re-archive. The shards specification has since been approved
  and is on its pull request at a later version. Disposition: for the PR #385
  branch.
- **The object lock has no read flag** (point 7 above). Disposition: backlog
  for the compiler, with the counter study's measurement as its witness.
- **No case today checks that a target named only on an untaken path costs
  nothing**; the property is in the design tree, not in evidence.
  Disposition: `share-pos-conditional-target-skipped` and Q0.
