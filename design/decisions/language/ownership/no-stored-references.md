# No stored references

Where a reference may appear: as a local name or a call argument, never inside a value.

### decision aggregates-hold-only-owned-values
Aggregates and generic instances contain only owned values, at any depth and under any wrapping.
- rejected: an agent proposed provenance metadata on each leaf; the owner rejected it, because a value with no reference in it can move by byte copy without leaving a name inside it that points at the old storage.
- rejected: an agent proposed admitting a reference inside a generic argument or an option-like wrapper; the owner rejected it, because one such position is enough to defeat moving values by byte copy.

### decision references-never-escape
A reference is a local name or a call argument, never part of a value, a result or a stored closure capture; a search returns an index from which its caller forms a reference.
- replaced: Returned references with origins tracked by the signature was on main; the owner changed it, because recomputing the address in the caller avoids provenance obligations on every signature a reference could escape through.
- rejected: an agent proposed a function value that captures a reference and is then stored or returned; the owner rejected it, because it stores the reference under another name and carries the same obligations out of the function that formed it.
- rejected: an agent proposed returning an index and treating a later use as a memory error when the storage was reused; the owner rejected it, because a stale index still in bounds names the slot's current occupant, a logic error the program detects with its own data.