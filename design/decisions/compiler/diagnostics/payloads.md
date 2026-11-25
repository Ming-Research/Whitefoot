# Rejection payloads

What a rejection's payload says: how it spells the source constructs it names, the effect rows it suggests, its normalized relations and clauses, and the match sites it lists.

### decision payloads-spell-constructs-as-written
Every payload string that names a source construct spells it as the writer would write it, through the one checker renderer for that kind of construct.
- replaced: Printing the checker's identities, such as field and variant ordinals, or `Debug` values was on main; the owner changed it, because the agent reading the payload repairs the source it wrote and cannot map those back to it.
- rejected: an agent proposed inventing an expression for a captured value without its source text, or using a binding's current spelling after the binding was overwritten; the owner rejected it, because either can name a different value.

### decision unwritable-values-are-described-in-words
A value with no source spelling is written as a description in words between two marks no source text contains, never as `?`, in payload fields and repair sentences alike.
- replaced: Printing `?` for a captured value with no source spelling, such as an index read before its binding was rewritten, was on main; the owner changed it, because `?` reads as source text and tells the writer nothing about which value the field names.
- rejected: an agent proposed leaving out the field or repair sentence that needs such a value; the owner rejected it, because [FN-8], [EFF-5] and [OP-4] require the goal, paths and residual in the payload.
- rejected: an agent proposed revising the specification to prescribe the description's words or to allow leaving the field out; the owner rejected it, because the amendment needs conformance cases and the model, and the specification leaves wording to the toolchain [DIAG-1].
- rejected: an agent proposed keeping `?` and listing the allowed places in the text check; the owner rejected it, because the check exists to leave no placeholder and a list of exceptions keeps the known ones.
- rejected: an agent proposed typing the payload fields that carry a source relation as source parts and description parts, returned by the renderers; the owner rejected it, because about 25 renderers and their callers would change and the repair sentences, being strings, stay outside the type.

### decision rejection-text-is-checked-as-a-whole
A gate check reads every pinned and conformance rejection's text: no `?` for a value, each backtick stretch is source or the writer's line, a source-relation field is source around its descriptions.
- replaced: Pinning the sentences of one rejection at a time, with no scan of all rejection text, was on main; the owner changed it, because a placeholder or a stretch that is not source passed unseen unless someone pinned that rejection.

### decision recursive-row-repairs-widen-by-suffix
For a recursive component, EFF-2's `expected_row` widens a growing parameter-cycle edge by the whole argument path it supplies, and the checker validates the rows before a repair is printed.
- rejected: an agent proposed merging growing paths at their longest common prefix; the owner rejected it, because the merged `writes(root.inner)` makes EFF-5 refuse a recursive call that passes `child.inner.other` as a separate read, while the full path keeps the separation.

### decision acyclic-row-repairs-keep-entries-separate
An acyclic EFF-2 repair removes covered entries from the exhibited row and keeps separate uncovered entries, rather than merging one parameter's entries into a wider write.
- rejected: an agent proposed merging one parameter's entries into one `writes` of their common path to avoid EFF-5 separation; the owner rejected it, because the merged write can cover storage the body never writes, which widens every caller's kills beyond what the body does.
- rejected: an agent proposed listing the exhibited set as the suggested row; the owner rejected it, because it can pair an entry with a write at or above its path, as `reads(stats.count), writes(stats.count)` did, which EFF-1 refuses at the declaration.

### decision normalized-constants-are-source-literals
A normalized proof relation keeps its mathematical form and writes each constant in its source type when that admits it, otherwise as a legal literal or an exact expression of literals.
- replaced: Printing normalized constants as bare integers, such as `-1`, was on main; the owner changed it, because a Whitefoot integer literal needs its suffix, and normalization can make a negative bound from unsigned operands or a value no literal holds.

### decision postcondition-failure-adds-clause
A postcondition failure keeps its normalized `relation` and also carries `clause`, the written comparison with each result datum replaced by the returned value's source spelling.
- rejected: an agent proposed changing the specification so that `relation` itself is shown as written; the owner rejected it, because tools read the normalized form, while normalization can reverse a comparison and fold what the writer wrote.

### decision one-compile-lists-every-missing-match
A non-exhaustive match records its edit site and the same structural walk continues, so the ERR-2 rejection lists every collected site with its missing variants in `other_sites`.
- replaced: Stopping at the first non-exhaustive match was on main; the owner changed it, because ERR-2 promises site-by-site edit lists, and one site per compile costs an agent adding a variant one round per match.
- rejected: an agent proposed re-running a function with its rejected matches masked, or collecting with a second checker; the owner rejected it, because [DIAG-2] forbids those walks, while the existing structural walk collects the sites before proof analysis uses its inventory.
- rejected: an agent proposed changing ERR-2 to promise only the first site; the owner rejected it, because that removes the stated toolchain promise and leaves an agent to find one edit per compile round.
