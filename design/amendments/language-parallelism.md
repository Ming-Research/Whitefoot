Node: language/parallelism

Decision: Parallel permission derives from checked ownership, effects, dataflow, control flow and domain proofs; a denied permission leaves an accepted program sequential, because overlap is an implementation liberty that must preserve sequential meaning, instead of a source parallel keyword that grants overlap.

Decision: `mustpar` on a counted loop or on the call of a statement whose callee does not wait asserts the permission [PAR-1, PAR-2] derives for it and is a rejection when that permission is denied, while adding no permission and being erased before lowering, because a writer who needs parallelism otherwise learns that it was lost only from a ledger or a measurement, instead of leaving lost parallelism silent ([design](../../research/investigations/io-model/WAITS.md#mustpar-asserts-independence-and-starts-contexts)).

Decision: `mustpar` on an expression statement whose callee waits starts that call in a new context, admitted only when every parameter of the callee is a value parameter and its result can be dropped, and every context an activation starts completes before the activation leaves by any edge, because the started call then shares no storage with its starter and needs no handle to be joined, instead of a separate `spawn` statement or unstructured tasks with join handles ([design](../../research/investigations/io-model/WAITS.md#mustpar-asserts-independence-and-starts-contexts)).

Decision: Permission and actualization are separate judgments, because a permission is a proved program property while scheduling is a cost choice, instead of making acceptance depend on a lowering or runtime decision.

Decision: A counted loop is a parallel-permission site in its own right, because iteration independence must not depend on rewriting the loop as sibling calls, instead of treating loop permission as a special case of the sibling-call rule.

Decision: A statement or counted loop that contains a waiting call has no overlap permission, because an overlapped member runs as a compute task and a compute task never waits, instead of overlapping waiting statements on the compute scheduler.

Rejected:
- Source staged-loop permission selected by suspension or native-operation classifications: rejected because a call's declared effect row covers, for the whole of the call, every place the callee may reach through its arguments, and where an operation happens to be implemented cannot authorize overlap that row does not; the loss of pipeline permission is established, but its separate runtime cost has not been measured.
- A separate `spawn` statement: rejected because starting a context is sound under the same independence `mustpar` already states, so a second keyword would name one judgment twice.
- Starting a context for every independent waiting statement without a marker: rejected because a started context outlives its statement, which changes what the program does rather than how fast, so it cannot be an implementation liberty.
- Unstructured tasks with join handles: rejected because a handle needs a new type and consuming rule, while joining at the activation's exit needs neither.
- Reference parameters for a started call: rejected because a reference cannot outlive its statement [REF-3] and a started call does.
