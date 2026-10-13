# Decimal text of binary64 values

## Question and scope

Whitefoot's standard library has no conversion between `f64` and decimal text.
Firn (the Redis-compatible server) and Halo (the Lua 5.1 engine) each carry
their own: Firn-wf's `firn/scores/decimal.wf`, `read.wf` and `write.wf`
(1,226 lines, also kept in this repository as `apps/firn/scores/` for the
compiler's program tests), Halo-wf's `lib/halo/number/decimal.wf`, `read.wf`
and `format.wf` (1,297 lines) and a third copy for JSON in
`lib/json/number/` (681 lines). The two `decimal.wf` files are 409 lines each
and nearly identical. Firn now targets Redis 7.2.16, which writes doubles
with a shortest-digit algorithm rather than `%.17g`, so each copy would grow
a third algorithm.

This investigation designs, without implementing, the standard library
operations the board item asks for:

1. read decimal text as `f64`, rounded to nearest with ties to even,
   reporting text that is not a number and results out of range;
2. write the shortest decimal that reads back to the same `f64`, into a
   caller-supplied buffer, with integral values of magnitude up to 2^62
   written as integers, as Redis 7.2.16 does;
3. write with N significant digits as C's `%.Ng` does (Firn uses 17, Halo 14).

It asks where the code should live (checked Whitefoot or the trusted C
runtime), which algorithms, what the language must express, what the
specification would say, which cases and oracles decide correctness, and
which choices belong to the owner. Infinity and NaN text, hexadecimal float
text and 80-bit `long double` (Firn's `INCRBYFLOAT`) stay with the callers;
`f32` is out of scope.

Evidence used: the active specification (v0.134), the standard library
sources under `lib/std`, the design-tree nodes
`language/standard-library`, `language/numeric-conversion`,
`language/surface-form/float-literals` and `compiler` (runtime), the four
downstream copies above, and two scripts in this directory:

- [reference.py](reference.py) computes every proposed result from its
  definition below with exact rational arithmetic, independent of any
  digit-generation algorithm. `python3 -I reference.py check 5000 3` compares
  it with Python's correctly rounded `repr` and `%.Ng` (David Gay's dtoa):
  0 differences on 5,000 random bit patterns at 1, 6, 14 and 17 digits and
  shortest; an earlier ad hoc run of the same functions found 0 on 6,008
  inputs including the edge values.
- [fpconv_reconstruction.py](fpconv_reconstruction.py) models Redis 7.2's
  double-to-text path (`fpconv_dtoa`, `d2string`, the Lua argument
  conversion), **written from memory because no Redis source is available
  offline**. Its cached-power table reproduces the first entry of Grisu's
  published table (`0xfa8fd5a0081c0288`, exponent -1220) and every output
  round-trips, but its results are a lead, not evidence; experiment E1 below
  replaces it with the real code.

## What Redis 7.2.16 does, and how sure this is

Recalled from the Redis 7.2 sources (`src/util.c`, `src/util.h`,
`src/script_lua.c`, `deps/fpconv`, `deps/lua/src/luaconf.h`); confidence on
a 1-5 scale, 5 when certain.

| Path | Behaviour | Confidence |
|---|---|---|
| Double replies (`ZSCORE`, `ZINCRBY`, RESP2 and RESP3) | `d2string`: `nan`; `inf`/`-inf`; zero as `0` or `-0` by its sign; else, when `double2ll` accepts the value, the integer's decimal digits; else `fpconv_dtoa` | 4 |
| `double2ll` | rejects values below `(double)(-LLONG_MAX/2)` or above `(double)(LLONG_MAX/2)`, both of which are ±2^62, then accepts when the value converted to `long long` and back is unchanged. So integral values with \|x\| <= 2^62 qualify, **including ±2^62** (the board item says "< 2^62") | 3 |
| Numbers passed from Lua to `redis.call` | `double2ll` then `fpconv_dtoa`, with no zero case, so `-0.0` is written `0` | 4 |
| `fpconv_dtoa` | night-shift's `fpconv`, a Grisu2 port with no fallback: optional `-`, `0` for zero, `inf`/`nan` for non-finite, else Grisu2 digits laid out by `emit_digits` | 4 that it is Grisu2 without fallback; 3 that Redis left `emit_digits` unchanged |
| `emit_digits` layout, digits D (n of them) times 10^K | plain integer `D` and K zeros when 0 <= K <= 7; plain decimal when -7 < K < 0, or K < 0 and the leading digit's exponent is within ±3; otherwise `d.ddde±X` with the exponent unpadded (`1e-7`, `1e+21`, `1.5e+300`) | 3 |
| Lua's `tostring` and number-to-string coercion | `LUA_NUMBER_FMT` `"%.14g"` through the C library | 5 |
| Reading scores and Lua numbers | C `strtod` (glibc), correctly rounded for decimal text; Redis 8.0, not 7.2, added `fast_float` | 3 |
| Licence of Redis 7.2.x | BSD-3-Clause (the licence change came with 7.4) | 4 |

**Finding: Redis 7.2's "shortest" output is Grisu2's, which is not always the
shortest.** Grisu2 searches a rounding interval it narrows conservatively and
never includes the interval's ends, and without Grisu3's failure detection
and fallback it returns a longer string when it cannot decide. An algorithm
with no fallback cannot always produce the shortest digits, so this holds
whatever the details. With the reconstruction (`python3 -I
fpconv_reconstruction.py compare 40000 9`), Grisu2's digits differ from the
exact shortest digits, both in fpconv's layout, on:

| Inputs (40,000 each) | Differ |
|---|---|
| Random bit patterns of finite doubles | 50 (0.13%) |
| 6-digit decimals between 1e-16 and 1e10 | 31 (0.08%) |
| Uniform in [0, 1) | 17 (0.04%) |
| 3-digit decimals between 1e19 and 1e25 | 4,752 (12%) |

The last family is common because a short decimal there often lies exactly
halfway between two doubles, and ties-to-even maps it to the even one: such a
value is the end of its own rounding interval, which Grisu2 excludes. Examples
(reconstruction): `1e23` replies `99999999999999990000000` where the shortest
is `1e+23`; `9.66e21` replies `9660000000000001000000` where the shortest is
`9.66e+21`.

So no correctly specified "shortest" operation can match Redis 7.2.16 byte
for byte, and the board item's completion condition ("the shortest output
equals Redis 7.2.16's formatting function on the same inputs") cannot be met
by the operation it names. Card 1 below asks how to resolve this.

## 1. Interface

A new standard library module `std::float_text`, registered by the graph row
`pkg::float_text: [];`. It follows the existing idiom of `std::text`
(`host_copy_bytes`): a caller-supplied `&[u8]` window `[start, end)`, `Ok`
carrying the index after the last byte written, and a `TooSmall` error
carrying the length the text needs. The hard part, choosing digits, is
separated from the easy part, laying them out:

```
public enum NonFinite {
  Infinite(public negative: Bool);
  NotANumber(public negative: Bool);
}

public struct FloatDigits {
  public readonly negative: Bool;
  public readonly digits: u64;
  public readonly count: u64;
  public readonly exponent: i64;
  invariant count_floor(d): 1_u64 <= d.count;
  invariant count_ceiling(d): d.count <= 17_u64;
  invariant digits_ceiling(d): d.digits <= 99999999999999999_u64;
}

public enum ReadError {
  ReadSyntax();
  ReadOverflow(public value: f64);
  ReadUnderflow(public value: f64);
}

public enum WriteError {
  WriteTooSmall(public required: u64);
}

public const float_text_max: u64 = 24_u64;

public const integer_text_max: u64 = 20_u64;

public fn f64_read(input: &[u8], start: u64, end: u64) -> value: Result<f64, ReadError> reads(input) contract {
  requires start <= end;
  requires end <= input^.len;
} doc "Reads the bytes from start to end as a decimal number, rounded to the nearest f64 with ties to even; ReadSyntax when they are not one, ReadOverflow or ReadUnderflow carrying the rounded infinity or zero when a nonzero number rounds to one.";

public fn f64_shortest(value: f64) -> digits: Result<FloatDigits, NonFinite> pure doc "The fewest decimal digits that read back as value, the nearest to value of those, or Err for an infinity or NaN.";

public fn f64_significant(value: f64, count: u64) -> digits: Result<FloatDigits, NonFinite> pure contract {
  requires 1_u64 <= count;
  requires count <= 17_u64;
} doc "Value rounded to count significant decimal digits with ties to even, or Err for an infinity or NaN.";

public fn digits_write(digits: FloatDigits, precision: u64, destination: &[u8], start: u64, end: u64) -> next: Result<u64, WriteError> writes(destination) contract {
  requires 1_u64 <= precision;
  requires precision <= 17_u64;
  requires start <= end;
  requires end <= destination^.len;
  ensures when Ok(value: after): start <= after;
  ensures when Ok(value: after): after <= end;
} doc "Writes digits in C's %g layout at precision from start; Ok carries the index after the text, WriteTooSmall the length the window needs.";

public fn i64_write(value: i64, destination: &[u8], start: u64, end: u64) -> next: Result<u64, WriteError> writes(destination) contract {
  requires start <= end;
  requires end <= destination^.len;
  ensures when Ok(value: after): start <= after;
  ensures when Ok(value: after): after <= end;
} doc "Writes value in decimal from start; Ok carries the index after the text, WriteTooSmall the length the window needs.";
```

How the three requested operations are composed:

| Request | Calls |
|---|---|
| Read decimal text | `f64_read`; a caller that, like Redis's score parser, refuses overflow and nonzero underflow treats both errors as refusals, and one that, like `strtod`, accepts them takes the carried value |
| `%.17g` (Firn), `%.14g` (Halo) | `f64_significant(value, 17)` then `digits_write(digits, 17, ...)`; the `Err` arm writes the caller's own infinity and NaN text (`inf`, `-inf`, `nan`, `-nan` as glibc prints them) |
| Shortest | `f64_shortest(value)` then `digits_write(digits, 17, ...)`, which lays the digits out exactly as `%.17g` would lay out the same digits |
| Integral values up to 2^62 as integers | `cvt.checked::<f64, i64>(value)`, a bound test against 2^62 and `i64_write`; this writes `-0.0` as `0`, as Redis's Lua argument path does |

Decisions inside this interface, each open to the owner:

- **Digits as a value, layout separately.** Every finite binary64 needs at
  most 17 significant digits, so `FloatDigits` fits in four words with no
  buffer, `pure`, and the digit functions need no window contract. A caller
  with another layout (Redis's `emit_digits`, JSON, Lua) reuses the digits.
  The alternative, one writer per requested format, repeats the window
  contract three times and hides the digits.
- **No integer option inside the float writer.** The 2^62 bound is
  `LLONG_MAX/2`, a Redis constant; composing it from `cvt.checked` and
  `i64_write` costs the caller a few lines and keeps the bound visible.
  `i64_write` is needed anyway: the standard library writes no integer text
  either.
- **Non-finite values as `Err`.** Proof facts relate integer terms only
  ([ENT-2]'s fragment types are the integer types), so a `requires` that the
  value is finite could not be discharged by any caller; `Result` makes the caller choose its
  own `inf`/`nan` spelling, which differs between Redis and Lua.
- **Precision limited to 1 to 17.** 17 digits identify every binary64 value
  and both consumers use 14 or 17; larger precisions need the exact
  arbitrary-length expansion (up to 767 significant digits), which Stage 1
  below has anyway, so the limit can be lifted when a consumer asks.
- **Whole-window reading.** `f64_read` reads exactly the window, not a
  prefix as `strtod` does with its end pointer; leading or trailing spaces,
  `inf`, `nan` and hexadecimal text are the caller's grammar.
- **Buffer sizing.** `float_text_max` (24 bytes, for example
  `-2.2250738585072014e-308`) bounds every `digits_write` text and
  `integer_text_max` (20 bytes, `-9223372036854775808`) every `i64_write`
  text, so a caller allocating those never sees `WriteTooSmall`; a caller
  with a smaller window gets the needed length, as with `CopyTooSmall`.

## 2. Where it lives and how it is computed

### Whitefoot, not the C runtime

The design tree already places it: standard library modules other than the
six host modules are Whitefoot source checked like any module
(`language/standard-library`), and the runtime is hand-reviewed C reserved
for the scheduler, completion runtime and exhaustion floor (`compiler`). The
board's runtime-verification item (C runtime reduced to primitives Whitefoot
cannot express) points the same way. A C implementation would add
unverified code to every program for no capability Whitefoot lacks: the
conversion needs only integer arithmetic, constant tables and byte windows.
Calling the host C library instead (`strtod`, `snprintf`) would also make
results depend on the host (MSVC's and older C libraries' rounding has
differed from glibc's) and still leave shortest output unsolved, since C has
no shortest-digits function.

### Algorithms

| Operation | Candidates | Needs |
|---|---|---|
| Read | Clinger's fast path (at most 15 digits and \|exponent\| <= 22: one exact `fmul.strict`/`fdiv.strict` by a power of ten); Eisel-Lemire (one 64x128-bit product with a table of 1,302 64-bit words of powers of five; declines a few inputs); Simple Decimal Conversion, SDC (an 800-digit decimal shifted by powers of two, exact with a "nonzero digit dropped" flag; no table, no big integers); Clinger's big-integer comparison | fast path: 23 `f64` constants. Eisel-Lemire: 64x64 to 128-bit product. SDC: an 800-byte array. Big integers: a fixed limb array and long division |
| Shortest | Ryu (2018, proof in the paper); Schubfach (Giulietti 2020, proof in the paper, used by JDK 19+); Dragonbox (fastest, most cases); Grisu2 (not always shortest, see above); Grisu3 (needs an exact fallback); exact search over SDC decimals, as Go's `roundShortest` did before Ryu | Ryu/Schubfach/Dragonbox: 64x128-bit product, a table of about 617 128-bit powers of ten (about 10 KB). SDC search: the SDC machinery only |
| N significant digits | Exact expansion by SDC (what Firn and Halo do now); Ryu-printf (about 100 KB of tables); Go's `ryuFtoaFixed64` (exact for up to 18 digits, shares Eisel-Lemire's table) | as above |

Recommended plan:

- **Stage 1, correctness:** SDC for all three operations, plus Clinger's fast
  path for reading. One mechanism, no large table, smallest proof surface,
  and it is what the downstream copies already run, validated by Firn on
  millions of inputs against Redis 7.0.15. Its cost grows with the decimal
  exponent: writing 1e300 shifts a decimal of hundreds of digits.
- **Stage 2, speed, only when a measurement asks:** Schubfach for shortest
  and Eisel-Lemire for reading, each tested against Stage 1 and the oracle.
  Schubfach is chosen over Ryu for its short loop-free core and over
  Dragonbox for its smaller case analysis; Ryu is an equal alternative if a
  port with a usable licence is wanted. Neither consumer needs the speed now:
  Firn's and Halo's shortest output must follow Redis's Grisu2 anyway
  (card 1), and most scores take Clinger's fast path.

### What the language must express, and witnesses to run first

Every operation needed exists in v0.134:

- A 64x64 to 128-bit product is `*wrap` for the low word and `imulhi` for the
  high word ([OP-1], [OP-8]); `imulhi` has an interval row in the fact table,
  but for two full-width `u64` words that row establishes nothing (E2, w1b
  below).
- Tables are `const` arrays of `u64` or of a const-eligible struct
  ([CONST-2]), indexed after a range proof; element ranges are derived
  from the constant ([ENT-3] S9).
- Bits of an `f64` come from `reinterpret::<f64, u64>`; `f64` results are
  built with `reinterpret::<u64, f64>`, and the fast path uses
  `fmul.strict`/`fdiv.strict`, which round exactly as IEEE 754 requires.
- The digit count invariant of `FloatDigits` is a range type invariant
  ([TYPE-11]).

No language gap was found by reading; E2 found three: in the `imulhi` fact
row, in bounding a quotient by its divisor, and in fields reached through a
reference (see
[E2 results](#e2-results-2026-10-12)). The open question is the proof burden,
which reading cannot settle: the downstream copies never prove their
bounds; they guard them at run time (`if index < decimal^.digits.len`, a
byte dropped silently when a 32-byte text is full) and use `+wrap` where an
exact operation would need a proof. A standard library version must not:
each such guard either goes with a proof or is reported as a gap
(owner-wide rule: a gap is never routed around with a run-time check). Before
Stage 1, experiment E2 compiles four witness programs in CI:

- W1: the 128-bit product and a table lookup indexed by
  `floor(e * log10(2))` computed as `ishr(e * 1262611_i64, 22_u32)` for
  `e` from the exponent field, with the index proved below the table length.
- W2: the SDC left shift over `Array<u8, 800>` with `count <= 800` as a type
  invariant and every subscript proved, no guard.
- W3: writing a computed-length text into `destination[start..end)` after
  one `end - start >= length` test, every subscript proved.
- W4: the decimal-text scanner over the input window.

A witness the checker refuses in its natural form becomes a language-gap
item with that program as its minimal witness, before the implementation
continues.

### E2 results (2026-10-12)

Dated measurements from three CI runs of the temporary witness job: run
38303849577 on branch head ffd67eedfa75, run 38306103794 on 9c592f2b78dc and
run 38306957238 on ed7f0e5ac2d9. All three use the compiler and
specification of the branch's base 51c5f4263f51 (specification v0.134; the
branch changes neither). The generator `witnesses/generate.py` writes the
programs and states each prior prediction (`expected`); W2 is split into the
left shift (w2a), the right shift (w2b) and the move toward the end (w2c),
and W1 and W3 into two files each, so one refusal does not hide the next.
Witnesses added after a run isolate what that run refused. `run` is the
compiled program's exit status, 0 meaning every value matched the one
computed in Python.

| Witness | Expected | Third run (38306957238) | Earlier runs | Disposition |
|---|---|---|---|---|
| w1a table lookup | accept | accepted, run 0 | first run: FORM-3 at `let entry = power_table[index];` | authoring error, fixed |
| w1b product and scale | reject | rejected, FN-9 `ensures high <= left;` in `product` at its return | first run: the same FORM-3; second run predicted accept, refused as now | gap: `imulhi` relations |
| w1c high word bound | reject | rejected, FN-9 `ensures result <= left;` at `return high;` | - | that gap alone, as predicted |
| w2a left shift | open | accepted, run 0 | the same | - |
| w2b right shift | reject | rejected, OP-6 at `cvt::<u64, u8>(quotient)` | the same | gap: quotient bound, as predicted |
| w2c front shift | reject | rejected, FN-9 (the `count <= 800` type invariant) at `return unit;` | the same | gap: field without an affine image |
| w2d front shift, bound restated | accept | accepted, run 0 | the same | the restated form is accepted |
| w2e quotient bound | reject | rejected, FN-9 `ensures result <= 9_u64;` | the same | quotient gap alone, as predicted |
| w2f field bound | reject | rejected, OP-2 at `let total = count + delta;` | second run: the same, predicted at the return | field gap, refused earlier than first predicted |
| w2g field bound with type invariant | reject | rejected, OP-2 at `let total = count + delta;` | - | predicted at the return: wrong |
| w2h field sum without postcondition | reject | rejected, OP-2 at `let total = count + delta;` | - | as predicted |
| w3a window write | open | accepted, run 0 | the same | - |
| w3b index write | open | accepted, run 0 | the same | - |
| w4 scan | open | accepted, run 0 | the same | - |

The w2c and w2d `main` tests each decimal's count against the room the move
needs before calling it: `append_digit` publishes nothing about the count,
so the call's requirement could not otherwise be proved in `main` (read, not
observed). The test is in the test driver only and changes nothing in
`shift_front`; w2f, w2g and w2h test their counters the same way.

**w1a.** `entry` is a fixed grammar atom (the graph file's `entry_decl` and
the `entry(...)` place base), and [FORM-3] excludes every such spelling from
IDENT, so `let entry` did not parse. With the binder renamed `row`, w1a is
accepted: the index `floor(e * log10(2)) + 324` is proved inside the
617-entry table from the exponent field alone.

**w1b: a gap in the `imulhi` fact row.** `product` returns
`imulhi(left, right)` as the high word and states `ensures high <= left;`,
which holds for every pair of words because the other factor is below 2^64.
The [ENT-3] S7 row for `imulhi` states exactly this relation (`r <= a` and
`r <= b` for nonnegative operands), but S7 computes every row in checked
`i128` arithmetic and establishes a row's bounds and relations only when each
of its computations is representable. The row's interval is the corner hull
of `floor(a * b / 2^64)`, and for two full-width `u64` operands the corner
product `(2^64 - 1)^2` exceeds the `i128` range, so the row establishes
nothing, not even the relation that needs no arithmetic. The checker does
the same: its row computes the hull with a checked multiplication and drops
the relations when that fails. The row therefore says nothing in exactly the
case `imulhi` exists for, 64-bit words with no narrower bound. The range
judgment's own reading of S7 rows ([RANGE-2]) takes a row's relations where
the row's and the relation's conditions hold, with no representability
condition, so the two rules disagree on when these relations hold. A writer
could pass the checker by adding `requires left <= 2^53` to `product`, which
keeps the product within `i128`, but that narrows a general helper to one
caller's range to get around the arithmetic; it stays in its natural form.
The minimal witness w1c is refused as predicted:

```wf
fn high_word(left: u64, right: u64) -> result: u64 pure contract {
  ensures result <= left;
} {
  let high = imulhi(left, right);
  return high;
}
```

Closing it needs the relations of a row to depend only on the row's
condition (here both operands nonnegative), not on the representability of
its interval, which alone admits `high_word`; and, to restore the high
word's interval as well, the `imulhi` hull computed so that its corner
products cannot overflow (each corner's result lies within the operand type,
so only the intermediate product needs wider arithmetic). Which form that
takes is the owner's decision.

**w2b: a gap in bounding a quotient by its divisor.** Each quotient digit is
`ishr(remainder * 10 + digit, shift)` with `remainder < 2^shift`, so it is
below ten, and storing it as a `u8` digit needs that bound (here only its
`<= 255` part, for the conversion; the digit type invariant needs `<= 9`).
No rule derives it. The [ENT-3] S7 row for `ishr` gives only the interval
hull of `floor(a / 2^s)` over the operand intervals and `r <= a`; with `s`
anywhere in `0..60` the hull is the dividend's own range. Nothing relates the
shift's result to the live value `ishl(1, shift)`, and even the exact
division row, which captures the images of `q = a / d`, yields only
`d * q <= a` at a later runtime product. Deriving `q <= c - 1` from
`a <= c * d - 1` divides by the symbolic `d`, which no certificate can do:
[PRF-1]'s integer tightening divides only by the two constant factors
[ENT-6] fixes, and a named multiplicity only multiplies. The checker matches
the specification here (its S7 shift row is the interval hull plus
`r <= a`). Stating the strongest natural loop invariant, `remainder <= mask`
instead of the witness's `remainder <= 2^60 - 1`, leaves the gap unchanged.
The minimal witness w2e is refused as predicted, at its postcondition:

```wf
fn leading_digit(total: u64, divisor: u64) -> result: u64 pure contract {
  requires 1_u64 <= divisor;
  requires divisor <= 1152921504606846976_u64;
  requires total < 10_u64 * divisor;
  ensures result <= 9_u64;
} {
  let quotient = total / divisor;
  return quotient;
}
```

The shift form adds a second missing piece: `ishr` by a variable amount
captures no division image at all. Closing the gap needs both: a fact
relating a quotient to its divisor without a runtime product
(`d * q <= a < d * q + d` for unsigned `/`, and for `ishr(a, s)` with the
divisor `ishl(1, s)` bound), and a certificate step that cancels a named
value proved positive, so that `d * q <= c * d - 1` with `d >= 1` gives
`q <= c - 1`. Which form those take is the owner's decision; the witness
stays in its natural form until then.

**w2c, w2f, w2g, w2h: a gap in fields reached through a reference.** What is
established: ordinary entailment gives an integer field reached through a
reference, such as `counter^.count`, no affine image. [ENT-6] gives images
to bindings and measures only, so the [ENT-3] S4 affine form of a
requirement over `counter^.count + delta` is not admitted, and [INV-1]
refuses such a field as an invariant term, so a writer cannot state a bound
over it either. Two consequences are observed. A requirement over
`counter^.count + delta` gives the local copy `let count = counter^.count;`
no affine fact, so `count + delta` is not proved to fit in `u64` (w2f, w2g,
w2h, OP-2). And after `set counter^.count = total;` the field is equal to
`total` only in the L0 domain, where `total` has no upper bound, so a
postcondition or type invariant over the stored field is not proved (w2c,
FN-9). The checker matches the specification here (its affine value of a
place with any field step is none).

Why `shift_front` proves the same overflow, and w2d's restated
`invariant fits: total <= 800_u64;`, when `advance` does not: reading the
specification and the checker answers it, and all four observations fit,
though no witness yet isolates it. `shift_front` takes part in the range
judgment ([RANGE-2]) because `Decimal` has a range type invariant (the
digits are at most nine), which makes the function have a range requirement;
`Counter` has none, so `advance` does not take part, with or without the
plain `count <= 800` invariant w2g added. In a function that takes part, an
OP-2 obligation or a local invariant that ordinary entailment leaves
unproved is owed to the range judgment, whose walk names a field read
exactly, so the copy and the field are one value, and holds the function's
affine requirements as path conditions; it proves the overflow and `fits`.
An FN-9 relation is not owed to it, which is why w2c is still refused at the
return. The requirement is written the same way in both programs, over the
parameter-rooted place the copy reads, the copy is the first read in both,
and the field is `u64` in both, so none of those explains the difference;
the w2g prediction (that the plain type invariant was the difference) was
wrong. A witness that adds a range type invariant to `Counter` would move
w2f's refusal to the return if this account is right. A finding in passing:
whether `count + delta` is accepted thus depends on whether the function has
an unrelated range invariant.

w2d shows a writer can restate the bound as `fits` where the function takes
part in the range judgment; w2h shows nothing can be written where it does
not. The minimal witness is w2h:

```wf
struct Counter {
  doc "One count.";
  count: u64;
}

fn advance(counter: &Counter, delta: u64) -> result: unit writes(counter) contract {
  requires counter^.count + delta <= 800_u64;
} {
  doc "Adds delta to the count; the requirement bounds the sum that is stored.";
  let count = counter^.count;
  let total = count + delta;
  set counter^.count = total;
  return unit;
}
```

The change that would close the gap in ordinary entailment gives such a
place a current affine image as a measure already has one: its entry image
is the one the requirement's premise names, a copy read from it takes that
image, a [SET-1] commit sets it to the committed value's image, the same
[ENT-5] writes that kill its L0 facts kill it, and [INV-1] admits it as a
term. `count + delta` and the postcondition over the field then both read
the requirement's sum. Which form that takes is the owner's decision.

## 3. Licences of ported code

The recommended plan ports no foreign code:

- SDC, Clinger's fast path, Schubfach and Eisel-Lemire are implemented from
  their papers; tables are generated by exact integer arithmetic (a generator
  in Python checks the committed constants in the tests).
- Firn's and Halo's `decimal.wf` are the owner's MIT-licensed code, so
  reusing them in the MIT-licensed Whitefoot repository needs no notice.
  **One provenance question:** their parameters (800 digits, shifts of at most
  60 bits, a `truncated` flag) are those of Go's `strconv/decimal.go`
  (BSD-3-Clause). If they were translated from Go rather than written from
  the algorithm's description, the Go notice applies to them already and
  would follow them into `lib/std`.

If a port is chosen instead:

- Whitefoot source has no comments, so a notice cannot sit in the `.wf`
  file. It goes in a file beside the module, `lib/std/float_text/NOTICE`,
  named from `lib/std/README.md`.
- The binary clause matters because every program that uses the module
  contains the compiled code: BSD-3-Clause (Go's `strconv`) requires the
  notice in the documentation of every binary distribution, a burden on
  every Whitefoot program; Boost 1.0 exempts machine code produced by a
  compiler. Ryu (Apache-2.0 or Boost-1.0), Dragonbox (Apache-2.0 with LLVM
  exception, or Boost-1.0) and fast_float (Apache-2.0, MIT or Boost-1.0) can
  each be taken under Boost 1.0.
- Firn's or Halo's port of Redis's `fpconv` (card 1, option A) is their
  repository's concern; its licence as recalled is Boost 1.0 (confidence 2),
  and E1 records it from the release.

## 4. Specification changes

`language/standard-library` decides that the specification states the
interface text of the host modules only, and that Whitefoot modules are
checked library code outside it. Card 2 asks whether this module's results
are specified anyway, as the board item's completion condition expects. The
text below is written so it serves either way: as the new rule [PRE-3]
(option B of card 2), or as the module's documented contract in
`lib/std/README.md` with the `doc` strings above (option A). Under option B,
the branch also archives v0.134, retitles to v0.135, amends the
standard-library decision, and adds `pkg::float_text` to the library graph;
the fence below is plain text because whether a new `wf-ebnf` fence joins
the grammar extraction was not checked.

````diff
@@ [MOD-10], last sentence @@
-The host modules are the standard library modules PRE-2 fixes; every other standard library module is written in Whitefoot and checked as every module is.
+The host modules are the standard library modules PRE-2 fixes; every other standard library module is written in Whitefoot and checked as every module is, and PRE-3 states the results of `std::float_text`.
@@ end of §14, after the PRE-2 records @@
+[PRE-3] The standard library module `std::float_text`, registered by the graph row `pkg::float_text: [];`, converts between `f64` values and decimal text. Its interface record declares exactly the declarations below, its implementation is Whitefoot source [MOD-10], and each function returns, for every argument its contract admits, exactly the result this rule states.
+
+```
+(the interface record of section 1 of the float-text investigation, verbatim)
+```
+
+A decimal text is a byte sequence of this form:
+
+```
+decimal_text := sign? significand exponent?
+sign         := "+" | "-"
+significand  := digit+ ("." digit*)? | "." digit+
+exponent     := ("e" | "E") sign? digit+
+digit        := "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
+```
+
+Let S be the integer its significand's digits form, F the number of digits after its point, and X the signed integer its exponent forms, zero when absent. A decimal text whose S is zero denotes the zero whose sign is negative exactly when its sign is `-`; every other decimal text denotes the nonzero rational of magnitude S × 10^(X − F) with its sign applied.
+For a nonzero real v, rnd(v) is the value [OP-6]'s rounding selects for v with destination `f64`: of the finite `f64` values, −2^1024 and 2^1024, the one nearest v, and of two equally near the even one, where ±2^1024 yields the infinity and a selected zero the zero with v's sign. The rounding interval of a finite nonzero `f64` x is the set of nonzero reals v with rnd(v) = x.
+
+`f64_read(input, start, end)` reads the bytes `input[start]` to `input[end − 1]`:
+
+| Those bytes | Result |
+|---|---|
+| Not a decimal text | `Err(error: ReadSyntax())` |
+| A decimal text denoting a zero | `Ok(value: that zero)` |
+| A decimal text denoting v, rnd(v) infinite | `Err(error: ReadOverflow(value: rnd(v)))` |
+| A decimal text denoting v, rnd(v) zero | `Err(error: ReadUnderflow(value: rnd(v)))` |
+| A decimal text denoting v, rnd(v) finite and nonzero | `Ok(value: rnd(v))` |
+
+A `FloatDigits` d denotes (−1)^s × d.digits × 10^d.exponent, s being 1 exactly when d.negative is `True()`. `f64_shortest(x)` and `f64_significant(x, p)` select by x, its sign s being its sign bit:
+
+| x | Result |
+|---|---|
+| An infinity | `Err(error: Infinite(negative: s))` |
+| A NaN | `Err(error: NotANumber(negative: s))` |
+| A zero | `Ok(value: FloatDigits(negative: s, digits: 0, count: 1, exponent: 0))` |
+| Finite and nonzero | `Ok(value: FloatDigits(negative: s, digits: c, count: n, exponent: q))`, with c, n and q below |
+
+For `f64_shortest`, n is the least positive integer for which the rounding interval of x contains some c × 10^q with integer q and 10^(n−1) ≤ c < 10^n; of those, (c, q) is the one nearest x, and of two equally near the one with even c. Every `f64` has such a c with n at most 17.
+For `f64_significant`, n is p; with E the integer ⌊log10 |x|⌋, let c0 be the integer nearest |x| / 10^(E − p + 1), the even one of two equally near; (c, q) is (10^(p − 1), E − p + 2) when c0 is 10^p and (c0, E − p + 1) otherwise.
+
+`digits_write(d, P, destination, start, end)` forms the text T. T begins with `-` exactly when d.negative is `True()`, and continues with `0` when d.digits is zero. Otherwise let G be the decimal digits of d.digits, padded on the left with zeros to d.count digits and then stripped of trailing zeros, g its length, and L = d.exponent + d.count − 1. When −4 ≤ L < P, T continues in fixed notation: for L ≥ 0, the first L + 1 digits of G, padded on the right with zeros to L + 1 digits, followed, when g > L + 1, by `.` and the remaining digits of G; for L < 0, `0.`, −L − 1 zeros and G. Otherwise T continues in exponential notation: the first digit of G, followed, when g > 1, by `.` and the remaining digits of G, then `e`, then `+` when L ≥ 0 and `-` otherwise, then |L| in decimal with a leading zero when it is below 10.
+`i64_write(v, destination, start, end)` forms the text T: `-` when v is negative, then the decimal digits of |v| without leading zeros, `0` for zero.
+Each writer, when end − start is at least T's length, writes T to `destination[start]` onward and returns `Ok(value: start + length)`; otherwise it returns `Err(error: WriteTooSmall(required: length))` and writes nothing. No other byte of `destination` changes. Every `digits_write` text has at most `float_text_max` bytes and every `i64_write` text at most `integer_text_max` bytes.
````

Informative consequences, not rule text: `digits_write(f64_significant(x,
P), P)` is C's `printf("%.Pg", x)` for finite x (checked against Python's
correctly rounded `%g` on 20,000 random values at 1, 6, 14 and 17 digits, 0
differences); and a decimal text written by
`digits_write(f64_shortest(x), 17)` reads back to x.

The rule reuses [OP-6]'s rounding and needs no new rounding definition.
[FORM-5]'s float literal spelling is a different selection (fewest bytes,
then the lexicographically least), made for unique spelling, not nearness;
the two need not agree and the rule above does not refer to it.

## 5. Cases and oracles

### Named cases

Expected results from [reference.py](reference.py)
(`python3 -I reference.py cases`), which computes them from the definitions
above. "Shortest" is `digits_write(f64_shortest(x), 17)`; the two Redis
columns come from the reconstruction and are to be confirmed by E1.

| Value | Bits | Shortest | `%.17g` | `%.14g` | Redis reply (reconstructed) | Redis Lua argument (reconstructed) |
|---|---|---|---|---|---|---|
| 0.1 | `0x1.999999999999ap-4` | `0.1` | `0.10000000000000001` | `0.1` | `0.1` | `0.1` |
| 0.1 + 0.2 | `0x1.3333333333334p-2` | `0.30000000000000004` | `0.30000000000000004` | `0.3` | same as shortest | same |
| 1/3 | `0x1.5555555555555p-2` | `0.3333333333333333` | `0.33333333333333331` | `0.33333333333333` | same as shortest | same |
| 1e17 | `0x1.6345785d8a000p+56` | `1e+17` | `1e+17` | `1e+17` | `100000000000000000` | `100000000000000000` |
| 1e16 | `0x1.1c37937e08000p+53` | `10000000000000000` | `10000000000000000` | `1e+16` | same as shortest | same |
| -0.0 | `-0x0.0p+0` | `-0` | `-0` | `-0` | `-0` | `0` |
| 0.0 | `0x0.0p+0` | `0` | `0` | `0` | `0` | `0` |
| smallest subnormal 2^-1074 | `0x0.0000000000001p-1022` | `5e-324` | `4.9406564584124654e-324` | `4.9406564584125e-324` | `5e-324` | `5e-324` |
| largest subnormal | `0x0.fffffffffffffp-1022` | `2.225073858507201e-308` | `2.2250738585072009e-308` | `2.2250738585072e-308` | same as shortest | same |
| smallest normal 2^-1022 | `0x1.0000000000000p-1022` | `2.2250738585072014e-308` | `2.2250738585072014e-308` | `2.2250738585072e-308` | same as shortest | same |
| largest finite | `0x1.fffffffffffffp+1023` | `1.7976931348623157e+308` | `1.7976931348623157e+308` | `1.7976931348623e+308` | same as shortest | same |
| 2^53 - 1 | `0x1.fffffffffffffp+52` | `9007199254740991` | `9007199254740991` | `9.007199254741e+15` | `9007199254740991` | same |
| 2^53 | `0x1.0000000000000p+53` | `9007199254740992` | `9007199254740992` | `9.007199254741e+15` | `9007199254740992` | same |
| 2^53 + 2 | `0x1.0000000000001p+53` | `9007199254740994` | `9007199254740994` | `9.007199254741e+15` | `9007199254740994` | same |
| 2^62 | `0x1.0000000000000p+62` | `4.611686018427388e+18` | `4.6116860184273879e+18` | `4.6116860184274e+18` | `4611686018427387904` | same |
| 2^62 + 1024 (next double) | `0x1.0000000000001p+62` | `4.611686018427389e+18` | `4.6116860184273889e+18` | `4.6116860184274e+18` | `4611686018427389000` | same |
| 2^63 | `0x1.0000000000000p+63` | `9.223372036854776e+18` | `9.2233720368547758e+18` | `9.2233720368548e+18` | `9223372036854776000` | same |
| 1e23 | `0x1.52d02c7e14af6p+76` | `1e+23` | `9.9999999999999992e+22` | `1e+23` | `99999999999999990000000` | same |
| 9.66e21 | `0x1.05d5b7119030cp+73` | `9.66e+21` | `9.660000000000001e+21` | `9.66e+21` | `9660000000000001000000` | same |
| 1e-5 | `0x1.4f8b588e368f1p-17` | `1e-05` | `1.0000000000000001e-05` | `1e-05` | `0.00001` | same |
| 1e-7 | `0x1.ad7f29abcaf48p-24` | `1e-07` | `9.9999999999999995e-08` | `1e-07` | `1e-7` | same |
| 123456789012345 | `0x1.c12218377de40p+46` | `123456789012345` | `123456789012345` | `1.2345678901234e+14` (tie to even) | same as shortest | same |
| 99999999999999.5 | `0x1.6bcc41e8fffe0p+46` | `99999999999999.5` | `99999999999999.5` | `1e+14` (tie to even, carry) | same as shortest | same |

Further digit-rounding cases: `%.1g` of 2.5 is `2`, `%.2g` of 0.125 is
`0.12`, `%.2g` of 0.375 is `0.38` (exact ties, to even). Each shortest text
in the table reads back to its bits through `f64_read`.

Reading (expected from Python's correctly rounded `float`, with the
overflow and underflow classes from exact rationals):

| Text | Result |
|---|---|
| `9007199254740993` (2^53 + 1, a tie) | `Ok` 2^53 (even) |
| `9007199254740995` (a tie) | `Ok` 2^53 + 4 (even) |
| `9007199254740993.` then 1,000 zeros then `1` | `Ok` 2^53 + 2 (just above the tie; the deciding digit lies past 800 digits) |
| `9007199254740993.` then 1,000 zeros | `Ok` 2^53 (an exact tie) |
| `2.4703282292062327e-324` (just below half of 2^-1074) | `Err ReadUnderflow(0.0)` |
| `2.4703282292062328e-324` (just above) | `Ok` 2^-1074 |
| `-1e-400` | `Err ReadUnderflow(-0.0)` |
| `1.7976931348623158e308` | `Ok` largest finite |
| `1.7976931348623159e308` | `Err ReadOverflow(inf)` |
| `-1e400` | `Err ReadOverflow(-inf)` |
| `0e99999999999999999999` | `Ok` 0.0 (a zero, not an underflow; an exponent beyond 64 bits) |
| `-0` | `Ok` -0.0 |
| `1.`, `.5`, `+.5e-3` | `Ok` 1.0, 0.5, 0.0005 |
| `0.1000000000000000055511151231257827021181583404541015625` | `Ok` 0.1 (its exact value) |
| empty, `.`, `+`, `e5`, `1e`, `1e+`, `1.2.3`, ` 1`, `1 `, `0x10`, `inf`, `nan` | `Err ReadSyntax()` |

Writers: `WriteTooSmall(required: 24)` for `-2.2250738585072014e-308` in a
23-byte window with nothing written; `i64_write` of `-9223372036854775808`
(20 bytes) and of 0.

Under card 2's option B these are conformance cases in
`tests/conformance/cases` (kind `run`, one program per family comparing
written bytes with a constant text and exiting 0); under option A they are
program tests in `tests/programs`.

### Sweep oracle for the standard library

Named cases cannot cover the rounding space, so a program test reads a list
of bit patterns and prints each result, and the test compares the output
with [reference.py](reference.py)'s results, promoted from this directory to
the test's own directory (formal tests do not import `research/`). Python 3
is already a gate prerequisite, and the reference is compiler-independent.
Inputs: fixed-seed random bit patterns (10,000), every `m × 10^e` for
m < 1,000 and e from -30 to 30 (about 60,000, dense in exact midpoints), the
powers of two and their neighbours, and texts formed from exact midpoints
with and without a nonzero digit after 800 digits. The reference's speed
(about 2,000 inputs per second for all functions) fits the gate; the exact
size is set after timing a small sample in CI.

### Oracle for "matches Redis 7.2.16 byte for byte" (experiment E1)

A temporary workflow on the work branch downloads `redis-7.2.16.tar.gz` from
download.redis.io, checks its SHA-256 against a value written into the
workflow, builds `redis-server`, and asks the real server, through
pipelined `redis-cli`, for each input given as its `%.17g` text (which
`strtod` reads back exactly):

- `ZADD k <text> m` then `ZSCORE k m`, giving `d2string`'s bytes;
- `EVAL "redis.call('set', KEYS[1], tonumber(ARGV[1])) return redis.call('get', KEYS[1])" 1 k <text>`,
  giving the Lua argument path's bytes.

It records each input's bits and both outputs as a table, and reports how
often `fpconv_dtoa`'s digits differ from `reference.py`'s shortest, on the
input families above. It also settles each confidence-3 row of the Redis
table: whether ±2^62 is written as an integer, the `emit_digits` layout
(`1e-7` or `1e-07`), and the `fpconv` licence and version. The table, kept
compact (bits in hex and the two outputs), becomes the byte-for-byte oracle
of whichever repository card 1 assigns the Redis layer to. Using the real
server rather than extracting functions from the source keeps the oracle
exactly what Redis replies. Redis 7.2.x is BSD-3-Clause, so building it in CI
raises no licence issue.

## 6. Choices for the owner

### Card 1. Does the standard library's shortest output follow the exact definition, with Redis's Grisu2 output reproduced in the downstream projects?

- **A (recommended).** The standard library's shortest digits are the exact
  shortest, nearest digits defined above, the definition Ryu, Schubfach,
  Python and JavaScript share. Reproducing Redis's Grisu2 output and layout
  (`fpconv_dtoa`, `double2ll`) is Redis compatibility, kept once in a
  downstream repository: Firn-wf already takes Halo-wf as a submodule, so one
  copy in Halo-wf can serve both. The board item's completion condition drops
  "equal to Redis 7.2.16 byte for byte" for the standard library and a
  Firn/Halo item takes it, with E1's table as its oracle. Cost: a Grisu2 port
  of about 250 lines downstream. Risk: none for the standard library.
- **B.** The standard library's "shortest" operation is defined as Grisu2 as
  fpconv implements it. Not recommended: the specification or library
  contract would define a function by an algorithm's imprecision, the name
  would be false on about 0.1% of values and on many exact midpoints, and
  every other Whitefoot program would inherit Redis's quirk.
- **C.** The standard library offers both: the exact shortest and a
  Redis-compatible Grisu2 writer. Not recommended: it puts one product's
  compatibility rule into the language's library, which the project rule
  "a project-local issue is fixed in the project" refuses; its only gain over
  A is one copy instead of one copy in Halo-wf.

Confidence 3/5: that Redis 7.2's output is Grisu2's rests on recall (the
`fpconv` name, the absence of a big-number fallback in it) and on a
reconstruction; E1 can overturn it, in which case the question disappears.

### Card 2. Does the specification state these results, against the decision that leaves Whitefoot library modules out of it?

- **A (recommended).** Keep the decision. The rule text of section 4 becomes
  the module's contract in its `doc` strings and `lib/std/README.md`, the
  named cases and the sweep become program tests, and the board item's
  completion condition changes from "the specification states each result,
  with conformance cases" to "the library contract states each result, with
  program tests against an independent oracle". Cost: no specification
  version. The tests are as independent as conformance cases would be.
- **B.** Specify them as [PRE-3] and add conformance cases. This amends
  `language/standard-library`, whose recorded reason for leaving library
  modules out is that only the build-supplied host definitions are
  conditions the safety guarantees depend on; this module's results are
  value contracts, not safety conditions. Gain: the results become language
  facts every implementation must meet, which suits a value relation as
  stable as IEEE rounding. Cost: a specification version, the archive step,
  the approval record and a design-tree change.

Confidence 3/5: both keep an independent oracle; the choice is about what the
specification is for, which is the owner's.

### Card 3. Is Stage 1 the exact decimal arithmetic for all three operations, with the fast algorithms deferred until a measurement asks for them?

- **A (recommended).** Stage 1 as above: SDC for reading, shortest and
  significant digits, plus Clinger's fast path, built on the downstream
  copies' method, written from the papers or the owner's own code. Stage 2
  (Schubfach, Eisel-Lemire) waits for a measurement that needs it.
- **B.** Implement Schubfach and Eisel-Lemire with SDC as fallback from the
  start. Cost: about 10 KB of tables and roughly twice the proof work;
  gain: speed no current consumer measures.

Confidence 4/5: the downstream copies already run this method, and neither
consumer's shortest output can use the standard library's anyway (card 1).

### Smaller choices, decided by the recommended interface unless the owner objects

- Precision limited to 1 to 17 digits (lifting it later is compatible).
- Whole-window reading; hexadecimal text, `inf`, `nan` and spaces left to
  callers (both consumers read hexadecimal text already and keep that code).
- The 2^62 integer rule composed by the caller, not an option.

### Work estimate

| Step | Content | Estimate (agent days) |
|---|---|---|
| E1 | Redis 7.2.16 oracle table and the Grisu2 difference measurement, temporary CI workflow | 0.5 |
| E2 | Witnesses W1 to W4 in CI; any refusal becomes a gap item | 0.5 |
| Stage 1 | `std::float_text` in Whitefoot: about 1,000 to 1,400 lines with proofs in place of the downstream guards | 3 to 5 |
| Tests | Promoted reference, sweep program and named cases | 1 |
| Card 2 B only | Specification v0.135, conformance cases, design-tree change | 1 to 2 |
| Stage 2, if asked | Schubfach and Eisel-Lemire with their tables | 3 to 5 |

The proof work is the uncertain part: no downstream copy proves its bounds,
so the Stage 1 estimate assumes E2 passes without a gap.

## Found along the way

- Four copies of decimal conversion live downstream, two of them almost
  identical 409-line `decimal.wf` files; all use run-time guards and
  wrapping arithmetic where a proof would stand. Disposition: they are the
  evidence for this item; their removal is Firn's and Halo's work once the
  module lands.
- The downstream `decimal.wf` provenance (Go `strconv/decimal.go`
  parameters) is unrecorded; see section 3. Disposition: to be asked before
  any code is copied into `lib/std`.
- The board item states the integer limit as "< 2^62"; Redis's `double2ll`
  admits ±2^62 itself (confidence 3). Disposition: E1 settles it, then the
  Firn/Halo item's text.

<a id="ordinary-contracts-over-integer-elements"></a>

## Ordinary contracts over integer elements

Decision brief, written 2026-10-13.

### Observation and independent witness

The owner selected option A, including shared obligations 1–6. The comparison
and predicted witnesses below describe the pre-amendment v0.144
[branch specification](https://github.com/Ming-Research/Whitefoot/blob/9e1aa2e921af7e1d0a65857aaf4a3da85a834699/spec/kernel-spec.md);
its line numbers are reading coordinates, not stable rule identities. The
[active specification](../../../spec/kernel-spec.md) owns the resulting language
rules, and conformance cases own their executable evidence.

The corpus at [the tracked-field range amendment](https://github.com/Ming-Research/Whitefoot/commit/d2060013556f5979a8424cb97c7fe1f5ab3467da)
reported `Source / FN-8 / InvalidRequires` at
[`decimal_left`'s requirement](../../../lib/std/float_text/decimal.wf)
(line 203): `requires 1_u8 <= decimal^.digits[0_u64];`.
Both float-text program tests stopped during compilation, before integer
formatting executed or the floating-point oracle ran. Both compile programs
using the same library module, including its decimal declarations
([test entry points](../../../compiler/tests/programs/float_text.rs),
lines 100 and 115; [embedded library records](../../../compiler/src/library.rs),
lines 94–111). At [the earlier corpus revision](https://github.com/Ming-Research/Whitefoot/commit/71b750270a94430a3b535622355f3257f19753a8),
both already stopped at the preceding RANGE-1 field guard in the same file,
line 50. The library sources and the ordinary requirement validator are
unchanged between these revisions. The newly reached refusal is therefore a
previously masked source-form restriction, not evidence of a runtime regression.

This complete, independent program isolates the declaration form. Its
predicted current result is FN-8 at `requires`; this smaller program has not
been compiled. The uncalling `main` isolates declaration and body checking from call-site
proof. The body needs the element bound to prove its subtraction; removing
the requirement would leave that OP-2 obligation unproved.

```wf
struct DigitCell {
  digits: Array<u8, 1>;
}

fn first_minus_one(cell: &DigitCell) -> result: u8 reads(cell.digits) contract {
  requires 1_u8 <= cell^.digits[0_u64];
} {
  let digit = cell^.digits[0_u64];
  let decreased = digit - 1_u8;
  return decreased;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

The direct normative reason is FN-8 (line 1404): “a subscript outside a
clause (b) place” is inadmissible “even when another clause states their
domain”. ENT-2(b) (line 3234) requires that the place's “final step selects a
readonly field”. Here the final step is `[0_u64]`, not a field selection.
The fact that `0 < 1` is statically known does not change that classification.
The [pre-amendment ordinary clause validator](https://github.com/Ming-Research/Whitefoot/blob/9e1aa2e921af7e1d0a65857aaf4a3da85a834699/compiler/src/semantic/check/requires.rs)
(lines 954–980) implemented precisely this boundary: a subscript ending the
place yields `InvalidRequires` at its clause. This is declaration admission,
not FN-8's later call-site `Unproved` or `Refuted` disposition.

FN-9 independently excludes “subscript” from its ordinary relation datums
(line 1463), and CALL-4 excludes subscripts from result projections (line
1531). Relaxing the FN-8 syntax check alone would therefore leave the term,
copy, support and publication rules incomplete. The float module also writes
the ordinary element requirement in `decimal_right` and
`decimal_binary_bits`; their equivalent declarations remain subject to the
same rule. The separate unrun witnesses for missing trim nonemptiness,
assignment count publication and scan induction
([trim](witnesses/w6d_trim_nonempty_contract.wf),
[assignment](witnesses/w6e_nonempty_call_contract.wf),
[scan](witnesses/w6h_scan_leading_digit.wf)) describe later questions and are
not evidence for the present formation refusal.

### Recorded grounds and what they do not establish

The [readonly-field investigation](../readonly-field-terms/DESIGN.md#soundness-every-change-is-an-overlapping-write)
starts with the condition that every change of a storage value kills facts
supported by that value. Its table includes direct field assignment inside
the declaring module, whole-element or ancestor replacement, projected
callee writes, offset reassignment, reference rebinding, consumption and
scope exit. Readonly fields are ordinary writable fields inside their own
module, so readonly syntax was never the argument that such facts could
survive writes indefinitely.

Its [alternatives](../readonly-field-terms/DESIGN.md#alternatives) table
(line 162) calls admitting every integer field below a subscript “yes, same
kills”. The following discussion (lines 169–172) says that their facts die
unless a write's index is proved distinct, so they may offer little retained
precision while adding overlap checks. It explicitly calls their closure-size
cost unmeasured. The [checks-and-proofs decision](../../../design/language/checks-and-proofs.md)
records that choice against every subscripted integer place. These are
historical design grounds, not proof that mutable element facts are unsound,
nor measurements of their cost in float text. The new evidence is a needed
ordinary boundary fact about one element, before any overlapping write.
It does not establish that all mutable-element workloads need the extension.

The same investigation's [offset analysis](../readonly-field-terms/DESIGN.md#offsets)
explains an actual support requirement: changing `idx[0]` changes the place
selected by `table[idx[0]]`. Its nonrecursive offset rule admits only an
ENT-2(a) tracked place or ENT-2(c) constant, whose support is already tracked.
Its [boundary analysis](../readonly-field-terms/DESIGN.md#boundary-of-this-change)
(lines 189–195) separately records why FN-9 subscripted datums were deferred:
formal offsets must denote the same selection in the body and in caller
substitution. The then-current measure implementation lost that identity;
this was not a proof that publication is inherently unsound.

A different [range-fact decision](../../../design/language/checks-and-proofs/range-facts.md)
rejects moving quantified range facts into ordinary entailment because
ordinary overlap kills would discard an entire quantified fact and duplicate
the range judgment's activation and version model. Admitting finitely many
ordinary scalar terms would not move quantifiers into L0. RANGE-1–3 would
still own quantified facts, their instances and their version definitions.

The [constitution](../../../docs/constitution.md#safety) requires proof of
partial-operation domains and allows no runtime substitute. Its
[compatibility clause](../../../docs/constitution.md#compatibility-and-evolution)
gives no reason here to select a language design by the number of files
that must change. The alternatives below are compared by semantics, proof
precision and mechanism boundaries, not migration effort.

### Shared obligations for an ordinary-term extension

The following are proposed requirements on options A and B, not claims that
the current language admits their new forms. They keep the existing
judgments and make the boundaries of the proposed amendment explicit.

1. **A term must name an existing integer value.** Extend ENT-2(b)'s final
   selection as stated by the chosen option. Keep its current root, typed
   place, proof-path identity and nonrecursive offset rules. Every subscript
   still owes OP-4. A requirement forms its places at body entry using only
   earlier requirements, and an expanded definition at the first requirement
   that uses it (ENT-2, lines 3270–3273). Its own conclusion cannot prove its
   subscript in bounds. Body reads form their own ordinary places. No guessed
   alias identity, unchecked index or implicit runtime test is added.
2. **Support follows the whole selected place and every offset.** Generalize
   ENT-5's clause-(b) support sentence (line 3755) from the selected readonly
   field to the selected integer storage. Keep support for offset values,
   every reference holder and every Box binding traversed. Use that same
   support through requirement establishment, ordinary copying, conditional
   result transport, value delivery, callee publication and loop continuing
   kills. A direct or ancestor write, unproved-distinct element write,
   projected callee write, changed offset, changed holder, consume or scope
   exit kills the affected fact. A proved-disjoint sibling or element write
   preserves it. A copied integer remains its copied value; surviving facts
   about that value do not retarget a later read of the storage.
3. **Entry meaning is not a promise of a snapshot.** The proposal selects
   function-entry meaning for every admitted subscript offset, including
   offsets that are fields of reference parameters, and corresponding
   pre-transfer actual values at the caller. This selector rule is a proposed
   language choice, not a consequence uniquely selected by current FN-9.
   Even when a row writes the reference parameter that supplies an offset,
   that offset is an entry selector, not an unqualified exit-state datum.
   Retain the existing
   entry-image stability rule (FN-9, lines 1476–1484; ENT-2, lines 3288–3289;
   ENT-5, lines 3783–3787): a referenced entry image has no new snapshot term,
   and an overlapping support kill makes it permanently unavailable. In
   particular, rebinding a formal index must not retarget an `ensures` read
   to its new body value; that clause's required index image is unavailable.
   An unqualified datum of a reference parameter with no write rooted at it
   retains the ordinary entry-image rule, not an invented immutable capture.
4. **Exit reads have their own membership proof.** For an ordinary
   postcondition over a reference parameter whose row writes it, select the
   referent as it stands at each selected return, with the formal index
   meaning fixed as above. Prove each new subscript's OP-4 membership in that
   exit state before querying the clause; its own desired conclusion cannot
   prove membership. An entry bound alone cannot justify a read after the
   collection shrinks. At the caller, apply projected effects and kills
   before publishing a verified exit relation at that same actual selection;
   unavailable selector support supplies no relation. Keep FN-9's existing
   restrictions on relation shape and its result-or-written-reference datum.
5. **Keep MSR-3 and CALL-4's separate boundaries.** This proposal adds no
   immutable index/element capture family. Explicit `entry` of a newly
   admitted subscripted integer stays outside ordinary MSR-3's
   field-and-dereference projection set (lines 3414–3435). Existing explicit
   entry datums and range entry reads retain their current meanings. Extending
   ordinary `entry(cell^.digits[i])` to a frozen element value is a separate
   unselected question. Owned result-array elements remain excluded as
   relation sources by
   CALL-4; neither option adds result projections or placement capture
   families. Already-admitted scalar result fields and owned descendants
   may nevertheless transport to a newly admitted element destination
   through existing ENT-3 and MSR-3 transports. That destination extension
   is intended and needs cases; it is distinct from admitting a result
   array subscript as a source datum.
6. **Effects stay ordinary.** Erased clauses add no exhibited runtime access
   (EFF-2, lines 1589–1592), but their actual reads join the existing contract
   footprint and separation judgment (EFF-5, lines 1622–1624). A `writes`
   path covers its reads, and a row lists `reads` before `writes` (EFF-1).
   Dynamic formal indices in rows retain their existing actual-value
   substitution. No readonly-for-the-call assumption follows from merely
   writing an element in a contract.

The minimal requirement and the subtraction example below need ordinary L0
bounds, ENT-3's copy equality and their existing consumers; they do not need
an affine image for every element. The proposed generalization therefore
does not enlarge ENT-6's tracked-field image category or INV-1's written
factor grammar. An element used in a body may be copied into an own integer
binding under the ordinary rules; that is not a substitute spelling demanded
for its contract. Direct composite affine reasoning over element images
would be an additional choice, with its own example and grounds. Existing
subscripted readonly fields already demonstrate this L0-only boundary. No
ordinary FN-9 obligation is assumed to be discharged by RANGE-2's deferred
ordinary-obligation route, whose enumerated families do not include FN-9.

### Option A: ordinary integer element terms with tracked offsets (selected)

**Proposed scope.** Generalize ENT-2(b) to any fragment-integer final selected
value below one or more subscripts on a place otherwise admitted there:
bare integer elements and mutable or readonly integer descendant fields.
Keep each offset restricted to ENT-2(a) or ENT-2(c), excluding nested
subscript reads used as offsets and unbound arithmetic expressions. This is
one ordinary term rule used by body facts and FN-8, not a special spelling
recognized only inside a requirement. Extend FN-9 parameter datums to these
formed places under the shared obligations above, including existing
readonly-field cases; do not extend result datums. Keep quantified clauses
and their derivation unchanged.

**Proposed rule-text sketch.** These clauses outline the amendment, not
replacement normative text already selected:

- ENT-2(b): a subscripted integer place has the existing clause-(b) roots
  and projections, at least one subscript, and a final selected fragment
  integer; each offset remains an (a) or (c) term. Its formation keeps OP-4
  and the existing earlier-requirement rule.
- FN-8: admit exactly those ENT-2(b) places among ordinary clause datums;
  retain the other totality and clause-expression conditions.
- ENT-5: a (b) term is supported by its selected integer storage and every
  offset, reference-holder and Box-binding support, with the existing
  transport and survival rules. The measure-descriptor boundary is unchanged.
- FN-9: parameter datums may project through these admitted subscripts;
  result projection admission is unchanged. Each selector denotes its
  function-entry term, substituted with its pre-transfer actual term, and
  stays available only while its entry-image support remains stable. Prove
  subscript membership independently at the selected return in the datum's
  selected state before proving its relation. Written-reference data read
  exit storage at that stable entry selector. Unavailable selection is an
  unproved postcondition, never a fresh snapshot or current-selector read.

**Before and after.** The following complete program shows
A's additional variable-index scope and a body operation that consumes the
ordinary requirement. It is currently predicted to fail FN-8 at its second
requirement; after A the identical complete source is intended to check.
This code
block is both the before-program and after-program; no source rewrite is
proposed. Neither result has been run.
The main function again isolates declaration/body checking rather than a
caller; call transport gets separate conformance cases below.

```wf
struct DigitCell {
  digits: Array<u8, 1>;
}

fn first_minus_one(cell: &DigitCell, index: u64) -> result: u8 reads(cell.digits[index]) contract {
  requires index < cell^.digits.len;
  requires 1_u8 <= cell^.digits[index];
} {
  let digit = cell^.digits[index];
  let decreased = digit - 1_u8;
  return decreased;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

A treats the literal index in float text and an already-tracked variable
index by the same storage and support rule. Its soundness argument is that
each established scalar fact names existing storage and every possible
change either overlaps that storage or changes included selector support;
all those events kill it. This argument depends on complete implementation
of those paths, including formal-body and caller substitutions, rather than
on the element being immutable. Ordinary kills sacrifice precision when
index distinctness is unproved; they do not re-establish a fact about another
element. No read-congruence solver or quantified premise is added.

The design cost is more ordinary terms, overlap queries and closure input in
programs that read mutable elements. No timing establishes its size. The
new boundary-contract use supplies a reason to reopen the former rejection;
it does not cancel the earlier observation that many update-loop facts will
die. Adoption would amend the readonly-only decision and explain that change
on its approval card. The recommendation is to consider this general rule
because tracked offsets already supply the required identity and kills;
zero being a constant is not an independent soundness reason to admit only
float text's first element.

### Option B: admit new ordinary element terms only at constant offsets

**Proposed scope.** Keep all currently admitted ENT-2(b) readonly-field
terms. Add bare or descendant fragment-integer element terms only when every
subscript offset is an ENT-2(c) constant: an integer literal, integer named
const or an in-scope integer const-generic parameter with OP-4's required
u64 type. Symbolic const generics are immutable within an instance; they
still require a bound proof. No new mutable-element term with an ENT-2(a)
variable offset is added. The new terms have the same support and copy
behavior in bodies and contracts, not a second contract-only fact category.
FN-9's newly admitted element projections are limited to these constant
selections; its existing non-subscripted datums remain unchanged. The shared
entry/exit obligations apply, with no mutable offset to snapshot in the new
postcondition cases. Existing readonly-field terms with variable offsets
remain outside ordinary FN-9's newly added projection scope.

**Proposed rule-text sketch.** Use A's FN-8 and ENT-5 sketches, with this
ENT-2(b) admission: retain every current readonly-field term; additionally
admit any otherwise qualifying fragment-integer place when every subscript
is an ENT-2(c) constant. Use A's FN-9 formation, stability and substitution
sketch only for parameter projections whose subscripts are all constants;
variable-indexed readonly-field parameter postconditions remain excluded.
The complete constant scope includes literal, named-const and const-generic
terms, each independently owing OP-4. No new `entry` or result datum is added.

**Before and after.** This complete program, also unrun, is predicted to
fail FN-8 now and to check after B. The identical block is both the before
and after source. Its `0_u64` is an ordinary integer constant, not a distinguished first-element operation. The variable-index
program of A would remain refused, even with a preceding bound requirement.

```wf
struct DigitCell {
  digits: Array<u8, 1>;
}

fn first_minus_one(cell: &DigitCell) -> result: u8 reads(cell.digits) contract {
  requires 1_u8 <= cell^.digits[0_u64];
} {
  let digit = cell^.digits[0_u64];
  let decreased = digit - 1_u8;
  return decreased;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

B limits the new selector forms and avoids mutable-selector substitution for
new postconditions, while retaining the same checks for writes to the
selected element and its ancestors. It makes no readonly-duration promise;
a helper may overwrite the element, and the old fact then dies. Its cost is
a semantic split between constant and tracked-variable selections despite
both having support the current system can name. It also retains two
FN-9 subscript boundaries: new constant element datums and excluded
variable-index readonly-field datums. This narrower experiment could be
chosen, but no soundness result or measured checking cost currently favors
it over A.

### Ordinary postcondition and caller witness for A and B

This complete source is another before-and-after witness for both ordinary
options: its spelling is unchanged. The ordinary subscripted `ensures` is
currently outside FN-9's relation datums; after either proposal the body must
verify it and the caller must obtain the new bound after the call's write.
All of those outcomes are predictions; this source has not been compiled.
The caller begins with zero, so retaining only a pre-call fact cannot prove
its final subtraction. Changing the helper's stored value to zero must fail
the postcondition, not merely change the program's exit status.

```wf
struct DigitCell {
  digits: Array<u8, 1>;
}

fn establish_first(cell: &DigitCell) -> result: unit writes(cell.digits) contract {
  ensures 1_u8 <= cell^.digits[0_u64];
} {
  set cell^.digits[0_u64] = 1_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let digits = array_filled::<u8, 1>(value: 0_u8);
  let cell = DigitCell(digits: digits);
  establish_first(cell: &cell);
  let digit = cell.digits[0_u64];
  let decreased = digit - 1_u8;
  return std::process::exit_status(code: decreased);
}
```

A's variable-selector variant must additionally require `index <
cell^.digits.len`, store at `[index]`, and publish that same selection. The
shared selector and membership obligations above govern that variant; B
continues to refuse it. The conformance plan below separates selected-return
verification from caller substitution and from selector invalidation.

### Option C: keep the ordinary rule and use singleton range clauses

**Proposed scope.** Change no language or checker rule. Use RANGE-1–3's
existing integer element terms and versioned facts for this library
requirement. This is an alternative writer form for the natural ordinary
requirement the compiler refuses, so applying it to the module needs the
owner's explicit exception to the no-workaround rule; this brief does not
apply it or presume that exception.

**Proposed rule-text sketch.** None: retain FN-8, FN-9, ENT-2 and RANGE-1–3
unchanged. Any module rewrite is an owner-approved exception, not an
amendment to the language or approval inferred by this brief.

**Before.** This complete ordinary program is predicted to fail FN-8:

```wf
struct DigitCell {
  digits: Array<u8, 1>;
}

fn first_minus_one(cell: &DigitCell) -> result: u8 reads(cell.digits) contract {
  requires 1_u8 <= cell^.digits[0_u64];
} {
  let digit = cell^.digits[0_u64];
  let decreased = digit - 1_u8;
  return decreased;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

**After.** This complete program states the requirement over the one
existing element of `Array<u8, 1>`. The conclusion reads the bare bound
variable `k`, so the body read at zero is a trigger for the range instance.
It is predicted to check under the current range rules, but has not run.
There is no ordinary `ensures` claim: RANGE-2 may discharge the body's
subtraction obligation, not an ordinary FN-9 result relation.

```wf
struct DigitCell {
  digits: Array<u8, 1>;
}

fn first_minus_one(cell: &DigitCell) -> result: u8 reads(cell.digits) contract {
  requires forall first(k in 0_u64..1_u64): 1_u8 <= cell^.digits[k];
} {
  let digit = cell^.digits[0_u64];
  let decreased = digit - 1_u8;
  return decreased;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

The fixed one-element Array makes the range instance's element exist. For a
variable-length collection, a singleton range requirement alone does not
prove that an element exists: RANGE-1 conditions its conclusions on read
definedness. An independent length precondition and OP-4 proof would remain
necessary. The gain is using the existing range semantics, including their
write definitions and instantiation. The cost is requiring quantification
for a scalar boundary property and making the function participate in the
range judgment. It leaves the ordinary-term restriction unresolved. Its
soundness rests on the existing range judgment, not ordinary overlap-killed
L0 facts, and is a different design direction from A and B.

### Proposed amendment map and discriminating acceptance checks

These are proposed implementation and conformance obligations for a chosen
option, not edits already made and not a new test manifest. A and B must
state the same admitted set in each affected rule. C changes no rule.

| Rule or decision | Proposed work for A; B's difference |
|---|---|
| ENT-2 | Generalize clause (b)'s final selected value, keep proof-path identity and nonrecursive offset admission, and retain body-entry formation under earlier requirements. B adds only constant-offset mutable element terms. |
| FN-8 | Admit that complete ordinary term set and keep totality and own-clause bound restrictions. B's variable-offset mutable elements remain refusals. |
| ENT-3, ENT-5 | Let established comparisons, requirements, copies and verified publications use the new terms with complete storage/offset/holder support on every transport and kill. Keep conservative unproved-alias kills. |
| FN-9 | Add the selected parameter subscript datums; retain relation-shape and written-reference/result conditions, entry-image stability, selected-return membership and caller pre-transfer selector identity. B adds constant selections only. |
| MSR-3, CALL-4 | Preserve explicit entry projection and result-datum boundaries; no new immutable element/index or placement capture family; existing scalar result/placement transports may gain admitted element destinations. |
| ENT-6, INV-1 | Keep current image and written affine factor eligibility; ordinary L0/copy consumers suffice for this proposal. No new affine element-factor family is selected. |
| OP-4, EFF-1/2/5, OWN-7 | Reuse ordinary bounds, row order and exactness, contract-read separation and overlap judgments; exercise new terms through them without weakening them. |
| RANGE-1/2/3 | Keep their quantified semantics and deferred-obligation list; A/B add no quantified ordinary facts, and C uses the existing singleton form. |
| Design tree | A/B replace the readonly-only ordinary-term choice in checks-and-proofs and update requires/postcondition rationale; C keeps it and records the owner's library exception. |

The existing [conformance manifest](../../../tests/conformance/manifest.jsonl)
also contains boundary cases affected by a chosen amendment. Their current
verdicts are evidence of the present rule, not results to flip mechanically:

| Existing case | Proposed disposition if A/B is selected |
|---|---|
| `fn8-neg-writable-field-below-subscript-requirement` | A admits its term category, but the existing source lacks a preceding index bound and has an unexhibited `reads(entries)` row. A positive successor must add the earlier bound and exhibit a body read (or remove that row); preserve a separate OP-4 missing-bound negative. B keeps its variable-offset refusal. |
| `fn9-neg-readonly-field-below-subscript-relation` | A admits its variable-index readonly-field datum; B keeps the FN-9 refusal. The existing body returns zero without exhibiting `reads(entries)`, because the contract is erased. An A positive successor must make the row exact, for example by reading the field in the body, rather than changing only the verdict. |
| `ent2-neg-indexed-writable-goal-datum` | Its constant-offset mutable-field comparison becomes an admitted ordinary fact under A/B; reassess the later addition through the exact reference alias as a positive consumer; validate that the proposed L0 facts suffice at this direct use. Change its doc and verdict only alongside the amendment. |
| `ent2-neg-placement-moved-owned-integer-field` | A/B admit the constant-offset destination, so existing owned-descendant placement transport should carry its integer bound. Promote a positive transport case; do not add a new capture family. |
| `call4-neg-result-field-stored-in-array-element` | A/B admit the destination for the already-admitted scalar result field. Its publication can then justify the later subscript. This changes destination transport, while a clause reading an owned result array element remains refused. |
| `ent2-neg-requirement-readonly-field-unbounded-subscript`, `ent2-neg-requirement-measure-unbounded-subscript`, `fn8-neg-formal-requirement-place-unbounded-subscript` | Keep their OP-4 missing-bound expectations. New element-term formation uses the same diagnostic owner. |

These are prospective case revisions, with no manifest edits or claimed
new verdicts from execution. A future amendment must read each source in
context and distinguish the intended rule from an earlier effect or bound
refusal. The additional proposed cases distinguish the domain, proof and
invalidation questions:

- The independent fixed-index declaration above: current FN-8 refusal;
  acceptance after A or B. Its bounds follow from `Array<u8, 1>` alone.
- A variable-index requirement after an independent earlier bound: accept
  under A; reject FN-8 under B. Removing the earlier bound, or placing it
  after the element clause, must reject at the element's OP-4 formation
  under A; the clause's own asserted value must not discharge that bound.
- A constant index equal to the length, and a symbolic const-generic offset
  lacking a proved bound: reject OP-4 after A/B, even when a contradictory
  element value is asserted. Literal, named-const and proved const-generic
  positive pairs check the full constant set of B.
- Use an admitted requirement through a copied integer to discharge the
  subtraction above. A producer and caller pair must independently verify
  a positive call and reject the call after the caller stores zero at the
  selected element; no callee body may be unfolded to supply its contract.
- After a requirement, write zero to that element, replace its whole Array,
  replace its containing struct, or call a helper whose projected row writes
  it. Each separate case must lose the old bound and reject the later
  subtraction with OP-2. A sibling write and a proved-distinct element write
  must preserve the bound; an unproved-distinct write must not.
- Under A, reassign the offset binding, change a holder, and exercise a loop
  that can do either on its continuing edge. Old facts must not discharge
  operations at the new selection. Consumption and scope exit must leave
  the supported facts inaccessible; a surviving earlier integer copy must
  retain only its own established value facts.
- A helper with `writes(cell.digits)` must verify an ordinary exit-element
  postcondition after storing one, and a caller must receive that bound
  after the call's writes kill its old bound. Storing zero must fail FN-9 at
  the selected return. The helper's index and caller's actual index must
  name the same selection, with two different actual indices exposing a
  substitution that incorrectly hard-codes one element.
- Under A, reassign a postcondition's formal index before return: the old
  entry image must become unavailable, never silently select the new index.
  A postcondition on a collection that shrinks must separately prove its
  exit subscript exists before its conclusion is queried; an entry-only
  length guarantee is insufficient.
- Use two reference actuals such that one callee effect can change the
  other's contract-read element or offset support. EFF-5 must reject the
  overlap; proved separation must admit the paired case. A read-only row
  does not turn the value into an immutable entry snapshot.
- Ordinary explicit `entry` over an element and an owned result Array's
  element remain formation refusals under both proposals. Existing range
  entry/read cases keep their current verdicts.
- The complete singleton-range program above must preserve its requirement
  through the ordinary-to-range deferred OP-2 path under C. A zero-element
  Array variant must fail at the body's OP-4 read rather than obtain a bound
  from a vacuous quantified clause.

### Recommendation and remaining uncertainty

Recommend considering A as the language decision, with B and C presented
as real alternatives. Its added expressibility follows the existing support
abstraction and addresses an observed ordinary interface need; the prior
rejection's limited-survival argument does not distinguish a constant
selection before mutation from another supported selection before mutation.
This is not a claim that the implementation or its checking cost has been
validated. The fixed families must remain deterministic and terminating,
and no timing or work budget may select source acceptance.

Confidence 3/5. The current declaration refusal and its normative reason
are settled by the observed CI error and unchanged validator. The ordinary
support argument has recorded precedent, but the complete proposed FN-9
selector substitution, formation-at-exit and transfer behavior still needs
independent review and the discriminating CI cases above. In particular, a
requirement-only relaxation is insufficient evidence for postcondition
publication. If those obligations need a different abstraction, bring that
back as a language choice rather than narrowing expected verdicts or
silently minting snapshots. No performance prediction is made, and no
measurement has selected between A and B. All complete programs in this
section are unrun source witnesses; only the library CI failure above is
reported as observed.
