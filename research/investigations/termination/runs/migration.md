# Migrating the corpus to the loop-progress rule

The [TERM-1] implementation rejected loops across the WF corpus: the standard
library, `tests/programs`, the conformance cases and the WF sources embedded in
compiler unit tests. This record lists how each rejected loop was changed,
what the changes show about the rule, and which checker changes the migration
prompted. It is evidence for the rule's authoring cost, not a usage
distribution: the corpus was written to exercise the compiler.

Counts come from `git diff "$(git merge-base main HEAD)"` over `tests`, `lib`,
`compiler/src` and `compiler/tests`, matching a removed line against the added
line it became. They exclude the new TERM-1 conformance cases, which are new
files, and the spec, repair text and parser fixture lines that name
`decreases`.

## Summary

| Change | Loops | Where |
|---|---|---|
| `==` exit rewritten to `>=` (one to `<=`) | 38 | 10 program loops, 13 DEFLATE loops, 13 compiler unit-test loops, 2 conformance cases |
| written `decreases` rank | 25 | programs 17, conformance 8 |
| independent fuel rank (`decreases fuel`, `-wrap` decrement) | 34 | compiler unit tests 31, conformance 3 |
| counted bound replacing an unbounded loop | 9 | BFS frontier, DEFLATE blocks and symbols (4), redis expiry batches, a shared counter, polling, list removal |
| restructured (hoisted bound, inlined step, local copy, do-while) | 9 | five octet loops, merge sort, DEFLATE bit reservoir, redis digit count, byte_string |

The fuel ranks are loops that never terminate by design: compiler tests whose
body breaks on an unchanging `Bool` parameter to observe a loop-head or join
fact. Each fuel rank adds an exit and a wrapping decrement that creates no
obligation, so the facts the test observes do not change.

## Derivation

- Most rejected loops had an exit test the rule already reads but in a form it
  does not derive: an equality (below), a bound computed in the body with a
  wrapping add (`let limit = at +wrap 8_u64;`, hoisted above the loop), or an
  operand formed by an exact add (`let left = at * 2 + 1;`), which prompted
  admitting exact sums, differences and literal multiples as operands.
- A merge loop, a nested `break` and a nested counted walk each test an
  unchanging guard before the real bound. Reading every leading exit test,
  with any one falling rank sufficient, accepts them unchanged.
- Loops whose exit test calls a length accessor (`let length =
  priority_queue_len(...)`) use a written rank over the public readonly field
  the accessor's postcondition names (`decreases queue.storage.inner.len`);
  the pop's postcondition proves it.
- `x / 10 < x` from `x >= 1` is not derived: it needs an integer rounding step
  (`x - x/10 >= 0.9x`) the affine layer does not take. Digit loops in the
  do-while shape know the new value is at least one, which suffices; one
  `while`-shaped digit count was rewritten to that shape.
- A `+wrap` step is exact only where L0 bounds the operand; a bound known only
  as a header invariant does not make it exact, so two loops use the exact
  `+` instead.

## Equality exits

`if i == n { break; }` with `i` rising by one is the most common rejected
shape. The continuing `i != n` does not bound `i`; a cursor that starts past
`n` wraps forever, and the fixed families do not combine a disequality with
an order. An earlier prototype that activated an order beside the
disequality could not prove the order at the backedge and was reverted. Every
equality exit was rewritten to `>=`, which says the same thing for a rising
cursor and is what the rule derives.

## Termination defects the rule found

- `byte_string.wf` `bs_find`: with an empty needle and a haystack of length
  `u64::MAX`, `last = hay -wrap needle` is `u64::MAX` and the scan cursor
  `start +wrap 1` wraps before exceeding it. An empty needle now returns
  position zero first, so `start <= last < u64::MAX`.
- `op4-pos-descriptor-index-images`' chunk loop and `hash_map_rebuild` relied
  on facts no check carried (a positive chunk size, a counting argument); the
  former has the requirement, the latter was rewritten (below).
- `raw_deflate_dynamic.wf`'s table decode stopped at `bit_length ==
  max_bits`; with a zero `max_bits` it would have walked until the length
  wrapped. It now stops at `>=`.

## Owned structure

Walks over owned lists and trees through a reference cursor (`sum`,
`raise_values`, `edge` in `owned_link_cursors.wf`, the REF-1 conformance
case) have no integer rank. The structural form accepts them: every backedge
moves the cursor into a `Box` below its referent and the body adds no `Box`.
A loop that inserts a node ahead of the cursor and then moves into it is
refused (`term1-neg-owned-descent-grows`).

`remove_even` removes a node by replacing the cursor's referent with its
tail, leaving the cursor in place. No form measures a shrinking referent, so
the walk is counted by the list's length; this is a gap, recorded in
`docs/todo.md`.

## Waits

The first implementation counted a call of any function that declares
`waits` as a wait, as [WAIT-1] classified calls. Review found that a source
function may declare `waits` and return at once, so `loop { pause(); }` was
accepted as waiting, because [WAIT-1] was checked in one direction only. A
second attempt counted only direct calls of host functions; the owner
refused it (Q34), since a loop that waits through a helper is the common
shape. The waiting kind now splits into `may_wait` and `must_wait`, checked
both ways at the definition, and a loop's wait is a call of a `must_wait`
function, a guarded atomic statement or a `let` spawn's join.

The corpus migration replaced every `waits` with `may_wait` and then applied
the compiler's `must_wait` repair until none remained. Afterwards `tests/`
writes `must_wait` 194 times and `may_wait` 142 times (`grep -row` over its
`.wf` and `.wfm` files, the new conformance cases included). Five sources
declared a waiting kind over a body that never waits, each a helper in a
test of spawns, frames or atomic statements; each now executes a real wait.
The two `@commands` loops of `redis_subset.wf` process a buffer rather than
wait, and write `decreases held - consumed`.

## Joins

A bisection whose arms narrow the interval from different sides lost both its
rank and its header bounds at the join: each arm's facts are over different
images of `lower` and `upper`. Carrying the relations the loop owes through
the join, when each arm proves them, accepts the textbook form; `merge_sort`
had moved the step into a helper whose postcondition could not state the
width's decrease (one datum per contract side) and now writes it inline. The
same rule accepts the formerly negative `inv1-neg-sequential-guarded-steps`,
now `inv1-pos-sequential-guarded-steps`. The [join
investigation](../../branch-join-relations/DESIGN.md) had predicted that
case's flip for any rule keeping such relations.

## Exchange

`swap` published nothing about its targets, so a window replaced by an
exchange could never be shown empty and released. The rebuild's swapped-out
table needed its length, so `swap` now moves each target's measure images and
closed bounds to the other.

## Hash map rebuild

`hash_map_rebuild` retried a one-slot pending window until a scan found a free
bucket. By pigeonhole the first scan always does, but no fact carries that,
so the retry loop has no progress. Every restructuring leaves an unreachable
path holding a generic owner that can be neither dropped nor left in a window
that must be released empty. The rebuild now moves each owner straight into
the first available bucket; on the unreachable path it puts the owner back
into the old table, parks the old table's remaining owners in a new `stale`
window of the map and reports failure. The allocation observer counts seven
more allocations for the seven maps of `hash-map-program.wf`.

## State advanced through calls

The DEFLATE decode loops advance the output position inside `emit_byte` and
`copy_distance`. A rank cannot name a struct field reached through a
reference, and a routed postcondition can name only its payload, so neither
the callee's advance nor the loop's descent can be stated. The symbol loops
are counted by `out^.len +sat 1_u64`, which bounds the symbols a block can
hold, with an `OutputFull` after the count. This meets the reopening
condition of the refused "field atoms in loop invariants".
