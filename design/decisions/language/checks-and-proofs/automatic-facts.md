# Automatic facts

The facts the checker derives by itself from bindings, operations, calls, returns, joins and matches, with no written proof.

### decision measure-binding-is-copy-equality
A binding whose right-hand side reads a measure gets the ordinary equality every copy of a tracked value gets, and no rule of its own.
- replaced: a separate fact rule for binding a measure was on main; the owner changed it, because a measure is a read of a readonly field like any other term the copy rule relates, so a second rule would be a second source for one fact.

### decision one-operation-fact-table
Every integer operation row but a conversion gives the value it binds the interval one fixed table computes from its operands' closed intervals, with order and offset relations to its operands.
- replaced: operation rows added one proof pattern at a time was on main; the owner changed it, because they forced proof-only masks, guards and checked conversions at all 21 natural integer idioms of the sweep (data: `research/investigations/automatic-operation-facts/DESIGN.md`).
- rejected: an agent proposed intervals read from operand types and literals alone; the owner rejected it, because a bounded operation feeding another loses its bound, which 7 of the 21 sweep idioms need (data: `research/investigations/automatic-operation-facts/DESIGN.md`).
- rejected: an agent proposed closed intervals without relations to the operands; the owner rejected it, because five sweep idioms bound a result by another term, such as a clamp to a runtime length (data: `research/investigations/automatic-operation-facts/DESIGN.md`).
- rejected: an agent proposed the difference interval of every result against every operand; the owner rejected it, because no idiom needs a relation beyond order and offset, and each further relation adds closure work.
- rejected: an agent proposed exact affine images for wrap forms that cannot wrap, saturating forms that cannot saturate and shifts that lose no bits; the owner rejected it, because the exact row already states that identity, and only it says that no overflow occurs.

### decision range-endpoints-through-contracts
A range endpoint reaches the caller only through the callee's ordinary requires and ensures, for Whitefoot bodies and linked definitions alike, by the same contract formation and result routes.
- replaced: a system-range automatic fact was on main; the owner changed it, because a family named by operation would be a second proof producer unrelated to the written callable boundary.

### decision runtime-divisor-quotient-facts
An admitted unsigned exact division publishes its quotient's bound for runtime divisors as well as literals, and an exact product of that quotient and divisor is bounded by the captured dividend.
- replaced: quotient facts only for written literal divisors was on main; the owner changed it, because runtime block decomposition needs these consequences of its own arithmetic, and finite value-image matching checks them without nonlinear search.
- rejected: an agent proposed requiring callers to supply an already-proved partition, or runtime tests for relations that must hold; the owner rejected it, because these are elementary consequences of the actual arithmetic that a fixed match can check.

### decision quotient-bound-as-affine-target
A goal `q <= k` on a captured quotient, with k a constant, is rewritten to `a <= (k + 1) * d - 1` over the captured dividend and divisor and decided by the fixed affine derivation.
- rejected: an agent proposed product terms with a certificate step that cancels a factor proved positive; the owner rejected it, because the extra generality has no current user, and the minimal quotient witness would still need an extra written step.
- rejected: an agent proposed leaving a quotient's bound by its divisor unproved; the owner rejected it, because the float-text module then cannot store each quotient digit as a proved byte.

### decision one-power-atom-per-amount
An exact unsigned `ishl` of 1 by a nonconstant amount binds one power atom per body, keyed by the amount's image and type, and an exact unsigned right shift by that amount divides by it.
- rejected: an agent proposed a fresh atom for each shift; the owner rejected it, because a writer's mask `ishl(1, s) - 1` must name the very divisor the shift divides by, or a bound `remainder <= mask` cannot bound the quotient.

### decision quotient-bound-unsigned-upper-only
The quotient-bound step and the power atom cover only the upper bound of an unsigned exact division and an unsigned exact right shift.
- rejected: an agent proposed capturing every division and shift form; the owner rejected it, because truncation breaks `d * q <= a` for a negative dividend, a wrapping shift masks its amount, and no current program needs a signed right shift or any of these forms.

### decision measure-observation-keeps-image
A successful measure observation keeps the measure's current affine image through its integer result and its copies.
- rejected: an agent proposed a separate equality proof for every copied measure; the owner rejected it, because a merge's two length reads must still denote the two lengths its contract constrains, and fresh unrelated atoms lose that identity before the checker can use it.

### decision field-places-have-images
A tracked integer field place, one binding root followed by field selections, has one current affine image as a measure has, replaced by a `set` to it and removed by every event that kills its facts.
- rejected: an agent proposed giving only a copy read of the field the entry atom, with no image for the stored field; the owner rejected it, because the store-side postcondition and type invariant stay unprovable, so the front-shift witness stays refused.
- rejected: an agent proposed leaving field reads through a reference with fresh atoms; the owner rejected it, because the float module cannot then read, add to and store back a bounded field naturally.
- rejected: an agent proposed images only for fields reached through a `&T` reference parameter; the owner rejected it, because other roots' fields carry their facts through the same kills, so `std::float_text`'s subtraction from a call result's field was refused for no soundness reason.

### decision entry-requirements-give-affine-premises
At body entry, an established requirement ordering with an admitted affine form but no entry projection supplies that affine inequality, through the existing signed decomposition.
- rejected: an agent proposed keeping composite requirements opaque; the owner rejected it, because a recursive merge must use its proved input-length sum to bound its output, and dropping the admitted contract forces a worse algorithm or a redundant runtime guard.
- rejected: an agent proposed general nonlinear or path search, or duplicating L0 bounds to widen the premise combinations; the owner rejected it, because the requirement is already an admitted affine contract that the existing decomposition supplies.

### decision per-variant-field-evidence
A local own enum carries a separate conditional context for each variant field that supplies contract data or struct type invariants, through binding, copying, moving, replacement and value delivery.
- replaced: success-payload evidence only for Result and Option was on main; the owner changed it, because Halo's array-slot and node-slot bounds must reach their own match binders through a named lookup result, and the enum type-invariant boundary needs the same contexts.
- replaced: evidence only over integer payloads was on main; the owner changed it, because the struct-payload probes show the same loss of facts when a payload is named or forwarded.
- rejected: an agent proposed extending only a direct call inside `propagate`; the owner rejected it, because naming, copying, value delivery and return forwarding would still lose the verified relation and need separate source forms.
- rejected: an agent proposed an implicit success route for an arbitrary enum; the owner rejected it, because a program-declared enum has no distinguished success or failure variant, so written clauses name their variant and field.

### decision give-delivers-integer-equality
A value initializer's `give d;` edge delivers the receiver's equality to d, to a literal's or named constant's value, or to a fresh carrier bound as a `let` of an integer expression would be.
- replaced: delivering only the relations that already mention d, with no image for a literal, was on main; the owner changed it, because `give length;` lost the bound that `let bound = length; give bound;` delivers, so writers rewrote value initializers as duplicated returning branches.
- rejected: an agent proposed delivering the receiver equality by closure without the substitution; the owner rejected it, because the closure has no rule that carries a disequality through an equality, so relations delivered today would be lost.

### decision give-binds-like-let
An own `give d;` of any other type binds the receiver exactly as `let x = d;` binds, carrying each relation over d's fields, nested fields and measures to the receiver's.
- replaced: carrying nothing through a value initializer of another type was on main; the owner changed it, because a struct chosen by `give move found;` in one branch and by a call in the other lost its type invariant, so the next call requiring it was refused.
- rejected: an agent proposed delivering only a struct's type invariants through a value initializer; the owner rejected it, because it is a path of its own for one source of facts, and a written postcondition over a field would still be lost at the join.
- rejected: an agent proposed calling the producer again, or repeating the consumer, in each branch; the owner rejected it, because it routes around the language gap with a rewritten program instead of closing it.

### decision return-reads-computed-value
A return of a computed integer expression, an integer `cvt` or an operation-table row gives a fresh returned value bound as a `let` of that expression binds; a returned call gives no datum.
- replaced: reading a return's result datum only from a place, a construction's field operand, a literal or a forwarded payload was on main; the owner changed it, because returning `value + 1_u64` was refused although `let r = value + 1_u64; return r;` proved it.
- rejected: an agent proposed admitting only a returned expression with an affine image; the owner rejected it, because a remainder, a minimum or a mask carries its bound only through the operation-fact rows, so `return value % 10_u64;` would still prove less than its `let` form.
- rejected: an agent proposed also publishing a returned call's verified relations on the returned value; the owner rejected it, because it adds a destination to the call publication rule and induction hypotheses at a return, which changes the call boundary.

### decision weakest-bound-outcome-joins
Each outcome's conditional numeric facts use the existing weakest-bound join and ordinary support kills, and loop heads remove facts whose support a continuing backedge changes.
- rejected: an agent proposed exact-template intersection, or identical written bounds required at a conditional-value join; the owner rejected it, because the existing difference-bound join already keeps the weakest common bound without a new inference family.
- rejected: an agent proposed branch-history enumeration, or a work budget that selects acceptance; the owner rejected it, because the finite rules keep common weaker consequences without correlating independent guards or equating different iterations.

### decision offset-disequalities-in-l0
L0 stores an offset disequality between two terms and tightens a bound at its excluded constant in either direction, with the same support, snapshot and join rules as a zero-offset one.
- replaced: keeping constant disequalities outside L0 was on main; the owner changed it, because the false edge of an equality sentinel test then cannot tighten the loop's upper bound, which its increment needs.
- rejected: an agent proposed a special case at branch entry for a constant comparison; the owner rejected it, because it misses a bound established after the test and splits one closure rule by the event that caused it.

### decision finite-join-disequality-candidates
Ordinary and delivery joins use ENT-5's finite disequality candidates, while a match's explicit exclusion set follows its own identical-fact intersection.
- rejected: an agent proposed keeping every common derivable nonzero exclusion at a join; the owner rejected it, because the finite candidates keep the existing consequences without listing every member of a match complement or inferring exclusions across wide gaps.

### decision value-arm-expression-equality
Each integer value arm starts from the same state after the scrutinee and establishes the selected equality over the scrutinee's admitted value expression.
- rejected: an agent proposed a fact rule only for tracked bindings; the owner rejected it, because a match on a computed value must keep the expression identity an equality guard keeps, and naming the computation would become a prerequisite for its proof.

### decision else-arm-exclusion-set
The remaining arm of an integer match establishes one exclusion fact pairing the scrutinee expression with the sorted set of arm values, which queries consult by membership.
- rejected: an agent proposed the accumulated state of an equality-test chain; the owner rejected it, because a long dispatch would hold one stored disequality or earlier-arm prefix per value, so its proof state grows with the square of the arms.
- rejected: an agent proposed using only the complement's interval hull, or no facts for the complement; the owner rejected it, because a long dispatch needs the exact complement.

### decision exclusion-fact-lifetime
An exclusion fact uses the scrutinee's read support, survives a join only as the same fact on every input that is not contradictory, and is never expanded into its members.
- rejected: an agent proposed permanent exclusions on mutable spellings; the owner rejected it, because a mutation must not retarget an old selection.
- rejected: an agent proposed unioning exclusion facts from different branches, or expanding the set at closure, scope exit or `give`; the owner rejected it, because a canonical existing fact needs no new set-inference search.

### decision invariant-bounds-enter-l0
A proved header or local invariant also establishes its exact two-term difference bounds as ordinary L0 facts, with ordinary support, kills, joins and snapshots.
- rejected: an agent proposed a composition rule local to one query, or projecting general affine premises into L0; the owner rejected it, because an equality-sentinel loop needs its header bound and the false guard's disequality in one closure to prove its increment.

### decision return-relation-scalar-addend
A normal-return relation may add one own integer parameter, with coefficient one, to one side's datum and constant.
- rejected: an agent proposed a fact of its own for the scan operation; the owner rejected it, because an ordinary contract lets callers prove both subtractions of reservation accounting, and user functions can use it too.
- rejected: an agent proposed that the runtime keep the reservation account; the owner rejected it, because it solves only this one interface and reverses the shape just chosen.
- rejected: an agent proposed a runtime check in Firn; the owner rejected it, because it routes around a language gap with a runtime check.

### decision growth-keeps-filled-facts
Growing a boxed Slots or Paged window keeps facts supported by its length and filled elements, while capacity facts and invalidated references die and the whole-cell write row stays.
- replaced: discarding unchanged element facts at growth was on main; the owner changed it, because growth changes where the storage is without changing the length or the filled elements.
- rejected: an agent proposed per-element equality postconditions for growth; the owner rejected it, because element types that cannot be compared and enum payload fields still need their facts kept.
- rejected: an agent proposed an effect row for growth that writes only the capacity; the owner rejected it, because parallel element reads could then overlap the relocation that frees their block.
- rejected: an agent proposed proving filled-element facts again in a loop after growth; the owner rejected it, because it hides the language gap and adds writer work for values that did not change.