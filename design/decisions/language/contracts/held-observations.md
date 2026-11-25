# Held observations

The `requires held` contract: a callable that reads several referents as one observation inside its caller's atomic statement.

### decision held-clause-names-reference-parameters
A `requires held` clause names reference parameters whose referents share one caller-held atomic statement and stay unchanged at its effect point.
- rejected: an agent proposed admitting a capture by its operation, as a privileged or directly written call; the owner rejected it, because it cannot be forwarded through the ordinary callable contract and breaks the constitution's principle on external interaction unless that is amended.
- rejected: an agent proposed ordinary read-only reference effects alone for the held promise; the owner rejected it, because they admit an unheld private reference and do not protect the referents against writes elsewhere in the surrounding statement.

### decision held-callable-writes-nothing
A `requires held` clause belongs to a callable that does not wait and whose effect row has no writes; writes to its own private and local storage keep their ordinary rules.
- rejected: an agent proposed admitting borrowed outputs through a contract that they are disjoint from every held state; the owner rejected it, because the no-write boundary excludes a write through an unlisted borrowed parameter that could alias another held state, with no negative origin facts.

### decision only-named-referents-protected
Only the referents a held requirement names are protected against every write in the caller statement's whole block.
- rejected: an agent proposed refusing every write in the caller statement, making it wholly read-only; the owner rejected it, because changing an unrelated held field does not change the protected observation.

### decision held-origins-are-forwarded
Reference aliases and projections keep finite held-origin facts, a contracted body receives one symbolic caller origin, and forwarding checks those facts against the requirement.
- rejected: an agent proposed tracking the history of scalar values; the owner rejected it, because ordinary references already give lifetime and no escape, and this one structural fact supplies the shared observation point.
- rejected: an agent proposed inspecting callee bodies recursively; the owner rejected it, because ordinary references already give lifetime and no escape, and this one structural fact supplies the shared observation point.
- rejected: an agent proposed runtime hold tokens; the owner rejected it, because ordinary references already give lifetime and no escape, and this one structural fact supplies the shared observation point.

### decision actual-held-set-may-be-smaller
A function actual's set of held parameters may be a subset of its formal's set.
- rejected: an agent proposed refusing function-kind bindings of held callables; the owner rejected it, because a callable that needs fewer protected inputs meets the stronger caller boundary, and the formal contract stays authoritative.