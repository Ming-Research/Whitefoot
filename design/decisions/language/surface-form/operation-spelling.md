# Operation spelling

How operations are spelled: infix arithmetic and comparison, named calls, exact meaning of bare spellings, and the multiplicity of a `use` premise.

### decision arithmetic-and-comparison-are-infix
Arithmetic and integer comparison are spelled infix and every other table operation is a named call, the spelling fixed by the operation's grammar class.
- replaced: Spelling all computation as prefix calls with mode suffixes was on main; the owner changed it, because three-address form allows one operation per expression, so an infix spelling needs no precedence and costs no lookahead.

### decision bare-operations-are-exact
The bare infix and dotless named spellings are exact mathematical operations whose domain must be proved before lowering, and none of them traps.
- rejected: an agent proposed a bare operator that traps; the owner rejected it, because a bare spelling that means trap gives partial arithmetic a hidden failure edge, while the wrap, checked and saturating modes keep their own meanings.

### decision type-application-takes-double-colon
The six integer comparison symbols are integer-only rows like `+`, and type application at a call site carries the `::` delimiter.
- replaced: Spelling comparisons as named calls was on main; the owner changed it, because `::` removes the clash between `<` as comparison and `<` as type application, and comparison is the most frequent operation.
- rejected: an agent proposed strong four-token lookahead to keep `<` overloaded; the owner rejected it, because it commits a comparison with a nested right operand to the call arm and turns two-token diagnostic attribution into a four-token case analysis.

### decision use-multiplicity-is-n-times
A `use` cites one premise with its multiplicity written `N times` before it, a relation premise always in parentheses and a named premise never.
- replaced: The spelling `use 3 * p` was on main; the owner changed it, because `*` claimed a multiplication whose right operand is a relation, which two tokens of lookahead could not decide, and a whitespace rule had to carry a difference the parser cannot see.
