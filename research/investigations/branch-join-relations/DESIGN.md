# Relations kept where branches rejoin

## Question

When the two branches of an `if` update different variables, a relation that
holds on every branch is lost where they rejoin. Binary search is the plainest
program that shows it:

```wf
loop (
  invariant ordered: low <= high,
  invariant within: high <= keys^.len
) {
  if low >= high {
    break;
  }
  ...
  if probe < key {
    set low = mid + 1_u64;
  } else {
    set high = mid;
  }
}
```

`within` fails [INV-1] at the backedge although one arm leaves `high` unchanged
and the other sets `high = mid` with `mid < high <= keys^.len`. Restating both
relations with a local `invariant` at the end of each arm does not help, and a
single unconditional `set high = mid;` passes. A separate gap: after
`low < high`, the midpoint `mid = low + (high - low) / 2_u64` is not provably
below `high`.

The same limit was met before: PR #169's lockstep growth under a branch
(probes p3a, p3f and p3h of the
[aggregate-postconditions investigation](https://github.com/mbbill/Whitefoot/blob/research/aggregate-postconditions/research/investigations/aggregate-postconditions/DESIGN.md#separate-finding-lockstep-growth-under-a-branch)),
the `docs/todo.md` item "Conditional measure preservation needs a precise
remaining diagnosis", and the `find_byte_or_end` witness of the
[writer-lost-facts investigation](https://github.com/mbbill/Whitefoot/blob/spec/loop-exit-and-give-facts/research/investigations/writer-lost-facts/DESIGN.md#shape-6-lockstep-arrays-and-struct-fields)
(PR #172). Snowghost's modules carry guards and clamps whose false branch
cannot happen, and one restructures a binary search into a tail-recursive
function whose `requires` carry the relations a loop could not.

The question is which relations should survive a join, under [ENT-1]'s
constraints: automatic derivation is specification-fixed, deterministic and
terminating, no solver takes part, and harder proofs arrive as written `use`
steps [PRF-1]. This record edits no specification, design tree or compiler;
its recommendation goes to the owner.

## Method

- **Rules read.** [ENT-2] through [ENT-6] of specification v0.78, above all
  [ENT-5]'s joins and loop rule and [ENT-6]'s image join and affine-premise
  join; [INV-1]; [PRF-1]; the `/`, `-` and `ishr` rows of [ENT-3.S7]; [ENT-4];
  the `design/language/checks-and-proofs` subtree with its ancestors;
  `design/skill/SKILL.md`; and the decision occasions of AGENTS.md ("How work
  proceeds"), which absorbed the former `docs/practice.md#decision-work`.
- **Probes.** The five probes of the question (b1, b3 to b6) and 22
  supplementary probes written for this record are kept in [probes/](probes/).
  Each is checked with `whitefootc --check <probe>.wf`.
- **Compilers.** The base is the gate-profile `whitefootc` of `9c4579fba`
  (specification v0.78, this branch's base). The prototype is a scratch copy
  of the same tree with five switches, each read from one environment variable
  and off by default; [prototype.patch](prototype.patch) is its complete diff:

  | Switch | Emulates |
  |---|---|
  | `WF_JOIN_B` | candidate (b): a proved unit-coefficient invariant also establishes its L0 relation |
  | `WF_JOIN_A` | candidate (a): each join input projects matching affine premises into L0 before the L0 join |
  | `WF_MID_DIV` | midpoint row: `/` and `ishr` publish `r < a` when a0 >= 1 and the divisor or shift is at least 2 or 1 |
  | `WF_MID_SUB` | midpoint row: exact `-` also bounds its result by the operands' closed difference |
  | `WF_LOOP_EXIT` | the approved loop-exit ruling of PR #172: header conclusions are not removed on edges leaving their loop |

  The Snowghost modules use text literals, which `main` does not have yet, so
  the census uses the same base and prototype patch applied to `69447dc6c`
  (`spec/text-literals`, v0.78 main merged with the text-literal amendment,
  which changes no entailment source).
- **Census.** Each clamp block and each guard whose condition is a comparison
  (or a `band`/`bor` of comparisons) in the Snowghost renderer modules is
  replaced by the local `invariant` its false branch rules out, and the module
  is rechecked under each configuration. The procedure and scope are in
  [Census](#census).
- **Cost.** The timing method of the
  [result-proof-transport investigation](../result-proof-transport/DESIGN.md#compilation-cost-and-erased-execution):
  prebuilt gate-profile compilers, one warmup per compiler and workload, five
  alternating samples, medians, maximum RSS. Details in [Cost](#cost).

## Probes and diagnoses

Base verdicts (v0.78, `9c4579fba`):

| Probe | Shape | Base verdict |
|---|---|---|
| b1 | binary search, no guard | OP-4 `mid < keys^.len`, Unproved |
| b3 | b1 plus `if mid >= high { return }`, three-way branch | INV-1 `within`, Backedge |
| b4 | b3 with a two-way branch | INV-1 `within`, Backedge |
| b5 | b4 with the update unconditional after an early return | accepted |
| b6 | b4 with both relations restated at the end of each arm | INV-1 `within`, Backedge |

### b3, b4 and b6: the join

Four rules produce the verdict together; the compiler implements each as
written.

1. [INV-1]: a proved header or local invariant becomes "one published affine
   fact" over the immutable value images of its proof point. It is never an L0
   fact. At the loop head the batch `h_low <= h_high`, `h_high <= m_len` is
   stated over the fresh header atoms `h_low`, `h_high` and the measure atom
   `m_len` of `keys^.len`.
2. [ENT-6], image join: a binding keeps its image only when every input holds
   the identical image, or the same non-delta form up to constants (common form
   plus a fresh delta atom). At b4's join `high` is `h_high` on the
   `probe < key` arm and `h_low + q` on the other (q the quotient atom), and
   `low` is `h_low + q + 1` against `h_low`; the forms differ, so both get fresh
   full-type atoms `L'` and `H'`.
3. [ENT-6], premise join: `h_high <= m_len` survives the join because it is
   canonically identical on both inputs, but no live binding has the image
   `h_high` any more.
4. [ENT-5], L0 join: the weakest common bound per ordered term pair survives.
   Neither input has an L0 bound between `high` and `keys^.len`, because the
   invariant never entered L0. On the `set high = mid` arm the relation was
   still derivable before the write (`mid < high` in L0 from the guard,
   `high <= keys^.len` as an affine theorem), but the pre-kill closure of
   [ENT-5] runs over L0 only, so nothing carries it across the kill through the
   commit value.

The backedge target `H' <= m_len` then has no parent in any [MSR-4] step.
`ordered` passes because its relation is in L0 on both inputs: on one arm the
commit value of `set low = mid + 1_u64` carries `v <= high` across the kill of
`low` [ENT-3.S5, ENT-5], and on the other S7's `+` offset gives `low <= mid`.
b5 passes because no join of different images happens: `high`'s image at the
backedge is `h_low + q`, and AUTO proves `h_low + q <= m_len` from the listed
pair (the division image `2q <= h_high - h_low`, `within`) and q >= 0. b6's
arm-end invariants are proved, but over different images (`h_high <= m_len`
and `h_low + q <= m_len`), so the canonical intersection keeps neither: "facts
are compared by canonical inequality and immutable value images rather than
invariant spelling" [INV-1].

**Verdict: the specification as written, not a compiler defect.** The loss has
two parts. The join (rule 2) discards the relation between the images, and the
L0 state that the join would have kept (rule 4) never held the relation,
because an invariant conclusion is affine only (rule 1).

### b1: the midpoint

[ENT-3.S7]'s `/` row publishes `half <= span` in L0 and, for a literal divisor,
the affine image `2*half <= span`. The subscript needs
`h_low + q <= m_len - 1`: sum the division image with the strict branch fact
`h_low - h_high <= -1`, halve over the integers, and add `within`. That is one
listed premise, one L0 image and a second listed premise under a tightening;
AUTO's families are zero, one or two listed premises, or one L0 image, each
followed by one DIRECT residual [ENT-6]. The `-` row's corner hull gives
`span` the interval [0, max] even under `low < high`, so no interval rule
reaches `half < span` either. **Verdict: the specification as written.** A
written certificate proves it (probe m3, and the conformance case
`prf1-pos-integer-tightening-midpoint`), as does the doubled blockless form
`2_u64 * mid + 1_u64 <= 2_u64 * high` when the length bound is an L0 fact
(`op4-pos-midpoint-automatic`); in a loop, where `within` is affine only, the
doubled form proves the relation but the subscript still fails (m2).

### Supplementary probes

| Probe | Shape | Base | Cause |
|---|---|---|---|
| p3a, p3h | #169's conditional lockstep push, with and without restated equalities | INV-1 `same` | the join, as b4 |
| p3f | two scalars grown together under a branch | INV-1 `same` | the join, as b4 |
| p3d, p3g | the same with the push unconditional, or with the relation from `requires` | accepted | controls |
| cm1 | a length-preserving call in one arm (the todo item's control) | INV-1 `retained` | the join, as b4 |
| cm2, cm3 | cm1 with the measures captured before the branch, or the call unconditional | accepted | controls |
| fb1 | #172's `find_byte_or_end` witness | FN-9 `result <= length` | the lower clamp's join, as b4 |
| m2 | b1 with `invariant inside: 2_u64 * mid + 1_u64 <= 2_u64 * high;` | OP-4 | `within` is affine only |
| m3 | b1 with the two-use certificate for `mid < high` | INV-1 `ordered` | the certificate's conclusion is affine only |
| m4, m7 | non-loop midpoint with `requires lo < hi`, `/ 2` and `ishr` | OP-4 | the midpoint |
| m5 | upper-biased midpoint `hi - (hi - lo) / 2` (false) | OP-4 | correct refusal |
| m6 | b1 with `ishr(span, 1_u32)` | OP-4 | the midpoint |
| m8 | b1 with the blockless `invariant inside: mid < high;` | INV-1 `inside` | the midpoint |
| n1 | b4 with `set high` to an unbounded value | INV-1 `ordered` | correct refusal |
| n2 | a local `q <= r` invalidated in one arm | INV-1 `still` | correct refusal |
| n3 | cm1 whose callee publishes no length | INV-1 `retained` | correct refusal |
| n4 | a scaled lockstep `2 * a <= b` | INV-1 `double` | outside unit coefficients |
| n5 | a counted loop advancing `kept` by two in one arm | INV-1 `behind` | correct refusal |
| n6 | a scan writing its position before a `break` | FN-9 | correct refusal |

p3a, p3f, p3h, cm1 and fb1 fail for the same reason as b4: a relation that is
an affine theorem, not an L0 fact, meets a join where the binding's or the
measure's images differ. p3g passes because a `requires` is an S4 source with
an exact L0 projection, and the L0 join keeps it.

### Implementation correspondence

The compiler matches the rules above. `join_affine_states` in
`compiler/src/semantic/entailment/flow/domain.rs` implements the image join
(identical image, delta folding, common form plus delta, else a fresh atom),
keeps a measure atom only when every input has it, and intersects affine facts
canonically in `join_affine_facts`; `join_at` in `state.rs` is the weakest-bound
L0 join. `activate_loop_invariant_batch` in `flow/invariants.rs` and the
`Proof` statement in `flow/walk.rs` publish affine facts only; the existing
`checked_affine_relation_l0` projection is used only to name the right-hand
term for [MSR-4]'s step 6. `remove_active_loop_invariants` removes the header
batch on every loop exit, as v0.78 [ENT-5] states. The `/`, `ishr` and `-`
rows of `flow/operation_facts.rs` publish exactly the S7 table's relations and
corner hulls. No probe exposes a compiler defect.

## Candidates

C below is the candidate set of [MSR-4]'s step 6: Z, every live measure term
and every live own integer binding with an image. T is the number of L0 terms
of a function, P the length of the automatic affine-premise sequence and k the
number of inputs of a join.

### (a) A closed join over the difference-bound fragment

**Rule.** At every join, before [ENT-5]'s L0 join, each non-contradictory input
closes its difference-bound fragment: every affine premise whose coefficient
vector equals `img(x) - img(y)` for two candidates x, y in C is established as
the L0 bound `x - y <= c`, c corrected by the images' constants. The L0 join
then keeps, per ordered pair, the weakest bound every input entails, and a
later query reads it over the join's fresh atoms through the L0 image index.
This is the standard join of difference-bound matrices over the live bindings
and measure terms. `WF_JOIN_A` implements it.

**Soundness.** A premise is a theorem over atoms and each candidate's image
denotes its current value, so the projected bound holds of the current values
on that input; the weakest-bound join is already sound.

**Determinism and termination.** The candidates and premises at a join are
fixed by the source; the projection is one pass, independent of order, followed
by the existing closure.

**Cost.** Per input, forming the image differences is O(|C|^2) vector
operations and each premise is one lookup; at most O(|C|^2) bounds are added.
The bound per join is O(k * |C|^2 * a), a the atoms per image, below the
closure's O(T^3).

**What it cannot do.** It projects only premises that are exactly a difference
of two current images. On b4's `set high = mid` arm the needed
`h_low + q <= m_len` follows from two premises (the division image and
`within`) and the interval q >= 0, which is an AUTO derivation, not a
difference-bound edge; `h_high <= m_len` names an atom no candidate holds any
more. The relation dies at the write, before the join. The prototype confirms
it: (a) accepts b6, p3a, p3f, p3h, cm1 and fb1, and leaves b3, b4 and m3
rejected.

**Variant (a+).** Replace the matching premise by the full [MSR-4]
disposition: for each pair (x, y) with x or y among the D candidates whose
images differ across the inputs, compute on every input the tightest c that
AUTO's families prove for `img(x) - img(y) <= c`, and keep the largest. This
accepts b4, and with the midpoint rows b1 without any written step. Its bound is
O(k * |D| * |C| * (P^2 + |C|^2)) candidate residuals per join, quartic in the
live candidates when D is large, and each join's result feeds later joins. It
was not prototyped.

### (b) A proved unit-coefficient invariant also establishes its L0 relation

**Rule.** When a proved header or local invariant target, normalized over its
source leaves, is one unit-coefficient difference bound over [ENT-2] terms
(`x - y <= c`, `x <= c` or `c <= x` over own integer bindings, measure terms,
named consts or const generics, and both bounds of an `==`), the relation is
also established as an L0 fact over those terms with their ordinary [ENT-5]
support, at the point the target becomes a fact: after the `invariant_stmt`,
and for a header batch at the loop head after the batch is activated. This is
exactly what [ENT-3.S4] does for a requirement whose root is a comparison of
terms. The affine theorem is unchanged. `WF_JOIN_B` implements it with the
existing `checked_affine_relation_l0` projection.

**Soundness.** At the proof point every atom of the target is the current image
of the term it projects to, so the relation holds of the current values there.
It stays true until an event writes, consumes or ends the scope of a support
member, and [ENT-5] kills it on exactly those events, which are also the events
that retarget the images. A header batch holds at every head by [INV-1]'s
induction (base batch, every backedge), so its projection holds at every head.
Nothing is trusted that was not proved: the L0 relation is a consequence of an
already-proved target.

**Determinism and termination.** One or two L0 establishments per proved
target per proof point, with no search.

**Cost.** O(1) per proved target; the closure keeps its O(T^3) bound per
materialization and the join its O(T^2) bound. The L0 matrix may hold more
finite cells, which the closure then carries.

**What it admits.** Every unit-coefficient invariant relation now takes part in
[ENT-5]'s pre-kill closure (carried across a write by a commit value, a call
datum or a placement datum) and in the L0 join. Beyond the probes: a writer can
replace a guard whose false branch cannot happen by the `invariant` of its
negation, and the relation then survives later joins. Two side effects need
checking: a certificate whose target L0 closure now derives becomes redundant
and is rejected [PRF-1], and a reject conformance case whose relation is true
may become accepted.

### (c) Written join relations

**Rule.** The writer names the relations a join must keep, for example

```wf
if probe < key {
  set low = mid + 1_u64;
} else {
  set high = mid;
} keeps (low <= high, high <= keys^.len)
```

Each named relation is an [INV-1] target proved on every reaching input edge,
in that edge's state after its scope exits, and is published at the
continuation over the joined images. The soundness argument is the loop
header's: control reaches the continuation along one input, on which the
relation was proved of the current values.

**Cost.** One [MSR-4] disposition per relation per input. The form needs new
grammar, a canonical formatting rule, diagnostics, and a conformance family;
the writer must restate the relation at every join it crosses, including nested
joins.

**Relation to (b).** Under (b), (c)'s effect for a unit-coefficient relation is
already expressible without new syntax: a local `invariant` at the end of each
arm (probe b6) establishes the same L0 relation on every input, and the L0 join
keeps it. (c) adds only relations outside that fragment, such as `a + b <= n`
or `sum <= 255 * i`.

### Midpoint: two S7 rows

- **Strict quotient.** The `/` row's relation becomes `r <= a - 1` when
  a0 >= 1 and b0 >= 2 (and stays `r <= a` when a0 >= 0 and b0 >= 0); the
  `ishr` row's likewise when a0 >= 1 and the shift's s0 >= 1. Sound:
  floor(a / b) <= a / 2 < a for a >= 1 and b >= 2.
- **Difference of terms.** The result bounds of an exact `-` whose operands are
  both terms also intersect `[-c(b - a), c(a - b)]`, where c(u - v) is the
  tightest closed L0 bound on `u - v`. Sound: r = a - b exactly.

The strict-quotient row alone does not fire on the midpoint: `span = high - low` has
the L0 interval [0, max] under `low < high`, because the corner hull ignores
the operands' mutual bound. With both rows, `span >= 1` and `half < span`
are L0 facts, the subscript of b1 is automatic through the L0 image of
`half < span` and the premise `within`, and `mid < high` becomes a blockless
invariant AUTO proves (m8). `mid < high` itself is a three-term relation
(`mid = low + half`) and never an L0 fact, so the loop still needs (b) and one
written line, or a guard, to keep `ordered` through the join.

### Interaction with loops

- **Header batches.** (b) establishes the projection after activation, on the
  conservative head state from which continuing kills already removed the
  loop-carried facts; the base batch and its preheader state are untouched. A
  counted binder's projection dies with the hidden update, and the
  exact-exhaustion rule is unchanged.
- **Backedges.** The next-header target is still proved over the current
  images; (b) only adds L0 parents for [MSR-4]'s DIRECT step.
- **Loop exits (#172).** The approved ruling keeps header conclusions on every
  exit edge. Under (b) the L0 projection is an ordinary fact and leaves on the
  same edges, killed by any write before the `break` (n6 stays rejected). #172
  rejected "copy header conclusions into L0" as its fix for loop exits because
  retaining the affine theorem already removed every measured clamp; (b) is
  proposed for the join, where retaining the theorem is not enough (fb1), and
  keeps the retained theorem beside it.
- **The give carrier (#172).** Bounded relation delivery reads L0; under (b) a
  unit-coefficient invariant about the carrier is delivered like any other L0
  bound.
- **(a) and (c)** act only at joins and change no loop rule; (c) at a join
  immediately before the backedge proves its relations per input, which is
  the loop's own backedge obligation taken one join earlier.

## Discriminating criterion

Recorded after the probe diagnoses and the prototype's probe runs, and before
any census or timing result was read. A candidate is recommended for
implementation when, on a compiler implementing it and no other language
change:

1. **Probes.** For a join rule: b3, b4, p3a, p3f, p3h, cm1 and fb1 are
   accepted, and b5, p3d, p3g, cm2 and cm3 stay accepted. A rule that leaves
   b4 rejected does not answer the question. For the midpoint rows: m4 and m7
   are accepted and b1's first rejection leaves OP-4.
2. **Negatives.** n1 to n6 and m5 stay rejected by the same rule. Every
   conformance case keeps its verdict, except a reject case whose relation holds
   on every execution and is derivable under the candidate's stated rule; each
   such flip is listed with the reason its relation holds, and landing the
   candidate replaces that case by a negative the candidate still refuses. An
   accept or run case that becomes rejected, a redundant certificate included,
   is listed.
3. **Census.** The candidate removes at least one census item that neither
   v0.78 nor the approved loop-exit ruling removes. Between candidates that meet
   1, 2 and 4, the larger census count per specification change decides.
4. **Cost.** On the conformance corpus and on every census module, the median
   check time stays within 10% of the unmodified compiler and peak RSS within
   10%, measured as in [Cost](#cost); the synthetic scale family grows no faster
   than the base; the added work per join or proof point has a stated bound no
   larger than the existing [ENT-4] closure's.
