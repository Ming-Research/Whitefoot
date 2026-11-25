# Frozen objects

Frozen objects: data several contexts read and none writes, held through `Frozen<T>` handles and read without an atomic statement.

### decision frozen-object-is-read-without-atomic
Data several contexts read and none writes is a `Frozen<T>`: `frozen_new` moves an owned value in, `frozen_share` makes another handle, and `inner` is read with no atomic statement.
- rejected: an agent proposed relaxing the rule against nested atomic statements for a `SharedRead` target whose object has no writer left; the owner rejected it, because `shared_share` makes writable handles at run time, so whether a writer remains cannot be decided where the program is checked.
- rejected: an agent proposed a conversion `freeze(Shared<T>) -> Frozen<T>` of an existing shared object; the owner rejected it, because it needs a run-time check that no other handle and no atomic statement remain, which no consumer needs while a frozen object can be built from an owned value.
- rejected: an agent proposed a new storage domain for snapshots; the owner rejected it, because a frozen object expresses the operation the prototype could not, within the release machinery for shared objects the language already has.
- rejected: an agent proposed relaxing the service-first abort of a snapshot's reserve; the owner rejected it, because that abort is the owner's policy for keeping service ahead of persistence, and taking readers' handles back is a library protocol, not a language rule.

### decision frozen-object-is-deeply-immutable
A frozen object's type has no part, at any depth, that is a `Shared`, a `SharedRead`, a `DecisionCell` or a host handle.
- rejected: an agent proposed a shallow freeze that leaves nested shared state writable; the owner rejected it, because a snapshot read through a frozen object must not see a later write, and those parts are the only ones through which a value reached from it could change.