# Taking a global sequence number inside an atomic statement without holding it exclusively

## Status

A design study written before any implementation or measurement. Nothing in
it was built or run. It reads the active specification `spec/kernel-spec.md`
(title v0.135) and the design tree at Whitefoot `c52e867ca` (main); line
numbers cite that revision. Each claim is marked **deduction** (argued from
cited rules), **measured** (with its run), **source-read** or **estimate**
(unmeasured judgment).

It answers Firn's request on the status board, "an atomic statement can take
a globally increasing number without holding the number exclusively". The
request reopens part of the owner's ruling on shared counters (2026-10-12,
option A, then the measurement that selected the sharded state; study and
draft in [Whitefoot#385](https://github.com/Ming-Research/Whitefoot/pull/385),
`research/investigations/shared-counters/DESIGN.md` on that branch, cited
below as "the counter study"). The owner decides on a card; this record gives
it its options and evidence, and registers the measurements the choice needs
(section 7).

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
surface `n = &seq^[ticket]` is candidate (B) of section 5.2, used here only to
state the need):

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
a new call property in the checker.

### 5.2 (B) A take target: the statement's own ticket (recommended)

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

## 6. Comparison

| | (A) call in block | (B) take target | (C) per-driver | (D) stamps | (E1) exclusive object |
|---|---|---|---|---|---|
| Final-table replay (N2) | holds | holds | fails | holds if reads write stamps | holds |
| Context and read chains (P1 inside the program) | holds | holds | fails | holds if reads write stamps | holds |
| Real-time order (N3) | holds | holds | fails | fails | holds |
| Distinct, dense (N1) | yes, one take per statement | yes | distinct, not dense | distinct pairs, not dense | yes |
| Exhaustion without wrap | yes | yes | not stated | stamps grow one per statement | writer's `+checked` |
| New specification | type, call, a "takes" call property | type, selector, target row, two SHARE-3 sentences | not statable | stamp semantics | none |
| Shared line per take, many drivers | one | one | none | targets' lines; reads write too | runtime lock and header |
| Measured or estimated rate at 8 P-cores | as (B) | estimate: 75 to 90% of no number (Q1) | near no number | every statement slower; unestimated | measured 2.81 M/s ceiling for the shortest section |
| One-thread target | plain increment | plain increment | plain | plain | plain |
| Multicore without RMW | as (B) | lock unit taken last, or refused (owner) | none needed | none needed | existing lock |
| Prior rejections | lock-free cells (unless one take) | answered: one step, header names it | driver unobservable | host-completion stamps | none |

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
| S1 | work, then one sequentially consistent FAA on one shared 64-byte line | prices (B) and (A) |
| S1t | S1 built a second time | noise control for the contended arm |
| S2 | work, then one FAA on the thread's own line | the instruction's cost without sharing |
| S3 | take a test-and-set lock on the thread's own line, work, FAA on the shared line, release | the take inside an uncontended hold, as in Firn's part |

- **Question.** At `w` = 400 ns, Firn's budget per write per core at 20
  million writes a second on 8 P-cores, is S1 / S0 at least 0.75 at the
  8-P-core list?
- **The comparison answers either way:** the line's transfer latency at 8
  cores may be well under 100 ns (S1 / S0 near 0.9) or above it (below 0.75).
- **Prediction.** S1 / S0 >= 0.80 at `w` = 400 on 8 P-cores, and >= 0.75 at
  16 hyperthreads; S3 / S1 >= 0.95. Descriptive: the smallest `w` at which
  S1 / S0 reaches 0.75, and S2 / S0 at every `w`.
- **Rejection.** If S1 / S0 < 0.75 at `w` = 400 on 8 P-cores beyond `e`, no
  design with one shared line per take, (A) or (B), can meet Firn's bar at
  Firn's size; the card returns to the owner with that result, and the
  remaining choices are (D)'s costs and its loss of real-time order, or E1.

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

Arms T0, T0t, T2 and Tbad as in Q2, plus T1, the take target of (B), and its
twin T1t. Lists as above, plus `0` (one driver).

- **Accept** if T1 / T0 >= 0.75 at 8 P-cores (Firn's criterion), T1 / T0 >=
  0.97 at one driver, and T1 passes N1, N2 and N3 in every correctness run
  while Tbad fails N1 and N3 in the same runs.
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

## 8. Recommendation

**Adopt (B), the take target, after Q1 and Q2 run and do not reject it.**
Q1 and Q2 need no language change and run in one temporary workflow on the
14900K.

Decisive reasons. (B) is the only candidate that gives the order Firn's
prefix states need, including real-time order (section 3.4), without an
exclusive hold: one FAA after the statement's last lock is a valid point of
the statement inside SHARE-3's existing one order (section 3.3), so the
number needs no new order, no new input and no new memory model. A target
form keeps "one take per statement" a property of the statement's header,
which a call inside the block (A) could only get from a new call property.
(C) does not give the order at all, and (D) loses real-time order and makes
every read write. On a one-thread target the take is a plain increment, and
on a multicore target without hardware add it rides on the locks atomic
statements already need, so it narrows no target beyond atomic statements
themselves.

Prior objections answered: *lock-free cells* — no cell, one take per
statement inside its one step; *optimistic retry* — a take happens once, on
the attempt that takes effect; *stamps of host completions* — the number is
the statement's place in the one order; *whole-program inference* — the
writer chooses the type.

**Confidence 3/5.** The semantic argument is a deduction from SHARE-3, WAIT-2
and two-phase locking (strong); the cost is an estimate between two measured
facts, a line's 118 million adds a second at 8 P-cores saturated and Firn's
400 ns budget. Could overturn it: Q1 below 0.75 at 400 ns (then no one-line
design meets Firn's bar); Q2 at or above 0.75 (then no change is needed); the
owner reading ruling 6 as forbidding the lock fallback on multicore targets
without hardware add (then takes are refused at composition there, option
A2).

## 9. Specification and design-tree changes that would follow

Not made; listed for the implementing branch after the owner's ruling.

- Specification: S1 to S6 of section 5.2, the PRE-1 declarations, a TYPE-9
  row, the measure table row (`Sequence`: absent, as other opaque states), and
  conformance cases: each S4 refusal; one context taking from `first` in
  order; exhaustion with `first = 2^63 - 1` (`Some`, then `None`, then
  `None`); a program test with the section 2 witness checking N1 and N2.
- Design tree: a language decision under
  `design/language/waiting/shared-objects` (a take target, instead of a call
  in the block, a per-driver number or stamps, with this record's reasons);
  compiler decisions under `design/compiler/waiting-contexts/state-locks`
  (the FAA after the last acquisition, the first use taking every group, the
  clamp, the lock unit taken last on targets without RMW, no handle count on
  take statements).

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

## 11. What is established and what is not

Established by deduction from the cited rules: the one order exists and is
already an input; an FAA after the last lock and before the first release is
a valid point of each statement and extends conflict, context, spawn/join and
real-time order; two takes in one statement or a take before a later lock
break the order; final-table replay needs only conflict order while P1 needs
real-time order; per-driver numbering fails the replay; stamps fail real-time
order and make reads write; the clamp keeps the counter from wrapping.

Measured elsewhere and cited: one contended line's ceiling and private lines'
rate in C, and the shortest exclusive statement's rate, on the 14900K (counter
study run 38304337014).

Not established: the cost of one take below saturation at Firn's size of
work (Q1); today's form in the witness's shape (Q2); the implemented take's
rate and correctness (Q3); the cost on AArch64 and on any embedded target;
whether the owner accepts the lock fallback on multicore targets without
hardware add.
