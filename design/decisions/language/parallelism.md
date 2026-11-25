# Parallelism

What may run at the same time in a program: overlap that the checker proves leaves the sequential meaning unchanged, and contexts the writer starts with `spawn`.

### decision overlap-permission-comes-from-proofs
Overlap of statements or iterations is permitted only by checked ownership, effects, dataflow, control flow and domain proofs; a denied permission leaves the program sequential.
- rejected: an agent proposed a source keyword that grants overlap; the owner rejected it, because overlap is a liberty of the implementation, and it must keep the program's sequential meaning.
- replaced: `mustpar`, a checked assertion of a permission on a counted loop or a call, was on main; the owner changed it, because no program wrote it, the permission ledger reports a denial where a test can check it, and beside `spawn` a second call prefix would read as a weaker start.
- replaced: Staged-loop permission chosen by how an operation suspends or where it is implemented was on main; the owner changed it, because a call's declared effect row covers every place the callee may reach, and where an operation is implemented cannot permit overlap that row does not.

### decision spawn-starts-a-concurrent-context
A `spawn` of a waiting call starts a context that runs concurrently with its starter, and a call that is not spawned runs in order in its caller's context.
- replaced: A sequential meaning under which an implementation could run any permitted waiting call as a context was on main; the owner changed it, because concurrent I/O has programs with no sequential schedule, and which calls ran concurrently followed an analysis the writer could not see.

### decision concurrency-is-no-worse-than-one-driver
An execution on several drivers is one the language permits on one; contexts that all wait with no host operation outstanding may be stopped with a report, and deadlock freedom is not promised.
- rejected: an agent proposed refusing a program whose guard may never become true; the owner rejected it, because whether a guard holds depends on what other contexts do at run time, and no surveyed language proves deadlock freedom for guarded waits.

### decision a-spawn-joins-at-a-fixed-point
A spawn stands as an expression statement or a `let` right-hand side, takes only value parameters, and joins when its activation leaves or, for a `let`, at the first later statement naming it.
- rejected: an agent proposed unstructured tasks with join handles, or a gather primitive; the owner rejected it, because a handle needs a new type and a rule for consuming it, while joining when the activation leaves needs neither.
- rejected: an agent proposed joining a bound spawn at its use inside a statement; the owner rejected it, because later code would then have a joined and an unjoined path to merge.
- rejected: an agent proposed reference parameters for a call run as a context; the owner rejected it, because a reference cannot outlive its statement [REF-3], and such a call does.
- rejected: an agent proposed a spawn of a callee that does not wait, run in order; the owner rejected it, because such a callee cannot interleave with anything, so the word would have two meanings.

### decision permission-is-apart-from-scheduling
Whether overlap is permitted is judged apart from whether the compiler or runtime actually overlaps it, and acceptance never depends on that choice.
- rejected: an agent proposed making acceptance depend on a lowering or runtime choice; the owner rejected it, because a permission is a proved property of the program, while scheduling is a choice of cost.

### decision a-counted-loop-is-a-permission-site
A counted loop is a site of parallel permission in its own right, judged by its own rule for iterations.
- rejected: an agent proposed treating loop permission as a special case of the rule for sibling calls; the owner rejected it, because whether iterations are independent must not depend on rewriting the loop as sibling calls.

### decision waiting-statements-never-overlap
A statement or counted loop that contains a waiting call has no overlap permission.
- rejected: an agent proposed overlapping waiting statements on the compute scheduler; the owner rejected it, because an overlapped member runs as a compute task, and a compute task never waits.
