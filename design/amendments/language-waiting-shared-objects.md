Node: language/waiting/shared-objects

Decision: State that several contexts reach is a shared object held through `Shared<T>` handles and reached only inside an `atomic s = &h { ... }` statement, whose block runs with exclusive access to the object's state and takes effect at one point within the statement, while the order in which different contexts' statements take effect is an input of the execution, because this keeps the sequential meaning [WAIT-2] by treating the object like a host whose answers depend on others, and a lexical block reaches the context's locals directly and shows the atomicity boundary in the source ([design](../../research/investigations/io-model/SHARED.md#the-idea)), instead of a transaction function that takes a callback and an environment.

Decision: An atomic statement counts as a waiting call, and its block and guard contain no waiting call and no atomic statement, because the existing waiting-function rule [WAIT-1] then makes nested acquisition, lock-order deadlock and holding the object across a host wait unrepresentable, instead of lock ordering or deadlock detection.

Decision: The object stays live until the statement completes whatever its block does with the target place, because the statement holds a handle of its own, instead of reading the target again when the block ends, which would refuse a block that moves or replaces the handle it was reached through.

Decision: An optional `when` guard, a `Bool` expression that writes nothing, makes the statement take effect at a point where the guard holds, and the block may use the guard as a proved fact, because blocking pops and subscriptions are waits on the shared state itself and the block's operations need the guard's condition as their requirement ([evidence](../../research/investigations/io-model/SHARED.md#the-first-version)), instead of a condition variable the writer signals.

Decision: Provisionally, every atomic statement acquires its object exclusively and nothing marks a statement that only reads, because the meaning (one point, one order) is the same either way and no measured workload yet has contending readers, instead of separate read and write forms; running statements that only read at the same time stays a runtime liberty, to be reopened by a workload where readers contend ([question](../../research/investigations/io-model/SHARED.md#remaining-questions)).

Rejected:
- A transaction function with a callback (`transact`, `inspect`, `transact_when`): rejected because the language has no closures, so every transaction needs a top-level function and an environment struct, and read, guarded and multi-object transactions each need another function.
- An implicit scope that acquires the object for the life of a reference formed through a handle: rejected because whether two adjacent statements form one transaction could not be read from the source.
- Asynchronous messages to the object (actors, channels): rejected because a sender that does not wait for the effect cannot know the order its messages take effect in, which fails the sharing rule.
- Atomic fields and lock-free cells: rejected because they expose interleavings of single reads and writes that the sequential meaning excludes.
- Ordering transactions by stamps of the host completions that produced them: rejected because it orders nothing a program can observe beyond the single point at which each statement takes effect.
