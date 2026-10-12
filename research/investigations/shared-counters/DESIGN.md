# Shared counters and maxima without an exclusive hold per update

## Status

A design study, then the measurements the owner asked for before a language
change, then a drafted change. Sections 1 to 8 were written before any
implementation or measurement and check the active specification
`spec/kernel-spec.md` (title v0.134) and the compiler and runtime at
Whitefoot `4ad4f5786` (main); line numbers there cite that revision.

The owner chose (board card on what Whitefoot should offer for a count many
cores update, 2026-10-12, option A) to measure three things without a
language change first. They ran on the 14900K on 2026-10-12 and
[section 9](#9-results-the-arms-that-need-no-language-change) records them:
the hand-built array of 16 shared objects reaches 0.31 to 0.66 of the rate of
private objects, a contended hardware add reaches 0.059 of private lines at 8
P-cores, and Firn now reads memory usage at about 5% of its GET rate. Neither
the writer pattern nor single-step cells is selected, so the ruling's third
outcome applies: the sharded state. Its specification change is drafted on
the work branch as v0.136 over main's v0.135
([section 10](#10-specification-changes)), its design-tree decisions are
proposed ([section 11](#11-design-tree-changes)), and its implementation is
planned, not done ([section 12](#12-implementation-plan)). All of it awaits
the owner's approval card; the drafted conformance cases fail until the
compiler implements the change.

Every number below is either quoted from a cited run, measured in a cited
run, or marked as an estimate. Where this study infers instead of reading, it
says so.

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

## 9. Results: the arms that need no language change

The owner chose, on the board card that asked what Whitefoot should offer for
a count many cores update (2026-10-12, option A), to take the sharded-state
direction but first to measure three things without a language change: the
hand-built array of 16 shared objects (W3), a hardware atomic add in C (W5
against W5p), and Firn without reading memory usage (F-heap). The ruling fixed
three outcomes: if the hand-built array is enough (R2), write it as a writer
pattern; if the hardware add reaches half the rate of each thread on its own
line (R3), build single-step integer cells (a) instead; if neither, implement
the sharded state, its specification change on its own approval card. This
section records what the runs show and which outcome they select.

### 9.1 Runs, host and noise

Two runs of the temporary workflow on the 14900K runner, both with compiler
release `wf-29de8a21eef8`, `/usr/bin/clang` 22.1.8, kernel 7.0.0-38, governor
`performance`, 64 contexts (64 threads for W5 and W5p), 5 s per cell:

- pilot, [run 38303980083](https://github.com/Ming-Research/Whitefoot/actions/runs/38303980083):
  one block of every arm at the 8-P-core list;
- main, [run 38304337014](https://github.com/Ming-Research/Whitefoot/actions/runs/38304337014):
  six blocks, each running every arm once per CPU list in an order rotated by
  one position per block. The pilot's spread did not change the registered
  six blocks.

Their `raw.tsv`, `host.txt` and `summary.txt` are kept in
[`research/experiments/shared-counters/results/`](../../experiments/shared-counters/results/).
Every rate below is the `ops_per_s` column of the main run's `raw.tsv`,
divided by 10^6, as the median over the six blocks with the lowest and highest
block in brackets; every ratio is of two such medians. The CPU lists are
`0,2`, `0,2,4,6` and `0,2,4,…,14` (one hyperthread of each of 2, 4 and 8
P-cores) and `0-15` (8 P-cores, both hyperthreads), with `WF_DRIVERS` equal to
the list's count; `host.txt` records the CPU map.

The registered noise measure is `e = max |median(W2t) / median(W2) − 1|` over
the lists (section 8.3), W2t being W2's source built a second time into a
byte-identical binary (`host.txt` records both hashes). The main run gives
`e = 0.091` (per list 0.938, 1.091, 0.965, 0.970); the pilot gave 0.089. That
exceeds the provisional limit of 0.03, so `summarize.py` prints every
registered condition as inconclusive: that is the registered rule's mechanical
output, and this record leaves the rule as written. The noise is the
contended arm's own: W2's blocks span 9.34 to 12.43 at 2 CPUs, while the
uncontended arms vary little (W1's blocks lie within 2.7% of each other at
every list, W5p's within 0.2%), so applying `e` to every arm overstates their
noise. Section 9.6 states, for each decision, how far the measured ratio lies
from its threshold in units of `e`.

### 9.2 The hand-built array

W3 is the array of 16 `Shared<u64>` a writer can build today (control d2):
each context adds one to element `index mod 16`. W1, each context on its own
object, is the ceiling an uncontended statement reaches; W2, every context on
one object, is the contended baseline.

Table 9.2a. Rate of 64 contexts updating one logical count, million updates per
second summed over the contexts (higher is better). Source: run 38304337014,
`raw.tsv` column `ops_per_s`, arms W1, W2 and W3.

| CPU list (drivers) | W1: own object per context, M/s (ceiling) | W2: one shared object, M/s (baseline) | W3: array of 16 shared objects, M/s [min–max] | W3 / W1 (registered stand-in for R2: ≥ 0.8 would make the array enough) | W3 / W2 (baseline 1) |
|---|---:|---:|---:|---:|---:|
| 2 CPUs (2) | 89.44 | 11.08 | 58.87 [43.81–65.94] | 0.658 | 5.3 |
| 4 CPUs (4) | 171.82 | 4.48 | 82.78 [81.24–84.11] | 0.482 | 18.5 |
| 8 P-cores (8) | 346.01 | 2.81 | 175.09 [139.50–177.92] | 0.506 | 62.3 |
| 16 hyperthreads (16) | 437.88 | 0.72 | 134.94 [95.11–150.28] | 0.308 | 186.1 |

The array is 5 to 186 times the single shared object, so splitting the count
removes most of the collapse. It stays at 0.31 to 0.66 of the private rate, and
its best block stays below 0.8 of W1's median at every list (0.74, 0.49, 0.51,
0.34). Unlike W1, it also falls from 175.1 at 8 P-cores to 134.9 at 16
hyperthreads. Two causes are
consistent with the source, neither separated by these arms: contexts, not
drivers, choose elements, so 64 contexts on 16 drivers put about four contexts
on each element and drivers meet on elements (section 5.5, d2); and each W3
statement still counts its handle, since the emitted LLVM of W3 calls
`wf__shared_share` as often as W2's does (4 calls against W2r's 3, `host.txt`),
so every update writes the element object's header too. That second cause is
board item gran-skip-handle-count; W2r / W2 = 1.06, 1.66, 1.22 and 1.20 (Table
3 of `summary.txt`) prices the handle count on the one shared object.

Table 9.2b. The array with one reader context that every millisecond reads all
16 elements in one statement of 16 targets (W3rd). Source: run 38304337014,
`raw.tsv` columns `ops_per_s`, `reads`, `read_ns` and `read_max_ns`, arm W3rd.

| CPU list | W3rd: rate with the reader, M/s | W3rd / W3 (ideal 1; registered P4 asked ≥ 0.95 of the sharded state) | mean read, µs (read_ns / reads, median over blocks; P4 asked ≤ 50) | longest read, µs (median over blocks of the longest) | reads per 5 s cell (about 5000 expected) |
|---|---:|---:|---:|---:|---:|
| 2 CPUs | 46.19 | 0.785 | 0.6 | 2.6 | 4122 |
| 4 CPUs | 84.90 | 1.026 | 1.2 | 9.6 | 4534 |
| 8 P-cores | 120.18 | 0.686 | 2.5 | 506.2 | 3998 |
| 16 hyperthreads | 105.75 | 0.784 | 5.6 | 22.1 | 3994 |

A read of all sixteen at one point costs microseconds on average, and the
reader falls behind its period by 9 to 20%. The writers lose up to 31% of
their rate while it runs, more than the twin's 9% noise. P4 was registered for
the sharded state (W4), not for W3, so these rows are descriptive.

### 9.3 The hardware atomic add

W5 is a C program whose 64 threads add one to a single 64-byte-aligned
`_Atomic uint64_t` with `atomic_fetch_add` (sequentially consistent); W5p is
the same program with each thread on its own padded line. It prices candidate
(a) without implementing it.

Table 9.3. Rate of 64 C threads, million increments per second summed over
the threads (higher is better). Source: run 38304337014, `raw.tsv` column
`ops_per_s`, arms W5 and W5p, with W1 and W3 from Table 9.2a.

| CPU list | W5p: own padded line per thread, M/s (ceiling) | W5: one shared line, M/s [min–max] | W5 / W5p (R3: ≥ 0.5 at 8 P-cores would select cells (a); P3 predicted ≤ 0.25) | W5 / W1 (Whitefoot statement on own objects = 1) | W5 / W3 (hand-built array = 1) |
|---|---:|---:|---:|---:|---:|
| 2 CPUs | 498.51 | 142.54 [110.94–150.26] | 0.286 | 1.59 | 2.42 |
| 4 CPUs | 997.04 | 124.86 [107.64–143.46] | 0.125 | 0.73 | 1.51 |
| 8 P-cores | 1993.71 | 118.06 [105.12–127.56] | 0.059 | 0.34 | 0.67 |
| 16 hyperthreads | 1669.50 | 98.88 [85.42–102.34] | 0.059 | 0.23 | 0.73 |

The contended add stays near 100 to 140 million a second whatever the CPU
count, as one cache line moving between cores does, while the private lines
grow with the P-cores. At 8 P-cores it reaches 0.059 of the private rate, and
below the hand-built array at 8 P-cores and at 16 hyperthreads. W5 and W5p run
64 OS threads time-sliced on the list's CPUs, where the Whitefoot arms run 64
contexts on as many drivers as the list has CPUs; the comparison is of the
cost of one contended or private increment, not of the schedulers.

### 9.4 Firn without reading memory usage

The registered F-heap arm (Firn with the heap reading removed and the peak
update kept) was not built. Firn's session measured the question it asks
during the acceptance of the lock-free heap reading (Whitefoot#381), recorded
on the board item for that acceptance (coord-q114, log entry of
2026-10-12T10:16:50Z), and section 2.1 quotes the build with no peak at all.

Table 9.4. Firn GET at pipeline depth 16, server on 8 P-cores, million
requests per second (higher is better); run 38234681969 reports the median of
4 rounds of 10 s per build.

| Firn build | Compiler | GET, M/s | Twin, M/s | Source |
|---|---|---:|---:|---|
| exclusive peak statement after every command | release of the time | 2.98 | — | Firn-wf run 38227806896 (section 2.1) |
| peak statement only above the connection's cached peak | release of the time | 19.21 | — | Firn-wf run 38227806896 (section 2.1) |
| the same | current release | 18.27 | 17.38 | Firn-wf run 38234681969 (coord-q114) |
| the same | experiment release with the lock-free heap reading | 25.65 | 25.74 | Firn-wf run 38234681969 (coord-q114) |
| no peak recording (the ideal) | release of the time | 26.99 | — | Firn-wf run 38227806896 (section 2.1) |

With the lock-free reading, Firn keeps the exact peak at 25.65 / 26.99 = 0.950
of the build that records no peak. The two numbers come from two runs, not
from interleaved blocks of one run, so the gap of about 5% is indicative only:
it is the size of the spread between the twins of the current-release build
(18.27 against 17.38) in the same run. Within that precision, reading memory
usage after each command now costs Firn little, and section 2.2's inference
holds: most of the gap between 19.21 and 26.99 was the heap reading's lock,
which the lock-free reading removed, not the shared update.

### 9.5 The other arms

- W0, the loop with no atomic statement, reports 99.7 to 674 thousand million
  operations a second (`raw.tsv` column `ops`, about 5 × 10^11 operations per
  5 s at 2 CPUs), far beyond what a loop doing one operation per step can
  reach; the optimizer has evidently reduced its batch to a closed form. It is
  a floor that measures nothing here, and no reading uses it.
- W2 and W1 reproduce section 2.1's witness rows (W2 11.08, 4.48, 2.81, 0.72
  against 9.6, 5.5, 2.7, 0.7; W1 89.44, 171.82, 346.01, 437.88 against 89.6,
  171.4, 348.1, 438.5), so the port of the harness did not move them.
- W6, a statement on the one object that writes nothing, runs at 13.32, 5.74,
  2.99 and 0.81: 1.06 to 1.28 times W2, collapsing as W2 does. Section 2.1's
  read-only row (27.4 to 41.9) is not reproduced by this source, whose handle
  is a value handle as W2's; that row's source remains unlocated (section 13).

### 9.6 What the results decide

- **R2, approximated** (the hand-built array is enough when W3 ≥ 0.8 × the
  sharded state at every list). With no sharded state, W1 stands in for its
  upper bound, which decides R2 in one direction only. W3 / W1 is below 0.8 at
  every list: by 18% of the threshold at 2 CPUs (about twice `e`), 40% at 4, 37%
  at 8 and 62% at 16 hyperthreads (6.8 times `e`). The stand-in therefore does
  not show the array to be enough. R2 itself stays undecided until the sharded
  state is measured (W4): if W4 is at most 1.25 times W3 at every list, R2
  then rejects the language change after its implementation.
- **R3** (cells (a) are recommended when W5 / W5p ≥ 0.5 at 8 P-cores). W5 /
  W5p = 0.059, 88% below the threshold (9.7 times `e`); the most favourable
  pairing of blocks, W5's highest against W5p's lowest, gives 0.064. Not met.
- **P3** (W5 / W5p ≤ 0.25 at 8 P-cores) holds, 76% below its bound.
- **F-heap** (descriptive): Firn reads memory usage at about 5% of its GET
  rate, as section 9.4 qualifies.

Against the owner's ruling: the hand-built array is not shown to be enough
and the hardware add reaches 0.059 of the private rate at 8 P-cores, not
half, so neither of the first two outcomes holds, and the third applies: implement
the sharded state, with its specification change on its own approval card.
The draft of that change is section 10, its design-tree decisions section 11
and its implementation plan section 12. The maximum needs no language change
(section 7), which section 9.4 now supports with a measurement.

## 10. Specification changes

For the recommendation (c), drafted on the work branch as specification
v0.136 over main's v0.135 (archived byte for byte as
`spec/kernel-spec-v0.135.md`). Every change awaits the owner's approval card;
no `spec/log.md` entry exists yet. The surface is section 5.4's with one name
changed: the token is called the *shard selector*, and a target naming one
shard a *shard target*.

### 10.1 Rule by rule

| Rule | Before (v0.135) | After (v0.136) |
|---|---|---|
| TYPE-2 | The functions of SHARE-1 form the shared and frozen handles, maps and key sets. | They also form shards states and the shard selector. |
| TYPE-9 | Homes table: storage forms in a `Box`; a map in a shared object's state or a `swap`'s exchange. | Adds `Shards<T>`, whose one home is a shared object's state, not a `swap`'s exchange; the consequence sentence says a `swap` of two whole shards targets is refused. |
| TYPE-10 | `len`, `cap`, `head` are the readonly fields of the storage shapes, the key set and the entries. | Adds the shards state's `len`. |
| REF-4 | A range reference over a `Ring` or an `Entries` is refused with its reason. | Adds the refusal of a range reference over a `Shards`, whose shards are kept apart and are no run of storage. |
| OP-4 | Indexable bases include `Entries<V>` and `ConcurrentHashMap<V>`, reached through `^`. | Adds `Shards<T>`, reached through `^`, its elements its shards of type T. The existing sentences then give it a `u64` offset and the obligation `i < p.len`, so an `AnyShard` offset is refused. |
| OP-9 | Layout pairs of the prelude's opaque structs; any other fieldless one `(32,16)`. | Adds `Shards<T>` `(8,8)`, its `len`, its shards living in its object and entering no sequence, and `AnyShard` `(1,1)`, carrying nothing. |
| MSR-1 | Measures table rows. | Adds `Shards<T>`: `len` counts shards, exact; no `cap`, `head` or `bytes`. |
| SHARE-1 | Shared and frozen objects, maps and key sets. | Adds the shards state: `len` shards of T; `shared_shards_new` copies its argument into every shard; `len` is at least one, fixed for the object's life and an input of the execution that an implementation may choose by the driver count; a shard target names one shard, which shard being an input, with no index and so no bounds obligation; `any_shard` returns the one selector value; no operation computes with shard values; a whole statement reads every shard at its one point, so its total counts every shard write before that point and none after. |
| SHARE-2, target places | A whole handle place, or a map handle place with one index step whose atom is a byte range or a key set. | Adds a `Shared<Shards<T>>` or `SharedRead<Shards<T>>` handle place with one index step whose atom is a place of type `AnyShard`: the shard target. |
| SHARE-2, binding table | Three rows: whole, entry, key-set. | Adds the shard target's row: `&T`, whose path is the state extended by an index step whose offset no relation names [OWN-7], the shard the statement selects. A whole target over a shards state uses the first row, binding `&Shards<T>`. |
| SHARE-2, overlap | Two targets on overlapping handle places each have an index step; a whole target is alone on its place. | Two targets on overlapping handle places are each an entry or key-set target; a whole or shard target is alone on its place. Two shard targets on two handles of one type compare as one root with an index step no relation names, so they may overlap (EFF-5 then refuses one call writing both). |
| SHARE-2, facts | A key-set binding establishes `es^.len == ks.len` at the block's entry. | Adds: a whole binding over a shards state establishes `all^.len >= 1_u64` there. |
| SHARE-2, guard | The guard writes no path. | The guard also reads no path that starts at a shards state. |
| WAIT-2 | Inputs of the execution include map positions, scan extents, released and key-set bytes and the driver count. | Adds the number of shards of each shards state and the shard each shard target names. |
| PRE-1 | Opaque structs through `Entries`; functions through `release`; the preorder. | Adds `opaque nocopy struct Shards<T: copy> { readonly len: u64; }` and `opaque struct AnyShard { }` after `Entries`, `fn shared_shards_new<T: copy>(value: T) -> result: Shared<Shards<T>> pure;` and `fn any_shard() -> result: AnyShard pure;` after `shared_map_new`, and both functions in the preorder there, which moves every later prelude ordinal. |

Unchanged, and why: SHARE-3, since its meaning is already exclusive access to
the whole states the targets name and its last sentence admits holding one
shard where the outcomes are unchanged, which they are because a shard
statement reaches only its shard; WAIT-1, WAIT-3, CAP-1, PAR-1, PAR-2 and
HOST-1, since shard and whole statements are atomic statements and so waiting
calls without overlap permission; EFF-1 to EFF-5, since a state is a path of
no row and a callee receiving `&Shards<T>` uses ordinary rows; OP-2, since a
shard update is the writer's arithmetic.

### 10.2 The guarantees, rule by rule

- **No data race.** Shards are reached only through targets [SHARE-1], and
  every access has SHARE-3's exclusive meaning. Holding one shard instead of
  the object is SHARE-3's last sentence, not a new rule.
- **No silent overflow.** No operation adds anything for the program
  [SHARE-1]: each shard update and each total is a block's own arithmetic, so
  `+` needs a proof (none exists for a concurrently changing value, section
  3.2), and `+wrap`, `+checked` and `+sat` state their results [OP-2].
- **A read of the whole.** A whole target holds every shard at one point
  [SHARE-3], so its total equals the value a single `Shared<u64>` updated by
  the same statements would hold at that point: no weaker read, and the same
  one order of all statements [WAIT-2].
- **Progress.** Shard and whole statements have no guard on the shards, so
  WAIT-2's promise for unguarded statements covers both; a whole statement
  takes effect even while shard statements keep arriving. The
  implementation keeps that promise with first-come locks per shard
  (section 12).
- **Guards.** Refused on any path into a shards state, in the first design. A
  guard on one shard whose identity is an input of the execution states no
  condition a writer can rely on, and a guard on the whole run would make
  every shard write look for watches on the object, a step on the path the
  design keeps free. Refusing now and admitting a whole-run guard later, when
  a program needs to wait for a total, changes no accepted program; the
  reverse would.
- **Overlap footprints.** A statement's footprint is still its handle places
  and index atoms, read [SHARE-2]; the selector is a place of a copy type
  that carries nothing. Inside a whole block, a counted loop summing the
  shards into a scalar accumulator meets PAR-2 as it would over any read-only
  storage, while `Shards` is not one of PAR-2's element-map bases, so a loop
  writing shards stays sequential. Neither needs a new clause.
- **Effects.** A function that updates a shard declares `waits` and a read of
  the handle place, as for any shared object; a callee receiving a whole
  binding declares `reads(parts)` or `writes(parts)` on its `&Shards<T>`
  parameter (case `share-pos-shards-whole-to-callee`).
- **Naming one's part.** The program names a shard only by the selector, never
  by a number, so a shard target has no bounds obligation and no index can be
  out of range; an integer index atom is SHARE-2's refusal and a selector as a
  subscript is OP-4's.
- **The driver count.** `len` is an input of the execution that an
  implementation may choose by the driver count [PRE-2]; no rule relates the
  two further, so a program that reads `len` learns nothing it relies on
  beyond `len >= 1`.

### 10.3 Conformance cases

Seventeen cases in `tests/conformance/cases/`, their manifest lines after the
existing shared-object cases. Each states the verdict the specification
requires; every one fails until the compiler implements the change, and so do
the two compiler unit tests that compare the compiler's prelude records and
measure table with the specification's text. That is expected on this draft
branch; no verdict was chosen to pass.

| Case | Verdict | What it shows |
|---|---|---|
| `share-pos-shards-count` | run, exit 40 | four contexts add one ten times each through shard targets; a whole read after the joins totals 40 whichever shards were named |
| `share-pos-shards-start-from-copies` | run, exit 11 | every shard starts as a copy, and `len >= 1` admits `all^[0_u64]` |
| `share-pos-shards-maximum` | run, exit 9 | `imax` through shard targets, maximum over the whole run |
| `share-pos-shards-beside-map-entry` | run, exit 31 | one statement names a map entry and a statistics shard |
| `share-pos-shards-whole-to-callee` | run, exit 70 | callees read and reset a `&Shards<u64>` under ordinary rows |
| `share-neg-shards-two-on-one-handle` | reject SHARE-2 | two shard targets on one handle place |
| `share-neg-shards-shard-beside-whole` | reject SHARE-2 | a shard target beside a whole target on one place |
| `share-neg-shards-guard-reads-shard` | reject SHARE-2 | a guard on a shard's value |
| `share-neg-shards-guard-reads-whole` | reject SHARE-2 | a guard on the whole state's `len` |
| `share-neg-shards-index-not-a-selector` | reject SHARE-2 | an integer where the selector belongs |
| `share-neg-shards-two-handles-one-call` | reject EFF-5 | shard targets on two handles of one type may be one shard |
| `type9-neg-shards-parameter` | reject TYPE-9 | a `Shards<u8>` value parameter |
| `type9-neg-shards-swapped` | reject TYPE-9 | a `swap` of two whole shards targets |
| `prov6-neg-shards-element-not-copy` | reject PROV-6 | a `Box<u8>` shard type against the copy bound |
| `op4-neg-shards-past-the-first` | reject OP-4 | `all^[1_u64]` is unproved, since only `len >= 1` is known |
| `op4-neg-shards-selector-as-subscript` | reject OP-4 | the selector as a subscript offset |
| `ref4-neg-shards-as-a-range` | reject REF-4 | a range reference over the shards |

The program test that section 5.4 names (16 contexts counting, an exact whole
read after the joins) belongs in `tests/programs/` with the implementation,
since it exercises the runtime's shard selection rather than a rule.

For (b): no specification change (SHARE-3's last sentence). For (a), if ever
adopted: PRE-1 declarations of `SharedCell<T: Int>` and its ten functions, a
SHARE-1 paragraph giving each operation's single-point meaning and
`cell_add_checked`'s unchanged-on-overflow rule, and the target capability row
of relaxed-fields decision 6 for the hardware route; section 9.3 is the
evidence against it for counts.

## 11. Design-tree changes

Proposed on the work branch, each awaiting the owner's approval:

- New node `design/language/waiting/shared-objects/sharded-states.md`: the
  shards state and its two target forms; that the number of shards and the
  selected shard are inputs and the program never supplies an index; the
  guard refusal; the copy bound; and the state-only home. Rejected: single-step
  integer cells (a), the hand-built array as the answer (d2), the hidden
  combine (c′), a runtime peak in the memory meter (d3), whole-program
  detection (d4) and per-context accumulation flushed (d1).
- `design/language/waiting/shared-objects.md`, third decision: the target
  forms gain the shard target, alone on its handle place.
- `design/compiler/waiting-contexts/state-locks.md`: first decision, a sharded
  object is held by one shard or whole; second decision, its shards are taken
  in index order; a new decision for the shard layout and selection (section
  12).
- For (b) B2, if pursued: `design/language/waiting/shared-objects/keyed-tables.md`'s
  refusal of version-checked reads narrowed to blocks that run on racing
  state, with the account in 5.3; owner decision required. Not part of this
  change.

## 12. Implementation plan

Not implemented on this branch. The order follows the gate: the checker first,
so the reject cases pass, then lowering and the runtime, so the run cases
pass, then the measurement.

**Prerequisite.** A shard statement must not write the object header. Today
every statement through a value handle counts the handle on the header
(section 2.2; W3 shows it, section 9.2), which would put one contended line
back on every update. Board item gran-skip-handle-count (skip the count when
the block neither moves nor writes the handle place) lands first or with this
work; W4r in section 8.2 separates the two if it does not.

**Checker.**

- Prelude records: the two opaque structs and two functions in the order
  PRE-1 states, so the record test and the diagnostic ordinals follow the
  specification; `Shards` gets its own storage identity, as `Entries` has, and
  a measured kind with `len` exact for the measure-table test.
- Placement [TYPE-9]: `Shards<T>`'s home is the type argument of `Shared` or
  `SharedRead`, checked where map placement is.
- Subscripts [OP-4]: `Shards<T>` reached through `^` with a `u64` offset and
  the ordinary `i < p.len` obligation; a range over it refused at REF-4 with
  the repair.
- Targets [SHARE-2]: classify a target whose handle has state type
  `Shards<T>` and whose index atom is a place of type `AnyShard` as a shard
  target; refuse any other index atom on a shards handle; extend the overlap
  check so overlapping handle places admit only entry and key-set targets.
  The shard binding's resolved place is the state extended by an index step
  with an unknown offset, so the existing OWN-7 and EFF-5 comparisons refuse
  what may be one shard without new code.
- Guards: refuse a guard whose footprint reads any path rooted at a shards
  state, at the guard `expr`, citing SHARE-2.
- Facts: a whole binding over a shards state publishes `all^.len >= 1` at the
  block's entry, where the key-set binding publishes its length equality.

**Lowering.**

- A shard target lowers to one runtime call that returns the address of the
  running thread's shard and takes its lock, and one that releases it; the
  binding is that address. The statement's lock-order group is the object's
  type, as for any target (state-locks, second decision).
- A whole target over a shards state takes every shard's lock in index order
  and binds the object's shard array; `all^.len` reads the header word and
  `all^[i]` is the address `first + i × stride`. Release is in any order after
  the block, as two-phase locking already requires.
- Targets of one statement that name one object are held once (state-locks,
  third decision): a whole hold absorbs a shard hold, and two shard targets
  on one object bind the same selected shard.
- No guard reads a shards state, so no watch is ever registered on a shard,
  and a shard release wakes nothing. A guarded statement that also has a
  shards target keeps its guard's watches on its other objects.

**Runtime layout, per object.**

- Header on its own 64-byte line: the handle count, `len` and the object's
  identity for lock order.
- Then `len` shards, each starting on a 64-byte boundary, holding its lock
  word and the T value, its stride `round_up(lock + size(T), 64)` so no two
  shards share a line. The allocation is counted in the heap the program holds
  [PRE-2] like any shared object's.
- `len` is the number of threads that may run contexts: twice the driver
  count on a multi-driver target, since a driver's role can pass to a spare
  thread (compiler/waiting-contexts/concurrent-map, fifth decision), and one on
  a single-driver or embedded target, where a shard target is the object's one
  lock and the object is a `Shared<T>` in all but name. At the measured 16
  drivers that is 32 shards, 2 KiB plus T per object (section 5.4, Memory).
- A shard target selects shard `user mod len`, `user` being the running
  thread's map-user number. A statement holding a shard never suspends, so it
  ends on the thread that began it; a displaced thread and its spare can meet
  on one shard, which is why each shard keeps a lock rather than a
  single-writer store (section 3.6).
- Each shard's lock is first-come (a ticket lock): uncontended it costs one
  atomic read-modify-write on the thread's own line, as the present object lock
  does, and a whole statement waiting for a shard waits for at most one shard
  statement per shard, which keeps WAIT-2's promise for the whole statement
  however fast shard statements arrive.

**The read-all path.** A whole statement costs `len` lock acquisitions in
index order plus the block's loop. At 32 shards the estimate is a few
microseconds; W3rd's 16-target read took 0.6 to 5.6 µs on average (section
9.2), the comparable figure. While it runs, shard statements on shards it
already holds wait; ones on shards it has not reached proceed.

**Validation.**

- The seventeen conformance cases of section 10.3, the compiler record and
  measure tests, and the program test of section 10.3.
- A runtime test of the shard lock and the whole hold under ThreadSanitizer,
  as the concurrent map's test runs, with a whole reader racing shard
  writers and an exact total checked after the joins.
- The registered experiment's remaining arms on the 14900K (section 8.2): W4,
  W4r and their reader variants, M1 to M4, and the Firn arms F0 to F3 from
  Firn's session, deciding P1, P2, P4, P5 and R1, R2, R4, R5. If the noise rule
  of section 8.3 is to change for that run, the change is recorded before it
  measures (section 13).

## 13. Found along the way

- **The heap reading took a global lock.** `wf__heap_in_use` took the pool's
  spin lock on every reading, which Firn's perf c2c located as the hottest
  contended line. Disposition: fixed by Whitefoot#381 (merged to main as
  `abc27fbd3`); section 9.4 measures the remainder at about 5% of Firn's GET.
- **Whole-object read-only statements take write holds.** The emitter passes
  `write = 1` for every object acquire, take and unlock
  (`emitter/shared.rs:1027,1059,1075`), while SHARE-3's last sentence and the
  runtime admit read holds. W6 confirms the cost: a statement that writes
  nothing runs at 1.06 to 1.28 times a writing one and collapses as it does
  (section 9.5). Disposition: deferred to (b); worth a board item, since every
  read-only statement on a hot object pays it.
- **The handle count on every statement through a value handle.** It writes
  the object header twice per statement; W2r / W2 = 1.06 to 1.66 prices it,
  and the W3 statements through a local reference pay it too (section 9.2).
  Disposition: board item gran-skip-handle-count, a prerequisite of (c)
  (section 12).
- **Contended objects collapse rather than plateau** (11.08 to 0.72 M/s from
  2 CPUs to 16 hyperthreads in W2). Disposition: board item
  gran-lock-no-collapse.
- **Section 2.1's read-only row has no located source, and W6 does not
  reproduce it.** W6 reads 2.99 M/s at 8 P-cores where the row says 30.3.
  Disposition: recorded; the row is Firn-wf's, and no reading here depends on
  it.
- **W0 measures nothing.** Its loop is folded by the optimizer (section 9.5).
  Disposition: deferred; make its batch observable when the sharded-state arms
  join the harness, since no reading uses it now.
- **The registered noise limit cannot decide contended arms.** `e` comes from
  W2, the most contended arm, whose blocks span a third of its median at 2 CPUs, while the
  uncontended arms vary under 3%. Disposition: for the W4 run, propose before
  measuring a twin of an uncontended arm (W1 built twice) and a per-arm noise
  measure; section 8.3's rule stays as registered for these results.
- **A parallel change to the same rejection.** Whitefoot#364 (the one-time
  decision cell, approved, not yet on main) narrows the shared-objects
  rejection of atomic fields and lock-free cells; the new sharded-states
  node's rejection of single-step cells must still read consistently once
  both land. Disposition: check at that merge.
- **Name collision to keep in mind.** OP-12 calls `set p = f(move p, ...)` "the
  atomic in-place update" (`:1215`), an ownership property unrelated to
  concurrency. The drafted surface uses "shard" and "selector" and no
  "atomic" in its operation names.

## 14. What is established and what is not

Established by reading the specification, design tree and source at
`4ad4f5786`: the rules in section 3; that whole-object statements take write
holds and that even read holds write the object's line; the handle-count
traffic; the runtime's own per-thread counters and the decision that chose
them; the prior refusals in section 4; and that (c) needs no new memory model
because its targets are ordinary atomic statements.

Measured on the 14900K (section 9, runs 38303980083 and 38304337014): the
hand-built array reaches 0.31 to 0.66 of the private-object rate and falls
from 8 P-cores to 16 hyperthreads; a contended hardware add reaches 0.059 of
private lines at 8 P-cores; the two witness rows of section 2.1 for one shared
object and for own objects reproduce; a read-only statement collapses as a
writing one does. Measured by Firn and quoted (section 9.4): Firn's GET with
the lock-free heap reading at 0.950 of the build without a peak, across two
runs.

Unverified: every cost estimate for (c) in sections 5, 6 and 12, since no
sharded state exists to measure (W4); R2's final reading, which needs W4;
B2's LLVM-model correctness; that the specification draft of section 10 is
complete and consistent, which the approval card and the implementation will
test; that the conformance cases compile and reach their verdicts, which
needs the implementation; the reader's latency for a whole target; the shard
selection's correctness under a driver handoff to a spare thread (argued from
per-shard locks, section 3.6, not tested).
