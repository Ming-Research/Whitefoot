# Waiting

Waiting functions: how a writer declares that a function may wait, where a waiting call may stand, and what a waiting function compiles to.

### decision waiting-is-a-declared-function-kind
A function that may wait declares `waits` after its effect row, a waiting call stands only in the body of another waiting function, and the entry may wait.
- rejected: an agent proposed deriving which functions wait from the call graph; the owner rejected it, because a callee's body would then change its callers' calling convention with no spelling the writer sees.
- rejected: an agent proposed letting every call wait and having the scheduler park a task that misses; the owner rejected it, because a task that waits under a helping join strands every join beneath it, which the language can exclude instead.
- rejected: an agent proposed a `wait` keyword at each waiting call; the owner rejected it, because the callee's signature already says which calls wait, so the keyword would repeat it at every call.
- rejected: an agent proposed a bridge that lets a function that does not wait block on a waiting one; the owner rejected it, because that bridge is the wait inside compute code that strands a join beneath it.

### decision waiting-function-is-a-resumable-frame
A waiting function compiles to a resumable frame made when it is entered and released once its caller has read its result; a pending host operation suspends the frame until it completes.
- rejected: an agent proposed a stack of its own per waiting context; the owner rejected it, because each context costs two kernel mappings, which bounds a process near the mapping limit, and a compute join on its stack stops every other context (data: `research/investigations/io-model/WAITS.md`).
- rejected: an agent proposed a host thread per context; the owner rejected it, because its stack and scheduling cost grow with the number of contexts, and that is the design the echo references beat.