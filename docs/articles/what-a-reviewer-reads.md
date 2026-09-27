# What a reviewer reads

Reviewing a change mostly means reviewing calls. For each call, the reviewer
asks what it may change, what it needs, and what it promises. Whitefoot puts
the answers in the callee's signature, and the compiler checks the callee's
body against that signature, so the reviewer can answer them without opening
the callee.

One function, declared in three languages:

```text
C          bool transfer(struct account *from, struct account *to, uint64_t amount);
Rust       fn transfer(from: &mut Account, to: &mut Account, amount: u64) -> bool
Whitefoot  fn transfer(from: &Account, to: &Account, amount: u64) -> moved: Bool
             writes(from.balance), writes(to.balance)
```

What each declaration tells the reviewer of a call:

```text
C          from and to may be one account; the call may change anything either
           reaches, and any global
Rust       from and to are two accounts; the call may change anything either
           reaches, and state behind a static
Whitefoot  the call changes at most from.balance and to.balance; arguments
           whose writes could overlap are rejected at the call
```

## 1. The effect row

The part after the result, `writes(from.balance), writes(to.balance)`, is the
function's effect row. Each entry is `reads(path)` or `writes(path)`. Every
path starts at a reference parameter and continues through fields, as in
`from.balance`, and through positions such as an index the caller passes. A
function that reads and writes no state has the row `pure`.

```
struct Account {
  owner: u32;
  balance: u64;
}

fn transfer(from: &Account, to: &Account, amount: u64) -> moved: Bool writes(from.balance), writes(to.balance) {
  let f = deref(from).balance;
  let t = deref(to).balance;
  if amount <= f {
    if t +defined amount {
      set deref(from).balance = f - amount;
      set deref(to).balance = t + amount;
      return True();
    }
  }
  return False();
}
```

There are no mutable globals: a `const` never changes, and everything else a
function can reach arrives through its parameters. The row, together with
the parameters passed by value, is therefore the complete list of what a
call can observe or change.

I/O works the same way. The standard library's functions that read or write
files, sockets and streams take a handle by reference and write it:

```
public fn write_once(factory: &HandleFactory, output: &OutputStream, source: &[u8], start: u64, end: u64) -> result: Result<u64, IoError> reads(source), writes(factory), writes(output) contract {
  requires start <= end;
  requires end <= deref(source).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Writes bytes of source from start toward end to output with one host write; Ok carries the index after the last byte written.";
```

A function that is given no handle cannot do I/O, and a `pure` function
reads and writes no state at all.

To read a call, put the arguments in place of the parameters. The call
`transfer(from: &a, to: &b, amount: 30_u64)` may write `a.balance` and
`b.balance`. `a.owner`, `b.owner` and every other variable of the caller
keep their values, and whatever the caller had proved about them still
holds after the call.

## 2. The row is checked both ways

Every access the body makes must lie under an entry of the row, and every
entry must cover at least one access. A function that reads a balance and
says it is `pure` is rejected:

```
fn balance_of(a: &Account) -> value: u64 pure {
  return deref(a).balance;
}
```

```text
row.wf:22:42: error[EFF-2]: EffectMismatch
  source: fn balance_of(a: &Account) -> value: u64 pure {
  marker:                                          ^^^^
  expected_row: reads(a.balance)
  found_row: pure
  missing: [reads(a.balance)]
  extra: []
  mechanical_fix: declare the row as `reads(a.balance)`, which covers every access the body makes and no other
```

So is an entry that nothing in the body uses:

```
fn balance_of(a: &Account, b: &Account) -> value: u64 reads(a.balance), reads(b.balance) {
  return deref(a).balance;
}
```

```text
extra.wf:22:55: error[EFF-2]: EffectMismatch
  source: fn balance_of(a: &Account, b: &Account) -> value: u64 reads(a.balance), reads(b.balance) {
  marker:                                                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  expected_row: reads(a.balance)
  found_row: reads(a.balance), reads(b.balance)
  missing: []
  extra: [reads(b.balance)]
  mechanical_fix: declare the row as `reads(a.balance)`, which covers every access the body makes and no other
```

A write that the row does not declare is refused at the `set` that makes it
(SET-1), as [What a rejection tells you](what-a-rejection-tells-you.md)
shows.

An entry may be wider than the accesses under it. `writes(a)` is accepted
for a body that writes only `a.balance`, and a reviewer then reads it as
"may write any part of `a`". When a row is rejected, the repair proposes the
narrowest row that covers the body.
The check runs both ways because a row that lists only what the body does
is a statement a reader can rely on without reading the body. A row padded
with extra entries would be a place to hide an effect.

## 3. Arguments that overlap

The body of `transfer` reads both balances before it writes either. If
`from` and `to` were one account, the second write would overwrite the
first, and the account would end up `amount` richer. The call that would do
that is rejected:

```
fn main() -> status: ExitStatus pure {
  let a = Account(owner: 1_u32, balance: 100_u64);
  let ok = transfer(from: &a, to: &a, amount: 30_u64);
  return exit_status(code: 0_u8);
}
```

```text
self.wf:24:12: error[EFF-5]: OverlappingCallEffects
  source:   let ok = transfer(from: &a, to: &a, amount: 30_u64);
  marker:            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  first: a.balance
  second: a.balance
  mechanical_fix: pass places that do not overlap, or pass the shared place through one argument only
```

At every call the compiler substitutes the arguments into the callee's row
and compares the entries that different arguments supply. Two that overlap,
where at least one is a write, must be proved apart. For two elements of one
array, that means proving their indices differ:

```
fn pay(accounts: &[Account], i: u64, j: u64, amount: u64) -> moved: Bool writes(accounts) contract {
  requires i < deref(accounts).len;
  requires j < deref(accounts).len;
} {
  return transfer(from: &deref(accounts)[i], to: &deref(accounts)[j], amount: amount);
}
```

```text
pay.wf:26:10: error[EFF-5]: UndischargedCallSeparation
  source:   return transfer(from: &deref(accounts)[i], to: &deref(accounts)[j], amount: amount);
  marker:          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  residual: deref(accounts)[i].balance and deref(accounts)[j].balance require their captured indices to be distinct
  mechanical_fix: when the two positions can differ here, prove them distinct before this call; otherwise pass places this call proves do not overlap
```

With `if i == j { return False(); }` before the call, `pay` is accepted, and
a program that pays from the first of three accounts to the third runs and
leaves the balances it should. The reviewer sees the same guard the compiler
used. `swap` is the one operation whose two arguments may name the same
place.

## 4. Contracts: what a call needs and what it gives

A contract has two kinds of clause. A `requires` is proved at every call and
is a fact inside the function
([Prove it or write a branch](prove-it-or-write-a-branch.md)). An `ensures`
goes the other way: it is proved at every `return` of the function, and each
caller receives it as a fact.

```
fn clamp(x: u64, hi: u64) -> r: u64 pure contract {
  ensures r <= hi;
} {
  if x <= hi {
    return x;
  }
  return hi;
}

fn pick(table: &[u8], i: u64) -> value: u8 reads(table) contract {
  requires 0_u64 < deref(table).len;
} {
  let last = deref(table).len - 1_u64;
  let j = clamp(x: i, hi: last);
  return deref(table)[j];
}
```

Both are accepted. In `pick`, the subscript `deref(table)[j]` is proved from
`clamp`'s signature alone: `j <= last` from the `ensures`, and
`last == deref(table).len - 1` from the line above. The compiler does not
look into `clamp`'s body at the call, and neither does the reviewer.

Without the contract on `clamp`, the subscript is rejected, and the repair
names the `ensures` as one of the routes:

```text
contract.wf:16:22: error[OP-4]: UndischargedBoundsObligation
  source:   return deref(table)[j];
  marker:                      ^^^
  residual: j < deref(table).len
  disposition: Unproved
  mechanical_fix: `j < deref(table).len` is not proved here: when facts that reach the access imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); when the callee whose result it reads can prove the bound, state it in that callee's `ensures`; or guard the access with `if j < deref(table).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare
```

An `ensures` is not trusted. Promising `r < hi` instead is rejected at the
`return` that does not keep the promise:

```text
promise.wf:8:5: error[FN-9]: UndischargedPostcondition
  source:     return x;
  marker:     ^^^^^^^^^
  concrete_function: clamp
  postcondition: promise.wf:5:3 "ensures r < hi;"
  conjunct: 0
  selector: promise.wf:4:30 "r: u64"
  relation: x - hi <= -1
  disposition: Unproved
  mechanical_fix: the postcondition is not proved where this `return` delivers its value: add a `requires` over the parameters the value is computed from, prove the bound before the return with an `invariant` whose `use` steps name the facts it follows from, or state a postcondition the body proves
```

An `ensures` is one comparison. One side is an integer result, the integer
inside an `Ok` result, or the length of storage the function changes. The
other may also be a parameter, another result, a length or a constant.
Either side may add a constant. It cannot state more than that, so what a
function computes beyond such bounds is still read from its body.

## 5. Interfaces

In a program built from modules, each module has an interface record,
`module.wfm`, that declares its public functions without their bodies:

```
public struct Account {
  public owner: u32;
  public balance: u64;
}

public fn transfer(from: &Account, to: &Account, amount: u64) -> moved: Bool writes(from.balance), writes(to.balance) doc "Moves amount from one account to the other when from holds it and to can take it, and says whether it did.";
```

The implementation repeats each declaration exactly, row and contract
included, and its body is checked against it (MOD-7). Other modules are
checked against the interface alone, never against the implementation
behind it (MOD-8).

The compiler prints a module's interface with every name resolved:

```text
$ whitefootc --graph modules.wfg --render-interface pkg::bank
module pkg::bank
public struct pkg::bank::Account { public owner : u32 ; public balance : u64 ; }
public fn pkg::bank::transfer ( from : & pkg::bank::Account , to : & pkg::bank::Account , amount : u64 ) -> moved : Bool writes ( from . balance ) , writes ( to . balance ) ;
```

and compares it with another revision's. Suppose a change makes `transfer`
clear `from.owner` when an account is emptied. The row of both the
declaration and the implementation must then say `writes(from)`, and the
comparison shows exactly that:

```text
$ whitefootc --graph modules.wfg --compare-interface pkg::bank --against ../v1/modules.wfg
- public fn pkg::bank::transfer ( from : & pkg::bank::Account , to : & pkg::bank::Account , amount : u64 ) -> moved : Bool writes ( from . balance ) , writes ( to . balance ) ;
+ public fn pkg::bank::transfer ( from : & pkg::bank::Account , to : & pkg::bank::Account , amount : u64 ) -> moved : Bool writes ( from ) , writes ( to . balance ) ;
whitefootc: pkg::bank: interface changed
```

The command fails when the interface changed and prints
`pkg::bank: interface unchanged` when it did not. The rendering leaves out
the `doc` text, which means nothing to the compiler.

## 6. What a signature does not say

- **Whether the call finishes, or how long it takes.** `pure` means no state
  is read or written. It promises nothing about termination, because the
  compiler has no termination checker.
- **Whether it allocates.** Allocation has no entry in the row. A program or
  an entry that must not allocate says so with `no_heap`
  ([Beyond memory](beyond-memory.md)).
- **How much stack it uses.**
- **What the standard library's functions do beyond their declarations.**
  Their definitions come with the build, and the declarations, contracts
  included, are trusted.
- **Anything the `doc` text says.**

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) or design |
|---|---|
| The row's grammar: `pure`, `reads`, `writes`, paths from reference parameters | EFF-1 |
| The row is checked both ways against the body | EFF-2 |
| Rows substituted at a call; overlapping writes proved apart | EFF-5 |
| A write needs a declared path | SET-1 |
| Storage is frame, heap or immutable `const`; there are no mutable globals | STOR-1, CONST-2 |
| The standard library's handles and I/O functions | PRE-2 |
| `requires` proved at each call | FN-8 |
| `ensures` proved at each return and published to callers | FN-9 |
| An implementation repeats its interface; other modules see only the interface | MOD-7, MOD-8 |
| `pure` promises no termination; allocation carries no row entry | EFF-3, STOR-8 |
| Why the row is exact | [`design/language/effects.md`](../../design/language/effects.md) |

Every program in this article is accepted, or rejected as shown, by the
compiler at commit `e041772d8`, with `whitefootc --check file.wf`. Each
fragment is compiled inside a file that begins with the `ExitStatus` and
`exit_status` aliases and a blank line. The fragments of sections 2 and 3
follow `Account` and `transfer`, each followed by a blank line. A fragment
without its own `main` is followed by a blank line and a `main` returning
`exit_status(code: 0_u8)`.

The `clamp` without a contract has the same body under the first line
`fn clamp(x: u64, hi: u64) -> r: u64 pure {`. `transfer` with two accounts,
and `pay` with the guard, were also built and run from a `main` that checks
the balances; both exit with 0. The interface example is a module program
with the modules `pkg` and `pkg::bank`: `v1` holds the interface above, and
`v2` holds the changed interface with its implementation. Both build and
run.
