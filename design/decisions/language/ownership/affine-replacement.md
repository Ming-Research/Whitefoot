# Affine replacement

How a program takes a value out of a place without leaving a hole: the exchange, whole-owner moves, the atomic update and reinitialization.

### decision replace-is-a-swap-with-a-local
The only built-in exchange is the two-place swap; replacing a place's value while keeping the old one is library code that swaps the place with a local and returns the local.
- replaced: A dedicated replace operation and a let-only replace statement was on main; the owner changed it, because a swap with a local makes exactly the moves a replace would, and the language should not carry an operation source code writes at no cost.
- rejected: an agent proposed a take that leaves a hole the checker proves refilled before scope end; the owner rejected it, because an open hole needs per-place vacancy flow, a repair of every edge leaving the scope and a meaning for references that reach it, for a window no consumer needs.
- replaced: Refusing whole-binding exchange on places of slice type and arena type was on main; the owner changed it, because neither is a type any more, so no value carries the static origin set or the confinement the refusal protected.

### decision field-move-consumes-the-owner
A move out of a field consumes the whole owner and releases its other affine parts, and a remaining linear part refuses the move unless it is taken in the same destructuring.
- rejected: an agent proposed a partial move that leaves the owner alive with one part missing; the owner rejected it, because a partly moved owner is a hole under another name, while a whole-owner consume keeps every place a program can name wholly present or gone.

### decision no-move-out-of-a-slot
A move out of a window slot or an array element is refused; the ways out are the removing operations, the place exchange and the two-place exchange.
- rejected: an agent proposed an element take that carries an obligation to refill the slot later; the owner rejected it, because those slots are covered by a length or an always-filled invariant, and nothing records that one of them is empty.

### decision atomic-in-place-update
`set p = f(move p, ...)` is one atomic update with no program point between its read and its commit, admitted when f has no failure exit and does not write, move or release any prefix of p.
- rejected: an agent proposed moving the value out into a temporary and assigning it back; the owner rejected it, because that leaves an observable gap, while the atomic update writes the rebuilding of a node in place directly and stores a failure back as an enum value.

### decision dead-binding-reinitialized-whole
A binding whose value was consumed is writable only by a `set` whose target is the complete binding, which reinitializes it; a place projected from a dead root is never writable.
- replaced: A dead root that nothing could revive was on main; the owner changed it, because the loop rule already admits a body that moves an outer binding and reinitializes it before the backedge, and a fallible step in a loop cannot use the atomic update.
- rejected: an agent proposed a write into a part of a dead owner; the owner rejected it, because it would bring one component back beside missing ones.

### decision vacancy-is-a-value
An empty element is a value, an Option-shaped element checked by an ordinary match, and never a state the checker tracks.
- rejected: an agent proposed typed holes, a taken slot flowing to an Option-like vacant type state; the owner rejected it, because per-place flow-sensitive type states are what this ownership model excludes, and vacancy would leak into every boundary signature.