# Call-site check

How a call compares the effects its arguments supply, so that signatures and caller facts decide interference.

### decision call-compares-substituted-rows
A call substitutes actual paths into the callee's row and refuses a compared pair with a write unless the shared path judgment proves the paths disjoint; read pairs are allowed.
- replaced: Comparing every pair of one call's entries was on main; the owner changed it, because one argument's two entries that overlap at every position can never be proved disjoint, so every call of the precise row was refused, forcing `writes(p)`.
- rejected: an agent proposed refusing at the declaration a row with two overlapping entries on one parameter, one a write; the owner rejected it, because the body would still declare `writes(p)` and its callers lose every fact about `p`, which the owner's performance-under-safety ruling refused.
- rejected: an agent proposed exempting every pair one argument supplies, pairs that depend on position values included; the owner rejected it, because a pair such as `reads(v[i]), writes(v[j])` is callable wherever the call proves its positions distinct, so the exemption removes no dead end.
- rejected: an agent proposed alias analysis of the callee's body; the owner rejected it, because signatures and caller facts then decide interference without reading the body.
- rejected: an agent proposed recovering unproved separation with a runtime check; the owner rejected it, because signatures and caller facts then decide interference without reading the body.

### decision by-value-arguments-join-the-comparison
A by-value argument adds the consumption of its place for a `move`, or a read of its place for a copy, to the call's pairwise comparison.
- rejected: an agent proposed leaving by-value arguments out of the comparison, since nothing about them is in the row; the owner rejected it, because moving out of a place that a second argument also reaches is the same interference as two writes.

### decision contract-reads-join-the-comparison
Each place a callee's contract reads through a reference parameter joins the call's comparison as a read by that argument, while staying erased proof syntax outside the runtime row.
- rejected: an agent proposed dropping the assumption that reference parameters are separate roots and killing facts on every parameter that may overlap; the owner rejected it, because that weakens every helper with two reference parameters of one type, even when callers pass separate objects.