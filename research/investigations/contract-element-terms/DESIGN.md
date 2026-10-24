# Ordinary contracts over integer elements

Board item `lang-contract-element-term`. Design for admitting integer
elements, and integer fields inside elements, as ordinary [ENT-2] clause (b)
terms, so that `requires` and `ensures` can bound them.

## Observation

The float-text module (board item `float-text-stage1-own`) writes three
ordinary requirements of this form, and both of its whole-program tests stop
at the first one, `Source / FN-8 / InvalidRequires`:

```wf
struct DigitCell {
  digits: Array<u8, 1>;
}

fn first_minus_one(cell: &DigitCell) -> result: u8 reads(cell.digits) contract {
  requires 1_u8 <= cell^.digits[0_u64];
} {
  let digit = cell^.digits[0_u64];
  let decreased = digit - 1_u8;
  return decreased;
}
```

FN-8 refuses "a subscript outside a clause (b) place", and clause (b) required
the place's final step to select a readonly field. The final step here is
`[0_u64]`. The offset being a constant does not change that classification.
FN-9 separately excluded "subscript" from its relation datums, so relaxing
FN-8 alone would leave the term, copy, support and publication rules
incomplete.

## Why readonly-only was chosen, and what that ground establishes

The [readonly-field investigation](../readonly-field-terms/DESIGN.md#soundness-every-change-is-an-overlapping-write)
rests the soundness of a clause (b) term on one condition: every change of
the stored value is a write that overlaps the term's support, so the
ordinary [ENT-5] kill ends every fact about it. Its table already lists
assignment inside the declaring module (a readonly field is an ordinary field
there), element and ancestor replacement, projected callee writes, offset
reassignment, holder rebinding, consumption and scope exit. Readonly syntax was
never the soundness argument; it was chosen because facts over mutable
elements die at almost every element write not proved distinct, so they
promised little retained precision for an unmeasured cost
([alternatives](../readonly-field-terms/DESIGN.md#alternatives)). The float
text case needs the opposite: one boundary fact about one element, used
before any overlapping write.

## Options

- **A, every integer element under tracked or constant offsets (selected by
  the owner on the design card `float-contract-element-term`).** Clause (b)
  admits every subscripted place whose final selected type is one fragment
  integer. Offsets stay clause (a) or clause (c) terms. One term rule serves
  body facts, requirements and postconditions.
- **B, constant offsets only.** Admits the float text case, but constants and
  tracked variables have the same tracked support; no soundness result or
  measured cost separates them, so the split is arbitrary.
- **C, keep readonly-only and write such contracts as one-element range
  facts.** Leaves the natural ordinary contract refused and reshapes the
  writer's contract around the gap.

## Design (option A)

1. **Term.** [ENT-2] clause (b): a place rooted as clause (a) with at least
   one subscript and a final selected fragment integer, an element or an
   integer field inside an element. Each subscript still owes [OP-4] where
   the place is formed; a requirement forms its places at body entry under the
   earlier requirements only, so a clause never proves its own subscript.
2. **Support.** [ENT-5]: the selected integer storage, plus each offset's,
   holder's and Box binding's support, exactly as for a readonly field. A
   direct or ancestor write, an element write not proved distinct, a callee
   write that reaches it, a changed offset, a rebound holder, consumption and
   scope exit kill the fact; a write proved disjoint keeps it. A copied
   integer keeps its copied value.
3. **Postconditions.** [FN-9] admits these places as parameter datums. An
   offset that is a parameter, or a field of one, denotes its function-entry
   image whatever the selected place denotes, so the entry-image stability
   rule and the caller's substitution govern it; this generalizes the
   sentence v0.149 added for a bare parameter offset. The place itself
   denotes the exit state under a written reference row, as a measure member
   or fragment-integer place already does, and its subscripts are judged at
   each selected return by v0.149's ENT-2 rule, so an entry bound never
   proves an exit read after the collection shrinks.
4. **Stores.** [ENT-3.S5] gives a `set` to such a place the commit value a
   direct integer place receives, so a body store proves a postcondition over
   the element.
5. **Call results.** [ENT-3.S12] narrows the storage excluded from
   conditional transport to indexed enum values, so a scalar call result
   stored into an element publishes to the element.
6. **Unchanged boundaries.** Explicit `entry` datums stay limited to field and
   dereference projections [MSR-3]; a subscript after the former reaches only
   a measure, as `entry(rows)^[i].len` already did, and MSR-3 now says so;
   owned result-array elements stay outside [CALL-4]'s result data; a
   returned subscripted place still gives no term [FN-9], so a body binds the
   element before returning it; range facts keep their own judgment; direct
   affine images of elements are not added; erased clause reads join the
   contract footprint under [EFF-5] as every clause read does.
7. **Loop heads.** A continuing kill compares no two subscripts at one
   position by their offsets [ENT-5], so a body write of `a[1]` removes a fact
   over `a[2]` at the head even though the literals differ.
8. **Redundant conditions.** Item 4 makes `set a[k] = v;` decide a later
   `if a[k] != v`, and a copy `let x = a[k];` relates x to the element, so
   [OP-5] rejects such a test as redundant, exactly as it rejects the same
   test of an integer variable after `set x = v;`. Programs and tests that
   checked an element right after writing it were rewritten: the value is
   folded into the observed exit status where the run is observed, and the
   decided test is removed where only acceptance is observed.

Cost: more ordinary terms and overlap queries in programs that read mutable
elements. No timing establishes its size; facts over elements written in
loops mostly die at the first unproved-distinct write, as the readonly-field
investigation predicted.

## Relation to the reference branch

A first implementation lived on the float-text branch (Whitefoot#387,
commits 397fc691f to c116a4cda), written against a base before v0.149 and
v0.153. Those two amendments since merged on main cover part of what that
branch built: postcondition subscripts judged at each selected return with
entry-image offsets (v0.149), and field and dereference places as offsets
with interned proof paths (v0.153). This change keeps main's mechanisms and
carries over only the term rule, its support, the S5 and S12 forms and the
FN-9 datum admission. Its conformance cases are the reference branch's.

## Coexistence with sealed type invariants

Whitefoot#484 (board item `firn-req-invariant-through-fields`) makes a
struct's type invariant hold for every sealed value, element values
included, and records a debt when a write may break it. It is paused behind a
Lean proof of that rule and will be reworked; this change merges first. The
two meet in three places, which the rework must merge:

- **Instance facts over element fields become ordinary terms.** #484
  establishes an invariant's instance at an anchor through binder datums
  formed as [ENT-2] terms. Below a subscript, a binder datum such as
  `window[i].value` was a term only when the field is readonly; after this
  change every integer field below a subscript is one. The instance facts
  #484 establishes there therefore become L0 facts the ordinary flow carries,
  and their kills are this change's support rule. The rework must check that
  each write that records a debt for such an anchor is also a write that
  overlaps the element term's support, so a fact never outlives the debt that
  says it may be false.
- **Checker code.** Both changes touch the entailment flow's events, sources
  and postcondition handling and the requirement checker; the reference
  branch's element-term commits and #484 change 26 of the same compiler
  files. The rework rebases its debt tracking onto the element term's place
  support rather than the readonly-field test this change removes.
- **Specification text.** The sentences this change edits (ENT-2 clause (b)
  and its justification, ENT-3.S5's commit forms, ENT-3.S12's transport
  exclusion, ENT-5's clause (b) support, FN-8's clause-expression list,
  FN-9's datum list and offset denotation) are not among the lines #484's
  specification diff changes, so the text merges without conflict; its
  TYPE-11 rules that cite clause (b) terms read the wider set after it.
