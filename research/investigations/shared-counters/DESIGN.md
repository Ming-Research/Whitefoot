# Shared counters and maxima without an exclusive hold per update

## Status

A design study, written before any implementation or measurement. It checks
the active specification `spec/kernel-spec.md` (title v0.134) and the
compiler and runtime at Whitefoot `4ad4f5786` (main). It changes no rule,
design-tree node or code; the specification changes in
[section 9](#9-specification-changes-proposal-only) are a proposal. No build,
test or measurement was run for it. Every number below is either quoted from
a cited run or marked as an estimate.

Line numbers cite `spec/kernel-spec.md` and source files at that revision.
Where this study infers instead of reading, it says so.

## 1. The question

Whitefoot programs share mutable state through shared objects [SHARE-1] and
change it only in atomic statements, whose guard and block run with
exclusive access to the targets' states and take effect at one point, all
statements taking effect in one order [SHARE-2, SHARE-3]
(`spec/kernel-spec.md:2276–2341`).

Stated precisely:

> N contexts [WAIT-2] on D drivers repeatedly apply an update to one logical
> integer that every context shares: an addition (a count) or a raise to a
> value (a maximum). Occasionally a context reads the value. What language
> surface lets an implementation run these updates at a rate that grows with
> D, close to the rate at which each context updates an object of its own,
> while
>
> 1. every update and every read keeps a meaning stated by SHARE-3's single
>    point and single order, or by an equally precise rule;
> 2. no accepted program has a data race, silent overflow or unproved
>    partial operation [constitution, Safety];
> 3. acceptance stays a deterministic proof judgment with no new
>    writer-accessible unsafe escape or runtime trap; and
> 4. the surface is general (counts, maxima, flags, per-command statistics)
>    rather than a path for one benchmark?

The minimal semantic witness, independent of firn:

```whitefoot
fn count_many(total: Shared<u64>) -> result: unit pure waits {
  for @count (index in 0_u64..1000000_u64) {
    atomic t = &total {
      set t^ = t^ +wrap 1_u64;
    }
  }
  return unit;
}
```

Run by 64 contexts, the program is correct today and its total is exact,
but every iteration takes the one object's lock, so its rate falls as
drivers are added (section 2). Nothing in the language lets the writer say
"each context adds to a part only it touches, and a reader sums the parts at
one point", because which driver runs a context is not observable [WAIT-2]
(`spec/kernel-spec.md:2260`), so the writer cannot select a per-driver part.
The runtime itself needed exactly that structure for heap accounting
(section 3.5).

## 2. Observations and what they attribute

### 2.1 The witness numbers

On the i9-14900K, native Linux, 64 contexts, three 5-second passes per
cell, median millions of operations per second (Firn-wf run 38230753463, the
runtime-contention witnesses on Firn-wf branch `claude/witness-contention`
at `6c0e973`, `research/experiments/runtime-contention/`):

| Witness | 2 P-cores | 4 P-cores | 8 P-cores | 16 hyperthreads |
|---|---:|---:|---:|---:|
| one atomic statement incrementing one shared object (`w_shared`) | 9.6 | 5.5 | 2.7 | 0.7 |
| the same, each context on its own object (`w_private`) | 89.6 | 171.4 | 348.1 | 438.5 |
| a read-only atomic statement on one shared object | 27.4 | 30.4 | 30.3 | 41.9 |

The third row's witness program is not among the seven the branch's
`run.env` lists (`base private shared read read_param write heap`); this
study takes its numbers as reported and could not read its source.

Firn GET at pipeline depth 16, 8 P-cores (Firn-wf redis-bench run
38227806896, recorded in Firn-wf
`research/investigations/memory-limit/README.md`, section "Peak recording
across cores", on branch `claude/peak-per-client`):

| Build | GET M/s |
|---|---:|
| exclusive peak statement after every command | 2.98 |
| peak statement only when a heap reading exceeds the connection's cached peak | 19.21 |
| no peak recording | 26.99 |

### 2.2 What the numbers contain

The per-update cost of `w_shared` is more than one exclusive lock. Reading
the runtime and emitter:

- **Every whole-object statement takes a write hold.** The emitter passes
  `write = 1` to `wf__shared_acquire`, `wf__shared_take` and
  `wf__shared_unlock` for every object target
  (`compiler/src/backend/emitter/shared.rs:1027,1059,1075`), and the
  lowering's object lock carries no read flag, unlike a map entry's
  (`compiler/src/lowering/builder/atomic.rs:155–163`). The runtime already
  admits shared read holds (`wf_shared_admits`,
  `compiler/src/backend/completion/bridge.c:3115–3119`; the interface comment
  at `completion/bridge.h:384–393`). So the read-only row was measured with
  exclusive holds.
- **Even a read hold writes the object's line.** Acquire and unlock each take
  the object's spin lock and update `holders`
  (`bridge.c:3224–3247,3365–3378`). A shared read hold would therefore still
  bounce one cache line per statement. This is an inference from the source;
  no shared-hold variant was measured.
- **The handle count.** A statement whose handle place is a value handle
  shares and releases the handle around the statement, one atomic
  read-modify-write each on the object header; the compiler skips that only
  for a handle place rooted at a reference parameter whose row does not
  write it (Firn-wf `runtime-contention/README.md`, "The handle count is not
  in the specification", citing `compiler/src/semantic/check/control/atomic.rs`
  and `compiler/src/lowering/builder/atomic.rs:138–147`). `w_shared` uses a
  value handle (`shared.wf`), so its updates carry this second contended
  write.
- **Collapse, not a plateau.** `w_shared` falls from 9.6 to 0.7 as cores are
  added. A lock that only serialized would hold roughly its single-holder
  rate; falling with cores indicates contention overhead in the lock itself
  (test-and-set spin lock, 256 spins before parking, `bridge.c:2990,3224–3251`).
  This is an inference; the attribution arm in section 8 separates it.

The Firn rows mix two costs. Firn's record attributes the collapse from
26.99 to 2.98 to the Meta statement (removing the heap reading alone changed
nothing beyond the twin's spread) and places the hottest contended cache line
of the 19.21 build on the runtime's heap counter read by `heap_in_use` (perf
c2c, same record). The source agrees: `wf__heap_in_use` takes the pool's
global spin lock on every reading and then sums up to 193 per-thread lines
(`bridge.c:1388,1410–1424`). A fix is in progress on Whitefoot branch
`claude/heap-read-nolock` (commit `45c544b24`, "Read the runtime pool's heap
total without the pool lock"). **So the remaining gap between 19.21 and
26.99 is mostly a runtime cost of reading the heap, not of the shared
update**; the per-connection check already made the update rare. This study
does not claim that split as measured; arm F-heap in section 8 measures it.

### 2.3 Where counters recur

Redis 7.0.15 keeps process-wide statistics updated by every command
(`total_commands_processed`, `keyspace_hits`, `keyspace_misses`, the
per-command `commandstats` table of calls and time). Firn's INFO currently
leaves "the per-command and per-error counts" out (Firn-wf
`firn/commands/info.wf:381`, the INFO doc string). Each such count is an
always-write update: unlike a maximum, it changes on every command, so no
check-then-update avoids the write.

The study therefore separates two kinds of update:

- **read-mostly**: a maximum, a set-once flag, a value updated only when it
  changes. Most updates find nothing to change.
- **always-write**: a count. Every update changes the value.

## 3. Current rules and constraints

### 3.1 Shared objects and atomic statements

- [SHARE-1] (`:2276`): a shared object's state is storage of no binding and
  belongs to no context; paths into it are formed only by atomic targets.
- [SHARE-2] (`:2307`): targets are a whole handle place, or a map handle place
  with one index step (`&[u8]` key or `KeySet`). Every atomic statement counts
  as a waiting call for [WAIT-1], [PAR-1] and [PAR-2]; its guard and block
  contain no waiting call and no atomic statement; the guard writes nothing.
  The statement's footprint is its handle places and index atoms, read.
- [SHARE-3] (`:2333`): one point, exclusive access, one order of all atomic
  statements and cancellation updates, source order within a context; a
  false guard waits. Its last sentence: "An implementation may hold less than
  a statement's states, or hold them together with other statements,
  wherever the program's outcomes are those it has when every statement's
  block executes with exclusive access to its states."
- [WAIT-2] (`:2255`): the order in which statements of different contexts take
  effect, map positions, scan extents and the driver count are inputs of the
  execution; where a context executes is not observable; weak progress for a
  begun statement whose guard is eventually always true.
- [WAIT-1] (`:1542`): a waiting call only in a waiting function.
- [PAR-1] (`:2159`): a statement containing a waiting call has no overlap
  permission. [HOST-1] (`:2250`): host effects are ordered through shared state
  only. [CAP-1] (`:2155`): a shared object adds no overlap permission.

### 3.2 Integers and overflow

[OP-2] (`:1002`) fixes the modes: the bare `+` is proof-required and its
domain obligation must be discharged statically; `+wrap` is defined modulo
2^K; `+checked` returns `Result<T, Overflow>`; `+sat` clamps. No runtime trap
exists. A proof about a concurrently changing value is unavailable outside an
exclusive hold: a guard is the only way a statement gets a fact about a shared
state [SHARE-2, ENT-3], and the
[relaxed-fields study](../relaxed-fields/README.md#s1--an-explicit-scalar-cell-type)
records why a concurrently mutable cell must never support a fact.

### 3.3 Effects

The state of a shared object is a path of no effect row: a function that
changes it declares `waits` and a read of the handle place it reaches the
object through ([design/language/waiting/shared-objects.md](../../../design/language/waiting/shared-objects.md),
fifth decision). Any new operation on shared state can follow that rule and
stay outside EFF-1's paths.

### 3.4 The memory meter

`heap_in_use` (PRE-2, `:2604`, declaration `:3111`) reads a holding that other
contexts change; its concurrent reading "may differ from the holding at every
single instant during the reading by at most the bytes those concurrent
allocations and releases moved", and each reading is an input of the
execution. That is a non-instantaneous read, admitted for a host observation,
not for shared state.

### 3.5 The runtime already shards a counter

The heap accounting design decision chose per-driver 64-bit counters
published with single-writer stores and summed on read, rejecting "one shared
atomic counter, as used by Redis 7.0.15 ... because all drivers would write
one cache line on every allocation and release"
([design/language/system-interface/memory-statistics.md](../../../design/language/system-interface/memory-statistics.md),
fourth decision and rejected list). The implementation keeps one 64-byte line
per allocating thread (`bridge.c:1376–1408`). The language's own runtime
needed per-driver state that no Whitefoot program can express. The
constitution's performance section asks for exactly this case to be revisited:
a restriction that excludes a better-performing implementation meeting the
safety requirements is revised (`docs/constitution.md`, Performance).

### 3.6 Threads, drivers and users

A map's users are threads, not drivers, "at most twice as many as drivers
since a driver's role can move to a spare thread", and "a user per driver"
was rejected "because a displaced thread can still be inside a statement when
its driver's role passes to a spare, so two threads would act as one user"
([design/compiler/waiting-contexts/concurrent-map.md](../../../design/compiler/waiting-contexts/concurrent-map.md),
fifth decision and rejected list). Any per-driver part must therefore stay
correct when two threads reach it; a lock on each part does, a single-writer
store does not.

## 4. Prior positions this study must answer

| Position | Where | Bearing |
|---|---|---|
| "Atomic fields and lock-free cells: rejected because they expose interleavings of single reads and writes inside what the context meaning makes one atomic step." | shared-objects.md, rejected list; io-model/SHARED.md:233 | Applies to any candidate that places separately atomic cells inside a state reached by blocks. A candidate whose every operation is one whole step in SHARE-3's order does not expose an interleaving inside a step. |
| "Transactions that read optimistically and retry: rejected because a block runs once, moves values and writes its context's locals, so it cannot be repeated." | shared-objects.md, rejected list | Applies to (b)'s write-free read route unless the block itself runs once. |
| Shared reads of a map entry selected "instead of ... reads that check a version afterwards, which a block that runs once cannot redo after a torn read." | waiting/shared-objects/keyed-tables.md:11 | The same objection, for keyed reads. Re-proposing a version check must say what changed. |
| "Shards each owned by one context and reached by messages: rejected because every command pays two hand-offs ..." | shared-objects.md, rejected list; shared-state/DESIGN.md:410 | Rejects owner contexts and messages, not parts reached directly under their own hold. |
| "A map sharded into `Shared` objects, each a lock and a table: rejected because ... shards serialize unrelated keys that share a shard." | keyed-tables.md, rejected list | Rejects key-partitioned shards. Per-driver parts partition by who updates, not by key. |
| "An order of store types inferred from every atomic statement of the program: rejected because it needs every function body." | shared-objects.md, rejected list | The same ground refuses whole-program inference of how an object is used. |
| Relaxed scalar fields (`Relaxed<T>`), owner rulings of 2026-10-09; design paused; "reopening restarts at decision 2's controls ... if a workload shows a cost these means cannot recover, such as a hot shared counter." | [relaxed-fields](../relaxed-fields/README.md#controls-and-the-pause-2026-10-09) | This study is that reopening's question. Section 5.1 shows why a relaxed cell does not answer it. |
| Per-target capabilities: native-width load/store minimum, RMW by capability and refused at composition when absent, never a hidden lock (owner, decision 6 of relaxed-fields). | relaxed-fields, owner rulings | Binds any candidate that requires hardware read-modify-write. |
| Embedded weak-CPU targets stay in scope. | relaxed-fields, opening section; constitution, development objectives | A candidate must work on a single-driver target with no hardware RMW. |

## 5. Candidates

### 5.1 Why a relaxed cell is not a fifth candidate

The paused `Relaxed<T>` design (S1) gives single indivisible accesses to a
cell inside a state already held for reading. For a process-wide counter it
does not help on its own: the cell lives in a whole object, a whole object's
read hold today writes the object's lock line (section 2.2), and an atomic
add on one cell is still one cache line written by every driver. It would
inherit (a)'s contention and (b)'s hold cost, plus the mixed event model the
relaxed-fields study left unproved. This study does not reopen it.

### 5.2 (a) Single-operation integer cells

**Surface.** A new prelude handle type whose state is one integer and which
is never an atomic target, with operations that are each one step:

```whitefoot
opaque nocopy struct SharedCell<T: Int> {
}

fn shared_cell_new<T: Int>(value: T) -> result: SharedCell<T> pure;
fn shared_cell_share<T: Int>(cell: &SharedCell<T>) -> result: SharedCell<T> reads(cell);
fn cell_load<T: Int>(cell: &SharedCell<T>) -> value: T reads(cell) waits;
fn cell_store<T: Int>(cell: &SharedCell<T>, value: T) -> result: unit reads(cell) waits;
fn cell_add_wrap<T: Int>(cell: &SharedCell<T>, value: T) -> before: T reads(cell) waits;
fn cell_add_checked<T: Int>(cell: &SharedCell<T>, value: T) -> before: Result<T, Overflow> reads(cell) waits;
fn cell_add_sat<T: Int>(cell: &SharedCell<T>, value: T) -> before: T reads(cell) waits;
fn cell_raise<T: Int>(cell: &SharedCell<T>, value: T) -> before: T reads(cell) waits;
fn cell_lower<T: Int>(cell: &SharedCell<T>, value: T) -> before: T reads(cell) waits;
fn cell_exchange_if<T: Int>(cell: &SharedCell<T>, expected: T, value: T) -> before: T reads(cell) waits;
```

Example, the peak and a command count:

```whitefoot
fn after_command(peak: &SharedCell<u64>, commands: &SharedCell<u64>, used: u64) -> result: unit reads(peak), reads(commands) waits {
  let old_peak = cell_raise::<u64>(cell: peak, value: used);
  let old_count = cell_add_wrap::<u64>(cell: commands, value: 1_u64);
  return unit;
}
```

**Rules.** PRE-1 declarations; a SHARE-1 paragraph: each operation is one
atomic state update that takes effect at one point in SHARE-3's order, as an
unguarded atomic statement whose block is that operation would;
`cell_add_checked` leaves the value unchanged when the mathematical sum is
outside T and returns `Err(error: Overflow())`; `cell_raise` stores
`imax(before, value)`. The operations are declared `waits`, so WAIT-1, PAR-1
and PAR-2 treat them as atomic statements without a new clause, and SHARE-2's
"no waiting call in a guard or block" excludes them from blocks.

**Safety.** No path into the cell exists, so no data race at source level and
no fact about its value: a result is an owned snapshot. There is no exact `+`
form, since no fact about the cell can discharge its domain; `+wrap`, `+checked`
and `+sat` are the declared semantics of OP-2's modes, so nothing overflows
silently.

**Guards and waking.** A guard cannot call a waiting function [SHARE-2], so no
guard reads a cell and no cell update needs to wake a watch. Waiting for a
cell's value is not offered (a writer who needs it uses `Shared<T>`).

**Exclusive statements on the same object.** Impossible by type: a
`SharedCell` is no target type. This is the point of a separate type. The
relaxed-fields study's S2 (`Shared<integer>` statements lowered to machine
atomics) showed that a single-operation lowering on an object that other
statements hold as a block needs a coordination protocol, since a machine
increment can fall between a block's two reads
([S2](../relaxed-fields/README.md#s2--optimize-existing-sharedinteger-statements)).

**Ordering.** Each operation is in SHARE-3's one order, so [HOST-1] and
[WAIT-2] are unchanged. Lowering: sequentially consistent `atomicrmw`/
`cmpxchg`, which on x86 is one lock-prefixed instruction and on AArch64
`LDADDAL`/`CASAL` or an exclusive loop (standard mappings; not checked in
emitted code). `cell_raise` and `cell_lower` can load first and write only
when the value changes, which is still one point (the load) when nothing
changes. On a single-driver target, contexts do not interleave inside a
non-suspending operation, so plain code suffices (inference from WAIT-2's
per-context execution and the absence of interrupt-level code in the
language). On a multi-driver target without hardware RMW, the object's
existing lock implements the same meaning; that is not the refused hidden
lock, because the operation's meaning is an atomic statement's, not a relaxed
cell's.

**Expected cost (estimate, unmeasured).**
- Count: one contended read-modify-write on one line per update instead of the
  statement's several. A contended line moves between cores at a rate that
  does not grow with cores, so the count's rate stays roughly flat in D. It
  should exceed `w_shared` (2.7 at 8 P-cores) several times and stay far
  below `w_private` (348.1). Arm W5 in section 8 prices it with a C loop
  before anything is implemented.
- Maximum: once the peak stops rising, `cell_raise` only loads a line that
  every core holds shared, so it scales like a read, near `w_private`.
- Firn GET peak: near the per-connection check's rate; the heap reading
  remains.

**Implementation cost.** Small to medium: one prelude type and ten functions,
lowering to LLVM atomics, a per-target capability table entry (relaxed-fields
decision 6), conformance cases.

**Generality.** General for single-word coordination: maxima, flags,
identifier allocation (`cell_add_wrap` returning the previous value), CAS-based
handoffs. Weak for hot counts, which is the always-write case.

### 5.3 (b) Shared and write-free read holds, with check-then-update

**Surface.** None new. The writer writes the read-mostly update as two
statements, which is legal today (SHARE-2/3):

```whitefoot
fn raise_peak(peak: &Shared<u64>, used: u64) -> result: unit reads(peak) waits {
  let seen = 0_u64;
  atomic p = &peak^ {
    set seen = p^;
  }
  if used > seen {
    atomic p = &peak^ {
      set p^ = imax(p^, used);
    }
  }
  return unit;
}
```

The second statement rechecks under its exclusive hold through `imax`, so the
result is exact whatever happened between the two statements. Firn's
per-connection `seen_peak` is the same pattern with the read replaced by a
connection-local cache (Firn-wf `firn/commands/eviction.wf`, `raise_peak`, on
branch `claude/peak-per-client` and later main commit `1ee04d3`).

**Implementation routes (no specification change).** SHARE-3's last sentence
admits holding less than exclusive access when outcomes are unchanged, and
the keyed-tables decision already shares read holds on entries without a
source mark (`keyed-tables.md:11`).

- **B1, shared read holds for whole objects.** Lower a whole target whose
  guard and block write nothing with `write = 0`, as entries already are.
  The runtime path exists. It still writes the object's lock and `holders`
  twice per statement (section 2.2), so the read stays bounded by one
  contended line. Expected gain over the read-only row (30.3 at 8 P-cores):
  small. Estimate.
- **B2, a write-free read route.** Each object carries a version word that an
  exclusive unlock advances. A statement with no guard, writing no target,
  whose reads of the state are all inline in the object's allocation (no
  `Box`, no map, no runtime-capacity storage on the path) copies those bytes
  with atomic loads between two version reads, and repeats the copy until the
  version is even and unchanged; then its block runs once, on the copy. After
  a bounded number of failed copies it takes a read hold (B1), so WAIT-2's
  progress promise holds. The copy's validated interval is the statement's
  point, inside its execution, so the outcome is that of an exclusive block
  at that point.

**Answer to the prior objection.** The keyed-tables and shared-objects
refusals concern a block that runs, then learns its reads were torn, and
cannot be redone. B2 never runs the block on racing state: only a bounded
byte copy repeats, and it moves, consumes, allocates and writes nothing the
program sees. What changed is the restriction to inline, pointer-free reads,
which makes the copy finite and safe to discard. B2 still re-proposes a
rejected alternative in narrowed form, so it needs an owner decision on the
keyed-tables node.

**Guards and waking.** B2 applies only to statements without a guard; a
guarded statement keeps the current watch protocol (`state-locks.md`, fifth
decision). Writers wake watches as today.

**Exclusive statements.** Unchanged; each exclusive unlock advances the
version, one store on the line the lock already owns.

**Ordering.** Unchanged meaning. Lowering must keep the version-copy-version
sequence ordered (acquire loads around atomic byte loads, as a seqlock
reader), and an exclusive writer's stores into a state some B2 reader may copy
must not be plain racing stores under LLVM's model; the relaxed-fields
study's bytewise-model note applies
([S1 alias audit](../relaxed-fields/README.md#s1--an-explicit-scalar-cell-type)).
This is an implementation obligation, not yet checked.

**Expected cost (estimate).** B2's read loads a line no one writes while the
peak is stable, so the read-only row should rise toward the `w_private` rate
(arm M2 in section 8). The count (always-write) is unchanged: every update
still takes the exclusive lock.

**Implementation cost.** B1 small (emitter and lowering flag). B2 medium: a
version word per object, a static read-set check in lowering, a copy route
with fallback, and model-correct atomic copies.

**Generality.** General for read-mostly state of any small inline type
(maxima, flags, configuration). No help for counts.

A related extension, **B3**, would let one statement `if c { writes }` run its
write-free condition on a B2 copy and take the lock only when `c` holds,
re-evaluating `c` under the lock. It is sound only when the code before the
first state write writes, moves and allocates nothing; it is not proposed
here, since the two-statement form above already expresses it.

### 5.4 (c) A sharded state: one part per driver, all parts for a read (recommended)

**Surface.** A new state kind `Shards<T>`, held, like a map, only as the state
of a shared object, with two target forms: one shard, which the
implementation selects, and the whole run of shards.

```whitefoot
opaque nocopy struct Shards<T: copy> {
  readonly len: u64;
}

opaque struct AnyShard {
}

fn shared_shards_new<T: copy>(value: T) -> result: Shared<Shards<T>> pure;
fn any_shard() -> result: AnyShard pure;
```

Example: per-command statistics and the heap peak in firn's terms.

```whitefoot
struct Stats {
  commands: u64;
  hits: u64;
  misses: u64;
  peak: u64;
}

fn stats_new() -> made: Shared<Shards<Stats>> pure {
  let zero = Stats(commands: 0_u64, hits: 0_u64, misses: 0_u64, peak: 0_u64);
  return shared_shards_new::<Stats>(value: zero);
}

fn after_command(stats: &Shared<Shards<Stats>>, used: u64) -> result: unit reads(stats) waits {
  let mine = any_shard();
  atomic s = &stats^[mine] {
    set s^.commands = s^.commands +wrap 1_u64;
    set s^.peak = imax(s^.peak, used);
  }
  return unit;
}

fn stats_total(stats: &Shared<Shards<Stats>>) -> total: Stats reads(stats) waits {
  let sum = Stats(commands: 0_u64, hits: 0_u64, misses: 0_u64, peak: 0_u64);
  atomic all = &stats^ {
    for @shard (i in 0_u64..all^.len) {
      set sum.commands = sum.commands +wrap all^[i].commands;
      set sum.hits = sum.hits +wrap all^[i].hits;
      set sum.misses = sum.misses +wrap all^[i].misses;
      set sum.peak = imax(sum.peak, all^[i].peak);
    }
  }
  return sum;
}
```

A command that already holds its key can count its hit in the same
statement, adding one uncontended part to a hold it takes anyway:

```whitefoot
let mine = any_shard();
atomic slot = &store^.keys[key], st = &store^.stats[mine] {
  match slot^ {
    Some(value: entry) => {
      set st^.hits = st^.hits +wrap 1_u64;
      encode_get(value: &entry^.payload, reply: &reply);
    }
    None() => {
      set st^.misses = st^.misses +wrap 1_u64;
      encode_missing(reply: &reply);
    }
  }
}
```

(Fragments in the style of the specification's examples, not compiled.
`Stats` is copy by OWN-1 because its fields are; inferred, not checked.)
`CONFIG RESETSTAT` is a whole-target statement that sets every shard.

**Meaning.** Each shard is a state of type T. A target `&h[a]` with `a` of type
`AnyShard` names one shard's state; which shard it names is an input of the
execution [WAIT-2], as a map position is. A whole target names every shard's
state, and its binding is a run of `len` shards. `len` is at least one, is an
input of the execution, and is fixed for the object's life. Both forms are
ordinary atomic statements: one point, exclusive access to what they name,
one order [SHARE-3]. A one-shard statement and a whole statement on the same
object therefore never overlap, and a whole statement reads every shard at
one point. So a sum computed in a whole statement equals the sum of every
update that took effect before it, and of none after: the same value a single
`Shared<u64>` would hold, with no weaker read semantics.

**Safety.** No new operation or memory model: shards are reached only through
targets, under exclusive access, and every arithmetic step is the writer's,
under OP-2's modes. The writer writes `+wrap` (or a proof, or `+checked`) for
each shard and for the sum. `T: copy` keeps handles, boxes and runtime-capacity
storage out of shards (each of those is `nocopy`), so a shard is inline and a
copy of the initial value is well defined. The token type `AnyShard` carries
nothing; it only selects the target row.

**Guards and waking.** First design: a guard that reads a path into a shards
state is refused (SHARE-2). A guard on one arbitrary shard has no useful
meaning. A guard on the whole run could be admitted later, with every
one-shard write waking its watches, as a write to a map entry wakes the
watches of the whole map (`state-locks.md`, fifth decision); not proposed
until a program needs to wait for a total.

**Exclusive statements on the same object.** Both forms are exclusive over
what they name. Lock order: a shards object is one object of its type; within
it, shards are taken in index order, as a map's entries are taken in sorted
key order (`state-locks.md`, second decision). A one-shard target takes one
shard; a whole target takes every shard in order. No cycle can form: a
one-shard statement waits only for a whole statement, which never waits for a
lock below an already held shard. Several one-shard targets on one handle
place are refused (they may select the same shard), as is a one-shard target
beside a whole target on that place, matching SHARE-2's rule for whole map
targets. Two handle places that may name one object follow SHARE-2's existing
"possibly one root" comparison, so two one-shard targets through different
handles of one type are treated as possibly the same shard.

**Ordering.** Unchanged: shard statements are atomic statements, hence
waiting calls without overlap permission [PAR-1, PAR-2]; [HOST-1] orders host
effects through state both reach, and a whole statement reaches every shard.

**Implementation sketch.** The object holds `len` shards, each on its own
cache line with its own lock word, after one header with the handle count.
The runtime's shard index is the running thread's user index (the concurrent
map's numbering, `concurrent-map.md`, fifth decision) modulo `len`; `len` is
the thread inventory the runtime may run contexts on, at most `2 × drivers`.
Because a displaced thread and its spare may meet on one shard, each shard
keeps its lock (section 3.6). A one-shard statement must not write the header:
the handle count must be skipped for it, which requires the compiler's
existing skip to cover value handles that the block neither moves nor
writes, or the shard path will contend on the header (an implementation
obligation; see the found-along-the-way list). On a single-driver target,
`len` is one, and the object is a `Shared<T>` in all but name.

**Expected cost (estimate, unmeasured).**
- Count, `w_shared` rewritten with one-shard statements: each update is an
  uncontended statement on a line its own thread owns, the same work as
  `w_private`, so near 89.6 / 171.4 / 348.1 / 438.5.
- Maximum: the same, or less than one statement when combined with a cached
  local value.
- Read: `len` lock acquisitions and a loop, a few microseconds at `len` 32
  (estimate); it delays one-shard updates only while it runs.
- Firn: per-command statistics counted inside the statement GET already
  takes add one uncontended lock per command; expected loss within a few
  percent of the uncounted build (provisional prediction P5 in section 8).

**Memory.** `len × 64` bytes per object at least (2 KiB at `len` 32), plus T's
size per shard. Suited to process-wide statistics, one object holding all of
them; unsuited to a per-key counter.

**Implementation cost.** Medium: two prelude types and two functions; SHARE-2
target rows, OP-4 subscript of a whole binding, a placement row; lowering of
the two target forms; the runtime's shard selection and in-order whole hold
(reusing the per-object lock machinery per shard); diagnostics; conformance
cases and a program test. No new memory model, no target capability, no LLVM
atomic beyond the existing locks.

**Generality.** General for any state whose updates commute or that a reader
can combine: counts, maxima and minima, per-command tables, histograms,
per-thread free lists. It is the Whitefoot form of kernel per-CPU data
(Linux `DEFINE_PER_CPU`, `percpu_counter`), which Whitefoot's target projects
use (constitution, development objectives: kernels, compilers, browsers). It
does not serve an update that needs the global value back at once, such as
allocating the next identifier; that remains a `Shared<T>` statement or (a).

**Variant (c′), hidden combine.** A `Tally<T>` type whose shards are invisible,
offering `tally_add_wrap`, `tally_raise` and `tally_read` with fixed sum or max
combine. Simpler to read, the same scalability, the same linearizable read.
Not preferred: each counter costs its own `len` lines, so Redis's per-command
table (about 240 commands × 4 counts) would take about 240 × 4 × 32 × 64 bytes,
1.9 MiB at `len` 32 and 7.5 MiB at `len` 128, against one object of `len` × `size(Stats)` for (c); the
combine is fixed per type, and a struct of several statistics cannot be
updated in one step.

### 5.5 (d) Other designs

**(d1) Per-context accumulation, flushed.** Each connection counts in its own
owned state and adds its count to one `Shared<Stats>` every k commands and
when it closes. Existing language. Cheap. It changes what a reader sees: another
connection's INFO misses up to k − 1 commands per connection, so the reported
total is not a value the counter held at one point, and read-your-writes for
one's own connection needs a flush before INFO. Firn's `seen_peak` is the exact
variant for a maximum, where a stale cache costs only an extra statement. For
counts it is a policy change, not a language one. Not recommended as the
general answer; acceptable where the program states that loss.

**(d2) A writer-built shard array.** Existing language: an
`Array<Shared<u64>, 16>` of handles, each context incrementing the element
`index mod 16`, the reader writing one statement with sixteen targets
`atomic s0 = &shards[0_u64], s1 = &shards[1_u64], ...` to read all at one point
(several targets of one type are admitted and taken in identity order,
SHARE-2 and `state-locks.md`, second decision). It is linearizable and cheap
when each element is used by one driver. Its limits: contexts, not drivers,
select elements, so two drivers share an element whenever their contexts map
to it (WAIT-2 hides the driver); the shard count is a source constant
written out in a header; and the handle traffic of value handles applies.
This is the existing-language control the experiment must beat (arm W3).

**(d3) A runtime-maintained peak in `MemoryMeter`.** A host function
`heap_peak(meter)` tracking the highest holding. Rejected: the true peak of a
sum of per-thread counters cannot be computed from per-thread maxima, so an
exact peak needs a global write at every allocation, the cost the
memory-statistics decision refused; and Redis records the maximum of sampled
readings after commands, not the allocation-level peak, so the reported value
would differ from Redis's. Special to one statistic.

**(d4) Whole-program detection of commute-only use.** The compiler could shard
a `Shared<u64>` whose every statement in the program only adds or reads.
Rejected on the ground already recorded for inferred lock orders: it needs
every function body, and a library change elsewhere would silently change an
object's representation and cost.

**(d5) A lock that does not collapse.** A queue or handoff lock for contended
whole objects, so `w_shared` holds a plateau near its single-holder rate
instead of falling to 0.7. A runtime improvement worth having for every
contended object (Whitefoot branch `claude/shared-take-fair` addresses a
related overtaking case), but it serializes by construction and cannot reach
the private rate. Orthogonal to the language question; recorded below.

## 6. Comparison

| | (a) cells | (b) read routes | (c) shards | (d2) writer shards |
|---|---|---|---|---|
| New surface | type + 10 functions | none | type, token, 2 target forms | none |
| Count rate vs `w_private` (estimate) | far below; flat in D | unchanged (exclusive) | near | near when no two drivers share an element |
| Maximum rate (estimate) | near (load-first) | near (B2) | near | near |
| Read meaning | one point | one point | one point | one point |
| New memory model | no | no | no | no |
| Needs hardware RMW | for speed; lock fallback keeps meaning | no | no | no |
| Guards | not applicable | unchanged; B2 guard-free only | refused on shard state | unchanged |
| Specification change | yes | no | yes | no |
| Design-tree change | yes | yes (keyed-tables rejection narrowed) | yes | no |
| Serves counts at scale | no | no | yes | partly |
| Serves next-identifier allocation | yes | yes (exclusive) | no | no |

## 7. Recommendation

**Adopt (c), the sharded state, for always-write statistics; keep the
existing check-then-update forms for maxima; defer (a) and (b).**

Decisive reason: (c) is the only candidate whose update path writes no line
another driver writes, which the count witness requires (same-object
increments fall to 2.7 M/s at 8 P-cores while per-context objects reach
348.1), and it gets there inside SHARE-3's existing one-point, one-order
meaning, with no new memory model, no target capability and no change for
single-driver embedded targets. It also expresses a structure the language's
own runtime needed and the memory-statistics decision chose for the same
contention reason (section 3.5).

For maxima no language change is needed now: the per-connection check (or
the two-statement form of 5.3) keeps the peak exact and makes the shared
update rare, and Firn's remaining gap is the heap reading, which
`claude/heap-read-nolock` addresses. (c) also expresses the peak exactly, in
one uncontended statement, when a program prefers that.

Prior objections answered:
- *Atomic fields and lock-free cells*: (c) has none; every access is an
  ordinary atomic statement.
- *Optimistic transactions*: (c) never retries a block.
- *Shards owned by contexts, reached by messages*: no owner, no message; each
  shard is reached directly under its own hold.
- *A map sharded into objects*: shards partition by updater, not by key, so
  unrelated keys never share a shard.
- *Inference from every function body*: the writer chooses the type.
- *The paused relaxed field's reopening condition* ("a hot shared counter"):
  answered without relaxed semantics, so that design stays paused.

(a) stays a candidate for single-word coordination at high rate, such as a
per-request identifier, when a program needs it; (b)'s B2 stays a candidate
compiler improvement for read-mostly whole objects, with its owner decision
on the narrowed version-check refusal. Neither is needed by the present
witnesses.

Confidence 3/5. The semantic argument rests on reading SHARE-2/3 and the
runtime; the cost estimates are unmeasured. The experiment below could
overturn it: if the writer-built shards (d2) come within 20% of (c), the
language change buys too little; if a contended hardware add reaches half
the private rate, (a) would serve counts more cheaply.

## 8. Registered experiment (written before any measurement)

### 8.1 Question and comparison

Does the sharded state let 64 contexts update one logical count at close to
the rate they update private objects, at 2, 4 and 8 P-cores and 16
hyperthreads of the i9-14900K, by a margin the existing-language shard array
does not reach; and does it keep Firn's GET within a small loss when Firn
counts per-command statistics?

The comparison could answer either way: (c) could fall short of the private
rate (shard selection, header writes, the reader), or (d2) could match it.

### 8.2 Arms

Witness arms, built in one Whitefoot experiment release that implements (c)
(release `wf-exp-<12 hex>` from the implementing branch), ported from the
Firn-wf runtime-contention harness (`6c0e973`: its gate, deadline, batches of
1024 operations, local LCG step and totals) into Whitefoot
`research/experiments/shared-counters/`, 64 contexts, each cell 5 s:

| Arm | Per operation | Role |
|---|---|---|
| W0 `base` | none | runtime floor |
| W1 `private` | `+wrap 1` on the context's own `Shared<u64>` | ceiling for an uncontended statement |
| W2 `shared` | `+wrap 1` on one `Shared<u64>` through a value handle | base; equals the witness row 1 |
| W2t | W2 rebuilt from the same source as a separate build | noise control (twin) |
| W2r | W2 with the handle reached through a read-only reference parameter | attributes the handle count |
| W3 `writer-shards` | `+wrap 1` on element `index mod 16` of an `Array<Shared<u64>, 16>` | existing-language control (d2) |
| W4 `shards` | `+wrap 1` in a one-shard statement on one `Shared<Shards<u64>>`, value handle | the proposal |
| W4r | W4 through a read-only reference parameter | separates a header-write defect from the design |
| W6 `read-shared` | read-only statement on one `Shared<u64>` | ties the reported row 3 to a source |

W3, W4 and W4r run with and without one extra reader context that every
millisecond reads the total at one point (W3: its 16-target statement; W4: a
whole target), reporting the reader's statement time.

Maximum arms, same harness, value `v = state mod 2^20` from the local LCG, so
that after warm-up almost no operation raises the maximum:

| Arm | Per operation |
|---|---|
| M1 | exclusive `imax` statement on one `Shared<u64>` |
| M2 | read statement, then exclusive `imax` only when `v` exceeds the value read (5.3) |
| M3 | context-local cache, exclusive `imax` only above it (Firn's pattern) |
| M4 | one-shard `imax` statement on `Shared<Shards<u64>>` |

Calibration arm W5, a C program outside Whitefoot, pinned the same way:
threads incrementing one 64-byte-aligned word with `atomic_fetch_add`
(sequentially consistent), against the same threads each incrementing its own
line (W5p). It prices (a) without implementing it.

Firn arms (requested from Firn's session; GET and SET at depth 16, 50
connections, server on 8 P-cores and on 16 hyperthreads, client on E-cores
16–31):

| Arm | Build |
|---|---|
| F0 | Firn main at the time of the run, built with the experiment release; twin F0t rebuilt from the same source |
| F1 | F0 plus `total_commands_processed`, `keyspace_hits`, `keyspace_misses` counted in one exclusive statement on a `Shared<Stats>` per command |
| F2 | the same counts in a writer-built array of 16 `Shared<Stats>` by connection id |
| F3 | the same counts in `Shared<Shards<Stats>>`, inside the statement GET already takes where it has one |
| F-heap | F0 with the heap reading removed and the peak update kept as in F0 |

### 8.3 Procedure

- Machine: the 14900K self-hosted runner through CI only, checked idle before
  the run, other sessions told before a long run. Witness CPU lists `0,2`,
  `0,2,4,6`, `0,2,4,…,14`, `0-15`, `WF_DRIVERS` equal to the list's count, as
  the source harness does; record the actual placement and P/E-core and SMT
  mapping in the results.
- Pilot first: one block of every arm at the 8-P-core list, timed, to see the
  spread and the run length.
- Then interleaved blocks: each block runs every arm once per CPU list in an
  order rotated by one position per block. Six blocks (provisional; the pilot's
  spread may change it before the main run, and the change is recorded).
- Every arm builds from one Whitefoot experiment release with `--full-lto`;
  W2 is also built with the main release the experiment branched from, to
  show the implementation did not move the base.
- Report per arm and CPU list the median and the range over blocks.
  Noise `e` is the largest `|median(W2t) / median(W2) − 1|` over the CPU lists
  (F0t/F0 for Firn). A ratio below is decided only when it clears its
  threshold by more than `e`; with `e > 0.03` (provisional) the cell is
  inconclusive. At most two corrective reruns, never a wider threshold.

### 8.4 Predictions and rejection conditions

All thresholds are self-chosen and provisional.

- **P1.** W4 ≥ 0.8 × W1 at 2, 4 and 8 P-cores; reported at 16 hyperthreads.
- **P2.** W4 ≥ 1.25 × W3 at 8 P-cores and at 16 hyperthreads.
- **P3.** W5 / W5p ≤ 0.25 at 8 P-cores (a contended hardware add stays far
  below private increments).
- **P4.** With the 1 kHz reader, W4 keeps ≥ 0.95 of its rate without the
  reader, and the whole-target read takes ≤ 50 µs at the 8-P-core list.
- **P5.** F3 GET ≥ 0.97 × F0 at 8 P-cores; F1 GET ≤ 0.5 × F0.
- Descriptive, no rejection: M1–M4, W2r against W2, W6, and F-heap against F0.

The recommendation is rejected if:

- **R1** W4 < 0.8 × W1 at 8 P-cores while W4r ≥ 0.8 × W1 does not explain it
  (a design cost, not a header-write defect);
- **R2** W3 ≥ 0.8 × W4 at every CPU list: the existing-language shard array
  serves counts nearly as well, and the language change is reduced to an
  ergonomic one, to be written as a pattern in `docs/patterns.md` instead;
- **R3** W5 / W5p ≥ 0.5 at 8 P-cores: a single-word hardware add then serves
  counts well enough, and (a), which also serves identifiers, becomes the
  recommendation;
- **R4** P4 fails: the read's cost or interference is too large for an INFO
  that may run often;
- **R5** F3 < 0.97 × F0 at 8 P-cores, or F2 within `e` of F3.

A W4r ≥ 0.8 × W1 with W4 < 0.8 × W1 is an implementation defect (header
writes on one-shard statements) to fix before the result counts; it does not
reject the design.

## 9. Specification changes (proposal only)

For the recommendation (c). Nothing here is applied; `spec/kernel-spec.md`
is unchanged.

| Rule | Change |
|---|---|
| PRE-1 (`:2343`, declarations `:2376–2396`, `:2543–2559`) | Add `opaque nocopy struct Shards<T: copy> { readonly len: u64; }`, `opaque struct AnyShard { }`, `fn shared_shards_new<T: copy>(value: T) -> result: Shared<Shards<T>> pure;` and `fn any_shard() -> result: AnyShard pure;`, with their diagnostic ordinals. |
| TYPE-2 (`:411–416`) | The opaque values the build forms through SHARE-1 functions include shards states and the `AnyShard` token. |
| TYPE-9 (homes table, `:571–574`) | Add a row: `Shards<T>` has one home, the state of a shared object, the type argument of `Shared<·>` or `SharedRead<·>`; not the exchange of a `swap`. |
| OP-4 (`:1065`) | Add `Shards<T>`, reached through `^`, to the indexable bases, its element type T, so a whole binding's shards are `all^[i]`. |
| SHARE-1 (`:2276`) | Define a shards state: `len` shards, each a copy of the value `shared_shards_new` takes; `len` at least one, an input of the execution, fixed for the object's life. |
| SHARE-2, target table (`:2311–2316`) | Add two rows: a `Shared<Shards<T>>` or `SharedRead<Shards<T>>` handle place with an index atom that is a place of type `AnyShard` binds `&T`, whose path is the shard the statement selects; the same handle place whole binds `&Shards<T>`. |
| SHARE-2, target overlap (`:2320`) | One handle place carries at most one `AnyShard` target and none beside a whole target on it; two `AnyShard` selections through handles that may name one object compare as possibly one shard. |
| SHARE-2, guard | A guard whose footprint reads a path into a shards state is a hard error citing SHARE-2 at the guard `expr`, with a repair. |
| SHARE-3 (`:2333`) | No change in wording; "the states its targets name" covers one shard or every shard. |
| WAIT-2 (`:2259`) | Add the number of shards of each shards object and the shard each `AnyShard` target selects to the inputs of the execution. |
| CAP-1, PAR-1, PAR-2, HOST-1, EFF-1–5, WAIT-1 | No change: shard statements are atomic statements, hence waiting calls without overlap permission, and shards are reached only through targets. |
| Conformance and programs | New cases: acceptance of both target forms and of a shard target beside a map entry; refusal of two `AnyShard` targets on one place, of one beside a whole target, of a guard reading a shard, of `Shards` outside its home, of a non-copy T; a program test whose 16 contexts count and whose whole read is exact after the joins. |

For (b): no specification change (SHARE-3's last sentence). For (a), if
adopted later: PRE-1 declarations of `SharedCell<T: Int>` and its ten
functions, a SHARE-1 paragraph giving each operation's single-point meaning
and `cell_add_checked`'s unchanged-on-overflow rule, and the target capability
row of relaxed-fields decision 6 for the hardware route.

## 10. Design-tree changes it would need (proposal only)

- `design/language/waiting/shared-objects.md`: a decision for sharded states,
  its grounds (section 7), and rejected entries for (c′) hidden combine, (d3)
  a runtime peak and (d4) whole-program detection, each with its reason here.
- `design/compiler/waiting-contexts/state-locks.md`: shards of one object are
  taken in index order; a one-shard target takes the running thread's shard,
  each shard keeping its own lock because two threads may share it.
- For (b) B2, if pursued: `design/language/waiting/shared-objects/keyed-tables.md`'s
  refusal of version-checked reads narrowed to blocks that run on racing
  state, with the account in 5.3; owner decision required.

## 11. Found along the way

- **The heap reading takes a global lock.** `wf__heap_in_use` takes the pool's
  spin lock on every reading (`bridge.c:1412–1414`), which Firn's perf c2c
  located as the hottest contended line. Disposition: being fixed on
  `claude/heap-read-nolock`. Its per-thread sum still reads up to 193 lines,
  each written by its owner; that remainder is unmeasured.
- **Whole-object read-only statements take write holds.** The emitter passes
  `write = 1` for every object acquire, take and unlock
  (`emitter/shared.rs:1027,1059,1075`), while SHARE-3's last sentence and the
  runtime admit read holds. No design-tree decision states sharing for whole
  objects; the keyed-tables decision covers entries only. Disposition:
  deferred to (b); worth a board item, since every read-only statement on a
  hot object pays it.
- **The handle count on every statement through a value handle.** It writes
  the object header twice per statement; the compiler's skip covers only
  read-only reference parameters. SHARE-2 requires only that the object stay
  live until the statement completes, which a block that neither moves nor
  writes the handle place guarantees without counting. Disposition: deferred;
  a prerequisite of (c)'s cost (arm W4 against W4r).
- **Contended objects collapse rather than plateau** (9.6 to 0.7 M/s). The
  object lock is a test-and-set spin lock with 256 spins before parking.
  Disposition: recorded as (d5); `claude/shared-take-fair` touches a related
  case.
- **The read-only witness has no located source.** The reported row 3 is not
  in the branch's witness list; arm W6 supplies one.
- **Name collision to keep in mind.** OP-12 calls `set p = f(move p, ...)` "the
  atomic in-place update" (`:1215`), an ownership property unrelated to
  concurrency. A surface added by this study should not use "atomic" for its
  operations.

## 12. What is established and what is not

Established by reading the specification, design tree and source at
`4ad4f5786`: the rules in section 3; that whole-object statements take write
holds and that even read holds write the object's line; the handle-count
traffic; the heap reading's lock; the runtime's own per-thread counters and
the decision that chose them; the prior refusals in section 4; and that (c)
needs no new memory model because its targets are ordinary atomic statements.
The witness and Firn numbers are quoted from the cited runs and records, not
reproduced.

Unverified: every cost estimate in sections 5 and 6; the attribution of
Firn's 19.21 to 26.99 gap to the heap reading (F-heap measures it); B2's
LLVM-model correctness; that `Stats` in the examples is copy; that the
fragments compile; the reader's latency; the shard selection's correctness
under a driver handoff to a spare thread (argued from per-shard locks,
section 3.6, not tested).

