# Range type invariants

Status: direction selected by the owner on 2026-10-08 (board card on how an
invariant over stored data reaches its callers, option A); design in progress,
not implemented. Baseline: branch `claude/proof-facts`, which already admits
range terms below an element ([range field terms](../range-field-terms/DESIGN.md))
and lets range facts serve ordinary obligations
([ordinary range obligations](../ordinary-range-obligations/DESIGN.md)).

## Question

A range fact crosses one call as a requirement or a postcondition
([range facts](../../../design/language/checks-and-proofs/range-facts.md)), and
live storage carries none across passes. A structure whose writers all
maintain a pointwise relation between two of its stores therefore cannot
state it where it is used: Snowghost's sibling move requires

```text
forall inv(k in first..order^.len) when order^[k].Open.block < targets^.len:
  targets^[order^[k].Open.block].entry_slot == k;
```

and the experiment release `wf-exp-7bf6aadcb051` accepts that callee but
refuses every caller (Snowghost-wf `research/m2-frag-a-proof` ad6108a,
`research/investigations/m2-edit-cost/inverse-proof/call-site.md`, hosted run
37823187962). The renderer maintains the relation by construction in about ten
writers; nothing checks that, and nothing carries it to the call. Establishing
it before the call costs a pass over the owner's slot span per edit, up to
tens of thousands of entries for a flat owner.

### What the gap costs

The "--par task grain" line measured Snowghost's incremental edit pair
(Whitefoot#287, `research/investigations/edit-parallelism/DESIGN.md`;
ECMAScript page, 3ec4bb491, Snowghost-wf run 37840469770, sequential profile
less a setup control): about 80% of each edit is the suffix translation
(`translate_reference_payload` 32%, `reference_owner_cursor` 26%,
`translate_reference_owner_suffix` 16%, `slot_read` 6%). Its subtrees write
disjoint targets, but `--par` admits none of it, because both recursive calls
write the context and their disjointness needs the stored invariant "each
block appears once in its owner's sequence". If that work ran in parallel the
Amdahl bound is 2.5 times at 4 workers; today the edit runs at 1.08 to 1.10
times sequential at 4 workers on the 14900K. A second, independent blocker
there, the parallel planner's refusal of an `if` or `match` group member, is
that line's to bring to the owner. These are bounds and profiles, not a
measured gain from this design.

## Selected direction

A struct may declare a range type invariant: a [TYPE-11] type invariant whose
body is a range clause [RANGE-1] over its binder's data. It is owed and assumed
at exactly the sites TYPE-11 fixes for an affine type invariant, as the range
form of each:

| TYPE-11 site | Range type invariant there |
|---|---|
| construction of the struct | owed, judged by the range judgment at the construction [RANGE-3] |
| parameter of the struct type or a reference to it | a range requirement of the function: owed at each call, active at entry [RANGE-2] |
| written reference parameter's exit state; result ordinal of the struct type | an unrouted range postcondition, owed at every exit that selects it and taken by the caller |
| `shared_new` argument; atomic statement on `Shared` of the struct | owed at the call; active at the block's entry, owed on each leaving edge |

TYPE-11 already requires every field of such a struct to be private or
`public readonly` [MOD-6], so only the declaring module writes the stores;
every writer is a function of that module whose written reference parameter
owes the invariant at exit. The range judgment then checks preservation: a
writer that touches neither store keeps the fact through precise field
support; a writer that moves entries proves it with a range invariant on its
loop.

This replaces the decision that live storage carries no range fact across
passes: the facts are now kept by the writers the checker already sees, not
by every mutation of an unrestricted document.

## What Snowghost's structure adds

From Snowghost's source (research/m2-frag-a 34077ee, `renderer/layout/`):

1. **Two bound variables over a store below an element.** Each block owner
   keeps its own order store, `Context.blocks[b].entries.payloads`, while the
   targets are the context-wide pools `Context.blocks`, `.paragraphs` and
   `.children`. The invariant ranges over `b` and over `k` in
   `blocks^[b].entries.payloads`, an indexed collection below an element,
   which [RANGE-1] refuses today ("a second index below an element"). The
   extension admits `p[i] ... .q[j]`: an element projection whose owned
   steps reach another indexable collection, subscripted again. Read
   definedness requires both selections to exist; a write to `p[i].q[j]`
   defines a new version of `p` at tuple `(i, j)` through the same
   projection rules.
2. **The owner in the relation.** The full relation is
   `(t.owner, t.entry_slot) == (b, k)` for the target `t` a live payload names,
   two conclusions of one clause.
3. **Live slots only.** A removed or retired entry keeps its payload cell,
   which names a target whose `entry_slot` is reset to `no_index`. The clause
   guards on liveness, a variant or a field of the cell, as `when` guards
   already allow.
4. **Paged storage.** The order store is a hand-written two-level
   `SlotPages`; with `Paged<T>` (Whitefoot #263) it becomes one indexed place.
   The first cases use flat arrays; Snowghost adopts the invariant after
   `Paged<T>` is on main.

## Writers and their preservation proofs

Snowghost's writers (sg's inventory, source reading only):

- single element: `pending_append`, `insert_before`, `append_flow`'s
  `entry_slot` stores and `record_child_entry`, `keep_outputs`,
  `commit_splice_sequence` (which relies on `insert_before`'s slot equalling
  the precomputed `input.allocated`, an equality the proof must state);
- many elements: `finish_sequence` (pending to pages, loop invariant
  "pending index == target.entry_slot"), `relocate_payloads` (shifts target
  indices by a pool base), `relocate_splice`, `publish_splice_payloads`,
  `retire_splice_payloads`.

Each single-element writer proves the invariant at exit from the entry
instance and the one written tuple. Each many-element writer needs a range
invariant on its loop over the processed prefix, the same shape as
`range3-pos-*` cases on main. Whether every one of these closes under
[RANGE-3]'s fixed derivation is the open risk the card named.

### Appending writers need placed window calls

RANGE-2 forgot every location a call writes, and `place_back`'s row
promises only the length change, so a writer that appends could not show the
appended element satisfies the invariant: in
`requires xs^.len == 0_u64; ensures forall zero(k in 0_u64..xs^.len): xs^[k] == 0_u64;`
around `place_back(window: xs, value: 0_u64)`, the new element is unknown
(found by the implementing agent from the derivation, not a compiler run).
`place_back` is therefore a write the walk places: the placed value at the
old length, every other element unchanged, the length one more; `take_back`
keeps the elements below the new length. The other window operations, which
move elements (`insert_at`, `remove_at`, `append`, `split_off`,
`place_front`, `take_front`) or reallocate (`grow`), still forget the window;
a writer that needs one of them reopens this.

## Criterion before implementation

The direction is supported if, with an experiment compiler:

1. a flat-array model of `Context` with the per-owner left inverse as a range
   type invariant is accepted with the sibling move as the callee, its caller
   needing no written fact, and the move loop certified;
2. models of `insert_before`, `finish_sequence` and `relocate_payloads`
   preserve the invariant with at most one written range loop invariant each;
3. a writer that breaks the relation (a wrong `entry_slot`, a skipped target in
   a relocation) is refused at its exit with RANGE-3;
4. check time of the conformance suite and the natural-form v2h is unchanged
   (they declare no range type invariant), measured on the 14900K.

It is rejected if a single-element writer cannot be proved without a
run-time test, or if a many-element writer needs a derivation step outside
[RANGE-3].

## Specification changes

- [TYPE-11]: a `type_invariant` body may be a range clause over binder data;
  its sites as in the table above.
- [RANGE-1]: element projections may subscript an indexable collection
  reached below an element; a range clause may be a type invariant body.
- [RANGE-2], [RANGE-3]: a construction is an owed site; type invariants are
  sources and owed sites as the table states.
- [GRAM-2]: `type_invariant` admits a `range_clause` body.

The design tree's "Live storage carries no range fact across passes" decision
is replaced, with the reason above.
