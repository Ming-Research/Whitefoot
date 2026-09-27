# Prove it or write a branch

Three languages, one line:

```text
C          b[i]          i out of range: undefined behavior
Rust       b[i]          i out of range: a panic, after a check at run time
Whitefoot  deref(b)[i]   i not proved in range: the program does not compile
```

Whitefoot has no check at run time for an index, and no undefined behavior
either. Before it accepts a program, the compiler has to prove that every
index is in range, that every `+` fits its type, that no division is by
zero, and that every narrowing conversion keeps its value. When it cannot
prove one of these, it rejects the program and says what is missing. You
then have two ways forward: supply the fact it needs, or write the branch
that makes the fact true. There is no third way; the compiler never adds a
check of its own.

This article shows both ways, first for an index and then for arithmetic,
and ends with what the accepted code compiles to.

## 1. Every partial operation has a goal

An operation that is not defined for every input carries a goal, a condition
that must hold wherever the operation runs:

| Operation | Goal |
|---|---|
| `deref(b)[i]` | `i < deref(b).len` |
| `a + b`, `a - b`, `a * b` | `a +defined b`, and so on: the exact result fits the type |
| `n / d`, `n % d` | `n /defined d`: `d` is not zero, and for a signed type the pair is not `MIN, -1` |
| `cvt::<u64, u8>(x)` | `cvt.defined::<u64, u8>(x)`: the value fits the destination |

Here is a function whose goal the compiler cannot prove:

```
fn get(b: &[u8], i: u64) -> value: u8 reads(b) {
  return deref(b)[i];
}
```

```text
get.wf:5:18: error[OP-4]: UndischargedBoundsObligation
  source:   return deref(b)[i];
  marker:                  ^^^
  residual: i < deref(b).len
  disposition: Unproved
  mechanical_fix: add `requires i < deref(b).len;` to the `contract` of `get`, which each caller then establishes; or guard the access with `if i < deref(b).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare
```

The `residual` is the goal that is still open. `Unproved` means the compiler
found no fact that settles it either way; a goal it can show false is
`Refuted` instead. The `mechanical_fix` names the two routes this article is
about.

## 2. Write the branch

If an out-of-range index is something the program should handle, the
handling is ordinary code:

```
fn get(b: &[u8], i: u64) -> value: Option<u8> reads(b) {
  if i < deref(b).len {
    return Some<u8>(value: deref(b)[i]);
  }
  return None<u8>();
}
```

This is accepted. The condition of an `if` is a fact inside its then-block,
so `i < deref(b).len` holds at the subscript, and the goal is proved. The
comparison is still executed, but it is one you wrote, in the place you
chose, with the failure case as a value the caller handles. That is the only
kind of check a Whitefoot program has.

## 3. Prove it: move the goal to the caller

Often the caller already knows the index is in range, and a branch inside
`get` would test something that is always true. Then the goal belongs in the
function's contract:

```
fn get(b: &[u8], i: u64) -> value: u8 reads(b) contract {
  requires i < deref(b).len;
} {
  return deref(b)[i];
}
```

Inside `get`, the requirement is a fact from the first line on. In exchange,
every call has to prove it. A caller that cannot is rejected in turn:

```
fn first(b: &[u8]) -> value: u8 reads(b) {
  return get(b: b, i: 0_u64);
}
```

```text
requires.wf:11:10: error[FN-8]: UndischargedCallRequirement
  source:   return get(b: b, i: 0_u64);
  marker:          ^^^^^^^^^^^^^^^^^^^
  concrete_callee: get
  requires_clause: requires.wf:5:3 "requires i < deref(b).len;"
  instantiated_goal: 0_u64 < deref(b).len
  disposition: Unproved
  mechanical_fix: add `requires 0_u64 < deref(b).len;` to the `contract` of `first`, which each caller then establishes; or guard the call with `if 0_u64 < deref(b).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare
```

The requirement is now the caller's goal, with the caller's argument in
place of `i`. The same two routes apply one level up. A caller that knows the
buffer is not empty writes the branch there; a caller that walks the buffer
needs neither, because a counted loop proves its own index:

```
fn first(b: &[u8]) -> value: Option<u8> reads(b) {
  if 0_u64 < deref(b).len {
    let x = get(b: b, i: 0_u64);
    return Some<u8>(value: x);
  }
  return None<u8>();
}

fn checksum(b: &[u8]) -> sum: u8 reads(b) {
  let sum = 0_u8;
  for (i in 0_u64..deref(b).len) {
    let x = get(b: b, i: i);
    set sum = sum +wrap x;
  }
  return sum;
}
```

Both are accepted. In `checksum`, the loop's range makes `i < deref(b).len` a
fact in the body. An index computed some other way, such as the `kept` of the
[README](../../README.md)'s first example, is proved with a loop invariant;
[Proofs without a solver, by hand](proofs-by-hand.md) shows how.

A requirement is proved once per call site, at compile time. Nothing is
checked when `get` is entered.

## 4. The same choice for arithmetic

A bare `+` on `u64` has the goal that the sum fits:

```
fn total(a: u64, b: u64) -> sum: u64 pure {
  return a + b;
}
```

```text
add.wf:5:10: error[OP-2]: UndischargedIntegerDomainObligation
  source:   return a + b;
  marker:          ^^^^^
  residual: a +defined b
  disposition: Unproved
  mechanical_fix: add `requires a +defined b;` to the `contract` of `total`, which each caller then establishes; or guard the operation with `if a +defined b` where skipping it is the intended behavior; or write the `+wrap`, `+checked` or `+sat` form
```

`a +defined b` is a Bool: it computes whether the exact sum fits, without
computing the sum. The rejection offers the same two routes as before, and a
third: say what the program means when the sum does not fit. Each of these
is accepted:

```
fn total_guarded(a: u64, b: u64) -> sum: Option<u64> pure {
  if a +defined b {
    let s = a + b;
    return Some<u64>(value: s);
  }
  return None<u64>();
}

fn total_checked(a: u64, b: u64) -> sum: Option<u64> pure {
  match a +checked b {
    Ok(value: s) => {
      return Some<u64>(value: s);
    }
    Err(error: e) => {
      return None<u64>();
    }
  }
}

fn total_bounded(a: u64, b: u64) -> sum: u64 pure contract {
  requires a <= 1000000_u64;
  requires b <= 1000000_u64;
} {
  return a + b;
}
```

`+checked` is the branch written for you: it returns `Ok` with the sum or
`Err` with an overflow, and the `match` has to handle both. In
`total_bounded`, the two requirements prove that the sum fits. `+wrap` and
`+sat` are not proofs but different operations: they wrap around or clamp,
and are the right choice when that is the arithmetic the program wants, as
in the checksum above.

Division and narrowing conversions work the same way. `total / count` is
accepted under `if count != 0_u64`, and `cvt::<u64, u8>(x)` is rejected with
the routes `requires cvt.defined::<u64, u8>(x);`, a guard, or
`cvt.checked::<u64, u8>` with its `Err` handled.

## 5. What the accepted code compiles to

This is the LLVM IR the compiler emits for the `get` with a requirement,
before optimization:

```text
define i8 @wf_get(ptr noalias nonnull nocapture %wf.arg.v0.data, i64 %wf.arg.v0.len, i64 %v1) #0 {
entry:
  %v0.data = insertvalue { ptr, i64 } poison, ptr %wf.arg.v0.data, 0
  %v0 = insertvalue { ptr, i64 } %v0.data, i64 %wf.arg.v0.len, 1
  %t0 = extractvalue { ptr, i64 } %v0, 0
  %t1 = getelementptr inbounds i8, ptr %t0, i64 %v1
  %v2 = load i8, ptr %t1
  ret i8 %v2
}
```

An address and a load: no comparison and no branch. The proved `+` in
`total_bounded` is a single instruction:

```text
define i64 @wf_total_bounded(i64 %v0, i64 %v1) #0 {
entry:
  %v2 = add nuw i64 %v0, %v1
  ret i64 %v2
}
```

`nuw` is LLVM's promise that the unsigned addition does not wrap. The
compiler can make it because it has proved it, and the optimizer may use it.
The `+wrap` in `checksum` compiles to a plain `add` without the flag, because
wrapping is its meaning.

A branch the program writes stays a branch: `first` keeps the comparison it
contains. The difference from a language that checks at run time is where
the checks come from. In the loop of the README's first
example, each of the seven safe Rust spellings we measured keeps a check that
the Whitefoot version does not have
([measurements](../../research/experiments/bounds-check-spellings/README.md#results)).

## 6. What it costs

- You write facts that other languages do not ask for: a requirement, a
  loop invariant, sometimes a branch you know is always taken.
- A goal the fixed procedure cannot prove has to be proved in steps you
  write, or turned into a branch. There is no `unsafe` to opt out with.
- In return, every check in the program is one you can see in the source,
  and every rejection names the goal that is missing and the routes that
  would close it.

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) |
|---|---|
| A subscript's goal | OP-4 |
| Integer goals and the `defined`, `wrap`, `checked` and `sat` forms | OP-2 |
| A conversion's goal and `cvt.checked` | OP-6 |
| Branch conditions, requirements and loop ranges as facts | ENT-3 |
| A requirement proved at each call | FN-8 |
| A goal proved from the facts, or rejected | ENT-6 |
| One rule, one location, and the repair routes | DIAG-1 |

Every function in this article is accepted, or rejected as shown, by the
compiler at commit `e041772d8`. `whitefootc --check file.wf` checks a file
without building it, and `whitefootc --emit-llvm file.wf -o file.ll` writes
the IR shown above.
