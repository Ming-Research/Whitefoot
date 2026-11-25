# Cleanup

How the compiler derives and emits the release of owned values: recursive types, displaced values, destructured and moved parts, and the one classification of which types release anything.

### decision a-cyclic-release-graph-releases-by-self-call
A derived drop of a type whose release graph closes on itself is one release action per node type that calls itself where the graph closes, and the stack ledger shows that call as a cycle row.
- replaced: An explicit worklist, decided by strongly connected components over the cleanup graph, was on main; the owner changed it, because the worklist allocated, and an allocation on the release path is a runtime trap the writer never wrote.
- replaced: Refusing types whose release graph has a cycle was on main; the owner changed it, because such a cycle can arise only where a heap is allowed, and a heap-allowed program's resource use is a runtime quantity already.

### decision an-assignment-releases-its-displaced-value-by-checked-liveness
A checked assignment carries whether it displaces a live value, and lowering releases that old value after storing the new one, with the same type-based cleanup as scope exit.
- rejected: an agent proposed working out source liveness again in lowering; the owner rejected it, because the same typed target may still hold a value, may have been consumed by the right-hand side, or may be a dead binding being set again.

### decision destructuring-and-box-moves-carry-their-residual-releases
A destructuring carries its field ordinals and the releases of the parts it covers, and a move out of a Box carries its path and the cleanup of the enclosing owner, apart from an ordinary read.
- rejected: an agent proposed hidden binders whose cleanup waits until scope exit; the owner rejected it, because skipped fields are not positional binders, and consuming the whole owner must release its left-over parts and enclosing cells exactly once.
- rejected: an agent proposed a flag on the read node; the owner rejected it, because every later reader of the node would have to interpret it.

### decision one-release-classification-serves-checker-and-cleanup
The checker's tail-transfer check, the IR and the cleanup emitter read one release classification, in `compiler/src/release.rs`, which handles recursive types.
- replaced: A separate list of releasing types in the checker, the IR and the backend was on main; the owner changed it, because the three lists disagreed, and an owner the checker leaves out but the cleanup releases breaks the reference check before a guaranteed tail call.