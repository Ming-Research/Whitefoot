<!-- Serves research/investigations/io-model/CONCURRENCY-MODEL.md: whether
     today's proof machinery discharges the per-block obligations of a
     monitor invariant. Removed or turned into conformance cases when an
     object invariant is specified and implemented. -->

# Monitor invariants with today's proof machinery

A monitor invariant I of a shared object asks each atomic block to prove one
thing: from I at its entry, plus its guard, I holds again at each of its
exits. No language form states I yet, so these probes stand in for the two
halves with forms that exist today:

- **Entry.** Either an `if` on I inside the block, whose then-arm receives I
  as a fact as the guard's arm does [ENT-3.S1], or a helper function that
  receives I as `requires`. Neither is how the feature would work; both give
  the checker I as a fact where the block begins.
- **Exit.** An `invariant` statement [INV-1] stating I over locals read back
  from the state, and over measures such as `q^.items.len`, which the checker
  must prove at that point.

`run.sh` checks each probe with the worktree's compiler and exits 1 when a
verdict differs from the table below. It is a manual research check, not a
gate.

```sh
sh research/experiments/monitor-invariants/run.sh
```

## Results

Checked at `f9ad34a17`'s compiler on Linux; the first twelve had the same
verdicts at `0a2d00283` and `2ff9181a4`:

| Probe | What it tests | Verdict |
|---|---|---|
| `queue-count` | `put` and `take` on a `Shared<Queue>` keep `count == items.len` | accepted |
| `queue-missed-update` | `put` forgets to raise `count` | INV-1, Refuted |
| `queue-split-transaction` | `put` reads `count` in one atomic statement and writes it in the next | INV-1, Unproved |
| `bank-values` | a transfer keeps `a + b == 100` over value parameters | accepted |
| `bank-values-equality-premise` | the same with `requires a + b == 100_u64` as one equality | OP-2, Unproved |
| `bank-field-premise` | the same with I over the fields of `&Accounts` | OP-2, Unproved |
| `bank-snapshot` | I over entry values tied to the fields by L0 equalities, with one bridging step | accepted |
| `bank-snapshot-without-bridge` | the same without the bridging step | INV-1, Unproved |
| `ghost-counters` | `items.len + consumed == produced` kept by `put` | accepted |
| `ghost-counters-unbounded` | the same with no bound on `produced` | OP-2, Unproved |
| `field-premise-direct` | `requires s^.a >= n` and then `s^.a - n` | OP-2, Unproved |
| `field-premise-copied` | the same with `s^.a` copied to a local first | accepted |
| `slot-cursor` | a cursor `next` into a `Slots` window kept `next < slots.len` by `claim`, which reads `slots[next]` and advances with wrap-around | accepted |
| `slot-cursor-weak-entry` | the same with only `next <= slots.len` at entry | OP-4, Unproved |
| `slot-cursor-missed-wrap` | `claim` forgets the wrap-around | INV-1, Unproved |
| `type-invariant-by-contract` | the cursor invariant carried through a reference by a `requires` and an `ensures` on the function that advances it, over two calls | accepted |
| `type-invariant-without-ensures` | the same without the `ensures` | FN-8, Unproved |

## What the results show

**A difference-bound invariant over fields and measures works end to end.**
`queue-count` proves `count == items.len` at the exit of both blocks. The
invariant also pays for the arithmetic: `count + 1` cannot overflow because
`count == len < cap`, and `count - 1` cannot underflow because
`count == len > 0`. The checker derives both automatically.

**An invariant can be what makes a block's own access safe.** `count` in
`queue-count` repeats `items.len`, so that invariant only tests the
machinery. `slot-cursor` keeps a cursor that nothing else bounds: the entry
fact `next < slots.len` is the only proof that `slots[next]` is in bounds
(`slot-cursor-weak-entry` is refused at the subscript without it), and the
exit obligation catches a cursor left one past the end
(`slot-cursor-missed-wrap`). Without an invariant, each block would have to
test the cursor at run time and choose what to do when it fails.

**A contract pair can carry an invariant across calls by hand.** An
`ensures` over a reference parameter's field and measure states the exit
state, so `type-invariant-by-contract` keeps `next < slots.len` through two
calls, and the second call's requirement is refused without it
(`type-invariant-without-ensures`). A type invariant would write that pair
once for every function of the declaring module (CONCURRENCY-MODEL.md 5.6).

**The two classic mistakes are refused, each for its own reason.**
- A block that forgets the update is refuted: the invariant is false where it
  is stated.
- A read-modify-write split over two atomic statements is unproved: the second
  block knows I, not that the count still equals the value the first block
  read. This is the lost update, refused before the program runs, because
  facts about the state end with each block (SHARE-2).

**An affine invariant (a sum of fields) needs two things the language does
not do today.**
- *A premise over the state's places.* Over value atoms, AUTO proves both the
  arithmetic and the invariant with no written steps (`bank-values`). Over the
  fields of a reference parameter, the same `requires` gives the body no usable
  affine premise (`bank-field-premise`). This is the gap `docs/todo.md`
  records for readonly-field terms: affine images of place terms need kills
  that follow their support.
- *An entry snapshot.* The same proof over the fields succeeds when I is
  stated over immutable entry values that L0 equalities tie to the fields
  (`bank-snapshot`). That is how the feature can be built: mint a value for
  each state place I names at the block's entry, and state I over those
  values.

  The exit proof then needs one named step, `a_now + b_now` against
  `a_next + b_next` (`bank-snapshot-without-bridge` shows it is needed). A
  checker that states I at the exit over the written values directly would
  not need it.

**Smaller findings.**
- An affine `requires` with `==` between two datums gives the body no affine
  premise, while the pair of bounds does (`bank-values-equality-premise`
  against `bank-values`). That is the specified rule, not a gap: only an
  ordering leaf supplies an affine image, and "Equality, disequality [...]
  supply no additional image" [ENT-3.S4]. A writer states an affine equality
  as two bounds. Main's `7868b607b`, which proves an equality requirement as
  its bound pair at a call, concerns the goal side and leaves this verdict
  unchanged.
- An L0 `requires s^.a >= n` does not discharge `s^.a - n` written on the
  place, but does once `s^.a` is copied to a local (`field-premise-direct`
  against `field-premise-copied`). This is an incompleteness of the domain
  query; `docs/todo.md` records it.
- A counter kept only for the proof grows without bound, so a `u64` field
  owes an overflow proof no program can give (`ghost-counters-unbounded`).
  Ghost state wants mathematical integers that are erased before lowering.

## Limits

- The probes check proofs. Every accepted probe also compiles and runs;
  only `queue-count`, `slot-cursor` and `type-invariant-by-contract` observe
  anything, exiting with 7 (the value put through the queue and taken back),
  31 (three claims over two slots, 10 + 11 + 10) and 20 (two claims over one
  slot).
- Only a single object's invariant is tested. Invariants over the elements of
  a storage (for example "every value in the keyspace is at most 512 MiB") are
  outside the fact language by an existing decision
  (`design/language/checks-and-proofs`: no quantified storage-element facts),
  and so outside what a monitor invariant can state.
- The entry stand-ins cost a runtime comparison (`if`) or a helper call; the
  feature would add neither.
