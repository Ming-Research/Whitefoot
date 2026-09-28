# What a rejection tells you

Whitefoot programs are often written in a loop: write, compile, read the
rejection, change the source, compile again. Much of that writing is done by
AI agents, and the rejections are written for whoever is in the loop. Each
compile reports one rejection, which names one rule and one location, states
what is still unproved, and, where the rule provides for it, lists changes
that compile when they are applied as written.

This article follows one small function through three compiles, then looks
at the other kinds of rejection and at how the repairs are tested.

## 1. One rejection, one rule, one location

The function adds one to every counter in an array:

```
fn bump_all(counts: &[u32]) -> result: unit reads(counts) {
  for (i in 0_u64..deref(counts).len) {
    set deref(counts)[i] = deref(counts)[i] + 1_u32;
  }
  return unit;
}
```

The first compile stops here:

```text
r0.wf:6:9: error[SET-1]: InvalidSetTarget
  source:     set deref(counts)[i] = deref(counts)[i] + 1_u32;
  marker:         ^^^^^^^^^^^^^^^^
  root_class: a reference whose declared row does not write this path
  required_classes: a live own-mode value binding, or a path below deref of a reference whose row declares that write
```

The first line has the same shape in every rejection: file, line and column,
the rule in brackets, and the kind of failure. SET-1 is a numbered rule in
the [specification](../../spec/kernel-spec.md), and its text is the complete
statement of what went wrong. Then comes the source line with the offending
part marked, and then one `label: value` line per field of the rejection.

The compiler reports one rejection per run. It walks the program in a fixed
order and stops at the first one, so the same compiler, given the same
program and options, always reports the same rejection. A program with three
mistakes takes three compiles.

## 2. Fields say what was found and what was needed

This rejection has no repair line. Its fields describe the mismatch instead:
the target of the `set` is a path under a reference whose row, `reads(counts)`,
does not declare a write, and a `set` needs either a local value or a path
whose row declares the write. Not every rule carries a repair; the
specification says which ones do.

The change is to declare the write:

```
fn bump_all(counts: &[u32]) -> result: unit writes(counts) {
```

## 3. The goal that is still open

The second compile gets further:

```text
r1.wf:6:28: error[OP-2]: UndischargedIntegerDomainObligation
  source:     set deref(counts)[i] = deref(counts)[i] + 1_u32;
  marker:                            ^^^^^^^^^^^^^^^^^^^^^^^^
  residual: deref(counts)[i] +defined 1_u32
  disposition: Unproved
  mechanical_fix: `deref(counts)[i] +defined 1_u32` is not proved here: when facts that reach the operation imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the operation with `if deref(counts)[i] +defined 1_u32` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare; or write the `+wrap`, `+checked` or `+sat` form
```

The index is fine: the loop proves `i < deref(counts).len`. What is open is
the `+`, which needs the sum to fit in a `u32`. The `residual` is that goal,
written with the program's own expressions: `deref(counts)[i]`, as the
function would spell it, not an internal name.

The `mechanical_fix` lists the routes that can close this particular goal,
each a change at the marked expression. Which routes appear depends on what
the goal reads. A requirement on the function is offered only when the goal
reads nothing but parameters the function has not changed on the way; this
goal reads an array element at the loop's index, which a requirement on
`bump_all` cannot name, so the routes are a proof, a guard, or a different
operation.

Both of the following compile. The first says what a full counter does; the
second skips it:

```
set deref(counts)[i] = deref(counts)[i] +sat 1_u32;
```

```
if deref(counts)[i] +defined 1_u32 {
  set deref(counts)[i] = deref(counts)[i] + 1_u32;
}
```

## 4. Unproved and refuted

`disposition` separates two situations. `Unproved` means the facts that reach
the goal do not settle it, and a fact supplied from outside, such as a guard
or a requirement, can. `Refuted` means the facts show the goal is false:

```
fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn main() -> status: ExitStatus pure {
  let r = small(x: 20_u64);
  return exit_status(code: 0_u8);
}
```

```text
refuted.wf:11:11: error[FN-8]: UndischargedCallRequirement
  source:   let r = small(x: 20_u64);
  marker:           ^^^^^^^^^^^^^^^^
  concrete_callee: small
  requires_clause: refuted.wf:5:3 "requires x < 10_u64;"
  instantiated_goal: 20_u64 < 10_u64
  disposition: Refuted
  mechanical_fix: `20_u64 < 10_u64` is false for the values that reach this call, so no fact can establish it here: pass arguments that satisfy it, or change the statements or requirements that fix those values
```

No guard is offered. A guard around a goal that is always false would
compile, but the code inside it could never run, so the repair asks for the
values to change instead. `requires_clause` points at the requirement the
call failed, and `instantiated_goal` is that requirement with the call's
arguments in place of the parameters.

## 5. Before the checker: the grammar

Mistakes in the text are found before any goal is considered. A missing
semicolon:

```text
gram.wf:6:3: error[GRAM-5]: UnexpectedToken
  source:   return exit_status(code: 0_u8);
  marker:   ^^^^^^
  expected: [";", "{", ",", "<", ">", "*", "+", "-", "+wrap", "+defined", "+checked", "+sat", "-wrap", "-defined", "-checked", "-sat", "*wrap", "*defined", "*checked", "*sat", "/", "/defined", "/checked", "%", "%defined", "%checked", "==", "!=", "<=", ">="]
  found: "return"
```

The marker is on the first token the grammar could not accept, and
`expected` lists every token that could have come there. The same field
explains a rule that has no counterpart in C. An expression does one
operation, so `x +wrap y *wrap z` stops at the second operator, where the
grammar expects the statement to end:

```text
nested.wf:5:20: error[GRAM-4]: UnexpectedToken
  source:   return x +wrap y *wrap z;
  marker:                    ^^^^^
  expected: [";", ","]
  found: "*wrap"
```

The product goes in its own `let`.

## 6. Repairs are tested by applying them

A repair is a promise that the change it describes compiles. The compiler's
tests hold that promise for 75 repairs
([`compiler/src/driver/pinned_repairs.rs`](../../compiler/src/driver/pinned_repairs.rs)).
Each is pinned as a pair:

- a rejected program;
- the rule and the exact repair text it must print;
- one program for each alternative, written by carrying out the repair's
  words.

Every repaired program must be accepted. It must also be accepted for the
right reason: no check in it may pass only because the facts at that point
contradict each other, which is how a guard around a refuted goal would pass.
The 75 pairs cover mostly goals, effect rows and opaque struct types. Other
rejections print fixed repair sentences that have no pair yet; the project's
[todo list](../../docs/todo.md) records this and names several of them.

## 7. For tools: JSON

`--diagnostic-format json` prints the same rejection as one JSON object per
line, with the fields the text leaves out: the category, the stage and the
byte interval.

```text
{"rule":"OP-2","kind":"UndischargedIntegerDomainObligation","category":"Source","stage":"Semantics","at":{"file":"r1.wf","line":6,"column":28},"bytes":{"start":221,"end":245},"source":"    set deref(counts)[i] = deref(counts)[i] + 1_u32;","detail":{"residual":"deref(counts)[i] +defined 1_u32","disposition":"Unproved","mechanical_fix":"`deref(counts)[i] +defined 1_u32` is not proved here: when facts that reach the operation imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the operation with `if deref(counts)[i] +defined 1_u32` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare; or write the `+wrap`, `+checked` or `+sat` form"}}
```

The text form is the default because the reader in the loop compares the
quoted source with its own program, and JSON would escape that source and
drop the marker.

## 8. What a rejection is not

- **Not a guess.** A repair is offered only where the rule provides one, and
  it names changes at the rejected construct. When the values are wrong, it
  says so instead of suggesting a guard.
- **Not a batch.** One rejection per compile keeps each one exact, at the
  cost of a round per mistake.
- **Not a compiler failure.** A missing compiler capability, an internal
  error or running out of resources also stops the compiler, but the record
  cites no rule and names a category such as `Unsupported` instead of
  `Source`. It says nothing about whether the program is valid.

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) or design |
|---|---|
| One rule and one location, the first rejection of a fixed traversal, repairs only where a rule gives them | DIAG-1 |
| A `set` target needs a local value or a declared write | SET-1, EFF-1 |
| The integer goal and its routes | OP-2, ENT-6 |
| A call's requirement, instantiated with its arguments | FN-8 |
| The grammar and one operation per expression | GRAM-4, GRAM-5, GRAM-6 |
| The text and JSON renderings | [`design/compiler/diagnostic-rendering.md`](../../design/compiler/diagnostic-rendering.md) |
| The wording of repairs and their pinned pairs | [`design/compiler/diagnostic-repairs.md`](../../design/compiler/diagnostic-repairs.md) |

Every program and rejection in this article comes from the compiler at
commit `e041772d8`, with `whitefootc --check file.wf`; the fragments are
compiled inside a file with the standard `ExitStatus` aliases and a `main`.
