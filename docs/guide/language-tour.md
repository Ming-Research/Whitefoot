# Language tour

This tour goes through Whitefoot one concept at a time. Each concept starts
with the problem it answers, shows a complete program, and says what the
compiler guarantees about it. It assumes you have installed the compiler and
run a program, as [Getting started](getting-started.md) does.

Every program below is a whole source file. The gate compiles each one and
checks the outcome its fence states: accepted, run with a given exit status,
or rejected under a given rule (see
[Writing examples](README.md#writing-examples)). When the text says a program
is rejected, the compiler rejects it and cites the rule named there. A
program without a `main` is a library: the compiler checks it and builds
nothing.

The [specification](../../spec/kernel-spec.md) defines the language and
numbers its rules, such as OP-2 or OWN-1. The tour names the rule behind each
guarantee so you can read the exact statement. The
[articles](../articles/integers.md) go deeper on single ideas; the tour
links to the relevant one and does not repeat it.

Three habits run through every program, and each is a rule, not a style:

- Every construct has one spelling, and the compiler rejects anything else
  rather than reformatting it ([FORM-1, FORM-2](../../spec/kernel-spec.md)).
  Two spaces per level, one blank line between items, no comments.
  Documentation goes in a `doc` entry, which the compiler checks like any
  other part of the declaration.
- Every numeric literal names its type, as in `250_u8`, and every call names
  its arguments, as in `exit_status(code: total)`.
- One operation per expression. A longer computation is a sequence of `let`
  bindings, so there is no operator precedence to learn
  ([GRAM-6](../../spec/kernel-spec.md)).

## 1. Functions

A function declares its parameters, its result and its effects. `pure`
means it reads and writes no state through references; section 8 explains
effects.

```whitefoot run exit=7 file=functions.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn at_least(value: u8, lower: u8) -> result: u8 pure {
  doc "Returns value, or lower when value is smaller.";
  if value < lower {
    return lower;
  }
  return value;
}

fn main() -> status: ExitStatus pure {
  let raised = at_least(value: 3_u8, lower: 7_u8);
  return exit_status(code: raised);
}
```

- `alias` gives a short name to something from another module. The
  standard library's process module supplies `ExitStatus`, the type of a
  program's result, and the function that builds one.
- `-> result: u8` names the result and gives its type. A `return` delivers
  the value.
- `doc` is the first entry of a body. It is part of the declaration, and it
  is the only place prose goes.
- `if` takes a `Bool` and has an optional `else`. Arguments are written
  `name: value`, so a swapped pair of arguments of the same type cannot go
  unnoticed.
- `main` is the entry. It returns an `ExitStatus`, and the program's exit
  status is the code it carries: this program exits with 7.

## 2. Integers: one spelling per meaning

In most languages `a + b` on two bytes that add up to 260 wraps, traps or
promotes. In Whitefoot a bare `+` means the exact sum, and the compiler
accepts it only where it has proved the sum fits the type. Every other
meaning has its own spelling:

| Spelling | Meaning |
|---|---|
| `a + b` | the exact sum, accepted only where it is proved to fit |
| `a +defined b` | whether the exact sum fits, as a `Bool` |
| `a +wrap b` | the sum modulo 2^8 for a `u8` |
| `a +checked b` | `Ok` with the sum, or `Err` with an `Overflow` |
| `a +sat b` | the sum clamped to the type's range |

The same suffixes apply to `-` and `*`, and `/` and `%` take `defined` and
`checked`. This function tries each form on the arguments it is given:

```whitefoot run exit=5 file=integers.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn summary(a: u8, b: u8) -> result: u8 pure {
  if a +defined b {
    let exact = a + b;
    return exact;
  }
  let failed = match a +checked b {
    Ok(value: sum) => {
      give 0_u8;
    }
    Err(error: overflow) => {
      give 1_u8;
    }
  }
  let wrapped = a +wrap b;
  let clamped = a +sat b;
  if clamped != 255_u8 {
    return 99_u8;
  }
  let code = wrapped +wrap failed;
  return code;
}

fn main() -> status: ExitStatus pure {
  let code = summary(a: 250_u8, b: 10_u8);
  return exit_status(code: code);
}
```

For 250 and 10 the exact sum does not fit, so the `if` is skipped. The
checked form reports an overflow (1), the wrapped sum is 4, the clamped sum
is 255, and the program exits with 5.

Inside the `if a +defined b` block the compiler knows the sum fits, so the
bare `+` there is accepted: a branch condition is a fact in its block.
Without such a fact the compiler rejects the operation under OP-2. A
narrowing conversion works the same way. Converting a `u32` to a `u8` is
only accepted where the value is known to fit:

```whitefoot reject OP-6 file=narrow.wf
fn narrow(value: u32) -> result: u8 pure {
  return cvt::<u32, u8>(value);
}
```

The compiler found no fact that `value` fits in a `u8`. You can supply the
fact, guard the conversion with a branch, or say what should happen when it
does not fit. `cvt.checked` returns a `Result`, and `cvt.wrap` keeps the low
bits:

```whitefoot file=narrow_checked.wf
fn narrow(value: u32) -> result: Result<u8, NarrowError> pure {
  return cvt.checked::<u32, u8>(value);
}
```

There are no implicit conversions and no mixed-type operations
([TYPE-4](../../spec/kernel-spec.md)): both operands of `+` have the same
type. [Integers](../articles/integers.md) covers the whole family, including
what each form compiles to.

## 3. Control flow

The conditional forms are fixed by the type they test. A `Bool` takes `if`.
An enum or an integer takes `match`. A `match` can deliver a value: each arm
ends in `give`, or leaves the function. An integer `match` lists constant
arms and ends in `else` when the listing is partial. Loops are a counted
`for` over a half-open range, and a general `loop` with `break` and
`continue`.

```whitefoot run exit=10 file=control.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

const letters: Array<u8, 4> =[97_u8, 98_u8, 99_u8, 97_u8];

fn weight(letter: u8) -> result: u8 pure {
  let score = match letter {
    97_u8 => {
      give 1_u8;
    }
    98_u8 => {
      give 3_u8;
    }
    else => {
      give 0_u8;
    }
  }
  return score;
}

fn halvings(start: u8) -> steps: u8 pure {
  let value = start;
  let steps = 0_u8;
  loop @halve {
    if value <= 1_u8 {
      break @halve;
    }
    set value = value / 2_u8;
    set steps = steps +wrap 1_u8;
  }
  return steps;
}

fn main() -> status: ExitStatus pure {
  let total = 0_u8;
  for (i in 0_u64..letters.len) {
    let points = weight(letter: letters[i]);
    set total = total +wrap points;
  }
  let steps = halvings(start: 40_u8);
  let code = total +wrap steps;
  return exit_status(code: code);
}
```

- `const` declares immutable data. There is no mutable global state: a
  function reaches only what its parameters give it, which is what makes the
  effect rows of section 8 complete.
- `for (i in 0_u64..letters.len)` visits `0` up to, but excluding,
  `letters.len`. The binder `i` is an unsigned 64-bit integer, and the loop's
  range is what proves `letters[i]` is in bounds.
- `set` changes a local. `let` creates one.
- `loop @halve` names the loop so `break @halve` says which loop it leaves.
  The letters weigh 1, 3, 0 and 1, and 40 halves five times before it
  reaches 1, so the program exits with 10.
- `value / 2_u8` is the exact quotient, accepted because the divisor is
  known to be nonzero.

## 4. Structs and enums

A struct names its fields. An enum names its variants, and each variant
names the fields it carries. Construction and matching name every field, in
declaration order; there is no positional form
([GRAM-8](../../spec/kernel-spec.md)).

```whitefoot run exit=48 file=shapes.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Rect {
  width: u32;
  height: u32;
}

enum Shape {
  Square(side: u32);
  Rectangle(extent: Rect);
  Empty();
}

fn area(shape: Shape) -> result: u32 pure {
  match shape {
    Square(side: edge) => {
      let product = edge *wrap edge;
      return product;
    }
    Rectangle(extent: room) => {
      let product = room.width *wrap room.height;
      return product;
    }
    Empty() => {
      return 0_u32;
    }
  }
}

fn main() -> status: ExitStatus pure {
  let square = Shape::Square(side: 6_u32);
  let room = Rect(width: 3_u32, height: 4_u32);
  let rectangle = Shape::Rectangle(extent: room);
  let first = area(shape: square);
  let second = area(shape: rectangle);
  let total = first +wrap second;
  let code = cvt.wrap::<u32, u8>(total);
  return exit_status(code: code);
}
```

A `match` over an enum names every variant. There is no wildcard arm,
because a wildcard is where a variant added later would be absorbed without
anyone noticing. Leave a variant out and the compiler rejects the match
under ERR-2:

```whitefoot reject ERR-2 file=nonexhaustive.wf
enum Sign {
  Neg();
  Zero();
  Pos();
}

fn code_of(sign: Sign) -> result: u64 pure {
  match sign {
    Neg() => {
      return 1_u64;
    }
    Zero() => {
      return 2_u64;
    }
  }
}
```

The areas are 36 and 12, so the first program exits with 48. A struct whose
fields can all be copied is itself copied on use, and so is an enum whose
payloads can be; section 7 says what happens to the types that cannot.

## 5. Failure is a value

Whitefoot has no exceptions, no panics and no unwinding. An operation that
can fail returns a `Result` or an `Option`, and the caller decides what to
do. `propagate` forwards a failure: it binds the `Ok` payload, or returns the
`Err` from the current function with the same error type
([ERR-3](../../spec/kernel-spec.md)).

```whitefoot run exit=43 file=failures.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum ParseError {
  NotDigit();
}

fn digit_value(byte: u8) -> result: Result<u8, ParseError> pure {
  if byte < 48_u8 {
    let bad = ParseError::NotDigit();
    return Err<u8, ParseError>(error: bad);
  }
  if byte > 57_u8 {
    let bad = ParseError::NotDigit();
    return Err<u8, ParseError>(error: bad);
  }
  let value = byte - 48_u8;
  return Ok<u8, ParseError>(value: value);
}

fn two_digits(tens: u8, ones: u8) -> result: Result<u8, ParseError> pure {
  let high = propagate digit_value(byte: tens);
  let low = propagate digit_value(byte: ones);
  let scaled = high *wrap 10_u8;
  let total = scaled +wrap low;
  return Ok<u8, ParseError>(value: total);
}

fn main() -> status: ExitStatus pure {
  let parsed = two_digits(tens: 52_u8, ones: 50_u8);
  let number = match parsed {
    Ok(value: n) => {
      give n;
    }
    Err(error: reason) => {
      return exit_status(code: 255_u8);
    }
  }
  let broken = two_digits(tens: 52_u8, ones: 120_u8);
  let failed = match broken {
    Ok(value: n) => {
      give 0_u8;
    }
    Err(error: reason) => {
      give 1_u8;
    }
  }
  let code = number +wrap failed;
  return exit_status(code: code);
}
```

- `Ok<u8, ParseError>(value: value)` names its type arguments. Generic
  constructors and functions are always given them explicitly (section 11).
- In `digit_value`, `byte - 48_u8` is the exact difference. It is accepted
  because the two earlier `if` statements returned for every `byte` below
  48, so `byte >= 48` is a fact at that line.
- Bytes 52 and 50 are the characters `4` and `2`, so `two_digits` gives 42.
  The second call fails on the byte 120 and `failed` is 1, so the program
  exits with 43.

`Option` works the same way with `Some<T>(value: ...)` and `None<T>()`.
Allocation never reports a failure either; section 14 says why.

## 6. Arrays, windows and bounds

Whitefoot has several storage shapes. `Array<T, N>` is a full array whose
length is fixed. `Slots<T, N>` is a window: storage for up to `N` elements
of which a prefix is filled, so its length changes as you append. `Ring`,
`Box` and the heap-capacity forms are described in the specification
([section 6](../../spec/kernel-spec.md)). A reference to a run of elements is
written `&[T]`.

An index is a partial operation: it is defined only when `i < len`. The
compiler accepts a subscript only where it has proved that, and rejects it
otherwise:

```whitefoot reject OP-4 file=unproved_index.wf
fn get(items: &[u8], position: u64) -> result: u8 reads(items) {
  return items^[position];
}
```

The compiler could not show `position < items^.len`. `items^` reads
through a reference (section 8). Here there are two ways forward. One is to
write the branch the failure case needs, with the failure as a value:

```whitefoot file=guarded_index.wf
fn get(items: &[u8], position: u64) -> result: Option<u8> reads(items) {
  if position < items^.len {
    let byte = items^[position];
    return Some<u8>(value: byte);
  }
  return None<u8>();
}
```

The other is to move the fact to the caller with a contract (section 9). The
next example shows a counted loop's range and a loop invariant as further
sources of such a fact, and section 10 shows how to direct a proof by hand. In
every case the compiler inserts no check of its own, so every comparison that
guards an access is one you can see in the source.

A counted loop proves its own index. This program appends the squares of
`0` to `7` to a window, then adds them up:

```whitefoot run exit=140 file=windows.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn sum_squares() -> total: u64 pure {
  let r = slots_new::<u64, 8>();
  for (
    i in 0_u64..8_u64,
    invariant h: r.len == i
  ) {
    let square = i *wrap i;
    place_back(window: &r, value: square);
  }
  let total = 0_u64;
  for (j in 0_u64..r.len) {
    let x = r[j];
    set total = total +wrap x;
  }
  return total;
}

fn main() -> status: ExitStatus pure {
  let total = sum_squares();
  let code = cvt.wrap::<u64, u8>(total);
  return exit_status(code: code);
}
```

The second loop's range makes `j < r.len` a fact in its body. The first loop
carries an `invariant`: a named fact, `r.len == i`, that holds before the
first iteration and again after each one. The sum 0 + 1 + 4 + 9 + 16 + 25 + 36
+ 49 is 140.

An index the loop computes itself needs an invariant that says where it
stays. This function keeps the non-space bytes of a buffer, in place, and
the invariant `kept <= i` is what proves the store `buf^[kept]` in range:

```whitefoot file=squeeze.wf
fn squeeze(buf: &[u8]) -> kept: u64 writes(buf) {
  let kept = 0_u64;
  for (
    i in 0_u64..buf^.len,
    invariant behind: kept <= i
  ) {
    let byte = buf^[i];
    if byte != 32_u8 {
      set buf^[kept] = byte;
      set kept = kept + 1_u64;
    }
  }
  return kept;
}
```

Remove the invariant and the compiler rejects the store, naming the fact
that is missing:

```whitefoot reject OP-4 file=squeeze_without_invariant.wf
fn squeeze(buf: &[u8]) -> kept: u64 writes(buf) {
  let kept = 0_u64;
  for (i in 0_u64..buf^.len) {
    let byte = buf^[i];
    if byte != 32_u8 {
      set buf^[kept] = byte;
      set kept = kept + 1_u64;
    }
  }
  return kept;
}
```

[Prove it or write a branch](../articles/prove-it-or-write-a-branch.md) walks
through the two routes for indices and arithmetic, and
[Proofs without a solver, by hand](../articles/proofs-by-hand.md) shows how
the compiler checks an invariant.

## 7. Ownership

Every value has one owner. Passing an owned value to a function that takes
it by value hands the value over, and the old name may not be used again.
Whether a type can be copied follows from what it holds: a struct of copy
fields is copy, and so is an enum whose payloads are. A type that holds a
`Box` or a `Slots` window is not. A declaration can also remove the ability
to copy with `nocopy`. A value that cannot be copied is passed on with an
explicit `move`, and the compiler rejects any later use of the name:

```whitefoot file=ownership.wf
nocopy struct Ticket {
  number: u32;
}

fn redeem(ticket: Ticket) -> result: u32 pure {
  doc "Takes the ticket by value and gives it up for good.";
  return ticket.number;
}

fn spend() -> result: u32 pure {
  let ticket = Ticket(number: 7_u32);
  let number = redeem(ticket: move ticket);
  return number;
}
```

Writing `move` on a value that can be copied is an error as well, so the
word marks exactly the places where ownership changes hands. Using `ticket`
after the `move` is rejected under OWN-1:

```whitefoot reject OWN-1 file=moved.wf
nocopy struct Ticket {
  number: u32;
}

fn redeem(ticket: Ticket) -> result: u32 pure {
  return ticket.number;
}

fn twice(ticket: Ticket) -> result: u32 pure {
  let first = redeem(ticket: move ticket);
  let second = redeem(ticket: move ticket);
  return second;
}
```

Release is derived from the type and runs no code of yours: there are no
destructors. A type declared `nodrop` is stricter still. It cannot be
discarded, so every path must pass it on or hand it to a function that
consumes it. The standard library's files, directories, listeners and
connections are of this kind, and code that could lose one is rejected.
([OWN-1](../../spec/kernel-spec.md) and [STOR-3](../../spec/kernel-spec.md)
state the rules.)

## 8. References and effects

A reference is written `&place`, and the referent is read as `place^`. A
reference is only a local name for a path: it can be passed to a call, but
it cannot be stored in a struct or returned
([REF-3](../../spec/kernel-spec.md)). That is why Whitefoot has no
lifetimes: a reference cannot outlive what it names. A function that finds
something returns an index, and the caller forms the reference.

A reference carries no "mutable" marker. Whether a function may write
through a parameter is read from its effect row, the `reads(...)` and
`writes(...)` entries after the result. A row names paths that start at a
reference parameter, and `pure` is the empty row. The compiler checks the row
against the body in both directions. This program also moves some money:

```whitefoot run exit=70 file=effects.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Account {
  owner: u32;
  balance: u64;
}

fn transfer(payer: &Account, payee: &Account, amount: u64) -> moved: Bool writes(payer.balance), writes(payee.balance) {
  let available = payer^.balance;
  let current = payee^.balance;
  if amount <= available {
    if current +defined amount {
      set payer^.balance = available - amount;
      set payee^.balance = current + amount;
      return True();
    }
  }
  return False();
}

fn main() -> status: ExitStatus pure {
  let a = Account(owner: 1_u32, balance: 100_u64);
  let b = Account(owner: 2_u32, balance: 5_u64);
  let done = transfer(payer: &a, payee: &b, amount: 30_u64);
  let left = a.balance;
  let code = cvt.wrap::<u64, u8>(left);
  return exit_status(code: code);
}
```

From the signature alone a reviewer knows `transfer` changes at most the
two balances, and whatever the caller had proved about `owner` still holds
after the call. The account `a` ends with 70.

A row that says more than the body does is rejected, and so is a row that
says less. This function only reads, so a `writes` entry is a claim the body
never exercises:

```whitefoot reject EFF-2 file=padded_row.wf
fn peek(counter: &u64) -> result: u64 writes(counter) {
  return counter^;
}
```

The row `reads(counter)` is the accepted form. At each call the compiler
substitutes the arguments into the callee's row and compares what different
arguments touch. Two places that overlap, with at least one written, must be
proved apart. Passing one account as both payer and payee is rejected under
EFF-5:

```whitefoot reject EFF-5 file=overlap.wf
struct Account {
  owner: u32;
  balance: u64;
}

fn transfer(payer: &Account, payee: &Account, amount: u64) -> moved: Bool writes(payer.balance), writes(payee.balance) {
  let available = payer^.balance;
  let current = payee^.balance;
  if amount <= available {
    if current +defined amount {
      set payer^.balance = available - amount;
      set payee^.balance = current + amount;
      return True();
    }
  }
  return False();
}

fn main() -> status: std::process::ExitStatus pure {
  let a = Account(owner: 1_u32, balance: 100_u64);
  let done = transfer(payer: &a, payee: &a, amount: 30_u64);
  return std::process::exit_status(code: 0_u8);
}
```

These two judgments, the exact row and the proved separation of arguments,
are what the rest of the language builds on: contracts rely on them to keep
facts across calls, the backend gets `noalias` from them, and section 12
turns them into parallelism. [What a reviewer reads](../articles/what-a-reviewer-reads.md)
follows the effect row through a review.

## 9. Contracts and type invariants

A `contract` block states what a function needs and what it gives back. Each
`requires` is a fact inside the function and a goal at every call. Each
`ensures` is a goal at every `return` and a fact for every caller. The
compiler checks a call against the callee's signature alone and never looks
into its body.

```whitefoot file=contracts.wf
fn clamp(x: u64, hi: u64) -> result: u64 pure contract {
  ensures result <= hi;
} {
  if x <= hi {
    return x;
  }
  return hi;
}

fn pick(table: &[u8], i: u64) -> result: u8 reads(table) contract {
  requires 0_u64 < table^.len;
} {
  let last = table^.len - 1_u64;
  let j = clamp(x: i, hi: last);
  return table^[j];
}
```

In `pick`, `table^.len - 1_u64` is accepted because of the `requires`, and
`table^[j]` is accepted because `clamp`'s `ensures` says `j <= last`. A
promise the body does not keep is rejected at the `return` that breaks it
(FN-9):

```whitefoot reject FN-9 file=broken_promise.wf
fn clamp(x: u64, hi: u64) -> result: u64 pure contract {
  ensures result < hi;
} {
  if x <= hi {
    return x;
  }
  return hi;
}
```

A struct can state a relation between its own fields as a type invariant.
It is owed wherever a value of the type is constructed or handed on, and it
is assumed at the entry of every function that takes one
([TYPE-11](../../spec/kernel-spec.md)). Here a cursor stays below its
window's length, so the read in `advance` needs no check:

```whitefoot run exit=20 file=cursor.wf
struct Table {
  slots: Slots<u64, 8>;
  next: u64;
  invariant cursor(table): table.next < table.slots.len;
}

fn advance(t: &Table) -> result: u64 reads(t.slots), writes(t.next) {
  let at = t^.next;
  let got = t^.slots[at];
  let after = at + 1_u64;
  if after == t^.slots.len {
    set t^.next = 0_u64;
  } else {
    set t^.next = after;
  }
  return got;
}

fn main() -> status: std::process::ExitStatus pure {
  let slots = slots_new::<u64, 8>();
  place_back(window: &slots, value: 10_u64);
  let table = Table(slots: move slots, next: 0_u64);
  let a = advance(t: &table);
  let b = advance(t: &table);
  let sum = a +wrap b;
  let code = cvt.wrap::<u64, u8>(sum);
  return std::process::exit_status(code: code);
}
```

The window holds one element, so both calls read it and the program exits
with 20. Contracts and invariants are erased before the program is lowered:
they cost nothing at run time, and they add no effect to a function's row.

## 10. Proofs you write

The compiler derives most facts by a fixed procedure with no solver, no
timeout and no work budget, so two machines never disagree about whether a
program is accepted ([ENT-1](../../spec/kernel-spec.md)). When a goal is
beyond that procedure, you add a named `invariant` with `use` steps that say
which known facts add up to it. The compiler checks the steps you wrote and
does not search for others. Here a weighted sum of three bounds needs the
facts named:

```whitefoot file=use_steps.wf
fn combined_ports(p: u32, p_cap: u32, q: u32, q_cap: u32, r: u32, r_cap: u32) -> result: unit pure contract {
  requires p <= p_cap;
  requires q <= q_cap;
  requires r <= r_cap;
} {
  invariant base_p: p <= p_cap;
  invariant base_q: q <= q_cap;
  invariant base_r: r <= r_cap;
  invariant combined: 2_u32 * p + 3_u32 * q + 4_u32 * r <= 2_u32 * p_cap + 3_u32 * q_cap + 4_u32 * r_cap {
    use 2 times base_p;
    use 3 times base_q;
    use 4 times base_r;
  }
  return unit;
}
```

`use 2 times base_p;` adds the fact `base_p` twice. A proof written where
the compiler needs none is an error (PRF-1), so the proofs in your source are
exactly the ones the compiler could not find:

```whitefoot reject PRF-1 file=redundant_use.wf
fn two_parts(first: u64, first_limit: u64, second: u64, second_limit: u64) -> result: unit pure contract {
  requires first <= first_limit;
  requires second <= second_limit;
} {
  invariant pair_bound: first + second <= first_limit + second_limit {
    use (first <= first_limit);
    use (second <= second_limit);
  }
  return unit;
}
```

The price is real: you write loop invariants and, now and then, a `use` step
that a solver would have found for you.
[Proofs without a solver, by hand](../articles/proofs-by-hand.md) works
through the procedure.

## 11. Generics

A generic function takes explicit type, constant or function arguments at
every call, and the compiler checks and compiles each instance as concrete
code. There is no inference and no dynamic dispatch. A bound says what the
argument must be able to do: `T: Int` admits the integer types, `T: copy`
admits types that can be copied.

```whitefoot run exit=9 file=generics.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn maximum<T: Int>(left: T, right: T) -> result: T pure {
  return imax(left, right);
}

fn main() -> status: ExitStatus pure {
  let small = maximum::<u8>(left: 4_u8, right: 9_u8);
  let signed = maximum::<i64>(left: -7_i64, right: -2_i64);
  if signed != -2_i64 {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: small);
}
```

A generic struct names its parameters the same way. Here `Float` admits the
two float types and `0_T` is the generic zero:

```whitefoot file=generic_struct.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Vector3<T: Float> {
  x: T;
  y: T;
  z: T;
}

fn dot3<T: Float>(left: Vector3<T>, right: Vector3<T>) -> result: T pure {
  let total = 0_T;
  let x_product = fmul.strict(left.x, right.x);
  let after_x = fadd.strict(total, x_product);
  let y_product = fmul.strict(left.y, right.y);
  let after_y = fadd.strict(after_x, y_product);
  let z_product = fmul.strict(left.z, right.z);
  return fadd.strict(after_y, z_product);
}

fn main() -> status: ExitStatus pure {
  let left = Vector3<f32>(x: 1.0_f32, y: 2.0_f32, z: 3.0_f32);
  let right = Vector3<f32>(x: 4.0_f32, y: 5.0_f32, z: 6.0_f32);
  let product = dot3::<f32>(left: left, right: right);
  if feq(product, 32.0_f32) {
    return exit_status(code: 0_u8);
  }
  return exit_status(code: 1_u8);
}
```

Behavior that varies, such as how a container hashes or compares its keys,
is passed as explicit function arguments, grouped under an `interface` and
supplied by a `binding`. There are no methods or traits, and a call always
names its callee:

```whitefoot run exit=0 file=interface.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

interface Source {
  fn value() -> result: u8 pure;
}

fn one() -> result: u8 pure {
  return 1_u8;
}

binding OneSource : Source {
  value = one;
}

fn read<interface Source>() -> result: u8 pure {
  return Source::value();
}

fn main() -> status: ExitStatus pure {
  let value = read::<OneSource>();
  if value == 1_u8 {
    return exit_status(code: 0_u8);
  } else {
    return exit_status(code: 1_u8);
  }
}
```

`read` imports the group with `interface Source` and calls its member by
qualified name. The caller selects the implementation with `OneSource`, and
the compiler specializes `read` for it, so the call is direct and costs
nothing at run time.

## 12. Parallelism from sequential code

A program never asks for parallelism. Because every function states what it
reads and writes, and because the compiler proves which array ranges are
apart, it may run two calls at the same time when it has proved that the
result equals the sequential one. This quicksort has no parallel construct:

```whitefoot file=quicksort.wf
fn quicksort(v: &[u64]) -> result: unit writes(v) {
  let n = v^.len;
  if n <= 1_u64 {
    return unit;
  }
  let p = partition(v: v);
  let after = p + 1_u64;
  let smaller = &v^[0_u64..p];
  let larger = &v^[after..n];
  quicksort(v: smaller);
  quicksort(v: larger);
  return unit;
}

fn partition(v: &[u64]) -> pivot_at: u64 writes(v) contract {
  requires v^.len >= 2_u64;
  ensures pivot_at < v^.len;
} {
  let n = v^.len;
  let last = n - 1_u64;
  let pivot = v^[last];
  let store = 0_u64;
  for (
    i in 0_u64..last,
    invariant behind: store <= i
  ) {
    let x = v^[i];
    if x < pivot {
      let displaced = v^[store];
      set v^[store] = x;
      set v^[i] = displaced;
      set store = store + 1_u64;
    }
  }
  let moved = v^[store];
  set v^[store] = pivot;
  set v^[last] = moved;
  return store;
}
```

The two recursive calls write the ranges `[0, p)` and `[p + 1, n)` of `v`,
and the compiler proves they do not overlap. Compile with `--par` and the
calls may run on idle workers; `--par-ledger` prints each decision with its
reason:

```sh
whitefootc --par --par-ledger sort.wf -o sort
```

A decision the compiler cannot prove stays sequential, and the program is
still accepted: permission to overlap is separate from acceptance. A
counted loop is a site of its own, and its iterations may overlap when each
writes only its own elements. [Write sequential code, get parallel
results](../articles/sequential-code-parallel-results.md) follows the
compiler from the checked rows to the parallel code.

## 13. Concurrency and waiting

Computation keeps its sequential meaning; concurrency is something you ask
for. A function that can wait for the host or for another context says
`waits` after its row, and only a waiting function may call it, so every
place a program can pause is visible in its signatures. `spawn` runs a call in
a context of its own, concurrently with its caller. A `Shared<T>` handle
names an object that several contexts hold, and its value is reached only
inside `atomic`, whose block runs with the object to itself and cannot wait:

```whitefoot file=contexts.wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn tick(value: u64) -> result: u64 pure waits {
  return value;
}

fn count_one(total: Shared<u64>) -> result: unit pure waits {
  atomic count = &total {
    set count^ = count^ +wrap 1_u64;
  }
  return unit;
}

fn start_batch(total: &Shared<u64>) -> result: unit reads(total) waits {
  for @start (index in 0_u64..4_u64) {
    let handle = shared_share::<u64>(shared: total);
    spawn count_one(total: move handle);
  }
  return unit;
}

fn main() -> status: ExitStatus pure waits {
  let pending = spawn tick(value: 41_u64);
  let answer = pending +wrap 1_u64;
  let total = shared_new::<u64>(value: 0_u64);
  start_batch(total: &total);
  let seen = 0_u64;
  atomic count = &total {
    set seen = count^;
  }
  if seen != 4_u64 {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
```

A spawned context ends before the function that started it returns. A
spawn bound by `let` is joined at the first later statement that names the
binding, so `pending` is ready by the time `answer` is computed. A `when`
clause on an `atomic` statement makes it wait until a condition on the
object holds, which is how a bounded queue between a producer and a
consumer is written:

```text
atomic items = &queue when items^.len < items^.cap {
  place_back(window: items, value: index);
}
```

Concurrency promises no failure that one driver would not show, and it does
not promise freedom from deadlock: contexts that wait for each other wait
forever, as they would on one thread. This part of the language is still
growing, so check the [specification](../../spec/kernel-spec.md) (the WAIT
and SHARE rules) for the current statement.

## 14. Modules, packages and the heap

A single source file is a *source bundle*. A larger program is a graph of
modules. The graph is one file, `modules.wfg`, which lists each module and
the modules it may use. Each module's directory holds an interface record,
`module.wfm`, with its public declarations, and implementation files that
repeat each declaration and add its body:

```text
pkg::bank: [];
pkg::app: [pkg::bank];
entry main = pkg::app::main;
```

A module is checked against its own files and the interfaces of the modules
it lists, never against their bodies, so one module can be verified while
the ones it uses are still being written ([MOD-1 to MOD-9](../../spec/kernel-spec.md)).
Another package is bound in the graph by `package name = "location";`, and
the standard library is always available as `std`. The compiler reads the
standard library from its own build, so it always matches the compiler.

The heap is optional. A source bundle that starts with `program no_heap;`,
or an entry that a graph declares with `no_heap`, cannot allocate: the
compiler rejects every heap type and every allocating call it reaches. A
program that fits the rule is accepted:

```whitefoot file=no_heap.wf
program no_heap;

alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let items = slots_new::<u8, 4>();
  place_back(window: &items, value: 9_u8);
  let size = items.len;
  let code = cvt.wrap::<u64, u8>(size);
  return exit_status(code: code);
}
```

and naming a `Box` is rejected (STOR-8):

```whitefoot reject STOR-8 file=no_heap_box.wf
program no_heap;

struct Cell {
  content: Box<u64>;
}
```

Allocation never returns a failure value, because it either succeeds or
the program stops outside the language. A program that cannot tolerate
exhaustion declares that it uses no heap, as above.

## Where next

- [Integers](../articles/integers.md), [Prove it or write a branch](../articles/prove-it-or-write-a-branch.md)
  and [Proofs without a solver, by hand](../articles/proofs-by-hand.md) take the
  arithmetic and proof ideas further.
- [What a rejection tells you](../articles/what-a-rejection-tells-you.md)
  explains the diagnostic format and how to use a repair suggestion.
- [What a reviewer reads](../articles/what-a-reviewer-reads.md) and
  [Write sequential code, get parallel results](../articles/sequential-code-parallel-results.md)
  cover effects and parallelism.
- [Safety claims](../safety-claims.md) states each guarantee with the rules
  and conformance cases it rests on, and what the specification does not
  promise.
- The [specification](../../spec/kernel-spec.md) defines every rule the tour
  cites.
