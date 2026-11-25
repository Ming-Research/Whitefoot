# Requires and entry contract

The contract block of a function: its definitions, requirements and postconditions, and where each is formed and proved.

### decision one-erased-contract-block
An ordinary function may carry one erased contract block after its signature, with shared definitions and requires and ensures clauses each checked on its own.
- replaced: a single final requirement block with local bindings and one Boolean check was on main; the owner changed it, because it looked like executable code so its erased role was hidden, a shared abbreviation had to be written twice, and the result had no name.

### decision define-substituted-per-clause
A define is a proof-only abbreviation substituted into every clause that uses it before the clause's range terms are checked; it never runs, stores a value or takes a snapshot.
- rejected: an agent proposed evaluating a define once into a hidden slot; the owner rejected it, because the same abbreviation is read before the call for requires and at the return for ensures, and no one stored value is right at both.
- rejected: an agent proposed refusing a define name in a per-element clause; the owner rejected it, because a writer could then say "the element after i" only with a guard, so define would fail where it is most needed.

### decision named-result-and-payload-route
Every function names its result in its signature, and a postcondition about a success payload writes `Ok(value: name)` or `Some(value: name)` to bind it.
- rejected: an agent proposed an unnamed result or a payload-only convention; the owner rejected it, because the name stands for the whole value, while the tracked facts live on the integer payload inside it or on a struct payload's fields.
- replaced: an `Ok`-only route over integer payloads was on main; the owner changed it, because an Option helper could state its payload bound only after becoming `Result<T, unit>`.

### decision requires-proved-at-call
Each requires clause is proved on its own at the call, in the caller's state, with no runtime check, and two clauses are one goal when they match after definitions expand.
- rejected: an agent proposed matching goals by spelling or by an algebraic normal form; the owner rejected it, because a goal proved once must be recognized wherever it appears again.
- replaced: a callee-entry prologue checking the requirement on every call was on main; the owner changed it, because it let a helper hide a protected leaf behind a runtime trap, and added the prologue's reads to the callee's effect row.

### decision contradictory-requirements-uncallable
A function instance whose requirements contradict each other is legal and uncallable: it is still checked, lowered as an unreachable stub, and publishes no postcondition.
- rejected: an agent proposed rejecting such an instance as a source error; the owner rejected it, because it is harmless dead code no call can reach, while rejecting it would need the checker to detect every contradiction, which a fixed no-search checker cannot promise.

### decision exclusive-parameter-entry-exit
In ensures, an exclusive parameter's bare measure or integer place is its exit state, and `entry(parameter)` selects its entry state.
- rejected: an agent proposed an owned run round trip, or a spelling only for the kernel; the owner rejected it, because a mutating helper must relate the caller's unchanged storage before and after the call without making both sides of an increment one moment.
- rejected: an agent proposed an entry image for integer places that the body's first write ends; the owner rejected it, because a count field kept beside the run it counts could then be neither proved after the body's write nor kept by the caller.

### decision unrouted-postconditions-at-error-exits
An unrouted postcondition, and so a type invariant over a written reference parameter, holds at every exit its caller continues from, explicit returns and propagated error exits alike.
- rejected: an agent proposed giving a caller exit-state relations only on the success arm; the owner rejected it, because that would take from every caller facts that a body restoring its state before propagating can prove.

### decision window-transfer-length-facts
Moving a window publishes an empty source and a destination length at least its entry length, and splitting publishes the split index as the source length; callers read exact totals after.
- rejected: an agent proposed two entry measures on one relation side, so moving or splitting a window could publish their exact sum; the owner rejected it, because the chosen extension admits one scalar parameter as the addend, and the caller can read the exact length after the call.

### decision linked-calls-same-boundary
Linked declarations and source definitions, a build-selected entry included, have the same call-site requirement boundary.
- rejected: an agent proposed an exception for the launcher, or an executable requirement check; the owner rejected it, because native linkage cannot justify a proof the ordinary caller lacks.

### decision requirement-places-at-entry
A requirement's places are formed at the callee's body entry, after the requirements written before it, so every subscript in such a place owes its bounds obligation there.
- rejected: an agent proposed forming a requirement's places in each caller's instantiated goal; the owner rejected it, because the fact is established in the callee's entry state, where the caller's proof of the subscript is not available.

### decision postcondition-places-at-returns
A postcondition's places are formed at each return that selects it, judged over what each place denotes there, and a formal subscript offset is replaced by each caller's actual.
- rejected: an agent proposed forming a postcondition's places at body entry; the owner rejected it, because a routed clause's subscript could not rely on the guard that selects its success return.
- rejected: an agent proposed judging the subscript on the offset's current binding and the base's length at the return; the owner rejected it, because the clause names the entry element, so an index equal to the entry length passed once the body appended.
- rejected: an agent proposed reading the relation over a rebound offset through an entry datum of the offset; the owner rejected it, because a place's offset must be a tracked place or a constant, so that kills reach the place through the offset's support.