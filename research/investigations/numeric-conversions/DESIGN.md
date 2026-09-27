# Numeric conversion coverage

## Question and scope

The initial baseline is main at `5dd9d5d7`, after generic struct constants.
The retained sources use the callable syntax from main at `345e2966a`; that
update does not change conversion semantics. TYPE-4 and OP-6 make `cvt` exact
and value-preserving: its result type is selected from
the complete source/destination type pair. OP-8 separately supplies same-width
bit reinterpretation and same-type floating-point rounding operations.

This investigation asks which useful numerical transformations can be written
directly, which need an ordinary composition, which require a failure branch
despite a known domain, and which are missing. It does not change the active
specification or select new operation spellings before comparing examples.
The immediate consumer is integer low-bit extraction for binary encoding;
the wider inventory includes integer range/signedness changes, saturation,
integer/float conversion and float precision changes.

## Evidence criteria

For each family, separate three observations:

1. Mathematical contract: preserved value, selected bits, rounded value, or
   clamped value; state the treatment of negative values, bounds and special
   floating-point values where applicable.
2. Accepted source: current result type, required branches or proofs, and
   whether an ordinary source function can express the intended total result.
3. Generated code: inspect emitted and optimized LLVM for parameterized
   helpers, so constant-folded main calls cannot establish a general cost.
   Distinguish a source-level Result from an actual surviving runtime branch.

Use complete small WF programs and boundary controls. Low-bit extraction must
include values beyond the destination range; value-preserving narrowing must
retain a failing out-of-range control. Signedness probes include negative
inputs and the high bit. Float probes include fractions, exactness boundaries,
signed zero, infinity and NaN. Existing conversion conformance cases remain
the baseline for already implemented OP-6 behavior, not a new redundant suite.

Compare retaining ordinary compositions, adding a total bit-truncation family,
and exposing a proof-required exact conversion alongside its checked form.
For rounding/saturation, first determine whether existing operations compose
with precise enough behavior; a missing convenience spelling alone does not
establish a runtime cost. No timing claim will follow from instruction counts.
Use primary Rust/LLVM documentation only where external comparisons help
distinguish semantics or lowering requirements.

The deliverable is a bounded inventory with source and emitted-code evidence,
ranked improvement options, and explicit unresolved policy questions. Retained
probe sources live beside this investigation for reproduction and are removed
or replaced when their observations are superseded. They are not daily test
inputs; any selected implementation extracts necessary regression evidence
into the maintained compiler/conformance suites. Deferred findings update the
existing numeric-conversion TODO rather than creating an unrelated work queue.

## Baseline coverage and observed limits

The 10 numeric primitive types give 90 distinct ordered pairs. OP-6 supplies
29 total exact conversions and 61 checked exact conversions. These are a
coverage partition, not 61 missing operations. Existing semantic tests cover
the pair classification; backend conversion tests cover the integer lowering
classes, float endpoints and special-value edges.

| Intent | Current source | Observation | Disposition |
|---|---|---|---|
| Exact widening, including unsigned to a strictly wider signed type | Total `cvt` | Direct extension/conversion and ordinary value evidence | Already supported |
| Exact narrowing or signed-to-unsigned value conversion | Checked `cvt` | Correct Result; a call-site range proof does not change that type | Consider an explicit proved form |
| Same-width bits, including signedness and int/float relabeling | `reinterpret` | Preserves bits; differs deliberately from numeric conversion | Keep this distinction |
| Low destination bits of a wider integer | Mask, checked `cvt`, match; reinterpret for a signed result | Total wrapper is expressible, but needs an impossible-error fallback in source | Consider a direct total truncation operation; no measured speedup established |
| Integer saturation | `imin`/`imax`, checked `cvt`, match | The tested i32-to-u8 helper loses the redundant conversion check after optimization | Ordinary composition is viable for this case |
| Float to integer after a chosen rounding rule | `ftrunc`/`ffloor`/`fceil`/`froundeven`, then checked `cvt` | Rounding exists; range/NaN/infinity rejection remains a real requirement | Do not call every Result unnecessary |
| Float to saturated integer | Explicit NaN policy and endpoint branches/clamps, then rounding and `cvt` | Expressible for tested i32/i64 destinations; redundant round-trip checks survive | Candidate for a direct total operation, after selecting its policy |
| Rounded large integer to float, or f64 to f32 | No direct conversion with this contract | Exact `cvt` can reject a value for which a rounded answer is wanted | Missing direct value semantics; float integral rounding is not precision conversion |
| Integer conversion inside a numeric generic | `cvt::<T, f64>` | Explicit compiler `Unsupported: Generics`, even when its result is discarded | Separate implementation gap under existing rules |

An identity `cvt::<T, T>` is deliberately absent under OP-6. Whether a future
uniform generic conversion family should admit identity remains a policy
question; the unsupported generic witness uses integer T and float destination,
so it does not depend on resolving that question.

### Source witnesses

[probes.wf](probes.wf) contains parameterized helpers and boundary observations.
The low-byte helper is the current ordinary composition:

```wf
fn low_byte(value: u32) -> result: u8 pure {
  let masked = iand(value, 255_u32);
  match cvt::<u32, u8>(masked) {
    Ok(value: byte) => {
      return byte;
    }
    Err(error: refused) => {
      return 0_u8;
    }
  }
}
```

The fallback has defined behavior, but is mathematically unreachable for this
mask. Writing it does not make `cvt` a proof-required operation. A source
change to the mask can make that fallback reachable without a type error.

[proved-result-type.wf](proved-result-type.wf) gives `value <= 255_u32` as a
verified requirement, then directly returns `cvt::<u32, u8>(value)` from a
u8-returning function. It is rejected at the return with FN-1 ReturnMismatch:
the result remains `Result<u8, NarrowError>`. This is specified behavior,
not failed interval reasoning or an implementation defect.

[checked-equality.wf](checked-equality.wf) has the stronger practical loss:

```wf
match cvt::<u64, u8>(index) {
  Ok(value: small) => {
    let restored = cvt::<u8, u64>(small);
    return values[restored];
  }
  Err(error: refused) => {
    return 0_u8;
  }
}
```

The function requires `index < 4_u64` and the table has four elements. The
compiler nevertheless reports OP-4 at `restored`, with residual
`restored < values.len`. Replacing only the selected subscript by `index`
is accepted. ENT-3.S5 publishes conversion equality only for total pairs;
the checked Ok payload has its own u8 range but no connection to the original
operand. The existing semantic test
`a_narrowing_conversion_carries_no_equality_into_its_ok_arm` expressly records
that rule. Any change here must amend the proof rule and its evidence together.

[generic-conversion.wf](generic-conversion.wf) instantiates one Int-generic
helper with u32 and i64 and discards its `cvt::<T, f64>` result. All pairs are
distinct numeric pairs and no uniform result type is demanded by the helper.
The compiler reports `Semantics/Unsupported: Generics`; the existing
`generic_conversion_is_reported_as_unsupported_instead_of_invalid_source`
test exposes the same capability boundary. Concrete conversion coverage does
not establish generic conversion support.

### Optimized helpers

With Apple Clang 21.0.0, arm64 Darwin and `-O2`, the unchanged emitted module
keeps ordinary externally visible helper definitions. No linkage rewriting,
forced inlining, proof insertion or constant substitution is used for this
comparison. Main's constant checks fold separately and are not cost evidence.

| Helper | Optimized LLVM body observation |
|---|---|
| `low_byte` and `signed_low_byte` | One `trunc i32 ... to i8` and return; no mask, Result or validity check |
| `bounded_byte` with a proved requirement | Still compares with 256 and selects the fallback; the entry range was not conveyed to LLVM |
| `saturating_byte` | Clamp and truncation; no separate conversion-validity check |
| `bit_resign` | Returns the input bits unchanged |
| `wide_unsigned` | One zero extension |
| `exact_small_float` / `exact_small_double` | One `uitofp` for their total type pairs |
| `exact_float`, `exact_integer`, `narrow_float` | Conversion plus reverse/exactness tests and Result construction, implementing their selected contract |
| `truncated_integer` / `rounded_integer` | Float rounding plus saturating cast, reverse cast, exactness test and Result construction |
| `saturating_integer` / `saturating_wide_integer` | NaN/bounds handling and remaining cast round-trip tests, despite the wrappers' mathematically total contracts |

Arm64 assembly agrees with the relevant distinction: both low-byte helpers
are just a return under the narrow-result ABI, while `bounded_byte` retains a
compare and conditional selections. This is evidence about these parameterized
helpers on this toolchain, not a wall-time measurement or a universal optimizer
guarantee. Inlining into a caller may change the result.

A separate exploratory control inserts only the already required
`value <= 255` as `llvm.assume` at `bounded_byte`'s entry. Optimization then
leaves that erased assumption and a truncation. This control is not shipped
compiler behavior. It shows that conveying the proof could remove this cost
without changing the source conversion API. A proof-required conversion and
general proof-to-backend transport are therefore alternatives with different
scope; their benefit must not be counted twice.

### Float boundaries matter

The probe checks that exact `u32 -> f32` accepts 2^24 and rejects 2^24+1;
exact `f64 -> i32` rejects 1.75; integral truncation before conversion maps
-1.75 to -1; ties-to-even maps 2.5 to 2; and infinity remains a checked error.
Exact `f64 -> f32` accepts 1.5, rejects the f64 value denoted by 1.1, preserves
negative zero and produces the specified canonical NaN. `froundeven(1.1)`
would instead round to the integer-valued float 1.0, so it cannot supply the
missing float precision conversion.

The i32 saturation composition explicitly chooses NaN-to-zero and clamps to
endpoints exactly representable in f64. That construction does not generalize
by substituting i64 endpoints: i64::MAX is not exactly representable in f64.
The wide helper instead compares with the exact exclusive upper bound 2^63,
returns the integer maximum on that edge, and handles the lower bound and NaN
separately. It checks 2^63, its predecessor float, negative infinity and NaN.
These are interface obligations, not details an unspecified cast may choose.

## Design alternatives

The comparison below records the pre-implementation research baseline. At
that point these spellings were proposals, not accepted WF syntax or selected
rules.

**A. Additive operations.** Keep existing `cvt` unchanged, add an explicitly
proved integer conversion such as `cvt.proved::<Src, Dst>(x)` returning Dst,
and add a total strict-width integer truncation such as
`itrunc::<Src, Dst>(x)`. This keeps current meanings and separates intent,
but retains the old type-pair-dependent `cvt` result shape alongside a second
exact-conversion interface. Migration cost alone is not a reason to prefer it.

**B. A uniform exact/checked/domain family.** Make bare `cvt` return Dst after
static domain proof, `cvt.checked` return Result, and `cvt.defined` return the
total domain predicate, on the same model as exact integer arithmetic. Total
type pairs discharge immediately. Truncation remains a separate bit operation.
This makes the selected failure policy explicit and simplifies a generic
function's result shape; it changes TYPE-4/OP-6 and every dependent contract,
operation inventory and conformance case. Integer range domains fit existing
proof terms, but float exactness needs a precisely specified predicate and
proof route; it must not be advertised as automatically inferable from ranges.
The completed proposal below supplies the rule-level trial for this option.

**C. Keep the interface and improve evidence/optimization.** Publish the
checked integer Ok payload's equality through existing value-associated Result
evidence, and transport selected proven range facts to lowering. This can
recover the indexed example and remove the demonstrated redundant check without
new operation spellings. It leaves impossible-failure arms in source and does
not provide rounded float conversion or direct total truncation.

The recommended proposal is B with C's integer Result evidence. They solve
different problems, rather than being mutually exclusive alternatives. General
proof-to-backend assumptions are not required: an admitted bare conversion can
lower directly under its retained domain proof. A checked equality extension
must capture the operand's value at conversion and cannot follow later
replacement of its original variable. The cases and implementation boundaries
below make those obligations explicit.

Direct rounded conversions to float and total float-to-integer saturation are
separate candidates. Their policy includes ties, overflow/underflow, signed
zero and NaNs. The [Rust numeric cast rules](https://doc.rust-lang.org/reference/expressions/operator-expr.html#numeric-cast)
provide a useful comparison: narrowing integers keeps low bits; float-to-int
truncates and saturates with NaN-to-zero; conversions to float round to nearest,
ties-to-even. WF need not put those different contracts behind one spelling.
[LLVM saturating float-to-int intrinsics](https://llvm.org/docs/LangRef.html#llvm-fptosi-sat-intrinsic)
provide a defined total primitive for the former policy, while ordinary raw
float-to-int instructions have a domain obligation. A broad general cast or
software float-conversion library is not selected by this study.

## Reproduction and evidence boundary

The initial probe run used compiler sources from `5dd9d5d7` (equal to the
compiler tree at `033f44a9`), with the old callable spelling retained in the
research sources at `432949aba`. That already-built, gate-validated compiler's
executable SHA-256 was
`c57f989b1b0073769d4999266f3353143d758b2f400c26a74e8d260b684afe33`.

After adapting only the callable spelling to main at `345e2966a`, the native
probe again exited 0. The three negative/capability outcomes below and the
accepted original-index control were reproduced. Optimized `low_byte`,
`signed_low_byte`, `bounded_byte` and `saturating_byte` retained the recorded
instruction forms. This run reused the compiler built at `9be78e355`, whose
compiler and specification trees equal `345e2966a`; its executable SHA-256 is
`cbffd4dd1ae8641ef03790457181188988bf70cc4af1a53c50c1f406307bb7f9`.

The retained sources test the pre-B language. Run them with the compiler and
specification from `345e2966a`, not a compiler implementing B. The setup below
runs from this investigation's checkout root, extracts that pinned baseline
into scratch space and builds it there. To reuse an existing binary from the
same compiler and specification
revision instead, skip the build and set `numeric_compiler` to its absolute
path. The generated-code comparison requires the Clang version recorded above.
Native construction and execution both run under the repository's shared
verification guard.

```sh
numeric_scratch="$(mktemp -d "${TMPDIR:-/tmp}/wf-numeric-conversions.XXXXXX")"
mkdir "$numeric_scratch/baseline"
git archive 345e2966a | tar -x -C "$numeric_scratch/baseline"
numeric_compiler="$numeric_scratch/target/gate/whitefootc"
perl .github/run-check.pl numeric-compiler-build cargo build --manifest-path "$numeric_scratch/baseline/compiler/Cargo.toml" --target-dir "$numeric_scratch/target" --profile gate --bin whitefootc --locked --offline
"$numeric_compiler" --emit-llvm -o "$numeric_scratch/conversions.ll" research/investigations/numeric-conversions/probes.wf
clang -O2 -S -emit-llvm -x ir "$numeric_scratch/conversions.ll" -o "$numeric_scratch/conversions-O2.ll"
clang -O2 -S -x ir "$numeric_scratch/conversions.ll" -o "$numeric_scratch/conversions-O2.s"
perl .github/run-check.pl numeric-native-probe sh -c '
  "$1" -o "$2/conversions" "$3" && "$2/conversions"
' sh "$numeric_compiler" "$numeric_scratch" research/investigations/numeric-conversions/probes.wf
```

The three separate negative/capability probes use the same `--emit-llvm` path:
`proved-result-type.wf` expects FN-1 at its return;
`checked-equality.wf` expects OP-4 at the restored index;
`generic-conversion.wf` expects compiler `Unsupported: Generics`, not a source
rejection. The indexed control replaces only `values[restored]` with
`values[index]` and must be accepted.

The exploratory assumption control adds `declare void @llvm.assume(i1)` to
the emitted module and, immediately inside `wf_bounded_byte`'s entry, adds:

```llvm
%audit.range = icmp ule i32 %v0, 255
call void @llvm.assume(i1 %audit.range)
```

Apply the same optimization command and compare only that function. This is
a stated hypothetical transport of its existing precondition, not permission
to insert unchecked assumptions in source or implementation. Generated LLVM,
assembly and executables are scratch outputs and are not retained in the
repository. These baseline observations establish neither a timing result
nor implementation of a new operation.

## Completion criteria for the B proposal

The proposal was selected against these discriminators; implementation must
preserve their distinctions:

- One exact conversion relation must determine bare, checked and domain-query
  behavior for every numeric type pair, including the proposed same-type rows.
  NaN handling and signed zero must not acquire inconsistent answers between
  these interfaces.
- An independent binary-value oracle must agree with the candidate domain
  algorithms at integer precision boundaries, float integer endpoints,
  subnormal/overflow edges and NaN/infinity. A rounded reverse conversion alone
  is insufficient evidence: include its integer-maximum collision controls.
- Each proposed proof rule must name its existing fact representation and a
  paired stale-value or control-flow counterexample. No general float solver,
  new guard-product analysis or proof work budget is assumed.
- Source sketches must cover static proof, explicit runtime branching,
  checked-result delivery, generic bodies and same-type instantiation, with
  outcomes separated from what the baseline compiler currently implements.
- The implementation plan must identify normative changes, frontend and proof
  owners, IR/lowering consumers, migration classes and discriminating checks.
  The proposal review and the implementation review have distinct scopes.

The standalone Rust oracle belongs beside this investigation because it checks
conversion-domain mathematics independently of the WF implementation. Its
only caller is the reproduction command documented with its results; it is
not a gate dependency and is removed or superseded when the selected domain's
maintained conformance oracle covers these observations.

### Domain-oracle result and reproduction

On arm64 Darwin with rustc 1.98.1, [domain-oracle.rs](domain-oracle.rs) completed
804,542 domain comparisons and 416 modular comparisons with exit 0. Its
integer-only binary-value reference agrees with the candidate cast/round-trip
tests on all u16 magnitudes, neighborhoods of integer powers of two, every
f32/f64 exponent with selected edge significands/signs, and 10,000 deterministic
additional bit samples. The modular checks exercise all 64 integer endpoint
pairs at their source boundaries. Explicit negative controls expose both
maximum round-trip collisions and signed widening by zero extension.

Run from this checkout root with the Rust toolchain installed:

```sh
numeric_scratch="$(mktemp -d "${TMPDIR:-/tmp}/wf-numeric-domain.XXXXXX")"
perl .github/run-check.pl numeric-domain-oracle sh -c '
  rustc --edition=2024 -O "$1" -o "$2/domain-oracle" && "$2/domain-oracle"
' sh research/investigations/numeric-conversions/domain-oracle.rs "$numeric_scratch"
```

This is a bounded comparison of domain algorithms and modular semantics,
not an exhaustive float proof, WF implementation, acceptance checker or timing
experiment. Host casts model the candidate round-trip algorithms using the
[Rust conversion rules](https://doc.rust-lang.org/reference/expressions/operator-expr.html#numeric-cast);
the reference side uses decoded significands and integer bounds. Existing WF
native probes separately cover cross-format canonical NaNs and signed zero;
the research oracle alone does not certify their LLVM lowering.

## Proposed rules for owner review

This section records the exact-conversion design. The requested implementation
scope is B, integer Result evidence and generic conversion support; the active
specification owns the resulting language rules. Low-bit wrapping is a
separable recommended companion;
rounded and saturated float interfaces remain follow-up scope as described
below. Implementation and conformance evidence must satisfy these stated
boundaries before the proposal can be considered complete.

### One relation, three interfaces

Define one partial value function `C(S,D,x)` and its total Boolean domain
`D(S,D,x)`. The following rows partition the supported pairs and values:

| Pair/value class | Domain and selected result |
|---|---|
| Same numeric type | Always defined; copy the complete input representation, including NaN payload/sign and negative zero |
| Distinct integer types | Defined exactly when the mathematical integer lies in the destination's closed range; preserve that integer |
| Integer to float | Defined exactly when the integer is representable in the destination format; integer zero gives positive zero |
| Float to integer | Defined exactly for finite integral values within the destination's closed integer range; either signed zero gives integer zero |
| Distinct float formats, finite input | Defined exactly when representable in the destination; preserve value and the sign of zero |
| Distinct float formats, infinity or NaN | Defined; preserve infinity's sign and use the destination canonical quiet NaN for every NaN |

All 100 ordered pairs of the ten numeric types are admitted. There are 39
whole-type total pairs (the original 29 plus ten same-type pairs) and 61
value-dependent pairs. Totality no longer selects the result type:

| Spelling | Result | Obligation/behavior |
|---|---|---|
| `cvt::<S,D>(x)` | `D` | Prove `D(S,D,x)` before lowering, then produce `C` without a validity guard |
| `cvt.checked::<S,D>(x)` | `Result<D,NarrowError>` | Return Ok(C) on the domain, Err otherwise, for every pair |
| `cvt.defined::<S,D>(x)` | `Bool` | Compute only the total domain answer; do not execute a partial cast on an invalid value |

The same-type row is recommended over continued refusal because a generic
conversion must not need a separate body when its two type parameters coincide.
It copies bits rather than canonicalizing same-format NaNs: there is no format
change to make their representation ambiguous, and generic specialization to
an identity should require no NaN test. Cross-format canonicalization remains
the existing OP-6 rule. This distinction is explicit table data, not a claim
that all exact conversions are bit-preserving. No new `Numeric` bound is needed;
the current Int and Float bounds already select each endpoint's capability.

### Proof boundary

Each bare call has a ConversionDomain obligation with the exact source type,
destination type and evaluated operand identity. It uses the existing signed
goal, support, closure and derivation machinery. Use these fixed routes:

1. The ordinary contradictory-state rule or an established identical positive
   `cvt.defined` goal discharges it. An identical negative goal refutes it in
   a consistent state. Calculating a Bool alone establishes neither sign.
2. Whole-type totality discharges it. For symbolic endpoints, evaluate the
   finite bound domains while preserving equality of repeated type parameters;
   at most 100 pairs are inspected, not independent choices for each occurrence.
3. A direct typed literal or scalar named const is decided by its already
   decoded exact value. Integer constants use integer arithmetic; floating
   constants use sign/significand/exponent bits, not the host's rounded cast.
   For a symbolic endpoint, retain the same correlated finite type choices:
   every answer must be true to prove the domain, or false to refute it;
   mixed answers remain unknown. The existing typed identities `0_T` and
   `1_T` supply their zero/one value for each choice. This adds no conversion
   syntax to const-expressions.
4. Integer-to-integer obligations normalize to the destination's upper and
   lower bounds, in that order, over the operand's current immutable integer
   image, using existing L0 and affine routes and their usual proof certificates.
5. Integer-to-float obligations additionally admit the sufficient closed
   interval `-2^p <= x <= 2^p`, with p=24 for f32 and p=53 for f64, through the
   same bound routes. This is not the entire exactness domain: larger even
   integers may also be representable. Constant evaluation or an exact domain
   predicate handles those cases; failure of this sufficient test is unknown,
   not proof that the conversion is undefined.

Otherwise the operation is rejected under OP-6 with its missing domain goal.
The same domain normalization is used when a `.defined` goal is queried by
FN-8 at a call boundary, rather than only by a bare conversion in the body.
Thus a caller with `x <= 255_u32` can discharge
`requires cvt.defined::<u32,u8>(x)` without executing a query. A failed
sufficient interval route cannot refute a `.defined` requirement or establish
its negation; only an exact constant answer or established negative goal does
so in the proposed initial routes. Positive domain facts are not automatically
projected back into all numerical inequalities: after an exact integer cast,
its equality and destination type bounds supply the ordinary consequences.
For nonconstant float operands there is no new automatic float arithmetic,
integrality, round-trip or bit-divisibility solver. A `.defined` condition or
an identical verified requirement supplies the proof. Merely proving a float
is within integer bounds does not prove it has no fractional part.

The predicate's origin follows the current immutable Bool-binding and support
rules used by integer `.defined` queries; changing the operand kills its use
as evidence about that operand. Exact conversion expressions may occur in
FN-8 erased definitions only for whole-type total pairs (including universally
total symbolic pairs). A preceding requirement does not make a partial
conversion legal inside another clause. `.defined` is total and usable in
requirements. FN-9 stays the existing integer relation fragment; no arbitrary
float postcondition or Bool-result theorem is introduced.

### Integer result evidence

An admitted bare integer conversion publishes ordinary mathematical equality
and preserves its input's existing integer image regardless of width or sign.
For checked integer-to-integer conversion, its local Result's private success
payload equals the evaluated input value inside that Result's conditional
context. Existing S5/S6/S7/S9 evaluated-value sources supply their L0 relations;
an untracked indirect read supplies its source and destination type bounds,
not a newly invented relation to mutable array storage. The conditional state
does not transport the operand's separate affine image.

Capture before subsequent mutation, use the existing pre-kill closure,
immutable operand identities and private Result payload term, and reuse the
current Result transport:
direct match, named binding, copy, whole-binding replacement, value delivery,
propagate and forwarded return. Each outcome keeps an isolated context; joins
retain common weaker bounds and continuing-backedge kills remain unchanged.
No guard product, body re-analysis or new path enumeration is introduced.

This release does not transport float-valued equalities or opaque domain
goals through Result. A checked conversion with a float endpoint returns the
correct value, but its Ok tag alone does not authorize a second bare conversion
of the old input. A writer already has the converted payload; when a domain
fact is needed for the input, branch on `.defined`. This is a specified proof
precision boundary, not an implementation omission disguised as a source error.
Extending it requires changing ENT-5 and FN-9's evidence vocabulary and is a
separate opportunity recorded in TODO.

### Source cases and expected outcomes

These are proposal examples, not claims that the baseline accepts new names.
First, range proof should replace the baseline's impossible-error branch:

```wf
fn byte(value: u32) -> result: u8 pure contract {
  requires value <= 255_u32;
} {
  return cvt::<u32, u8>(value);
}
```

Removing the requirement rejects an unconstrained parameter. Replacing the
body's argument by `256_u32` also rejects; changing only the destination to
u64 needs no requirement. Masking into a local first and using its existing
S7 bound also proves the u8 conversion, including a binary-encoding consumer.

Generic conversion keeps one result shape and an explicit obligation:

```wf
fn convert<S: Int, D: Int>(value: S) -> result: D pure contract {
  requires cvt.defined::<S, D>(value);
} {
  return cvt::<S, D>(value);
}

fn attempt<S: Int, D: Float>(value: S) -> result: Result<D, NarrowError> pure {
  return cvt.checked::<S, D>(value);
}

fn same<T: Float>(value: T) -> result: T pure {
  return cvt::<T, T>(value);
}
```

Check the symbolic bodies before calls, then concrete instances through the
existing FN-2 path. Include an unused malformed body, forwarded type parameters,
constant arguments and nested generic calls. A same-type Float instance must
preserve a noncanonical NaN's bits; a cross-format instance must canonicalize.
Unknown numeric width never becomes a guessed concrete type. An unconstrained
`<T>` cannot use numeric operations merely because current callers use integers.

The two mutation controls below are distinct:

```wf
let permitted = cvt.defined::<u32, u8>(value);
set value = 300_u32;
if permitted {
  let invalid = cvt::<u32, u8>(value);
}
```

This rejects: the saved Bool does not describe the replacement value. In the
next sketch, the entry state has `index < 4_u64`:

```wf
let pending = cvt.checked::<u64, u8>(index);
set index = 1000_u64;
match pending {
  Ok(value: small) => {
    let restored = cvt::<u8, u64>(small);
    let selected = values[restored];
  }
  Err(error: refused) => {
  }
}
```

The selected read of a four-element array accepts: the saved payload still
represents the old index. Changing only that read to `values[index]` rejects.
Changing the saved Result to another conversion before the match must replace
its evidence, and a write through any possible alias must apply normal kills.

| Case | Required observation |
|---|---|
| Guarded float conversion | `.defined::<f64,i32>(x)` true edge admits bare conversion; merely `0 <= x < 4` does not prove integrality |
| Exactness boundary | 2^24 and 2^24+2 convert exactly to f32; 2^24+1 does not |
| Float integer edge | f64 2^63 fails i64 conversion; its predecessor succeeds; the negative endpoint succeeds |
| Special values | NaN/infinity fail integer destinations; cross-format floats admit/canonicalize them as the table states; signed zero behavior is observed by reinterpretation |
| Predicate mismatch | A proof about u8 cannot authorize i8, another operand, or the false edge |
| Caller initialization | A caller's `x <= 255_u32` proves a callee's `.defined::<u32,u8>(x)` requirement by the same domain normalization; x=256 fails |
| Join | Results known below 4 and below 8 join to below 8; a four-element index still needs a new bound |
| Independent Results | Facts under one success tag cannot be conjoined with a second unselected Result's facts |
| Loop | A Result or predicate replaced on a continuing backedge carries no previous iteration's evidence; a fresh value in the body can establish fresh evidence |
| Delivery | Direct and named matches, copies, propagate, give and forwarded return preserve exactly the existing transport routes |
| Aggregate storage | Saving a Result in a struct/array does not add ENT-5 transport; after loading, ordinary payload type facts remain |
| Contract | `.defined` is legal in requires; a partially defined conversion inside a clause remains inadmissible |
| Const expression | Existing const-expression grammar is unchanged; a runtime bare conversion of a scalar named constant can use its value |

### Domain mathematics and lowering

For a finite binary input, decode `(-1)^s * m * 2^e` exactly and remove powers
of two from m. Integrality is `m=0` or `e>=0`; integer bounds are compared with
integer magnitudes. Integer-to-float exactness needs at most p significant bits
after removing trailing zero bits (all current integers fit the float exponent
range). A nonzero binary64 value fits binary32 exactly when that normalized
significand has at most 24 bits, its least significant exponent is at least
-149, and its highest is at most 127. Handle zero, infinities and NaNs in the
table's distinct rows. These are finite exact tests, not a search procedure.

For runtime checked/domain operations, the current saturating cast and reverse
comparison can be shared, retaining both maximum-collision exclusions in
`backend/emitter/conversion/float_endpoint.rs`. A naive round trip accepts
u64::MAX after it rounds upward and saturates back, and similarly accepts f64
2^63 for i64. A `.defined` lowering cannot execute an out-of-domain raw
float-to-integer instruction before deciding its answer. LLVM's
[saturating conversion contract](https://llvm.org/docs/LangRef.html#saturating-floating-point-to-integer-conversions)
provides a total building block. Bare conversion consumes the proved domain
and emits the direct cast; cross-format float NaN canonicalization remains
required result behavior, not a failure check to remove.

Use an explicit conversion mode in checked expressions, goals and IR. Do not
infer the mode from the type-pair totality or from an incidental Result layout.
Semantic formation accepts numeric symbolic endpoint types; only concrete
numeric pairs reach IR. Result interning depends on Checked mode, including
total and symbolic pairs. Keep the shared type-argument parser's reinterpret
consumer on its own existing operation rules.

There is no new compiler pass or general `llvm.assume` family. Domain and result
relations use the existing proof ledger and its ordinary emission boundary.
Byte extraction should already be a cast in unoptimized emitted IR under B;
this is a stronger and more direct criterion than hoping a fallback match is
optimized away. Predicate lowering still performs real work when the writer
asks for runtime validation. No timing claim follows from this design.

The domain helper belongs under the existing flow module because both bare
operations and signed contract queries need the same rules at the same program
state. Its bound normalization is positive-only: an exact negative fact or
constant can refute a domain, while failure of a sufficient interval cannot.
Result production supplies the existing private payload term to the ordinary
evaluated-value fact sources, avoiding an invented storage-assignment identity.
These are representation choices within the existing proof walk, not new
acceptance passes.

### Companion operations and explicit deferrals

For the original low-bit request, recommend `cvt.wrap::<S,D>` over `itrunc` as
a separable companion, with integer endpoints only and result `wrap_D(x)` as
OP-2 already defines it. It admits all integer width/sign pairs: narrowing
keeps low bits, same-width conversion reinterprets signedness, and widening a
negative signed input preserves its mathematical residue modulo the destination
width. For example, `cvt.wrap::<i8,u32>(-1_i8)` is 4294967295, not 255. This last
case requires sign extension even with an unsigned destination. The historical
baseline helper used destination signedness and would be wrong for that case;
the exact-conversion implementation now selects extension from source signedness,
so its shared integer cast supplies the wrapping result directly. The name
describes modular value semantics and uses the existing mode vocabulary; no
competing `itrunc` spelling is proposed. This
mode publishes no exact input equality. It retains ordinary destination-type
bounds and a typed modular expression identity; admitting that identity does
not add an arithmetic rewrite or a new automatic proof family. Constant operands
have the same runtime value semantics; the existing proof rules do not gain
general constant folding of operation trees. Integer-generic endpoints keep
one destination result shape; float endpoints are rejected during formation.

The compiler addition reuses the explicit conversion-mode path and integer
cast, without a domain check or a Result. Qualification requires all 64 integer
type pairs at formation, native modular-result checks using an independent
integer remainder oracle, representative `trunc`/`sext`/`zext`/same-width emitted
forms, integer-generic and contract composition, and rejection of false exact
equalities after wrapping. Existing exact, checked and domain-query behavior
must remain unchanged. These are correctness criteria, not a timing claim.

Do not block the exact family on additional float result policies. A later
rounded-to-float operation should explicitly fix nearest/ties-to-even,
overflow to signed infinity, gradual underflow, signed zero, and cross-format
canonical NaN; a later total float-to-integer operation should separately
select NaN handling, rounding and saturation. The source examples establish
that these are real missing/direct-interface questions, but no consumer yet
selects their complete surface. `froundeven` cannot substitute for format
rounding. Their semantics must not be smuggled into bare `cvt` or `.wrap`.
General float Result facts and general backend range-assumption transport are
also deferred; neither is necessary to implement the stated B contract.

## Implementation sequence after owner review

The sequence below governs the requested implementation; a listed step does
not assert that its completion criterion has passed.

| Step | Change and affected owners | Completion criterion |
|---|---|---|
| 1. Normative contract | TYPE-4/5, OP-1/6/7/8, FN-8 and ENT-2/3/5/6; extend the retained type-argument list and the operation inventory; propose the current spec's successor and archive its outgoing bytes | One domain/value table, fixed proof routes, same-type behavior, result evidence and diagnostics agree; owner-rulable amendments stay beside the tree |
| 2. Formation and generics | `semantic/model.rs`, `goal.rs`, `check/expressions/calls{,/conversions}.rs`, `check/nominal_instances.rs`, `check/requires.rs`, `check/generics.rs` and stable goal substitution | Explicit modes, uniform result shape, symbolic numeric endpoints, checked Result interning, correct contract admissibility; no generic Unsupported residue for the planned cases |
| 3. Proof and evidence | `entailment/flow.rs` obligation dispatch/identity; focused domain helper under flow; `flow/sources.rs`, `flow/results.rs`, `state.rs`, derivation inventory and diagnostics | Domain checks and integer value images use the shared ledger; stale-value, join, loop and Result-isolation controls give the specified outcomes |
| 4. Lowering | `lowering.rs`, `lowering/builder.rs`, `backend/emitter{,/conversion}.rs`, `conversion/float_endpoint.rs` and affected checked-model visitors | Exact has no validity guard, Defined is total without Result construction, Checked agrees with it and produces the same exact value; same-type is a copy; NaN normalization and signedness survive |
| 5. Corpus migration | Conversion semantic/backend tests, entails/generic/contract tests, OP-6/TYPE-4 conformance cases, maintained programs and current guidance | Migrate old partial calls to `.checked` when preserving a genuinely fallible contract; use bare calls with actual proofs for invariant conversions; deliberately replace the old no-equality expectation with paired positive/stale-value controls |
| 6. Verification and publication | Ordinary compiler/conformance/program test homes and existing gates; owner-ruling process | Independent completion review/DCR, full `make check` and required CI on the delivered revision; no root entries, benchmark gates or research dependencies added |

The separate integer `.wrap` extension uses steps 1/2/4/5 without extending the
proof solver. Its rule and mode remain distinct from exact conversion.
Each coherent implementation revision stays on the existing Draft PR. Use the
call-boundary syntax now on main; at implementation start rebase or merge
current main, settle the successor spec version then, and recheck the actual
generated inventory.

Step 1 precedes implementation. Once its rules and mode/type representation
are fixed, backend work and corpus/fixture preparation can proceed in parallel
with the proof work in disjoint files. Generic substitution and model changes
share files, so one owner integrates them. Run expensive verification once
through the shared guard after focused failures are resolved; use the existing
conversion matrix tests rather than adding one full compiler build per pair.

Required checks are all 100 pairs' interface shapes, representative valid and
invalid domain values per class, the code cases above, and native boundary
observations with an independent bit-value oracle. Compare emitted helpers
before optimization, then ordinary optimized/native execution; poison-free
query lowering and preservation of specified NaN/zero results need separate
observations. Include same-type NaN bits and signed-to-unsigned wrap widening,
which the old distinct-pair exact suite could not observe. Whole-program
timings remain unclaimed; proof-cost qualification reuses the existing dense
Result consumer if this new producer materially changes its state sizes.
