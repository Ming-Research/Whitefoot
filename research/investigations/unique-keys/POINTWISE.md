# Range facts over a derived numbering

Status: candidate N of [the investigation](DESIGN.md#candidates), selected
and implemented on this branch as [RANGE-1] through [RANGE-5] of the active
specification, with [PAR-2]'s certified elements. The owner's rulings Q1 to
Q4 of 2026-10-01 selected the direction; the design tree's
[checks and proofs](../../../design/language/checks-and-proofs.md),
[range facts](../../../design/language/checks-and-proofs/range-facts.md),
[loop permission](../../../design/language/parallelism/loop-permission.md)
and [range judgment](../../../design/compiler/range-judgment.md) nodes hold
the decisions; this record holds the derivation, the programs and the
measurements.

## The problem

Snowghost's style stage cascades each element from its parent. Done level by
level, the elements of one tree level are independent, and the result of each
belongs at the element's document (preorder) index. The loop that does it,

```text
for (k in 0..count) {
  let element = slots^[k];          // this level's k-th element, a preorder index
  let parent = parents^[element];   // its parent's preorder index
  set results^[element] = cascade(results^[parent], ...);
}
```

writes `results^` at an index read from storage. [PAR-2] admits a write only
through an affine element `a*k + b` or a proved range, so the loop was denied,
and the only parallel alternative stored results in level order and copied
them back. Whether two iterations collide depends on two facts about the
derived arrays: a level lists no element twice, and a parent lies one level
above its element. Neither is a relation between scalar terms, so the fact
language had no way to state them, and the design tree had refused quantified
storage facts outright.

## The architecture rule: each pass derives its facts

The live document carries no range fact (owner ruling Q2). Scripts change the
DOM between passes, so a fact kept with it would be owed again by every
mutation, and acyclicity of the linked arena, which the library Forest route
proves, is never needed: each pass walks the elements it styles in preorder,
and that walk already refuses a parent that does not precede its element.
The facts are stated about the arrays the pass derives (depths, per-level
lists, positions) and are established by the loops that build them, as loop
invariants, then handed to a callee as its requirements, or back to a caller
as the postconditions of the function that built them; no fact outlives the
pass. The cost is the walk the pass makes anyway, plus one grouping pass.

## The language

What is new, with the rule that defines it:

- **A range clause** [RANGE-1], as a function requirement or postcondition
  or a loop header invariant:
  `forall NAME(x in a..b, y in c..d) when g1, g2: c1, c2`, one or
  two bound variables, guards and conclusions comparing integer terms. Terms
  are literals, consts, bound variables, live integer values, measures,
  segment lengths and integer element reads (`p^[i]`, `b.inner[i]`,
  `s.inner[d][k]`). A read in a clause owes nothing where it is written: an
  instance claims its conclusions only where its reads select existing
  elements.
- **The range judgment** [RANGE-2, RANGE-3]: a fact is proved at every call
  of a function requiring it, at its loop's entry, on every backedge and at
  every exit of a function promising it, by the fixed derivation described
  [below](#how-the-compiler-proves-it); after a call the caller holds the
  callee's postconditions, a routed one in the arm that takes its variant.
- **Fill contents** [PRE-1]: `box_array_filled(count, v)`'s row promises
  every element v, and `box_segments_filled(lengths, v)`'s, under `Some`,
  every element v and each segment its length, as range postconditions.
- **A certificate** [RANGE-5]: `apart(i, j) { use NAME(args); ... }` on a
  counted loop asks the judgment to prove that two distinct iterations touch
  no element of shared storage in common. Usually the block is empty: the
  instances the proof needs are formed from the reads the body makes.
  A certificate that does not hold is a compile error (owner ruling Q3).
- **Certified elements** [PAR-2]: the writes a holding certificate placed
  become a further admitted family of the counted permission judgment, whose
  reads of the written storage must be the certificate's own.

Everything else is the existing language: affine requirements, loop
invariants, ordinary bounds proofs, effect rows.

## The witness programs

Both are conformance cases, `range5-pos-level-cascade.wf` and
`range5-pos-children-fresh-values.wf` in `tests/conformance/cases/`, which
check and run at this branch's compiler; the first was this directory's
`level_cascade.wf`, kept with its documentation.

### [The level cascade](../../../tests/conformance/cases/range5-pos-level-cascade.wf)

`cascade` stands in for a style pass. Its first loop derives each element's
depth from its parent, refusing a parent that does not precede its element,
with the invariant

```wf
invariant forall up(e in 0_u64..at) when parents^[e] < n: parents^[e] < e, depths.inner[parents^[e]] + 1_u64 == depths.inner[e]
```

Its third loop groups the elements by depth into a `Segments`, writing each
element's preorder index `at` into its level's next slot and the slot's
offset into `positions`, with

```wf
invariant forall fresh(d in 0_u64..rows, k in 0_u64..slots.inner[d].len) when slots.inner[d][k] != absent: slots.inner[d][k] < at,
invariant forall listed(d in 0_u64..rows, k in 0_u64..slots.inner[d].len) when slots.inner[d][k] != absent: positions.inner[slots.inner[d][k]] == k, depths.inner[slots.inner[d][k]] == d
```

and then calls `cascade_level` for each level. `cascade_level` requires

```wf
requires forall listed(k in 0_u64..slots^.len) when slots^[k] < results^.len: positions^[slots^[k]] == k, depths^[slots^[k]] == level;
requires forall up(e in 0_u64..parents^.len) when parents^[e] < parents^.len: depths^[parents^[e]] + 1_u64 == depths^[e];
```

and its loop carries `apart(i, j) { }`.

### [One array of children, distinct by fresh values](../../../tests/conformance/cases/range5-pos-children-fresh-values.wf)

This is the shape Snowghost's `child_ranges` has: one array holding every
element's children, each parent's run contiguous, filled in document order.
No inverse table exists. The producer writes each child as its own document
index `at`, larger than every index already in the array, so

```wf
invariant forall fresh(q in 0_u64..children.inner.len) when children.inner[q] != absent: children.inner[q] < at,
invariant forall nodup(a in 0_u64..children.inner.len, b in 0_u64..children.inner.len) when a != b, children.inner[a] != absent: children.inner[a] != children.inner[b]
```

hold at every header. `mark_children` takes one parent's run and requires
`nodup` over it; its loop writes `results^[kids^[k]]` under `apart(i, j) { }`.

The preservation of `nodup` after `set children.inner[place] = at` is one case
split per position:

| Case | Why the two slots still differ |
|---|---|
| neither is `place` | both are unchanged, and the old `nodup` holds |
| `a` is `place` | `children[a]` is now `at`; `children[b]` is unchanged, so by the old `fresh` it is `absent` or below `at` |
| `b` is `place` | symmetric: `children[a]` is not `absent`, so it is below `at` |

The array needs `absent` as its fill: with a fill of 0, an unwritten slot
would equal element 0.

## How the compiler proves it

The range judgment runs after ordinary entailment, over the checked program
(`compiler/src/semantic/range_judgment/`). It walks each function that
states a range clause or calls one that requires one.

**The state.** Each binding has a symbolic value; each storage location
holding a run of elements (a `Box<Array>`'s content, a range parameter's run,
a `Segments`) has a current *version*. A write makes a new version defined
by the old one; a move into a struct or out of a payload aliases the
location, so `made.slots.inner` and `slots.inner` stay one container. A fact
is a clause plus what its places and values denoted when it became active:
it never changes meaning, and a question about a newer version reaches it
only through the newer version's definition. A branch forks the state; at
the join each value, version and condition the arms disagree on is defined by
the arm taken, through one selector per join. A loop header forgets what a
dry walk of the body writes, then assumes the loop's invariants.

**One obligation** (a call's requirement, a loop entry, a backedge, an
exit) becomes a finite problem: fresh bound variables, their ranges, the
guards and the existence of every read as hypotheses, the state's conditions
and joins, and the negated conclusion. The derivation then:

1. instantiates every active fact at each tuple an element read of the
   problem selects through one of the fact's own reads, after expanding
   every definition; an instance's own reads form no further instance;
2. fires an instance whose premises the hypotheses entail;
3. decides a write's read, the written element or the old one, when only
   one case is consistent, and otherwise tries both; likewise a join's arm,
   two reads of one version that may be one element, and a disequality as
   `<` or `>`;
4. judges each branch's literals by solving unit equalities, identifying
   reads of one version at one solved index tuple (read congruence), and
   asking whether the inequalities, each tightened over the integers, have
   a rational solution, by Fourier-Motzkin elimination.

Only the problem's size is bounded, by 4096 atoms and 256 formed instances
of one fact; every branch runs to completion. A first implementation also
capped the branches and one elimination's inequalities and tightened what
elimination derives, but those depend on the order of splits and
eliminations, which the specification leaves open, so two checkers could
have disagreed at a cap or on a verdict.

The frontier case of a backedge (`e == at` after `for (at ...)`) is step 3's
read pair: `parents^[e]` and `parents^[at]` are either one element, where the
body's own condition on `parent` applies, or different, where the old fact
does.

**The certificate** walks the loop body twice from the loop's entry state,
with the binder at i and at j, recording every element read and write of
storage that exists before the body runs. For each write of the i-walk and
each access of the j-walk to the same container, the problem adds `i != j`
and the two index tuples' equality and must be contradictory. In
`cascade_level`, the write pair `results^[slots^[i]]` and
`results^[slots^[j]]` closes by `listed(i)`, `listed(j)` and read
congruence: equal slots give equal positions, so `i == j`. The write and the
parent read, `results^[slots^[i]]` and `results^[parents^[slots^[j]]]`, close
by `listed(i)`, `listed(j)` and `up(slots^[j])`: one is at depth `level` and
the other at `level - 1`. Every instance is formed from a read the body
makes, so the certificate block is empty.

**PAR-2** then admits the certified writes as a family, provided every read
of the written storage is a measure or one of the certificate's reads.

## Observations

At compiler revision 9ea2818b (`make -C compiler build`), on a
4-processor Linux host:

- **The witnesses.** `whitefootc --check` accepts the level cascade and the
  children array; both exit 0 built sequentially and with `--par`.
  `--par-ledger` permits `cascade_level`'s level loop (line 27) and
  `mark_children`'s loop (line 13), each "eligible; no accumulator". The
  passes' other loops stay denied, rightly: each depends on what earlier
  iterations wrote (a depth read at the parent, a per-parent or per-level
  counter, a running offset) or hands the whole result run to a callee;
  `main`'s fill and checksum loops are permitted.
- **No runtime trace.** The scatter case
  `tests/conformance/cases/range5-pos-scatter-through-left-inverse.wf`
  emits byte-identical LLVM sequentially with and without its `apart`
  clause, and its `--par` chunk function's loop body has the same compares,
  branches, loads and stores as the sequential loop.
- **Soundness cases.** Each rejection in the `range*` conformance cases
  names the rule and the site: a false left inverse (`<=` for `==`), a
  missing depth requirement, a call handing the whole written run to a
  writer, a grouping that writes 0 instead of the element, a chained
  instance with no written `use`, the state after a break, a write to a copy
  taken for a write to its source, through a `let` or a match binder, and a
  header that forgot neither a write only later iterations reach nor the
  variant of a written enum; a range postcondition left unproved is
  rejected at the exit that owes it, a return, a routed return or a
  propagated error exit, and at a generic function's integer instance.
  Earlier builds of this branch accepted the five before the
  postconditions, and 60b4b3b9 rejected
  `range2-pos-fill-segments-direct-match.wf`, whose match binder takes the
  storage a call made rather than copying stored storage.
- **Proof cost.** Checking the level cascade takes 0.87 to 0.96 s in three
  runs; callgrind attributes 91% of its 10.2 billion instructions to the
  range judgment, 90% to solving owed facts (`Walker::require`), with
  repeated Fourier-Motzkin elimination the largest part. The children array checks in
  0.07 s. Over Snowghost's whole renderer, at b787fe51, the judgment is
  within the run-to-run spread of the front end: 108.3 s and 106.2 s with
  the judgment skipped by a temporary switch, against 108.5 s and 109.2 s
  with it, two builds each; at 450fea25 the same comparison gave 102.9 s and 102.7 s
  against 105.4 s and 105.1 s. Making the derivation incremental is
  recorded in `docs/todo.md`.

The proof-cost figures come from these commands. The check time is
`/usr/bin/time whitefootc --check` on the case; the instruction shares are
`valgrind --tool=callgrind whitefootc --check` on it, read with
`callgrind_annotate --inclusive=yes` at `range_judgment::judge_program` and
`Walker::require`. The front end is `front_end_ms` of
`whitefootc --report --graph modules.wfg --entry proto_style -o OUT` in
Snowghost's `renderer/`; the switch was a measurement patch, never committed,
that returned an empty judgment for every function when the environment
variable `WF_SKIP_RANGE` was set.

## Snowghost shape D

Snowghost's style prototype gained shape D on its branch
`proved-level-cascade`: C's flat match, then `cascade_levels`, the level
cascade's program over the prototype's traversal, with `cascade_level`'s
loop writing each element's computed values at its preorder index under an
empty certificate. It was written at Snowghost 7358347 against this
branch's compiler at 450fea25, and checked, timed and its ledger read again
at Snowghost 414cd5e and a6d65c8, with the `whitefoot/` pin at this
branch's 145f6e9e. The Snowghost record,
`research/investigations/concurrency/DESIGN.md`, "Shape D: a proved level
cascade", holds the tables and runs 22 to 25.

- **Accepted as written.** The facts and the empty certificate checked on
  the first build; the only rewrite was for the permission judgment, not
  the proof: the loop body's call of `cascade_element`, which returns two
  values, moved into `cascade_into`, since [PAR-2] refuses a body binding an
  ordered result list (`docs/todo.md`).
- **Same results.** `proto_style check` agrees on the six pages present on
  the host, `--par` and sequential, with the checksums Snowghost recorded
  before the port, and on a page whose deepest element lies at the
  traversal's depth ceiling. A review of the port found that D first
  refused that page by one level, a bound in Snowghost's code, not in the
  proof; a D that leaves one level uncascaded fails the check.
- **Permitted and split.** `--par-ledger` permits the level loop, one
  accumulator under `band`, and splits it; the depth walk, the grouping and
  the loop over levels stay sequential, as they must.
- **Speed.** At four workers the cascade alone, over one match, takes
  0.0347 s against 0.0417 s for shape C's preorder cascade, which is
  sequential in every build, on ecma262 (1.20 times faster), 0.0062 s
  against 0.0177 s on html5 (2.85 times), and 1.6 and 1.5 times faster on
  the flat and unbalanced synthetic pages. The cascade is 2.6 to 5 percent
  of the real pages' style stage, which matching dominates, and on both
  real pages the whole stage at four workers takes the same time in C and
  D, the cascade's gain not separated from the match's variation. D's
  sequential cascade adds the validating walk and the grouping: 0.0520 s
  against C's 0.0417 s on ecma262, though 0.0127 s against 0.0178 s on
  html5, a difference not attributed further. Snowghost's `run.sh style`
  with `SHAPES="C D cascade-c cascade-d"` and `RUNS=5` produced these
  figures.
- **Compile time.** The range judgment stays within the run-to-run spread
  of the renderer's front end ([Observations](#observations)).

## Snowghost's inherited pass

Snowghost's style stage (mbbill/Snowghost#24) keeps one pass in document
order, `inherited_pass` in `renderer/style/inherited.wf`, 26 percent of its
four-worker stage on ecma262 (Snowghost's
`research/investigations/style/DESIGN.md`, criterion 2). As a level loop in
D's shape it needs three things D's cascade does not. At this branch's
compiler as of 9ea2818b:

- **A read at an ancestor chosen before the loop.** An element that
  declares no custom properties shares its nearest declaring ancestor's
  set, so iteration k reads `sets^[owners^[parent]]` and writes
  `sets^[element]` only where the element owns its set. With

  ```wf
  requires forall owned(e in 0_u64..owners^.len) when owners^[e] < owners^.len: depths^[owners^[e]] <= depths^[e];
  ```

  beside `listed` and `up`, the empty certificate separates them: the
  parent's owner lies at depth level - 1 or above, every write at depth
  level. Reading `owners^[element]` instead, whose depth may be level, is
  rejected naming the write and that read. The compiler test
  `a_certificate_places_owned_elements_of_two_roots_and_an_ancestor_read`
  in `compiler/src/semantic/tests/range_facts.rs` holds both programs.
- **An owned element and a reference to one element.** In the same test
  `sets` holds a `nocopy` struct owning a `Box<Slots<u64>>`:
  `set sets^[element] = move made` is a certified element, and
  `entry_count(list: &sets^[source])` a read the certificate places.
- **Several roots.** The test certifies writes to two roots, `sets` and
  `sizes`; a probe of the same level over integer arrays, with eight arrays
  each written at the element and read at the parent beside the set read
  at the parent's owner, was permitted too and not kept.
- **Establishing `owned` is the cost.**
  [`owner_loop.wf`](owner_loop.wf) derives the owners after the depth walk,
  with `owned` as the invariant of its own loop. Its check takes 12.7 s,
  against 0.86 s without that invariant; 11.3 s of it is one backedge
  problem the derivation refutes in 5,463 branches, splitting pair after
  pair of reads the contradiction does not use (`docs/todo.md`, "The range
  judgment splits every open read pair"). Computing the owners in the
  depth walk instead, its root and depth arms writing `owners^[at]` and
  `owned` beside `up` in its header, takes 392.6 s: two of its problems
  open 66,463 and 76,111 branches.

## Criteria and result

Candidate N's entry under [Candidates](DESIGN.md#candidates) recorded four
criteria before the paper derivation:

| Criterion | Result |
|---|---|
| Every fact used is pointwise: its body reads one element and elements at values it holds | Met for W1: `up`, `fresh` and `listed` each read one element, the last two indexing a `Segments` element by its level and slot. The children array's `nodup` reads two elements and is pairwise, outside the criterion |
| Every preservation premise is an existing runtime check or loop bound | Met: the walk's precedence refusal and the grouping's room check |
| The consumer gains no state and no branch | Met in the loop body; `cascade_level` takes `positions`, `depths` and `level` as proof-only arguments |
| The written certificate steps are counted | None: every certificate in W1, the children array and Snowghost is empty |

## Found by the derivation and the implementation

- **The frontier element needs a read-pair split.** A backedge's new element
  `e == at` is covered by the body's condition on `parents^[at]`, and the old
  ones by the old fact; neither applies until the derivation asks whether
  `parents^[e]` and `parents^[at]` are one element.
- **A join must remember which arm was taken.** A loop body with an `if` on
  the parent's kind joins two writes; without the join's arm conditions as a
  disjunction the backedge cannot tell which write applied.
- **No completeness proof.** Neither the counting argument nor "every slot
  of a level is filled" is needed. An unfilled slot stays `absent`, which the
  consumer's existing bounds guard skips.
- **Proof-only parameters.** `cascade_level` takes `depths`, `positions` and
  `level` only so its contract can name them. Erasing them would need its
  own rule.
- **A loop's exits must be joined.** A first implementation gave back a
  loop's forgotten header state after a `break`, which lost an ordinary
  `loop`'s range invariants after it and kept a counted loop's affine
  invariants as conditions after a break had changed the values. The state
  after a loop is now the join of its breaks and, for a counted loop, its
  last header; `range2-neg-break-state` was accepted before and is rejected
  now, and `range3-pos-ordinary-loop-invariant` the reverse.

## Limits

- **The live DOM.** Nothing is stated about the linked arena itself: not
  acyclicity, not facts that survive a mutation. Incremental restyling that
  keeps a numbering across edits would need order keys with gaps, or the
  model route.
- **Bound variables.** At most two; distinctness within each segment of a
  `Segments` is stated as a left inverse instead of a three-variable
  `nodup`.
- **Proof cost.** The derivation explores every open pair of reads, so a
  fact with a nested read such as `owned` costs seconds per loop
  ([above](#snowghosts-inherited-pass)); the level cascade's figures are in
  [Observations](#observations).

## Comparison with the library Forest route

| Need | Library Forest ([derivation](DESIGN.md#worked-derivation-a-library-forest)) | Range facts |
|---|---|---|
| Object proved | The live linked arena under every mutation | Each pass's derived arrays, rebuilt from the arena |
| Distinctness | `NoDup` of a flattened ghost model, through structural induction | A stored left inverse or fresh values, and read congruence |
| Read/write separation | Subtree footprints over the model | A stored depth and read congruence |
| New proof machinery | Ghost types, Seq/Nat models, recursive definitions, induction, unfold, existentials, content snapshots, generalized invariants | Range clauses, a fixed derivation, a two-iteration certificate |
| Facts across DOM edits | Covered | Not covered |
