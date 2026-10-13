# Shared counters and maxima without an exclusive hold per update

## Status

A design study, then the measurements the owner asked for before a language
change, then the approved sharded-state change. Sections 1 to 8 were written before any
implementation or measurement and check the active specification
at the time and the compiler and runtime at
Whitefoot `4ad4f5786` (main); line numbers there cite that revision.

The owner chose (board card on what Whitefoot should offer for a count many
cores update, 2026-10-12, option A) to measure three things without a
language change first. Two of them ran on the 14900K on 2026-10-12 and
[section 9](#9-results-the-arms-that-need-no-language-change) records them:

- The hand-built array of 16 shared objects reaches 0.66, 0.48, 0.51 and
  0.31 of the rate of private objects at 2, 4 and 8 P-cores and 16
  hyperthreads, below the 0.8 that would make it the answer at every CPU
  count.
- A contended hardware add reaches 0.29, 0.13, 0.06 and 0.06 of the rate of
  private lines, 0.059 at 8 P-cores against the 0.5 that would select
  single-step cells.
- The noise control, the contended arm built twice, differs by up to 9%
  between its twins, three times the provisional 3% limit, so the summary
  script marks every registered condition inconclusive by rule. The
  decisions stand anyway because the ratios lie far beyond their thresholds
  measured in that noise: the cells' ratio 9.7 times it below, the array's 2.0
  to 6.8 times it below, and even the array's best block stays under 0.8 at
  every list. The noise is the contended arm's own; the uncontended arms
  vary by under 3% (section 9.1).
- The runtime-floor arm W0, a loop with no atomic statement, measures
  nothing: the optimizer folded its loop (section 9.5). No reading uses it.

The third measurement, Firn without reading memory usage, was not run as a
new arm: Firn's session had already measured it while accepting the
lock-free heap reading (Whitefoot#381), and with that reading Firn's GET is
25.65 million a second against 26.99 million for the build that keeps no
maximum, across two runs, so about 5% within the twins' spread there
([section 9.4](#94-firn-without-reading-memory-usage)).

Neither the writer pattern nor single-step cells is selected, so the ruling's
third outcome applies: the sharded state. Its specification and design
decisions are owner-approved; the specification change is recorded as
v0.143 in `spec/log.md` ([section 10](#10-specification-changes)). The
checker, lowering and runtime are implemented on this branch
([section 12](#12-implementation-plan)); the conformance cases reached their
verdicts and unit tests passed in CI at `8cf048252`. PR #409 (skip handle count), merged into this branch with main, gives the
lean shard path: a shard statement whose block neither moves nor writes the
handle place writes no handle count. The 14900K acceptance measurement has not run. The registered comparison
that could still reject the change after its implementation, the sharded
state at most 1.25 times the array's rate at every CPU count (R2), waits for
the acceptance measurement. The criteria the implementation must meet, and a C model
that gates it, were registered on 2026-10-12 before any implementation or
C-model measurement, replacing the earlier prediction P1
([section 8.5](#85-criteria-for-the-implementation-registered-2026-10-12)),
and the C model's arms, procedure and decision rule were registered before it
ran ([section 8.6](#86-c-model-run-registered-2026-10-12-before-measurement)).

Firn then asked that each driver's shard can own a buffer or a queue and that
one statement can swap out every shard's content (Firn's analysis of writes
without one exclusive hold on its metadata object, gap G1: a write command
appends to its driver's pending AOF buffer beside the key it holds, and the
disk writer swaps every buffer for a fresh one every 10 ms). The approved change folds
that need into the same design rather than a second kind of state: the
shard type needs only `drop`, and the constructor calls a maker once per
shard instead of copying one value
([section 5.4, owned shard values](#owned-shard-values)).

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
correct when two threads reach it. The sharded state of section 5.4 avoids the
case by keeping a part for each thread, not each driver: a displaced thread
and its spare are two threads with two parts, so a part has one writing
thread. Its lock excludes a whole target on another thread, not a second
writer.

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

(This surface is the one registered before the measurements. The drafted
specification replaces the copy bound and the copied argument with a `drop`
bound and a maker called once per shard, for Firn's request that shards own
buffers and queues; [owned shard values](#owned-shard-values), at the end of
this section, gives the change and its reasons, and section 10 the rules.)

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
An atomic block never suspends, so it starts and ends on one thread, and a
thread runs one statement at a time: a shard is written by one thread alone
and writers never contend with each other (section 3.6). Each shard keeps a
lock only to exclude a whole target on another thread, which reads the total
or swaps out every shard's content and must know that no shard is half
written. A one-shard statement must not write the header: the handle count
must be skipped for it, which requires the compiler's existing skip to cover
value handles that the block neither moves nor writes, or the shard path will
contend on the header (an implementation obligation; see the
found-along-the-way list). It also changes no holder count, and the lock's
fast path is inlined in the generated code ([section
12](#12-implementation-plan), runtime layout). On a single-driver target,
`len` is one, and the object is a `Shared<T>` in all but name.

**Expected cost (estimate, unmeasured).**
- Count, `w_shared` rewritten with one-shard statements: each update is an
  uncontended statement on a line its own thread owns, the same work as
  `w_private` on the ordinary statement path, so near 89.6 / 171.4 / 348.1 /
  438.5 (the lean shard path of section 12 does less than that path, and
  [section 8.5](#85-criteria-for-the-implementation-registered-2026-10-12)
  registers the criteria it must meet).
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
can combine: counts, maxima and minima, per-command tables, histograms in a
constant-capacity array. Per-thread free lists, which hold boxes or handles,
are outside it, since the copy bound of section 10 keeps a shard inline (the
drafted `drop` bound brings them inside; see
[owned shard values](#owned-shard-values)). It is the Whitefoot form of kernel per-CPU data
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

#### Owned shard values

Added after the measurements, for Firn's request; section 10 states the
rules.

**The need.** Firn's analysis of writes without one exclusive hold on its
metadata object (Firn-wf, design (b) and gap G1) moves each write's share of
that object into one part per driver: a write command holds its key's entry
and its driver's part in one atomic statement, appends the command's bytes to
that part's growable buffer and may queue an expiry there; every 10 ms the
disk writer holds every part in one statement, swaps each part's buffer for an
empty one and merges the taken buffers outside the hold. With `T: copy` none
of it can be written: a `Box<Slots<u8>>` buffer and a priority queue are
`nocopy`. Firn's completion condition: its witness compiles with a released
compiler, and on the 14900K 64 contexts writing 100 000 random keys, with a
swap-out of every part each 10 ms, scale by at least 1.6 times per doubling
of P-cores from 2 to 4 to 8.

**Why the copy bound was there, and what is left of it.** The registered
surface had three reasons, two stated and one implied:

1. A copy type keeps handles, boxes and runtime-capacity storage out of the
   shard, so a shard is inline storage of fixed size. Every type has a fixed
   inline size [OP-9]: a `Box` is one pointer inline and its content lives in
   its own cell, as a map entry's `Box` does. A shard's lines hold T's inline
   part and what T owns lives elsewhere, so this needs no bound.
2. A copy of the initial value is well defined. This holds only for the
   copying constructor, which a maker replaces (below).
3. A copy type's release is empty, so releasing the object had nothing to do
   per shard. With owned values the state's release must release every
   shard, which needs exactly `drop`, the bound `Shared<T>` and
   `ConcurrentHashMap<V>` already carry; a linear T stays refused.

So one rule covers counters and buffers alike: `Shards<T: drop>`, each shard
made by a maker. A counter pays a one-line maker; there is no second kind of
shards state and no second constructor (rejected in the node: two
constructors state one meaning twice).

**"Call this `len` times" without closures.** Whitefoot has no closures; its
form for supplying behaviour is a function-kind generic parameter [FN-3], a
compile-time argument checked against a formal signature at its written
argument [FN-4] and instantiated per actual [FN-2]. The constructor takes one:

```whitefoot
opaque nocopy struct Shards<T: drop> {
  readonly len: u64;
}

fn shared_shards_new<T: drop, fn make() -> made: T pure>() -> result: Shared<Shards<T>> pure;
```

The formal is `pure` and carries no `waits`, so a maker that waits is
refused (FN-4 refuses a waiting actual for a plain formal, as
`fn4-neg-waiting-actual-for-plain-formal` already shows) and
`shared_shards_new` stays a pure call that does not wait. A pure maker with no
parameters returns the same value at every call, up to the identity of its
allocations, which is not observable, so how often it runs, `len`, already an
input of the execution, shows only in heap readings, which are inputs too
[PRE-2]. A maker cannot capture run-time data such as a configured capacity;
the creator then fills the shards in one whole statement right after
creation, before it shares the handle (the node rejects a seed argument for
that reason). The no-heap closure already follows function-kind actuals
[STOR-8], so a `no_heap` entry sees a maker that allocates. This is the first
prelude record with a function-kind parameter, so PRE-1's preorder sentence
names function-kind parameters to keep the diagnostic ordinals fixed.

**The witness in the drafted surface** (Firn's G1 sketch adjusted to the
prelude's window operations; a fragment, not compiled):

```whitefoot
struct Part {
  log: Box<Slots<u8>>;
  refused: Bool;
}

fn part_new() -> made: Part pure {
  let log = box_slots_new::<u8>(capacity: 65536_u64);
  return Part(log: move log, refused: False());
}

fn write(keys: &Shared<ConcurrentHashMap<u64>>, parts: &Shared<Shards<Part>>, k: &[u8]) -> r: unit reads(keys), reads(parts), reads(k) waits {
  let mine = any_shard();
  atomic slot = &keys^[k], p = &parts^[mine] {
    if p^.refused {
      return unit;
    }
    set slot^ = Some<u64>(value: 1_u64);
    if p^.log.inner.len < p^.log.inner.cap {
      place_back(window: &p^.log.inner, value: 1_u8);
    }
  }
  return unit;
}

fn drain(parts: &Shared<Shards<Part>>) -> r: unit reads(parts) waits {
  atomic all = &parts^ {
    for @each (i in 0_u64..all^.len) {
      let fresh = box_slots_new::<u8>(capacity: 65536_u64);
      swap(first: &all^[i].log, second: &fresh);
    }
  }
  return unit;
}
```

A real append grows a full buffer with `grow` instead of skipping the byte,
and a real drain keeps what it takes: since `len` is known only inside a
whole statement, the taken buffers go into a collection grown inside the
hold, or each part keeps a spare buffer that the drain swaps in, so the hold
allocates nothing and the spares are refilled by later shard statements.
That choice is Firn's.

**Combination with an entry target.** SHARE-2 already admits targets on
different handle places. A map entry target and a shard target name objects
of two types that never unify, so their paths start at two roots, and the
lock order takes the two type groups in type order. Nothing new.

**What each guarantee rests on.**

- *No data race.* Unchanged: shards are reached only through targets, and a
  `swap` inside a whole statement is a write under exclusive access
  [SHARE-3].
- *No leak or double free.* A shard always holds exactly one value. `swap`
  exchanges two owners and leaves both live [OP-11, OWN-1]; a shard is
  reached only through a reference, a shard or whole binding or a callee's
  `&Shards<T>` parameter, so moving its content out is always OWN-1's refusal
  of a consume through a reference (case
  `own1-neg-shards-move-out-of-shard`); replacing a shard's value with
  `set` releases the old one [WIN-3]; releasing the state releases every
  shard [SHARE-1]; a linear T is refused by the `drop` bound (case
  `prov6-neg-shards-element-nodrop`). No implementation step copies a
  shard's value.
- *No index out of range.* Unchanged: a shard target has no index, and a
  whole target's subscripts owe `i < len`. `len >= 1` is now MSR-2's
  standing fact, so it holds in callees receiving `&Shards<T>` too.
- *Release at the object's end.* The release of the last handle runs T's
  compiler-derived release on each shard in index order and then frees the
  object (state-locks); no statement can be executing then [SHARE-1].
- *Memory statistics.* A maker's allocations and the buffers a whole
  statement swaps in are ordinary allocations counted in the heap the
  program holds [PRE-2], and a taken buffer released after the swap lowers
  it. A shard's own measures, `p^.log.inner.len` through a shard binding or
  `all^[i].log.inner.cap` through a subscript, are ordinary measure places
  [MSR-1], killed by writes as an element's are [MSR-2]. No new rule.
- *Progress.* Unchanged: shard and whole statements have no guard on the
  shards. Allocation inside a whole statement lengthens its hold but is total
  [STOR-8]; the spare-buffer form keeps it out.
- *Type invariants.* A shard is no TYPE-11 site, as an element of a storage
  shape and a map's entry are not: a shard target cannot assume an
  invariant that a whole target's element writes never owe. The node records
  the choice; section 13 records the general gap.

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
| W4t | W4 rebuilt from the same source as a separate binary | noise control for the implementation run (section 8.7) |
| W4r | W4 through a read-only reference parameter | separates a header-write defect from the design |
| W4rd | W4 plus one extra context reading the total in one whole-target statement about every 1 ms | write retention and longest whole read (section 8.7) |
| W6 `read-shared` | read-only statement on one `Shared<u64>` | ties the reported row 3 to a source |
| W7 `c-model` | C model of the planned layout: a lock word and a value on each thread's own line, plus a reader of the total every 1 ms (added 2026-10-12, [section 8.5](#85-criteria-for-the-implementation-registered-2026-10-12)) | gate before any implementation, against W5p |
| W7t | W7 built twice from the same source | noise control for W7 |
| W7rd | W7 with its 1 ms reader of the total; W7 itself runs without one (added 2026-10-12, [section 8.6](#86-c-model-run-registered-2026-10-12-before-measurement)) | the reader criteria of section 8.5 on the C model |
| W5pN | W5p's binary run with one thread per CPU of the list instead of 64 (added 2026-10-12, section 8.6) | W7's comparison at W7's thread count |

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

Owned-buffer arm, the second workload of the sharded-state board item:

| Arm | Per operation | Role |
|---|---|---|
| A1 | 64 contexts write a random key among 100,000 in a `Shared<ConcurrentHashMap<u64>>` and append one byte, when room remains, to their shard of `Shared<Shards<AofPart>>` in the same atomic statement; `AofPart` owns a `Box<Slots<u8>>` | exercises owned values in shards, as Firn's pending AOF buffers do; an extra context swaps out all buffers about every 10 ms and counts drained bytes |

A1 populates the map before the timed interval and counts completed write
statements as operations. Its `check` counts drained bytes, including a final
residual drain after timing ends; `check < ops` exposes bytes skipped by full
buffers. The summary reports these counts and drained/write ratios alongside
the throughput criteria, whose operations remain completed write statements.
The reader fields count and time only periodic drains. Each shard buffer holds
1,048,576 bytes. A nominal 10 ms interval fills it only above 104,857,600
appends per second on one shard, conservative headroom for a path that also
updates a random map entry. This is a sizing assumption, not a measurement;
delayed drains can still fill a buffer. A1 is a witness of the shared map and
owned-buffer path, not a measurement of AOF file I/O or durability.

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

- **P1.** Replaced on 2026-10-12 by the criteria of
  [section 8.5](#85-criteria-for-the-implementation-registered-2026-10-12);
  it asked W4 ≥ 0.8 × W1 at 2, 4 and 8 P-cores.
- **P2.** W4 ≥ 1.25 × W3 at 8 P-cores and at 16 hyperthreads.
- **P3.** W5 / W5p ≤ 0.25 at 8 P-cores (a contended hardware add stays far
  below private increments).
- **P4.** With the 1 kHz reader, W4 keeps ≥ 0.95 of its rate without the
  reader, and the whole-target read takes ≤ 50 µs at the 8-P-core list.
- **P5.** F3 GET ≥ 0.97 × F0 at 8 P-cores; F1 GET ≤ 0.5 × F0.
- Descriptive, no rejection: M1–M4, W2r against W2, W6, and F-heap against F0.

The recommendation is rejected if:

- **R1** W4 < 0.5 × W5p at 8 P-cores while W4r ≥ 0.5 × W5p does not explain it
  (a design cost, not a header-write defect; threshold replaced on 2026-10-12,
  section 8.5);
- **R2** W3 ≥ 0.8 × W4 at every CPU list: the existing-language shard array
  serves counts nearly as well, and the language change is reduced to an
  ergonomic one, to be written as a pattern in `docs/patterns.md` instead;
- **R3** W5 / W5p ≥ 0.5 at 8 P-cores: a single-word hardware add then serves
  counts well enough, and (a), which also serves identifiers, becomes the
  recommendation;
- **R4** P4 fails: the read's cost or interference is too large for an INFO
  that may run often;
- **R5** F3 < 0.97 × F0 at 8 P-cores, or F2 within `e` of F3.

A W4r ≥ 0.5 × W5p with W4 < 0.5 × W5p is an implementation defect (header
writes on one-shard statements) to fix before the result counts; it does not
reject the design.

<a id="shards-implementation-criteria"></a>

### 8.5 Criteria for the implementation (registered 2026-10-12)

Registered on 2026-10-12, before any implementation and before any C-model
measurement, after the owner asked how the design guarantees its performance.
It replaces P1 (W4 ≥ 0.8 × W1) and the 0.8 threshold of R1. W1, an object per
context, runs at 0.17 to 0.18 of W5p, C with each thread adding to its own
cache line, at 2, 4 and 8 P-cores (89.44 / 498.51, 171.82 / 997.04, 346.01 /
1993.71, Tables 9.2a and 9.3), so 0.8 × W1 is only about 14% of C's rate, too
low a bar for a design whose purpose is to approach it. W5p is called C
own-line below.

**What the lock is for.** A one-shard target selects the shard by the index of
the running thread (`user mod len`, not the context number). An atomic block
never suspends, so it starts and ends on one thread, and a thread runs one
statement at a time: a shard is written by one thread alone, and writers never
contend with each other. The per-shard lock exists only to exclude a whole
target on another thread, which reads the total or swaps out every shard's
content and must know that no shard is half written. Uncontended, taking it is
one locked atomic instruction on the thread's own cache line, the same cost
class as C own-line (each thread `atomic_fetch_add` on its own line).

**The lean shard path.** A shard update does not change the handle count, does
not change the holder count, and inlines the lock's fast path in the generated
code instead of calling a runtime function. Each update then touches only the
thread's own cache line; the header is read-only for it, so cross-core line
migration for an update is zero. W1 runs the ordinary atomic-statement path
(the handle count twice, the holder count, a runtime call) and reaches 0.17 to
0.18 of C own-line, 5.6, 5.8 and 5.8 times slower at 2, 4 and 8 P-cores, so
the lean path has that distance to close. The lean path depends on a separate board item, an atomic statement
through a value handle does not change the handle count (gran-skip-handle-count,
the prerequisite of section 12), which is completed before the implementation.

**Step 1, a C model, before any implementation.** On the 14900K, a C model of
the planned layout: one cache line per thread holding the lock word and the
value together, and a reader that reads the total once per millisecond.
Arms W7 `c-model` and its twin W7t, the same source built twice as the noise
control, run interleaved with W5p under the procedure of section 8.3.

- W7 below 0.5 × W5p: the locked approach itself is insufficient. Switch to
  the design in which the writer side executes no atomic instruction (below)
  and measure the C model again.
- W7 at or above 0.5 × W5p: implement the compiler and the runtime.

**Step 2, after implementation.**

- On 2, 4 and 8 P-cores, W4 ≥ 0.5 × W5p. On 8 P-cores that is ≥ 1 billion
  updates a second (0.5 × W5p there is 996.9 million a second; today one
  shared object runs at 2.81 million and the hand-written 16-object array at
  175.1 million). 16 hyperthreads are reported only.
- With the total read once per millisecond, write throughput stays ≥ 0.95 of
  its rate without the reader, and one whole read takes ≤ 50 µs (P4, kept).
- Firn GET with sharded counting ≥ 0.97 × Firn GET with no counting (P5,
  kept).
- Kill line, kept: W4 below 1.25 × W3, the hand-written array, at 8 P-cores
  and 16 hyperthreads withdraws the language change (P2 and R2 above).

**If a criterion of step 2 is not met, in order.**

1. Attribute with control variants: W4r, the same updates through a read-only
   parameter, which shows whether the header is still written, and the C
   model itself.
2. Fix the step that is slow: the fast path not inlined, or the holder count
   still written.
3. If still short, switch to the design in which the writer side executes no
   atomic instruction.
4. If still short, return to the owner with the data.

**The writer side without an atomic instruction (second step, not the first
version).** A shard update is plain loads and stores, so it costs no locked
instruction. A whole target takes the place of the lock by issuing the
operating system's process-wide asymmetric barrier (Linux `membarrier`,
Windows `FlushProcessWriteBuffers`), so that every shard update begun before it
is complete and visible, and then reads or swaps the shards. macOS
has no such call, so its mechanism is open and is derived only if this step is
reached. The cost moves to the whole target, which becomes a system call per
read, and the C model measures that cost again before the design is chosen.

### 8.6 C model run (registered 2026-10-12, before measurement)

Registered on 2026-10-12, before the C model was built on the 14900K or run
anywhere. It fixes how step 1 of
[section 8.5](#85-criteria-for-the-implementation-registered-2026-10-12) is
measured and read; section 8.5's thresholds are unchanged.

**Question.** Does the planned shard update, a first-come lock taken and
released around an add on the thread's own cache line, keep at least half the
rate of C own-line, each thread adding to its own line with one atomic add, at
2, 4 and 8 P-cores? The C model does only what the planned lean shard path
does, with no Whitefoot runtime around it, so its rate bounds the
implementation's from above: if W7 stays below half of C own-line, W4 cannot
reach it either, and the locked design is replaced before it is built.

**The model** (`research/experiments/shared-counters/w7.c`):

- One allocation: a header line holding the handle count and the number of
  shards `len`, then `len` shards, each a 64-byte line holding a ticket lock
  (two 32-bit words, the next ticket and the ticket being served) and a
  64-bit count. Nothing writes the header after it is made.
- `len` is twice the number of CPUs in the list, as section 12 sets it to the
  threads that may run contexts, twice the drivers. The shards above the
  writers' numbers stand for the spare threads; only the reader takes their
  locks.
- One writer thread per CPU of the list, standing for the drivers: writer `i`
  has map-user number `i` in a thread-local and selects shard `i mod len`,
  reading `len` from the header on every update; since `i < len`, the
  division is skipped, as generated code may skip it. Each update draws a
  ticket with one atomic fetch-and-add (acquire), waits until the served
  ticket equals it (one load when uncontended), adds 1 to the count with a
  plain load and store, and releases by storing the next ticket (release),
  then does the harness's local step, as W5 and W5p do after their add.
  The threads are confined to the list with `taskset`, as W5p's are, and not
  pinned one to a CPU, since the Whitefoot runtime does not pin its drivers.
- W7rd adds one reader thread pinned to CPU 16, the first E-core, outside
  every list, so that it takes no CPU time from the writers and the
  comparison W7rd / W7 prices only its locks; in Whitefoot the reader would be
  a context on a driver, whose CPU cost W4's reader arm measures. Every
  millisecond (absolute 1 ms ticks; a reader more than one period late starts
  its period again) it takes every shard's lock in index order, sums the
  counts, releases every lock, and records the time from before the first
  lock to after the last release. Per cell it reports the number of reads,
  their mean, median, 99th percentile and longest time.
- A cell fails, and leaves no row, if the shards' counts do not sum exactly
  to the writers' operations, if a whole read sees a smaller total than the
  read before it, or if the reader made no read or did not run on its CPU.

**Arms**, each cell 5 s, built with `/usr/bin/clang -O2 -pthread` as W5 and
W5p were: W5p (the same source, flags and 64 threads as the measured W5p,
unchanged), W5pN (W5p's binary with one thread per CPU), W7, W7t (W7's
source built a second time into its own binary) and W7rd (W7's binary with
the reader).

**Thread count.** W7 has one writer per CPU, because in the planned runtime a
shard has exactly one writer, the thread running its driver; 64 C threads on
fewer CPUs would put several writers on a shard and measure a design that is
not planned. The gate therefore compares W7 with W5pN, C own-line at the same
thread count. W5p at 64 threads, the number section 8.5 quotes, runs in the
same blocks; W7 / W5p and W5pN / W5p are reported, not decided.

**CPU lists and procedure.** The lists of section 8.3: `0,2`, `0,2,4,6` and
`0,2,4,…,14` (one hyperthread of each of 2, 4 and 8 P-cores) decide; `0-15`
(8 P-cores, both hyperthreads) is reported only. The 14900K runner through CI
only, checked idle (under 5% busy over 5 s), every heavy step under the
host-wide lock of `run-check.pl`. A smoke run on a GitHub-hosted runner first
checks the wiring and decides nothing. A pilot, one block of every arm at the
8-P-core list, shows the run length and that the host runs the arms; it does
not change the number of blocks. The main run is 12 blocks, more than section
8.3's six because the interval below is formed over blocks, and costs about
the time of the earlier six-block run of ten arms. Each block runs every arm
once per list; the arm order starts one position later in each block and runs
backwards in odd blocks, so that W7 runs before its twin in half the blocks
and after it in the other half.

**Decision rule, per CPU list.** Ratios are paired within a block: for each
block, the ratio of the two arms' rates in that block on that list. A
ratio's reading is the median of its block ratios and a 95% percentile
bootstrap interval of that median: 10,000 resamples of the block ratios,
drawn with replacement by Python's `random.Random(20261012)`, the interval
running from the 251st to the 9,750th of the sorted resample medians
(`summarize.py`).

- **Twin.** The interval of W7t / W7 must contain 1; otherwise the list is
  void, and none of its readings count. With identical binaries and a
  correctly covering interval, a list is void by chance about 5% of the time,
  and one of the three lists about 14% of the time; the one rerun below
  exists for that.
- **Gate**, W7 / W5pN against 0.5: met when the interval's lower bound is at
  least 0.5, not met when its upper bound is below 0.5, otherwise
  inconclusive.
- **Reader**, decided at the same three lists: W7rd / W7 against 0.95 in the
  same way; and the median whole read, taken per block as the cell's median
  read time, against 50 µs: met when the interval's upper bound is at most
  50 µs, not met when its lower bound exceeds it. The 99th percentile and the
  longest read are reported, not decided: the longest read of a 5 s cell on a
  general-purpose kernel measures interrupts as much as the lock.
- Fewer than six paired blocks at a list decide nothing there.

**Outcome.** The gate is met when it is met at all three lists: implement the
compiler and the runtime. It is not met when it is not met at any list that is
not void: switch to the writer side without an atomic instruction (section
8.5). Otherwise, inconclusive or void at a list with none not met, the whole
main run is repeated once, and the repeat's reading decides alone; both runs
are reported, and a repeat that is still undecided goes to the owner with the
data. A reader criterion not met by the C model does not change the gate,
since the gate is about the update path, but it shows that the locked design
cannot meet section 8.5's reader criteria either, and it goes to the owner as
a decision before the implementation starts.

**Predictions.** W7 / W5pN between 0.6 and 0.9 at each of 2, 4 and 8
P-cores, and within 0.05 of the same value at all three. Reason: C own-line
spends about 23 cycles per update (249 million updates a second per core,
Table 9.3), nearly all of it the locked add. The model's update is one locked
add of the same kind on the same private line, plus an uncontended load of
the served ticket, a plain store of the count and a plain release store; the
locked instruction of the next update must first drain those two stores to a
line already in the core's cache, so the update costs one locked add plus a
few cycles. Neither touches another core's line, so the ratio should not fall
with more cores. W7t / W7 within 0.01 of 1. With the reader: W7rd / W7 at
least 0.99, and a median whole read of 1 to 10 µs at 8 P-cores: each of the
eight written shards costs about two line transfers to and from the reader
(its ticket and its release), each of the order of 100 ns, and the eight
spare shards stay in the reader's cache.

**Falsifiers.**

- W7 / W5pN below 0.5 at any list (the prediction fails, and so does the
  gate): the locked add is not the dominant cost, or the lock's other steps
  cost far more than estimated.
- W7 / W5pN above 1: the model does less than it states (for example, the
  compiler removed the lock), which is a model defect to fix before the
  result counts.
- W7's rate per writer at 8 P-cores below 0.9 times its rate at 2 P-cores,
  where W5p's per-thread rate stays within 0.2% (Table 9.3): a line shared
  between writers sits on the update path, a model defect, or the header line
  is not read-only as intended.
- W5pN / W5p further than 0.05 from 1: the thread-count convention matters,
  and the choice of W5pN as the comparison is reported with both ratios.
- W7rd / W7 below 0.95, or a median whole read above 50 µs: the reader's
  locks interfere more than estimated (the reader criteria fail on the
  model).

### 8.7 Sharded-state implementation run (registered before measurement)

This run measures the branch implementation directly, with the compiler built
from the checked-out branch revision on the 14900K runner and each Whitefoot
arm built with `--full-lto`. It adds W4, its separate-build twin W4t, W4r,
W4rd, M1–M4 and A1 to the existing witness arms. The default rotation is
`W0 W1 W2 W2t W2r W6 W3 W3rd W4 W4t W4r W4rd M1 M2 M3 M4 A1 W5 W5p`.
An `ARMS` selection omitting a required comparison or twin leaves that
criterion inconclusive. Firn F0–F3 remain a separate downstream run.

**Procedure.** CI only on the i9-14900K, under the host-wide lock of
`run-check.pl`, after checking under 5% CPU busy over 5 s. Record the branch
revision, compiler and twin binary hashes, CPU map, governor, CPU lists and
order in `host.txt`. Pilot first: one block of 5 s cells at
`0,2,4,6,8,10,12,14`; it checks run length and wiring and decides nothing.
The main run has 12 blocks, 5 s per cell, 64 writer contexts (64 threads for
W5/W5p), at `0,2`, `0,2,4,6`, `0,2,4,6,8,10,12,14` and `0-15`, with
`WF_DRIVERS` equal to the CPU count. Reader and drain contexts are extra,
scheduled on the same drivers. Use `ORDER=alternate`: rotate the starting
arm by one each block and reverse the rotated order in odd blocks, as section
8.6 does. Record a changed block count before dispatch; fewer than six paired
blocks decide nothing. The pilot precedes a main dispatch, rather than
automatically launching a longer run. With all 19 arms, the main has 912
cells, 4,560 s of nominal cell time, before startup and build costs.

**Decision rule.** Reuse section 8.6's paired-block medians and 95%
percentile bootstrap intervals: 10,000 resamples, `random.Random(20261012)`,
the 251st through 9,750th sorted resample medians. At each CPU list the
W4t/W4 interval must contain 1, with at least six complete paired blocks;
otherwise that list's criteria are INCONCLUSIVE. A throughput lower-bound
criterion is PASS when the interval's lower bound reaches the threshold,
FAIL when the upper bound is below it, and INCONCLUSIVE otherwise. Missing
cells within observed block numbers and nonpositive rates leave the affected
criterion inconclusive. The reducer cannot infer a wholly absent block from
`raw.tsv`; acceptance also requires a successful workflow and agreement with
the block count recorded in `host.txt`. A partial artifact from a failed or
interrupted run does not complete this procedure. Cross-list
scaling ratios pair the same block at both lists and require valid twins at
both endpoints. Report medians, intervals, sample counts and thresholds:

- W4/W5p ≥ 0.5 at 2, 4 and 8 P-cores; the 16-hyperthread ratio is descriptive.
- W4/W3 ≥ 1.25 at 8 P-cores and 16 hyperthreads; smaller lists are descriptive.
- W4rd/W4 ≥ 0.95 at 8 P-cores; other lists are descriptive. The longest
  W4rd whole read across all cells at 8 P-cores must be ≤ 50 µs. This is the
  requested implementation criterion, rather than the C model's median-read
  criterion: PASS only with valid twin evidence, at least six complete reader
  blocks and at least one read in every cell, all maxima ≤ 50 µs; FAIL if
  an observed maximum exceeds 50 µs under those evidence conditions.
- W4(8 P-cores)/W4(2 P-cores) ≥ 3, the board's count-scaling criterion.
- A1(4 P-cores)/A1(2 P-cores) ≥ 1.6 and
  A1(8 P-cores)/A1(4 P-cores) ≥ 1.6, the board's owned-buffer criteria.

The W4 twin controls noise in this run; it is not an A1 twin and cannot
establish A1-specific variability. M1–M4 and W4r are descriptive controls.
No new threshold replaces section 8.5's implementation thresholds. A failed
criterion follows section 8.5's attribution steps. With inconclusive twin
or interval evidence and no failed criterion, repeat the whole main run at
most once; the repeat decides alone, both runs are reported, and unresolved
evidence goes to the owner, as section 8.6 does.

**Generated-code check.** Record emitted LLVM handle-count calls for every
selected Whitefoot arm, distinguishing explicit handle sharing and teardown
from the update statement. W4's shard statement must have no retain/release.
The requested W2 statement falsifier is unavailable on this branch: the
handle-place liveness optimization merged from PR #409 also skips W2's
statement because its block does not overwrite or move its value handle.
Program-wide sharing or teardown calls do not falsify that optimization.
Keep the W2 workload unchanged and expose this limitation with its counts;
do not attribute a throughput difference to retained W2 statement counts.

**Recorded before the main dispatch (2026-10-13).** The pilot, run
38412811971 at 7e93d8718, built every arm and ran one block at 8 P-cores.
It found W4t/W4 at 0.999, W4/W5p at 0.743 and W4/W3 at 5.61, each far from
its threshold, and the W4rd reader and the A1 drain starved: one whole read
and one drain in a 5 s cell instead of about 5,000 and 500, while the 64
writers never wait. The main run therefore has 6 blocks, the registered
minimum, and the arms `W3 W4 W4t W4r W4rd M1 M2 M3 M4 A1 W5p`, every
comparison and twin a criterion above needs; W0, W1, W2, W2t, W2r, W6, W3rd
and W5 were measured in the section 8.6 runs and decide nothing here. W4rd
and A1 still run, at every list, to record the starvation; a reader or
drain cell with fewer than half its nominal reads is reported as starved,
not as a pass or fail of the reader and A1 criteria, which wait for the
single-driver progress decision.

### 8.8 Taken-yield cost attribution (registered 2026-10-13, before measurement)

**Question.** Is the W4 throughput loss after the cooperative-yield fix
primarily the cost of taken yields, rather than the inline boundary that
every shard statement now executes? The comparison supplied for this
registration is run 38427370760 after the fix against run 38413746180 before
it: W4 with 64 contexts fell from 368.7 / 737.9 / 1476.3 to
277.3 / 496.3 / 948.8 million operations per second at 2 / 4 / 8 P-cores.
These two runs motivate the question; they are not the same-source
attribution comparison below. Code inspection suggests three process-global
atomic read-modify-writes per context switch among nine such instructions;
their contention is a hypothesis to test, not an established attribution.

**Comparison and construction.** Keep W4 and its independently built twin
W4t at the accepted threshold of 64 immediately answered waiting boundaries.
Add W4n256 with threshold 256 and W4nmax with threshold 1,048,576. Each
variant compiler is built in CI from a disposable copy of the exact same
checkout revision, changing only the inline emitter's threshold literal and
the runtime's threshold definition and matching static assertion, including
its diagnostic text. Each replacement must match exactly once or the build
fails. The branch's compiler and runtime source stay unchanged. Each variant
compiler builds only W4's source entry, with the same full-LTO options; its
retained LLVM also belongs to that entry. Require each variant executable to
differ byte for byte from W4, and require W4 to equal a repeat build of the
same entry by the unpatched compiler. Record the source revision, compiler
and executable hashes, thresholds and checks with the host evidence.

W4nmax reduces the nominal yield frequency 16,384-fold from W4. It is a
low-yield diagnostic, not literally a never-yield arm: even at the supplied
W4 rates a 5 s cell would cross that threshold roughly 1,322 / 2,367 / 4,524
times in aggregate. Displacement can also cause a switch before the
threshold. These variants are diagnostics only and never accepted behavior;
all implementation acceptance criteria still use W4 at N=64.

**Procedure and evidence rule.** Run `ARMS="W4 W4t W4n256 W4nmax W4rd W5p"`
with `BLOCKS=6`, 64 writers and 5 s cells, using section 8.7's CPU lists,
alternating rotation, idle check and CI lock. The attribution decision lists
are 2 / 4 / 8 P-cores; 16 hyperthreads remain descriptive. Reuse the
same-block ratios, six complete paired blocks, 95% percentile bootstrap
intervals and W4t/W4 interval containing 1 from section 8.7. Report
W4n256/W4, W4nmax/W4 and W4nmax/W5p at each decision list. A partial workflow,
missing comparison or invalid twin leaves the affected prediction
inconclusive. These are attribution predictions, not replacement acceptance
thresholds.

**Predictions and falsifiers.** Registered before dispatch:

- If taken switching dominates, W4nmax/W5p returns to at least 0.70 at each
  of 2 / 4 / 8 P-cores. Use section 8.7's lower-bound rule to read this
  prediction.
- W4n256 recovers most of the W4-to-W4nmax throughput gap at each decision
  list: the paired-block fraction `(R256 - R64) / (Rmax - R64)` is at least
  0.50, where R is the cell's aggregate operations per second. A nonpositive
  denominator cannot support a recovered-gap claim and leaves it
  inconclusive. Use the same interval rule for the 0.50 prediction.
- If W4nmax stays below 0.60 of W5p at a decision list (the interval's upper
  bound is below 0.60), taken yields are not the dominant cost there; inspect
  the inline code next. Between 0.60 and 0.70 the comparison does not settle
  the attribution.
- If the incremental loss per taken yield does not grow from 2 to 8
  drivers, the process-global-counter explanation is not dominant. For d
  drivers, rates R64 and Rmax and **measured** taken-switch rates s64 and
  smax, estimate service-time cost per removed yield as
  `d * (1/R64 - 1/Rmax) / (s64/R64 - smax/Rmax)` seconds. Pair the 8-driver
  estimate over the 2-driver estimate within the same block; an upper
  interval bound at or below 1 falsifies growth, a lower bound above 1
  supports it, and an interval crossing 1 is inconclusive. Nonpositive
  denominators or costs cannot establish this attribution.

The current scheduler report exposes borrowing and reassignment, not taken
yield counts. Without those measured switch rates the last falsifier stays
inconclusive. The reducer may report the model obtained by substituting
`sN = RN/N`, clearly marked as nominal, not measured: it assumes one
threshold-driven yield per N updates, continuous runnable competition and no
extra displacement switches. The model's growth is descriptive evidence
only and cannot decide the global-counter explanation. Varying thresholds
also changes how often the slow boundary and completion reaping execute, so
a recovered rate attributes the threshold-driven path as a whole, not the
three global instructions individually.

**Reader distribution (descriptive).** W4rd remains at N=64. Record each
read interval exactly as before: from the clock reading after sleep returns
to the clock reading after the whole-target statement, including its wait
and sum. Accumulate a context-local fixed histogram after the end clock
reading and publish it once on completion. Buckets have inclusive upper
bounds 1, 2, 4, ... , 2^63 nanoseconds; the last bucket holds larger spans.
The reducer reports each cell's nearest-rank p50 and p99 as bucket upper
bounds capped by that cell's observed maximum, including the overflow bucket, along
with its exact observed maximum and read count. Missing or malformed
histograms are unavailable evidence, never a zero latency. These new
percentiles are descriptive; they do not change section 8.7's maximum-read
acceptance criterion.

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
  upper bound, which decides R2 in one direction only. The array would be
  enough only if it reached 0.8 at every list, so one list clearly below
  settles it; W3 / W1 is below 0.8 at all four: by 18% of the threshold at 2
  CPUs (2.0 times `e`), 40% at 4 (4.4 times), 37% at 8 (4.0 times) and 62% at
  16 hyperthreads (6.8 times), and W3's best block stays below 0.8 of W1's
  median at every list (section 9.2). The stand-in therefore does not show
  the array to be enough. R2 itself stays undecided until the sharded
  state is measured (W4): if W4 is at most 1.25 times W3 at every list, R2
  then rejects the language change after its implementation.
- **R3** (cells (a) are recommended when W5 / W5p ≥ 0.5 at 8 P-cores). W5 /
  W5p = 0.059, 88% below the threshold (9.7 times `e`); the most favourable
  pairing of blocks, W5's highest against W5p's lowest, gives 0.064. Not met.
- **P3** (W5 / W5p ≤ 0.25 at 8 P-cores) holds, 76% below its bound.
- **F-heap** (descriptive), answered by the acceptance numbers of the
  lock-free heap reading rather than a new arm: keeping the exact maximum
  costs Firn about 5% of its GET rate, 25.65 against 26.99 million a second
  for the build with no maximum, across two runs and within the twins'
  spread there (section 9.4).

Against the owner's ruling: the hand-built array is not shown to be enough
and the hardware add reaches 0.059 of the private rate at 8 P-cores, not
half, so neither of the first two outcomes holds, and the third applies: implement
the sharded state, with its specification change on its own approval card.
The change is section 10, its design-tree decisions section 11
and its implementation plan section 12. The maximum needs no language change
(section 7), which section 9.4 now supports with a measurement.

## 10. Specification changes

The owner-approved sharded-state change (c) is recorded as specification
v0.143 in `spec/log.md`; the merged base is archived byte for byte as
`spec/kernel-spec-v0.142.md`. The surface is section 5.4's with one name
changed, the token being called the *shard selector* and a target naming one
shard a *shard target*, and with the copy bound and copied argument replaced
by a `drop` bound and a maker ([owned shard values](#owned-shard-values)).

### 10.1 Rule by rule

| Rule | Before the sharded-state amendment | Approved sharded-state amendment |
|---|---|---|
| TYPE-2 | The functions of SHARE-1 form the shared and frozen handles, maps and key sets. | They also form shards states and the shard selector. |
| TYPE-9 | Homes table: storage forms in a `Box`; a map in a shared object's state or a `swap`'s exchange. | Adds `Shards<T>`, whose one home is a shared object's state, not a `swap`'s exchange; the consequence sentence says a `swap` of two whole shards targets is refused. |
| TYPE-10 | `len`, `cap`, `head` are the readonly fields of the storage shapes, the key set and the entries. | Adds the shards state's `len`. |
| TYPE-11 | A target whose handle place is `Shared` or `SharedRead` of the struct establishes its invariants at the block's entry and owes them on leaving; no site covers an element or a map entry. | Adds a consequence sentence: a shard target's handle place is `Shared` or `SharedRead` of a `Shards`, so its block neither assumes nor owes the shard's invariants, as for an element or a map entry. Accepts and refuses the same programs as the sites list alone. |
| REF-4 | A range reference over a `Ring` or an `Entries` is refused with its reason. | Adds the refusal of a range reference over a `Shards`, whose shards are kept apart and are no run of storage. |
| OP-4 | Indexable bases include `Entries<V>` and `ConcurrentHashMap<V>`, reached through `^`. | Adds `Shards<T>`, reached through `^`, its elements its shards of type T. The existing sentences then give it a `u64` offset and the obligation `i < p.len`, so an `AnyShard` offset is refused. |
| OP-9 | Layout pairs of the prelude's opaque structs; any other fieldless one `(32,16)`. | Adds `Shards<T>` `(8,8)`, its `len`, its shards living in its object and entering no sequence, and `AnyShard` `(1,1)`, carrying nothing. |
| MSR-1 | Measures table rows. | Adds `Shards<T>`: `len` counts shards, exact; no `cap`, `head` or `bytes`. |
| MSR-2 | Standing facts: `Z <= P.len` and the others, and the constants of `Array<T, N>`, `Slots<T, N>` and `Ring<T, N>`. | Adds the standing fact `1 <= P.len` for every `Shards<T>` place, with empty support, so it holds in a whole block and in a callee receiving `&Shards<T>` alike. |
| ENT-2, exact paths | No step of an exact path is a loop header's opaque identity, a keyed entry's position or a run of every segment. | Also names a shard target's shard. The shard step was already excluded, its offset being no literal, const or read binding; the list now says so. |
| ENT-3.S1 | At an `atomic_stmt` with a guard, the guard's goals and comparison are established at the block's entry. | At every `atomic_stmt` the entry facts SHARE-2 and TYPE-11 state for its targets are also established there, so the key-set binding's `es^.len == ks.len`, which v0.135 states in SHARE-2 with no source among ENT-3's, now has one. Accepts what an implementation of v0.135's SHARE-2 sentence accepts. |
| SHARE-1 | Shared and frozen objects, maps and key sets. | Adds the shards state: `len` shards of T; releasing it releases every shard's value; `shared_shards_new` calls its maker once per shard and stores each result; `len` is at least one, fixed for the object's life and an input of the execution that an implementation may choose by the driver count; a shard target names one shard, which shard being an input, with no index and so no bounds obligation; `any_shard` returns the one selector value; after creation a shard changes only by the writes of the blocks of statements whose targets name it, under their operations' own rules, and no implementation step combines or adjusts shards; by SHARE-3, a total a whole block computes counts exactly the statements on the object that took effect before its point. |
| SHARE-2, target places | A whole handle place, or a map handle place with one index step whose atom is a byte range or a key set. | Adds a `Shared<Shards<T>>` or `SharedRead<Shards<T>>` handle place with one index step, the target's own, whose atom is a place of type `AnyShard`: the shard target. |
| SHARE-2, binding table | Three rows: whole, entry, key-set. | Adds the shard target's row: `&T`, whose path is the state extended by an index step whose offset no relation names [OWN-7], the shard the target names. A whole target over a shards state uses the first row, binding `&Shards<T>`. |
| SHARE-2, overlap | Two targets on overlapping handle places each have an index step; a whole target is alone on its place. | Two targets on overlapping handle places are each an entry or key-set target; a whole or shard target is alone on its place. Two shard targets on two handles of one type compare as one root with an index step no relation names, so they may overlap (EFF-5 then refuses one call writing both). |
| SHARE-2, facts | A key-set binding establishes `es^.len == ks.len` at the block's entry, "as a guard's comparison is established there". | The same fact, now citing ENT-3.S1 as its source. |
| SHARE-2, guard | The guard writes no path. | The guard also reads no path that starts at a shards state. |
| WAIT-2 | Inputs of the execution include map positions, scan extents, released and key-set bytes and the driver count. | Adds the number of shards of each shards state and the shard each shard target names. |
| PRE-1 | Opaque structs through `Entries`; functions through `release`; the preorder lists type, const and value parameters. | Adds `opaque nocopy struct Shards<T: drop> { readonly len: u64; }` and `opaque struct AnyShard { }` after `Entries`, `fn shared_shards_new<T: drop, fn make() -> made: T pure>() -> result: Shared<Shards<T>> pure;` and `fn any_shard() -> result: AnyShard pure;` after `shared_map_new`, and both functions in the preorder there, which moves every later prelude ordinal; the preorder also lists function-kind parameters, `make` being the first a prelude record declares. |

Unchanged, and why: SHARE-3, since its meaning is already exclusive access to
the whole states the targets name and its last sentence admits holding one
shard where the outcomes are unchanged, which they are because a shard
statement reaches only its shard; WAIT-1, WAIT-3, CAP-1, PAR-1, PAR-2 and
HOST-1, since shard and whole statements are atomic statements and so waiting
calls without overlap permission; EFF-1 to EFF-5, since a state is a path of
no row and a callee receiving `&Shards<T>` uses ordinary rows; OP-2, since a
shard update is the writer's arithmetic; OP-11, OWN-1, WIN-3 and PROV-6,
since a shard is an owned place that `swap` and `set` treat as any other and a
shard binding is a reference, so an owned shard value is exchanged or replaced
but never moved out, and the `drop` bound is PROV-6's ordinary filter; FN-3,
FN-4 and STOR-8, since a prelude record's function-kind parameter is checked
and followed as any other.

### 10.2 The guarantees, rule by rule

- **No data race.** Shards are reached only through targets [SHARE-1], and
  every access has SHARE-3's exclusive meaning. Holding one shard instead of
  the object is SHARE-3's last sentence, not a new rule.
- **No silent overflow.** No operation adds anything for the program
  [SHARE-1]: each shard update and each total is a block's own arithmetic, so
  `+` needs a proof (none exists for a concurrently changing value, section
  3.2), and `+wrap`, `+checked` and `+sat` state their results [OP-2].
- **No leak or double free.** A shard always holds one owned value: `swap`
  exchanges [OP-11], `set` replaces and releases [WIN-3], a consume through
  any reference to a shard is refused [OWN-1], releasing the state releases every shard
  [SHARE-1], and a linear shard type is refused by the `drop` bound [PROV-6].
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
  two further, so a program learns nothing it relies on beyond MSR-2's
  standing `1 <= len`.

### 10.3 Conformance cases

Twenty cases in `tests/conformance/cases/`, their manifest lines after the
existing shared-object cases. Each states the verdict the specification
requires; all reached those verdicts in CI at `8cf048252`, whose unit tests
also passed, including the prelude-record and measure-table comparisons
with the specification's text. No verdict was chosen to pass.

| Case | Verdict | What it shows |
|---|---|---|
| `share-pos-shards-count` | run, exit 40 | four contexts add one ten times each through shard targets; a whole read after the joins totals 40 whichever shards were named |
| `share-pos-shards-start-from-maker` | run, exit 11 | every shard holds what the maker returned, and the standing `1 <= len` admits `all^[0_u64]` |
| `share-pos-shards-maximum` | run, exit 9 | `imax` through shard targets, maximum over the whole run |
| `share-pos-shards-beside-map-entry` | run, exit 31 | one statement names a map entry and a statistics shard |
| `share-pos-shards-whole-to-callee` | run, exit 70 | callees read and reset a `&Shards<u64>` under ordinary rows |
| `share-pos-shards-owned-buffers` | run, exit 60 | each shard owns a `Box<Slots<u8>>` the maker allocated; three contexts append through shard targets; whole targets swap every buffer for a fresh one and sum what they took, 24 then 20 then 0, with 16 writes counted |
| `share-neg-shards-two-on-one-handle` | reject SHARE-2 | two shard targets on one handle place |
| `share-neg-shards-shard-beside-whole` | reject SHARE-2 | a shard target beside a whole target on one place |
| `share-neg-shards-guard-reads-shard` | reject SHARE-2 | a guard on a shard's value |
| `share-neg-shards-guard-reads-whole` | reject SHARE-2 | a guard on the whole state's `len` |
| `share-neg-shards-index-not-a-selector` | reject SHARE-2 | an integer where the selector belongs |
| `share-neg-shards-two-handles-one-call` | reject EFF-5 | shard targets on two handles of one type may be one shard |
| `type9-neg-shards-parameter` | reject TYPE-9 | a `Shards<u8>` value parameter |
| `type9-neg-shards-swapped` | reject TYPE-9 | a `swap` of two whole shards targets |
| `prov6-neg-shards-element-nodrop` | reject PROV-6 | a `nodrop` shard type against the `drop` bound |
| `fn4-neg-shards-maker-waits` | reject FN-4 | a maker that waits, against the formal that does not |
| `own1-neg-shards-move-out-of-shard` | reject OWN-1 | moving a shard's buffer out through the shard binding |
| `op4-neg-shards-past-the-first` | reject OP-4 | `all^[1_u64]` is unproved, since only `1 <= len` is known |
| `op4-neg-shards-selector-as-subscript` | reject OP-4 | the selector as a subscript offset |
| `ref4-neg-shards-as-a-range` | reject REF-4 | a range reference over the shards |

The owned-buffer case's exit status separates the faults it guards: a swap
that left the old buffer in place would give second and third drains of 44
and 44 instead of 20 and 0, a lost append a first sum other than 24, and a
lost update of a shard's count a total other than 16. Its capacity, 16 bytes per shard, holds all twelve
first-round bytes even when every context lands on one shard, so the status
is the same for every number of shards and every selection. It does not
observe a leak or a double free; the runtime test under AddressSanitizer in
section 12 does.

The program test that section 5.4 names is `tests/programs/shared_shards.wf`:
16 contexts each increment two fields together 20 000 times, while a reader
checks 10 000 whole snapshots with separate passes over the two fields.
Both final sums after the joins must be 320 000. Its program-test adapter
builds once and runs on one driver and on four, three times each. It exercises
the runtime's shard selection and exclusion rather than a source rule;
this added coverage awaits CI.

For (b): no specification change (SHARE-3's last sentence). For (a), if ever
adopted: PRE-1 declarations of `SharedCell<T: Int>` and its ten functions, a
SHARE-1 paragraph giving each operation's single-point meaning and
`cell_add_checked`'s unchanged-on-overflow rule, and the target capability row
of relaxed-fields decision 6 for the hardware route; section 9.3 is the
evidence against it for counts.

## 11. Design-tree changes

Owner-approved on the work branch, as recorded in `design/log.md`:

- New node `design/language/waiting/shared-objects/sharded-states.md`: the
  shards state and its two target forms; that the number of shards and the
  selected shard are inputs and the program never supplies an index; the
  guard refusal; the `drop` bound with shards made by a pure maker that does
  not wait; the state-only home; the selector as a fieldless copy equality
  type in the existing target grammar; and that a shard is no type-invariant
  site (provisional). Rejected: single-step integer cells for counts (a),
  provisional and limited to counts; the hand-built array as the answer (d2),
  provisional; the hidden combine (c′); a runtime peak in the memory meter
  (d3); whole-program detection (d4); per-context accumulation flushed (d1);
  a maker with a seed argument; shards of `Option<T>` starting `None`; and a
  dedicated target syntax for the selector. The rejections of (a) and (d2)
  cite sections 9.3, 9.6 and 9.2; (a)'s is reopened by a run under a noise
  rule that decides contended arms with the ratio within its noise of 0.5,
  and (d2)'s by R2, if the implemented state reaches at most 1.25 times the
  array's rate at every CPU count.
- `design/language/waiting/shared-objects.md`, third decision: the target
  forms gain the shard target, alone on its handle place.
- `design/compiler/waiting-contexts/state-locks.md`: first decision, a sharded
  object is held by one shard or whole; second decision, its shards are taken
  in index order; third decision, a whole hold absorbs shard holds and shard
  targets on one object hold one shard; and seven new decisions, each with its
  own grounds and refused alternative: the one-allocation layout with the
  header and each shard on lines of their own; a first-come lock per shard,
  which excludes only whole targets, with the design without a writer-side
  atomic instruction as its second step; the number of shards (provisional);
  the selection `user mod len`, which gives a shard one writer; the lean shard
  update, which changes neither the handle count nor the holder count and
  inlines the lock's fast path; no watch on a shard; and construction by the
  maker in index order before the first handle, with the last release running
  the shard type's release on each shard.
- For (b) B2, if pursued: `design/language/waiting/shared-objects/keyed-tables.md`'s
  refusal of version-checked reads narrowed to blocks that run on racing
  state, with the account in 5.3; owner decision required. Not part of this
  change.

## 12. Implementation plan

The checker, lowering and runtime are implemented on this branch. The
conformance cases reached their verdicts and unit tests passed in CI at
`8cf048252`. The end-to-end contention program completes the program-test
coverage below. PR #409 (skip handle count) is merged into this branch, and the 14900K
acceptance measurement has not run. The bullets below describe the
implementation obligations, including that prerequisite.

**Gate before any of it.** The C model of section 8.5 (W7) runs first, under
the procedure and decision rule of section 8.6; below 0.5 × W5p the plan
below is replaced by the design without a writer-side atomic instruction.

**Prerequisite.** A shard statement must not write the object header. Today
every statement through a value handle counts the handle on the header
(section 2.2; W3 shows it, section 9.2), which would put one contended line
back on every update. The board item "an atomic statement through a value
handle does not change the handle count" (gran-skip-handle-count: skip the
count when the block neither moves nor writes the handle place) is PR #409,
merged into this branch with main; the lean shard path of section 8.5 uses
it through the same skip every target takes.
W4r in section 8.2 separates the two if it is not.

**Checker.**

- Prelude records: the two opaque structs and two functions in the order
  PRE-1 states, so the record test and the diagnostic ordinals follow the
  specification; `Shards` gets its own storage identity, as `Entries` has, and
  a measured kind with `len` exact for the measure-table test.
  `shared_shards_new` is the first prelude record with a function-kind
  parameter: its `make` formal is checked against each written actual by the
  ordinary FN-4 path (a waiting or effectful maker is refused there), the
  call graph gains the edge to the actual that FN-6 and the no-heap closure
  [STOR-8] already follow for source generics, and the preorder places the
  formal between the type parameter and the value parameters.
- Bound [PROV-6]: `T: drop`, the ordinary filter at the call.
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
- Facts: every `len` term over a `Shards<T>` place carries the standing fact
  `1 <= len` with empty support [MSR-2], where the standing measure facts are
  added, so a callee receiving `&Shards<T>` has it too; the key-set binding's
  length equality stays where it is published now, at the block's entry,
  which ENT-3.S1 now names as its source.
- Type invariants [TYPE-11]: no change; a shard target's handle type is not
  `Shared` of the struct, so the existing site check already gives it none.
- Diagnostics: each new refusal carries its repair [DIAG-1]: a shard target
  for an integer index atom (`let mine = any_shard();` and `&h[mine]`), a
  loop over `len` for a range over the shards, and a second statement for a
  shard target beside another target on its handle place.

**Lowering.**

- A shard target lowers to the address of the running thread's shard, its
  lock taken and released by a fast path inlined in the generated code, with
  the runtime called only when a whole target holds the lock; the binding is
  that address. It changes no holder count; PR #409 omits the handle-count write
  on a shard statement whose block keeps the handle place (section 8.5). The statement's lock-order group is the object's type,
  as for any target (state-locks, second decision).
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
- `AnyShard` carries nothing: `any_shard()` lowers to no code, and a shard
  target's index atom enters the statement's footprint as a read [SHARE-2] and loads nothing.
- `shared_shards_new::<T, fn make>` is emitted per actual, as a source
  generic instance is: it reads `len` from the runtime, makes one allocation,
  header and shards together, and calls `make` for shard 0 up to shard
  `len - 1`, storing each result in its shard, before it returns the first
  handle, so no other context can observe a shard that is not yet made.
  Allocation is total [STOR-8], so no shard is ever left unmade.
- The object's release, when the last handle goes and no statement holds it
  [SHARE-1], runs T's compiler-derived release [STOR-3, PROV-6] on each shard
  in index order and then frees the allocation. The handle's drop helper,
  which today releases a shared object's state in place when it removes the
  last handle (`emit_shared_drop_helper` in `backend/emitter/cleanup.rs`,
  read at this branch's head), gains a loop over the shards that runs T's
  cleanup jobs on each; for a T that needs no cleanup, a count or a
  statistics struct, the loop is omitted and the release stays one free.
- A shard's owned content is ordinary storage: a `Box` a maker allocates, a
  buffer a whole statement swaps in and one it releases after the swap are
  counted in the heap the program holds like any allocation [PRE-2], with no
  shard-specific accounting.

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
  starts and ends on one thread, and a thread runs one statement at a time; a
  displaced thread and its spare are two threads with two shards (section
  3.6). A shard therefore has one writer, and its lock excludes only a whole
  target on another thread.
- Each shard's lock is first-come (a ticket lock): uncontended it costs one
  locked atomic instruction on the thread's own line, the cost class of C
  own-line (section 8.5), and a whole statement waiting for a shard waits for
  at most one shard statement per shard, which keeps WAIT-2's promise for the
  whole statement however fast shard statements arrive.

**The read-all path.** A whole statement costs `len` lock acquisitions in
index order plus the block's loop. At 32 shards the estimate is a few
microseconds; W3rd's 16-target read took 0.6 to 5.6 µs on average (section
9.2), the comparable figure. While it runs, shard statements on shards it
already holds wait; ones on shards it has not reached proceed.

**Validation.**

- The twenty conformance cases of section 10.3, the compiler record and
  measure tests, and `tests/programs/shared_shards.wf` of section 10.3.
- A runtime test of the shard lock and the whole hold under ThreadSanitizer,
  as the concurrent map's test runs, with a whole reader racing shard
  writers and an exact total checked after the joins, and the same test
  with owned shard buffers under AddressSanitizer: writers append to their
  shard's buffer while a whole statement swaps every buffer out and releases
  what it took, and the object's release frees the rest, so a leak, a double
  free or a use after a swap fails it.
- Firn's acceptance for its pending buffers, once a released compiler carries
  the change: its witness compiles, and on the 14900K 64 contexts writing
  100 000 random keys, with a swap-out of every shard each 10 ms, scale by at
  least 1.6 times per doubling of P-cores from 2 to 4 to 8. Firn's session
  runs it; it is Firn's completion condition, not one of this study's
  registered predictions.
- The registered experiment's remaining arms on the 14900K (section 8.2): W4,
  W4r and their reader variants, M1 to M4, and the Firn arms F0 to F3 from
  Firn's session, deciding the criteria of section 8.5 (which replace P1), P2,
  P4, P5 and R1, R2, R4, R5. The C model W7 runs before any of it (section
  8.5). If the noise rule of section 8.3 is to change for that run, the change
  is recorded before it measures (section 13).

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
- **The shard selector is an equality type.** `AnyShard` is a struct that
  removes no capability and has no field, so [OP-16] makes it an equality
  type and `any_shard() == any_shard()` is admitted, always true. It gives a
  program nothing to learn and no way to choose a shard. Disposition:
  declined; no rule changed for it, and the sharded-states node now records
  the property with the selector's decision.
- **The key-set binding's length fact had no ENT-3 source.** v0.135's SHARE-2
  says a key-set binding establishes `es^.len == ks.len` at the block's entry
  "as a guard's comparison is established there", while ENT-3 says nothing
  else is a fact and its S1 names only a guarded statement's guard. The
  draft's `len >= 1` sentence for a whole shards binding copied that shape.
  Disposition: fixed in the draft; ENT-3.S1 now establishes every entry fact
  SHARE-2 and TYPE-11 state for a statement's targets, and the shards fact
  became MSR-2's standing `1 <= len`.
- **Type invariants never reach elements, map entries or shards.** TYPE-11's
  sites are constructions, parameters, results, `shared_new` and
  `frozen_new` arguments and targets on `Shared` of the struct itself, so a
  struct with an invariant held as an element of a storage shape, as a map
  entry's `Some` value, or as a shard gives a program no fact and owes
  nothing until it reaches one of those sites. Sound, since nothing assumes
  the invariant there, but such a struct's invariant is useless in the
  places shared state keeps it. A map entry target `&Option<V>` over a
  struct V is the case on main. Disposition: not fixed here (it needs
  element-position obligations, a TYPE-11 redesign); a board backlog item
  for the Whitefoot project, with this paragraph as its witness: a map of a
  struct whose invariant is `used <= cap`, read through an entry target and
  passed to a function taking `&V`, is refused for an invariant the program
  never broke.
- **`swap` of two shards' contents in one call may be refused.** OP-11's
  equality allowance names "equal-depth slots under one identical array or
  window"; `swap(first: &all^[i].log, second: &all^[j].log)` is two
  equal-depth places under one `Shards`, and `Entries` has the same shape, so
  the allowance as written does not cover them and the call may be refused
  as possibly equal and not proved disjoint. Inferred from reading, not
  checked against the compiler. A swap through a local avoids it, and the
  drafted cases do not need it. Disposition: deferred; check when the
  checker change lands, and if refused, propose extending the allowance to
  every indexable base of OP-4.
- **A prelude record had never taken a function-kind parameter.** PRE-1's
  preorder sentence listed type, const and value parameters only. Disposition:
  fixed in the draft by naming function-kind parameters there.
- **Name collision to keep in mind.** OP-12 calls `set p = f(move p, ...)` "the
  atomic in-place update" (`:1215`), an ownership property unrelated to
  concurrency. The drafted surface uses "shard" and "selector" and no
  "atomic" in its operation names.

## 14. What is established and what is not

Established by reading the specification, design tree and source at
`4ad4f5786`: the rules in section 3; that whole-object statements take write
holds and that even read holds write the object's line; the handle-count
traffic; the runtime's own counters local to each driver and compute worker and the decision that chose
them; the prior refusals in section 4; and that (c) needs no new memory model
because its targets are ordinary atomic statements.

Measured on the 14900K (section 9, runs 38303980083 and 38304337014): the
hand-built array reaches 0.31 to 0.66 of the private-object rate and falls
from 8 P-cores to 16 hyperthreads; a contended hardware add reaches 0.059 of
private lines at 8 P-cores; the two witness rows of section 2.1 for one shared
object and for own objects reproduce; a read-only statement collapses as a
writing one does. The registered noise, 9% between the contended arm's twins,
exceeds the provisional 3% limit, so these readings rest on their distance
from the thresholds, 2.0 to 9.7 times that noise, not on the registered rule
(section 9.6). Measured by Firn and quoted (section 9.4): Firn's GET with the
lock-free heap reading at 0.950 of the build without a peak, across two runs.

The specification amendment is owner-approved and recorded as v0.143 in
`spec/log.md`. The checker, lowering and runtime are implemented, and the
conformance cases reached their verdicts in CI at `8cf048252`.

Unverified: every cost estimate for (c) in sections 5, 6 and 12, since the
14900K acceptance measurement has not run (W4); R2's final reading, which needs W4;
B2's LLVM-model correctness; the reader's latency for a whole target; the shard
selection's correctness under a driver handoff to a spare thread (argued from
distinct thread numbers giving each thread its own shard, section 3.6, not tested); that the lean shard path reaches the criteria of section 8.5, which the C model W7 and then W4 will show; that owned shard values are released exactly once at the object's end and that Firn's pending buffers scale as its acceptance needs, which the AddressSanitizer test and Firn's run of section 12 will show.
