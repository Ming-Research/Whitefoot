# Range helper frames

## Question and scope

After a call, the range judgment forgets every location the callee's row
writes through a reference argument and imports only the callee's declared
postconditions [RANGE-2]. A storage helper that grows or appends to a window
therefore keeps a caller's range facts only by stating, in its contract, that
the elements it found are still there. Specification v0.127 cannot state it:
a range term reads no entry image [RANGE-1], and an [FN-9] relation admits no
subscript. Snowghost-wf's witnesses (`research/investigations/m2-paged/`,
`reserve-helper-frame.wf`, `append-helper-frame.wf` and their `-boundary`
callers) are rejected by RANGE-1 at the entry read, and their callers lose an
inverse-table type invariant at the return and at the propagated error exit
(RANGE-3).

The owner chose direction B on the board card "how does an ordinary storage
helper say in its contract that old elements are unchanged?" (2026-10-11):
admit entry versions of reference parameters in range postconditions, and add
a proposition that an element is unchanged, separate from `==`. This note
fixes its spelling, meaning, proof at the callee, use at the caller and
refusals, before the specification is amended. Element relocation to other
positions (direction C) and effect-row classifications (D) stay out of scope.

## Why both parts

Entry versions alone cover a helper whose element type has an equality
[OP-16]: `sequence^.payloads.inner[k] == entry(sequence)^.payloads.inner[k]`.
They do not cover Snowghost's generic `push_item<T: drop>`, which every pool
append in its layout uses; its pools hold `Paragraph` and `Context` values,
which own boxes and have no equality. The design tree's value-equality node
refuses expanding `==` into integer projections inside generic range clauses,
because a proof would then state an equality that execution does not have.
What the caller needs is weaker than equality: that no write of the helper
reached the element, so every projection it already knew, such as
`entry_slot`, still has its value.

## Entry versions

In a range postcondition, a place rooted at `entry(p)`, where p is a
reference parameter whose row declares a write rooted at it [MSR-3], denotes
the storage p received as it stood at function entry, and every selection
below it is a range read of that version. A place rooted at `entry(q)` for any
other parameter keeps MSR-3's refusal; an `own` parameter's storage keeps
RANGE-1's refusal.

- Callee: the walk's entry state already holds one version per location;
  `entry(p)` reads name those versions.
- Caller: before applying the call's effects, the walk records the version
  each argument's storage holds; after the ordinary forgetting, the imported
  postcondition's `entry(p)` places denote those recorded versions, so the
  caller's facts on them meet the callee's exit relations in one problem.
- Exits: an unrouted range postcondition is selected by every explicit
  return and every propagated error exit [RANGE-3], so a frame also holds
  where the helper fails.

## The unchanged proposition

Spelling: a conclusion `P unchanged`, where P is an element projection rooted
at a reference parameter whose row writes it, for example

```
ensures forall preserved(k in 0_u64..entry(cell)^.inner.len): cell^.inner[k] unchanged;
```

It is one-sided: it relates P at exit to the same selection spelled from
`entry(p)`, with each subscript evaluated as written. A two-sided spelling
would admit relating different positions, which is relocation (direction C)
and would need its own ownership account. `unchanged` becomes a reserved
grammar word: existing binding names using it, including
`tests/programs/cancellation.wf` and embedded compiler fixtures, are renamed.

Meaning: the element storage P selects at exit is the version the same
selection held at entry. No write of the function reached it or any of its
owned descendants [OWN-7]. A relocation that keeps versions also keeps it, as
`grow` and `grow_paged` do once Whitefoot#352 lands, and so does `place_back`
for every element below the old length. It states nothing about the
element's type: it needs no equality and holds for any `T`. Like every range
clause, it claims nothing where any read the instance forms is undefined
[RANGE-1]: the implicit entry read is one of those reads. In particular, an
exit selection that did not exist at entry receives no frame claim or caller
definition. This follows RANGE-1's rule that an instance claims its conclusions
only where every read it forms is defined [RANGE-3], as the owner selected on
2026-10-11; it adds no separate definedness obligation.

Callee proof: the walk gives every element selection one whole-element read,
an integer-valued read with no meaning of its own, next to its field reads. A
write overlapping any projection of an element at the written tuple makes the
new version's whole-element read unknown there; at every other tuple it is
the old version's, by the existing write definition [RANGE-2]. `P unchanged`
is the equality of P's whole-element read at the exit version with its read
at the entry version, decided by [RANGE-3]'s derivation unchanged: no new
theory, no search and no new instantiation round. The judgment is
conservative: an element written and then restored, or taken and placed back,
is not proved unchanged.

Caller use: an instance of an imported `unchanged` conclusion at tuple t, with
its ranges and guards holding and its reads defined, defines each read of the
exit version at P's selection and t, with any projection, as the same
projection read of the recorded entry version. It joins step 2 as one more
definition, not as a new axiom. Instances form only at tuples that reads
already in the problem select, so a projection the caller never reads costs
nothing, and the 256-instance and 4096-atom ceilings still apply. At a
concrete instance of a generic callee, P's projections are those of the
instantiated element type, which is what lets `entry_slot` of a `Block` pass
through `push_item<T>`.

## Refusals

Each is a hard error, with a repair [DIAG-1]:

- `unchanged` outside a range postcondition's conclusions, such as in a
  guard, a requirement, a loop invariant, a type invariant or an ordinary
  relation: RANGE-1. Only a postcondition has both an entry and an exit
  state.
- `unchanged` on a place that is not an element projection rooted at a
  reference parameter whose row writes it: RANGE-1. A measure or a whole
  window changes under ordinary appends and is stated by ordinary relations,
  and a parameter without a write row changes nothing.
- A body that replaces the whole container, such as `set cell^ = ...` or
  `set p^.inner = ...`, writes every element. So does a call that forgets
  the storage, such as an unknown callee without its own frame, or a loop
  header that forgets it. In each case the whole-element read is unknown and
  the postcondition fails with RANGE-3 at the exit. A version the walk cannot
  trace back to entry is likewise unproved, never assumed.

## Relation to take_back

The RANGE-2 amendment selected on the card "should RANGE-2 define
`take_back`'s returned value as the image of the old last element?" makes
that returned value the old element's storage. A helper that takes an
element and hands it back still needs a two-sided relation, which is
relocation, and stays out of scope. Every element below the new length keeps
its version, so `window^[k] unchanged` for k below the new length is provable
after `take_back`, and below the old length after `place_back`.

## Acceptance and rejection

The proposal is accepted when:

- Snowghost's `append-helper-frame.wf`, rewritten with `unchanged`, compiles
  unmodified at the generic `push_item<T: drop>` and at `append_target`'s
  `Block` instance;
- `reserve-helper-frame.wf` compiles with the entry-version `==` frame, which
  needs Whitefoot#352;
- the `-boundary` callers keep their inverse invariant at the return and at
  the propagated error exit;
- a frame quantified through the exit length after an append is accepted:
  the new index has an undefined entry selection and its instance claims
  nothing, documenting the ordinary range-clause definedness rule;
- negative cases are refused: writing the element then claiming
  `unchanged`, replacing the window, `unchanged` in a requirement, and a
  caller relying on an element not covered by the frame's range or at or
  beyond the entry length.

It is rejected, and returns to the board card with the evidence, if carrying
whole-element versions through a generic instance needs work beyond the
fixed instantiation rounds. It is also rejected if a relocation the language
admits loses version identity in the implementation; `grow` without
Whitefoot#352 is such a relocation.
