# Ownership

Single owners consumed by explicit moves, references as names for paths, and the one overlap relation every check shares.

### decision single-owners-and-path-references
Each value has one owner and is consumed by an explicit move; a reference is a local name for a path rooted at a local, a parameter or a named constant, with no borrow mode or region.
- replaced: Shared and exclusive borrow modes held in lexical regions, with loan endpoints listed singly was on main; the owner changed it, because the callee's effect row already says whether it may write through a reference, and whether a reference is usable is a checked fact about its path.
- rejected: an agent proposed general inferred borrow checking with implicit reborrows and inferred non-lexical lifetimes; the owner rejected it, because a reference that only names a path has no extent to infer: it stops being usable when a proper prefix of its path changes or its root ends.

### decision one-overlap-relation
Reference validity, call interference and parallel permission share one conservative overlap relation on complete paths, in which paths through different variants of one enum overlap.
- rejected: an agent proposed a separate alias analysis for each check that asks about overlap; the owner rejected it, because one relation lets validity, call interference and parallel permission share the same separation proof.
- replaced: Treating every index that is not a literal as overlapping every other was on main; the owner changed it, because insert, remove, heap adjustment and partition then need extra element moves or allocation to reach two elements already proved to be in different slots.
- replaced: Index separation only among the targets of one multi-place commit was on main; the owner changed it, because the same proof must serve reference validity, calls and statement overlap.
- rejected: an agent proposed treating every access to one allocation as a conflict with the whole allocation; the owner rejected it, because uneven recursive partitions and rows of runtime width need independent access to disjoint parts of one block.

### decision unknown-depth-target-covers
A reference target of unknown depth has a subtree cover for interference and its own current-target identity; finite suffixes are compared only between accesses that share one captured target.
- replaced: An overlap relation over exact finite paths only was on main; the owner changed it, because iterative descent through owned links needs finite covers of paths whose depth is known only at run time.
- rejected: an agent proposed treating a wildcard as an ordinary exact path step; the owner rejected it, because a cover must include every possible ancestor or descendant conflict.
- rejected: an agent proposed proving two unrelated cursors separate from the spelling of their suffixes; the owner rejected it, because a cover must include every possible ancestor or descendant conflict, and only one current node's sibling fields are known to be disjoint.

### decision derived-release-runs-no-code
Every release action is derived from the value's type and runs no user code, and no declaration, attribute or binding attaches a finalizer to an edge that leaves a scope.
- rejected: an agent proposed destructors or native finalizers chosen by a value's origin; the owner rejected it, because a user-defined release would hide code and effects on every scope exit, and with one heap the derived release needs no store, provider or capability.

### decision no-global-mutable-state
There is no global mutable state, only immutable const items; state that several contexts share is a shared object each reaches by a handle and changes only in atomic statements.
- rejected: an agent proposed a shared mutable static with an access discipline of its own; the owner rejected it, because a function that reached a global could not be pure, and every proof that rests on purity, the permission for statements to overlap first, would collapse.

### decision window-parts-beside-live-indices
An indexed position is separated from a window's append slot or free slots only where the compared state proves the index below the length, and never from the window's last filled slot.
- rejected: an agent proposed never separating an indexed position from the append slot; the owner rejected it, because an appending call must be provable not to disturb a live reference or a fact about a filled slot, which is why the window parts exist.
- rejected: an agent proposed treating an index that an effect row takes from another argument as never live; the owner rejected it, because a call whose requirement bounds that index proves it at entry as it proves two indices distinct, and refusing it would reject calls the parts already separate.

### decision consuming-use-writes-move
A consuming use of an existing owned place whose type or bound does not grant copy writes `move`, in `match` and `propagate` too; a temporary needs no marker.
- replaced: Implicit consumption of a bare place that is not copy was on main; the owner changed it, because selecting one field could consume the whole owner and release its other fields with no written point of consumption.

### decision const-read-as-whole-value
A const item of a copy type can be read as a whole value, which copies it into the reader's storage, as well as through a subscript, a measure, a field or a reference.
- replaced: Allowing only subscript, measure, field and reference reads of a const item was on main; the owner changed it, because no ownership, release or safety reason stood behind it, and it forced an element-by-element copy of a template the writer means to change.
- rejected: the owner proposed whole-value reads of every const item, structs included; the owner rejected it, because a struct declared `nocopy` can be a const, and a whole-value read is a copy.