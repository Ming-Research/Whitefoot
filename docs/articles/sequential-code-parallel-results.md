# Write sequential code, get parallel results

Here is a quicksort, written the way a textbook writes it:

```
fn quicksort(v: &[u64]) -> result: unit writes(v) {
  let n = deref(v).len;
  if n <= 1_u64 {
    return unit;
  }
  let p = partition(v: v);
  let after = p + 1_u64;
  let smaller = &deref(v)[0_u64..p];
  let larger = &deref(v)[after..n];
  quicksort(v: smaller);
  quicksort(v: larger);
  return unit;
}
```

It has no thread, no task, no `spawn`, no lock and no annotation asking for
parallelism. Compiled with `--par`, the two recursive calls run at the same
time on different workers. A run that sorts 2 million numbers takes 0.18 s
built without `--par` and 0.07 s with it on four workers, and the sorted
result is the same array the sequential program produces
([measurement record](../../research/experiments/par-quicksort/README.md)).

This article shows how the compiler gets there, step by step: how it knows
which memory each statement touches, how it decides that two statements
touch different memory, and what it emits once it has decided.

## 1. Every function says what it touches

A Whitefoot signature lists what the function reads and writes through its
reference parameters. `quicksort` and `partition` both say `writes(v)`; the
checker that confirms the list is sorted says `reads(v)`; a function that
touches nothing through references says `pure`.

The list is checked against the body, and it has to be exact. A body that
writes through a reference its row does not list is rejected:

```text
error[SET-1]: InvalidSetTarget
  source:     set deref(v)[0_u64] = 0_u64;
  root_class: a reference whose declared row does not write this path
```

So is a row that lists something the body never touches:

```text
error[EFF-2]: EffectMismatch
  expected_row: reads(v)
  found_row: reads(v), reads(w)
```

Because the rows are checked, the compiler can trust them at every call. It
never has to look inside a callee to know what a call touches; the callee's
signature says so, and the callee's own check guarantees the signature is
true.

## 2. From a call to the memory it touches

At the call `quicksort(v: smaller)`, the callee's row `writes(v)` names its
parameter. Replace the parameter with the argument, and the call writes
whatever `smaller` refers to, the range `deref(v)[0_u64..p]`, that is, the
elements at positions `0` up to but not including `p`. In the same way,
`quicksort(v: larger)` writes the positions from `after` up to `n`.

Every statement gets such a footprint: the places it writes and the places it
reads, including the places its argument expressions read. A `let` writes
the name it defines.

## 3. Two ranges that do not overlap

Two ranges are disjoint when one ends before the other starts. Here the
first range ends at `p` and the second starts at `after`, so the question is
whether `p <= after`.

The compiler answers it with the same fact procedure it uses for bounds
checks. `let after = p + 1_u64;` records that `after` is exactly one more
than `p`, the fact `p - after <= -1`, and that already says more than
`p <= after`. The [proofs-by-hand article](proofs-by-hand.md) works through
that procedure; nothing new is needed here.

The same judgment decides whether two arguments of one call may overlap, so
a program never gets two different answers to the question "do these two
places overlap?"

## 4. Two statements may overlap

The rule for parallel statements, [PAR-1](../../spec/kernel-spec.md), says
two adjacent statements may run with overlapping execution exactly when
neither writes anything the other reads or writes. Reading the same memory
from both sides is fine.

`--par-ledger` prints every decision the compiler makes, with its reason.
Among the decisions it prints for the quicksort program in the measurement
record, whose `main` fills an array, sorts it and checks it:

| Where | Statements | Decision | Reason |
|---|---|---|---|
| `quicksort` | `let p = partition(v: v);` then `let after = p + 1_u64;` | denied | the first writes `p`, which the second reads |
| `quicksort` | `quicksort(v: smaller);` then `quicksort(v: larger);` | permitted | the two ranges are disjoint |
| `quicksort` | `quicksort(v: larger);` then `return unit;` | denied | the return may skip what follows it |
| `partition` | its loop | denied | `store` carries a value from one iteration to the next |
| `fill` | its loop | denied | the random-number state is read seven times per iteration, and a reduction reads its accumulator once |
| `sorted` | its loop | denied | a `return` leaves the loop |
| `main` | `fill(v: all);` then `quicksort(v: all);` | denied | both write the whole array |
| `main` | `quicksort(v: all);` then `let ok = sorted(v: all);` | denied | the first writes what the second reads |

The denials matter as much as the permission. Each one names the conflict,
so a writer who wants more parallelism knows what to change: for example,
`fill` would parallelize if each element's value were computed from its
index instead of from the previous element's state.

## 5. Loops

Loops follow the same idea, with a rule of their own,
[PAR-2](../../spec/kernel-spec.md). Iterations may overlap when each
iteration writes its own elements, or when the only value carried from one
iteration to the next is combined with one associative operation, such as
`+wrap`, `imin` or `ior`:

```
fn total(src: &[u64]) -> sum: u64 reads(src) {
  let sum = 0_u64;
  for (i in 0_u64..deref(src).len) {
    let x = deref(src)[i];
    set sum = sum +wrap x;
  }
  return sum;
}
```

```text
PAR loop        total.wf:3  loop  permitted   eligible; one accumulator under +wrap
```

Workers sum their own chunks, and the partial sums are combined; because
`+wrap` is associative and commutative, the total is the one the sequential
loop computes. Change the body to
`let doubled = sum *wrap 2_u64; set sum = doubled +wrap x;` and the loop is
denied: that recurrence cannot be split into independent chunks.

## 6. From permission to parallel code

For a recursive function whose calls may overlap, the compiler emits two
versions of it.

- The **parallel version** takes a budget. If the budget is used up, it
  calls the sequential version and returns. Otherwise it runs `partition`,
  forms the two ranges, offers the first recursive call to the other workers,
  runs the second call itself with the budget reduced by one, and then waits
  for the first. If no worker has taken the first call by then, it runs it
  itself.
- The **sequential version** is the plain program, with no parallel code in
  it at all.

The first call into the recursion gets a budget of `floor(log2(64 ×
workers))` levels, at most 24, which is 8 on four workers. The top levels of
the recursion, where the ranges are large, fan out; everything below runs the
sequential version at full sequential speed. With one worker, the `--par`
build takes the same 0.18 s as the sequential build.

Workers that run out of work take offered calls from other workers. A
worker waiting for a call it offered works on other offered calls in the
meantime, and sleeps only when there is no work anywhere.

## 7. Why the result is the same

The two calls write disjoint memory, and neither reads what the other
writes. Whatever order their instructions interleave in, every element ends
up with the value the sequential program gives it. PAR-1 states this as a
guarantee about the language: under a permitted overlap, every binding and
every piece of state equals the source-order result, and the number of
workers, the schedule and whether any overlap happened at all cannot be
observed. An implementation that never overlaps anything is a correct
implementation, and no program depends on the overlap happening.

## 8. How this compares

Most languages leave both halves of this work to the programmer: you write
the fork and the join yourself (Cilk's `spawn` and `sync`, or a join call in
a library), and you make sure the two halves do not overlap.

Parallelizing compilers, such as GCC's `-ftree-parallelize-loops` and LLVM's
Polly, handle loops whose array subscripts are simple linear expressions of
the loop counter. Recursion is outside their model.

Automatic parallelization of recursive divide-and-conquer code has been done
before: Rugina and Rinard (*Automatic Parallelization of Divide and Conquer
Algorithms*, PPoPP 1999) parallelized C programs such as quicksort with a
whole-program pointer and symbolic bounds analysis. What Whitefoot changes is
where the knowledge comes from. The compiler reads each callee's declared
and checked row instead of analyzing the whole program, so the decision is
local: changing a function's body cannot change a permission elsewhere
unless it changes the function's signature or contract. And each decision is
printed with its reason.

## 9. Limits

- `--par` has to be turned on. Without it, the program runs sequentially.
- `fill`, `sorted` and each `partition` run on one worker, and the first
  call partitions all 2 million numbers before any parallel work starts, so
  the whole run is 2.6 times faster on four workers, not 4 times.
- Permission is not speed. Whether an overlap pays off depends on how much
  work each side does; the budget keeps small calls sequential. The numbers
  above come from one host; the record lists its limitations.
- There are no explicit threads, locks or tasks in the language.
  High-concurrency I/O for servers is being designed separately.

| Step | Rule in the [specification](../../spec/kernel-spec.md) or design |
|---|---|
| Rows state reads and writes, checked against the body | EFF-1, EFF-2 |
| A call's footprint from the callee's row | EFF-5 |
| Disjoint places and ranges | OWN-7 |
| Adjacent statements may overlap | PAR-1 |
| Loop iterations may overlap | PAR-2 |
| The two versions and the recursion budget | [`design/compiler/parallel-lowering/two-worlds.md`](../../design/compiler/parallel-lowering/two-worlds.md) |

Every program, diagnostic and ledger line in this article comes from the
compiler at commit `3cd7e8139`; the times come from the measurement record,
which states the compiler it used.
