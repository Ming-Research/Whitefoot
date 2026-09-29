<!-- Serves the owner's question of 2026-09-29: whether the rule that makes
     compute parallelism safe (a program means its sequential execution)
     also makes concurrent I/O safe, and if not, what does. It proposes the
     model for waiting contexts that replaces the progress rule of
     SHARED.md's "Progress while a guard waits" and the sequential meaning
     WAITS.md gave waiting calls. Its surviving decisions go to the design
     tree and the specification; this record stays as their grounds. -->

# The concurrency model for waiting contexts

## The question

While ruling on the progress of a statement that waits on a guard
(`SHARED.md`, "Progress while a guard waits"), the owner saw that guards are
one case of a general problem (written in Chinese, translated here):

> I feel this is really a broader problem. Any waits function "may" stop, and
> once it stops it may block what comes after it. For example, one thread
> reads a file pipe, or accepts, and another thread connects. It is the same.

A context that reads a pipe another context writes, or accepts a connection
another context makes, has the same shape as a consumer started before its
producer.

The owner then put the question directly (written in Chinese, translated
here):

> The reason we built par, concurrent computation and concurrent I/O, is to
> make traditional multithreading safe without TLA+ or the like. For
> computation this is perfect, but I/O becomes much harder because of
> blocking. I am no longer sure that the premise, making a program safe on
> one thread to make it safe on many, really holds for I/O. Should we relax
> the idea so that it guarantees only concurrent computation, and let I/O go
> back to explicit threads or coroutines, like goroutines? And if we give it
> up, how is safety kept?

This record answers from first principles. It checks the answer against the
constitution and the design tree, and it tests with probes what the proof
machinery can already do. It proposes a model, names what it changes, and
lists the open questions.

## 1. What makes threads unsafe

The hazards of shared-memory threads fall into two groups:

- **Safety** (nothing bad happens):
  1. *Data races*: unsynchronized reads and writes of the same memory.
  2. *Atomicity violations*: a step of one thread lands between two steps of
     another that were meant to be one, as in the lost update.
  3. *Broken invariants and orderings*: a relation between pieces of state,
     or an assumption that A happens before B, fails in some interleaving.
- **Liveness** (something good eventually happens):
  4. *Deadlock*: threads wait for each other in a cycle.
  5. *Starvation and livelock*: a thread never gets its turn.

Tools like TLA+ are used for 3, 4 and 5. They enumerate interleavings, or a
bounded model of them, because the number of interleavings grows
exponentially with the threads and their steps.

## 2. Why compute parallelism is sequentially equivalent, and I/O is not

Overlapped computation [PAR-1, PAR-2] is parallelism, not concurrency:

- overlapped statements have disjoint footprints, so they share nothing;
- they never wait, since a waiting call denies overlap [WAIT-1];
- they do not talk to each other.

With no interaction, every interleaving computes what the sequential order
computes, so all five hazards are absent. The premise "safe on one thread,
safe on many" holds exactly because the parts do not interact.

Concurrent I/O exists to interact, with the world and among its own
activities. Once activities wait for each other there is in general no
sequential order equivalent to the concurrent execution. Two witnesses
settle it:

- **A bounded buffer has no sequential schedule.** A producer puts 5,000
  items through a ring of capacity 8 to a consumer. In any sequential order
  one side runs first and stops, full or empty, before the other begins. The
  same holds for a pipe written past its kernel buffer, and for any bounded
  channel. Under a sequential meaning such programs are wrong by definition,
  including PR #173's own `tests/programs/shared_objects.wf`, which only
  concurrency lets finish.
- **Concurrency can hang a sequentially correct program.** A starter spawns a
  producer, then polls an object for the producer's write with atomic
  statements that have no guard. In order, the producer finishes first and the
  first poll succeeds. Concurrently on one driver, the poll loop never
  suspends, so the producer never runs. A host call that blocks the driver
  thread does the same (`docs/todo.md`, "A context's operation with no
  readiness form still blocks the thread every context shares").

A third point concerns values. The order of different contexts' atomic
statements is already an input of the execution [WAIT-2]. A read with no
guard may therefore see the old value where the sequential order would have
shown the new one. That is not a hang but a different result, and a correct
program is correct for every order. So "write before read" is only
guaranteed by a guard or a join, never by source position across contexts.

**Conclusion.** The sequential meaning cannot define concurrent I/O. It must
either call ordinary bounded pipelines wrong, or leave hangs of correct
programs undefined. The premise holds for computation and should stay there.

## 3. What the constitution asks for

`docs/constitution.md`, "Safety":

> Accepted Whitefoot programs must exclude undefined behavior, memory
> corruption, data races, uninitialized reads, silent overflow, and any other
> operation whose required safety conditions have not been established by
> machine proof. [...] Logic errors, including unintended nontermination, may
> remain.

Deadlock is a kind of nontermination. The constitution requires hazard 1,
data races, to be excluded by machine proof, and allows 4 and 5 to remain as
logic errors, the way an infinite loop may. Without a race, hazards 2 and 3
are logic errors too, unless a required safety condition depends on them, as
when an index is proved in bounds from a count another context keeps. The
model excludes 2 by construction anyway (4.2) and lets a program prove 3
(section 5). That is the split this model makes: safety by construction and
proof, liveness as a runtime obligation plus detection.

## 4. The model

### 4.1 Contexts are concurrent sequential processes

- **`spawn f(args)` starts a context.** It is a statement form of a call whose
  callee waits. The owner chose the keyword `spawn`.
  - It takes only value parameters, as WAIT-2 already requires.
  - An expression-statement spawn is joined before the activation leaves.
  - A `let` spawn is joined before its binding is next used.

  Spawned contexts are structured: none outlives the activation that spawned
  it (as in Trio nurseries and Kotlin's `coroutineScope`).
- **Meaning.** Each context executes its own statements in order. Contexts
  interleave only at waits: host operations, atomic statements and joins. The
  order in which different contexts pass those points is an input of the
  execution, as the order of host completions already is [WAIT-2]. A spawned
  context runs concurrently with its starter from the spawn until the join.
  That is what spawn means, not an implementation liberty.
- **A call that is not spawned executes in order.** The implementation may
  still run it as a context when [WAIT-2]'s permission holds, but nothing
  depends on it.
- **Computation keeps the sequential meaning.** Overlap under PAR-1 and PAR-2
  stays an implementation liberty that preserves the meaning of the context
  it happens in.

### 4.2 Safety by construction

| Hazard | How it is excluded | Status |
|---|---|---|
| Data races | A context takes only value parameters; state two contexts share is a `Shared<T>` object reached only inside atomic statements [SHARE-1, SHARE-2] | in the language |
| Atomicity | An atomic block takes effect at one point; it cannot wait or nest, so a lock is never held across a wait and lock-order deadlock cannot be written [SHARE-2, SHARE-3] | in the language |
| Broken invariants | A monitor invariant per object, proved block by block (section 5) | proposed |
| Irreproducible runs | Behavior is a function of the host outcomes and the atomic order [WAIT-2]; recording both replays a run | stated; no recorder yet |

Compared with the languages the question names:

- **Go** can race on shared memory and relies on a dynamic detector. Locks can
  be held across blocking calls and nested in any order.
- **Rust** excludes data races with `Send` and `Sync`, but leaves lock order,
  holding a lock across an `.await` with an async mutex, and the atomicity of
  critical sections to the writer.

Section 7 has the checked comparison.

### 4.3 Liveness: what the runtime owes, and what it does not

**Promised by the implementation, not proved per program:**
- A spawned context runs concurrently with its starter.
- Every context that waits for nothing eventually runs (fair scheduling).
- Optionally, a statement whose guard is true from some point on takes effect
  (weak fairness). This is what the handoff bound in
  `design/amendments/compiler-waiting-contexts.md` provides.

**Not promised:**
- the absence of deadlock: two contexts that each wait for the other's write,
  or a context waiting for its own later statement, wait forever;
- strong fairness for a guard that is true only now and then;
- any timing.

A deadlock is a logic error, and the runtime detects and reports it where it
can: every context waits, and nothing is in flight. That is Go's test too,
and like Go's it misses a cycle among some contexts while others run. Go 1.27
also reports a goroutine blocked on a primitive that no runnable goroutine
can reach (section 7). The same reachability test over shared-object handles
is a candidate for reporting a partial cycle; it is untested here.

**What the runtime must still fix for the promise to hold.** These are today's
gaps, all recorded in `docs/todo.md`:
- A host operation with no asynchronous form blocks the driver thread: a pipe
  read or write, a connect, and every file operation on a host with no ring.
  It must go to a helper and park its context.
- Drivers do not preempt, and a waiting call answered at once does not
  suspend. A context that loops on such calls, or computes forever, holds back
  its driver. A yield after some number of non-suspending waits, taken only
  when another context is ready, would fix this; its cost must be measured.
- A driver reaps host completions only when no context is ready.
- A cycle of guard waits stops the program on one driver but hangs on several.
- On a host with no ring, and on Windows, only one driver runs. This affects
  speed, not the promise.

### 4.4 What becomes of `mustpar`

`mustpar` has three forms [PAR-4]:
1. on a counted loop, asserting PAR-2;
2. on a call that does not wait, asserting PAR-1 with the next statement;
3. on a call that waits, asserting WAIT-2's permission.

**Form 3 becomes `spawn`.** In practice it already means spawn: every
`mustpar` in `tests/programs` (7 of 7) is form 3, and this compiler runs each
one as a context. Under the model the two are different judgments. `mustpar`
asserts that an overlap the implementation may choose is permitted, and
changes no meaning. `spawn` changes the meaning: the spawned context runs
concurrently with its starter. So `spawn` does not rename form 3; it replaces
it.

**Forms 1 and 2.** The owner suggested retiring `mustpar` entirely. Forms 1
and 2 add no permission and change no behavior. With `--par`, the compiler
overlaps permitted computation whether or not it is marked; without it, it
overlaps none, marked or not (`design/compiler/parallel-lowering`). The
marker only turns a denied permission into a rejection at its site.

What the repository shows:
- No program, experiment or compiler test writes forms 1 and 2; only the
  conformance cases of the marker do, and `docs/patterns.md` teaches form 1.
  The compute and parallel programs
  (`tests/programs/compute`, `tests/programs/parallel`) and the parallel
  experiments (for example `research/experiments/par-quicksort`) rely on the
  permission without asserting it.
- The reason recorded for the marker (`design/language/parallelism`) is that
  without it, a writer who needs parallelism "learns that it was lost only
  from a ledger or a measurement". The ledger exists:
  `whitefootc --par-ledger` prints each permission and each denial.

For retiring it entirely:
- One call prefix instead of two. Beside `spawn`, `mustpar f()` would be a
  checked claim about computation that starts nothing, while `spawn f()`
  starts concurrency. The pair invites reading `mustpar` as a weaker start.
- A performance claim can be checked where performance is checked: a
  program's test can read the ledger, as a benchmark reads a time. The
  language then carries no word whose only effect is a refusal about speed.
- It retires PAR-4 and its checker path. Of the twelve `par4-*` conformance
  cases, the four about waiting calls become `spawn` cases, and the eight
  about forms 1 and 2 (and the marker's placement) retire with the rule.

Against:
- The marker fails the build at the site, and it survives edits: a change
  that breaks a loop's independence is refused where it is made, instead of
  slowing the program. A ledger check in a test catches the same loss only
  in the programs a test covers.

This record recommends retiring `mustpar` entirely, with `spawn` for form 3
and the ledger for forms 1 and 2. No program depends on forms 1 and 2, and
the refusal they give can move into a test. If a program is later found
whose parallelism is lost silently and the loss matters, that is the evidence
to reopen it, and a site-level assertion can return in whatever form the
case needs.

## 5. Monitor invariants

### 5.1 The rule

This is Hoare's monitor proof rule (1974), in the form concurrent separation
logic calls a resource invariant. Each shared object has an invariant I over
its state. The checker proves:

1. **Creation.** The value handed to `shared_new` satisfies I.
2. **Each atomic block.** Assuming I at entry, together with the guard when
   there is one, I holds at every edge that leaves the block: its end,
   `return`, `break`, `give` and error propagation.

A guard evaluation that reads false changes nothing, since the guard writes
nothing [SHARE-2], so it needs no proof.

### 5.2 Why this covers every interleaving

The object's state is reached only through an atomic statement's binding
[SHARE-1], and blocks on one object take effect one at a time [SHARE-3]. Any
execution therefore acts on the state as a sequence of whole blocks, in some
order and from any contexts.

- I holds after creation.
- Each block takes a state satisfying I to a state satisfying I.

By induction on the sequence, I holds between any two blocks. The proof
obligations are one per block, not one per interleaving, and a block's proof
never mentions another context.

### 5.3 Why WF can check it, and what is missing

What is already there:

- The guard is a fact at the block's entry [ENT-3.S1].
- The binding is an ordinary reference root. The block is ordinary checked
  code, and every fact about the state ends with the block [SHARE-2].
- A written invariant at a program point is a proof obligation the checker
  already discharges [INV-1], with AUTO and, when needed, written `use` steps
  [PRF-1].

What is missing:

- a declared invariant for an object;
- I as a fact at each block's entry;
- the creation and exit obligations.

`research/experiments/monitor-invariants/` tests how far today's machinery
carries the proofs. It uses stand-ins for the entry fact: an `if` on I inside
the block, or a helper's `requires`.

- **Difference-bound invariants over fields and measures work end to end.**
  `count == items.len` is proved at the exit of a bounded queue's `put` and
  `take`. The invariant also discharges the arithmetic: `count + 1` from
  `count == len < cap`, and `count - 1` from `count == len > 0`.
- **The two classic mistakes are refused.**
  - A block that forgets its update is refuted.
  - A read-modify-write split over two atomic statements is unproved: the
    second block knows I, not that the count still equals what the first one
    read. This is the lost update, refused at compile time.
- **Affine invariants (sums of fields) need an entry snapshot.** Over
  immutable values, AUTO proves the invariant and its arithmetic with no
  written steps. Over the fields of a reference, today's affine images give no
  usable premise.
  - Stated over entry values that L0 equalities tie to the fields, the proof
    succeeds with one named bridging step.
  - So the checker should mint a value for each state place I names at the
    block's entry, and state I over those values. At an exit, it can state I
    over the written values directly and avoid the bridge.
- **Ghost state should be erased mathematical integers.** A counter kept only
  for the proof (`produced`, `consumed`) grows without bound. A `u64` field
  owes an overflow proof no program can give.

### 5.4 Where I is declared

The design tree says no struct invariant exists as a fact, and privacy adds
no implicit type invariant (`design/language/checks-and-proofs`; [ENT-3]). It
refused type-level struct invariants because every construction and field
write would owe the invariant again. A monitor invariant is different: it is
owed only at creation and at each exit of an atomic block, the only points
where another context can observe the state, and it may be false inside a
block. Still, where it lives decides how general it looks.

- **(A) On the state type, applying only to objects of that type.** For
  example, a clause in the struct that holds for every `Shared<Queue>` and
  that no other use of `Queue` sees.
  - For: one declaration per state type, and the type is where a reader looks.
  - Against: two objects of one type cannot have different invariants without
    a wrapper type, and a spelling on the struct must not read as a general
    struct invariant.
- **(B) As a parameter of the handle type** (`Shared<Queue, queue_ok>`, with
  `queue_ok` a named relation).
  - For: per object type, explicit.
  - Against: a second generic parameter at every use.
- **(C) At `shared_new`.** The handle's type would still have to carry it for
  the blocks to know it, which is (B).

This record recommends (A), with a spelling that says the invariant belongs
to shared objects. The owner decides the surface.

### 5.5 Limits

- **No invariants over the elements of a storage.** "Every value in the
  keyspace is at most 512 MiB" is out, because the fact language carries no
  quantified element facts (`design/language/checks-and-proofs`). What an
  invariant can state is what the proof language states: difference bounds
  and affine relations over fields, measures and constants.
- **One object at a time.** An invariant across two objects needs them in one
  object, or the deferred multi-object statement.
- **Nothing between blocks.** An invariant says nothing about the state
  between a context's blocks beyond "I held when I left and holds when I
  return".

## 6. What this changes

The changes below are grouped by where they land.

**Specification**
- `GRAM-4` adds `spawn`.
- WAIT-2 is rewritten from "the meaning of an execution is its sequential
  execution" to the context meaning of 4.1, with the runtime obligations of
  4.3.
- PAR-4 is retired with `mustpar` (4.4); if the owner keeps forms 1 and 2,
  it loses form 3 only.
- SHARE-3's progress sentences, added by PR #173, give way to WAIT-2's
  context meaning. The weak-fairness sentence is kept if the owner wants it.
- A new rule states the object invariant, its creation and exit obligations,
  and its entry fact.
- ENT-3's "no struct invariant ... exists" gains the monitor invariant as a
  checked source.

**Design tree**
- `language/parallelism`:
  - Decision 3's "a program means its sequential execution" applies to
    computation.
  - If the owner retires `mustpar` (4.4), decision 2 is retired; either way
    decision 4's `let` form moves to `spawn`.
  - The refusal of "a separate `spawn` statement" loses its ground: `mustpar`
    asserts a permission while `spawn` starts a concurrent activity, so they
    are two judgments, not one named twice.
  - The refusal of "a marker whose meaning is that a context must start" loses
    its ground too. Its reason was that SHARE-3 promises progress for every
    covered call, so a marker would add no meaning; under the model, `spawn`
    is that marker, and the meaning it adds is concurrency.
- `language/waiting/shared-objects`:
  - Decision 1's reason, that "every other rule keeps its sequential
    reading", is restated for the context meaning.
  - Decision 7 (progress) is replaced by the context meaning.
  - The refusal of atomic fields and lock-free cells, "that the sequential
    meaning excludes", is restated: they would expose interleavings of single
    reads and writes inside what the model makes one atomic step.
  - A decision on the object invariant is added.
- `compiler/waiting-contexts`: the pending amendment's "which calls start"
  decision is withdrawn, because only spawned calls start. The handoff bound
  and the join placement stand.

**Compiler and runtime**
- Parse and check `spawn`; lower it as today's context start.
- Remove the pass that starts unmarked calls reaching a guard.
- Add the invariant's declaration, entry facts and obligations.
- Close the runtime gaps of 4.3.

**Tests and guidance**
- Every waiting-call `mustpar` becomes `spawn`: the seven in
  `tests/programs`, the waiting-call examples of `docs/patterns.md`, the four
  `share-pos-*` conformance cases that start contexts, the compiler tests
  that write one, and the io-completion bench's `context_starts.wf`.
- Four `par4-*` conformance cases become `spawn` cases. If `mustpar` is
  retired, the other eight retire with PAR-4, and `docs/patterns.md` drops its
  `mustpar for` example.

**PR #173**
- **Still stands:**
  - shared objects, atomic statements and guards as facts;
  - STOR-3;
  - the Redis subset;
  - the join placement;
  - the handoff bound, as the runtime's choice or as the weak-fairness
    promise if the owner makes it (4.3).
- **Replaced:**
  - the progress sentences of SHARE-3, WAIT-2 and PAR-4 that it adds;
  - the pass that starts unmarked calls;
  - `mustpar`'s waiting-call form, or all of `mustpar` if the owner retires
    it (4.4).

## 7. Prior art

Each claim below was checked against the source named with it. The survey
is one pass, so "found no" means no more than that.

**The proof rule is classical.**
- *Conditional critical regions.* Hoare's "Towards a Theory of Parallel
  Programming" (1972) gave `with r when B do C` with an invariant for each
  resource r. It is the direct ancestor of `atomic s = &h when B { ... }`
  with a monitor invariant.
- *Monitors.* Hoare, "Monitors: An Operating System Structuring Concept"
  (CACM 17(10), 1974), developing Brinch Hansen's concept. The invariant
  holds "before and after every procedure call" and "must also be made true
  after initialization of the data, and before every wait instruction". The
  paper leaves deadlock to the writer: "Assertion-oriented proof methods
  cannot prove absence of such risks".
- *Owicki and Gries* (Acta Informatica 6, 1976) prove general interference
  freedom: every assertion of one process is checked against every atomic
  action of the others, so the checks grow with the product of the
  processes' sizes and a proof is not compositional. Regions avoid that
  because the shared state is reached only inside them. The same paper uses
  auxiliary variables, the ghost state of 5.3.
- *Concurrent separation logic.* O'Hearn, "Resources, Concurrency and Local
  Reasoning" (TCS 375, 2007), builds on the 1972 paper. Each resource has an
  invariant; a region assumes it with its guard and must restore it; the
  initialization establishes it. For soundness each invariant must be
  "precise", which Brookes proved sufficient. In WF the ownership that
  separation logic states with `*` is already given by value parameters and
  by the state being reachable only through the binding [SHARE-1].

**Languages with a guarded exclusive operation.**
- *Ada protected objects* have entries whose barriers are checked on call
  and re-checked after exclusive operations (Ada 2022 RM 9.5.3). *SPARK*
  proves the pre- and postconditions of protected operations, but GNATprove
  lists "a protected type annotated with a type invariant" as unsupported. A
  property of the protected state is restated on each operation. Deadlock is
  addressed by the ceiling-priority protocol under the Ravenscar profile, on
  one core.
- *Eiffel SCOOP* (Nienaltowski and Meyer) turns the precondition of a call on
  a separate object into a wait condition, and states a proof rule with the
  class invariant on paper; the tools check contracts at run time.
- *Chalice* (Leino and Müller) checks one monitor invariant per class
  statically and excludes deadlock by locking levels, a lock order. It
  verifies through Boogie and an SMT solver, and its 2009 tutorial notes
  that "condition variables are not yet available".
- *Software transactional memory.* Harris and Peyton Jones, "Transactional
  memory with data invariants" (TRANSACT 2006), added invariants to GHC's STM
  that are checked at run time at the end of every transaction. The paper
  remarks that read-only invariants "may be more amenable to static
  verification". GHC's `base` 4.11 still has them (`alwaysSucceeds`,
  `always`); 4.12 no longer does.

This survey found no implemented general-purpose language that combines a
guarded atomic block with a compiler-checked invariant per object. Ada and
SPARK have the block and no invariant proof. Chalice and later VeriFast (with
condition variables, Hamin and Jacobs, ESOP 2018) prove the invariant with
solvers or interactive proof and do not guard the acquire. STM checked
invariants at run time, and GHC removed them.

**The languages the question names.**
- *Go* shares memory. The Go memory model says a race on a multiword value
  "can lead to arbitrary memory corruption", and the race detector finds
  only the races that happen in a run. The runtime reports a deadlock only
  when all goroutines are asleep. Go 1.27 (August 2026) made the
  `goroutineleak` profile generally available: it reports a goroutine
  blocked on a primitive that no runnable goroutine can reach.
- *Rust* excludes data races with `Send` and `Sync` and calls deadlock safe
  (the Rustonomicon). A `std::sync::MutexGuard` is not `Send`, so holding it
  across `.await` is refused where a spawn needs `Send`, as `tokio::spawn`
  does, and allowed with `spawn_local`. `tokio::sync::Mutex` may be held
  across `.await`. Critical sections and lock order are the writer's.
- *Structured concurrency* was stated by Sústrik (libdill, 2016) and Smith
  (Trio's nurseries, 2018), and reached by Kotlin's `coroutineScope`: no
  child outlives the block that opened its scope. `spawn` in 4.1 is
  structured the same way, bounded by the activation.
- *Erlang* isolates process heaps and communicates by messages, but public
  ETS tables are shared, with each single-object update atomic. Two
  `gen_server`s that call each other block until the default 5,000 ms
  timeout.

**What follows for WF.**
1. The rule of section 5 is Hoare's and O'Hearn's rule, applied where WF's
   rules already give its premises: the state is reached only inside a
   region, and one region runs at a time.
2. What would be new is putting it in a language, checked without a solver
   by the machinery of 5.3.
3. No surveyed language proves deadlock freedom for guarded waits. The ones
   that exclude some deadlock impose a lock order (Chalice) or a priority
   ceiling (Ravenscar). WF excludes lock-order deadlock by refusing nested
   atomic statements, and leaves guard cycles to detection, as Hoare left
   them to the writer. That is the split of 4.3.

Sources:
- Hoare 1974: https://classes.cs.uchicago.edu/archive/2022/spring/33100-1/papers/hoare-monitors.pdf
- Brinch Hansen, monitors and Concurrent Pascal: http://pascal.hansotten.com/uploads/pbh/Monitors%20and%20Concurrent%20Pascal.pdf
- Owicki and Gries 1976: https://link.springer.com/article/10.1007/BF00268134
- O'Hearn 2007: http://www0.cs.ucl.ac.uk/staff/p.ohearn/papers/concurrency.pdf
- Ada 2022 RM 9.5.3: http://www.ada-auth.org/standards/22rm/html/RM-9-5-3.html
- GNATprove limitations: https://docs.adacore.com/spark2014-docs/html/ug/en/appendix/gnatprove_limitations.html
- SPARK concurrency: https://docs.adacore.com/spark2014-docs/html/ug/en/source/concurrency.html
- SCOOP: https://se.inf.ethz.ch/~meyer/publications/concurrency/scoop_laser.pdf
- Chalice tutorial: https://www.microsoft.com/en-us/research/wp-content/uploads/2016/12/krml197.pdf
- Hamin and Jacobs 2018: https://people.cs.kuleuven.be/~bart.jacobs/esop18.pdf
- STM invariants: https://timharris.uk/papers/2006-transact.pdf, and `GHC.Conc` in `base` 4.11 and 4.12 on Hackage
- Go memory model and 1.27 notes: https://go.dev/ref/mem, https://go.dev/doc/go1.27
- Rust: https://doc.rust-lang.org/std/sync/struct.MutexGuard.html, https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html
- Structured concurrency: https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/
- Erlang: https://www.erlang.org/doc/apps/stdlib/gen_server.html, https://www.erlang.org/doc/apps/stdlib/ets.html

## 8. Open questions for the owner

1. The model of 4.1: spawned contexts are concurrent, and unspawned calls and
   computation keep the sequential meaning.
2. Whether weak fairness for guards is promised (4.3).
3. `mustpar`: retire it entirely, as this record recommends, or keep forms 1
   and 2 as assertions about computation (4.4).
4. Where the invariant is declared (5.4), and whether ghost state (erased
   mathematical integers) comes with it or later.
5. Whether PR #173 lands first with its progress rule withdrawn, carrying
   what still stands (section 6), and the model follows as its own change; or
   the model is built on #173 before it lands.

## 9. What would test it

- A bounded producer and consumer through a `Shared` ring, with the invariant
  `len + consumed == produced`: accepted, finishing on 1 and on several
  drivers, with a missed update or a split transaction refused.
- The Redis subset's keyspace with an invariant it can state, for example a
  key count kept equal to the map's length.
- Two contexts joined by a pipe, and a server that accepts a connection its
  own spawned client makes: both finish on 1 driver and on several once the
  blocking host calls are routed off the driver thread. Today they would stop.
- A starter that polls an object for a spawned producer's write: it finishes
  on 1 driver once the yield exists.
- A cycle of guard waits: reported on 1 driver and on several.
