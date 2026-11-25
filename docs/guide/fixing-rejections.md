# Fixing rejections

Whitefoot accepts a program only after it has proved the facts that make each
partial operation safe. A rejection means the compiler could not establish one
of them. This guide groups the rejections by what they protect and gives, for
each group, the one change that usually repairs it, with a program the
compiler rejects and the same program after the change.

Three documents share the work of explaining a rejection:

- This guide is for a person reading a rejection. It says what each group of
  rules protects and which change repairs the group.
- The writing guide of
  [Whitefoot-kit](https://github.com/Ming-Research/Whitefoot-kit/blob/main/docs/writing.md)
  is for the agent that writes programs. It says how to write forms the
  compiler accepts.
- The `mechanical_fix` line in the compiler's message is for the one construct
  that was rejected. It names the instance, such as the goal that is still
  open or the name that is missing, and the compiler owns its words.

The guide shows the compiler's message only inside the output blocks below,
which the repository's gate compares byte for byte with what the compiler
prints. The prose does not repeat the message's words, so when the compiler
rewords a repair the gate turns those blocks red and they are updated
together. The numbered rules are in the
[specification](../../spec/kernel-spec.md); the first line of a message names
the rule, the file, the line and the column. [Getting started](getting-started.md)
reads one message field by field.

Each group below lists the rules it covers, so a rule number in a message leads
to exactly one section. A section's example cites one rule of its group; the
other rules of the group are repaired by the same kind of change. Where a rule
has a row in the [safety claims](../safety-claims.md), the section links it.
Every repair here is one change among those a message may offer; the message
for your program may offer others.

## Text and layout

Rules: FORM-1, FORM-2, FORM-3, FORM-4, FORM-5, FORM-7, GIVE-1, GRAM-1,
GRAM-2, GRAM-3, GRAM-4, GRAM-5, GRAM-6, GRAM-8, GRAM-9, GRAM-10, GRAM-11.

These rejections come before any proof. FORM-1 gives every construct exactly
one spelling and one layout, and the grammar is deterministic, so the checker
and the reader derive the same program from the same bytes. A rejection here
says where the text leaves that shape.

The change: write the one form the grammar accepts at the marked position.

The example is FORM-3. `copy` is a fixed word of the grammar, so a name
cannot be spelled that way; rename it at its declaration and at every use.

```whitefoot reject FORM-3 file=reserved.wf
fn main() -> status: std::process::ExitStatus pure {
  let copy = 1_u8;
  return std::process::exit_status(code: copy);
}
```

```whitefoot-output
placeholder
```

The repaired program:

```whitefoot file=reserved.wf
fn main() -> status: std::process::ExitStatus pure {
  let copy_value = 1_u8;
  return std::process::exit_status(code: copy_value);
}
```

## Names and modules

Rules: MOD-1, MOD-2, MOD-3, MOD-4, MOD-5, MOD-6, MOD-7, MOD-8, MOD-9,
MOD-10, MOD-11, TYPE-6.

A name resolves only in the module its prefix names, and only to a
declaration that module makes accessible. MOD-5 is that lookup; MOD-6 keeps a
declaration private unless its module marks it public. A program therefore
reaches only the declarations its module graph allows.

The change: write a prefix and a name that the named module declares and
makes accessible.

The example is MOD-5. `pkg::missing` names nothing the module declares.

```whitefoot reject MOD-5 file=lookup.wf
fn zeta() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn alpha() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return pkg::missing();
}
```

```whitefoot-output
placeholder
```

The repaired program calls a function the module does declare:

```whitefoot file=lookup.wf
fn zeta() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn alpha() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return pkg::alpha();
}
```

## Types, construction and generics

Rules: CONST-1, CONST-2, FN-1, FN-2, FN-3, FN-4, FN-5, FN-6, FN-10, SNAP-1,
STOR-5, STOR-8, TYPE-2, TYPE-5, TYPE-9, TYPE-10.

A type fixes where a value of it may live and how it is built. TYPE-9 gives
each storage shape its homes: a shape whose capacity is chosen at run time
lives in the content of a `Box`, while a shape whose capacity is a constant
may sit inline. The prelude's opaque types are built only by their
constructing operations.

The change: use the form the type admits, here by writing the capacity as a
constant so the shape may sit in the result.

The example is TYPE-9. A runtime-capacity `Slots<u8>` cannot be a result.

```whitefoot reject TYPE-9 file=placement.wf
fn make() -> result: Slots<u8> pure {
  return slots_new::<u8, 2>();
}
```

```whitefoot-output
placeholder
```

The repaired program gives the type its constant capacity:

```whitefoot file=placement.wf
fn make() -> result: Slots<u8, 2> pure {
  return slots_new::<u8, 2>();
}
```

A `Box` around the runtime-capacity shape is the other home the rule admits.

## Operations and modes

Rules: OP-1, OP-5, OP-10, OP-11, OP-12.

Every computation names one operation from the operation table, with its
mode, and nothing is overloaded. A rejection here says the operation, the
types or the mode written is not a row of that table, so no implicit
behavior, such as an unstated rounding or overflow, is ever chosen for you.

The change: write an operation and mode that the table has for those types.

The example is OP-1. `cvt.wrap` converts between integer types; it has no
float source.

```whitefoot reject OP-1 file=operation.wf
fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.wrap::<f64, u8>(1.0_f64);
  return std::process::exit_status(code: 0_u8);
}
```

```whitefoot-output
placeholder
```

The repaired program uses the checked conversion, which returns a `Result`
that the program must handle:

```whitefoot file=operation.wf
fn main() -> status: std::process::ExitStatus pure {
  let code = cvt.checked::<f64, u8>(1.0_f64);
  match code {
    Ok(value: found) => {
    }
    Err(error: failed) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
```

## Goals an operation poses

Rules: ENT-2, MSR-1, MSR-3, OP-2, OP-4, OP-6, OP-14.

Some operations are partial: a subscript needs `i < len`, an exact `+` needs
a sum that fits, a bare conversion needs a value in range. Each carries a goal
the compiler must prove from the facts that reach it, and an unproved goal
rejects the program, so no accepted program checks at run time or traps
there. [Safety claims](../safety-claims.md) lists OP-4 under memory
corruption and OP-2 and OP-6 under silent overflow.

The change: guard the operation with a condition that states the goal, so the
operation runs only where the goal holds. Other routes close the same goal,
such as a requirement on the function or an invariant with `use` steps; the
message lists the ones that fit your goal.

The example is OP-4. Nothing says `i` is below the length of `b`.

```whitefoot reject OP-4 file=bounds.wf
fn get(b: &[u8], i: u64) -> result: u8 reads(b) {
  return b^[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

```whitefoot-output
placeholder
```

The repaired program reads the element only where the condition holds:

```whitefoot file=bounds.wf
fn get(b: &[u8], i: u64) -> result: u8 reads(b) {
  if i < b^.len {
    return b^[i];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

## Contracts, invariants and proofs

Rules: CALL-4, CALL-6, FN-8, FN-9, HOLD-1, INV-1, PRF-1, RANGE-1, RANGE-3,
RANGE-4, RANGE-5, TYPE-11.

A function states what its callers must establish (`requires`) and what it
guarantees (`ensures`); a loop states what stays true across its iterations
(an `invariant`). The checker uses only facts written there or derived from
them, so a goal it cannot derive is a rejection that names the missing fact.
When a fact is true but not derivable, the repair writes it down where the
checker reads it, and a proof of a loop's fact is an `invariant` whose `use`
steps name the facts that imply it.

The change: state the missing fact at the place the checker reads it.

The example is FN-8. The callee requires `x < 10_u64` and the caller says
nothing about `y`.

```whitefoot reject FN-8 file=contract.wf
fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn caller(y: u64) -> result: u64 pure {
  let r = small(x: y);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
```

```whitefoot-output
placeholder
```

The repaired program has the caller require the same fact, so each of its
callers must then establish it:

```whitefoot file=contract.wf
fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn caller(y: u64) -> result: u64 pure contract {
  requires y < 10_u64;
} {
  let r = small(x: y);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
```

## Ownership, liveness and windows

Rules: LIV-1, OWN-1, OWN-11, PROV-6, WIN-3.

Every value has one owner, and a value that cannot be copied is given away
only by an explicit `move`, after which its binding is dead and cannot be
read. Liveness is joined at every branch, which is what lets each release run
unconditionally. [Safety claims](../safety-claims.md) lists OWN-1 and LIV-1
under memory corruption and under uninitialized reads, and WIN-3 and OWN-11
under uninitialized reads.

The change: write `move` at the one place a value leaves its owner, and use
the value nowhere after that.

The example is OWN-1. `Parcel` is `nocopy`, so returning `p` gives it away
without saying so.

```whitefoot reject OWN-1 file=ownership.wf
nocopy struct Parcel {
  value: u8;
}

fn pass(p: Parcel) -> result: Parcel pure {
  return p;
}
```

```whitefoot-output
placeholder
```

The repaired program writes the move:

```whitefoot file=ownership.wf
nocopy struct Parcel {
  value: u8;
}

fn pass(p: Parcel) -> result: Parcel pure {
  return move p;
}
```

## References

Rules: REF-1, REF-2, REF-3, REF-4, TYPE-7, TYPE-8.

A reference names a place, is valid only until a write, move or release
reaches that place, and never leaves the function that formed it: it cannot
be returned or stored in an aggregate. A reference therefore never outlives
its target. [Safety claims](../safety-claims.md) lists REF-2 and REF-3 under
memory corruption.

The change: return owned data and let the caller form the reference.

The example is REF-3. `&local` would point at a place that disappears when
the function returns.

```whitefoot reject REF-3 file=escape.wf
fn leak() -> result: i32 pure {
  let local = 0_i32;
  return &local;
}
```

```whitefoot-output
placeholder
```

The repaired program returns the value:

```whitefoot file=escape.wf
fn leak() -> result: i32 pure {
  let local = 0_i32;
  return local;
}
```

## Effect rows

Rules: EFF-1, EFF-2, SET-1.

A function declares in its effect row which places it reads and writes, and a
caller's proofs rely on that row. A `set` whose target the row does not
declare as written is rejected, and a body that does more than its row says
is rejected by EFF-2. The row is how a caller learns which of its facts a
call can change.

The change: declare in the row the write the body makes.

The example is SET-1. The row says `reads(c)` but the body assigns to a field.

```whitefoot reject SET-1 file=row.wf
struct Cell {
  value: i32;
}

fn clear(c: &Cell) -> result: unit reads(c) {
  set c^.value = 0_i32;
  return unit;
}
```

```whitefoot-output
placeholder
```

The repaired program declares the write:

```whitefoot file=row.wf
struct Cell {
  value: i32;
}

fn clear(c: &Cell) -> result: unit writes(c.value) {
  set c^.value = 0_i32;
  return unit;
}
```

## Aliased arguments

Rules: EFF-5.

At a call, the paths the arguments supply are substituted into the callee's
row, and two paths that are not proved disjoint, one of them written, are
rejected. This keeps a call from writing a place while another argument reads
or writes the same place. [Safety claims](../safety-claims.md) lists EFF-5
under data races.

The change: pass places the checker can show are different.

The example is EFF-5. `copy_across` reads through `source` and writes through
`destination`, and the call passes the same cell as both.

```whitefoot reject EFF-5 file=alias.wf
struct Cell {
  value: u64;
}

fn copy_across(source: &Cell, destination: &Cell) -> result: unit reads(source.value), writes(destination.value) {
  let v = source^.value;
  set destination^.value = v;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = Cell(value: 4_u64);
  copy_across(source: &a, destination: &a);
  return std::process::exit_status(code: 0_u8);
}
```

```whitefoot-output
placeholder
```

The repaired program passes two different cells:

```whitefoot file=alias.wf
struct Cell {
  value: u64;
}

fn copy_across(source: &Cell, destination: &Cell) -> result: unit reads(source.value), writes(destination.value) {
  let v = source^.value;
  set destination^.value = v;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = Cell(value: 4_u64);
  let b = Cell(value: 0_u64);
  copy_across(source: &a, destination: &b);
  return std::process::exit_status(code: 0_u8);
}
```

## Concurrency

Rules: SHARE-1, SHARE-2, WAIT-1, WAIT-3.

A call that may wait is part of its callee's signature, and only a function
that itself waits may make one. A spawn starts a context whose footprint is
the storage its arguments moved or copied into it, so a spawned callee takes
only value parameters [WAIT-3]; a shared object is reached only through the
atomic forms [SHARE-2]. Together they let the checker prove that two contexts
never touch the same storage unannounced. [Safety claims](../safety-claims.md)
lists SHARE-2 and WAIT-3 under data races.

The change: declare `waits` on the function that makes the waiting call.

The example is WAIT-1. `close_it` calls a function that waits and does not
declare it.

```whitefoot reject WAIT-1 file=waiting.wf
fn close_it(factory: &std::io::HandleFactory, directory: std::fs::DirectoryRead) -> result: unit writes(factory) {
  std::fs::close_directory(factory: factory, directory: move directory);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

```whitefoot-output
placeholder
```

The repaired program declares the wait:

```whitefoot file=waiting.wf
fn close_it(factory: &std::io::HandleFactory, directory: std::fs::DirectoryRead) -> result: unit writes(factory) waits {
  std::fs::close_directory(factory: factory, directory: move directory);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

## Errors and matches

Rules: ERR-2, ERR-3, GRAM-12.

A recoverable failure is an ordinary `Result` or `Option` value, and a `match`
over an enum must name every variant exactly once. A missing arm is a
rejection because the program would otherwise have an input with no defined
behavior.

The change: add the arm for each missing variant.

The example is ERR-2. `Blue` has no arm.

```whitefoot reject ERR-2 file=arms.wf
enum Color {
  Red();
  Green();
  Blue();
}

fn pick(c: Color) -> result: i32 pure {
  match c {
    Red() => {
      return 1_i32;
    }
    Green() => {
      return 2_i32;
    }
  }
}
```

```whitefoot-output
placeholder
```

The repaired program names all three variants:

```whitefoot file=arms.wf
enum Color {
  Red();
  Green();
  Blue();
}

fn pick(c: Color) -> result: i32 pure {
  match c {
    Red() => {
      return 1_i32;
    }
    Green() => {
      return 2_i32;
    }
    Blue() => {
      return 3_i32;
    }
  }
}
```

## Stops that are not rejections

A compiler can also stop for a reason that says nothing about your program: a
capability it has not implemented, an internal error, or running out of
resources. Such a stop cites no numbered rule and names a category other than
`Source`; the specification's rule on rejections, DIAG-1, defines only the
source rejections above. Nothing in this guide repairs a stop of that kind.
[What a rejection tells you](../articles/what-a-rejection-tells-you.md)
describes how to tell them apart.
