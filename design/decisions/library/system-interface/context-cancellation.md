# Context cancellation

How one context ends another's host waits: the watch each wait takes, the cancellation handles, how a firing reaches waits and guards, and what a cancelled wait returns.

### decision host-waits-take-explicit-cancel-watch
Every host wait that takes an optional deadline, and `sleep_until`, takes an explicit `CancelWatch` right after its deadline, with `cancel_never` for a wait that must finish.
- rejected: an agent proposed one bound value that replaces the deadline (S2); the owner rejected it, because a copy watch naming runtime state needs another lifetime rule when the state is released while copies remain, and it hides the per-wait watch.
- rejected: an agent proposed a watch set on the context (S3); the owner rejected it, because ambient cancellation hides the capability from the wait's parameters, needs an escape for a wait that must finish, and needs a rule for spawned contexts.
- rejected: an agent proposed a watch bound into a resource handle (S4); the owner rejected it, because sleeps have no such handle, and mixed cancellable and must-finish use of one resource would need two bindings.

### decision cancel-handles-are-droppable-structs
`CancelSource` and `CancelWatch` are droppable opaque structs whose private fields hold `Shared<CancelState>` and `SharedRead<CancelState>`, with explicit close functions equal to dropping.
- replaced: A `nodrop` source and watch, closed explicitly, was on main; the owner changed it, because that catches only two forgotten-fire mistakes at compile time, while it blocks storing sources in droppable shared state.
- rejected: an agent proposed a host-specific non-empty release for cancellation handles; the owner rejected it, because their state is ordinary shared runtime memory, which a private shared field releases with no rule chosen by host origin.
- rejected: an agent proposed making only sources droppable; the owner rejected it, because a watch keeps the same ordinary shared state, and a connection's context still could not get a new watch.
- rejected: an agent proposed copy watches; the owner rejected it, because their state can disappear while copies remain, while ordinary ownership keeps every pending wait's state live with no reference counting at park.

### decision cancel-firing-finds-waits-at-fire
A firing finds waits on each driver's own intrusive watch list and ends them through the existing deadline cancellation route.
- rejected: an agent proposed registering each parked wait under the source's shared lock; the owner rejected it, because every request would pay lock and list traffic for a firing that happens once, while find-at-fire matched polling (data: `research/investigations/context-cancellation/README.md`).

### decision cancellation-is-io-error-cancelled
Cancellation is the payload-free IoError variant `Cancelled`, carried by `ReadFailed` for reads, and `sleep_until` returns `Result<unit, unit>` whose `Err` reports cancellation.
- rejected: an agent proposed reporting a firing as `DeadlinePassed`, as the performance prototype did; the owner rejected it, because a fired watch must be told apart from a reached deadline.

### decision guards-observe-cancel-state
`cancel_state` returns a `SharedRead<CancelState>` over a watch's state, whose public `fired` stays true once fired and false for `cancel_never`, for host waits and atomic guards alike.
- rejected: an agent proposed a program-owned shared stop flag beside cancellation; the owner rejected it, because its update and `cancel_fire` would be two transitions observed apart.
- rejected: an agent proposed a bare `cancel_fired` query; the owner rejected it, because a read-only row neither registers a guard dependency nor gives a held, coherent observation of the state.
- rejected: an agent proposed an atomic target made only for cancellation; the owner rejected it, because `SharedRead` gives the same protection through the ordinary shared-object abstraction.
- rejected: an agent proposed that cancellation interrupt a pending statement without running its body; the owner rejected it, because it changes the chosen meaning of guard state and needs a new outcome and outgoing-edge model.

### decision cancel-fire-is-a-waiting-transition
`cancel_fire` declares `waits` and makes one held transition of the shared state, waking guard watches while host waits are still found at fire.
- rejected: an agent proposed a nonwaiting firing inside an atomic block; the owner rejected it, because it would either acquire its held state again, hidden and nested, or change a state the block must observe stably.