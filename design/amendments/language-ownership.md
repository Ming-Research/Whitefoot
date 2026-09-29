Node: language/ownership

Decision: There is no global mutable state, only immutable const items, because a function that reached a global directly could not be pure and every proof that rests on purity, the permission for two statements to overlap first among them, would collapse, while a global handed in as a parameter is no different from a value created at the program's root, and state that several contexts share is a shared object each holds a handle to and changes only in atomic statements whose order is an input of the execution [SHARE-1, SHARE-3], instead of a shared mutable static with an access discipline of its own.

Rejected:
- Replaced decision beginning "There is no global mutable state, only immutable const items": rejected because its last reason, that with no shared-memory threads there is nothing a global lock would guard, no longer holds once contexts on several drivers reach one shared object through a lock; the choice itself stands, and the replacement states where shared state lives instead.
