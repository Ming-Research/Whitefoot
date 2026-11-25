# Result registers

Which stored aggregate results return in registers rather than through the caller's destination, and how such a function's body is emitted.

### decision small-stored-results-return-in-registers
A stored aggregate result returns as an LLVM first-class value when each leaf gets its own return register on x86-64, at most three integer words and two floats; a larger one keeps the destination.
- replaced: A destination for every stored aggregate was on main; the owner changed it, because every call that stays out of line paid a store and a reload of its small result, measured on the hash-map `find`, `insert` and `remove` calls (data: `research/investigations/result-registers/DESIGN.md`).
- rejected: an agent proposed returning every stored aggregate by value; the owner rejected it, because LLVM returns a value with more integer leaves than registers through a hidden pointer anyway, and on x86-64 a third float leaf would lose a signaling NaN's bits in the x87 stack.
- rejected: an agent proposed a 16-byte bound; the owner rejected it, because results that fit it still go through a hidden pointer or the x87 stack on x86-64, while three 64-bit words exceed it yet return in registers.
- rejected: an agent proposed a 16-byte bound on top of the leaf budget; the owner rejected it, because it sends 24-byte three-leaf results such as `(u64, Option<u64>)` through memory though they return in registers, and the maintained programs return such results.
- rejected: an agent proposed `fastcc` for internal functions; the owner rejected it, because the destination is an IR parameter that no calling convention removes, and linked C bodies share the callable ABI.
- rejected: an agent proposed packing a result of at most two words into integer registers, as C and Rust do; the owner rejected it, because it needs coercion rules per target and packing code at every boundary, while every such result in the maintained programs already fits one register per leaf.
- rejected: an agent proposed a budget per target, eight integer and eight float leaves on AArch64; the owner rejected it, because signatures, linked definitions and their tests would then differ by target, while the results between the two budgets lie on no measured path.

### decision a-register-returning-body-sits-behind-an-entry
A function returning in registers keeps its destination-form body as an internal `<symbol>.body`, and its public symbol gives the body a frame slot, calls it and returns the loaded value.
- rejected: an agent proposed joining every return in one block that loads the result slot; the owner rejected it, because the host folds the last exit test into a `select` there before threading it, which removed a loop of `records` (data: `research/investigations/result-registers/DESIGN.md`).
- rejected: an agent proposed loading and returning the slot at each exit; the owner rejected it, because the host gathers those returns into one block itself and folds the same test, and the joined aggregates grew the hash-map `find` from 144 to 250 instructions.
- rejected: an agent proposed marking the internal body always-inline; the owner rejected it, because the always-inliner runs before each function is optimized, so the body would be merged into its entry unoptimized and meet the same folding.