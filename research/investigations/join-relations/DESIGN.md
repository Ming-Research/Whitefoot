# Transporting written loop relations through control joins

## Question, scope and prior criterion

This is the written design for C2 in the natural `loop { match }`
interpreter plan. The owner selected Q137 option B: finite transport of
active, base-proved affine header relations at joins, together with
per-input proof at terminal induction edges. This document states the
precise rule and its implementation plan; the rule is now in the active
[specification](../../../spec/kernel-spec.md) as a proposal awaiting the
owner's approval, which remains the authority, and
[Observed after implementation](#observed-after-implementation) records what
the implementation showed.

The question is whether the existing numeric disposition can carry the
writer's relation over differing current images without making acceptance
depend on how a branch set is parenthesized. The relevant obligations are
[ENT-5]'s all-input join, [ENT-6]'s fixed automatic families and join-shape
commitment, and [INV-1]'s simultaneous induction batches.

Before running the independent model, the comparison and rejection criteria
are fixed as follows:

- Compare eager transport at every syntactic merge against transport once
  at a canonical boundary of a region containing only merges and edge
  cleanup. Each candidate batch reads frozen input states.
- Reject eager transport if an intermediate transported theorem lets a
  later query combine more original premises than the flat branch set can.
- Reject the canonical-boundary proposal if two parenthesizations of the
  same ordered, cleaned inputs and active candidate environment produce
  different facts or dispositions, if any published fact is false on a
  contributing input, or if it expands successive branches into products of
  histories.
- Exercise a failed input, contradictory inputs, equality components,
  inactive or unformable candidates, candidate-order changes, later real
  consumers, and terminal batches. A concrete integer valuation is the
  independent truth oracle; the model is not a WF compiler or a substitute
  for conformance or CI.

The investigation follows the [research method](../../README.md): bounded
evidence answers this scheduling question. There is no performance claim,
compiler build, test-suite run or runtime experiment in this task.

## Evidence and the four witnesses

The inspected worktree is based on
`94b41f20b0f23f98f3fb13576f1afe11385bb088`, with active specification v0.95.
The task supplied the selected plan in another worktree, at
`<scratch-root>/cellword/research/investigations/match-dispatch/DESIGN.md`,
“Stage 4: loop { match }” and “Stage 4: the gaps and the plan”, and the
analysis `<scratch-root>/astra-chk/checker-gaps.md`, Q1 option B. That Q1 is
Q137 in the owner's plan. The language analysis and the separate C1 analysis
were also read. These are input records, not files this branch owns.

**Observed on 2026-10-07:** the supplied older `whitefootc --check` reproduced
these results. Its SHA-256 is
`f9dd9c13f8ffc8e178efa9528ab7e08fbf3bbb1cb587065d23358856511a1f4c`;
this identifies the executable, not an independently verified build commit.
The first small check and its repeat completed in under a tenth of a second,
so the remaining small witnesses were checked. These are verdict observations,
not a performance comparison. The compiler does not contain this proposal.

| Supplied witness | Observed old verdict | Proposed C2 verdict and reason |
|---|---|---|
| `join-min.wf` | INV-1, `xb`, backedge `x <= limit` | Accept: the Set edge proves the current payload bound; the Keep edge uses the current iteration hypothesis. Prove the terminal batch on those edges. |
| `helper-min.wf` | INV-1, `fb`, backedge `fp + 4 <= stack^.inner.len` | Accept: the call edge uses its immutable entry datum and verified length equality; the other edge uses the header image. |
| `helper-post-use.wf` | OP-4 at `stack^.inner[fp]` after the conditional | Accept: the common suffix requires an ordinary join and transport of `fb`; that current-image theorem implies the subscript bound. Terminal-only proof is insufficient. |
| `correlated-cache.wf` | INV-1, `sum`, backedge `acc + cache == limit` | Accept: both equality components hold for `(limit, 0)` and `(0, limit)`. Neither separate coordinate intervals nor equality between their input atoms is required. |
| `join-only-set.wf`, `join-only-keep.wf`, `helper-all.wf`, `correlated-one.wf` | All accept | Remain accepted; they isolate the incoming proofs needed above. |

The proposed verdicts were deductions from the rule when written; the
implementation's observed verdicts are in
[Observed after implementation](#observed-after-implementation). C1 remains a separate prerequisite for using the
complete [MSR-4] disposition in all numeric consumers. C2 neither special-cases
the helper nor changes [CALL-3]'s descriptor kills.

Source checksums identify the exact supplied inputs:

```text
67ab853c07530dd97c1ee2cb6a837139c4fc3866d62ab1cc00577e5ffa9ea612  join-min.wf
b804ba0276078877317bef060b11ff793082c4241a07d741c20f0975a4de75d0  helper-min.wf
9d7c7d2ba6db339b3f5b3884071a3ecf6f5d2225478cabaaee0c35a5a9ac92ec  helper-post-use.wf
6e95bf183742f2ff9ab8b15590b0860db26e9d884cc3f5314534f6c1e4941c64  correlated-cache.wf
4a1164ee6d372f2e9c4ee72efb17c79a094accce3018eebd58e049468f79668f  join-only-set.wf
bc5d138dc85ceec034c002cc1738fa9025860dcb2ca38753a06294b30f3fb9b8  join-only-keep.wf
52f967b5b3796a19872df8205324a66dd09dec5e74679b759c4a9749e25e18f7  helper-all.wf
1f71719f7b3ee3a7f0019b6d1a558e617e57c10e1741f6591068317b5819fa24  correlated-one.wf
```

## Canonical proof boundaries

### Why freezing one merge is insufficient

The issue is proof power, even with unchanged images. In mathematical notation
let six integer atoms have independent type intervals and let the available
non-L0 premises be:

```text
P: a + b <= 0
Q: c + d <= 0
R: e + f <= 0
H: a + b + c + d <= 0             written header template
T: a + b + c + d + e + f <= 0     written header template
U: 2*a + 2*b + c + d + e + f <= 0 consumer
```

The current images in this example are replacement values; the base-proved
header assumptions name older images and cannot prove these instances. Inputs
A and B contain P, Q, R. Input C additionally contains T. H is automatically
proved on every input by P+Q. T is not automatic on A or B: its proof needs
three published premises, whereas AUTO admits at most two. There is no L0
bridge in this example. Integer tightening adds no successful candidate.

An eager inner join of A and B publishes H. At an outer join with C, H+R now
proves T on that intermediate input, so the outer join publishes T. A flat
join of A, B, C publishes H but not T. The difference is observable: T+P
proves U after the eager nested joins; H, P, Q, R cannot prove U with a
coefficient-one pair after the flat join. Merely checking whether the final
state proves T would miss this counterexample: H+R proves T there too, but a
query does not publish its result.

Thus the eager rule contradicts [ENT-6]'s join-shape commitment. It is not an
unsound theorem, but it is a different automatic proof system selected by
parenthesization. Frozen siblings do not settle it.

### Selected refinement of Q137

A **canonical join boundary** is the end of a maximal region made solely of
control merges and the ordinary edge closure and lexical-scope removal that
can be applied separately to each incoming state. An intermediate merge in
that region has no published proof state. Collect its incoming edges into
one ordered frontier and perform the ordinary join and one transport batch
at the boundary. This is a definition of the semantic join, not an optimizer
permission to choose when transport happens.

A shared suffix statement ends such a region before that statement. So do a
new control split, a value delivery or binding, a call, a write, a required
proof judgment, an effectful release, and entering or leaving a loop's
header environment. A `doc`, bare block boundary, and an implementation's
snapshot or cache demand do not. A nontrivial cleanup action is a real event;
its obligations and effects run at their original point, with a join before
it when it consumes several alternatives. Only pure lexical kills and their
closure travel independently on frontier edges.

The terminal boundary at a next loop header is different: [INV-1] consumes
the frontier by proving its batch on each input, without forming a joined
numeric state. At an intermediate boundary with a real suffix, compute the
ordinary join and optional transport, then execute the suffix once. Never
push that suffix back into the inputs or carry alternatives through a new
split. Branch-entry conditions are still established on the selected edges;
this rule only flattens consecutive merges, not branch tests.

For the same ordered cleaned inputs, loop environment and continuation,
rebracketing a merge-only region gives the same frontier. Ordinary n-ary
joining sees the same inputs, candidate queries see the same frozen states,
and simultaneous publication adds the same inequalities, modulo fresh atom
renaming. This establishes the requested nested/flat agreement. The statement
is about regrouping the same branch set. Moving or duplicating an executable
suffix, changing its facts, or inserting a local proof changes the input
problem; [ENT-6] does not promise invariance under arbitrary equivalent
program rewrites. A transported H at an earlier *real* boundary may help
prove T at a later boundary in both spellings.

### Independent finite model

The one-off script is outside the repository, at
`<model-scratch>/model.py`, with SHA-256
`0644c1fad15f470b5bc4a3b7c7e917bc0415993b6e2218413243dc9765de0db4`.
It imports only Python's standard library and no compiler or repository code.
Run it with `python3 -B <model-scratch>/model.py`.
Its retained JSON is `results.json` beside it. Both are disposable research
scratch; this document retains the question, construction and results.

The model uses six integer atoms with interval -2 through 2, exact integer
coefficient vectors, DIRECT by interval substitution, and AUTO's zero-, one-
and unordered-two-premise families, including repetitions and both specified
integer tightenings. Its L0-image and bridge families are empty by construction.
A merge intersects canonical facts and then adds candidates proved on every
non-bottom input. A candidate can have one inequality or an indivisible
pair; independent activity, liveness and formability flags exercise the gates.
The canonical evaluator collects leaves before performing that operation;
the eager comparator recursively performs it at each internal tree node.

For reproduction of the finite enumeration: let P, Q, R, H, T be the bounds
above and let -H be H's reverse. Enumerate all 64 subsets of
`{P,Q,R,H,T,-H}` as input fact sets, all 2,080 unordered pairs including equal
sets, and candidates H, T and the equality `(H,-H)`. Independently enumerate
all 15,625 valuations in `{-2,-1,0,1,2}^6`; a valuation belongs to an input
exactly when it satisfies every input premise. Every output inequality is
checked on every valuation belonging to either input. Bit sets compress this
truth-table comparison without changing its enumeration.

Results, after a small sample followed by the full bounded run:

| Comparison | Observed result |
|---|---|
| Eager `((A,B),C)` versus flat `(A,B,C)` | Eager publishes T and proves U; flat does neither. This falsifies the eager proposal. |
| Canonical version of those forms | Identical publication; both leave U unproved. |
| All ordered binary parenthesizations and permutations of the first 2 through 5 inputs from `[A,B,C,bottom,bad]`, with `bad={P,Q}` | 1,814 comparisons agree with their flat frontier. Duplicate permutations are counted, not described as distinct programs. |
| Truth-table oracle over the 2,080 input pairs | All 3,752 output-fact occurrences are valid; 21,941,770 input-valuation/fact checks, counting repetitions across pairs. |
| Adversarial gates | One failing live input prevents publication; bottom and absent inputs are neutral; empty/all-bottom frontiers stay bottom; unavailable operands, inactive candidates and a failed reverse equality component publish nothing. Candidate reversal changes no result. |
| Mandatory batch | A failed member on one input fails the batch; optional success for H cannot prove sibling T; bottom-only and absent induction inputs are vacuous. |
| A real intervening boundary | Its already-published H can help the later T query, as specified. |
| Twenty successive two-way diamonds, each ending at a real event | 40 frontier input visits, without constructing the 1,048,576 branch histories. This is a model count, not a compiler timing claim. |

This is bounded evidence for scheduling, conjunction and publication, not a
formal verification of WF, the implementation, the complete MSR-4 bridge,
checked i128 overflow, storage invalidation or source cleanup. Liveness and
formability are input flags here; the source-rule argument and planned cases
below must validate their actual derivation. The canonical agreement follows
also from collecting the same frontier; exhaustive small trees can catch an
implementation of that construction, not prove every possible CFG correct.

### Indexed next-header model criterion

Before the additional identity model runs, its criterion is: instantiate a
counted header's `table[i].len` at i+1, compare an isolated target-instance
key with erroneous reuse of the current-place key, and reject the latter
when it carries a true current-slot bound to a false next-slot bound.
Enumerate small arrays independently as the truth oracle. A next index
outside the table must fail formation even when the current index is valid;
the target-instance length/capacity standing relation must remain available.
This models the proposed term identity and formation boundary, not the full
compiler's offset or reference resolution.

The additional script `<model-scratch>/indexed_model.py` has SHA-256
`42299336d9854fecd779bfbb90a3567f56d8081930d0b5d67e1d7cbd3c7121f0`;
`indexed-results.json` records its completed bounded run. It enumerates
tables of 1 through 4 slots, every slot length in 0 through 2 with capacity
2, and every valid current index. Of 426 instances, 306 have a valid next
slot and 120 fail next-slot formation. Reusing the current-slot key proves
68 false next-slot bounds; the smallest is lengths `[1,0]`, current index
0, and target next length at least 1. The isolated keys prove none of those
bounds, preserve the type's length/capacity relation, and distinguish other
loops and edges. These counts and the detected bad-key mutation were
observed after the small sample; no WF or Rust test was run.

## Exact proposed specification edits

These were the replacement passages for integration; the active
specification now carries them, reflowed to one sentence per line, with the
INV-1 diagnostic wording adjusted as noted below. Preserve every unmentioned
clause. The archive, next
title, conformance amendment and eventual approval logs belong to integration.
Q137 selects relation transport and per-input induction. Q146 below proposes
the canonical boundary that makes that direction consistent with ENT-6.

### ENT-2: a counted next-header measure's target-local identity

**Before:** clause (b) identifies a readonly indexed place by its source
spelling and declaration; its offsets are existing tracked places or
constants. That does not distinguish current `table[i].len` from the same
written operand instantiated at the counted next header, where i means i+1.
Merely changing its affine image while retaining the old L0 term would let a
current-slot bound prove a different slot's length.

**After:** a finite compiler-owned target-local measure term represents that
next-place instance. Source term formation and source offset syntax do not
broaden. This supporting vocabulary change is required to make INV-1's
next-value substitution precise for indexed operands; its scalar substitution
alone needs no new term.

Change “The final alternative (i)” in the term inventory to “Alternative
(i)”, leaving its definition unchanged, and append this alternative:

> Alternative (j) is a compiler-owned target-instance measure term used
> while forming or proving an [INV-1] counted next-header relation. For a
> measure factor whose place contains the counted binder in a subscript
> offset, each measure of that selected measured place, and each measure
> prefix needed to discharge its subscripts, has a target-instance term
> when its own path contains that offset. The term is identified by the
> concrete function instance, the for statement, the incoming induction
> edge, the ordinary source root's declaration event and canonical source
> path with every counted-binder offset replaced
> by the one next-binder selector for that loop and edge, and the selected
> measure member. That selector denotes [INV-1]'s checked current-binder
> plus one value; all its occurrences on the edge denote that one value.
> Prefixes without a substituted offset use their ordinary terms.
> Other source offsets keep their ordinary declaration and spelling
> identities. Resolved storage paths serve validity and support checking,
> and establish no additional equality between different source spellings.
>
> A target-instance place is formed from base to leaf, proving each
> [OP-4] bound in the frozen incoming state using these substituted
> operands before the selected measure term becomes available. The term
> has the measure's ordinary u64 type, a current affine image belonging
> to this target instance, and exactly [MSR-1] and [MSR-2]'s applicable
> standing images and relations for that measured place. Measures of one
> target-instance place share that place's standing relations. These are
> available only in the query's formation view. A term in this namespace
> is distinct from every ordinary current-place term, including one with
> the same source spelling. Existing ordinary bounds, atoms and measure
> datums keep their current-place denotations. No equality or relation
> between a target-instance measure and a current-place measure is
> established by their spelling or by this instantiation.
>
> A formed target-instance measure is a live measure candidate for
> [MSR-4] in that formation view, and is the side term submitted by a
> normalized component naming it. It participates in the same numeric
> disposition and receives no query against the ordinary current-place
> term in its place. Target-instance terms and their private standing
> facts add no premise to another batch member's frozen input and leave
> no ordinary fact at the continuation or next header. Their identities
> and proofs remain in the retained derivation. They evaluate no runtime
> expression and introduce no writer-visible binding or offset form.

This is conservative about equality with another source offset. It adds no
array-read congruence or proof search to identify a target slot with an
ordinary slot named through a different binding. A requirement for that
additional composition would be a separate language gap, not a reason to
reuse the current i term unsoundly. The four C2 witnesses contain no such
next-indexed measure and do not select a broader congruence rule.

### ENT-5: canonical joins and relation transport

**Before:** arm and branch exits join at each syntactic continuation. L0 keeps
the weakest all-input bound; affine facts keep identical inequalities over
identical immutable images. There is no transfer of a written relation to a
new joined image. Terminal joins happen before the induction query.

**After:** the ordinary domains retain their join rules over the canonical
frontier; an explicitly bounded candidate batch adds all-input relations at
a real continuation. A terminal induction consumer reads inputs directly.

Replace the sentence beginning “Header invariant conclusions and local
invariant conclusions alike follow” with:

> Header and local invariant conclusions retain their immutable value-image
> meaning on every edge, including edges leaving their loop; their ordinary
> survival at a join is the canonical intersection specified below and in
> [ENT-6]. The additional transport below proves fresh instances of active
> header relations. A header invariant's name leaves lexical scope with its
> loop body [INV-1].

Immediately before “Joins: at the continuation of a `match_stmt`”, insert:

> A canonical join frontier is defined on the conservative structural graph
> [FN-1]. Starting at a continuation that merges control inputs, replace an
> incoming merge-only continuation by its incoming edges, repeatedly, in
> source edge order. A merge-only continuation performs control merging and
> the pre-exit closure, lexical scope kills and surviving-state closure of
> this rule, and has one successor. Apply those edge events separately to
> every input routed through it, in their original order. The expansion ends
> at an incoming source action, control split, value transfer, required
> judgment, loop-header environment boundary, or cleanup action with an
> effect or required judgment. These endpoints and the region's final
> continuation are its cut points. Documentation and administrative block
> boundaries add no cut point. A maximal expanded region has one semantic
> join at its final continuation and has no intermediate joined fact state.
> Every new control split consumes one state at its cut point, so this
> expansion contains incoming structural edges, not combinations of earlier
> branch histories. Edge order is the source NodePath order specified below;
> each routed edge also retains its ordered cleanup sequence.
>
> At a cut point with an ordinary continuation, form the ordinary join of
> that frontier using the domain rules below and [ENT-6], before adding
> transported header relations. At a cut point whose continuation is solely
> a next-header induction judgment, apply [INV-1]'s per-input batch to the
> frontier. A loop's entry and exit each delimit the active header
> environment. A loop continuation still joins its own exit edges; the
> active environment there is that of its enclosing body.
>
> The transport candidates are exactly the affine header relations of loops
> lexically enclosing the destination whose complete base batch succeeded.
> Visit enclosing loops from outermost to innermost and their written
> relations in source order. Each candidate denotes its written relation
> instantiated with the current operand values at this point; a counted
> binder here denotes its current value. Local invariant declarations,
> requirements, postconditions and range clauses supply their ordinary
> facts, and supply no additional transport templates. Header templates
> remain active independently of whether the current iteration's original
> theorem survives ordinary intersection.
>
> A candidate is eligible when every operand is live and admitted by
> [INV-1], [ENT-2] and [MSR-1] on each non-contradictory input and in the
> ordinary joined state. For each such state, form the candidate in a
> separate view of that frozen state, visiting written operands left to
> right and subscript prefixes from base outward. Each subscript bound is
> submitted to [MSR-4] before the measure below it is formed. Formation
> uses current resolved places, reference validity, offset values and
> measure images, together with the existing standing facts for admitted
> terms. These views publish no fact into another candidate's view. An
> unavailable operand, unproved formation bound or unrepresentable affine
> form makes this optional candidate ineligible. [INV-1]'s source formation
> ceilings and checked integer arithmetic also apply to these forms.
>
> For each eligible relation, instantiate its [INV-1] normalized components
> separately with every non-contradictory input's current images and submit
> each component to [MSR-4]. Each query retains the source side's L0 term
> when available, including the side required by the bridge. A relation is
> transported exactly when every component is proved on every such input.
> The ordered relation's one component, or the equality's complete pair,
> is then established over the ordinary joined current images with a
> derivation recording the template, each input's substitution and proof,
> the output substitution, and the contradictory inputs' dispositions.
> All candidates read frozen inputs and the ordinary joined state; publish
> successful relations together in candidate and component order, as
> [ENT-6] specifies. A failed optional candidate publishes nothing and
> creates no source rejection. A frontier with no non-contradictory input
> has the contradictory state and an empty transported sequence.
>
> Transport establishes a new immutable theorem about the current joined
> values. It changes neither the denotation of an existing named proof nor
> any input value's identity. A later write or scope exit follows the
> ordinary image and support rules above. Transport changes no runtime
> operation, evaluation, effect, reference or control edge.

Change the opening of the existing “Joins:” paragraph to:

> Joins: at the continuation of a `match_stmt` or `value_match`, the ordinary
> fact state is the join over its canonical frontier, initially comprising
> every arm exit edge reaching that continuation on the conservative
> structural graph [FN-1], each taken after its applicable edge events;
> an arm every path of which leaves by `return`, `break` to an enclosing
> loop, or `propagate`'s error edge contributes nothing there.

Keep the existing weakest-bound, signed-goal, empty/contradictory and edge
ordering clauses, and their application to `if`, value delivery, loop breaks
and counted false-header exits. “The join of closed states is closed” still
refers to the ordinary L0 join, not saturation of the added affine theorems.
Pure value-delivery *joins* use this frontier; the give's substitution and
receiver binding are value-transfer cut points, preserving [GIVE-1]'s scope.

In the ordinary-loop paragraph replace the two sentences starting “At every
reachable normal body fallthrough” and “If no normal fallthrough” with the
following cross-references, retaining the intervening next-head restriction:

> [INV-1] proves preservation at the ordinary loop's induction frontier.
>
> [INV-1] determines vacuity from that frontier.

In the counted-loop paragraph replace the sentence starting “At every
reachable normal body fallthrough” with:

> [INV-1] proves the counted next-header batch on each induction input,
> including the hidden next-binder image and its representability judgment.

Replace the following “This order is fixed” sentence with:

> The order is preheader establishment and closure, simultaneous base proof,
> continuing-kill subtraction, header-batch activation, S11 body-entry
> establishment, body flow, and [INV-1]'s per-input induction judgment.

These references leave one owner for the exact proof boundary and update
order, rather than two separately worded induction rules.

### ENT-6: image formation and the automatic premise list

**Before:** a join retains identical images or compatible constant deltas,
and otherwise gives a scalar a fresh full-type atom. Only canonically
identical affine premises survive. Delta folding promises nested/flat
agreement but does not regulate additional intermediate publications.

**After:** image joining and intersection run once on ENT-5's frontier;
the precisely enumerated transported facts are additional automatic premises.
AUTO's candidate families, coefficients and lack of saturation are unchanged.

Replace “At a control-flow join, a binding keeps an identical image held on
every non-contradictory input.” with:

> At an [ENT-5] canonical join, a binding keeps an identical image held on
> every non-contradictory frontier input.

Retain the following normalization and delta construction. Replace the
sentence beginning “A delta atom is an ordinary shared atom” with:

> A delta atom is an ordinary shared atom at query points; at a later join
> it is folded as above. [ENT-5]'s frontier fixes the input states and the
> placement of added premises: regrouping the same ordered incoming edges
> solely by merge-only continuations gives the same joined images and
> automatic facts, up to renaming fresh atoms, and the same dispositions.

Replace the sentence beginning “They create no independently selectable
premise” with:

> These transfers create independently selectable premises exactly for
> invariant conclusions, the specification-fixed automatic images below,
> and the transported header relations of [ENT-5].

Replace the two sentences beginning “Every live measure term carries” and
“That atom is not a source binding” with:

> A live measure term has its current immutable affine image. A measure
> whose [MSR-1] rule supplies a standing constant or captured range image
> uses that rule. Every other measure uses a fresh atom over the complete
> u64 interval when its current image is first required. A kill of the
> measure term's support [MSR-2] removes its current-image association;
> the atom and any retained theorem about its former value stay immutable.
> A canonical join retains a measure's image when every non-contradictory
> input has that identical image, and otherwise supplies a fresh joined
> atom when required. This association is shared by every consumer at that
> point and establishes no equality with a predecessor's different image.

This makes the input/output substitutions explicit; it does not equate
pre-call and post-call lengths or retarget an old observed length.

Replace the affine-sequence paragraph starting “At a join, an inequality
survives exactly when” through the paragraph ending “Ordinary L0 relations
are not copied into that list.” with:

> At a canonical join, the ordinary surviving affine sequence consists of
> exactly the inequalities canonically identical on every
> non-contradictory frontier input. It is ordered by first occurrence in
> the first such input, using [ENT-5]'s edge order, and its representative
> on each input is that input's first occurrence. Append the relations
> transported under [ENT-5] in that rule's candidate and component order.
> The complete sequence represents each canonical inequality once, at its
> first occurrence. Contradictory inputs are neutral; an entirely
> contradictory frontier has an empty affine sequence because L0 already
> proves every target. Source and derivation categories determine evidence,
> not proof authority. Ordinary L0 relations stay in their closed domain
> and its query index.

The next paragraph defining AUTO is unchanged. In particular, asking MSR-4
about an optional candidate does not publish successful residuals. A published
transported theorem participates in later ordinary queries just like any
other proved affine theorem. [ENT-1]'s inventory already includes ENT-6's
specified automatic affine images; it needs no independent new fact source.

### INV-1: induction inputs, simultaneous batches and diagnostics

**Before:** a match's already joined continuation is usually the state of
its one syntactic body-fallthrough backedge. The target batch is simultaneous,
but predecessor relations may already have been erased.

**After:** prove each incoming terminal edge's entire batch in its complete
post-cleanup state; never join those inputs solely to prove induction.

Replace the sentence beginning “Such an atom denotes the [ENT-2] measure
term” with:

> A measure factor denotes its current [ENT-2] measure term, or that
> rule's target-instance term at a counted next-header substitution,
> lifted from u64 to its mathematical integer value. [MSR-2] fixes
> support and [ENT-6] fixes the lifetime of its current-image association
> and the immutable meaning of a theorem about a former image.

This removes the existing “no conclusion resting on it survives the write”
wording: ordinary supported facts die, while an immutable theorem may still
serve a live saved copy. It cannot prove a new current measure by retargeting.

Replace the subscript-placement sentence beginning “A subscript inside a
measure place is an ordinary [OP-4] occurrence” with:

> A subscript inside a measure place owes [OP-4]'s bound with offsets and
> prefixes resolved in the relation's current value environment. At a local
> invariant it is judged in the entering state. At a loop header its base
> and next-header instances are judged in the corresponding induction
> input below, before any target from that batch is published; a counted
> next-header instance uses the next-binder substitution also in offsets.
> [ENT-5] owns the formability checks of optional transport instances.

Replace the batch passage beginning “For the base batch” and ending “an
internal affine term or value-image identifier is never the writer-facing
residual” with:

> For the base batch, form and submit every header target to [MSR-4] in
> the complete preheader state, using the counted initialization where
> [ENT-5] supplies one. Every target reads that same state. The batch is
> published as the current-iteration assumptions exactly when all bases
> succeed. No target assumes a conclusion of its own base batch.
> The formed operand instances accompany those assumptions. Their
> formation obligations are part of the same base and next-header
> induction: they license the header's measure images and publish no
> additional numeric inequality beyond its written relations.
>
> A loop's induction frontier consists of the normal body-fallthrough edges
> reaching its next header, expanded through merge-only continuations by
> [ENT-5]. Each input first performs the ordinary body effects and its
> applicable edge cleanup in source order. On each non-contradictory input,
> form and prove the complete next-header batch against that frozen complete
> state while retaining the current-iteration assumptions. Successful
> members supply no premise to another member or input. The next-header
> batch succeeds exactly when every member is proved on every input.
> A contradictory input is discharged by its contradiction. With no
> reaching input the batch is vacuous. A `break`, `return`, or `propagate`
> error edge creates no induction input.
>
> An ordinary next-header target uses the current operand values on its
> input. A counted input first forms the exact mathematical current-binder
> plus one and proves that hidden update representable in u64; it then
> substitutes that next value for every binder occurrence in the target,
> including occurrences in indexed operands, and uses the current values
> of other operands. Formation and proof use this target substitution
> while the premise state still denotes pre-update values. A measure
> selected by a changed offset denotes the selected next place, not the
> measure at the old offset, through [ENT-2]'s target-instance term.
> This is induction for an arbitrary iteration;
> neither a particular second iteration nor endpoint re-evaluation supplies
> a premise.
>
> No joined numeric state or optional transport batch is formed solely for
> this induction judgment. Other required source judgments on its inputs
> still run at their specified points. After successful induction, exactly
> [ENT-5]'s head facts are available at another iteration; arbitrary body
> facts and optional successes are not additional induction hypotheses.
>
> Failure reports the invariant name, base or backedge, the failing
> incoming edge's source location and route, and the complete required
> source relation after any next-binder substitution. Inputs follow
> [ENT-5]'s edge order; within an input the header relations use source
> order and an equality uses its forward then reverse component. For
> each owning relation, report its first failing input and component in
> that order. Selection among violations at distinct source nodes follows
> [DIAG-1]. The residual uses
> source bindings and paths; it contains no internal atom identifier.
> Failure to form an indexed operand identifies its subscript and
> required bound under [OP-4]; a normalized-relation formation failure
> keeps this rule's owning-invariant location. Required formation or
> proof failure rejects even when optional transport previously failed
> silently at another point.

All remaining INV-1 clauses stay, including header names, local certificates,
proof erasure, and counted exhaustion. A successful transported fact can
make a later explicit certificate redundant under the unchanged [PRF-1];
that is a consequence of the specified automatic fact change, not permission
to weaken the certificate rule.

The continue task (Q138) adds its explicitly resolved edges to this same
induction frontier and applies its own loop/scope cleanup first. Its ordinary
and counted obligations use this one sink; this design does not introduce a
second `continue` checker or change that task's syntax.

## Candidate, soundness and cost details

### Candidates and support

- Lexical activity belongs to the original declaration's enclosing loop
  body, not to an optimization, the current use of its proof name or whether
  the original theorem survived an earlier merge. Enter an inner loop only
  after its complete base batch succeeds; outer active templates precede it.
  Exiting the inner loop removes its templates. Its already-proved immutable
  conclusions may still survive ordinary intersection, as before.
- The source relation is the template. Substitutions are by its resolved
  operands, **not by replacing every matching affine atom**. In one input
  two operands can share an atom, or one can be constant, while on another
  they differ. Per-input whole-relation proof and the runtime selection of
  one complete input justify the output theorem. Equating all input atoms
  to all output atoms would lose this distinction and is not the rule.
- Candidate normalization uses INV-1's existing ordered components and i128
  ceilings. Duplicate written relations are visited in source order; output
  canonicalization removes duplicate inequalities without changing success.
  An equality transports neither component unless both succeed everywhere.
- A write ends a measure association according to descriptor support. A
  preserving call can re-prove the relation using its current exit measure
  and immutable entry datum. The presence of the old theorem or an identical
  path spelling alone proves nothing about that new measure.
- The approved requirement “live and formable at this join” is read
  conservatively and literally: indexed operands must also be formable in
  the **ordinary joined state before transport**. Input-wise bounds alone
  do not introduce a new all-input formation-fact rule. An optional
  candidate whose bound needs a sibling transported relation is skipped,
  even if that sibling succeeds. A later real cut may use the published
  sibling. This avoids a circular formation/publication dependency and
  keeps a fixed one-pass candidate rule. It can leave an indexed template
  unavailable at a join; the ordinary mandatory consumer must still prove
  its own bounds. No runtime guard is a substitute for that proof.
- A reference must remain valid and resolve through the ordinary exact-term
  rules on every input and at the output. A possibly aliased origin set is
  not an equality of selected storages. Candidate formation must not revive
  a consumed root, expired holder, old offset or killed variant refinement.
- A structural absence (`return`, escaping `break`, error transfer) is not an
  input. An unselected but structurally reaching branch still contributes,
  unless the existing ENT-4 judgment establishes contradiction. No new
  theorem prover classifies a path impossible. All-bottom and empty ordinary
  joins stay bottom; the affine publication list is empty, not an arbitrary
  list of vacuously formed dead relations. Required source structural checks
  and FN-9 entry-stability metadata retain their own reachability policies.
- A header relation may be false temporarily within an iteration. Optional
  transport then fails silently; restoration before the actual induction
  input can still succeed. A break after a write does not owe restoration
  and receives no newly rebound theorem for the loop it exits.

### Soundness argument

Assume the current type, image, support and MSR-4 rules are sound. For each
consistent incoming runtime state represented by input i, an eligible template
R is well-formed there, and its retained derivation establishes
`R(values_i)`. Every execution reaching the continuation selects exactly one
of those inputs. The joined current bindings and measures denote that
selected input's values, so `R(values_joined)` holds. The output formation
gate establishes that the published expression still denotes live valid
places there. This is an all-input introduction rule, not an affine
substitution identifying mutually exclusive atoms. Equality uses the
conjunction of its two independently checked components.

No target proves itself: base hypotheses are introduced only after the whole
base succeeds; optional transport assumes only its frozen current states;
mandatory batches publish no successful prefix; and the next iteration is
checked from a symbolic header, not a replay of the preceding iteration's
facts. An earlier transported theorem is sound for its own immutable image,
so use at a later real boundary is sound; canonical cuts additionally fix
which such intermediate theorems are automatic. Cleanup precedes each
relevant query, and existing support rules govern every subsequent kill.

### Finite work

Let an ordinary boundary have m non-contradictory frontier inputs, h active written
relations, and s(r) written subscript occurrences in relation r. Each
relation has one or two normalized components. The new positive numeric
queries are bounded by

```text
sum over r: (m + 1) * s(r) + 2 * m
```

The `+1` accounts for formability in the raw joined state. A mandatory
ordinary-loop terminal batch uses only its m inputs, so its bound is
`sum_r m * (s(r) + 2)`. A counted terminal batch adds m hidden-update
representability queries, also when h is zero; a failed hidden update
cannot be bypassed by an empty written header. The finite target-instance
measure vocabulary adds at most the measures of the written operands and
their subscript prefixes per counted edge. Term/type/reference formation and
normalization also visit the fixed source template. Failure classification
may ask the negated component once, raising the component-query bound to
4m per relation; it never restarts a batch. Contradiction is established once
per input by the existing closure. Every query uses the unchanged complete
MSR-4 families, not a constant-time solver: AUTO pair work and ordinary
closure costs remain separate. Optional failures exhaust the same finite
families; a timeout cannot turn one into a skip or an input into bottom.

The frontier traversal stops at the next real event or split and visits
structural edges with their finite lexical cleanup chains. Sequential
branches therefore have separate frontiers, not Cartesian products. There
is no relation saturation, dynamic iteration count, body unrolling or
inference of new templates. A function can add at most two transported
components per active written relation per canonical boundary. This bounds
the number of new facts by a polynomial in source size. It does **not** show
that a many-arm compiler check meets the phase's seconds-scale target; that
requires C3's cost evidence.

## C3 and the implementation plan

### Joins that are no longer required

In `loop { match ... }` with no common body suffix, each final arm (including
trailing nested branches) can finish at the induction sink. All joins solely
combining those exits for the header query disappear. No ordinary L0 join,
joined affine images, optional current-header transport, success-context
join, or separation intersection is needed *solely to serve that numeric
induction sink*. Retain the per-edge proofs and all other judgments.

A join before the indexed write in `helper-post-use.wf` remains necessary.
So do joins before calls, later branch tests, local proofs, value receivers,
loop preheaders, loop exit consumers and cleanup judgments. Branch-local
closures, term snapshots and required range/ownership/permission work remain;
a numeric sink is not permission to bypass another consumer. If a downstream
analysis requires a state at a terminal boundary, supply that analysis's
specified join without feeding it back into induction proof or adding an
extra transport round. C3 can make these residual joins exact and demanded;
its caching and forcing points must not become semantic cut points.

### Checker mechanisms and interfaces

The following is a plan, not declarations already present in the compiler.
Keep the current single entailment traversal and private interfaces.

| Owner | Planned change and contract |
|---|---|
| `compiler/src/semantic/entailment/flow.rs`, `LoopFrame` | Retain the resolved header templates, base-batch success, lexical destination scope, counted binder, and per-input induction outcomes. Store template eligibility independently of `published_invariants`, whose named theorem remains immutable. Add a private continuation description distinguishing a real consumer from an induction sink. |
| `flow/walk.rs`, `walk_block` and branch/value/loop arms | Return or route an ordered frontier through merge-only continuations. Materialize once at the source-defined cut point; execute a shared suffix once. Route implicit fallthrough and Q138 continue edges to the same loop-owned sink. Preserve atomic exit checks, scope effects, reachability, capture lifetime and all structural judgments. |
| `flow/events.rs`, `exit_scopes_to`, loop exits | Apply closure-before-kill and final surviving closure separately to pending edges. An effectful release or required cleanup check seals the frontier before that action. Keep continuing-kill collection and entry-image stability behavior. |
| `flow/domain.rs`, `join_flows`, `join_affine_states`, `join_affine_facts` | Retain ordinary n-ary joining, contradiction handling and canonical intersection. At an ordinary canonical boundary, freeze inputs, form the ordinary output, then request and append exactly the transport batch. Give every current output measure one shared image, with no inherited equality to disagreeing inputs. |
| `flow/invariants.rs`, `prove_affine_relation_batch` and checked-form helpers; `flow/judge.rs` subscript formation | Share target instantiation between optional transport and mandatory induction. Return formed components, source-side terms, formation evidence and failure data. An optional formation failure is data, not a mandatory OP-4 obligation inserted into the function. A mandatory instance registers and answers its real obligations. Never create an unknown image to resurrect a dead binding. A counted target environment substitutes next-binder values in offsets as well as scalar atoms without changing the premise state. |
| `flow/prover.rs`, `ProofContext`, `ProofGoal::Affine` | Consume the complete MSR-4 route, retaining the actual right term for each orientation; C1 supplies that shared disposition. Return derivation roots, not only `TargetDisposition`. Query-local term formation and caches must not enrich another candidate's input. Stable term/image identities and the frozen context define each query view. |
| `term.rs` and the term/image vocabulary in `flow/domain.rs` | Add ENT-2's finite counted target-instance measure keys, with the loop and incoming-edge identity and substituted selector. Build only written operand prefixes and their type-owned measures. Keep their standing facts in the formation view and pass their own side term to the prover; ordinary current-slot L0 facts and images cannot be read under the target key. |
| `state.rs`, derivation nodes; `semantic/entailment.rs`, outcomes | Represent a transported relation by template identity, output substitution, ordered input substitutions and component roots, with neutral contradictions accounted for. Existing `JoinedSourceProofProvenance` intersects identical conclusions; it cannot alone justify substitution to different images. Add a distinct all-input relation derivation. Record every required incoming batch, including failures and the no-edge case, with aggregate status as a view of those records. |
| `semantic/check/acceptance.rs` and diagnostic consumers | Ensure each structurally required induction input is represented and undischarged/missing outcomes reject. Extend the owning loop obligation's complete input inventory through the established acceptance-record boundary. Render the first failing route, relation and component from source data. Preserve checked facts in both facts-on and facts-off modes. |

An optional failure should be retained only as explanatory data, never as an
acceptance authority. For a mandatory failure the primary diagnostic is,
for example, `INV-1`, invariant `fb`, backedge from `Set`'s false branch,
`fp + 4_u64 <= stack^.inner.len`, unproved/refuted as established on that
input. It must not print a fresh join-atom name or another input's success.
The specified input priority is within one owning invariant; selection among
different invariant nodes remains the existing diagnostic policy.
For an ordinary later consumer, keep its own rule and residual (OP-4 for the
indexed write) and, when relevant failed transport evidence is used, name the
specific relation and input that could not carry it. An optional failure alone
is not proof of why an arbitrary later goal failed. Do not repeat the old
helper diagnostic's advice to add an equality its `ensures` already contains.

The design gap in the current interface is concrete: `prove_affine_relation_batch`
returns a disposition, while transport needs the proofs and the input/output
substitutions. Extend that result once for both clients rather than proving
again just to recover evidence. `record_loop_invariant_outcomes` currently
stores one step result per header relation; extend it to per-edge evidence
without losing the existing aggregate consumers. C3 owns the ordinary join's
cost representation; C2 owns when a relation may be established.

## Test and integration plan

There is no `tests/conformance/README.md` at the inspected revision. The
conventions read instead are the manifest's header and
[runner.py](../../../tests/conformance/runner.py)'s module documentation:
canonical case sources, `rules`, `expect`, `status: "runnable"`, and a
normative explanation in `doc`. Conformance expectations follow the amended
specification, not observed compiler limitations. This design adds no cases
or expectations; integration adds them with the implementation.

### Normative evidence

Use source-only `accept` and owning-rule `reject` cases. A new runtime case is
unnecessary for this erased proof rule. The four supplied failures must be
promoted as canonical standalone cases, with body docs and an ordinary entry
where the adapter requires one; no formal test imports a research witness.
Their semantic content stays the same. The first four rows below fail with
the old rule for the observed reasons above. Negative controls already
rejected under the old rules are **not** claimed to fail before this change:
reuse existing coverage where possible, and demonstrate a mutation of the
new transport rule would wrongly accept each genuinely new safety case.

| Planned observation | Expected result and discriminating failure |
|---|---|
| Guarded replacement plus untouched input, from `join-min` | New acceptance; old INV-1 rejection loses the two distinct current images. A guard-reversed input must fail its own backedge. |
| Conditional preserving helper, from `helper-min` | New acceptance; old INV-1 rejection loses the call edge's current-measure bridge at the join. Removing preservation from the helper's contract remains a negative, not an expectation to weaken. |
| Shared suffix after conditional helper, from `helper-post-use` | New acceptance; old OP-4 fails before induction. This detects an implementation of terminal proof alone. |
| Correlated equality, from `correlated-cache`, also with a real suffix that consumes the equality | New acceptance; old INV-1 loses correlation. A changed arm satisfying only one equality orientation detects publishing a successful half. |
| Same branch set flat and nested, including the P/Q/R/H/T/U counterexample | The canonical pair has identical facts and U is unproved in both. Current code lacks the new transport facts; an eager-transport mutation distinguishes the forms and fails the new expectation. This is a counterexample to a tempting implementation, not a reported old compiler bug. |
| Relation false at an internal join, restored before induction | Accept if all later mandatory goals prove. This distinguishes optional transport from an added invariant obligation at every join. Old behavior may already accept; first check for existing coverage. |
| Multiple ordinary/counting headers, nested loops, inactive inner template and changed outer image | Active outer then inner templates only; source order or sibling success never proves a base/step target. Existing simultaneous-batch cases protect old behavior; add only missing cross-boundary observations. |
| Indexed measure changed through its offset, descriptor or reference holder | Optional formation fails without poisoning an unrelated suffix; a later required indexed use still rejects if its bound fails. Reject a counted next-header measure when `i + 1` selects outside the table even if current `i` is valid. Require a positive paired case with both bounds proved. Old compiler behavior for these new combined cases is unmeasured. |
| A counted next-slot measure with a true current-slot bound and a false next-slot bound | Reject under the new ENT-2 target-instance identity. `[1,0]` lengths and i=0 independently refute copying `table[i].len >= 1` to the next slot. Pair with the always-valid `len <= cap` relation at a proved in-bounds next slot. A mutation reusing the ordinary side term must fail this negative; no old-compiler verdict is claimed. |
| Contradictory, structurally absent, empty and single-surviving frontiers | Contradiction is neutral only for numeric joining. One consistent failing predecessor always prevents publication. Reuse current reachability controls and add a transport-specific false-positive mutant. |
| Break/return/error exits; counted exhaustion; Q138 continues | Exit routes owe no induction; every actual next-header route does, after cleanup and the counted hidden update. Existing breaks and exhaustion cases remain; continue-specific cases belong to Q138 using the same sink. |

### Existing expectations that legitimately change

Source inspection identifies these integration changes; none is edited here:

- `ent6-neg-join-one-arm-advances-accumulator` is currently a mandatory
  INV-1 rejection whose manifest explicitly says its relation holds on every
  execution. With per-input counted induction, the unchanged arm preserves
  `sum <= 255*i <= 255*(i+1)`; the updating arm has the byte's upper bound.
  Change its verdict and explanatory doc to acceptance under the amended
  ENT-5/INV-1, preserving its source observation. Its old negative name can
  be renamed with the case in the same amendment. Do not remove its gate
  entry. This deduction needs CI on the changed compiler.
- `inv1-neg-sequential-guarded-steps` currently loses a bound over successive
  conditional increments. Each join before the next source consumer can
  transport `fast <= limit`; after a permitted increment, `fast < limit`
  proves the bound for the new image. Change the expectation to acceptance
  when integrated, under the new ENT-5 rule. Do not describe it as a test fix
  without the specification change.
- Compiler case `a_failing_body_probe_is_reported_before_the_header_backedge`
  in `semantic/tests/loop_invariants.rs` relies on the old join losing
  `hi <= spare`. Its local proof now succeeds by transport. Preserve that
  program as positive evidence, and preserve the diagnostic-order
  observation with an independently unproved body target; document both
  reasons. A header-only negative expectation is no longer its oracle.

Keep the truly invalid existing cases, including
`ent6-neg-nested-join-does-not-invent-a-bound`, `inv1-neg-backedge-unproved`,
`inv1-neg-base-unproved`, killed-measure and invalid-subscript cases.
The two existing nested/flat positive conformance cases remain positive.
Audit every affected failure after integration; this is a bounded inspected
inventory, not a claim that no other automatic-proof or PRF-1 redundancy
expectation changes. Q145's redundant-runtime-test change is separate and
may independently reject old control guards; report those as its rule changes.

### Compiler obligations and CI

Put representation evidence in the existing entailment and loop-invariant
unit-test homes, not WF language tests disguised as Rust fixtures:

- Assert retained all-input derivations name every contributor and the exact
  input/output relation instances, with both components for equality; a
  missing or failed required edge keeps the function unaccepted. Old code
  has neither transport roots nor an input inventory, so these observations
  are unavailable before implementation.
- Count numeric join construction for an N-arm terminal sink versus an
  otherwise identical common-suffix consumer: the former constructs no
  terminal numeric join, the latter exactly the specified canonical one.
  Instrumentation is test-only structural evidence, not a wall-time limit.
  The existing walker eagerly calls `join_flows` and fails the first count.
- Vary eager/lazy forcing, hash insertion and query order while preserving
  source order; compare dispositions and source-ordered failing-input
  diagnostics. Use the existing eager ordinary-join oracle for C3, and a
  small flat-frontier reference for C2's transport scheduler. Do not compare
  two invocations of the new implementation as the only oracle.
- Exercise all formation failures and cleanup barriers, immutable named
  proofs versus new current-image facts, no publication from a failed batch,
  and cache invalidation after writes. Deliberately publish one failed
  component, admit one dead operand, or drop one live bad predecessor and
  require the corresponding case to fail. Do not claim every preservation
  control has a red baseline; it protects against these specific mutations.

Run the focused CI checks for the touched semantic tests, conformance and
acceptance/diagnostic records, then the complete project gate at the
integration revision. No suite has been run in this design task. C3's
separate requested investigation must measure arm scaling and query counts,
including failed optional transports and persistent shared suffixes, under
its unchanged oracle and prescribed CI machine. No checking-time result is
asserted here.

## Completion review and evidence limits

A separate read-only agent using the inherited GPT-6 model reviewed the
four-file change against base
`94b41f20b0f23f98f3fb13576f1afe11385bb088`, including the untracked
investigation and design node, relevant specification and design ancestors,
checker consumers, existing expectations and both finite models. Its focused
re-review found all four findings resolved: distinct counted next-slot term
identity and source-path spelling, the INV-1 measure-retention contradiction,
the omitted counted hidden-update query cost, and diagnostic ordering across
distinct source nodes. No findings remained within that scope. This evidence
record was added after the review; it changes no proposed rule.

The review checked applicable prose/evidence, design consistency and
decision-to-artifact correspondence requirements, plus proposed specification
and test-integrity requirements. Full semantic soundness remained unverified;
implementation, execution wiring and deletion checks were not applicable to
this design-only change. The reviewer independently checked artifact hashes
and inspected the model results without rerunning them. Witness verdicts are
the author's observed old-compiler results, not independent reviewer runs.
Whitespace inspection found no errors. No build, suite, design lint or CI ran.
Formation, reference validity, cleanup, checked i128 arithmetic, implementation
behavior and performance still need their specified implementation evidence.

## Disposition and approval boundary

Q137 option B is approved as the direction. Its terminal-only alternative
cannot admit `helper-post-use`; scalar interval widening cannot preserve a
correlated equality; transporting arbitrary inferred relations has no
approved finite candidate set. The proposed
[design node](../../../design/language/checks-and-proofs/join-relations.md)
records the chosen relation family and the canonical-boundary refinement.
The existing loop-exit retention decision stands: this proposal creates no
new obligation or current-image transport for the header of a loop being
exited. The no-SMT, no-budget and finite-certificate decisions also stand.

---

**Q146 — Use canonical merge-only frontiers for Q137's new publications?**

- **Background.** The independent finite model finds the frozen-batch eager
  counterexample above: P+Q establishes H at an inner join; H+R establishes
  T at the outer join, enabling T+P for U, while a flat join cannot publish
  T and cannot prove U. ENT-6 promises the same branch set's nested/flat
  agreement. The approved per-relation/input query bound also excludes
  silently iterating transport until no more relation succeeds.
- **Options.** A, recommended: one frozen transport batch at each canonical
  merge-only frontier, and per-input proof at terminal induction. This
  preserves regrouping and bounds new queries; an intermediate synthetic
  merge adds no theorem that only its parenthesization can supply. Real
  suffixes remain boundaries, so this is not invariance under arbitrary
  source rewrites. B: revise the approved bound and specify finite
  saturation of the written template set at canonical boundaries. This
  could make more relations automatic, but requires a new closure rule,
  as many as h rounds for h templates, a larger query bound, and evidence
  about formation and implication order. It is unnecessary for the four
  C2 witnesses and is not selected here.
- **Confidence 4/5.** The counterexample settles that frozen eager batches
  are insufficient; the frontier identity argument and bounded model
  support A. Full source formation, cleanup, i128 and implementation tests
  remain CI obligations and can expose an incomplete boundary definition.

---

Found along the way: the existing TODO's guarded-update entry proposes
rewriting a loop or adding a runtime recheck; it is superseded in place by
this approved direction, still marked unimplemented. The old helper
OP-4 diagnostic recommends an already-present postcondition; its repair is
included in the diagnostic plan. The absent conformance README is not
replaced with another document; the existing manifest and runner own the
conventions. General join loss outside active header templates remains the
separate TODO item “An affine bound is lost at a statement join where the
binding's images differ”; this bounded mechanism does not claim to close it.

## Observed after implementation

Implemented on the phase-1 branch of
[PR #270](https://github.com/Ming-Research/Whitefoot/pull/270), on top of
026074111. Observed with that compiler, built locally on an Apple M5:

- The four C2 witnesses `join-min`, `helper-min`, `helper-post-use` and
  `correlated-cache`, and `rich`, are accepted; the four accepted controls
  stay accepted; `range-payload`, `range-use` and `validate-targets-unit`
  stay refused, as they wait for Q139.
- The guard-reversed input is refused at INV-1 with the failing incoming
  edge named; the old compiler refuses it too, so it guards against a
  transport that drops a failing input rather than showing a change.
- The two conformance expectations this design named change from reject to
  accept, renamed `ent6-pos-join-one-arm-advances-accumulator` and
  `inv1-pos-sequential-guarded-steps`. The compiler test
  `a_failing_body_probe_is_reported_before_the_header_backedge` keeps its
  diagnostic-order observation with a guard under which the probe fails
  independently, and its old program is kept as a positive test.
- Five compiler tests that counted the intermediate joins of nested `if`s
  now see one join per canonical frontier, which is the nested/flat
  agreement this rule exists for; their dispositions and evidence are
  unchanged.
- The INV-1 diagnostic names the failing incoming edge. Reporting a
  component separately was not implemented; the specification text says
  the input's disposition is its first failing component's.
- The stage-3 interpreter generated as `loop { match }` with `continue`
  (`gen.py`) is accepted in 7.6 s, against a refusal after 12.0 s by
  026074111; the dispatch lowering splits it into 318 arm functions and it
  runs CoreMark with the final CRC wasmi and Silverfir-nano produce.
