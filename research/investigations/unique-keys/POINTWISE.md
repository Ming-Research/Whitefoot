# Pointwise facts over a derived numbering

Status: paper derivation of candidate N in [the investigation](DESIGN.md#candidates).
Nothing here is selected, specified or implemented. The runtime half of the
witness compiles and runs today; every proof form below is a proposal.

## The candidate

The library Forest derivation proves that the live, linked DOM is a forest
and derives distinct indices from a ghost model. W1 and W2, however, write
through index arrays that the style stage rebuilds on every pass from that
DOM: Snowghost's `walk_elements`, `child_ranges` and `level_arrays`
(`renderer/proto/style/traversal.wf` at Snowghost
`7503e37b887b79da66a7c2161551fc1c9175df6a`). Whether those arrays repeat an
index is a property of the functions that build them, not of the DOM; the
walk already refuses links that do not form its preorder.

Candidate N therefore proves facts about the derived numbering only, and only
facts of one shape:

- **Pointwise.** A fact is `forall x in R: P(x)`, where `P` is quantifier-free
  and reads only the element at `x` and elements at values it reads, such as
  `positions[slots[x]]`. A write at one index can falsify at most the
  instances that read that index, so a preservation proof is a case split on
  the written index rather than a derivation over all elements.
- **Separation by a stored function.** Two indices read from storage differ
  when some array maps them to values already known to differ: in one state,
  `f[x] != f[y]` implies `x != y`. A stored left inverse, `positions[slots[k]]
  == k`, makes the slots of one level pairwise distinct; a stored depth makes a
  parent and an element of the next level distinct. [PAR-2]'s affine element
  rests on the same argument with an arithmetic inverse.
- **Numbering turns global properties into pointwise ones.** In preorder,
  "my parent precedes me" is a pointwise fact that excludes cycles; depth is
  a pointwise relation to the parent; a left inverse replaces "no
  duplicates". Reachability or acyclicity of the linked arena itself is never
  stated.

Facts are ordinary checked facts rather than type invariants: a write that
overlaps their support kills them, and only code that wants to keep one
proves its preservation. No ghost model, inductive type, recursive logical
function, structural induction, existential witness, content snapshot value,
struct-invariant generalization or type brand appears below.

## The witness program

`level_cascade.wf` in this directory is the runtime half of W1 as Snowghost
would write it with level-parallel cascading into document order. Its
`measure_depths` stands in for the element walk's per-element step: it
refuses a parent that does not precede its element, as `walk_elements` and
`parent_slot` do. `level_segments` groups the elements by depth into a
`Segments`, as `level_arrays` groups them into `level_start` and
`level_elements`, and writes each element's offset into `positions`, as
`level_arrays` writes `level_positions`. `cascade_level` is one level of the
cascade: each element's result, written at its own preorder index, from its
parent's result. The tree is `div(section(span, img), article(p, a))`; level
2 writes preorder indices 2, 3, 5 and 6.

Observations at compiler source `22d0923bdf5ce7dbf4752cf9a302dc4154254869`
(no compiler source differs on this branch), built with
`cargo build --profile gate --bin whitefootc --locked`:

- `whitefootc --check level_cascade.wf` exits 0, and the native executable
  exits 0, its result sum 748 and last result 113 matching sequential order.
- Changing the cascade's `base` from 100 to 101 makes the executable exit 3,
  so the result check is not vacuous. Making element 2's parent element 3, a
  parent that follows it, makes `measure_depths` refuse and the executable
  exit 1.
- `whitefootc --par --par-ledger --emit-llvm` denies the cascade loop at
  line 108 with condition 2 ("the body writes storage that is neither
  introduced by the iteration nor the accumulator, at set
  results^[element] = ..."). That loop is the consumer this derivation
  targets. The loops of `measure_depths` and `level_segments` are denied too
  and stay sequential by design: each reads or advances state an earlier
  iteration wrote.

## Annotated program

The proposed forms used below are:

1. **Quantified clauses.** `forall x in a..b: P` and
   `forall d in a..b, k in c..e: P` in a named `requires`, a named `ensures`
   and a loop header `invariant`. `P` combines comparisons with `and`, `or`
   and `implies` over existing terms and element reads `a^[t]`, whose offset
   may itself be a bound variable or an element read. For a `Segments` place
   `s`, `s[d][k]` names element k of segment d. These reads are terms only
   inside quantified facts.
2. **Support and kill.** A quantified fact's support is every storage and
   measure its reads name. Any overlapping write kills it, as a write kills
   an existing fact today [ENT-5, OWN-7].
3. **Instantiation.** `use name at (t, ...)` instantiates a quantified fact at
   written terms and owes their range bounds. Nothing chooses an instance.
4. **Write laws and case splits.** After `set a[t] = v`, a read `a[u]` equals
   `v` when `u == t` and its old value otherwise. A header invariant's
   preservation splits a fresh bound element on equality with each written
   index; each case is discharged with listed `use` steps.
5. **Content laws.** `box_segments_filled` and `box_array_filled` publish
   that every element equals the supplied value; [OP-13] already states this
   behavior, but no contract carries it as a fact.
6. **Read congruence.** In one state, `a[x] != a[y]` proves `x != y`. This
   is the only new separation rule; it feeds the shared overlap judgment
   [OWN-7].
7. **Cross-iteration certificate.** `apart (i, j) { use ...; }` in a counted
   loop's header introduces two fresh iterations `i != j`. Every pair of
   accesses to a written root, from different iterations and with at least
   one write, must then be proved separated from the listed instances, the
   access paths' guards, read congruence and the existing affine families.
8. **Transport.** Quantified facts travel through routed results, struct
   construction and written-reference exits as integer facts do today
   [FN-9, CALL-4, MSR-3].

The certificate syntax is illustrative. Where a body is unchanged from
`level_cascade.wf`, it is elided.

### The walk's step

```text
fn measure_depths(parents: &[u64], depths: &[u64]) -> result: Option<u64>
    reads(parents), writes(depths) contract {
  requires depths^.len == parents^.len;
  ensures shaped when Some(value: deepest):
    forall x in 0_u64..parents^.len:
      parents^[x] == top or
        (parents^[x] < x and depths^[x] == depths^[parents^[x]] + 1);
} {
  let count = parents^.len;
  let deepest = 0_u64;
  for (
    at in 0_u64..count,
    invariant shaped: forall x in 0_u64..at:
      parents^[x] == top or
        (parents^[x] < x and depths^[x] == depths^[parents^[x]] + 1)
  ) {
    ... unchanged: the parent == top case, the parent < at case
        and the refusal ...
  }
  return Some<u64>(value: deepest);
}
```

### Grouping by depth

```text
fn level_segments(depths: &[u64], levels: u64) -> result: Option<Levels>
    reads(depths) contract {
  requires levels <= depth_ceiling;
  requires depths^.len <= 4294967294_u64;
  ensures when Some(value: made): made.positions.inner.len == depths^.len;
  ensures inverse when Some(value: made):
    forall d in 0_u64..made.slots.inner.len, k in 0_u64..made.slots.inner[d].len:
      made.slots.inner[d][k] < depths^.len implies
        made.positions.inner[made.slots.inner[d][k]] == k;
  ensures leveled when Some(value: made):
    forall d in 0_u64..made.slots.inner.len, k in 0_u64..made.slots.inner[d].len:
      made.slots.inner[d][k] < depths^.len implies
        depths^[made.slots.inner[d][k]] == d;
} {
  ... counting loop and box_segments_filled unchanged ...
      let rows = slots.inner.len;
      for (
        at in 0_u64..count,
        invariant fresh: forall d in 0_u64..rows, k in 0_u64..slots.inner[d].len:
          slots.inner[d][k] == absent or slots.inner[d][k] < at,
        invariant inverse: forall d in 0_u64..rows, k in 0_u64..slots.inner[d].len:
          slots.inner[d][k] < count implies
            positions.inner[slots.inner[d][k]] == k,
        invariant leveled: forall d in 0_u64..rows, k in 0_u64..slots.inner[d].len:
          slots.inner[d][k] < count implies depths^[slots.inner[d][k]] == d
      ) {
        ... unchanged: segment^[offset] = at; positions.inner[at] = offset;
            fill.inner[depth] = offset + 1, after the existing checks ...
      }
  ...
}
```

### One level of the cascade

```text
fn cascade_level(slots: &[u64], parents: &[u64], depths: &[u64],
                 positions: &[u64], matched: &[u64], results: &[u64],
                 level: u64, base: u64) -> result: unit
    reads(slots), reads(parents), reads(depths), reads(positions),
    reads(matched), writes(results) contract {
  requires parents^.len == results^.len;
  requires matched^.len == results^.len;
  requires depths^.len == results^.len;
  requires positions^.len == results^.len;
  requires inverse: forall k in 0_u64..slots^.len:
    slots^[k] < results^.len implies positions^[slots^[k]] == k;
  requires leveled: forall k in 0_u64..slots^.len:
    slots^[k] < results^.len implies depths^[slots^[k]] == level;
  requires shaped: forall x in 0_u64..parents^.len:
    parents^[x] == top or
      (parents^[x] < x and depths^[x] == depths^[parents^[x]] + 1);
} {
  let count = slots^.len;
  let total = results^.len;
  for (
    k in 0_u64..count,
    apart (i, j) {
      use inverse at (i);
      use inverse at (j);
      use leveled at (i);
      use leveled at (j);
      use shaped at (slots^[i]);
      use shaped at (slots^[j]);
    }
  ) {
    ... unchanged body: element = slots^[k]; if element < total,
        read matched^[element] and parents^[element], read
        results^[parent] when parent < total, and write
        results^[element] ...
  }
  return unit;
}
```

`depths`, `positions` and `level` are new parameters that the body never
reads. They name the proof's support. Lowering would pass two range
references and one integer per level call; an erasure rule for parameters
read only by contracts is a separate question.

### The caller

```text
for (level in 0_u64..rows) {
  cascade_level(slots: &made.slots.inner[level], parents: ..., depths: ...,
                positions: &made.positions.inner[0_u64..count], matched: ...,
                results: ..., level: level, base: 100_u64);
}
```

At each call the caller instantiates `inverse` and `leveled` of
`level_segments` at `d = level`, leaving `k` bound; this gives the callee's
`inverse` and `leveled` for the segment `&made.slots.inner[level]`. `shaped`
passes unchanged. The level loop writes only `results`, whose storage
overlaps none of the facts' support, so the facts survive every level. The
level loop itself stays sequential, since level d reads level d - 1's
results.

## What the checker proves

### `measure_depths`: one invariant, five steps

The iteration writes only `depths^[at]`.

- **Entry.** The range `0..0` is empty.
- **New element `x == at`.** On `parent == top`, the left disjunct. On
  `parent < at`, the existing check, the right disjunct: `parents^[at] < at`
  is that check, and `depths^[at] == depths^[parent] + 1` holds after the
  write because `parent != at` leaves `depths^[parent]` unchanged (one write
  law step).
- **Old element `x < at`.** Instantiate the old invariant at `x`. Its reads
  `depths^[x]` and `depths^[parents^[x]]` are at indices below `at`, `x < at`
  directly and `parents^[x] < x` from the instance, so two write-law steps
  keep both reads.

The refusal exits carry no `ensures`, since `shaped` is routed to `Some`.

### `level_segments`: three invariants, about twelve steps

The iteration writes `slots[depth][offset] = at`, `positions[at] = offset` and
`fill[depth] = offset + 1`.

- **Entry.** The `box_segments_filled` content law makes every slot `absent`.
  `fresh` follows, and `inverse` and `leveled` hold vacuously because
  `absent` exceeds `count` (`count <= 4294967294`): three steps.
- **`fresh`, for a fresh `(d, k)`.** If `(d, k) == (depth, offset)`, the slot
  holds `at < at + 1`. Otherwise the slot is unchanged and the old instance
  gives `absent` or a value below `at`. Two cases. Element separation for
  the "otherwise" case comes from the `Segments` element rule [REF-4, OWN-7]:
  a different segment index or a different offset in one segment is a
  different place.
- **`inverse`.** In the written slot, `positions[at] == offset` by the write
  law. In any other slot holding `v < count`, the old `fresh` instance gives
  `v < at`, so `v != at`, so the write at `positions[at]` leaves
  `positions[v]` unchanged, and the old `inverse` instance gives `k`. Four
  steps.
- **`leveled`.** In the written slot, `depths^[at] == depth` because `depth`
  was read from `depths^[at]` and nothing writes `depths`. Elsewhere the
  slot is unchanged. Two steps.
- **Exit.** At `at == count` the invariants become the `ensures` and travel
  into `made` with the construction `Levels(slots: move slots, positions:
  move positions)`.

No fact about `fill` or `sizes` is needed. `offset < segment^.len`, the
existing refusal moved from the whole array to the element's own segment,
supplies the write's bounds. A wrong count therefore leaves slots `absent`
or refuses the element; it never falsifies `inverse` or `leveled`, because
an overwritten or never written slot simply has no instance with a value
below `count`.

### `cascade_level`: the cross-iteration obligation

For fresh iterations `i != j`, write `e_i = slots^[i]`, `e_j = slots^[j]` and
`p_i = parents^[e_i]`. The loop writes only `results`; `slots`, `positions`,
`depths`, `parents` and `matched` are read-only, so every iteration reads them
in the loop's entry state and the required facts hold throughout. The
accesses to `results` from two iterations give three kinds of pairs:

- **Write against write**, under `e_i < total` and `e_j < total`: `inverse`
  at `i` and `j` give `positions^[e_i] == i` and `positions^[e_j] == j`, so
  `positions^[e_i] != positions^[e_j]`, and read congruence gives
  `e_i != e_j`.
- **Read against write**, under `p_i < total` and `e_j < total`: `leveled` at
  `i` and `j` give `depths^[e_i] == level == depths^[e_j]`. `shaped` at `e_i`
  rules out `p_i == top`, since `p_i < total`, and gives
  `depths^[e_i] == depths^[p_i] + 1`. Hence `depths^[p_i] + 1 ==
  depths^[e_j]`, so the two depth reads differ, and read congruence gives
  `p_i != e_j`.
- **Write against read**: the same with `i` and `j` exchanged, using `shaped`
  at `e_j`.

Reads against reads need nothing. Within one iteration, source order is
kept. These are the six listed instances, three applications of read
congruence and the existing affine derivation.

### The whole chain

About 25 written steps prove the chain from the walk to the parallel
cascade: five in `measure_depths`, about twelve in `level_segments`, two
partial instantiations at the call and six in the loop certificate. These
counts are hand counts of the derivation above, not measurements of a
checker.

## Criteria and result

Candidate N's entry under [Candidates](DESIGN.md#candidates) records four
criteria, stated to the owner before this derivation was written:

| Criterion | Result |
|---|---|
| Every fact used is pointwise | Met: `shaped`, `fresh`, `inverse` and `leveled` each read one element and elements at values it holds |
| Every preservation premise is an existing runtime check or loop bound | Met: `parent < at` is the walk's precedence refusal, and `offset < segment^.len` is `level_arrays`' room check against the element's own segment |
| The consumer gains no state and no branch | Met in the loop body; the guards `element < total` and `parent < total` are the sequential program's. Each level call passes three proof-only arguments |
| Step count per write | About 25 written steps for the chain, at most four per case |

The derivation found one fact that is not pointwise: disjointness of the
level ranges. With `level_start` and `level_elements` as separate arrays,
deciding that a slot of level d's range belongs to no other level needs
`level_start` to be monotone across arbitrary gaps, a two-index fact.
`Segments` already owns exactly that disjointness in the kernel, so writing
the grouping as a `Segments` removes the only non-pointwise step. This
matches the `Segments` boundary rule: no operation changes the boundaries
after `box_segments_filled` builds them [TYPE-9].

## Found by the derivation

- **No completeness proof.** Neither the counting argument nor "every slot
  of a level is filled" is needed. An unfilled slot stays `absent`, which
  the consumer's existing bounds guard skips.
- **Existing checks are the premises.** The walk's refusal of a parent that
  does not precede its element and the room check before a level write are
  each the exact premise of one preservation case.
- **The proof's support must stay alive.** `depths` and `positions` must
  outlive the cascade. Snowghost's `level_arrays` already writes
  `level_positions` and the walk already writes `depths`; `Traversal` keeps
  the former but not the latter. Keeping an array that is already built adds
  memory lifetime, not computation; a ghost array would remove even that.
- **Proof-only parameters.** The consumer takes `depths`, `positions` and
  `level` only so its contract can name them. Erasing such parameters would
  need its own rule.

## Limits

- **The live DOM.** The candidate states nothing about the linked arena:
  not acyclicity, not that a sibling chain repeats no node, not facts that
  survive its mutation. If a consumer needs proofs carried across DOM edits,
  such as incremental restyling that keeps a numbering instead of rebuilding
  it, this candidate does not cover it. Maintaining a numbering under edits
  would need order keys with gaps, or the model route.
- **Unwritten consumers.** W2's permutation and the per-parent sibling-
  position pass have not been written. The sibling-position pass would use
  `parent` as the separating function between parents and a stored position
  as the left inverse within one parent.
- **The calculus.** The fact grammar, certificate syntax, write-law case
  generation, content laws, transport rules and the PAR-2 amendment are
  sketches. Neither their soundness nor their checking cost is established.
  The step counts above are by hand.
- **The refusal it reopens.** The fact language carries no quantified
  storage-element facts
  ([checks and proofs](../../../design/language/checks-and-proofs.md)), for
  two reasons: every write would owe the fact again, and establishing one
  needs a derivation over elements. Here a write kills the fact unless the
  writer proves its preservation, and both introduction forms are fixed and
  finite. The refusal's alternative, occupancy as tags or options checked
  through a bounded index, cannot express distinctness without a runtime
  check. Reopening it is an owner decision; this record does not make it.
- **Prior art.** The left-inverse witness of injectivity and the use of
  preorder intervals for ancestry are standard; they are not claimed as new.

## Comparison with the library Forest route

| Need | Library Forest ([derivation](DESIGN.md#worked-derivation-a-library-forest)) | Candidate N |
|---|---|---|
| Object proved | The live linked arena under every mutation | Each stage's derived numbering, rebuilt from the arena |
| Distinctness | `NoDup` of a flattened ghost model, through structural induction | A stored left inverse and read congruence |
| Read/write separation | Subtree footprints over the model | A stored depth and read congruence |
| New proof machinery | Ghost types, Seq/Nat models, recursive definitions, induction, unfold, existentials, content snapshots, generalized invariants | Pointwise quantified facts, instantiation, write-law case splits, content laws, one congruence rule, a cross-iteration certificate |
| Facts across DOM edits | Covered | Not covered |
