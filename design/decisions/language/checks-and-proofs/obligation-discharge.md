# Obligation discharge

How each obligation is discharged: recursive components, local certificates, named constants, expected failures and allocation sizes.

### decision component-induction-hypotheses
While a call-graph component is checked, a call to one of its members receives every callee postcondition as an induction hypothesis, and every selected return is proved once.
- replaced: withholding every summary of a component while it is checked was on main; the owner changed it, because strong induction on the order in which calls return establishes partial correctness without a summary fixed point.
- replaced: limiting same-component hypotheses to generated type-invariant postconditions was on main; the owner changed it, because Snowghost's recursive halving count needs its written result bound after its recursive calls, and the induction holds for written clauses too.

### decision certificate-steps-share-context
Every step of a local certificate reads the same entering context and publishes nothing; only the checked outer invariant becomes a fact.
- rejected: an agent proposed cumulative steps that build on each other; the owner rejected it, because one flat weighted combination is checked without any search over step order.

### decision named-const-affine-atom
An integer-typed named const is an affine atom folded at formation to the one value it declares.
- rejected: an agent proposed leaving named consts out of the affine fragment; the owner rejected it, because a limit declared once must mean that number in the relations written about it, or a stale inline digit silently splits the code from the proof.

### decision failure-is-typed-outcome
An expected failure is a typed outcome or intended control flow, and an always-true relation is a verified contract or invariant.
- rejected: an agent proposed an impossible-case branch added to satisfy the checker; the owner rejected it, because a branch whose false edge is not intended behavior is a proof written as executable code.
- replaced: implicit runtime checks kept on every unproved obligation was on main; the owner changed it, because the trap surface was neither stated nor enumerable, and a caller could not tell from a signature when a callee would trap.
- rejected: an agent proposed that every fallible operation return a value; the owner rejected it, because each site where the checker falls short would force an error arm for a condition impossible in correct code, and a stuck writer could swallow a violated invariant.
- rejected: an agent proposed prove-or-handle everywhere as language law; the owner rejected it, because a fixed no-search checker leaves true but unprovable goals that would hide inside genuine fallibility.
- rejected: an agent proposed a total-access operation beside the ordinary index; the owner rejected it, because with discharge at the caller the value branch already is the total access.

### decision host-exhaustion-outside-proofs
Running out of a host resource follows the specification's external-availability boundary and creates neither a source fact nor a proof fallback.
- rejected: an agent proposed a hidden availability trap; the owner rejected it, because host quota and exhaustion are outside the source-outcome model.

### decision allocation-size-checked-at-runtime
An allocation's byte size is computed at run time with checked arithmetic, and a size that wraps or exceeds the target's maximum is heap exhaustion, so no count carries a static obligation.
- replaced: a static allocation-size obligation on every runtime count was on main; the owner changed it, because writers clamped counts to ceilings no program needed; Snowghost bounds 23 of its 36 counts by clamps, 12 of 14 unneeded (data: `research/investigations/layout-friction/DESIGN.md`).
- rejected: an agent proposed standing facts bounding every measured place's length by its element type's limit; the owner rejected it, because they bound only counts read from storage, so they remove three of the nine needed clamps (data: `research/investigations/layout-friction/DESIGN.md`).

### decision segments-filled-returns-cell
`box_segments_filled` returns its cell, summing its lengths and sizing its block at run time with the same checked arithmetic, so a size that wraps or cannot be allocated is heap exhaustion.
- replaced: `box_segments_filled` returning `None` past a size predicate of `2^62` bytes was on main; the owner changed it, because no supported heap can supply the sizes it refused, so every caller matched a refusal no program could act on.
- rejected: an agent proposed returning `None` when the selected target cannot allocate the size; the owner rejected it, because a program's result would then depend on the target it was built for.