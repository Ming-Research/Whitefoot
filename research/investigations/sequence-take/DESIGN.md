# Taking a global sequence number inside an atomic statement without holding it exclusively

## Status

The design study and registered comparisons were written before implementation
or measurement, against specification v0.135 and Whitefoot `c52e867ca`.
Source line references in those sections identify that historical revision.
The owner selected candidate (V) on 2026-10-13, reporting Q1 S1/S0 = 1.000
against the 0.75 go threshold and Q2 T2/T0 = 0.015 against the same threshold.
Those reported results authorize the implementation; they are not new
measurements made by this amendment.

The draft implementation uses `AtomicValue` and specification v0.151, with
v0.150 as its main-line base. Section 12 records its name comparison and
integration boundaries. Compiler and program tests and the Q3 experiment
entry are authored but have not been run in this worktree. Each original
argument remains labeled **deduction**, **measured**, **source-read** or
**estimate** according to its evidence.

It answers Firn's request on the status board, "an atomic statement can take
a globally increasing number without holding the number exclusively". The
request reopens part of the owner's ruling on shared counters (2026-10-12,
option A, then the measurement that selected the sharded state; study and
draft in [Whitefoot#385](https://github.com/Ming-Research/Whitefoot/pull/385),
`research/investigations/shared-counters/DESIGN.md` on that branch, cited
below as "the counter study"). The owner decides on a card; this record gives
it its options and evidence, and registers the measurements the choice needs
(section 7).

Revised after the owner's first comment on the card (2026-10-12): the owner
asked whether every atomic statement would pay for the take, whether Firn's
code would need two statement forms for AOF on and off, found the take target
inelegant, and asked whether a general shared atomic value, needed in more
than one place, would serve better. Section 5.6 adds that candidate (V), and
sections 6 to 9 now compare it with the take target (B) and recommend (V).
The take target's analysis in sections 3 and 5.2 stands: (V) contains it as
one use.

## 1. The question

Firn's append-only file (AOF) work (Firn-wf's write-path study, "Writes
without one exclusive hold on Meta", sections 3 (b), 3 (c) and witness G2 of
section 5) splits the pending AOF buffer into one part per driver, and the
disk writer merges the parts by a sequence number every write takes. Today a
number can only come from a shared object named as a target of the write's
atomic statement, which gives the block exclusive access to it [SHARE-3], so
every write on every core queues on that object for its whole statement.

Stated precisely:

> Contexts on D drivers run atomic statements that each must obtain a `u64`
> number as part of their effect. What language surface lets an
> implementation hand out these numbers at a rate that grows with D, close to
> the rate of the same statements without numbers, while
>
> 1. a number is part of its statement's effect, so it has a meaning stated
>    by SHARE-3's one point and one order;
> 2. if statement A takes effect before statement B and both take numbers,
>    A's number is smaller, and no two statements take the same number;
> 3. near the top of `u64` a take reports exhaustion instead of wrapping,
>    with no silent overflow, data race or unproved partial operation;
> 4. on a target with one thread of execution a take costs a plain
>    increment, and on a multicore target without hardware read-modify-write
>    the take is either implemented within the rules on target capabilities
>    or refused there explicitly; and
> 5. the surface is general, not a path for one program?

Firn's acceptance, restated: Firn's G2 example compiles with a released
compiler; on the 14900K's 8 P-cores its rate is at least 75% of the same
program without the take; and the `(key, number)` log, sorted by number,
replays to the final table in every run.

## 2. The need as a semantic witness, independent of Firn

The structure recurs wherever a concurrent store keeps a log of its changes:
a write-ahead log, a replication stream, commit numbers of a
multi-version store, event sourcing, change-data capture. Each writer logs
its change with a number, the logs are merged by number, and the merged
stream must replay to the store's state.

Minimal witness (sketch in the specification's style, not compiled; the
surface `n = &seq[ticket]` is candidate (B) of section 5.2, used here only to
state the need; the recommended candidate (V) of section 5.6 writes the same
statement as `s = &seq` with `atomic_add(cell: s, value: 1_u64)` in the
block):

```whitefoot
struct Record {
  key: u64;
  value: u64;
  number: u64;
}

fn writer(table: Shared<ConcurrentHashMap<u64>>, seq: Shared<Sequence>, index: u64) -> log: Box<Slots<Record>> pure waits {
  let log = box_slots_new::<Record>(capacity: 100000_u64);
  let ticket = next_number();
  let state = index;
  for @op (i in 0_u64..100000_u64) {
    set state = step(state: state);
    let key = state % 100000_u64;
    let bytes = key_bytes(key: key);
    let taken = None<u64>();
    atomic slot = &table[bytes], n = &seq[ticket] {
      set slot^ = Some<u64>(value: i);
      set taken = n^;
    }
    match taken {
      Some(value: number) => {
        slots_push(slots: &log, value: Record(key: key, value: i, number: number));
      }
      None() => {
        break @op;
      }
    }
  }
  return log;
}
```

The root spawns 64 writers, joins them, merges their logs by `number` and
checks the properties below. The logs here are the writers' own (returned at
the join); Firn keeps them in shared per-driver parts drained every 10 ms,
which needs the separate request for parts holding owned values (Firn's G1)
and changes nothing below.

The required properties, each a check a wrong design fails:

- **N1, distinct and dense.** Sorted, the numbers are `first, first + 1, ...`
  with no repeat and no gap. Fails for any design that hands out numbers
  outside the one order.
- **N2, final-table replay.** Applying the records in number order to an
  empty table gives the final table. Firn's stated acceptance.
- **N3, real-time order.** If statement A completed before statement B began,
  as a host clock read by A's context after A and by B's context before B
  shows, then A's number is smaller. This is the part of Firn's prefix-state
  guarantee (Firn's P1: every number-prefix of the log restores a state the
  server passed through) that N2 does not test; section 3.4 shows why it is
  needed and that N2 alone passes designs that break P1.

## 3. What the specification already defines about effect order

### 3.1 The one order

**Source-read.** [SHARE-3] (`spec/kernel-spec.md:2336-2342`): an atomic
statement takes effect at one point after it begins and before it completes;
its guard and block execute with exclusive access to its targets' states, and
every read and write of those states takes effect at that point. "The atomic
statements and the cancellation-state updates of [PRE-2] take effect in one
order [WAIT-2], and those of one context take effect in its source order."
[WAIT-2] (`:2262`) lists "the order in which atomic statements ... of
different contexts take effect" among the inputs of the execution, and
(`:2263`) "where a context executes, and whether two contexts execute at the
same time, are not observable". SHARE-3's last sentence (`:2342`) lets an
implementation hold less than a statement's states, or hold them together
with other statements, wherever the outcomes equal those of exclusive
execution.

So the specification already defines one total order of all atomic
statements, including statements on disjoint objects. "A takes effect before
B" is defined for every pair. Today a program observes the restriction of
that order only through shared state, so two statements on disjoint objects
have an order no outcome reveals. A take makes the order of taking statements
observable as numbers. It needs no new order and no new input: the number is
a function of the existing input.

### 3.2 What the one order must be consistent with

**Deduction** from the cited rules:

1. **Conflict order.** Two statements whose targets share a state run with
   exclusive access, so the later in the one order sees the earlier's writes.
2. **Context order.** SHARE-3 states it directly.
3. **Spawn and join.** A context's constructs run one at a time [WAIT-2]; a
   spawned context starts after the spawn executes and the starter waits at
   the join until it completes [WAIT-3]. Since each statement takes effect
   between its beginning and its completion, statements before a spawn
   precede the started context's statements, and the started context's
   statements precede the starter's after the join.
4. **Real time, including order learned through the host.** SHARE-3's "one
   point after it begins and before it completes" places each point between
   two events of the execution. The design record behind the rule says what
   it means: "Rule 3's single point is linearizability. When client X
   receives the reply to `SET` and then tells client Y, and Y sends `GET`,
   Y's statement begins after X's completed, so it sees the write"
   ([io-model SHARED.md](../io-model/SHARED.md), section on the meaning). So a
   statement that completed before another began, as any host channel can
   show, precedes it.

Item 4 is intended but not written as a rule a conformance case can test: the
specification has no sentence saying that a statement completing before
another begins takes effect before it, and today no case could observe the
difference for statements on disjoint objects. A take makes it observable,
so the amendment that adds the take should also state it (section 5.2, rule
S6). Recorded in section 10.

### 3.3 What a fetch-and-add inside the hold is consistent with

Let every taking statement perform one sequentially consistent fetch-and-add
(FAA) on one shared word, after it has taken every lock it takes and before
it releases any. The runtime takes locks in one global order and releases all
of them when the block ends (two-phase locking;
[state-locks](../../../design/compiler/waiting-contexts/state-locks.md),
second and fourth decisions).

**Deduction.** Define each taking statement's point as its FAA's instant.

- *Valid point.* The FAA lies after the statement begins and before it
  completes, and inside the interval in which it holds every lock it takes.
  By the two-phase-locking argument, any instant in that interval is a valid
  serialization point for the statement against every statement it conflicts
  with.
- *Conflict order.* If A and B conflict and A precedes B, A releases the
  shared lock before B takes it, so FAA(A) < release(A) < take(B) < FAA(B).
- *Context and spawn/join order.* One context's statements execute one after
  another, so their FAAs do too; spawn and join order the contexts' events in
  time.
- *Real time.* FAAs on one word are linearizable: if A completes before B
  begins, FAA(A) precedes FAA(B) in the word's modification order.

The FAA order is therefore a total order of the taking statements that extends
all four relations of section 3.2, and it can serve as the restriction of
SHARE-3's one order to them. Statements that take no number fit around it as
the two-phase-locking serialization places them. The value each FAA returns
is the number of FAAs before it plus the first number, so the numbers are
distinct, dense, and increasing in effect order.

Two placement conditions are necessary, each shown by a counterexample.

- **After the last lock.** The runtime takes lock groups lazily, at the
  first use of a group's target, together with every earlier group in the
  type order (state-locks, second decision). Suppose A holds x (type X), does
  its FAA, then takes y (type Y, later in the order). B holds y, does its
  FAA, writes y and releases. A then takes y and reads B's write: A follows B
  in the conflict order, yet A's number is smaller. So the FAA must follow
  the statement's last acquisition: the first use of a number must take every
  group not yet taken.
- **One take per statement and sequence.** If a statement took twice, another
  statement's FAA could fall between its two: A gets 5 and 7, B gets 6. With
  A and B each at one point, no order makes A's numbers all smaller or all
  larger than B's. Two takes would expose an interleaving inside what SHARE-3
  makes one step, which is exactly the ground on which "atomic fields and
  lock-free cells" were rejected
  ([shared-objects](../../../design/language/waiting/shared-objects.md),
  rejected list). So a statement takes at most once from each sequence.

### 3.4 Is conflict-order consistency enough for Firn?

**For the final-table replay (N2): yes. Deduction.** Every statement that
writes key k's entry holds k's entry as a target, so these statements
pairwise conflict, and their numbers increase in their effect order. The
final value of k is the one the last of them wrote, which carries the largest
number among k's records; replay in number order applies it last. Keys no
statement writes are untouched by both. This needs only per-key conflict
order, and needs every writing statement to take a number and log, which is
the program's obligation.

**For Firn's prefix states (P1): no.** P1 requires every number-prefix of the
log to restore a state the server passed through. Firn's own example: client
C1 sets `a`; client C2 reads `a` and then, because of it, sets `b`. The two
writes conflict with nothing in common. Under an order consistent only with
conflicts, `b` may carry a smaller number than `a`, and a crash between them
restores `b` without `a`, a state that never existed. Two further relations
are needed:

- the chain through C2's read and C2's context order (`set a` conflicts with
  C2's read of `a`, which precedes C2's `set b` in C2's source order), which
  section 3.2 items 1 and 2 give; and
- order learned outside the program: a client that receives `OK` for
  `SET a` on one connection and then sends `SET b` on another expects any
  surviving prefix holding `b` to hold `a`. The two statements run in two
  contexts that share no state; only real-time order (section 3.2 item 4)
  orders them.

The FAA design gives all four relations, so it gives P1. A design that tracks
only causality inside the program (section 5.4) gives the first two and
loses the last.

**Consequence for Firn's acceptance test.** N2 alone cannot tell a global
order from a per-key one: numbering per key stripe, which Firn's own study
rejects for breaking P1, passes N2. The witness should add N1 and N3. N3 is
checkable in a correctness run (not a timing run): each context reads the
monotonic clock before and after each statement and logs `(number, before,
after)`; a violation is a pair with `after(A) < before(B)` and
`number(A) > number(B)`, found in one pass over the records sorted by number
keeping the least `after` seen among larger numbers. The check relies on the
host clock as a witness of real time; on the 14900K's Linux, CLOCK_MONOTONIC
is one clock for all CPUs (an assumption of the test, not a language
guarantee). A deliberately wrong arm that numbers per context must fail N1
and N3 in the same run, which shows the checker detects the fault it guards.

## 4. Constraints from the runtime and from earlier rulings

- **Lock order and lazy groups** (state-locks, second decision): see section
  3.3's first placement condition.
- **One driver is not one thread.** With one driver, a monitor can hand the
  driver's role to a spare thread while the displaced thread is still inside
  a statement ([bounded waits](../../../design/compiler/waiting-contexts/bounded-waits.md),
  third decision; [concurrent map](../../../design/compiler/waiting-contexts/concurrent-map.md),
  fifth decision and the rejected "user per driver"). Two threads can
  therefore execute statements at once even when the driver count [PRE-2]
  (`:2613-2614`) is one. A plain increment is safe only on a target whose
  runtime has one thread of execution and no spare threads, such as the
  single-driver embedded target PRE-2 names; the desktop runtime with
  `WF_DRIVERS=1` still needs an atomic RMW, uncontended. Firn's request
  ("on one driver it is a plain increment") holds only in the former sense.
- **Target capabilities** (owner's ruling 6 on relaxed fields, 2026-10-09,
  [relaxed-fields](../relaxed-fields/README.md#owner-rulings-2026-10-09)):
  explicit per-target capabilities, read-modify-write by capability and
  refused at composition when absent, never a hidden lock; bounded interrupt
  masking admissible on a qualified single-core runtime. The ruling governs
  scalar operations on relaxed cells. The same study records that "whether
  `atomic` statements can be offered on such a multicore port at all depends
  on its hold/lifetime and handoff implementation, which remains unverified"
  (section "Single-core interrupt exclusion versus a hidden lock").
- **The counter ruling** (counter study, sections 7 to 9): for counts,
  single-step integer cells were set aside because a contended hardware add
  reached 0.059 of private lines at 8 P-cores against the 0.5 threshold, and
  the sharded state was selected. The counter study itself notes that shards
  do not serve "an update that needs the global value back at once, such as
  allocating the next identifier; that remains a `Shared<T>` statement or
  (a)" (its section 5.4), and its comparison table credits cells with
  identifier allocation.
- **Prior rejections a candidate must answer** (shared-objects rejected
  list): atomic fields and lock-free cells (interleavings inside one step);
  transactions that read optimistically and retry (a block runs once);
  ordering transactions by stamps of host completions ("orders nothing a
  program can observe beyond the single point"); an order inferred from every
  atomic statement of the program (needs every function body).
- **Progress.** WAIT-2's guarantee (`:2264`): a begun statement whose guard is
  absent or eventually always true eventually takes effect. A take must add no
  wait that one driver would not show.

## 5. Candidates

### 5.1 (A) A single-step sequence cell, taken by a call inside the block

**Surface.** A prelude handle type `SharedSequence`, never an atomic target,
and `sequence_take(seq: &SharedSequence) -> Option<u64>`, admitted only in an
atomic statement's block (a non-waiting call, so SHARE-2's ban on waiting
calls does not exclude it).

**Meaning.** The take is part of the statement's effect: its number is the
count of statements that took from the sequence before it, plus the first
number, or `None` once that reaches 2^63.

**What the rules must add.** Section 3.3's second condition forces "at most
one take per statement and sequence", and a call can sit in a callee, a loop
or a branch. The checker would need a new call property, carried through
every callee's boundary like `waits`, saying that a function takes, plus a
count along every path; or the lowering would have to turn several takes
into one FAA of their number, which the block does not know when it uses
its first number. The handle is also no target, so the statement's header no
longer names everything it touches, against the shared-objects decision that
a statement names each object it holds.

**Lowering, safety, cost, embedded targets.** As (B), section 5.2.

**Effect on the earlier ruling.** It is a narrow single-step cell, the
counter study's candidate (a) cut down to one operation.

**Not recommended.** It buys nothing over (B) at the machine level and costs
a new call property in the checker. Candidate (V) of section 5.6 keeps its
appeal, an operation called in the block, without its cost: the operation
takes the statement's binding instead of a handle, so the header names the
value and the one-operation count stays inside the block.

### 5.2 (B) A take target: the statement's own ticket

**Surface.** A new state kind, as `ConcurrentHashMap` and the drafted
`Shards` are, held only as the state of a shared object, with one target form
that takes a number and no whole target:

```whitefoot
opaque nocopy struct Sequence {
}

opaque struct NextNumber {
}

fn shared_sequence_new(first: u64) -> result: Shared<Sequence> pure;
fn next_number() -> result: NextNumber pure;
```

Firn's G2 rewritten. The writer's cut takes a number itself instead of
peeking: every write it follows took a smaller number and every later write a
larger one, so no peek operation is needed.

```whitefoot
fn write(keys: &Shared<ConcurrentHashMap<u64>>, parts: &Shared<Shards<Part>>, seq: &Shared<Sequence>, k: &[u8]) -> r: unit reads(keys), reads(parts), reads(seq), reads(k) waits {
  let mine = any_shard();
  let ticket = next_number();
  atomic slot = &keys^[k], p = &parts^[mine], n = &seq^[ticket] {
    match n^ {
      Some(value: number) => {
        set slot^ = Some<u64>(value: number);
        frame_append(log: &p^.log, sequence: number, bytes: k);
      }
      None() => {
        set p^.refused = True();
      }
    }
  }
  return unit;
}

fn cut(parts: &Shared<Shards<Part>>, seq: &Shared<Sequence>) -> s: Option<u64> reads(parts), reads(seq) waits {
  let ticket = next_number();
  let s = None<u64>();
  atomic all = &parts^, n = &seq^[ticket] {
    set s = n^;
  }
  return s;
}
```

(Sketch, not compiled. `Shards<Part>` with an owned buffer needs Firn's G1;
the take does not depend on it.)

**Rules sketched** (no specification edit made; each would be stated once,
the others citing it):

- **S1, PRE-1.** The declarations above; `Sequence` and `NextNumber` join the
  prelude's opaque structs.
- **S2, TYPE-9.** A row: `Sequence` [SHARE-1] is only the state of a shared
  object, the type argument of `Shared<·>` or `SharedRead<·>`.
- **S3, SHARE-1.** A value of `Sequence` is a sequence: a first number and
  the statements that took from it. `shared_sequence_new` returns a handle to
  a new shared object whose state is a sequence with its argument as first
  number. `next_number` returns the take selector, the one value of
  `NextNumber`. No path reaches a sequence's state; a statement reaches it
  only through a take target.
- **S4, SHARE-2, target rows.** A handle place of type `Shared<Sequence>`
  followed by one index step whose atom is a place of type `NextNumber` is a
  take target; its binding is a read-only `&Option<u64>` whose path is the
  number the statement takes, a place no other statement reaches. A whole
  target whose handle place has type `Shared<Sequence>` or
  `SharedRead<Sequence>`, a take target through a `SharedRead<Sequence>`
  place, two take targets on one handle place, a write through a take
  binding, and a guard that names a take binding are each a hard error
  citing SHARE-2 at that target, `place` or guard, with a repair [DIAG-1].
  Two take targets on two handle places that may name one sequence follow
  SHARE-2's "possibly one root" comparison; S5 makes their bindings equal
  when they name one object.
- **S5, SHARE-3, the number.** A statement with a take target takes one
  number from each sequence its take targets name, as part of taking effect.
  Its binding holds `Some(value: f + k)` when `f + k` is below 2^63 and
  `None()` otherwise, where `f` is the sequence's first number and `k` is how
  many statements that took from that sequence took effect before it. The
  binding establishes at the block's entry that a `Some` value it holds is
  below 2^63, as a key-set binding establishes its length (so `number + 1`
  needs no further proof).
- **S6, SHARE-3, real time** (clarifying item 4 of section 3.2, needed once
  numbers make the order observable). A statement whose completion precedes
  another's beginning in the execution, through one context's order, a spawn
  or join, or a host outcome a context receives after another context's host
  operation produced it [WAIT-2, HOST-1], takes effect before it. The wording
  needs care so that it adds no host order HOST-1 denies; it is a sketch.
- **Unchanged.** WAIT-2 gains no input: the numbers are determined by the
  one order, already an input. HOST-1, PAR-1, PAR-2 and CAP-1 are unchanged:
  a take is part of an atomic statement, which already counts as a waiting
  call. A statement that leaves its block by an error edge still took effect
  [FN-1, ERR-3] and still took its number.

Consequences derived from S5 (not rules): numbers are distinct; they increase
in the one order; they are dense from `f`; exhaustion is permanent, since
`k` only grows; on every driver count the same sequence of statements
exhausts at the same take.

**Lowering** (implementation decisions for the design tree, proposed only):

- The sequence object holds one 64-bit word, initialized to `f`, on a cache
  line of its own.
- A take is one sequentially consistent `atomicrmw add` of 1, after the
  statement's last lock acquisition and before its first release. The first
  use of a take binding takes every lock group not yet taken; a statement
  whose path never reads the binding takes at the block's end, before
  releasing. A statement whose guard reads false takes nothing, since it
  releases and parks without taking effect.
- A result `v >= 2^63` yields `None()`, and the take then stores `2^63` back.
  **Deduction:** once any FAA returns at least 2^63 the word never again
  holds less than 2^63 (FAAs only raise it, and the store writes 2^63), and
  it exceeds 2^63 by at most the number of threads between their FAA and
  their store, at most twice the driver count (bounded waits, third
  decision), so it never wraps. No compare-and-swap loop is needed.
- A take statement through a value handle must not count its handle on the
  object header, or every take writes a second contended line (counter
  study, section 2.2, "The handle count", and its found-along-the-way item on
  skipping the handle count; the same obligation the drafted shard target
  carries).
- A target whose runtime has one thread of execution: a plain load, add and
  store, with the same exhaustion test.
- A multicore target without hardware RMW (the dual-core Cortex-M0+ RP2040
  has none, but has hardware spinlocks): the sequence is a lock unit after
  every other in the lock order, held only for the load, add and store, with
  the same lock primitive that target's hold implementation uses. **Argument
  that this is not the refused hidden lock:** ruling 6 refuses a lock hidden
  behind a scalar operation that the source presents as lock-free, because
  such a lock can wait for another context and, in an interrupt handler,
  deadlock on a preempted holder. A take is part of an atomic statement,
  which already waits, already takes locks in the global order and is never
  entered from an interrupt handler; the sequence's lock is taken last and
  holds no other lock while held, so it adds no cycle of waits and no wait
  one driver would not show. Wherever atomic statements exist on a target,
  takes exist, so takes narrow no target beyond atomic statements
  themselves. Whether the owner accepts this reading of ruling 6 is part of
  the decision (option A2 of the card refuses takes on such targets
  instead).
- AArch64 without LSE atomics implements the FAA as an exclusive-monitor
  loop; its progress is the same assumption the existing spin locks make
  there, so WAIT-2's guarantee gains no new premise.

**Safety.** No path reaches the sequence's state, so no data race at source
level; the binding is read-only and holds a value fixed at the statement's
point. No silent overflow: S5 yields `None()` at the bound and the counter
cannot wrap (lowering above). No unproved partial operation: the writer's
arithmetic on the number is ordinary OP-2 arithmetic, with the bound fact of
S5. No runtime trap or new writer-accessible escape. The take never waits, so
WAIT-2's progress guarantee is unchanged.

**Cost (estimate, unmeasured except where marked).**

- *One driver, desktop runtime:* one uncontended locked add on a line that
  stays in one core's cache, a few nanoseconds (estimate), against Firn's
  one-core SET at about 300 ns per command (3.33 million a second, measured
  by Firn's session): about 1% or less.
- *Many drivers:* one contended line transferred per take. **Measured**
  ceiling of one line (counter study, run
  [38304337014](https://github.com/Ming-Research/Whitefoot/actions/runs/38304337014),
  arm W5, 64 C threads saturating it): 142.54, 124.86, 118.06 and 98.88
  million adds a second at 2, 4 and 8 P-cores and 16 hyperthreads, against
  498.51, 997.04, 1993.71 and 1669.50 when each thread has its own line.
  Firn's goal of about 20 million writes a second at 8 P-cores would use 17%
  of the 8-P-core ceiling and 20% of the 16-hyperthread one. Below
  saturation each take costs about one cross-core line transfer, tens of
  nanoseconds (estimate), against a budget of about 400 ns per write per core
  at 20 million a second on 8 cores: an estimated loss of 10 to 25%, inside
  Firn's 75% bar but not by a margin an estimate can settle. Q1 (section 7)
  measures exactly this before anything is built.
- *Hold length:* the take sits inside the hold, so it lengthens every lock the
  statement holds by one transfer. Firn's parts are one driver's own and its
  benchmark keys are spread over 100 000, so the longer hold should matter
  only for hot keys (estimate).
- *Laziness:* the first use of a number takes every remaining lock group, so
  a target reached only on a conditional tail after that use is taken on
  every path. Firn's write takes its part on every path anyway.
- *Single-thread embedded target:* a load, add, compare and store.
- *Multicore without RMW:* one more short lock per take; no such runtime
  exists today, so unmeasured.

**Effect on the earlier ruling.** None for counts: cells stay set aside and
the sharded state stays the answer for statistics. This candidate is not a
cell: it has no load, store, add of other amounts or compare-and-swap, it
cannot be used outside an atomic statement, and its value has a meaning only
as the statement's place in the one order. It answers the case the counter
study set aside ("allocating the next identifier") and shows that the cells
as drafted there would not have answered it either: a `cell_add_wrap` called
before or after the write statement is its own step in the one order, so
writes to one key could carry numbers in the opposite order of their effects
(A takes 5, B takes 6, then B's statement and A's statement run in that
order). The owner's criterion for cells (a contended add at half the rate of
private lines) compared cells with per-core counting, which a sequence cannot
use; the criterion for a sequence is Firn's: the take keeps at least 75% of
the rate without it.

**Generality.** Logs, replication streams, commit numbers, unique
identifiers ordered by creation, and fair tickets. It does not serve counts at
scale, which the sharded state serves.

**Variant (B'), a number derived from the hold's own lock words.** Each lock
word already takes an atomic write at acquisition; a statement could combine
version counters of its targets' locks instead of using a separate word. Such
numbers are not global: two statements on disjoint objects get unrelated
versions. Making them comparable across objects is (D).

### 5.3 (C) A per-driver sequence merged by (epoch, driver)

**Surface.** None the writer sees beyond a take; the runtime numbers each
take `(epoch, driver, local)`, with `local` counting per driver and `epoch`
advanced by a global cut (Firn's writer every 10 ms), and the merge orders by
the triple.

**Meaning.** It cannot be stated in the specification: which driver runs a
context is not observable [WAIT-2] (`:2263`), so a number that encodes it
would have to become a new input of the execution, and then no rule ties it
to effect order.

**Holds the replay condition? No. Deduction, two counterexamples.**

- Same key, two drivers, one epoch. Driver 1 sets `x = 1` as `(5, 1, 7)`;
  later driver 0 sets `x = 2` as `(5, 0, 9)`. The merge puts driver 0's record
  first, replay ends with `x = 1`, the table holds `x = 2`. N2 fails.
- One context, two drivers. A driver with nothing to run takes about half of
  another's ready contexts ([waiting contexts](../../../design/compiler/waiting-contexts.md),
  first decision), so a context can take `(5, 1, 7)` on driver 1 and then
  `(5, 0, 3)` on driver 0: its own two writes are merged in reverse. N2 and
  N3 fail.

Repairing either requires advancing the epoch whenever a statement follows
one from another driver on a shared state, which needs a stamp on every state:
that is (D).

**Cost.** No shared line, so near the rate without numbers. **Rejected:** it
does not give the order the request requires.

### 5.4 (D) Lamport or hybrid logical stamps over the statement's targets

**Surface.** A take whose value is a stamp: each lock unit (object, map entry
or entry stripe) carries a stamp, each context carries a clock passed through
spawn and join, and a statement's number is one more than the largest of its
context's clock and its targets' stamps, written back to its targets and its
clock, ties broken by a context identity.

**Holds within the program, with one extra cost. Deduction.** Conflicting
statements see each other's stamps, and a context's clock carries its order,
so the stamp order extends section 3.2's items 1 to 3, if reads also write a
stamp: context C2 sets `y` with stamp 100, then reads `a` (stamp 7) under a
read hold; C1 then overwrites `a`. C1's stamp is 8 unless C2's read left 100
on `a`'s lock, and with 8 the log places C1's write of `a` before C2's write of
`y`, although C2's read saw the old `a` after setting `y`, so a prefix holding
the new `a` without `y` restores a state that never existed. So every read
hold must write its unit's stamp, the write-free read holds the counter study
considered for read-mostly state (its candidate (b)) become impossible, and
GET-like statements write a shared line on every call.

**Fails real time. Deduction.** Two contexts that share no state but are
ordered through the host (a client told by one connection's reply sends on
another) carry unrelated stamps. The log may order the later write first, so
P1 fails for Firn, and S6's linearizability, which the language intends for
every statement, does not hold for numbers. This is the prior rejection of
"ordering transactions by stamps of the host completions": stamps order what
the program saw, not the single point at which statements take effect.

**Hybrid logical clocks.** Taking `max(physical clock, stamps) + 1` respects
real time only if clock skew between cores has a bound and each statement
waits out that bound before releasing (commit wait). No target states such a
bound (x86's invariant time-stamp counter is synchronized in practice, not by
an architectural guarantee), and the wait costs every statement. The
acceptance of a program's log order would then rest on an unproved host
property.

**Cost.** Every statement of every program reads and writes stamps on its
lock lines (unless the compiler proves no take exists anywhere, which is
whole-program inference, rejected for lock orders on that ground); every read
writes. No hardware RMW beyond the locks, an advantage on targets without it.
Numbers are pairs or packed words, not dense.

**Rejected:** it breaks real-time order, the property Firn's prefix states
and SHARE-3's intended meaning both need, and charges every read a shared
write.

### 5.5 (E) No language change: what Firn could do

- **E1. An exclusive `Shared<u64>` target in every write.** Correct today.
  **Measured** bound: the shortest such statement, one `+1` on one object,
  runs 11.08, 4.48, 2.81 and 0.72 million a second at 2, 4 and 8 P-cores and
  16 hyperthreads, against 89.44, 171.82, 346.01 and 437.88 when each context
  has its own object (counter study arms W2 and W1, run 38304337014). It falls
  as cores are added, so Firn's writes cannot scale 1.6 times per doubling
  with it. Q2 (section 7) measures it in the witness's own shape.
- **E2. The compiler releasing the sequence's lock right after its use.**
  Two-phase locking allows releasing a lock taken last right after its last
  use; state-locks' fourth decision defers this "until a workload's holds
  show it to matter". It shortens the section to the increment, but E1's
  numbers are for a section that is already only the increment; the
  runtime's lock, not the section, sets the bound.
- **E3. The compiler lowering `Shared<u64>` statements that only add one to an
  FAA.** Valid only if every statement on that object, in the whole program,
  only adds one: a statement holding the object whole and reading it twice
  could see an FAA between its reads. That is whole-program inference of how
  an object is used, rejected for lock orders and for counters (counter study
  (d4)) on the ground that it needs every function body and a library change
  would silently change an object's cost.
- **E4. Host clock readings inside the statement as numbers.** Fast and
  without a shared line, but the specification orders clock readings of
  different contexts by nothing, readings can tie at 20 million writes a
  second, and the order would rest on the host's clock synchronization. It
  routes around the gap with an unstated host property; rejected.
- **E5. Blocks of numbers reserved per connection.** Breaks effect order: A
  reserves 100 to 115, B 116 to 131, B writes `x` with 116, then A writes `x`
  with 100.
- **E6. Key stripes, or one MULTI/EXEC per writer cut.** Firn's (e1) and (e2):
  break P1 or change the file's shape (Firn's study, section 3 (e)).
- **E7. AOF off.** Firn's step 1, its design (a), needs no number and is
  independent of this request.

Doing nothing therefore leaves Firn a choice between E1's bound and giving up
P1, which is the gap the request reports.

### 5.6 (V) A shared atomic value, held by an atomic statement without exclusion (recommended)

The owner asked for a general atomic value, usable in more than one place,
instead of a construct made for the take. This candidate gives one, and the
take becomes one ordinary use of it.

**The idea.** E3 above was sound only if every statement on a `Shared<u64>`
in the whole program performed one read-modify-write on it. A state type can
make that a property of the type instead of the program: a new state kind
whose every access, in every statement, is at most one operation. A
statement that performs one operation on a state and nothing else with it
has the same outcome whether it holds the state exclusively for its whole
block or performs that operation as one hardware step after its other locks
are taken. SHARE-3's last sentence (`:2342`) already permits exactly that
("hold less ... wherever the program's outcomes are those it has when every
statement's block executes with exclusive access"). So the meaning is the
ordinary atomic statement's, unchanged, and the lock-free lowering is an
instance of an existing permission.

**Surface** (sketch, not compiled; the name is a label to settle, see below):

```whitefoot
opaque nocopy struct Atomic<T> {
}

fn shared_atomic_new(value: u64) -> result: Shared<Atomic<u64>> pure;
fn atomic_load(cell: &Atomic<u64>) -> value: u64 reads(cell);
fn atomic_add(cell: &Atomic<u64>, value: u64) -> before: Option<u64> writes(cell);
fn atomic_raise(cell: &Atomic<u64>, value: u64) -> before: u64 writes(cell);
```

`shared_atomic_new` owes `value <= 2^63`, `atomic_add` owes `value <= 2^32`
and `atomic_raise` owes `value <= 2^63`, each as an [FN-8] requirement (a
constant or a bounded quantity discharges it; section "Overflow" says why the
bounds exist). Only `u64` is admitted in the first design. A whole target on
a `Shared<Atomic<u64>>` handle binds `&Atomic<u64>`, the state, as any whole
target does; a `SharedRead<Atomic<u64>>` target admits only `atomic_load`,
by the existing read-only authority rule of SHARE-2, since the other two
write their argument.

Uses, each one statement:

```whitefoot
// Firn's AOF write: one form for AOF on and off.
atomic slot = &keys^[k], p = &parts^[mine], s = &seq^ {
  set slot^ = Some<u64>(value: payload);
  if logging {
    match atomic_add(cell: s, value: 1_u64) {
      Some(value: number) => {
        frame_append(log: &p^.log, sequence: number, bytes: k);
      }
      None() => {
        set p^.refused = True();
      }
    }
  }
}

// Firn's memory peak after a command: exact, no per-connection cache needed.
atomic m = &peak^ {
  let before = atomic_raise(cell: m, value: used);
}

// A connection identifier, as Redis gives each client one.
let id = None<u64>();
atomic c = &next_client^ {
  set id = atomic_add(cell: c, value: 1_u64);
}
```

The standard library can also offer one-call forms, such as
`atomic_add_now(value: &Shared<Atomic<u64>>, amount: u64) -> Option<u64>
reads(value) waits`, written in Whitefoot as the one-target statement above;
they add no rule.

**Rules sketched** (no specification edit made):

- **V1, PRE-1.** The declarations above; `Atomic` joins the prelude's opaque
  structs.
- **V2, TYPE-9.** A row: `Atomic<u64>` [SHARE-1] is only the state of a shared
  object.
- **V3, SHARE-1, the value and its operations.** An atomic value holds a `u64`
  of at most 2^63, which `shared_atomic_new` sets to its argument.
  `atomic_load` returns it. `atomic_add` returns `Some(value: v)` and makes
  the value `v + a` when `v + a < 2^63`, where `v` is the value before and `a`
  the argument; otherwise it returns `None()` and makes the value 2^63, which
  is then permanent for adds. `atomic_raise` makes the value `imax(v, a)` and
  returns `v`.
- **V4, SHARE-2, one operation per statement.** A binding of type
  `&Atomic<u64>` is the `cell` argument of an atomic-value operation call in
  the block and appears nowhere else: not in the guard, not as another call's
  argument (`swap` included), not as an alias. On every path through the
  block at most one atomic-value operation call executes, counting the calls
  of every such binding of the statement together, and no such call is inside
  a loop. A violation is a hard error citing SHARE-2 at the offending call,
  `place` or guard, with a repair [DIAG-1].
- **SHARE-3 is unchanged.** The operation is a read or write of the target's
  state, so it takes effect at the statement's point, with exclusive access,
  in the one order, as every other read and write of a target does. S6 of
  section 5.2 (real time) is still needed, because numbers make the order
  observable.

Why V4 counts all bindings together (**deduction**): two operations in one
statement, on one value or on two, must take effect at one point. Statement A
adds to `x` then to `y`, statement B adds to `y` then to `x`. With one
hardware step each, A's step on `x` can precede B's and B's step on `y`
precede A's: A sees B's effect on `y` and B sees A's on `x`, a cycle no one
order explains. Making both atomic together needs a lock, which is what the
type is meant to avoid. So the rule is one operation per statement path, and
it also covers two targets that name one value, which no static rule can rule
out [SHARE-2].

**Semantics of the uses.** A statement whose only targets are atomic values
is a single step in the one order: it is linearizable and its result is an
owned snapshot, with no fact about the value afterwards. A statement that
also holds other targets performs the operation at its point, so the number
an `atomic_add` returns has every property of section 3: distinct, increasing
in the one order, consistent with conflict, context, spawn/join and real-time
order, and the replay and prefix properties of section 3.4. The take of (B)
is exactly `atomic_add(cell: s, value: 1_u64)` on a value only adds touch.

**Overflow.** No silent wrap. `atomic_add` saturates at 2^63 and reports it
as `None()`. The bounds let a fetch-and-add implement it without a
compare-and-swap loop (**deduction**): an add whose fetch returns
`v + a >= 2^63` reports `None()` and stores 2^63 back; while the word exceeds
2^63 every fetch returns at least 2^63, so every later add reports `None()`;
a load reports `min(word, 2^63)`; and the word exceeds 2^63 by at most one
argument of at most 2^32 for each thread between its fetch and its store, at
most twice the driver count of threads (bounded waits, third decision), so it
stays far below 2^64 and never wraps in hardware. `atomic_raise` cannot
overflow. Subtraction is left out: a decrement after saturation would leave
the saturated range, so gauges such as a client count stay in `Shared<T>` or
the sharded state until a program needs one here.

**Guards.** V4 keeps atomic values out of guards. A guard that read one would
need every write to wake its watchers. A write is a single hardware step with
no lock to find a watch under, so every write of every atomic value would
have to read a watcher flag after its step, with an ordering on both sides so
that a watch registered before the write is never missed, and take the
runtime's wake path when it finds one; the flag's check would be paid by
programs that never wait on a value. Not proposed until a program needs to
wait for a value, such as a ticket lock's "now serving".

**Effect rows and footprints.** Nothing new. The state is a path of no row,
reached only through a target, and the operations' rows `reads(cell)` and
`writes(cell)` act on the binding, which the statement's footprint removes,
as it removes every path into a target's state (SHARE-2). A function using
one declares `waits` and reads its handle place, as for every shared object.

**PAR-1 and PAR-2.** Unchanged: the operations exist only inside atomic
statements, which count as waiting calls and have no overlap permission. A
parallel compute loop cannot add to an atomic value; it reduces (PAR-2) or
its contexts use the sharded state.

**Progress.** `atomic_add` and `atomic_load` are single wait-free steps on
x86-64 (`lock xadd`, `mov`) and on AArch64 with LSE (`ldaddal`).
`atomic_raise` loads first and returns without writing when the value is not
below its argument, a single step; otherwise it is `ldumaxal` on AArch64 with
LSE and a compare-and-swap loop on x86-64, each retry following another raise
that succeeded, so it is lock-free; without LSE every operation is an
exclusive-monitor loop. These are the premises the existing spin locks already
rest on. A statement whose only targets are atomic values also performs the
existing local yield check before acquiring any target, using the ordinary
acquisition path's wait counter, threshold, driver-token read and yield point.
The state operation adds no wait; the statement retains WAIT-2's scheduling
progress guarantee.

**No cost to anything that does not use it.** A statement without an
atomic-value target is compiled exactly as today; no other statement, object
or program gains a word, a flag, a check or a branch. A statement that names
one but whose path performs no operation, such as Firn's write with AOF off,
performs no atomic-value state operation and does not touch that state word,
and keeps lazy lock taking on that path. The atomic-only form still executes
its local yield check. A statement whose only
targets are atomic values takes no state lock and performs the local yield
check before any target acquisition. Its state operation uses one atomic
instruction plus that local yield check; saturation and lifetime handling
retain the conditions in section 12. An unchanged handle place needs no
extra count update. On a path that performs the operation, the
first operation takes every lock group not yet taken (section 3.3), then
performs one atomic instruction.

**Firn's AOF on and off: one form.** The operation sits inside `if logging`,
so AOF-off writes execute no operation and pay nothing for the sequence,
while AOF-on writes take their number at the statement's point. The take
target (B) has the same property only if its rule is changed to take on the
paths that read the binding; as written in S5 it takes on every path, so with
(B) Firn would need two statement forms or pay the take with AOF off.

**Embedded targets.** On a target with one thread of execution, a plain load,
add, compare and store. On a multicore target without hardware
read-modify-write, the atomic value is an ordinary lock unit, taken in the
lock order with the statement's other targets and held for the one operation.
This is not the hidden lock refused by ruling 6 of 2026-10-09: that ruling
refuses a lock behind a scalar operation that the source presents as
lock-free. Here the source says what it means, an atomic statement that holds
its targets, and the lock is that statement's own visible hold; the hardware
step on capable targets is the permitted holding of less. The counter study's
standalone cells could not make this argument, since their operations were
not atomic statements.

**Relation to the sharded state and to the earlier ruling.** The earlier
ruling set single-step cells aside for counts on throughput alone: one
contended line reaches 0.059 of private lines at 8 P-cores (118.06 against
1993.71 million a second, run 38304337014). That stands, and hot always-write
counts go to the sharded state. Atomic values serve what shards cannot or
need not:

| Use | Why an atomic value | Shards? |
|---|---|---|
| Ordered numbers taken in a statement (Firn's AOF sequence, rewrite cut) | one global order, at the statement's point | cannot: no global order |
| Identifiers (client IDs, request IDs) | the global value back at once | cannot (counter study, section 5.4) |
| Maxima that rarely rise (Firn's memory peak) | a raise that finds nothing to change only loads a line every core can share (estimate) | also can, with a whole read to combine |
| Counts updated at a low rate (connections accepted, rewrites started) | one value, read at once, no combine | can, at more memory and a whole read |
| Hot always-write counts (commands, hits) | no: one line at 0.059 of private | yes |

**Name.** `Atomic` follows C11's `_Atomic` and Rust's `AtomicU64` for a
value read and written in single indivisible steps. The meanings differ in
two ways: those languages let one critical region perform any number of
operations and offer weaker orderings, while here every operation is
sequentially consistent and a statement performs at most one. The keyword
`atomic` also names the statement. AGENTS.md asks to compare before borrowing
a convention; the name is for the owner to settle, and `Counter` or `Cell`
are alternatives (`Cell` already means unsynchronized interior mutability in
Rust).

**Cost.** As (B) for the sequence: one contended line per take, 17% of the
measured 8-P-core ceiling at Firn's 20 million writes a second (section 5.2).
A raise that does not rise costs a load of a shared line (estimate: near an
uncontended statement; Q1 arm S4). An identifier allocation at connection
rate costs nothing measurable (estimate).

**Implementation cost.** Somewhat larger than (B): one state type and three
operations instead of one target row and a selector; V4's per-path count in
the checker (a count over the block's paths, like definite assignment, with
loops refused); lowering of a lock-free target and of one-target statements
with no lock; the saturation protocol; conformance cases for each V4
refusal, saturation at `2^63 - 1` and raise.

**Prior objections.** *Atomic fields and lock-free cells* exposed
interleavings inside one step; V4 permits one operation per statement, so no
interleaving inside a step exists. *Whole-program inference* (E3, counter
study (d4)): the type carries the property, so no function body elsewhere
changes a value's lowering. *The paused relaxed fields*: atomic values are
separate objects, never fields of another state, so no whole hold has to
exclude a lock-free field; the relaxed design stays paused. *Shared objects
reached only inside atomic statements*: kept.

**Variant (V'), an atomic value as a field of another state** (Firn's peak
inside Meta): rejected, because a whole hold on the containing state would
have to exclude the field's lock-free steps or let them fall between its
reads, the coordination problem the relaxed-fields study left unsolved (its
S2).

**Variant (V''), standalone operations with their own meaning** (the counter
study's candidate (a)): rejected as the base, because a value taken by a
standalone operation is its own step and is not ordered with the write it
labels (section 5.2, effect on the ruling), and its lock fallback on targets
without read-modify-write is the hidden lock ruling 6 refuses. Standalone
calls survive as library forms defined by the one-target statement.

## 6. Comparison

| | (V) atomic value target | (A) call in block | (B) take target | (C) per-driver | (D) stamps | (E1) exclusive object |
|---|---|---|---|---|---|---|
| Final-table replay (N2) | holds | holds | holds | fails | holds if reads write stamps | holds |
| Context and read chains (P1 inside the program) | holds | holds | holds | fails | holds if reads write stamps | holds |
| Real-time order (N3) | holds | holds | holds | fails | fails | holds |
| Distinct, dense (N1) | yes, among the adds performed | yes, one take per statement | yes | distinct, not dense | distinct pairs, not dense | yes |
| Exhaustion without wrap | saturates at 2^63, reported | yes | yes | not stated | stamps grow one per statement | writer's `+checked` |
| Serves other uses | identifiers, maxima, low-rate counts | identifiers only | no | no | no | everything, slowly |
| Firn's AOF on and off | one form; the off path performs nothing | one form | two forms, or the off path pays | one form | one form | one form |
| Cost to statements not using it | none | none | none | none | every statement stamps | none |
| New specification | type, three operations, one SHARE-2 rule | type, call, a "takes" call property | type, selector, target row, two SHARE-3 sentences | not statable | stamp semantics | none |
| Shared line per take, many drivers | one | one | one | none | targets' lines; reads write too | runtime lock and header |
| Measured or estimated rate at 8 P-cores | as (B) | as (B) | estimate: 75 to 90% of no number (Q1) | near no number | every statement slower; unestimated | measured 2.81 M/s ceiling for the shortest section |
| One-thread target | plain operation | plain increment | plain increment | plain | plain | plain |
| Multicore without RMW | the statement's own visible hold | as (B) | lock unit taken last, or refused (owner) | none needed | none needed | existing lock |
| Prior rejections | answered: one operation per statement, the type carries it | lock-free cells (unless one take) | answered: one step, header names it | driver unobservable | host-completion stamps | none |

## 7. Registered measurements (written before any measurement)

All on the i9-14900K self-hosted runner through CI, checked idle first, the
other sessions told before a long run. Shapes reuse the counter study's
harness (counter study section 8 and `research/experiments/shared-counters/`
on its branch, ported from Firn-wf's runtime-contention witnesses): a start
gate carrying the common deadline, batches of 1024 operations with one
context-local LCG step each and one clock reading per batch, totals added
once, one output line per run (`witness=... ops=... ns=... ops_per_s=...`),
5 s cells. CPU lists `0,2`, `0,2,4,6`, `0,2,4,...,14` (one hyperthread of 2,
4 and 8 P-cores) and `0-15` (8 P-cores, both hyperthreads), plus `0` for the
one-driver cells; `WF_DRIVERS` equal to the list's count. A pilot of one block
at the 8-P-core list first, timed; then six interleaved blocks, each running
every arm once per list in an order rotated by one position per block.
Report the median and range over blocks. Noise `e` is the largest
`|median(twin) / median(arm) - 1|` over the lists for the contended arm and
its twin; a ratio is decided only when it clears its threshold by more than
`e`. The counter study found its contended arm's twins 9% apart; if `e`
exceeds 0.05 at the 8-P-core list, one rerun with 10 s cells, never a wider
threshold.

### Q1. Does one shared FAA per operation keep 75% at Firn's size of work? (before choosing; no language change)

A C program shaped like the counter study's `w5.c`, one thread per CPU of the
list (a driver runs one statement at a time; the counter study's 64
time-sliced threads would distort per-operation latency). Each operation is
`w` nanoseconds of private work, a chain of dependent LCG steps on the
thread's own padded line folded into the check value so the optimizer keeps
it (the counter study's W0 lesson), calibrated in the pilot so that one thread
of S0 runs at `w` = 100, 200, 400 and 800 ns per operation; the calibrated
step counts are recorded.

| Arm | Per operation | Role |
|---|---|---|
| S0 | work only | the rate without a number |
| S0t | S0 built a second time | noise control for uncontended arms |
| S1 | work, then one sequentially consistent FAA on one shared 64-byte line | prices the take in (V), (B) and (A) |
| S1t | S1 built a second time | noise control for the contended arm |
| S2 | work, then one FAA on the thread's own line | the instruction's cost without sharing |
| S3 | take a test-and-set lock on the thread's own line, work, FAA on the shared line, release | the take inside an uncontended hold, as in Firn's part |
| S4 | work, then a load of one shared line that no thread writes after warm-up, and a compare | prices (V)'s raise of a maximum that has stopped rising |

- **Question.** At `w` = 400 ns, Firn's budget per write per core at 20
  million writes a second on 8 P-cores, is S1 / S0 at least 0.75 at the
  8-P-core list? Second, descriptive: is S4 / S0 at least 0.95 there?
- **The comparison answers either way:** the line's transfer latency at 8
  cores may be well under 100 ns (S1 / S0 near 0.9) or above it (below 0.75).
- **Prediction.** S1 / S0 >= 0.80 at `w` = 400 on 8 P-cores, and >= 0.75 at
  16 hyperthreads; S3 / S1 >= 0.95. Descriptive: the smallest `w` at which
  S1 / S0 reaches 0.75, and S2 / S0 at every `w`.
- **Rejection.** If S1 / S0 < 0.75 at `w` = 400 on 8 P-cores beyond `e`, no
  design with one shared line per take, (V), (A) or (B), can meet Firn's bar
  at Firn's size; the card returns to the owner with that result, and the
  remaining choices for the sequence are (D)'s costs and its loss of
  real-time order, or E1. (V) could still be wanted for identifiers and
  maxima; the card would then ask that separately.

### Q2. Does today's language already serve? (before choosing)

A Whitefoot witness in the same harness: 64 contexts; each operation is one
atomic statement on a map entry, key drawn from 100 000 by the local LCG,
and on the context's own `Shared<Part>`, where `Part` holds a fixed ring
`Array<u64, 4096>` of records and a position (a copy type, so no new
language form).

| Arm | Statement | Role |
|---|---|---|
| T0 | entry and own part; record `(key, 0)` | the rate without a number |
| T0t | T0 built a second time | noise control |
| T2 | T0 plus a whole target on one `Shared<u64>` sequence, `+checked 1`; record `(key, n)` | today's form (E1) |
| Tbad | T0 with numbers `index * 2^40 + local count` | correctness runs only: must fail N1 and N3 |

Correctness runs, separate from timing (1 s, 8-P-core list, each context also
recording clock readings before and after each statement and keeping every
record): N1, N2 and N3 of section 2 checked by the root after the joins. T2
must pass all three and Tbad must fail N1 and N3 in every run; otherwise the
checker is wrong and Q3 waits for it.

- **Question.** Is T2 / T0 at least 0.75 at 8 P-cores?
- **Prediction.** T2 / T0 <= 0.25 at 8 P-cores, T2 bounded near W2's 2.81
  million a second.
- **Rejection of the request.** If T2 / T0 >= 0.75 at 8 P-cores beyond `e`,
  today's form serves this shape and the language change is not needed; the
  card says so.
- Descriptive: T0's nanoseconds per operation at each list, which selects the
  `w` of Q1 to compare with in Q3.

### Q3. Acceptance after implementation (on a Whitefoot experiment release)

Arms T0, T0t, T2 and Tbad as in Q2, plus T1, the take as the implemented
candidate writes it (with (V), an atomic-value target and `atomic_add` of 1
in the block; with (B), the take target), and its twin T1t. With (V) also:

| Arm | Statement | Role |
|---|---|---|
| T1off | T1's statement with the operation inside a branch never taken | the "no cost when not used" claim: the AOF-off path |
| TP | T0 plus a one-target statement `atomic_raise` on one shared maximum after each operation, values from the local LCG below 2^20 so the maximum stops rising | a maximum through (V) |
| TC | a one-target statement `atomic_add` of 1 on one shared count per operation | descriptive: confirms hot counts still belong in the sharded state |

Lists as above, plus `0` (one driver).

- **Accept** if T1 / T0 >= 0.75 at 8 P-cores (Firn's criterion), T1 / T0 >=
  0.97 at one driver, and T1 passes N1, N2 and N3 in every correctness run
  while Tbad fails N1 and N3 in the same runs. With (V), also T1off / T0
  within `e` of 1 at every list, and TP / T0 >= 0.90 at 8 P-cores
  (prediction); TC / W1 is reported, expected near the counter study's 0.34
  for W5 / W1 at 8 P-cores.
- **Reject the implementation, not the design,** if T1 / T0 < 0.75 at 8
  P-cores while Q1's S1 / S0 at T0's measured work per operation is at least
  0.75: the runtime adds cost beyond one line (handle counting on the
  sequence's header, the forced taking of lock groups), to attribute with the
  emitted code before judging the design.
- **Reject the design** if both are below 0.75.
- **Stop** on any N1, N2 or N3 failure of T1: a soundness defect.
- Firn's own acceptance (its write-path study's step 2 comparison: per-driver
  parts with numbers against parts without, AOF on, at least 75%) stays
  Firn's and follows Q3.

The harness for Q1 and Q2 is `research/experiments/sequence-take/`, run by
the temporary workflow `tmp-sequence-take.yml`, which is removed after the run.

## 8. Recommendation

**Adopt (V), the shared atomic value held by an atomic statement without
exclusion, after Q1 and Q2 run and do not reject it.** Q1 and Q2 need no
language change and run in one temporary workflow on the 14900K.

Decisive reasons. The sequence needs one FAA after the statement's last lock
(section 3.3); every candidate that gives Firn's prefix states, real-time
order included, without an exclusive hold, does exactly that, so the choice
among (V), (A) and (B) is of surface and reach, not of cost. (V) gives that
FAA as an ordinary operation on an ordinary target, adds no rule to SHARE-3
(the lock-free lowering is the existing permission to hold less, licensed by
the one-operation rule V4), serves identifiers, maxima and low-rate counts
as well as sequences, lets Firn keep one statement form whose AOF-off path
performs nothing, and on multicore targets without hardware add is the
statement's own visible hold rather than a lock behind a scalar operation.
(B) is (V) restricted to adding one, with a selector the owner found
inelegant and a rule that takes on every path. (A) needs a new call property.
(C) does not give the order, (D) loses real-time order and makes every read
write, and E1 is bounded at 2.81 million a second at 8 P-cores.

Nothing changes for code that does not use it: statements without an
atomic-value target compile as today. Hot always-write counts stay with the
sharded state; the earlier ruling's measurement still decides them.

Prior objections answered: *lock-free cells* — one operation per statement,
inside its one step; *optimistic retry* — the operation happens once, on the
attempt that takes effect; *stamps of host completions* — the number is the
statement's place in the one order; *whole-program inference* — the type
carries the one-operation property.

**Confidence 3/5.** The semantic argument is a deduction from SHARE-3, WAIT-2
and two-phase locking (strong); the cost is an estimate between two measured
facts, a line's 118 million adds a second at 8 P-cores saturated and Firn's
400 ns budget. Could overturn it: Q1 below 0.75 at 400 ns (then no one-line
design meets Firn's bar for the sequence, though (V) may still serve
identifiers and maxima); Q2 at or above 0.75 (then no change is needed for the
sequence); a program that needs two operations in one statement (V4 refuses
it; that program needs an exclusive hold); the owner reading ruling 6 as
forbidding the lock form on multicore targets without hardware add (then
atomic values are refused at composition there).

## 9. Specification and design-tree amendment

The implementing branch follows (V). Section 12 records the selected spelling
and the current target and lifetime boundaries; the draft amendment uses
v0.151 because main has already advanced to v0.150. The scope is:

- Specification: V1 to V4 of section 5.6 and S6 of section 5.2 (real time);
  the measure table row (`AtomicValue`: absent); and
  conformance cases: each V4 refusal (two operations on one path, across one
  binding and across two; an operation in a loop; the binding in a guard, as
  another call's argument and in a `swap`); `atomic_add` through a
  `SharedRead` target refused; one context adding from a first value in
  order; saturation with a first value of `2^63 - 2` (`Some`, then `None`,
  then `None`, and a load of 2^63), plus `None` immediately from `2^63 - 1`; `atomic_raise` returning the value before;
  a program test with the section 2 witness checking N1 and N2.
- Design tree: a language decision under
  `design/language/waiting/shared-objects` (an atomic value held by a
  statement for one operation, instead of a take target, standalone cells, a
  call in the block, a per-driver number or stamps, with this record's
  reasons); compiler decisions under
  `design/compiler/waiting-contexts/state-locks` (the operation after the
  last acquisition, the first operation taking every group, no lock and no
  handle count for stable handle places, the saturation protocol, and the
  capability requirements for the currently absent degraded target backends).
- Library convenience: the one-call forms of section 5.6 can be written in
  Whitefoot and introduce no additional language rule.
- With (B) instead: S1 to S6 of section 5.2, with S5 changed to take only on
  the paths that read the binding if Firn is to keep one statement form.

## 10. Found along the way

- **SHARE-3's real-time meaning is not a written rule.** The intended
  linearizability (SHARED.md) follows from "one point after it begins and
  before it completes" only if beginnings and completions are events of one
  execution order; no sentence states that a statement completing before
  another begins precedes it. Unobservable today for statements on disjoint
  objects; observable once numbers exist. Disposition: S6, in the same
  amendment as the take.
- **"On one driver it is a plain increment" does not hold on the desktop
  runtime.** Spare-thread role handoff lets two threads run statements with
  one driver (section 4). Disposition: stated in (B)'s lowering; Firn's
  request should say "on a target with one thread of execution".
- **Firn's acceptance test cannot detect the property it most needs.** The
  final-table replay passes per-key numbering, which breaks P1. Disposition:
  N1 and N3 added to the witness, with a wrong arm that must fail them
  (sections 2, 3.4, 7).
- **The counter study's cells would not have served sequences.** Its section
  5.4 and comparison table credit candidate (a) with identifier allocation;
  for a number that must follow its statement's effect, a cell operation
  outside the statement is its own step and misorders writes to one key
  (section 5.2, effect on the ruling). Disposition: that study lives on
  another branch, which this study does not edit; its next revision should
  narrow the sentence to identifiers whose order does not matter.
- **Firn's memory peak could become one exact statement.** Firn now keeps
  each connection's last seen peak and takes an exclusive statement only
  above it; with (V) a one-target `atomic_raise` after each command keeps the
  exact peak without the cache, at the cost of a load of a shared line
  (estimate; Q3 arm TP). Disposition: offered to Firn once (V) exists; no
  change needed before.
- **Two operations in one statement are refused by (V).** A statement that
  wants both a sequence number and a byte count from two atomic values must
  use two statements, or an exclusive `Shared<T>` for one of them. No present
  witness needs it; reopen when a program shows a statement that must change
  two values at one point.

## 11. What is established and what is not

Established by deduction from the cited rules: the one order exists and is
already an input; an FAA after the last lock and before the first release is
a valid point of each statement and extends conflict, context, spawn/join and
real-time order; two takes in one statement or a take before a later lock
break the order; final-table replay needs only conflict order while P1 needs
real-time order; per-driver numbering fails the replay; stamps fail real-time
order and make reads write; the clamp keeps the counter from wrapping; a
statement performing one operation on an atomic value has the outcome of
holding it exclusively, and two operations in one statement, on one value or
two, cannot be single hardware steps.

Measured elsewhere and cited: one contended line's ceiling and private lines'
rate in C, and the shortest exclusive statement's rate, on the 14900K (counter
study run 38304337014).

Q1 and Q2 now have the owner-reported selection results summarized in Status.
Still unverified by this amendment: the implemented operation's correctness
and rate in Q3; the maximum's cost in TP; the cost on AArch64 and embedded
targets. Degraded forms govern future targets and are not implemented
capabilities of this base.
`AtomicValue` is the implementation's proposed name, compared in section 12.



## 12. Implementation surface and target boundaries

The implementing amendment uses `AtomicValue`, a non-generic opaque state
containing the one bounded `u64` described in section 5.6. `Shared<AtomicValue>`
and `SharedRead<AtomicValue>` retain the existing handle and authority forms.
The name denotes the checked statement-level invariant: at most one operation
on atomic values along a statement path, taking effect with its other writes.
It does not promise a particular machine instruction or lock-free execution
on every target. Generalization to other value domains requires a further
language decision rather than unused type arguments now.

### Naming comparison

| Candidate | Meaning and comparison | Disposition |
|---|---|---|
| `AtomicValue` | A shared value whose one operation joins the atomic statement's effect point; its permitted domain is specified separately. | Proposed name. |
| `Atomic<T>` / `AtomicU64` | C's `_Atomic` and Rust's atomic integer types permit independent loads, stores and read-modify-write operations, with explicit memory-order choices. Their unsigned addition wraps; their operations need not belong to one enclosing statement. | Do not borrow this broader API convention for the narrower Whitefoot invariant. |
| `Counter` / `Sequence` | Suggests a counting or numbering use, while the same state supports an exact maximum and observations. | Too narrow for the selected operations. |
| `Cell` | Describes a storage location without the statement-level invariant; Rust's `Cell` additionally suggests unsynchronized interior mutability. | Does not identify the checked property. |

The C comparison uses [N1570, sections 6.7.2.4 and
7.17](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf); the Rust
comparison uses the [atomic integer API](https://doc.rust-lang.org/stable/core/sync/atomic/type.AtomicU64.html)
and [atomic ordering documentation](https://doc.rust-lang.org/stable/core/sync/atomic/index.html).
These names are compared for semantics, not used to import another language's
memory model into Whitefoot.

### Integration boundaries found in the current base

The current main-line specification is already v0.150, so this amendment
archives its bytes and uses v0.151. Its SHARE-3 already orders completed
shared-effect calls before subsequently begun effects. The amendment makes
completion explicitly cover atomic statements as well as calls, without a
second order or additional input.

Section 5.6's saturation rule and the owner's instruction report `None` when
an add reaches 2^63. The earlier section 9 proposal of `Some, None, None` from
2^63 - 1 contradicted that rule. Section 9 now starts that sequence at
2^63 - 2, and the boundary tests also require immediate `None` from
2^63 - 1. This follows the owner's explicit boundary instruction rather than
changing the rule to fit the old example. The tests await CI evidence.

The current target inventory contains only x86-64 and AArch64 hosted targets.
There is no single-execution-thread target or multicore target without
64-bit atomic read-modify-write in that inventory, and no backend/runtime for
either degraded form. Those forms are requirements for future targets, not
missing implementations for any currently supported target. The target's thread model, not `WF_DRIVERS=1`, must
select a future ordinary-access form: the hosted runtime may run a displaced
executor and its replacement simultaneously. The existing bounds are 64
drivers and 128 executors. With each in-flight add at most 2^32, the maximum
transient excess from adds awaiting their saturation store is at most 2^39,
so the raw word cannot wrap. Loads and the returned old value of a raise must
both clamp this transient representation to 2^63.

The current shared-object lifetime contract permits an atomic block to move,
release or replace the handle it targeted. A statement whose handle place
stays live needs no extra retain; one that can lose the handle needs the
existing statement-owned retain until its exit. The no-extra-retain fast form
therefore requires a handle place proved to remain live. Disabling necessary retention would permit use after free. The
implementation preserves lifetime and identifies these cases separately from
the ordinary no-retain fast form; any requirement to eliminate their retain
needs an ownership-transfer design, not a silent relaxation of lifetime.

The sharded-state interface mentioned by the sketch is absent in this base.
The accepted current witness shape therefore uses a map entry, an ordinary
shared part and an atomic value, as the existing Q2 harness does. It does not
claim that the separate sharded-state work is implemented here.

Source inspection also found two integration limits to track separately.
The compiler's older shared/frozen constructors are absent from its explicit
allocation-fact seed; whether later paths independently prevent inappropriate
pure-call reuse has not been established here. The new constructor carries
its allocation fact without joining the heap-only list that forbids calls in
`no_heap` programs. Changing the older constructors requires a focused witness
and an audit of their consumers.

Compositions that use snapshot export already insert signal-mask preparation
and restoration at every atomic statement. The new target retains these
hooks, so in those compositions an atomic-only statement has that existing
cost as well as its one state operation. Q3's current graph does not use the
exporter. Eliminating those hooks for an atomic-only statement would need a
separate proof that its block cannot prepare an export through a forwarding
call; this amendment does not silently bypass the export protocol.


### Scheduling progress at the direct statement boundary

Source inspection found that the draft zero-lock lowering removed the
acquisition that called `wf_context_pass`. That call is the hosted runtime's
cooperative scheduling boundary: after 64 immediately answered waits it lets
another ready context run, and its driver-token read also hands back a
displaced invocation. Without it, a context polling an atomic value on one
driver can prevent its ready producer from ever raising it, violating WAIT-2.
The deadline monitor cannot repair a witness with no registered timer.

The selected correction, option A of the owner's non-blocking yield-check
card, is the existing local yield check before an atomic-only statement
acquires any target. It uses the same runtime function and therefore the
same counter, threshold, driver-token read and yield point as ordinary
acquisition. The cost commitment is one atomic instruction plus the local
yield check, with no state lock; no different scheduler or weaker WAIT-2
rule is introduced. Mixed statements retain their ordinary acquisition check.

The formal [atomic polling program](../../../tests/programs/atomic_value_progress.wf)
is now wired into the program tests on one and four drivers. The backend
regression requires the scheduling pass in its polling function, and the
lowering checks cover placement before acquiring any target. The first
[owner-run Linux gate](https://github.com/Ming-Research/Whitefoot/actions/runs/38462704179),
on revision `6e6624986`, passed the native polling witness on one and four
drivers. Unit compilation stopped before its tests could run; the corrected
unit cases and the repaired ordering program still require the next CI round.
No local execution is claimed.

### Temporary IR comparison budget treatment

The hosted old/new IR comparison uses `sequence-take/baseline-ir` and
`sequence-take/experiment-ir`. The existing budget helper returns without a
missing-row verdict when `WHITEFOOT_TIME_BUDGET_RECORD` is unset and always
preserves the command's exit code. This temporary dispatch-only workflow sets
no such record and invokes no budget verdict, so an unbudgeted label does not
fail either stage. Under the owner's instruction no provisional numbers are
invented. The temporary workflow is removed before the branch becomes ready;
its emitted-body comparison still fails for a differing body or a failed
compiler command.
