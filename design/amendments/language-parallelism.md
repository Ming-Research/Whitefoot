Node: language/parallelism

Decision: Parallel permission derives from checked ownership, effects, dataflow, control flow and domain proofs; a denied permission leaves an accepted program sequential, because overlap is an implementation liberty that must preserve sequential meaning, instead of a source parallel keyword that grants overlap.

Decision: `mustpar` on a counted loop or on the call of a statement asserts the permission [PAR-1, PAR-2, WAIT-2] derives for it and is a rejection when that permission is denied, while adding no permission and being erased before lowering, because a writer who needs parallelism otherwise learns that it was lost only from a ledger or a measurement, instead of leaving lost parallelism silent ([design](../../research/investigations/io-model/WAITS.md#mustpar-asserts-independence)).

Decision: A program means its sequential execution, and an implementation may execute an expression statement whose callee waits alongside the statements after it, as a context of its own that completes before the activation leaves by any edge, exactly when every parameter of the callee is a value parameter and its result can be dropped, because the call then shares no storage with those statements, so running it early changes only when its host effects happen, which host-effect order already leaves open [HOST-1], and one path-disjointness judgment then authorizes every concurrency the implementation adds ([design](../../research/investigations/io-model/WAITS.md#the-program-means-its-sequential-execution)), instead of a marker that starts a context and promises that it progresses while its starter waits.

Decision: A waiting call in a `let` right-hand side whose callee takes only value parameters may also execute alongside the statements after it, completing before its binding is next read, written or released and before the activation leaves, and `mustpar` asserts that permission as it does for an expression statement, because the binding is then the call's only other footprint and nothing reaches it before that point, so a program can issue several requests and combine their answers without a second concurrency construct ([design](../../research/investigations/io-model/WAITS.md#a-bound-context-is-joined-where-its-result-is-first-used)), instead of join handles or a gather primitive that the language would have to type and consume.

Decision: Permission and actualization are separate judgments, because a permission is a proved program property while scheduling is a cost choice, instead of making acceptance depend on a lowering or runtime decision.

Decision: A counted loop is a parallel-permission site in its own right, because iteration independence must not depend on rewriting the loop as sibling calls, instead of treating loop permission as a special case of the sibling-call rule.

Decision: A statement or counted loop that contains a waiting call has no overlap permission, because an overlapped member runs as a compute task and a compute task never waits, instead of overlapping waiting statements on the compute scheduler.

Rejected:
- Source staged-loop permission selected by suspension or native-operation classifications: rejected because a call's declared effect row covers, for the whole of the call, every place the callee may reach through its arguments, and where an operation happens to be implemented cannot authorize overlap that row does not; the loss of pipeline permission is established, but its separate runtime cost has not been measured.
- A separate `spawn` statement: rejected because running a call as a context is sound under the same independence `mustpar` already states, so a second keyword would name one judgment twice.
- A marker whose meaning is that a context must start, with a guarantee that it progresses while another context waits: rejected because a sequential execution of the same program already conforms, so the guarantee would add a second meaning that no program's correctness needs and that ties the language to one implementation.
- Unstructured tasks with join handles: rejected because a handle needs a new type and consuming rule, while joining at the activation's exit needs neither.
- Reference parameters for a call run as a context: rejected because a reference cannot outlive its statement [REF-3] and such a call does.
