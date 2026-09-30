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

## Current replay protocol

The original v0.78 observations below are historical. The maintained
`prototype.patch` now applies to compiler source
`4459df880b04a7e87f46398969e1382b52637af0` (v0.82); the original patch is in
PR #178's `f892d870ece812c9950f054016b63520f17f1dbe` revision. The new base
already retains loop-exit facts and supports text literals, so the prototype
no longer emulates either change. Its switches remain research-only; no
production compiler or specification is changed by this investigation.

Before the new timing results, the replay retains the original 10% time/RSS
criterion and distinguishes three observations:

- `replay.py --mode verdicts` invokes the CLI on all saved probes, every
  conformance source at the pinned baseline, the sixteen pinned Snowghost
  modules, six collection modules and reconstructed program bundles. It records source-check outcomes, not native
  execution of conformance `run` cases. Base versus all-switches-off must
  agree; every candidate difference is reported without changing the manifest.
- `--mode cost` measures unmodified inputs with prebuilt compilers, one
  warmup and five measured rounds, rotating configuration order. Each row
  gives summed process wall time and maximum per-process RSS. Base, switches
  off, (a), (b), rows and (b)+rows run on every module and the complete
  conformance source set. (a+) already exceeds the original analytic work
  bound, so its cost replay is limited to the collections and the scale
  family; a full Snowghost cost pass cannot make it eligible. Its semantic
  conformance sweep still runs. Its extra Snowghost verdict checks were stopped
  after `proto::style` exceeded three minutes on one core under `WF_JOIN_AP=1`;
  that interruption is not a language rejection and supplies no completed
  timing sample. The remaining replay omits (a+) on Snowghost, not from the
  probes or conformance source sweep. No noisy single sample selects a rule.
- The scale family has 4, 8, 16, 32 and 64 independently guarded counters,
  each with a header bound and a local invariant, including a continuing
  backedge. This is a newly specified reconstruction, not the lost original
  chain generator. The full-module rewrites in `rewrites.patch` are likewise
  explicit new witnesses, not recovered bytes of the earlier scratch edits.

The lost census automation and its exact claimed 80 program bundles have not
been recovered. Before timing, the replay instead reconstructs 71 current
program bundles covering all 75 distinct `tests/programs` source files named
by `census.tsv`: ordinary files with `main`, plus the Slab, Indexed and Deflate
bundles used by `compiler/tests/programs/containers.rs` and `raw_deflate.rs`.
The script refuses an uncovered census source. Their cost is measured as one
sum, as the original program-cost method prescribed. This is a current,
auditable replacement inventory, not a reproduction of the missing historical
80-bundle list. (a+) is omitted here for the same analytic reason as on
Snowghost. A failed measured criterion is enough to withhold implementation;
passing this matrix still does not retroactively establish the historical
criterion. Census counts are neither a general usage distribution nor a
substitute for the rule's semantic grounds.

The replay requires Python 3.12 or newer and a Unix host with `wait4`.
Python here only extracts pinned source, invokes the compiler, and records
outcomes and OS resource usage. It implements no acceptance rule. Run it
explicitly under `.github/run-check.pl`; it is not a formal gate dependency.
The replay script, patches and result data serve this investigation and retire
when superseding evidence replaces the claims they support.

## Current source-verdict results

The [complete source-verdict table](verdicts.csv) and
[compiler/input identity](verdicts-identity.json) record 1,572 baseline
conformance cases, 30 probes, sixteen Snowghost modules, six collections and
five accepted scale inputs, plus 71 reconstructed program bundles: 1,700
workloads and 11,813 workload/configuration pairs. Every baseline outcome
equals the all-switches-off prototype. All original Snowghost, collection,
scale and program inputs are accepted under every measured configuration.
(a+) has no Snowghost/program rows for the reason above.
The sweep was resumed after fixing its parsing of the CLI's multi-module
diagnostic envelope; development scale inputs rejected for noncanonical
whitespace were corrected and rechecked, not used as cost samples.

The candidate changes exactly the six conformance source verdicts listed in
the historical [evaluation](#evaluation-against-the-criterion): (a), (a+) and
(b) accept `inv1-neg-sequential-guarded-steps`; (b) rejects the same three
formerly accepted certificate cases; the rows reject the same two midpoint
certificate cases. No manifest expectation or formal case was edited. These
are source-check results only; this investigation did not execute the
conformance `run` cases under a modified language prototype.

The original join witnesses still fail on the base, (b) accepts b3/b4/b6 and
the lockstep/measure witnesses, and the rows accept m4/m7. The negative
witnesses n1–n6 and m5 keep their original source-rule refusals. The narrowed
inference mechanisms therefore still answer the original problem on v0.82;
this observation alone does not settle their cost or approve a language rule.

## Reproduce the current replay

Create two clean scratch worktrees at
`4459df880b04a7e87f46398969e1382b52637af0`. In one, apply this directory's
`prototype.patch` with `git apply`; leave the other unmodified. Build each
with `make -C compiler build` (the target takes the shared check lock).
Clone `https://github.com/mbbill/Snowghost.git` into a scratch checkout; the
script extracts the four full commit IDs it records, not its current branch.
From this Whitefoot checkout, with the following shell variables naming the
built executables, the baseline source checkout (`replay_source`), Snowghost
checkout and a fresh output directory:

```sh
for mode in verdicts rewrites cost; do
  perl .github/run-check.pl branch-join-replay \
    python3 research/investigations/branch-join-relations/replay.py \
      --source-root "$replay_source" --base "$replay_base" --prototype "$replay_prototype" \
      --snowghost "$replay_snowghost" --output "$replay_output" --mode "$mode"
done
```

Round 0 of `cost.csv` is warmup; summarize rounds 1–5 by median wall time
and maximum RSS per workload/configuration. Divide each candidate by `base`
on that same workload. Keep `off` as an inert-prototype control. Each identity
JSON records executable, patch and replay-script hashes, the compiler source,
the host and the pinned Snowghost revisions. A source-verdict sweep may use
`--resume` after an interrupted invocation; it appends only missing recorded
workload/configuration pairs. Use a fresh output directory after changing a
compiler or source input. Tool failures stop the script rather than becoming
source rejections. Multi-module source diagnostics are retained from the CLI's
per-module stdout alongside its JSON driver summary.

## Method

- **Rules read.** [ENT-2] through [ENT-6] of specification v0.78, above all
  [ENT-5]'s joins and loop rule and [ENT-6]'s image join and affine-premise
  join; [INV-1]; [PRF-1]; the `/`, `-` and `ishr` rows of [ENT-3.S7]; [ENT-4];
  the `design/language/checks-and-proofs` subtree with its ancestors;
  `design/skill/SKILL.md`; and the decision occasions of AGENTS.md ("How work
  proceeds"), which absorbed the former `docs/practice.md#decision-work`.
- **Probes.** The five probes of the question (b1, b3 to b6) and 25
  supplementary probes written for this record are kept in [probes/](probes/).
  Each is checked with `whitefootc --check <probe>.wf`.
- **Compilers.** The base is the gate-profile `whitefootc` of `9c4579fba`
  (specification v0.78, this branch's base). The prototype is a scratch copy
  of the same tree with six switches, each read from one environment variable
  and off by default; [the historical prototype patch](https://github.com/mbbill/Whitefoot/blob/f892d870ece812c9950f054016b63520f17f1dbe/research/investigations/branch-join-relations/prototype.patch) is its complete diff:

  | Switch | Emulates |
  |---|---|
  | `WF_JOIN_B` | candidate (b): a proved unit-coefficient invariant also establishes its L0 relation |
  | `WF_JOIN_A` | candidate (a): each join input projects matching affine premises into L0 before the L0 join |
  | `WF_JOIN_AP` | variant (a+): each join input establishes in L0 the tightest bound AUTO's families prove for every pair of candidates one of whose images differs across the inputs |
  | `WF_MID_DIV` | midpoint row: `/` and `ishr` publish `r < a` when a0 >= 1 and the divisor or shift is at least 2 or 1 |
  | `WF_MID_SUB` | midpoint row: exact `-` also bounds its result by the operands' closed difference |
  | `WF_LOOP_EXIT` | the approved loop-exit ruling of PR #172: header conclusions are not removed on edges leaving their loop |

  The `WF_JOIN_AP` hunk was built into a separate binary, so the other
  configurations run and are timed without it. The Snowghost modules use text
  literals, which `main` does not have yet, so the census uses the same base
  and prototype patch applied to `69447dc6c` (`spec/text-literals`, v0.78 main
  merged with the text-literal amendment, which changes no entailment source).
- **Census.** Each clamp block and each guard whose condition is a comparison
  (or a `band`/`bor` of comparisons) in the Snowghost renderer modules,
  `lib/std/collections` and `tests/programs` is replaced by the local
  `invariant` its false branch rules out, and the module is rechecked under
  each configuration; a site whose removal needs a relation written elsewhere
  is rewritten by hand. The procedure and scope are in [Census](#census).
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
| sm0 | the todo item's sparse-map extent, reduced: a length-preserving call in one arm of a counted loop | INV-1 `rebuilt_extent` | the join, as b4 |
| sm1 | sm0 with the extent captured before the branch and restated after it | accepted | control |
| p3n | Snowghost's line-break lockstep without its room guards | FN-8 `push_run` | three affine premises, no join |
| n1 | b4 with `set high` to an unbounded value | INV-1 `ordered` | correct refusal |
| n2 | a local `q <= r` invalidated in one arm | INV-1 `still` | correct refusal |
| n3 | cm1 whose callee publishes no length | INV-1 `retained` | correct refusal |
| n4 | a scaled lockstep `2 * a <= b` | INV-1 `double` | outside unit coefficients |
| n5 | a counted loop advancing `kept` by two in one arm | INV-1 `behind` | correct refusal |
| n6 | a scan writing its position before a `break` | FN-9 | correct refusal |

p3a, p3f, p3h, cm1, fb1 and sm0 fail for the same reason as b4: a relation
that is an affine theorem, not an L0 fact, meets a join where the binding's or
the measure's images differ. p3g passes because a `requires` is an S4 source
with an exact L0 projection, and the L0 join keeps it. The todo item also
reports its full sparse-map loop refused with an explicit extent bridge; that
source is written in retired syntax and was not ported, and the reduced loop
with the bridge (sm1) is accepted, so this record does not reproduce that
remaining refusal.

p3n fails before any join. The requirement `starts.len < starts.cap` of
`push_run` follows from three header relations (`same`, `bounded`, `cap_s`)
and the counted range `i < length`; AUTO combines at most two listed premises
and reads only L0 in its residual [ENT-6], and the header relations are affine
only. **Verdict: the specification as written.** Snowghost's line-break module
guards every push instead (see [Census](#targeted)).

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
it: (a) accepts b6, p3a, p3f, p3h, cm1, fb1 and sm0, and leaves b3, b4, m3 and
p3n rejected.

**Variant (a+).** Replace the matching premise by AUTO's families: for each
ordered pair (x, y) of candidates with x or y among the D candidates whose
images differ across the inputs, each input establishes in L0 the tightest c
for which DIRECT, one listed premise, a pair of listed premises or one L0
image, each with its integer tightenings and a DIRECT residual, proves
`img(x) - img(y) <= c`; the L0 join then keeps the largest. `WF_JOIN_AP`
implements it. It is sound for the reason (a) is, and deterministic and
terminating because each family is finite. Its bound is
O(k * |D| * |C| * (P^2 + |C|^2)) residual checks per join, each over O(a)
atoms: quartic in the live candidates when D is large, and each join's result
feeds later joins. The prototype accepts every join probe (b3, b4, b6, p3a,
p3f, p3h, cm1, fb1, sm0) and, with the midpoint rows, b1, m2 and m8 with no
written step at all; p3n, which has no join, and n1 to n6 and m5 stay
rejected. Its cost was measured only in single unpaired runs of the
prototype: `collections::vector` 49 ms against 77 ms for v0.78 and (a+),
`hash_map` 153 ms against 553 ms, `ordered_map` 370 ms against 1709 ms.

### (b) A proved unit-coefficient invariant also establishes its L0 relation

**Rule.** When a proved header or local invariant target, normalized over its
source leaves, is one unit-coefficient difference bound over [ENT-2] terms
(`x - y <= c`, `x <= c` or `c <= x` over own integer bindings, measure terms,
named consts or const generics, and both bounds of an `==`), the relation is
also established as an L0 fact over those terms with their ordinary [ENT-5]
support, at the point the target becomes a fact: after the `invariant_stmt`,
and for a header batch at the loop head after the batch is activated. This is
what [ENT-3.S4] does for a requirement whose root is a comparison of terms,
applied to the normalized form an invariant target already has. The affine
theorem is unchanged. `WF_JOIN_B` implements it with the
existing `checked_affine_relation_l0` projection.

**Soundness.** At the proof point every atom of the target is the current image
of the term it projects to, so the relation holds of the current values there.
It stays true until an event writes, consumes or ends the scope of a support
member, and [ENT-5] kills it on exactly those events, which are also the events
that retarget the images. A header batch holds at every head by [INV-1]'s
induction (base batch, every backedge), so its projection holds at every head.
Nothing is trusted that was not proved: the L0 relation is a consequence of an
already-proved target.

**A second source.** #172 refused an L0 copy of header conclusions as its
loop-exit fix because the copy "adds a second source for the same theorem and
changes what L0 closure sees". At a join both are the point. The L0 relation
is not the affine theorem restated: it is over place terms and dies at the
first write to a support member, while the theorem over immutable images stays
true of the old values; and L0 is the only state the pre-kill closure and the
weakest-bound join read. [ENT-3.S4] avoids a duplicate in the other direction,
an ordering leaf with an L0 projection adding no affine premise, so as not to
enlarge AUTO's premise combinations; (b) adds no premise, and the relation
reaches AUTO only through the L0-image family, which is bounded by the
candidate pairs either way.

**Determinism and termination.** One or two L0 establishments per proved
target per proof point, with no search.

**Cost.** O(1) per proved target; the closure keeps its O(T^3) bound per
materialization and the join its O(T^2) bound. The L0 matrix may hold more
finite cells, which the closure then carries.

**What it admits.** Every unit-coefficient invariant relation now takes part in
[ENT-5]'s pre-kill closure (carried across a write by a commit value, a call
datum or a placement datum) and in the L0 join, and the [ENT-4] closure chains
any number of them where AUTO combines at most two. The prototype accepts b3,
b4, b6, p3a, p3f, p3h, cm1, fb1 and sm0, and p3n by that chaining. m3 is
accepted too: its certificate's target `mid < high` compares two terms, so its
conclusion also enters L0 and the commit value of `set low = mid + 1_u64`
carries `low <= high` across the write. A writer can replace a guard whose
false branch cannot happen by the `invariant` of its negation, and the relation
then survives later joins. Two side effects need checking: a certificate whose
target the L0 closure now derives becomes redundant and is rejected [PRF-1],
and a reject conformance case whose relation is true may become accepted.

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

**Determinism and termination.** Each named relation is one [MSR-4]
disposition per input edge, as a header target is at its base and backedges.

**Cost.** One [MSR-4] disposition per relation per input. The form needs new
grammar, a canonical formatting rule, diagnostics, and a conformance family;
the writer must restate the relation at every join it crosses, including nested
joins.

**What it admits.** Any affine relation every input proves, including ones no
L0 rule can hold: a three-term bound such as `pos + advance <= length` after
branches choose the advance, or the scaled accumulator bound
`sum <= 255_u32 * i` that `ent6-neg-join-one-arm-advances-accumulator` keeps
refused today. It changes no automatic verdict, so it flips no conformance
case.

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
- **Difference of terms.** The result bounds of `-`, and of `-wrap` where its
  condition holds, whose operands are both terms also intersect
  `[-c(b - a), c(a - b)]`, where c(u - v) is the tightest closed L0 bound on
  `u - v`. Sound: r = a - b exactly.

The strict-quotient row alone does not fire on the midpoint:
`span = high - low` has the L0 interval [0, max] under `low < high`, because
the corner hull ignores the operands' mutual bound. With both rows, `span >= 1` and `half < span`
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
  keeps the retained theorem beside it. The L0 relation is not a member of the
  header batch that v0.78 removes at an exit, so (b) alone, without #172,
  already keeps a unit-coefficient header relation on an exit edge with no
  intervening write; the census's four #172 sites are removed that way too.
  Landing (b) without #172 would therefore make the v0.78 removal apply only to
  relations outside the unit-coefficient fragment.
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

## Census

### Scope and procedure

The Snowghost renderer modules, as the four worktrees held them when the
census copies were taken: `trial/run-css-selectors` at `dc0defa`
(`base::geometry`, `base::static_atoms`, `base::atom`, `dom`,
`text::line_break`, `css::syntax`, `css::rules`, `css::selectors`,
`image::png`, `html::tokenizer`, `html::tree_builder`), `research/concurrency`
at `5edc2f7` (`proto::style`), `trial/run-url` at `2377817`
(`text::normalization`, `text::idna`, `url`) and `trial/run-font` at `3773a1d`
(`font`). A module several worktrees share is byte-identical in all of them
and is counted once. Oracle drivers and table generators are test support and
are left out. The only edit is the final text-literal spelling of two escapes
in `html::tokenizer` (`'\u{9}'` as `'\t'`, `'\u{d}'` as `'\r'`). Whitefoot's
own code is `lib/std/collections` (six modules, checked through the library's
graph) and every file of `tests/programs`, the multi-file programs checked as
the bundles their tests compile.

**In place.** Every `if` of four shapes is enumerated: a clamp
(`if c { set x = n; }`), an exit guard whose then-branch ends in `return` or
`break`, an exit guard whose then-branch is empty and whose else-branch ends
so, and a skip guard (`if c { body }`). When the condition is a comparison, or
a `band` or `bor` of comparison bindings, the `if` is replaced by a local
`invariant`: `x <= n` for a clamp, the negated condition for a then-exit
guard, and the condition for an else-exit guard and for a skip guard, whose
body is kept. The module is then checked under v0.78, the #172 emulation, and
the #172 emulation with (b), with (a), with the midpoint rows, and with (b) and
the rows. A configuration that accepts the replacement has proved that the
removed branch never runs there, so an accepted replacement never changes what
the program does. A site is classified by the first configuration in that order
that accepts it. [census.tsv](census.tsv) lists every tested site with its
classification.

**Targeted.** A site whose removal needs a relation written somewhere else (a
header invariant, a contract, a loop in place of a recursion) cannot be tested
in place. These sites were found from the writers' notes, the clamp helpers,
every midpoint and every recursive search, rewritten by hand in the natural
form, and checked the same way.

### In place

| Module | `if` sites | Testable | v0.78 | #172 | (b), (a) | Rows | None |
|---|---:|---:|---:|---:|---:|---:|---:|
| `base::geometry` | 4 | 2 | 0 | 0 | 0 | 0 | 2 |
| `base::static_atoms` | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `base::atom` | 22 | 18 | 0 | 0 | 0 | 0 | 18 |
| `dom` | 21 | 15 | 0 | 0 | 0 | 0 | 15 |
| `text::line_break` | 154 | 49 | 2 | 0 | 0 | 0 | 47 |
| `text::normalization` | 43 | 28 | 5 | 0 | 0 | 0 | 23 |
| `text::idna` | 130 | 76 | 8 | 0 | 0 | 0 | 68 |
| `url` | 289 | 161 | 19 | 4 | 4 | 0 | 134 |
| `css::syntax` | 180 | 82 | 0 | 0 | 0 | 0 | 82 |
| `css::rules` | 41 | 31 | 0 | 0 | 0 | 0 | 31 |
| `css::selectors` | 333 | 129 | 1 | 0 | 0 | 0 | 128 |
| `image::png` | 138 | 76 | 0 | 0 | 0 | 0 | 76 |
| `html::tokenizer` | 197 | 71 | 1 | 0 | 0 | 0 | 70 |
| `html::tree_builder` | 632 | 160 | 0 | 0 | 0 | 0 | 160 |
| `proto::style` | 177 | 83 | 0 | 0 | 0 | 0 | 83 |
| `font` | 138 | 61 | 0 | 0 | 0 | 0 | 61 |
| **Snowghost** | 2499 | 1042 | 36 | 4 | 4 | 0 | 998 |
| `collections::vector` | 10 | 8 | 0 | 0 | 0 | 0 | 8 |
| `collections::deque` | 3 | 1 | 0 | 0 | 0 | 0 | 1 |
| `collections::slab` | 19 | 9 | 0 | 0 | 0 | 0 | 9 |
| `collections::hash_map` | 21 | 12 | 0 | 0 | 0 | 0 | 12 |
| `collections::priority_queue` | 19 | 14 | 0 | 0 | 0 | 0 | 14 |
| `collections::ordered_map` | 40 | 19 | 0 | 0 | 0 | 0 | 19 |
| `tests/programs` (80 bundles with a site) | 1542 | 896 | 70 | 0 | 0 | 0 | 826 |
| **Whitefoot** | 1654 | 959 | 70 | 0 | 0 | 0 | 889 |

Every site (b) accepts, (a) accepts too, and no site needs the rows. The four
join sites are the upper clamps at `url/builder.wf:111`, `url/ipv4.wf:91`,
`url/parser.wf:615` and `url/parser.wf:847`. Each is followed by a lower clamp
whose join discards the header invariant's affine bound (probe fb1): the upper
clamp exists to put `result <= length` into L0, where the lower clamp's join
keeps it. The four #172 sites are `url/parser.wf:55` (a guard) and the clamps
at `url/parser.wf:574`, `741` and `883`, which the writer-lost-facts census
also found (it cites the `let` line above each `if`). The 36 sites v0.78
already proves dead are guards and clamps no join explains: `url/parser.wf`
36, 322, 329, 654, 710, 759, 765, 1035, 1100, 1110, 1151, 1179;
`url/base.wf` 132, 249, 288; `url/ipv4.wf` 205; `url/ipv6.wf` 200, 207;
`url/path.wf` 274; `text/idna/lookup.wf` 62, 88, 107, 149, 174, 199, 225;
`text/idna/punycode.wf` 248; `text/normalization/lookup.wf` 20, 46, 65, 107,
124; `text/line_break/lookup.wf` 26, 36; `css/selectors/types.wf` 116;
`html/tokenizer/charref.wf` 140. None is a census item of this question; they
are recorded for the Snowghost owners.

In Whitefoot's own code no candidate removes a site in place. Of the 70 sites
v0.78 already proves, 45 are the programs' self-checks against expected
constants, kept on purpose, and 25 are guards whose condition the checker
already proves (ten of them in `tests/programs/wfgrep.wf`); none is a census
item of this question either.

The 998 Snowghost sites and 889 Whitefoot sites no configuration removes are
dominated by guards on input data (a malformed span, a byte that may not be a
digit, a traversal that may not describe a run), relations about stored element
values, which the fact language excludes [ENT-2], and facts that would have to
cross a call with no contract; 49 replacements could not be tested at all,
because hoisting a skip guard's body collides with a name (TYPE-6) or changes
an effect (EFF-2). They were not classified further, with one exception.
`url/host.wf:57` clamps `pos +wrap advance`, where nested branches choose an
advance of 1 or 3 and only the branch choosing 3 has checked
`pos + 2 < length`; #172 recorded it as path-correlated. Keeping `pos + advance <= length`
across those joins needs a three-term relation, which no L0 rule keeps. (c)
could keep it with one written relation at each of the three joins once the
`+wrap` position additions become exact; no prototype tested that.

### Targeted

Each rewrite replaces the written workaround by the natural form and is
checked as a whole module (Snowghost, text-literal compilers) or program.
"Rows" is both midpoint rows.

| Rewrite | v0.78 | (a) | (b) | Rows | (b) and rows |
|---|---|---|---|---|---|
| `text::line_break`: contracts on `push_run`, `extend_last_run`, `build_runs`, `write_runs`, without the six room guards (`text/line_break/runs.wf:126`-`131`) and two index guards (`line_break.wf:70`, `71`) | FN-8 | FN-8 | accepted | | |
| `font/cmap.wf:276` `find_glyph` as a loop, `span - 1` midpoint, `invariant inside: middle < hi;` | INV-1 `ordered` | INV-1 `within` | accepted | INV-1 `ordered` | accepted |
| the same loop with `span / 2_u64` | INV-1 `inside` | INV-1 `inside` | INV-1 `inside` | INV-1 `ordered` | accepted |
| the same loop with `span / 2_u64` and a `middle >= hi` return guard instead of the line | INV-1 `within` | INV-1 `within` | accepted | INV-1 `within` | accepted |
| the recursion with `span / 2_u64` (`font/cmap.wf:285`) | OP-4 | | | accepted | |
| `tests/programs/compute/merge_sort.wf:4` `search_step` inlined as a loop, `upper - 1` midpoint, `invariant inside: middle < upper;` | INV-1 | accepted | accepted | | |
| the same loop with `span / 2_u64` | INV-1 | INV-1 | INV-1 | INV-1 | accepted |
| `search_step` with `span / 2_u64` (`merge_sort.wf:11`) | OP-4 | OP-4 | OP-4 | accepted | |

Empty cells were not run. The line-break result under (b) also holds with the
#172 emulation. Without the `inside` line every loop form is rejected under
every configuration except (a+), which was not run on these modules.

(a) accepts the inlined merge sort only because `let last = upper - 1_u64;`
keeps `upper`'s header image alive in a live binding, so the premise
`inside` matches the image difference of `upper` and `last` at the join; the
same loop with `span / 2_u64`, and `find_glyph` with either midpoint, have no
such binding and are rejected. (a)'s reach therefore depends on incidental
bindings.

### Counts by candidate

Items that exist because a relation is lost where branches rejoin, or
because of the midpoint, with the configurations measured to remove them. (c)
was not prototyped; its column is the rule's reading, not a measurement.

| Item | Sites | (a) | (b) | Rows | (c), by its rule |
|---|---|---|---|---|---|
| upper clamp kept to survive a later lower clamp's join | `url/builder.wf:111`, `url/ipv4.wf:91`, `url/parser.wf:615`, `847` | 4 | 4 | 0 | 4, one written relation per join |
| lockstep room and index guards | `text/line_break/runs.wf:126`-`131`, `line_break.wf:70`, `71` | 0 | 8, with contracts | 0 | needs certificates for the chained requirements too |
| binary search written as a recursion | `font/cmap.wf:276`, `tests/programs/compute/merge_sort.wf:4` | 1 (merge sort, `upper - 1`) | 2, with one `inside` line | 0 | 2, relations named at each join |
| midpoint taken over `span - 1` | `font/cmap.wf:285`, `tests/programs/compute/merge_sort.wf:11` | 0 | 0 | 2 | 0 |

Beside these, (b) alone also removes the four loop-exit sites
(`url/parser.wf:55`, `574`, `741`, `883`) that #172 removes. The CSS scan
clamps (`css/rules/scan.wf:86`, `113`, `187`, `css/selectors/scan.wf:624`) are
a candidate item whose rewrite was not measured in this record. One site,
`url/host.wf:57`, needs a three-term relation across joins and is removed by no
measured candidate. `lib/std/collections` and the rest of `tests/programs`
contain no item.

Totals by candidate over the measured items: (a) 5, (b) 14 (18 with the
loop-exit sites), rows 2, (a+) not measured on Snowghost.

## Cost

### Method

As in the result-proof-transport record: prebuilt gate-profile compilers; for
each workload one warmup round and five measured rounds, the configurations
alternating in an order rotated every round; the median wall time of the five
and the largest peak RSS (`/usr/bin/time -f %M`) are reported. Each sample
checks the whole workload, one compiler process per unit. The workloads are
the complete conformance corpus (1,424 cases, a multi-module case through its
graph), each of the sixteen Snowghost census modules through `--check-module`
(text-literal compilers), each of the six `lib/std/collections` modules, the
80 `tests/programs` bundles that have a census site (summed), and a synthetic
family `chain-n` for n = 4, 8, 16, 32, 64: n counters ordered by n
unit-coefficient header invariants, n guarded decrements (2n statement joins)
per iteration and n local invariants, so the projected relations, the L0
matrix and the joins all grow with n. The configurations are the unmodified
v0.78 compiler, the prototype with every switch off (the patch's inert cost),
(a), (b), the two rows, (b) with the rows and, on the main-branch workloads,
(a+). This method was prepared but not run (see [current replay protocol](#current-replay-protocol)).

### Results

The five-sample timing was not run. What was measured:

- **Conformance corpus, one pass, paired per case.** The sweep checks each of
  the 1,424 cases under every configuration in turn. Total seconds: v0.78
  30.80, prototype with switches off 30.28, (a) 29.93, (b) 30.25, rows 30.10,
  (b) and rows 30.20, #172 emulation 30.07, #172 with (b) 29.91. The spread is
  within 3% and not ordered by configuration, so one pass cannot separate these
  configurations from noise; it bounds none of them within 10% by the
  criterion's method.
- **(a+), single unpaired runs.** `collections::vector` 49 against 77 ms,
  `hash_map` 153 against 553 ms, `ordered_map` 370 against 1709 ms. The
  `chain` family took 27, 76, 431 and 3505 ms for n = 4, 16, 32, 64 under
  v0.78, 25, 81, 337 and 3377 ms under (b) and 25, 80, 344 and 3234 ms under
  (a+), single runs each.

### Bounds

With T the L0 terms of a function, C the [MSR-4] candidates (at most T), P
the automatic affine premises, a the atoms of an image and k the inputs of a
join:

| Candidate | Added work | Where | Against the [ENT-4] closure, O(T^3) |
|---|---|---|---|
| (b) | at most two L0 establishments | per proved invariant target | constant |
| (a) | O(k * \|C\|^2 * a) image differences and lookups, at most O(\|C\|^2) new bounds | per join | within it |
| rows | one closed-bound read | per `-`, `/` or `ishr` | constant |
| (a+) | O(k * \|D\| * \|C\| * (P^2 + \|C\|^2)) residual checks of O(a) each | per join | exceeds it when D and C grow together |

(b) and (a) add L0 facts, so later closures carry more finite cells, but the
closure's bound does not depend on how many cells are finite.

## Evaluation against the criterion

Each clause is read as it was recorded. (c) was not prototyped and is not
evaluated here.

**1. Probes.** (b) accepts b3, b4, p3a, p3f, p3h, cm1 and fb1, and b5, p3d,
p3g, cm2 and cm3 stay accepted: met. (a) leaves b3 and b4 rejected: not met,
and by the clause's own terms it does not answer the question. (a+) meets it.
The rows accept m4 and m7, and b1's first rejection becomes INV-1 `ordered` at
the backedge: met.

**2. Negatives.** Under every candidate n1 to n6 and m5 keep their base
rejections, with the same rule and the same invariant or obligation. The
conformance sweep (all 1,424 manifest cases, each checked once per
configuration; (a+) was not swept) changes these verdicts; every other case
keeps its verdict:

| Case | Base | (a) | (b) | Rows | (a+) |
|---|---|---|---|---|---|
| `inv1-neg-sequential-guarded-steps` | INV-1 | accepted | accepted | INV-1 | not run |
| `inv1-pos-operation-and-mode-proof-names` | run | run | PRF-1 | run | not run |
| `prf1-pos-active-header-reference` | accepted | accepted | PRF-1 | accepted | not run |
| `prf1-pos-certificate-after-exhaustion` | accepted | accepted | PRF-1 | accepted | not run |
| `prf1-pos-integer-tightening-midpoint` | run | run | run | PRF-1 | not run |
| `op4-pos-recursive-window-contract` | accepted | accepted | accepted | PRF-1 | not run |
The one reject case that becomes accepted is the flip the clause admits. Its
relation `fast <= limit` holds on every execution, as the case's own
description says ("the source is semantically bounded"): each guarded step
first checks `fast < limit`. Under (b) the header relation enters L0 at the
head, each step's commit value carries it across `set fast = fast + 1_u64`, and
the join keeps it; (a) projects it at each join input. Landing either would
replace the case by a negative the rule still refuses, such as the scaled
accumulator of `ent6-neg-join-one-arm-advances-accumulator` or probe n4.

Every accepted or run case that becomes rejected is a redundant certificate:
[PRF-1] reports `RedundantUseBlock` because the candidate makes the certified
target automatic. Under (b), `use 3 times sum_bound` proves
`3 * sum <= 3 * i`, which AUTO proves once `sum <= i` is an L0 fact (the listed
premise summed with itself leaves `sum - i <= 0`, which the L0 image
discharges). The three cases test proof-name resolution, an active header
reference and a certificate after exhaustion, not the automatic boundary.
Under the rows, the two midpoint certificates become redundant, one of them in
the recursive binary search Snowghost writes. The clause lists these cases; it
does not exempt them. Read strictly, "every conformance case keeps its
verdict" fails for (b) and for the rows on redundant certificates alone. That
reading would fail every rule that widens automatic derivation, since [PRF-1]
rejects each certificate the widening makes automatic, and a midpoint row that
makes the midpoint automatic necessarily makes the midpoint certificates
redundant. This record therefore reads the listing sentence as the clause's
treatment of these cases: landing the candidate rewrites each certificate to a
target the new rule still does not prove. The owner may read it the other
way.

**3. Census.** (b) removes the four URL upper clamps in place and, in the
measured rewrites, the eight line-break guards and both binary-search
recursions, none of which v0.78 or the #172 emulation removes: met. (a)
removes the four URL clamps and the merge-sort recursion: met, though (a)
fails clause 1. The rows remove the two `span - 1` midpoints: met.

**4. Cost.** Not evaluated: the five-sample timing was not run. The stated
bounds of (b), (a) and the rows are within the closure's; the bound of (a+)
is not. The single runs of (a+) on `lib/std/collections` (up to 4.6 times
v0.78) would fail the 10% clause if the timing confirmed them.

## Recommendation

Implement (b), and the two midpoint rows as a separate change. Do not
implement (a) or (a+); defer (c).

**(b)** acts where the relation is lost. b3, b4 and b6 lose it twice: at the
write, because [ENT-5]'s pre-kill closure carries only L0 facts and an
invariant's conclusion is affine only, and at the join, because the L0 state
never held it and the image join gives the changed bindings fresh atoms. (b)
puts a proved unit-coefficient relation into L0 at its proof point; the
existing commit-value transport then carries it across the write and the
existing weakest-bound join keeps it. It adds no join rule, enumerates no path
and searches nothing: one or two L0 establishments per proved target. It is
the only prototyped join rule within the closure's bound that meets clauses 1
to 3 of the criterion (see [Evaluation](#evaluation-against-the-criterion));
clause 4 awaits the five-sample timing, so the recommendation is conditional
on it. A writer can predict it: a
proved comparison of terms behaves after its proof as the same comparison
does after a guard. It follows recorded decisions: [ENT-3.S4] gives a
requirement's comparison its L0 relation, and the automatic-facts node keeps a
conditional value's common bounds with "the existing difference-bound join"
rather than a new inference family.

**The midpoint rows** are independent of the join and touch only
[ENT-3.S7]. They make `mid < high` automatic after `low < high` for the
`span / 2_u64` and `ishr` spellings, which the writers avoided by taking the
midpoint over `span - 1`. With (b) and the rows, the loop binary search needs
its two header relations and one line, `invariant inside: mid < high;`, and no
guard, recursion or certificate.

**What stays written.** `mid < high` in the loop (three terms, never an L0
fact; only (a+) makes the line unnecessary), relations beyond unit-coefficient
differences across a join (`url/host.wf:57`, `sum <= 255 * i`), and relations
about stored element values, which the fact language excludes.

## Rejected alternatives

- **(a), the join-input difference-bound closure.** Rejected: the binary
  search's relation dies at the write in the `set high = mid` arm, before any
  join, so a rule that acts at the join has nothing left to keep. (a) leaves
  b3, b4 and p3n, the CSS scans and the line-break lockstep rejected, and every
  item it removes (b) also removes.
- **(a+), the tightest bound per pair at every join.** Rejected on cost:
  its bound exceeds the closure's, and single runs on `lib/std/collections`
  took 1.6 to 4.6 times as long as v0.78. Its one gain over (b) with the rows
  is the unannotated loop binary search, one written line. Unconfirmed by the
  five-sample method.
- **(c), written join relations.** Deferred rather than rejected. Under (b) a
  unit-coefficient relation needs no new form (an arm-end `invariant`, probe
  b6); (c) acts only at joins, so it cannot help where a relation dies at a
  write with no join (the CSS scans, p3n); and the census has one site that
  needs a three-term relation across a join (`url/host.wf:57`), which also
  needs its `+wrap` arithmetic rewritten. The tree refused the conditional-value
  analogue, "requiring identical written bounds at a conditional-value join",
  because the difference-bound join already keeps the weakest common bound.
  Reopen when a consumer must keep a relation beyond unit-coefficient
  differences across a join.
- **Replace the affine conclusion by its L0 relation.** Rejected: it would
  remove accepted proofs. AUTO combines listed premises and reads L0 only in a
  residual or in its final L0-image family, and b5's proof needs `within` as a
  listed premise beside the division image; the affine theorem also outlives a
  write for an alias of the old value. (b) keeps both.
- **Copy header conclusions into L0 only on loop exits.** #172's refused fix
  for its loop-exit shape, not re-proposed. (b) establishes the relation where
  it is proved and lets the ordinary kills decide its lifetime; the retained
  theorem of #172 stays beside it.
- **The strict-quotient row alone.** Insufficient: `span = high - low` has the
  L0 interval [0, max] under `low < high`, so the row's condition a0 >= 1 never
  holds for the midpoint; the difference row is what makes it fire.
- **An AUTO family "one premise plus one L0 image" for the midpoint.**
  Rejected: it widens every AUTO query by O(P * |C|^2) candidates, moving the
  automatic boundary for every goal, to recover one idiom the two rows give at
  O(1) per operation.
- **Path-sensitive joins, or re-proving invariants at every join.** Rejected:
  [ENT-5] performs no conjunction of guards or path enumeration, the
  automatic-facts node refused branch-history enumeration for conditional
  values, and #172 declined a failure-silent re-proof at `break` because such a
  judgment is new to [INV-1].

## Where the decision goes

If the owner accepts the recommendation, the implementing branch carries:

- **Design tree.** A direct edit of the live node
  `design/language/checks-and-proofs/automatic-facts.md` on the draft branch,
  with two decisions.
  The first states (b) beside the body-entry requirement decision it mirrors:
  a proved invariant target that normalizes to one unit-coefficient difference
  bound over terms also establishes that L0 relation with ordinary support,
  because a relation every branch keeps must cross the write and the join
  through the existing L0 transport, instead of a join-time closure, a per-join
  tightest-bound family or written join relations. The second extends the
  operation-row decision with the two midpoint rows. It must answer that
  node's refused "difference interval of every result against every operand":
  the `-` row reads one closed bound the state already holds and narrows the
  result's interval, where the refused alternative published a new relation
  per operand. The ruling is logged in `design/log.md` only after owner approval.
- **Specification.**
  - [INV-1], at "On success its normalized target and immutable value images
    become one published affine fact after the statement": a target that
    normalizes to one unit-coefficient difference bound over terms, constants
    or measure terms also establishes that relation, as [ENT-3.S4] words it for
    a requirement's comparison.
  - [ENT-5]'s loop rule, where the header batch is "added to the conservative
    head state" of an ordinary loop and "activated there" for a counted loop:
    the batch's relations are established with it.
  - [ENT-6], at "proved header invariants are the only source-written relations
    reintroduced over those header images": the sentence also names their L0
    relations.
  - [ENT-3.S7]: the `/` and `ishr` rows gain the strict order, the `-` and
    `-wrap` result bounds are intersected with the operands' closed difference,
    the literal scaled division image cites the strict order where it holds, and
    [DIAG-2]'s `OperationFact` parents include the difference bound the `-` row
    read.
- **Conformance.** Positive cases for the join (b4), the lockstep (p3f), the
  length-preserving call (cm1), the chain (p3n) and the two rows (m4, m7);
  negatives from n1 to n6 and m5; and each flip criterion 2 lists, replaced as
  it requires.
- **Guidance.** `docs/patterns.md` P8 can show the binary-search loop, and the
  todo item this record answers closes.
