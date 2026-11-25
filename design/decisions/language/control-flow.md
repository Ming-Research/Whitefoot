# Control flow

The language's loops and their edges: early transfers to the next iteration, when a loop has a normal successor, and guaranteed self tail calls.

### decision continue-reaches-a-labeled-loop
`continue;` goes to the next iteration of the nearest enclosing loop and `continue @label;` to the labeled one, through ordinary cleanup, with a counted loop's binder updated once.
- rejected: an agent proposed a `continue` that carries values; the owner rejected it, because assignment already supplies the backedge values, while transfer arguments would need a second interface for binding state, with its own evaluation order and move rules, and no use was shown.
- rejected: an agent proposed an unlabeled `continue` only; the owner rejected it, because an early next instruction inside a nested decoding or copy loop must reach the outer interpreter loop, and the existing labels supply that target.

### decision a-break-free-loop-has-no-exit
An ordinary loop has a normal successor exactly when a `break` resolves to it, so a final loop without one delivers a value initializer by divergence; counted loops keep their exit edge.
- replaced: A normal successor edge on every ordinary loop was on main; the owner changed it, because a return-only or endless loop cannot fall through, and the trailing `return` it required invented a path that never runs.

### decision musttail-marks-a-checked-self-transfer
A call marked `musttail` must be a direct self call in the sole return position, with reference arguments rooted only at incoming reference parameters, and it transfers without growing the stack.
- rejected: an agent proposed an advisory hint to the optimizer; the owner rejected it, because an agent needs a constant-stack transfer the checker can check, and an ordinary call leaves both the optimization and a hidden cleanup after the call outside that guarantee.
- rejected: an agent proposed an implicit promise of a tail transfer for every recursive call; the owner rejected it, because an ordinary call leaves the optimization and the cleanup after the call outside any guarantee.
- rejected: an agent proposed a tail annotation on the whole function; the owner rejected it, because a function can hold both tail and non-tail recursive calls, and the writer's obligation is at the chosen call.
- rejected: an agent proposed allowing references rooted at owned parameters or locals when their address happens to stay stable; the owner rejected it, because replacing the frame does not keep that activation's storage, and the incoming reference parameter is the boundary the source can state.

### decision tail-calls-release-before-the-transfer
A self-tail transfer runs the remaining ordinary releases after capturing its arguments, and only when no live reference names the owner released.
- rejected: an agent proposed keeping the cleanup after the call; the owner rejected it, because the language has only compiler-derived release and no user finalizer, so an owner no reference names can be freed first without a visible change.
- rejected: an agent proposed rejecting every tail call that has an unreferenced affine local; the owner rejected it, because the language has no user finalizer, so releasing such an owner before the transfer changes nothing the program can see.
