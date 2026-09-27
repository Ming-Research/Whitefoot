# Proofs without a solver, by hand

The first example in the [README](../../README.md) removes a bounds check
with one line:

```
invariant behind: kept <= i
```

This article follows the compiler from that line to its conclusion that the
store `buf[kept]` is always in range, in steps you can do with a pencil. No
SMT solver is involved. The procedure is short enough for the specification
to state in full, it has no timeout, and every machine reaches the same
answer. The price is that it proves less on its own than a solver would; the
last sections show where it stops and what you write then.

Here is the loop again. It keeps the non-space bytes of a buffer, in place:

```
fn squeeze(buf: &[u8]) -> kept: u64 writes(buf) {
  let kept = 0_u64;
  for (
    i in 0_u64..deref(buf).len,
    invariant behind: kept <= i
  ) {
    let byte = deref(buf)[i];
    if byte != 32_u8 {
      set deref(buf)[kept] = byte;
      set kept = kept + 1_u64;
    }
  }
  return kept;
}
```

The store `set deref(buf)[kept] = byte` needs `kept < deref(buf).len`. Below,
`deref(buf).len` is shortened to `len`.

## 1. Every fact is a difference

The compiler keeps what it knows about integers in one shape:

```text
x - y <= c
```

Here x and y are values in the program and c is a whole number. A bound
against a constant uses a value called Z, which is always zero. Facts
translate like this:

| Written | As differences |
|---|---|
| `i < n` | `i - n <= -1` |
| `kept <= i` | `kept - i <= 0` |
| `kept` is a `u64`, so `0 <= kept` | `Z - kept <= 0` |
| `i <= 100` | `i - Z <= 100` |
| `a == b` | `a - b <= 0` and `b - a <= 0` |

A strict comparison becomes `<= -1` because the values are integers: `i < n`
says the same as `i <= n - 1`.

At the store, the compiler knows these facts:

- `i - len <= -1`, because the loop body runs only while `i < len`;
- `kept - i <= 0`, the invariant; section 5 shows why it can be trusted;
- `Z - i <= 0` and `Z - kept <= 0`, because both values are unsigned.

## 2. Draw the facts as arrows

Draw each value as a point and each fact `x - y <= c` as an arrow from x to y
with the label c. Read the arrow as "x is at most y plus c". The facts at the
store are four arrows:

```text
kept --0--> i --(-1)--> len
Z --0--> kept
Z --0--> i
```

Two arrows in a row add up. Adding `x - y <= a` and `y - z <= b` gives
`x - z <= a + b`, which is an arrow from x to z with the label a + b. When
two arrows connect the same two points in the same direction, the smaller
label wins, because it says more.

## 3. Closure: add arrows until nothing changes

Keep adding arrows until no new arrow appears and no label gets smaller. The
result is the closure: the label from x to y is then the lightest path from x
to y. With four values the closure fits in a table. Row x, column y holds the
c of `x - y <= c`; an empty cell means nothing is known.

Before:

| x \ y | Z | kept | i | len |
|---|---|---|---|---|
| **Z** | | 0 | 0 | |
| **kept** | | | 0 | |
| **i** | | | | -1 |
| **len** | | | | |

After:

| x \ y | Z | kept | i | len |
|---|---|---|---|---|
| **Z** | | 0 | 0 | -1 |
| **kept** | | | 0 | -1 |
| **i** | | | | -1 |
| **len** | | | | |

The closure found two new facts. `Z - len <= -1` says the buffer is not empty
inside the loop; that is true, and nobody asked for it. `kept - len <= -1` is
the fact the store needs.

The compiler also knows each value's type range, such as
`len - Z <= 18446744073709551615` for a `u64`. Those arrows add nothing to
this question, so the tables leave them out.

## 4. A goal is a path

`kept < len` is `kept - len <= -1`. It holds when some path from `kept` to
`len` has labels that add up to -1 or less:

```text
kept --0--> i --(-1)--> len        total: -1
```

That path exists, so the check is gone and the store compiles to a plain
store. The same path pays for the addition too. Follow it one arrow further,
to the largest `u64`, and `kept` is at most 18446744073709551614, so
`kept + 1` fits and needs no check either.

The path also shows why the invariant has to be exactly this strong. Write
`kept <= i + 1` instead, and the first arrow becomes `kept --1--> i`. The path
now totals 0, which proves only `kept <= len`, and the compiler rejects the
store with the fact that is missing:

```text
error[OP-4]: UndischargedBoundsObligation
  source:       set deref(buf)[kept] = byte;
  marker:                     ^^^^^^
  residual: kept < deref(buf).len
```

## 5. Where the invariant comes from: induction

The compiler does not take `kept <= i` on trust. It proves two things, and
only then uses the invariant inside the loop.

**Before the first iteration.** `kept` is 0 and `i` starts at 0, so
`kept - i <= 0` holds.

**From one iteration to the next.** Assume `kept - i <= 0` at the top of an
iteration. The body has two paths.

- The byte is a space. `kept` does not change, so `kept - i <= 0` still
  holds.
- The byte is kept, and `kept` becomes `kept + 1`. The new value is one more
  than the old one, an arrow `new --1--> old`, and the old one was at most
  `i`, an arrow `old --0--> i`. Together they give `new - i <= 1`. After the
  write the compiler forgets every fact about the old value and keeps what it
  derived about the new one.

After the `if` the two paths meet. The compiler keeps a fact there only if it
holds on both paths, and then with the larger of the two labels:
`kept - i <= 1`. You can ask the compiler to confirm this. After the `if`,
`invariant joined: kept <= i + 1_u64;` is accepted, and
`invariant joined: kept <= i;` is rejected, because it is false on one of the
two paths.

At the end of the iteration `i` becomes `i + 1`, so the next iteration needs
`kept - (i + 1) <= 0`, which is `kept - i <= 1`. That is the fact that
survived where the paths met. So the invariant holds at the top of every
iteration. Two small checks stand in for an argument about all iterations.

## 6. A negative cycle means the code cannot run

Arrows can form a cycle. If the labels around a cycle add up to less than
zero, following it from x back to x gives `x - x <= -1`, which says
`0 <= -1`. The facts cannot all be true at once, so the program never reaches
that point.

```
fn pick(buf: &[u8], i: u64) -> r: u8 reads(buf) {
  let n = deref(buf).len;
  if i < n {
    if n <= i {
      return deref(buf)[n];
    }
    return deref(buf)[i];
  }
  return 0_u8;
}
```

Inside the inner `if`, the facts include `i - n <= -1` and `n - i <= 0`. The
cycle `i --(-1)--> n --0--> i` totals -1. The compiler accepts
`deref(buf)[n]`, a read one past the end, because it has proved that the line
never runs. Change the inner test to `n <= i + 1` (the language wants the sum
in its own `let`) and the cycle totals 0. The line now runs when `n` equals
`i + 1`, and the compiler rejects the read with the residual
`n < deref(buf).len`.

## 7. Where the procedure stops

A difference relates two values. Many useful facts relate more, such as

```text
first + second + third <= first_limit + second_limit + third_limit
```

given `first <= first_limit`, `second <= second_limit` and
`third <= third_limit`. For sums like this the compiler has a second step with
a fixed menu. To prove a target it tries each of these, and nothing else:

1. the target directly: it is one known difference, or it follows when each
   value is replaced by its known bound;
2. the target minus one known difference or one earlier `invariant`, with the
   rest checked directly;
3. the target minus two earlier `invariant`s added together, with the rest
   checked directly.

(A few facts the compiler records the same way, such as a requirement that
relates more than two values, count as earlier invariants in items 2 and 3.)

With two parts the menu is enough. `first + second <= first_limit +
second_limit` is accepted with no help: take away `first <= first_limit`
(item 2), and what remains, `second <= second_limit`, is a known difference.

With three parts and only the three requirements, it is not. After one
difference is taken away, `second + third <= second_limit + third_limit`
remains. That is neither one known difference nor a consequence of the
values' bounds, and there is no earlier invariant to take away, so the
compiler rejects the target. You then name the facts to add:

```
invariant component_sum: first + second + third <= first_limit + second_limit + third_limit {
  use (first <= first_limit);
  use (second <= second_limit);
  use (third <= third_limit);
}
```

The compiler adds the three facts, subtracts the sum from the target, and
checks that what remains follows directly; here nothing remains. It checks
the steps you wrote and does not look for others. Writing the proof in stages
works as well. Each invariant is proved by the same menu and then counts as
an earlier invariant, so `invariant pair: first + second <= first_limit +
second_limit;` followed by the three-part target is accepted without a `use`
line.

`use` lines the compiler did not need are an error. The same block over the
two-part sum is rejected, because the menu proves that target on its own, and
the rejection says to remove the block. The proofs that appear in source are
therefore exactly the ones the compiler could not find.

A `use` line can also scale a fact, as in `use 3 times pair;`, and the
compiler divides by a factor that every coefficient shares. The specification
lists every step; there are no others.

## 8. Why not a solver

Verifiers such as Dafny, Verus and F\* send their proof goals to an SMT
solver, which decides far more than this procedure does. The cost is that its
answer can depend on things that are not in the program. Zhou et al.
(*Mariposa: Measuring SMT Instability in Automated Program Verification*,
FMCAD 2023) took 17,043 queries from six verification projects and found the
newest solver version they tested unstable on 2.6% of them, and on up to 5.0%
in a single project. A change that means nothing, such as renaming a
variable, turned a passing proof into a failure or a timeout.

Whitefoot gives up that power in exchange for predictability:

- The procedure is fixed by the specification
  ([ENT-1](../../spec/kernel-spec.md)). It has no timeout and no work budget,
  and two conforming compilers reach the same verdict on every machine.
- Each step is one you can do by hand, as above, so you can tell before
  compiling whether a proof will go through.
- When a proof does not go through, the rejection names the missing fact,
  which is the next thing to write.

The price is real. You write loop invariants, and now and then `use` steps,
that a solver would have found for you.

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) |
|---|---|
| Facts as differences, closure, contradiction | ENT-4 |
| Forgetting facts after a write; what survives where paths meet | ENT-5 |
| An invariant before the first iteration and from one iteration to the next | INV-1 |
| The fixed menu for sums | ENT-6 (`DIRECT`, `AUTO`) |
| `use` steps | PRF-1 |
| No timeout, the same verdict everywhere | ENT-1 |

Every function in this article is accepted, or rejected as shown, by the
compiler at commit `da368080f`. `whitefootc --check file.wf` checks a file
without building it.
