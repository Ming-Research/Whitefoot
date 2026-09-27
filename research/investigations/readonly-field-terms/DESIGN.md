# Readonly fields below subscripts as terms

## Question and scope

Specification v0.69 made a subscripted place an [ENT-2] term only through the
prelude's measures: clause (b) named "a place whose final step selects a
readonly field", but then required the prefix to be a measured type having
that measure, and the endpoint paragraph confined the subscript admission to
measure places. A writer's own readonly field reached through a subscript,
such as `deref(nodes)[i].count` in an index-based tree, was therefore no term:
it could not bound a counted loop, appear in a `requires`, or be copied with a
`let` equal to its source. Specification v0.70, which added modules, left these
rules unchanged but confined `readonly` to public fields and made such a field
ordinary inside its declaring module [TYPE-2, MOD-6]; this change lands as
v0.71.

This investigation decides, from the kill semantics and the index-based tree
and layout use cases, whether that field should be a term, and bounds what the
change reaches. The owner's 2026-09-20 ruling ("a place with subscripts is a
term when its last step is a readonly field") is the history of the question,
not its ground.

## Soundness: every change is an overlapping write

A term over storage is sound when every event that can change the value the
term denotes kills every fact supported by it. [ENT-5] kills a fact when a
write overlaps its support under [OWN-7], or when a support binding is
consumed or leaves scope. For a place `P[i].f` whose last step selects a
readonly field `f`, [TYPE-2] makes `f` never a `set` target and never an
argument at a written parameter outside its declaring module; inside that
module it is an ordinary field. The complete set of events that change it:

| event | written place | why it overlaps `P[i].f` |
| --- | --- | --- |
| `set P[j] = v`, `replace`, `update` of an element | `P[j]` | prefix of the term's place unless `j != i` is proved |
| `set P = v`, `set owner = v` | a prefix | prefix |
| `swap(first: &P[a], second: &P[b])` | `P[a]`, `P[b]` | prefix unless both offsets are proved distinct from `i` |
| `remove_at`, `insert_at` on a window | `P.filled` | [WIN-2]: every live index overlaps `filled` |
| `place_back`, `append`, `split_off` into a window | `P.next`, `P.free` | [WIN-2]: separate from `P[i]` only where `i < P.len` is proved at the event; an index at the length is the slot written |
| `place_front`, `take_front` | `P` | prefix |
| `take_back` | `P.last` | overlaps `P[i]` unless `i != P.len - 1` is proved |
| call whose row writes `P` or a prefix | projected actual | prefix; a range actual reaches its element storage [CALL-3] |
| call whose row writes `P[i].f` itself, such as the declaring module's `writes(rows[i].count)` | projected actual | the term's own place [EFF-2] |
| `set P[i].f = v` inside the declaring module | `P[i].f` | the term's own place |
| write to the offset binding `i` | `i` | the offset's support is part of the term's support |
| rebinding a reference the place reads through | the holder | the holder is a support member |
| consume or scope exit of the root or offset | — | kills (c) and (d) |

On 2026-09-26 kernel-spec v0.74 fixed every index and every range as
overlapping `P.last` whatever its value, because one `writes(P.last)` entry
covers every `take_back` a callee makes; the `take_back` row above states the
v0.73 rule, which no event answered. [The last filled
slot](../one-argument-rows/DESIGN.md#the-last-filled-slot) records why.

A write to a sibling field `P[i].g` or to an element at a proved-distinct
offset overlaps none of these and correctly keeps the fact. Measures over
subscripted places already relied on exactly this table, so the extension adds
no kill rule to the language. That every event in the table is an [ENT-5]
overlap kill is a statement about the specification; the compiler did not
implement two rows of it until this change and the repair below, which are
the kill-coverage results of this investigation. The specification now states the ground once, in [ENT-2]'s
clause (b) paragraph, and [ENT-5]'s support sentence covers every clause (b)
term: the storage of its readonly field and the support of every offset in its
place.

Verification in the compiler found one kill the general term needed and the
measure path already had: the tracked-place arm of `event_kills_term` checked
only resolved-place overlap and consumption, not the support of offsets in
the term's own spelled path. Clause (a) places have no offsets, so this was
not a defect before; a subscripted place term without it would have survived
`set i = 2` and discharged a subscript at the new element (reproduced before
the fix). The arm now also applies `event_kills_offset_support`.

The independent review of this change (about sixty probes) found a second
compiler gap, which existing measure terms shared: a call whose `&[T]`
parameter is written killed nothing when its actual was a range formed at the
argument, such as `&rows[1_u64..3_u64]` or `&deref(view)[1_u64..3_u64]`,
because the flow's argument-referent walk returned no place for a range
formation. The language rule was unaffected (the projected write overlaps the
element storage under [EFF-2] and [CALL-3]); the compiler did not implement it,
and a measure term over such an actual let an accepted program read out of
bounds on main. The repair (branch `fix/range-argument-kill`, merged here)
gives that walk, the holder chain, the permission argument places and the
PAR-2 range recording the formed range's place. Its `call3-*-inline-*` cases
cover measure terms; `ent5-neg-readonly-field-inline-range-callee-write` and
`ent5-neg-readonly-field-inline-reslice-callee-write` cover the new terms, and
every review probe that read out of bounds is now rejected.

The same review found that a requirement could name an element that does not
exist yet and that its fact survived the `place_back` that creates it (review
probe p50): nothing judged a clause subscript, and [WIN-2] separated every
index from `r.next` without asking whether it was below the length at the
event. Both halves are closed, and either alone rejects p50.

- A requirement forms its places at body entry, in the state holding the
  requirements before it, and a definition's places are formed in the first
  requirement that expands it, so each subscript owes [OP-4] there [ENT-2].
  Forming the place in each caller's instantiated goal instead was refused:
  the callee establishes the requirement's fact in its own entry state, and a
  place is a term only where its subscripts are discharged, so the place must
  be formed in that state. A clause's own conjuncts come after nothing, so a
  requirement cannot bound its own subscript.
- [WIN-2] separates `r[i]` from `r.next`, `r.free` and `r.last` only where
  `i < r.len` is proved in the state the two places are compared in. A
  subscript formed there and a place a valid reference names are live by
  construction; an [ENT-5] event proves each fact's index in its entry state,
  a loop header in its preheader state because the length falls only at an
  event that kills every such fact anyway; and an effect row's index position
  beside a part is submitted to the call's entry state as index distinctness
  is ([EFF-5]). Since v0.74 (2026-09-26) `r.last` is separated from no
  index at all, as the note under the event table says.

With the judgment alone p50 is refused at its clause; with the liveness
condition alone the fact dies at `place_back` and the loop's cell read is
refused. The 107 review probes keep their verdicts except p50. The cases
`ent2-neg-requirement-names-append-slot-element`,
`ent2-neg-requirement-readonly-field-unbounded-subscript`,
`ent2-neg-requirement-measure-unbounded-subscript` and
`ent2-neg-definition-expanded-before-bound` reject at the clause, their
`ent2-pos-requirement-*-bounded-by-earlier-requirement` pairs run,
`win2-pos-live-index-survives-appends` keeps a live fact across two appends,
and `eff5-pos-row-index-live-beside-append` admits a row position the call
proves live. An `ensures` place's formation point is still unstated and
unjudged; `docs/todo.md` records it with the FN-9 extension.

Two paths the judgment first missed were closed after the completion review.
An [FN-4] query runs over a body-less function whose premises are a formal's
requirements, and nothing had formed their places; a formal's block has the
same formation rules [FN-8], so each formal requirement now forms its places
once, when the formal is formed, in the state holding the formal's earlier
requirements, and every query premise is a formed place.
`fn8-neg-formal-requirement-place-unbounded-subscript` rejects at the formal's
clause and `fn8-pos-formal-requirement-place-bounded-by-earlier-requirement`
runs. The judgment also read an offset spelled through a definition, as in
`deref(rows)[row]` after `define row = i;`, as the definition's own binding,
which no fact names, and refused a place main accepted; the place now takes
the offset the definition expands to [FN-8], so
`ent2-pos-requirement-place-offset-through-definition` and
`ent2-pos-definition-place-offset-through-definition-chain` run and
`ent2-neg-requirement-place-offset-through-definition-unbounded` rejects.

Evidence: the conformance cases
`ent5-neg-readonly-field-offset-reassigned-in-loop`,
`ent5-neg-readonly-field-element-replaced`,
`ent5-neg-readonly-field-element-swapped`,
`ent5-neg-readonly-field-window-shift` and
`ent5-neg-readonly-field-callee-writes-base` each reject a program whose
runtime would read out of bounds; `ent5-pos-readonly-field-sibling-and-distinct-writes`
keeps the fact across non-overlapping writes. Reference-holder rebinding and
unproved-distinct element writes were checked with the same shape during the
change. Under [MOD-6] every readonly case is a module-form directory that
declares the struct in a module of its own; `ent5-neg-readonly-field-assigned-in-declaring-module`
and `ent5-neg-readonly-field-row-writes-field` reject the two rows the
declaring module adds.

## Alternatives

| alternative | sound | reaches the use cases | term growth |
| --- | --- | --- | --- |
| A. No change: bind a `let` copy first | yes | local loops only | none |
| B. Readonly fields below subscripts (selected) | yes, no new kill rule | loops, requirements, copies | only readonly fields a program reads below subscripts |
| C. Every integer field below subscripts | yes, same kills | same, plus mutable fields | every such read, killed by every unproved-distinct element write |
| D. Quantified per-element facts | needs instantiation at every read and re-proof at every write | yes | unbounded family |

A fails the requirement use case: a copy is detached from the storage it came
from, so a callee cannot state a relation between one element's field and
another storage, and a caller's fact about an element never reaches the
callee. B covers that case with no new mechanism. C differs from B only for
fields a program assigns in place; in a loop that updates `nodes[j].x`, every
`nodes[i].x` fact dies unless `i != j` is proved, so those terms rarely survive
to be useful while every element write pays the overlap test against them.
Its closure-size cost is unmeasured; it stays the recorded follow-up in
`docs/todo.md`. D is refused by the checks-and-proofs node's existing
decision on quantified storage facts.

Restricting B to the prelude's measures is the v0.69 state. A writer's
readonly field changes by exactly the events of the table above, so the
restriction was a special case with no soundness difference.

## Boundary of this change

The term joins the L0 fragment wherever [ENT-2] terms are consumed: comparison
origins and branch facts [ENT-3.S1], requirements and their caller discharge
[FN-8, ENT-3.S4], `let` copy equalities [ENT-3.S5] and the S7 rows that read a
term, and counted endpoints [ENT-3.S11].

It deliberately does not reach three surfaces, each for its own reason:

- [FN-9] relation datums. Publishing a relation over a formal-subscripted place
  needs the formal offset substituted on both the body and the caller side. The
  existing measure path reads such an offset as an unknown capture (a relation
  over `deref(rows)[i].len` renders as `[?]`), which is only unusable today
  because nothing is provable about that term; admitting readonly fields there
  without fixing both substitutions would make it unsound. FN-9's datum list is
  unchanged and `fn9-neg-readonly-field-below-subscript-relation` pins it.
- [INV-1] affine atoms and [ENT-6] automatic images. Invariant atoms admit no
  field selection at all except measure members, and ENT-6's candidate set is
  specified over measure terms. A readonly field joins as clause (a) field
  places do: L0 only. A relation such as `first + count <= kids.len` therefore
  still needs own parameters or a measure.
- Tracked-place offsets with projections. [ENT-2] admits `nodes[n.parent].count`,
  but the compiler captures only literal, const and bare-binding offsets. A read
  of such a place, in a body or a clause, is reported as an unsupported compiler
  capability, never as a source verdict, as a measure read over one already was;
  reading it as no term would let the missing fact surface later as a source
  rejection.

`docs/todo.md` records each with its validation criterion.

## Offsets

v0.69 listed an offset as "a written integer literal, a live `own`
fragment-integer place, or an in-scope const generic" and called every other
offset "a place this version does not represent, reported as the compiler
capability it is". That made a compiler limit a clause of the language and
left open whether an element-read offset was a language question at all. The
restriction is kept exactly where it is principled: an offset must itself be
something whose changes the fact system tracks, a clause (a) term or a
constant, because a term dies only through its support and an offset's support
is part of it. A write to `idx[0]` changes which element `table[idx[0]]`
selects, and no term records `idx[0]` as support. So v0.71 states: every offset
of a clause (b) place is a clause (a) or clause (c) term; a place with any other
offset is no term; an implementation that cannot represent an admitted offset
reports the place as unsupported [DIAG-1].

Observable before and after:

| program | v0.69 compiler | v0.71 |
| --- | --- | --- |
| `requires k < deref(table)[deref(idx)[0_u64]].len` | unsupported | FN-8 rejection (`fn8-neg-element-offset-in-clause-place`) |
| endpoint `table[idx[0_u64]].len` | OP-4 at the offset | ENT-2 rejection |
| endpoint `entries[slots[0_u64]].width` | ENT-2 rejection | ENT-2 rejection (`ent2-neg-element-offset-readonly-field-endpoint`) |
| `deref(nodes)[cursor.at].count` | body read accepted as no term; in a clause, FN-8 rejection | unsupported in both (unit test) |

A clause (b) term as an offset, as in `nodes[nodes[i].parent].count`, is tracked
as well and would be a principled recursive extension; it stays out to keep the
rule non-recursive and is a recorded follow-up.

## Clause exclusivity

"A term is exactly one of" needs the clauses to be disjoint. v0.69's clause (b)
began "a place whose final step selects a readonly field", which every
unsubscripted readonly field and every unsubscripted measure such as `run.len`
also is under clause (a). v0.71 separates them by subscripts alone: clause (a)
has none, clause (b) has at least one. An unsubscripted measure and a range's
`len` are clause (a) tracked places, whose support (the resolved place, that is
the descriptor word) is what [MSR-2] already gives a measure, so no fact
changes.

## Checking cost

New terms arise only where a program reads a readonly integer field below a
subscript; each distinct spelling is one term, exactly as a measure over the
same place is, and dies with its support. Before this change only a handful of
conformance cases and no maintained program or library performed such a read,
so existing checks change cost negligibly. No timing experiment was run:
nothing here selects between alternatives by cost, and C's cost question is
left open with its own criterion.
