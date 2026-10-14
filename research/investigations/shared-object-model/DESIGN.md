# One object model for shared state

> **Superseded direction (2026-10-13).** The owner redirected this
> investigation away from a proof-only handle identity: the root cause is the
> multi-object atomic statement, not missing identity. The current study is
> [ONE-OBJECT.md](ONE-OBJECT.md); this record is kept for its problem
> statement, five classes, case set and criteria, which
> [STUDY.md](STUDY.md) applies to all candidates.

Status: question and criteria written before any measurement or design work.
Board item `shared-object-model`; owner ruling on scope: option A of card
`shared-object-model-scope` (2026-10-14), "description plus identity, all five
classes together".

## The question

The checker records facts, separation and invalidation by *place*: a variable
and the fields and indices reached through it. Ordinary references keep that
sound, because two places that can be written at once are equal or provably
disjoint. A shared object is a runtime object reached through copyable
handles, and two handles may name the same object. The specification and the
checker cover this rule by rule with exceptions; more than 30 board items
concern it. They fall into five classes:

1. **Identity.** The checker cannot tell whether two handles name one object,
   and a program cannot prove they differ (several `clear_both(first, second)`
   calls in Firn's multi-database code; two keys of one table; capacities
   `n+1` and `n+2`).
2. **Release.** What an object does when its last handle goes is a list of
   type names in the specification and compiler; a missing name is usually a
   wrong acceptance (the #437 `musttail` case).
3. **Invariants.** Facts and type invariants do not follow a value into a
   frozen object, a table entry or a shard.
4. **Explicit-close values.** A value that must be closed explicitly (a write
   file handle) cannot live in a shared object, which must drop its state
   automatically.
5. **Effects.** A `pure` effect row does not see a change made through a
   handle, so a merge or reorder rule had to be narrowed to calls that do not
   wait.

**Proposal under test.** One model with two parts gives all five.
*Description*: each runtime type declares in one place what release does and
what its identity means; a missing declaration fails the build.
*Identity*: each shareable handle carries a proof-only identity (erased
before lowering); creation makes a fresh identity, copying and conversion keep
it; a program states with `invariant` that two identities differ. Release,
effect visibility, invariant transport and explicit-close ownership are then
derived from description and identity instead of listed per type.

The question is whether this single model, with automatic derivation that is
specification-fixed, deterministic and terminating and no solver, decides all
five classes without a per-type exception list. The atomic-cell proposal
(card `atomic-cells-into-model`) joins the scope only if the owner rules it
in; it is not assumed here.

## Comparison that can answer either way

Three candidates are compared on the same case set:

- **M** the proposed model (description plus identity).
- **P** the current patches (the specification and checker as they stand,
  with #437 merged).
- **S** the smaller scope of option B, identity and description only, with
  classes 3 to 5 handled by the current patches.

The case set is fixed before any candidate is worked through and holds:
the five examples on the board item, each with its expected verdict; every
Firn call site named in the open items of the item's plan (the 7
multi-database calls, RENAME, COPY, SMOVE, LMOVE); the 13 rules the decision
cell changed; and at least five programs the investigation's author did not
draw from the board, to catch a model fitted to the known cases. Candidate
verdicts are produced by reading the rule text, not by a prototype; a
prototype follows only for the surviving candidate.

Recorded per candidate and case: accepted or rejected as expected; whether
the verdict needed a type-name list or a rule-specific exception; the number
of specification rules changed; whether derivation stays within the fixed,
terminating, solver-free procedure.

## Criteria fixed in advance

**M answers the question (adopted as the design basis) if all of:**

1. The five board examples give the expected verdicts: `clear_both` accepted,
   and rejected once the second table is replaced by a copy of the first;
   `walk` rejected; `reader` accepted; `park` accepted with the last handle
   obliged to take the file back; `bump` treated as a mutation of shared
   state and not merged or reordered.
2. Every Firn call site in the case set is decided as the board item
   expects, with no runtime key comparison added to the program.
3. No exception list keyed by shared type name remains in the model's rules;
   a new shared type needs only its description.
4. Derivation of identity facts uses only the fixed, terminating rules and
   `use` steps checked as written; no acceptance depends on a timeout, search
   or work budget.
5. The programs not drawn from the board show no verdict the model gets wrong
   that candidate P gets right.

**M is rejected, or narrowed, if any of:**

- A case in the set needs a solver, search or a budget to decide, or needs an
  unbounded identity-equality derivation (reject the identity part).
- Class 5 cannot be expressed without a new effect system. Then split class 5
  out, keep M for classes 1 to 4, and report that the scope reduces to
  option B plus classes 3 and 4. The card gave this as its confidence limit.
- The number of specification rules changed is not smaller than the
  per-class patches would change (counted over the case set), so the
  unification buys nothing.
- Any case is accepted by M that P correctly rejects (a soundness loss).

## Earlier work to address

Prior work the investigation must answer rather than skip: #478
(descriptions and identity of runtime types), the handle distinctness study on
branch `claude/shared-handle-distinctness`, #484 (closed-value validity, for
class 3), #441 (per-case multi-target check, whose cost grows with targets and
which cannot accept a program correct only when the objects differ), #437
(release class, merged first), and the shared-state design that already put
keyed tables as fields with entries named in the statement header.

## What this record does not claim

No measurement or implementation exists yet. Verdicts from reading rule text
describe the specification as written, not a compiler. Any result will be
reported for the case set tested only.
