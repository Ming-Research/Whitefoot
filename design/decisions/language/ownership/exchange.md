# Exchange

The built-in two-place `swap`: which pairs of places it admits and what storage it moves.

### decision built-in-two-place-swap
`swap` is a built-in operation that takes a reference to each of two places and exchanges their values, and it is admitted when both name the same place, where it does nothing.
- replaced: A multi-place commit that reads every target before writing any was on main; the owner changed it, because the commit must prove its targets distinct before it orders its writes, which puts a proof or a runtime branch into every partition loop.

### decision swap-excludes-possible-ancestry
`swap` admits equal captured targets and disjoint storage, but needs proof that neither target may be a proper ancestor of the other.
- rejected: an agent proposed extending the equality allowance to every possible overlap of targets of unknown depth; the owner rejected it, because swapping a recursive owner with its own descendant would make a cycle or lose ownership, while equal array slots stay a harmless no-op.
- rejected: an agent proposed requiring distinct targets for every exchange; the owner rejected it, because the same-slot allowance serves indexed partition and heap adjustment without a branch.

### decision swap-equality-across-index-steps
`swap`'s equality allowance covers two slots of equal depth whose containing paths are identical step for step or differ only in index steps.
- rejected: an agent proposed tightening the checker to require slots under one identical array or window; the owner rejected it, because the relation is sound for rows chosen by unrelated indices, so it would refuse a correct exchange and make the writer prove two rows identical.

### decision runtime-capacity-contents-not-swapped
A window of runtime capacity, a run of segments or a paged window inside a heap cell is never exchanged in place; a program exchanges the owning cells instead.
- rejected: an agent proposed exchanging the two cells' pointers when both places are cell contents; the owner rejected it, because a content reached through a reference parameter has no cell slot to rewrite, so one written exchange would be admitted in one place and refused in another.
- rejected: an agent proposed exchanging whole blocks in place; the owner rejected it, because blocks of different capacities cannot hold each other's contents without reallocating and rewriting both owners, which an exchange through references cannot reach.