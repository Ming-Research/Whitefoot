# Automatic operation facts

Status: implemented. The owner approved the recommendation on 2026-09-25;
specification v0.73, the compiler and the conformance cases implement it, and
the owner's approval of the design-tree amendment the same day placed its
decision in
[language/checks-and-proofs/automatic-facts](../../../design/language/checks-and-proofs/automatic-facts.md).
Sections 1 to 4 are the research round as it was ruled on;
[section 5](#5-implementation) records what landed and its measurements.

## Question

[ENT-3] admits a narrow set of arithmetic idioms as automatic facts, each
added for one proof pattern, with no general criterion. The owner asked for a
principled rule and approved this investigation first. The concrete case:
`let hi = ishr(x, 32_u32); let narrow = cvt::<u64, u32>(hi);` is rejected,
while masking `hi` with `iand` first is accepted.

## Summary

- **Inventory.** Ten operation-derived fact rows exist today (S5's
  conversion equality; S7's wrapping and exact constant offsets, checked-arm
  offsets, unsigned quotient, remainders, unsigned `iand` and shifted one;
  S9's const-array ranges; S14's product interval), plus [ENT-6]'s affine
  images for exact `+`, `-` and constant-scaled `*`. Of the 34 [OP-1]
  spellings that produce an integer, 24 establish no fact at all, among them
  `ishr`, `ior`, `ixor`, `imin`, `imax`, `iabs`, `ineg`, `ipopcount`, `iclz`
  and the saturating rows. Each existing row was added for one proof
  pattern, most for one program: sha256's schedule offsets, deflate's
  code-length table and bit mask, the compute kernels' block division, the
  binary-arithmetic grid product, and halving and remainder bounds with the
  source-carried proof work.
- **Evidence.** The corpus under `tests/programs` and `lib/` needs a new
  operation fact at 3 sites (telemetry's shifted byte, dir_walk's byte-length
  clamp and its wrap-order guard); 18 further workarounds are already
  unnecessary today. A constructed sweep of 22 natural integer idioms is
  rejected at every one of them today; 21 of those were predicted rejected
  before the run and one was uncertain. In the shift case the optimizer
  deletes the proof-only mask and the checked conversion, so the cost there
  is source text and an impossible error arm, not runtime.
- **Recommendation.** Replace the S7 menu and S14 with one table: every
  integer-valued operation row publishes the interval its operands' closed
  intervals give its result, rows bounded by an operand (`iand`, `ior`,
  `imin`, `imax`, `ishr`, `/`, `%`, `imulhi`, `iabs`, `+sat`, `-sat`) publish
  that order relation, and additive rows (`+`, `-` and their wrap forms when
  they cannot wrap) publish the offset relation that generalizes S7's
  constant offsets. A checked row's success payload receives the exact row's
  facts through the existing conditional Result context. All facts are
  ordinary L0 difference bounds, so no premise enters `AUTO`'s pair family.
  The pre-recorded selection criterion selects this step (D, whose relations
  the recommendation narrows to order and offset) and rejects the larger
  exact-image step (X).
- **Owner decisions** are listed in [Decisions for the owner](#decisions-for-the-owner)
  with the owner's ruling of 2026-09-25.
- **Implementation.** [Section 5](#5-implementation) records the landed
  rule, three conformance verdicts that moved from reject to accept where
  one was predicted, one certificate the new facts made redundant, and the
  checking cost against the pre-recorded criterion.

## Constraints recovered before choosing

- [ENT-1] and the [checks-and-proofs](../../../design/language/checks-and-proofs.md)
  decisions: automatic derivation is a fixed, deterministic, terminating
  family run to completion, and "an author can determine from this rule alone
  whether a target is automatic" ([ENT-6]). A new rule must therefore be table
  data a writer can evaluate by hand, with no search and no fixed point.
- A nonempty certificate is rejected when `AUTO` already proves its target
  ([PRF-1]). Stronger automatic facts are therefore not purely widening: they
  can turn an existing `use` block into a redundant-certificate rejection.
- The [automatic-facts](../../../design/language/checks-and-proofs/automatic-facts.md)
  node already publishes S14's product interval "because the checker had
  already computed that interval ... and discarding it made a writer restate
  by hand what the checker had just proved". Its division-image decision
  remains grounded in the compute-model consumers.
- Cost: `AUTO` tries every pair of listed premises (the two-premise-cutoff and
  entering-context entries in [todo](../../../docs/todo.md)). The
  [incremental closure](../../../design/compiler/incremental-closure.md) and
  its middle-term selection determine what added L0 facts cost.
- The [constitution](../../../docs/constitution.md) prefers runtime
  performance over writing ease and accepts proof work, but requires checking
  cost to permit practical iteration. The
  [expected-failure decision](../../../design/language/checks-and-proofs/obligation-discharge.md)
  refuses impossible-case branches added only to satisfy the checker.

## 1. Inventory

### 1.1 Fact sources derived from an operation

Unless its row says otherwise, a source establishes at an `ordinary_let_rhs`
binding and, through [ENT-3.S5]'s commit rule, at a [SET-1] commit value.
None applies to a `return`, `give`, argument or contract-clause position.

| Source | Operation | Required shape and operands | Fact |
|---|---|---|---|
| S5 | integer `cvt` | operand a term or constant, after [OP-6] succeeds | `y = p` |
| S7 wrapping offset | `+wrap`, `-wrap` | constant `k` (either side of `+`, subtrahend of `-`); the closed state must prove `p ± k` in range | `s = p ± k` |
| S7 exact offset | `+`, `-` | constant `k` as above | `s = p ± k` |
| S7 checked arm | `+checked`, `-checked` | `match` scrutinee directly the call, or a bare let-bound name with no intervening kill or `set`; constant `k` | `Ok` binder `w = p ± k` |
| S7 quotient | unsigned `/` | direct binding; operands admitted terms or constants | `q <= a`; for a positive literal divisor `k*q <= a` (affine premise); captured images for the later product consequence `product <= a` |
| S7 remainder | `%` | direct binding; unsigned divisor term or constant; signed divisor a nonzero literal or named const | unsigned `r < d`; signed `-(abs(d)-1) <= r <= abs(d)-1` |
| S7 bit-and | unsigned `iand` | direct binding; each operand that is a term or constant | `r <= a`, `r <= b` |
| S7 shifted one | unsigned `ishl.wrap` | direct binding; value operand spelled as a typed literal or earlier named const equal to one | `r != 0` |
| S9 | const-array subscript | `let x = c[i]`, `c` a bare named const `Array<T, N>` | `min(c) <= x <= max(c)` |
| S14 | nonconstant `*` | domain discharged by the interval-product route only | the four-product hull |
| [ENT-6] images | exact `+`, `-`; `*` with a constant operand; integer `cvt` | any binding | exact affine image; every other integer operation receives a fresh full-range atom |
| [ENT-5] | `cvt.checked` | evaluation | conditional context: the payload equals the input |

### 1.2 Every integer row of the operation table

"None" means the result is a fresh term and atom carrying only its type
range.

| [OP-1] row | Result | L0 fact today | Affine image today |
|---|---|---|---|
| `+wrap`, `-wrap` | T | constant offset when provably in range | none |
| `*wrap` | T | none | none |
| `+`, `-` | T | constant offset | sum or difference |
| `*` | T | S14 hull on the interval-product route only | scaled when one operand is constant |
| `+defined`, `-defined`, `*defined` | Bool | goal origin | — |
| `+checked`, `-checked` | Result | constant-offset `Ok` arm in a direct match | — |
| `*checked` | Result | none | — |
| `/` | T | unsigned `q <= a`; literal `k*q <= a`; product consequence | none |
| `%` | T | unsigned `r < d`; signed literal divisor interval | none |
| `/defined`, `%defined`, `/checked`, `%checked` | Bool, Result | goal origin; none | — |
| `ineg.wrap`, `ineg` | T | none | none |
| `ineg.defined`, `ineg.checked` | Bool, Result | goal origin; none | — |
| `==` `!=` `<` `<=` `>` `>=` | Bool | comparison origin | — |
| `cvt` (integer pairs) | Dst | equality | copy |
| `cvt.checked`, `cvt.defined` | Result, Bool | conditional context; goal origin | — |
| `iand` | T | unsigned `r <= a`, `r <= b` | none |
| `ior`, `ixor`, `inot` | T | none | none |
| `ishl.wrap` | T | unsigned `r != 0` for a spelled one | none |
| `ishr.wrap`, `ishl`, `ishr` | T | none | none |
| `ishl.defined`, `ishr.defined` | Bool | goal origin | — |
| `irotl`, `irotr`, `ibswap`, `imulhi` | T | none | none |
| `ipopcount`, `iclz`, `ictz` | u32 | none | none |
| `+sat`, `-sat`, `*sat` | T | none | none |
| `imin`, `imax` | T | none | none |
| `iabs.wrap`, `iabs` | T | none | none |
| `iabs.defined`, `iabs.checked` | Bool, Result | goal origin; none | — |
| `reinterpret` (integer pairs) | T | none | none |

Three probes on the measured compiler confirm rows the table marks "none"
where a fact is obviously true: `iand(value_i32, 255_i32)` cannot prove
`cvt::<i32, u8>`, `slot *wrap 8_u64` with `slot < 8` cannot index a 64-slot
array, and exact `ineg` of a value in `0..=100` cannot prove
`negated + 100_i32`.

### 1.3 Why each row exists

Recovered with `git log -S` on the rule text and the compiler's derivation
kinds:

- **S7 constant offsets, checked arms, S9 and the retired S8.** The
  obligation-discharge batch-1 candidate (`81b73664b`, 2026-08-06) followed
  the [hand simulation](../obligation-discharge/SIMULATION.md) of utf8parse,
  deflate and sha256, whose L0 assumed "constant propagation incl. const-array
  element ranges, linear arithmetic (transitivity, ± constants, halving)".
  sha256's schedule reads `extend_index -wrap 2_u64`; deflate's
  `code_lengths[code_length_order[code_index]]` needed S9. S8's midpoint
  halving was struck by the [candidate review](../obligation-discharge/CANDIDATE-REVIEW.md)
  because no corpus program wrote it, which also noted that "the manifestly
  more-demanded shape ... the `irem` remainder bound `r < n` ... is absent
  while midpoint is present, i.e. selection tracked anticipation, not need".
- **Unsigned `iand` and the shifted one.** v0.28 (`1d0633e85`, 2026-08-15)
  added "only two measured unsigned direct-binding sources" for deflate's
  `read_bits`: the [acceptance probe](../obligation-discharge/ACCEPTANCE.md)
  isolated the chain `high != 0`, `mask = high - 1`, `value <= mask`,
  `value < high`, and recorded ior, ixor, right-shift, signed and
  local-one shapes as deliberate near misses.
- **Unsigned quotient with a literal divisor.** `d5fd20985` (2026-09-01),
  within the source-carried proof work that became v0.41; its tests are
  halving (`count / 2 <= count`, `2 * (count / 2) <= count`), the image
  binary-search midpoint certificates name.
- **Remainder bounds.** The source-carried proof implementation (`1c8c596b9`,
  2026-09-02, activated as v0.41), after the review quoted above.
- **Runtime divisors and the product consequence.** v0.57 (`cabb96de4`,
  2026-09-13) for the compute kernels' runtime block decomposition
  ([runtime division images](../compute-model/DESIGN.md#runtime-division-images)).
- **S14.** v0.45 (`69cec6cca`) from the
  [binary-arithmetic sweep](../binary-arithmetic/README.md#the-finding): an
  admitted product discarded its interval and the following `+` failed.
- **S5 for every integer pair.** Widened from total pairs by the proved
  conversion redesign (`62e4a744f`, 2026-09-23).

### 1.4 Asymmetries

- `/` by `2^k` bounds its quotient; `ishr` by `k`, the same value for an
  unsigned operand, bounds nothing. `*` by a constant has an exact image;
  `ishl` by a constant, which is that multiplication when no bit is lost, and
  `*wrap` by a constant that cannot wrap have none.
- `iand` has upper bounds; `ior` has no lower bound and `ixor` no bit-length
  bound. Signed `iand` by a nonnegative mask gets nothing.
- `ishl.wrap(one, n) != 0` depends on how the one is spelled: a local bound
  to one gets nothing, and the conformance case `ent3-neg-stage8b-local-one`
  asserts that rejection.
- A product's interval depends on which of three domain routes succeeded.
- `ineg` and `inot` are affine functions of their operand but receive fresh
  atoms; `imin`, `imax`, `iabs`, the saturating rows and the counting rows,
  whose ranges are elementary, publish nothing.
- The checked-arm fact exists only for a constant offset in a direct match,
  while `cvt.checked` has the general conditional transport.
- The signed remainder needs a literal divisor; the unsigned one accepts a
  term.

## 2. Evidence

All verdicts come from `whitefootc` built at `efe40194a` (this branch's base)
with `perl .github/run-check.pl aof-build cargo build --manifest-path
compiler/Cargo.toml --profile gate --bin whitefootc --locked --offline`
(exit 0, 70.8 s; binary SHA-256
`a0043d9ffd148132365d033616c942afed4bf0495f179476f646fc18f6499285`; Rust
1.98.1, Linux x86_64), invoked as `whitefootc --emit-llvm -o
<scratch-root>/<name>.ll <sources>`. Exit 0 is acceptance and exit 1 a source
rejection; every rejection below is a `Semantics/Source` issue whose rule and
residual are quoted.

### 2.1 The shift case

| Source of `high_word` | Result |
|---|---|
| `let hi = ishr(x, 32_u32); let narrow = cvt::<u64, u32>(hi);` | OP-6, residual `cvt.defined::<u64, u32>(ishr(x, 32_u32))` |
| the same with `ishr.wrap` | OP-6, residual `cvt.defined::<u64, u32>(ishr.wrap(x, 32_u32))` |
| `let masked = iand(hi, 4294967295_u64);` then `cvt` of `masked` | accepted |
| `match cvt.checked::<u64, u32>(hi)` with an `Err` arm returning zero | accepted |

`opt -O2` (LLVM 18.1.3) reduces both accepted forms to the same body:
`%v2 = lshr i64 %v0, 32`, `trunc`, `ret`. The mask and the checked branch
exist only in the source. The same shape is
[i01](probes/i01-high-word.wf) of the sweep below.

### 2.2 Corpus census

Method: in `tests/programs` (88 files) and `lib/containers` (6 files), list
every narrowing conversion, checked conversion, constant mask, shift, bit
operation, division, remainder and clamp with `grep`, read the guards and
proof statements around each, and for every workaround found remove it in a
copy and check the copy with the program's own bundle. Eleven unchanged
originals were accepted as controls. The search is by these operation
spellings, so a workaround spelled some other way would be missed.

**Sites that need a new operation fact (3).** Each copy is rejected today:

| Program | Workaround | Rejection without it |
|---|---|---|
| `telemetry_packet.wf` | `propagate cvt.checked::<u32, u8>(first_word)` after `ishr.wrap(value, 24_u32)` | OP-6 `cvt.defined::<u32, u8>(ishr.wrap(value, 24_u32))` |
| `dir_walk.wf` | a `value_if` clamp to 126 plus `cvt.checked` with an unreachable fallback; the literal `give` delivers no relation, and `imin(cursor, 126_u64)` gets no fact | OP-6 `cvt.defined::<u64, u8>(imin(cursor, 126_u64))` |
| `dir_walk.wf` | guard `name_at <= name_end` after `name_end = name_at +wrap name_count`, where `name_count` is `lo \| (hi << 8)` and `name_at <= 4096` | FN-8 `name_at <= name_end` at `open_directory_from_factory` |

**Workarounds already unnecessary today (18).** Each copy is accepted:
14 checked conversions (the four `low_byte` helpers in `tests/programs/parallel/`,
`io_complete_first_slice`, `io_open_and_read`, `tcp_client`, telemetry's
three masked bytes, and four in the deflate decoders) and 4 guards (deflate's
`code_count <= 19`, `distance_count <= 32` and `bounded < 19` after `% 19`,
and wfgrep's `digit < glyphs` after `% 10`). The existing facts that discharge
them are the `iand` and remainder rows, dominating comparisons, the
`read_bits` and `read_at` postconditions and the counted-range binder.
`io_propagate_open`, `io_vacant_read` and `io_write_prefix` have the same
shape and were not checked individually. The checked conversions are the
mechanical translation of the former Result-returning `cvt` when the proved
form arrived (`62e4a744f`), not evidence about the rule proposed here.

**Proof steps.** The corpus has 41 `use` lines in 8 files and 293
`invariant` lines. The `use` blocks scale a premise by a runtime or literal
factor or chain more than two premises (bfs, histogram, prefix, stencil,
radix scatter, grow-vector, priority-queue, byte_string); none bounds an
operation result. The local invariants state lengths, capacities, coverage
and products; the two priority-queue lemmas about `(at - 1) / 2` are already
automatic. Operation facts remove none of them.

The corpus undercounts demand because it was written against the current
menu: deflate passes both `count` and `mask` to `read_bits` so that the
existing `iand` row can bound the result, and wfgrep's guards are input-length
checks that no operation fact could remove.

### 2.3 Idiom sweep: alternatives, predictions and selection criterion

This section was committed in `d17282bb5` before any probe was compiled; its
text below is unchanged.

This section is recorded before the sweep is run. The corpus under
`tests/programs` was written in the language that has the gap, so it
under-reports demand (the
[binary-arithmetic sweep](../binary-arithmetic/README.md#what-was-measured)
made the same observation). The sweep therefore also checks natural forms of
common integer idioms, written without masks or guards whose only purpose is
a proof, each with a real consumer obligation (a narrowing `cvt`, a subscript,
an exact operation's domain, or a call requirement). The probes are in
[probes/](probes/); names starting `c` are controls expected to be accepted
today.

Candidate alternatives, from smallest to largest:

- **M, menu extension.** [ENT-3.S7] gains one row: a direct binding of
  unsigned `ishr` or `ishr.wrap` by a written constant amount `k` below the
  width establishes `r <= max(T) >> k` and, for an admitted term operand `a`,
  `r <= a`.
- **K, static intervals.** Every pure total integer-valued row establishes
  the constant interval its fixed transfer table computes from each operand's
  static interval: a literal or named constant is its value, every other
  operand its full type range, restricted by the row's own proved domain.
  No closure is read.
- **I, closed intervals.** The same table, with each term operand's interval
  taken from its strongest closed L0 bounds through Z at the binding, the
  query [ENT-3.S7]'s wrapping-offset row already makes.
- **D, difference intervals.** I, plus for each admitted term operand `x` the
  table's interval of `r - x` over the operand box, published only where it is
  narrower than what the result and operand intervals already imply through Z.
- **X, exact images.** D, plus a wrapping add, subtract or multiply proved not
  to wrap, and a shift by a constant proved to lose no bits, take the affine
  image of the corresponding exact operation.

Predicted verdicts (A accept, R reject) before any probe was compiled:

| Probe | Consumer obligation | Today | M | K | I | D | X |
|---|---|---|---|---|---|---|---|
| i01-high-word | `cvt` u64 to u32 | R | A | A | A | A | A |
| i02-top-byte | `cvt` u32 to u8 | R | A | A | A | A | A |
| i03-base64-sextet | subscript below 64 | R | A | A | A | A | A |
| i04-field-then-shift | subscript below 16 | R | R | R | A | A | A |
| i05-bitset-word | subscript below 4 | R | R | R | A | A | A |
| i06-hash-top-bits | subscript below 64 | R | A | A | A | A | A |
| i07-clamp-constant | subscript through a loop bound | R | R | A | A | A | A |
| i08-clamp-length | subscript through a loop bound | R | R | R | R | A | A |
| i09-floor-then-decrement | exact `-` domain | R | R | A | A | A | A |
| i10-popcount-table | subscript below 65 | R | R | A | A | A | A |
| i11-leading-zero-class | exact `-` domain, subscript | R | R | A | A | A | A |
| i14-pack-fields | `cvt` u32 to u8 | R | R | R | A | A | A |
| i15-xor-bytes | `cvt` u32 to u8 | R | R | R | A | A | A |
| i16-u16-decode | `cvt` u64 to u16 | R | R | R | A | A | A |
| i17-wrap-sum-order | exact `-` domain | R | R | R | R | A | A |
| i18-absolute-unsigned | `cvt` i32 to u32 | R | R | A | A | A | A |
| i19-heap-parent-shift | subscript below a length | R | A | R | R | A | A |
| i20-swap-under-max | two subscripts below a length | R | R | R | R | A | A |
| i22-variable-top-bits | `cvt` u64 to u16 | R | R | R | A | A | A |
| i23-low-bits-table | subscript below 32 | R | R | R | A | A | A |
| i24-saturating-consumed | exact `-` domain | R | R | R | R | A | A |
| c12-mask-ring-slot | subscript below 1024 | A | A | A | A | A | A |
| c13-quotient-row | subscript below 8 | uncertain | | | | | |
| c17-exact-sum-order | exact `-` domain | A | A | A | A | A | A |
| c21-residue-byte | `cvt` u64 to u8 | A | A | A | A | A | A |

Selection criterion. Each step up the ladder M, K, I, D, X must be justified
by at least two sweep idioms that today's compiler rejects, that the larger
alternative accepts by its written table, and that the smaller one does not.
Selection stops at the first step that fails. An idiom that today's compiler
accepts leaves the demand count. A rejected control stops the analysis until
its cause is understood. The corpus census is reported separately and does not
enter this count, because it was measured before the criterion was written.
The hand derivations are predictions; the implementation round checks them
mechanically, and measured checking cost may still move the selection down a
step under the cost criterion recorded with the validation plan.

### 2.4 Sweep results

Every "Today" prediction held. All 21 idioms are rejected, each at its
predicted consumer:

| Probe | Rule | Residual |
|---|---|---|
| i01 | OP-6 | `cvt.defined::<u64, u32>(ishr(x, 32_u32))` |
| i02 | OP-6 | `cvt.defined::<u32, u8>(ishr.wrap(word, 24_u32))` |
| i03 | OP-4 | `index < alphabet.len` |
| i04 | OP-4 | `index < digits.len` |
| i05 | OP-4 | `word_index < deref(words).len` |
| i06 | OP-4 | `index < heads.len` |
| i07 | OP-4 | `at < deref(values).len` |
| i08 | OP-4 | `at < deref(destination).len` |
| i09 | OP-2 | `first -defined 1_u64` |
| i10 | OP-4 | `index < classes.len` |
| i11 | OP-2 | `64_u32 -defined leading` |
| i14 | OP-6 | `cvt.defined::<u32, u8>(ior(iand(low, 15_u32), ishl(iand(high, 15_u32), 4_u32)))` |
| i15 | OP-6 | `cvt.defined::<u32, u8>(ixor(iand(left, 255_u32), iand(right, 255_u32)))` |
| i16 | OP-6 | `cvt.defined::<u64, u16>(ior(cvt::<u8, u64>(deref(bytes)[at]), ishl(cvt::<u8, u64>(deref(bytes)[at + 1_u64]), 8_u32)))` |
| i17 | OP-2 | `end -defined start` |
| i18 | OP-6 | `cvt.defined::<i32, u32>(iabs(delta))` |
| i19 | OP-4 | `parent < deref(keys).len` |
| i20 | OP-4 | `left < deref(values).len` |
| i22 | OP-6 | `cvt.defined::<u64, u16>(ishr(hash, 64_u32 - bits))` |
| i23 | OP-4 | `bits < table.len` |
| i24 | OP-2 | `budget -defined remaining` |

Controls c12, c17 and c21 are accepted. The uncertain c13 is rejected with
OP-4 `row < rows.len`. The proof it needs adds the listed image
`8*row <= cell` to the L0 bound `cell <= 63` and tightens by 8; that is not one
of `AUTO`'s families, which pair only listed premises and try a strongest L0
image alone. So even the existing division row cannot bound a quotient by its
dividend's constant bound. Its alternative verdicts were not recorded in
advance; derived afterwards, K cannot bound it (the dividend's type range
divided by 8) and I can (`63 / 8 = 7`).

Two probes were respelled after the first run for reasons unrelated to
proofs, keeping their idiom and obligations: i20 exchanged two `u64` slots
with `swap`, which [OP-11] refuses for copy places, and now reads and writes
them back; i23 named a binding `entry`, a reserved spelling.

Applying the criterion to the 21 idioms recorded in advance:

| Step | Idioms the larger accepts and the smaller does not | Count | Outcome |
|---|---|---|---|
| M to K | i07, i09, i10, i11, i18 | 5 | justified |
| K to I | i04, i05, i14, i15, i16, i22, i23 | 7 | justified |
| I to D | i08, i17, i19, i20, i24 | 5 | justified |
| D to X | none | 0 | stop |

Counting c13 by its later derivation would add one idiom to the K-to-I step
and change no outcome.

The criterion selects D. M accepts i19 through its `r <= a` relation, which K
lacks; that does not change any step. Within D, every relation the five
D-step idioms need is either an order relation (`imin`, `imax`, `ishr`,
`-sat` against an operand) or an offset relation (`+wrap` that cannot wrap).
The recommendation therefore publishes those two kinds only (section 3.3);
the general difference interval of D adds relations no probe uses.

### 2.5 Sufficiency of the predicted facts

The table verdicts above are hand derivations. To check the consumer side
mechanically, [sufficiency.wf](probes/sufficiency.wf) replaces each idiom's
operation by a pure helper whose verified `ensures` states exactly the fact
the smallest predicted accepting alternative would establish (for example
`result <= 4294967295_u64` for i01, `result <= room` for i08,
`result >= start` for i17); the helper proves it with a guard, and
[ENT-3.S12] publishes it at the caller's binding. The whole file, covering all 22
rejected probes, is accepted.

Three mutations of that file show the check is not vacuous: widening i01's
bound to `4294967296_u64` is rejected at the `cvt`, relating i08's result to
`count` instead of `room` is rejected at the subscript, and weakening i17's
relation to `result >= 0_u64` is rejected at `end - start`.

The three corpus sites pass the same simulation in their full programs:
telemetry with `result <= 255_u32` for the shifted word; dir_walk with
`result <= 126_u64` for the clamp; and dir_walk without its order guard, with
`result <= 65280_u64` for the shifted high byte, `result <= 65535_u64` for the
merged length and `result >= start` for the wrapping sum. Adding the no-wrap
precondition `start <= 18446744073709486080_u64` to the simulated sum, which is
what the offset relation requires, is still accepted, so the condition is
derivable at that call.

This establishes that the stated facts suffice where they are established. It
does not establish the table's arithmetic (section 3.2 reports an exploratory
check of it) or an implementation's cost.

## 3. Candidate criteria

### 3.1 Alternatives

The sweep ladder (M, K, I, D, X) is defined in section 2.3. Alternatives
outside it:

- **Keep the menu (status quo).** Leaves every asymmetry of section 1.4.
- **Declared operation contracts.** Give OP-1 rows prelude-style `ensures`.
  A one-datum-a-side template can state `r <= a` but not an interval that
  depends on operand intervals, so the rule would split across two mechanisms.
- **Verified helpers in libraries.** What section 2.5 simulates. Every project
  would restate operation semantics as contracts, and the helper still needs
  a guard or mask inside.
- **A bit-pattern (known-bits) domain.** More precise for masks, but a second
  fact family with its own joins, kills and closure; intervals and order
  relations cover every observed idiom.
- **Intervals on nested goal trees.** Source has no nested operation
  ([GRAM-9]); only contract-clause trees nest, where no probe needs a bound.

### 3.2 Result intervals and their soundness

A **corner hull** is the least and greatest value of an expression over every
combination of operand-interval endpoints. It is exact for an expression that
is monotone in each operand whenever the other operands are fixed, in either
direction: fixing all but one operand puts that operand's extreme at an
endpoint, and repeating this for each operand reaches a corner. Addition,
subtraction, multiplication, truncating division on each sign of the divisor,
`floor(a / 2^s)`, `a * 2^s`, `-a`, `M + m - a` (`inot`), `abs` on each sign,
`min`, `max`, clamping and `floor(a * b / 2^K)` are all such expressions, so
the hull is sound for them. Where the direction depends on a sign (the
divisor, `abs`), the interval is split into its negative and nonnegative
parts, a divisor's nonnegative part starting at one because the discharged
domain excludes zero.

Bit rows are not monotone and have fixed formulas valid when the operands are
nonnegative, because a nonnegative two's-complement value is its unsigned
binary number: `a & b <= min(a, b)`; `max(a, b) <= a | b <= a + b` and
`a | b < 2^bitlen(max(a, b))`; `a ^ b <= a | b`; `popcount(a) <= bitlen(a)`;
`clz(a) = K - bitlen(a)`; `ctz(a) <= bitlen(a) - 1` for `a >= 1`. `iand`
needs only one nonnegative operand: the result's sign bit is then clear and
its bits are a subset of that operand's.

Wrap modes: [OP-2] returns `wrap_T(z)`, which equals `z` whenever `z` is in
T, so a wrap row whose exact corner hull lies in T has the exact row's
interval, which is S7's existing condition for constant offsets; otherwise it
has only its type range. `ineg.wrap` and `iabs.wrap` return the minimum for
the minimum and follow the exact row only when `a0 > m`.

Shifts: [OP-8] masks a `.wrap` amount to `k & (K - 1)`. The amount interval
`[k0, k1]` is therefore the effective amount when `k1 <= K - 1`; otherwise any
amount in `[0, K - 1]` is possible. An exact shift's discharged domain already
bounds the amount by `K - 1`. `ishr` is arithmetic (floor) for signed T and
logical for unsigned T, both `floor(a / 2^s)` on their operand values. `ishl`
has a result interval only when every corner of `a * 2^s` is in T with
`a0 >= 0`, because then no bit leaves the value and none reaches a signed
sign bit. This subsumes the shifted one: `1 * 2^s` for `s` in `[0, K - 1]` is
`[1, 2^(K-1)]`, within an unsigned T, whatever the one's spelling.

Arithmetic: every endpoint is computed with checked i128 arithmetic, as
[ENT-6]'s interval-product rule already does; an unrepresentable endpoint (a
`u64` `imulhi` corner, for instance) establishes nothing, so a failure can
only lose a fact. Exact rows establish only after their own obligation
succeeds, so their operands lie in the admitted domain.

Generic bodies: a type-parameter-typed value is not an L0 fragment type in
the schema judgment ([ENT-1]), so no fact arises there; a const-generic
operand contributes its type's range. Concrete instances compute numerically.

An exploratory enumeration, run once from a scratch script that was not
retained, compared the section 4.3 table with the [OP-2] and [OP-8]
semantics for every row except `irotl`, `irotr` and `ibswap`, which establish
only single values. For 4-bit signed and unsigned types it covered every
operand interval and every shift-amount interval within `[0, 9]`; for 8-bit
types, intervals over a fixed endpoint set, sampling at most 250 operand
boxes per row; and the three `reinterpret` rows at both widths. Negation and
absolute value ran for signed types only, as [OP-1] admits. No result bound
or relation failed in 76,027,581 evaluations. Three seeded errors were each
reported: a remainder upper bound one too small, `-sat`'s order relation made
strict, and `+wrap` published without its no-wrap condition. The enumerative
tests of section 4.5 are the retained form of this check.

### 3.3 Relational facts: order and offset

The relation criterion is stated per row, not per demanded idiom.

- **Order relations** hold for every admitted operand value (for the marked
  rows, every nonnegative value): `iand` below each nonnegative operand; `ior`
  above each operand; `imin` below and `imax` above each operand; `ishr` below
  its value operand; unsigned `/` and `%` below the dividend and `%` below the
  divisor; `imulhi` below each operand; `iabs` above its operand; `+sat` above
  and `-sat` below the first operand. Rows where the order depends on the
  other operand's value (`*`, `ishl`, `*sat`) or does not exist (`ixor`,
  `inot`, `ineg`, rotations) have none.
- **Offset relations** belong to the additive rows: `r - a` lies in `b`'s
  interval for `+`, and in the negated interval for `-`, for the exact rows
  and for their wrap forms when the hull proves no wrap. With a constant `b`
  this is S7's constant-offset equality.

Considered and not selected: D's full difference interval of `r - x` for
every row and operand (for example `r - a` in `[3 a0, 3 a1]` for `4 * a`).
It is sound by the same corner argument, but no probe uses it and each
relation adds closure work.

### 3.4 Where facts attach

At the destinations S5 already images: an `ordinary_let_rhs` binding and a
[SET-1] commit value, with operand intervals read before the commit's target
kill; and, through [ENT-5]'s existing conditional transport, a checked row's
private success payload (replacing S7's direct-match shape for `+checked` and
`-checked`). A `return`, `give` or call-argument value has no term, and
contract clauses keep their goal trees; writers bind first, as they already
must. An operand that is not a term (a subscripted read, for instance)
contributes its type range, and S9 remains the source for const-array reads.

An admitted term includes a measure term. The current S7 rows read their
operands through a reader that omits measure terms, so
`let r = x % deref(src).len;` establishes nothing although the specification
admits the measure (the todo item "Some ENT-3 sources read no measure
operand"). The implementation must read every S7 operand with one complete
ENT-2 term reader, which repairs that item for the replaced rows.

### 3.5 Interaction with the fact system

- **[ENT-4].** Results are ordinary difference bounds; no closure rule is
  added. A contradictory state already derives everything.
- **[ENT-5].** A result interval names only `r` and Z, so writes to operands
  leave it true, exactly as S14's interval behaves; an order or offset
  relation dies with either term. Joins keep the weakest common bound; facts
  established in a loop body do not reach the next head, and loop-carried
  values need header invariants as before.
- **[ENT-6] and `AUTO`.** No fact enters the listed affine premises, so the
  pair family and the two-premise cutoff are untouched. Implicit type bounds
  through Z already relate every pair of terms, so the L0-to-affine index keeps
  its size and only its bounds tighten. The division images remain listed
  premises, unchanged.
- **[PRF-1] redundancy.** A block becomes redundant only if the new facts let
  `AUTO` prove its target. The corpus blocks scale a premise or chain more
  than two. Among the 45 conformance cases with blocks, the operations whose
  facts change are mostly exact `+` and `-`, whose offset relations restate
  the affine image the checker already has, plus the two binary-search
  midpoint blocks (`2*half <= span` with `lo < hi`), which still need their
  pair because no order relation is scaled. This is a reading, not a proof;
  the implementation round must recheck every block mechanically.
- **Conformance verdicts.** `ent3-neg-stage8b-local-one` flips from reject
  to accept. Seven other negative cases whose facts the table touches
  (`fn9-neg-wrapped-negation-nonnegative`, `fn9-neg-saturation-is-not-wrapping`,
  `op2-neg-zero-quotient-predecessor`, `prf1-neg-wrapped-index-certificate`,
  `ent5-neg-wrapped-index-replacement`, `op4-neg-join-wrapped-increment-bound`,
  `op2-neg-break-removes-exhaustion-bound`) keep their verdicts by hand
  analysis; two of them may change from unproved to refuted, still under
  FN-9. The compiler test enumerating "no S7 source" shapes (local one,
  `ior`, exact `ishl`) changes with the rule.
- **Other consumers.** More proved separations can widen parallel
  permission, so `--par-ledger` output may change; lowering receives no new
  fact family ([backend facts](../../../design/compiler/backend-facts.md)).

[Section 5.3](#53-predictions-and-outcomes) compares these predictions with
the implementation.

### 3.6 Checking cost

- **Premises.** Zero new listed premises.
- **Closure reads.** Each operation binding reads its operands' closed
  bounds, which S7's wrapping-offset row already does. The corpus has about
  1,180 integer operation bindings (589 `+wrap`, 62 `-wrap`, 79 `*wrap`, 122
  `+`, 69 `-`, 37 `*`, 20 `/`, 45 `%`, 158 named rows), of which about 490
  constant-offset wrap bindings read the closure today: roughly 2.4 times as
  many reads. The remembered closed view and incremental insertion make a read
  after one or two new facts a repair, not a full closure.
- **Closure middles.** The closure skips terms that carry only implicit range
  edges. A term whose only live edges run to Z cannot improve a path either
  (entering and leaving it adds a nonnegative cycle in a consistent state), so
  result intervals need not add middles if that exclusion is extended; order
  and offset relations do add middles.
- **Derivation roots.** [DIAG-2] retains every S7 fact "even when no later
  query consumes it". With S7 facts at most of the roughly 1,180 operation
  bindings, the proposal switches S7 roots to the ordinary reachability
  pruning.
- **Not measured here.** No prototype was built in this round. The
  implementation round measures under the criterion in section 4.5.

## 4. Recommendation

### 4.1 The rule

Replace [ENT-3.S7] with one operation-fact source: every integer-valued
operation row publishes on the value it binds the result interval that its
fixed table computes from its operands' closed intervals, plus the row's order
or offset relations to each operand term; a checked row's success payload
receives the same facts through the conditional Result context. Retire S14
into the `*` row. Keep S5 and S9 and the unsigned division images (the
literal scaled image and the captured product consequence) unchanged.

Grounds: the rule is a fixed table evaluated once per binding from the closed
state that already exists there, so it is deterministic and terminating with
no search and no fixed point; a writer can evaluate it by hand; each entry
follows from [OP-2] and [OP-8] by the corner argument or an elementary
bit-level fact; the pre-recorded criterion selects this step; the three
corpus sites and all 22 sweep probes become provable; and no premise enters
`AUTO`. What could change it: measured checking cost failing the criterion
below, or a program that needs the exact sum identity of a non-wrapping wrap
operation (which would reopen X).

### 4.2 Rejected alternatives

- **Status quo and menu extension (M).** An `ishr` row fixes the reported
  case and repeats the pattern the todo entry describes; K alone accepts five
  more sweep idioms.
- **Static intervals (K).** Reads no closure, but loses every chain in which
  one bounded operation feeds another (seven recorded idioms and c13, among
  them the 16-bit decode). It is the fallback if closure reads prove too
  costly.
- **Closed intervals without relations (I).** Misses five sweep idioms whose
  bound is another term: a clamp to a runtime length, a heap parent by shift,
  a pair bound by `imax`, a saturating subtraction and a wrapping sum.
- **Full difference intervals (D as defined).** Adds relations no probe uses.
- **Exact images for non-wrapping wrap forms and lossless shifts (X).** No
  probe distinguishes it from D; the exact row already states the identity,
  and with the new intervals the exact row is provable where X would apply
  (dir_walk can write `name_at + name_count`). Reopen when a proof needs the
  identity and the writer cannot use the exact row.
- **Declared operation contracts, library helpers, a bit-pattern domain,
  nested goal-tree intervals.** Section 3.1.

### 4.3 Proposed specification text

This is the text as proposed; [section 5.1](#51-specification-v072) records
how the landed amendment differs.

Replace the complete [ENT-3.S7] item with:

> [ENT-3.S7]
> - S7 (operation facts).
> S7 is the source of facts about the value an integer operation row produces; an integer `cvt` keeps [ENT-3.S5]'s equality instead.
> At an `ordinary_let_rhs` binding and at a [SET-1] commit value whose right-hand side is one row of the table below with integer operands, after that row's own obligations have succeeded, establish on the bound value r the row's result bounds and, for each operand that is an admitted [ENT-2] term x, the row's relations to x.
> Each operand has one interval, read in the closed state where the right-hand side is evaluated, which for a commit precedes the target kill: a typed literal or integer-typed named const has its value; an admitted term has the least and greatest values its strongest closed L0 bounds through Z permit; every other operand, a const generic included, has its type's interval.
> At a contradictory point S7 establishes nothing, every relation being derivable there already.
> Below, T is the selected type with width K and interval [m, M]; a in [a0, a1] and b in [b0, b1] are the first and second operands' intervals; for a shift, b is the amount and s is [b0, b1] when b1 <= K - 1 and [0, K - 1] otherwise; and bitlen(x) is the number of binary digits of x >= 0, with bitlen(0) = 0.
> The corner hull of an expression is the least and greatest of its values at every combination of operand-interval endpoints; where a row names a split, the operand interval is first divided into its negative part and its nonnegative part, a divisor's nonnegative part starting at one, and the hull is taken over the nonempty parts.
> Every value is computed over mathematical integers with checked i128 arithmetic.
> When every operand interval is one value, the result bounds are the row's exact value whether or not its condition holds.
> Otherwise a row establishes nothing when its condition does not hold or a computation is unrepresentable.
> Result bounds are established as `lo <= r` and `r <= hi` through Z, each limited to the interval of the row's result type; an order relation is the stated difference bound; an offset relation is the pair of difference bounds placing `r - x` in the stated interval.
> These facts have the ordinary [ENT-5] support of their terms.
>
> | Row | Condition | Result bounds | Relations |
> |---|---|---|---|
> | `+` | none | corner hull of a + b | offset: r - a in [b0, b1], r - b in [a0, a1] |
> | `+wrap` | the corner hull of a + b lies in [m, M] | that hull | as `+` |
> | `-` | none | corner hull of a - b | offset: r - a in [-b1, -b0] |
> | `-wrap` | the corner hull of a - b lies in [m, M] | that hull | as `-` |
> | `*` | none | corner hull of a * b; when [ENT-6]'s interval-product rule discharged the domain, over the operand intervals that rule selected | none |
> | `*wrap` | the corner hull of a * b lies in [m, M] | that hull | none |
> | `+sat`, `-sat`, `*sat` | none | the corner hull of the unsaturated result, each end clamped to [m, M] | when a0 >= 0 and b0 >= 0: `+sat` a <= r and b <= r; `-sat` r <= a |
> | `/` | none | corner hull of the truncating quotient, b split | when a0 >= 0 and b0 >= 0: r <= a |
> | `%` | none | [min(0, max(a0, 1 - D)), max(0, min(a1, D - 1))] with D = max(abs(b0), abs(b1)) | when a0 >= 0 and b0 >= 0: r <= a and r < b |
> | `ineg` | none | corner hull of -a | none |
> | `ineg.wrap` | a0 > m | corner hull of -a | none |
> | `iabs` | none | corner hull of abs(a), a split | a <= r |
> | `iabs.wrap` | a0 > m | corner hull of abs(a), a split | a <= r |
> | `inot` | none | corner hull of M + m - a | none |
> | `iand` | a0 >= 0 or b0 >= 0 | 0 <= r, and r <= x1 for each operand interval [x0, x1] with x0 >= 0 | r <= x for each such operand term x |
> | `ior` | a0 >= 0 and b0 >= 0 | [max(a0, b0), min(a1 + b1, 2^bitlen(max(a1, b1)) - 1)] | a <= r and b <= r |
> | `ixor` | a0 >= 0 and b0 >= 0 | [0, min(a1 + b1, 2^bitlen(max(a1, b1)) - 1)] | none |
> | `ishl`, `ishl.wrap` | a0 >= 0 and the corner hull of a * 2^s lies in [m, M] | that hull | none |
> | `ishr`, `ishr.wrap` | none | corner hull of floor(a / 2^s) | when a0 >= 0: r <= a |
> | `imin` | none | [min(a0, b0), min(a1, b1)] | r <= a and r <= b |
> | `imax` | none | [max(a0, b0), max(a1, b1)] | a <= r and b <= r |
> | `imulhi` | none | corner hull of floor(a * b / 2^K) | when a0 >= 0 and b0 >= 0: r <= a and r <= b |
> | `ipopcount` | a0 >= 0 | [1 when a0 >= 1, otherwise 0; bitlen(a1)] | none |
> | `ipopcount` | a0 < 0 | [0, K] | none |
> | `iclz` | a0 >= 0 | [K - bitlen(a1), K - bitlen(a0)] | none |
> | `iclz` | a0 < 0 | [0, K] | none |
> | `ictz` | a0 >= 1 | [0, bitlen(a1) - 1] | none |
> | `ictz` | a0 <= 0 | [0, K] | none |
> | `reinterpret` between integer types | [a0, a1] lies in both endpoint types' intervals | [a0, a1] | offset: r - a in [0, 0] |
> | `reinterpret` between integer types | a1 < 0 and the destination is unsigned | [a0 + 2^K, a1 + 2^K] | none |
> | `reinterpret` between integer types | a0 > the signed destination's maximum | [a0 - 2^K, a1 - 2^K] | none |
>
> `irotl`, `irotr` and `ibswap` establish only the single-value case above.
> An unsigned exact division `q = a / d` whose operands are admitted terms or constants also captures the exact immutable value images of q, a, and d, and when d is a positive written integer literal k it retains the separate affine value image `k*q <= a`.
> [The following three paragraphs of the current item, from "At a later direct ordinary exact-multiplication binding" through "cannot retarget a captured relation to the replacement; a still-live alias of an old value may continue to use the old relation under [ENT-5].", stay unchanged.]
> A signed division, a nonterm operand, and a division at any other destination capture no image.
> No other operation shape establishes an S7 fact.

Dependent edits, stated as before and after:

- [ENT-3.S14]: delete the item; add "The label S14 is retired, not reused: its
  product interval is the `*` row of [ENT-3.S7]."
- [ENT-5], after the paragraph beginning "Evaluating
  `cvt.checked::<Src, Dst>(x)` with integer Src and Dst", add: "Evaluating
  `+checked`, `-checked`, `*checked`, `/checked`, `%checked`, `ineg.checked`
  or `iabs.checked` creates a conditional context the same way: its private
  success parameter denotes the exact row's mathematical result, and at that
  parameter it establishes exactly the [ENT-3.S7] facts the exact row
  establishes on an ordinary let of its result; its `Err` outcome carries
  none." This replaces the current S7 sentence beginning "For a `match` whose
  scrutinee is directly `p +checked k`".
- [ENT-6] interval-product paragraph: "established by [ENT-3.S14]" becomes
  "established by [ENT-3.S7]'s `*` row".
- [ENT-2] clause (g): "used only to carry constant bounds, S7's exact
  mathematical-zero disequality, and [ENT-6]'s normalized integer-domain
  components" becomes "used only to carry constant bounds and [ENT-6]'s
  normalized integer-domain components".
- [DIAG-2]: remove the sentence "Every new S7 fact is retained even when no
  later query consumes it.", the sentences introducing `BitAndBound`,
  `ShiftOneNonzero`, `UnsignedDivisionBound`, `UnsignedRemainderBound` and
  `SignedRemainderBound`, and the sentence beginning "A signed row where
  unsigned is required"; keep the `UnsignedDivisionProduct`,
  `RequirementAffineImage` and closing sentences; and add: "`OperationFact`
  roots each [ENT-3.S7] result bound, order relation and offset relation and
  carries the selected row, its destination, the relation, and as parents the
  closed operand bounds the row read; a literal or named-constant operand
  contributes its value and no parent. [ENT-3.S7]'s literal scaled division
  image cites the root of the quotient's order relation together with the
  exact q and a value images." S7 roots then follow the ordinary reachability
  pruning.

The amendment archives v0.69 and declares v0.70. Its accepted set widens
except where [PRF-1] finds a newly redundant block (none expected) and where
`ent3-neg-stage8b-local-one` flips.

### 4.4 Proposed design-tree revision

Node `language/checks-and-proofs/automatic-facts`. Replace the decision
beginning "An admitted nonconstant integer multiplication's proved interval
is published" with:

> Decision: Every integer operation row other than a conversion publishes on the value it binds the interval one fixed table computes from its operands' closed intervals, rows bounded by an operand publish that order, additive rows publish their offset, and a checked row's success payload receives the same facts, because rows added one proof pattern at a time bounded a division by a power of two but not the equal right shift, bounded a masked shift but not the shift alone, and bounded a shifted one only when the one was spelled as a literal, which forced proof-only masks, guards and checked conversions, instead of a row per demanded idiom or intervals read from types and literals alone.

and add to its `Rejected:` list:

> - Intervals from operand types and literals alone: rejected because a bounded operation feeding another loses its bound, which seven of the 21 recorded [sweep idioms](../../../research/investigations/automatic-operation-facts/DESIGN.md#24-sweep-results) need.
> - The difference interval of every result against every operand: rejected because no probe needs a relation beyond order and offset, and each relation adds closure work.
> - Exact affine images for wrap forms that cannot wrap: rejected because the exact row already states that identity and becomes provable wherever these images would apply.

The division-image decision stays; its first words become "An unsigned exact
division ... publishes the quotient's order relation and captured images".

### 4.5 Validation plan

**Specification-derived evidence.** For each table row, one accepted case at
the exact endpoint the row gives and one rejected case one past it (for
example `ishr(x_u64, 32)` converts to u32 while `ishr(x_u64, 31)` does not;
`imin(count, 16)` bounds a loop over 16 slots and `imin(count, 17)` does
not). Additional cases:

- signed rows: `iand` with one nonnegative operand accepted, with none
  rejected; `ishl` into the sign bit rejected; arithmetic `ishr` of a negative
  interval; `%` with a runtime signed divisor;
- wrap forms: a non-wrapping `+wrap` publishes its offset, a possibly wrapping
  one publishes nothing; a `.wrap` shift whose amount interval exceeds `K - 1`
  uses `[0, K - 1]`;
- support: a result interval survives an operand write; an order relation dies
  with its operand; a commit publishes through its commit value;
- measure operands: each relation row with a direct `deref(p).len` operand and
  with the same length bound first, accepted alike, and a write that kills the
  measure;
- checked rows: the `Ok` payload of `start +checked count` carries the offset
  relation, the `Err` arm nothing;
- the single-value case (`irotl(1_u32, 3_u32)` indexing a nine-slot table),
  an unrepresentable `u64` `imulhi` (nothing), and a generic `T` operation
  (nothing in the schema);
- the sweep probes as accepted cases, with `ent3-neg-stage8b-local-one`
  replaced by an accepted local-one case and a rejected case whose shifted
  value may be zero.

**Programs to simplify in the same change.** `telemetry_packet.wf` (four
bare conversions) and `dir_walk.wf` (`imin` clamp; drop the order guard or
use exact `+`). The 18 workarounds of section 2.2 are independent of the
ruling and can be removed with it or separately. Update
[P16](../../../docs/patterns.md#p16-choose-a-conversion-interface-from-the-intended-behavior)
to say that operation results carry their table interval.

**Soundness tests of the table.** For 8-bit types, enumerate operand
intervals drawn from a fixed endpoint set and compare each row's result
bounds, order and offset relations with the exact image computed by
enumerating every operand value; include every wrap and shift-amount edge.

**Cost, criterion recorded before timing.** Build the merge base and the
candidate with the gate profile in separate worktrees; on one host under the
verification lock, warm both once, then run five alternating pairs of
`whitefootc --emit-llvm` over every program bundle the program tests compile,
the `lib/containers` programs, and the
[checking-cost fixtures](../proof-certificate-architecture/CHECKING-COST.md#flow-selection-2026-09-16),
and time the conformance run once per compiler. Select the candidate when:
the summed median over the program bundles rises by at most 10%; no bundle
rises by both more than 25% and more than 50 ms; the checking-cost fixtures
rise by at most 10%; every unchanged source emits byte-identical LLVM; and
every `--par-ledger` difference is explained. Temporary counters report the
S7 facts established, closure reads at bindings and middle terms per closure,
and the checking time is split into formation, automatic derivation,
certificate checking and fact propagation before any cost is attributed.
The predicted cause of a regression is fact propagation at operation
bindings; its falsifier is a same-source variant of the candidate that reads
only static operand intervals, which must remove most of the regression if
the prediction holds. On failure, attribute the cost, apply the section 3.6
mitigations and remeasure; if it still fails, return the measured tradeoff
between D and the static-interval fallback K to the owner.

**Gate.** `make check` on the exact revision, including the full conformance
adapter and every program test.

### Decisions for the owner

1. Approve the direction: one S7 table of result intervals with order and
   offset relations, replacing the S7 menu and S14 (D as refined in 3.3), or
   select K, I or M instead.
2. Approve reading operand intervals from the closed state at each operation
   binding, about 2.4 times today's binding-time reads, subject to the cost
   criterion; K is the fallback that reads none.
3. Approve moving the checked-arm fact into the conditional Result context for
   every checked integer row.
4. Approve the flip of `ent3-neg-stage8b-local-one` and the end of
   spelling-keyed shift facts.
5. Approve or adjust the cost thresholds in section 4.5.
6. Confirm X stays deferred with the stated reopening condition.
7. Decide whether the 18 already-unnecessary workarounds are removed with the
   implementation or separately.

Ruling, 2026-09-25: decisions 1 to 6 as recommended, with X recorded in
[docs/todo.md](../../../docs/todo.md) under its reopening condition; the
workarounds of decision 7 are removed in the implementing change.

## 5. Implementation

### 5.1 Specification v0.73

Main reached v0.70 with modular compilation (#85), v0.71 with the
readonly-field terms (#118) and v0.72 with the standard library (#129) before
the amendment landed, so the amendment lands as v0.73, in one merge with the
one-argument rows (#126) and the repair wording (#125): it archives main's
v0.72 bytes as `spec/kernel-spec-v0.72.md` and titles the active file v0.73.
The edits were made against v0.70's text and apply unchanged to v0.71 and
v0.72: the one paragraph both earlier amendments and this one edit is
[ENT-2]'s term list, where #118 rewrote clause (b) and this amendment edits
clause (g). The ENT-3 edits follow section 4.3, with these
differences:

- Relations are established only to an operand that is an admitted term other
  than a literal or named-const value. A relation to a constant restates the
  result bounds through Z, and each relation adds closure work (section 3.3).
- A const generic operand, a symbolic constant term of [ENT-2] clause (c), is
  read like any other admitted term: its interval comes from its closed L0
  bounds rather than its type, and it takes the row's relations.
- The two sentences on a one-value operand box and on a failed condition
  became one positive rule: the result bounds are the exact value when every
  operand interval is one value, and otherwise the table's bounds when the
  condition holds and every computation is representable. The condition
  column says "always" where the draft said "none". Neither changes a row.
- A sentence after the table states why the rows hold: each corner hull is
  exact because its expression is monotone in each operand while the others
  are fixed, and each bit row's bounds read nonnegative operands, whose
  two's-complement bits are their binary digits.
- `irotl`, `irotr` and `ibswap` are table rows whose condition never holds, so
  the single-value rule is their only fact, instead of a separate sentence.
- S14 is deleted without the proposed retirement sentence: the specification
  records no history, and S2, S3, S8 and S10 are already absent without one.
- The division capture names its excluded forms directly: a signed
  division, a nonterm operand, a conditional success payload and every other
  division form capture no image.
- [DIAG-2] gives each `OperationFact` as parents the closed operand bounds
  the row read, or, for a `*` row over the interval-product rule's intervals,
  that multiplication's discharged IntegerDomain derivation, and prunes it by
  ordinary reachability. The literal scaled division image cites the
  quotient's order relation `q <= a`, or the quotient's upper result bound
  when the dividend is a literal or named-const value.

The design-tree amendment, approved and applied to
[language/checks-and-proofs/automatic-facts](../../../design/language/checks-and-proofs/automatic-facts.md),
replaces the product-interval decision as section 4.4 proposes and lists M, K,
I, the full difference interval and X as rejected. The division-image
decision keeps its words: the quotient's bound it publishes is now the `/`
row's order relation.

### 5.2 Compiler

- `compiler/src/semantic/entailment/flow/operation_facts.rs` holds the table
  as data over checked `i128` intervals and reads no fact state. Its tests
  compare every row with an independently written reference semantics at
  every operand value of every 4-bit interval box and of 8-bit boxes whose
  endpoints lie on type edges, around zero and at powers of two, and check
  the single-value rows and the reported shift. Ten seeded unsound edits (a
  remainder bound, a saturating relation, a wrap condition, the shift-amount
  rule, the `ior` ceiling, the `iand` sign condition, the reinterpret offset,
  the population-count bound, the divisor split and one exact value) each
  fail these tests, and the unmodified table passes them.
- One establishment path in `flow/sources.rs` serves an ordinary `let`, a
  [SET-1] commit value and a checked row's success payload [ENT-5]. It reads
  every operand with the one term reader that includes measure terms, closes
  the state at most once per binding and only when an operand is a
  nonconstant term, and records each fact as an `OperationFact` whose parents
  are the derivations of the operand bounds it read.
- Removed: the S14 event kind, the checked-arm outcome facts and the
  `BitAndBound`, `ShiftOneNonzero`, `UnsignedDivisionBound`,
  `UnsignedRemainderBound` and `SignedRemainderBound` roots with their
  retention records.
- Modular compilation (#85) needs nothing further here. S7 reads and
  establishes facts inside one body, and a callee in another module reaches
  its caller only through its contract, as before. The module-form case
  `ent3-pos-s7-callee-in-another-module` checks both sides: the callee
  proves its published bound with its own `ishr` row, and the caller's rows
  read that bound at the call. `ent3-neg-s7-no-callee-contract-across-modules`
  shows that without the contract nothing about the result crosses.

### 5.3 Predictions and outcomes

- **Conformance verdicts.** Section 3.5 predicted one change. Three cases
  moved from reject to accept, each as a direct consequence of a table row.
  Each is renamed to a positive id and paired with a new negative that keeps
  the property the old case protected:

  | Former case | Now | Why it is accepted | Paired negative |
  |---|---|---|---|
  | `ent3-neg-stage8b-local-one` | `ent3-pos-s7-local-one-shift` | the local one has the closed interval [1, 1], so `ishl.wrap` bounds the shift to [1, 2^31] (decision 4) | `ent3-neg-s7-shift-may-be-zero` |
  | `op4-neg-callee-minimum-subscript` | `op4-pos-callee-minimum-subscript` | `imin(x, 7_u64)` is at most 7, an index of the eight-entry table | `op4-neg-callee-minimum-past-end` |
  | `op2-neg-branch-quotient-images` | `op2-pos-branch-quotient-interval` | each branch's quotient by two has the interval [0, 2^63 - 1], which names only the result, survives the `give` join and proves the doubled product's domain | `op2-neg-branch-runtime-quotient-images` |

  The seven other negatives named in section 3.5 keep their verdicts, and no
  other case changed. The new cases are one accepted case per row family at
  the endpoints its rows give (`ent3-pos-s7-arithmetic-rows`,
  `-division-rows`, `-bit-rows`, `-shift-rows`, `-order-rows`,
  `-count-rows`, `-reinterpret-and-single-values`), the support and reader
  cases (`ent3-pos-s7-measure-operand`,
  `ent3-pos-s7-interval-survives-operand-write`,
  `ent3-neg-s7-relation-dies-with-operand`), the checked payloads
  (`ent5-pos-checked-payload-facts`, `ent5-neg-checked-product-one-past`), the
  two module-form cases above, and rejected cases one past an endpoint or
  outside a condition
  (`ent3-neg-s7-shift-right-one-past`, `-wrap-may-wrap`,
  `-signed-and-without-mask`, `-shift-into-sign-bit`,
  `-wrap-amount-past-width`, `-signed-remainder-one-past`,
  `-popcount-one-past`, `-reinterpret-mixed-signs`,
  `-unrepresentable-high-product`). They carry the sweep idioms in condensed
  form, so the probes did not become cases one by one.
- **[PRF-1].** Predicted: no newly redundant certificate. One became
  redundant. Radix scatter's `output_count <= 33554944_u64` needed
  `use full_low_bound; use full_high_bound; use 2 times capacity_limit;`; the
  `+` row now reads both addends' closed upper bounds, so the sum's interval
  alone proves the invariant. The block is removed and the invariant remains
  a checked statement.
- **Compiler tests.** Ten tests changed with the rule. Four
  originating-acceptance canaries used `imin` or `imax` as a shape without a
  fact and now sit one past the endpoint each row gives. Three tests whose
  target the new facts make provable in L0 (conditional affine transport, an
  indexed separation, an [MSR-4] bridge premise) were given operands that
  still need the mechanism under test. The checked-offset kill test now
  expects the payload's interval to survive the write and only its relation
  to die, and two test helpers learned the new node.
- **Measure operands.** `let r = x % deref(src).len;` now bounds `r` by the
  measure, which repairs the todo item for every S7 row and the checked
  payloads; the item stays open for the other readers it lists.
- **Corpus.** The three sites of section 2.2 and all workarounds found there
  are removed: 17 checked conversions (the 14 counted there and the three
  same-shape ones) become bare `cvt`, the 4 guards are gone, telemetry's
  shifted byte converts with bare `cvt`, and dir_walk clamps with
  `imin(cursor, 126_u64)`, converts with bare `cvt` and drops its order guard.
- **Sweep.** The candidate accepts all 21 sweep idioms, the uncertain c13,
  the three controls and the sufficiency file.
- **Diagnostics.** As section 3.5 allowed, `fn9-neg-wrapped-negation-nonnegative`
  and `fn9-neg-saturation-is-not-wrapping` now report their postcondition
  refuted rather than unproved, still under FN-9; the other five named
  negatives keep their verdicts, and the two that report a disposition keep
  it.

### 5.4 Checking cost

Measured on 2026-09-25 on a 4-CPU Intel Xeon (2.80 GHz) Linux 6.18.44 host
with 15 GiB of memory and Rust 1.98.1. The host verification lock was held
for every timing run. The base is the merge base with main, `6b66e5237`, and
the candidate is `9db4b6361`. Each was built with the gate profile in its own
tree. Both were warmed once, and every source then got five alternating pairs
of `whitefootc --emit-llvm`, with the order reversed on odd rounds. The binary
SHA-256 identities are:

```text
base      bdc8450950ae12c6f23660be412b2aaf871f191d26bffbbd97736a00cb105433
candidate adeeae0b46412002388157c61196c32815770773b4dfdfedc0fc662b0de720c1
```

The raw pairs are in [cost/](cost/): one row per invocation with its time,
exit status and LLVM SHA-256, for the program bundles and for the three
fixture runs.

| Criterion (section 4.5) | Threshold | Result |
|---|---|---|
| Summed median over the 82 program bundles the program tests compile, `lib/containers` included, each compiler compiling its own tree | at most +10% | 18.091 s against 18.091 s, +0.0% |
| A bundle rising by both more than 25% and more than 50 ms | none | none; the largest rises are the three deflate bundles, +14.6% to +17.4% (+69 to +90 ms) |
| Checking-cost fixtures (fixed, growing and control at 16, 64 and 256, fixed at 4096) | at most +10% | summed medians −2.5% and −1.1% in two full runs; see below for one cell |
| LLVM of unchanged sources | byte-identical | identical for all 65 bundles whose sources are unchanged, and for radix scatter, whose edit removed only a `use` block |
| `--par-ledger` differences | each explained | none for unchanged sources; the 16 bundles with edited sources differ only in line numbers, or where a removed `match` or guard changed the statement sequence the ledger pairs |

By source, the 65 bundles with unchanged sources sum to −0.6% and the 17 with
edited sources to +1.5%. To separate the rule's cost from the edits, both
compilers also compiled the base tree's sources of the edited bundles with the
most work. The deflate bundles take 16% to 20% longer (+79 to +111 ms, LLVM
identical), dir_walk 10% longer, and wfgrep and telemetry within 3%. The
table's cost therefore concentrates in operation-dense code, and it stays
under the per-bundle line even there. On its own edited sources dir_walk is
31% faster, because the removed workarounds were themselves checked.

In the first fixture run, fixed-256 was 41% slower (48.1 ms against 67.8 ms).
Its candidate samples ranged from 46.7 to 81.3 ms, while every other cell
stayed within ±9%. The fixtures contain no integer operation binding, so S7
does no work in them. Two repeats measured the cell at +3.3% and +6.4%, with
candidate samples between 45.3 and 51.8 ms. During the repeats the host's
load average was 1.5 to 2.7 while one compiler process ran at a time, so
processes outside the lock shared the machine.

The conformance run, every case once with `whitefootc --check`, took 32.6 s
for the base over its 1266 cases and 33.6 s for the candidate over its 1290.

No threshold failed, so the static-interval fallback K is not selected and
no cost was attributed. The temporary counters and the falsifier of section
4.5 exist for attribution and were not needed. The fixtures came from the
proof-use-cost runner's generator and compare mode with its three `own`
spellings removed. The committed generator still writes the parameter syntax
from before the ownership redesign, which current compilers reject with
GRAM-3, and its historical comparisons run the compilers that required that
syntax.

## Concerns and opportunities

Within the assessed scope (ENT-3's operation-derived sources, their ENT-4,
ENT-5, ENT-6, PRF-1 and DIAG-2 consumers, and the corpus and probes above),
the recommended rule removes the asymmetries of section 1.4 by one table
instead of more rows, reuses the existing L0 representation, supports, joins
and conditional Result transport, and adds no premise family. Three concerns
remain and are addressed by the plan rather than assumed away: the checking
cost of about 2.4 times more binding-time closure reads is unmeasured; a
stronger automatic set can make an existing certificate redundant under
PRF-1, which only a mechanical recheck settles; and the table itself joins
the trusted base, so its arithmetic needs the enumerative soundness tests.
One opportunity is declined: S9 could read the index interval and bound an
element read by the entries the index can reach, but no program needs it.

## Reproduction

Probe sources are in [probes/](probes/). Each probe and variant was checked
with the command in section 2 from `<repository-root>`; the corpus variants
edit one site of a copy placed under `<scratch-root>` and check it with its
program's bundle (the deflate bundle is `raw_deflate.wf`,
`raw_deflate_dynamic.wf`, `raw_deflate_dynamic_decode.wf`,
`raw_deflate_boundary.wf`). The optimized bodies come from `opt -O2 -S` on the
emitted modules. The probes stay beside this document as the selection
experiment's evidence, as the deciding probes of other investigations do: they
reproduce each idiom's rejection before the change, its acceptance after it
(section 5.3) and the sufficiency simulation of section 2.5. The landed rule's
regression evidence is the conformance cases listed in section 5.3, not these
files.
