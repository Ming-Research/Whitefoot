# Permission judgment

How the checker decides that adjacent statements of one block may overlap: by the paths each one reads and writes.

### decision overlap-is-judged-on-statement-pairs
Overlap is judged on a pair of adjacent statements of one block, and a run of statements may all overlap only when every pair in it may.
- replaced: A window of call members, with the statements between them classified apart, was on main; the owner changed it, because every statement is judged by the paths it reads and writes, so a statement between two calls is a judged member, not a form that ends a group by its syntax.

### decision overlap-uses-the-ordinary-path-judgment
Each statement's writes must be disjoint from the other's reads and writes under the ordinary path-overlap judgment, a by-value consumption counting as a write.
- rejected: an agent proposed a separate aliasing model for parallel permission, by reliance edges, schedule-parametric ownership, exclusive borrows read as writes, or a weakened overlap rule; the owner rejected it, because each one duplicates the ordinary judgment or weakens it.

### decision both-paths-read-in-the-first-state
Both statements' paths are read in the state before the first, and the first's ensures map the second's indices into that state.
- rejected: an agent proposed comparing the two paths in different entry states; the owner rejected it, because an index known only after an append must not look distinct from the append slot it names.

### decision an-unresolved-path-overlaps-all
An overlap not proved disjoint denies permission, and a path the implementation cannot resolve overlaps every path.
- rejected: an agent proposed defaulting an unclassified statement or an unresolved path to an empty footprint; the owner rejected it, because permission would then be granted by omission.
