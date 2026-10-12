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
  so bounds on the high word are derived automatically.
- Tables are `const` arrays of `u64` or of a const-eligible struct
  ([CONST-2]), indexed after a range proof; element ranges are derived
  from the constant ([ENT-3] S9).
- Bits of an `f64` come from `reinterpret::<f64, u64>`; `f64` results are
  built with `reinterpret::<u64, f64>`, and the fast path uses
  `fmul.strict`/`fdiv.strict`, which round exactly as IEEE 754 requires.
- The digit count invariant of `FloatDigits` is a range type invariant
  ([TYPE-11]).

No language gap was found by reading; E2 found one in the right shift and a
candidate one in storing a bounded count (see
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

Dated measurement: CI run 38303849577 of the temporary witness job, on branch
head ffd67eedfa75, whose compiler and specification are those of its base
51c5f4263f51 (specification v0.134; the branch changes neither). The
generator `witnesses/generate.py` writes the programs and states each prior
prediction (`expected`); W2 is split into the left shift (w2a), the right
shift (w2b) and the move toward the end (w2c), and W1 and W3 into two files
each, so one refusal does not hide the next. After this run three witnesses
were added: w2d, w2c with the bound restated, and w2e and w2f, the minimal
witnesses of the two refusals below. `run` is the compiled program's
exit status, 0 meaning every value matched the one computed in Python.

| Witness | Expected | Compile | Run | Disposition |
|---|---|---|---|---|
| w1a table lookup | accept | rejected: FORM-3 at `let entry = power_table[index];` | - | authoring error, fixed; rerun pending |
| w1b product and scale | accept | rejected: the same FORM-3 | - | authoring error, fixed; rerun pending |
| w2a left shift | open | accepted | 0 | - |
| w2b right shift | reject | rejected: OP-6 `UndischargedConversionDomainObligation` at `cvt::<u64, u8>(quotient)` | - | language gap, as predicted |
| w2c front shift | open | rejected: FN-9 `UndischargedPostcondition` (the `count <= 800` type invariant) at `return unit;` | - | candidate language gap; prediction now reject |
| w3a window write | open | accepted | 0 | - |
| w3b index write | open | accepted | 0 | - |
| w4 scan | open | accepted | 0 | - |
| w2d front shift, bound restated | accept | not yet run | - | shows whether the restated form is accepted |
| w2e quotient bound | reject | not yet run | - | w2b's gap alone |
| w2f field bound | reject | not yet run | - | w2c's candidate gap alone |

The w2c and w2d `main` now tests each decimal's count against the room the
move needs before calling it: `append_digit` publishes nothing about the
count, so the call's requirement could not otherwise be proved in `main`
(read, not observed: the run reported `shift_front`'s refusal, not `main`'s).
The test is in the test driver only and changes nothing in `shift_front`.

**w1a and w1b.** `entry` is a fixed grammar atom (the graph file's
`entry_decl` and the `entry(...)` place base), and [FORM-3] excludes every
such spelling from IDENT, so `let entry` does not parse. The binder is now
`row`. Reading both programs against the keyword atoms and the reserved
operation names [OP-1] found no other reserved spelling; `let scaled` beside
`fn scaled` is a value beside a callable, which [TYPE-6] admits.

**w2b: the gap.** Each quotient digit is `ishr(remainder * 10 + digit, shift)`
with `remainder < 2^shift`, so it is below ten, and storing it as a `u8`
digit needs that bound (here only its `<= 255` part, for the conversion; the
digit type invariant needs `<= 9`). No rule derives it. The [ENT-3] S7 row
for `ishr` gives only the interval hull of `floor(a / 2^s)` over the operand
intervals and `r <= a`; with `s` anywhere in `0..60` the hull is the
dividend's own range. Nothing relates the shift's result to the live value
`ishl(1, shift)`, and even the exact division row, which captures the images
of `q = a / d`, yields only `d * q <= a` at a later runtime product. Deriving
`q <= c - 1` from `a <= c * d - 1` divides by the symbolic `d`, which no
certificate can do: [PRF-1]'s integer tightening divides only by the two
constant factors [ENT-6] fixes, and a named multiplicity only multiplies. The
checker matches the specification here (its S7 shift row is the interval
hull plus `r <= a`). Stating the strongest natural loop invariant,
`remainder <= mask` instead of the witness's `remainder <= 2^60 - 1`, leaves
the gap unchanged. Minimal semantic witness, apart from this program, run as
w2e with a `main` that calls it on five values:

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

The postcondition is true for every admitted input and is expected to be
refused under [FN-9]. The shift form adds a second missing piece: `ishr` by
a variable amount captures no division image at all. Closing the gap needs
both: a fact relating a quotient to its divisor without a runtime product
(`d * q <= a < d * q + d` for unsigned `/`, and for `ishr(a, s)` with the
divisor `ishl(1, s)` bound), and a certificate step that cancels a named
value proved positive, so that `d * q <= c * d - 1` with `d >= 1` gives
`q <= c - 1`. Which form those take is the owner's decision; the witness
stays in its natural form until then.

**w2c: a candidate gap.** The requirement
`decimal^.count + delta <= 800` gives the body an affine premise over the
entry values ([ENT-3] S4), from which `total = count + delta` and every
`digits[index + delta]` subscript are proved: the checker reports body
obligations before FN-9 relations, so the FN-9 report shows those were
discharged. The return's goal, though, is over the field `decimal^.count`
after the `set`, and that current value has no affine image: [ENT-6] gives
images to bindings and measures, a field reached through a reference is
neither, and [INV-1] refuses such a field as an invariant term, so a writer
cannot even state the bound over it. Only the L0 route reaches the goal: the
`set` establishes `decimal^.count = total` ([ENT-3] S5), and `total` has no
L0 upper bound. The checker matches the specification here (its affine value
of a place with any field step is none). The contract already implies the
bound and the program stores exactly the bounded value, so a writer must
restate what the contract says, as `invariant fits: total <= 800_u64;` after
`let total`, which [ENT-3] S16 publishes as an L0 fact over `total` that the
two loops do not kill. That restatement is the program w2d; the witness
w2c keeps its natural form. Minimal semantic witness, run as w2f with a
`main` that calls it on four counters:

```wf
struct Counter {
  doc "One count.";
  count: u64;
}

fn advance(counter: &Counter, delta: u64) -> result: unit writes(counter) contract {
  requires counter^.count + delta <= 800_u64;
  ensures counter^.count <= 800_u64;
} {
  let count = counter^.count;
  let total = count + delta;
  set counter^.count = total;
  return unit;
}
```

The postcondition holds for every admitted call and is expected to be
refused under [FN-9]. The change that would close it gives an integer place
reached through field steps, such as `counter^.count`, a current affine image
as a measure already has one: killed by the same [ENT-5] writes as its L0
facts, set by a [SET-1] commit to the committed value's image, and admitted
as an [INV-1] term. The goal over the field then reads `count + delta`, which
the requirement's premise bounds. Which form that takes is the owner's
decision.

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
