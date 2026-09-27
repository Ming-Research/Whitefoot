# Integers

What does `a + b` mean for two 8-bit values that add up to 260?

```text
C          unsigned char operands are promoted to int, so a + b is 260;
           for signed int an overflow is undefined behavior
Rust       a panic in a debug build; 4 in a release build, unless overflow
           checks are turned on
Whitefoot  the program does not compile
```

In Whitefoot, `a + b` means the exact sum and nothing else, and it is
accepted only where the compiler has proved that the sum fits the type. Every
other meaning a program might want, wrapping around, clamping, reporting the
overflow, has its own spelling. This article goes through them, and ends
with what each compiles to.

## 1. Eight types and no silent conversions

The integer types are `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32` and
`u64`. Both operands of an operation have the same type, and the result has
that type too. There is no promotion and no implicit widening:

```
fn f(a: u8, b: u32) -> r: u32 pure {
  return a +wrap b;
}
```

```text
mixed.wf:5:18: error[TYPE-5]: TypeMismatch
  source:   return a +wrap b;
  marker:                  ^
  expected: own u8
  found: own u32
```

A literal names its type, as in `1_u64` or `-128_i8`, and its value must lie
in that type's range. A bare `1` is not a literal at all:

```text
untyped.wf:5:11: error[FORM-5]: UnexpectedToken
  source:   let x = 1;
  marker:           ^
  expected: [IDENT, TYPEID, "pkg", "std", "&", "entry", "move", "if", "propagate", "match", literal, "musttail", OPNAME, "deref"]
  found: "1"
```

## 2. One spelling per meaning

Addition has five spellings. For `u8`, with `a = 250_u8` and `b = 10_u8`:

| Spelling | Meaning | Type | Here |
|---|---|---|---|
| `a + b` | the exact sum, accepted only where it is proved to fit | `u8` | rejected |
| `a +defined b` | whether the exact sum fits | `Bool` | false |
| `a +wrap b` | the sum modulo 2^8 | `u8` | 4 |
| `a +checked b` | `Ok` with the sum, or `Err(error: Overflow())` | `Result` | `Err` |
| `a +sat b` | the sum clamped to the type's range | `u8` | 255 |

The last four columns come from running this program, which exits with 0:

```
fn main() -> status: ExitStatus pure {
  let w = 250_u8 +wrap 10_u8;
  if w != 4_u8 {
    return exit_status(code: 1_u8);
  }
  let s = 250_u8 +sat 10_u8;
  if s != 255_u8 {
    return exit_status(code: 2_u8);
  }
  match 250_u8 +checked 10_u8 {
    Ok(value: v) => {
      return exit_status(code: 3_u8);
    }
    Err(error: e) => {
    }
  }
  if 250_u8 +defined 10_u8 {
    return exit_status(code: 4_u8);
  }
  return exit_status(code: 0_u8);
}
```

The exact form is rejected before the program runs. Here the compiler can
show the goal is false, so the disposition is `Refuted`, and the repair asks
for other operands or another form:

```text
exact.wf:5:11: error[OP-2]: UndischargedIntegerDomainObligation
  source:   let x = 250_u8 + 10_u8;
  marker:           ^^^^^^^^^^^^^^
  residual: 250_u8 +defined 10_u8
  disposition: Refuted
  mechanical_fix: the operands that reach this operation make `250_u8 +defined 10_u8` false, so the exact operation cannot execute here: change the operands or their type, or write the `+wrap`, `+checked` or `+sat` form for the result the program intends
```

`+defined` is a question, not an addition. It computes whether the exact sum
would fit, and an `if` on it makes the exact `+` provable in its then-block.
[Prove it or write a branch](prove-it-or-write-a-branch.md) shows how the
exact forms are proved.

Subtraction and multiplication have the same five forms. The other
operations have the forms that make sense for them:

| Operations | Forms |
|---|---|
| `+`, `-`, `*` | exact, `defined`, `wrap`, `checked`, `sat` |
| `/`, `%` | exact, `defined`, `checked` |
| `ineg`, `iabs` (signed types only) | exact, `defined`, `wrap`, `checked` |
| `ishl`, `ishr` | exact, `defined`, `wrap` |

## 3. Division has no wrap

Division by zero has no answer modulo 2^K, so there is no wrapping division;
`/wrap` is not even a token:

```text
divwrap.wf:5:12: error[GRAM-1]: UnclassifiedToken
  source:   return n /wrap d;
  marker:            ^^^^^
  found: "/wrap"
```

The exact `n / d` needs `d != 0`, and for a signed type also that the pair is
not the type's minimum divided by `-1`, whose quotient does not fit. The
checked form tells the two failures apart: `-128_i8 /checked -1_i8` returns
`Err(error: DivOverflow())`, and a zero divisor returns
`Err(error: DivideByZero())`. Division truncates toward zero, and a nonzero
remainder has the sign of the dividend.

## 4. Shifts, negation and absolute value

These are named operations with positional arguments. A shift amount is a
`u32`. The exact shift needs the amount to be smaller than the width; the
wrapping shift uses the amount modulo the width:

```
fn f(x: u64, k: u32) -> r: u64 pure {
  let a = ishl(x, 3_u32);
  let b = ishl.wrap(x, k);
  return a +wrap b;
}
```

Both are accepted: `3 < 64` is proved from the literal, and `ishl.wrap` has
no goal. An unknown amount with the exact form is rejected with the usual
routes, the last of which is the wrapping form:

```text
shiftk.wf:5:10: error[OP-2]: UndischargedIntegerDomainObligation
  source:   return ishl(x, k);
  marker:          ^^^^^^^^^^
  residual: ishl.defined(x, k)
  disposition: Unproved
  mechanical_fix: add `requires ishl.defined(x, k);` to the `contract` of `f`, which each caller then establishes; or guard the operation with `if ishl.defined(x, k)` where skipping it is the intended behavior; or write the `ishl.wrap` form
```

`ineg` and `iabs` exist only for signed types. Their exact forms exclude the
minimum value, whose negation does not fit; `ineg.wrap(-128_i8)` is `-128_i8`.

## 5. Signed and unsigned come from the type

The spelling of a comparison or shift is the same for signed and unsigned
types, and the operand type decides its meaning: `<` compares as signed
numbers for `i32` and as unsigned numbers for `u32`, and `ishr` is an
arithmetic shift for signed types and a logical shift for unsigned ones.
Because both operands must have the same type, there is no case where the
two readings compete.

## 6. Conversions

A conversion is written, and it names both types:

| Spelling | Meaning |
|---|---|
| `cvt::<Src, Dst>(x)` | the same value in `Dst`, accepted only where it is proved to fit |
| `cvt.defined::<Src, Dst>(x)` | whether it fits |
| `cvt.checked::<Src, Dst>(x)` | `Ok` with the value, or `Err(error: NarrowError())` |
| `cvt.wrap::<Src, Dst>(x)` | integers only: the value modulo 2^K of `Dst` |
| `reinterpret::<Src, Dst>(x)` | the same bits, for example an `f32` as a `u32` |

Of the 100 ordered pairs of numeric types, 39 always fit, such as `u8` to
`u64`, and `cvt` between them needs no proof. The other 61 depend on the
value: `cvt::<i32, u32>(x)` is accepted under `requires 0_i32 <= x;`, and
`cvt::<u64, u8>(x)` needs its value proved below 256. With `x = 300_u64`,
`cvt.wrap::<u64, u8>(x)` is `44_u8` and `cvt.checked::<u64, u8>(x)` is an
`Err`.

## 7. What each spelling compiles to

Each row is the body of a function that adds two 8-bit parameters, `u8` or,
for the signed row, `i8`; the exact rows' functions have requirements that
bound their operands. The compiler emits this LLVM IR:

| Spelling | LLVM IR |
|---|---|
| `a + b`, unsigned, proved | `add nuw i8 %v0, %v1` |
| `a + b`, signed, proved | `add nsw i8 %v0, %v1` |
| `a +wrap b` | `add i8 %v0, %v1` |
| `a +sat b` | `call i8 @llvm.uadd.sat.i8(i8 %v0, i8 %v1)` |
| `a +checked b` | `call { i8, i1 } @llvm.uadd.with.overflow.i8(i8 %v0, i8 %v1)`, and the overflow bit selects `Ok` or `Err` |
| `a +defined b` | the same intrinsic, keeping only its overflow bit |

The exact form is the only one that carries a no-wrap flag, `nuw` or `nsw`.
The flag promises LLVM that the addition does not overflow, which the
optimizer may use to simplify the code around it. The compiler can make that
promise because it has proved it. The wrapping form carries no flag, because
overflow is part of its meaning.

## 8. The same meaning in every build

There is one build. `a +wrap b` wraps and `a + b` is proved in the program
you test and in the program you ship, and no flag changes that. There is no
form that stops the program on overflow either: a spelling such as `.trap`
does not exist.

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) |
|---|---|
| The primitive types | TYPE-1 |
| No implicit conversions; both operands of one type | TYPE-4, TYPE-5 |
| Literals carry their type and fit its range | FORM-5, FORM-7 |
| Every computation names one operation; nothing is overloaded | OP-1 |
| The integer forms, their goals and results; division and shifts | OP-2 |
| Conversions and their domains | OP-6 |
| Operation names; signedness from the operand type | OP-7 |
| Saturation, shifts and the other lowerings | OP-8 |

Every program in this article is accepted, or rejected as shown, by the
compiler at commit `e041772d8`. The two programs with a `main` were also
built and run, and both exit with 0. The IR is `whitefootc --emit-llvm`
output before optimization; the fragments are compiled inside a file with
the standard `ExitStatus` aliases and a `main`.
