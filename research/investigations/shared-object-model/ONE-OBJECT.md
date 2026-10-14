# One object per atomic statement

## Status

A design study written 2026-10-13/14 before any implementation. Nothing in it
was compiled or run. Rules are cited by ID from the active specification
(`spec/kernel-spec.md`, v0.146). Firn sites are cited from Firn-wf `main`
(`55a8495`) and its branch `claude/select-db`. Every performance statement is
a deduction from the runtime design in `design/compiler/waiting-contexts/` and
the measurements cited on the board; none was measured for this study.

It replaces the direction of [DESIGN.md](DESIGN.md) (description plus a
proof-only handle identity). The owner, in the session of 2026-10-13, after a
discussion of why handle identity was needed at all (translated):

> When we define a shared object we implicitly promise that the process inside
> it is atomic: from beginning to end nobody else interferes. Only then is it
> transparent to the checker. Two objects of different types in one `atomic`
> only appear to work: we have merged two atomic regions, and the safety rests
> on the fact that two types cannot name one thing, which is a coincidence.
> With two objects of one type the problem breaks out, not because they are of
> one type but because the checker's coincidental rule no longer hides it. The
> root of the problem is that we tried to invent a form in which several
> unrelated locked objects cross each other and share one atomic space.

and, on the direction below: if it solves the classes on the board item
without a large performance regression (a theoretical argument suffices for
now), it is the clean and simple solution wanted; adding syntax later for an
individual problem is preferable to the multi-object form.

## 1. The argument

**A one-object statement rests on one premise:** its guard and block have
exclusive access to the object's state from its first access to its last
[SHARE-3]. Inside the block the state is reached only through the statement's
binding [SHARE-1], so to the checker it behaves as a value the block owns for
the duration. Place-based facts, separation and invalidation [REF-1, OWN-7,
ENT-5] are sound without any knowledge of handles.

**A multi-object statement rests on a second premise:** its objects are
distinct, or the runtime merges them. That is a run-time fact. SHARE-2
approximates it statically: targets whose state types do not unify start at
two roots, targets whose types unify are compared as paths from one root. For
different types the argument is valid but borrowed: it also needs every state
to be reachable only through its own target, an invariant that any later
feature storing references would break. For one type there is no static
argument at all, which is the first class of the board item
`shared-object-model`.

Almost everything the multi-object form added exists to carry the second
premise or to manage several locks: the static order of types and the
run-time order of identities, merging duplicate objects into one hold,
"unifying types start at one root", the per-partition invariant check of
#441, lazy type groups, and the proposals to add a proof-only identity
(#478, branch `claude/shared-handle-distinctness`). A statement that holds one
object needs none of them, and statements that each hold one object can never
wait for each other in a cycle.

**Consequence: the atomic domain is the object.** Things that must change in
one atomic step are, by definition, one atomic domain, and therefore one
object. What made "put it in one object" look restrictive was conflating the
atomic domain (semantics, program structure) with the lock unit
(implementation, performance). The concurrent hash map already shows they are
separable: one object, an unbounded number of dynamically created parts, each
locked alone, with the checker comparing two entries as two subscripts of one
root.

### 1.1 What `atomic` states once locks are inferred

With the header reduced to the object and every lock derived (sections 2.1
and 3), `atomic` carries no locking information. It is not sugar: it states
the three things no analysis can recover from the code, and lock inference
takes them as its input.

1. **Which steps are one step.** Two contexts each running `let n =
   s^.count; set s^.count = n +wrap 1;` add 2 if the read and the write are
   one step and may add 1 if they are two; a balance check and its debit must
   be one step, a log line after them need not be. Which grouping the writer
   meant is the program's meaning, so the language must state it; a compiler
   that chose the boundaries would choose the program's outcomes.
2. **Where facts about shared state hold.** Inside the block nobody else
   changes the state, so `if s^.balance >= x` proves the subtraction that
   follows; outside, another context may write between any two steps, so no
   fact about the state survives from one step to the next. The block is the
   scope of those facts and its end kills them [REF-2].
3. **Which object, and where waiting is excluded.** The header fixes the
   object (the atomic domain) when the statement begins; the block may not
   wait [WAIT-1, SHARE-2], and the guard and deadline attach to it.

Under the multi-object form the header was also a lock declaration (choosing
targets chose locks); separating the two leaves `atomic` with only its
meaning. The split is the one the lock-inference literature makes: the
programmer marks atomic sections and the compiler infers the locks
(McCloskey et al., "Autolocker", POPL 2006; Emmi et al., "Lock allocation",
POPL 2007; Cherem, Chilimbi and Gulwani, "Inferring locks for atomic
sections", PLDI 2008). The other mode the language offers is the atomic value
(section 2.3), whose every operation is its own instant, for values where a
single operation is all the atomicity wanted.

## 2. The model

### 2.1 One handle per statement, nothing else in the header

    atomic s = &store {
      let slot = &s^.keys[key];   // a local name for a path, not a hold
      ... slot^ ... s^.meta ...
    }

**Principle (owner, 2026-10-14):** an atomic statement means exclusive access
to the whole shared object; every lock optimization the compiler makes must be
equivalent to locking the whole object, and under that equivalence it may make
any optimization.

The header names the object and nothing else: `atomic s = &h`, with the
optional guard and deadline. The block has the whole state at one point
[SHARE-3]; which parts are locked is lowering's choice (section 3). The header
carries no list of parts because the compiler can find every part such a list
could name (owner, 2026-10-14: "the best solution is fully automatic
inference"):

- A field is found from the block's footprint, proofs included (section 2.5).
- A map entry `s^.t[k]`, a key-set selection `s^.t[ks]` and an element of an
  array of maps `s^.dbs[i]` need their key or index before the lock is taken,
  because a map's entries are locked together in sorted key order. When the
  key's storage and its binding are written, moved and released nowhere in
  the block (the block's write footprint, including calls' projected rows
  [EFF-5], does not overlap them), the key's value is the one it has when the
  statement begins, and lowering locks that entry, with the map's other such
  entries, at the map's first use. Otherwise lowering holds the map whole.
  Both are correct; the second is slower, and `--lock-ledger` reports it.
- A key computed from held state (a key name stored in an entry) cannot be
  known before the hold by any rule. The map is held whole, or the writer uses
  two statements: the first reads the key, the second locks both entries and
  checks that the first still names the second.

The design tree rejected "a table with header entries taken at its first use
in the block" because the take could read key bytes the block had written or
released, and rejected the other repair, refusing every write or release of a
key's storage in the block, because it refuses programs for a lowering's
timing (compiler/waiting-contexts/state-locks). This rule does neither: a key
the block writes is never locked in advance, and no program is refused; its
map is held whole instead.

**A leading block that computes the keys was considered and is not proposed**
(owner's question, 2026-10-14). If it reads no held state, it has exactly the
power of code written before `atomic`, which needs no syntax. If it reads
held state, it reads that state before holding it, so its keys can be stale
when the locks are taken; making that sound needs a validation and retry
(the "optimistic lock location prediction" of deterministic databases), which
a block that runs once, moves values and writes locals cannot have. So keys
are computed before the statement, and the compiler finds them.

### 2.2 Where the special state kinds live

TYPE-9 gives each restricted type one home. The homes change from "the whole
state of an object" to "a part of a shared state":

| type | home today | home proposed |
|---|---|---|
| `ConcurrentHashMap<V>` | the whole state of a `Shared` | any field (at any depth, including a fixed array element) of a shared state; never a local, never moved in or out whole; exchanged only by `swap` |
| `Shards<T>` (#385) | the whole state | the same as the map |
| an atomic value (section 2.3) | `DecisionCell` handle; #403's `AtomicValue` state | its own one-word object, `Atomic<T>`, whose handle may be anywhere a handle may |

This keeps the owner's principle of 2026-10-05, "a concurrent hash table is
shared by nature; a type's behavior should not change with where it is put"
(design log, `ConcurrentHashMap` entry): a map exists only inside shared state
and behaves the same wherever it sits there. It also avoids what made the
earlier table-field design (#208) complex: header grants, row checks and
provenance sets existed because the owner had then made a table an ordinary
movable value (Q18), so the checker had to track which state a table belonged
to. A map that can only ever be a part of a shared state is reached only by a
path from that state's binding, so there is nothing to track.

### 2.2.1 Shards as a field (owner, 2026-10-14)

`Shards<T>` is a field type of a shared state. A place through such a field,
used as a place of `T` inside a block, is the part of the thread executing the
statement; lowering acquires only that part's ticket lock (one fetch-add on a
line only that thread writes), changes the value with ordinary loads and
stores and releases it with one store. `any_shard()` and the selector value are
retired: the old header needed an index atom, the new one names only the
object. There is no subscript and the number of parts is not observable.
Proposed whole access (form open until the specification): the shards are made
with an initial value and a merge function, and only two whole operations
exist, a merged read (INFO) and a take that returns the merged value and resets
every part (CONFIG RESETSTAT, the AOF batch). When the merge function is
associative and commutative (sum, maximum, merging two sequence-ordered runs),
the result does not depend on which part a write landed in. Reading the local
part gives only that part, which a buffer needs (its own length before an
append). Thresholds for shards are relative to a C program giving the same
guarantee by the best known method: throughput at least 0.9 of it, longest
whole read at most twice its longest under the same load.

### 2.3 Atomic values: one-word objects whose statements are single operations

Everything in a shared state is held state, reached inside an atomic statement
as if the whole object were locked. The one other kind of shared state is the
**atomic value** (`Atomic<T>`, name open): a shared object whose state is one
word, `T` an integer type, `Bool` or an enum whose variants carry no payload,
and whose only statements are single operations:

| operation | integers | `Bool` | payload-free enum |
|---|---|---|---|
| load, store, exchange, compare-and-exchange | yes | yes | yes |
| add: wrapping, and #403's bounded saturating add | yes | | |
| max, min | yes | | |
| and, or, xor | bitwise | logical | |

Handles are made and shared as `Shared` handles are (`atomic_new`,
`atomic_share`, names open) and live wherever a handle may: a local, a
context's arguments, a struct a context keeps. An operation is an atomic
statement with one step on a one-word object, which the hardware performs as
one instruction, so it never waits. Because it never waits it is admitted
anywhere: inside another statement's block, in a guard (loads only, section
2.4) and in a function that does not wait. This keeps "one statement, one
object": each operation is its own statement on its own object, and since
none waits, none can be part of a cycle of waits.

**Meaning.** Every operation is one point in the total order of shared effects
[SHARE-3, WAIT-2]: sequentially consistent, with no weaker order offered, so
the language gains no memory model. Cost: on x86-64 a load is a plain load, a
read-modify-write is the locked instruction it would be anyway (max and min
are a compare-and-exchange loop) and a store is an exchange; on AArch64,
load-acquire, store-release and the LSE read-modify-writes, which include an
atomic maximum.

**Ordering with the enclosing statement** (generalized from #403): inside an
atomic statement, every lock the statement takes is taken before its first
operation on an atomic value. The statement's point can then be placed
immediately before that operation, since two-phase locking makes any point
between the last acquisition and the first release valid, so a sequence number
taken there orders statements by effect, and a verdict claimed there is
visible before the long statement ends.

**Checker.** An operation's result is a fresh value; no fact about an atomic
value's content is kept from one operation to the next, since others change it
at any point. A function that operates on one carries the shared-effect marker
(section 2.6).

**Why its own object and not a field of a shared state** (the previous draft
of this section). A field would be reached through a handle without a
statement, a second way into a state; one inside a map entry would have to
outlive the entry's removal; and a long statement holding the object would
still have to let others reach it. A one-word object needs none of this: it is
today's `DecisionCell` generalized, with the same handle lifetime.

The decision verdict becomes `Atomic<DecisionState>` with compare-and-exchange
from `Open`; #403's atomic value becomes `Atomic<u64>` with add and max; a
cancellation's `fired` becomes `Atomic<Bool>`. The 13 rules the decision cell
changed and #403's "one operation per path, outside loops, no alias" become
this one section.

**What it gives up.** The design tree rejected general atomic fields because
they expose single reads and writes inside what the context meaning makes one
step (language/waiting/shared-objects). An atomic value does expose them: a
program can load, decide and store, and lose a race to another context doing
the same. That is a race condition, not a data race, which CAP-1 already does
not exclude; the type shows where it can happen, and compare-and-exchange is
the operation that avoids it. Adopting this reverses that rejection in full,
which the owner decides; the decision cell reversed it in part.

**Frozen parts: not proposed.** A part no statement writes is already read
without a lock (section 3.1), so a frozen part would only add reading outside
a statement, which `Frozen<T>` [SHARE-1] already gives as a separate object.
`Frozen<T>` stays as it is. It is not a reader-writer lock: no operation writes
a frozen object's value and its type may hold no shared handle, decision cell
or host handle [SHARE-1], so no writer exists and a read without a lock cannot
see a partial write. A changed value is a new frozen object whose handle
replaces the old one; readers holding the old handle keep reading the old
value until they release it. The reader-writer behaviour, for state that does
change, is the read hold that lowering derives inside an atomic statement
(section 3.1, item 3).

### 2.4 Crossing objects

The only interaction between two objects inside one statement is an operation
on an atomic value, which holds nothing and waits for nothing. No statement
holds a second object.

The recurring cross-object need in the repository is a guard of the form
"work is available or this was cancelled" (`cancellation.wf`,
`cancel-guard-run-readiness`, Firn's `refresh_idle_limit`). Proposed: a guard
may load atomic values. Every write to an atomic value checks the value's
watcher word, one load on the line the write already owns, and when watches
exist wakes them as a write to held state does today. The guard's held reads
take effect at the statement's point and its loads right after it (the
ordering rule of section 2.3); a cancellation flag only goes from false to
true, so a guard that saw it true stays true. Waiting on the held state of two
independent objects (a Go-style select) is not expressible; section 8 gives
the syntax that would add it without merging atomic regions.

### 2.4.1 The needs and where each goes

| need | example | form | ground |
|---|---|---|---|
| a counter every command writes and a report reads | INFO's command and hit counts | a `Shards` field of the state | one word written by every core reached 118 M/s on 8 P-cores for C's atomic add against 1,994 M/s for a line per thread (run 38304337014); shards reached 0.60 of the per-thread line (run 38458676162) |
| a maximum that rises rarely | `used_memory_peak` | `Atomic<u64>`: load, then max only when larger | loads of a line nobody writes leave it in every core's cache; only a new peak writes |
| one number line in effect order | AOF sequence | `Atomic<u64>` add inside the key's statement | must be global; ordering rule of section 2.3 |
| a one-shot verdict visible mid-statement | SCRIPT KILL | `Atomic<DecisionState>` compare-and-exchange | in the held object, KILL would wait for the script |
| a flag read often and set rarely | cancel, stop, busy, shutdown | `Atomic<Bool>` | read in loops inside other statements' blocks and in guards |
| a scalar setting every command reads | timeouts, memory limit | `Atomic<T>`, or a `Frozen` value replaced whole | a load per command, a store per CONFIG SET |
| several values that change together | `Meta` | held state | the lock optimizations of section 3, equivalent to locking the object |

### 2.5 What the checker sees

One root per statement, with ordinary paths below it. Specifically:

- Separation between entries, shards and fields is OWN-7 on paths below one
  root. Map entries under byte keys need a key-disequality fact to be proved
  apart; key-set entries already compare by integer index (section 9, open choice 2).
- Facts about the state hold from the statement's entry to its exit and are
  killed at exit like the facts of any reference whose referent leaves scope
  [REF-2].
- Type invariants: the state's type invariants, including those relating two
  fields, are assumed at entry and owed at exit [TYPE-11], because
  semantically the block has the whole state. For the implementation to match,
  **a part counts as accessed when the block's proofs mention it, not only
  when its code reads it**, so a statement that relies on `meta.count ==
  keys.len` holds both. This also settles the board item about atomic
  bindings mentioned only in proofs (`firn-wf-atomic-proof-mentions`) as a
  consequence rather than an exception.
- The checker never compares handles, has no identity facts, no per-type
  overlap rule and no partition enumeration.

### 2.6 What stays outside this model

These classes of the board item are orthogonal to the number of objects per
statement and keep their own designs:

- **Release (class 2):** each runtime type declares its release action in one
  place (#478's description part, without its identity part).
- **Explicit-close state (class 4):** proposed shape: a shared object whose
  state is linear has linear handles, each closed by `shared_close(h) ->
  Option<T>`, the last closer receiving the state and its obligation. Firn's
  AOF file then lives in its own small object (only its handles are linear),
  taken out by exchanging `Some(file)` for `None` in a statement, written
  outside any statement and put back; waiters use a guard on `Some`. This
  needs checking against the reasons the lease study (card `lease-direction`)
  rejected "the last handle takes the state back" before it is proposed.
- **Effects (class 5):** the effect row gains one marker for "may perform
  shared effects" (statements, operations on atomic values, cancellation
  firing);
  `waits` implies it. Merging and reordering rules and PAR-1/PAR-2 consult
  only it, and the declaration-derived "decision-cell-free boundary"
  [SHARE-3, PAR-1, PAR-2] retires. This is the one place where shared state
  is not transparent: memory safety does not need to know, but "may this call
  be reordered" does.

## 3. Lock units and lock order (lowering)

### 3.1 Units are computed from the program's statements

An executable's composition lowers its whole closure
(compiler/incremental-compilation, first decision), so lowering sees every
atomic statement on every shared state type of the program. For each such
type and instance it computes:

1. **Never-written parts.** A part that no statement writes, moves or swaps
   after construction is read without any lock. The state is initialized
   before any handle to it exists elsewhere, and a handle reaches another
   context only through a spawn or a held statement, both of which order the
   initialization before the read.
2. **Units by co-access.** Every field, every map, every `Shards` and every
   element of an array of maps or shards starts as a candidate unit. A part P
   is protected by another unit Q's lock when every statement that writes P
   holds Q exclusively and every statement that reads P holds Q in either
   mode; P then has no lock of its own. Parts always used together therefore
   share one lock, parts used apart keep separate locks, and the writer does
   not arrange fields for locking.
3. **Modes.** A unit no statement writes in a given statement is held for
   reading there (`gran-read-hold-read-only`).

Acceptance never depends on this analysis: module verdicts stay modular and
whole-program information only selects how an accepted statement is lowered.
The design tree's rejection of "an order of store types inferred from every
atomic statement of the program", because it needs every function body, was
about acceptance; here the order is the declaration's (section 3.2) and only
lowering reads every body. The costs are performance at a distance, since a
new statement elsewhere can split or merge a type's units, which
`--lock-ledger` shows, and recomputing a type's units when any statement on it
changes in an incremental build.

**A part used in one place still needs a lock.** One statement in the source
runs in as many contexts as reach it: Firn's SET statement runs in every
connection's context at once. Eliding the lock of a part reached by only one
context would need a proof that its statements execute in one context (one
spawn site, executed once, the creator not touching it afterwards). That proof
is fragile under ordinary edits, and a part only one context uses normally
belongs in that context's locals, so it is not proposed. What the analysis
does elide is the lock of a part that is never written (1) or always used
together with another unit (2).

**Possible later: word parts lowered to hardware atomics.** A word-sized part
that every statement touches at most once, by one load, store or
read-modify-write, after all its other locks, could use a hardware atomic
instead of a lock with the statement's one-point meaning intact (#403's
conditions). It is fragile in the same way: one statement elsewhere that
reads two such counters together disables it for both. Explicit atomic values
(section 2.3) remain the form whose cost the writer can see.

### 3.2 One static order

Holding a part whole (a whole `s^.dbs` for FLUSHALL) takes each unit inside it
in order; no unit keeps a counter on the parts enclosing it.

Units are ordered by their position in the state's declaration (field order,
then array index, then, within a map, the sorted key order of today). A
statement takes each unit at its first use on the path, together with every
earlier unit it will need and has not taken, and takes the entries and units
its guard reads before the guard, which are today's rules with field order in
place of the order of type names and with the run-time identity order gone.
The writer now chooses the order: a part needed only conditionally (Firn's
`meta` when logging) can be declared after the parts every command uses, the
case the atomic-targets study found today's type-name order cannot express
(its section 0, "a conditional target whose type precedes a used one").

### 3.3 Watches per unit

A guard registers its watch on the units it reads, not on the object, so a
write to a key's entry does not wake a guard waiting on `meta`. Today the
same separation comes from separate objects; keeping it is an obligation of
this design (R3).

## 4. Performance, argued

The comparison point is today's equivalent program: the same parts as
separate objects, the form Firn already uses.

**Per statement, Firn's SET with AOF on.** Today: read two handles from
`Keyspace` (no handle count since #409), take the `keys` entry (gate, cell),
take the `meta` object's lock, both ordered by type at compile time. Proposed:
read one handle, take the `keys` part's entry, take the `meta` unit's lock,
ordered by field position at compile time. The atomic operations and the
cache lines written are the same; one handle read fewer.

**Regression risks and what removes each:**

- **R1 false sharing.** Parts of one object sit in one allocation; two units'
  lock words on one cache line would bounce between cores that touch
  different parts. Lowering pads each unit's lock word to its
  own line (128 bytes on cores that fetch line pairs). Cost: one or two lines
  per unit, negligible beside a map.
- **R2 coarse units or ancestor counters.** If unrelated fields shared one
  lock, or if every statement registered on an object-wide gate, unrelated
  statements would meet. Section 3.1 merges only parts that are always used
  together, and there are no intention counters on enclosing parts. The
  object header (count, descriptor) is read-only on the statement path.
- **R3 spurious wakes.** Watches per object would wake every guard on any
  write. Section 3.3 keeps them per unit, matching today's separate objects.
- **R4 a callee given the whole state.** A call whose row reaches `s` whole
  reaches every map in it, and a map reached by a call is held whole; today
  the same is true of a map reference passed to a call [SHARE-2]. Writers
  pass the part (`&s^.meta`), as Firn passes `meta` today; `--lock-ledger`
  reports a whole hold.
- **R5 whole-subtree holds.** FLUSHALL, INFO and transactions over all
  databases take 16 map gates and `meta`, the same as the 17 targets of the
  `select-db` branch.

**Expected improvements:**

- no run-time sort or duplicate merge of objects of one type;
- one handle per context instead of four in Firn (`keys`, `meta`, `server`,
  `scripts`), so spawning a connection retains one object;
- the lock order is chosen by the writer (section 3.2), so conditional parts
  cost nothing when unused regardless of their type;
- atomic values replace exclusive statements for flags, maxima and
  sequences: on 8 P-cores a hardware add on one word ran 118 M/s against
  2.81 M/s for an exclusive statement on one object (board item
  `firn-req-shared-max`, run 38304337014), with shards still the answer for
  counters that must scale further.

**Conclusion:** by deduction, no statement that today touches a set of
separate objects costs more when those objects are parts of one, provided
R1 to R3 are met in lowering; several paths become cheaper. The deduction
fails, and the design must be revisited, if a measured statement on parts is
slower than the same statement on separate objects beyond twin noise
(section 10).

## 5. The board's classes and requests under the model

- **Identity (class 1).** Firn's multi-database calls become two subscripts
  of `s^.dbs`; `clear_both` receives `&s^.dbs[i]` and `&s^.dbs[j]` after
  `i != j`. Two keys of one table are two subscripts of one map (section
  9.5). Capacities `n+1` and `n+2` no longer arise: there are no two objects
  to compare. Items `firn-req-distinct-shared-maps`, `atomic-binder-distinct-keys`
  and `coord-wfbl-04-24` close by this design or reduce to the key fact.
- **Release (class 2):** unchanged, section 2.6.
- **Invariants (class 3):** for held parts, section 2.5; frozen parts carry
  the invariants established at construction; map entries and shard slots
  follow the sealed-value rule of #484.
- **Explicit-close values (class 4):** section 2.6.
- **Effects (class 5):** section 2.6.
- **Global sequence (`firn-req-sequence-take`):** an `Atomic<u64>` added
  inside the key's statement after its locks (section 2.3). #403's
  `AtomicValue` placement and its special rules are not needed.
- **Decision verdict (`firn-req-attempt-publish`):** an
  `Atomic<DecisionState>`.
- **Read holds (`gran-read-hold-read-only`):** per unit, derived from the
  footprint; no language change.
- **Shards (`firn-req-shared-max`, `firn-req-shards-owned`):** a `Shards`
  field; Firn's "lock a key and my shard" is one block using `&s^.keys[k]`
  and `&s^.parts[mine]`, with `mine` taken from `any_shard()` before the
  statement, so both are locked alone.
- **Deadline (`firn-req-wait-key-or-flag`):** `until d { } else { }`
  unchanged, and simpler to withdraw: a pending statement's partial holds are
  units of one object, released in reverse order.
- **Spawn in a block (`firn-req-timer-from-atomic`):** unchanged.
- **Cancellation guards:** section 2.4.

## 6. Firn under the model

All 62 multi-target statements on Firn `main` name parts of one `Keyspace`
value whose fields are four handles (`keys`, `meta`, `server`, `scripts`) and
a flag; one (`refresh_idle_limit`, `firn/server/server.wf:253`) adds a
cancellation view, read for its `fired` flag. The keyspace becomes one object:

    struct Store {
      keys: ConcurrentHashMap<Entry>;
      meta: Meta;
      server: ServerState;
      scripts: EnginePool;
    }

    // before (firn/commands/strings.wf and 40 others)
    atomic slot = &store^.keys[key], m = &store^.meta { ... }
    // after: the key is a slice of the read-only input, so its entry is
    // locked alone; meta is locked at its first use
    atomic s = &store { let slot = &s^.keys[key]; ... s^.meta ... }

The `select-db` branch is where the multi-object form hurts most and where one
object helps most. Today it names sixteen databases as sixteen targets
(`firn/commands/transaction.wf:249`, `scripting/entry.wf:489`,
`commands/info.wf:485`, `commands/eviction.wf:495`, `commands/databases.wf:522`)
and passes them to helpers with sixteen reference parameters
(`held_databases_run`). With `dbs: Array<ConcurrentHashMap<Entry>, 16>` the
header is `atomic s = &store` and the helper takes `&s^.dbs`. MOVE is, with
`from`, `into` and the key all fixed when the statement begins,

    atomic s = &store { move_value(from: &s^.dbs[from][key], to: &s^.dbs[into][key]); ... }

after Redis's own refusal of `from == into`, and SWAPDB is
`swap(first: &s^.dbs[left], second: &s^.dbs[right])` under `left != right`,
the branch Firn already writes.

## 7. What the model removes

From the specification: SHARE-2's paragraph on two handles naming one object
and on unifying state types; "a whole target alone on its handle place"; the
decision-cell rules and the decision-cell-free boundary; #403's atomic-value
rules (not merged). From the design tree and runtime: the type order and
identity order of locks, duplicate-object merging, per-type lock groups
(replaced by per-field order). From the research lines: the identity part of
#478, the handle-distinctness study, the #441 partition check. From the
conformance suite: the cases that test those rules (about half of the 67
files with a multi-target statement; the rest are rewritten into one object).

## 8. What the model gives up, and syntax that could restore it

- **Waiting on the held state of several independent objects** (select over
  two channels). If a program needs it: a `select` of several one-object
  statements, each with its guard; the runtime registers each guard and runs
  the first that becomes ready. Only one alternative executes, holding one
  object, so no atomic regions merge. No current program needs it (the
  deadline card `wait-key-or-cancel-form` reached the same conclusion for its
  option C, a general select).
- **Holding an object found inside held state** (the atomic-targets study's
  option B). Under this model the handle is copied out and used in a second
  statement. No current program needs it.
- **Atomically combining two objects created separately by unrelated code.**
  They must be designed as parts of one object. This is the price, and the
  point: the atomic domain is declared in the types.

## 9. Open choices

Settled in discussion with the owner (2026-10-14):

- **Principle:** every lock optimization is equivalent to locking the whole
  object (section 2.1).
- **Header:** the object alone, with entries inferred (section 2.1); no
  leading key block.
- **Lock units:** computed from the program's statements (section 3.1), not a
  structural rule.
- **Frozen parts:** not added; `Frozen<T>` stays (section 2.3).

Still open:

1. **Guards over atomic values.** Allow (section 2.4) or express
   cancellation only through `until` and a cancellation clause on the
   statement.
2. **Two keys of one table.** Either a prelude byte comparison whose `false`
   result is a key-disequality fact for OWN-7, or the existing key-set form,
   whose entries compare by integer index. The first is one new fact kind,
   general rather than shared-specific; the second needs nothing new and is
   what Firn's RENAME already does.
3. **Atomic values (section 2.3):** reverse the rejection of general atomic
   fields in full; a one-word object rather than a field of a shared state;
   the name; the operation table and its add modes; sequential consistency
   only.

## 10. Criteria and check plan, fixed before work

**Adopt** if all hold:

1. Every program in section 6 and the five board examples are written under
   the model with the expected verdicts, by reading the proposed rules.
2. No rule of the model mentions a handle comparison, an identity, or a list
   of shared type names.
3. The specification rules removed outnumber the rules added (counted rule by
   rule in the amendment).
4. After implementation, on the 14900K's 8 P-cores, interleaved with twins:
   Firn's SET, GET and MSET with AOF off and on are within twin noise of
   today's multi-object build; a guard on `meta` is woken no more often than
   today under a key-writing load (R3); a shard update stays at or above 0.5
   of the C per-thread line (the `firn-req-shared-max` criterion).

**Revisit** if any holds: a case in section 5 needs a second held object;
lowering cannot meet R1 to R3 without a shared write on the statement path;
or criterion 4 fails beyond twin noise.

**Check plan.** Conformance cases: one handle per statement (positive) and two
handles (negative, citing SHARE-2); two map entries proved apart by index and
by key fact; a field and an entry in one statement; a cross-field type
invariant held by a statement that mentions only one field in code (it must
hold both); an atomic value operated on inside another object's statement; a
guard over an atomic value woken by a store; a statement whose operation on an
atomic value precedes a later
lazily taken unit (the lowering must take the unit first). Compiler tests: a
key the block writes makes its map a whole hold, and an unwritten key an
entry hold (`--lock-ledger`); a never-written field is read without a lock; two
fields always used together share one lock and fields used apart do not. Programs: Firn's
SET shape, MOVE and SWAPDB over `dbs`, the cancellation guard. Measurements as
in criterion 4.

## 11. Atomic values against every open request (2026-10-14)

The owner asked why so many designs grew around needs that one atomic value
with compare-and-exchange answers, and whether `DecisionCell` is anything but
that. This section takes each open board item that concerns shared words, in
turn, and compares the current design with `Atomic<T>` (section 2.3). Numbers
are measured on the 14900K's 8 P-cores where a run is cited and are otherwise
deductions.

**Why the designs multiplied.** Two earlier decisions pushed every need into a
narrow shape. The design tree rejected "general atomic fields and general
lock-free cells" (language/waiting/shared-objects), so each need was given a
type that did not count as one: `DecisionCell` (one-shot, "exposes no
intermediate contents"), #403's atomic value (only a statement target, one
operation per path, at the statement's point), `SharedRead` for cancellation
guards (#304), and cancellation-state updates as their own kind of shared
effect. And the single-step integer cell was rejected on a counter criterion
(one word reached 0.06 of a line per thread against a 0.5 bar,
card `gran-shared-counters-card`), a correct verdict for counters that was
then read as a verdict on atomic values in general. With the rejection
reversed, the narrow types are `Atomic<T>` with fewer operations.

`DecisionCell` is `Atomic<DecisionState>` restricted to load and
compare-and-exchange from `Open`: the same machine operations (a plain load, a
locked compare-and-exchange). The one property it adds is that the type
guarantees a decision never reverts; a library type in `std` with a private
`Atomic<DecisionState>` field and the two claim functions keeps that guarantee
without a prelude rule.

1. **Counters and the maximum (`firn-req-shared-max`, `firn-scope-info`,
   cards `shards-merge-with-reader-gap`, `shards-suspension-accounting-approve`).**
   Counters every command writes stay on shards: one word written by every
   core reached 118 M/s in C (run 38304337014), shards 0.60 of a line per
   thread, about 1.2 G/s (run 38458676162). The maximum moves to an atomic
   value: load, and max only when larger. A load of a line nobody writes stays
   in every core's cache, so the common path costs about what not recording
   costs (Firn's GET: 26.99 M/s without the peak against 2.98 M/s with an
   exclusive statement per command); a write happens only on a new peak, and
   no whole read takes every shard lock. The shard cards stay, for counters.
2. **AOF sequence (`firn-req-sequence-take`, cards `atomic-value-spec-approve`,
   `atomic-value-yield-check`, `firn-write-meta-aof`).** `Atomic<u64>` add
   inside the key's statement, after its locks (section 2.3): the instruction
   #403 emits, without its target-only, once-per-path, no-loop and no-alias
   rules. Q1 measured one shared add after 400 ns of private arithmetic at
   1.000 of none; Q2 measured today's exclusive object at 0.015. #403's tests
   and harness carry over. The yield-check card is item 6's problem.
3. **SCRIPT KILL (`firn-req-attempt-publish`, `wf-need-firn-script-pool`).**
   `Atomic<DecisionState>` compare-and-exchange, identical code to the merged
   cell. The pool's all-or-nothing SCRIPT KILL across engines gains a form the
   one-shot cell lacks: one `Atomic<u64>` holding a kill bit and a count of
   running scripts that have written, updated by compare-and-exchange, is a
   reusable protocol where a one-shot cell needs a fresh cell per epoch.
4. **Busy-script timer from the statement's start (`firn-req-timer-from-atomic`,
   card `spawn-inside-atomic-form`).** The script stores its start instant in an
   `Atomic<u64>` as its statement's first act, which never waits; a command
   waiting for a key uses `until start + threshold` and, on expiry, reloads the
   start and either answers BUSY or waits again. That is Redis's timing, which
   the deadline card said needs the start published. The card's option B
   ("publish one value from the statement") is exactly an atomic value; option
   A (spawn in the block) is no longer needed by Firn and stays a general
   backlog capability.
5. **Waiting for a key or a flag (`firn-req-wait-key-or-flag`, card
   `wait-key-or-cancel-form`).** Not an atomic-value problem: the statement
   waits while acquiring, before any guard runs. The deadline (`until`) stays;
   item 4 makes its timing exact.
6. **Progress on one driver (`firn-req-single-driver-progress`, card
   `single-driver-progress-direction`; also `atomic-value-yield-check`).** Not
   an atomic-value problem: a context that computes without waiting starves its
   driver whether it polls an atomic value, a decision cell or nothing. The
   scheduler's handoff (option A of the progress card) answers both cards, so
   an atomic operation stays one instruction with no yield check.
7. **Scripts not holding the write state while Lua runs (`firn-script-meta-free`,
   `wf-need-firn-par-commit`).** Firn found that a script must read the memory
   limit and whether an AOF rewrite is running during execution, and take a
   sequence number at commit. Those are atomic loads and one add, so a script
   need not hold `meta` while Lua runs even with AOF on, which the owner's
   option D had to leave out.
8. **Cancellation (`CancelSource`, `CancelWatch`, `SharedRead`, the
   cancellation-state update).** `fired` becomes an `Atomic<Bool>` with
   watches (section 2.4). A guarded statement that today holds the cancel state
   as a second object (a second lock and watch) loads a word instead, and a
   firing is a store plus a watcher check instead of a statement on the cancel
   object.
9. **Read-mostly settings (`gran-read-hold-read-only`).** That item notes a
   read hold still writes the lock word and holder count, so the line still
   moves between cores. For scalar settings an atomic load writes nothing and
   scales with cores; read holds remain for compound state.
10. **Weak cores (`firn-wf-mcu-atomics`, v2).** The same target questions as
    atomic statements: native read-modify-write is one instruction; one core
    without it masks interrupts in privileged mode; two cores without it need a
    hardware spinlock or refuse; 64-bit values on 32-bit cores need the
    declared atomic width. Nothing new.

**Not atomic-value problems:** shards holding owned buffers
(`firn-req-shards-owned`), the explicit-close lease (`firn-req-linear-resource-lease`),
distinct maps (section 5), invariants through fields, release declarations.

**Spec accounting.** Retired: the decision cell's 13 rule changes, #403's
SHARE-2 paragraph and placement rows (not merged), the decision-cell-free
boundary in PAR-1, PAR-2, SHARE-2 and SHARE-3, the cancellation-state update as
a kind of shared effect. Added: section 2.3's one section and the guard rule of
section 2.4.
